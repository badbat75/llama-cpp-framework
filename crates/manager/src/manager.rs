//! The command line of `pai-manager`, the runtime service binary that
//! runs llama-server instances as services. `run-instance <id>` is what the
//! unit `llama.cpp@<id>` runs: it launches instance `id`
//! (`launch::instance_command`). `save-slots <id>` is that unit's `ExecStop=`
//! on Linux: it saves the conversation snapshots of the instance being
//! stopped. `serve` is the manager's own service: the
//! D-Bus interface on the system bus (`manager::dbus`, Linux only).

#[cfg(target_os = "linux")]
mod accounts;
// The sinks for the log of who acted, the service installs on `with_log`.
pub mod action_log;
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
    /// `save-slots <id>`: the instance unit's `ExecStop=` on Linux, saving
    /// the conversation snapshots of the instance being stopped.
    SaveSlots(String),
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
    /// `run-instance` or `save-slots` without its id.
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
    if command == "save-slots" {
        let (id, extra) = rest.split_first().ok_or(ArgsError::MissingId)?;
        if let Some(arg) = extra.first() {
            return Err(ArgsError::ExtraArgument(arg.clone()));
        }
        return Ok(ManagerCommand::SaveSlots(id.clone()));
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

/// The line the manager prints for a failure of its services' sync: the
/// SCM itself, or the service of the instance it names.
pub fn sync_failure_message(failure: &windows::SyncFailure) -> String {
    use crate::strings;
    let detail = |e: &service::ServiceError| {
        service::error_reply(&service::ManagerError::Service(e.clone())).1
    };
    match failure {
        windows::SyncFailure::Scm(e) => strings::fill("manager-scm-sync-failed", &[&detail(e)]),
        windows::SyncFailure::Instance(id, e) => {
            strings::fill("manager-service-sync-failed", &[id, &detail(e)])
        }
    }
}

/// The argv bytes of a process, as Linux writes them: `/proc/<pid>/cmdline`,
/// NUL after every argument.
#[cfg(target_os = "linux")]
fn proc_cmdline(pid: u32) -> std::io::Result<Vec<u8>> {
    let path = std::path::Path::new("/proc")
        .join(pid.to_string())
        .join("cmdline");
    std::fs::read(path)
}

/// The port and state folder of the instance being stopped, read out of the
/// main process's own `/proc/<pid>/cmdline` (Linux, the unit's `ExecStop=`).
///
/// `$MAINPID` is systemd's: unset when it knows the main process is already
/// gone, so a crash leaves nothing to save. A pid that is not a number, or one
/// whose `/proc` entry cannot be read (it exited in between), is no process to
/// read either: nothing is read and there is no target. The state folder of
/// `server.ini` is never a fallback, it is not where a running engine writes.
/// The target is the launch itself (`slot_target`): the port and folder the
/// instance was started with, and a launch without `--slot-save-path` had
/// snapshots off, whatever `server.ini` says now.
#[cfg(target_os = "linux")]
fn save_slots_target(
    mainpid: Option<&str>,
    read_cmdline: impl Fn(u32) -> std::io::Result<Vec<u8>>,
) -> Option<(u16, std::path::PathBuf)> {
    let pid: u32 = mainpid?.parse().ok()?;
    let bytes = read_cmdline(pid).ok()?;
    // `/proc/<pid>/cmdline` is the argv, NUL after every argument: split on
    // the NULs and drop the empty fields (the separators and the trailer).
    let args: Vec<String> = bytes
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect();
    crate::slot_save::slot_target(&args)
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
    #[cfg(target_os = "linux")]
    use crate::slot_save;
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
            for failure in windows::scm::ScmService::new(exe).sync(&[]) {
                let (windows::SyncFailure::Scm(e) | windows::SyncFailure::Instance(_, e)) = failure;
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
        // The unit's `ExecStop=`: the instance's own stop writes the
        // snapshots, within the core's one budget, against the port and the
        // folder of the launch. The journal gets one line per model saved and
        // one per error. Always success: a stop that fails is a stop that
        // does not happen, and the dump of a save that did not complete is
        // the previous one, untouched.
        #[cfg(target_os = "linux")]
        ManagerCommand::SaveSlots(_id) => {
            let target = save_slots_target(std::env::var("MAINPID").ok().as_deref(), proc_cmdline);
            let Some((port, dir)) = target else {
                return Ok(0);
            };
            let (saved, errors) = slot_save::save_all(port, &dir, slot_save::SAVE_BUDGET);
            for t in &saved {
                eprintln!("{}", strings::fill("snapshot-saved-row", &[&t.describe()]));
            }
            for e in &errors {
                eprintln!("{}", strings::fill("snapshot-failed-row", &[&e]));
            }
            return Ok(0);
        }
        // Windows takes the snapshots inside `run-instance`, at the
        // service's own stop: the command exists for the Linux unit.
        #[cfg(not(target_os = "linux"))]
        ManagerCommand::SaveSlots(_) => {
            return Err((1, strings::t("manager-save-slots-unavailable")))
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
            windows::entry::serve(|failure| eprintln!("{}", sync_failure_message(failure)))
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
                LaunchError::NoModelProfiles => {
                    // The instance reads its render, never the pool: the
                    // message names the file that has no model profile section.
                    let model_profiles = crate::paths::instance_models(&id)
                        .unwrap_or_else(|| crate::paths::engine_config_dir().join("models.ini"));
                    strings::fill("manager-no-model-profiles", &[&model_profiles.display()])
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

    /// The two refusals the manager answers for a model profile, byte for byte
    /// in both strings files, next step included: the manager sends the key and
    /// the values, and the client's text is the file's whole sentence (the CLI's
    /// file names the keys of `models.ini`, the window's the fields of its
    /// page). A test has no entry point to pick `cli.toml`, so the decoded
    /// refusal renders the window's file.
    #[test]
    fn the_two_resource_refusals_carry_their_next_step() {
        use crate::manager::service::ManagerError;
        const GUI_OVER_MEMORY: &str = "`{0}` does not fit the GPU memory of instance `{1}`: it needs {2} MiB, the node has {3} MiB. To make it fit, lower its Context size, or pick a smaller K-cache type or V-cache type.";
        const CLI_OVER_MEMORY: &str = "`{0}` does not fit the GPU memory of instance `{1}`: it needs {2} MiB, the node has {3} MiB. To make it fit, lower its ctx-size, or pick a smaller cache-type-k or cache-type-v.";
        const GUI_MODEL_MISSING: &str = "`{0}` names a model file that is not there: `{2}` is missing for instance `{1}`. Put the file at that path on the server machine, or pick another Model file for the profile.";
        const CLI_MODEL_MISSING: &str = "`{0}` names a model file that is not there: `{2}` is missing for instance `{1}`. Put the file at that path on the server machine, or set the profile's model to another file.";
        for (name, file, over_memory, model_missing) in [
            (
                "cli.toml",
                include_str!("../../core/strings/cli.toml"),
                CLI_OVER_MEMORY,
                CLI_MODEL_MISSING,
            ),
            (
                "en_US.toml",
                include_str!("../../core/strings/en_US.toml"),
                GUI_OVER_MEMORY,
                GUI_MODEL_MISSING,
            ),
        ] {
            let table = crate::strings::parse(file);
            assert_eq!(
                table
                    .get("model-profile-over-memory")
                    .expect("model-profile-over-memory in the strings file"),
                over_memory,
                "{name}: model-profile-over-memory"
            );
            assert_eq!(
                table
                    .get("model-profile-model-missing")
                    .expect("model-profile-model-missing in the strings file"),
                model_missing,
                "{name}: model-profile-model-missing"
            );
        }
        let over = ManagerError::Invalid(pai_core::checks::Refusal {
            key: "model-profile-over-memory",
            values: ["qwen", "default", "1446", "512"]
                .map(String::from)
                .to_vec(),
        });
        let back = pai_core::checks::Refusal::decode(&service::error_reply(&over).1)
            .expect("the refusal crosses the wire");
        assert_eq!(
            back.text(),
            "`qwen` does not fit the GPU memory of instance `default`: it needs 1446 MiB, the node has 512 MiB. To make it fit, lower its Context size, or pick a smaller K-cache type or V-cache type."
        );
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
            serve.contains("sync_failure_message("),
            "the Windows Serve arm does not report with sync_failure_message:\n{serve}"
        );
        assert!(!code.contains("manager-boot-start-failed"), "{code}");
    }

    /// An instance's service that could not be set up is reported under
    /// `manager-service-sync-failed`, naming the instance.
    #[test]
    fn a_failed_instance_service_names_the_instance() {
        let e = windows::scm_error("lan-2", 5);
        let detail = service::error_reply(&service::ManagerError::Service(e.clone())).1;
        let text = sync_failure_message(&windows::SyncFailure::Instance("lan-2".into(), e));
        assert_eq!(
            text,
            crate::strings::fill(
                "manager-service-sync-failed",
                &[&"lan-2".to_string(), &detail]
            )
        );
        assert!(text.contains("`lan-2`"), "{text}");
        assert!(text.contains("ERROR_ACCESS_DENIED (5)"), "{text}");
    }

    /// A failure of the SCM itself (opening it, listing its services) is
    /// tied to no instance: its own key, with no instance id, never
    /// "instance ``".
    #[test]
    fn an_scm_wide_failure_names_no_instance() {
        let cli = crate::strings::parse(include_str!("../../core/strings/cli.toml"));
        let key = "manager-scm-sync-failed";
        let template = cli
            .get(key)
            .unwrap_or_else(|| panic!("{key} missing from cli.toml"));
        assert!(
            template.contains("{0}") && !template.contains("{1}"),
            "{template:?}"
        );
        let lower = template.to_lowercase();
        assert!(lower.contains("service"), "{template:?}");
        assert!(!lower.contains("instance `"), "{template:?}");

        let e = service::ServiceError("service control manager: ERROR_ACCESS_DENIED (5)".into());
        let detail = service::error_reply(&service::ManagerError::Service(e.clone())).1;
        let text = sync_failure_message(&windows::SyncFailure::Scm(e));
        assert!(!text.starts_with(key), "the key, not its text: {text}");
        assert_eq!(text, crate::strings::fill(key, &[&detail]));
        assert!(text.contains("ERROR_ACCESS_DENIED (5)"), "{text}");
        assert!(!text.contains("``"), "an empty id: {text}");
        assert!(!text.contains("instance `"), "{text}");
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

    /// `save-slots <id>` is the instance unit's `ExecStop=`: the same instance
    /// the unit started, so the snapshots land for the instance being stopped.
    #[test]
    fn parse_args_reads_save_slots() {
        assert_eq!(
            parse(&["save-slots", "default"]),
            Ok(ManagerCommand::SaveSlots("default".into()))
        );
        assert_eq!(
            parse(&["save-slots", "lan-2"]),
            Ok(ManagerCommand::SaveSlots("lan-2".into()))
        );
    }

    /// The id is the unit's `%i`, so a `save-slots` on its own names no
    /// instance and saves nothing.
    #[test]
    fn parse_args_refuses_save_slots_without_id() {
        assert_eq!(parse(&["save-slots"]), Err(ArgsError::MissingId));
    }

    /// One instance per call, and only the subcommand and its id: a
    /// port or a folder from the command line would be a target the unit
    /// did not launch.
    #[test]
    fn parse_args_refuses_save_slots_with_extra_arguments() {
        assert_eq!(
            parse(&["save-slots", "default", "lan-2"]),
            Err(ArgsError::ExtraArgument("lan-2".into()))
        );
        assert_eq!(
            parse(&["save-slots", "default", "--port", "8080"]),
            Err(ArgsError::ExtraArgument("--port".into()))
        );
        for cmd in ["Save-Slots", "save_slots", "--save-slots"] {
            assert_eq!(
                parse(&[cmd, "default"]),
                Err(ArgsError::UnknownCommand(cmd.into())),
                "subcommand {cmd:?}"
            );
        }
    }

    /// The usage text names the command a failing `ExecStop=` prints into the
    /// journal, so an administrator reading it sees the stop's own job.
    #[test]
    fn usage_text_names_save_slots() {
        let cli = crate::strings::parse(include_str!("../../core/strings/cli.toml"));
        for key in ["manager-usage", "manager-unknown-command"] {
            let text = &cli[key];
            assert!(
                text.contains("pai-manager save-slots <id>"),
                "{key}: {text:?}"
            );
        }
    }

    /// The budget of the save is the core's one constant, the same one the
    /// clients' wrapper hands over, so the stop's save and a client's save
    /// share one deadline. The unit's `TimeoutStopSec=` is read in the unit's
    /// own test (`xtask` `system_files.rs`).
    #[test]
    fn the_save_slots_save_uses_the_cores_budget() {
        let source = include_str!("manager.rs");
        let code = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("the code");
        assert!(
            code.contains("SAVE_BUDGET"),
            "save-slots must save within slot_save::SAVE_BUDGET, the design's one budget"
        );
        assert!(
            !code.contains(concat!("from_secs(", "240)")),
            "the budget is the core's constant, not a copy of its seconds here"
        );
    }

    /// The `/proc/<pid>/cmdline` bytes of a launch, NUL after every argument
    /// including the last, as Linux writes them.
    #[cfg(target_os = "linux")]
    fn cmdline(args: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for arg in args {
            out.extend_from_slice(arg.as_bytes());
            out.push(0);
        }
        out
    }

    /// A `/proc/<pid>/cmdline` reader with a canned answer, logging the pids it
    /// was asked for. The log is shared so the test can check a read that must
    /// never happen.
    #[cfg(target_os = "linux")]
    struct Reader {
        asked: std::sync::Arc<std::sync::Mutex<Vec<u32>>>,
        bytes: Vec<u8>,
        fails: bool,
    }

    #[cfg(target_os = "linux")]
    impl Reader {
        fn answers(&self, pid: u32) -> std::io::Result<Vec<u8>> {
            self.asked.lock().expect("the read log").push(pid);
            if self.fails {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "the process is gone",
                ))
            } else {
                Ok(self.bytes.clone())
            }
        }
        fn asked(&self) -> Vec<u32> {
            self.asked.lock().expect("the read log").clone()
        }
    }

    #[cfg(target_os = "linux")]
    fn reader(bytes: Vec<u8>, fails: bool) -> Reader {
        Reader {
            asked: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            bytes,
            fails,
        }
    }

    /// `$MAINPID` is systemd's own: unset when it knows the main process is
    /// already gone (systemd.service(5) `ExecStop=`), so a crash leaves nothing
    /// to save. No pid means no read of `/proc`, and so no target: the state
    /// folder of `server.ini` is never a fallback, it is not where a running
    /// engine writes.
    #[cfg(target_os = "linux")]
    #[test]
    fn no_main_process_id_gives_no_target_and_reads_nothing() {
        let r = reader(Vec::new(), false);
        for mainpid in [None, Some(""), Some("abc"), Some("-3"), Some("42x")] {
            assert_eq!(
                save_slots_target(mainpid, |pid| r.answers(pid)),
                None,
                "MAINPID {mainpid:?} is no process to read"
            );
        }
        assert_eq!(
            r.asked(),
            Vec::<u32>::new(),
            "a MAINPID that is not a number must not be read"
        );
    }

    /// A pid systemd names that is no process (it exited in between, its
    /// `/proc` entry gone): no target, nothing to save, the stop goes on.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_main_process_that_cannot_be_read_gives_no_target() {
        let r = reader(Vec::new(), true);
        assert_eq!(save_slots_target(Some("4242"), |pid| r.answers(pid)), None);
        assert_eq!(r.asked(), vec![4242], "the read is the pid systemd named");
    }

    /// The launched argv is the target: a `--slot-save-path` is the folder the
    /// running engine writes to, `--port` the port it answers on. A launch
    /// without `--slot-save-path` had snapshots off, whatever `server.ini`
    /// says now.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_cmdline_of_the_main_process_gives_the_launch_target() {
        let r = reader(
            cmdline(&[
                "--model",
                "/models/qwen.gguf",
                "--slot-save-path",
                "/state/dir",
                "--port",
                "8081",
            ]),
            false,
        );
        assert_eq!(
            save_slots_target(Some("4242"), |pid| r.answers(pid)),
            Some((8081, std::path::PathBuf::from("/state/dir"))),
            "the port and folder the instance was launched with"
        );
        assert_eq!(r.asked(), vec![4242], "the read is the MAINPID's");
    }

    /// The engine's own default port when the launch named none: the same
    /// reading `slot_target` gives an argument list, never a port read out of
    /// the file.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_cmdline_without_a_port_gives_the_default_port() {
        let r = reader(cmdline(&["--slot-save-path", "/state/dir"]), false);
        assert_eq!(
            save_slots_target(Some("4242"), |pid| r.answers(pid)),
            Some((
                crate::server_cfg::DEFAULT_PORT as u16,
                std::path::PathBuf::from("/state/dir")
            ))
        );
    }

    /// Snapshots off at launch, or an empty argv (a read that answered
    /// nothing): no target.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_cmdline_with_no_slot_save_path_gives_no_target() {
        let r = reader(
            cmdline(&["--model", "/models/qwen.gguf", "--port", "8081"]),
            false,
        );
        assert_eq!(
            save_slots_target(Some("4242"), |pid| r.answers(pid)),
            None,
            "the launch had snapshots off"
        );
        assert_eq!(r.asked(), vec![4242]);

        let empty = reader(Vec::new(), false);
        assert_eq!(
            save_slots_target(Some("4242"), |pid| empty.answers(pid)),
            None,
            "an empty cmdline names no folder"
        );
    }

    /// The stop's own save, with no `$MAINPID` in the environment: the run
    /// answers 0, asking for nothing. systemd runs `ExecStop=` after a start
    /// that succeeded, also when the process ended on its own, and a stop that
    /// fails is a stop that does not happen: a save with no target is success.
    ///
    /// A plain unit test: the manager crate's e2e (`src/tests/mod.rs`) owns the
    /// process-wide data root, so this path must touch no `paths::`, and
    /// `MAINPID` is set only by systemd for a unit's stop commands, never in a
    /// test process.
    #[cfg(target_os = "linux")]
    #[test]
    fn save_slots_with_no_mainpid_exits_zero_asking_nothing() {
        assert!(
            std::env::var_os("MAINPID").is_none(),
            "MAINPID is set in this environment: the unset case cannot be pinned here"
        );
        assert_eq!(
            run(&["save-slots".into(), "default".into()]),
            Ok(0),
            "no target is not a failure, the stop goes on"
        );
    }
}
