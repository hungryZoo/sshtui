//! Discovery and execution of the SSH executable.

use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// A failure locating or running ssh.
#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error(
        "the `ssh` command was not found in PATH; install the OpenSSH client and make sure `ssh` is on PATH"
    )]
    NotFound,
    #[error("host alias must not be empty or start with '-'")]
    InvalidAlias,
    #[error("cannot run {program}: {source}")]
    Io {
        program: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("ssh exited with {status}: {stderr}")]
    Failed { status: String, stderr: String },
}

/// Locates ssh on the current process's PATH.
pub fn find_ssh() -> Result<PathBuf, SshError> {
    which::which("ssh").map_err(|_| SshError::NotFound)
}

/// Locates ssh in an explicit search path, resolving relative entries against `cwd`.
pub fn find_ssh_in(search_path: Option<&OsStr>, cwd: &Path) -> Result<PathBuf, SshError> {
    which::which_in("ssh", search_path, cwd).map_err(|_| SshError::NotFound)
}

/// Runs ssh with stdin closed, returning lossy UTF-8 stdout on success.
/// A failed exit includes the status and trimmed stderr.
pub fn run_capture(ssh: &Path, args: &[OsString]) -> Result<String, SshError> {
    let output = Command::new(ssh)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|source| SshError::Io {
            program: ssh.to_path_buf(),
            source,
        })?;

    if !output.status.success() {
        return Err(SshError::Failed {
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().into(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Hands the restored terminal over to ssh.
/// On Unix ssh replaces the current process. Elsewhere it runs as a child and returns its exit code.
pub fn run_interactive(ssh: &Path, args: &[OsString]) -> Result<i32, SshError> {
    let mut command = Command::new(ssh);
    command
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    #[cfg(unix)]
    {
        Err(SshError::Io {
            program: ssh.to_path_buf(),
            source: command.exec(),
        })
    }
    #[cfg(not(unix))]
    {
        let status = command.status().map_err(|source| SshError::Io {
            program: ssh.to_path_buf(),
            source,
        })?;
        Ok(status.code().unwrap_or(1))
    }
}

/// Runs ssh as a child with `input` on stdin while stdout and stderr stay attached to the terminal.
/// Returns the child's exit code, or 1 if it has no exit code.
pub fn run_with_input(ssh: &Path, args: &[OsString], input: &[u8]) -> Result<i32, SshError> {
    let io_error = |source| SshError::Io {
        program: ssh.to_path_buf(),
        source,
    };
    let mut child = Command::new(ssh)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(io_error)?;

    let written = child
        .stdin
        .take()
        .map_or(Ok(()), |mut stdin| stdin.write_all(input));
    let status = child.wait().map_err(io_error)?;
    if let Err(source) = written
        && source.kind() != io::ErrorKind::BrokenPipe
    {
        return Err(io_error(source));
    }
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn write_executable(path: &Path, contents: &str) {
        use std::os::unix::fs::PermissionsExt;

        std::fs::write(path, contents).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn finds_ssh_on_an_explicit_search_path() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();

        #[cfg(unix)]
        let ssh = {
            let ssh = bin.join("ssh");
            write_executable(&ssh, "");
            ssh
        };
        #[cfg(windows)]
        let ssh = {
            let ssh = bin.join("ssh.exe");
            std::fs::write(&ssh, "").unwrap();
            ssh
        };

        let search_path = std::env::join_paths([&bin]).unwrap();
        assert_eq!(find_ssh_in(Some(&search_path), dir.path()).unwrap(), ssh);
        assert_eq!(
            find_ssh_in(Some(OsStr::new("bin")), dir.path()).unwrap(),
            ssh
        );
    }

    #[test]
    fn reports_not_found_for_empty_or_missing_search_path() {
        let dir = tempfile::tempdir().unwrap();
        let search_path = std::env::join_paths([dir.path()]).unwrap();
        let error = find_ssh_in(Some(&search_path), dir.path()).unwrap_err();
        assert!(matches!(error, SshError::NotFound));
        assert!(error.to_string().contains("PATH"));
        assert!(matches!(
            find_ssh_in(None, dir.path()),
            Err(SshError::NotFound)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn captures_stdout_and_passes_a_spaced_argument_whole() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("fake-ssh");
        write_executable(
            &program,
            "#!/bin/sh\nif read -r unexpected; then exit 1; fi\nprintf '%s\\n' \"$#\" \"$1\"\n",
        );
        let output = run_capture(&program, &["one argument".into()]).unwrap();
        assert_eq!(output, "1\none argument\n");
    }

    #[cfg(unix)]
    #[test]
    fn captures_exit_status_and_trimmed_stderr_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("fake-ssh");
        write_executable(
            &program,
            "#!/bin/sh\nprintf '  query failed  \\n' >&2\nexit 7\n",
        );
        let error = run_capture(&program, &[]).unwrap_err();
        match error {
            SshError::Failed { status, stderr } => {
                assert_eq!(status, "exit status: 7");
                assert_eq!(stderr, "query failed");
            }
            other => panic!("expected a failed exit, got {other:?}"),
        }
    }

    #[test]
    fn reports_io_error_for_a_missing_program() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing-ssh");
        let error = run_capture(&missing, &[]).unwrap_err();
        match error {
            SshError::Io { program, source } => {
                assert_eq!(program, missing);
                assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
            }
            other => panic!("expected an I/O error, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn passes_arguments_and_stdin_and_returns_the_child_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("fake-ssh");
        write_executable(
            &program,
            "#!/bin/sh\ncat > \"$1\" || exit 1\nprintf '%s\\n' \"$3\" \"$4\" > \"$2\" || exit 1\nexit 7\n",
        );
        let recorded_input = dir.path().join("recorded-input");
        let recorded_args = dir.path().join("recorded-args");
        let args = [
            recorded_input.clone().into_os_string(),
            recorded_args.clone().into_os_string(),
            "one argument".into(),
            "another".into(),
        ];
        let input = b"first line\nsecond line\0";
        assert_eq!(run_with_input(&program, &args, input).unwrap(), 7);
        assert_eq!(std::fs::read(recorded_input).unwrap(), input);
        assert_eq!(
            std::fs::read_to_string(recorded_args).unwrap(),
            "one argument\nanother\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn returns_the_exit_code_when_the_child_closes_stdin_early() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("fake-ssh");
        write_executable(&program, "#!/bin/sh\nexec 0<&-\nexit 7\n");
        let input = vec![b'x'; 1024 * 1024];
        assert_eq!(run_with_input(&program, &[], &input).unwrap(), 7);
    }

    #[cfg(unix)]
    #[test]
    fn returns_one_when_the_child_exits_without_a_code() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("fake-ssh");
        write_executable(&program, "#!/bin/sh\nkill -TERM \"$$\"\n");
        assert_eq!(run_with_input(&program, &[], b"").unwrap(), 1);
    }

    #[test]
    fn reports_io_error_for_a_missing_program_when_passing_stdin() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing-ssh");
        assert!(matches!(
            run_with_input(&missing, &[], b"input"),
            Err(SshError::Io { program, source })
                if program == missing && source.kind() == io::ErrorKind::NotFound
        ));
    }
}
