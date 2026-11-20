use std::ffi::OsString;
use std::fs;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};
use tempfile::{TempDir, tempdir};

use super::render;
use crate::app::{App, Backend, FormField, Mode};
use crate::config::{self, ConfigSet, PendingWrite, WriteError, WriteOutcome};
use crate::dialog::{DialogError, PickOutcome};
use crate::keys::{KeyInfo, PublicKeyInfo};
use crate::paths::SshPaths;
use crate::ssh::effective::EffectiveConfig;

const HOSTS: &str = "# Saved hosts\nHost web\nHost db\nHost queue\n";

/// Backend fixture with a temporary config and predictable effective values.
pub(crate) struct FakeBackend {
    _temp: TempDir,
    paths: SshPaths,
    effective: Result<EffectiveConfig, String>,
    read_only: bool,
    keys: Vec<KeyInfo>,
    unavailable: Option<String>,
    hidden_hint: Option<String>,
}

impl FakeBackend {
    /// Creates a fake session from an optional config in a temporary home.
    pub(crate) fn new(contents: &str) -> Self {
        let temp = tempdir().unwrap();
        let paths = SshPaths::resolve(None, Some(temp.path()), temp.path()).unwrap();
        if !contents.is_empty() {
            fs::create_dir(paths.ssh_dir()).unwrap();
            fs::write(paths.config_file(), contents).unwrap();
        }
        Self {
            _temp: temp,
            paths,
            effective: Ok(EffectiveConfig {
                user: Some("alice".into()),
                host_name: Some("www.example.com".into()),
                port: Some("2222".into()),
                identity_files: vec!["~/.ssh/first".into(), "~/.ssh/second".into()],
            }),
            read_only: false,
            keys: Vec::new(),
            unavailable: None,
            hidden_hint: None,
        }
    }

    fn with_keys(mut self) -> Self {
        self.keys = ["id_a", "id_b"]
            .into_iter()
            .map(|name| KeyInfo {
                path: self.paths.ssh_dir().join(name),
                file_name: name.into(),
                public: Some(PublicKeyInfo {
                    key_type: "ssh-ed25519".into(),
                    comment: Some("test key".into()),
                }),
            })
            .collect();
        self
    }
}

impl Backend for FakeBackend {
    fn paths(&self) -> &SshPaths {
        &self.paths
    }

    fn config_prefix(&self) -> Vec<OsString> {
        Vec::new()
    }

    fn load_config(&self) -> Result<ConfigSet, String> {
        config::load(&self.paths).map_err(|error| error.to_string())
    }

    fn effective(&self, _: &str) -> Result<EffectiveConfig, String> {
        self.effective.clone()
    }

    fn discover_keys(&self) -> Vec<KeyInfo> {
        self.keys.clone()
    }

    fn is_writable(&self, _: &Path) -> bool {
        !self.read_only
    }

    fn commit(&self, pending: &PendingWrite) -> Result<WriteOutcome, WriteError> {
        pending.commit()
    }

    fn picker_unavailable_reason(&self) -> Option<String> {
        self.unavailable.clone()
    }

    fn hidden_files_hint(&self) -> Option<String> {
        self.hidden_hint.clone()
    }

    fn pick_file(&self) -> Result<PickOutcome, DialogError> {
        Ok(PickOutcome::Cancelled)
    }
}

fn press(app: &mut App<FakeBackend>, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn set_field(app: &mut App<FakeBackend>, field: FormField, value: &str) {
    while app.form().unwrap().focus() != field {
        press(app, KeyCode::Tab);
    }
    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.handle_paste(value);
}

fn draw(app: &App<FakeBackend>, width: u16, height: u16) -> TestBackend {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render(frame, app)).unwrap();
    terminal.backend().clone()
}

