use std::ffi::OsString;
use std::fs;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};
use tempfile::{TempDir, tempdir};
use unicode_width::UnicodeWidthStr;

use super::{ColorMode, HitMap, Theme, render};
use crate::app::{App, Backend, ClickTarget, FormField, Mode};
use crate::config::{self, ConfigSet, PendingWrite, WriteError, WriteOutcome};
use crate::dialog::{DialogError, PickOutcome};
use crate::keys::{KeyInfo, PublicKeyInfo};
use crate::paths::SshPaths;
use crate::ssh::effective::EffectiveConfig;
use crate::update::{InstallMethod, UpdatePlan, latest_url};

const HOSTS: &str = "# Saved hosts\nHost web\nHost db\nHost queue\n";
const MANY_HOSTS: &str =
    "Host web\nHost db\nHost queue\nHost cache\nHost proxy\nHost mail\nHost files\nHost git\n";

/// Backend fixture with a temporary config and predictable effective values.
pub(crate) struct FakeBackend {
    _temp: TempDir,
    paths: SshPaths,
    effective: Result<EffectiveConfig, String>,
    read_only: bool,
    keys: Vec<KeyInfo>,
    unavailable: Option<String>,
    hidden_hint: Option<String>,
    load_error: Option<String>,
    update_plan: Option<UpdatePlan>,
}

impl FakeBackend {
    /// Creates a fake session from an optional config in a temporary home.
    pub(crate) fn new(contents: &str) -> Self {
        Self::with_ssh_dir(contents, None)
    }

    fn with_ssh_dir(contents: &str, dir: Option<&str>) -> Self {
        let temp = tempdir().unwrap();
        let paths = SshPaths::resolve(
            dir.map(std::ffi::OsStr::new),
            Some(temp.path()),
            temp.path(),
        )
        .unwrap();
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
            load_error: None,
            update_plan: None,
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
    fn update_plan(&self) -> Option<UpdatePlan> {
        self.update_plan.clone()
    }

    fn paths(&self) -> &SshPaths {
        &self.paths
    }

    fn config_prefix(&self) -> Vec<OsString> {
        Vec::new()
    }

    fn load_config(&self) -> Result<ConfigSet, String> {
        if let Some(error) = &self.load_error {
            return Err(error.clone());
        }
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
    draw_with_hits(app, width, height).0
}

fn draw_with_hits(app: &App<FakeBackend>, width: u16, height: u16) -> (TestBackend, HitMap) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| hits = render(frame, app, &Theme::new(ColorMode::TrueColor)))
        .unwrap();
    (terminal.backend().clone(), hits)
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
    let app = mockup_list();
    let view = draw(&app, 100, 28);
    contains(
        &view,
        &[
            "sshtui  12 hosts",
            "  bastion         ops@bastion.example.com",
            "› prod-web-1      deploy@203.0.113.10",
            "│  prod-web-1",
            "│  deploy@203.0.113.10",
            "User       deploy",
            "Address    203.0.113.10",
            "Port       22",
            "Key file   ~/.ssh/id_ed25519",
            "Source     ~/.ssh/config:14",
            "enter runs",
            "$ ssh prod-web-1",
            " enter  connect   n new   e edit   d delete   / filter   r reload   q quit",
        ],
    );
    assert_eq!(view.buffer()[(57, 5)].modifier, Modifier::BOLD);
    assert_eq!(view.buffer()[(57, 6)].fg, MUTED);
    assert_eq!(view.buffer()[(57, 14)].fg, MUTED);
    assert_eq!(view.buffer()[(59, 15)].fg, ACCENT);
}

const BG: Color = Color::Rgb(21, 23, 28);
const TEXT: Color = Color::Rgb(216, 219, 225);
const MUTED: Color = Color::Rgb(141, 148, 160);
const ACCENT: Color = Color::Rgb(111, 211, 193);
const WARN: Color = Color::Rgb(230, 181, 102);
const ERROR: Color = Color::Rgb(240, 138, 138);
const OK: Color = Color::Rgb(156, 207, 130);
const RULE: Color = Color::Rgb(58, 64, 75);
const INPUT: Color = Color::Rgb(32, 36, 44);
const INPUT_FOCUS: Color = Color::Rgb(43, 49, 59);
const INPUT_DIM: Color = Color::Rgb(27, 30, 36);
const ROW: Color = Color::Rgb(38, 43, 52);
const DEL_BG: Color = Color::Rgb(58, 34, 38);
const ADD_BG: Color = Color::Rgb(31, 51, 38);

const MOCKUP_HOSTS: [(&str, &str, &str); 12] = [
    ("bastion", "ops", "bastion.example.com"),
    ("prod-web-1", "deploy", "203.0.113.10"),
    ("prod-web-2", "deploy", "203.0.113.11"),
    ("prod-db", "postgres", "203.0.113.20"),
    ("staging", "deploy", "staging.example.com"),
    ("ci-runner", "ci", "198.51.100.31"),
    ("gpu-box", "heonzoo", "192.0.2.44"),
    ("homelab-nas", "admin", "nas.home.arpa"),
    ("pi", "pi", "192.0.2.7"),
    ("router", "root", "192.0.2.1"),
    ("backup", "borg", "backup.example.net"),
    ("git-mirror", "git", "198.51.100.8"),
];

fn mockup_list() -> App<FakeBackend> {
    use std::fmt::Write;

    let mut config = "# Saved hosts\n".repeat(10);
    for (alias, user, address) in MOCKUP_HOSTS {
        writeln!(
            config,
            "Host {alias}\n    User {user}\n    HostName {address}"
        )
        .unwrap();
    }
    let mut backend = FakeBackend::new(&config);
    backend.effective = Ok(EffectiveConfig {
        user: Some("deploy".into()),
        host_name: Some("203.0.113.10".into()),
        port: Some("22".into()),
        identity_files: vec!["~/.ssh/id_ed25519".into()],
    });
    let mut app = App::new(backend);
    press(&mut app, KeyCode::Down);
    app
}

fn mockup_new_form() -> App<FakeBackend> {
    new_form_values(FakeBackend::new(""))
}

fn new_form_values(backend: FakeBackend) -> App<FakeBackend> {
    let mut app = App::new(backend);
    for (field, value) in [
        (FormField::Nickname, "prod-web-3"),
        (FormField::User, "deploy"),
        (FormField::Address, "203.0.113.12"),
        (FormField::KeyFile, "~/.ssh/id_ed25519"),
    ] {
        set_field(&mut app, field, value);
    }
    app
}

fn staging_backend() -> FakeBackend {
    let mut backend = FakeBackend::new(&format!(
        "{}Host staging\n    HostName staging.example.com\n    Port 22\n    IdentityFile ~/.ssh/id_rsa\n\nHost ci-runner\n",
        "# staging hosts\n".repeat(20)
    ));
    backend.effective = Ok(EffectiveConfig {
        user: Some("deploy".into()),
        ..EffectiveConfig::default()
    });
    backend
}

#[test]
fn list_shows_filter_count_and_empty_results() {
    let mut app = mockup_list();
    press(&mut app, KeyCode::Char('/'));
    app.handle_paste("PROD");
    let view = draw(&app, 100, 28);
    contains(
        &view,
        &[
            "3 of 12 hosts",
            "/ PROD",
            " enter  apply filter   ↑↓ select   esc clear filter",
        ],
    );
    assert_eq!(view.cursor_position(), (9, 3).into());
    assert_eq!(view.buffer()[(2, 3)].bg, INPUT_FOCUS);
    assert_eq!(view.buffer()[(3, 3)].fg, ACCENT);
    assert_eq!(view.buffer()[(4, 5)].fg, ACCENT);
    assert_eq!(view.buffer()[(4, 6)].fg, ACCENT);
    assert_eq!(view.buffer()[(4, 6)].modifier, Modifier::BOLD);
    assert_eq!(view.buffer()[(8, 6)].fg, TEXT);
    assert!(!screen(&view).join("\n").contains("enter runs"));

    app.handle_paste("xxxxx");
    let view = draw(&app, 100, 28);
    contains(
        &view,
        &["0 of 12 hosts", "/ PRODxxxxx", "No matching hosts"],
    );
    press(&mut app, KeyCode::Enter);
    let view = draw(&app, 100, 28);
    contains(&view, &["/ PRODxxxxx", "No matching hosts"]);
    assert_eq!(view.buffer()[(2, 26)].bg, ROW);
    assert_eq!(view.buffer()[(3, 26)].fg, MUTED);
}

