//! The command line of `pai-manager`, the runtime service binary that
//! runs llama-server instances as services. `run-instance <id>` is what the
//! unit `llama.cpp@<id>` runs: it launches instance `id`
//! (`launch::instance_command`). `save-slots <id>` is that unit's `ExecStop=`
//! on Linux: it saves the conversation snapshots of the instance being
//! stopped. `wait-ready <id>` is that unit's `ExecStartPost=` on Linux: the
//! start job ends when the instance's router answers `GET /health` (slice RS1).
//! `serve` is the manager's own service: the
//! D-Bus interface on the system bus (`manager::dbus`, Linux only).

#[cfg(target_os = "linux")]
mod accounts;
// The sinks for the log of who acted, the service installs on `with_log`.
pub mod action_log;
#[cfg(target_os = "linux")]
pub mod dbus;
pub mod pipe;
pub mod service;
// The `vulkan-probe` child: the node's Vulkan drivers, printed by the child the
// manager spawns to read them.
pub mod vulkan_child;
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
    /// `wait-ready <id>`: the instance unit's `ExecStartPost=` on Linux. The
    /// start job ends when the instance's router answers `GET /health` with
    /// 200 (slice RS1, ARCHITECTURE.md "An instance is started once its router
    /// answers").
    WaitReady(String),
    /// `serve`: the D-Bus / named-pipe service entry.
    Serve,
    /// `uninstall-services`: stop and delete every instance's service (the
    /// MSI, when it removes or repairs the manager's files).
    UninstallServices,
    /// `vulkan-probe`: the child the manager spawns to read the Vulkan driver of
    /// each GPU, printing one JSON document on stdout (ARCHITECTURE.md "About
    /// and node info", AN2). A hidden subcommand: the manager runs it, nobody
    /// types it, so the usage text does not name it.
    VulkanProbe,
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
    if command == "vulkan-probe" {
        if let Some(arg) = rest.first() {
            return Err(ArgsError::ExtraArgument(arg.clone()));
        }
        return Ok(ManagerCommand::VulkanProbe);
    }
    if command == "save-slots" {
        let (id, extra) = rest.split_first().ok_or(ArgsError::MissingId)?;
        if let Some(arg) = extra.first() {
            return Err(ArgsError::ExtraArgument(arg.clone()));
        }
        return Ok(ManagerCommand::SaveSlots(id.clone()));
    }
    if command == "wait-ready" {
        let (id, extra) = rest.split_first().ok_or(ArgsError::MissingId)?;
        if let Some(arg) = extra.first() {
            return Err(ArgsError::ExtraArgument(arg.clone()));
        }
        return Ok(ManagerCommand::WaitReady(id.clone()));
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

/// The argv of the main process systemd names, read out of its
/// `/proc/<pid>/cmdline` (Linux): no pid, one that is no number, a process
/// whose entry cannot be read (it exited in between), is no argv to read.
/// `$MAINPID` is systemd's own: unset when it knows the main process is already
/// gone, so a crash leaves nothing to read.
#[cfg(target_os = "linux")]
fn mainpid_args<R>(mainpid: Option<&str>, read_cmdline: R) -> Option<Vec<String>>
where
    R: Fn(u32) -> std::io::Result<Vec<u8>>,
{
    let pid: u32 = mainpid?.parse().ok()?;
    let bytes = read_cmdline(pid).ok()?;
    // `/proc/<pid>/cmdline` is the argv, NUL after every argument: split on
    // the NULs and drop the empty fields (the separators and the trailer).
    Some(
        bytes
            .split(|byte| *byte == 0)
            .filter(|field| !field.is_empty())
            .map(|field| String::from_utf8_lossy(field).into_owned())
            .collect(),
    )
}

/// The port and state folder of the instance being stopped, read out of the
/// main process's own `/proc/<pid>/cmdline` (Linux, the unit's `ExecStop=`).
///
/// The target is the launch itself (`slot_target`): the port and folder the
/// instance was started with, and a launch without `--slot-save-path` had
/// snapshots off, whatever `server.ini` says now. The state folder of
/// `server.ini` is never a fallback, it is not where a running engine writes.
#[cfg(target_os = "linux")]
fn save_slots_target(
    mainpid: Option<&str>,
    read_cmdline: impl Fn(u32) -> std::io::Result<Vec<u8>>,
) -> Option<(u16, std::path::PathBuf)> {
    let args = mainpid_args(mainpid, read_cmdline)?;
    crate::slot_save::slot_target(&args)
}

/// What one `GET /health` of the instance's router answered (slice RS1).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Health {
    /// 200: the router answers, the instance is ready.
    Ready,
    /// 503: the router is up and still loading.
    Loading,
    /// No answer at all: the port refuses the connection, the request is cut
    /// short, the reply is unreadable.
    Unreachable,
}

/// The poll's interval, the design's 2 s between two reads.
pub const WAIT_READY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// The poll's deadline, the design's 10 min: past it the start fails, with the
/// unit's `TimeoutStartSec=` (slice RS2) as the backstop behind it.
pub const WAIT_READY_DEADLINE: std::time::Duration = std::time::Duration::from_secs(600);

/// One `GET /health` on `<host>:<port>`, mapped to `Health`.
pub fn health_of(host: &str, port: u16, timeout: std::time::Duration) -> Health {
    match crate::http::request_host(host, port, "GET", "/health", None, timeout) {
        Ok(res) if res.status == 200 => Health::Ready,
        // The router answers and is not ready: 503 is its "still loading", any
        // other status is a router up and not serving yet. Both are retried,
        // only a 200 is the ready.
        Ok(_) => Health::Loading,
        Err(_) => Health::Unreachable,
    }
}

/// Why `wait-ready` did not become ready. The caller turns it into the journal
/// line of the failed start, naming the instance.
#[derive(Debug, PartialEq, Eq)]
pub enum ReadyFailure {
    /// There is nothing to poll: no `$MAINPID`, one that is no number, a
    /// process whose cmdline cannot be read, a cmdline naming a port that is no
    /// listener.
    NoPort,
    /// The router never answered 200 within the deadline.
    Deadline { attempts: u32 },
}

