//! Lookup and removal planning for entries in the per-user known_hosts file.

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::config::plan::PendingWrite;
use crate::paths::SshPaths;
use crate::ssh::effective::EffectiveConfig;

pub mod hash;

/// File name of the per-user host key database inside the ssh directory.
pub const FILE_NAME: &str = "known_hosts";

const DEFAULT_PORT: u16 = 22;

/// Returns the known_hosts path inside the injected ssh directory.
pub fn file_path(paths: &SshPaths) -> PathBuf {
    paths.ssh_dir().join(FILE_NAME)
}

/// Returns the name ssh looks up for a connection.
pub fn lookup_name(host_key_alias: Option<&str>, host_name: &str, port: u16) -> String {
    if let Some(alias) = host_key_alias.filter(|alias| !alias.is_empty()) {
        return alias.to_ascii_lowercase();
    }

    let host_name = host_name.to_ascii_lowercase();
    if port == DEFAULT_PORT {
        host_name
    } else {
        format!("[{host_name}]:{port}")
    }
}

/// Returns the lookup name for an alias from its effective settings.
pub fn lookup_name_for(alias: &str, effective: &EffectiveConfig) -> String {
    let port = effective
        .port
        .as_deref()
        .and_then(|port| port.parse::<u16>().ok())
        .filter(|&port| port != 0)
        .unwrap_or(DEFAULT_PORT);
    lookup_name(
        effective.host_key_alias.as_deref(),
        effective.host_name.as_deref().unwrap_or(alias),
        port,
    )
}

/// Returns whether a known_hosts line is a plain or hashed entry for exactly `name`.
/// Matching is more conservative than `ssh-keygen -R`: it skips marker lines and
/// ignores wildcard and negated patterns when looking for an exact match.
pub fn line_matches(mut line: &[u8], name: &str) -> bool {
    while let [rest @ .., b'\r' | b'\n'] = line {
        line = rest;
    }
    while let [b' ' | b'\t', rest @ ..] = line {
        line = rest;
    }
    if name.is_empty() || matches!(line, [] | [b'#' | b'@', ..]) {
        return false;
    }

    let mut fields = line.splitn(2, is_field_separator);
    let (Some(hosts), Some(key_fields)) = (fields.next(), fields.next()) else {
        return false;
    };
    if key_fields.iter().all(is_field_separator) {
        return false;
    }

    let name = name.as_bytes();
    if hosts.starts_with(b"|") {
        return hashed_name_matches(hosts, name);
    }
    hosts.split(|&byte| byte == b',').any(|pattern| {
        pattern.eq_ignore_ascii_case(name)
            && !pattern.starts_with(b"!")
            && !pattern.contains(&b'*')
            && !pattern.contains(&b'?')
    })
}

fn is_field_separator(&byte: &u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

fn hashed_name_matches(hosts: &[u8], name: &[u8]) -> bool {
    let Some(fields) = hosts.strip_prefix(b"|1|") else {
        return false;
    };
    let mut fields = fields.splitn(2, |&byte| byte == b'|');
    let salt = fields.next().and_then(hash::base64_decode);
    let digest = fields.next().and_then(hash::base64_decode);
    let (Some(salt), Some(digest)) = (salt, digest) else {
        return false;
    };
    digest.len() == 20 && digest.as_slice() == hash::hmac_sha1(&salt, name)
}

/// Result of removing the entries for one name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    /// File contents without the matching lines.
    pub updated: Vec<u8>,
    /// 1-based numbers of the removed lines, ascending.
    pub removed_lines: Vec<usize>,
}

/// Removes every line that `line_matches` for `name`, preserving all other bytes.
pub fn remove_entries(contents: &[u8], name: &str) -> Removal {
    let mut updated = Vec::with_capacity(contents.len());
    let mut removed_lines = Vec::new();
    for (index, line) in contents.split_inclusive(|&byte| byte == b'\n').enumerate() {
        if line_matches(line, name) {
            removed_lines.push(index + 1);
        } else {
            updated.extend_from_slice(line);
        }
    }
    Removal {
        updated,
        removed_lines,
    }
}

/// A planned removal awaiting confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetPlan {
    /// Lookup name whose entries will be removed.
    pub name: String,
    /// Number of lines that will be removed.
    pub removed: usize,
    /// The file replacement to commit.
    pub pending: PendingWrite,
}

/// Errors from reading the known_hosts file.
#[derive(Debug, thiserror::Error)]
pub enum KnownHostsError {
    /// The known_hosts file could not be read.
    #[error("cannot read {}: {source}", path.display())]
    Read {
        /// Path that could not be read.
        path: PathBuf,
        /// Filesystem error from reading the file.
        source: io::Error,
    },
}