#[test]
fn list_shows_all_key_files_above_the_source_and_read_only_label() {
    let mut backend = FakeBackend::new(HOSTS);
    backend.read_only = true;
    let rows = screen(&draw(&App::new(backend), 100, 28));
    let positions = [
        "│  web",
        "User       alice",
        "Address    www.example.com",
        "Port       2222",
        "Key file   ~/.ssh/first",
        "           ~/.ssh/second",
        "Source     ~/.ssh/config:2",
        "Read-only file",
    ]
    .map(|label| rows.iter().position(|row| row.contains(label)).unwrap());
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn list_shows_the_effective_settings_error() {
    let mut backend = FakeBackend::new(HOSTS);
    backend.effective = Err("could not resolve test host".into());
    let view = draw(&App::new(backend), 100, 28);
    contains(
        &view,
        &[
            "Effective settings: could not resolve",
            "│  test host",
            "Source     ~/.ssh/config:2",
        ],
    );
    assert_eq!(view.buffer()[(57, 8)].fg, ERROR);
}

#[test]
fn new_form_shows_field_requirements_command_preview_and_help() {
    let mut app = mockup_new_form();
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    for (y, field, note) in [
        (4, "Nickname", "optional · fill in to save this host"),
        (6, "User", "optional"),
        (8, "Address", "required"),
        (10, "Port", "optional"),
        (12, "Key file", "optional"),
    ] {
        assert!(rows[y].contains(field));
        assert!(rows[y].ends_with(note));
        assert!(rows[y + 1].trim().is_empty());
        assert_eq!(
            view.buffer()[(53, y as u16)].fg,
            if note == "required" { WARN } else { MUTED }
        );
    }
    contains(
        &view,
        &[
            "New connection",
            "On enter",
            "1 Save  prod-web-3 to ~/.ssh/config",
            "2 Run   $ ssh prod-web-3",
            "Leave Nickname empty to connect once without saving.",
            "enter  save & connect   tab next   ctrl+l pick key   ctrl+o browse   esc back",
        ],
    );
    assert_eq!(view.buffer()[(12, 16)].modifier, Modifier::BOLD);
    set_field(&mut app, FormField::Nickname, "");
    set_field(&mut app, FormField::KeyFile, "");
    set_field(&mut app, FormField::Port, "2222");
    contains(
        &draw(&app, 100, 28),
        &[
            "1 Run   $ ssh -p 2222 deploy@203.0.113.12",
            "Fill in Nickname to save this host to ~/.ssh/config.",
            "enter  connect   tab next   ctrl+u clear field   esc back",
        ],
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
    contains(&draw(&app, 400, 28), &["On enter", &command]);

    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('r'));
    press(&mut app, KeyCode::Char('n'));
    set_field(&mut app, FormField::Address, "www.example.com");
    contains(
        &draw(&app, 100, 28),
        &["On enter", "ssh -F none www.example.com"],
    );
}

#[test]
fn edit_form_shows_original_values_inheritance_and_shared_options() {
    let backend = FakeBackend::new(
        "Host web docs\n  User deploy\n  IdentityFile ~/one\n  IdentityFile ~/two\n",
    );
    let mut app = App::new(backend);
    press(&mut app, KeyCode::Char('e'));
    let view = draw(&app, 100, 28);
    contains(
        &view,
        &[
            "Edit web",
            "~/.ssh/config:1",
            "inherited: www.example.com",
            "inherited: 2222",
            "read-only",
            "  IdentityFile ~/one",
            "  IdentityFile ~/two",
            "Option changes also apply to docs. Renaming affects only web.",
            "1 Review changes to ~/.ssh/config",
            "enter  review changes   tab next   ctrl+u clear field   esc back",
        ],
    );
    let buffer = view.buffer();
    assert_eq!(buffer[(16, 6)].symbol(), "d");
    assert_eq!(buffer[(16, 8)].fg, MUTED);
    assert_eq!(buffer[(53, 4)].fg, WARN);
    assert_eq!(buffer[(53, 8)].fg, MUTED);
    assert_eq!(buffer[(53, 12)].fg, WARN);
    assert_eq!(buffer[(15, 12)].bg, BG);
    assert_eq!(buffer[(2, 19)].fg, WARN);
}

#[test]
fn form_shows_the_unavailability_reason_and_hidden_files_hint() {
    let mut backend = FakeBackend::new("");
    backend.unavailable = Some("Browse is unavailable in this session".into());
    backend.hidden_hint = Some("Hidden files: press Ctrl+H in the dialog".into());
    let mut app = App::new(backend);
    let normal = screen(&draw(&app, 100, 28)).join("\n");
    assert!(!normal.contains("Browse is unavailable"));
    assert!(!normal.contains("Hidden files:"));
    set_field(&mut app, FormField::KeyFile, "");
    let view = draw(&app, 100, 28);
    contains(
        &view,
        &[
            "ctrl+l pick key",
            "ctrl+o browse",
            "Browse is unavailable in this session",
            "Hidden files: press Ctrl+H in the dialog",
        ],
    );
    let browse = screen(&view)[26].find("ctrl+o").unwrap() as u16;
    assert_eq!(view.buffer()[(browse, 26)].fg, MUTED);
    assert_eq!(view.buffer()[(browse, 26)].modifier, Modifier::BOLD);
}

#[test]
fn form_shows_validation_errors_and_positions_the_cursor_in_korean_input() {
    let mut app = mockup_new_form();
    set_field(&mut app, FormField::Address, "서버");
    let view = draw(&app, 100, 28);
    assert_eq!(view.buffer()[(16, 8)].symbol(), "서");
    assert_eq!(view.buffer()[(18, 8)].symbol(), "버");
    assert_eq!(view.cursor_position(), (20, 8).into());

    set_field(&mut app, FormField::Port, "invalid");
    let view = draw(&app, 100, 28);
    contains(
        &view,
        &[
            "Port must be a number from 1 to 65535",
            "Fix Port to continue",
        ],
    );
    assert_eq!(view.cursor_position(), (23, 10).into());
    assert_eq!(view.buffer()[(15, 11)].fg, ERROR);
}

#[test]
fn long_wide_input_scrolls_without_hiding_the_cursor() {
    let mut app = App::new(FakeBackend::new(""));
    let value = format!("{}끝", "가".repeat(50));
    set_field(&mut app, FormField::Address, &value);

    let view = draw(&app, 43, 20);
    let cursor = view.cursor_position();
    assert_eq!(cursor, (40, 8).into());
    assert_eq!(view.buffer()[(cursor.x - 2, cursor.y)].symbol(), "끝");

    press(&mut app, KeyCode::Home);
    let view = draw(&app, 43, 20);
    assert_eq!(view.cursor_position(), (14, 8).into());
    assert_eq!(view.buffer()[(14, 8)].symbol(), "가");
}

fn edit_confirmation() -> App<FakeBackend> {
    confirm_edit(FakeBackend::new(
        "Host web\n  HostName www.example.com\n  User alice\n  Port 2222\n",
    ))
}

fn confirm_edit(backend: FakeBackend) -> App<FakeBackend> {
    let mut app = App::new(backend);
    press(&mut app, KeyCode::Char('e'));
    set_field(&mut app, FormField::User, "deploy");
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode(), Mode::Confirm(_)));
    app
}

