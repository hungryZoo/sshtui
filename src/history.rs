//! Extraction of unsaved SSH destinations from shell history.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::validate::{parse_port, validate_address, validate_key_file, validate_user};

/// Format of a shell history file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFormat {
    /// zsh, plain or extended (`: <start>:<elapsed>;<command>`), with metafied bytes.
    Zsh,
    /// Commands separated by newlines or NULs, with optional numeric timestamps.
    Plain,
    /// fish entries beginning with "- cmd: ".
    Fish,
}

/// Returns the commands in file order (oldest first).
pub fn parse_commands(format: HistoryFormat, bytes: &[u8]) -> Vec<String> {
    let decoded;
    let bytes = if format == HistoryFormat::Zsh {
        decoded = unmetafy_zsh(bytes);
        decoded.as_slice()
    } else {
        bytes
    };
    let history = String::from_utf8_lossy(bytes);
    let separators: &[_] = if format == HistoryFormat::Plain {
        &['\n', '\0']
    } else {
        &['\n']
    };
    let lines = history
        .split_terminator(separators)
        .map(|line| line.strip_suffix('\r').unwrap_or(line));
    let mut commands: Vec<_> = match format {
        HistoryFormat::Zsh => zsh_commands(lines),
        HistoryFormat::Plain => lines
            .map(|line| {
                line.trim_start_matches(|ch: char| ch.is_ascii_control() || ch == '\u{fffd}')
            })
            .filter(|line| {
                !line.strip_prefix('#').is_some_and(|timestamp| {
                    is_decimal(timestamp.strip_prefix('+').unwrap_or(timestamp))
                })
            })
            .map(str::to_owned)
            .collect(),
        HistoryFormat::Fish => lines
            .filter_map(|line| line.strip_prefix("- cmd: "))
            .map(unescape_fish)
            .collect(),
    };
    commands.retain(|command| !command.trim().is_empty());
    commands
}

fn is_decimal(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn unmetafy_zsh(bytes: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut bytes = bytes.iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == 0x83
            && let Some(escaped) = bytes.next()
        {
            decoded.push(escaped ^ 0x20);
        } else {
            decoded.push(byte);
        }
    }
    decoded
}

fn zsh_commands<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut commands = Vec::new();
    let mut lines = lines;
    while let Some(line) = lines.next() {
        let mut line = zsh_command_body(line);
        let mut command = String::new();
        while let Some(prefix) = line.strip_suffix('\\') {
            let Some(next_line) = lines.next() else {
                break;
            };
            command.push_str(prefix);
            command.push('\n');
            line = next_line;
        }
        command.push_str(line);
        commands.push(command);
    }
    commands
}

fn zsh_command_body(line: &str) -> &str {
    let Some((start, rest)) = line
        .strip_prefix(": ")
        .and_then(|rest| rest.split_once(':'))
    else {
        return line;
    };
    match rest.split_once(';') {
        Some((elapsed, command)) if is_decimal(start) && is_decimal(elapsed) => command,
        _ => line,
    }
}

fn unescape_fish(line: &str) -> String {
    let mut command = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        let decoded = if ch == '\\' {
            match chars.peek() {
                Some('\\') => chars.next(),
                Some('n') => {
                    chars.next();
                    Some('\n')
                }
                _ => None,
            }
        } else {
            None
        };
        command.push(decoded.unwrap_or(ch));
    }
    command
}

/// Connection settings taken from one ssh command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshInvocation {
    /// User to connect as, when specified.
    pub user: Option<String>,
    /// Destination host name or address.
    pub address: String,
    /// None for the default port 22.
    pub port: Option<String>,
    /// First identity file specified, if any.
    pub key_file: Option<String>,
}

const SHELL_SPECIALS: &str = "`$|&;<>(){}*?#\n";
const FLAGS: &str = "46AaCfgKkMNnqsTtvXxYy";
const VALUE_OPTIONS: &str = "BbcDEeFIiJLlmOoPpQRSWw";
const UNSUPPORTED_OPTIONS: &str = "FJWGQVO";

