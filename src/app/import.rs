//! Shell history suggestions and their popup selection.

use crate::config::{ConfigSet, HostEntry, OptionKey};
use crate::history::{self, SavedHost, Suggestion};

/// Selection in the shell history import popup.
#[derive(Debug, Default)]
pub struct ImportState {
    pub(super) selection: usize,
}

impl ImportState {
    /// Returns the index of the selected suggestion.
    pub fn selection(&self) -> usize {
        self.selection
    }
}

pub(super) fn suggestions(commands: &[String], config: &ConfigSet) -> Vec<Suggestion> {
    if commands.is_empty() {
        return Vec::new();
    }
    let saved: Vec<_> = config
        .hosts
        .iter()
        .map(|entry| SavedHost {
            alias: &entry.name,
            host_name: block_host_name(config, entry),
        })
        .collect();
    history::suggest(commands.iter().map(String::as_str), &saved)
}

fn block_host_name<'a>(config: &'a ConfigSet, entry: &HostEntry) -> Option<&'a str> {
    let parsed = &config.files.get(entry.file_index)?.parsed;
    let block = parsed.blocks().get(entry.block_index)?;
    parsed.lines()[block.header + 1..block.end]
        .iter()
        .filter_map(|line| line.directive())
        .find_map(|directive| match directive.args.as_slice() {
            [arg] if directive.is(OptionKey::HostName.keyword()) => Some(arg.value.as_str()),
            _ => None,
        })
}