fn mockup_review() -> App<FakeBackend> {
    let mut app = App::new(staging_backend());
    press(&mut app, KeyCode::Char('e'));
    set_field(&mut app, FormField::Port, "2222");
    set_field(&mut app, FormField::KeyFile, "~/.ssh/work_ed25519");
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode(), Mode::Confirm(_)));
    app
}

#[test]
fn confirmation_shows_numbered_colored_diff_and_scrolls_by_diff_lines() {
    let mut app = mockup_review();
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    assert!(rows[1].starts_with("  sshtui  Save staging"));
    assert!(rows[1].ends_with("~/.ssh/config"));
    assert!(rows[3].ends_with("2 lines will change  -2  +2"));
    assert!(rows[5].ends_with("   21   21    Host staging"));
    assert!(rows[7].ends_with("   23       -     Port 22"));
    assert!(
        rows[8].ends_with("        23  +     Port 2222"),
        "{rows:#?}"
    );
    assert!(rows[9].ends_with("   24       -     IdentityFile ~/.ssh/id_rsa"));
    assert!(rows[10].ends_with("        24  +     IdentityFile ~/.ssh/work_ed25519"));
    assert!(rows[24].ends_with("The current file is kept as ~/.ssh/.config.sshtui.bak"));
    assert!(rows[26].ends_with(" y  save   n back to form   ↑↓ scroll"));
    let buffer = view.buffer();
    assert_eq!(buffer[(23, 3)].fg, ERROR);
    assert_eq!(buffer[(27, 3)].fg, OK);
    for x in 2..98 {
        assert_eq!((buffer[(x, 7)].fg, buffer[(x, 7)].bg), (ERROR, DEL_BG));
        assert_eq!((buffer[(x, 8)].fg, buffer[(x, 8)].bg), (OK, ADD_BG));
        assert_eq!(buffer[(x, 5)].fg, MUTED);
    }
    assert_eq!(buffer[(14, 7)].modifier, Modifier::BOLD);
    press(&mut app, KeyCode::Down);
    let view = draw(&app, 100, 28);
    assert!(screen(&view)[5].contains("HostName"));
}

#[test]
fn confirmation_abbreviates_a_path_under_the_home_directory() {
    let app = confirm_edit(FakeBackend::with_ssh_dir(HOSTS, Some("work-ssh")));
    let rows = screen(&draw(&app, 80, 16));
    assert!(rows[0].ends_with("~/work-ssh/config"));
    assert!(rows[13].ends_with("~/work-ssh/.config.sshtui.bak"));
}

#[test]
fn confirmation_separates_hunks_when_edits_are_far_apart() {
    let config = format!(
        "Host staging\n    User deploy\n{}    Port 22\n",
        "    # keep\n".repeat(8)
    );
    let mut app = App::new(FakeBackend::new(&config));
    press(&mut app, KeyCode::Char('e'));
    set_field(&mut app, FormField::User, "ops");
    set_field(&mut app, FormField::Port, "2222");
    press(&mut app, KeyCode::Enter);
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    let separator = rows.iter().position(|line| line.trim() == "⋯").unwrap();
    assert!(
        rows[..separator]
            .iter()
            .any(|line| line.ends_with("User ops"))
    );
    assert!(
        rows[separator..]
            .iter()
            .any(|line| line.ends_with("Port 2222"))
    );
    assert_eq!(view.buffer()[(2, separator as u16)].fg, MUTED);
}

#[test]
fn confirmation_clips_the_front_of_a_long_path_on_the_header_row() {
    let dir = format!("{}한{}", "a".repeat(27), "b".repeat(16));
    let app = confirm_edit(FakeBackend::with_ssh_dir(HOSTS, Some(&dir)));
    let view = draw(&app, 42, 16);
    let rows = screen(&view);
    assert_eq!(
        rows[0],
        format!("sshtui  Save web   {}/config", "b".repeat(16))
    );
    assert_eq!(rows[1], "─".repeat(42));
    assert!(rows[4].contains("# Saved hosts"), "{rows:?}");

    let wider = draw(&app, 43, 16);
    assert_eq!(wider.buffer()[(18, 0)].symbol(), "한");
    assert_eq!(wider.buffer()[(20, 0)].symbol(), "b");
}

#[test]
fn key_picker_shows_labels_and_selection_over_the_form() {
    let mut backend = FakeBackend::new("");
    backend.keys = ["id_a", "id_ed25519", "id_rsa", "work_ed25519"]
        .into_iter()
        .map(|name| KeyInfo {
            path: backend.paths.ssh_dir().join(name),
            file_name: name.into(),
            public: (name == "id_ed25519").then(|| PublicKeyInfo {
                key_type: "ssh-ed25519".into(),
                comment: Some("me@host".into()),
            }),
        })
        .collect();
    let mut app = new_form_values(backend);
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Down);
    assert!(matches!(app.mode(), Mode::KeyPicker(_)));
    let view = draw(&app, 100, 28);
    contains(
        &view,
        &[
            "New connection",
            "Select a key file",
            "4 keys in ~/.ssh",
            "~/.ssh/id_a",
            "› ~/.ssh/id_ed25519",
            "ssh-ed25519 me@host",
            "~/.ssh/id_rsa",
            "~/.ssh/work_ed25519",
            "enter  use this key   ↑↓ select   esc cancel",
        ],
    );
    let rows = screen(&view);
    assert!(!rows.join("\n").contains("optional"));
    assert!(!rows.join("\n").contains("On enter"));
    let buffer = view.buffer();
    assert_eq!(buffer[(2, 1)].fg, MUTED);
    assert_eq!(buffer[(15, 4)].bg, INPUT_DIM);
    assert_eq!(buffer[(15, 12)].bg, INPUT_DIM);
    for (pos, symbol) in [
        ((20, 8), "┌"),
        ((79, 8), "┐"),
        ((20, 15), "└"),
        ((79, 15), "┘"),
    ] {
        assert_eq!(buffer[pos].symbol(), symbol);
        assert_eq!(buffer[pos].fg, TEXT);
    }
    assert_eq!(buffer[(23, 12)].bg, ROW);
    assert_eq!(buffer[(23, 12)].fg, ACCENT);
    assert_eq!(buffer[(25, 12)].modifier, Modifier::BOLD);
    assert_eq!(buffer[(76, 12)].fg, MUTED);
}

#[test]
fn key_picker_uses_singular_count_and_keeps_the_scrolled_selection_visible() {
    for count in [1, 30] {
        let mut backend = FakeBackend::new("");
        backend.keys = (0..count)
            .map(|index| {
                let name = format!("key-{index:02}");
                KeyInfo {
                    path: backend.paths.ssh_dir().join(&name),
                    file_name: name,
                    public: None,
                }
            })
            .collect();
        let mut app = App::new(backend);
        press(&mut app, KeyCode::F(2));
        for _ in 1..count {
            press(&mut app, KeyCode::Down);
        }
        let (view, hits) = draw_with_hits(&app, 100, 28);
        let selected = format!("› ~/.ssh/key-{:02}", count - 1);
        let heading = if count == 1 {
            "1 key in ~/.ssh"
        } else {
            "30 keys in ~/.ssh"
        };
        contains(&view, &[heading, &selected]);
        if count > 1 {
            assert!(!screen(&view).join("\n").contains("~/.ssh/key-00"));
            assert_eq!(popup_corner(&view, "┌").1, 2);
        }
        assert_text_hit(
            &view,
            &hits,
            &selected,
            Some(ClickTarget::KeyRow(count - 1)),
        );
    }
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
            "enter  connect anyway   esc back to list",
        ),
        (edited, "enter  ok"),
    ] {
        let view = draw(&app, 100, 28);
        contains(
            &view,
            &[
                "Saved settings differ from effective SSH settings",
                "User: saved \"deploy\" but ssh will use \"alice\"",
                help,
            ],
        );
        assert_eq!(view.buffer()[(23, 11)].fg, WARN);
        assert_eq!(view.buffer()[(2, 1)].fg, MUTED);
        assert_eq!(view.buffer()[(2, 5)].bg, BG);
        assert!(!screen(&view).join("\n").contains("enter runs"));
    }
}

