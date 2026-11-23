//! Preparation and preview of writes to the root config or an included file.

use std::path::PathBuf;

use crate::paths::SshPaths;
use crate::validate::{ValidationError, check_nickname_unique, validate_nickname};

use super::diff::{DiffHunk, diff_lines};
use super::edit::{
    EditError, HostEdit, NewHost, OptionChanges, add_host, apply_host_edit, delete_pattern,
};
use super::model::ParsedConfig;
use super::reader::{ConfigSet, HostEntry, LoadedFile};
use super::writer::{WriteError, WriteOutcome, create_file, replace_file};

/// Number of context lines in previews.
pub const DIFF_CONTEXT: usize = 2;

/// A requested change that cannot be planned from the loaded configuration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error(transparent)]
    Edit(#[from] EditError),
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// The entry does not match the loaded configuration any more.
    #[error("host {0:?} is no longer present in the loaded configuration; reload and try again")]
    HostNotFound(String),
}

/// A prepared change to one file, not yet written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingWrite {
    /// File to write as loaded, or the root config path when creating it.
    pub path: PathBuf,
    /// Bytes loaded from the file; None when the file does not exist yet.
    pub original: Option<Vec<u8>>,
    /// Bytes to write on commit.
    pub updated: Vec<u8>,
    /// Preview of the change with `DIFF_CONTEXT` lines of context.
    pub hunks: Vec<DiffHunk>,
}

impl PendingWrite {
    fn new(path: PathBuf, original: Option<&[u8]>, updated: Vec<u8>) -> Self {
        let hunks = diff_lines(original.unwrap_or_default(), &updated, DIFF_CONTEXT);
        Self {
            path,
            original: original.map(<[u8]>::to_vec),
            updated,
            hunks,
        }
    }

    fn for_loaded(file: &LoadedFile, updated: Vec<u8>) -> Self {
        Self::new(file.path.clone(), Some(file.parsed.bytes()), updated)
    }

    /// Plans replacing an existing file whose loaded bytes are `original`.
    pub fn replacement(path: PathBuf, original: &[u8], updated: Vec<u8>) -> Self {
        Self::new(path, Some(original), updated)
    }

    /// Returns whether the file already has exactly the bytes to be written.
    pub fn is_noop(&self) -> bool {
        self.original.as_deref() == Some(self.updated.as_slice())
    }

    /// Creates or replaces the file. A no-op returns the given path without accessing the filesystem.
    pub fn commit(&self) -> Result<WriteOutcome, WriteError> {
        if self.is_noop() {
            return Ok(WriteOutcome {
                target: self.path.clone(),
                backup: None,
            });
        }
        match &self.original {
            Some(original) => replace_file(&self.path, original, &self.updated),
            None => create_file(&self.path, &self.updated),
        }
    }
}

/// Plans adding a new host block to the root config (creating it when missing).
pub fn plan_add(
    paths: &SshPaths,
    set: &ConfigSet,
    host: &NewHost,
) -> Result<PendingWrite, PlanError> {
    validate_nickname(&host.name)?;
    check_nickname_unique(
        &host.name,
        set.hosts.iter().map(|entry| entry.name.as_str()),
    )?;

    let path = paths.config_file();
    if let Some(file) = set.files.iter().find(|file| file.path == path) {
        return Ok(PendingWrite::for_loaded(
            file,
            add_host(&file.parsed, host)?,
        ));
    }
    let updated = add_host(&ParsedConfig::parse(Vec::new()), host)?;
    Ok(PendingWrite::new(path, None, updated))
}

/// Plans renaming `entry` and/or changing the options of its block, in the file that defines it.
pub fn plan_edit(
    set: &ConfigSet,
    entry: &HostEntry,
    new_name: Option<&str>,
    changes: &OptionChanges,
) -> Result<PendingWrite, PlanError> {
    let file = entry_file(set, entry)?;
    if let Some(name) = new_name.filter(|&name| name != entry.name) {
        validate_nickname(name)?;
        let other_names = set.hosts.iter().filter(|other| {
            (other.file_index, other.block_index, other.pattern_index)
                != (entry.file_index, entry.block_index, entry.pattern_index)
        });
        check_nickname_unique(name, other_names.map(|other| other.name.as_str()))?;
    }

    let edit = HostEdit {
        block_index: entry.block_index,
        pattern_index: entry.pattern_index,
        new_name: new_name.map(str::to_owned),
        changes: changes.clone(),
    };
    let updated = apply_host_edit(&file.parsed, &edit)?;
    Ok(PendingWrite::for_loaded(file, updated))
}

