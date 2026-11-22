use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tempfile::{TempDir, tempdir};

use super::*;
use crate::config::{DiffTag, PendingWrite, ReadWarning, WriteOutcome, load};
use crate::dialog::DialogError;
use crate::keys::KeyInfo;
use crate::known_hosts::ForgetPlan;
use crate::paths::SshPaths;
use crate::ssh::args::config_file_args;
use crate::update::InstallMethod;
use crate::validate::Field;

const HOSTS: &str = "Host web\n  HostName web.example.com\nHost db\nHost queue\n";
const EDITABLE: &str = "Host web\n  HostName example.com\n  User alice\n";
const MULTIPLE_KEYS: &str = "Host web\n  IdentityFile ~/first\n  IdentityFile ~/second\n";

enum CommitBehavior {
    Success,
    Stale,
    IoFailure,
    WriteFile,
}

struct FakeBackend {
    paths: SshPaths,
    _temp: TempDir,
    loaded: RefCell<Result<ConfigSet, String>>,
    load_calls: Cell<usize>,
    prefix: RefCell<Vec<OsString>>,
    live_prefix: bool,
    effective_values: RefCell<HashMap<String, Result<EffectiveConfig, String>>>,
    effective_calls: RefCell<Vec<String>>,
    writable: Cell<bool>,
    writable_calls: RefCell<Vec<PathBuf>>,
    forget_plan: Result<Option<ForgetPlan>, String>,
    forget_calls: RefCell<Vec<String>>,
    writes: RefCell<Vec<PendingWrite>>,
    commit_behaviors: RefCell<VecDeque<CommitBehavior>>,
    keys: Vec<KeyInfo>,
    pick: RefCell<Option<Result<PickOutcome, DialogError>>>,
    pick_calls: Cell<usize>,
    unavailable: Option<String>,
    hidden_hint: Option<String>,
    update_plan: Option<UpdatePlan>,
    update_plan_calls: Cell<usize>,
}

impl FakeBackend {
    fn new(contents: &str) -> Self {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let ssh_dir = home.join(".ssh");
        let paths = SshPaths::resolve(Some(ssh_dir.as_os_str()), Some(&home), temp.path()).unwrap();
        if !contents.is_empty() {
            fs::create_dir_all(&ssh_dir).unwrap();
            fs::write(paths.config_file(), contents).unwrap();
        }
        let config = load(&paths).unwrap();
        Self {
            paths,
            _temp: temp,
            loaded: RefCell::new(Ok(config)),
            load_calls: Cell::new(0),
            prefix: RefCell::default(),
            live_prefix: false,
            effective_values: RefCell::default(),
            effective_calls: RefCell::default(),
            writable: Cell::new(true),
            writable_calls: RefCell::default(),
            forget_plan: Ok(None),
            forget_calls: RefCell::default(),
            writes: RefCell::default(),
            commit_behaviors: RefCell::default(),
            keys: Vec::new(),
            pick: RefCell::default(),
            pick_calls: Cell::new(0),
            unavailable: None,
            hidden_hint: None,
            update_plan: None,
            update_plan_calls: Cell::new(0),
        }
    }

    fn set_effective(&self, alias: &str, effective: EffectiveConfig) {
        self.effective_values
            .borrow_mut()
            .insert(alias.into(), Ok(effective));
    }

    fn effective_count(&self, alias: &str) -> usize {
        self.effective_calls
            .borrow()
            .iter()
            .filter(|name| *name == alias)
            .count()
    }

    fn commit_with(&self, behavior: CommitBehavior) {
        self.commit_behaviors.borrow_mut().push_back(behavior);
    }

    fn with_keys(mut self) -> Self {
        self.keys = ["id_a", "id_b"]
            .into_iter()
            .map(|name| KeyInfo {
                path: self.paths.ssh_dir().join(name),
                file_name: name.into(),
                public: None,
            })
            .collect();
        self
    }
}

impl Backend for FakeBackend {
    fn update_plan(&self) -> Option<UpdatePlan> {
        self.update_plan_calls.set(self.update_plan_calls.get() + 1);
        self.update_plan.clone()
    }

    fn paths(&self) -> &SshPaths {
        &self.paths
    }

    fn config_prefix(&self) -> Vec<OsString> {
        if self.live_prefix {
            config_file_args(&self.paths, self.paths.config_file().is_file())
        } else {
            self.prefix.borrow().clone()
        }
    }

    fn load_config(&self) -> Result<ConfigSet, String> {
        self.load_calls.set(self.load_calls.get() + 1);
        self.loaded.borrow().clone()
    }

    fn effective(&self, alias: &str) -> Result<EffectiveConfig, String> {
        self.effective_calls.borrow_mut().push(alias.into());
        self.effective_values
            .borrow()
            .get(alias)
            .cloned()
            .unwrap_or_else(|| Err("effective config unavailable".into()))
    }

    fn discover_keys(&self) -> Vec<KeyInfo> {
        self.keys.clone()
    }

    fn is_writable(&self, path: &Path) -> bool {
        self.writable_calls.borrow_mut().push(path.into());
        self.writable.get()
    }

    fn plan_forget(&self, name: &str) -> Result<Option<ForgetPlan>, String> {
        self.forget_calls.borrow_mut().push(name.into());
        self.forget_plan.clone()
    }

    fn commit(&self, pending: &PendingWrite) -> Result<WriteOutcome, WriteError> {
        self.writes.borrow_mut().push(pending.clone());
        match self.commit_behaviors.borrow_mut().pop_front() {
            Some(CommitBehavior::Stale) => Err(WriteError::Stale {
                path: pending.path.clone(),
            }),
            Some(CommitBehavior::IoFailure) => Err(WriteError::Io {
                action: "write",
                path: pending.path.clone(),
                source: io::Error::other("fake write failure"),
            }),
            Some(CommitBehavior::WriteFile) => {
                let outcome = pending.commit()?;
                *self.loaded.borrow_mut() = load(&self.paths).map_err(|e| e.to_string());
                Ok(outcome)
            }
            Some(CommitBehavior::Success) | None => Ok(WriteOutcome {
                target: pending.path.clone(),
                backup: pending
                    .original
                    .as_ref()
                    .map(|_| pending.path.with_extension("backup")),
            }),
        }
    }

    fn picker_unavailable_reason(&self) -> Option<String> {
        self.unavailable.clone()
    }

    fn hidden_files_hint(&self) -> Option<String> {
        self.hidden_hint.clone()
    }

    fn pick_file(&self) -> Result<PickOutcome, DialogError> {
        self.pick_calls.set(self.pick_calls.get() + 1);
        self.pick.take().unwrap_or(Ok(PickOutcome::Cancelled))
    }
}

type TestApp = App<FakeBackend>;

fn key(app: &mut TestApp, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn ch(app: &mut TestApp, input: char) {
    key(app, KeyCode::Char(input));
}

fn ctrl(app: &mut TestApp, input: char) {
    app.handle_key(KeyEvent::new(KeyCode::Char(input), KeyModifiers::CONTROL));
}

fn set_field(app: &mut TestApp, field: FormField, value: &str) {
    while app.form().unwrap().focus() != field {
        key(app, KeyCode::Tab);
    }
    ctrl(app, 'u');
    app.handle_paste(value);
}

fn value(app: &TestApp, field: FormField) -> &str {
    app.form().unwrap().input(field).value()
}

fn names(app: &TestApp) -> Vec<&str> {
    app.filtered_hosts()
        .map(|row| row.entry.name.as_str())
        .collect()
}

fn status(app: &TestApp, level: StatusLevel) -> &str {
    let status = app.status().unwrap();
    assert_eq!(status.level, level);
    &status.text
}

fn confirm(app: &TestApp) -> &ConfirmState {
    let Mode::Confirm(confirm) = app.mode() else {
        panic!("expected confirm, got {:?}", app.mode());
    };
    confirm
}

fn notice(app: &TestApp) -> &NoticeState {
    let Mode::Notice(notice) = app.mode() else {
        panic!("expected notice, got {:?}", app.mode());
    };
    notice
}

fn fill_named_host(app: &mut TestApp, name: &str) {
    set_field(app, FormField::Nickname, name);
    set_field(app, FormField::User, "alice");
    set_field(app, FormField::Address, "example.com");
}

fn basic_expected() -> EffectiveConfig {
    EffectiveConfig {
        user: Some("alice".into()),
        host_name: Some("example.com".into()),
        ..EffectiveConfig::default()
    }
}

#[test]
fn empty_config_starts_in_new_form_and_escape_quits() {
    let mut app = App::new(FakeBackend::new(""));
    assert!(matches!(app.form().unwrap().kind(), FormKind::New));
    for field in FormField::ALL {
        assert_eq!(value(&app, field), "");
        assert_eq!(app.form().unwrap().placeholder(field), None);
    }
    assert_eq!(app.host_count(), 0);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.exit(), Some(&Exit::Quit));
}

