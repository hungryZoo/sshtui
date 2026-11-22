//! Diff review, key selection, and acknowledgment screens.

use std::path::Path;

use crate::config::{DiffHunk, PendingWrite};
use crate::keys::KeyInfo;

use super::FormState;

const PAGE_LINES: usize = 10;

#[derive(Debug)]
pub(super) enum ConfirmAction {
    Edit(FormState),
    Delete,
}

/// A planned edit or deletion awaiting confirmation.
#[derive(Debug)]
pub struct ConfirmState {
    pub(super) pending: PendingWrite,
    pub(super) action: ConfirmAction,
    pub(super) alias: String,
    scroll: usize,
}

impl ConfirmState {
    pub(super) fn new(pending: PendingWrite, alias: String, action: ConfirmAction) -> Self {
        Self {
            pending,
            action,
            alias,
            scroll: 0,
        }
    }

    /// Returns the confirmation title for the requested change.
    pub fn title(&self) -> String {
        match self.action {
            ConfirmAction::Edit(_) => format!("Save {}", self.alias),
            ConfirmAction::Delete => format!("Delete {}", self.alias),
        }
    }

    /// Returns the path of the file that will be changed.
    pub fn path(&self) -> &Path {
        &self.pending.path
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
}

impl KeyPickerState {
    pub(super) fn new(form: FormState, keys: Vec<KeyInfo>) -> Self {
        Self {
            form,
            keys,
            selection: 0,
        }
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
    title: String,
    lines: Vec<String>,
    pub(super) connect_alias: Option<String>,
}

impl NoticeState {
    pub(super) fn new(lines: Vec<String>, connect_alias: Option<String>) -> Self {
        Self {
            title: "Saved settings differ from effective SSH settings".into(),
            lines,
            connect_alias,
        }
    }

    /// Returns the heading of the notice.
    pub fn title(&self) -> &str {
        &self.title
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
