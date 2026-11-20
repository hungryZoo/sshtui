//! Discovery and execution of the SSH executable.

use std::ffi::{OsStr, OsString};
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
}