#[test]
fn status_sits_above_the_footer_rule_and_uses_severity_colors() {
    let mut info = App::new(FakeBackend::new(HOSTS));
    press(&mut info, KeyCode::Char('r'));
    let mut backend = FakeBackend::new(HOSTS);
    backend.read_only = true;
    let mut warning = App::new(backend);
    press(&mut warning, KeyCode::Char('e'));
    let mut error = App::new(FakeBackend::new(""));
    press(&mut error, KeyCode::Enter);

    for (app, message, color) in [
        (info, "Reloaded", OK),
        (warning, "is read-only", WARN),
        (error, "address is required", ERROR),
    ] {
        let view = draw(&app, 100, 28);
        assert!(screen(&view)[24].contains(message));
        assert_eq!(view.buffer()[(2, 24)].fg, color);
        assert_eq!(view.buffer()[(2, 25)].symbol(), "─");
    }
}

#[test]
fn status_wraps_a_long_unbroken_path_at_width_forty() {
    let path = format!("/{}", "x".repeat(48));
    let message = format!("{path} was changed by another program");
    assert_eq!(message.len(), 80);
    let mut backend = FakeBackend::new("");
    backend.load_error = Some(message);
    let app = App::new(backend);
    let rows = screen(&draw(&app, 40, 16));
    assert_eq!(rows[12], path[..40]);
    assert!(
        rows.join(" ").contains("was changed by another program"),
        "{rows:?}"
    );
    assert!(rows[13].ends_with("another program"));

    let narrower = screen(&draw(&app, 39, 16));
    assert!(narrower[12].ends_with("another"));
    assert_eq!(narrower[13], "program");
}

#[test]
fn list_gives_up_one_host_row_for_short_status() {
    let mut app = App::new(FakeBackend::new(MANY_HOSTS));
    let hosts = ["web", "db", "queue", "cache", "proxy"];
    let rows = screen(&draw(&app, 80, 11));
    for (row, host) in rows[4..9].iter().zip(hosts) {
        assert!(row[..20].contains(host), "{row}");
    }
    assert!(!rows.iter().any(|row| row.starts_with("    mail")));
    press(&mut app, KeyCode::Char('r'));
    let rows = screen(&draw(&app, 80, 11));
    for (row, host) in rows[4..8].iter().zip(hosts) {
        assert!(row[..20].contains(host), "{row}");
    }
    assert_eq!(rows[8], "  Reloaded");
    assert!(!rows.iter().any(|row| row.starts_with("    proxy")));
}

fn stale_edit() -> App<FakeBackend> {
    let dir = "x".repeat(150);
    let mut app = confirm_edit(FakeBackend::with_ssh_dir(MANY_HOSTS, Some(&dir)));
    fs::write(
        app.backend().paths().config_file(),
        format!("# externally changed\n{MANY_HOSTS}"),
    )
    .unwrap();
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode(), Mode::List));
    assert!(app.status().unwrap().text.ends_with("reloaded, try again"));
    app
}

#[test]
fn status_uses_at_most_three_rows_without_overwriting_body_or_help() {
    let app = stale_edit();
    let view = draw(&app, 40, 13);
    let rows = screen(&view);
    assert!(rows[4].starts_with("› web"));
    assert!(rows[5].starts_with("  db"));
    assert!(rows[6].starts_with("  queue"));
    for y in 8..11 {
        assert!(!rows[usize::from(y)].is_empty());
        assert_eq!(view.buffer()[(0, y)].fg, ERROR);
    }
    assert!(!rows.join(" ").contains("reloaded, try again"));
    assert!(rows[12].starts_with(" enter  connect"));
}

