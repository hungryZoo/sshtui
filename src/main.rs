use std::process::ExitCode;

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
        CliAction::Run => {
            eprintln!("sshtui: the TUI is not implemented yet");
            ExitCode::FAILURE
        }
        CliAction::Version => {
            println!("sshtui {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        CliAction::Help => {
            println!("Usage: sshtui [OPTIONS]");
            println!("  -h, --help     Print help");
            println!("  -V, --version  Print version");
            ExitCode::SUCCESS
        }
        CliAction::Invalid(arg) => {
            eprintln!("sshtui: unknown argument {arg:?}");
            ExitCode::from(2)
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
