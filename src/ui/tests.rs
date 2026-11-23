use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};
use tempfile::{TempDir, tempdir};
use unicode_width::UnicodeWidthStr;

use super::{ColorMode, HitMap, Hover, HoverFrame, HoverKey, Theme, render};
use crate::app::{App, Backend, ClickTarget, FormField, Mode};
use crate::config::{self, ConfigSet, PendingWrite, WriteError, WriteOutcome};
use crate::dialog::{DialogError, PickOutcome};
use crate::keys::{KeyInfo, PublicKeyInfo};
use crate::known_hosts::{self, ForgetPlan};
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
    history_commands: Vec<String>,
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
                host_key_alias: None,
                port: Some("2222".into()),
                identity_files: vec!["~/.ssh/first".into(), "~/.ssh/second".into()],
            }),
            read_only: false,
            keys: Vec::new(),
            unavailable: None,
            hidden_hint: None,
            load_error: None,
            update_plan: None,
            history_commands: Vec::new(),
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

    fn recent_ssh_commands(&self) -> Vec<String> {
        self.history_commands.clone()
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

    fn plan_forget(&self, name: &str) -> Result<Option<ForgetPlan>, String> {
        known_hosts::plan_forget(&self.paths, name).map_err(|error| error.to_string())
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
    draw_with_style(app, width, height, ColorMode::TrueColor, None)
}

fn draw_with_style(
    app: &App<FakeBackend>,
    width: u16,
    height: u16,
    mode: ColorMode,
    hover: HoverFrame<'_>,
) -> (TestBackend, HitMap) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| hits = render(frame, app, &Theme::new(mode), hover))
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
        ],
    );
    assert_eq!(
        screen(&view)[26],
        "   enter  connect   e edit   d delete   x forget(known_host)   / filter   r reload   esc quit"
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
const HOVER_ROW: Color = Color::Rgb(30, 33, 40);
const HOVER_SELECTED: Color = Color::Rgb(46, 52, 63);
const HOVER_ACCENT: Color = Color::Rgb(155, 229, 214);
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
        host_key_alias: None,
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
    assert_eq!(view.buffer()[(2, 26)].bg, ACCENT);
    assert_eq!(view.buffer()[(3, 26)].fg, BG);
}

#[test]
fn new_row_follows_the_hosts_and_shows_its_own_detail_and_help() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    let (view, hits) = draw_with_hits(&app, 100, 28);
    let rows = screen(&view);
    assert!(rows[7].starts_with("    queue"));
    assert!(rows[8].starts_with("    + New connection n"));
    assert_eq!(view.buffer()[(21, 8)].fg, MUTED);
    assert_text_hit(&view, &hits, "+ New connection", Some(ClickTarget::NewRow));
    assert_eq!(hits.hit(53, 8), Some(&ClickTarget::NewRow));
    assert_eq!(ClickTarget::NewRow.hover_key(), Some(HoverKey::NewRow));

    press(&mut app, KeyCode::End);
    press(&mut app, KeyCode::Down);
    let (view, hits) = draw_with_hits(&app, 100, 28);
    let rows = screen(&view);
    assert!(rows[8].starts_with("  › + New connection"));
    assert_eq!(view.buffer()[(2, 8)].bg, ROW);
    assert_eq!(view.buffer()[(53, 8)].bg, ROW);
    assert_eq!(view.buffer()[(57, 5)].modifier, Modifier::BOLD);
    assert_eq!(view.buffer()[(57, 6)].fg, MUTED);
    assert_eq!(view.buffer()[(57, 7)].fg, MUTED);
    assert!(rows[5].ends_with("│  New connection"));
    let description: Vec<_> = rows[6..8]
        .iter()
        .map(|row| row.split_once("│  ").unwrap().1)
        .collect();
    assert_eq!(
        description.join(" "),
        "Add a host or connect once without saving."
    );
    assert!(!rows.join("\n").contains("enter runs"));
    assert!(rows[26].starts_with("   enter  new   "));
    assert_eq!(view.buffer()[(2, 26)].bg, ACCENT);
    assert_text_hit(
        &view,
        &hits,
        " enter  new",
        Some(ClickTarget::Key(KeyCode::Enter)),
    );
}

#[test]
fn new_row_scrolls_with_hosts_and_is_selected_when_no_hosts_match() {
    let mut app = App::new(FakeBackend::new(MANY_HOSTS));
    press(&mut app, KeyCode::End);
    press(&mut app, KeyCode::Down);
    let (view, hits) = draw_with_hits(&app, 80, 10);
    let rows = screen(&view);
    assert!(rows[6].starts_with("    git"));
    assert!(rows[7].starts_with("  › + New connection"));
    assert_eq!(hits.hit(2, 7), Some(&ClickTarget::NewRow));

    press(&mut app, KeyCode::Char('/'));
    app.handle_paste("missing");
    press(&mut app, KeyCode::Enter);
    let (view, hits) = draw_with_hits(&app, 100, 28);
    let rows = screen(&view);
    assert!(rows[1].ends_with("0 of 8 hosts"));
    assert!(rows[5].starts_with("  No matching hosts"));
    assert!(rows[6].starts_with("  › + New connection"));
    assert_eq!(hits.hit(2, 5), None);
    assert_eq!(hits.hit(2, 6), Some(&ClickTarget::NewRow));
    assert_eq!(
        rows[26],
        "   enter  new   e edit   d delete   x forget(known_host)   / filter   esc clear filter"
    );
    assert_eq!(view.buffer()[(2, 26)].bg, ACCENT);
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
            "enter  save & connect   tab next   ↑↓ pick key   ctrl+o browse   esc back",
        ],
    );
    assert_eq!(view.buffer()[(12, 19)].modifier, Modifier::BOLD);
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
    assert_eq!(buffer[(2, 22)].fg, WARN);
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
            "↑↓ pick key",
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
    assert_eq!(cursor, (40, 6).into());
    assert_eq!(view.buffer()[(cursor.x - 2, cursor.y)].symbol(), "끝");

    press(&mut app, KeyCode::Home);
    let view = draw(&app, 43, 20);
    assert_eq!(view.cursor_position(), (14, 6).into());
    assert_eq!(view.buffer()[(14, 6)].symbol(), "가");
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
    assert!(rows[26].ends_with("r reload   esc quit"));

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
    assert_eq!(buffer[(20, 26)].symbol(), "e");
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
    assert!(rows[20].contains("Fix Port to continue"));
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
        footer
            .contains("enter  save & connect   tab next   ↑↓ pick key   ctrl+o browse   esc back")
    );
    assert!(!footer.contains("ctrl+u"));
    assert_eq!(view.buffer()[(2, 26)].bg, ACCENT);
    press(&mut app, KeyCode::BackTab);
    let view = draw(&app, 100, 28);
    let footer = &screen(&view)[26];
    assert!(footer.contains("tab next   ctrl+u clear field   esc back"));
    assert!(!footer.contains("↑↓ pick key") && !footer.contains("ctrl+o"));
}

