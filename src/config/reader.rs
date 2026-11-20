//! Expansion of `Include` directives and collection of `Host` entries.

use std::collections::HashSet;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use glob::{MatchOptions, glob_with};

use crate::paths::SshPaths;

use super::model::ParsedConfig;

const MAX_INCLUDE_DEPTH: usize = 16;
const INCLUDE_MATCH_OPTIONS: MatchOptions = MatchOptions {
    case_sensitive: true,
    require_literal_separator: true,
    require_literal_leading_dot: true,
};

/// A concrete host alias found on a `Host` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry {
    /// The alias (one pattern of the Host line).
    pub name: String,
    /// File that defines it.
    pub source: PathBuf,
    /// Index into `ConfigSet::files`.
    pub file_index: usize,
    /// Index into that file's `ParsedConfig::blocks()`.
    pub block_index: usize,
    /// Index into the header directive's `args`.
    pub pattern_index: usize,
    /// 1-based line number of the Host line.
    pub line_number: usize,
    /// Other patterns on the same Host line, in order, including wildcard and negated patterns.
    pub siblings: Vec<String>,
}

/// A parsed file and its path as encountered during loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedFile {
    /// The root or glob match path before canonicalization.
    pub path: PathBuf,
    /// The parsed contents of the file.
    pub parsed: ParsedConfig,
}

/// A non-fatal issue encountered while expanding an Include directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadWarning {
    /// An included file exists but could not be read.
    Unreadable { path: PathBuf, error: String },
    /// The Include argument uses `${VAR}` or `%` tokens, which are not expanded.
    UnsupportedInclude {
        file: PathBuf,
        line_number: usize,
        pattern: String,
    },
    /// The Include argument is not a valid glob pattern.
    InvalidGlob {
        file: PathBuf,
        line_number: usize,
        pattern: String,
    },
    /// Include nesting deeper than 16 levels.
    DepthExceeded { path: PathBuf },
}

/// An error loading the root configuration file.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// The root configuration could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// The files, host aliases, and warnings from a configuration load.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigSet {
    /// Root config first, then included files in discovery order. Each file appears once.
    pub files: Vec<LoadedFile>,
    /// Listable hosts in the order encountered during Include expansion.
    pub hosts: Vec<HostEntry>,
    /// Issues encountered while loading includes.
    pub warnings: Vec<ReadWarning>,
}

/// Returns whether the pattern contains no `*` or `?` and does not start with `!`.
pub fn is_listable_pattern(pattern: &str) -> bool {
    !pattern.starts_with('!') && !pattern.contains(['*', '?'])
}

/// Resolves an Include argument against the home or SSH directory as appropriate.
/// Returns `None` if the argument contains `${` or `%`.
pub fn resolve_include_pattern(paths: &SshPaths, arg: &str) -> Option<PathBuf> {
    if arg.contains("${") || arg.contains('%') {
        return None;
    }

    let path = Path::new(arg);
    Some(if path.starts_with("~") {
        paths.expand_tilde(arg)
    } else if path.is_absolute() {
        path.to_path_buf()
    } else {
        paths.ssh_dir().join(path)
    })
}

/// Loads the root configuration and recursively expands its Include directives.
pub fn load(paths: &SshPaths) -> Result<ConfigSet, ReadError> {
    let path = paths.config_file();
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(ConfigSet::default()),
        Err(source) => return Err(ReadError::Io { path, source }),
    };
    let root = fs::canonicalize(&path).map_err(|source| ReadError::Io {
        path: path.clone(),
        source,
    })?;
    let mut reader = ConfigReader {
        paths,
        seen: HashSet::from([root]),
        config: ConfigSet::default(),
    };
    reader.visit(path, bytes, 0);
    Ok(reader.config)
}

struct ConfigReader<'a> {
    paths: &'a SshPaths,
    seen: HashSet<PathBuf>,
    config: ConfigSet,
}