/// Reads known_hosts and plans removing the entries for `name`.
/// Returns `Ok(None)` when the file does not exist or nothing matches.
pub fn plan_forget(paths: &SshPaths, name: &str) -> Result<Option<ForgetPlan>, KnownHostsError> {
    let path = file_path(paths);
    let contents = match fs::read(&path) {
        Ok(contents) => contents,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(KnownHostsError::Read { path, source }),
    };
    let Removal {
        updated,
        removed_lines,
    } = remove_entries(&contents, name);
    if removed_lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(ForgetPlan {
        name: name.to_owned(),
        removed: removed_lines.len(),
        pending: PendingWrite::replacement(path, &contents, updated),
    }))
}

#[cfg(test)]
mod tests {
    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::config::writer::WriteError;

    macro_rules! entry {
        ($host:literal) => {
            concat!(
                $host,
                " ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEk9zFM25++kJaRZ0WiPq2Rf/WXTnx0PsWWfN4crdP6X\n"
            )
        };
    }

    const SAMPLE: &str = concat!(
        "# comment\n",
        entry!("example.com,192.0.2.1"),
        entry!("*.example.com"),
        entry!("@cert-authority *.example.com"),
        entry!("EXAMPLE.com"),
        entry!("other.example.com"),
        entry!("!example.com,*"),
        entry!("|1|jN+dgNbNb3ppckN470BGDfcIluM=|BBVTRb++39vYeyXY3Bx9uWLpByY="),
        entry!("|1|jBdJaGrc1+QnnZl01Cq2lS9csU0=|41IYMBZM4Dz9DCfcmHdB3apXY6k="),
    );
    const WITHOUT_EXAMPLE: &str = concat!(
        "# comment\n",
        entry!("*.example.com"),
        entry!("@cert-authority *.example.com"),
        entry!("other.example.com"),
        entry!("!example.com,*"),
        entry!("|1|jBdJaGrc1+QnnZl01Cq2lS9csU0=|41IYMBZM4Dz9DCfcmHdB3apXY6k="),
    );

    #[test]
    fn removes_exact_plain_and_hashed_entries() {
        let removal = remove_entries(SAMPLE.as_bytes(), "example.com");
        assert_eq!(removal.removed_lines, [2, 5, 8]);
        assert_eq!(removal.updated, WITHOUT_EXAMPLE.as_bytes());

        assert_eq!(
            remove_entries(SAMPLE.as_bytes(), "[10.0.0.5]:2222").removed_lines,
            [9]
        );
        assert_eq!(
            remove_entries(SAMPLE.as_bytes(), "192.0.2.1").removed_lines,
            [2]
        );
    }

    #[test]
    fn preserves_entries_when_only_a_wildcard_or_a_different_port_matches() {
        for name in ["a.example.com", "10.0.0.5"] {
            let removal = remove_entries(SAMPLE.as_bytes(), name);
            assert!(removal.removed_lines.is_empty());
            assert_eq!(removal.updated, SAMPLE.as_bytes());
        }
    }

    #[test]
    fn builds_lookup_names() {
        for (alias, host, port, expected) in [
            (None, "Example.COM", 22, "example.com"),
            (None, "10.0.0.5", 2222, "[10.0.0.5]:2222"),
            (Some("My-Alias"), "x", 2222, "my-alias"),
            (Some(""), "h", 22, "h"),
            (None, "::1", 22, "::1"),
            (None, "::1", 2222, "[::1]:2222"),
        ] {
            assert_eq!(lookup_name(alias, host, port), expected);
        }
    }

    #[test]
    fn builds_lookup_names_from_effective_settings() {
        let mut effective = EffectiveConfig::default();
        assert_eq!(lookup_name_for("Web", &effective), "web");

        effective.host_name = Some("Example.COM".into());
        assert_eq!(lookup_name_for("Web", &effective), "example.com");
        for port in ["abc", "0", "65536"] {
            effective.port = Some(port.into());
            assert_eq!(lookup_name_for("Web", &effective), "example.com");
        }

        effective.port = Some("2222".into());
        assert_eq!(lookup_name_for("Web", &effective), "[example.com]:2222");
        effective.host_key_alias = Some("My-Alias".into());
        assert_eq!(lookup_name_for("Web", &effective), "my-alias");
    }

    #[test]
    fn matches_host_fields_with_spaces_tabs_or_non_utf8_bytes_elsewhere() {
        for line in [
            b"  \tExample.COM ssh-ed25519 key\r\n".as_slice(),
            b"example.com\tssh-ed25519\tkey",
            b"example.com ssh-ed25519 \xff",
            b"\xff,example.com ssh-ed25519 key",
        ] {
            assert!(line_matches(line, "example.com"));
        }
    }

    #[test]
    fn rejects_lines_without_an_exact_unmarked_host_entry() {
        for line in [
            b"".as_slice(),
            b" \t\r\n",
            b" \t# example.com key",
            b"@cert-authority example.com ssh-ed25519 key",
            b"\t@revoked example.com ssh-ed25519 key",
            b"example.com",
            b"example.com \t\r\n",
            b"exa?ple.com ssh-ed25519 key",
            b"\xffexample.com ssh-ed25519 key",
        ] {
            assert!(!line_matches(line, "example.com"));
        }
        assert!(!line_matches(b",example.com ssh-ed25519 key", ""));
        for pattern in ["*.example.com", "exampl?.com", "!example.com"] {
            assert!(!line_matches(format!("{pattern} key").as_bytes(), pattern));
        }
    }

