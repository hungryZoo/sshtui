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
            " sshtui  3 hosts",
            "│ web                ro│",
            "│ db                 ro│",
            "│ queue              ro│",
            "╭ web ",
            "User      alice",
            "Address   www.example.com",
            "Port      2222",
            "Key file  ~/.ssh/first",
            "│           ~/.ssh/second",
            "Source    ~/.ssh/config:2",
            "Read-only file",
            "Enter connect  n new  e edit  d delete  / filter  r reload  q quit",
        ],
    );
    assert!(view.buffer()[(1, 2)].modifier.contains(Modifier::BOLD));
    assert!(!view.buffer()[(1, 3)].modifier.contains(Modifier::BOLD));
}

#[test]
fn list_shows_filter_count_and_empty_results() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    press(&mut app, KeyCode::Char('/'));
    app.handle_paste("web");
    let view = draw(&app, 80, 20);
    contains(&view, &["1 of 3 hosts", "/web"]);
    assert_eq!(view.cursor_position(), (5, 2).into());

    app.handle_paste("xxxxx");
    let view = draw(&app, 80, 20);
    contains(&view, &["0 of 3 hosts", "/webxxxxx", "No matching hosts"]);

    press(&mut app, KeyCode::Enter);
    contains(&draw(&app, 80, 20), &["/webxxxxx", "No matching hosts"]);
}

#[test]
fn list_shows_all_key_files_above_the_source_and_read_only_label() {
    let mut backend = FakeBackend::new(HOSTS);
    backend.read_only = true;
    let rows = screen(&draw(&App::new(backend), 100, 20));
    let positions = [
        "╭ web ",
        "User      alice",
        "Address   www.example.com",
        "Port      2222",
        "Key file  ~/.ssh/first",
        "│           ~/.ssh/second",
        "Source    ~/.ssh/config:2",
        "Read-only file",
    ]
    .map(|label| rows.iter().position(|row| row.contains(label)).unwrap());
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
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
    let view = draw(&app, 100, 24);
    let rows = screen(&view);
    for (row, field, note) in [
        (2, "Nickname", "optional"),
        (3, "User", "optional"),
        (4, "Address", "required"),
        (5, "Port", "optional"),
        (6, "Key file", "optional"),
    ] {
        assert_eq!(
            (&rows[row][4..14].trim(), &rows[row][90..99].trim()),
            (&field, &note)
        );
    }
    contains(
        &view,
        &[
            "New connection",
            "(address is required)",
            "[ Keys F2 ]  [ Browse F3 ]",
            "Tab/Shift+Tab move  Enter connect  Esc back",
        ],
    );
    set_field(&mut app, FormField::User, "deploy");
    set_field(&mut app, FormField::Address, "www.example.com");
    set_field(&mut app, FormField::Port, "22");
    contains(
        &draw(&app, 100, 24),
        &["Command", "ssh -p 22 deploy@www.example.com"],
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
    contains(&draw(&app, 400, 24), &["Command", &command]);

    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('r'));
    press(&mut app, KeyCode::Char('n'));
    set_field(&mut app, FormField::Address, "www.example.com");
    contains(
        &draw(&app, 100, 24),
        &["Command", "ssh -F none www.example.com"],
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
            "User      deploy",
            "inherited: www.example.com",
            "inherited: 2222",
            "read-only",
            "  IdentityFile ~/one",
            "  IdentityFile ~/two",
            "Option changes also apply to docs. Renaming affects only web.",
            "Tab/Shift+Tab move  Enter save  Esc back",
        ],
    );
    assert_eq!(view.buffer()[(12, 6)].fg, Color::DarkGray);
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
            "[ Keys F2 ]",
            "Browse is unavailable in this session",
            "Hidden files: press Ctrl+H in the dialog",
        ],
    );
    let browse_row = screen(&view)
        .iter()
        .position(|line| line.contains("[ Browse F3 ]"))
        .unwrap() as u16;
    assert_eq!(view.buffer()[(15, browse_row)].fg, Color::DarkGray);
}

#[test]
fn form_shows_validation_errors_and_positions_the_cursor_in_korean_input() {
    let mut app = App::new(FakeBackend::new(""));
    set_field(&mut app, FormField::Address, "서버");
    let view = draw(&app, 100, 24);
    contains(&view, &["서", "버"]);
    assert_eq!(view.cursor_position(), (16, 4).into());

    set_field(&mut app, FormField::Port, "invalid");
    let view = draw(&app, 100, 24);
    let error = app.validation_error(FormField::Port).unwrap().to_string();
    contains(&view, &[&error]);
    let cursor = view.cursor_position();
    assert_eq!(cursor.x, 19);
    assert_eq!(view.buffer()[(12, cursor.y + 1)].fg, Color::Red);
}

