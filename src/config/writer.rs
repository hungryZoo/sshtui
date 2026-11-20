//! Detection of external changes, backups, and replacement via temporary files.

use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions, Permissions};
use std::io::{self, ErrorKind, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

use tempfile::{Builder, NamedTempFile};

/// A configuration write that failed or would overwrite external changes.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The file no longer matches the bytes that were loaded (or appeared/disappeared).
    #[error("{path} was changed by another program; reload and try again")]
    Stale { path: PathBuf },
    /// A filesystem operation failed.
    #[error("cannot {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// The paths affected by a successful write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOutcome {
    /// The file written, with symlinks resolved when replacing an existing file.
    pub target: PathBuf,
    /// Backup of the previous contents; None when the file was newly created.
    pub backup: Option<PathBuf>,
}

/// Returns the backup name for `file_name`: `.<file_name>.sshtui.bak`.
pub fn backup_file_name(file_name: &OsStr) -> OsString {
    let mut name = OsString::from(".");
    name.push(file_name);
    name.push(".sshtui.bak");
    name
}

/// Replaces a file if its current bytes match `original`, keeping a backup.
pub fn replace_file(
    path: &Path,
    original: &[u8],
    updated: &[u8],
) -> Result<WriteOutcome, WriteError> {
    let target =
        fs::canonicalize(path).map_err(|source| existing_error("resolve", path, source))?;
    let current = fs::read(&target).map_err(|source| existing_error("read", path, source))?;
    if current != original {
        return Err(stale(path));
    }

    let file_name = target
        .file_name()
        .ok_or_else(|| missing_file_name(&target))?;
    let permissions = fs::metadata(&target)
        .map_err(|source| existing_error("read permissions for", path, source))?
        .permissions();
    let backup = target.with_file_name(backup_file_name(file_name));
    prepared_temp(&backup, original, Some(permissions.clone()))?
        .persist(&backup)
        .map_err(|error| io_error("save backup to", &backup, error.error))?;
    prepared_temp(&target, updated, Some(permissions))?
        .persist(&target)
        .map_err(|error| io_error("replace", &target, error.error))?;

    Ok(WriteOutcome {
        target,
        backup: Some(backup),
    })
}

/// Creates a file that must not exist yet, creating its parent directory if needed.
pub fn create_file(path: &Path, contents: &[u8]) -> Result<WriteOutcome, WriteError> {
    if path.file_name().is_none() {
        return Err(missing_file_name(path));
    }
    let parent = parent_dir(path);
    let mut directories = fs::DirBuilder::new();
    directories.recursive(true);
    #[cfg(unix)]
    directories.mode(0o700);
    directories
        .create(parent)
        .map_err(|source| io_error("create directory", parent, source))?;

    #[cfg(unix)]
    let permissions = Some(Permissions::from_mode(0o600));
    #[cfg(not(unix))]
    let permissions = None;

    prepared_temp(path, contents, permissions)?
        .persist_noclobber(path)
        .map_err(|error| {
            if error.error.kind() == ErrorKind::AlreadyExists || fs::symlink_metadata(path).is_ok()
            {
                stale(path)
            } else {
                io_error("create", path, error.error)
            }
        })?;

    Ok(WriteOutcome {
        target: path.to_path_buf(),
        backup: None,
    })
}

/// Returns whether the existing file can be opened for writing without modifying it.
pub fn is_writable(path: &Path) -> bool {
    OpenOptions::new().write(true).open(path).is_ok()
}