#[test]
fn form_guidance_styles_missing_fields_and_live_commands() {
    let mut app = App::new(FakeBackend::new(""));
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    assert!(rows[9].is_empty());
    assert!(rows[19].contains("Fill in Address to continue"));
    assert_eq!(view.buffer()[(4, 19)].fg, ERROR);
    assert_eq!(rows[17].trim_start(), "─".repeat(96));
    set_field(&mut app, FormField::Address, "www.example.com");
    let view = draw(&app, 100, 28);
    assert!(screen(&view)[19].ends_with("1 Run   $ ssh www.example.com"));
    assert_eq!(view.buffer()[(12, 19)].fg, MUTED);
    assert_eq!(view.buffer()[(14, 19)].fg, ACCENT);
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
            "enter  cancel   ←→ choose   y delete   n back to list   ↑↓ scroll",
        ],
    );
    assert!(!screen(&view)[3].contains('+'));
}

#[test]
fn delete_question_highlights_the_current_choice_and_both_choices_are_clickable() {
    for (mode, accent, error, muted) in [
        (ColorMode::TrueColor, ACCENT, ERROR, MUTED),
        (ColorMode::Ansi, Color::Cyan, Color::Red, Color::DarkGray),
    ] {
        let mut app = mockup_list();
        press(&mut app, KeyCode::Char('d'));
        for (highlighted, enter) in [("Cancel", "cancel"), ("Delete", "delete")] {
            let (view, hits) = draw_with_style(&app, 100, 28, mode, None);
            let rows = screen(&view);
            let question = &rows[24];
            assert!(question.starts_with("  Delete prod-web-1 from ~/.ssh/config?"));
            assert!(rows[23].ends_with("The current file is kept as ~/.ssh/.config.sshtui.bak"));
            assert_eq!(
                rows[26],
                format!("   enter  {enter}   ←→ choose   y delete   n back to list   ↑↓ scroll")
            );
            assert_text_hit(&view, &hits, "←→ choose", None);
            assert_text_hit(
                &view,
                &hits,
                &format!(" enter  {enter}"),
                Some(ClickTarget::Key(KeyCode::Enter)),
            );
            for (label, code, color) in [("Cancel", 'n', accent), ("Delete", 'y', error)] {
                let column = question.rfind(label).unwrap() as u16;
                let width = label.width() as u16;
                let cell = &view.buffer()[(column, 24)];
                let columns = if label == highlighted {
                    assert_eq!(cell.modifier, Modifier::BOLD);
                    for x in column - 1..=column + width {
                        assert_eq!(view.buffer()[(x, 24)].bg, color);
                    }
                    column - 1..column + width + 1
                } else {
                    assert_eq!(cell.fg, muted);
                    assert!(!cell.modifier.contains(Modifier::BOLD));
                    column..column + width
                };
                for x in columns {
                    assert_eq!(
                        hits.hit(x, 24),
                        Some(&ClickTarget::Key(KeyCode::Char(code)))
                    );
                }
            }
            press(&mut app, KeyCode::Right);
        }
    }
}

#[test]
fn delete_question_keeps_the_choices_when_the_window_is_short_and_narrow() {
    let mut app = mockup_list();
    press(&mut app, KeyCode::Char('d'));
    let (view, hits) = draw_with_hits(&app, 40, 7);
    let rows = screen(&view);
    assert!(rows[4].starts_with("Delete prod-web-1"));
    assert!(rows[4].ends_with("Cancel    Delete"));
    assert!(!rows.join("\n").contains("The current file is kept as"));
    assert_text_hit(
        &view,
        &hits,
        "Cancel",
        Some(ClickTarget::Key(KeyCode::Char('n'))),
    );
    assert_eq!(hits.hit(39, 4), Some(&ClickTarget::Key(KeyCode::Char('y'))));
}

#[test]
fn forget_confirmation_shows_the_lookup_name_removals_backup_and_hints() {
    for (count, summary) in [
        (1, "1 entry for [www.example.com]:2222 will be removed  -1"),
        (
            2,
            "2 entries for [www.example.com]:2222 will be removed  -2",
        ),
    ] {
        let backend = FakeBackend::new(HOSTS);
        let entry = format!(
            "[www.example.com]:2222 ssh-ed25519 fake-key-{}-tail\n",
            "a".repeat(110)
        );
        fs::write(known_hosts::file_path(backend.paths()), entry.repeat(count)).unwrap();
        let mut app = App::new(backend);
        press(&mut app, KeyCode::Char('x'));

        let view = draw(&app, 100, 28);
        let rows = screen(&view);
        assert!(rows[1].starts_with("  sshtui  Forget host key for web"));
        assert!(rows[1].ends_with("~/.ssh/known_hosts"));
        assert!(rows[3].ends_with(summary));
        for (index, line) in rows[5..5 + count].iter().enumerate() {
            assert_eq!(view.buffer()[(14, 5 + index as u16)].symbol(), "-");
            assert!(line.contains("[www.example.com]:2222 ssh-ed25519 fake-key-"));
            assert!(!line.contains("-tail"));
        }
        assert!(rows[23].ends_with("The current file is kept as ~/.ssh/.known_hosts.sshtui.bak"));
        assert!(
            rows[26]
                .ends_with(" enter  cancel   ←→ choose   y forget   n back to list   ↑↓ scroll")
        );
    }
}

