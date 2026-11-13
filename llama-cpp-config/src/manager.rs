//! The command line of `private-ai-manager`, the runtime service binary
//! (PLAN.md step 1). `run-instance <id>` is what the unit `llama.cpp@<id>`
//! runs: it launches instance `id` (`runstate::instance_command`).

pub mod service;

/// A parsed `private-ai-manager` command line.
#[derive(Debug, PartialEq, Eq)]
pub enum ManagerCommand {
    /// `run-instance <id>`.
    RunInstance(String),
    /// `serve`: the D-Bus / named-pipe service entry.
    Serve,
}

/// Why a command line was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum ArgsError {
    /// No subcommand at all.
    NoCommand,
    /// A subcommand that does not exist.
    UnknownCommand(String),
    /// `run-instance` without its id.
    MissingId,
    /// The first argument past a complete command.
    ExtraArgument(String),
}

/// Parse the arguments after the program name.
pub fn parse_args(args: &[String]) -> Result<ManagerCommand, ArgsError> {
    let (command, rest) = args.split_first().ok_or(ArgsError::NoCommand)?;
    if command == "serve" {
        if let Some(arg) = rest.first() {
            return Err(ArgsError::ExtraArgument(arg.clone()));
        }
        return Ok(ManagerCommand::Serve);
    }
    if command != "run-instance" {
        return Err(ArgsError::UnknownCommand(command.clone()));
    }
    let (id, extra) = rest.split_first().ok_or(ArgsError::MissingId)?;
    if let Some(arg) = extra.first() {
        return Err(ArgsError::ExtraArgument(arg.clone()));
    }
    Ok(ManagerCommand::RunInstance(id.clone()))
}

/// The `private-ai-manager` entry point, returning the exit code: on Unix
/// the process becomes llama-server and returns only on failure; on Windows
/// it waits for llama-server and returns its code. A failure prints one
/// error line on stderr.
pub fn main() -> i32 {
    crate::strings::set_reader(crate::strings::Reader::Cli);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (code, message) = match run(&args) {
        Ok(code) => return code,
        Err(failure) => failure,
    };
    eprintln!("{}", crate::strings::fill("error-line", &[&message]));
    code
}

/// What `main` does, its failure as an exit code and a message.
fn run(args: &[String]) -> Result<i32, (i32, String)> {
    use crate::runstate::LaunchError;
    use crate::strings;

    let command = parse_args(args).map_err(|e| {
        let text = match e {
            ArgsError::NoCommand => strings::t("manager-usage"),
            ArgsError::UnknownCommand(c) => strings::fill("manager-unknown-command", &[&c]),
            ArgsError::MissingId => strings::t("manager-missing-id"),
            ArgsError::ExtraArgument(a) => strings::fill("manager-extra-argument", &[&a]),
        };
        (2, text)
    })?;
    let id = match command {
        ManagerCommand::RunInstance(id) => id,
        ManagerCommand::Serve => return Err((1, strings::t("manager-serve-unavailable"))),
    };
    let exe = crate::paths::llama_server_exe().ok_or_else(|| {
        let name = crate::paths::binary_name("llama-server");
        (1, strings::fill("binary-not-found", &[&name]))
    })?;
    let mut cmd = crate::runstate::instance_command(&id, &exe).map_err(|e| {
        let text = match e {
            LaunchError::InvalidId => strings::fill("manager-invalid-id", &[&id]),
            LaunchError::NoConfig(p) => strings::fill("manager-no-config", &[&p.display(), &id]),
            LaunchError::NoPresets => {
                let presets = crate::paths::system_config_dir().join("presets.ini");
                strings::fill("manager-no-presets", &[&presets.display()])
            }
            LaunchError::StateDir(p, kind) => {
                strings::fill("manager-state-dir", &[&p.display(), &kind])
            }
        };
        (1, text)
    })?;
    let failed = |e: std::io::Error| {
        (
            1,
            strings::fill("manager-launch-failed", &[&exe.display(), &e]),
        )
    };
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Returns only when the exec failed.
        Err(failed(cmd.exec()))
    }
    #[cfg(not(unix))]
    {
        let status = cmd.status().map_err(failed)?;
        Ok(status.code().unwrap_or(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<ManagerCommand, ArgsError> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_args(&args)
    }

    #[test]
    fn parse_args_refuses_no_command() {
        assert_eq!(parse(&[]), Err(ArgsError::NoCommand));
    }

    #[test]
    fn parse_args_refuses_run_instance_without_id() {
        assert_eq!(parse(&["run-instance"]), Err(ArgsError::MissingId));
    }

    #[test]
    fn parse_args_refuses_extra_arguments() {
        assert_eq!(
            parse(&["run-instance", "default", "lan-2"]),
            Err(ArgsError::ExtraArgument("lan-2".into()))
        );
        assert_eq!(
            parse(&["run-instance", "default", "--port", "8080"]),
            Err(ArgsError::ExtraArgument("--port".into()))
        );
    }

    #[test]
    fn parse_args_refuses_unknown_command() {
        for cmd in [
            "start",
            "Run-Instance",
            "run_instance",
            "--run-instance",
            "",
        ] {
            assert_eq!(
                parse(&[cmd, "default"]),
                Err(ArgsError::UnknownCommand(cmd.into())),
                "subcommand {cmd:?}"
            );
        }
    }

    #[test]
    fn parse_args_reads_serve() {
        assert_eq!(parse(&["serve"]), Ok(ManagerCommand::Serve));
    }

    #[test]
    fn parse_args_refuses_serve_with_arguments() {
        assert_eq!(
            parse(&["serve", "default"]),
            Err(ArgsError::ExtraArgument("default".into()))
        );
        assert_eq!(
            parse(&["serve", "--system"]),
            Err(ArgsError::ExtraArgument("--system".into()))
        );
        for cmd in ["Serve", "--serve", "serve-all"] {
            assert_eq!(
                parse(&[cmd]),
                Err(ArgsError::UnknownCommand(cmd.into())),
                "subcommand {cmd:?}"
            );
        }
    }

    #[test]
    fn parse_args_reads_run_instance() {
        assert_eq!(
            parse(&["run-instance", "default"]),
            Ok(ManagerCommand::RunInstance("default".into()))
        );
        assert_eq!(
            parse(&["run-instance", "lan-2"]),
            Ok(ManagerCommand::RunInstance("lan-2".into()))
        );
    }
}
