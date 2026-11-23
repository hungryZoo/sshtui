//! Screen state and state transitions.

mod backend;
mod confirm;
mod form;
mod import;
mod input;
mod list;
mod update;

use std::ffi::OsString;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::config::{
    ConfigSet, HostEntry, WriteError, WriteOutcome, block_fields, plan_add, plan_delete, plan_edit,
};
use crate::dialog::PickOutcome;
use crate::history::Suggestion;
use crate::i18n::Lang;
use crate::known_hosts;
use crate::ssh::args::{ArgsError, ConnectTarget, config_file_args, connect_args, preview};
use crate::ssh::effective::{EffectiveConfig, ExpectedConfig, find_mismatches};
use crate::ssh::keysetup;
use crate::update::UpdatePlan;
use crate::validate::ValidationError;

pub use self::backend::Backend;
pub use self::confirm::{ConfirmChoice, ConfirmKind, ConfirmState, KeyPickerState, NoticeState};
pub use self::form::{FormField, FormKind, FormState};
pub use self::import::ImportState;
pub use self::input::TextInput;
pub use self::list::HostRow;
pub use self::update::UpdateState;

use self::confirm::ConfirmAction;
use self::input::{is_ctrl, is_plain};
use self::list::ListState;

/// Why the event loop should stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    /// Leave the TUI.
    Quit,
    /// Leave the TUI and run ssh with these arguments.
    Connect(Vec<OsString>),
    /// Leave the TUI and run the approved upgrade.
    Update(UpdatePlan),
}

/// A key installation that must run outside the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInstall {
    /// Alias or address shown to the user.
    pub label: String,
    /// Arguments for ssh.
    pub args: Vec<OsString>,
    /// The public key line sent on stdin.
    pub public_key: String,
}

enum KeyInstallAfter {
    Direct,
    Add,
    Edit(ExpectedConfig),
}

struct PendingKeyInstall {
    label: String,
    request: Option<KeyInstall>,
    after: KeyInstallAfter,
}

/// Something on screen that can be clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickTarget {
    /// Row index in the filtered host list.
    HostRow(usize),
    /// The row that opens a new connection form.
    NewRow,
    /// A field and the display column in its full value.
    FormField {
        /// The field to focus.
        field: FormField,
        /// Column including horizontal scrolling, or `u16::MAX` for the end.
        column: u16,
    },
    /// Row index in the form key list or key picker.
    KeyRow(usize),
    /// Row index in the shell history suggestions.
    SuggestionRow(usize),
    /// A button or hint that acts like an unmodified key press.
    Key(KeyCode),
    /// The update notice in the header.
    UpdateNotice,
    /// An area that swallows clicks without acting on them.
    Inert,
    /// Outside the active popup.
    Outside,
}

/// Severity of a status message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
    Warning,
    Error,
}

/// A transient message displayed after an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// Severity of the message.
    pub level: StatusLevel,
    /// User-facing text.
    pub text: String,
}

/// The currently active screen or overlay.
#[derive(Debug)]
pub enum Mode {
    List,
    Form(FormState),
    KeyPicker(KeyPickerState),
    /// Selects a connection to import from shell history.
    Import(ImportState),
    Confirm(ConfirmState),
    Notice(NoticeState),
    Update(UpdateState),
}

/// Application state and the side effects available to it.
pub struct App<B: Backend> {
    backend: B,
    lang: Lang,
    config: ConfigSet,
    history_commands: Vec<String>,
    suggestions: Vec<Suggestion>,
    list: ListState,
    mode: Mode,
    status: Option<Status>,
    exit: Option<Exit>,
    full_redraw: bool,
    update_plan: Option<UpdatePlan>,
    key_install: Option<PendingKeyInstall>,
}

impl<B: Backend> App<B> {
    /// Loads the configuration and selects the last connected host when available.
    /// With no hosts or history suggestions the app starts on the new-connection form.
    pub fn new(backend: B) -> Self {
        Self::with_lang(backend, Lang::En)
    }

