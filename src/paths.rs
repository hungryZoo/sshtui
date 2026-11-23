//! SSH directory resolution via `SSHTUI_SSH_DIR` and tilde expansion.

use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Environment variable that overrides the ssh directory (default: `~/.ssh`).
pub const SSH_DIR_ENV: &str = "SSHTUI_SSH_DIR";

/// An error determining the ssh directory.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathsError {
    /// Neither an override nor a home directory is available.
    #[error("cannot determine the ssh directory: no home directory found; set SSHTUI_SSH_DIR")]
    NoHome,
    /// The override could not be made absolute.
    #[error("invalid SSHTUI_SSH_DIR value {0:?}: {1}")]
    InvalidOverride(OsString, String),
}

/// Resolved ssh directory and the user's home directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshPaths {
    home: Option<PathBuf>,
    ssh_dir: PathBuf,
    overridden: bool,
}

impl SshPaths {
    /// Resolves the ssh directory from an optional override and home without filesystem access.
    /// Relative overrides are resolved against `cwd`.
    pub fn resolve(
        override_dir: Option<&OsStr>,
        home: Option<&Path>,
        cwd: &Path,
    ) -> Result<Self, PathsError> {
        let (ssh_dir, overridden) = match override_dir.filter(|dir| !dir.is_empty()) {
            Some(dir) => {
                let path = Path::new(dir);
                let ssh_dir = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    cwd.join(path)
                };
                if !ssh_dir.is_absolute() {
                    return Err(PathsError::InvalidOverride(
                        dir.to_os_string(),
                        "path could not be made absolute using the current directory".into(),
                    ));
                }
                (ssh_dir, true)
            }
            None => (home.ok_or(PathsError::NoHome)?.join(".ssh"), false),
        };

        Ok(Self {
            home: home.map(Path::to_path_buf),
            ssh_dir,
            overridden,
        })
    }

    /// Reads the override and home from the environment, and the current directory if needed.
    pub fn from_env() -> Result<Self, PathsError> {
        let override_dir = env::var_os(SSH_DIR_ENV);
        let home = env::home_dir();
        let cwd = match override_dir.as_deref() {
            Some(dir) if !dir.is_empty() && !Path::new(dir).is_absolute() => env::current_dir()
                .map_err(|err| PathsError::InvalidOverride(dir.to_os_string(), err.to_string()))?,
            _ => PathBuf::new(),
        };
        Self::resolve(override_dir.as_deref(), home.as_deref(), &cwd)
    }

    /// Returns the resolved ssh directory.
    pub fn ssh_dir(&self) -> &Path {
        &self.ssh_dir
    }

    /// Returns the path to the config file in the ssh directory.
    pub fn config_file(&self) -> PathBuf {
        self.ssh_dir.join("config")
    }

    /// Returns true when the directory came from `SSHTUI_SSH_DIR`.
    pub fn is_overridden(&self) -> bool {
        self.overridden
    }

    /// Returns the user's home directory when available.
    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    /// Expands a leading tilde path component to the home directory when available.
    pub fn expand_tilde(&self, input: &str) -> PathBuf {
        self.home().map_or_else(
            || PathBuf::from(input),
            |home| expand_tilde_from(home, input),
        )
    }

    /// Abbreviates the home prefix component-wise, using `/` in the abbreviated path.
    /// Paths are converted to strings lossily.
    pub fn abbreviate_home(&self, path: &Path) -> String {
        let Some(rest) = self.home().and_then(|home| path.strip_prefix(home).ok()) else {
            return path.to_string_lossy().into_owned();
        };

        let mut abbreviated = String::from("~");
        for component in rest.components() {
            abbreviated.push('/');
            abbreviated.push_str(&component.as_os_str().to_string_lossy());
        }
        abbreviated
    }
}

