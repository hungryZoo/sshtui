use std::ffi::OsStr;
use std::io::{self, IsTerminal};
use std::process::ExitCode;

use sshtui::app::{App, Exit};
use sshtui::paths::SshPaths;
use sshtui::ssh::exec::{find_ssh, run_interactive};
use sshtui::system::SystemBackend;
use sshtui::ui::ColorMode;
use sshtui::update::{self, CURRENT_VERSION, CheckOutcome, InstallMethod, UpdatePlan};
use sshtui::{dialog, history, i18n, state, tui};

const NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, PartialEq, Eq)]
enum CliAction {
    Run,
    Version,
    Help,
    CheckUpdate,
    Update,
    Invalid(String),
}

fn parse_args(args: &[String]) -> CliAction {
    match args {
        [] => CliAction::Run,
        [flag] if flag == "--version" || flag == "-V" => CliAction::Version,
        [flag] if flag == "--help" || flag == "-h" => CliAction::Help,
        [flag] if flag == "--check-update" => CliAction::CheckUpdate,
        [flag] if flag == "--update" => CliAction::Update,
        [flag, extra, ..]
            if matches!(
                flag.as_str(),
                "--version" | "-V" | "--help" | "-h" | "--check-update" | "--update"
            ) =>
        {
            CliAction::Invalid(extra.clone())
        }
        [first, ..] => CliAction::Invalid(first.clone()),
    }
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    match parse_args(&args) {
        CliAction::Run => run().unwrap_or_else(|error| {
            eprintln!("sshtui: {error}");
            ExitCode::FAILURE
        }),
        CliAction::Version => {
            println!("{NAME} {CURRENT_VERSION}");
            ExitCode::SUCCESS
        }
        CliAction::Help => {
            println!("Usage: {NAME} [OPTIONS]");
            println!("  -h, --help      Print help");
            println!("  -V, --version   Print version");
            println!("  --check-update  Check for a new release");
            println!("  --update        Check for a new release and install it if available");
            println!("  SSHTUI_SSH_DIR  Override the SSH directory (default: ~/.ssh)");
            println!("  {}     Force the TUI language (ko or en)", i18n::LANG_ENV);
            println!(
                "  SSHTUI_NO_MOUSE  Disable mouse support (keeps the terminal's own text selection)"
            );
            println!(
                "  {}  Disable shell history import when nonempty and not 0",
                history::DISABLE_ENV
            );
            println!(
                "  {}  Use one history file instead of the default shell history files",
                history::HISTORY_FILE_ENV
            );
            println!(
                "  {}  Disable automatic update checks and notices when nonempty and not 0",
                update::DISABLE_ENV
            );
            ExitCode::SUCCESS
        }
        CliAction::CheckUpdate => check_update(false),
        CliAction::Update => check_update(true),
        CliAction::Invalid(arg) => {
            eprintln!("sshtui: unknown argument {arg:?}");
            ExitCode::from(2)
        }
    }
}

fn check_update(apply: bool) -> ExitCode {
    let outcome = update::check_now();
    let method = update::detect_install_method();
    println!("{}", update::report(&outcome, CURRENT_VERSION, &method));
    match outcome {
        CheckOutcome::UpToDate { .. } => ExitCode::SUCCESS,
        CheckOutcome::Unreachable => ExitCode::from(11),
        CheckOutcome::UpdateAvailable { .. } if !apply => ExitCode::from(10),
        CheckOutcome::UpdateAvailable { .. } if method == InstallMethod::Unknown => {
            eprintln!(
                "{NAME}: unknown installation method. Update manually from {}",
                update::latest_url()
            );
            ExitCode::FAILURE
        }
        CheckOutcome::UpdateAvailable { latest } => apply_update(&UpdatePlan {
            current: CURRENT_VERSION.into(),
            latest,
            method,
        }),
    }
}

