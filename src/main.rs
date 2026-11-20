use std::io::{self, IsTerminal};
use std::process::ExitCode;

use sshtui::app::{App, Exit};
use sshtui::paths::SshPaths;
use sshtui::ssh::exec::{find_ssh, run_interactive};
use sshtui::system::SystemBackend;
use sshtui::{dialog, tui};

#[derive(Debug, PartialEq, Eq)]
enum CliAction {
    Run,
    Version,
    Help,
    Invalid(String),
}

fn parse_args(args: &[String]) -> CliAction {
    match args {
        [] => CliAction::Run,
        [flag] if flag == "--version" || flag == "-V" => CliAction::Version,
        [flag] if flag == "--help" || flag == "-h" => CliAction::Help,
        [flag, extra, ..] if matches!(flag.as_str(), "--version" | "-V" | "--help" | "-h") => {
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
            println!("sshtui {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        CliAction::Help => {
            println!("Usage: sshtui [OPTIONS]");
            println!("  -h, --help     Print help");
            println!("  -V, --version  Print version");
            println!("  SSHTUI_SSH_DIR  Override the SSH directory (default: ~/.ssh)");
            ExitCode::SUCCESS
        }
        CliAction::Invalid(arg) => {
            eprintln!("sshtui: unknown argument {arg:?}");
            ExitCode::from(2)
        }
    }
}

fn run() -> anyhow::Result<ExitCode> {
    let paths = SshPaths::from_env()?;
    let ssh = find_ssh()?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        anyhow::bail!("an interactive terminal is required");
    }

    let backend = SystemBackend::new(paths, ssh.clone(), dialog::system_picker());
    match tui::run(App::new(backend))? {
        Exit::Quit => Ok(ExitCode::SUCCESS),
        Exit::Connect(args) => {
            let code = run_interactive(&ssh, &args)?;
            Ok(u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CliAction, parse_args};

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
}