#[test]
fn long_status_renders_in_small_terminals() {
    let app = stale_edit();
    for (width, height) in [(1, 1), (5, 2), (20, 5)] {
        let rows = screen(&draw(&app, width, height));
        assert!(rows[0].starts_with(&"sshtui"[..usize::from(width.min(6))]));
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
        .position(|line| line.starts_with("  › host-34"))
        .unwrap();
    assert_eq!(view.buffer()[(2, last_host as u16)].bg, ROW);
}

fn update_fixture(method: InstallMethod) -> App<FakeBackend> {
    let mut backend = FakeBackend::new(HOSTS);
    backend.update_plan = Some(UpdatePlan {
        current: "1.0.0".into(),
        latest: "2.0.0".into(),
        method,
    });
    App::new(backend)
}

#[test]
fn update_notice_appears_only_with_a_plan() {
    let notice = "update available u";
    let app = update_fixture(InstallMethod::Brew);
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    assert!(rows[1].starts_with("  sshtui  3 hosts"));
    assert!(rows[1].ends_with(notice));
    assert!(rows[26].ends_with("r reload   q quit"));

    let start = 98 - notice.len() as u16;
    let buffer = view.buffer();
    assert_eq!(buffer[(start - 1, 1)].fg, TEXT);
    for x in start..98 {
        assert_eq!(buffer[(x, 1)].fg, WARN);
        assert_eq!(buffer[(x, 1)].modifier.contains(Modifier::BOLD), x == 97);
    }

    let app = App::new(FakeBackend::new(HOSTS));
    assert!(!screen(&draw(&app, 100, 28))[1].contains(notice));
}

#[test]
fn update_notice_is_hidden_when_it_would_not_fit() {
    let app = update_fixture(InstallMethod::Brew);
    let rows = screen(&draw(&app, 30, 10));
    assert_eq!(rows[0], "sshtui  3 hosts");
}

#[test]
fn update_modal_shows_versions_installer_commands_and_confirmation() {
    let mut app = update_fixture(InstallMethod::Cargo);
    press(&mut app, KeyCode::Char('u'));
    let view = draw(&app, 100, 28);
    contains(
        &view,
        &[
            "Update sshtui",
            "1.0.0 → 2.0.0",
            "Installed via  cargo",
            "Command        $ cargo install --git",
            crate::update::REPOSITORY,
            "--tag v2.0.0 --locked",
            "enter  update and exit   esc cancel",
        ],
    );
    assert_eq!(view.buffer()[(38, 12)].fg, MUTED);
    assert_eq!(view.buffer()[(40, 12)].fg, ACCENT);
    let wrapped = screen(&draw(&app, 45, 20));
    assert!(wrapped.iter().any(|row| row.contains("cargo install")));
    assert!(
        wrapped
            .iter()
            .any(|row| row.contains("--locked") && !row.contains("cargo install"))
    );
}

#[test]
fn update_modal_shows_manual_download_for_unknown_installations() {
    let mut app = update_fixture(InstallMethod::Unknown);
    press(&mut app, KeyCode::Char('u'));
    let view = draw(&app, 100, 28);
    let release = latest_url();
    let (first, rest) = release.split_at(39);
    contains(
        &view,
        &[
            "Update sshtui",
            "Installed via  unknown (manual install)",
            &format!("Download       {first}"),
            rest,
            "esc  close",
        ],
    );
    assert!(!screen(&view).join("\n").contains("enter  update"));
}

#[test]
fn update_modal_renders_in_tiny_terminals() {
    let mut app = update_fixture(InstallMethod::Brew);
    press(&mut app, KeyCode::Char('u'));
    for (width, height) in [(1, 1), (5, 2), (20, 5)] {
        draw(&app, width, height);
    }
}

#[test]
fn list_rules_and_detail_values_share_the_mockup_columns() {
    let view = draw(&mockup_list(), 100, 28);
    let buffer = view.buffer();
    for x in 2..98 {
        assert_eq!(buffer[(x, 3)].bg, INPUT);
        for y in [4, 25] {
            assert_eq!(buffer[(x, y)].symbol(), "─");
            assert_eq!(buffer[(x, y)].fg, RULE);
        }
    }
    for y in 5..25 {
        assert_eq!(buffer[(54, y)].symbol(), "│");
        assert_eq!(buffer[(54, y)].fg, RULE);
    }
    let rows = screen(&view);
    assert!(rows[3].ends_with("/ filter hosts"));
    for (y, label, value) in [
        (8, "User", "deploy"),
        (9, "Address", "203.0.113.10"),
        (10, "Port", "22"),
        (11, "Key file", "~/.ssh/id_ed25519"),
        (12, "Source", "~/.ssh/config:14"),
    ] {
        let details = &rows[y].split_once("│  ").unwrap().1;
        assert_eq!(details[..11].trim(), label);
        assert_eq!(&details[11..], value);
        assert_eq!(buffer[(57, y as u16)].fg, MUTED);
        assert_eq!(buffer[(68, y as u16)].fg, TEXT);
    }
}

#[test]
fn list_selection_styles_the_entire_inner_row() {
    let view = draw(&mockup_list(), 100, 28);
    let buffer = view.buffer();
    for x in 2..54 {
        assert_eq!(buffer[(x, 6)].bg, ROW);
        assert_eq!(buffer[(x, 5)].bg, BG);
    }
    assert_eq!(buffer[(1, 6)].bg, BG);
    assert_eq!(buffer[(54, 6)].bg, BG);
    assert_eq!(buffer[(2, 6)].symbol(), "›");
    assert_eq!(buffer[(2, 6)].fg, ACCENT);
    assert_eq!(buffer[(4, 6)].symbol(), "p");
    assert_eq!(buffer[(4, 6)].modifier, Modifier::BOLD);
    assert_eq!(buffer[(20, 6)].symbol(), "d");
    assert_eq!(buffer[(20, 6)].fg, TEXT);
    assert_eq!(buffer[(20, 5)].symbol(), "o");
    assert_eq!(buffer[(20, 5)].fg, MUTED);
}

#[test]
fn list_shows_details_starting_at_sixty_four_columns() {
    let app = mockup_list();
    let narrow = draw(&app, 63, 17);
    assert_eq!(narrow.buffer()[(0, 0)].symbol(), "s");
    assert_eq!(narrow.buffer()[(0, 5)].symbol(), "›");
    let rows = screen(&narrow).join("\n");
    assert!(!rows.contains("Address"));
    assert!(!rows.contains('│'));

    let wide = draw(&app, 64, 18);
    assert_eq!(wide.buffer()[(2, 1)].symbol(), "s");
    contains(&wide, &["│  prod-web-1", "Address    203.0.113.10"]);
}

#[test]
fn list_read_only_marker_is_warn_colored_and_right_aligned() {
    let mut backend = FakeBackend::new(
        "Host backup\n    User borg\n    HostName backup.example.net\nHost git-mirror\n",
    );
    backend.read_only = true;
    let view = draw(&App::new(backend), 100, 28);
    let buffer = view.buffer();
    for (x, letter) in (44..53).zip("read-only".chars()) {
        for y in [5, 6] {
            assert_eq!(buffer[(x, y)].symbol(), letter.to_string());
            assert_eq!(buffer[(x, y)].fg, WARN);
        }
        assert_eq!(buffer[(x, 5)].bg, ROW);
    }
    assert_eq!(buffer[(53, 6)].symbol(), " ");
    assert_eq!(buffer[(54, 6)].symbol(), "│");
}

#[test]
fn header_styles_the_product_name_and_title_independently() {
    let view = draw(&mockup_list(), 100, 28);
    assert_eq!(screen(&view)[1], "  sshtui  12 hosts");
    for x in 2..8 {
        let product = &view.buffer()[(x, 1)];
        assert_eq!((product.fg, product.bg), (ACCENT, BG));
        assert_eq!(product.modifier, Modifier::BOLD);
    }
    for x in 10..18 {
        let title = &view.buffer()[(x, 1)];
        assert_eq!((title.fg, title.bg), (MUTED, BG));
        assert!(!title.modifier.contains(Modifier::BOLD));
    }
}

#[test]
fn help_styles_keys_and_descriptions_independently() {
    let view = draw(&mockup_list(), 100, 28);
    let buffer = view.buffer();
    for x in 2..9 {
        let key = &buffer[(x, 26)];
        assert_eq!((key.fg, key.bg), (BG, ACCENT));
        assert_eq!(key.modifier, Modifier::BOLD);
    }
    for x in 10..17 {
        let description = &buffer[(x, 26)];
        assert_eq!(description.fg, TEXT);
        assert!(!description.modifier.contains(Modifier::BOLD));
    }
    for x in 17..20 {
        assert_eq!(buffer[(x, 26)].symbol(), " ");
        assert_eq!(buffer[(x, 26)].bg, BG);
    }
    assert_eq!(buffer[(20, 26)].symbol(), "n");
    assert_eq!(buffer[(20, 26)].fg, TEXT);
    assert_eq!(buffer[(20, 26)].modifier, Modifier::BOLD);
    assert_eq!(buffer[(22, 26)].fg, MUTED);
}

#[test]
fn form_highlights_the_focused_label_input_and_padding() {
    let view = draw(&mockup_new_form(), 100, 28);
    let buffer = view.buffer();
    for x in 15..51 {
        assert_eq!(buffer[(x, 12)].bg, INPUT_FOCUS);
        assert_eq!(buffer[(x, 10)].bg, INPUT);
    }
    assert_eq!(buffer[(2, 12)].symbol(), "›");
    assert_eq!(buffer[(2, 12)].fg, ACCENT);
    assert_eq!(buffer[(4, 12)].fg, ACCENT);
    assert_eq!(buffer[(4, 12)].modifier, Modifier::BOLD);
    assert_eq!(buffer[(4, 10)].fg, MUTED);
    assert_eq!(buffer[(15, 12)].symbol(), " ");
    assert_eq!(buffer[(16, 12)].symbol(), "~");
    assert_eq!(buffer[(16, 10)].symbol(), "2");
    assert_eq!(buffer[(50, 12)].symbol(), " ");
    assert_eq!(buffer[(51, 12)].bg, BG);
    assert_eq!(view.cursor_position(), (33, 12).into());
}

#[test]
fn form_places_nonempty_field_errors_directly_below_the_input() {
    let mut app = App::new(staging_backend());
    press(&mut app, KeyCode::Char('e'));
    set_field(&mut app, FormField::Port, "70000");
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    assert!(rows[1].starts_with("  sshtui  Edit staging"));
    assert!(rows[1].ends_with("~/.ssh/config:21"));
    assert!(rows[10].contains("› Port        70000"));
    assert_eq!(
        rows[11],
        "               Port must be a number from 1 to 65535"
    );
    assert!(rows[12].is_empty());
    assert!(rows[13].contains("Key file"));
    assert!(rows[17].contains("Fix Port to continue"));
    let buffer = view.buffer();
    assert_eq!(buffer[(2, 10)].fg, ERROR);
    assert_eq!(buffer[(4, 10)].fg, ERROR);
    assert_eq!(buffer[(4, 10)].modifier, Modifier::BOLD);
    assert_eq!(buffer[(15, 10)].bg, INPUT_FOCUS);
    assert_eq!(buffer[(15, 11)].fg, ERROR);
    assert_eq!(buffer[(53, 8)].fg, WARN);
    assert_eq!((buffer[(3, 26)].bg, buffer[(3, 26)].fg), (ROW, MUTED));
    assert!(rows[26].contains("enter  review changes"));
}

#[test]
fn form_hints_reflect_the_action_and_focused_field() {
    let mut app = mockup_new_form();
    let view = draw(&app, 100, 28);
    let footer = &screen(&view)[26];
    assert!(
        footer.contains(
            "enter  save & connect   tab next   ctrl+l pick key   ctrl+o browse   esc back"
        )
    );
    assert!(!footer.contains("ctrl+u"));
    assert_eq!(view.buffer()[(2, 26)].bg, ACCENT);
    press(&mut app, KeyCode::BackTab);
    let view = draw(&app, 100, 28);
    let footer = &screen(&view)[26];
    assert!(footer.contains("tab next   ctrl+u clear field   esc back"));
    assert!(!footer.contains("ctrl+l") && !footer.contains("ctrl+o"));
}

#[test]
fn form_guidance_styles_missing_fields_and_live_commands() {
    let mut app = App::new(FakeBackend::new(""));
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    assert!(rows[9].is_empty());
    assert!(rows[16].contains("Fill in Address to continue"));
    assert_eq!(view.buffer()[(4, 16)].fg, ERROR);
    assert_eq!(rows[14].trim_start(), "─".repeat(96));
    set_field(&mut app, FormField::Address, "www.example.com");
    let view = draw(&app, 100, 28);
    assert!(screen(&view)[16].ends_with("1 Run   $ ssh www.example.com"));
    assert_eq!(view.buffer()[(12, 16)].fg, MUTED);
    assert_eq!(view.buffer()[(14, 16)].fg, ACCENT);
}

#[test]
fn confirmation_puts_key_hints_below_the_diff_for_edit_and_delete() {
    let view = draw(&edit_confirmation(), 100, 28);
    contains(
        &view,
        &[
            "1 line will change  -1  +1",
            "y  save   n back to form   ↑↓ scroll",
        ],
    );
    let buffer = view.buffer();
    assert_eq!(buffer[(85, 1)].fg, MUTED);
    assert_eq!(buffer[(2, 25)].fg, RULE);
    assert_eq!(buffer[(2, 26)].bg, ACCENT);
    assert_eq!(buffer[(11, 26)].bg, BG);

    let mut app = mockup_list();
    press(&mut app, KeyCode::Char('d'));
    let view = draw(&app, 100, 28);
    contains(
        &view,
        &[
            "Delete prod-web-1",
            "3 lines will change  -3",
            "y  delete   n back to list   ↑↓ scroll",
        ],
    );
    assert!(!screen(&view)[3].contains('+'));
}

fn popup_over_fill(app: &App<FakeBackend>, fill: &str) -> TestBackend {
    use ratatui::layout::Rect;
    use ratatui::text::Span;

    let mut terminal = Terminal::new(TestBackend::new(100, 28)).unwrap();
    let theme = Theme::new(ColorMode::TrueColor);
    terminal
        .draw(|frame| {
            let body = Rect::new(2, 2, 96, 23);
            for row in body.rows() {
                frame.render_widget(Span::raw(fill.repeat(96)), row);
            }
            let mut hits = HitMap::default();
            match app.mode() {
                Mode::KeyPicker(state) => super::overlays::key_picker(
                    frame,
                    state,
                    app.backend().paths(),
                    body,
                    &theme,
                    &mut hits,
                ),
                Mode::Notice(state) => {
                    super::overlays::notice(frame, state, body, &theme, &mut hits)
                }
                Mode::Update(state) => {
                    super::overlays::update(frame, state, body, &theme, &mut hits)
                }
                _ => unreachable!("expected a popup"),
            };
        })
        .unwrap();
    terminal.backend().clone()
}

fn popup_corner(view: &TestBackend, symbol: &str) -> (u16, u16) {
    let buffer = view.buffer();
    buffer
        .area
        .rows()
        .flat_map(|row| (row.left()..row.right()).map(move |x| (x, row.y)))
        .find(|&position| buffer[position].symbol() == symbol)
        .unwrap()
}

#[test]
fn popups_show_their_actions_and_clear_the_area_underneath() {
    let mut picker = App::new(FakeBackend::new("").with_keys());
    press(&mut picker, KeyCode::F(2));
    let mut acknowledgment = edit_confirmation();
    press(&mut acknowledgment, KeyCode::Enter);
    let mut update = update_fixture(InstallMethod::Brew);
    press(&mut update, KeyCode::Char('u'));
    let mut manual = update_fixture(InstallMethod::Unknown);
    press(&mut manual, KeyCode::Char('u'));

    for (app, actions) in [
        (picker, "enter  use this key   ↑↓ select   esc cancel"),
        (
            connection_notice(),
            "enter  connect anyway   esc back to list",
        ),
        (acknowledgment, "enter  ok"),
        (update, "enter  update and exit   esc cancel"),
        (manual, "esc  close"),
    ] {
        contains(&draw(&app, 100, 28), &[actions]);
        let clean = popup_over_fill(&app, " ");
        let dirty = popup_over_fill(&app, "#");
        let (left, top) = popup_corner(&clean, "┌");
        let (right, bottom) = popup_corner(&clean, "┘");
        assert_eq!(dirty.buffer()[(left - 1, top)].symbol(), "#");
        assert_eq!(dirty.buffer()[(left, top)].fg, TEXT);
        assert_eq!(dirty.buffer()[(left + 3, top + 1)].modifier, Modifier::BOLD);
        for y in top..=bottom {
            for x in left..=right {
                assert_eq!(dirty.buffer()[(x, y)], clean.buffer()[(x, y)]);
            }
        }
    }
}

#[test]
fn status_styles_do_not_bleed_into_adjacent_screen_rows() {
    let mut info = App::new(FakeBackend::new(HOSTS));
    press(&mut info, KeyCode::Char('r'));
    let mut warning = App::new(FakeBackend::new(""));
    press(&mut warning, KeyCode::F(2));
    let mut error = App::new(FakeBackend::new(""));
    press(&mut error, KeyCode::Enter);
    for (app, color) in [(info, OK), (warning, WARN), (error, ERROR)] {
        let view = draw(&app, 200, 24);
        assert_eq!(view.buffer()[(2, 20)].fg, color);
        assert_eq!(view.buffer()[(2, 21)].fg, RULE);
        assert_eq!(view.buffer()[(197, 22)].fg, TEXT);
    }
}

#[test]
fn all_screen_modes_draw_at_the_required_terminal_sizes() {
    let mut edited = App::new(FakeBackend::new(HOSTS));
    press(&mut edited, KeyCode::Char('e'));
    let mut filter = mockup_list();
    press(&mut filter, KeyCode::Char('/'));
    filter.handle_paste("prod");
    let mut delete = App::new(FakeBackend::new(HOSTS));
    press(&mut delete, KeyCode::Char('d'));
    let mut picker = App::new(FakeBackend::new("").with_keys());
    press(&mut picker, KeyCode::F(2));
    let mut update = update_fixture(InstallMethod::Brew);
    press(&mut update, KeyCode::Char('u'));
    let mut manual = update_fixture(InstallMethod::Unknown);
    press(&mut manual, KeyCode::Char('u'));
    let mut acknowledgment = edit_confirmation();
    press(&mut acknowledgment, KeyCode::Enter);
    let states = [
        mockup_list(),
        mockup_new_form(),
        filter,
        edited,
        edit_confirmation(),
        delete,
        picker,
        connection_notice(),
        acknowledgment,
        update,
        manual,
    ];
    for (width, height) in [
        (1, 1),
        (5, 2),
        (20, 5),
        (40, 10),
        (63, 17),
        (64, 18),
        (80, 24),
        (100, 28),
        (200, 60),
    ] {
        for app in &states {
            assert_eq!(
                draw(app, width, height).buffer().area,
                ratatui::layout::Rect::new(0, 0, width, height)
            );
        }
    }
}

#[test]
fn short_forms_keep_the_fields_and_cursor_before_guidance() {
    let app = mockup_new_form();
    for (height, expected) in [
        (28, [true, true, true]),
        (22, [true, true, false]),
        (17, [true, false, false]),
        (11, [false, false, false]),
        (8, [false, false, false]),
    ] {
        let view = draw(&app, 100, height);
        let rows = screen(&view);
        let visible = rows.join("\n");
        for (element, shown) in ["On enter", "2 Run", "Leave Nickname empty"]
            .into_iter()
            .zip(expected)
        {
            assert_eq!(
                visible.contains(element),
                shown,
                "height {height}, element {element}: {rows:?}"
            );
        }
        let cursor = view.cursor_position();
        assert!(rows[usize::from(cursor.y)].contains("Key file"));
        assert_eq!(view.buffer()[(cursor.x, cursor.y)].bg, INPUT_FOCUS);
        assert!(visible.contains("ctrl+l pick key"));
    }
}

#[test]
fn inherited_form_input_uses_muted_text_and_a_focused_background() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    press(&mut app, KeyCode::Char('e'));
    press(&mut app, KeyCode::Tab);
    let view = draw(&app, 100, 28);
    let input = &view.buffer()[(16, 6)];
    assert_eq!(input.symbol(), "i");
    assert_eq!(input.fg, MUTED);
    assert_eq!(input.bg, INPUT_FOCUS);
    assert_eq!(view.cursor_position(), (16, 6).into());
}

