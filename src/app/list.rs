//! Host filtering, selection, and cached lookups.

use std::collections::HashMap;
use std::path::PathBuf;

use crossterm::event::KeyEvent;

use crate::config::{ConfigSet, HostEntry, ReadWarning};
use crate::ssh::effective::EffectiveConfig;

use super::{Backend, TextInput};

/// A filtered list row and the writability of its source file.
#[derive(Debug, Clone, Copy)]
pub struct HostRow<'a> {
    /// Alias and source location.
    pub entry: &'a HostEntry,
    /// Whether changes to the source file are unavailable.
    pub read_only: bool,
}

#[derive(Default)]
pub(super) struct ListState {
    filter: TextInput,
    pub(super) filtering: bool,
    pub(super) selection: usize,
    visible: Vec<usize>,
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
            self.refilter(config, None, 0);
        }
    }

    pub(super) fn paste_filter(&mut self, config: &ConfigSet, text: &str) {
        let length = self.filter.value().len();
        self.filter.paste(text);
        if self.filter.value().len() != length {
            self.refilter(config, None, 0);
        }
    }

    pub(super) fn clear_filter(&mut self, config: &ConfigSet) {
        if !self.filter.value().is_empty() {
            self.filter.set(String::new());
            self.refilter(config, None, 0);
        }
    }

    pub(super) fn selected<'a>(&self, config: &'a ConfigSet) -> Option<&'a HostEntry> {
        self.visible
            .get(self.selection)
            .map(|&index| &config.hosts[index])
    }

    pub(super) fn last_index(&self) -> usize {
        self.visible.len().saturating_sub(1)
    }

    pub(super) fn select(&mut self, index: usize) {
        self.selection = index.min(self.last_index());
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
        for entry in &config.hosts {
            self.writable
                .entry(entry.source.clone())
                .or_insert_with(|| backend.is_writable(&entry.source));
        }
        self.refilter(config, preferred, self.selection);
    }

    fn refilter(&mut self, config: &ConfigSet, preferred: Option<&str>, fallback: usize) {
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
        self.select(preferred.unwrap_or(fallback));
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