/// The value of a launch flag in its space form, the LAST occurrence when the
/// flag appears twice (the engine keeps the last), as `slot_save` reads it: a
/// flag with nothing after it has no value.
#[cfg(target_os = "linux")]
fn launch_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .rev()
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
}

/// What the main process's argv says about where the readiness check asks. The
/// unit is `Type=exec`, so the argv of the first rounds is the manager's own
/// (`pai-manager run-instance <id>`): the engine's argv, named by its argv[0],
/// is the only one that says where a router answers.
#[cfg(target_os = "linux")]
enum LaunchTarget {
    /// The engine's argv: the address and port to ask.
    Engine(String, u16),
    /// argv[0] is no `llama-server`: the main process has not become the engine,
    /// so this round has nothing to ask and is counted not ready, retried like
    /// a 503. The engine's default port is no port to knock on here: any
    /// listener answering 200 there (another instance, a foreign server) is not
    /// this instance being ready.
    NotEngine,
    /// The engine's argv names a `--port` that is no listener (`0`, a value
    /// that is no number): the launch cannot answer anywhere, and reading its
    /// argv again changes nothing.
    NoPort,
}

/// Whether argv[0] is the engine: the instance's main process becomes
/// llama-server, so its basename names it (`paths::binary_name` is the
/// project's name of that program).
#[cfg(target_os = "linux")]
fn argv0_is_engine(args: &[String]) -> bool {
    let engine = crate::paths::binary_name("llama-server");
    args.first().is_some_and(|argv0| {
        std::path::Path::new(argv0)
            .file_name()
            .is_some_and(|name| name == engine.as_str())
    })
}

/// Where the readiness check asks, out of the launch's argv.
///
/// Only the engine's argv is asked anything at all (`argv0_is_engine`): the
/// address and the port come from what the engine was launched with.
///
/// The port is the LAST `--port` in the space form `launch.rs` writes, as
/// `slot_save::slot_target` reads the same argv, so the readiness and the save
/// ask the same port of the same launch; no `--port` is the engine's own
/// default, `server_cfg::DEFAULT_PORT`. A `--port=8123` the framework did not
/// build names no port, so it is the default too.
///
/// The address is the launch's `--host`, the address the engine bound.
/// `0.0.0.0` and `::` are bound everywhere and are no destination, so they are
/// polled on loopback, the address that reaches every binding; a launch with no
/// `--host` is polled there too.
///
/// `NoPort` is a `--port` that is no listener (`0`, a value that is no number):
/// the launch cannot answer anywhere, and reading its argv again changes
/// nothing.
#[cfg(target_os = "linux")]
fn launch_target(args: &[String]) -> LaunchTarget {
    if !argv0_is_engine(args) {
        return LaunchTarget::NotEngine;
    }
    let port = match launch_flag(args, "--port") {
        // A port the launch named must be a listener.
        Some(text) => match text.parse::<u16>().ok().filter(|port| *port != 0) {
            Some(port) => port,
            None => return LaunchTarget::NoPort,
        },
        None => crate::server_cfg::DEFAULT_PORT as u16,
    };
    let host = match launch_flag(args, "--host") {
        Some(host) if host != "0.0.0.0" && host != "::" => host.to_string(),
        _ => crate::http::LOOPBACK.to_string(),
    };
    LaunchTarget::Engine(host, port)
}

