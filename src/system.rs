//! System services used by the application.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::app::Backend;
use crate::config::{self, ConfigSet, PendingWrite, WriteError, WriteOutcome};
use crate::dialog::{DialogError, FilePicker, PickOutcome};
use crate::history::{self, HistorySource};
use crate::keys::{self, KeyInfo};
use crate::known_hosts::{self, ForgetPlan};
use crate::paths::SshPaths;
use crate::ssh::args::config_file_args;
use crate::ssh::effective::{EffectiveConfig, EffectiveResolver, SystemResolver};
use crate::ssh::{exec, keysetup};
use crate::state;
use crate::update::UpdatePlan;

/// Backend that talks to the real file system, ssh and the native dialog.
pub struct SystemBackend {
    paths: SshPaths,
    ssh: PathBuf,
    picker: Box<dyn FilePicker>,
    update_plan: Option<UpdatePlan>,
    state_file: Option<PathBuf>,
    history_sources: Vec<HistorySource>,
}

impl SystemBackend {
    /// Creates a backend using the resolved paths, executable, and native picker.
    pub fn new(paths: SshPaths, ssh: PathBuf, picker: Box<dyn FilePicker>) -> Self {
        Self {
            paths,
            ssh,
            picker,
            update_plan: None,
            state_file: None,
            history_sources: Vec::new(),
        }
    }

    /// Sets the update plan available for this session.
    pub fn with_update_plan(mut self, plan: Option<UpdatePlan>) -> Self {
        self.update_plan = plan;
        self
    }

    /// Sets the optional file for remembering the last connected alias.
    pub fn with_state_file(mut self, path: Option<PathBuf>) -> Self {
        self.state_file = path;
        self
    }

    /// Sets the shell history files to read at startup.
    pub fn with_history_sources(mut self, sources: Vec<HistorySource>) -> Self {
        self.history_sources = sources;
        self
    }

    /// Path of the ssh executable.
    pub fn ssh(&self) -> &Path {
        &self.ssh
    }
}

impl Backend for SystemBackend {
    fn paths(&self) -> &SshPaths {
        &self.paths
    }

    fn config_prefix(&self) -> Vec<OsString> {
        config_file_args(&self.paths, self.paths.config_file().exists())
    }

    fn load_config(&self) -> Result<ConfigSet, String> {
        config::load(&self.paths).map_err(|error| error.to_string())
    }

    fn recent_ssh_commands(&self) -> Vec<String> {
        history::read_ssh_commands(&self.history_sources)
    }

    fn last_host(&self) -> Option<String> {
        self.state_file.as_deref().and_then(state::read_last_host)
    }

    fn record_connection(&self, alias: &str) {
        if let Some(path) = &self.state_file {
            let _ = state::write_last_host(path, alias);
        }
    }

    fn effective(&self, alias: &str) -> Result<EffectiveConfig, String> {
        SystemResolver::new(self.ssh.clone(), self.config_prefix())
            .resolve(alias)
            .map_err(|error| error.to_string())
    }

    fn discover_keys(&self) -> Vec<KeyInfo> {
        keys::discover_keys(self.paths.ssh_dir()).unwrap_or_default()
    }

    fn read_public_key(&self, key_file: &str) -> Result<String, String> {
        let path = keysetup::public_key_path(&self.paths, key_file);
        keysetup::read_public_key(&path).map_err(|error| error.to_string())
    }

    fn install_key(&self, args: &[OsString], public_key: &str) -> Result<i32, String> {
        let input = format!("{public_key}\n");
        exec::run_with_input(&self.ssh, args, input.as_bytes()).map_err(|error| error.to_string())
    }

    fn is_writable(&self, path: &Path) -> bool {
        config::writer::is_writable(path)
    }

    fn plan_forget(&self, name: &str) -> Result<Option<ForgetPlan>, String> {
        known_hosts::plan_forget(&self.paths, name).map_err(|error| error.to_string())
    }

    fn commit(&self, pending: &PendingWrite) -> Result<WriteOutcome, WriteError> {
        pending.commit()
    }

    fn picker_unavailable_reason(&self) -> Option<String> {
        self.picker.unavailable_reason()
    }

    fn hidden_files_hint(&self) -> Option<String> {
        self.picker.hidden_files_hint()
    }

    fn pick_file(&self) -> Result<PickOutcome, DialogError> {
        self.picker.pick_file(self.paths.ssh_dir())
    }

