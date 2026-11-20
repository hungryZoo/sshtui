//! File pickers backed by native desktop commands.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{DialogError, FilePicker, PickOutcome, SessionEnv};

pub(super) const PICKER_TITLE: &str = "Select an SSH private key";
const NO_TOOL: &str = "Browse is unavailable: no supported dialog tool found";

/// Which external tool shows the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// AppleScript on macOS.
    Osascript,
    /// Zenity file chooser.
    Zenity,
    /// KDE file chooser.
    Kdialog,
}

/// A program and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// The executable to run.
    pub program: OsString,
    /// Arguments passed directly to the executable.
    pub args: Vec<OsString>,
}

/// AppleScript source used on macOS. The start directory is passed as an argument
/// (`on run argv`), never interpolated into the script.
pub const OSASCRIPT_SOURCE: &str = r#"on run argv
	set startDir to POSIX file (item 1 of argv)
	try
		set chosen to choose file with prompt "Select an SSH private key" default location startDir with invisibles
		return POSIX path of chosen
	on error number -128
		return ""
	end try
end run"#;

/// Chooses kdialog on KDE when installed, otherwise prefers zenity and falls back to kdialog.
pub fn choose_unix_tool(env: &SessionEnv) -> Option<Tool> {
    let is_kde = env
        .current_desktop
        .as_deref()
        .is_some_and(|desktop| desktop.to_ascii_lowercase().contains("kde"));

    if env.has_kdialog && (is_kde || !env.has_zenity) {
        Some(Tool::Kdialog)
    } else if env.has_zenity {
        Some(Tool::Zenity)
    } else {
        None
    }
}

/// Builds the command line for `tool` starting in `start_dir`.
pub fn invocation(tool: Tool, start_dir: &Path) -> Invocation {
    let dir = start_dir.as_os_str();
    let (program, args) = match tool {
        Tool::Osascript => (
            "osascript",
            vec!["-e".into(), OSASCRIPT_SOURCE.into(), dir.into()],
        ),
        Tool::Zenity => {
            let mut filename = OsString::from("--filename=");
            filename.push(dir);
            if !dir.as_encoded_bytes().ends_with(b"/") {
                filename.push("/");
            }
            (
                "zenity",
                vec![
                    "--file-selection".into(),
                    format!("--title={PICKER_TITLE}").into(),
                    filename,
                ],
            )
        }
        Tool::Kdialog => (
            "kdialog",
            vec![
                "--title".into(),
                PICKER_TITLE.into(),
                "--getopenfilename".into(),
                dir.into(),
            ],
        ),
    };
    Invocation {
        program: program.into(),
        args,
    }
}

/// Interprets a failed status or empty selection as cancellation.
/// Removes one trailing LF or CRLF and preserves all other whitespace.
pub fn parse_output(success: bool, stdout: &[u8]) -> PickOutcome {
    let path = stdout
        .strip_suffix(b"\r\n")
        .or_else(|| stdout.strip_suffix(b"\n"))
        .unwrap_or(stdout);
    if !success || path.is_empty() {
        return PickOutcome::Cancelled;
    }

    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(std::ffi::OsStr::from_bytes(path))
    };
    #[cfg(not(unix))]
    let path = PathBuf::from(String::from_utf8_lossy(path).as_ref());

    PickOutcome::Picked(path)
}

/// Returns a hint for showing hidden files, or None when the tool forces them.
pub fn hidden_files_hint(tool: Tool) -> Option<&'static str> {
    match tool {
        Tool::Osascript => None,
        Tool::Zenity => Some("Hidden files: press Ctrl+H in the dialog"),
        Tool::Kdialog => Some("Hidden files: press Alt+. in the dialog"),
    }
}

/// Finds the nearest existing directory at or above `dir`, falling back to `dir`.
pub fn existing_start_dir(dir: &Path) -> PathBuf {
    dir.ancestors()
        .find(|ancestor| ancestor.is_dir())
        .unwrap_or(dir)
        .to_path_buf()
}