/// `wait-ready`: `GET /health` on the address and port of the launch every
/// `interval` until it answers 200, giving up at `deadline`. The launch's argv
/// is read out of `$MAINPID`'s `/proc` entry again on every round: the unit is
/// `Type=exec`, so the first read is `pai-manager run-instance <id>`, which is
/// no engine and names no port, and a later one is the engine's. A round whose
/// argv is not the engine's asks nothing and is counted not ready, retried to
/// the deadline. The `/proc` read, the clock and the HTTP read are arguments,
/// so a test drives the 10 minutes and the 2 seconds with no real wait.
#[cfg(target_os = "linux")]
pub fn wait_ready<C, S, H, R>(
    mainpid: Option<&str>,
    read_cmdline: R,
    mut elapsed: C,
    mut sleep: S,
    mut health: H,
    interval: std::time::Duration,
    deadline: std::time::Duration,
) -> Result<(), ReadyFailure>
where
    C: FnMut() -> std::time::Duration,
    S: FnMut(std::time::Duration),
    H: FnMut(&str, u16) -> Health,
    R: Fn(u32) -> std::io::Result<Vec<u8>>,
{
    let mut attempts = 0u32;
    loop {
        // A main process systemd no longer names is a start that is over: the
        // poll has nothing left to ask, so it fails at once.
        let args = mainpid_args(mainpid, &read_cmdline).ok_or(ReadyFailure::NoPort)?;
        match launch_target(&args) {
            LaunchTarget::NoPort => return Err(ReadyFailure::NoPort),
            // The main process is not the engine yet: the round asks nothing and
            // is the start still loading, not a start with nothing to check.
            LaunchTarget::NotEngine => {}
            LaunchTarget::Engine(host, port) => {
                attempts += 1;
                if health(&host, port) == Health::Ready {
                    return Ok(());
                }
            }
        }
        // A round that is not the ready is retried until the deadline, a cmdline
        // that is not the engine's included: the last one is the read that finds
        // the clock spent, so no sleep runs past it.
        if elapsed() >= deadline {
            return Err(ReadyFailure::Deadline { attempts });
        }
        sleep(interval);
    }
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
        // The hidden child `service::VulkanProbe` spawns: it prints the node's
        // Vulkan drivers on stdout and answers nothing else, so the command ends
        // here.
        ManagerCommand::VulkanProbe => {
            vulkan_child::run()?;
            return Ok(0);
        }
        // The unit's `ExecStartPost=`: the start job ends when this command
        // exits, so it waits for the instance's router to answer `GET /health`
        // with 200, and a failed readiness is the start's own failure, with its
        // line in the journal.
        ManagerCommand::WaitReady(id) => {
            // The readiness of an instance that cannot exist is not worth
            // 10 minutes of the start job.
            if !crate::paths::valid_instance_id(&id) {
                return Err((1, strings::fill("manager-invalid-id", &[&id])));
            }
            #[cfg(target_os = "linux")]
            {
                // The clock of the whole poll, and one read bound by the
                // interval: a read never costs more than the gap between two.
                let started = std::time::Instant::now();
                let outcome = wait_ready(
                    std::env::var("MAINPID").ok().as_deref(),
                    proc_cmdline,
                    move || started.elapsed(),
                    std::thread::sleep,
                    |host, port| health_of(host, port, WAIT_READY_INTERVAL),
                    WAIT_READY_INTERVAL,
                    WAIT_READY_DEADLINE,
                );
                return match outcome {
                    Ok(()) => Ok(0),
                    Err(ReadyFailure::NoPort) => {
                        Err((1, strings::fill("manager-wait-ready-no-port", &[&id])))
                    }
                    Err(ReadyFailure::Deadline { attempts }) => Err((
                        1,
                        strings::fill("manager-wait-ready-timeout", &[&id, &attempts]),
                    )),
                };
            }
            // Windows has no `After=` between an instance and another unit: the
            // command is the Linux unit's start-post.
            #[cfg(not(target_os = "linux"))]
            return Err((1, strings::t("manager-wait-ready-unavailable")));
        }
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

    /// Slice AN2 of `ARCHITECTURE.md` ("About and node info"): the manager
    /// spawns its own executable as `pai-manager vulkan-probe` to read the
    /// node's Vulkan drivers, so the subcommand is a command of the parser.
    #[test]
    fn parse_args_reads_the_vulkan_probe_child() {
        assert_eq!(parse(&["vulkan-probe"]), Ok(ManagerCommand::VulkanProbe));
    }

    /// The child takes no id and no flag: it reads the whole node.
    #[test]
    fn parse_args_refuses_the_vulkan_probe_with_an_argument() {
        for arg in ["default", "--help", "8"] {
            assert_eq!(
                parse(&["vulkan-probe", arg]),
                Err(ArgsError::ExtraArgument(arg.into())),
                "`vulkan-probe {arg}` is no command"
            );
        }
    }

    /// The subcommand is hidden: the manager spawns it, nobody types it, so the
    /// usage the manager prints for a bad command line (and the refusal of an
    /// unknown command, which repeats it) must not offer it. The manager is a
    /// command line, so its usage lives in the CLI's file: `en_US.toml` carries
    /// no `manager-*` key at all (the 16 of them are in `cli.toml`).
    #[test]
    fn the_usage_never_offers_the_hidden_subcommand() {
        let table = crate::strings::parse(include_str!("../../core/strings/cli.toml"));
        for key in ["manager-usage", "manager-unknown-command"] {
            let text = table
                .get(key)
                .unwrap_or_else(|| panic!("{key} in cli.toml"));
            assert!(
                !text.contains("vulkan-probe"),
                "cli.toml: {key} offers the subcommand only the manager spawns:\n{text}"
            );
        }
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

    // ── Slice RS1: `pai-manager wait-ready <id>` ───────────────────────────
    // ARCHITECTURE.md "An instance is started once its router answers".

    // The two numbers the poll is driven with: only the Linux tests run it, the
    // command itself is the Linux unit's `ExecStartPost=`.
    #[cfg(target_os = "linux")]
    const RS1_SEC2: std::time::Duration = std::time::Duration::from_secs(2);
    #[cfg(target_os = "linux")]
    const RS1_MIN10: std::time::Duration = std::time::Duration::from_secs(600);

    /// `wait-ready <id>` is the instance unit's `ExecStartPost=`: the same
    /// instance the unit started, so the readiness it waits for is the one the
    /// unit owns.
    #[test]
    fn rs1_parse_args_reads_wait_ready() {
        assert_eq!(
            parse(&["wait-ready", "default"]),
            Ok(ManagerCommand::WaitReady("default".into()))
        );
        assert_eq!(
            parse(&["wait-ready", "lan-2"]),
            Ok(ManagerCommand::WaitReady("lan-2".into()))
        );
    }

    /// The id is the unit's `%i`: on its own the command names no instance, and
    /// a port or a second id on the command line would be a target the unit did
    /// not start.
    #[test]
    fn rs1_parse_args_refuses_wait_ready_without_its_id_and_with_more() {
        assert_eq!(parse(&["wait-ready"]), Err(ArgsError::MissingId));
        assert_eq!(
            parse(&["wait-ready", "default", "lan-2"]),
            Err(ArgsError::ExtraArgument("lan-2".into()))
        );
        assert_eq!(
            parse(&["wait-ready", "default", "--port", "8080"]),
            Err(ArgsError::ExtraArgument("--port".into()))
        );
        for cmd in ["Wait-Ready", "wait_ready", "--wait-ready"] {
            assert_eq!(
                parse(&[cmd, "default"]),
                Err(ArgsError::UnknownCommand(cmd.into())),
                "subcommand {cmd:?}"
            );
        }
    }

    /// The usage names the command: the design's package rule is that an old
    /// manager answers `wait-ready` with its usage text, and an administrator
    /// reads that text in the journal of a failed start.
    #[test]
    fn rs1_usage_text_names_wait_ready() {
        let cli = crate::strings::parse(include_str!("../../core/strings/cli.toml"));
        for key in ["manager-usage", "manager-unknown-command"] {
            let text = &cli[key];
            assert!(
                text.contains("pai-manager wait-ready <id>"),
                "{key}: {text:?}"
            );
        }
    }

    /// The design's two numbers, as the constants the command polls with: 2 s
    /// between two reads, 10 min of patience, the unit's `TimeoutStartSec=`
    /// (slice RS2) behind it.
    #[test]
    fn rs1_the_poll_reads_every_2_seconds_for_10_minutes() {
        assert_eq!(WAIT_READY_INTERVAL, std::time::Duration::from_secs(2));
        assert_eq!(WAIT_READY_DEADLINE, std::time::Duration::from_secs(600));
    }

    // The command polled with the two constants, not with copies of their
    // seconds: the poll's own pace is pinned by the seam-driven runs below
    // (`rs1_a_503_is_retried_until_the_200`, `rs1_the_deadline_is_a_failed_start`),
    // and the journal line of a failed start by
    // `rs1_run_with_no_mainpid_exits_non_zero_naming_the_instance`. The source
    // text of the arm is not pinned: the RS1 review retired that grep.

    /// A loopback listener that answers one request with `status` and closes:
    /// the router's `/health` in its ready (200) and loading (503) answer.
    fn rs1_health_listener(status: u16, body: &str) -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("the port").port();
        let body = body.to_string();
        std::thread::spawn(move || {
            let Ok((mut stream, _addr)) = listener.accept() else {
                return;
            };
            // Drain the request head, so the client is not left writing into a
            // socket nobody reads.
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                if std::io::Read::read(&mut stream, &mut byte).unwrap_or(0) == 0 {
                    return;
                }
                head.push(byte[0]);
                if head.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let reason = if status == 200 {
                "OK"
            } else {
                "Service Unavailable"
            };
            let text = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let _ = std::io::Write::write_all(&mut stream, text.as_bytes());
            let _ = std::io::Write::flush(&mut stream);
        });
        port
    }

    /// The router's two answers, read off a real loopback listener: 200 is
    /// ready, 503 is up and loading (the design's "Ready = the router answers
    /// `GET /health` with 200").
    #[test]
    fn rs1_health_reads_the_routers_200_and_503() {
        for (status, body, want) in [
            (200, r#"{"status":"ok"}"#, Health::Ready),
            (503, r#"{"error":"loading"}"#, Health::Loading),
        ] {
            let port = rs1_health_listener(status, body);
            assert_eq!(
                health_of("127.0.0.1", port, std::time::Duration::from_secs(5)),
                want,
                "a {status} answer on the health port"
            );
        }
    }

    /// A port nobody listens on, the router not bound yet: no answer at all, so
    /// the read is `Unreachable` and the poll retries it.
    #[test]
    fn rs1_health_calls_a_refused_port_unreachable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("the port").port();
        drop(listener);
        assert_eq!(
            health_of("127.0.0.1", port, std::time::Duration::from_secs(5)),
            Health::Unreachable,
            "a refused connection is no answer"
        );
    }

    /// The scripted `/health` and the virtual clock of one `wait-ready` run:
    /// every sleep advances the clock, so the 2-second interval and the
    /// 10-minute deadline are driven with no real wait.
    #[cfg(target_os = "linux")]
    #[derive(Clone)]
    struct Rs1Poll {
        /// The pids whose `/proc` entry was read.
        pids: Vec<u32>,
        /// The `/proc` answers still to hand out; the last one is repeated once
        /// they run out (the launch's cmdline does not change after the exec).
        cmdlines: std::collections::VecDeque<Vec<u8>>,
        last_cmdline: Vec<u8>,
        /// The addresses `/health` was asked on, in the same order as `asked`.
        hosts: Vec<String>,
        /// The ports `/health` was asked on.
        asked: Vec<u16>,
        slept: Vec<std::time::Duration>,
        elapsed: std::time::Duration,
        answers: std::collections::VecDeque<Health>,
        last: Health,
        /// A poll that never gives up must fail the test, not hang it.
        runaway: std::time::Duration,
    }

    /// One `wait-ready` run over a scripted `/health`, a clock that advances on
    /// every sleep, and a `$MAINPID` whose cmdline is `bytes` (or unreadable,
    /// when `fails`). The snapshot is the state the run left behind.
    #[cfg(target_os = "linux")]
    fn rs1_poll(
        mainpid: Option<&str>,
        bytes: Vec<u8>,
        fails: bool,
        answers: &[Health],
        last: Health,
        interval: std::time::Duration,
        deadline: std::time::Duration,
    ) -> (Result<(), ReadyFailure>, Rs1Poll) {
        rs1_poll_seq(
            mainpid,
            std::slice::from_ref(&bytes),
            fails,
            answers,
            last,
            interval,
            deadline,
        )
    }

    /// The same run, with a SEQUENCE of `/proc` answers: the unit is `Type=exec`,
    /// so `ExecStartPost=` runs while the main process is still
    /// `pai-manager run-instance <id>`, and the cmdline of a later read is the
    /// engine's. The last entry is repeated once the sequence runs out.
    #[cfg(target_os = "linux")]
    fn rs1_poll_seq(
        mainpid: Option<&str>,
        cmdlines: &[Vec<u8>],
        fails: bool,
        answers: &[Health],
        last: Health,
        interval: std::time::Duration,
        deadline: std::time::Duration,
    ) -> (Result<(), ReadyFailure>, Rs1Poll) {
        let (last_cmdline, queue) = match cmdlines.split_last() {
            Some((last, rest)) => (last.clone(), rest.to_vec()),
            None => (Vec::new(), Vec::new()),
        };
        let st = std::rc::Rc::new(std::cell::RefCell::new(Rs1Poll {
            pids: Vec::new(),
            cmdlines: queue.into(),
            last_cmdline,
            hosts: Vec::new(),
            asked: Vec::new(),
            slept: Vec::new(),
            elapsed: std::time::Duration::ZERO,
            answers: answers.iter().copied().collect(),
            last,
            runaway: deadline * 4,
        }));
        let read = std::rc::Rc::clone(&st);
        let health = std::rc::Rc::clone(&st);
        let clock = std::rc::Rc::clone(&st);
        let sleep = std::rc::Rc::clone(&st);
        let outcome = wait_ready(
            mainpid,
            move |pid| {
                let mut g = read.borrow_mut();
                g.pids.push(pid);
                if fails {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "the process is gone",
                    ));
                }
                Ok(match g.cmdlines.pop_front() {
                    Some(bytes) => bytes,
                    None => g.last_cmdline.clone(),
                })
            },
            move || clock.borrow().elapsed,
            move |d| {
                let mut g = sleep.borrow_mut();
                g.slept.push(d);
                g.elapsed += d;
                assert!(
                    g.elapsed <= g.runaway,
                    "the poll ran to {:?} with a deadline of {deadline:?}: it never gives up",
                    g.elapsed
                );
            },
            move |host, port| {
                let last = health.borrow().last;
                let mut g = health.borrow_mut();
                g.hosts.push(host.to_string());
                g.asked.push(port);
                g.answers.pop_front().unwrap_or(last)
            },
            interval,
            deadline,
        );
        let snapshot = st.borrow().clone();
        (outcome, snapshot)
    }

    /// The port of a launched instance, as `$MAINPID`'s cmdline names it: the
    /// engine's own argv, argv[0] included (the poll tells the pre-exec cmdline
    /// of the unit from the engine by it).
    #[cfg(target_os = "linux")]
    fn rs1_launched() -> Vec<u8> {
        cmdline(&[
            "llama-server",
            "--model",
            "/models/qwen.gguf",
            "--port",
            "8123",
        ])
    }

    /// The router answers 200 the first time it is asked: the start is ready,
    /// the command exits 0, and it waits nothing at all.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_a_200_at_the_first_read_is_ready_at_once() {
        let (outcome, st) = rs1_poll(
            Some("4242"),
            rs1_launched(),
            false,
            &[Health::Ready],
            Health::Unreachable,
            RS1_SEC2,
            RS1_MIN10,
        );
        assert_eq!(outcome, Ok(()), "the router answered 200");
        assert_eq!(st.asked, vec![8123], "one read, on the port of the launch");
        assert_eq!(st.slept, Vec::new(), "ready at once: no sleep at all");
    }

    /// A 503 is the router up and still loading: the read is retried, and the
    /// 200 that follows is the ready. One interval between two reads.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_a_503_is_retried_until_the_200() {
        let (outcome, st) = rs1_poll(
            Some("4242"),
            rs1_launched(),
            false,
            &[Health::Loading, Health::Loading, Health::Ready],
            Health::Ready,
            RS1_SEC2,
            RS1_MIN10,
        );
        assert_eq!(outcome, Ok(()), "a 503 is loading, not a failure");
        assert_eq!(
            st.asked,
            vec![8123, 8123, 8123],
            "the router is asked again after a 503"
        );
        assert_eq!(st.slept, vec![RS1_SEC2, RS1_SEC2], "2 s between two reads");
    }

    /// A refused port is the router not bound yet: retried like a 503, and the
    /// 200 that follows is the ready.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_a_refused_port_is_retried_until_the_200() {
        let (outcome, st) = rs1_poll(
            Some("4242"),
            rs1_launched(),
            false,
            &[Health::Unreachable, Health::Ready],
            Health::Ready,
            RS1_SEC2,
            RS1_MIN10,
        );
        assert_eq!(
            outcome,
            Ok(()),
            "a refused port is the router not up yet, not a failure"
        );
        assert_eq!(
            st.asked,
            vec![8123, 8123],
            "the router is asked again after a refused port"
        );
        assert_eq!(st.slept, vec![RS1_SEC2], "2 s between two reads");
    }

    /// The router never answers 200: the command gives up at the deadline, so
    /// systemd fails the start. Every wait is the interval, the last one does
    /// not run past the deadline, and the failure counts the reads it made.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_the_deadline_is_a_failed_start() {
        let (outcome, st) = rs1_poll(
            Some("4242"),
            rs1_launched(),
            false,
            &[],
            Health::Loading,
            RS1_SEC2,
            RS1_MIN10,
        );
        let Err(failure) = outcome else {
            panic!("a router that never answers 200 is not ready: {outcome:?}");
        };
        let ReadyFailure::Deadline { attempts } = failure else {
            panic!("the router was read, so this is the deadline: {failure:?}");
        };
        assert!(
            st.slept.iter().all(|d| *d == RS1_SEC2),
            "every wait is the interval: {:?}",
            st.slept
        );
        let total: std::time::Duration = st.slept.iter().sum();
        assert!(
            total + RS1_SEC2 >= RS1_MIN10,
            "the poll gave up after {total:?} of a 10-minute deadline"
        );
        assert!(
            total <= RS1_MIN10 + RS1_SEC2,
            "the poll ran past its deadline: {total:?}"
        );
        assert!(
            st.asked.len() >= 2,
            "the router was read {} times",
            st.asked.len()
        );
        assert!(
            st.asked.iter().all(|p| *p == 8123),
            "every read is the port of the launch: {:?}",
            st.asked
        );
        assert_eq!(
            attempts as usize,
            st.asked.len(),
            "the failure counts the reads the poll made"
        );
    }

    /// (c) No `$MAINPID`, a `$MAINPID` that is not a number, a process that
    /// cannot be read, a `--port` that names no listener: there is no port to
    /// poll, so the command fails at once, naming the instance, and asks the
    /// router nothing. A launch that leaves `--port` out is NOT here: it polls
    /// the default port (`rs1_a_launch_with_no_port_polls_the_default_port`).
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_no_mainpid_or_a_gone_pid_fails_early() {
        for (mainpid, bytes, fails, reads, why) in [
            (None, Vec::new(), false, 0, "systemd knows no main process"),
            (Some("abc"), Vec::new(), false, 0, "MAINPID is not a number"),
            (Some("4242"), Vec::new(), true, 1, "the process is gone"),
            (
                Some("4242"),
                cmdline(&["llama-server", "--port", "0"]),
                false,
                1,
                "--port 0 is no listener",
            ),
        ] {
            let (outcome, st) = rs1_poll(
                mainpid,
                bytes,
                fails,
                &[],
                Health::Ready,
                RS1_SEC2,
                RS1_MIN10,
            );
            let Err(failure) = outcome else {
                panic!("{why}: no port is not readiness: {outcome:?}");
            };
            assert_eq!(
                failure,
                ReadyFailure::NoPort,
                "{why}: a start that cannot be checked fails at once"
            );
            assert_eq!(st.pids.len(), reads, "{why}: the /proc reads");
            assert!(st.asked.is_empty(), "{why}: the router is asked nothing");
            assert!(st.slept.is_empty(), "{why}: the command fails at once");
        }
    }

    /// A `--port` the launch did not write in its space form: the two readings
    /// of the design ("as `slot_save::slot_target` does", the space form only)
    /// and of the engine (the last `--port` flag) disagree, so the pin is the
    /// disjunction: no port at all and the router asked nothing, or every read
    /// on the one port the launch could have meant. Never a made-up port.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_a_port_the_launch_did_not_name_polls_nothing_else() {
        for (args, only, why) in [
            (
                cmdline(&["llama-server", "--port=8123"]),
                crate::server_cfg::DEFAULT_PORT as u16,
                "the equals form is not the form the launch writes",
            ),
            (
                cmdline(&["llama-server", "--port", "8123", "--port"]),
                8123,
                "the last --port has no value",
            ),
        ] {
            let (outcome, st) = rs1_poll(
                Some("4242"),
                args,
                false,
                &[],
                Health::Loading,
                RS1_SEC2,
                RS1_MIN10,
            );
            match outcome {
                Err(ReadyFailure::NoPort) => {
                    assert!(st.asked.is_empty(), "{why}: no port asks nothing");
                    assert!(st.slept.is_empty(), "{why}: no port fails at once");
                }
                other => {
                    assert!(!st.asked.is_empty(), "{why}: {other:?} with no read at all");
                    assert!(
                        st.asked.iter().all(|p| *p == only),
                        "{why}: every read is {only}, got {:?}",
                        st.asked
                    );
                }
            }
        }
    }

    /// (a) The unit is `Type=exec`: `ExecStartPost=` runs while the main process
    /// is still `pai-manager run-instance <id>`, whose cmdline names no port.
    /// The cmdline is read again on every round, so the poll moves to the
    /// engine's port when the exec lands and the 200 there is the ready: exit 0.
    /// The RS1 review (finding 1) moved the pre-exec half of this pin: that
    /// cmdline asks NO port at all, so every read here is the engine's, and how
    /// many rounds the exec takes is the coder's. The pre-exec read itself is
    /// pinned by `rs1_a_200_on_the_default_port_before_the_exec_is_not_the_ready`.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_a_cmdline_sequence_that_becomes_the_launch_is_ready() {
        let (outcome, st) = rs1_poll_seq(
            Some("4242"),
            &[
                cmdline(&["pai-manager", "run-instance", "default"]),
                cmdline(&[
                    "llama-server",
                    "--models-preset",
                    "/etc/llama.cpp/models.ini",
                    "--host",
                    "0.0.0.0",
                    "--port",
                    "8123",
                ]),
            ],
            false,
            &[Health::Unreachable, Health::Ready],
            Health::Ready,
            RS1_SEC2,
            RS1_MIN10,
        );
        assert_eq!(
            outcome,
            Ok(()),
            "the engine answers 200 on its port: a healthy instance is ready"
        );
        assert!(
            st.pids.len() >= 2 && st.pids.iter().all(|p| *p == 4242),
            "the cmdline is read again on every round: {:?}",
            st.pids
        );
        assert!(
            !st.asked.is_empty() && st.asked.iter().all(|p| *p == 8123),
            "the pre-exec cmdline names no port and is no engine, so it asks \
             nothing: every read is the engine's, got {:?}",
            st.asked
        );
        assert_eq!(
            st.asked.last(),
            Some(&8123),
            "the read that made the start ready is the engine's"
        );
        assert!(
            !st.hosts.is_empty() && st.hosts.iter().all(|h| h == "127.0.0.1"),
            "0.0.0.0 is bound everywhere, so the read goes to loopback: {:?}",
            st.hosts
        );
        assert!(
            !st.slept.is_empty() && st.slept.iter().all(|d| *d == RS1_SEC2),
            "2 s between two reads: {:?}",
            st.slept
        );
    }

    /// (RS1 review, finding 1) The pre-exec cmdline is not the engine, and the
    /// engine's default port is the ENGINE's default: a listener answering 200
    /// there (another instance, a foreign server) must not end the start. The
    /// read of `pai-manager run-instance <id>` is counted not ready and retried,
    /// and the engine's argv, named by its argv[0], is the first thing the poll
    /// asks anything on.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_a_200_on_the_default_port_before_the_exec_is_not_the_ready() {
        let (outcome, st) = rs1_poll_seq(
            Some("4242"),
            &[
                cmdline(&["/usr/bin/pai-manager", "run-instance", "default"]),
                cmdline(&[
                    "/usr/bin/llama-server",
                    "--models-preset",
                    "/etc/llama.cpp/models.ini",
                    "--host",
                    "0.0.0.0",
                    "--port",
                    "8123",
                ]),
            ],
            false,
            &[],
            // Every ask is answered 200: a listener sits on whatever port the
            // poll asks, and a 200 is what ends a start.
            Health::Ready,
            RS1_SEC2,
            RS1_MIN10,
        );
        assert_eq!(
            outcome,
            Ok(()),
            "the engine's 200 on its own port is the ready"
        );
        assert_eq!(
            st.asked,
            vec![8123],
            "the pre-exec cmdline asks nothing, the engine its own port: {:?} \
             (a 200 on the engine's default port, before the exec, is not this \
             instance being ready)",
            st.asked
        );
        assert_eq!(
            st.pids,
            vec![4242, 4242],
            "the pre-exec read is counted not ready and retried, not the ready"
        );
    }

    /// (b) The engine launched with no `--port` (`launch.rs` leaves it out when
    /// the profile's `Port` is unset): the poll is retried until the injected
    /// deadline and fails then, NOT at once. The port it asks is the default,
    /// the launch named none. The pre-exec cmdline is not here: the RS1 review
    /// (finding 1) took it out of the default-port rule, it is pinned by
    /// `rs1_a_main_process_that_never_becomes_the_engine_is_retried_asking_nothing`.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_a_cmdline_that_never_names_a_port_is_retried_to_the_deadline() {
        let (outcome, st) = rs1_poll_seq(
            Some("4242"),
            &[cmdline(&[
                "llama-server",
                "--models-preset",
                "/etc/llama.cpp/models.ini",
                "--host",
                "0.0.0.0",
            ])],
            false,
            &[],
            Health::Unreachable,
            RS1_SEC2,
            RS1_MIN10,
        );
        let Err(failure) = outcome else {
            panic!("a router that never answers 200 is not ready: {outcome:?}");
        };
        let ReadyFailure::Deadline { attempts } = failure else {
            panic!(
                "a launch that names no port is polled on the default port, \
                 so the failure is the deadline, not the port: {failure:?}"
            );
        };
        assert!(
            st.pids.len() >= 2,
            "the cmdline is read again after every round: {} reads",
            st.pids.len()
        );
        assert!(
            st.asked.len() >= 2,
            "the router is read {} times, not given up at once",
            st.asked.len()
        );
        assert!(
            st.asked
                .iter()
                .all(|p| *p == crate::server_cfg::DEFAULT_PORT as u16),
            "no --port means the engine's default: {:?}",
            st.asked
        );
        assert!(
            st.hosts.iter().all(|h| h == "127.0.0.1"),
            "no --host is loopback: {:?}",
            st.hosts
        );
        let total: std::time::Duration = st.slept.iter().sum();
        assert!(
            st.slept.iter().all(|d| *d == RS1_SEC2),
            "every wait is the interval: {:?}",
            st.slept
        );
        assert!(
            total + RS1_SEC2 >= RS1_MIN10,
            "the poll gave up after {total:?} of a 10-minute deadline"
        );
        assert!(
            total <= RS1_MIN10 + RS1_SEC2,
            "the poll ran past its deadline: {total:?}"
        );
        assert_eq!(
            attempts as usize,
            st.asked.len(),
            "the failure counts the reads the poll made"
        );
    }

    /// (RS1 review, finding 1) The other half of the same rule: a main process
    /// that never becomes the engine is a start still loading, not a start with
    /// nothing to check. The poll is retried to the injected deadline, asking
    /// nothing on the way: the engine's default port is no port to knock on
    /// before the exec, where any listener answers 200.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_a_main_process_that_never_becomes_the_engine_is_retried_asking_nothing() {
        let (outcome, st) = rs1_poll_seq(
            Some("4242"),
            &[cmdline(&[
                "/usr/bin/pai-manager",
                "run-instance",
                "default",
            ])],
            false,
            &[],
            // A 200 answers whatever the poll asks: only the engine's own argv
            // may read it as the ready.
            Health::Ready,
            RS1_SEC2,
            RS1_MIN10,
        );
        let Err(failure) = outcome else {
            panic!("a main process that is not the engine is not a ready instance: {outcome:?}");
        };
        let ReadyFailure::Deadline { .. } = failure else {
            panic!(
                "a pre-exec cmdline is counted not ready and retried, it names no \
                 port that cannot answer: {failure:?}"
            );
        };
        assert!(
            st.pids.len() >= 2,
            "the cmdline is read again after every round: {} reads",
            st.pids.len()
        );
        assert!(
            st.asked.is_empty(),
            "the pre-exec cmdline asks no port at all: {:?}",
            st.asked
        );
        let total: std::time::Duration = st.slept.iter().sum();
        assert!(
            st.slept.iter().all(|d| *d == RS1_SEC2),
            "every wait is the interval: {:?}",
            st.slept
        );
        assert!(
            total + RS1_SEC2 >= RS1_MIN10,
            "the poll gave up after {total:?} of a 10-minute deadline"
        );
        assert!(
            total <= RS1_MIN10 + RS1_SEC2,
            "the poll ran past its deadline: {total:?}"
        );
    }

    /// (d) A launch with no `--port` (`launch.rs` leaves it out when the
    /// profile's `Port` is unset, and the engine applies its own default): the
    /// poll asks `server_cfg::DEFAULT_PORT`, as `slot_save::slot_target` reads
    /// it, not "no port".
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_a_launch_with_no_port_polls_the_default_port() {
        // The fixture carries `--slot-save-path`: `slot_save::slot_target` answers
        // None for an argv without it (snapshots off), and the cross-check below
        // compares the two readers of the SAME launch.
        let args = cmdline(&[
            "llama-server",
            "--models-preset",
            "/etc/llama.cpp/models.ini",
            "--host",
            "0.0.0.0",
            "--slot-save-path",
            "/var/lib/private-ai/state/default",
        ]);
        let (outcome, st) = rs1_poll(
            Some("4242"),
            args.clone(),
            false,
            &[Health::Ready],
            Health::Unreachable,
            RS1_SEC2,
            RS1_MIN10,
        );
        assert_eq!(
            outcome,
            Ok(()),
            "a launch with no --port is polled on the engine's default"
        );
        assert_eq!(
            st.asked,
            vec![crate::server_cfg::DEFAULT_PORT as u16],
            "the default port, not no port"
        );
        assert_eq!(st.slept, Vec::new(), "ready at once: no sleep at all");
        // The same reading as the snapshot machinery, which asks the same port
        // of the same launch.
        let argv: Vec<String> = String::from_utf8_lossy(&args)
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        assert_eq!(
            pai_core::slot_save::slot_target(&argv).map(|(port, _)| port),
            Some(st.asked[0]),
            "the readiness and the save ask the same port of the same launch"
        );
    }

    /// (e) A launch bound to one LAN address does not answer on `127.0.0.1`: the
    /// poll asks the address of `--host`.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_a_launch_on_a_lan_address_polls_that_address() {
        let (outcome, st) = rs1_poll(
            Some("4242"),
            cmdline(&[
                "llama-server",
                "--models-preset",
                "/etc/llama.cpp/models.ini",
                "--host",
                "192.168.50.210",
                "--port",
                "8123",
            ]),
            false,
            &[Health::Ready],
            Health::Unreachable,
            RS1_SEC2,
            RS1_MIN10,
        );
        assert_eq!(outcome, Ok(()), "the router answers on its own address");
        assert_eq!(
            st.hosts,
            vec!["192.168.50.210".to_string()],
            "the read goes to the address the engine bound"
        );
        assert_eq!(st.asked, vec![8123], "on the port of the launch");
    }

    /// (e) An address bound everywhere is not a destination: `0.0.0.0` and `::`
    /// are polled on loopback, the address that reaches every binding.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_a_launch_bound_to_every_address_polls_loopback() {
        for host in ["0.0.0.0", "::"] {
            let (outcome, st) = rs1_poll(
                Some("4242"),
                cmdline(&[
                    "llama-server",
                    "--models-preset",
                    "/etc/llama.cpp/models.ini",
                    "--host",
                    host,
                ]),
                false,
                &[Health::Ready],
                Health::Unreachable,
                RS1_SEC2,
                RS1_MIN10,
            );
            assert_eq!(outcome, Ok(()), "--host {host} answers on loopback");
            assert_eq!(
                st.hosts,
                vec!["127.0.0.1".to_string()],
                "--host {host} is no destination"
            );
        }
    }

    /// The host's own routed address: the primary source of the default route,
    /// picked with no packet sent (a UDP "connect" only chooses the route).
    /// `None` on a machine that cannot show the LAN pin: no route, no address,
    /// loopback only.
    #[cfg(target_os = "linux")]
    fn rs1_lan_address() -> Option<std::net::IpAddr> {
        let probe = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        probe.connect("192.0.2.1:80").ok()?;
        let ip = probe.local_addr().ok()?.ip();
        (!ip.is_loopback()).then_some(ip)
    }

    /// (e) The address reaches a real listener: a router bound to the host's own
    /// LAN address answers 200 to a read on that address, and nothing on
    /// loopback. A machine with no routed address, CI among them, cannot show
    /// this: the test skips and says why, it does not fail.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_health_answers_on_the_lan_address_it_is_asked() {
        let Some(ip) = rs1_lan_address() else {
            eprintln!(
                "skipped: this host has no routed address to pin (no route, or loopback only)"
            );
            return;
        };
        // The listener sits on that address alone. A bind that fails (the
        // address went away, the kernel refuses it) is the same skip.
        let Ok(listener) = std::net::TcpListener::bind((ip, 0)) else {
            eprintln!("skipped: cannot bind a listener on {ip}");
            return;
        };
        let port = listener.local_addr().expect("the port").port();
        std::thread::spawn(move || {
            let Ok((mut stream, _addr)) = listener.accept() else {
                return;
            };
            // Drain the request head, as `rs1_health_listener` does: a socket
            // closed while the client is still writing is RST, and the answer in
            // the kernel's buffer is lost with it.
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                if std::io::Read::read(&mut stream, &mut byte).unwrap_or(0) == 0 {
                    return;
                }
                head.push(byte[0]);
                if head.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let _ = std::io::Write::write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok",
            );
            let _ = std::io::Write::flush(&mut stream);
        });
        let addr = ip.to_string();
        assert_eq!(
            health_of(&addr, port, std::time::Duration::from_secs(5)),
            Health::Ready,
            "a router bound to {addr} answers a read on {addr}"
        );
    }

    /// A bad id is refused at once: the readiness of an instance that cannot
    /// exist is not worth 10 minutes of the start job. The sentence is the
    /// crate's own for an id that is no file name.
    #[test]
    fn rs1_run_refuses_a_bad_id() {
        let long = "a".repeat(33);
        for id in ["", "Bad Id!", "-x", long.as_str()] {
            let failure =
                run(&["wait-ready".into(), id.into()]).expect_err("a bad id is no instance");
            assert_ne!(failure.0, 0, "id {id:?} is a failed start");
            assert_eq!(
                failure.1,
                crate::strings::fill("manager-invalid-id", &[&id.to_string()]),
                "id {id:?}"
            );
        }
    }

    /// The unit's `ExecStartPost=` with no `$MAINPID` in the environment: the
    /// command fails at once, naming the instance, asking for nothing. A plain
    /// unit test: the crate's e2e owns the process-wide data root, so this path
    /// touches no `paths::`, and `MAINPID` is set by systemd for a unit's
    /// commands, never in a test process.
    #[cfg(target_os = "linux")]
    #[test]
    fn rs1_run_with_no_mainpid_exits_non_zero_naming_the_instance() {
        assert!(
            std::env::var_os("MAINPID").is_none(),
            "MAINPID is set in this environment: the unset case cannot be pinned here"
        );
        let failure = run(&["wait-ready".into(), "default".into()])
            .expect_err("no main process means no port to poll");
        assert_ne!(
            failure.0, 0,
            "a start whose readiness cannot be checked fails"
        );
        assert!(
            failure.1.contains("default"),
            "the journal line names the instance: {}",
            failure.1
        );
        assert!(
            !failure.1.starts_with("manager-"),
            "a strings key, not its text: {}",
            failure.1
        );
    }
}
