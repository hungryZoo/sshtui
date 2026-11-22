//! Release discovery, cache management, and installer commands.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::ssh::args::preview;

/// Version of the running executable.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Source and release repository.
pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
/// Minimum time between automatic check attempts.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Environment variable that disables automatic update checks and notices.
pub const DISABLE_ENV: &str = "SSHTUI_NO_UPDATE_CHECK";

const NAME: &str = env!("CARGO_PKG_NAME");

/// Returns the URL of the latest release page.
pub fn latest_url() -> String {
    format!("{REPOSITORY}/releases/latest")
}

/// Returns the base URL of the latest release assets.
pub fn download_url() -> String {
    format!("{}/download", latest_url())
}

/// Returns the raw installer URL derived from the GitHub repository.
pub fn install_script_url() -> String {
    let repo = REPOSITORY
        .strip_prefix("https://github.com/")
        .unwrap_or(REPOSITORY);
    format!("https://raw.githubusercontent.com/{repo}/main/install.sh")
}

/// Parses a stable MAJOR.MINOR.PATCH version with an optional leading v.
pub fn parse_semver(text: &str) -> Option<(u32, u32, u32)> {
    fn number(text: &str) -> Option<u32> {
        if (text.len() > 1 && text.starts_with('0'))
            || !text.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        text.parse().ok()
    }

    let mut parts = text.strip_prefix('v').unwrap_or(text).split('.');
    let version = (
        number(parts.next()?)?,
        number(parts.next()?)?,
        number(parts.next()?)?,
    );
    parts.next().is_none().then_some(version)
}

/// Returns true when both versions parse and the candidate is newer.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    matches!(
        (parse_semver(candidate), parse_semver(current)),
        (Some(candidate), Some(current)) if candidate > current
    )
}

/// A nonempty value other than "0" disables automatic checks.
pub fn is_disabled_value(value: Option<&OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != "0")
}

/// Reads the environment setting for automatic checks.
pub fn is_disabled() -> bool {
    is_disabled_value(std::env::var_os(DISABLE_ENV).as_deref())
}

/// The most recent check attempt and known upstream version.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cache {
    /// Unix seconds of the last attempt (successful or not).
    pub checked_at: u64,
    /// Newest version seen upstream, without the v.
    pub latest: Option<String>,
}

impl Cache {
    /// Parses recognized key=value lines and defaults invalid entries.
    pub fn parse(text: &str) -> Self {
        let mut cache = Self::default();
        for (key, value) in text.lines().filter_map(|line| line.split_once('=')) {
            match key {
                "checked_at" => cache.checked_at = value.parse().unwrap_or_default(),
                "latest" => cache.latest = release_version(value).map(str::to_owned),
                _ => {}
            }
        }
        cache
    }

    /// Serializes the check timestamp and optional release version.
    pub fn render(&self) -> String {
        format!(
            "checked_at={}\nlatest={}\n",
            self.checked_at,
            self.latest.as_deref().unwrap_or_default()
        )
    }

    /// Returns whether no attempt was recorded in the last check interval.
    pub fn is_stale(&self, now: u64) -> bool {
        self.checked_at == 0 || now.saturating_sub(self.checked_at) >= CHECK_INTERVAL.as_secs()
    }
}

/// Returns the platform cache path, with home and relative fallbacks.
pub fn cache_path() -> PathBuf {
    dirs::cache_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join(".cache")))
        .unwrap_or_else(|| PathBuf::from(".").join(".cache"))
        .join(NAME)
        .join("update-check")
}

/// Reads the cache, defaulting when the file is missing or unreadable.
pub fn read_cache(path: &Path) -> Cache {
    fs::read_to_string(path).map_or_else(|_| Cache::default(), |text| Cache::parse(&text))
}

/// Creates parent directories and atomically replaces the cache file.
pub fn write_cache(path: &Path, cache: &Cache) -> io::Result<()> {
    let parent = cache_parent(path);
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(cache.render().as_bytes())?;
    file.persist(path)?;
    Ok(())
}

fn cache_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