#[test]
fn forget_question_counts_entries_and_offers_forget() {
    for (count, entries) in [(1, "entry"), (2, "entries")] {
        let backend = FakeBackend::new(HOSTS);
        let entry = "[www.example.com]:2222 ssh-ed25519 fake-key\n";
        fs::write(known_hosts::file_path(backend.paths()), entry.repeat(count)).unwrap();
        let mut app = App::new(backend);
        press(&mut app, KeyCode::Char('x'));
        let (view, hits) = draw_with_hits(&app, 100, 28);
        let rows = screen(&view);
        assert_eq!(
            rows[24],
            format!(
                "  Remove {count} known_hosts {entries} for [www.example.com]:2222?    Cancel    Forget"
            )
        );
        let forget = rows[24].rfind("Forget").unwrap() as u16;
        assert_eq!(
            hits.hit(forget, 24),
            Some(&ClickTarget::Key(KeyCode::Char('y')))
        );
        press(&mut app, KeyCode::Right);
        assert!(
            screen(&draw(&app, 100, 28))[26]
                .ends_with(" enter  forget   ←→ choose   y forget   n back to list   ↑↓ scroll")
        );
    }
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
                    None,
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
        (22, [true, true, true]),
        (17, [true, true, false]),
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
        assert!(visible.contains("↑↓ pick key"));
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
            render(frame, &app, &ansi, None);
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
            render(frame, &app, &ansi, None);
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
        ("e edit", KeyCode::Char('e')),
        ("d delete", KeyCode::Char('d')),
        ("x forget(known_host)", KeyCode::Char('x')),
        ("/ filter", KeyCode::Char('/')),
        ("r reload", KeyCode::Char('r')),
        ("esc quit", KeyCode::Esc),
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
fn escape_is_the_last_list_hint_and_reflects_an_applied_filter() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    let escape = Some(ClickTarget::Key(KeyCode::Esc));
    let (view, hits) = draw_with_hits(&app, 100, 28);
    assert!(screen(&view)[26].ends_with("esc quit"));
    assert!(!screen(&view)[26].contains("q quit"));
    assert_text_hit(&view, &hits, "esc quit", escape.clone());

    press(&mut app, KeyCode::Char('/'));
    app.handle_paste("web");
    press(&mut app, KeyCode::Enter);
    assert!(!app.is_filtering());
    let (view, hits) = draw_with_hits(&app, 100, 28);
    assert!(screen(&view)[26].ends_with("esc clear filter"));
    assert_text_hit(&view, &hits, "esc clear filter", escape);
}

#[test]
fn filtered_list_hints_keep_escape_and_drop_reload_when_space_runs_out() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    let unfiltered = draw(&app, 96, 28);
    assert_eq!(
        screen(&unfiltered)[26],
        "   enter  connect   e edit   d delete   x forget(known_host)   / filter   r reload   esc quit"
    );
    press(&mut app, KeyCode::Char('/'));
    app.handle_paste("web");
    press(&mut app, KeyCode::Enter);

    let wide = draw(&app, 100, 28);
    assert_eq!(
        screen(&wide)[26],
        "   enter  connect   e edit   d delete   x forget(known_host)   / filter   esc clear filter"
    );

    let (view, hits) = draw_with_hits(&app, 96, 28);
    assert_eq!(
        screen(&view)[26],
        "   enter  connect   e edit   d delete   x forget(known_host)   / filter   esc clear filter"
    );
    assert_text_hit(
        &view,
        &hits,
        "x forget(known_host)",
        Some(ClickTarget::Key(KeyCode::Char('x'))),
    );
    assert_text_hit(
        &view,
        &hits,
        "/ filter",
        Some(ClickTarget::Key(KeyCode::Char('/'))),
    );
    assert_text_hit(
        &view,
        &hits,
        "esc clear filter",
        Some(ClickTarget::Key(KeyCode::Esc)),
    );
    assert!(
        (0..96).all(|column| hits.hit(column, 26) != Some(&ClickTarget::Key(KeyCode::Char('r'))))
    );
}