fn screen(backend: &TestBackend) -> Vec<String> {
    let buffer = backend.buffer();
    buffer
        .area
        .rows()
        .map(|row| {
            (row.left()..row.right())
                .map(|x| buffer[(x, row.y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

fn contains(backend: &TestBackend, expected: &[&str]) {
    let rendered = screen(backend).join("\n");
    for line in expected {
        assert!(rendered.contains(line), "missing {line:?} in:\n{rendered}");
    }
}

#[test]
fn list_shows_hosts_selection_effective_values_and_source() {
    let mut backend = FakeBackend::new(HOSTS);
    backend.read_only = true;
    let app = App::new(backend);
    let view = draw(&app, 100, 20);
    contains(
        &view,
        &[
            "sshtui · 3 hosts",
            "web [ro]",
            "db [ro]",
            "queue [ro]",
            "Alias: web",
            "User: alice",
            "Address: www.example.com",
            "Port: 2222",
            "Key file: ~/.ssh/first",
            "Key file: ~/.ssh/second",
            "Source: ~/.ssh/config:2",
            "Read-only file",
            "Enter connect  n new  e edit  d delete  / filter  r reload  q quit",
        ],
    );
    assert!(view.buffer()[(0, 1)].modifier.contains(Modifier::REVERSED));
    assert!(!view.buffer()[(0, 2)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn list_shows_filter_count_and_empty_results() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    press(&mut app, KeyCode::Char('/'));
    app.handle_paste("web");
    let view = draw(&app, 80, 20);
    contains(&view, &["1 of 3 hosts", "/web"]);
    assert_eq!(view.cursor_position(), (4, 1).into());

    app.handle_paste("xxxxx");
    let view = draw(&app, 80, 20);
    contains(&view, &["0 of 3 hosts", "/webxxxxx", "No matching hosts"]);

    press(&mut app, KeyCode::Enter);
    contains(&draw(&app, 80, 20), &["/webxxxxx", "No matching hosts"]);
}

#[test]
fn list_shows_the_effective_settings_error() {
    let mut backend = FakeBackend::new(HOSTS);
    backend.effective = Err("could not resolve test host".into());
    let app = App::new(backend);
    contains(&draw(&app, 80, 20), &["could not resolve test host"]);
}

#[test]
fn new_form_shows_field_requirements_command_preview_and_help() {
    let mut app = App::new(FakeBackend::new(""));
    contains(
        &draw(&app, 100, 24),
        &[
            "New connection",
            "Nickname (optional)",
            "User (optional)",
            "Address (required)",
            "Port (optional)",
            "Key file (optional)",
            "Command: (address is required)",
            "Ctrl+L / F2: pick from ~/.ssh   Ctrl+O / F3: browse",
            "Tab/Shift+Tab move  Enter connect  Esc back",
        ],
    );
    set_field(&mut app, FormField::User, "deploy");
    set_field(&mut app, FormField::Address, "www.example.com");
    set_field(&mut app, FormField::Port, "22");
    contains(
        &draw(&app, 100, 24),
        &["Command: ssh -p 22 deploy@www.example.com"],
    );
}

#[test]
fn form_preview_uses_the_loaded_config_until_reload() {
    let mut backend = FakeBackend::new(HOSTS);
    let dir = backend.paths.ssh_dir();
    let config = backend.paths.config_file();
    backend.paths = SshPaths::resolve(Some(dir.as_os_str()), backend.paths.home(), dir).unwrap();
    let mut app = App::new(backend);
    press(&mut app, KeyCode::Char('n'));
    set_field(&mut app, FormField::Address, "www.example.com");

    fs::remove_file(&config).unwrap();
    let command = crate::ssh::args::preview(&[
        "-F".into(),
        config.into_os_string(),
        "www.example.com".into(),
    ]);
    contains(&draw(&app, 400, 24), &[&format!("Command: {command}")]);

    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('r'));
    press(&mut app, KeyCode::Char('n'));
    set_field(&mut app, FormField::Address, "www.example.com");
    contains(
        &draw(&app, 100, 24),
        &["Command: ssh -F none www.example.com"],
    );
}

#[test]
fn edit_form_shows_original_values_inheritance_and_shared_options() {
    let backend = FakeBackend::new(
        "Host web docs\n  User deploy\n  IdentityFile ~/one\n  IdentityFile ~/two\n",
    );
    let mut app = App::new(backend);
    press(&mut app, KeyCode::Char('e'));
    let view = draw(&app, 100, 24);
    contains(
        &view,
        &[
            "Edit web",
            "User:     deploy",
            "inherited: www.example.com",
            "inherited: 2222",
            "read-only",
            "  IdentityFile ~/one",
            "  IdentityFile ~/two",
            "Option changes also apply to docs. Renaming affects only web.",
            "Tab/Shift+Tab move  Enter save  Esc back",
        ],
    );
    assert!(view.buffer()[(10, 3)].modifier.contains(Modifier::DIM));
}

#[test]
fn form_shows_the_unavailability_reason_and_hidden_files_hint() {
    let mut backend = FakeBackend::new("");
    backend.unavailable = Some("Browse is unavailable in this session".into());
    backend.hidden_hint = Some("Hidden files: press Ctrl+H in the dialog".into());
    let app = App::new(backend);
    let view = draw(&app, 100, 24);
    contains(
        &view,
        &[
            "Ctrl+L / F2: pick from ~/.ssh",
            "Browse is unavailable in this session",
            "Hidden files: press Ctrl+H in the dialog",
        ],
    );
    assert!(!screen(&view).join("\n").contains("Ctrl+O / F3: browse"));
}

#[test]
fn form_shows_validation_errors_and_positions_the_cursor_in_korean_input() {
    let mut app = App::new(FakeBackend::new(""));
    set_field(&mut app, FormField::Address, "서버");
    let view = draw(&app, 100, 24);
    contains(&view, &["서", "버"]);
    assert_eq!(view.cursor_position(), (25, 3).into());

    set_field(&mut app, FormField::Port, "invalid");
    let view = draw(&app, 100, 24);
    let error = app.validation_error(FormField::Port).unwrap().to_string();
    contains(&view, &[&error]);
    let cursor = view.cursor_position();
    assert_eq!(cursor.x, 28);
    assert_eq!(view.buffer()[(0, cursor.y + 1)].fg, Color::Red);
}

#[test]
fn long_wide_input_scrolls_without_hiding_the_cursor() {
    let mut app = App::new(FakeBackend::new(""));
    let value = format!("{}끝", "가".repeat(50));
    set_field(&mut app, FormField::Address, &value);

    let view = draw(&app, 43, 20);
    let cursor = view.cursor_position();
    assert_eq!(cursor, (41, 3).into());
    assert_eq!(view.buffer()[(cursor.x - 2, cursor.y)].symbol(), "끝");

    press(&mut app, KeyCode::Home);
    let view = draw(&app, 43, 20);
    assert_eq!(view.cursor_position(), (21, 3).into());
    assert_eq!(view.buffer()[(21, 3)].symbol(), "가");
}

fn edit_confirmation() -> App<FakeBackend> {
    let mut app = App::new(FakeBackend::new(
        "Host web\n  HostName www.example.com\n  User alice\n  Port 2222\n",
    ));
    press(&mut app, KeyCode::Char('e'));
    set_field(&mut app, FormField::User, "deploy");
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode(), Mode::Confirm(_)));
    app
}

#[test]
fn confirmation_shows_numbered_colored_diff_and_scrolls_by_diff_lines() {
    let mut app = edit_confirmation();
    let path = app.backend().paths().config_file();
    let view = draw(&app, 120, 16);
    contains(
        &view,
        &[
            "Save web",
            &path.to_string_lossy(),
            "   3      -   User alice",
            "        3 +   User deploy",
            "y/Enter confirm  n/Esc cancel  Up/Down scroll",
        ],
    );
    assert_eq!(view.buffer()[(10, 4)].fg, Color::Red);
    assert_eq!(view.buffer()[(10, 5)].fg, Color::Green);

    press(&mut app, KeyCode::Down);
    let view = draw(&app, 120, 16);
    assert!(screen(&view)[2].contains("HostName"));
}

#[test]
fn key_picker_shows_labels_and_selection_over_the_form() {
    let mut app = App::new(FakeBackend::new("").with_keys());
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Down);
    assert!(matches!(app.mode(), Mode::KeyPicker(_)));
    let view = draw(&app, 100, 24);

    contains(
        &view,
        &[
            "New connection",
            "Select a key file",
            "id_a (ssh-ed25519, test key)",
            "id_b (ssh-ed25519, test key)",
            "Enter select  Esc cancel",
        ],
    );
    let selected = screen(&view)
        .iter()
        .position(|line| line.contains("id_b"))
        .unwrap() as u16;
    assert!(
        view.buffer()[(14, selected)]
            .modifier
            .contains(Modifier::REVERSED)
    );
}

fn connection_notice() -> App<FakeBackend> {
    let mut app = App::new(FakeBackend::new(""));
    set_field(&mut app, FormField::Nickname, "web");
    set_field(&mut app, FormField::Address, "www.example.com");
    set_field(&mut app, FormField::User, "deploy");
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode(), Mode::Notice(_)));
    app
}

#[test]
fn notice_shows_the_mismatch_and_the_help_for_each_response_mode() {
    let mut edited = edit_confirmation();
    press(&mut edited, KeyCode::Enter);
    assert!(matches!(edited.mode(), Mode::Notice(_)));
    for (app, help) in [
        (
            connection_notice(),
            "Enter connect anyway  Esc back to list",
        ),
        (edited, "Enter/Esc back to list"),
    ] {
        contains(
            &draw(&app, 100, 24),
            &[
                "Saved settings differ from effective SSH settings",
                "User: saved \"deploy\" but ssh will use \"alice\"",
                help,
            ],
        );
    }
}

#[test]
fn status_uses_the_penultimate_row_and_severity_colors() {
    let mut info = App::new(FakeBackend::new(HOSTS));
    press(&mut info, KeyCode::Char('r'));
    let mut backend = FakeBackend::new(HOSTS);
    backend.read_only = true;
    let mut warning = App::new(backend);
    press(&mut warning, KeyCode::Char('e'));
    let mut error = App::new(FakeBackend::new(""));
    press(&mut error, KeyCode::Enter);

    for (app, message, color) in [
        (info, "Reloaded", Color::Reset),
        (warning, "is read-only", Color::Yellow),
        (error, "address is required", Color::Red),
    ] {
        let view = draw(&app, 100, 20);
        assert!(screen(&view)[18].contains(message));
        assert_eq!(view.buffer()[(0, 18)].fg, color);
    }
}

#[test]
fn all_screens_render_in_small_terminals() {
    let mut filtered = App::new(FakeBackend::new(HOSTS));
    press(&mut filtered, KeyCode::Char('/'));
    filtered.handle_paste("서버");
    let mut picker = App::new(FakeBackend::new("").with_keys());
    press(&mut picker, KeyCode::F(2));
    let mut edited = App::new(FakeBackend::new(HOSTS));
    press(&mut edited, KeyCode::Char('e'));
    let states = [
        App::new(FakeBackend::new(HOSTS)),
        App::new(FakeBackend::new("")),
        filtered,
        edited,
        edit_confirmation(),
        picker,
        connection_notice(),
    ];
    for (width, height) in [(1, 1), (5, 2), (10, 3), (1, 5), (20, 5), (40, 10)] {
        for app in &states {
            draw(app, width, height);
        }
    }
}

#[test]
fn list_scrolls_to_the_last_selected_host() {
    let contents: String = (0..35)
        .map(|index| format!("Host host-{index:02}\n"))
        .collect();
    let mut app = App::new(FakeBackend::new(&contents));
    assert!(
        !screen(&draw(&app, 80, 10))
            .iter()
            .any(|line| line.contains("host-34"))
    );

    press(&mut app, KeyCode::End);
    let view = draw(&app, 80, 10);
    let last_host = screen(&view)
        .iter()
        .position(|line| line.starts_with("host-34"))
        .unwrap();
    assert!(
        view.buffer()[(0, last_host as u16)]
            .modifier
            .contains(Modifier::REVERSED)
    );
}