/// Plans removing `entry` (the whole block when it is the only pattern).
pub fn plan_delete(set: &ConfigSet, entry: &HostEntry) -> Result<PendingWrite, PlanError> {
    let file = entry_file(set, entry)?;
    let updated = delete_pattern(&file.parsed, entry.block_index, entry.pattern_index)?;
    Ok(PendingWrite::for_loaded(file, updated))
}

fn entry_file<'a>(set: &'a ConfigSet, entry: &HostEntry) -> Result<&'a LoadedFile, PlanError> {
    set.files
        .get(entry.file_index)
        .filter(|file| {
            file.parsed
                .blocks()
                .get(entry.block_index)
                .and_then(|block| file.parsed.lines()[block.header].directive())
                .and_then(|header| header.args.get(entry.pattern_index))
                .is_some_and(|arg| arg.value == entry.name)
        })
        .ok_or_else(|| PlanError::HostNotFound(entry.name.clone()))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::Path;

    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::config::{Change, DiffLine, DiffTag, load};
    use crate::validate::Field;

    fn fixture() -> (TempDir, SshPaths) {
        let temp = tempdir().unwrap();
        let ssh = temp.path().join("ssh");
        let home = temp.path().join("home");
        let paths = SshPaths::resolve(Some(ssh.as_os_str()), Some(&home), temp.path()).unwrap();
        (temp, paths)
    }

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn host(name: &str) -> NewHost {
        NewHost {
            name: name.into(),
            host_name: Some(format!("{name}.example.com")),
            user: None,
            port: None,
            identity_file: None,
            password_authentication: None,
        }
    }

    fn host_names(set: &ConfigSet) -> Vec<&str> {
        set.hosts.iter().map(|entry| entry.name.as_str()).collect()
    }

    fn names(dir: &Path) -> Vec<OsString> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn constructs_a_replacement_with_the_standard_preview_context() {
        let path = PathBuf::from("known_hosts");
        let original = b"first\nsecond\nthird\nremoved\nfourth\nfifth\nsixth\n";
        let updated = b"first\nsecond\nthird\nfourth\nfifth\nsixth\n";
        let pending = PendingWrite::replacement(path.clone(), original, updated.to_vec());

        assert_eq!(pending.path, path);
        assert_eq!(pending.original.as_deref(), Some(original.as_slice()));
        assert_eq!(pending.updated, updated);
        assert_eq!(pending.hunks, diff_lines(original, updated, DIFF_CONTEXT));
    }

    #[test]
    fn plans_and_creates_a_missing_config() {
        let (_temp, paths) = fixture();
        let set = load(&paths).unwrap();
        let pending = plan_add(&paths, &set, &host("web")).unwrap();

        assert_eq!(pending.path, paths.config_file());
        assert_eq!(pending.original, None);
        assert!(!paths.ssh_dir().exists());
        assert_eq!(pending.hunks.len(), 1);
        assert_eq!(pending.hunks[0].lines.len(), 2);
        assert!(
            pending.hunks[0]
                .lines
                .iter()
                .all(|line| line.tag == DiffTag::Added)
        );
        assert_eq!(
            pending.commit().unwrap(),
            WriteOutcome {
                target: paths.config_file(),
                backup: None
            }
        );
        assert_eq!(fs::read(paths.config_file()).unwrap(), pending.updated);
        assert_eq!(host_names(&load(&paths).unwrap()), ["web"]);
    }

    #[test]
    fn adds_to_the_root_config_and_backs_it_up() {
        let (_temp, paths) = fixture();
        let root = paths.config_file();
        let original = b"Host db\n";
        write(&root, original);
        let set = load(&paths).unwrap();

        let pending = plan_add(&paths, &set, &host("web")).unwrap();
        assert_eq!(pending.original.as_deref(), Some(original.as_slice()));
        assert_eq!(fs::read(&root).unwrap(), original);
        let outcome = pending.commit().unwrap();
        assert_eq!(fs::read(outcome.backup.unwrap()).unwrap(), original);
        assert_eq!(fs::read(root).unwrap(), pending.updated);
        assert_eq!(host_names(&load(&paths).unwrap()), ["db", "web"]);
    }

    #[test]
    fn rejects_duplicate_nicknames_in_the_root_and_included_files() {
        let (_temp, paths) = fixture();
        write(&paths.config_file(), b"Include conf.d/*\nHost root\n");
        write(&paths.ssh_dir().join("conf.d/web.conf"), b"Host web\n");
        let set = load(&paths).unwrap();

        for name in ["root", "web"] {
            assert_eq!(
                plan_add(&paths, &set, &host(name)),
                Err(ValidationError::DuplicateNickname(name.into()).into())
            );
        }
    }

    #[test]
    fn edits_only_the_file_defining_an_included_host() {
        let (_temp, paths) = fixture();
        let root = paths.config_file();
        let included = paths.ssh_dir().join("conf.d/web.conf");
        let root_bytes = b"Include conf.d/*\nHost root\n";
        let included_bytes = b"Host web\n  User alice\n";
        write(&root, root_bytes);
        write(&included, included_bytes);
        let set = load(&paths).unwrap();
        let entry = set.hosts.iter().find(|entry| entry.name == "web").unwrap();

        let pending = plan_edit(&set, entry, Some("cdn"), &OptionChanges::default()).unwrap();
        assert_eq!(pending.path, included);
        assert_eq!(fs::read(&included).unwrap(), included_bytes);
        let outcome = pending.commit().unwrap();

        assert_eq!(fs::read(root).unwrap(), root_bytes);
        assert_eq!(fs::read(&included).unwrap(), b"Host cdn\n  User alice\n");
        assert_eq!(fs::read(outcome.backup.unwrap()).unwrap(), included_bytes);
        assert_eq!(host_names(&load(&paths).unwrap()), ["cdn", "root"]);
    }

    #[test]
    fn rejects_renaming_to_another_hosts_name_and_allows_keeping_the_same_name() {
        let (_temp, paths) = fixture();
        write(&paths.config_file(), b"Include other.conf\nHost db\n");
        write(&paths.ssh_dir().join("other.conf"), b"Host web\n");
        let set = load(&paths).unwrap();
        let entry = set.hosts.iter().find(|entry| entry.name == "db").unwrap();
        let changes = OptionChanges::default();

        assert_eq!(
            plan_edit(&set, entry, Some("web"), &changes),
            Err(ValidationError::DuplicateNickname("web".into()).into())
        );
        assert!(
            plan_edit(&set, entry, Some("db"), &changes)
                .unwrap()
                .is_noop()
        );
    }

    #[test]
    fn validates_new_names_before_checking_for_duplicates() {
        let (_temp, paths) = fixture();
        write(&paths.config_file(), b"Host db '-invalid'\n");
        let set = load(&paths).unwrap();
        let error = PlanError::Validation(ValidationError::LeadingDash {
            field: Field::Nickname,
        });

        assert_eq!(
            plan_add(&paths, &set, &host("-invalid")),
            Err(error.clone())
        );
        assert_eq!(
            plan_edit(
                &set,
                &set.hosts[0],
                Some("-invalid"),
                &OptionChanges::default()
            ),
            Err(error)
        );
    }

    #[test]
    fn renames_or_deletes_only_the_selected_pattern_in_a_shared_block() {
        let (_temp, paths) = fixture();
        let original = b"Host db cache\n  User alice\n";
        write(&paths.config_file(), original);
        let set = load(&paths).unwrap();
        let entry = &set.hosts[1];
        assert_eq!(entry.name, "cache");

        let renamed = plan_edit(&set, entry, Some("redis"), &OptionChanges::default()).unwrap();
        assert_eq!(renamed.updated, b"Host db redis\n  User alice\n");
        renamed.commit().unwrap();
        let reloaded = load(&paths).unwrap();
        assert_eq!(host_names(&reloaded), ["db", "redis"]);

        let deleted = plan_delete(&reloaded, &reloaded.hosts[1]).unwrap();
        assert_eq!(deleted.updated, b"Host db\n  User alice\n");
        deleted.commit().unwrap();
        assert_eq!(host_names(&load(&paths).unwrap()), ["db"]);
    }

    #[test]
    fn deletes_the_entire_block_for_a_single_pattern() {
        let (_temp, paths) = fixture();
        let root = paths.config_file();
        write(&root, b"Host db\n  User alice\nHost web\n  User bob\n");
        let set = load(&paths).unwrap();
        let pending = plan_delete(&set, &set.hosts[0]).unwrap();

        assert_eq!(pending.updated, b"Host web\n  User bob\n");
        pending.commit().unwrap();
        assert_eq!(fs::read(root).unwrap(), b"Host web\n  User bob\n");
        assert_eq!(host_names(&load(&paths).unwrap()), ["web"]);
    }

    #[test]
    fn plans_against_loaded_bytes_and_rejects_external_changes_on_commit() {
        let (_temp, paths) = fixture();
        let root = paths.config_file();
        let original = b"Host db\n";
        let external = b"Host external\n";
        write(&root, original);
        let set = load(&paths).unwrap();
        fs::write(&root, external).unwrap();

        let pending =
            plan_edit(&set, &set.hosts[0], Some("web"), &OptionChanges::default()).unwrap();
        assert_eq!(pending.original.as_deref(), Some(original.as_slice()));
        assert!(matches!(pending.commit(), Err(WriteError::Stale { path }) if path == root));
        assert_eq!(fs::read(root).unwrap(), external);
        assert_eq!(names(paths.ssh_dir()), ["config"]);
    }

    #[test]
    fn rejects_entries_with_missing_indices_or_mismatched_names() {
        let (_temp, paths) = fixture();
        write(&paths.config_file(), b"Host db\n");
        let set = load(&paths).unwrap();
        let entry = &set.hosts[0];
        let invalid_entries = [
            HostEntry {
                file_index: set.files.len(),
                ..entry.clone()
            },
            HostEntry {
                block_index: 1,
                ..entry.clone()
            },
            HostEntry {
                pattern_index: 1,
                ..entry.clone()
            },
            HostEntry {
                name: "other".into(),
                ..entry.clone()
            },
        ];

        for invalid in invalid_entries {
            let error = PlanError::HostNotFound(invalid.name.clone());
            assert_eq!(
                plan_edit(&set, &invalid, None, &OptionChanges::default()),
                Err(error.clone())
            );
            assert_eq!(plan_delete(&set, &invalid), Err(error));
        }
    }

    #[test]
    fn no_op_commit_leaves_the_file_and_directory_unchanged() {
        let (_temp, paths) = fixture();
        let root = paths.config_file();
        let original = b"Host db\n";
        write(&root, original);
        let set = load(&paths).unwrap();
        let modified = fs::metadata(&root).unwrap().modified().unwrap();
        let before = names(paths.ssh_dir());

        let pending =
            plan_edit(&set, &set.hosts[0], Some("db"), &OptionChanges::default()).unwrap();
        assert!(pending.is_noop());
        assert!(pending.hunks.is_empty());
        let expected = WriteOutcome {
            target: root.clone(),
            backup: None,
        };
        assert_eq!(pending.commit().unwrap(), expected);
        assert_eq!(names(paths.ssh_dir()), before);
        assert_eq!(fs::metadata(&root).unwrap().modified().unwrap(), modified);
        assert_eq!(fs::read(&root).unwrap(), original);

        fs::remove_file(&root).unwrap();
        assert_eq!(pending.commit().unwrap(), expected);
        assert!(names(paths.ssh_dir()).is_empty());
    }

    #[test]
    fn previews_an_option_change_as_one_removal_and_one_addition() {
        let (_temp, paths) = fixture();
        write(
            &paths.config_file(),
            b"# header\n# note\nHost db\n  User alice\n  Port 22\n  ForwardAgent no\n# footer\n",
        );
        let set = load(&paths).unwrap();
        let changes = OptionChanges {
            user: Change::Set("bob".into()),
            ..OptionChanges::default()
        };

        let pending = plan_edit(&set, &set.hosts[0], None, &changes).unwrap();
        assert_eq!(pending.hunks.len(), 1);
        let lines = &pending.hunks[0].lines;
        assert_eq!(lines.len(), 2 * DIFF_CONTEXT + 2);
        assert_eq!(lines.first().unwrap().text, "# note");
        assert_eq!(lines.last().unwrap().text, "  ForwardAgent no");
        assert_eq!(
            lines[DIFF_CONTEXT..DIFF_CONTEXT + 2],
            [
                DiffLine {
                    tag: DiffTag::Removed,
                    old_line: Some(4),
                    new_line: None,
                    text: "  User alice".into(),
                },
                DiffLine {
                    tag: DiffTag::Added,
                    old_line: None,
                    new_line: Some(4),
                    text: "  User bob".into(),
                },
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn adds_a_host_through_a_symlink_to_the_root_config() {
        use std::os::unix::fs::symlink;

        let (temp, paths) = fixture();
        let root = paths.config_file();
        let target = temp.path().join("real/config");
        write(&target, b"Host db\n");
        fs::create_dir(paths.ssh_dir()).unwrap();
        symlink("../real/config", &root).unwrap();
        let set = load(&paths).unwrap();

        let pending = plan_add(&paths, &set, &host("web")).unwrap();
        assert_eq!(pending.path, root);
        assert_eq!(
            pending.commit().unwrap().target,
            fs::canonicalize(&target).unwrap()
        );
        assert!(fs::symlink_metadata(root).unwrap().file_type().is_symlink());
        assert_eq!(fs::read(target).unwrap(), pending.updated);
        assert_eq!(host_names(&load(&paths).unwrap()), ["db", "web"]);
    }
}