#[test]
fn narrow_list_hints_keep_the_badge_and_escape_together() {
    let app = App::new(FakeBackend::new(HOSTS));
    let (view, hits) = draw_with_hits(&app, 40, 12);
    assert_eq!(screen(&view)[11], " enter  connect   e edit   esc quit");
    assert_text_hit(
        &view,
        &hits,
        " enter  connect",
        Some(ClickTarget::Key(KeyCode::Enter)),
    );
    assert_text_hit(
        &view,
        &hits,
        "esc quit",
        Some(ClickTarget::Key(KeyCode::Esc)),
    );
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
fn history_header_shows_accented_clickable_singular_and_plural_notices() {
    for (commands, notice) in [
        (&["ssh first.example"][..], "1 suggestion i"),
        (
            &["ssh first.example", "ssh second.example"][..],
            "2 suggestions i",
        ),
    ] {
        let mut backend = FakeBackend::new(HOSTS);
        backend.history_commands = commands
            .iter()
            .map(|command| (*command).to_owned())
            .collect();
        let app = App::new(backend);
        let (view, hits) = draw_with_hits(&app, 100, 28);
        assert!(screen(&view)[1].ends_with(notice));
        assert_text_hit(
            &view,
            &hits,
            notice,
            Some(ClickTarget::Key(KeyCode::Char('i'))),
        );
        let key = &view.buffer()[(97, 1)];
        assert_eq!(key.fg, ACCENT);
        assert!(key.modifier.contains(Modifier::BOLD));
    }
}

#[test]
fn history_header_precedes_update_and_is_hidden_first_when_space_runs_out() {
    let mut backend = FakeBackend::new(HOSTS);
    backend.history_commands = vec!["ssh first.example".into(), "ssh second.example".into()];
    backend.update_plan = Some(UpdatePlan {
        current: "1.0.0".into(),
        latest: "2.0.0".into(),
        method: InstallMethod::Brew,
    });
    let mut app = App::new(backend);
    let notices = "2 suggestions i   update available u";
    let (view, hits) = draw_with_hits(&app, 100, 28);
    assert!(screen(&view)[1].ends_with(notices));
    assert_text_hit(
        &view,
        &hits,
        "2 suggestions i",
        Some(ClickTarget::Key(KeyCode::Char('i'))),
    );
    assert_text_hit(
        &view,
        &hits,
        "update available u",
        Some(ClickTarget::UpdateNotice),
    );

    let (view, hits) = draw_with_hits(&app, 45, 12);
    assert!(!screen(&view)[0].contains("suggestion"));
    assert_text_hit(
        &view,
        &hits,
        "update available u",
        Some(ClickTarget::UpdateNotice),
    );
    assert!(
        (0..45).all(|column| hits.hit(column, 0) != Some(&ClickTarget::Key(KeyCode::Char('i'))))
    );

    press(&mut app, KeyCode::Char('i'));
    let (view, hits) = draw_with_hits(&app, 100, 28);
    assert!(screen(&view)[1].ends_with(notices));
    assert_eq!(view.buffer()[(97, 1)].fg, MUTED);
    assert_text_hit(&view, &hits, "2 suggestions i", Some(ClickTarget::Outside));
}

#[test]
fn history_popup_shows_extracted_fields_and_click_targets() {
    let mut backend = FakeBackend::new(HOSTS);
    backend.history_commands = [
        "ssh ops@new.example -ignored-remote-arg",
        "ssh plain.example",
        "ssh -p 2222 ops@new.example",
        "ssh -p 2222 -i '~/.ssh/work key' ops@new.example remote-command-marker",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut app = App::new(backend);
    press(&mut app, KeyCode::Char('i'));
    let (view, hits) = draw_with_hits(&app, 100, 28);
    let lines = screen(&view);
    let title = lines
        .iter()
        .find(|line| line.contains("Import from shell history"))
        .unwrap();
    assert!(title.contains("3 found"));
    let selected_row = lines
        .iter()
        .position(|line| line.contains("› ops@new.example:2222"))
        .unwrap();
    let selected = &lines[selected_row];
    assert!(selected.contains("ops@new.example:2222  ~/.ssh/work key"));
    assert!(selected.ends_with("×2  │"));
    let key_column = selected.find("~/.ssh/work key").unwrap();
    let count_column = selected.find('×').unwrap();
    for column in [key_column, count_column] {
        let x = selected[..column].width() as u16;
        assert_eq!(view.buffer()[(x, selected_row as u16)].fg, MUTED);
    }
    assert_eq!(lines[26], "   enter  fill form   ↑↓ select   esc close");
    contains(
        &view,
        &["Pick one to open the new connection form with these values."],
    );
    assert!(!lines.join("\n").contains("remote-command-marker"));
    for (text, index) in [
        ("› ops@new.example:2222", 0),
        ("plain.example", 1),
        ("×2", 0),
    ] {
        let target = ClickTarget::SuggestionRow(index);
        assert_text_hit(&view, &hits, text, Some(target.clone()));
        assert_eq!(target.hover_key(), Some(HoverKey::SuggestionRow(index)));
    }
    let hovered = fully_hovered(&app, HoverKey::SuggestionRow(0));
    let x = selected[..selected.find('›').unwrap()].width() as u16;
    assert_eq!(
        hovered.buffer()[(x, selected_row as u16)].bg,
        HOVER_SELECTED
    );
    assert_eq!(hits.hit(0, 0), Some(&ClickTarget::Outside));
}

#[test]
fn history_popup_scrolls_ten_rows_with_the_selection_in_view() {
    let mut backend = FakeBackend::new(HOSTS);
    backend.history_commands = (0..12)
        .map(|index| format!("ssh history-{index:02}.example"))
        .collect();
    let mut app = App::new(backend);
    press(&mut app, KeyCode::Char('i'));
    for (selection, first_row) in [(0, 0), (11, 2)] {
        if selection > 0 {
            for _ in 0..selection {
                press(&mut app, KeyCode::Down);
            }
        }
        let (view, hits) = draw_with_hits(&app, 100, 28);
        let visible_rows: Vec<_> = hits
            .regions
            .iter()
            .filter_map(|(_, target)| match *target {
                ClickTarget::SuggestionRow(index) => Some(index),
                _ => None,
            })
            .collect();
        assert_eq!(
            visible_rows,
            (first_row..first_row + 10).collect::<Vec<_>>()
        );
        contains(
            &view,
            &[
                "12 found",
                &format!("› history-{:02}.example", 11 - selection),
            ],
        );
        for index in visible_rows {
            assert_text_hit(
                &view,
                &hits,
                &format!("history-{:02}.example", 11 - index),
                Some(ClickTarget::SuggestionRow(index)),
            );
        }
    }
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
        assert_text_hit(&view, &hits, "↑↓ pick key", None);
        for (text, code) in [
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
        (deleted, "y delete", "n back to list"),
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

fn fully_hovered(app: &App<FakeBackend>, key: HoverKey) -> TestBackend {
    let start = Instant::now();
    let mut hover = Hover::default();
    hover.set(Some(key), start);
    draw_with_style(
        app,
        100,
        28,
        ColorMode::TrueColor,
        Some((&hover, start + Hover::DURATION)),
    )
    .0
}

#[test]
fn unselected_host_hover_interpolates_over_only_the_target_row() {
    let app = mockup_list();
    let base = draw(&app, 100, 28);
    let start = Instant::now();
    let mut hover = Hover::default();
    hover.set(Some(HoverKey::HostRow(0)), start);
    let (initial, initial_hits) =
        draw_with_style(&app, 100, 28, ColorMode::TrueColor, Some((&hover, start)));
    assert_eq!(base.buffer(), initial.buffer());
    let (middle, middle_hits) = draw_with_style(
        &app,
        100,
        28,
        ColorMode::TrueColor,
        Some((&hover, start + Hover::DURATION / 3)),
    );
    let (full, full_hits) = draw_with_style(
        &app,
        100,
        28,
        ColorMode::TrueColor,
        Some((&hover, start + Hover::DURATION)),
    );
    assert_eq!(initial_hits, middle_hits);
    assert_eq!(initial_hits, full_hits);

    for y in 0..28 {
        for x in 0..100 {
            let cell = (x, y);
            if y == 5 && (2..54).contains(&x) {
                assert_eq!(middle.buffer()[cell].bg, Color::Rgb(27, 30, 36));
                assert_eq!(full.buffer()[cell].bg, HOVER_ROW);
            } else {
                assert_eq!(base.buffer()[cell], middle.buffer()[cell]);
                assert_eq!(base.buffer()[cell], full.buffer()[cell]);
            }
        }
    }
    assert_eq!(base.buffer()[(20, 5)].fg, MUTED);
    assert_eq!(full.buffer()[(20, 5)].fg, TEXT);
}

#[test]
fn selected_host_hover_brightens_the_entire_selected_row() {
    let app = mockup_list();
    let view = fully_hovered(&app, HoverKey::HostRow(1));
    for x in 2..54 {
        assert_eq!(view.buffer()[(x, 6)].bg, HOVER_SELECTED);
    }
    assert_eq!(view.buffer()[(4, 6)].modifier, Modifier::BOLD);
    assert_eq!(view.buffer()[(20, 6)].fg, TEXT);
}

#[test]
fn hovering_the_inactive_filter_uses_the_focused_input_background() {
    let app = mockup_list();
    let view = fully_hovered(&app, HoverKey::Key(KeyCode::Char('/')));
    for x in 2..98 {
        assert_eq!(view.buffer()[(x, 3)].bg, INPUT_FOCUS);
    }
    assert_eq!(view.buffer()[(3, 3)].fg, MUTED);
}

#[test]
fn hovering_an_unfocused_editable_field_brightens_its_label_and_input() {
    let app = mockup_new_form();
    let view = fully_hovered(&app, HoverKey::FormField(FormField::Port));
    assert_eq!(view.buffer()[(4, 10)].fg, TEXT);
    for x in 15..51 {
        assert_eq!(view.buffer()[(x, 10)].bg, INPUT_FOCUS);
    }
}

#[test]
fn hovering_a_read_only_field_brightens_its_label_only() {
    let mut app = App::new(FakeBackend::new(
        "Host web\n  IdentityFile ~/one\n  IdentityFile ~/two\n",
    ));
    press(&mut app, KeyCode::Char('e'));
    assert!(app.form().unwrap().is_read_only(FormField::KeyFile));
    assert_ne!(app.form().unwrap().focus(), FormField::KeyFile);
    let base = draw(&app, 100, 28);
    let view = fully_hovered(&app, HoverKey::FormField(FormField::KeyFile));
    assert_eq!(view.buffer()[(4, 12)].fg, TEXT);
    for x in 15..98 {
        assert_eq!(view.buffer()[(x, 12)], base.buffer()[(x, 12)]);
    }
}

#[test]
fn hovering_an_ordinary_hint_brightens_its_whole_pair_and_description() {
    let app = mockup_list();
    let view = fully_hovered(&app, HoverKey::Key(KeyCode::Char('e')));
    for x in 20..26 {
        assert_eq!(view.buffer()[(x, 26)].bg, ROW);
    }
    assert_eq!(view.buffer()[(19, 26)].bg, BG);
    assert_eq!(view.buffer()[(26, 26)].bg, BG);
    assert_eq!(view.buffer()[(22, 26)].fg, TEXT);
}

#[test]
fn hovering_the_new_row_uses_host_row_backgrounds() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    for (selected, background) in [(false, HOVER_ROW), (true, HOVER_SELECTED)] {
        if selected {
            press(&mut app, KeyCode::End);
            press(&mut app, KeyCode::Down);
        }
        let view = fully_hovered(&app, HoverKey::NewRow);
        for x in 2..54 {
            assert_eq!(view.buffer()[(x, 8)].bg, background);
        }
        assert_eq!(view.buffer()[(54, 8)].bg, BG);
    }
}

#[test]
fn hovering_the_primary_action_brightens_its_badge() {
    let app = mockup_list();
    let view = fully_hovered(&app, HoverKey::Key(KeyCode::Enter));
    for x in 2..9 {
        assert_eq!(view.buffer()[(x, 26)].bg, HOVER_ACCENT);
    }
    assert_eq!(view.buffer()[(10, 26)].bg, BG);
}

#[test]
fn hovering_key_picker_rows_brightens_selected_and_unselected_backgrounds() {
    let mut app = App::new(FakeBackend::new("").with_keys());
    press(&mut app, KeyCode::F(2));
    let (base, hits) = draw_with_hits(&app, 100, 28);
    for (index, color) in [(0, HOVER_SELECTED), (1, HOVER_ROW)] {
        let view = fully_hovered(&app, HoverKey::KeyRow(index));
        let (area, _) = hits
            .regions
            .iter()
            .find(|(_, target)| *target == ClickTarget::KeyRow(index))
            .unwrap();
        for x in area.left()..area.right() {
            assert_eq!(view.buffer()[(x, area.y)].bg, color);
        }
        let other_y = if index == 0 { area.y + 1 } else { area.y - 1 };
        assert_eq!(
            view.buffer()[(area.x, other_y)],
            base.buffer()[(area.x, other_y)]
        );
    }
}

#[test]
fn hovering_the_update_notice_brightens_its_background() {
    let app = update_fixture(InstallMethod::Brew);
    let view = fully_hovered(&app, HoverKey::UpdateNotice);
    for x in (98 - "update available u".len() as u16)..98 {
        assert_eq!(view.buffer()[(x, 1)].bg, ROW);
        assert_eq!(view.buffer()[(x, 1)].fg, WARN);
    }
    assert_eq!(view.buffer()[(78, 1)].bg, BG);
}

#[test]
fn focused_inputs_and_disabled_badges_keep_their_original_styles() {
    let mut list = mockup_list();
    press(&mut list, KeyCode::Char('/'));
    list.handle_paste("prod");
    let form = mockup_new_form();
    let invalid = App::new(FakeBackend::new(""));
    assert!(invalid.form_first_error().is_some());
    for (app, key) in [
        (&list, HoverKey::Key(KeyCode::Char('/'))),
        (&form, HoverKey::FormField(form.form().unwrap().focus())),
        (&invalid, HoverKey::Key(KeyCode::Enter)),
    ] {
        assert_eq!(
            fully_hovered(app, key).buffer(),
            draw(app, 100, 28).buffer()
        );
    }
}

#[test]
fn ansi_host_hover_underlines_the_name_at_the_threshold_without_changing_backgrounds() {
    let app = mockup_list();
    let start = Instant::now();
    let mut hover = Hover::default();
    let key = HoverKey::HostRow(0);
    hover.set(Some(key), start);
    let forty_percent = start + Hover::DURATION.mul_f32(1.0 - 0.6_f32.cbrt());
    assert!((hover.intensity(&key, forty_percent) - 0.4).abs() < 0.001);
    let (base, _) = draw_with_style(&app, 100, 28, ColorMode::Ansi, None);
    let (faint, _) = draw_with_style(
        &app,
        100,
        28,
        ColorMode::Ansi,
        Some((&hover, forty_percent)),
    );
    let (full, _) = draw_with_style(
        &app,
        100,
        28,
        ColorMode::Ansi,
        Some((&hover, start + Hover::DURATION)),
    );
    assert_eq!(faint.buffer(), base.buffer());
    for y in 0..28 {
        for x in 0..100 {
            let original = &base.buffer()[(x, y)];
            let hovered = &full.buffer()[(x, y)];
            if y == 5 && (4..11).contains(&x) {
                assert!(hovered.modifier.contains(Modifier::UNDERLINED));
                assert_eq!(hovered.bg, original.bg);
            } else {
                assert_eq!(hovered, original);
            }
        }
    }
}

#[test]
fn a_popup_suppresses_hover_on_the_backdrop() {
    let mut app = update_fixture(InstallMethod::Brew);
    press(&mut app, KeyCode::Char('u'));
    let (base, hits) = draw_with_hits(&app, 100, 28);
    assert_eq!(hits.hit(4, 5), Some(&ClickTarget::Outside));
    assert_eq!(
        fully_hovered(&app, HoverKey::HostRow(0)).buffer(),
        base.buffer()
    );
    assert_eq!(
        fully_hovered(&app, HoverKey::UpdateNotice).buffer(),
        base.buffer()
    );
}

#[test]
fn hover_renders_at_small_sizes_in_each_screen_mode() {
    let mut picker = App::new(FakeBackend::new("").with_keys());
    press(&mut picker, KeyCode::F(2));
    let mut update = update_fixture(InstallMethod::Brew);
    press(&mut update, KeyCode::Char('u'));
    let start = Instant::now();
    let mut hover = Hover::default();
    let step = Duration::from_millis(4);
    for (index, key) in [
        HoverKey::Key(KeyCode::Char('/')),
        HoverKey::HostRow(0),
        HoverKey::FormField(FormField::Address),
        HoverKey::KeyRow(0),
        HoverKey::Key(KeyCode::Enter),
    ]
    .into_iter()
    .enumerate()
    {
        hover.set(Some(key), start + step * index as u32);
    }
    let apps = [
        mockup_list(),
        mockup_new_form(),
        edit_confirmation(),
        picker,
        connection_notice(),
        update,
    ];
    for (width, height) in [(1, 1), (5, 2), (20, 5)] {
        for app in &apps {
            let (view, hover_hits) = draw_with_style(
                app,
                width,
                height,
                ColorMode::TrueColor,
                Some((&hover, start + step * 5)),
            );
            assert_eq!(
                view.buffer().area,
                ratatui::layout::Rect::new(0, 0, width, height)
            );
            assert_eq!(hover_hits, draw_with_hits(app, width, height).1);
        }
    }
}

#[test]
fn new_form_shows_checkbox_labels_states_focus_and_notes() {
    let mut app = mockup_new_form();
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    assert_eq!(
        rows[14],
        "    Key login  [ ] Install the public key on the server  asks for the server password once"
    );
    assert_eq!(
        rows[15],
        "    Password   [ ] Disable password prompt (recommended)"
    );
    let buffer = view.buffer();
    assert_eq!(buffer[(15, 14)].bg, BG);
    assert_eq!(buffer[(15, 15)].bg, BG);
    assert_eq!(buffer[(57, 14)].fg, MUTED);

    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Char(' '));
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    assert_eq!(
        rows[15],
        "  › Password   [x] Disable password prompt (recommended)  make sure key login already works"
    );
    let warning = rows[15].find("make sure").unwrap() - ('›'.len_utf8() - 1);
    assert_eq!(view.buffer()[(warning as u16, 15)].fg, WARN);
    assert_eq!(view.buffer()[(2, 15)].fg, ACCENT);

    press(&mut app, KeyCode::BackTab);
    let (view, hits) = draw_with_hits(&app, 100, 28);
    assert_text_hit(
        &view,
        &hits,
        "space toggle",
        Some(ClickTarget::Key(KeyCode::Char(' '))),
    );
    app.handle_click(ClickTarget::Key(KeyCode::Char(' ')), false);
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    assert!(rows[14].starts_with("  › Key login  [x] Install the public key on the server"));
    assert!(rows[15].ends_with("[x] Disable password prompt (recommended)"));
    assert!(rows[26].contains("enter  save & connect   tab next   space toggle   esc back"));
}

#[test]
fn form_key_rows_show_current_and_focused_keys_and_their_hit_targets() {
    let mut app = new_form_values(FakeBackend::new("").with_keys());
    set_field(&mut app, FormField::KeyFile, "~/.ssh/id_b");
    let (view, hits) = draw_with_hits(&app, 100, 28);
    let rows = screen(&view);
    assert!(rows[12].contains("Key file"));
    assert!(rows[13].starts_with("                 ~/.ssh/id_a"));
    assert!(rows[14].starts_with("               ● ~/.ssh/id_b"));
    assert!(rows[13].ends_with("ssh-ed25519 test key"));
    assert_eq!(view.buffer()[(17, 13)].fg, MUTED);
    assert_eq!(view.buffer()[(17, 14)].fg, TEXT);
    assert_text_hit(&view, &hits, "~/.ssh/id_a", Some(ClickTarget::KeyRow(0)));
    assert_eq!(hits.hit(15, 14), Some(&ClickTarget::KeyRow(1)));
    assert_eq!(
        hits.hit(15, 14).and_then(ClickTarget::hover_key),
        Some(HoverKey::KeyRow(1))
    );

    app.handle_click(hits.hit(15, 13).unwrap().clone(), false);
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    assert!(rows[13].starts_with("               › ~/.ssh/id_a"));
    assert!(!rows[14].contains('●'));
    assert_eq!(view.buffer()[(15, 13)].fg, ACCENT);
    assert_eq!(view.buffer()[(17, 13)].bg, ROW);
    assert_eq!(view.buffer()[(17, 13)].fg, TEXT);
}

#[test]
fn form_key_rows_limit_the_window_and_scroll_to_the_cursor_or_current_value() {
    let mut backend = FakeBackend::new("");
    backend.keys = (0..6)
        .map(|index| {
            let name = format!("test_key_{index}");
            KeyInfo {
                path: backend.paths.ssh_dir().join(&name),
                file_name: name,
                public: None,
            }
        })
        .collect();
    let mut app = App::new(backend);
    set_field(&mut app, FormField::KeyFile, "");
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    let first = rows
        .iter()
        .position(|line| line.contains("Key file"))
        .unwrap()
        + 1;
    for index in 0..4 {
        assert!(rows[first + index].ends_with(&format!("~/.ssh/test_key_{index}")));
    }
    assert!(rows[first + 4].ends_with("… 2 more"));
    assert_eq!(view.buffer()[(15, (first + 4) as u16)].fg, MUTED);
    for _ in 0..5 {
        press(&mut app, KeyCode::Down);
    }
    let (view, hits) = draw_with_hits(&app, 100, 28);
    let rows = screen(&view);
    assert!(rows[first].ends_with("~/.ssh/test_key_1"));
    assert!(rows[first + 3].ends_with("› ~/.ssh/test_key_4"));
    assert!(rows[first + 4].ends_with("… 1 more"));
    assert_eq!(
        hits.hit(15, (first + 3) as u16),
        Some(&ClickTarget::KeyRow(4))
    );

    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Tab);
    let view = draw(&app, 100, 28);
    contains(&view, &["● ~/.ssh/test_key_5"]);
    assert!(!screen(&view).join("\n").contains("more"));
}

#[test]
fn compact_form_keeps_the_fields_keys_checkboxes_and_enter_guidance_in_order() {
    let mut app = new_form_values(FakeBackend::new("").with_keys());
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Char(' '));
    let view = draw(&app, 80, 24);
    let rows = screen(&view);
    for (index, content) in [
        "Nickname",
        "User",
        "Address",
        "Port",
        "Key file",
        "● ~/.ssh/id_a",
        "~/.ssh/id_b",
        "Key login",
        "Password",
        "───",
        "On enter",
        "1 Save",
        "2 Key   install ~/.ssh/id_a.pub on the server",
        "3 Run",
    ]
    .into_iter()
    .enumerate()
    {
        assert!(rows[4 + index].contains(content), "{rows:#?}");
    }
}

