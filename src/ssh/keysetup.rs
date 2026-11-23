//! Public key validation and argument construction for installing keys with SSH.

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use crate::keys::parse_public_key_line;
use crate::paths::SshPaths;

use super::args::{ArgsError, ConnectTarget, connect_args};

/// Remote command that appends the public key read from stdin to authorized_keys.
pub const REMOTE_INSTALL_SCRIPT: &str = r#"exec sh -c 'umask 077; mkdir -p "$HOME/.ssh" || exit 1; cd "$HOME/.ssh" || exit 1; key=$(cat); [ -n "$key" ] || { echo "sshtui: no key received" >&2; exit 1; }; touch authorized_keys || exit 1; if grep -qxF -- "$key" authorized_keys; then echo "sshtui: key already installed"; else [ -z "$(tail -c 1 authorized_keys)" ] || echo >> authorized_keys; printf "%s\n" "$key" >> authorized_keys || exit 1; echo "sshtui: key installed"; fi; if command -v restorecon >/dev/null 2>&1; then restorecon -F "$HOME/.ssh" "$HOME/.ssh/authorized_keys" >/dev/null 2>&1 || true; fi'"#;

/// Builds the ssh arguments that install a public key for the given target.
pub fn install_args(
    paths: &SshPaths,
    prefix: &[OsString],
    target: &ConnectTarget,
) -> Result<Vec<OsString>, ArgsError> {
    let mut target = target.clone();
    if let ConnectTarget::Direct {
        disable_password, ..
    } = &mut target
    {
        *disable_password = false;
    }
    let mut args = connect_args(paths, prefix, &target)?;
    args.splice(
        prefix.len()..prefix.len(),
        ["-o".into(), "PasswordAuthentication=yes".into()],
    );
    args.push(REMOTE_INSTALL_SCRIPT.into());
    Ok(args)
}

/// Returns the public key path for a private key path typed in the form.
pub fn public_key_path(paths: &SshPaths, key_file: &str) -> PathBuf {
    let mut path = paths.expand_tilde(key_file.trim());
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    if !name.as_encoded_bytes().ends_with(b".pub") {
        name.push(".pub");
        path.set_file_name(name);
    }
    path
}

/// Why a public key cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum KeySetupError {
    /// The public key file does not exist.
    #[error("public key file not found: {path}")]
    Missing { path: PathBuf },
    /// An I/O error prevented reading the file.
    #[error("cannot read public key file: {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The file does not begin with a public key line.
    #[error("not a public key file: {path}")]
    NotAPublicKey { path: PathBuf },
}