fn open_cache_lock(path: &Path) -> io::Result<File> {
    // Keep the lock on a separate file that stays in place when the cache is replaced.
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".lock");
    fs::create_dir_all(cache_parent(path))?;
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
}

fn release_version(version: &str) -> Option<&str> {
    parse_semver(version)?;
    Some(version.strip_prefix('v').unwrap_or(version))
}

/// Extracts a stable version from the effective release redirect URL.
pub fn version_from_release_url(url: &str) -> Option<String> {
    let (_, tag) = url.split_once("/tag/")?;
    release_version(tag).map(str::to_owned)
}

/// Builds the curl arguments for reading the effective release URL.
pub fn fetch_args(url: &str, null_device: &str) -> Vec<OsString> {
    [
        "-fsSLI",
        "--max-time",
        "2",
        "-o",
        null_device,
        "-w",
        "%{url_effective}",
        url,
    ]
    .map(OsString::from)
    .into()
}

fn fetch_latest() -> Option<String> {
    let null_device = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let output = Command::new("curl")
        .args(fetch_args(&latest_url(), null_device))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    version_from_release_url(std::str::from_utf8(&output.stdout).ok()?)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Records a stale check attempt before spawning a thread to fetch and cache the latest release.
pub fn spawn_check_if_stale<F>(path: PathBuf, now: u64, fetch: F) -> Option<JoinHandle<()>>
where
    F: FnOnce() -> Option<String> + Send + 'static,
{
    if !read_cache(&path).is_stale(now) {
        return None;
    }
    let lock = open_cache_lock(&path).ok()?;
    lock.try_lock().ok()?;
    let mut cache = read_cache(&path);
    if !cache.is_stale(now) {
        return None;
    }
    cache.checked_at = now;
    write_cache(&path, &cache).ok()?;
    thread::Builder::new()
        .spawn(move || {
            // Hold the reservation until the result is written, even across processes.
            let _lock = lock;
            if let Some(latest) = fetch() {
                cache.latest = Some(latest);
                let _ = write_cache(&path, &cache);
            }
        })
        .ok()
}

/// Starts a detached automatic check if enabled and due.
pub fn maybe_spawn_check() {
    if !is_disabled() {
        drop(spawn_check_if_stale(cache_path(), unix_now(), fetch_latest));
    }
}

/// Result of a foreground update check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// A newer release is available.
    UpdateAvailable { latest: String },
    /// The fetched release is no newer than the current version.
    UpToDate { latest: String },
    /// The latest release could not be retrieved.
    Unreachable,
}

/// Records a foreground check, preserving the previous latest on failure, and classifies it.
pub fn check_with(path: &Path, now: u64, current: &str, fetched: Option<String>) -> CheckOutcome {
    if let Ok(lock) = open_cache_lock(path)
        && lock.lock().is_ok()
    {
        let mut cache = read_cache(path);
        cache.checked_at = now;
        if let Some(latest) = &fetched {
            cache.latest = Some(latest.clone());
        }
        let _ = write_cache(path, &cache);
    }

    match fetched {
        Some(latest) if is_newer(&latest, current) => CheckOutcome::UpdateAvailable { latest },
        Some(latest) => CheckOutcome::UpToDate { latest },
        None => CheckOutcome::Unreachable,
    }
}

/// Fetches the latest release synchronously, regardless of the automatic check settings.
pub fn check_now() -> CheckOutcome {
    let now = unix_now();
    let fetched = fetch_latest();
    check_with(&cache_path(), now, CURRENT_VERSION, fetched)
}

/// Formats the foreground check result and, if available, the upgrade command.
pub fn report(outcome: &CheckOutcome, current: &str, method: &InstallMethod) -> String {
    match outcome {
        CheckOutcome::UpdateAvailable { latest } => format!(
            "{NAME} {current} → {latest} available\nupgrade: {}\nrelease: {}",
            method.hint(latest),
            latest_url()
        ),
        CheckOutcome::UpToDate { latest } => {
            format!("{NAME} {current} is up to date (latest release: {latest})")
        }
        CheckOutcome::Unreachable => format!(
            "{NAME} {current}: could not reach {} (offline, no release yet, or curl missing)",
            latest_url()
        ),
    }
}

