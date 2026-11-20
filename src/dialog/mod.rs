//! The native file picker trait and its platform-specific implementations.

use std::path::{Path, PathBuf};

/// External command pickers and their argument handling.
pub mod command;
/// Windows native file picker.
#[cfg(windows)]
pub mod windows;

/// Result of showing the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickOutcome {
    /// The selected file path.
    Picked(PathBuf),
    /// The picker closed without a selection.
    Cancelled,
}

/// Errors that prevent a picker from returning an outcome.
#[derive(Debug, thiserror::Error)]
pub enum DialogError {
    /// The picker cannot be shown in this session; the string explains why.
    #[error("{0}")]
    Unavailable(String),
    /// The picker command could not be executed.
    #[error("file dialog failed: {0}")]
    Failed(String),
}

/// A native "open file" dialog.
pub trait FilePicker {
    /// Why "Browse" is disabled, or None when the dialog can be shown.
    fn unavailable_reason(&self) -> Option<String>;
    /// Instructions for showing hidden files when the platform cannot force them.
    fn hidden_files_hint(&self) -> Option<String>;
    /// Shows the dialog starting in `start_dir` and blocks until it closes.
    fn pick_file(&self, start_dir: &Path) -> Result<PickOutcome, DialogError>;
}

/// Operating system family used to choose a picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// macOS.
    MacOs,
    /// Windows.
    Windows,
    /// Linux and other Unix systems.
    OtherUnix,
}

impl Platform {
    /// Returns the platform for which this program was compiled.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::OtherUnix
        }
    }
}

/// Snapshot of the environment facts that decide availability.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionEnv {
    /// Any of SSH_CONNECTION, SSH_CLIENT, SSH_TTY is set and non-empty.
    pub over_ssh: bool,
    /// DISPLAY is set and non-empty.
    pub display: bool,
    /// WAYLAND_DISPLAY is set and non-empty.
    pub wayland_display: bool,
    /// XDG_CURRENT_DESKTOP value, if any.
    pub current_desktop: Option<String>,
    /// zenity was found on PATH.
    pub has_zenity: bool,
    /// kdialog was found on PATH.
    pub has_kdialog: bool,
}

impl SessionEnv {
    /// Reads the process environment and looks up zenity and kdialog with `which`.
    pub fn from_process() -> Self {
        let is_set = |name| std::env::var_os(name).is_some_and(|value| !value.is_empty());
        Self {
            over_ssh: ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
                .into_iter()
                .any(is_set),
            display: is_set("DISPLAY"),
            wayland_display: is_set("WAYLAND_DISPLAY"),
            current_desktop: std::env::var("XDG_CURRENT_DESKTOP").ok(),
            has_zenity: which::which("zenity").is_ok(),
            has_kdialog: which::which("kdialog").is_ok(),
        }
    }
}

/// Returns why a GUI dialog cannot be shown, or None.
pub fn gui_unavailable_reason(platform: Platform, env: &SessionEnv) -> Option<String> {
    if env.over_ssh {
        return Some("Browse is unavailable over SSH (no local desktop session)".into());
    }
    if platform == Platform::OtherUnix {
        if !env.display && !env.wayland_display {
            return Some(
                "Browse is unavailable: no graphical session (DISPLAY and WAYLAND_DISPLAY are not set)"
                    .into(),
            );
        }
        if !env.has_zenity && !env.has_kdialog {
            return Some("Browse is unavailable: install zenity or kdialog".into());
        }
    }
    None
}

/// Builds the picker for the current platform.
pub fn system_picker() -> Box<dyn FilePicker> {
    let platform = Platform::current();
    let env = SessionEnv::from_process();
    let unavailable = gui_unavailable_reason(platform, &env);

    #[cfg(windows)]
    {
        Box::new(windows::RfdPicker::new(unavailable))
    }
    #[cfg(not(windows))]
    {
        let tool = if platform == Platform::MacOs {
            Some(command::Tool::Osascript)
        } else {
            command::choose_unix_tool(&env)
        };
        Box::new(command::CommandPicker::new(tool, unavailable))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_disables_the_dialog_on_all_platforms_before_other_checks() {
        for platform in [Platform::MacOs, Platform::Windows, Platform::OtherUnix] {
            for env in [
                SessionEnv {
                    over_ssh: true,
                    ..SessionEnv::default()
                },
                SessionEnv {
                    over_ssh: true,
                    display: true,
                    has_zenity: true,
                    ..SessionEnv::default()
                },
            ] {
                assert_eq!(
                    gui_unavailable_reason(platform, &env).as_deref(),
                    Some("Browse is unavailable over SSH (no local desktop session)")
                );
            }
        }
    }

    #[test]
    fn unix_requires_a_display_before_checking_tools() {
        let env = SessionEnv::default();
        assert_eq!(
            gui_unavailable_reason(Platform::OtherUnix, &env).as_deref(),
            Some(
                "Browse is unavailable: no graphical session (DISPLAY and WAYLAND_DISPLAY are not set)"
            )
        );

        let env = SessionEnv {
            display: true,
            ..env
        };
        assert_eq!(
            gui_unavailable_reason(Platform::OtherUnix, &env).as_deref(),
            Some("Browse is unavailable: install zenity or kdialog")
        );
    }

    #[test]
    fn wayland_and_zenity_are_enough_on_unix() {
        let env = SessionEnv {
            wayland_display: true,
            has_zenity: true,
            ..SessionEnv::default()
        };
        assert_eq!(gui_unavailable_reason(Platform::OtherUnix, &env), None);
    }

    #[test]
    fn native_platforms_do_not_require_unix_display_variables_or_tools() {
        let env = SessionEnv::default();
        for platform in [Platform::MacOs, Platform::Windows] {
            assert_eq!(gui_unavailable_reason(platform, &env), None);
        }
    }

    #[test]
    fn fake_picker_can_be_used_as_a_trait_object() {
        struct FakePicker;

        impl FilePicker for FakePicker {
            fn unavailable_reason(&self) -> Option<String> {
                None
            }

            fn hidden_files_hint(&self) -> Option<String> {
                None
            }

            fn pick_file(&self, start_dir: &Path) -> Result<PickOutcome, DialogError> {
                Ok(PickOutcome::Picked(start_dir.join("id_test")))
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let picker: Box<dyn FilePicker> = Box::new(FakePicker);
        assert_eq!(picker.unavailable_reason(), None);
        assert_eq!(picker.hidden_files_hint(), None);
        assert_eq!(
            picker.pick_file(dir.path()).unwrap(),
            PickOutcome::Picked(dir.path().join("id_test"))
        );
    }
}
