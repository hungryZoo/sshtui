//! System services used by the application.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::app::Backend;
use crate::config::{self, ConfigSet, PendingWrite, WriteError, WriteOutcome};
use crate::dialog::{DialogError, FilePicker, PickOutcome};
use crate::keys::{self, KeyInfo};
use crate::paths::SshPaths;
use crate::ssh::args::config_file_args;
use crate::ssh::effective::{EffectiveConfig, EffectiveResolver, SystemResolver};

/// Backend that talks to the real file system, ssh and the native dialog.
pub struct SystemBackend {
    paths: SshPaths,
    ssh: PathBuf,
    picker: Box<dyn FilePicker>,
}

impl SystemBackend {
    /// Creates a backend using the resolved paths, executable, and native picker.
    pub fn new(paths: SshPaths, ssh: PathBuf, picker: Box<dyn FilePicker>) -> Self {
        Self { paths, ssh, picker }
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

    fn effective(&self, alias: &str) -> Result<EffectiveConfig, String> {
        SystemResolver::new(self.ssh.clone(), self.config_prefix())
            .resolve(alias)
            .map_err(|error| error.to_string())
    }

    fn discover_keys(&self) -> Vec<KeyInfo> {
        keys::discover_keys(self.paths.ssh_dir()).unwrap_or_default()
    }

    fn is_writable(&self, path: &Path) -> bool {
        config::writer::is_writable(path)
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