#[test]
fn long_wide_input_scrolls_without_hiding_the_cursor() {
    let mut app = App::new(FakeBackend::new(""));
    let value = format!("{}끝", "가".repeat(50));
    set_field(&mut app, FormField::Address, &value);

    let view = draw(&app, 43, 20);
    let cursor = view.cursor_position();
    assert_eq!(cursor, (28, 4).into());
    assert_eq!(view.buffer()[(cursor.x - 2, cursor.y)].symbol(), "끝");

    press(&mut app, KeyCode::Home);
    let view = draw(&app, 43, 20);
    assert_eq!(view.cursor_position(), (12, 4).into());
    assert_eq!(view.buffer()[(12, 4)].symbol(), "가");
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

#[test]
fn confirmation_shows_numbered_colored_diff_and_scrolls_by_diff_lines() {
    let mut app = edit_confirmation();
    let view = draw(&app, 120, 16);
    contains(
        &view,
        &[
            "Save web",
            "~/.ssh/config",
            "   3      -   User alice",
            "        3 +   User deploy",
            "y/Enter confirm  n/Esc cancel  Up/Down scroll",
        ],
    );
    assert_eq!(view.buffer()[(12, 6)].fg, Color::Red);
    assert_eq!(view.buffer()[(12, 7)].fg, Color::Green);

    press(&mut app, KeyCode::Down);
    let view = draw(&app, 120, 16);
    assert!(screen(&view)[4].contains("HostName"));
}

#[test]
fn confirmation_abbreviates_a_path_under_the_home_directory() {
    let app = confirm_edit(FakeBackend::with_ssh_dir(HOSTS, Some("work-ssh")));
    let rows = screen(&draw(&app, 80, 16));
    assert_eq!(rows[2].trim_matches('│').trim(), "~/work-ssh/config");
}

#[test]
fn confirmation_wraps_the_path_at_cell_width_and_caps_it_at_two_rows() {
    let dir = format!("{}한{}", "a".repeat(27), "b".repeat(50));
    let app = confirm_edit(FakeBackend::with_ssh_dir(HOSTS, Some(&dir)));
    let view = draw(&app, 34, 16);
    let rows = screen(&view);
    assert_eq!(
        rows[2].trim_matches('│').trim(),
        format!("~/{}", "a".repeat(27))
    );
    assert_eq!(view.buffer()[(2, 3)].symbol(), "한");
    assert_eq!(
        rows[3].trim_matches('│').trim(),
        format!("한 {}", "b".repeat(28))
    );
    assert!(rows[5].contains("# Saved hosts"), "{rows:?}");
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
    assert_eq!(view.buffer()[(14, selected)].bg, Color::Cyan);
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
        (info, "Reloaded", Color::Green),
        (warning, "is read-only", Color::Yellow),
        (error, "address is required", Color::Red),
    ] {
        let view = draw(&app, 100, 20);
        assert!(screen(&view)[18].contains(message));
        assert_eq!(view.buffer()[(0, 18)].fg, color);
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
    assert_eq!(rows[13], path[..40]);
    assert!(
        rows.join(" ").contains("was changed by another program"),
        "{rows:?}"
    );
    assert!(rows[14].ends_with("another program"));

    let narrower = screen(&draw(&app, 39, 16));
    assert!(narrower[13].ends_with("another"));
    assert_eq!(narrower[14], "program");
}

#[test]
fn list_gives_up_no_rows_without_status_and_one_row_for_short_status() {
    let mut app = App::new(FakeBackend::new(MANY_HOSTS));
    let rows = screen(&draw(&app, 80, 9));
    for (row, host) in rows[2..7]
        .iter()
        .zip(["web", "db", "queue", "cache", "proxy"])
    {
        assert_eq!(row.split_whitespace().nth(1), Some(host));
    }

    press(&mut app, KeyCode::Char('r'));
    let rows = screen(&draw(&app, 80, 9));
    for (row, host) in rows[2..6].iter().zip(["web", "db", "queue", "cache"]) {
        assert_eq!(row.split_whitespace().nth(1), Some(host));
    }
    assert_eq!(rows[7], "Reloaded");
    assert!(!rows.iter().any(|row| row.starts_with("│ proxy")));
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
    let view = draw(&app, 40, 10);
    let rows = screen(&view);
    assert!(rows[2].starts_with("│ web"));
    assert!(rows[3].starts_with("│ db"));
    assert!(rows[4].starts_with("│ queue"));
    for y in 6..9 {
        assert!(!rows[usize::from(y)].is_empty());
        assert_eq!(view.buffer()[(0, y)].fg, Color::Red);
    }
    assert!(!rows.join(" ").contains("reloaded, try again"));
    assert!(rows[9].starts_with("Enter connect"));
}

#[test]
fn long_status_renders_in_small_terminals() {
    let app = stale_edit();
    for (width, height) in [(1, 1), (5, 2), (20, 5)] {
        let rows = screen(&draw(&app, width, height));
        assert!(
            rows[0]
                .trim_start()
                .starts_with(&"sshtui"[..usize::from(width.min(6))])
        );
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
        .position(|line| line.starts_with("│ host-34"))
        .unwrap();
    assert_eq!(view.buffer()[(1, last_host as u16)].bg, Color::Cyan);
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
fn update_notice_and_help_appear_only_with_a_plan() {
    let notice = "v2.0.0 available (u: update)";
    let width = 100;
    let app = update_fixture(InstallMethod::Brew);
    let view = draw(&app, width, 20);
    let rows = screen(&view);
    assert!(rows[0].starts_with(" sshtui  3 hosts"));
    assert!(rows[0].ends_with(notice));
    assert!(rows[19].ends_with("r reload  u update  q quit"));

    let buffer = view.buffer();
    let start = width - notice.len() as u16;
    assert!(!buffer[(start - 1, 0)].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[(8, 0)].fg, Color::Reset);
    for x in start..width {
        assert_eq!(buffer[(x, 0)].fg, Color::Yellow);
        assert!(buffer[(x, 0)].modifier.contains(Modifier::BOLD));
    }

    let app = App::new(FakeBackend::new(HOSTS));
    let rows = screen(&draw(&app, width, 20));
    assert!(!rows[0].contains(notice));
    assert!(!rows[19].contains("u update"));
}

#[test]
fn update_notice_is_hidden_when_it_would_not_fit() {
    let app = update_fixture(InstallMethod::Brew);
    let rows = screen(&draw(&app, 30, 10));
    assert_eq!(rows[0], " sshtui  3 hosts");
}

#[test]
fn update_modal_shows_versions_installer_commands_and_confirmation() {
    let mut app = update_fixture(InstallMethod::Cargo);
    press(&mut app, KeyCode::Char('u'));
    let command = format!(
        "cargo install --git {} --tag v2.0.0 --locked",
        crate::update::REPOSITORY
    );
    let view = draw(&app, 120, 20);
    contains(
        &view,
        &[
            "Update sshtui",
            "Current        1.0.0",
            "Latest         2.0.0",
            "Installed via  cargo",
            "Command",
            &command,
            "Enter update and exit  Esc cancel",
        ],
    );

    let wrapped = screen(&draw(&app, 45, 20));
    assert!(
        wrapped
            .iter()
            .any(|row| row.contains("cargo install --git"))
    );
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
    let view = draw(&app, 120, 20);
    contains(
        &view,
        &[
            "Update sshtui",
            "Installed via  unknown (manual install)",
            &format!("Download       {}", latest_url()),
            "Esc close",
        ],
    );
    assert!(!screen(&view).join("\n").contains("Enter update"));
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
fn list_panels_are_rounded_and_detail_values_share_one_column() {
    let view = draw(&App::new(FakeBackend::new(HOSTS)), 80, 16);
    let rows = screen(&view);
    assert!(rows[1].starts_with("╭ Hosts "));
    assert!(rows[1].contains("╮╭ web "));
    assert!(rows[14].contains("╯╰"));
    assert_eq!(view.buffer()[(0, 1)].fg, Color::Cyan);
    assert_eq!(view.buffer()[(24, 1)].fg, Color::DarkGray);
    for (y, label, value) in [
        (2, "User", "alice"),
        (3, "Address", "www.example.com"),
        (4, "Port", "2222"),
        (5, "Key file", "~/.ssh/first"),
        (6, "", "~/.ssh/second"),
        (7, "", ""),
        (8, "Source", "~/.ssh/config:2"),
    ] {
        let details = rows[y].split('│').nth(3).unwrap();
        assert_eq!(details[1..11].trim(), label);
        assert_eq!(details[11..].trim_end(), value);
    }
}

#[test]
fn list_selection_styles_the_entire_inner_row() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    press(&mut app, KeyCode::Down);
    let view = draw(&app, 80, 16);
    for x in 1..23 {
        let selected = &view.buffer()[(x, 3)];
        assert_eq!((selected.fg, selected.bg), (Color::Black, Color::Cyan));
        assert!(selected.modifier.contains(Modifier::BOLD));
        assert_eq!(view.buffer()[(x, 2)].bg, Color::Reset);
    }
    assert_eq!(view.buffer()[(0, 3)].bg, Color::Reset);
    assert_eq!(view.buffer()[(23, 3)].bg, Color::Reset);
}

#[test]
fn list_shows_details_starting_at_fifty_columns() {
    let app = App::new(FakeBackend::new(HOSTS));
    let narrow = draw(&app, 49, 16);
    assert_eq!(narrow.buffer()[(48, 1)].symbol(), "╮");
    assert!(!screen(&narrow).join("\n").contains("Address"));
    let wide = draw(&app, 50, 16);
    contains(
        &wide,
        &["╮╭ web ", "Address   www.example.", "│           com"],
    );
}

#[test]
fn list_read_only_marker_is_yellow_and_right_aligned() {
    let mut backend = FakeBackend::new(HOSTS);
    backend.read_only = true;
    let view = draw(&App::new(backend), 80, 16);
    let buffer = view.buffer();
    for (x, symbol) in [(21, "r"), (22, "o")] {
        assert_eq!(buffer[(x, 3)].symbol(), symbol);
        assert_eq!(buffer[(x, 3)].fg, Color::Yellow);
        assert_eq!(buffer[(x, 2)].bg, Color::Cyan);
        assert_eq!(buffer[(x, 2)].fg, Color::Yellow);
    }
    assert_eq!(buffer[(23, 3)].symbol(), "│");
}

#[test]
fn header_badge_stops_before_the_screen_title() {
    let view = draw(&App::new(FakeBackend::new(HOSTS)), 80, 16);
    let rows = screen(&view);
    assert_eq!(rows[0], " sshtui  3 hosts");
    assert!(!rows[0].contains('·'));
    for x in 0..8 {
        let badge = &view.buffer()[(x, 0)];
        assert_eq!((badge.fg, badge.bg), (Color::Black, Color::Cyan));
        assert!(badge.modifier.contains(Modifier::BOLD));
    }
    for x in 8..16 {
        let title = &view.buffer()[(x, 0)];
        assert_eq!(title.bg, Color::Reset);
        assert!(!title.modifier.contains(Modifier::BOLD));
    }
}

#[test]
fn help_styles_keys_and_descriptions_independently() {
    let view = draw(&App::new(FakeBackend::new(HOSTS)), 80, 16);
    for x in 0..5 {
        let key = &view.buffer()[(x, 15)];
        assert_eq!(key.fg, Color::Cyan);
        assert!(key.modifier.contains(Modifier::BOLD));
    }
    for x in 6..13 {
        let description = &view.buffer()[(x, 15)];
        assert_eq!(description.fg, Color::DarkGray);
        assert!(!description.modifier.contains(Modifier::BOLD));
    }
    assert_eq!(view.buffer()[(13, 15)].fg, Color::Reset);
    assert_eq!(view.buffer()[(15, 15)].symbol(), "n");
    assert_eq!(view.buffer()[(15, 15)].fg, Color::Cyan);
}

#[test]
fn form_underlines_the_value_and_padding_with_the_focus_color() {
    let mut app = App::new(FakeBackend::new(""));
    set_field(&mut app, FormField::Address, "example.com");
    let view = draw(&app, 80, 24);
    let buffer = view.buffer();
    for x in 12..67 {
        assert!(buffer[(x, 4)].modifier.contains(Modifier::UNDERLINED));
        assert_eq!(buffer[(x, 4)].fg, Color::Cyan);
        assert_eq!(buffer[(x, 3)].modifier, Modifier::UNDERLINED);
        assert_eq!(buffer[(x, 3)].fg, Color::Reset);
    }
    assert_eq!(buffer[(12, 4)].symbol(), "e");
    assert_eq!(buffer[(66, 4)].symbol(), " ");
    assert_eq!(buffer[(69, 4)].fg, Color::DarkGray);
    assert!(!buffer[(69, 4)].modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn form_places_nonempty_field_errors_directly_below_the_input() {
    let mut app = App::new(FakeBackend::new(""));
    let view = draw(&app, 80, 24);
    assert!(screen(&view)[5].starts_with("│ Port"));
    set_field(&mut app, FormField::Address, "-x");
    let view = draw(&app, 80, 24);
    let error = app
        .validation_error(FormField::Address)
        .unwrap()
        .to_string();
    let rows = screen(&view);
    assert!(rows[4].contains("Address   -x"));
    assert!(rows[5].contains(&error));
    assert_eq!(view.buffer()[(12, 5)].fg, Color::Red);
    assert!(rows[6].starts_with("│ Port"));
}

#[test]
fn form_buttons_reflect_the_action_and_browse_availability() {
    let app = App::new(FakeBackend::new(""));
    let view = draw(&app, 80, 24);
    contains(
        &view,
        &["[ Keys F2 ]", "[ Browse F3 ]", "[ Connect ]", "[ Cancel ]"],
    );
    let actions = screen(&view)
        .iter()
        .position(|row| row.contains("[ Connect ]"))
        .unwrap() as u16;
    assert_eq!(view.buffer()[(2, actions)].bg, Color::Cyan);
    assert_eq!(view.buffer()[(15, actions)].bg, Color::Reset);

    let mut backend = FakeBackend::new(HOSTS);
    backend.unavailable = Some("Browse is unavailable in this session".into());
    let mut app = App::new(backend);
    press(&mut app, KeyCode::Char('e'));
    let view = draw(&app, 80, 24);
    contains(
        &view,
        &[
            "[ Save ]",
            "[ Cancel ]",
            "Browse is unavailable in this session",
        ],
    );
    let keys = screen(&view)
        .iter()
        .position(|row| row.contains("[ Browse F3 ]"))
        .unwrap() as u16;
    assert_eq!(view.buffer()[(15, keys)].fg, Color::DarkGray);
}

#[test]
fn form_command_panel_styles_preview_errors_and_live_commands() {
    let mut app = App::new(FakeBackend::new(""));
    for (text, color) in [
        ("(address is required)", Color::Red),
        ("ssh www.example.com", Color::Reset),
    ] {
        let view = draw(&app, 80, 24);
        let rows = screen(&view);
        let title = rows
            .iter()
            .position(|row| row.contains("╭ Command "))
            .unwrap() as u16;
        assert!(rows[usize::from(title + 1)].contains(text));
        let first_content = (3..10)
            .find(|&x| view.buffer()[(x, title + 1)].symbol() == &text[..1])
            .unwrap();
        assert_eq!(view.buffer()[(first_content, title + 1)].fg, color);
        assert!(rows[usize::from(title + 2)].contains('╰'));
        set_field(&mut app, FormField::Address, "www.example.com");
    }
}

#[test]
fn confirmation_puts_buttons_below_the_diff_and_styles_its_columns() {
    let view = draw(&edit_confirmation(), 100, 16);
    let rows = screen(&view);
    assert!(rows[2].contains("~/.ssh/config"));
    assert!(rows[13].contains("[ Confirm y ]  [ Cancel n ]"));
    assert_eq!(view.buffer()[(2, 2)].fg, Color::DarkGray);
    assert_eq!(view.buffer()[(2, 13)].bg, Color::Cyan);
    assert_eq!(view.buffer()[(17, 13)].bg, Color::Reset);
    assert_eq!(view.buffer()[(5, 6)].fg, Color::DarkGray);
    assert_eq!(view.buffer()[(12, 6)].fg, Color::Red);
    assert_eq!(view.buffer()[(10, 7)].fg, Color::DarkGray);
    assert_eq!(view.buffer()[(12, 7)].fg, Color::Green);
    assert_eq!(view.buffer()[(12, 5)].fg, Color::Reset);
}

fn popup_over_fill(app: &App<FakeBackend>, fill: &str) -> TestBackend {
    use ratatui::layout::Rect;
    use ratatui::text::Span;

    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal
        .draw(|frame| {
            let body = Rect::new(0, 1, 100, 22);
            for row in body.rows() {
                frame.render_widget(Span::raw(fill.repeat(100)), row);
            }
            match app.mode() {
                Mode::KeyPicker(state) => super::overlays::key_picker(frame, state, body),
                Mode::Notice(state) => super::overlays::notice(frame, state, body),
                Mode::Update(state) => super::overlays::update(frame, state, body),
                _ => unreachable!("expected a popup"),
            }
        })
        .unwrap();
    terminal.backend().clone()
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
        (picker, &["Enter select  Esc cancel"][..]),
        (connection_notice(), &["[ Connect anyway ]", "[ Back ]"][..]),
        (acknowledgment, &["[ OK ]"][..]),
        (update, &["[ Update and exit ]", "[ Cancel ]"][..]),
        (manual, &["[ Close ]"][..]),
    ] {
        contains(&draw(&app, 100, 24), actions);
        let clean = popup_over_fill(&app, " ");
        let dirty = popup_over_fill(&app, "#");
        let bounds = |symbol| {
            let mut cells = clean
                .buffer()
                .area
                .rows()
                .flat_map(|row| (row.left()..row.right()).map(move |x| (x, row.y)));
            cells
                .find(|&position| clean.buffer()[position].symbol() == symbol)
                .unwrap()
        };
        let (left, top) = bounds("╭");
        let (right, bottom) = bounds("╯");
        assert_eq!(dirty.buffer()[(left - 1, top)].symbol(), "#");
        assert_eq!(dirty.buffer()[(left, top)].fg, Color::Cyan);
        assert!(
            dirty.buffer()[(left + 2, top)]
                .modifier
                .contains(Modifier::BOLD)
        );
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
    for (app, color) in [
        (info, Color::Green),
        (warning, Color::Yellow),
        (error, Color::Red),
    ] {
        let view = draw(&app, 200, 24);
        assert_eq!(view.buffer()[(0, 22)].fg, color);
        assert_eq!(view.buffer()[(0, 23)].fg, Color::Cyan);
        assert_eq!(view.buffer()[(199, 23)].fg, Color::Reset);
    }
}

#[test]
fn all_screen_modes_draw_at_the_required_terminal_sizes() {
    let mut edited = App::new(FakeBackend::new(HOSTS));
    press(&mut edited, KeyCode::Char('e'));
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
        App::new(FakeBackend::new(HOSTS)),
        App::new(FakeBackend::new("")),
        edited,
        edit_confirmation(),
        delete,
        picker,
        connection_notice(),
        acknowledgment,
        update,
        manual,
    ];
    for (width, height) in [(1, 1), (5, 2), (20, 5), (40, 10), (80, 24), (200, 60)] {
        for app in &states {
            assert_eq!(
                draw(app, width, height).buffer().area,
                ratatui::layout::Rect::new(0, 0, width, height)
            );
        }
    }
}

#[test]
fn short_forms_hide_actions_before_preview_and_key_buttons() {
    let mut app = App::new(FakeBackend::new(""));
    set_field(&mut app, FormField::KeyFile, "");
    for (height, expected) in [
        (14, [true, true, true]),
        (13, [false, true, true]),
        (10, [false, false, true]),
        (9, [false, false, false]),
        (8, [false, false, false]),
    ] {
        let view = draw(&app, 80, height);
        let rows = screen(&view);
        let visible = rows.join("\n");
        for (element, shown) in ["[ Connect ]", "╭ Command ", "[ Keys F2 ]"]
            .into_iter()
            .zip(expected)
        {
            assert_eq!(
                visible.contains(element),
                shown,
                "height {height}, element {element}"
            );
        }
        let cursor = view.cursor_position();
        assert!(rows[usize::from(cursor.y)].contains("Key file"));
        assert!(
            view.buffer()[(cursor.x, cursor.y)]
                .modifier
                .contains(Modifier::UNDERLINED)
        );
    }
}

#[test]
fn inherited_form_input_uses_muted_text_and_a_focused_underline() {
    let mut app = App::new(FakeBackend::new(HOSTS));
    press(&mut app, KeyCode::Char('e'));
    press(&mut app, KeyCode::Tab);
    let view = draw(&app, 80, 24);
    let input = &view.buffer()[(12, 3)];
    assert_eq!(input.symbol(), "i");
    assert_eq!(input.fg, Color::DarkGray);
    assert!(input.modifier.contains(Modifier::UNDERLINED));
    assert_eq!(input.underline_color, Color::Cyan);
}
