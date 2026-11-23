//! Host filtering, selection, and cached lookups.

use std::collections::HashMap;
use std::path::PathBuf;

use crossterm::event::KeyEvent;

use crate::config::{ConfigSet, FieldState, HostEntry, OptionKey, ReadWarning, block_fields};
use crate::ssh::effective::EffectiveConfig;

use super::{Backend, TextInput};

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
    visible: Vec<usize>,
    summaries: Vec<String>,
    effective: HashMap<String, Result<EffectiveConfig, String>>,
    writable: HashMap<PathBuf, bool>,
}

impl ListState {
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

pub(super) fn describe_warning(warning: &ReadWarning) -> String {
    match warning {
        ReadWarning::Unreadable { path, error } => {
            format!("Cannot read included file {}: {error}", path.display())
        }
        ReadWarning::UnsupportedInclude {
            file,
            line_number,
            pattern,
        } => format!(
            "{}:{line_number}: unsupported Include pattern {pattern:?}",
            file.display()
        ),
        ReadWarning::InvalidGlob {
            file,
            line_number,
            pattern,
        } => format!(
            "{}:{line_number}: invalid Include pattern {pattern:?}",
            file.display()
        ),
        ReadWarning::DepthExceeded { path } => {
            format!("Include nesting is too deep at {}", path.display())
        }
    }
}
