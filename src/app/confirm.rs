//! Diff review, key selection, and acknowledgment screens.

use std::path::Path;

use crate::config::{DiffHunk, PendingWrite};
use crate::i18n::Lang;
use crate::keys::KeyInfo;

use super::{FormState, KeyInstall};

const PAGE_LINES: usize = 10;

#[derive(Debug)]
pub(super) enum ConfirmAction {
    Edit {
        form: FormState,
        install: Option<Box<KeyInstall>>,
    },
    Delete,
    Forget {
        name: String,
        removed: usize,
    },
}

/// What a confirmation will do when accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKind {
    /// Save changes to a host configuration.
    Save,
    /// Delete a host from the configuration.
    Delete,
    /// Remove a host's known_hosts entries.
    Forget,
}

/// The highlighted answer on a destructive confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmChoice {
    /// Return to the list.
    Cancel,
    /// Apply the destructive change.
    Proceed,
}

/// A planned change awaiting confirmation.
#[derive(Debug)]
pub struct ConfirmState {
    pub(super) pending: PendingWrite,
    pub(super) action: ConfirmAction,
    pub(super) alias: String,
    lang: Lang,
    scroll: usize,
    choice: Option<ConfirmChoice>,
}

impl ConfirmState {
    pub(super) fn new(
        lang: Lang,
        pending: PendingWrite,
        alias: String,
        action: ConfirmAction,
    ) -> Self {
        let choice =
            (!matches!(action, ConfirmAction::Edit { .. })).then_some(ConfirmChoice::Cancel);
        Self {
            pending,
            action,
            alias,
            lang,
            scroll: 0,
            choice,
        }
    }

    /// Returns the confirmation title for the requested change.
    pub fn title(&self) -> String {
        self.lang.confirm_title(self.kind(), &self.alias)
    }

    /// Returns the language of the confirmation.
    pub(crate) fn lang(&self) -> Lang {
        self.lang
    }

    /// Returns the path of the file that will be changed.
    pub fn path(&self) -> &Path {
        &self.pending.path
    }

    /// Returns whether the confirmation will remove a host.
    pub fn is_delete(&self) -> bool {
        matches!(self.action, ConfirmAction::Delete)
    }

    /// Returns the kind of change awaiting confirmation.
    pub fn kind(&self) -> ConfirmKind {
        match self.action {
            ConfirmAction::Edit { .. } => ConfirmKind::Save,
            ConfirmAction::Delete => ConfirmKind::Delete,
            ConfirmAction::Forget { .. } => ConfirmKind::Forget,
        }
    }

    /// Returns the highlighted answer, or None when the confirmation has no choice row.
    pub fn choice(&self) -> Option<ConfirmChoice> {
        self.choice
    }

    pub(super) fn toggle_choice(&mut self) {
        self.choice = self.choice.map(|choice| match choice {
            ConfirmChoice::Cancel => ConfirmChoice::Proceed,
            ConfirmChoice::Proceed => ConfirmChoice::Cancel,
        });
    }

    /// Returns the destructive confirmation question with the display path.
    pub(crate) fn question(&self, path: &str) -> Option<String> {
        match &self.action {
            ConfirmAction::Edit { .. } => None,
            ConfirmAction::Delete => Some(self.lang.delete_question(&self.alias, path)),
            ConfirmAction::Forget { name, removed } => {
                Some(self.lang.forget_question(*removed, name))
            }
        }
    }

    /// Returns the known_hosts lookup name when host key entries will be removed.
    pub fn forget_name(&self) -> Option<&str> {
        match &self.action {
            ConfirmAction::Forget { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Returns whether the pending write has a loaded file to back up.
    pub fn has_original_file(&self) -> bool {
        self.pending.original.is_some()
    }

    /// Returns the diff to display before saving.
    pub fn hunks(&self) -> &[DiffHunk] {
        &self.pending.hunks
    }

    /// Returns the offset in diff lines.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub(super) fn up(&mut self, page: bool) {
        self.scroll = self
            .scroll
            .saturating_sub(if page { PAGE_LINES } else { 1 });
    }

    pub(super) fn down(&mut self, page: bool) {
        let lines: usize = self.hunks().iter().map(|hunk| hunk.lines.len()).sum();
        self.scroll =
            (self.scroll + if page { PAGE_LINES } else { 1 }).min(lines.saturating_sub(1));
    }
}

/// Available key files and the form underneath the picker.
#[derive(Debug)]
pub struct KeyPickerState {
    pub(super) form: FormState,
    keys: Vec<KeyInfo>,
    selection: usize,
    lang: Lang,
}

impl KeyPickerState {
    pub(super) fn new(form: FormState, keys: Vec<KeyInfo>, lang: Lang) -> Self {
        Self {
            form,
            keys,
            selection: 0,
            lang,
        }
    }

    /// Returns the language of the key picker.
    pub(crate) fn lang(&self) -> Lang {
        self.lang
    }

    /// Returns the form below the overlay.
    pub fn form(&self) -> &FormState {
        &self.form
    }

    /// Returns the available private keys and their public metadata.
    pub fn keys(&self) -> &[KeyInfo] {
        &self.keys
    }

    /// Returns the selected index in the list.
    pub fn selection(&self) -> usize {
        self.selection
    }

    pub(super) fn select(&mut self, index: usize) {
        self.selection = index.min(self.keys.len().saturating_sub(1));
    }

    pub(super) fn up(&mut self) {
        self.select(self.selection.saturating_sub(1));
    }

    pub(super) fn down(&mut self) {
        self.select(self.selection + 1);
    }
}

/// An acknowledgment screen that can continue to a saved alias.
#[derive(Debug)]
pub struct NoticeState {
    lang: Lang,
    lines: Vec<String>,
    pub(super) connect_alias: Option<String>,
}

impl NoticeState {
    pub(super) fn new(lines: Vec<String>, connect_alias: Option<String>) -> Self {
        Self {
            lang: Lang::En,
            lines,
            connect_alias,
        }
    }

    pub(super) fn with_lang(mut self, lang: Lang) -> Self {
        self.lang = lang;
        self
    }

    /// Returns the language of the notice.
    pub(crate) fn lang(&self) -> Lang {
        self.lang
    }

    /// Returns the heading of the notice.
    pub fn title(&self) -> &str {
        self.lang.text().mismatch_title
    }

    /// Returns the individual lines of the notice.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Returns whether accepting the notice starts an SSH connection.
    pub fn connects(&self) -> bool {
        self.connect_alias.is_some()
    }
}
