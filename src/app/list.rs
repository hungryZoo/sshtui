//! Host filtering, selection, and cached lookups.

use std::collections::HashMap;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};

use crate::config::{ConfigSet, FieldState, HostEntry, OptionKey, block_fields};
use crate::ssh::effective::EffectiveConfig;

use super::{Backend, TextInput};

/// An action on the host list's key bar, in display order.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ListAction {
    /// Connect to the selected host or open the new connection form.
    #[default]
    Open,
    /// Edit the selected host.
    Edit,
    /// Review deletion of the selected host.
    Delete,
    /// Review removal of the selected host's known keys.
    Forget,
    /// Start editing the host filter.
    Filter,
    /// Reload the host configuration.
    Reload,
    /// Clear the applied filter or quit.
    Leave,
}

impl ListAction {
    /// All actions in display order.
    pub const ALL: [Self; 7] = [
        Self::Open,
        Self::Edit,
        Self::Delete,
        Self::Forget,
        Self::Filter,
        Self::Reload,
        Self::Leave,
    ];

    /// The action's position in the key bar.
    pub(crate) fn index(self) -> usize {
        self as usize
    }

    /// The key that runs this action directly.
    pub(crate) fn shortcut(self) -> KeyCode {
        match self {
            Self::Open => KeyCode::Enter,
            Self::Edit => KeyCode::Char('e'),
            Self::Delete => KeyCode::Char('d'),
            Self::Forget => KeyCode::Char('x'),
            Self::Filter => KeyCode::Char('/'),
            Self::Reload => KeyCode::Char('r'),
            Self::Leave => KeyCode::Esc,
        }
    }
}

/// A filtered list row and the writability of its source file.
#[derive(Debug, Clone, Copy)]
pub struct HostRow<'a> {
    /// Alias and source location.
    pub entry: &'a HostEntry,
    /// User and address taken from the alias's own block.
    pub summary: &'a str,
    /// Whether changes to the source file are unavailable.
    pub read_only: bool,
}

#[derive(Default)]
pub(super) struct ListState {
    filter: TextInput,
    pub(super) filtering: bool,
    pub(super) selection: usize,
    pub(super) action: ListAction,
    visible: Vec<usize>,
    summaries: Vec<String>,
    effective: HashMap<String, Result<EffectiveConfig, String>>,
    writable: HashMap<PathBuf, bool>,
}

impl ListState {
    pub(super) fn move_action(&mut self, right: bool) {
        let index = self.action.index();
        let next = if right {
            (index + 1).min(ListAction::ALL.len() - 1)
        } else {
            index.saturating_sub(1)
        };
        self.action = ListAction::ALL[next];
    }

    pub(super) fn rows<'a>(
        &'a self,
        config: &'a ConfigSet,
    ) -> impl ExactSizeIterator<Item = HostRow<'a>> {
        self.visible.iter().map(|&index| {
            let entry = &config.hosts[index];
            HostRow {
                entry,
                summary: &self.summaries[index],
                read_only: self.is_read_only(entry),
            }
        })
    }

    pub(super) fn is_read_only(&self, entry: &HostEntry) -> bool {
        !self.writable.get(&entry.source).copied().unwrap_or(false)
    }

    pub(super) fn filter(&self) -> &TextInput {
        &self.filter
    }

    pub(super) fn edit_filter(&mut self, config: &ConfigSet, key: KeyEvent) {
        let length = self.filter.value().len();
        self.filter.handle_key(key);
        if self.filter.value().len() != length {
            self.refilter(config, None, Some(0));
        }
    }

    pub(super) fn paste_filter(&mut self, config: &ConfigSet, text: &str) {
        let length = self.filter.value().len();
        self.filter.paste(text);
        if self.filter.value().len() != length {
            self.refilter(config, None, Some(0));
        }
    }

    pub(super) fn clear_filter(&mut self, config: &ConfigSet) {
        if !self.filter.value().is_empty() {
            self.filter.set(String::new());
            self.refilter(config, None, Some(0));
        }
    }

    pub(super) fn selected<'a>(&self, config: &'a ConfigSet) -> Option<&'a HostEntry> {
        self.visible
            .get(self.selection)
            .map(|&index| &config.hosts[index])
    }

    pub(super) fn last_host_index(&self) -> usize {
        self.visible.len().saturating_sub(1)
    }

    pub(super) fn new_row_index(&self) -> usize {
        self.visible.len()
    }

    pub(super) fn new_row_selected(&self) -> bool {
        self.selection == self.new_row_index()
    }

    pub(super) fn select(&mut self, index: usize) {
        self.selection = index.min(self.new_row_index());
    }

    pub(super) fn selected_effective(
        &self,
        config: &ConfigSet,
    ) -> Option<&Result<EffectiveConfig, String>> {
        let entry = self.selected(config)?;
        self.effective.get(&entry.name)
    }

    pub(super) fn refresh_selected(&mut self, backend: &impl Backend, config: &ConfigSet) {
        if let Some(entry) = self.selected(config) {
            self.effective(backend, &entry.name);
        }
    }

    pub(super) fn effective(
        &mut self,
        backend: &impl Backend,
        alias: &str,
    ) -> &Result<EffectiveConfig, String> {
        self.effective
            .entry(alias.to_owned())
            .or_insert_with(|| backend.effective(alias))
    }

    pub(super) fn reload(
        &mut self,
        backend: &impl Backend,
        config: &ConfigSet,
        preferred: Option<&str>,
    ) {
        self.action = ListAction::Open;
        self.effective.clear();
        self.writable.clear();
        self.summaries = config
            .hosts
            .iter()
            .map(|entry| host_summary(config, entry))
            .collect();
        for entry in &config.hosts {
            self.writable
                .entry(entry.source.clone())
                .or_insert_with(|| backend.is_writable(&entry.source));
        }
        let fallback =
            (!self.new_row_selected() || self.visible.is_empty()).then_some(self.selection);
        self.refilter(config, preferred, fallback);
    }

    fn refilter(&mut self, config: &ConfigSet, preferred: Option<&str>, fallback: Option<usize>) {
        let filter = self.filter.value().to_lowercase();
        self.visible = config
            .hosts
            .iter()
            .enumerate()
            .filter(|(_, entry)| filter.is_empty() || entry.name.to_lowercase().contains(&filter))
            .map(|(index, _)| index)
            .collect();

        let preferred = preferred.and_then(|name| {
            self.visible
                .iter()
                .position(|&index| config.hosts[index].name == name)
        });
        self.selection = preferred
            .or_else(|| fallback.map(|index| index.min(self.last_host_index())))
            .unwrap_or_else(|| self.new_row_index());
    }
}

fn host_summary(config: &ConfigSet, entry: &HostEntry) -> String {
    let fields = config
        .files
        .get(entry.file_index)
        .and_then(|file| block_fields(&file.parsed, entry.block_index).ok());
    let value = |key| match fields.as_ref()?.get(key) {
        FieldState::Value(value) => Some(value.as_str()),
        _ => None,
    };
    let address = value(OptionKey::HostName).unwrap_or(&entry.name);
    match value(OptionKey::User) {
        Some(user) => format!("{user}@{address}"),
        None => address.to_owned(),
    }
}
