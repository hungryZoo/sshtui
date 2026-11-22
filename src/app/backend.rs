//! Side effects required by the application state.

use std::ffi::OsString;
use std::path::Path;

use crate::config::{ConfigSet, PendingWrite, WriteError, WriteOutcome};
use crate::dialog::{DialogError, PickOutcome};
use crate::keys::KeyInfo;
use crate::known_hosts::ForgetPlan;
use crate::paths::SshPaths;
use crate::ssh::effective::EffectiveConfig;
use crate::update::UpdatePlan;

/// Every side effect the application state needs.
pub trait Backend {
    /// Returns the resolved SSH paths.
    fn paths(&self) -> &SshPaths;
    /// Returns SSH config arguments evaluated at call time.
    fn config_prefix(&self) -> Vec<OsString>;
    /// Loads the current SSH configuration.
    fn load_config(&self) -> Result<ConfigSet, String>;
    /// Resolves the settings SSH would use for an alias.
    fn effective(&self, alias: &str) -> Result<EffectiveConfig, String>;
    /// Returns the available private keys and their public metadata.
    fn discover_keys(&self) -> Vec<KeyInfo>;
    /// Returns whether a config file can be changed.
    fn is_writable(&self, path: &Path) -> bool;
    /// Plans removing the known_hosts entries stored under `name`.
    /// Returns Ok(None) when the file is missing or has no entry for the name.
    fn plan_forget(&self, _name: &str) -> Result<Option<ForgetPlan>, String> {
        Ok(None)
    }
    /// Writes the planned change if the original content still matches.
    fn commit(&self, pending: &PendingWrite) -> Result<WriteOutcome, WriteError>;
    /// Returns the reason the native file dialog is unavailable, if any.
    fn picker_unavailable_reason(&self) -> Option<String>;
    /// Returns platform-specific instructions for showing hidden files, if needed.
    fn hidden_files_hint(&self) -> Option<String>;
    /// Shows the native dialog starting in the SSH directory.
    fn pick_file(&self) -> Result<PickOutcome, DialogError>;
    /// Returns the update plan available when the session starts.
    fn update_plan(&self) -> Option<UpdatePlan> {
        None
    }
}