#[test]
fn list_navigation_stops_at_boundaries_and_accepts_alias_keys() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(names(&app), ["web", "db", "queue"]);
    assert_eq!(app.host_count(), 3);
    assert_eq!(app.filtered_hosts().next().unwrap().entry.line_number, 1);
    key(&mut app, KeyCode::Up);
    assert_eq!(app.selection(), Some(0));
    ch(&mut app, 'j');
    assert_eq!(app.selection(), Some(1));
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Down);
    assert_eq!(app.selection(), Some(2));
    ch(&mut app, 'k');
    assert_eq!(app.selection(), Some(1));
    ch(&mut app, 'g');
    assert_eq!(app.selection(), Some(0));
    ch(&mut app, 'G');
    assert_eq!(app.selection(), Some(2));
    key(&mut app, KeyCode::Home);
    assert_eq!(app.selection(), Some(0));
}

#[test]
fn list_enter_connects_using_the_current_prefix() {
    for prefix in [Vec::new(), vec!["-F".into(), "config with spaces".into()]] {
        let mut app = App::new(FakeBackend::new(HOSTS));
        app.backend().prefix.replace(prefix.clone());
        key(&mut app, KeyCode::Enter);
        let mut expected = prefix;
        expected.push("web".into());
        assert_eq!(app.exit(), Some(&Exit::Connect(expected)));
    }
}

#[test]
fn escape_quits_the_unfiltered_list() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.exit(), Some(&Exit::Quit));
}

#[test]
fn escape_clears_an_applied_filter_then_quits() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, '/');
    ch(&mut app, 'w');
    key(&mut app, KeyCode::Enter);
    assert_eq!(names(&app), ["web"]);

    key(&mut app, KeyCode::Esc);
    assert_eq!(app.filter().value(), "");
    assert_eq!(names(&app), ["web", "db", "queue"]);
    assert_eq!(app.exit(), None);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.exit(), Some(&Exit::Quit));
}

#[test]
fn escape_clears_filter_input_and_leaves_the_list_open() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, '/');
    ch(&mut app, 'w');
    assert!(app.is_filtering());

    key(&mut app, KeyCode::Esc);
    assert_eq!(app.filter().value(), "");
    assert!(!app.is_filtering());
    assert_eq!(app.exit(), None);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.exit(), Some(&Exit::Quit));
}

#[test]
fn escape_closes_the_key_popup_then_returns_from_the_form() {
    let mut app = App::new(FakeBackend::new(HOSTS).with_keys());
    ch(&mut app, 'n');
    key(&mut app, KeyCode::F(2));
    assert!(matches!(app.mode(), Mode::KeyPicker(_)));

    key(&mut app, KeyCode::Esc);
    assert!(matches!(app.mode(), Mode::Form(_)));
    assert_eq!(app.exit(), None);
    key(&mut app, KeyCode::Esc);
    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(app.exit(), None);
}

#[test]
fn q_still_quits_the_unfiltered_list() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, 'q');
    assert_eq!(app.exit(), Some(&Exit::Quit));
}

#[test]
fn clicking_escape_clears_filter_input_or_an_applied_filter_before_quitting() {
    for applied in [false, true] {
        let mut app = App::new(FakeBackend::new(HOSTS));
        ch(&mut app, '/');
        ch(&mut app, 'w');
        if applied {
            key(&mut app, KeyCode::Enter);
        }

        app.handle_click(ClickTarget::Key(KeyCode::Esc), false);
        assert_eq!(app.filter().value(), "");
        assert!(!app.is_filtering());
        assert_eq!(app.exit(), None);
        app.handle_click(ClickTarget::Key(KeyCode::Esc), false);
        assert_eq!(app.exit(), Some(&Exit::Quit));
    }
}

#[test]
fn filter_uses_case_insensitive_substrings_and_owns_command_characters() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, 'G');
    ch(&mut app, '/');
    assert!(app.is_filtering());
    ch(&mut app, 'U');
    assert_eq!(names(&app), ["queue"]);
    assert_eq!(app.selection(), Some(0));
    ch(&mut app, 'q');
    assert_eq!(app.filter().value(), "Uq");
    assert!(names(&app).is_empty());
    assert_eq!(app.selection(), None);
    assert_eq!(app.exit(), None);
    key(&mut app, KeyCode::Backspace);
    key(&mut app, KeyCode::Enter);
    assert!(!app.is_filtering());
    assert_eq!(app.filter().value(), "U");
    assert_eq!(names(&app), ["queue"]);
    key(&mut app, KeyCode::Esc);
    assert_eq!(names(&app), ["web", "db", "queue"]);
    ch(&mut app, '/');
    ch(&mut app, 'w');
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.filter().value(), "");
    assert!(!app.is_filtering());

    ch(&mut app, '/');
    ch(&mut app, 'w');
    key(&mut app, KeyCode::Enter);
    ch(&mut app, 'q');
    assert_eq!(app.filter().value(), "w");
    assert_eq!(app.exit(), Some(&Exit::Quit));
}

#[test]
fn filter_arrows_move_and_no_matching_host_cannot_connect() {
    let mut app = App::new(FakeBackend::new("Host web web2 db\n"));
    ch(&mut app, '/');
    ch(&mut app, 'w');
    key(&mut app, KeyCode::Down);
    assert_eq!(app.selection(), Some(1));
    key(&mut app, KeyCode::Up);
    assert_eq!(app.selection(), Some(0));
    ch(&mut app, 'k');
    assert_eq!(app.filter().value(), "wk");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.exit(), None);
    assert!(matches!(app.mode(), Mode::List));
}

#[test]
fn caches_effective_successes_and_failures_and_writability_until_reload() {
    let backend = FakeBackend::new(HOSTS);
    backend.set_effective("web", basic_expected());
    let mut app = App::new(backend);
    for _ in 0..2 {
        assert_eq!(app.selected_effective(), Some(&Ok(basic_expected())));
        ch(&mut app, 'j');
        assert_eq!(
            app.selected_effective(),
            Some(&Err("effective config unavailable".into()))
        );
        ch(&mut app, 'k');
    }
    assert_eq!(app.backend().effective_count("web"), 1);
    assert_eq!(app.backend().effective_count("db"), 1);
    assert_eq!(app.backend().writable_calls.borrow().len(), 1);
    ch(&mut app, 'r');
    assert_eq!(status(&app, StatusLevel::Info), "Reloaded");
    assert_eq!(app.backend().effective_count("web"), 2);
    ch(&mut app, 'j');
    assert_eq!(app.backend().effective_count("db"), 2);
    assert_eq!(app.backend().writable_calls.borrow().len(), 2);
}

#[test]
fn read_only_hosts_cannot_be_edited_or_deleted_but_can_connect() {
    let backend = FakeBackend::new(HOSTS);
    backend.writable.set(false);
    let mut app = App::new(backend);
    assert!(app.filtered_hosts().all(|row| row.read_only));
    for command in ['e', 'd'] {
        ch(&mut app, command);
        assert!(matches!(app.mode(), Mode::List));
        assert_eq!(
            status(&app, StatusLevel::Warning),
            format!(
                "{} is read-only",
                app.backend().paths().config_file().display()
            )
        );
    }
    assert_eq!(app.backend().writable_calls.borrow().len(), 1);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.exit(), Some(&Exit::Connect(vec!["web".into()])));
}

#[test]
fn release_is_ignored_repeat_is_handled_and_ctrl_c_exits_any_screen() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    app.handle_key(KeyEvent::new_with_kind(
        KeyCode::Char('q'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));
    assert_eq!(app.exit(), None);
    app.handle_key(KeyEvent::new_with_kind(
        KeyCode::Down,
        KeyModifiers::NONE,
        KeyEventKind::Repeat,
    ));
    assert_eq!(app.selection(), Some(1));
    ch(&mut app, '/');
    ctrl(&mut app, 'c');
    assert_eq!(app.exit(), Some(&Exit::Quit));

    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, 'q');
    assert_eq!(app.exit(), Some(&Exit::Quit));

    for mode in [
        Mode::Form(FormState::new()),
        Mode::Notice(NoticeState::new(vec![], None)),
    ] {
        let mut app = App::new(FakeBackend::new(HOSTS));
        app.mode = mode;
        ctrl(&mut app, 'c');
        assert_eq!(app.exit(), Some(&Exit::Quit));
    }

    let mut app = App::new(FakeBackend::new(HOSTS).with_keys());
    ch(&mut app, 'n');
    ctrl(&mut app, 'l');
    assert!(matches!(app.mode(), Mode::KeyPicker(_)));
    ctrl(&mut app, 'c');
    assert_eq!(app.exit(), Some(&Exit::Quit));

    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, 'd');
    assert!(matches!(app.mode(), Mode::Confirm(_)));
    ctrl(&mut app, 'c');
    assert_eq!(app.exit(), Some(&Exit::Quit));
}