fn shell_tokens(command: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut token = None;
    let mut quote = None;
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        if quote != Some('\'') && SHELL_SPECIALS.contains(ch) {
            return None;
        }
        match (quote, ch) {
            (None, ' ' | '\t') => {
                if let Some(token) = token.take() {
                    tokens.push(token);
                }
            }
            (None, '\'' | '"') => {
                quote = Some(ch);
                token.get_or_insert_with(String::new);
            }
            (Some(open), close) if open == close => quote = None,
            (None, '\\') => {
                let escaped = chars.next()?;
                if SHELL_SPECIALS.contains(escaped) {
                    return None;
                }
                token.get_or_insert_with(String::new).push(escaped);
            }
            (Some('"'), '\\') if matches!(chars.peek(), Some('"' | '\\')) => {
                token.get_or_insert_with(String::new).push(chars.next()?);
            }
            _ => token.get_or_insert_with(String::new).push(ch),
        }
    }
    if quote.is_some() {
        return None;
    }
    tokens.extend(token);
    Some(tokens)
}

#[derive(Default)]
struct ConnectionOptions<'a> {
    user: Option<&'a str>,
    address: Option<&'a str>,
    port: Option<&'a str>,
    key_file: Option<&'a str>,
}

impl<'a> ConnectionOptions<'a> {
    fn set_config(&mut self, directive: &'a str) -> Option<()> {
        let (key, value) = split_config_option(directive);
        let field = if key.eq_ignore_ascii_case("User") {
            &mut self.user
        } else if key.eq_ignore_ascii_case("HostName") {
            &mut self.address
        } else if key.eq_ignore_ascii_case("Port") {
            &mut self.port
        } else if key.eq_ignore_ascii_case("IdentityFile") {
            &mut self.key_file
        } else if key.eq_ignore_ascii_case("ProxyJump") || key.eq_ignore_ascii_case("ProxyCommand")
        {
            return None;
        } else {
            return Some(());
        };
        field.get_or_insert(value?);
        Some(())
    }
}

fn split_config_option(directive: &str) -> (&str, Option<&str>) {
    let directive = directive.trim_start();
    let Some((key, separator_and_value)) = directive
        .find(['=', ' ', '\t'])
        .map(|index| directive.split_at(index))
    else {
        return (directive, None);
    };
    let value = separator_and_value.trim_start();
    let value = value.strip_prefix('=').unwrap_or(value).trim();
    (key, (!value.is_empty()).then_some(value))
}

/// Parses a simple ssh command, returning None when its connection cannot be represented safely.
pub fn parse_ssh_command(command: &str) -> Option<SshInvocation> {
    let tokens = shell_tokens(command)?;
    let mut tokens = tokens.iter().map(String::as_str);
    if tokens.next()? != "ssh" {
        return None;
    }

    let mut options = ConnectionOptions::default();
    let destination = loop {
        let token = tokens.next()?;
        if token == "--" {
            break tokens.next()?;
        }
        let Some(cluster) = token
            .strip_prefix('-')
            .filter(|cluster| !cluster.is_empty())
        else {
            break token;
        };
        let mut letters = cluster.chars();
        while let Some(letter) = letters.next() {
            if UNSUPPORTED_OPTIONS.contains(letter) {
                return None;
            }
            if FLAGS.contains(letter) {
                continue;
            }
            if !VALUE_OPTIONS.contains(letter) {
                return None;
            }
            let value = if letters.as_str().is_empty() {
                tokens.next()?
            } else {
                letters.as_str()
            };
            match letter {
                'p' => {
                    options.port.get_or_insert(value);
                }
                'l' => {
                    options.user.get_or_insert(value);
                }
                'i' => {
                    options.key_file.get_or_insert(value);
                }
                'o' => options.set_config(value)?,
                _ => {}
            }
            break;
        }
    };

    let destination = parse_destination(destination)?;
    let address = options.address.unwrap_or(destination.address);
    let user = options.user.or(destination.user);
    let port = match options.port.or(destination.port) {
        Some(port) => Some(parse_port(port).ok()??),
        None => None,
    };
    validate_address(address).ok()?;
    if let Some(user) = user {
        validate_user(user).ok()?;
    }
    if let Some(key_file) = options.key_file {
        validate_key_file(key_file).ok()?;
    }
    Some(SshInvocation {
        user: user.map(str::to_owned),
        address: address.to_owned(),
        port: port.filter(|&port| port != 22).map(|port| port.to_string()),
        key_file: options.key_file.map(str::to_owned),
    })
}