#[test]
fn form_key_rows_follow_validation_and_are_hidden_under_the_picker_or_when_read_only() {
    let mut app = App::new(FakeBackend::new("").with_keys());
    set_field(&mut app, FormField::KeyFile, "\"bad");
    let view = draw(&app, 100, 28);
    let rows = screen(&view);
    let field = rows
        .iter()
        .position(|line| line.contains("Key file"))
        .unwrap();
    assert!(rows[field + 1].contains("Key file must not contain"));
    assert!(rows[field + 2].contains("~/.ssh/id_a"));
    press(&mut app, KeyCode::F(2));
    let view = draw(&app, 100, 28);
    assert_eq!(
        screen(&view)
            .iter()
            .filter(|line| line.contains("~/.ssh/id_a"))
            .count(),
        1
    );

    let mut app = App::new(FakeBackend::new(
        "Host web\n  IdentityFile ~/first\n  IdentityFile ~/second\n  PasswordAuthentication no extra\n",
    ).with_keys());
    press(&mut app, KeyCode::Char('e'));
    let view = draw(&app, 100, 28);
    contains(
        &view,
        &[
            "IdentityFile ~/first",
            "PasswordAuthentication no extra",
            "read-only",
        ],
    );
    assert!(!screen(&view).join("\n").contains("~/.ssh/id_a"));
}