#[test]
fn load_failure_and_include_warnings_are_reported() {
    let broken = FakeBackend::new(HOSTS);
    *broken.loaded.borrow_mut() = Err("cannot load config".into());
    let app = App::new(broken);
    assert_eq!(status(&app, StatusLevel::Error), "cannot load config");
    assert_eq!(app.host_count(), 0);
    assert!(matches!(app.mode(), Mode::Form(_)));

    let backend = FakeBackend::new(HOSTS);
    backend.loaded.borrow_mut().as_mut().unwrap().warnings = vec![
        ReadWarning::UnsupportedInclude {
            file: backend.paths.config_file(),
            line_number: 8,
            pattern: "${USER}/config".into(),
        },
        ReadWarning::DepthExceeded {
            path: backend.paths.config_file(),
        },
    ];
    let mut app = App::new(backend);
    let warning = status(&app, StatusLevel::Warning);
    assert!(warning.contains("${USER}/config"));
    assert!(warning.contains("2 warnings"));
    ch(&mut app, 'r');
    assert_eq!(app.status().unwrap().level, StatusLevel::Warning);
    *app.backend().loaded.borrow_mut() = Err("cannot reload config".into());
    ch(&mut app, 'r');
    assert_eq!(status(&app, StatusLevel::Error), "cannot reload config");
    assert_eq!(app.host_count(), 0);
    assert!(matches!(app.mode(), Mode::List));
}

#[test]
fn status_is_cleared_by_the_next_handled_key() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, 'r');
    assert!(app.status().is_some());
    app.handle_key(KeyEvent::new_with_kind(
        KeyCode::Down,
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));
    assert!(app.status().is_some());
    key(&mut app, KeyCode::Down);
    assert_eq!(app.status(), None);
}

#[test]
fn form_focus_wraps_in_both_directions() {
    let mut app = App::new(FakeBackend::new(""));
    for expected in FormField::ALL.into_iter().cycle().skip(1).take(5) {
        key(&mut app, KeyCode::Tab);
        assert_eq!(app.form().unwrap().focus(), expected);
    }
    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(app.form().unwrap().focus(), FormField::KeyFile);
    key(&mut app, KeyCode::Down);
    assert_eq!(app.form().unwrap().focus(), FormField::Nickname);
    key(&mut app, KeyCode::Up);
    assert_eq!(app.form().unwrap().focus(), FormField::KeyFile);
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT));
    assert_eq!(app.form().unwrap().focus(), FormField::Port);
}

#[test]
fn unnamed_form_connects_directly_without_writing() {
    let mut app = App::new(FakeBackend::new(""));
    set_field(&mut app, FormField::User, "alice");
    set_field(&mut app, FormField::Address, "example.com");
    set_field(&mut app, FormField::Port, "2222");
    set_field(&mut app, FormField::KeyFile, "~/.ssh/id");
    key(&mut app, KeyCode::Enter);
    assert_eq!(
        app.exit(),
        Some(&Exit::Connect(vec![
            "-p".into(),
            "2222".into(),
            "-i".into(),
            app.backend().paths().ssh_dir().join("id").into_os_string(),
            "alice@example.com".into(),
        ]))
    );
    assert!(app.backend().writes.borrow().is_empty());
}

#[test]
fn empty_address_is_required_and_receives_focus() {
    let mut app = App::new(FakeBackend::new(""));
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.exit(), None);
    assert_eq!(status(&app, StatusLevel::Error), "address is required");
    assert_eq!(app.form().unwrap().focus(), FormField::Address);
}

#[test]
fn invalid_input_prevents_submission_and_focuses_the_first_error() {
    let mut app = App::new(FakeBackend::new(""));
    for (field, bad, good, message) in [
        (FormField::User, "-alice", "alice", "user must not start"),
        (
            FormField::Address,
            "-host",
            "example.com",
            "address must not start",
        ),
        (FormField::Port, "65536", "", "port must be a number"),
    ] {
        set_field(&mut app, field, bad);
        set_field(&mut app, FormField::KeyFile, "");
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.exit(), None);
        assert_eq!(app.form().unwrap().focus(), field);
        assert!(status(&app, StatusLevel::Error).contains(message));
        assert!(app.backend().writes.borrow().is_empty());
        set_field(&mut app, field, good);
    }
}

#[test]
fn named_form_commits_the_block_and_connects_if_values_match() {
    let backend = FakeBackend::new("");
    backend.set_effective("web", basic_expected());
    let mut app = App::new(backend);
    fill_named_host(&mut app, "web");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.exit(), Some(&Exit::Connect(vec!["web".into()])));
    let writes = app.backend().writes.borrow();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].path, app.backend().paths().config_file());
    let text = str::from_utf8(&writes[0].updated).unwrap();
    assert!(text.starts_with("Host web\n"));
    assert!(text.contains("User alice"));
    assert!(text.contains("HostName example.com"));
    assert!(!text.contains("Port"));
    assert!(!text.contains("IdentityFile"));
    assert_eq!(app.backend().load_calls.get(), 2);
    assert_eq!(app.backend().effective_count("web"), 1);
}

#[test]
fn a_new_alias_is_checked_against_unfiltered_hosts() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, '/');
    ch(&mut app, 'q');
    key(&mut app, KeyCode::Enter);
    assert_eq!(names(&app), ["queue"]);
    ch(&mut app, 'n');
    fill_named_host(&mut app, "web");
    key(&mut app, KeyCode::Enter);
    assert!(app.backend().writes.borrow().is_empty());
    assert_eq!(app.exit(), None);
    assert_eq!(app.form().unwrap().focus(), FormField::Nickname);
    assert!(status(&app, StatusLevel::Error).contains("already in use"));
}

#[test]
fn newly_saved_mismatches_can_connect_or_return_to_list() {
    for response in [KeyCode::Enter, KeyCode::Esc] {
        let backend = FakeBackend::new("");
        backend.commit_with(CommitBehavior::WriteFile);
        backend.set_effective(
            "web",
            EffectiveConfig {
                user: Some("bob".into()),
                ..basic_expected()
            },
        );
        let mut app = App::new(backend);
        fill_named_host(&mut app, "web");
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.exit(), None);
        let notice = notice(&app);
        assert!(notice.title().contains("effective SSH settings"));
        assert_eq!(
            notice.lines(),
            ["User: saved \"alice\" but ssh will use \"bob\""]
        );
        assert_eq!(app.backend().effective_count("web"), 1);
        key(&mut app, response);
        assert!(matches!(app.mode(), Mode::List));
        assert_eq!(names(&app), ["web"]);
        let connect = Exit::Connect(vec!["web".into()]);
        assert_eq!(
            app.exit(),
            if response == KeyCode::Enter {
                Some(&connect)
            } else {
                None
            }
        );
    }
}

#[test]
fn stale_new_write_reloads_and_allows_retry_with_the_same_inputs() {
    let mut backend = FakeBackend::new("");
    backend.live_prefix = true;
    backend.commit_with(CommitBehavior::Stale);
    backend.commit_with(CommitBehavior::WriteFile);
    let mut app = App::new(backend);
    fill_named_host(&mut app, "web");
    assert_eq!(app.form_preview(), Some(Ok("ssh -F none web".into())));
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.exit(), None);
    assert_eq!(app.backend().load_calls.get(), 2);
    assert_eq!(value(&app, FormField::Nickname), "web");
    assert_eq!(value(&app, FormField::User), "alice");
    assert_eq!(value(&app, FormField::Address), "example.com");
    assert_eq!(
        status(&app, StatusLevel::Error),
        format!(
            "{} was changed by another program; reloaded, press Enter to retry",
            app.backend().paths().config_file().display()
        )
    );
    key(&mut app, KeyCode::Enter);
    assert_eq!(
        app.exit(),
        Some(&Exit::Connect(vec![
            "-F".into(),
            app.backend().paths().config_file().into_os_string(),
            "web".into(),
        ]))
    );
    assert_eq!(app.backend().writes.borrow().len(), 2);
    assert_eq!(app.backend().load_calls.get(), 3);
}