/// Runs the given program and reads its selection from standard output.
pub fn run_invocation(invocation: &Invocation) -> Result<PickOutcome, DialogError> {
    let output = Command::new(&invocation.program)
        .args(&invocation.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| DialogError::Failed(error.to_string()))?;

    Ok(parse_output(output.status.success(), &output.stdout))
}

/// Picker backed by an external command.
#[derive(Debug, Clone)]
pub struct CommandPicker {
    tool: Option<Tool>,
    unavailable: Option<String>,
}

impl CommandPicker {
    /// Creates a picker with the selected tool and any session availability restriction.
    pub fn new(tool: Option<Tool>, unavailable: Option<String>) -> Self {
        Self { tool, unavailable }
    }
}

impl FilePicker for CommandPicker {
    fn unavailable_reason(&self) -> Option<String> {
        self.unavailable
            .clone()
            .or_else(|| self.tool.is_none().then(|| NO_TOOL.into()))
    }

    fn hidden_files_hint(&self) -> Option<String> {
        self.tool.and_then(hidden_files_hint).map(str::to_owned)
    }

    fn pick_file(&self, start_dir: &Path) -> Result<PickOutcome, DialogError> {
        if let Some(reason) = self.unavailable_reason() {
            return Err(DialogError::Unavailable(reason));
        }
        let tool = self
            .tool
            .ok_or_else(|| DialogError::Unavailable(NO_TOOL.into()))?;
        run_invocation(&invocation(tool, &existing_start_dir(start_dir)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_tool_selection_prefers_the_desktop_tool_and_falls_back() {
        for (desktop, has_zenity, has_kdialog, expected) in [
            (Some("KDE"), true, true, Some(Tool::Kdialog)),
            (Some("ubuntu:kde"), true, true, Some(Tool::Kdialog)),
            (Some("GNOME"), true, true, Some(Tool::Zenity)),
            (Some("ubuntu:GNOME"), true, true, Some(Tool::Zenity)),
            (Some("KDE"), true, false, Some(Tool::Zenity)),
            (None, false, true, Some(Tool::Kdialog)),
            (None, false, false, None),
        ] {
            let env = SessionEnv {
                current_desktop: desktop.map(str::to_owned),
                has_zenity,
                has_kdialog,
                ..SessionEnv::default()
            };
            assert_eq!(choose_unix_tool(&env), expected, "{env:?}");
        }
    }

    #[test]
    fn osascript_passes_the_directory_as_a_separate_argument() {
        let dir = "/keys with spaces";
        let invocation = invocation(Tool::Osascript, Path::new(dir));
        assert_eq!(invocation.program, "osascript");
        assert_eq!(invocation.args, ["-e", OSASCRIPT_SOURCE, dir]);
        assert!(
            invocation.args[1]
                .to_str()
                .unwrap()
                .contains("with invisibles")
        );
        assert!(
            invocation.args[1]
                .to_str()
                .unwrap()
                .contains("default location")
        );
        assert!(!invocation.args[1].to_str().unwrap().contains(dir));
    }

    #[test]
    fn zenity_adds_a_trailing_slash_only_when_needed() {
        for dir in ["/keys with spaces", "/keys with spaces/"] {
            let invocation = invocation(Tool::Zenity, Path::new(dir));
            assert_eq!(invocation.program, "zenity");
            assert_eq!(
                invocation.args,
                [
                    "--file-selection",
                    "--title=Select an SSH private key",
                    "--filename=/keys with spaces/",
                ]
            );
        }
    }

    #[test]
    fn kdialog_passes_the_directory_as_a_separate_argument() {
        let dir = "/keys with spaces";
        let invocation = invocation(Tool::Kdialog, Path::new(dir));
        assert_eq!(invocation.program, "kdialog");
        assert_eq!(
            invocation.args,
            [
                "--title",
                "Select an SSH private key",
                "--getopenfilename",
                dir
            ]
        );
    }

    #[test]
    fn output_keeps_the_path_and_removes_only_one_line_ending() {
        for (output, path) in [
            ("/keys/id_key", "/keys/id_key"),
            ("/keys/id_key\n", "/keys/id_key"),
            ("/keys/id_key\r\n", "/keys/id_key"),
            (
                " /keys with spaces/id_key \t\n",
                " /keys with spaces/id_key \t",
            ),
            ("/keys/id_key\n\n", "/keys/id_key\n"),
            ("/keys/id_key\r", "/keys/id_key\r"),
        ] {
            assert_eq!(
                parse_output(true, output.as_bytes()),
                PickOutcome::Picked(path.into())
            );
        }
    }

    #[test]
    fn failed_status_or_empty_selection_cancels() {
        for output in ["", "\n", "\r\n"] {
            assert_eq!(
                parse_output(true, output.as_bytes()),
                PickOutcome::Cancelled
            );
        }
        assert_eq!(
            parse_output(false, b"/keys/id_key\n"),
            PickOutcome::Cancelled
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_output_preserves_non_utf8_path_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let path = b"/keys/id_\xff";
        assert_eq!(
            parse_output(true, b"/keys/id_\xff\n"),
            PickOutcome::Picked(PathBuf::from(std::ffi::OsStr::from_bytes(path)))
        );
    }

    #[test]
    fn hidden_file_hints_match_each_tool() {
        assert_eq!(hidden_files_hint(Tool::Osascript), None);
        assert_eq!(
            hidden_files_hint(Tool::Zenity),
            Some("Hidden files: press Ctrl+H in the dialog")
        );
        assert_eq!(
            hidden_files_hint(Tool::Kdialog),
            Some("Hidden files: press Alt+. in the dialog")
        );
        let picker = CommandPicker::new(Some(Tool::Zenity), None);
        assert_eq!(picker.unavailable_reason(), None);
        assert_eq!(
            picker.hidden_files_hint().as_deref(),
            hidden_files_hint(Tool::Zenity)
        );
    }

    #[test]
    fn existing_start_dir_uses_the_nearest_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("keys");
        std::fs::create_dir(&nested).unwrap();
        let file = nested.join("regular-file");
        std::fs::write(&file, []).unwrap();

        assert_eq!(existing_start_dir(&nested), nested);
        assert_eq!(existing_start_dir(&nested.join("missing/child")), nested);
        assert_eq!(existing_start_dir(&file), nested);
    }

    #[test]
    fn unavailable_picker_returns_the_session_reason_without_launching() {
        let dir = tempfile::tempdir().unwrap();
        let reason = "No local desktop session";
        for tool in [Some(Tool::Osascript), None] {
            let picker = CommandPicker::new(tool, Some(reason.into()));
            assert_eq!(picker.unavailable_reason().as_deref(), Some(reason));
            assert!(matches!(
                picker.pick_file(dir.path()),
                Err(DialogError::Unavailable(message)) if message == reason
            ));
        }
    }

    #[test]
    fn missing_tool_has_a_default_unavailability_reason() {
        let dir = tempfile::tempdir().unwrap();
        let picker = CommandPicker::new(None, None);
        assert_eq!(
            picker.unavailable_reason().as_deref(),
            Some("Browse is unavailable: no supported dialog tool found")
        );
        assert_eq!(picker.hidden_files_hint(), None);
        assert!(matches!(
            picker.pick_file(dir.path()),
            Err(DialogError::Unavailable(message)) if message == NO_TOOL
        ));
    }

    #[cfg(unix)]
    #[test]
    fn run_invocation_distinguishes_selection_cancellation_and_spawn_failure() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake picker");
        std::fs::write(&script, "#!/bin/sh\nprintf '%s\\n' \"$1\"\nexit \"$2\"\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();

        let selection = dir.path().join("key with spaces");
        let mut invocation = Invocation {
            program: script.into_os_string(),
            args: vec![selection.clone().into_os_string(), "0".into()],
        };
        assert_eq!(
            run_invocation(&invocation).unwrap(),
            PickOutcome::Picked(selection)
        );

        invocation.args[1] = "1".into();
        assert_eq!(run_invocation(&invocation).unwrap(), PickOutcome::Cancelled);

        invocation.program = dir.path().join("missing picker").into_os_string();
        assert!(matches!(
            run_invocation(&invocation),
            Err(DialogError::Failed(_))
        ));
    }
}
