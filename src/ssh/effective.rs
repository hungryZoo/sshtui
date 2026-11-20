//! Execution of `ssh -G` and parsing of its output.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use crate::config::edit::OptionKey;
use crate::paths::SshPaths;

use super::args::is_valid_alias;
use super::exec::{self, SshError};

/// Values ssh would actually use for an alias.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectiveConfig {
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<String>,
    /// In the order printed by ssh.
    pub identity_files: Vec<String>,
}

/// Parses known fields from `ssh -G` output, ignoring malformed lines.
pub fn parse_ssh_g(output: &str) -> EffectiveConfig {
    let mut config = EffectiveConfig::default();
    for line in output.lines() {
        let Some((key, value)) = line.split_once(' ') else {
            continue;
        };
        if value.is_empty() {
            continue;
        }

        match key {
            "hostname" => config.host_name = Some(value.into()),
            "user" => config.user = Some(value.into()),
            "port" => config.port = Some(value.into()),
            "identityfile" => config.identity_files.push(value.into()),
            _ => {}
        }
    }
    config
}

/// Builds the query arguments by appending `-G` and the alias to the config prefix.
pub fn ssh_g_args(prefix: &[OsString], alias: &str) -> Vec<OsString> {
    let mut args = prefix.to_vec();
    args.extend(["-G".into(), alias.into()]);
    args
}

/// Values entered for a saved host. Empty strings are omitted from comparison.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExpectedConfig {
    pub host_name: String,
    pub user: String,
    pub port: String,
    pub identity_file: String,
}

/// One entered value that ssh would not use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub key: OptionKey,
    pub expected: String,
    /// The reported value, or all reported identity files joined by `, `. A missing value is `-`.
    pub actual: String,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: saved {:?} but ssh will use {:?}",
            self.key.keyword(),
            self.expected,
            self.actual
        )
    }
}

/// Compares entered values with the effective configuration in form order.
pub fn find_mismatches(
    paths: &SshPaths,
    expected: &ExpectedConfig,
    effective: &EffectiveConfig,
) -> Vec<Mismatch> {
    let mut mismatches = Vec::new();

    let scalar_fields = [
        (
            OptionKey::HostName,
            expected.host_name.as_str(),
            effective.host_name.as_deref(),
            str::eq_ignore_ascii_case as fn(&str, &str) -> bool,
        ),
        (
            OptionKey::User,
            expected.user.as_str(),
            effective.user.as_deref(),
            str::eq,
        ),
        (
            OptionKey::Port,
            expected.port.as_str(),
            effective.port.as_deref(),
            ports_equal,
        ),
    ];
    for (key, entered, actual, equal) in scalar_fields {
        if !entered.is_empty() && !actual.is_some_and(|actual| equal(entered, actual)) {
            mismatches.push(Mismatch {
                key,
                expected: entered.into(),
                actual: actual.unwrap_or("-").into(),
            });
        }
    }

    if !expected.identity_file.is_empty() {
        let path = paths.expand_tilde(&expected.identity_file);
        if !effective
            .identity_files
            .iter()
            .any(|file| paths.expand_tilde(file) == path)
        {
            let actual = if effective.identity_files.is_empty() {
                "-".into()
            } else {
                effective.identity_files.join(", ")
            };
            mismatches.push(Mismatch {
                key: OptionKey::IdentityFile,
                expected: expected.identity_file.clone(),
                actual,
            });
        }
    }
    mismatches
}

fn ports_equal(expected: &str, actual: &str) -> bool {
    match (expected.parse::<u16>(), actual.parse::<u16>()) {
        (Ok(expected), Ok(actual)) => expected == actual,
        _ => expected == actual,
    }
}

/// Source of effective configurations, allowing the UI to be tested without ssh.
pub trait EffectiveResolver {
    /// Returns the values ssh would use for the given alias.
    fn resolve(&self, alias: &str) -> Result<EffectiveConfig, SshError>;
}

/// Queries the system ssh executable.
#[derive(Debug, Clone)]
pub struct SystemResolver {
    ssh: PathBuf,
    prefix: Vec<OsString>,
}