#[test]
fn color_mode_controls_background_input_underline_and_badges() {
    let app = mockup_new_form();
    let view = draw(&app, 100, 28);
    let buffer = view.buffer();
    assert_eq!((buffer[(0, 0)].fg, buffer[(0, 0)].bg), (TEXT, BG));
    assert_eq!(buffer[(97, 20)].bg, BG);

    let mut terminal = Terminal::new(TestBackend::new(100, 28)).unwrap();
    let ansi = Theme::new(ColorMode::Ansi);
    terminal
        .draw(|frame| {
            render(frame, &app, &ansi);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].bg, Color::Reset);
    assert_eq!(buffer[(16, 4)].bg, Color::Reset);
    assert!(buffer[(16, 4)].modifier.contains(Modifier::UNDERLINED));
    assert_eq!(buffer[(16, 12)].underline_color, Color::Cyan);
    assert!(buffer[(16, 12)].modifier.contains(Modifier::UNDERLINED));
    assert_eq!(buffer[(53, 8)].fg, Color::Yellow);
    assert_eq!(
        (buffer[(2, 26)].fg, buffer[(2, 26)].bg),
        (Color::Black, Color::Cyan)
    );

    let app = mockup_list();
    terminal
        .draw(|frame| {
            render(frame, &app, &ansi);
        })
        .unwrap();
    let cell = &terminal.backend().buffer()[(4, 6)];
    assert_eq!((cell.fg, cell.bg), (Color::Black, Color::Cyan));
    assert_eq!(cell.modifier, Modifier::BOLD);
}