fn prepared_temp(
    destination: &Path,
    contents: &[u8],
    permissions: Option<Permissions>,
) -> Result<NamedTempFile, WriteError> {
    let mut temp = Builder::new()
        .prefix(".sshtui-")
        .tempfile_in(parent_dir(destination))
        .map_err(|source| io_error("create temporary file for", destination, source))?;
    let file = temp.as_file_mut();
    file.write_all(contents)
        .map_err(|source| io_error("write temporary file for", destination, source))?;
    if let Some(permissions) = permissions {
        file.set_permissions(permissions)
            .map_err(|source| io_error("set temporary permissions for", destination, source))?;
    }
    file.sync_all()
        .map_err(|source| io_error("sync temporary file for", destination, source))?;
    Ok(temp)
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

fn stale(path: &Path) -> WriteError {
    WriteError::Stale {
        path: path.to_path_buf(),
    }
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> WriteError {
    WriteError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

fn existing_error(action: &'static str, path: &Path, source: io::Error) -> WriteError {
    if source.kind() == ErrorKind::NotFound {
        stale(path)
    } else {
        io_error(action, path, source)
    }
}

fn missing_file_name(path: &Path) -> WriteError {
    io_error(
        "write",
        path,
        io::Error::new(ErrorKind::InvalidInput, "expected a file name"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(dir: &Path) -> Vec<OsString> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn replaces_existing_file_and_reports_the_canonical_paths() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        fs::write(&path, b"original").unwrap();

        let outcome = replace_file(&path, b"original", b"updated").unwrap();

        let target = fs::canonicalize(&path).unwrap();
        let backup = target.with_file_name(".config.sshtui.bak");
        assert_eq!(
            outcome,
            WriteOutcome {
                target,
                backup: Some(backup)
            }
        );
        assert_eq!(fs::read(path).unwrap(), b"updated");
        assert_eq!(names(temp.path()), [".config.sshtui.bak", "config"]);
    }

    #[test]
    fn updates_the_backup_to_the_immediately_previous_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        let backup = temp.path().join(".config.sshtui.bak");
        fs::write(&path, b"first").unwrap();

        replace_file(&path, b"first", b"second").unwrap();
        assert_eq!(fs::read(&backup).unwrap(), b"first");
        replace_file(&path, b"second", b"third").unwrap();
        assert_eq!(fs::read(&backup).unwrap(), b"second");
        assert_eq!(fs::read(path).unwrap(), b"third");
    }

    #[test]
    fn detects_external_changes_without_creating_any_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        fs::write(&path, b"original").unwrap();
        let original = fs::read(&path).unwrap();
        fs::write(&path, b"external").unwrap();

        assert!(matches!(
            replace_file(&path, &original, b"updated"),
            Err(WriteError::Stale { path: stale }) if stale == path
        ));
        assert_eq!(fs::read(path).unwrap(), b"external");
        assert_eq!(names(temp.path()), ["config"]);
    }

    #[test]
    fn detects_a_deleted_file_without_creating_any_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        fs::write(&path, b"original").unwrap();
        fs::remove_file(&path).unwrap();

        assert!(matches!(
            replace_file(&path, b"original", b"updated"),
            Err(WriteError::Stale { path: stale }) if stale == path
        ));
        assert!(names(temp.path()).is_empty());
    }

    #[test]
    fn cleans_up_when_a_backup_cannot_be_persisted() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        let backup = temp.path().join(".config.sshtui.bak");
        fs::write(&path, b"original").unwrap();
        fs::create_dir(&backup).unwrap();

        assert!(matches!(
            replace_file(&path, b"original", b"updated"),
            Err(WriteError::Io { .. })
        ));
        assert_eq!(fs::read(path).unwrap(), b"original");
        assert!(backup.is_dir());
        assert_eq!(names(temp.path()), [".config.sshtui.bak", "config"]);
    }

    #[test]
    fn preserves_crlf_and_non_utf8_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        let original = b"Host a\r\n  # \xff\r\n";
        let updated = b"Host b\r\n  # \xff\r\n";
        fs::write(&path, original).unwrap();

        let outcome = replace_file(&path, original, updated).unwrap();
        assert_eq!(fs::read(path).unwrap(), updated);
        assert_eq!(fs::read(outcome.backup.unwrap()).unwrap(), original);
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[cfg(unix)]
    #[test]
    fn replacement_and_backup_keep_the_original_permissions() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        fs::write(&path, b"original").unwrap();
        fs::set_permissions(&path, Permissions::from_mode(0o640)).unwrap();

        let outcome = replace_file(&path, b"original", b"updated").unwrap();
        assert_eq!(mode(&path), 0o640);
        assert_eq!(mode(&outcome.backup.unwrap()), 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn preserves_the_symlink_and_backs_up_in_the_real_directory() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        let target = real.join("config");
        let link = temp.path().join("link");
        fs::create_dir(&real).unwrap();
        fs::write(&target, b"original").unwrap();
        symlink("real/config", &link).unwrap();

        let outcome = replace_file(&link, b"original", b"updated").unwrap();
        assert_eq!(outcome.target, fs::canonicalize(&target).unwrap());
        assert_eq!(
            outcome.backup.unwrap(),
            fs::canonicalize(real.join(".config.sshtui.bak")).unwrap()
        );
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(target).unwrap(), b"updated");
        assert_eq!(
            fs::read(real.join(".config.sshtui.bak")).unwrap(),
            b"original"
        );
        assert_eq!(names(temp.path()), ["link", "real"]);
    }

    #[cfg(unix)]
    #[test]
    fn replaces_the_final_target_of_a_symlink_chain() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("config");
        let inner = temp.path().join("inner");
        let outer = temp.path().join("outer");
        fs::write(&target, b"original").unwrap();
        symlink("config", &inner).unwrap();
        symlink("inner", &outer).unwrap();

        let outcome = replace_file(&outer, b"original", b"updated").unwrap();
        assert_eq!(outcome.target, fs::canonicalize(&target).unwrap());
        assert_eq!(fs::read_link(inner).unwrap(), Path::new("config"));
        assert_eq!(fs::read_link(outer).unwrap(), Path::new("inner"));
        assert_eq!(fs::read(target).unwrap(), b"updated");
    }

    #[test]
    fn creates_the_file_and_missing_parent_directories() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("ssh");
        let nested = parent.join("conf.d");
        let path = nested.join("config");

        let outcome = create_file(&path, b"contents").unwrap();

        assert_eq!(
            outcome,
            WriteOutcome {
                target: path.clone(),
                backup: None
            }
        );
        assert_eq!(fs::read(&path).unwrap(), b"contents");
        assert_eq!(names(&nested), ["config"]);
        #[cfg(unix)]
        {
            assert_eq!(mode(&parent), 0o700);
            assert_eq!(mode(&nested), 0o700);
            assert_eq!(mode(&path), 0o600);
        }
    }

    #[test]
    fn creation_preserves_existing_files_and_leaves_no_temporary_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        fs::write(&path, b"existing").unwrap();

        assert!(matches!(
            create_file(&path, b"new"),
            Err(WriteError::Stale { path: stale }) if stale == path
        ));
        assert_eq!(fs::read(path).unwrap(), b"existing");
        assert_eq!(names(temp.path()), ["config"]);
    }

    #[test]
    fn checks_writability_without_changing_or_creating_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        let missing = temp.path().join("missing");
        fs::write(&path, b"original").unwrap();

        assert!(is_writable(&path));
        assert!(!is_writable(&missing));
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(names(temp.path()), ["config"]);
    }

    #[cfg(unix)]
    #[test]
    fn reports_read_only_files_as_unwritable_when_the_os_denies_writes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        fs::write(&path, b"original").unwrap();
        fs::set_permissions(&path, Permissions::from_mode(0o400)).unwrap();

        // Privileged processes may still open a mode-0400 file for writing.
        let can_open = OpenOptions::new().write(true).open(&path).is_ok();
        assert_eq!(is_writable(&path), can_open);
        assert_eq!(fs::read(path).unwrap(), b"original");
    }

    #[test]
    fn backup_names_have_the_required_hidden_prefix_and_suffix() {
        assert_eq!(backup_file_name(OsStr::new("config")), ".config.sshtui.bak");
        assert_eq!(
            backup_file_name(OsStr::new("web.conf")),
            ".web.conf.sshtui.bak"
        );
    }
}