impl SystemResolver {
    /// Creates a resolver using the given executable and config arguments.
    pub fn new(ssh: PathBuf, prefix: Vec<OsString>) -> Self {
        Self { ssh, prefix }
    }
}

impl EffectiveResolver for SystemResolver {
    fn resolve(&self, alias: &str) -> Result<EffectiveConfig, SshError> {
        if !is_valid_alias(alias) {
            return Err(SshError::InvalidAlias);
        }
        let output = exec::run_capture(&self.ssh, &ssh_g_args(&self.prefix, alias))?;
        Ok(parse_ssh_g(&output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: &str = concat!(
        "host Web\n",
        "user alice\n",
        "hostname web.example.com\n",
        "port 2222\n",
        "identityfile ~/.ssh/id_test\n",
        "identityfile /tmp/my dir/key\n",
        "identityfile /abs/other\n",
    );

    #[test]
    fn parses_ssh_output_including_all_identity_files() {
        assert_eq!(
            parse_ssh_g(OUTPUT),
            EffectiveConfig {
                host_name: Some("web.example.com".into()),
                user: Some("alice".into()),
                port: Some("2222".into()),
                identity_files: vec![
                    "~/.ssh/id_test".into(),
                    "/tmp/my dir/key".into(),
                    "/abs/other".into()
                ],
            }
        );
    }

    #[test]
    fn ignores_malformed_and_unknown_lines_and_accepts_crlf() {
        assert_eq!(parse_ssh_g(""), EffectiveConfig::default());
        assert_eq!(
            parse_ssh_g(
                "user\r\nport \r\nunknown value\r\nhostname web\r\nidentityfile  key path \r\n"
            ),
            EffectiveConfig {
                host_name: Some("web".into()),
                identity_files: vec![" key path ".into()],
                ..EffectiveConfig::default()
            }
        );
    }

    #[test]
    fn matches_all_fields_even_when_hostname_case_differs() {
        let home = tempfile::tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        let expected = ExpectedConfig {
            host_name: "Web.Example.COM".into(),
            user: "alice".into(),
            port: "2222".into(),
            identity_file: "~/.ssh/id_test".into(),
        };
        assert!(find_mismatches(&paths, &expected, &parse_ssh_g(OUTPUT)).is_empty());
    }

    #[test]
    fn reports_a_different_user_and_formats_the_message() {
        let home = tempfile::tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        let expected = ExpectedConfig {
            user: "alice".into(),
            ..ExpectedConfig::default()
        };
        let mismatches = find_mismatches(&paths, &expected, &parse_ssh_g("user root\n"));
        assert_eq!(
            mismatches,
            [Mismatch {
                key: OptionKey::User,
                expected: "alice".into(),
                actual: "root".into()
            }]
        );
        assert_eq!(
            mismatches[0].to_string(),
            r#"User: saved "alice" but ssh will use "root""#
        );
    }

    #[test]
    fn compares_ports_numerically_with_a_string_fallback() {
        let home = tempfile::tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        let mut expected = ExpectedConfig {
            port: "022".into(),
            ..ExpectedConfig::default()
        };
        let effective = parse_ssh_g("port 22\n");
        assert!(find_mismatches(&paths, &expected, &effective).is_empty());

        expected.port = "2222".into();
        assert_eq!(
            find_mismatches(&paths, &expected, &effective),
            [Mismatch {
                key: OptionKey::Port,
                expected: "2222".into(),
                actual: "22".into()
            }]
        );

        expected.port = "invalid".into();
        assert!(find_mismatches(&paths, &expected, &parse_ssh_g("port invalid\n")).is_empty());
        assert_eq!(
            find_mismatches(&paths, &expected, &parse_ssh_g("port other\n")),
            [Mismatch {
                key: OptionKey::Port,
                expected: "invalid".into(),
                actual: "other".into()
            }]
        );
    }

    #[test]
    fn compares_expanded_paths_and_reports_the_full_identity_list() {
        let home = tempfile::tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        let absolute = home
            .path()
            .join(".ssh")
            .join("id_test")
            .to_string_lossy()
            .into_owned();
        let mut expected = ExpectedConfig {
            identity_file: "~/.ssh/id_test".into(),
            ..ExpectedConfig::default()
        };
        let effective = EffectiveConfig {
            identity_files: vec!["/abs/other".into(), absolute.clone()],
            ..EffectiveConfig::default()
        };
        assert!(find_mismatches(&paths, &expected, &effective).is_empty());

        expected.identity_file = absolute.clone();
        assert!(find_mismatches(&paths, &expected, &parse_ssh_g(OUTPUT)).is_empty());

        expected.identity_file = "~/.ssh/id_missing".into();
        assert_eq!(
            find_mismatches(&paths, &expected, &effective),
            [Mismatch {
                key: OptionKey::IdentityFile,
                expected: "~/.ssh/id_missing".into(),
                actual: format!("/abs/other, {absolute}"),
            }]
        );
    }

    #[test]
    fn skips_empty_inputs_and_reports_missing_fields_in_order() {
        let home = tempfile::tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        assert!(
            find_mismatches(&paths, &ExpectedConfig::default(), &parse_ssh_g(OUTPUT)).is_empty()
        );

        let expected = ExpectedConfig {
            host_name: "web".into(),
            user: "alice".into(),
            port: "22".into(),
            identity_file: "/abs/key".into(),
        };
        assert_eq!(
            find_mismatches(&paths, &expected, &EffectiveConfig::default()),
            [
                Mismatch {
                    key: OptionKey::HostName,
                    expected: "web".into(),
                    actual: "-".into()
                },
                Mismatch {
                    key: OptionKey::User,
                    expected: "alice".into(),
                    actual: "-".into()
                },
                Mismatch {
                    key: OptionKey::Port,
                    expected: "22".into(),
                    actual: "-".into()
                },
                Mismatch {
                    key: OptionKey::IdentityFile,
                    expected: "/abs/key".into(),
                    actual: "-".into()
                },
            ]
        );
    }

    #[test]
    fn builds_query_arguments_with_config_prefix() {
        assert_eq!(ssh_g_args(&[], "web"), ["-G", "web"]);
        assert_eq!(
            ssh_g_args(&["-F".into(), "my config".into()], "web"),
            ["-F", "my config", "-G", "web"]
        );
    }

    struct FakeResolver;

    impl EffectiveResolver for FakeResolver {
        fn resolve(&self, alias: &str) -> Result<EffectiveConfig, SshError> {
            Ok(EffectiveConfig {
                host_name: Some(alias.into()),
                ..EffectiveConfig::default()
            })
        }
    }

    #[test]
    fn can_use_a_resolver_through_a_trait_object() {
        let resolver: &dyn EffectiveResolver = &FakeResolver;
        assert_eq!(
            resolver.resolve("web").unwrap().host_name.as_deref(),
            Some("web")
        );
    }

    #[test]
    fn rejects_invalid_aliases_before_spawning_ssh() {
        let dir = tempfile::tempdir().unwrap();
        let resolver = SystemResolver::new(
            dir.path().join("missing-ssh"),
            vec!["-F".into(), "none".into()],
        );
        for alias in ["", "-oProxyCommand=x"] {
            assert!(matches!(
                resolver.resolve(alias),
                Err(SshError::InvalidAlias)
            ));
        }
    }

    #[test]
    fn resolves_a_host_with_system_ssh_and_a_temporary_config() {
        let Ok(ssh) = exec::find_ssh() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        std::fs::write(
            &config,
            "Host web\n    HostName Web.Example.COM\n    User alice\n    Port 2222\n    IdentityFile /abs/key\n",
        )
        .unwrap();

        let resolver = SystemResolver::new(ssh, vec!["-F".into(), config.into_os_string()]);
        let effective = resolver.resolve("web").unwrap();
        assert_eq!(effective.host_name.as_deref(), Some("web.example.com"));
        assert_eq!(effective.user.as_deref(), Some("alice"));
        assert_eq!(effective.port.as_deref(), Some("2222"));
        assert!(
            effective
                .identity_files
                .iter()
                .any(|file| file == "/abs/key")
        );
    }
}