impl ConfigReader<'_> {
    fn visit(&mut self, path: PathBuf, bytes: Vec<u8>, depth: usize) {
        let parsed = ParsedConfig::parse(bytes);
        let line_count = parsed.lines().len();
        let file_index = self.config.files.len();
        self.config.files.push(LoadedFile { path, parsed });

        for line_index in 0..line_count {
            let file = &self.config.files[file_index];
            let Some(directive) = file.parsed.lines()[line_index].directive() else {
                continue;
            };
            let line_number = line_index + 1;
            if directive.is("Host")
                && let Some(block_index) = file.parsed.block_of_line(line_index)
            {
                let args = &directive.args;
                self.config.hosts.extend(
                    args.iter()
                        .enumerate()
                        .filter(|(_, arg)| is_listable_pattern(&arg.value))
                        .map(|(pattern_index, arg)| HostEntry {
                            name: arg.value.clone(),
                            source: file.path.clone(),
                            file_index,
                            block_index,
                            pattern_index,
                            line_number,
                            siblings: args
                                .iter()
                                .enumerate()
                                .filter(|(index, _)| *index != pattern_index)
                                .map(|(_, sibling)| sibling.value.clone())
                                .collect(),
                        }),
                );
            } else if directive.is("Include") && !directive.args.is_empty() {
                // Copy the arguments before recursion can grow the files vector.
                let source = file.path.clone();
                let args: Vec<_> = directive.args.iter().map(|arg| arg.value.clone()).collect();
                for arg in args {
                    self.include(&source, line_number, &arg, depth + 1);
                }
            }
        }
    }

    fn include(&mut self, file: &Path, line_number: usize, arg: &str, depth: usize) {
        let Some(pattern) = resolve_include_pattern(self.paths, arg) else {
            self.config.warnings.push(ReadWarning::UnsupportedInclude {
                file: file.to_path_buf(),
                line_number,
                pattern: arg.to_owned(),
            });
            return;
        };
        let Ok(matches) = glob_with(&pattern.to_string_lossy(), INCLUDE_MATCH_OPTIONS) else {
            self.config.warnings.push(ReadWarning::InvalidGlob {
                file: file.to_path_buf(),
                line_number,
                pattern: arg.to_owned(),
            });
            return;
        };

        let mut paths = Vec::new();
        for result in matches {
            match result {
                Ok(path) => paths.push(path),
                Err(error) if error.error().kind() != ErrorKind::NotFound => {
                    self.warn_unreadable(error.path().to_path_buf(), error.error());
                }
                Err(_) => {}
            }
        }
        paths.sort_unstable_by(|left, right| left.as_os_str().cmp(right.as_os_str()));
        for path in paths {
            if let Err(error) = self.visit_include(&path, depth) {
                self.warn_unreadable(path, &error);
            }
        }
    }

    fn visit_include(&mut self, path: &Path, depth: usize) -> io::Result<()> {
        let metadata = match fs::metadata(path) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            result => result?,
        };
        if !metadata.is_file() {
            return Ok(());
        }
        if depth > MAX_INCLUDE_DEPTH {
            self.config.warnings.push(ReadWarning::DepthExceeded {
                path: path.to_path_buf(),
            });
        } else if self.seen.insert(fs::canonicalize(path)?) {
            let bytes = fs::read(path)?;
            self.visit(path.to_path_buf(), bytes, depth);
        }
        Ok(())
    }

    fn warn_unreadable(&mut self, path: PathBuf, error: &io::Error) {
        self.config.warnings.push(ReadWarning::Unreadable {
            path,
            error: error.to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{TempDir, tempdir};

    fn fixture() -> (TempDir, SshPaths) {
        let temp = tempdir().unwrap();
        let ssh_dir = temp.path().join("ssh");
        let home = temp.path().join("home");
        let paths = SshPaths::resolve(Some(ssh_dir.as_os_str()), Some(&home), temp.path()).unwrap();
        (temp, paths)
    }

    fn write(path: &Path, contents: impl AsRef<[u8]>) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn host_names(config: &ConfigSet) -> Vec<&str> {
        config.hosts.iter().map(|host| host.name.as_str()).collect()
    }

    fn file_paths(config: &ConfigSet) -> Vec<&Path> {
        config
            .files
            .iter()
            .map(|file| file.path.as_path())
            .collect()
    }

    #[test]
    fn extracts_host_aliases_with_their_source_and_indices() {
        let (_temp, paths) = fixture();
        let root = paths.config_file();
        let input = b"Port 22\n# local\nHost web\n HostName example.com\nHost web";
        write(&root, input);

        let config = load(&paths).unwrap();
        assert_eq!(file_paths(&config), [root.as_path()]);
        assert_eq!(config.files[0].parsed.bytes(), input);
        assert_eq!(
            config.hosts,
            [
                HostEntry {
                    name: "web".into(),
                    source: root.clone(),
                    file_index: 0,
                    block_index: 0,
                    pattern_index: 0,
                    line_number: 3,
                    siblings: vec![],
                },
                HostEntry {
                    name: "web".into(),
                    source: root,
                    file_index: 0,
                    block_index: 1,
                    pattern_index: 0,
                    line_number: 5,
                    siblings: vec![],
                }
            ]
        );
    }

    #[test]
    fn skips_wildcards_negations_and_match_headers() {
        let (_temp, paths) = fixture();
        write(
            &paths.config_file(),
            "Host *\nHost *.example.com\nHost web-?\nHost !secret\nMatch host x\n HostName hidden\nHost visible\n",
        );

        let config = load(&paths).unwrap();
        assert_eq!(host_names(&config), ["visible"]);
        assert_eq!(config.hosts[0].block_index, 5);
    }

    #[test]
    fn keeps_sibling_patterns_in_argument_order() {
        let (_temp, paths) = fixture();
        write(
            &paths.config_file(),
            "Host web db *.internal\nHost !secret stage stage web-?\n",
        );

        let config = load(&paths).unwrap();
        assert_eq!(host_names(&config), ["web", "db", "stage", "stage"]);
        assert_eq!(config.hosts[0].pattern_index, 0);
        assert_eq!(config.hosts[0].siblings, ["db", "*.internal"]);
        assert_eq!(config.hosts[1].pattern_index, 1);
        assert_eq!(config.hosts[1].siblings, ["web", "*.internal"]);
        assert_eq!(config.hosts[2].pattern_index, 1);
        assert_eq!(config.hosts[3].pattern_index, 2);
        assert_eq!(config.hosts[2].siblings, ["!secret", "stage", "web-?"]);
        assert_eq!(config.hosts[3].siblings, ["!secret", "stage", "web-?"]);
    }

    #[test]
    fn extracts_case_insensitive_host_directives_with_equals() {
        let (_temp, paths) = fixture();
        write(&paths.config_file(), "host=web\nHOST web2\n");

        assert_eq!(host_names(&load(&paths).unwrap()), ["web", "web2"]);
    }

    #[test]
    fn expands_relative_include_globs_inside_a_dot_ssh_directory() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let ssh = home.join(".ssh");
        let paths = SshPaths::resolve(Some(ssh.as_os_str()), Some(&home), temp.path()).unwrap();
        let root = paths.config_file();
        let included = ssh.join("conf.d/a.conf");
        write(&root, b"Include conf.d/*.conf\n");
        write(&included, b"Host included\n");

        let config = load(&paths).unwrap();
        assert_eq!(file_paths(&config), [root.as_path(), included.as_path()]);
        assert_eq!(host_names(&config), ["included"]);
        assert_eq!(config.files[1].parsed.bytes(), b"Host included\n");
        assert_eq!(config.warnings, []);
    }

    #[test]
    fn resolves_nested_relative_includes_from_the_ssh_directory() {
        let (_temp, paths) = fixture();
        let ssh = paths.ssh_dir();
        let outer = ssh.join("conf.d/outer.conf");
        let nested = ssh.join("nested.conf");
        write(&paths.config_file(), "Include conf.d/*.conf\n");
        write(&outer, "Include nested.conf\nHost outer\n");
        write(&nested, "Host nested\n");
        write(&ssh.join("conf.d/nested.conf/unused"), "Host wrong\n");

        let config = load(&paths).unwrap();
        assert_eq!(host_names(&config), ["nested", "outer"]);
        assert_eq!(file_paths(&config), [paths.config_file(), outer, nested]);
    }

    #[test]
    fn visits_glob_matches_alphabetically_at_the_include_line() {
        let (_temp, paths) = fixture();
        let ssh = paths.ssh_dir();
        let first = ssh.join("conf.d/a.conf");
        let second = ssh.join("conf.d/b.conf");
        write(
            &paths.config_file(),
            "Host before\n Include conf.d/*.conf\nHost after\n",
        );
        write(&second, "Host second\n");
        write(&first, "Host first\n");

        let config = load(&paths).unwrap();
        assert_eq!(host_names(&config), ["before", "first", "second", "after"]);
        assert_eq!(
            file_paths(&config),
            [paths.config_file(), first.clone(), second.clone()]
        );
        assert_eq!(config.hosts[1].source, first);
        assert_eq!(config.hosts[1].file_index, 1);
        assert_eq!(config.hosts[2].source, second);
        assert_eq!(config.hosts[2].file_index, 2);
    }

    #[test]
    fn expands_tilde_against_the_injected_home_and_keeps_absolute_paths() {
        let (temp, paths) = fixture();
        let root = paths.config_file();
        let relative_to_home = paths.home().unwrap().join("extra/config");
        let absolute = temp.path().join("other directory/config");
        let absolute_arg = absolute
            .to_string_lossy()
            .replace('\\', r"\\")
            .replace('"', r#"\""#);
        write(
            &root,
            format!("Include ~/extra/config\nInclude \"{absolute_arg}\"\n"),
        );
        write(&relative_to_home, "Host from-home\n");
        write(&absolute, "Host from-absolute\n");

        let config = load(&paths).unwrap();
        assert_eq!(host_names(&config), ["from-home", "from-absolute"]);
        assert_eq!(file_paths(&config), [root, relative_to_home, absolute]);
    }

    #[test]
    fn expands_multiple_include_arguments_in_argument_order() {
        let (_temp, paths) = fixture();
        let ssh = paths.ssh_dir();
        write(&paths.config_file(), "Include z.conf 'a config.conf'\n");
        write(&ssh.join("a config.conf"), "Host from-a\n");
        write(&ssh.join("z.conf"), "Host from-z\n");

        assert_eq!(host_names(&load(&paths).unwrap()), ["from-z", "from-a"]);
    }

    #[test]
    fn ignores_unmatched_globs_and_missing_files() {
        let (_temp, paths) = fixture();
        write(
            &paths.config_file(),
            "Include *.unknown missing.conf missing-dir/*.conf\nHost present\n",
        );

        let config = load(&paths).unwrap();
        assert_eq!(host_names(&config), ["present"]);
        assert_eq!(config.files.len(), 1);
        assert!(config.warnings.is_empty());
    }

    #[test]
    fn visits_each_file_once_when_includes_form_a_cycle() {
        let (_temp, paths) = fixture();
        let root = paths.config_file();
        let a = paths.ssh_dir().join("a");
        let b = paths.ssh_dir().join("b");
        write(&root, "Include a b\nHost root\n");
        write(&a, "Host a\nInclude b\n");
        write(&b, "Include a config\nHost b\n");

        let config = load(&paths).unwrap();
        assert_eq!(host_names(&config), ["a", "b", "root"]);
        assert_eq!(file_paths(&config), [root, a, b]);
        assert!(config.warnings.is_empty());
    }

    #[test]
    fn warns_about_unsupported_include_tokens_with_their_location() {
        let (_temp, paths) = fixture();
        let root = paths.config_file();
        write(&root, "# comment\nInclude ${HOME}/x %d/x\nHost next\n");

        let config = load(&paths).unwrap();
        assert_eq!(host_names(&config), ["next"]);
        assert_eq!(
            config.warnings,
            [
                ReadWarning::UnsupportedInclude {
                    file: root.clone(),
                    line_number: 2,
                    pattern: "${HOME}/x".into(),
                },
                ReadWarning::UnsupportedInclude {
                    file: root,
                    line_number: 2,
                    pattern: "%d/x".into(),
                }
            ]
        );
    }

    #[test]
    fn returns_an_empty_set_for_a_missing_root() {
        let (_temp, paths) = fixture();
        assert_eq!(load(&paths).unwrap(), ConfigSet::default());
    }

    #[test]
    fn glob_matches_regular_files_without_crossing_separators_or_matching_hidden_names() {
        let (_temp, paths) = fixture();
        let ssh = paths.ssh_dir();
        let visible = ssh.join("conf.d/visible.conf");
        write(&paths.config_file(), "Include conf.d/*.conf\n");
        write(&visible, "Host visible\n");
        write(&ssh.join("conf.d/.hidden.conf"), "Host hidden\n");
        write(&ssh.join("conf.d/upper.CONF"), "Host upper\n");
        write(&ssh.join("conf.d/nested.conf/deep.conf"), "Host deep\n");

        let config = load(&paths).unwrap();
        assert_eq!(host_names(&config), ["visible"]);
        assert_eq!(file_paths(&config), [paths.config_file(), visible]);
        assert!(config.warnings.is_empty());
    }

    #[test]
    fn resolves_relative_absolute_tilde_and_unsupported_patterns() {
        let (temp, paths) = fixture();
        let absolute = temp.path().join("external/config");
        let home = paths.home().unwrap();
        assert_eq!(
            resolve_include_pattern(&paths, "conf.d/*.conf"),
            Some(paths.ssh_dir().join("conf.d/*.conf"))
        );
        assert_eq!(
            resolve_include_pattern(&paths, &absolute.to_string_lossy()),
            Some(absolute)
        );
        assert_eq!(
            resolve_include_pattern(&paths, "~"),
            Some(home.to_path_buf())
        );
        assert_eq!(
            resolve_include_pattern(&paths, "~/extra/config"),
            Some(home.join("extra/config"))
        );
        assert_eq!(
            resolve_include_pattern(&paths, "~other/extra/config"),
            Some(paths.ssh_dir().join("~other/extra/config"))
        );
        for pattern in ["${HOME}/x", "unclosed/${", "extra/%d", "extra/%%"] {
            assert_eq!(resolve_include_pattern(&paths, pattern), None);
        }
    }

    #[cfg(unix)]
    #[test]
    fn deduplicates_symlinks_and_preserves_the_first_glob_path() {
        let (_temp, paths) = fixture();
        let root = paths.config_file();
        let target = paths.ssh_dir().join("actual.conf");
        let alias = paths.ssh_dir().join("alias.conf");
        write(&root, "Include alias.conf actual.conf\n");
        write(&target, "Host linked\n");
        std::os::unix::fs::symlink(&target, &alias).unwrap();

        let config = load(&paths).unwrap();
        assert_eq!(host_names(&config), ["linked"]);
        assert_eq!(file_paths(&config), [root.as_path(), alias.as_path()]);
        assert_eq!(config.hosts[0].source, alias);
        assert!(config.warnings.is_empty());
    }

    #[test]
    fn reports_invalid_globs_and_continues_with_later_arguments() {
        let (_temp, paths) = fixture();
        let root = paths.config_file();
        write(&root, "\nInclude invalid[ good.conf\n");
        write(&paths.ssh_dir().join("good.conf"), "Host good\n");

        let config = load(&paths).unwrap();
        assert_eq!(host_names(&config), ["good"]);
        assert_eq!(
            config.warnings,
            [ReadWarning::InvalidGlob {
                file: root,
                line_number: 2,
                pattern: "invalid[".into(),
            }]
        );
    }

    #[test]
    fn skips_includes_beyond_depth_sixteen_and_can_visit_them_later() {
        let (_temp, paths) = fixture();
        let ssh = paths.ssh_dir();
        let deep_file = ssh.join("level-17");
        write(
            &paths.config_file(),
            "Include level-1\nHost root\nInclude level-17\n",
        );
        for level in 1..=16 {
            write(
                &ssh.join(format!("level-{level}")),
                format!("Host at-{level}\nInclude level-{}\n", level + 1),
            );
        }
        write(&deep_file, "Host at-17\n");

        let config = load(&paths).unwrap();
        let expected_hosts: Vec<_> = (1..=16)
            .map(|level| format!("at-{level}"))
            .chain(["root".into(), "at-17".into()])
            .collect();
        assert_eq!(host_names(&config), expected_hosts);
        assert_eq!(config.files.len(), 18);
        assert_eq!(config.files[17].path, deep_file);
        assert_eq!(
            config.warnings,
            [ReadWarning::DepthExceeded { path: deep_file }]
        );
    }

    #[test]
    fn reports_a_root_read_error() {
        let (_temp, paths) = fixture();
        let root = paths.config_file();
        fs::create_dir_all(&root).unwrap();

        assert!(matches!(load(&paths), Err(ReadError::Io { path, .. }) if path == root));
    }

    #[cfg(unix)]
    #[test]
    fn warns_about_an_unreadable_include_and_continues() {
        let (_temp, paths) = fixture();
        let bad = paths.ssh_dir().join("loop.conf");
        write(&paths.config_file(), "Include loop.conf\nHost next\n");
        std::os::unix::fs::symlink(&bad, &bad).unwrap();

        let config = load(&paths).unwrap();
        assert_eq!(host_names(&config), ["next"]);
        assert_eq!(config.files.len(), 1);
        assert!(matches!(
            config.warnings.as_slice(),
            [ReadWarning::Unreadable { path, error }] if path == &bad && !error.is_empty()
        ));
    }
}
