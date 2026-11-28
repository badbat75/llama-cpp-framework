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
    // The record of the pair this launch applies, written before the engine
    // starts: the drift read compares what runs with the pair on disk (IP3c).
    // A failure is one line in the instance's log, never a reason to refuse
    // the launch.
    if launch.is_ok() {
        if let Err(e) = service::write_applied(&id) {
            eprintln!(
                "{}",
                strings::fill("manager-applied-failed", &[&id, &e.to_string()])
            );
        }
    }
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
#[path = "manager_tests.rs"]
mod tests;