#[test]
fn other_new_write_errors_preserve_the_form_without_reloading() {
    let backend = FakeBackend::new("");
    backend.commit_with(CommitBehavior::IoFailure);
    let mut app = App::new(backend);
    fill_named_host(&mut app, "web");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.exit(), None);
    assert!(matches!(app.mode(), Mode::Form(_)));
    assert_eq!(app.backend().load_calls.get(), 1);
    assert_eq!(value(&app, FormField::Nickname), "web");
    assert!(status(&app, StatusLevel::Error).contains("fake write failure"));
}

#[test]
fn preview_tracks_direct_and_alias_targets_and_reports_validation_errors() {
    let mut app = App::new(FakeBackend::new(""));
    assert_eq!(app.form_preview(), Some(Err("address is required".into())));
    set_field(&mut app, FormField::User, "alice");
    set_field(&mut app, FormField::Address, "example.com");
    set_field(&mut app, FormField::Port, "2222");
    assert_eq!(
        app.form_preview(),
        Some(Ok("ssh -p 2222 alice@example.com".into()))
    );
    set_field(&mut app, FormField::Nickname, "web");
    assert_eq!(app.form_preview(), Some(Ok("ssh web".into())));
    set_field(&mut app, FormField::Port, "oops");
    assert!(matches!(app.form_preview(), Some(Err(error)) if error.contains("port")));
}

#[test]
fn validation_errors_are_available_for_each_form_field_as_it_changes() {
    let mut app = App::new(FakeBackend::new(""));
    for (field, invalid, error) in [
        (
            FormField::Nickname,
            "a*",
            ValidationError::ForbiddenChar {
                field: Field::Nickname,
                ch: '*',
            },
        ),
        (
            FormField::User,
            "-u",
            ValidationError::LeadingDash { field: Field::User },
        ),
        (
            FormField::Address,
            "",
            ValidationError::Empty {
                field: Field::Address,
            },
        ),
        (FormField::Port, "0", ValidationError::InvalidPort),
        (
            FormField::KeyFile,
            "\"key",
            ValidationError::ForbiddenChar {
                field: Field::KeyFile,
                ch: '"',
            },
        ),
    ] {
        set_field(&mut app, field, invalid);
        assert_eq!(app.validation_error(field), Some(error));
    }
    set_field(&mut app, FormField::Nickname, "valid");
    assert_eq!(app.validation_error(FormField::Nickname), None);
    assert_eq!(
        app.validation_error(FormField::Port),
        Some(ValidationError::InvalidPort)
    );
    assert_eq!(app.status(), None);
}

#[test]
fn edit_form_uses_block_values_placeholders_and_read_only_lines() {
    let backend = FakeBackend::new("Host web\n  User alice\n  Port 22\n  Port 2222\n");
    backend.set_effective(
        "web",
        EffectiveConfig {
            host_name: Some("real.example.com".into()),
            host_key_alias: None,
            user: Some("other".into()),
            port: Some("22".into()),
            identity_files: vec!["~/first".into(), "~/second".into()],
        },
    );
    let mut app = App::new(backend);
    ch(&mut app, 'e');
    let form = app.form().unwrap();
    assert!(matches!(form.kind(), FormKind::Edit { entry, .. } if entry.name == "web"));
    assert_eq!(value(&app, FormField::Nickname), "web");
    assert_eq!(value(&app, FormField::User), "alice");
    assert_eq!(form.placeholder(FormField::User), None);
    assert_eq!(value(&app, FormField::Address), "");
    assert_eq!(
        form.placeholder(FormField::Address),
        Some("real.example.com")
    );
    assert_eq!(form.placeholder(FormField::KeyFile), Some("~/first"));
    assert!(form.is_read_only(FormField::Port));
    assert_eq!(
        form.original_lines(FormField::Port).unwrap(),
        ["  Port 22", "  Port 2222"]
    );
    set_field(&mut app, FormField::Port, "2200");
    ch(&mut app, '1');
    assert_eq!(value(&app, FormField::Port), "");
    assert_eq!(app.validation_error(FormField::Port), None);
    assert_eq!(app.backend().effective_count("web"), 1);
    key(&mut app, KeyCode::Enter);
    assert_eq!(status(&app, StatusLevel::Info), "No changes");

    let mut app = App::new(FakeBackend::new("Host web\n"));
    ch(&mut app, 'e');
    assert_eq!(app.form().unwrap().placeholder(FormField::User), None);
}

#[test]
fn submitting_an_unchanged_edit_returns_without_committing() {
    let mut app = App::new(FakeBackend::new(EDITABLE));
    ch(&mut app, 'e');
    assert_eq!(app.form_preview(), Some(Ok("ssh web".into())));
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(status(&app, StatusLevel::Info), "No changes");
    assert!(app.backend().writes.borrow().is_empty());
}

#[test]
fn an_edit_previews_removed_and_added_lines_before_commit() {
    let mut app = App::new(FakeBackend::new(EDITABLE));
    ch(&mut app, 'e');
    set_field(&mut app, FormField::User, "bob");
    key(&mut app, KeyCode::Enter);
    let confirm = confirm(&app);
    assert_eq!(confirm.title(), "Save web");
    assert_eq!(confirm.path(), app.backend().paths().config_file());
    let lines = &confirm.hunks()[0].lines;
    assert!(
        lines
            .iter()
            .any(|line| line.tag == DiffTag::Removed && line.text.trim() == "User alice")
    );
    assert!(
        lines
            .iter()
            .any(|line| line.tag == DiffTag::Added && line.text.trim() == "User bob")
    );
    assert!(app.backend().writes.borrow().is_empty());
}

#[test]
fn cancelling_confirm_restores_the_edit_and_accepting_writes_it() {
    let backend = FakeBackend::new(EDITABLE);
    backend.commit_with(CommitBehavior::Success);
    let mut app = App::new(backend);
    ch(&mut app, 'e');
    set_field(&mut app, FormField::User, "bob");
    key(&mut app, KeyCode::Enter);
    ch(&mut app, 'n');
    assert_eq!(value(&app, FormField::User), "bob");
    assert_eq!(app.form().unwrap().focus(), FormField::User);
    assert!(app.backend().writes.borrow().is_empty());
    key(&mut app, KeyCode::Enter);
    ch(&mut app, 'y');
    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(app.selection(), Some(0));
    assert_eq!(
        status(&app, StatusLevel::Info),
        format!(
            "Saved web (backup: {})",
            app.backend()
                .paths()
                .config_file()
                .with_extension("backup")
                .display()
        )
    );
    assert_eq!(app.backend().writes.borrow().len(), 1);
    assert_eq!(app.backend().load_calls.get(), 2);
}

#[test]
fn clearing_an_optional_field_plans_only_a_removal() {
    let mut app = App::new(FakeBackend::new(EDITABLE));
    ch(&mut app, 'e');
    set_field(&mut app, FormField::User, "");
    key(&mut app, KeyCode::Enter);
    let changed: Vec<_> = confirm(&app)
        .hunks()
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .filter(|line| line.tag != DiffTag::Context)
        .map(|line| (line.tag, line.text.trim()))
        .collect();
    assert_eq!(changed, [(DiffTag::Removed, "User alice")]);
}

#[test]
fn blank_edit_address_is_allowed_only_when_originally_absent() {
    let mut app = App::new(FakeBackend::new(EDITABLE));
    ch(&mut app, 'e');
    set_field(&mut app, FormField::Address, "");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.form().unwrap().focus(), FormField::Address);
    assert_eq!(status(&app, StatusLevel::Error), "address is required");

    let mut app = App::new(FakeBackend::new("Host web\n  User alice\n"));
    ch(&mut app, 'e');
    assert_eq!(app.validation_error(FormField::Address), None);
    set_field(&mut app, FormField::User, "bob");
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode(), Mode::Confirm(_)));
}

#[test]
fn edit_rename_requires_a_unique_nonempty_alias_and_selects_the_new_name() {
    let backend = FakeBackend::new(HOSTS);
    backend.commit_with(CommitBehavior::WriteFile);
    let mut app = App::new(backend);
    ch(&mut app, 'j');
    ch(&mut app, 'e');
    for (name, error) in [("", "nickname is required"), ("web", "already in use")] {
        set_field(&mut app, FormField::Nickname, name);
        key(&mut app, KeyCode::Enter);
        assert!(status(&app, StatusLevel::Error).contains(error));
        assert!(app.backend().writes.borrow().is_empty());
    }
    set_field(&mut app, FormField::Nickname, "db2");
    key(&mut app, KeyCode::Enter);
    assert_eq!(confirm(&app).title(), "Save db2");
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(names(&app), ["web", "db2", "queue"]);
    assert_eq!(app.selection(), Some(1));
    assert_eq!(app.backend().effective_count("db2"), 1);
}