    #[test]
    fn rejects_malformed_or_unsupported_hashes() {
        for line in [
            b"|1|jN+dgNbNb3ppckN470BGDfcIluM= key".as_slice(),
            b"|1|????|BBVTRb++39vYeyXY3Bx9uWLpByY= key",
            b"|1|jN+dgNbNb3ppckN470BGDfcIluM=|Zm9v key",
            b"|1|jN+dgNbNb3ppckN470BGDfcIluM=|BBVTRb++39vYeyXY3Bx9uWLpByY=| key",
            b"|2|jN+dgNbNb3ppckN470BGDfcIluM=|BBVTRb++39vYeyXY3Bx9uWLpByY= key",
        ] {
            assert!(!line_matches(line, "example.com"));
        }
    }

    #[test]
    fn preserves_remaining_bytes_and_handles_unterminated_lines() {
        for (contents, expected, lines) in [
            (
                b"# \xff\r\nweb key\r\nother key\r\n".as_slice(),
                b"# \xff\r\nother key\r\n".as_slice(),
                [2].as_slice(),
            ),
            (b"# comment\nweb key", b"# comment\n", &[2]),
            (
                b"# comment\nweb key\nother key",
                b"# comment\nother key",
                &[2],
            ),
            (b"web key\nWEB key", b"", &[1, 2]),
        ] {
            let removal = remove_entries(contents, "web");
            assert_eq!(removal.updated, expected);
            assert_eq!(removal.removed_lines, lines);
        }
    }

    fn fixture() -> (TempDir, SshPaths) {
        let temp = tempdir().unwrap();
        let ssh = temp.path().join("ssh");
        let paths =
            SshPaths::resolve(Some(ssh.as_os_str()), Some(temp.path()), temp.path()).unwrap();
        fs::create_dir(paths.ssh_dir()).unwrap();
        (temp, paths)
    }

    #[test]
    fn skips_missing_files_and_files_without_a_match() {
        let (_temp, paths) = fixture();
        let path = file_path(&paths);
        assert_eq!(path, paths.ssh_dir().join("known_hosts"));
        assert!(plan_forget(&paths, "example.com").unwrap().is_none());
        assert!(!path.exists());

        fs::write(&path, SAMPLE).unwrap();
        assert!(plan_forget(&paths, "missing.com").unwrap().is_none());
        assert_eq!(fs::read(path).unwrap(), SAMPLE.as_bytes());
    }

    #[test]
    fn plans_a_removal_and_backs_up_the_original_on_commit() {
        let (_temp, paths) = fixture();
        let path = file_path(&paths);
        fs::write(&path, SAMPLE).unwrap();
        let plan = plan_forget(&paths, "example.com").unwrap().unwrap();
        assert_eq!(plan.name, "example.com");
        assert_eq!(plan.removed, 3);
        assert_eq!(plan.pending.path, path);
        assert_eq!(plan.pending.original.as_deref(), Some(SAMPLE.as_bytes()));
        assert_eq!(plan.pending.updated, WITHOUT_EXAMPLE.as_bytes());
        assert_eq!(fs::read(&path).unwrap(), SAMPLE.as_bytes());

        let outcome = plan.pending.commit().unwrap();
        let backup = outcome.backup.unwrap();
        assert_eq!(backup.file_name().unwrap(), ".known_hosts.sshtui.bak");
        assert_eq!(fs::read(backup).unwrap(), SAMPLE.as_bytes());
        assert_eq!(fs::read(path).unwrap(), WITHOUT_EXAMPLE.as_bytes());
    }

    #[test]
    fn rejects_changes_made_after_planning() {
        let (_temp, paths) = fixture();
        let path = file_path(&paths);
        fs::write(&path, SAMPLE).unwrap();
        let plan = plan_forget(&paths, "example.com").unwrap().unwrap();
        fs::write(&path, WITHOUT_EXAMPLE).unwrap();

        assert!(matches!(
            plan.pending.commit(),
            Err(WriteError::Stale { path: changed }) if changed == path
        ));
        assert_eq!(fs::read(&path).unwrap(), WITHOUT_EXAMPLE.as_bytes());
        assert!(!paths.ssh_dir().join(".known_hosts.sshtui.bak").exists());
    }

    #[test]
    fn reports_a_read_error_when_the_path_is_a_directory() {
        let (_temp, paths) = fixture();
        let path = file_path(&paths);
        fs::create_dir(&path).unwrap();
        assert!(matches!(
            plan_forget(&paths, "example.com"),
            Err(KnownHostsError::Read { path: failed, .. }) if failed == path
        ));
    }
}