#[cfg(not(windows))]
fn expand_tilde_from(home: &Path, input: &str) -> PathBuf {
    let path = Path::new(input);
    match path.strip_prefix("~") {
        Ok(rest) if rest.as_os_str().is_empty() => home.to_path_buf(),
        Ok(rest) => home.join(rest),
        Err(_) => path.to_path_buf(),
    }
}

#[cfg(windows)]
fn expand_tilde_from(home: &Path, input: &str) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let separators = ['/', '\\'];
    let Some(rest) = input.strip_prefix('~') else {
        return PathBuf::from(input);
    };
    if !rest.is_empty() && !rest.starts_with(separators) {
        return PathBuf::from(input);
    }
    let rest = rest.trim_start_matches(separators);
    if rest.is_empty() {
        return home.to_path_buf();
    }

    let home_units: Vec<_> = home
        .as_os_str()
        .encode_wide()
        .map(|unit| {
            if unit == u16::from(b'/') {
                u16::from(b'\\')
            } else {
                unit
            }
        })
        .collect();
    let home = PathBuf::from(OsString::from_wide(&home_units));
    // Append the suffix without treating it as a drive path or resolving dot components.
    let mut expanded = home.join("").into_os_string();
    expanded.push(rest.replace('/', "\\"));
    PathBuf::from(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[cfg(windows)]
    const ROOT: &str = "C:/";
    #[cfg(not(windows))]
    const ROOT: &str = "/";

    #[test]
    fn default_ssh_directory_and_config() {
        let home = Path::new(ROOT).join("home/al");
        let paths = SshPaths::resolve(None, Some(&home), Path::new(ROOT)).unwrap();
        assert_eq!(paths.ssh_dir(), home.join(".ssh"));
        assert_eq!(paths.config_file(), home.join(".ssh/config"));
        assert_eq!(paths.home(), Some(home.as_path()));
        assert!(!paths.is_overridden());
    }

    #[test]
    fn empty_override_is_unset() {
        let cwd = Path::new(ROOT);
        let home = cwd.join("home/al");
        assert_eq!(
            SshPaths::resolve(Some(OsStr::new("")), Some(&home), cwd),
            SshPaths::resolve(None, Some(&home), cwd)
        );
        assert_eq!(
            SshPaths::resolve(Some(OsStr::new("")), None, cwd),
            Err(PathsError::NoHome)
        );
    }

    #[test]
    fn absolute_override_retains_home() {
        let cwd = Path::new(ROOT);
        let home = cwd.join("home/al");
        let override_dir = cwd.join("custom");
        let paths = SshPaths::resolve(Some(override_dir.as_os_str()), Some(&home), cwd).unwrap();
        assert_eq!(paths.ssh_dir(), override_dir);
        assert!(paths.is_overridden());
        assert_eq!(paths.home(), Some(home.as_path()));
        assert_eq!(paths.expand_tilde("~/key"), home.join("key"));
    }

    #[test]
    fn relative_override_uses_current_directory() {
        let cwd = Path::new(ROOT).join("work");
        let paths = SshPaths::resolve(Some(OsStr::new("custom/../ssh")), None, &cwd).unwrap();
        assert_eq!(paths.ssh_dir(), cwd.join("custom/../ssh"));
        assert!(paths.ssh_dir().is_absolute());
        assert!(paths.is_overridden());
    }

    #[test]
    fn relative_override_requires_an_absolute_result() {
        let dir = OsStr::new("custom");
        assert!(matches!(
            SshPaths::resolve(Some(dir), None, Path::new("work")),
            Err(PathsError::InvalidOverride(value, _)) if value == dir
        ));
    }

    #[test]
    fn no_home_or_override_is_an_error() {
        assert_eq!(
            SshPaths::resolve(None, None, Path::new(ROOT)),
            Err(PathsError::NoHome)
        );
    }

    #[test]
    fn override_works_without_a_home() {
        let cwd = Path::new(ROOT);
        let override_dir = cwd.join("custom");
        let paths = SshPaths::resolve(Some(override_dir.as_os_str()), None, cwd).unwrap();
        assert_eq!(paths.ssh_dir(), override_dir);
        assert_eq!(paths.home(), None);
        assert_eq!(paths.expand_tilde("~/a"), Path::new("~/a"));
        assert_eq!(
            paths.abbreviate_home(&override_dir),
            override_dir.to_string_lossy()
        );
    }

    #[test]
    fn expands_only_a_leading_tilde_component() {
        let home = Path::new(ROOT).join("home/al");
        let paths = SshPaths::resolve(None, Some(&home), Path::new(ROOT)).unwrap();
        assert_eq!(paths.expand_tilde("~"), home);
        assert_eq!(paths.expand_tilde("~/a/b"), home.join("a/b"));
        assert_eq!(paths.expand_tilde("~//a/b"), home.join("a/b"));
        for input in ["~user/x", "/abs", "rel/~/x", ""] {
            assert_eq!(paths.expand_tilde(input), Path::new(input), "{input:?}");
        }
    }

    #[test]
    fn expands_tilde_with_native_separators() {
        let home = tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        assert_eq!(
            paths.expand_tilde("~/a/b").as_os_str(),
            home.path().join("a").join("b").as_os_str()
        );
    }

    #[cfg(windows)]
    #[test]
    fn expands_mixed_separators_and_keeps_dot_components() {
        let home = tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        for (input, suffix) in [
            (r"~\a\b", r"a\b"),
            (r"~/a\b", r"a\b"),
            ("~/a/../b", r"a\..\b"),
            ("~/a/./b/.", r"a\.\b\."),
        ] {
            let mut expected = home.path().join("").into_os_string();
            expected.push(suffix);
            assert_eq!(paths.expand_tilde(input).into_os_string(), expected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn keeps_backslashes_in_expanded_paths() {
        let home = tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(home.path()), home.path()).unwrap();
        assert_eq!(
            paths.expand_tilde(r"~/a\b").as_os_str(),
            home.path().join(r"a\b").as_os_str()
        );
    }

    #[cfg(windows)]
    #[test]
    fn expands_tilde_with_windows_separators() {
        let cwd = Path::new(ROOT);
        let home = cwd.join("home/al");
        let paths = SshPaths::resolve(None, Some(&home), cwd).unwrap();
        assert_eq!(paths.expand_tilde(r"~\a\b"), home.join("a/b"));
        assert_eq!(
            paths.expand_tilde("~/D:/key"),
            Path::new("C:/home/al/D:/key")
        );
    }

    #[cfg(windows)]
    #[test]
    fn expanded_abbreviation_round_trips_with_a_verbatim_home() {
        let home = Path::new(r"\\?\C:\home\al");
        let paths = SshPaths::resolve(None, Some(home), Path::new(ROOT)).unwrap();
        let path = home.join("a").join("b");
        assert_eq!(paths.abbreviate_home(&path), "~/a/b");
        assert_eq!(paths.expand_tilde("~/a/b"), path);
    }

    #[test]
    fn abbreviates_only_the_home_and_its_descendants() {
        let cwd = Path::new(ROOT);
        let home = cwd.join("home/al");
        let paths = SshPaths::resolve(None, Some(&home), cwd).unwrap();
        assert_eq!(paths.abbreviate_home(&home), "~");
        assert_eq!(paths.abbreviate_home(&home.join("a").join("b")), "~/a/b");
        for outside in [cwd.join("other/x"), cwd.join("home/alice/x")] {
            assert_eq!(paths.abbreviate_home(&outside), outside.to_string_lossy());
        }
    }

    #[test]
    fn expanded_abbreviation_round_trips() {
        let cwd = Path::new(ROOT);
        let home = cwd.join("home/al");
        let paths = SshPaths::resolve(None, Some(&home), cwd).unwrap();
        let path = home.join(".ssh/my key");
        assert_eq!(paths.expand_tilde(&paths.abbreviate_home(&path)), path);
    }
}