/// The detected installation source and any paths or package details it requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallMethod {
    Brew,
    Cargo,
    /// An installer script or a hand-copied binary in a user directory.
    Script {
        bin_dir: PathBuf,
    },
    Deb {
        arch: &'static str,
    },
    Rpm {
        arch: &'static str,
    },
    Unknown,
}

/// Maps Rust architecture names to Debian and optional RPM architecture names.
pub fn package_arch(arch: &str) -> Option<(&'static str, Option<&'static str>)> {
    match arch {
        "x86_64" => Some(("amd64", Some("x86_64"))),
        "aarch64" => Some(("arm64", Some("aarch64"))),
        "arm" => Some(("armhf", None)),
        _ => None,
    }
}

/// Classifies an installation from its location and the available package databases.
pub fn classify_install(
    exe: &Path,
    home: Option<&Path>,
    has_dpkg: bool,
    has_rpm: bool,
    arch: &str,
    windows: bool,
) -> InstallMethod {
    let in_cargo_bin = home.is_some_and(|home| exe.starts_with(home.join(".cargo").join("bin")));
    if windows {
        return if in_cargo_bin {
            InstallMethod::Cargo
        } else {
            InstallMethod::Unknown
        };
    }

    let exe_text = exe.to_string_lossy();
    if ["/Cellar/", "/homebrew/", "/linuxbrew/"]
        .iter()
        .any(|segment| exe_text.contains(segment))
    {
        return InstallMethod::Brew;
    }
    if in_cargo_bin {
        return InstallMethod::Cargo;
    }
    if let Some(bin_dir) = exe.parent()
        && home.is_some_and(|home| bin_dir.starts_with(home))
    {
        return InstallMethod::Script {
            bin_dir: bin_dir.to_path_buf(),
        };
    }
    if (exe.starts_with("/usr/bin") || exe.starts_with("/usr/local/bin"))
        && let Some((deb_arch, rpm_arch)) = package_arch(arch)
    {
        if has_dpkg {
            return InstallMethod::Deb { arch: deb_arch };
        }
        if has_rpm && let Some(arch) = rpm_arch {
            return InstallMethod::Rpm { arch };
        }
    }
    InstallMethod::Unknown
}

/// Detects the installation source of the running executable.
pub fn detect_install_method() -> InstallMethod {
    let Ok(exe) = std::env::current_exe() else {
        return InstallMethod::Unknown;
    };
    classify_install(
        &exe,
        dirs::home_dir().as_deref(),
        !cfg!(windows) && Path::new("/var/lib/dpkg/status").is_file(),
        !cfg!(windows) && Path::new("/var/lib/rpm").is_dir(),
        std::env::consts::ARCH,
        cfg!(windows),
    )
}

/// A single upgrade process and its exact arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeStep {
    /// Executable to run.
    pub program: OsString,
    /// Arguments passed directly to the executable.
    pub args: Vec<OsString>,
}

impl UpgradeStep {
    fn new(program: &str, args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    fn download(path: &Path, url: String) -> Self {
        Self::new(
            "curl",
            [
                OsStr::new("-fsSL"),
                OsStr::new("-o"),
                path.as_os_str(),
                OsStr::new(&url),
            ],
        )
    }

    fn display(&self) -> String {
        // SSH's preview applies the same argument quoting after its fixed program name.
        preview(&self.args).replacen("ssh", &self.program.to_string_lossy(), 1)
    }
}

impl InstallMethod {
    /// Returns the installation source label shown in the update dialog.
    pub fn describe(&self) -> String {
        match self {
            Self::Brew => "Homebrew".into(),
            Self::Cargo => "cargo".into(),
            Self::Script { bin_dir } => format!("install.sh in {}", bin_dir.display()),
            Self::Deb { .. } => "Debian package (.deb)".into(),
            Self::Rpm { .. } => "RPM package".into(),
            Self::Unknown => "unknown (manual install)".into(),
        }
    }

    /// Returns the first upgrade command or the release page for manual updates.
    pub fn hint(&self, latest: &str) -> String {
        self.display_steps(latest)
            .into_iter()
            .next()
            .unwrap_or_else(latest_url)
    }

