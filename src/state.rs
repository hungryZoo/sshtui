//! Persistence for the last connected host alias.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

const MAX_READ_BYTES: u64 = 256;

/// Returns the last-host file path under the per-user state or local data directory.
pub fn default_state_file() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .map(|dir| dir.join("sshtui").join("last-host"))
}

/// Reads the last connected alias. Returns None when missing, unreadable, empty or malformed.
/// Uses only the first line within the first 256 bytes.
pub fn read_last_host(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(MAX_READ_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    let first_line = bytes.split(|&byte| byte == b'\n').next()?;
    let alias = std::str::from_utf8(first_line).ok()?.trim();
    if alias.is_empty() || alias.chars().any(char::is_control) {
        return None;
    }
    Some(alias.to_owned())
}

/// Records the last connected alias, creating parent directories as needed.
/// Replaces the previous state after the new alias has been written.
pub fn write_last_host(path: &Path, alias: &str) -> io::Result<()> {
    if alias.trim().is_empty() || alias.chars().any(char::is_control) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid host alias",
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temp = NamedTempFile::new_in(parent)?;
    writeln!(temp, "{alias}")?;
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_parent_directories_and_overwrites_the_previous_alias() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("state").join("sshtui").join("last-host");
        write_last_host(&path, "server-long").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"server-long\n");
        assert_eq!(read_last_host(&path).as_deref(), Some("server-long"));

        write_last_host(&path, "서버").unwrap();
        assert_eq!(fs::read(&path).unwrap(), "서버\n".as_bytes());
        assert_eq!(read_last_host(&path).as_deref(), Some("서버"));
    }

    #[test]
    fn ignores_missing_unreadable_and_malformed_state() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("last-host");
        assert_eq!(read_last_host(&path), None);
        assert_eq!(read_last_host(temp.path()), None);
        for bytes in [
            b"".as_slice(),
            b"  \nvalid",
            b"bad\talias",
            b"bad\0alias",
            b"\xff\nvalid",
        ] {
            fs::write(&path, bytes).unwrap();
            assert_eq!(read_last_host(&path), None);
        }
    }

    #[test]
    fn reads_only_the_first_line_within_the_byte_limit() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("last-host");
        let mut bytes = b" \tweb-01 \r\n".to_vec();
        bytes.extend([0xff; 300]);
        fs::write(&path, bytes).unwrap();
        assert_eq!(read_last_host(&path).as_deref(), Some("web-01"));

        let bytes = format!("{}é\n", "a".repeat(MAX_READ_BYTES as usize - 1));
        fs::write(&path, bytes).unwrap();
        assert_eq!(read_last_host(&path), None);
    }

    #[test]
    fn rejects_bad_aliases_before_writing() {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("sshtui");
        let path = parent.join("last-host");
        for alias in [
            "",
            " \u{2003} ",
            "\tbad",
            "two\nlines",
            "nul\0byte",
            "bad\u{7f}",
        ] {
            assert_eq!(
                write_last_host(&path, alias).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        assert!(!parent.exists());
    }
}