#[test]
fn enter_guidance_includes_the_key_install_step_for_direct_and_edited_connections() {
    let mut app = new_form_values(FakeBackend::new(""));
    set_field(&mut app, FormField::Nickname, "");
    app.handle_click(
        ClickTarget::FormField {
            field: FormField::InstallKey,
            column: u16::MAX,
        },
        false,
    );
    app.handle_click(
        ClickTarget::FormField {
            field: FormField::DisablePassword,
            column: u16::MAX,
        },
        false,
    );
    let view = draw(&app, 100, 28);
    contains(
        &view,
        &[
            "1 Key   install ~/.ssh/id_ed25519.pub on the server",
            "2 Run   $ ssh -o PasswordAuthentication=no",
        ],
    );

    let mut app = App::new(FakeBackend::new(HOSTS));
    press(&mut app, KeyCode::Char('e'));
    app.handle_click(
        ClickTarget::FormField {
            field: FormField::InstallKey,
            column: u16::MAX,
        },
        false,
    );
    let view = draw(&app, 100, 28);
    contains(
        &view,
        &[
            "1 Review changes to ~/.ssh/config",
            "2 Key   install ~/.ssh/first.pub on the server",
        ],
    );
}

fn visible_row(backend: &TestBackend, y: u16) -> String {
    let buffer = backend.buffer();
    let mut x = buffer.area.left();
    let mut line = String::new();
    while x < buffer.area.right() {
        let symbol = buffer[(x, y)].symbol();
        let width = symbol.width().max(1) as u16;
        assert!(x + width <= buffer.area.right(), "{symbol:?} at ({x}, {y})");
        for continuation in x + 1..x + width {
            assert_eq!(buffer[(continuation, y)].symbol(), " ");
        }
        line.push_str(symbol);
        x += width;
    }
    line.trim_end().to_owned()
}