/// Reads and validates the single public key line of a `.pub` file.
pub fn read_public_key(path: &Path) -> Result<String, KeySetupError> {
    const MAX_KEY_BYTES: u64 = 16 * 1024;

    let read_error = |source: io::Error| {
        let path = path.to_path_buf();
        if source.kind() == io::ErrorKind::NotFound {
            KeySetupError::Missing { path }
        } else {
            KeySetupError::Read { path, source }
        }
    };
    let invalid_key = || KeySetupError::NotAPublicKey {
        path: path.to_path_buf(),
    };
    let file = File::open(path).map_err(read_error)?;
    let mut reader = BufReader::new(file.take(MAX_KEY_BYTES));
    let mut bytes = Vec::new();
    reader.read_until(b'\n', &mut bytes).map_err(read_error)?;

    let line = std::str::from_utf8(&bytes)
        .map_err(|_| invalid_key())?
        .trim();
    if line.contains("PRIVATE KEY") || parse_public_key_line(line).is_none() {
        return Err(invalid_key());
    }
    Ok(line.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::args::config_file_args;
    use crate::validate::ValidationError;
    use std::fs;
    use tempfile::tempdir;

    const KEY: &str = "ssh-ed25519 AAAA alice@web";

    #[test]
    fn builds_install_arguments_for_an_alias() {
        let home = tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        let target = ConnectTarget::Alias("web".into());
        let args = install_args(&paths, &[], &target).unwrap();
        assert_eq!(
            args,
            [
                "-o",
                "PasswordAuthentication=yes",
                "web",
                REMOTE_INSTALL_SCRIPT
            ]
        );
    }

    #[test]
    fn enables_password_authentication_after_the_config_prefix_for_a_direct_target() {
        let home = tempdir().unwrap();
        let paths = SshPaths::resolve(
            Some(home.path().as_os_str()),
            Some(home.path()),
            home.path(),
        )
        .unwrap();
        let prefix = config_file_args(&paths, true);
        let target = ConnectTarget::Direct {
            user: "alice".into(),
            address: "example.com".into(),
            port: "2222".into(),
            key_file: "~/my key".into(),
            disable_password: true,
        };
        let args = install_args(&paths, &prefix, &target).unwrap();
        assert_eq!(
            args,
            [
                "-F".into(),
                paths.config_file().into_os_string(),
                "-o".into(),
                "PasswordAuthentication=yes".into(),
                "-p".into(),
                "2222".into(),
                "-i".into(),
                home.path().join("my key").into_os_string(),
                "alice@example.com".into(),
                REMOTE_INSTALL_SCRIPT.into(),
            ]
        );
    }

    #[test]
    fn propagates_invalid_target_errors() {
        let home = tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        let target = ConnectTarget::Alias("-web".into());
        assert_eq!(
            install_args(&paths, &[], &target),
            Err(ArgsError::InvalidAlias)
        );

        let target = ConnectTarget::Direct {
            user: String::new(),
            address: "example.com".into(),
            port: "0".into(),
            key_file: String::new(),
            disable_password: true,
        };
        assert_eq!(
            install_args(&paths, &[], &target),
            Err(ArgsError::Validation(ValidationError::InvalidPort))
        );
    }

    #[test]
    fn expands_tilde_and_appends_a_public_key_suffix_to_the_file_name() {
        let home = tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        for (input, expected) in [
            ("~/.ssh/id_ed25519", home.path().join(".ssh/id_ed25519.pub")),
            (
                "~/.ssh/id_ed25519.pub",
                home.path().join(".ssh/id_ed25519.pub"),
            ),
            ("my.key", PathBuf::from("my.key.pub")),
            ("  \tmy.key  \n", PathBuf::from("my.key.pub")),
        ] {
            assert_eq!(public_key_path(&paths, input), expected);
        }
    }

    #[test]
    fn reads_only_the_trimmed_first_line_with_its_comment() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("id_ed25519.pub");
        fs::write(&path, format!("\t{KEY}  \r\n")).unwrap();
        assert_eq!(read_public_key(&path).unwrap(), KEY);

        fs::write(&path, format!("{KEY}\nssh-rsa BBBB other@web\n")).unwrap();
        assert_eq!(read_public_key(&path).unwrap(), KEY);
    }

    #[test]
    fn limits_the_first_line_to_16_kib() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("id_ed25519.pub");
        let prefix = format!("{KEY}{}", "x".repeat(16 * 1024 - KEY.len()));
        fs::write(&path, format!("{prefix}ignored\n")).unwrap();
        assert_eq!(read_public_key(&path).unwrap(), prefix);
    }

    #[test]
    fn distinguishes_missing_files_and_read_errors() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("missing.pub");
        assert!(matches!(
            read_public_key(&missing),
            Err(KeySetupError::Missing { path }) if path == missing
        ));
        assert!(matches!(
            read_public_key(dir.path()),
            Err(KeySetupError::Read { path, .. }) if path == dir.path()
        ));
    }

    #[test]
    fn rejects_private_key_markers_and_unparseable_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("id_ed25519.pub");
        for contents in [
            b"-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n".as_slice(),
            b"ssh-ed25519 AAAA PRIVATE KEY\n",
            b"",
            b"ssh-ed25519\n",
        ] {
            fs::write(&path, contents).unwrap();
            let error = read_public_key(&path).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("not a public key file: {}", path.display())
            );
            assert!(
                matches!(error, KeySetupError::NotAPublicKey { path: invalid } if invalid == path)
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn remote_script_installs_a_key_once() {
        let home = tempdir().unwrap();
        let authorized_keys = home.path().join(".ssh/authorized_keys");
        let key_line = format!("{KEY}\n");
        let output = run_remote_script(home.path(), key_line.as_bytes());
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("key installed"));
        assert_eq!(fs::read_to_string(&authorized_keys).unwrap(), key_line);

        let output = run_remote_script(home.path(), key_line.as_bytes());
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("already installed"));
        assert_eq!(fs::read_to_string(&authorized_keys).unwrap(), key_line);
    }

    #[cfg(unix)]
    #[test]
    fn remote_script_adds_a_line_break_after_an_unterminated_key() {
        let home = tempdir().unwrap();
        let ssh_dir = home.path().join(".ssh");
        fs::create_dir(&ssh_dir).unwrap();
        let authorized_keys = ssh_dir.join("authorized_keys");
        let existing_key = "ssh-rsa BBBB other@web";
        fs::write(&authorized_keys, existing_key).unwrap();

        let output = run_remote_script(home.path(), format!("{KEY}\n").as_bytes());
        assert!(output.status.success());
        assert_eq!(
            fs::read_to_string(&authorized_keys).unwrap(),
            format!("{existing_key}\n{KEY}\n")
        );
    }

    #[cfg(unix)]
    #[test]
    fn remote_script_leaves_authorized_keys_unchanged_when_input_is_empty() {
        let home = tempdir().unwrap();
        let ssh_dir = home.path().join(".ssh");
        fs::create_dir(&ssh_dir).unwrap();
        let authorized_keys = ssh_dir.join("authorized_keys");
        let key_line = format!("{KEY}\n");
        fs::write(&authorized_keys, &key_line).unwrap();

        let output = run_remote_script(home.path(), b"");
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(fs::read_to_string(&authorized_keys).unwrap(), key_line);
    }

    #[cfg(unix)]
    fn run_remote_script(home: &Path, input: &[u8]) -> std::process::Output {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new("sh")
            .args(["-c", REMOTE_INSTALL_SCRIPT])
            .env("HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }
}