#[test]
fn editing_shared_blocks_explains_the_effect_on_sibling_patterns() {
    let mut app = App::new(FakeBackend::new(
        "Host web docs *.example.com !test\n  User alice\n",
    ));
    ch(&mut app, 'e');
    assert_eq!(
        app.form().unwrap().shared_notice().as_deref(),
        Some("Option changes also apply to docs, *.example.com, !test. Renaming affects only web.")
    );
}

#[test]
fn an_edit_mismatch_notice_returns_to_list_for_either_response() {
    for response in [KeyCode::Enter, KeyCode::Esc] {
        let backend = FakeBackend::new(
            "Host web\n  HostName example.com\n  User alice\n  IdentityFile ~/a\n  IdentityFile ~/b\n",
        );
        backend.set_effective(
            "web",
            EffectiveConfig {
                port: Some("22".into()),
                ..basic_expected()
            },
        );
        let mut app = App::new(backend);
        ch(&mut app, 'e');
        set_field(&mut app, FormField::User, "bob");
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Enter);
        assert_eq!(
            notice(&app).lines(),
            ["User: saved \"bob\" but ssh will use \"alice\""]
        );
        assert_eq!(app.backend().effective_count("web"), 2);
        key(&mut app, response);
        assert!(matches!(app.mode(), Mode::List));
        assert_eq!(app.exit(), None);
    }
}

#[test]
fn stale_edit_reloads_and_returns_to_the_list() {
    let backend = FakeBackend::new(EDITABLE);
    backend.commit_with(CommitBehavior::Stale);
    let mut app = App::new(backend);
    ch(&mut app, 'e');
    set_field(&mut app, FormField::User, "bob");
    key(&mut app, KeyCode::Enter);
    ch(&mut app, 'y');
    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(app.backend().load_calls.get(), 2);
    assert!(status(&app, StatusLevel::Error).contains("changed by another program"));
}

#[test]
fn other_confirmed_write_errors_return_to_the_list_without_reloading() {
    let backend = FakeBackend::new(EDITABLE);
    backend.commit_with(CommitBehavior::IoFailure);
    let mut app = App::new(backend);
    ch(&mut app, 'd');
    ch(&mut app, 'y');
    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(app.backend().load_calls.get(), 1);
    assert_eq!(names(&app), ["web"]);
    assert!(status(&app, StatusLevel::Error).contains("fake write failure"));
}

#[test]
fn review_scrolls_by_lines_and_pages_within_the_diff() {
    let mut app = App::new(FakeBackend::new(EDITABLE));
    ch(&mut app, 'd');
    assert_eq!(confirm(&app).scroll(), 0);
    key(&mut app, KeyCode::Up);
    assert_eq!(confirm(&app).scroll(), 0);
    key(&mut app, KeyCode::Down);
    assert_eq!(confirm(&app).scroll(), 1);
    key(&mut app, KeyCode::PageDown);
    assert_eq!(confirm(&app).scroll(), 2);
    key(&mut app, KeyCode::PageUp);
    assert_eq!(confirm(&app).scroll(), 0);
}

#[test]
fn deleting_requires_confirmation_and_updates_the_real_temporary_config() {
    let backend = FakeBackend::new(HOSTS);
    backend.commit_with(CommitBehavior::WriteFile);
    let mut app = App::new(backend);
    key(&mut app, KeyCode::End);
    ch(&mut app, 'd');
    assert_eq!(confirm(&app).title(), "Delete queue");
    key(&mut app, KeyCode::Esc);
    assert!(matches!(app.mode(), Mode::List));
    assert!(app.backend().writes.borrow().is_empty());
    assert_eq!(
        fs::read_to_string(app.backend().paths().config_file()).unwrap(),
        HOSTS
    );
    ch(&mut app, 'd');
    ch(&mut app, 'y');
    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(names(&app), ["web", "db"]);
    assert_eq!(app.selection(), Some(1));
    let contents = fs::read_to_string(app.backend().paths().config_file()).unwrap();
    assert_eq!(contents, "Host web\n  HostName web.example.com\nHost db\n");
    let backup = app.backend().paths().ssh_dir().join(".config.sshtui.bak");
    assert_eq!(fs::read_to_string(&backup).unwrap(), HOSTS);
    assert_eq!(
        status(&app, StatusLevel::Info),
        format!(
            "Deleted queue (backup: {})",
            backup.canonicalize().unwrap().display()
        )
    );
    assert_eq!(app.backend().writes.borrow().len(), 1);
}

fn forget_backend(removed: usize) -> (FakeBackend, PendingWrite) {
    let mut backend = FakeBackend::new(HOSTS);
    backend.set_effective(
        "queue",
        EffectiveConfig {
            host_name: Some("queue.example.com".into()),
            ..EffectiveConfig::default()
        },
    );
    let original = "queue.example.com ssh-ed25519 fake-queue-key\n".repeat(removed);
    let pending = PendingWrite::replacement(
        known_hosts::file_path(backend.paths()),
        original.as_bytes(),
        Vec::new(),
    );
    backend.forget_plan = Ok(Some(ForgetPlan {
        name: "queue.example.com".into(),
        removed,
        pending: pending.clone(),
    }));
    (backend, pending)
}

#[test]
fn forget_uses_the_cached_effective_host_port_and_host_key_alias() {
    for (port, host_key_alias, expected) in [
        ("22", None, "host"),
        ("2222", None, "[host]:2222"),
        ("2222", Some("KeyAlias"), "keyalias"),
    ] {
        let backend = FakeBackend::new(EDITABLE);
        backend.set_effective(
            "web",
            EffectiveConfig {
                host_name: Some("HOST".into()),
                host_key_alias: host_key_alias.map(str::to_owned),
                port: Some(port.into()),
                ..EffectiveConfig::default()
            },
        );
        let mut app = App::new(backend);
        ch(&mut app, 'x');

        assert_eq!(*app.backend().forget_calls.borrow(), [expected]);
        assert_eq!(app.backend().effective_count("web"), 1);
        assert!(matches!(app.mode(), Mode::List));
        assert_eq!(
            status(&app, StatusLevel::Info),
            format!("No entry for {expected} in ~/.ssh/known_hosts")
        );
        assert!(app.backend().writes.borrow().is_empty());
    }
}

#[test]
fn forget_reports_a_planning_error_in_the_list() {
    let mut backend = FakeBackend::new(EDITABLE);
    backend.set_effective("web", EffectiveConfig::default());
    backend.forget_plan = Err("cannot read known_hosts".into());
    let mut app = App::new(backend);
    ch(&mut app, 'x');

    assert_eq!(*app.backend().forget_calls.borrow(), ["web"]);
    assert_eq!(status(&app, StatusLevel::Error), "cannot read known_hosts");
    assert!(matches!(app.mode(), Mode::List));
}

#[test]
fn forget_reports_an_effective_lookup_error_without_planning() {
    let mut app = App::new(FakeBackend::new(EDITABLE));
    ch(&mut app, 'x');

    assert_eq!(
        status(&app, StatusLevel::Error),
        "Cannot resolve web: effective config unavailable"
    );
    assert_eq!(app.backend().effective_count("web"), 1);
    assert!(app.backend().forget_calls.borrow().is_empty());
    assert!(matches!(app.mode(), Mode::List));
}

#[test]
fn clicking_forget_opens_confirmation_for_a_read_only_config_host() {
    let (backend, pending) = forget_backend(1);
    backend.writable.set(false);
    let mut app = App::new(backend);
    key(&mut app, KeyCode::End);
    assert!(app.filtered_hosts().nth(2).unwrap().read_only);
    app.handle_click(ClickTarget::Key(KeyCode::Char('x')), false);

    let state = confirm(&app);
    assert_eq!(state.title(), "Forget host key for queue");
    assert_eq!(state.kind(), ConfirmKind::Forget);
    assert_eq!(state.forget_name(), Some("queue.example.com"));
    assert_eq!(state.path(), pending.path);
    assert!(!state.is_delete());
    assert_eq!(*app.backend().forget_calls.borrow(), ["queue.example.com"]);
    assert!(app.backend().writes.borrow().is_empty());
}

#[test]
fn cancelling_forget_returns_to_the_list_without_committing() {
    let (backend, _) = forget_backend(1);
    let mut app = App::new(backend);
    key(&mut app, KeyCode::End);
    for cancel in [KeyCode::Char('n'), KeyCode::Esc] {
        ch(&mut app, 'x');
        assert!(matches!(app.mode(), Mode::Confirm(_)));
        key(&mut app, cancel);
        assert!(matches!(app.mode(), Mode::List));
        assert_eq!(app.selection(), Some(2));
        assert!(app.backend().writes.borrow().is_empty());
    }
}