fn apply_update(plan: &UpdatePlan) -> ExitCode {
    match update::run_upgrade(plan) {
        Ok(true) => {
            println!("{NAME}: updated. Start {NAME} again to use the new version.");
            ExitCode::SUCCESS
        }
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("{NAME}: update failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn mouse_enabled(value: Option<&OsStr>) -> bool {
    value.is_none_or(|value| value.is_empty() || value == "0")
}

fn color_mode(colorterm: Option<&OsStr>) -> ColorMode {
    match colorterm.and_then(OsStr::to_str) {
        Some(value)
            if value.eq_ignore_ascii_case("truecolor") || value.eq_ignore_ascii_case("24bit") =>
        {
            ColorMode::TrueColor
        }
        _ => ColorMode::Ansi,
    }
}

fn run() -> anyhow::Result<ExitCode> {
    let paths = SshPaths::from_env()?;
    let ssh = find_ssh()?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        anyhow::bail!("an interactive terminal is required");
    }

    let plan = update::plan();
    update::maybe_spawn_check();
    let history_sources = history::history_sources(
        dirs::home_dir().as_deref(),
        dirs::config_dir().as_deref(),
        &|name| std::env::var_os(name),
    );
    let backend = SystemBackend::new(paths, ssh.clone(), dialog::system_picker())
        .with_update_plan(plan)
        .with_state_file(state::default_state_file())
        .with_history_sources(history_sources);
    let options = tui::Options {
        mouse: mouse_enabled(std::env::var_os("SSHTUI_NO_MOUSE").as_deref()),
        color: color_mode(std::env::var_os("COLORTERM").as_deref()),
    };
    let app = App::with_lang(
        backend,
        i18n::detect(
            &|name| std::env::var_os(name),
            i18n::system_locale().as_deref(),
        ),
    );
    match tui::run(app, options)? {
        Exit::Quit => Ok(ExitCode::SUCCESS),
        Exit::Connect(args) => {
            let code = run_interactive(&ssh, &args)?;
            Ok(u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from))
        }
        Exit::Update(plan) => Ok(apply_update(&plan)),
    }
}

#[cfg(test)]
mod tests {
    use super::{CliAction, OsStr, mouse_enabled, parse_args};

    #[test]
    fn chooses_true_color_only_for_supported_colorterm_values() {
        use super::{ColorMode, color_mode};

        for value in ["truecolor", "24bit", "TrueColor", "24BIT"] {
            assert_eq!(color_mode(Some(OsStr::new(value))), ColorMode::TrueColor);
        }
        for value in [None, Some(""), Some("256color")] {
            assert_eq!(color_mode(value.map(OsStr::new)), ColorMode::Ansi);
        }
    }

    #[test]
    fn mouse_is_enabled_unless_the_environment_value_is_nonempty_and_nonzero() {
        for (value, enabled) in [
            (None, true),
            (Some(""), true),
            (Some("0"), true),
            (Some("1"), false),
        ] {
            assert_eq!(mouse_enabled(value.map(OsStr::new)), enabled);
        }
    }

    #[test]
    fn no_arguments_runs_tui() {
        assert_eq!(parse_args(&[]), CliAction::Run);
    }

    #[test]
    fn version_flag_shows_version() {
        assert_eq!(parse_args(&["--version".into()]), CliAction::Version);
    }

    #[test]
    fn short_help_flag_shows_help() {
        assert_eq!(parse_args(&["-h".into()]), CliAction::Help);
    }

    #[test]
    fn unknown_argument_is_invalid() {
        assert_eq!(
            parse_args(&["--unknown".into()]),
            CliAction::Invalid("--unknown".into())
        );
    }

    #[test]
    fn update_flags_must_be_used_alone() {
        for (flag, action) in [
            ("--check-update", CliAction::CheckUpdate),
            ("--update", CliAction::Update),
        ] {
            assert_eq!(parse_args(&[flag.into()]), action);
            assert_eq!(
                parse_args(&[flag.into(), "--help".into()]),
                CliAction::Invalid("--help".into())
            );
        }
        assert_eq!(
            parse_args(&["--update".into(), "--check-update".into()]),
            CliAction::Invalid("--check-update".into())
        );
    }
}