fn assert_text_hit(view: &TestBackend, hits: &HitMap, text: &str, target: Option<ClickTarget>) {
    for (row, line) in screen(view).iter().enumerate() {
        if let Some(index) = line.find(text) {
            let start = line[..index].width() as u16;
            for column in start..start + text.width() as u16 {
                assert_eq!(
                    hits.hit(column, row as u16),
                    target.as_ref(),
                    "{text:?} at column {column}"
                );
            }
            return;
        }
    }
    panic!("missing hit-test text {text:?}");
}

#[test]
fn host_hits_cover_rows_and_use_scrolled_filtered_indices() {
    let mut app = App::new(FakeBackend::new(MANY_HOSTS));
    let (_, hits) = draw_with_hits(&app, 80, 12);
    for index in 0..6 {
        for column in [2, 12, 42] {
            assert_eq!(
                hits.hit(column, 4 + index as u16),
                Some(&ClickTarget::HostRow(index))
            );
        }
    }
    assert_eq!(hits.hit(1, 4), None);
    assert_eq!(hits.hit(43, 4), None);
    for x in 2..78 {
        assert_eq!(hits.hit(x, 2), Some(&ClickTarget::Key(KeyCode::Char('/'))));
    }

    press(&mut app, KeyCode::End);
    let (view, hits) = draw_with_hits(&app, 80, 12);
    assert!(screen(&view)[4].starts_with("    queue"));
    assert_eq!(hits.hit(12, 4), Some(&ClickTarget::HostRow(2)));
    assert_eq!(hits.hit(12, 9), Some(&ClickTarget::HostRow(7)));

    press(&mut app, KeyCode::Char('/'));
    app.handle_paste("e");
    let (view, hits) = draw_with_hits(&app, 80, 12);
    assert!(screen(&view)[5].starts_with("    queue"));
    assert_eq!(hits.hit(12, 5), Some(&ClickTarget::HostRow(1)));
    assert_eq!(hits.hit(12, 2), None);
}

#[test]
fn list_key_hints_register_whole_pairs_and_leave_gaps_unmapped() {
    let app = update_fixture(InstallMethod::Brew);
    let (view, hits) = draw_with_hits(&app, 100, 28);
    for (text, code) in [
        (" enter  connect", KeyCode::Enter),
        ("n new", KeyCode::Char('n')),
        ("e edit", KeyCode::Char('e')),
        ("d delete", KeyCode::Char('d')),
        ("/ filter", KeyCode::Char('/')),
        ("r reload", KeyCode::Char('r')),
        ("q quit", KeyCode::Char('q')),
    ] {
        assert_text_hit(&view, &hits, text, Some(ClickTarget::Key(code)));
    }
    assert!((17..20).all(|column| hits.hit(column, 26).is_none()));

    let (view, hits) = draw_with_hits(&app, 18, 12);
    assert_eq!(hits.hit(14, 11), Some(&ClickTarget::Key(KeyCode::Enter)));
    assert_eq!(hits.hit(17, 11), None);
    assert_eq!(screen(&view)[11], " enter  connect");
}

#[test]
fn update_notice_hit_requires_a_plan_and_room_in_the_header() {
    let app = update_fixture(InstallMethod::Brew);
    let (view, hits) = draw_with_hits(&app, 100, 28);
    let text = "update available u";
    assert_text_hit(&view, &hits, text, Some(ClickTarget::UpdateNotice));
    assert_eq!(hits.hit(97 - text.width() as u16, 1), None);
    let (_, hits) = draw_with_hits(&app, 30, 12);
    assert!((0..30).all(|column| hits.hit(column, 0).is_none()));
    let app = App::new(FakeBackend::new(HOSTS));
    let (_, hits) = draw_with_hits(&app, 100, 28);
    assert!((0..100).all(|column| hits.hit(column, 1).is_none()));
}

#[test]
fn form_hits_cover_field_labels_inputs_and_note_columns() {
    let mut app = App::new(FakeBackend::new(""));
    let (view, hits) = draw_with_hits(&app, 100, 28);
    for (field, row) in FormField::ALL.into_iter().zip((4..=12).step_by(2)) {
        for (x, column) in [
            (2, u16::MAX),
            (14, u16::MAX),
            (15, 0),
            (16, 0),
            (20, 4),
            (50, 34),
            (53, u16::MAX),
        ] {
            assert_eq!(
                hits.hit(x, row),
                Some(&ClickTarget::FormField { field, column })
            );
        }
        assert_eq!(hits.hit(0, row), None);
    }
    assert_text_hit(
        &view,
        &hits,
        "tab next",
        Some(ClickTarget::Key(KeyCode::Tab)),
    );
    assert_text_hit(&view, &hits, "ctrl+u clear field", None);

    set_field(&mut app, FormField::KeyFile, "");
    let (view, hits) = draw_with_hits(&app, 20, 5);
    let cursor = view.cursor_position();
    assert_eq!(
        hits.hit(cursor.x, cursor.y),
        Some(&ClickTarget::FormField {
            field: FormField::KeyFile,
            column: 0
        })
    );
    assert!(!hits.regions.iter().any(|(_, target)| matches!(
        target,
        ClickTarget::FormField {
            field: FormField::Nickname,
            ..
        }
    )));
}