    /// Builds the upgrade steps for a valid release with downloads under work_dir.
    pub fn upgrade_steps(&self, latest: &str, work_dir: &Path) -> Option<Vec<UpgradeStep>> {
        let latest = release_version(latest)?;
        let steps = match self {
            Self::Brew => vec![UpgradeStep::new("brew", ["upgrade", NAME])],
            Self::Cargo => vec![UpgradeStep::new(
                "cargo",
                [
                    "install",
                    "--git",
                    REPOSITORY,
                    "--tag",
                    &format!("v{latest}"),
                    "--locked",
                ],
            )],
            Self::Script { bin_dir } => {
                let script = work_dir.join("install.sh");
                vec![
                    UpgradeStep::download(&script, install_script_url()),
                    UpgradeStep::new(
                        "sh",
                        [
                            script.as_os_str(),
                            OsStr::new("--bin-dir"),
                            bin_dir.as_os_str(),
                        ],
                    ),
                ]
            }
            Self::Deb { arch } => {
                let name = format!("{NAME}_{arch}.deb");
                let package = work_dir.join(&name);
                vec![
                    UpgradeStep::download(&package, format!("{}/{name}", download_url())),
                    UpgradeStep::new(
                        "sudo",
                        [OsStr::new("dpkg"), OsStr::new("-i"), package.as_os_str()],
                    ),
                ]
            }
            Self::Rpm { arch } => vec![UpgradeStep::new(
                "sudo",
                [
                    "rpm",
                    "-U",
                    &format!("{}/{NAME}-{arch}.rpm", download_url()),
                ],
            )],
            Self::Unknown => return None,
        };
        Some(steps)
    }

    /// Formats the upgrade steps with a placeholder for the temporary directory.
    pub fn display_steps(&self, latest: &str) -> Vec<String> {
        self.upgrade_steps(latest, Path::new("<tmp>"))
            .into_iter()
            .flatten()
            .map(|step| step.display())
            .collect()
    }
}

/// The release and installation method used for a notice or an upgrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePlan {
    /// Version of the running executable.
    pub current: String,
    /// Newest release version without the v.
    pub latest: String,
    /// Method used to upgrade the current installation.
    pub method: InstallMethod,
}

impl UpdatePlan {
    /// Returns the one-line list notice.
    pub fn notice(&self) -> String {
        format!("v{} available (u: update)", self.latest)
    }
}

/// Builds a plan only when the cached latest release is newer than the running version.
pub fn plan_from(cache: &Cache, current: &str, method: InstallMethod) -> Option<UpdatePlan> {
    let latest = release_version(cache.latest.as_deref()?)?;
    is_newer(latest, current).then(|| UpdatePlan {
        current: current.into(),
        latest: latest.into(),
        method,
    })
}

/// Reads the latest cached release without contacting the network.
pub fn plan() -> Option<UpdatePlan> {
    if is_disabled() {
        return None;
    }
    let cache = read_cache(&cache_path());
    plan_from(&cache, CURRENT_VERSION, detect_install_method())
}