fn visible_position(backend: &TestBackend, text: &str) -> (u16, u16) {
    let buffer = backend.buffer();
    for y in buffer.area.top()..buffer.area.bottom() {
        let line = visible_row(backend, y);
        if let Some(index) = line.find(text) {
            return (line[..index].width() as u16, y);
        }
    }
    panic!("missing {text:?} in terminal buffer\n{buffer:?}");
}

fn assert_visible(backend: &TestBackend, expected: &[&str]) {
    for text in expected {
        visible_position(backend, text);
    }
}

#[test]
fn korean_list_shows_localized_rows_and_aligned_details() {
    use crate::i18n::Lang;

    let mut backend = FakeBackend::new(HOSTS);
    backend.read_only = true;
    let mut app = App::with_lang(backend, Lang::Ko);
    let view = draw(&app, 100, 28);
    assert_eq!(visible_row(&view, 1), "  sshtui  호스트 3개");
    assert_eq!(visible_row(&view, 3), "   / 호스트 필터");
    assert_visible(
        &view,
        &[
            "+ 새 접속",
            "읽기 전용",
            "읽기 전용 파일",
            "Enter 실행 명령",
        ],
    );
    for (index, (label, value)) in [
        ("사용자", "alice"),
        ("주소", "www.example.com"),
        ("포트", "2222"),
        ("키 파일", "~/.ssh/first"),
        ("출처", "~/.ssh/config:2"),
    ]
    .into_iter()
    .enumerate()
    {
        let y = 8 + index as u16 + u16::from(index == 4);
        let line = visible_row(&view, y);
        let label_index = line.find(label).unwrap();
        let value_index = line.find(value).unwrap();
        assert_eq!(line[..label_index].width(), 57, "{line}");
        assert_eq!(line[..value_index].width(), 68, "{line}");
    }
    assert_eq!(view.buffer()[(51, 5)].symbol(), "용");

    let help =
        "enter  접속   e 편집   d 삭제   x known_host 삭제   / 필터   r 다시 읽기   esc 종료";
    for width in [100, 96] {
        assert_eq!(visible_row(&draw(&app, width, 28), 26).trim_start(), help);
    }

    press(&mut app, KeyCode::Char('/'));
    app.handle_paste("queue");
    let view = draw(&app, 100, 28);
    assert_visible(
        &view,
        &[
            "호스트 3개 중 1개",
            "enter  필터 적용   ↑↓ 선택   esc 필터 해제",
        ],
    );
    press(&mut app, KeyCode::Enter);
    let host_help = visible_row(&draw(&app, 95, 28), 26);
    press(&mut app, KeyCode::Down);
    let new_help = visible_row(&draw(&app, 95, 28), 26);
    let (_, host_secondary) = host_help.split_once("접속   ").unwrap();
    let (_, new_secondary) = new_help.split_once("새 접속   ").unwrap();
    assert_eq!(host_secondary, new_secondary);
    assert!(new_secondary.ends_with("esc 필터 해제"));
}

#[test]
fn korean_new_form_aligns_inputs_checkboxes_and_enter_steps() {
    use crate::i18n::Lang;

    let mut app = App::with_lang(FakeBackend::new(""), Lang::Ko);
    for (field, value) in [
        (FormField::Nickname, "prod-web"),
        (FormField::User, "deploy"),
        (FormField::Address, "203.0.113.12"),
        (FormField::Port, "2222"),
        (FormField::KeyFile, "~/.ssh/id_ed25519"),
    ] {
        set_field(&mut app, field, value);
    }
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Char(' '));
    let (view, hits) = draw_with_hits(&app, 100, 28);
    for (field, label, content) in [
        (FormField::Nickname, "닉네임", "prod-web"),
        (FormField::User, "사용자", "deploy"),
        (FormField::Address, "주소", "203.0.113.12"),
        (FormField::Port, "포트", "2222"),
        (FormField::KeyFile, "키 파일", "~/.ssh/id_ed25519"),
        (FormField::InstallKey, "키 로그인", "[x]"),
        (FormField::DisablePassword, "비밀번호", "[ ]"),
    ] {
        let (y, line) = (0..28)
            .map(|y| (y, visible_row(&view, y)))
            .find(|(_, line)| line.trim_start_matches([' ', '›']).starts_with(label))
            .unwrap();
        assert_eq!(line[..line.find(label).unwrap()].width(), 4);
        assert_eq!(
            line[..line.find(content).unwrap()].width(),
            15 + usize::from(!field.is_checkbox())
        );
        if !field.is_checkbox() {
            assert_eq!(
                hits.hit(15, y),
                Some(&ClickTarget::FormField { field, column: 0 })
            );
            assert_eq!(
                hits.hit(14, y),
                Some(&ClickTarget::FormField {
                    field,
                    column: u16::MAX
                })
            );
        }
    }
    assert_visible(
        &view,
        &[
            "새 접속",
            "필수",
            "선택, 입력하면 이 호스트를 저장",
            "서버에 공개 키 설치",
            "서버 비밀번호 1회 입력",
            "비밀번호 입력 비활성화(권장)",
            "Enter 실행 내용",
            "enter  저장 후 접속   tab 다음   space 전환   esc 뒤로",
        ],
    );

    let (save_column, save_y) = visible_position(&view, "1 저장");
    let (key_column, key_y) = visible_position(&view, "2 키");
    let (run_column, run_y) = visible_position(&view, "3 실행");
    assert_eq!(save_column, key_column);
    assert_eq!(save_column, run_column);
    for (y, content) in [
        (save_y, "prod-web →"),
        (key_y, "서버에 ~/.ssh/id_ed25519.pub 설치"),
        (run_y, "$ ssh prod-web"),
    ] {
        let line = visible_row(&view, y);
        assert_eq!(line[..line.find(content).unwrap()].width(), 12, "{line}");
    }
}