#[test]
fn input_hits_include_the_display_width_of_horizontally_scrolled_text() {
    let mut app = App::new(FakeBackend::new(""));
    let field = FormField::Address;
    set_field(&mut app, field, &format!("{}끝", "가".repeat(50)));
    let (view, hits) = draw_with_hits(&app, 43, 20);
    let cursor = view.cursor_position();
    let first = 14;
    let offset = 102 - (cursor.x - first);
    assert_eq!(offset, 76);
    for x in first - 1..=cursor.x {
        assert_eq!(
            hits.hit(x, cursor.y),
            Some(&ClickTarget::FormField {
                field,
                column: offset + x.saturating_sub(first)
            })
        );
    }
    let target = hits.hit(first + 1, cursor.y).unwrap().clone();
    app.handle_click(target, false);
    assert_eq!(app.form().unwrap().input(field).cursor(), 38);
}

#[test]
fn form_key_hints_are_hit_targets_including_disabled_browse() {
    let new = mockup_new_form();
    let mut backend = FakeBackend::new(HOSTS);
    backend.unavailable = Some("Browse is unavailable".into());
    let mut edit = App::new(backend);
    press(&mut edit, KeyCode::Char('e'));
    set_field(&mut edit, FormField::KeyFile, "");
    for (app, submit) in [
        (&new, " enter  save & connect"),
        (&edit, " enter  review changes"),
    ] {
        let (view, hits) = draw_with_hits(app, 100, 28);
        for (text, code) in [
            ("ctrl+l pick key", KeyCode::F(2)),
            ("ctrl+o browse", KeyCode::F(3)),
            (submit, KeyCode::Enter),
            ("esc back", KeyCode::Esc),
        ] {
            assert_text_hit(&view, &hits, text, Some(ClickTarget::Key(code)));
        }
    }
    let (_, hits) = draw_with_hits(&new, 20, 28);
    assert!(
        !hits
            .regions
            .iter()
            .any(|(_, target)| *target == ClickTarget::Key(KeyCode::F(3)))
    );
}

#[test]
fn confirm_action_hints_use_their_letter_keys() {
    let edited = edit_confirmation();
    let mut deleted = mockup_list();
    press(&mut deleted, KeyCode::Char('d'));
    for (app, primary, back) in [
        (edited, " y  save", "n back to form"),
        (deleted, " y  delete", "n back to list"),
    ] {
        let (view, hits) = draw_with_hits(&app, 100, 28);
        for (text, code) in [(primary, 'y'), (back, 'n')] {
            assert_text_hit(
                &view,
                &hits,
                text,
                Some(ClickTarget::Key(KeyCode::Char(code))),
            );
        }
        assert_text_hit(&view, &hits, "↑↓ scroll", None);
        assert_eq!(hits.hit(2, 25), None);
    }
}

#[test]
fn popup_targets_cover_key_rows_and_swallow_background_clicks() {
    let mut picker = App::new(FakeBackend::new("").with_keys());
    press(&mut picker, KeyCode::F(2));
    let (view, hits) = draw_with_hits(&picker, 100, 28);
    for (index, name) in ["~/.ssh/id_a", "~/.ssh/id_b"].into_iter().enumerate() {
        assert_text_hit(&view, &hits, name, Some(ClickTarget::KeyRow(index)));
    }
    assert_eq!(hits.hit(0, 0), Some(&ClickTarget::Outside));
    assert_eq!(hits.hit(12, 4), Some(&ClickTarget::Outside));
    assert_eq!(view.buffer()[(20, 8)].symbol(), "┌");
    assert_eq!(hits.hit(20, 8), Some(&ClickTarget::Inert));

    let mut acknowledgment = edit_confirmation();
    press(&mut acknowledgment, KeyCode::Enter);
    let mut update = update_fixture(InstallMethod::Brew);
    press(&mut update, KeyCode::Char('u'));
    let mut manual = update_fixture(InstallMethod::Unknown);
    press(&mut manual, KeyCode::Char('u'));
    let (_, list_hits) = draw_with_hits(&mockup_list(), 100, 28);
    assert_eq!(list_hits.hit(12, 5), Some(&ClickTarget::HostRow(0)));

    for (app, actions) in [
        (
            connection_notice(),
            &[
                (" enter  connect anyway", KeyCode::Enter),
                ("esc back to list", KeyCode::Esc),
            ][..],
        ),
        (acknowledgment, &[(" enter  ok", KeyCode::Enter)][..]),
        (
            update,
            &[
                (" enter  update and exit", KeyCode::Enter),
                ("esc cancel", KeyCode::Esc),
            ][..],
        ),
        (manual, &[(" esc  close", KeyCode::Esc)][..]),
    ] {
        let (view, hits) = draw_with_hits(&app, 100, 28);
        for &(text, code) in actions {
            assert_text_hit(&view, &hits, text, Some(ClickTarget::Key(code)));
        }
        assert_eq!(hits.hit(12, 5), Some(&ClickTarget::Outside));
        let (right, bottom) = popup_corner(&view, "┘");
        assert_eq!(view.buffer()[(right - 1, bottom - 1)].symbol(), " ");
        assert_eq!(hits.hit(right - 1, bottom - 1), Some(&ClickTarget::Inert));
    }
}

#[test]
fn popup_help_hits_take_priority_over_the_outside_region() {
    let mut picker = App::new(FakeBackend::new("").with_keys());
    press(&mut picker, KeyCode::F(2));
    let (view, hits) = draw_with_hits(&picker, 100, 28);
    assert_text_hit(
        &view,
        &hits,
        " enter  use this key",
        Some(ClickTarget::Key(KeyCode::Enter)),
    );
    assert_text_hit(
        &view,
        &hits,
        "esc cancel",
        Some(ClickTarget::Key(KeyCode::Esc)),
    );
    assert_text_hit(&view, &hits, "↑↓ select", Some(ClickTarget::Outside));
    assert_eq!(hits.hit(22, 26), Some(&ClickTarget::Outside));

    let mut acknowledgment = edit_confirmation();
    press(&mut acknowledgment, KeyCode::Enter);
    let (view, hits) = draw_with_hits(&acknowledgment, 100, 28);
    assert_text_hit(
        &view,
        &hits,
        " enter  ok",
        Some(ClickTarget::Key(KeyCode::Enter)),
    );
}

#[test]
fn every_hit_region_is_nonempty_and_within_small_frames() {
    let mut picker = App::new(FakeBackend::new("").with_keys());
    press(&mut picker, KeyCode::F(2));
    let mut update = update_fixture(InstallMethod::Brew);
    press(&mut update, KeyCode::Char('u'));
    let apps = [
        App::new(FakeBackend::new(HOSTS)),
        App::new(FakeBackend::new("")),
        edit_confirmation(),
        picker,
        connection_notice(),
        update,
    ];
    for (width, height) in [(1, 1), (5, 2), (20, 5)] {
        for app in &apps {
            let (view, hits) = draw_with_hits(app, width, height);
            let frame = view.buffer().area;
            for (area, target) in &hits.regions {
                assert!(!area.is_empty(), "{target:?} in {area:?}");
                assert_eq!(
                    area.intersection(frame),
                    *area,
                    "{target:?} outside {frame:?}"
                );
            }
        }
    }
}