struct Destination<'a> {
    user: Option<&'a str>,
    address: &'a str,
    port: Option<&'a str>,
}

fn parse_destination(target: &str) -> Option<Destination<'_>> {
    let uri = target.strip_prefix("ssh://");
    let authority = uri.map_or(target, |uri| {
        uri.split_once('/').map_or(uri, |(host, _)| host)
    });
    let (user, host) = match authority.rsplit_once('@') {
        Some((user, host)) => (Some(user), host),
        None => (None, authority),
    };
    let (address, port) = if let Some(bracketed) = host.strip_prefix('[') {
        let (address, suffix) = bracketed.split_once(']')?;
        let port = if suffix.is_empty() {
            None
        } else if uri.is_some() {
            Some(suffix.strip_prefix(':')?)
        } else {
            return None;
        };
        (address, port)
    } else if uri.is_some() {
        match host.split_once(':') {
            Some((address, port)) => (address, Some(port)),
            None => (host, None),
        }
    } else {
        (host, None)
    };
    Some(Destination {
        user,
        address,
        port,
    })
}

/// A connection found in history that is not saved yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Connection settings from the matching commands.
    pub invocation: SshInvocation,
    /// Number of history commands that resolve to this connection.
    pub count: usize,
}

/// A saved host used to skip destinations that are already configured.
#[derive(Debug, Clone, Copy)]
pub struct SavedHost<'a> {
    /// Alias used to select the saved host.
    pub alias: &'a str,
    /// Host name configured for the alias, if available.
    pub host_name: Option<&'a str>,
}

/// Upper bound on the number of suggestions.
pub const MAX_SUGGESTIONS: usize = 20;

/// Builds suggestions from commands given oldest first. The result is newest first.
pub fn suggest<'a>(
    commands: impl IntoIterator<Item = &'a str>,
    saved: &[SavedHost<'_>],
) -> Vec<Suggestion> {
    let saved: HashSet<_> = saved
        .iter()
        .flat_map(|host| [Some(host.alias), host.host_name])
        .flatten()
        .map(str::to_ascii_lowercase)
        .collect();
    let mut connections = HashMap::new();
    for (index, command) in commands.into_iter().enumerate() {
        let Some(invocation) = parse_ssh_command(command) else {
            continue;
        };
        let address = invocation.address.to_ascii_lowercase();
        if saved.contains(&address) {
            continue;
        }
        let key = (invocation.user.clone(), address, invocation.port.clone());
        match connections.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert((
                    index,
                    Suggestion {
                        invocation,
                        count: 1,
                    },
                ));
            }
            Entry::Occupied(mut entry) => {
                let (last, suggestion) = entry.get_mut();
                *last = index;
                suggestion.count += 1;
                suggestion.invocation.address = invocation.address;
                if invocation.key_file.is_some() {
                    suggestion.invocation.key_file = invocation.key_file;
                }
            }
        }
    }
    let mut ordered: Vec<_> = connections.into_values().collect();
    ordered.sort_unstable_by_key(|(index, _)| Reverse(*index));
    ordered
        .into_iter()
        .take(MAX_SUGGESTIONS)
        .map(|(_, suggestion)| suggestion)
        .collect()
}

/// Environment variable that replaces the default history files with one file.
pub const HISTORY_FILE_ENV: &str = "SSHTUI_HISTORY_FILE";
/// Environment variable that disables reading shell history when nonempty and not "0".
pub const DISABLE_ENV: &str = "SSHTUI_NO_HISTORY";
/// Only this many bytes at the end of each file are read.
pub const MAX_READ_BYTES: u64 = 256 * 1024;

/// A history file and how to parse it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySource {
    /// File to read.
    pub path: PathBuf,
    /// Format of the file contents.
    pub format: HistoryFormat,
}