#[test]
fn confirming_forget_commits_the_plan_without_reloading_config() {
    for (accept, removed, message) in [
        (KeyCode::Char('y'), 1, "Removed 1 known_hosts entry"),
        (KeyCode::Enter, 2, "Removed 2 known_hosts entries"),
    ] {
        let (backend, pending) = forget_backend(removed);
        let backup = pending.path.with_extension("backup");
        let mut app = App::new(backend);
        key(&mut app, KeyCode::End);
        let loads = app.backend().load_calls.get();
        ch(&mut app, 'x');
        assert!(matches!(app.mode(), Mode::Confirm(_)));
        assert!(app.backend().writes.borrow().is_empty());
        key(&mut app, accept);

        assert_eq!(*app.backend().writes.borrow(), [pending]);
        assert!(matches!(app.mode(), Mode::List));
        assert_eq!(app.selection(), Some(2));
        assert_eq!(names(&app), ["web", "db", "queue"]);
        assert_eq!(app.backend().load_calls.get(), loads);
        assert_eq!(
            status(&app, StatusLevel::Info),
            format!(
                "{message} for queue.example.com (backup: {})",
                backup.display()
            )
        );
    }
}

#[test]
fn a_stale_forget_prompts_retry_without_reloading_config() {
    let (backend, pending) = forget_backend(1);
    backend.commit_with(CommitBehavior::Stale);
    let mut app = App::new(backend);
    key(&mut app, KeyCode::End);
    ch(&mut app, 'x');
    ch(&mut app, 'y');

    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(app.selection(), Some(2));
    assert_eq!(app.backend().load_calls.get(), 1);
    assert_eq!(
        status(&app, StatusLevel::Error),
        format!(
            "{} was changed by another program; press x to try again",
            pending.path.display()
        )
    );
    assert_eq!(*app.backend().writes.borrow(), [pending]);
}

#[test]
fn a_forget_write_error_returns_to_the_list_without_reloading_config() {
    let (backend, pending) = forget_backend(1);
    backend.commit_with(CommitBehavior::IoFailure);
    let mut app = App::new(backend);
    key(&mut app, KeyCode::End);
    ch(&mut app, 'x');
    ch(&mut app, 'y');

    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(app.backend().load_calls.get(), 1);
    assert_eq!(
        status(&app, StatusLevel::Error),
        format!(
            "cannot write {}: fake write failure",
            pending.path.display()
        )
    );
}

#[test]
fn x_is_filter_input_until_the_filter_is_applied() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, '/');
    ch(&mut app, 'x');

    assert!(app.is_filtering());
    assert_eq!(app.filter().value(), "x");
    assert!(app.backend().forget_calls.borrow().is_empty());
}

#[test]
fn forget_does_nothing_when_no_hosts_match_the_filter() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, '/');
    app.handle_paste("missing");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.selection(), None);
    ch(&mut app, 'x');

    assert!(matches!(app.mode(), Mode::List));
    assert!(app.status().is_none());
    assert!(app.backend().forget_calls.borrow().is_empty());
}

#[test]
fn key_picker_selects_abbreviated_paths_and_cancel_keeps_the_value_and_focus() {
    let mut app = App::new(FakeBackend::new("").with_keys());
    set_field(&mut app, FormField::KeyFile, "~/initial");
    set_field(&mut app, FormField::Address, "example.com");
    ctrl(&mut app, 'l');
    let Mode::KeyPicker(picker) = app.mode() else {
        panic!("key picker did not open");
    };
    assert_eq!(picker.keys().len(), 2);
    assert_eq!(picker.selection(), 0);
    assert_eq!(value(&app, FormField::KeyFile), "~/initial");
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.form().unwrap().focus(), FormField::Address);
    assert_eq!(value(&app, FormField::KeyFile), "~/initial");

    key(&mut app, KeyCode::F(2));
    for code in [
        KeyCode::Up,
        KeyCode::Char('j'),
        KeyCode::Down,
        KeyCode::Char('k'),
        KeyCode::Down,
    ] {
        key(&mut app, code);
    }
    assert!(matches!(app.mode(), Mode::KeyPicker(picker) if picker.selection() == 1));
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode(), Mode::Form(_)));
    assert_eq!(value(&app, FormField::KeyFile), "~/.ssh/id_b");
    assert_eq!(value(&app, FormField::Address), "example.com");
    assert_eq!(app.form().unwrap().focus(), FormField::KeyFile);
}

#[test]
fn empty_key_list_warns_and_keeps_the_form_open() {
    let mut app = App::new(FakeBackend::new(""));
    ctrl(&mut app, 'l');
    assert!(matches!(app.mode(), Mode::Form(_)));
    assert_eq!(
        status(&app, StatusLevel::Warning),
        format!(
            "No private keys found in {}",
            app.backend().paths().ssh_dir().display()
        )
    );
}

#[test]
fn unavailable_browser_exposes_the_reason_and_skips_the_native_dialog() {
    let mut backend = FakeBackend::new("");
    backend.unavailable = Some("No desktop session".into());
    backend.hidden_hint = Some("Show hidden files in the dialog".into());
    let mut app = App::new(backend);
    ctrl(&mut app, 'o');
    assert_eq!(status(&app, StatusLevel::Warning), "No desktop session");
    assert_eq!(
        app.picker_unavailable_reason().as_deref(),
        Some("No desktop session")
    );
    assert_eq!(
        app.hidden_files_hint().as_deref(),
        Some("Show hidden files in the dialog")
    );
    assert_eq!(app.backend().pick_calls.get(), 0);
    assert!(!app.take_full_redraw());
}

#[test]
fn browser_reports_all_outcomes_and_requests_one_redraw_per_dialog() {
    let mut app = App::new(FakeBackend::new(""));
    set_field(&mut app, FormField::KeyFile, "~/initial");
    let selected = app.backend().paths().ssh_dir().join("id_other");
    for (outcome, expected_error) in [
        (Ok(PickOutcome::Picked(selected)), None),
        (Ok(PickOutcome::Cancelled), None),
        (
            Err(DialogError::Failed("fake dialog error".into())),
            Some("file dialog failed: fake dialog error"),
        ),
    ] {
        app.backend().pick.replace(Some(outcome));
        key(&mut app, KeyCode::F(3));
        assert_eq!(value(&app, FormField::KeyFile), "~/.ssh/id_other");
        assert!(app.take_full_redraw());
        assert!(!app.take_full_redraw());
        assert_eq!(
            app.status()
                .map(|status| (status.level, status.text.as_str())),
            expected_error.map(|message| (StatusLevel::Error, message))
        );
    }
    assert_eq!(app.backend().pick_calls.get(), 3);
}

#[test]
fn list_and_native_key_pickers_preserve_read_only_key_fields() {
    let mut app = App::new(FakeBackend::new(MULTIPLE_KEYS).with_keys());
    ch(&mut app, 'e');
    ctrl(&mut app, 'l');
    assert!(matches!(app.mode(), Mode::KeyPicker(_)));
    key(&mut app, KeyCode::Enter);
    assert_eq!(value(&app, FormField::KeyFile), "");
    let selected = app.backend().paths().ssh_dir().join("id_other");
    app.backend()
        .pick
        .replace(Some(Ok(PickOutcome::Picked(selected))));
    ctrl(&mut app, 'o');
    assert_eq!(app.backend().pick_calls.get(), 1);
    assert_eq!(value(&app, FormField::KeyFile), "");
    assert_eq!(
        app.form()
            .unwrap()
            .original_lines(FormField::KeyFile)
            .unwrap(),
        ["  IdentityFile ~/first", "  IdentityFile ~/second"]
    );
    key(&mut app, KeyCode::Enter);
    assert_eq!(status(&app, StatusLevel::Info), "No changes");
}

#[test]
fn text_input_edits_by_character_and_reports_terminal_columns() {
    let mut input = TextInput::default();
    let press =
        |input: &mut TextInput, code| input.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    for char in ['a', '한', 'Ａ', '\t', '\0'] {
        press(&mut input, KeyCode::Char(char));
    }
    assert_eq!(input.value(), "a한Ａ");
    assert_eq!(input.cursor(), 3);
    assert_eq!(input.cursor_column(), 5);

    press(&mut input, KeyCode::Left);
    assert_eq!(input.cursor_column(), 3);
    press(&mut input, KeyCode::Delete);
    assert_eq!(input.value(), "a한");
    press(&mut input, KeyCode::Home);
    press(&mut input, KeyCode::Backspace);
    assert_eq!(input.cursor(), 0);
    press(&mut input, KeyCode::Char('x'));
    assert_eq!(input.value(), "xa한");
    press(&mut input, KeyCode::Right);
    press(&mut input, KeyCode::Backspace);
    assert_eq!(input.value(), "x한");
    press(&mut input, KeyCode::End);
    press(&mut input, KeyCode::Right);
    press(&mut input, KeyCode::Delete);
    assert_eq!(input.cursor_column(), 3);
    input.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(input.value(), "");
    assert_eq!(input.cursor(), 0);
}