#[test]
fn korean_confirmations_show_questions_choices_backups_and_help() {
    use crate::i18n::Lang;

    let backend = FakeBackend::new(HOSTS);
    fs::write(
        known_hosts::file_path(backend.paths()),
        "[www.example.com]:2222 ssh-ed25519 fake-key\n",
    )
    .unwrap();
    let mut app = App::with_lang(backend, Lang::Ko);

    press(&mut app, KeyCode::Char('d'));
    let view = draw(&app, 100, 28);
    assert_visible(
        &view,
        &[
            "sshtui  web 삭제",
            "1줄 변경 예정  -1",
            "~/.ssh/config에서 web 삭제?    취소    삭제",
            "현재 파일 백업: ~/.ssh/.config.sshtui.bak",
            "enter  취소   ←→ 선택 이동   y 삭제   n 목록으로   ↑↓ 스크롤",
        ],
    );
    press(&mut app, KeyCode::Right);
    assert_visible(&draw(&app, 100, 28), &["enter  삭제   ←→ 선택 이동"]);

    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('x'));
    assert_visible(
        &draw(&app, 100, 28),
        &[
            "sshtui  web 호스트 키 삭제",
            "[www.example.com]:2222 항목 1개 삭제 예정  -1",
            "[www.example.com]:2222 known_hosts 항목 1개 삭제?    취소    삭제",
            "현재 파일 백업: ~/.ssh/.known_hosts.sshtui.bak",
            "enter  취소   ←→ 선택 이동   y 삭제   n 목록으로   ↑↓ 스크롤",
        ],
    );
}

#[test]
fn korean_popups_show_titles_and_key_hints() {
    use crate::i18n::Lang;

    let mut backend = FakeBackend::new(HOSTS).with_keys();
    backend.history_commands = vec!["ssh deploy@other.example.com".into()];
    backend.update_plan = Some(UpdatePlan {
        current: "1.0.0".into(),
        latest: "2.0.0".into(),
        method: InstallMethod::Brew,
    });
    let mut app = App::with_lang(backend, Lang::Ko);
    assert_visible(&draw(&app, 100, 28), &["후보 1개 i", "업데이트 있음 u"]);

    for (key, expected) in [
        (
            'i',
            &[
                "셸 히스토리에서 가져오기",
                "1개 발견",
                "enter  폼 채우기   ↑↓ 선택   esc 닫기",
            ][..],
        ),
        (
            'u',
            &[
                "sshtui 업데이트",
                "설치 방식",
                "명령",
                "enter  업데이트 후 종료   esc 취소",
            ][..],
        ),
    ] {
        press(&mut app, KeyCode::Char(key));
        assert_visible(&draw(&app, 100, 28), expected);
        press(&mut app, KeyCode::Esc);
    }

    press(&mut app, KeyCode::Char('n'));
    press(&mut app, KeyCode::F(2));
    assert_visible(
        &draw(&app, 100, 28),
        &[
            "키 파일 선택",
            "~/.ssh에 키 2개",
            "enter  이 키 사용   ↑↓ 선택   esc 취소",
        ],
    );
    press(&mut app, KeyCode::Esc);
    set_field(&mut app, FormField::Nickname, "new");
    set_field(&mut app, FormField::Address, "www.example.com");
    set_field(&mut app, FormField::User, "deploy");
    press(&mut app, KeyCode::Enter);
    assert_visible(
        &draw(&app, 100, 28),
        &[
            "저장 설정과 실제 SSH 설정 불일치",
            "사용자: 저장 값 \"deploy\", SSH 사용 값 \"alice\"",
            "enter  그대로 접속   esc 목록으로",
        ],
    );
}

#[test]
fn korean_rendering_drops_a_wide_character_that_would_cross_the_edge() {
    use crate::i18n::Lang;

    let name = "가".repeat(24);
    let mut app = App::with_lang(FakeBackend::new(&format!("Host {name}\n")), Lang::Ko);
    press(&mut app, KeyCode::Char('e'));
    for (width, count) in [(29, 10), (30, 11), (1, 0)] {
        let view = draw(&app, width, 28);
        if count > 0 {
            assert_eq!(
                visible_row(&view, 1),
                format!("sshtui  {}", "가".repeat(count))
            );
        }
        for y in 0..28 {
            assert!(visible_row(&view, y).width() <= usize::from(width));
        }
    }
}

#[test]
fn korean_validation_errors_are_shown_in_the_form() {
    use crate::i18n::Lang;

    let mut app = App::with_lang(FakeBackend::new(HOSTS), Lang::Ko);
    press(&mut app, KeyCode::Char('n'));
    assert_visible(&draw(&app, 100, 28), &["주소 입력 필요"]);
    press(&mut app, KeyCode::Enter);
    assert_visible(&draw(&app, 100, 28), &["주소 필수"]);

    set_field(&mut app, FormField::Address, "-server");
    assert_visible(&draw(&app, 100, 28), &["주소 첫 글자 '-' 사용 불가"]);
    set_field(&mut app, FormField::Address, "server");
    set_field(&mut app, FormField::Port, "65536");
    assert_visible(&draw(&app, 100, 28), &["포트는 1-65535 범위의 숫자만 허용"]);
    set_field(&mut app, FormField::Port, "22");
    set_field(&mut app, FormField::Nickname, "web");
    assert_visible(&draw(&app, 100, 28), &["이미 사용 중인 닉네임 \"web\""]);
}