    fn update_plan(&self) -> Option<UpdatePlan> {
        self.update_plan.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::config::{OptionChanges, plan_edit};
    use crate::ssh::exec::find_ssh;

    struct FakePicker;

    impl FilePicker for FakePicker {
        fn unavailable_reason(&self) -> Option<String> {
            Some("fake availability".into())
        }

        fn hidden_files_hint(&self) -> Option<String> {
            Some("fake hidden files hint".into())
        }

        fn pick_file(&self, start_dir: &Path) -> Result<PickOutcome, DialogError> {
            Ok(PickOutcome::Picked(start_dir.join("picked_key")))
        }
    }

    fn fixture() -> (TempDir, SystemBackend) {
        let temp = tempdir().unwrap();
        let paths =
            SshPaths::resolve(Some("ssh".as_ref()), Some(temp.path()), temp.path()).unwrap();
        let ssh = temp.path().join("fake-ssh");
        fs::write(&ssh, "#!/bin/sh\nexit 1\n").unwrap();
        (temp, SystemBackend::new(paths, ssh, Box::new(FakePicker)))
    }

    fn write_config(backend: &SystemBackend, contents: &[u8]) {
        fs::create_dir_all(backend.paths().ssh_dir()).unwrap();
        fs::write(backend.paths().config_file(), contents).unwrap();
    }

    #[test]
    fn history_reads_only_ssh_commands_from_the_injected_zsh_file() {
        let (temp, backend) = fixture();
        let path = temp.path().join(".zsh_history");
        fs::write(
            &path,
            ": 1:0;ssh first.example\n: 2:0;echo ssh ignored\n: 3:0;ssh-add key\n: 4:0;ssh -p 2222 alice@second.example\n",
        )
        .unwrap();

        assert!(backend.recent_ssh_commands().is_empty());
        let backend = backend.with_history_sources(vec![HistorySource {
            path,
            format: history::HistoryFormat::Zsh,
        }]);
        assert_eq!(
            backend.recent_ssh_commands(),
            ["ssh first.example", "ssh -p 2222 alice@second.example"]
        );
    }

    #[test]
    fn last_host_round_trips_using_the_injected_state_file() {
        let (temp, backend) = fixture();
        let path = temp.path().join("state").join("last-host");
        let backend = backend.with_state_file(Some(path.clone()));
        assert_eq!(backend.last_host(), None);
        backend.record_connection("web");
        assert_eq!(backend.last_host().as_deref(), Some("web"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "web\n");
        backend.record_connection("db");
        assert_eq!(backend.last_host().as_deref(), Some("db"));
    }

    #[test]
    fn backend_without_a_state_file_has_no_last_host_and_creates_no_files() {
        let (temp, backend) = fixture();
        let files = || {
            fs::read_dir(temp.path())
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>()
        };
        let before = files();
        backend.record_connection("web");
        assert_eq!(backend.last_host(), None);
        assert_eq!(files(), before);
    }

    #[test]
    fn loads_the_injected_config() {
        let (temp, backend) = fixture();
        write_config(&backend, b"Host web\n  HostName example.com\n");

        let config = backend.load_config().unwrap();
        assert_eq!(config.hosts.len(), 1);
        assert_eq!(config.hosts[0].name, "web");
        assert_eq!(config.hosts[0].source, backend.paths().config_file());
        assert_eq!(backend.ssh(), temp.path().join("fake-ssh"));
    }

    #[test]
    fn reads_the_public_key_from_a_companion_file_in_the_injected_directory() {
        let (_temp, backend) = fixture();
        fs::create_dir(backend.paths().ssh_dir()).unwrap();
        let key = "ssh-ed25519 AAAA fixture";
        fs::write(
            backend.paths().ssh_dir().join("id_fixture.pub"),
            format!("{key}\nssh-ed25519 BBBB ignored\n"),
        )
        .unwrap();
        assert_eq!(backend.read_public_key("~/ssh/id_fixture").unwrap(), key);
    }

    #[cfg(unix)]
    #[test]
    fn installs_a_single_public_key_line_on_stdin_and_returns_the_exit_code() {
        use std::os::unix::fs::PermissionsExt;

        let (temp, backend) = fixture();
        fs::write(backend.ssh(), "#!/bin/sh\ncat > \"$1\" || exit 1\nexit 7\n").unwrap();
        fs::set_permissions(backend.ssh(), fs::Permissions::from_mode(0o755)).unwrap();
        let received = temp.path().join("received");
        let key = "ssh-ed25519 AAAA fixture";
        assert_eq!(
            backend
                .install_key(&[received.clone().into_os_string()], key)
                .unwrap(),
            7
        );
        assert_eq!(fs::read_to_string(received).unwrap(), format!("{key}\n"));
    }

    #[test]
    fn calculates_config_prefix_from_current_file_existence() {
        let (_temp, backend) = fixture();
        assert_eq!(backend.config_prefix(), ["-F", "none"]);

        write_config(&backend, b"Host web\n");
        assert_eq!(
            backend.config_prefix(),
            ["-F".into(), backend.paths().config_file().into_os_string()]
        );

        fs::remove_file(backend.paths().config_file()).unwrap();
        assert_eq!(backend.config_prefix(), ["-F", "none"]);
    }

    #[test]
    fn commits_the_planned_change_and_creates_a_backup() {
        let (_temp, backend) = fixture();
        let original = b"Host web\n  User alice\n";
        write_config(&backend, original);
        let loaded = backend.load_config().unwrap();
        let pending = plan_edit(
            &loaded,
            &loaded.hosts[0],
            Some("docs"),
            &OptionChanges::default(),
        )
        .unwrap();

        assert!(backend.is_writable(&pending.path));
        let result = backend.commit(&pending).unwrap();
        assert_eq!(
            fs::read(result.target).unwrap(),
            b"Host docs\n  User alice\n"
        );
        assert_eq!(fs::read(result.backup.unwrap()).unwrap(), original);
    }

    #[test]
    fn commits_the_known_hosts_removal_and_creates_a_backup() {
        let (_temp, backend) = fixture();
        let path = known_hosts::file_path(backend.paths());
        let original = b"web ssh-ed25519 fake-web-key\ndb ssh-ed25519 fake-db-key\n";
        fs::create_dir_all(backend.paths().ssh_dir()).unwrap();
        fs::write(&path, original).unwrap();

        let plan = backend.plan_forget("web").unwrap().unwrap();
        assert_eq!(plan.removed, 1);
        assert_eq!(fs::read(&path).unwrap(), original);
        let outcome = backend.commit(&plan.pending).unwrap();
        assert_eq!(outcome.target, path.canonicalize().unwrap());
        assert_eq!(fs::read(&path).unwrap(), b"db ssh-ed25519 fake-db-key\n");
        let backup = path.with_file_name(".known_hosts.sshtui.bak");
        assert_eq!(outcome.backup, Some(backup.canonicalize().unwrap()));
        assert_eq!(fs::read(backup).unwrap(), original);
    }

    #[test]
    fn plans_no_removal_when_known_hosts_is_missing() {
        let (_temp, backend) = fixture();
        assert_eq!(backend.plan_forget("web"), Ok(None));
    }

    #[test]
    fn discovers_keys_only_in_the_injected_directory() {
        let (_temp, backend) = fixture();
        assert!(backend.discover_keys().is_empty());

        let directory = backend.paths().ssh_dir();
        fs::create_dir(directory).unwrap();
        let path = directory.join("id_test");
        fs::write(&path, b"-----BEGIN OPENSSH PRIVATE KEY-----\n").unwrap();

        let keys = backend.discover_keys();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].path, path);
    }

    #[test]
    fn delegates_picker_metadata_and_the_start_directory() {
        let (_temp, backend) = fixture();
        assert_eq!(
            backend.pick_file().unwrap(),
            PickOutcome::Picked(backend.paths().ssh_dir().join("picked_key"))
        );
        assert_eq!(
            backend.picker_unavailable_reason().as_deref(),
            Some("fake availability")
        );
        assert_eq!(
            backend.hidden_files_hint().as_deref(),
            Some("fake hidden files hint")
        );
    }

    #[test]
    fn resolves_effective_values_from_the_injected_config_if_ssh_exists() {
        let Ok(ssh) = find_ssh() else {
            return;
        };
        let (_temp, fixture) = fixture();
        let backend = SystemBackend::new(fixture.paths, ssh, Box::new(FakePicker));
        write_config(
            &backend,
            b"Host web\n  HostName www.example.com\n  User deploy\n  Port 2222\n",
        );

        let values = backend.effective("web").unwrap();
        assert_eq!(values.host_name.as_deref(), Some("www.example.com"));
        assert_eq!(values.user.as_deref(), Some("deploy"));
        assert_eq!(values.port.as_deref(), Some("2222"));
    }
}
