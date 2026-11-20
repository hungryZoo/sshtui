//! Private key discovery in the top level of the SSH directory.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

/// Type and comment taken from a `.pub` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKeyInfo {
    /// Key type, e.g. `ssh-ed25519`.
    pub key_type: String,
    /// Comment (rest of the line after the base64 blob), if any.
    pub comment: Option<String>,
}

/// A private key file found directly inside the SSH directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    /// Path to the directory entry as discovered.
    pub path: PathBuf,
    /// File name (lossy UTF-8).
    pub file_name: String,
    /// Information from `<file_name>.pub` when that file exists and parses.
    pub public: Option<PublicKeyInfo>,
}

impl KeyInfo {
    /// Formats a one-line label with the file name and available public key information.
    pub fn label(&self) -> String {
        match &self.public {
            Some(public) => match &public.comment {
                Some(comment) => format!("{} ({}, {comment})", self.file_name, public.key_type),
                None => format!("{} ({})", self.file_name, public.key_type),
            },
            None => self.file_name.clone(),
        }
    }
}

/// Checks the header of a private key, allowing trailing CR, LF, spaces, and tabs.
pub fn is_private_key_first_line(line: &[u8]) -> bool {
    let end = line
        .iter()
        .rposition(|byte| !matches!(byte, b'\r' | b'\n' | b' ' | b'\t'));
    line.starts_with(b"-----BEGIN")
        && end.is_some_and(|end| line[..=end].ends_with(b"PRIVATE KEY-----"))
}

/// Parses a public key's type and optional comment, requiring a second field for the blob.
pub fn parse_public_key_line(line: &str) -> Option<PublicKeyInfo> {
    let (key_type, rest) = line
        .lines()
        .next()?
        .trim()
        .split_once(char::is_whitespace)?;
    if !key_type
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'@' | b'_'))
    {
        return None;
    }

    let comment = rest
        .trim_start()
        .split_once(char::is_whitespace)
        .map(|(_, comment)| comment.trim_start().to_owned());
    Some(PublicKeyInfo {
        key_type: key_type.to_owned(),
        comment,
    })
}