/// Runs the upgrade with inherited stdio after the caller has restored the terminal.
pub fn run_upgrade(plan: &UpdatePlan) -> io::Result<bool> {
    let work_dir = tempfile::tempdir()?;
    let work_path = std::path::absolute(work_dir.path())?;
    let Some(steps) = plan.method.upgrade_steps(&plan.latest, &work_path) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no upgrade command for this installation or version",
        ));
    };
    for step in steps {
        let display = step.display();
        println!("{NAME}: running: {display}");
        let status = Command::new(&step.program)
            .current_dir(&work_path)
            .args(&step.args)
            .status()
            .map_err(|error| io::Error::new(error.kind(), format!("{display}: {error}")))?;
        if !status.success() {
            eprintln!("{NAME}: {display} failed ({status})");
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_compares_stable_versions() {
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("0.0.0"), Some((0, 0, 0)));
        for invalid in [
            "",
            "1.2",
            "1.2.3-rc1",
            "1.2.3.4",
            "1.2.3+build",
            "+1.2.3",
            "1.02.3",
            "4294967296.0.0",
            " 1.2.3",
        ] {
            assert_eq!(parse_semver(invalid), None, "{invalid:?}");
        }

        assert!(is_newer("2.0.0", "1.9.9"));
        assert!(is_newer("v1.10.0", "1.9.9"));
        assert!(is_newer("1.2.4", "1.2.3"));
        assert!(!is_newer("1.2.3", "v1.2.3"));
        assert!(!is_newer("1.2.3", "1.2.4"));
        assert!(!is_newer("invalid", "1.2.3"));
        assert!(!is_newer("1.2.3", "invalid"));
    }

    #[test]
    fn extracts_only_stable_release_tags() {
        assert_eq!(
            version_from_release_url("https://example.com/releases/tag/v1.1.1").as_deref(),
            Some("1.1.1")
        );
        assert_eq!(
            version_from_release_url("https://example.com/releases/tag/1.2.3").as_deref(),
            Some("1.2.3")
        );
        for url in [
            "https://example.com/releases/latest",
            "https://example.com/releases",
            "",
            "https://example.com/releases/tag/15.2.0-beta",
        ] {
            assert_eq!(version_from_release_url(url), None, "{url:?}");
        }
    }

    #[test]
    fn queries_the_redirect_with_a_two_second_timeout() {
        let url = "https://example.com/releases/latest";
        for null_device in ["/dev/null", "NUL"] {
            assert_eq!(
                fetch_args(url, null_device),
                [
                    "-fsSLI",
                    "--max-time",
                    "2",
                    "-o",
                    null_device,
                    "-w",
                    "%{url_effective}",
                    url
                ]
            );
        }
    }

    #[test]
    fn cache_round_trips_and_defaults_invalid_entries() {
        let cache = Cache {
            checked_at: 42,
            latest: Some("1.2.3".into()),
        };
        assert_eq!(cache.render(), "checked_at=42\nlatest=1.2.3\n");
        assert_eq!(Cache::parse(&cache.render()), cache);
        let default = Cache::default();
        for contents in [
            "",
            "latest=\n",
            "checked_at=abc\ngarbage\nignored=value\nlatest=bad",
        ] {
            assert_eq!(Cache::parse(contents), default);
        }
        assert_eq!(default.render(), "checked_at=0\nlatest=\n");
    }

    #[test]
    fn cache_becomes_stale_exactly_after_one_day() {
        assert!(Cache::default().is_stale(0));
        let cache = Cache {
            checked_at: 100,
            latest: None,
        };
        assert!(!cache.is_stale(99));
        assert!(!cache.is_stale(100 + 86_399));
        assert!(cache.is_stale(100 + 86_400));
    }

    #[test]
    fn writes_and_reads_cache_in_nested_directory() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp
            .path()
            .join("nested")
            .join("cache")
            .join("update-check");
        assert_eq!(read_cache(&path), Cache::default());
        let cache = Cache {
            checked_at: 123,
            latest: Some("4.5.6".into()),
        };
        write_cache(&path, &cache).unwrap();
        assert_eq!(read_cache(&path), cache);
        assert_eq!(read_cache(path.parent().unwrap()), Cache::default());
    }

    #[test]
    fn records_an_attempt_before_fetching_and_stores_success() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("update-check");
        let observed_path = path.clone();
        let now = CHECK_INTERVAL.as_secs() + 1;
        write_cache(
            &path,
            &Cache {
                checked_at: 1,
                latest: None,
            },
        )
        .unwrap();

        spawn_check_if_stale(path.clone(), now, move || {
            assert_eq!(read_cache(&observed_path).checked_at, now);
            Some("1.2.3".into())
        })
        .unwrap()
        .join()
        .unwrap();
        assert_eq!(
            read_cache(&path),
            Cache {
                checked_at: now,
                latest: Some("1.2.3".into()),
            }
        );
    }

    #[test]
    fn failed_attempt_keeps_the_previous_release_and_advances_the_timestamp() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("update-check");
        let mut cache = Cache {
            checked_at: 1,
            latest: Some("1.0.0".into()),
        };
        write_cache(&path, &cache).unwrap();
        let now = 1 + CHECK_INTERVAL.as_secs();

        spawn_check_if_stale(path.clone(), now, || None)
            .unwrap()
            .join()
            .unwrap();
        cache.checked_at = now;
        assert_eq!(read_cache(&path), cache);
    }

    #[test]
    fn skips_fetching_for_fresh_cache_or_when_the_attempt_cannot_be_recorded() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("update-check");
        let cache = Cache {
            checked_at: 100,
            latest: Some("1.2.3".into()),
        };
        write_cache(&path, &cache).unwrap();
        let original = fs::read(&path).unwrap();
        assert!(
            spawn_check_if_stale(path.clone(), 101, || panic!("fresh cache fetched")).is_none()
        );
        assert_eq!(fs::read(path).unwrap(), original);

        let blocked = temp.path().join("blocked");
        fs::create_dir(&blocked).unwrap();
        assert!(
            spawn_check_if_stale(blocked, 101, || panic!("unrecorded attempt fetched")).is_none()
        );
    }

    #[test]
    fn competing_automatic_checks_do_not_fetch_twice() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("update-check");
        let (release, wait) = std::sync::mpsc::channel();
        let pending = spawn_check_if_stale(path.clone(), 1, move || {
            wait.recv().unwrap();
            Some("1.2.3".into())
        })
        .unwrap();

        assert!(
            spawn_check_if_stale(path.clone(), 1, || panic!("duplicate attempt fetched")).is_none()
        );
        release.send(()).unwrap();
        pending.join().unwrap();
        assert_eq!(read_cache(&path).latest.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn foreground_check_reports_and_records_each_outcome() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("update-check");
        let current = "1.0.0";

        assert_eq!(
            check_with(&path, 100, current, Some("2.0.0".into())),
            CheckOutcome::UpdateAvailable {
                latest: "2.0.0".into()
            }
        );
        assert_eq!(
            read_cache(&path),
            Cache {
                checked_at: 100,
                latest: Some("2.0.0".into())
            }
        );
        assert_eq!(
            check_with(&path, 101, current, Some(current.into())),
            CheckOutcome::UpToDate {
                latest: current.into()
            }
        );
        assert_eq!(
            read_cache(&path),
            Cache {
                checked_at: 101,
                latest: Some(current.into())
            }
        );
        assert_eq!(
            check_with(&path, 102, current, None),
            CheckOutcome::Unreachable
        );
        assert_eq!(
            read_cache(&path),
            Cache {
                checked_at: 102,
                latest: Some(current.into())
            }
        );
    }

    #[test]
    fn only_nonempty_values_other_than_zero_disable_checks() {
        assert!(!is_disabled_value(None));
        for value in ["", "0"] {
            assert!(!is_disabled_value(Some(OsStr::new(value))));
        }
        for value in ["1", "true"] {
            assert!(is_disabled_value(Some(OsStr::new(value))));
        }
    }

    #[test]
    fn classifies_homebrew_before_user_owned_installs() {
        for exe in [
            "/opt/homebrew/bin/x",
            "/usr/local/Cellar/x/1.0/bin/x",
            "/home/linuxbrew/.linuxbrew/bin/x",
        ] {
            assert_eq!(
                classify_install(
                    Path::new(exe),
                    Some(Path::new("/home")),
                    false,
                    false,
                    "x86_64",
                    false
                ),
                InstallMethod::Brew
            );
        }
    }

    #[test]
    fn classifies_user_owned_installs_and_limits_windows_to_cargo() {
        let home = Path::new("/home/user");
        let cargo = home.join(".cargo").join("bin").join("x");
        let local = home.join(".local").join("bin");
        let script = local.join("x");
        for windows in [false, true] {
            assert_eq!(
                classify_install(&cargo, Some(home), false, false, "x86_64", windows),
                InstallMethod::Cargo
            );
        }
        assert_eq!(
            classify_install(&script, Some(home), false, false, "x86_64", false),
            InstallMethod::Script { bin_dir: local }
        );
        for exe in [
            script.as_path(),
            Path::new("/opt/homebrew/bin/x"),
            Path::new("/usr/bin/x"),
        ] {
            assert_eq!(
                classify_install(exe, Some(home), true, true, "x86_64", true),
                InstallMethod::Unknown
            );
        }
    }

    #[test]
    fn classifies_system_packages_by_database_path_and_architecture() {
        for (exe, dpkg, rpm, arch, expected) in [
            (
                "/usr/bin/x",
                true,
                true,
                "x86_64",
                InstallMethod::Deb { arch: "amd64" },
            ),
            (
                "/usr/bin/x",
                false,
                true,
                "x86_64",
                InstallMethod::Rpm { arch: "x86_64" },
            ),
            ("/usr/bin/x", false, false, "x86_64", InstallMethod::Unknown),
            (
                "/usr/local/bin/x",
                true,
                false,
                "aarch64",
                InstallMethod::Deb { arch: "arm64" },
            ),
            (
                "/usr/bin/x",
                false,
                true,
                "aarch64",
                InstallMethod::Rpm { arch: "aarch64" },
            ),
            (
                "/usr/bin/x",
                true,
                false,
                "arm",
                InstallMethod::Deb { arch: "armhf" },
            ),
            ("/usr/bin/x", false, true, "arm", InstallMethod::Unknown),
            ("/usr/bin/x", true, true, "riscv64", InstallMethod::Unknown),
            (
                "/usr/bin-other/x",
                true,
                true,
                "x86_64",
                InstallMethod::Unknown,
            ),
            (
                "/opt/custom/bin/x",
                true,
                true,
                "x86_64",
                InstallMethod::Unknown,
            ),
        ] {
            assert_eq!(
                classify_install(Path::new(exe), None, dpkg, rpm, arch, false),
                expected
            );
        }
    }

    fn assert_step(step: &UpgradeStep, program: &str, args: &[&OsStr]) {
        assert_eq!(step.program, program);
        assert_eq!(step.args, args);
    }

    #[test]
    fn prepares_homebrew_and_cargo_commands() {
        let work_dir = Path::new("arbitrary");
        let brew = InstallMethod::Brew
            .upgrade_steps("1.2.3", work_dir)
            .unwrap();
        assert_eq!(brew.len(), 1);
        assert_step(&brew[0], "brew", &["upgrade", "sshtui"].map(OsStr::new));

        let cargo = InstallMethod::Cargo
            .upgrade_steps("v1.2.3", work_dir)
            .unwrap();
        assert_eq!(cargo.len(), 1);
        assert_step(
            &cargo[0],
            "cargo",
            &[
                "install", "--git", REPOSITORY, "--tag", "v1.2.3", "--locked",
            ]
            .map(OsStr::new),
        );
    }

    #[test]
    fn prepares_script_download_and_run_with_the_original_bin_directory() {
        let work_dir = Path::new("arbitrary");
        let bin_dir = PathBuf::from("my tools");
        let script = work_dir.join("install.sh");
        let steps = InstallMethod::Script {
            bin_dir: bin_dir.clone(),
        }
        .upgrade_steps("1.2.3", work_dir)
        .unwrap();
        assert_eq!(steps.len(), 2);
        assert_step(
            &steps[0],
            "curl",
            &[
                OsStr::new("-fsSL"),
                OsStr::new("-o"),
                script.as_os_str(),
                OsStr::new(&install_script_url()),
            ],
        );
        assert_step(
            &steps[1],
            "sh",
            &[
                script.as_os_str(),
                OsStr::new("--bin-dir"),
                bin_dir.as_os_str(),
            ],
        );
    }

    #[test]
    fn prepares_debian_and_rpm_commands() {
        let work_dir = Path::new("arbitrary");
        let package = work_dir.join("sshtui_arm64.deb");
        let deb = InstallMethod::Deb { arch: "arm64" }
            .upgrade_steps("1.2.3", work_dir)
            .unwrap();
        assert_eq!(deb.len(), 2);
        assert_step(
            &deb[0],
            "curl",
            &[
                OsStr::new("-fsSL"),
                OsStr::new("-o"),
                package.as_os_str(),
                OsStr::new(&format!("{}/sshtui_arm64.deb", download_url())),
            ],
        );
        assert_step(
            &deb[1],
            "sudo",
            &[OsStr::new("dpkg"), OsStr::new("-i"), package.as_os_str()],
        );

        let rpm = InstallMethod::Rpm { arch: "x86_64" }
            .upgrade_steps("1.2.3", work_dir)
            .unwrap();
        assert_eq!(rpm.len(), 1);
        assert_step(
            &rpm[0],
            "sudo",
            &[
                "rpm",
                "-U",
                &format!("{}/sshtui-x86_64.rpm", download_url()),
            ]
            .map(OsStr::new),
        );
    }

    #[test]
    fn rejects_unknown_installations_and_bad_versions() {
        let work_dir = Path::new("arbitrary");
        assert!(
            InstallMethod::Unknown
                .upgrade_steps("1.2.3", work_dir)
                .is_none()
        );
        assert!(
            InstallMethod::Brew
                .upgrade_steps("1.2.3-rc1", work_dir)
                .is_none()
        );
    }

    #[test]
    fn formats_commands_with_a_placeholder_and_quotes_paths() {
        let method = InstallMethod::Script {
            bin_dir: PathBuf::from("my team's tools"),
        };
        let script = Path::new("<tmp>").join("install.sh");
        assert_eq!(
            method.display_steps("1.2.3"),
            [
                format!(
                    "curl -fsSL -o '{}' {}",
                    script.display(),
                    install_script_url()
                ),
                format!("sh '{}' --bin-dir 'my team'\\''s tools'", script.display()),
            ]
        );
    }

    #[test]
    fn derives_release_and_installer_urls_from_repository() {
        assert_eq!(
            latest_url(),
            "https://github.com/hungryZoo/sshtui/releases/latest"
        );
        assert_eq!(
            download_url(),
            "https://github.com/hungryZoo/sshtui/releases/latest/download"
        );
        assert_eq!(
            install_script_url(),
            "https://raw.githubusercontent.com/hungryZoo/sshtui/main/install.sh"
        );
    }

    #[test]
    fn plans_and_announces_only_a_newer_cached_release() {
        let current = "1.2.3";
        let mut cache = Cache {
            checked_at: 1,
            latest: Some("1.2.4".into()),
        };
        let plan = plan_from(&cache, current, InstallMethod::Brew).unwrap();
        assert_eq!(
            plan,
            UpdatePlan {
                current: current.into(),
                latest: "1.2.4".into(),
                method: InstallMethod::Brew,
            }
        );
        assert_eq!(plan.notice(), "v1.2.4 available (u: update)");
        for latest in [Some(current), Some("1.2.2"), None] {
            cache.latest = latest.map(str::to_owned);
            assert_eq!(plan_from(&cache, current, InstallMethod::Brew), None);
        }
    }

    #[test]
    fn reports_available_up_to_date_and_unreachable_results() {
        assert_eq!(
            report(
                &CheckOutcome::UpdateAvailable {
                    latest: "0.2.0".into()
                },
                "0.1.0",
                &InstallMethod::Brew
            ),
            format!(
                "sshtui 0.1.0 → 0.2.0 available\nupgrade: brew upgrade sshtui\nrelease: {}",
                latest_url()
            )
        );
        assert_eq!(
            report(
                &CheckOutcome::UpdateAvailable {
                    latest: "0.2.0".into()
                },
                "0.1.0",
                &InstallMethod::Unknown
            ),
            format!(
                "sshtui 0.1.0 → 0.2.0 available\nupgrade: {}\nrelease: {}",
                latest_url(),
                latest_url()
            )
        );
        assert_eq!(
            report(
                &CheckOutcome::UpToDate {
                    latest: "0.1.0".into()
                },
                "0.1.0",
                &InstallMethod::Unknown
            ),
            "sshtui 0.1.0 is up to date (latest release: 0.1.0)"
        );
        assert_eq!(
            report(&CheckOutcome::Unreachable, "0.1.0", &InstallMethod::Unknown),
            format!(
                "sshtui 0.1.0: could not reach {} (offline, no release yet, or curl missing)",
                latest_url()
            )
        );
    }
}