/// Chooses fish or zsh based on the file name, defaulting to plain history.
pub fn format_for_path(path: &Path) -> HistoryFormat {
    let Some(name) = path.file_name().map(|name| name.to_string_lossy()) else {
        return HistoryFormat::Plain;
    };
    if name.contains("fish") {
        HistoryFormat::Fish
    } else if name.contains("zsh") {
        HistoryFormat::Zsh
    } else {
        HistoryFormat::Plain
    }
}

fn inferred_source(path: OsString) -> HistorySource {
    let path = PathBuf::from(path);
    let format = format_for_path(&path);
    HistorySource { path, format }
}

/// Lists candidate history files using only the supplied directories and environment lookup.
pub fn history_sources(
    home: Option<&Path>,
    config_dir: Option<&Path>,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Vec<HistorySource> {
    let env = |name| env(name).filter(|value| !value.is_empty());
    if env(DISABLE_ENV).is_some_and(|value| value != "0") {
        return Vec::new();
    }
    if let Some(path) = env(HISTORY_FILE_ENV) {
        return vec![inferred_source(path)];
    }

    let fish_dir = env("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".local/share")));
    let candidates = [
        env("HISTFILE").map(inferred_source),
        env("ZDOTDIR").map(|dir| HistorySource {
            path: PathBuf::from(dir).join(".zsh_history"),
            format: HistoryFormat::Zsh,
        }),
        home.map(|home| HistorySource {
            path: home.join(".zsh_history"),
            format: HistoryFormat::Zsh,
        }),
        home.map(|home| HistorySource {
            path: home.join(".bash_history"),
            format: HistoryFormat::Plain,
        }),
        fish_dir.map(|dir| HistorySource {
            path: dir.join("fish/fish_history"),
            format: HistoryFormat::Fish,
        }),
        home.map(|home| HistorySource {
            path: home.join(".history"),
            format: HistoryFormat::Plain,
        }),
        home.map(|home| HistorySource {
            path: home.join(".sh_history"),
            format: HistoryFormat::Plain,
        }),
        home.map(|home| HistorySource {
            path: home.join(".mksh_history"),
            format: HistoryFormat::Plain,
        }),
        config_dir.map(|dir| HistorySource {
            path: dir.join("nushell/history.txt"),
            format: HistoryFormat::Plain,
        }),
        home.map(|home| HistorySource {
            path: home.join(".local/share/powershell/PSReadLine/ConsoleHost_history.txt"),
            format: HistoryFormat::Plain,
        }),
        config_dir.map(|dir| HistorySource {
            path: dir.join("Microsoft/Windows/PowerShell/PSReadLine/ConsoleHost_history.txt"),
            format: HistoryFormat::Plain,
        }),
    ];
    let mut sources: Vec<HistorySource> = Vec::new();
    for source in candidates.into_iter().flatten() {
        if !sources.iter().any(|prior| prior.path == source.path) {
            sources.push(source);
        }
    }
    sources
}

/// Reads the tail of each source and returns only ssh commands, oldest first within each file.
pub fn read_ssh_commands(sources: &[HistorySource]) -> Vec<String> {
    sources
        .iter()
        .filter_map(read_source)
        .flatten()
        .filter(|command| {
            command
                .trim_start()
                .strip_prefix("ssh")
                .is_some_and(|rest| rest.starts_with([' ', '\t']))
        })
        .collect()
}

fn read_source(source: &HistorySource) -> Option<Vec<String>> {
    if !fs::metadata(&source.path).ok()?.is_file() {
        return None;
    }
    let mut file = File::open(&source.path).ok()?;
    let start = file.metadata().ok()?.len().saturating_sub(MAX_READ_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_READ_BYTES).read_to_end(&mut bytes).ok()?;
    let bytes = if start == 0 {
        bytes.as_slice()
    } else {
        let first_separator = bytes.iter().position(|&byte| {
            byte == b'\n' || (source.format == HistoryFormat::Plain && byte == 0)
        })?;
        &bytes[first_separator + 1..]
    };
    Some(parse_commands(source.format, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use HistoryFormat::{Fish, Plain, Zsh};
    use tempfile::tempdir;

    #[test]
    fn zsh_parses_plain_extended_and_continued_records() {
        let bytes = b"ssh plain\r\n: 123:0;ssh extended\r\n: 124:2;ssh multi\\\r\nfirst\\\n: 1:0;second\n: not-a-timestamp:0;ssh other\n: 125:0;\n";
        assert_eq!(
            parse_commands(Zsh, bytes),
            [
                "ssh plain",
                "ssh extended",
                "ssh multi\nfirst\n: 1:0;second",
                ": not-a-timestamp:0;ssh other",
            ]
        );
    }

    #[test]
    fn zsh_unmetafies_before_lossy_utf8_decoding() {
        assert_eq!(unmetafy_zsh(b"\x83\xa3"), [0x83]);
        assert_eq!(
            parse_commands(Zsh, b"ssh \xc3\x83\xa3\nssh \x83\xa3\nssh \x83"),
            ["ssh Ã", "ssh �", "ssh �"]
        );
    }

    #[test]
    fn plain_skips_only_numeric_timestamps_and_handles_crlf_and_invalid_utf8() {
        assert_eq!(
            parse_commands(Plain, b"#123456\r\nssh one\r\n\n  \r\n#\n#12x\nssh \xff\r"),
            ["ssh one", "#", "#12x", "ssh �"]
        );
    }

    #[test]
    fn plain_skips_tcsh_numeric_timestamps() {
        assert_eq!(
            parse_commands(Plain, b"#+1700000000\r\nssh one\n#+\n#+12x\n"),
            ["ssh one", "#+", "#+12x"]
        );
    }

    #[test]
    fn plain_splits_nul_records_and_strips_leading_control_characters() {
        assert_eq!(
            parse_commands(Plain, b"\x81\x01ssh one\0\t\x7fssh two\r\n\x1f\0ssh \xff"),
            ["ssh one", "ssh two", "ssh �"]
        );
    }

    #[test]
    fn fish_keeps_command_entries_and_decodes_escapes_once() {
        assert_eq!(
            parse_commands(
                Fish,
                b"- cmd: ssh one\\nnext\r\n  when: 123\r\n- cmd: ssh two\\\\n\\t\xff\n- cmd: \n"
            ),
            ["ssh one\nnext", r"ssh two\n\t�"]
        );
    }

    #[test]
    fn ssh_parses_connection_settings() {
        for (command, user, address, port, key_file) in [
            ("ssh host", None, "host", None, None),
            ("ssh user@host", Some("user"), "host", None, None),
            (
                "ssh -p 2222 user@host",
                Some("user"),
                "host",
                Some("2222"),
                None,
            ),
            ("ssh -p2222 host", None, "host", Some("2222"), None),
            (
                "ssh -vvA -i ~/.ssh/id_ed25519 host",
                None,
                "host",
                None,
                Some("~/.ssh/id_ed25519"),
            ),
            ("ssh -l root host", Some("root"), "host", None, None),
            ("ssh -l a b@host", Some("a"), "host", None, None),
            (
                "ssh -o Port=2200 -o User=deploy host",
                Some("deploy"),
                "host",
                Some("2200"),
                None,
            ),
            (
                r#"ssh -o "IdentityFile ~/.ssh/k" host"#,
                None,
                "host",
                None,
                Some("~/.ssh/k"),
            ),
            (
                "ssh -o HostName=10.0.0.5 alias",
                None,
                "10.0.0.5",
                None,
                None,
            ),
            ("ssh host uptime", None, "host", None, None),
            ("ssh -L 8080:localhost:80 host", None, "host", None, None),
            (
                "ssh ssh://user@host:2222",
                Some("user"),
                "host",
                Some("2222"),
                None,
            ),
            ("ssh ssh://[::1]:2222", None, "::1", Some("2222"), None),
            ("ssh -p 22 host", None, "host", None, None),
            ("ssh 'host'", None, "host", None, None),
            ("ssh -- host", None, "host", None, None),
            ("ssh ssh://a@b@[::1]/a@b", Some("a@b"), "::1", None, None),
            ("ssh [::1]", None, "::1", None, None),
            (
                "ssh -vp02222 host -o ProxyJump=remote-arg",
                None,
                "host",
                Some("2222"),
                None,
            ),
            (
                r#"ssh -i path\ with\ space h"o"'st'"#,
                None,
                "host",
                None,
                Some("path with space"),
            ),
            (
                " \tssh\t-i 'key;with$chars' host",
                None,
                "host",
                None,
                Some("key;with$chars"),
            ),
        ] {
            assert_eq!(
                parse_ssh_command(command),
                Some(SshInvocation {
                    user: user.map(str::to_owned),
                    address: address.to_owned(),
                    port: port.map(str::to_owned),
                    key_file: key_file.map(str::to_owned),
                }),
                "{command}"
            );
        }
    }

    #[test]
    fn ssh_prefers_the_first_explicit_value_for_each_setting() {
        let invocation = parse_ssh_command(
            r#"ssh -o "pOrT = 2200" -p2222 -l first -o user=second -o identityfile=a -i b -o "hostname real" -o hostname=ignored ssh://uri@alias:2022/path"#,
        )
        .unwrap();
        assert_eq!(invocation.user.as_deref(), Some("first"));
        assert_eq!(invocation.port.as_deref(), Some("2200"));
        assert_eq!(invocation.key_file.as_deref(), Some("a"));
        assert_eq!(invocation.address, "real");

        let invocation =
            parse_ssh_command("ssh -p022 -o Port=2222 -o User=first -l second -i a -i b host")
                .unwrap();
        assert_eq!(invocation.port, None);
        assert_eq!(invocation.user.as_deref(), Some("first"));
        assert_eq!(invocation.key_file.as_deref(), Some("a"));
    }

    #[test]
    fn tokenization_handles_literal_text_and_double_quote_escapes() {
        assert_eq!(
            shell_tokens(r#"ssh '' "a\"b\\c\d" '$HOME;*' one\ two"#).unwrap(),
            ["ssh", "", r#"a"b\c\d"#, "$HOME;*", "one two"]
        );
    }

    #[test]
    fn ssh_rejects_other_commands_shell_syntax_and_unrepresentable_connections() {
        for command in [
            "sudo ssh host",
            "VAR=x ssh host",
            "/usr/bin/ssh host",
            "ssh",
            "ssh -p",
            "ssh -J jump host",
            "ssh -F cfg host",
            "ssh -W host:22 jump",
            "ssh -Q key",
            "ssh -V",
            "ssh -O exit host",
            "ssh -o ProxyJump=j host",
            "ssh -o pRoXyCoMmAnD=nc host",
            "ssh -Z host",
            "ssh $HOST",
            r#"ssh "$HOST""#,
            "ssh host | tee log",
            "ssh host; ls",
            "ssh \x60cat h\x60",
            "ssh host\nid",
            "ssh \"host",
            "ssh 'host",
            "ssh host\\",
            "ssh -p 0 host",
            "ssh -p abc host",
            "ssh -p '' host",
            "ssh -i '-file' host",
            "ssh -l 'two users' host",
            "ssh -- -host",
            "ssh -G host",
            "ssh ssh://[::1",
            "ssh ssh://[::1]extra",
            "ssh ssh://host:",
            "scp a host:b",
            "sshfs host: /mnt",
        ] {
            assert_eq!(parse_ssh_command(command), None, "{command}");
        }
    }

    #[test]
    fn suggestions_group_by_connection_and_keep_latest_address_and_key() {
        let suggestions = suggest(
            [
                "ssh -i first alice@Host",
                "ssh -i second alice@host",
                "ssh alice@host -v",
                "ssh -p2200 alice@host",
                "ssh bob@host",
                "ssh -o Port=22 alice@HOST",
            ],
            &[],
        );
        assert_eq!(suggestions.len(), 3);
        assert_eq!(
            suggestions[0],
            Suggestion {
                invocation: SshInvocation {
                    user: Some("alice".into()),
                    address: "HOST".into(),
                    port: None,
                    key_file: Some("second".into()),
                },
                count: 4,
            }
        );
        assert_eq!(suggestions[1].invocation.user.as_deref(), Some("bob"));
        assert_eq!(suggestions[1].count, 1);
        assert_eq!(suggestions[2].invocation.port.as_deref(), Some("2200"));
        assert_eq!(suggestions[2].count, 1);
    }

    #[test]
    fn suggestions_skip_saved_addresses_and_limit_by_recency() {
        let saved = [
            SavedHost {
                alias: "work",
                host_name: Some("Db.Example"),
            },
            SavedHost {
                alias: "personal",
                host_name: None,
            },
        ];
        let commands: Vec<_> = (0..=MAX_SUGGESTIONS)
            .map(|i| format!("ssh host{i}"))
            .collect();
        let recent = suggest(
            commands.iter().map(String::as_str).chain([
                "ssh -p2022 user@WORK",
                "ssh db.example",
                "ssh PERSONAL",
                "ssh host0",
                "echo ignored",
            ]),
            &saved,
        );
        assert_eq!(recent.len(), MAX_SUGGESTIONS);
        assert_eq!(recent[0].invocation.address, "host0");
        assert_eq!(recent[0].count, 2);
        for (suggestion, i) in recent[1..].iter().zip((2..=MAX_SUGGESTIONS).rev()) {
            assert_eq!(suggestion.invocation.address, format!("host{i}"));
        }
    }

    fn source(path: impl Into<PathBuf>, format: HistoryFormat) -> HistorySource {
        HistorySource {
            path: path.into(),
            format,
        }
    }

    #[test]
    fn format_uses_only_the_file_name_with_fish_taking_precedence() {
        assert_eq!(format_for_path(Path::new("other/fish_history")), Fish);
        assert_eq!(format_for_path(Path::new("other/.zsh_history")), Zsh);
        assert_eq!(format_for_path(Path::new("fish/zsh_history")), Zsh);
        assert_eq!(format_for_path(Path::new("zsh/fish-zsh")), Fish);
        assert_eq!(format_for_path(Path::new("zsh/.bash_history")), Plain);
    }

    #[test]
    fn sources_honor_disable_and_a_single_override() {
        let home = Some(Path::new("home"));
        let config = Some(Path::new("config"));
        for (disable, override_file, formats) in [
            ("yes", "custom-fish", vec![]),
            ("0", "custom-fish", vec![source("custom-fish", Fish)]),
            ("", "custom-zsh", vec![source("custom-zsh", Zsh)]),
        ] {
            let env = |name: &str| match name {
                DISABLE_ENV => Some(disable.into()),
                HISTORY_FILE_ENV => Some(override_file.into()),
                _ => None,
            };
            assert_eq!(history_sources(home, config, &env), formats);
        }
    }

    #[test]
    fn sources_in_default_order_and_skip_unavailable_directories() {
        let empty = |name: &str| (name == HISTORY_FILE_ENV).then(OsString::new);
        let home = Path::new("home");
        let config = Path::new("config");
        let nushell = source(config.join("nushell/history.txt"), Plain);
        let powershell = source(
            config.join("Microsoft/Windows/PowerShell/PSReadLine/ConsoleHost_history.txt"),
            Plain,
        );
        assert_eq!(
            history_sources(Some(home), Some(config), &empty),
            [
                source(home.join(".zsh_history"), Zsh),
                source(home.join(".bash_history"), Plain),
                source(home.join(".local/share/fish/fish_history"), Fish),
                source(home.join(".history"), Plain),
                source(home.join(".sh_history"), Plain),
                source(home.join(".mksh_history"), Plain),
                nushell.clone(),
                source(
                    home.join(".local/share/powershell/PSReadLine/ConsoleHost_history.txt"),
                    Plain,
                ),
                powershell.clone(),
            ]
        );
        assert_eq!(
            history_sources(None, Some(config), &empty),
            [nushell, powershell]
        );
        assert!(history_sources(None, None, &empty).is_empty());
    }

    #[test]
    fn sources_use_shell_environment_and_remove_duplicate_paths() {
        let home = Path::new("home");
        let env = |name: &str| match name {
            "HISTFILE" => Some(home.join(".bash_history").into_os_string()),
            "ZDOTDIR" => Some("dotfiles".into()),
            "XDG_DATA_HOME" => Some("data".into()),
            _ => None,
        };
        let from_env = [
            source(home.join(".bash_history"), Plain),
            source("dotfiles/.zsh_history", Zsh),
            source("data/fish/fish_history", Fish),
        ];
        assert_eq!(history_sources(None, None, &env), from_env);
        assert_eq!(
            history_sources(Some(home), None, &env),
            [
                from_env[0].clone(),
                from_env[1].clone(),
                source(home.join(".zsh_history"), Zsh),
                from_env[2].clone(),
                source(home.join(".history"), Plain),
                source(home.join(".sh_history"), Plain),
                source(home.join(".mksh_history"), Plain),
                source(
                    home.join(".local/share/powershell/PSReadLine/ConsoleHost_history.txt"),
                    Plain,
                ),
            ]
        );
    }

    #[test]
    fn history_reads_only_ssh_commands_from_tcsh_and_ksh() {
        let temp = tempdir().unwrap();
        let tcsh = temp.path().join(".history");
        let ksh = temp.path().join(".sh_history");
        fs::write(&tcsh, b"#+1700000000\nssh tcsh-host\n#+1700000001\nls\n").unwrap();
        fs::write(&ksh, b"\x81\x01ssh ksh-first\0pwd\0\x7fssh ksh-last\0").unwrap();
        let sources = history_sources(Some(temp.path()), None, &|_| None);
        assert_eq!(
            read_ssh_commands(&sources),
            ["ssh tcsh-host", "ssh ksh-first", "ssh ksh-last"]
        );
    }

    #[test]
    fn history_discards_the_first_nul_delimited_fragment_of_a_bounded_tail() {
        let temp = tempdir().unwrap();
        let path = temp.path().join(".sh_history");
        let tail = b"ssh fragment\0ssh first\0ssh last\0";
        let mut bytes = vec![b'x'; MAX_READ_BYTES as usize - tail.len() + 1];
        bytes[0] = 0x81;
        bytes.extend_from_slice(tail);
        fs::write(&path, bytes).unwrap();
        assert_eq!(
            read_ssh_commands(&[source(path, Plain)]),
            ["ssh first", "ssh last"]
        );
    }

    #[test]
    fn history_reads_only_ssh_commands_in_source_order_without_changing_files() {
        let temp = tempdir().unwrap();
        let bash = temp.path().join("bash");
        let fish = temp.path().join("fish");
        let missing = temp.path().join("missing");
        let plain_bytes = b"#123\nssh first\nscp a b\nsshfs host\nssh\n  ssh\tsecond\n";
        let fish_bytes = b"- cmd: ssh third\n  when: 123\n- cmd: echo no\n";
        fs::write(&bash, plain_bytes).unwrap();
        fs::write(&fish, fish_bytes).unwrap();
        assert_eq!(
            read_ssh_commands(&[
                source(&missing, Zsh),
                source(&bash, Plain),
                source(temp.path(), Plain),
                source(&fish, Fish),
            ]),
            ["ssh first", "  ssh\tsecond", "ssh third"]
        );
        assert_eq!(fs::read(bash).unwrap(), plain_bytes);
        assert_eq!(fs::read(fish).unwrap(), fish_bytes);
        assert!(!missing.exists());
    }

    #[test]
    fn history_reads_the_bounded_tail_and_discards_the_first_fragment() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("history");
        let prefix = b"ssh too-old\nnot-a-command: ";
        let ending = b"\nssh tail-first\r\nssh tail-last";
        let mut bytes = prefix.to_vec();
        bytes.extend_from_slice(b"ssh fragment\n");
        bytes.resize(prefix.len() + MAX_READ_BYTES as usize - ending.len(), b'x');
        bytes.extend_from_slice(ending);
        fs::write(&path, &bytes).unwrap();
        assert_eq!(
            read_ssh_commands(&[source(&path, Plain)]),
            ["ssh tail-first", "ssh tail-last"]
        );
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}