/// Lists private keys directly inside `ssh_dir`, sorted by file name.
/// A missing directory yields an empty list. Other directory read errors are returned.
pub fn discover_keys(ssh_dir: &Path) -> io::Result<Vec<KeyInfo>> {
    let entries = match fs::read_dir(ssh_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let mut keys = Vec::new();
    for entry in entries {
        let file_name = entry?.file_name();
        if file_name.as_encoded_bytes().ends_with(b".pub") {
            continue;
        }
        let path = ssh_dir.join(&file_name);
        let Some(line) = read_first_line(&path, 256) else {
            continue;
        };
        if is_private_key_first_line(&line) {
            let mut public_name = file_name.clone();
            public_name.push(".pub");
            keys.push(KeyInfo {
                path,
                file_name: file_name.to_string_lossy().into_owned(),
                public: read_public_key(&ssh_dir.join(public_name)),
            });
        }
    }
    keys.sort_unstable_by(|left, right| {
        left.file_name
            .cmp(&right.file_name)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(keys)
}

fn read_public_key(path: &Path) -> Option<PublicKeyInfo> {
    let line = read_first_line(path, 16_384)?;
    if is_private_key_first_line(&line) {
        return None;
    }
    parse_public_key_line(std::str::from_utf8(&line).ok()?)
}

fn read_first_line(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if metadata.permissions().mode() & 0o777 == 0 {
            return None;
        }
    }

    let mut reader = BufReader::new(File::open(path).ok()?.take(limit));
    let mut line = Vec::new();
    reader.read_until(b'\n', &mut line).ok()?;
    Some(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const PRIVATE_KEY: &[u8] = b"-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n";

    #[test]
    fn recognizes_private_headers_and_trailing_whitespace() {
        for line in [
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            "-----BEGIN RSA PRIVATE KEY-----",
            "-----BEGIN EC PRIVATE KEY-----",
            "-----BEGIN ENCRYPTED PRIVATE KEY-----",
            "-----BEGIN PRIVATE KEY-----",
            "-----BEGIN PRIVATE KEY-----\n",
            "-----BEGIN PRIVATE KEY-----\r\n",
            "-----BEGIN PRIVATE KEY----- \t \r\n",
        ] {
            assert!(is_private_key_first_line(line.as_bytes()));
        }
    }

    #[test]
    fn rejects_other_headers_and_invalid_boundaries() {
        for line in [
            "-----BEGIN PUBLIC KEY-----",
            "-----BEGIN CERTIFICATE-----",
            "ssh-ed25519 AAAA...",
            "",
            "-----BEGIN",
            " -----BEGIN OPENSSH PRIVATE KEY-----",
            "x-----BEGIN OPENSSH PRIVATE KEY-----",
            "PuTTY-User-Key-File-3: ssh-ed25519",
            "-----BEGIN OPENSSH PRIVATE KEY----- extra",
            "-----BEGIN OPENSSH PRIVATE KEY-----\x0c",
        ] {
            assert!(!is_private_key_first_line(line.as_bytes()));
        }
    }

    #[test]
    fn parses_public_key_type_and_comment() {
        let public = parse_public_key_line("ssh-ed25519 AAAA user@host").unwrap();
        assert_eq!(public.key_type, "ssh-ed25519");
        assert_eq!(public.comment.as_deref(), Some("user@host"));
    }

    #[test]
    fn preserves_spaces_in_public_key_comment() {
        let public = parse_public_key_line("ssh-rsa AAAA my laptop key").unwrap();
        assert_eq!(public.comment.as_deref(), Some("my laptop key"));
        let public = parse_public_key_line(" ssh-rsa\tAAAA  my  laptop\tkey \r\n").unwrap();
        assert_eq!(public.comment.as_deref(), Some("my  laptop\tkey"));
    }

    #[test]
    fn parses_public_key_without_comment_and_with_crlf() {
        for line in ["ssh-rsa AAAA", "ssh-rsa AAAA \r\n"] {
            let public = parse_public_key_line(line).unwrap();
            assert_eq!(public.key_type, "ssh-rsa");
            assert_eq!(public.comment, None);
        }
    }

    #[test]
    fn parses_security_key_type() {
        let public = parse_public_key_line("sk-ssh-ed25519@openssh.com AAAA yubikey").unwrap();
        assert_eq!(public.key_type, "sk-ssh-ed25519@openssh.com");
        assert_eq!(public.comment.as_deref(), Some("yubikey"));
    }

    #[test]
    fn rejects_missing_public_key_fields_and_invalid_types() {
        for line in ["", " \t\r\n", "ssh-ed25519  ", "ssh/rsa AAAA", "ssh-é AAAA"] {
            assert_eq!(parse_public_key_line(line), None);
        }
    }

    #[test]
    fn discovers_sorted_keys_with_optional_public_info() {
        let dir = tempdir().unwrap();
        let ssh_dir = dir.path().join(".");
        for (name, contents) in [
            ("id_rsa", PRIVATE_KEY),
            ("id_ed25519", PRIVATE_KEY),
            ("id_ed25519.pub", b"ssh-ed25519 AAAA user@host\r\n\xff"),
            ("config", b"Host example\n"),
            ("known_hosts", b"example ssh-ed25519 AAAA\n"),
            ("authorized_keys", b"ssh-ed25519 AAAA\n"),
        ] {
            fs::write(ssh_dir.join(name), contents).unwrap();
        }

        let keys = discover_keys(&ssh_dir).unwrap();
        assert_eq!(
            keys,
            [
                KeyInfo {
                    path: ssh_dir.join("id_ed25519"),
                    file_name: "id_ed25519".into(),
                    public: Some(PublicKeyInfo {
                        key_type: "ssh-ed25519".into(),
                        comment: Some("user@host".into()),
                    }),
                },
                KeyInfo {
                    path: ssh_dir.join("id_rsa"),
                    file_name: "id_rsa".into(),
                    public: None,
                },
            ]
        );
        assert_eq!(
            keys[0].path.as_os_str(),
            ssh_dir.join("id_ed25519").as_os_str()
        );
    }

    #[test]
    fn excludes_public_key_files() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("id_ed25519.pub"), b"ssh-ed25519 AAAA\n").unwrap();
        assert!(discover_keys(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn skips_subdirectories_and_their_keys() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("id_dir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("id_nested"), PRIVATE_KEY).unwrap();
        assert!(discover_keys(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn accepts_private_header_with_non_utf8_body() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("id_binary"),
            b"-----BEGIN OPENSSH PRIVATE KEY-----\n\xff\xfe",
        )
        .unwrap();
        let keys = discover_keys(dir.path()).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].file_name, "id_binary");
    }

    #[test]
    fn checks_only_the_first_256_bytes_of_a_private_key() {
        let dir = tempdir().unwrap();
        let oversized_line = format!("-----BEGIN{}", "A".repeat(1000));
        fs::write(dir.path().join("id_oversized"), &oversized_line).unwrap();
        fs::write(
            dir.path().join("id_late_header_end"),
            format!("{oversized_line} PRIVATE KEY-----\n"),
        )
        .unwrap();
        fs::write(
            dir.path().join("id_header_at_limit"),
            format!("-----BEGIN{}PRIVATE KEY-----AAAA", " ".repeat(230)),
        )
        .unwrap();
        let keys = discover_keys(dir.path()).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].file_name, "id_header_at_limit");
    }

    #[test]
    fn excludes_empty_files() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("id_empty"), b"").unwrap();
        assert!(discover_keys(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn returns_empty_for_missing_directory_and_propagates_other_errors() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("missing");
        assert!(discover_keys(&missing).unwrap().is_empty());
        let file = dir.path().join("file");
        fs::write(&file, b"").unwrap();
        assert!(discover_keys(&file).is_err());
    }

    #[test]
    fn keeps_private_keys_with_unusable_public_files() {
        let dir = tempdir().unwrap();
        let key = dir.path().join("id_key");
        let public = dir.path().join("id_key.pub");
        fs::write(&key, PRIVATE_KEY).unwrap();
        for contents in [b"".as_slice(), b"ssh-ed25519 AAAA \xff\n", PRIVATE_KEY] {
            fs::write(&public, contents).unwrap();
            let keys = discover_keys(dir.path()).unwrap();
            assert_eq!(keys.len(), 1);
            assert_eq!(keys[0].path, key);
            assert_eq!(keys[0].public, None);
        }
    }

    #[test]
    fn checks_only_the_first_16384_bytes_of_a_public_key() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("id_key"), PRIVATE_KEY).unwrap();
        let prefix = "ssh-ed25519 AAAA ";
        let comment = "x".repeat(16_384 - prefix.len());
        fs::write(
            dir.path().join("id_key.pub"),
            format!("{prefix}{comment}ignored\n"),
        )
        .unwrap();
        let keys = discover_keys(dir.path()).unwrap();
        let public = keys[0].public.as_ref().unwrap();
        assert_eq!(public.comment.as_deref(), Some(comment.as_str()));
    }

    #[cfg(unix)]
    #[test]
    fn follows_regular_file_symlinks_and_skips_inaccessible_files() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let dir = tempdir().unwrap();
        let target_dir = dir.path().join("targets");
        fs::create_dir(&target_dir).unwrap();
        let target = target_dir.join("key");
        let link = dir.path().join("id_link");
        let inaccessible = dir.path().join("id_inaccessible");
        fs::write(&target, PRIVATE_KEY).unwrap();
        symlink(&target, &link).unwrap();
        symlink(&target_dir, dir.path().join("id_dir_link")).unwrap();
        symlink(target_dir.join("missing"), dir.path().join("id_broken")).unwrap();
        fs::write(&inaccessible, PRIVATE_KEY).unwrap();
        fs::set_permissions(&inaccessible, fs::Permissions::from_mode(0o000)).unwrap();

        let keys = discover_keys(dir.path()).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].path, link);
    }

    #[test]
    fn formats_all_three_labels() {
        let mut key = KeyInfo {
            path: PathBuf::from("id_x"),
            file_name: "id_x".into(),
            public: None,
        };
        assert_eq!(key.label(), "id_x");

        key.file_name = "id_rsa".into();
        key.public = parse_public_key_line("ssh-rsa AAAA");
        assert_eq!(key.label(), "id_rsa (ssh-rsa)");

        key.file_name = "id_ed25519".into();
        key.public = parse_public_key_line("ssh-ed25519 AAAA user@host");
        assert_eq!(key.label(), "id_ed25519 (ssh-ed25519, user@host)");
    }
}