#[test]
fn paste_inserts_only_the_first_sanitized_line_into_the_active_input() {
    let mut app = App::new(FakeBackend::new(MULTIPLE_KEYS));
    ch(&mut app, 'e');
    set_field(&mut app, FormField::User, "ab");
    key(&mut app, KeyCode::Left);
    app.handle_paste("한\tＡ\0\u{7f}\r\nignored");
    assert_eq!(value(&app, FormField::User), "a한Ａb");
    assert_eq!(
        app.form().unwrap().input(FormField::User).cursor_column(),
        5
    );
    set_field(&mut app, FormField::KeyFile, "ignore\nthis");
    assert_eq!(value(&app, FormField::KeyFile), "");

    key(&mut app, KeyCode::Esc);
    app.handle_paste("unused");
    assert_eq!(app.filter().value(), "");
    ch(&mut app, '/');
    ch(&mut app, 'W');
    app.handle_paste("\te\0B\nignored");
    assert_eq!(app.filter().value(), "WeB");
    assert_eq!(names(&app), ["web"]);
}

fn update_fixture(method: InstallMethod) -> (TestApp, UpdatePlan) {
    let plan = UpdatePlan {
        current: "1.0.0".into(),
        latest: "2.0.0".into(),
        method,
    };
    let mut backend = FakeBackend::new(HOSTS);
    backend.update_plan = Some(plan.clone());
    (App::new(backend), plan)
}

#[test]
fn update_key_requires_a_plan_read_once_at_startup() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    key(&mut app, KeyCode::Char('u'));
    assert!(matches!(app.mode(), Mode::List));
    assert!(app.exit().is_none());

    let (mut app, plan) = update_fixture(InstallMethod::Brew);
    key(&mut app, KeyCode::Char('r'));
    assert_eq!(app.update_plan(), Some(&plan));
    key(&mut app, KeyCode::Char('u'));
    assert!(matches!(app.mode(), Mode::Update(_)));
    assert!(app.exit().is_none());
    assert_eq!(app.backend().update_plan_calls.get(), 1);
}

#[test]
fn update_esc_returns_to_list_and_enter_requests_upgrade() {
    let (mut app, plan) = update_fixture(InstallMethod::Cargo);
    key(&mut app, KeyCode::Char('u'));
    key(&mut app, KeyCode::Esc);
    assert!(matches!(app.mode(), Mode::List));
    assert!(app.exit().is_none());

    key(&mut app, KeyCode::Char('u'));
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.exit(), Some(&Exit::Update(plan)));
}

#[test]
fn unknown_installation_cannot_request_upgrade() {
    let (mut app, _) = update_fixture(InstallMethod::Unknown);
    key(&mut app, KeyCode::Char('u'));
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode(), Mode::Update(_)));
    assert!(app.exit().is_none());
}

#[test]
fn update_key_is_text_while_editing_the_filter() {
    let (mut app, _) = update_fixture(InstallMethod::Brew);
    key(&mut app, KeyCode::Char('/'));
    key(&mut app, KeyCode::Char('u'));
    assert!(matches!(app.mode(), Mode::List));
    assert!(app.is_filtering());
    assert_eq!(app.filter().value(), "u");
}

#[test]
fn update_key_is_text_in_the_connection_form() {
    let (mut app, _) = update_fixture(InstallMethod::Brew);
    key(&mut app, KeyCode::Char('n'));
    key(&mut app, KeyCode::Char('u'));
    assert!(matches!(app.mode(), Mode::Form(_)));
    assert_eq!(app.form().unwrap().input(FormField::Nickname).value(), "u");
}

#[test]
fn clicking_a_host_selects_it_and_fetches_its_effective_config() {
    let backend = FakeBackend::new(HOSTS);
    let effective = basic_expected();
    backend.set_effective("queue", effective.clone());
    let mut app = App::new(backend);
    ch(&mut app, 'r');
    let reloaded = app.status().cloned();
    app.handle_click(ClickTarget::HostRow(3), true);
    assert_eq!(app.selection(), Some(0));
    assert_eq!(app.status(), reloaded.as_ref());
    assert_eq!(app.exit(), None);
    assert_eq!(app.backend().effective_count("queue"), 0);

    app.handle_click(ClickTarget::HostRow(2), false);
    assert_eq!(app.selection(), Some(2));
    assert_eq!(app.selected_effective(), Some(&Ok(effective)));
    assert_eq!(app.backend().effective_count("queue"), 1);
    assert_eq!(app.status(), None);
    assert_eq!(app.exit(), None);
}

#[test]
fn double_clicking_a_filtered_row_connects_and_keeps_filter_input_active() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, '/');
    app.handle_paste("e");
    assert_eq!(names(&app), ["web", "queue"]);

    app.handle_click(ClickTarget::HostRow(1), false);
    assert_eq!(app.selection(), Some(1));
    assert!(app.is_filtering());
    assert_eq!(app.exit(), None);

    app.handle_click(ClickTarget::HostRow(1), true);
    assert_eq!(app.exit(), Some(&Exit::Connect(vec!["queue".into()])));
    assert!(app.is_filtering());
    assert_eq!(app.filter().value(), "e");
}

#[test]
fn list_wheel_moves_one_row_and_stops_at_both_ends() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    for (down, index) in [
        (true, 1),
        (true, 2),
        (true, 2),
        (false, 1),
        (false, 0),
        (false, 0),
    ] {
        app.handle_scroll(down);
        assert_eq!(app.selection(), Some(index));
    }
    assert_eq!(app.backend().effective_count("db"), 1);
    assert_eq!(app.backend().effective_count("queue"), 1);
}

#[test]
fn list_key_clicks_open_the_form_or_quit_and_update_notice_requires_a_plan() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, 'r');
    app.handle_click(ClickTarget::UpdateNotice, false);
    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(status(&app, StatusLevel::Info), "Reloaded");

    app.handle_click(ClickTarget::Key(KeyCode::Char('n')), false);
    assert!(matches!(app.mode(), Mode::Form(_)));
    assert_eq!(app.status(), None);
    app.handle_click(ClickTarget::Key(KeyCode::Esc), false);
    app.handle_click(ClickTarget::Key(KeyCode::Char('q')), false);
    assert_eq!(app.exit(), Some(&Exit::Quit));

    let (mut app, _) = update_fixture(InstallMethod::Cargo);
    app.handle_click(ClickTarget::UpdateNotice, false);
    assert!(matches!(app.mode(), Mode::Update(_)));
}

#[test]
fn clicking_form_fields_focuses_them_and_positions_the_cursor_by_cell() {
    let mut app = App::new(FakeBackend::new(""));
    let field = FormField::Address;
    set_field(&mut app, field, "a한b");
    key(&mut app, KeyCode::Tab);

    for (column, cursor) in [(0, 0), (2, 1), (3, 2), (20, 3), (u16::MAX, 3)] {
        app.handle_click(ClickTarget::FormField { field, column }, false);
        let form = app.form().unwrap();
        assert_eq!(form.focus(), field);
        assert_eq!(form.input(field).cursor(), cursor);
        assert_eq!(form.input(field).value(), "a한b");
    }
}

#[test]
fn clicking_a_read_only_field_only_changes_focus() {
    let mut app = App::new(FakeBackend::new(MULTIPLE_KEYS));
    ch(&mut app, 'e');
    let field = FormField::KeyFile;
    let form = app.form().unwrap();
    let original = form.original_lines(field).unwrap().to_vec();
    let cursor = form.input(field).cursor();
    assert_ne!(form.focus(), field);

    app.handle_click(ClickTarget::FormField { field, column: 3 }, false);
    let form = app.form().unwrap();
    assert_eq!(form.focus(), field);
    assert_eq!(form.input(field).cursor(), cursor);
    assert_eq!(form.original_lines(field), Some(original.as_slice()));
}