    /// Loads the application using the specified display language.
    pub fn with_lang(backend: B, lang: Lang) -> Self {
        let mut app = Self {
            update_plan: backend.update_plan(),
            history_commands: backend.recent_ssh_commands(),
            backend,
            lang,
            config: ConfigSet::default(),
            suggestions: Vec::new(),
            list: ListState::default(),
            mode: Mode::List,
            status: None,
            exit: None,
            full_redraw: false,
            key_install: None,
        };
        app.load_config();
        let last_host = app.backend.last_host();
        app.reload_list(last_host.as_deref());
        if app.config.hosts.is_empty() && app.suggestions.is_empty() {
            app.mode = app.new_form();
        }
        app
    }

    /// Handles a press or repeat event on the active screen.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.exit.is_some()
            || self.key_install.is_some()
            || !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            return;
        }
        self.status = None;
        if is_ctrl(key, 'c') {
            self.exit = Some(Exit::Quit);
            return;
        }

        let mode = std::mem::replace(&mut self.mode, Mode::List);
        self.mode = match mode {
            Mode::List => self.handle_list_key(key),
            Mode::Form(form) => self.handle_form_key(form, key),
            Mode::KeyPicker(picker) => self.handle_picker_key(picker, key),
            Mode::Import(import) => self.handle_import_key(import, key),
            Mode::Confirm(confirm) => self.handle_confirm_key(confirm, key),
            Mode::Notice(notice) => self.handle_notice_key(notice, key),
            Mode::Update(update) => self.handle_update_key(update, key),
        };
    }

    /// Handles a left click, with `double` set for a second click on the same target.
    pub fn handle_click(&mut self, target: ClickTarget, double: bool) {
        if self.exit.is_some() || self.key_install.is_some() {
            return;
        }

        let code = match (&mut self.mode, target) {
            (_, ClickTarget::Key(code)) => code,
            (Mode::List, ClickTarget::HostRow(index))
                if index < self.list.rows(&self.config).len() =>
            {
                self.status = None;
                self.list.select(index);
                self.refresh_selected();
                if double {
                    self.connect_selected();
                }
                return;
            }
            (Mode::List, ClickTarget::NewRow) => {
                self.list.select(self.list.new_row_index());
                self.list.filtering = false;
                KeyCode::Char('n')
            }
            (Mode::List, ClickTarget::UpdateNotice) if self.update_plan.is_some() => {
                KeyCode::Char('u')
            }
            (Mode::Form(form), ClickTarget::FormField { field, column }) => {
                self.status = None;
                form.click_field(field, column);
                return;
            }
            (Mode::Form(form), ClickTarget::KeyRow(index)) => {
                if form.select_key(index, self.backend.paths()) {
                    self.status = None;
                }
                return;
            }
            (Mode::KeyPicker(picker), ClickTarget::KeyRow(index))
                if index < picker.keys().len() =>
            {
                picker.select(index);
                KeyCode::Enter
            }
            (Mode::Import(import), ClickTarget::SuggestionRow(index))
                if index < self.suggestions.len() =>
            {
                import.selection = index;
                KeyCode::Enter
            }
            (
                Mode::KeyPicker(_) | Mode::Import(_) | Mode::Notice(_) | Mode::Update(_),
                ClickTarget::Outside,
            ) => KeyCode::Esc,
            _ => return,
        };
        self.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    /// Handles one mouse wheel step in the active screen.
    pub fn handle_scroll(&mut self, down: bool) {
        if matches!(
            self.mode,
            Mode::List | Mode::Confirm(_) | Mode::KeyPicker(_) | Mode::Import(_)
        ) {
            let code = if down { KeyCode::Down } else { KeyCode::Up };
            self.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
        }
    }

    /// Inserts a bracketed paste into the focused form input or active filter.
    pub fn handle_paste(&mut self, text: &str) {
        if self.exit.is_some() || self.key_install.is_some() {
            return;
        }
        match &mut self.mode {
            Mode::Form(form) => form.paste(text),
            Mode::List if self.list.filtering => {
                self.list.paste_filter(&self.config, text);
                self.refresh_selected();
            }
            _ => {}
        }
    }

    /// Returns the requested event-loop exit, if any.
    pub fn exit(&self) -> Option<&Exit> {
        self.exit.as_ref()
    }

    /// Returns the installation waiting to run, once.
    pub fn take_key_install(&mut self) -> Option<KeyInstall> {
        self.key_install.as_mut()?.request.take()
    }

    /// Reports the result of the installation taken earlier.
    pub fn finish_key_install(&mut self, result: Result<i32, String>) {
        let Some(PendingKeyInstall { label, after, .. }) = self
            .key_install
            .take_if(|pending| pending.request.is_none())
        else {
            return;
        };
        let error = match result {
            Ok(0) => None,
            Ok(code) => Some(self.lang.key_install_exit(&label, code)),
            Err(error) => Some(self.lang.key_install_error(&label, &error)),
        };
        if let Some(error) = error {
            if matches!(after, KeyInstallAfter::Add) {
                if self
                    .list
                    .selected(&self.config)
                    .is_none_or(|entry| entry.name != label)
                {
                    self.list.clear_filter(&self.config);
                    self.reload_list(Some(&label));
                }
                self.mode = Mode::List;
            }
            self.set_status(StatusLevel::Error, error);
            return;
        }
        match after {
            KeyInstallAfter::Direct => {
                if let Mode::Form(form) = &self.mode {
                    self.connect(form.target());
                }
            }
            KeyInstallAfter::Add => {
                if let Mode::Form(form) = std::mem::replace(&mut self.mode, Mode::List) {
                    self.mode = self.finish_add_host(form, label);
                }
            }
            KeyInstallAfter::Edit(expected) => {
                self.set_status(StatusLevel::Info, self.lang.key_installed(&label));
                self.mode = self.edit_result(&label, &expected);
            }
        }
    }

    fn queue_key_install(&mut self, request: KeyInstall, after: KeyInstallAfter) {
        self.key_install = Some(PendingKeyInstall {
            label: request.label.clone(),
            request: Some(request),
            after,
        });
    }

    /// Returns true once after a native file dialog has closed.
    pub fn take_full_redraw(&mut self) -> bool {
        std::mem::take(&mut self.full_redraw)
    }

    /// Returns the active screen.
    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    /// Returns the most recent status message, if any.
    pub fn status(&self) -> Option<&Status> {
        self.status.as_ref()
    }

    /// Returns the cached update plan for this session.
    pub fn update_plan(&self) -> Option<&UpdatePlan> {
        self.update_plan.as_ref()
    }

    /// Returns the unsaved shell history destinations, newest first.
    pub fn suggestions(&self) -> &[Suggestion] {
        &self.suggestions
    }

    /// Returns the language used for display.
    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// Returns the backend used by this application.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Iterates over the hosts matching the current filter.
    pub fn filtered_hosts(&self) -> impl ExactSizeIterator<Item = HostRow<'_>> {
        self.list.rows(&self.config)
    }

    /// Returns the selected host index in the filtered rows.
    pub fn selection(&self) -> Option<usize> {
        self.list
            .selected(&self.config)
            .map(|_| self.list.selection)
    }

    /// Returns whether the new connection row is selected.
    pub fn new_row_selected(&self) -> bool {
        self.list.new_row_selected()
    }

    /// Returns the filter's value and cursor.
    pub fn filter(&self) -> &TextInput {
        self.list.filter()
    }

    /// Returns whether the list is accepting filter input.
    pub fn is_filtering(&self) -> bool {
        self.list.filtering
    }

    /// Returns the cached effective settings or error for the selected row.
    pub fn selected_effective(&self) -> Option<&Result<EffectiveConfig, String>> {
        self.list.selected_effective(&self.config)
    }

    /// Previews the selected alias's SSH command using the loaded configuration.
    pub fn selected_command(&self) -> Option<String> {
        let entry = self.list.selected(&self.config)?;
        self.loaded_connection_args(&ConnectTarget::Alias(entry.name.clone()))
            .ok()
            .map(|args| preview(&args))
    }

    /// Returns the host count before applying the filter.
    pub fn host_count(&self) -> usize {
        self.config.hosts.len()
    }

    /// Returns the active form, including the one behind the key picker.
    pub fn form(&self) -> Option<&FormState> {
        match &self.mode {
            Mode::Form(form) => Some(form),
            Mode::KeyPicker(picker) => Some(picker.form()),
            _ => None,
        }
    }

    /// Returns the validation error for a field in the active form.
    pub fn validation_error(&self, field: FormField) -> Option<ValidationError> {
        self.form()?.validation_error(field, &self.config.hosts)
    }

    /// Returns the first validation error in form field order.
    pub fn form_first_error(&self) -> Option<(FormField, ValidationError)> {
        self.form()?.first_error(&self.config.hosts)
    }

    /// Returns the live SSH command preview for the active form, if present.
    pub fn form_preview(&self) -> Option<Result<String, String>> {
        self.preview_with(|target| self.connection_args(target))
    }

    /// Previews the command using the loaded config snapshot without checking the filesystem.
    pub(crate) fn loaded_form_preview(&self) -> Option<Result<String, String>> {
        self.preview_with(|target| self.loaded_connection_args(target))
    }

    fn loaded_connection_args(&self, target: &ConnectTarget) -> Result<Vec<OsString>, ArgsError> {
        let paths = self.backend.paths();
        let root = paths.config_file();
        let has_root = self.config.files.iter().any(|file| file.path == root);
        connect_args(paths, &config_file_args(paths, has_root), target)
    }

    fn preview_with(
        &self,
        args: impl FnOnce(&ConnectTarget) -> Result<Vec<OsString>, ArgsError>,
    ) -> Option<Result<String, String>> {
        let form = self.form()?;
        Some(match form.first_error(&self.config.hosts) {
            Some((_, error)) => Err(self.lang.validation_error(&error)),
            None => args(&form.target())
                .map(|args| preview(&args))
                .map_err(|error| error.to_string()),
        })
    }

    /// Returns why the native file dialog is unavailable in this session.
    pub fn picker_unavailable_reason(&self) -> Option<String> {
        self.backend.picker_unavailable_reason()
    }

    /// Returns the platform's instructions for showing hidden files, if needed.
    pub fn hidden_files_hint(&self) -> Option<String> {
        self.backend.hidden_files_hint()
    }

    fn reload(&mut self, preferred: Option<&str>) {
        self.load_config();
        self.reload_list(preferred);
    }

    fn load_config(&mut self) {
        self.config = match self.backend.load_config() {
            Ok(config) => {
                if let Some(first) = config.warnings.first() {
                    self.set_status(
                        StatusLevel::Warning,
                        self.lang.warning_count(first, config.warnings.len()),
                    );
                }
                config
            }
            Err(error) => {
                self.set_status(StatusLevel::Error, error);
                ConfigSet::default()
            }
        };
        self.suggestions = import::suggestions(&self.history_commands, &self.config);
    }

    fn reload_list(&mut self, preferred: Option<&str>) {
        self.list.reload(&self.backend, &self.config, preferred);
        self.refresh_selected();
    }

    fn refresh_selected(&mut self) {
        self.list.refresh_selected(&self.backend, &self.config);
    }

    fn reload_selected(&mut self) {
        let alias = self
            .list
            .selected(&self.config)
            .map(|entry| entry.name.clone());
        self.reload(alias.as_deref());
    }

    fn set_status(&mut self, level: StatusLevel, text: impl Into<String>) {
        self.status = Some(Status {
            level,
            text: text.into(),
        });
    }

    fn connection_args(&self, target: &ConnectTarget) -> Result<Vec<OsString>, ArgsError> {
        connect_args(self.backend.paths(), &self.backend.config_prefix(), target)
    }

    fn connect(&mut self, target: ConnectTarget) {
        match self.connection_args(&target) {
            Ok(args) => {
                if let ConnectTarget::Alias(alias) = &target {
                    self.backend.record_connection(alias);
                }
                self.exit = Some(Exit::Connect(args));
            }
            Err(error) => self.set_status(StatusLevel::Error, error.to_string()),
        }
    }

    fn connect_selected(&mut self) {
        if let Some(entry) = self.list.selected(&self.config) {
            self.connect(ConnectTarget::Alias(entry.name.clone()));
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) -> Mode {
        if !is_plain(key) {
            return Mode::List;
        }
        if self.list.filtering {
            match key.code {
                KeyCode::Enter => self.list.filtering = false,
                KeyCode::Esc => {
                    self.list.filtering = false;
                    self.list.clear_filter(&self.config);
                }
                KeyCode::Up => self.list.select(self.list.selection.saturating_sub(1)),
                KeyCode::Down => self.list.select(self.list.selection + 1),
                KeyCode::Char(_) | KeyCode::Backspace => self.list.edit_filter(&self.config, key),
                _ => {}
            }
            self.refresh_selected();
            return Mode::List;
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.list.select(self.list.selection.saturating_sub(1));
            }
            KeyCode::Down | KeyCode::Char('j') => self.list.select(self.list.selection + 1),
            KeyCode::Home | KeyCode::Char('g') => self.list.select(0),
            KeyCode::End | KeyCode::Char('G') => self.list.select(self.list.last_host_index()),
            KeyCode::Enter if !self.new_row_selected() => self.connect_selected(),
            KeyCode::Enter | KeyCode::Char('n') => return self.new_form(),
            KeyCode::Char('e') => return self.edit_selected(),
            KeyCode::Char('d') => return self.delete_selected(),
            KeyCode::Char('x') => return self.forget_selected(),
            KeyCode::Char('/') => self.list.filtering = true,
            KeyCode::Char('r') => {
                self.reload_selected();
                if self.status.is_none() {
                    self.set_status(StatusLevel::Info, self.lang.text().reloaded);
                }
                return Mode::List;
            }
            KeyCode::Esc if !self.list.filter().value().is_empty() => {
                self.list.clear_filter(&self.config);
            }
            KeyCode::Esc | KeyCode::Char('q') => self.exit = Some(Exit::Quit),
            KeyCode::Char('i') => {
                if self.suggestions.is_empty() {
                    self.set_status(StatusLevel::Info, self.lang.text().no_history_suggestions);
                    return Mode::List;
                }
                return Mode::Import(ImportState::default());
            }
            KeyCode::Char('u') => {
                return self.update_plan.clone().map_or(Mode::List, |plan| {
                    Mode::Update(UpdateState::new(plan, self.lang))
                });
            }
            _ => {}
        }
        self.refresh_selected();
        Mode::List
    }

    fn handle_update_key(&mut self, update: UpdateState, key: KeyEvent) -> Mode {
        if is_plain(key) {
            match key.code {
                KeyCode::Esc => return Mode::List,
                KeyCode::Enter if update.can_run() => {
                    self.exit = Some(Exit::Update(update.plan));
                    return Mode::List;
                }
                _ => {}
            }
        }
        Mode::Update(update)
    }

    fn writable_selection(&mut self) -> Option<HostEntry> {
        let entry = self.list.selected(&self.config)?;
        if self.list.is_read_only(entry) {
            self.set_status(
                StatusLevel::Warning,
                self.lang.path_read_only(&entry.source),
            );
            return None;
        }
        Some(entry.clone())
    }

    fn new_form(&self) -> Mode {
        Mode::Form(FormState::new().with_keys(self.backend.discover_keys()))
    }

    fn edit_selected(&mut self) -> Mode {
        let Some(entry) = self.writable_selection() else {
            return Mode::List;
        };
        let Some(file) = self.config.files.get(entry.file_index) else {
            self.set_status(StatusLevel::Error, self.lang.missing_config(&entry.name));
            return Mode::List;
        };
        match block_fields(&file.parsed, entry.block_index) {
            Ok(original) => {
                let effective = self.list.selected_effective(&self.config);
                let values = effective.and_then(|result| result.as_ref().ok());
                Mode::Form(FormState::edit(
                    entry,
                    original,
                    values,
                    self.backend.discover_keys(),
                ))
            }
            Err(error) => {
                self.set_status(StatusLevel::Error, error.to_string());
                Mode::List
            }
        }
    }

    fn delete_selected(&mut self) -> Mode {
        let Some(entry) = self.writable_selection() else {
            return Mode::List;
        };
        match plan_delete(&self.config, &entry) {
            Ok(pending) => Mode::Confirm(ConfirmState::new(
                self.lang,
                pending,
                entry.name,
                ConfirmAction::Delete,
            )),
            Err(error) => {
                self.set_status(StatusLevel::Error, error.to_string());
                Mode::List
            }
        }
    }

    fn forget_selected(&mut self) -> Mode {
        let Some(entry) = self.list.selected(&self.config) else {
            return Mode::List;
        };
        let alias = entry.name.clone();
        let name = match self.list.effective(&self.backend, &alias) {
            Ok(effective) => known_hosts::lookup_name_for(&alias, effective),
            Err(error) => {
                let message = self.lang.cannot_resolve(&alias, error);
                self.set_status(StatusLevel::Error, message);
                return Mode::List;
            }
        };
        match self.backend.plan_forget(&name) {
            Ok(Some(plan)) => Mode::Confirm(ConfirmState::new(
                self.lang,
                plan.pending,
                alias,
                ConfirmAction::Forget {
                    name: plan.name,
                    removed: plan.removed,
                },
            )),
            Ok(None) => {
                let paths = self.backend.paths();
                let path = paths.abbreviate_home(&known_hosts::file_path(paths));
                self.set_status(
                    StatusLevel::Info,
                    self.lang.no_known_host_entry(&name, &path),
                );
                Mode::List
            }
            Err(error) => {
                self.set_status(StatusLevel::Error, error);
                Mode::List
            }
        }
    }

    fn handle_form_key(&mut self, mut form: FormState, key: KeyEvent) -> Mode {
        if is_ctrl(key, 'l') || (is_plain(key) && key.code == KeyCode::F(2)) {
            let keys = self.backend.discover_keys();
            if !keys.is_empty() {
                return Mode::KeyPicker(KeyPickerState::new(form, keys, self.lang));
            }
            self.set_status(
                StatusLevel::Warning,
                self.lang.no_private_keys(self.backend.paths().ssh_dir()),
            );
        } else if is_ctrl(key, 'o') || (is_plain(key) && key.code == KeyCode::F(3)) {
            self.browse(&mut form);
        } else if is_plain(key) {
            match key.code {
                KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    form.previous_field()
                }
                KeyCode::BackTab => form.previous_field(),
                KeyCode::Tab => form.next_field(),
                KeyCode::Up | KeyCode::Down => {
                    form.move_vertical(key.code == KeyCode::Down, self.backend.paths());
                }
                KeyCode::Enter => return self.submit(form),
                KeyCode::Esc => {
                    if self.config.hosts.is_empty() && !form.is_from_history() {
                        self.exit = Some(Exit::Quit);
                        return Mode::Form(form);
                    }
                    return Mode::List;
                }
                _ => form.handle_input(key),
            }
        } else {
            form.handle_input(key);
        }
        Mode::Form(form)
    }

    fn browse(&mut self, form: &mut FormState) {
        if let Some(reason) = self.picker_unavailable_reason() {
            self.set_status(StatusLevel::Warning, reason);
            return;
        }
        let outcome = self.backend.pick_file();
        self.full_redraw = true;
        match outcome {
            Ok(PickOutcome::Picked(path)) => {
                form.set_key_file(self.backend.paths().abbreviate_home(&path));
            }
            Ok(PickOutcome::Cancelled) => {}
            Err(error) => self.set_status(StatusLevel::Error, error.to_string()),
        }
    }

    fn submit(&mut self, mut form: FormState) -> Mode {
        if let Some((field, error)) = form.first_error(&self.config.hosts) {
            form.set_focus(field);
            self.set_status(StatusLevel::Error, self.lang.validation_error(&error));
            return Mode::Form(form);
        }

        let public_key = if form.is_checked(FormField::InstallKey) {
            let result = form
                .install_key_file()
                .ok_or_else(|| self.lang.text().needs_key_file.into())
                .and_then(|path| self.backend.read_public_key(path));
            match result {
                Ok(key) => Some(key),
                Err(error) => {
                    form.set_focus(FormField::KeyFile);
                    self.set_status(StatusLevel::Error, error);
                    return Mode::Form(form);
                }
            }
        } else {
            None
        };

        if matches!(form.kind(), FormKind::New) && !form.value(FormField::Nickname).is_empty() {
            return self.add_host(form, public_key);
        }

        let target = form.target();
        let install = match public_key {
            Some(key) => match self.build_key_install(&target, key) {
                Ok(install) => Some(install),
                Err(error) => {
                    self.set_status(StatusLevel::Error, error.to_string());
                    return Mode::Form(form);
                }
            },
            None => None,
        };
        let nickname = form.value(FormField::Nickname);
        match form.kind() {
            FormKind::New => {
                if let Some(install) = install {
                    self.queue_key_install(install, KeyInstallAfter::Direct);
                } else {
                    self.connect(target);
                }
                Mode::Form(form)
            }
            FormKind::Edit { entry, original } => {
                let new_name = (entry.name != nickname).then_some(nickname);
                let changes = form.option_changes(original);
                match plan_edit(&self.config, entry, new_name, &changes) {
                    Ok(pending) if pending.is_noop() => {
                        if let Some(install) = install {
                            self.queue_key_install(install, KeyInstallAfter::Edit(form.expected()));
                        } else {
                            self.set_status(StatusLevel::Info, self.lang.text().no_changes);
                        }
                        Mode::List
                    }
                    Ok(pending) => Mode::Confirm(ConfirmState::new(
                        self.lang,
                        pending,
                        nickname.to_owned(),
                        ConfirmAction::Edit {
                            form,
                            install: install.map(Box::new),
                        },
                    )),
                    Err(error) => {
                        self.set_status(StatusLevel::Error, error.to_string());
                        Mode::Form(form)
                    }
                }
            }
        }
    }

    fn build_key_install(
        &self,
        target: &ConnectTarget,
        public_key: String,
    ) -> Result<KeyInstall, ArgsError> {
        let label = match target {
            ConnectTarget::Alias(alias) => alias,
            ConnectTarget::Direct { address, .. } => address,
        };
        let args =
            keysetup::install_args(self.backend.paths(), &self.backend.config_prefix(), target)?;
        Ok(KeyInstall {
            label: label.clone(),
            args,
            public_key,
        })
    }

    fn add_host(&mut self, form: FormState, public_key: Option<String>) -> Mode {
        let host = form.new_host();
        let pending = match plan_add(self.backend.paths(), &self.config, &host) {
            Ok(pending) => pending,
            Err(error) => {
                self.set_status(StatusLevel::Error, error.to_string());
                return Mode::Form(form);
            }
        };

        match self.backend.commit(&pending) {
            Ok(_) => {
                self.reload(Some(&host.name));
                if let Some(key) = public_key {
                    match self.build_key_install(&ConnectTarget::Alias(host.name), key) {
                        Ok(install) => {
                            self.queue_key_install(install, KeyInstallAfter::Add);
                            Mode::Form(form)
                        }
                        Err(error) => {
                            self.set_status(StatusLevel::Error, error.to_string());
                            Mode::List
                        }
                    }
                } else {
                    self.finish_add_host(form, host.name)
                }
            }
            Err(WriteError::Stale { path }) => {
                self.reload_selected();
                self.set_status(StatusLevel::Error, self.lang.stale_add(&path));
                Mode::Form(form)
            }
            Err(error) => {
                self.set_status(StatusLevel::Error, error.to_string());
                Mode::Form(form)
            }
        }
    }

    fn finish_add_host(&mut self, form: FormState, alias: String) -> Mode {
        let mismatches = self.mismatch_lines(&alias, &form.expected());
        if mismatches.is_empty() {
            self.connect(ConnectTarget::Alias(alias));
            Mode::Form(form)
        } else {
            Mode::Notice(NoticeState::new(mismatches, Some(alias)).with_lang(self.lang))
        }
    }

    fn handle_confirm_key(&mut self, mut confirm: ConfirmState, key: KeyEvent) -> Mode {
        if is_plain(key) {
            match key.code {
                KeyCode::Char('y') => return self.commit_confirmed(confirm),
                KeyCode::Enter if confirm.choice() != Some(ConfirmChoice::Cancel) => {
                    return self.commit_confirmed(confirm);
                }
                KeyCode::Char('n') | KeyCode::Esc | KeyCode::Enter => {
                    return match confirm.action {
                        ConfirmAction::Edit { form, .. } => Mode::Form(form),
                        ConfirmAction::Delete | ConfirmAction::Forget { .. } => Mode::List,
                    };
                }
                KeyCode::Left
                | KeyCode::Right
                | KeyCode::Tab
                | KeyCode::BackTab
                | KeyCode::Char('h' | 'l') => confirm.toggle_choice(),
                KeyCode::Up => confirm.up(false),
                KeyCode::Down => confirm.down(false),
                KeyCode::PageUp => confirm.up(true),
                KeyCode::PageDown => confirm.down(true),
                _ => {}
            }
        }
        Mode::Confirm(confirm)
    }

    fn commit_confirmed(&mut self, confirm: ConfirmState) -> Mode {
        let ConfirmState {
            pending,
            action,
            alias,
            ..
        } = confirm;
        let result = self.backend.commit(&pending);
        let edit = match action {
            ConfirmAction::Edit { form, install } => Some((form, install)),
            ConfirmAction::Delete => None,
            ConfirmAction::Forget { name, removed } => {
                return self.finish_forget(result, &name, removed);
            }
        };
        match result {
            Ok(outcome) => {
                self.reload(Some(&alias));
                if self.status.is_none() {
                    self.set_status(
                        StatusLevel::Info,
                        self.lang
                            .confirmed(edit.is_some(), &alias, outcome.backup.as_deref()),
                    );
                }
                if let Some((form, install)) = edit {
                    let expected = form.expected();
                    if let Some(install) = install {
                        self.queue_key_install(*install, KeyInstallAfter::Edit(expected));
                    } else {
                        return self.edit_result(&alias, &expected);
                    }
                }
            }
            Err(WriteError::Stale { path }) => {
                self.reload_selected();
                self.set_status(StatusLevel::Error, self.lang.stale_confirm(&path));
            }
            Err(error) => self.set_status(StatusLevel::Error, error.to_string()),
        }
        Mode::List
    }

    fn edit_result(&mut self, alias: &str, expected: &ExpectedConfig) -> Mode {
        let mismatches = self.mismatch_lines(alias, expected);
        if mismatches.is_empty() {
            Mode::List
        } else {
            Mode::Notice(NoticeState::new(mismatches, None).with_lang(self.lang))
        }
    }

    fn finish_forget(
        &mut self,
        result: Result<WriteOutcome, WriteError>,
        name: &str,
        removed: usize,
    ) -> Mode {
        match result {
            Ok(outcome) => {
                self.set_status(
                    StatusLevel::Info,
                    self.lang
                        .removed_entries(removed, name, outcome.backup.as_deref()),
                );
            }
            Err(WriteError::Stale { path }) => {
                self.set_status(StatusLevel::Error, self.lang.stale_forget(&path))
            }
            Err(error) => self.set_status(StatusLevel::Error, error.to_string()),
        }
        Mode::List
    }

    fn mismatch_lines(&mut self, alias: &str, expected: &ExpectedConfig) -> Vec<String> {
        let Ok(effective) = self.list.effective(&self.backend, alias) else {
            return Vec::new();
        };
        find_mismatches(self.backend.paths(), expected, effective)
            .into_iter()
            .map(|mismatch| self.lang.mismatch(&mismatch))
            .collect()
    }

    fn handle_picker_key(&self, mut picker: KeyPickerState, key: KeyEvent) -> Mode {
        if is_plain(key) {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => picker.up(),
                KeyCode::Down | KeyCode::Char('j') => picker.down(),
                KeyCode::Enter => {
                    if let Some(key) = picker.keys().get(picker.selection()) {
                        let value = self.backend.paths().abbreviate_home(&key.path);
                        picker.form.set_key_file(value);
                        picker.form.set_focus(FormField::KeyFile);
                    }
                    return Mode::Form(picker.form);
                }
                KeyCode::Esc => return Mode::Form(picker.form),
                _ => {}
            }
        }
        Mode::KeyPicker(picker)
    }

    fn handle_import_key(&self, mut import: ImportState, key: KeyEvent) -> Mode {
        if is_plain(key) {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    import.selection = import.selection.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    import.selection =
                        (import.selection + 1).min(self.suggestions.len().saturating_sub(1));
                }
                KeyCode::Enter => {
                    if let Some(suggestion) = self.suggestions.get(import.selection()) {
                        return Mode::Form(
                            FormState::from_history(&suggestion.invocation)
                                .with_keys(self.backend.discover_keys()),
                        );
                    }
                }
                KeyCode::Esc => return Mode::List,
                _ => {}
            }
        }
        Mode::Import(import)
    }

    fn handle_notice_key(&mut self, notice: NoticeState, key: KeyEvent) -> Mode {
        if is_plain(key) {
            match key.code {
                KeyCode::Enter => {
                    if let Some(alias) = notice.connect_alias {
                        self.connect(ConnectTarget::Alias(alias));
                    }
                    return Mode::List;
                }
                KeyCode::Esc => return Mode::List,
                _ => {}
            }
        }
        Mode::Notice(notice)
    }
}

#[cfg(test)]
mod tests;
