//! The command line of `pai-manager`, the runtime service binary that
//! runs llama-server instances as services. `run-instance <id>` is what the
//! unit `llama.cpp@<id>` runs: it launches instance `id`
//! (`launch::instance_command`). `serve` is the manager's own service: the
//! D-Bus interface on the system bus (`manager::dbus`, Linux only).

#[cfg(target_os = "linux")]
pub mod dbus;
pub mod pipe;
pub mod service;
// The Windows side's decisions, OS-neutral (the Win32 glue is cfg(windows)).
pub mod windows;

/// A parsed `pai-manager` command line.
#[derive(Debug, PartialEq, Eq)]
pub enum ManagerCommand {
    /// `run-instance <id>`.
    RunInstance(String),
    /// `serve`: the D-Bus / named-pipe service entry.
    Serve,
    /// `uninstall-services`: stop and delete every instance's service (the
    /// MSI, when it removes or repairs the manager's files).
    UninstallServices,
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
    if command == "uninstall-services" {
        if let Some(arg) = rest.first() {
            return Err(ArgsError::ExtraArgument(arg.clone()));
        }
        return Ok(ManagerCommand::UninstallServices);
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

/// The `pai-manager` entry point, returning the exit code: on Unix
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
    use crate::launch::LaunchError;
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
        // The MSI ignores the outcome (its log keeps what is printed): a
        // service left behind is deleted by the next manager's own sync.
        #[cfg(windows)]
        ManagerCommand::UninstallServices => {
            let exe = std::env::current_exe()
                .map_err(|e| (1, strings::fill("manager-uninstall-services-failed", &[&e])))?;
            for (_, e) in windows::scm::ScmService::new(exe).sync(&[]) {
                let detail = service::error_reply(&service::ManagerError::Service(e)).1;
                eprintln!(
                    "{}",
                    strings::fill("manager-uninstall-services-failed", &[&detail])
                );
            }
            return Ok(0);
        }
        #[cfg(not(windows))]
        ManagerCommand::UninstallServices => {
            return Err((1, strings::t("manager-uninstall-services-unavailable")))
        }
        #[cfg(target_os = "linux")]
        ManagerCommand::Serve => {
            let _served =
                dbus::serve().map_err(|e| (1, strings::fill("manager-serve-failed", &[&e])))?;
            // The connection serves on its own thread; this one only keeps it.
            loop {
                std::thread::park();
            }
        }
        #[cfg(windows)]
        ManagerCommand::Serve => {
            windows::entry::serve(|id, e| {
                let detail = service::error_reply(e).1;
                eprintln!(
                    "{}",
                    strings::fill("manager-service-sync-failed", &[&id, &detail])
                );
            })
            .map_err(|e| (1, strings::fill("manager-serve-failed", &[&e])))?;
            return Ok(0);
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        ManagerCommand::Serve => return Err((1, strings::t("manager-serve-unavailable"))),
    };
    // A launch error is its message: on Windows the service writes it into
    // the instance's log.
    let launch = crate::paths::llama_server_exe()
        .ok_or_else(|| {
            let name = crate::paths::binary_name("llama-server");
            strings::fill("binary-not-found", &[&name])
        })
        .and_then(|exe| {
            let cmd = crate::launch::instance_command(&id, &exe).map_err(|e| match e {
                LaunchError::InvalidId => strings::fill("manager-invalid-id", &[&id]),
                LaunchError::NoConfig(p) => {
                    strings::fill("manager-no-config", &[&p.display(), &id])
                }
                LaunchError::NoPresets => {
                    let presets = crate::paths::engine_config_dir().join("presets.ini");
                    strings::fill("manager-no-presets", &[&presets.display()])
                }
                LaunchError::StateDir(p, kind) => {
                    strings::fill("manager-state-dir", &[&p.display(), &kind])
                }
            })?;
            Ok((exe, cmd))
        });
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let (exe, mut cmd) = launch.map_err(|text| (1, text))?;
        // Returns only when the exec failed.
        let e = cmd.exec();
        Err((
            1,
            strings::fill("manager-launch-failed", &[&exe.display(), &e]),
        ))
    }
    #[cfg(windows)]
    {
        let (shown, cmd) = match launch {
            Ok((exe, cmd)) => (exe.display().to_string(), Ok(cmd)),
            Err(text) => (String::new(), Err(text)),
        };
        let launch_failed =
            move |e: &std::io::Error| strings::fill("manager-launch-failed", &[&shown, e]);
        windows::entry::run_instance(&id, cmd, Box::new(launch_failed)).map_err(|text| (1, text))
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
    fn usage_text_names_both_commands() {
        let cli = crate::strings::parse(include_str!("../../core/strings/cli.toml"));
        for key in ["manager-usage", "manager-unknown-command"] {
            let text = &cli[key];
            assert!(
                text.contains("pai-manager serve")
                    && text.contains("pai-manager run-instance <id>"),
                "{key}: {text:?}"
            );
            assert!(
                !text.contains(concat!("private-ai", "-manager")),
                "{key}: {text:?}"
            );
        }
    }

    /// The executable is `pai-manager`, the name the unit `llama.cpp@<id>`
    /// runs (`pai-manager run-instance <id>`) and the usage text gives.
    #[test]
    fn the_executable_is_named_pai_manager() {
        let manifest = include_str!("../Cargo.toml");
        let bin = manifest.split("[[bin]]").nth(1).expect("a [[bin]] table");
        let name = bin
            .lines()
            .find_map(|l| l.trim().strip_prefix("name = "))
            .expect("a bin name");
        assert_eq!(name, "\"pai-manager\"");
        assert!(
            !manifest.contains(concat!("private-ai", "-manager.rs")),
            "{manifest}"
        );
    }

    #[test]
    fn parse_args_reads_uninstall_services() {
        assert_eq!(
            parse(&["uninstall-services"]),
            Ok(ManagerCommand::UninstallServices)
        );
    }

    /// The MSI runs it with no argument: it removes every instance's
    /// service, never one picked on the command line.
    #[test]
    fn parse_args_refuses_uninstall_services_with_arguments() {
        assert_eq!(
            parse(&["uninstall-services", "default"]),
            Err(ArgsError::ExtraArgument("default".into()))
        );
        for cmd in [
            "Uninstall-Services",
            "uninstall_services",
            "--uninstall-services",
        ] {
            assert_eq!(
                parse(&[cmd]),
                Err(ArgsError::UnknownCommand(cmd.into())),
                "subcommand {cmd:?}"
            );
        }
    }

    /// At its start the Windows manager sets up each instance's SCM service
    /// (`windows::entry::serve` syncs them) and starts none: a failure there
    /// says the instance's service could not be set up, not that a start at
    /// boot failed, under a key of its own; the old key is gone. The
    /// Windows arm only type-checks on Linux, so its key is read in the
    /// source, before the tests.
    #[test]
    fn a_failed_service_sync_says_the_service_could_not_be_set_up() {
        let cli = crate::strings::parse(include_str!("../../core/strings/cli.toml"));
        assert!(
            !cli.contains_key("manager-boot-start-failed"),
            "the old key is left in cli.toml"
        );
        let key = "manager-service-sync-failed";
        let text = cli
            .get(key)
            .unwrap_or_else(|| panic!("{key} missing from cli.toml"));
        assert!(text.contains("{0}") && text.contains("{1}"), "{text:?}");
        let lower = text.to_lowercase();
        assert!(
            lower.contains("service") && lower.contains("set up"),
            "{text:?}"
        );
        for word in ["boot", "start"] {
            assert!(!lower.contains(word), "{word} in {text:?}");
        }
        let source = include_str!("manager.rs");
        let code = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("the code");
        let serve = code
            .split("#[cfg(windows)]\n        ManagerCommand::Serve =>")
            .nth(1)
            .expect("the Windows Serve arm");
        let serve = serve.split("ManagerCommand::").next().unwrap();
        assert!(
            serve.contains(&format!("strings::fill(\"{key}\"")),
            "the Windows Serve arm does not report with {key}:\n{serve}"
        );
        assert!(!code.contains("manager-boot-start-failed"), "{code}");
    }

    #[test]
    fn usage_text_names_uninstall_services() {
        let cli = crate::strings::parse(include_str!("../../core/strings/cli.toml"));
        for key in ["manager-usage", "manager-unknown-command"] {
            let text = &cli[key];
            assert!(
                text.contains("pai-manager uninstall-services"),
                "{key}: {text:?}"
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