#[test]
fn form_key_clicks_browse_open_the_picker_submit_or_return() {
    let mut backend = FakeBackend::new(HOSTS).with_keys();
    backend.unavailable = Some("Native browsing is unavailable".into());
    let mut app = App::new(backend);
    ch(&mut app, 'n');
    app.handle_click(ClickTarget::Key(KeyCode::F(3)), false);
    assert_eq!(
        status(&app, StatusLevel::Warning),
        "Native browsing is unavailable"
    );
    assert_eq!(app.backend().pick_calls.get(), 0);
    app.handle_scroll(true);
    assert_eq!(app.form().unwrap().focus(), FormField::Nickname);
    assert!(app.status().is_some());

    app.handle_click(ClickTarget::Key(KeyCode::F(2)), false);
    assert!(matches!(app.mode(), Mode::KeyPicker(_)));
    assert_eq!(app.status(), None);
    app.handle_click(ClickTarget::Key(KeyCode::Esc), false);
    assert!(matches!(app.mode(), Mode::Form(_)));
    app.handle_click(ClickTarget::Key(KeyCode::Esc), false);
    assert!(matches!(app.mode(), Mode::List));

    ch(&mut app, 'n');
    set_field(&mut app, FormField::Address, "example.com");
    app.handle_click(ClickTarget::Key(KeyCode::Enter), false);
    assert_eq!(app.exit(), Some(&Exit::Connect(vec!["example.com".into()])));
}

#[test]
fn confirmation_clicks_cancel_or_commit_and_wheel_scrolls_the_diff() {
    let mut app = App::new(FakeBackend::new(EDITABLE));
    ch(&mut app, 'd');
    for (down, line) in [(false, 0), (true, 1), (true, 2), (true, 2), (false, 1)] {
        app.handle_scroll(down);
        assert_eq!(confirm(&app).scroll(), line);
    }
    app.handle_click(ClickTarget::Key(KeyCode::Char('n')), false);
    assert!(matches!(app.mode(), Mode::List));
    assert!(app.backend().writes.borrow().is_empty());

    ch(&mut app, 'd');
    app.handle_click(ClickTarget::Key(KeyCode::Char('y')), false);
    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(app.backend().writes.borrow().len(), 1);
}

#[test]
fn picker_wheel_moves_selection_and_clicks_choose_a_key_or_cancel() {
    let mut app = App::new(FakeBackend::new("").with_keys());
    set_field(&mut app, FormField::KeyFile, "~/previous");
    key(&mut app, KeyCode::F(2));
    for (down, index) in [(true, 1), (true, 1), (false, 0), (false, 0)] {
        app.handle_scroll(down);
        assert!(matches!(app.mode(), Mode::KeyPicker(picker) if picker.selection() == index));
    }

    app.set_status(StatusLevel::Info, "Keep this message");
    for target in [ClickTarget::Inert, ClickTarget::KeyRow(2)] {
        app.handle_click(target, false);
        assert!(matches!(app.mode(), Mode::KeyPicker(picker) if picker.selection() == 0));
        assert_eq!(status(&app, StatusLevel::Info), "Keep this message");
    }
    app.handle_click(ClickTarget::Outside, false);
    assert!(matches!(app.mode(), Mode::Form(_)));
    assert_eq!(value(&app, FormField::KeyFile), "~/previous");
    assert_eq!(app.status(), None);

    key(&mut app, KeyCode::F(2));
    app.handle_click(ClickTarget::KeyRow(1), false);
    assert!(matches!(app.mode(), Mode::Form(_)));
    assert_eq!(value(&app, FormField::KeyFile), "~/.ssh/id_b");
    assert_eq!(app.form().unwrap().focus(), FormField::KeyFile);
}

#[test]
fn outside_closes_notices_and_enter_accepts_their_actions() {
    for target in [ClickTarget::Outside, ClickTarget::Key(KeyCode::Enter)] {
        let backend = FakeBackend::new("");
        backend.set_effective("web", EffectiveConfig::default());
        let mut app = App::new(backend);
        fill_named_host(&mut app, "web");
        key(&mut app, KeyCode::Enter);
        assert!(notice(&app).connects());
        let expected =
            (target == ClickTarget::Key(KeyCode::Enter)).then(|| Exit::Connect(vec!["web".into()]));
        app.handle_click(target, false);
        assert!(matches!(app.mode(), Mode::List));
        assert_eq!(app.exit(), expected.as_ref());
    }

    let (mut app, plan) = update_fixture(InstallMethod::Cargo);
    app.handle_click(ClickTarget::UpdateNotice, false);
    app.handle_click(ClickTarget::Outside, false);
    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(app.exit(), None);
    app.handle_click(ClickTarget::UpdateNotice, false);
    app.handle_click(ClickTarget::Key(KeyCode::Enter), false);
    assert_eq!(app.exit(), Some(&Exit::Update(plan)));
}

#[test]
fn unrelated_click_targets_preserve_mode_and_status() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, 'r');
    for target in [
        ClickTarget::KeyRow(0),
        ClickTarget::Outside,
        ClickTarget::Inert,
    ] {
        app.handle_click(target, false);
        assert!(matches!(app.mode(), Mode::List));
        assert_eq!(status(&app, StatusLevel::Info), "Reloaded");
    }

    ch(&mut app, 'n');
    key(&mut app, KeyCode::Enter);
    let error = app.status().cloned();
    assert!(error.is_some());
    for target in [ClickTarget::HostRow(0), ClickTarget::UpdateNotice] {
        app.handle_click(target, true);
        assert!(matches!(app.mode(), Mode::Form(_)));
        assert_eq!(app.status(), error.as_ref());
        assert_eq!(app.exit(), None);
    }
}

#[test]
fn clicks_and_scrolls_are_ignored_after_exit_has_been_requested() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    ch(&mut app, 'q');
    for target in [
        ClickTarget::HostRow(2),
        ClickTarget::Key(KeyCode::Char('n')),
    ] {
        app.handle_click(target, true);
    }
    app.handle_scroll(true);
    assert_eq!(app.exit(), Some(&Exit::Quit));
    assert!(matches!(app.mode(), Mode::List));
    assert_eq!(app.selection(), Some(0));
    assert_eq!(app.backend().effective_count("queue"), 0);
}

#[test]
fn text_cursor_maps_display_cells_to_character_boundaries() {
    let mut input = TextInput::default();
    input.set_cursor_to_column(10);
    assert_eq!(input.cursor(), 0);
    input.set("a한e\u{301}👩‍💻z".into());
    for (column, index) in [
        (0, 0),
        (1, 1),
        (2, 1),
        (3, 2),
        (4, 4),
        (5, 4),
        (6, 7),
        (7, 8),
        (100, 8),
    ] {
        input.set_cursor_to_column(column);
        assert_eq!(input.cursor(), index, "column {column}");
    }
}

#[test]
fn host_row_summaries_use_values_from_the_loaded_host_block() {
    let mut app = App::new(FakeBackend::new(
        "User inherited\nHost both\n  User deploy\n  HostName first.example.com\nHost address\n  HostName second.example.com\nHost user\n  User ops\nHost neither\n",
    ));
    assert_eq!(
        app.filtered_hosts()
            .map(|row| row.summary)
            .collect::<Vec<_>>(),
        [
            "deploy@first.example.com",
            "second.example.com",
            "ops@user",
            "neither"
        ],
    );
    assert_eq!(app.backend.effective_calls.borrow().as_slice(), ["both"]);

    let path = app.backend.paths.config_file();
    fs::write(path, "Host both\n  HostName changed.example.com\n").unwrap();
    *app.backend.loaded.borrow_mut() = Ok(load(&app.backend.paths).unwrap());
    assert_eq!(
        app.filtered_hosts().next().unwrap().summary,
        "deploy@first.example.com"
    );
    ch(&mut app, 'r');
    assert_eq!(
        app.filtered_hosts().next().unwrap().summary,
        "changed.example.com"
    );
}

#[test]
fn selected_command_uses_the_loaded_prefix_and_current_selection() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    let root = app.backend.paths.config_file();
    key(&mut app, KeyCode::Down);
    fs::remove_file(&root).unwrap();
    assert_eq!(
        app.selected_command(),
        Some(preview(&["-F".into(), root.into_os_string(), "db".into()]))
    );
    ch(&mut app, '/');
    app.handle_paste("no match");
    assert_eq!(app.selected_command(), None);
}

#[test]
fn form_first_error_is_in_field_order_and_works_behind_the_picker() {
    let mut app = App::new(FakeBackend::new("").with_keys());
    set_field(&mut app, FormField::Port, "70000");
    assert_eq!(
        app.form_first_error(),
        Some((
            FormField::Address,
            ValidationError::Empty {
                field: Field::Address
            }
        ))
    );
    set_field(&mut app, FormField::Address, "203.0.113.12");
    key(&mut app, KeyCode::F(2));
    assert_eq!(
        app.form_first_error(),
        Some((FormField::Port, ValidationError::InvalidPort))
    );
    key(&mut app, KeyCode::Esc);
    set_field(&mut app, FormField::Port, "22");
    assert_eq!(app.form_first_error(), None);
}
