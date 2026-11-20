//! SSH argument lists and their preview strings.

use std::ffi::OsString;

use crate::paths::SshPaths;
use crate::validate::{
    ValidationError, parse_port, validate_address, validate_key_file, validate_user,
};

/// What a connection targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectTarget {
    /// A saved alias: `ssh <alias>`.
    Alias(String),
    /// A one-off connection built from form fields. Empty strings mean "not given".
    Direct {
        user: String,
        address: String,
        port: String,
        key_file: String,
    },
}

/// A connection target that cannot be passed to ssh.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgsError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("host alias must not be empty or start with '-'")]
    InvalidAlias,
}

/// Selects the injected config with `-F`, using `none` when the file is absent.
/// The default SSH directory needs no arguments.
pub fn config_file_args(paths: &SshPaths, config_exists: bool) -> Vec<OsString> {
    if !paths.is_overridden() {
        return Vec::new();
    }

    let config = if config_exists {
        paths.config_file().into_os_string()
    } else {
        "none".into()
    };
    vec!["-F".into(), config]
}

/// Builds the arguments after the program name, starting with the config prefix.
pub fn connect_args(
    paths: &SshPaths,
    prefix: &[OsString],
    target: &ConnectTarget,
) -> Result<Vec<OsString>, ArgsError> {
    let mut args = prefix.to_vec();
    match target {
        ConnectTarget::Alias(alias) => {
            if !is_valid_alias(alias) {
                return Err(ArgsError::InvalidAlias);
            }
            args.push(alias.into());
        }
        ConnectTarget::Direct {
            user,
            address,
            port,
            key_file,
        } => {
            validate_address(address)?;
            validate_user(user)?;
            let port = parse_port(port)?;
            validate_key_file(key_file)?;

            if let Some(port) = port {
                args.extend(["-p".into(), port.to_string().into()]);
            }
            if !key_file.is_empty() {
                args.extend(["-i".into(), paths.expand_tilde(key_file).into_os_string()]);
            }

            let destination = if user.is_empty() {
                address.clone()
            } else {
                format!("{user}@{address}")
            };
            args.push(destination.into());
        }
    }
    Ok(args)
}

/// Formats a command line for display only, quoting arguments when needed.
pub fn preview(args: &[OsString]) -> String {
    let mut command = String::from("ssh");
    for arg in args {
        let arg = arg.to_string_lossy();
        command.push(' ');
        if !arg.is_empty()
            && arg
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"@%+=:,./-_~".contains(&byte))
        {
            command.push_str(&arg);
        } else {
            command.push('\'');
            command.push_str(&arg.replace('\'', r"'\''"));
            command.push('\'');
        }
    }
    command
}

pub(super) fn is_valid_alias(alias: &str) -> bool {
    !alias.is_empty() && !alias.starts_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::Field;

    fn direct(user: &str, address: &str, port: &str, key_file: &str) -> ConnectTarget {
        ConnectTarget::Direct {
            user: user.into(),
            address: address.into(),
            port: port.into(),
            key_file: key_file.into(),
        }
    }

    #[test]
    fn connects_to_saved_aliases() {
        let home = tempfile::tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        let args = connect_args(&paths, &[], &ConnectTarget::Alias("web".into())).unwrap();
        assert_eq!(args, ["web"]);
        assert_eq!(preview(&args), "ssh web");

        let args = connect_args(&paths, &[], &ConnectTarget::Alias("legacy#alias".into())).unwrap();
        assert_eq!(args, ["legacy#alias"]);
    }

    #[test]
    fn rejects_empty_and_option_like_aliases() {
        let home = tempfile::tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        for alias in ["", "-oProxyCommand=x"] {
            assert_eq!(
                connect_args(&paths, &[], &ConnectTarget::Alias(alias.into())),
                Err(ArgsError::InvalidAlias)
            );
        }
    }

    #[test]
    fn connects_to_an_address_without_options() {
        let home = tempfile::tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        let args = connect_args(&paths, &[], &direct("", "example.com", "", "")).unwrap();
        assert_eq!(args, ["example.com"]);
    }

    #[test]
    fn builds_direct_arguments_in_order_and_expands_tilde() {
        let home = tempfile::tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        let target = direct("alice", "example.com", "2222", "~/.ssh/id_ed25519");
        let args = connect_args(&paths, &[], &target).unwrap();
        let key_path = home.path().join(".ssh").join("id_ed25519").into_os_string();
        assert_eq!(
            args,
            [
                "-p".into(),
                "2222".into(),
                "-i".into(),
                key_path,
                "alice@example.com".into()
            ]
        );
    }

    #[test]
    fn normalizes_ports_and_preserves_ipv6_destinations() {
        let home = tempfile::tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        let args = connect_args(&paths, &[], &direct("", "host", "022", "")).unwrap();
        assert_eq!(args, ["-p", "22", "host"]);
        let args = connect_args(&paths, &[], &direct("alice", "::1", "", "")).unwrap();
        assert_eq!(args, ["alice@::1"]);
    }

    #[test]
    fn propagates_form_validation_errors() {
        let home = tempfile::tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        let cases = [
            (
                direct("", "-x", "", ""),
                ValidationError::LeadingDash {
                    field: Field::Address,
                },
            ),
            (
                direct("-l", "host", "", ""),
                ValidationError::LeadingDash { field: Field::User },
            ),
            (direct("", "host", "0", ""), ValidationError::InvalidPort),
            (
                direct("", "host", "", "a\"b"),
                ValidationError::ForbiddenChar {
                    field: Field::KeyFile,
                    ch: '"',
                },
            ),
            (
                direct("", "", "", ""),
                ValidationError::Empty {
                    field: Field::Address,
                },
            ),
        ];
        for (target, error) in cases {
            assert_eq!(
                connect_args(&paths, &[], &target),
                Err(ArgsError::Validation(error))
            );
        }
    }

    #[test]
    fn prefixes_arguments_for_an_overridden_config() {
        let dir = tempfile::tempdir().unwrap();
        let default = SshPaths::resolve(None, Some(dir.path()), dir.path()).unwrap();
        assert!(config_file_args(&default, true).is_empty());
        assert!(config_file_args(&default, false).is_empty());

        let paths = SshPaths::resolve(Some(dir.path().as_os_str()), None, dir.path()).unwrap();
        assert_eq!(config_file_args(&paths, false), ["-F", "none"]);

        let config = dir.path().join("config").into_os_string();
        let prefix = config_file_args(&paths, true);
        assert_eq!(prefix, ["-F".into(), config.clone()]);
        let args = connect_args(&paths, &prefix, &ConnectTarget::Alias("web".into())).unwrap();
        assert_eq!(args, ["-F".into(), config.clone(), "web".into()]);
        let args = connect_args(&paths, &prefix, &direct("", "host", "22", "")).unwrap();
        assert_eq!(
            args,
            ["-F".into(), config, "-p".into(), "22".into(), "host".into()]
        );
    }

    #[test]
    fn keeps_a_key_path_with_spaces_in_one_argument() {
        let home = tempfile::tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        let args = connect_args(&paths, &[], &direct("", "host", "", "/keys/my key")).unwrap();
        assert_eq!(args, ["-i", "/keys/my key", "host"]);
        assert_eq!(preview(&args), "ssh -i '/keys/my key' host");
    }

    #[test]
    fn preview_quotes_apostrophes_and_empty_arguments() {
        assert_eq!(preview(&[]), "ssh");
        assert_eq!(
            preview(&["a'b".into(), "".into(), "@%+=:,./-_~Az09".into()]),
            r"ssh 'a'\''b' '' @%+=:,./-_~Az09"
        );
    }
}
