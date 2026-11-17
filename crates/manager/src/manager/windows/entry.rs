//! `serve` and `run-instance` as Windows services, under the SCM's
//! dispatcher; run from a console (the dispatcher refuses with
//! `ERROR_FAILED_SERVICE_CONTROLLER_CONNECT`), each runs in the foreground
//! instead.

use super::pipe_server::{self, WinManager};
use super::scm::ScmService;
use super::{
    controls_accepted, ends_instance, ends_manager, exit_report, instance_save_target,
    manager_controls_accepted, service_name, stop_checkpoint, SyncFailure, TokenAuthority,
    ERROR_SERVICE_SPECIFIC_ERROR, STOP_TICK, STOP_WAIT_HINT,
};
use crate::manager::action_log::{manager_log_path, FileActionLog};
use crate::manager::service::{ActionLog, Manager, SystemResolver};
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{
    self, ServiceControlHandlerResult, ServiceStatusHandle,
};
use windows_service::{define_windows_service, service_dispatcher};

/// The dispatcher's error when the process was not started by the SCM.
const NOT_A_SERVICE: i32 = 1063;

/// Where a failure to set up the services at the manager's start goes.
pub type Report = fn(&SyncFailure);

/// What `serve`'s service main reports failures to.
static SERVE_REPORT: Mutex<Option<Report>> = Mutex::new(None);
/// Turns a failure to spawn llama-server into its message.
pub type LaunchFailed = Box<dyn Fn(&std::io::Error) -> String + Send>;

/// What `run-instance`'s service main runs: the instance, llama-server's
/// command or why there is none, and the spawn failure's message.
type Launch = (String, Result<Command, String>, LaunchFailed);
static INSTANCE: Mutex<Option<Launch>> = Mutex::new(None);

define_windows_service!(ffi_serve_main, serve_main);
define_windows_service!(ffi_instance_main, instance_main);

/// `pai-manager serve`: the manager's service, `report` told of every
/// service it could not set up (the SCM itself, or an instance's). Returns when the service
/// stops; from a console it serves until the pipe fails.
pub fn serve(report: Report) -> std::io::Result<()> {
    *lock(&SERVE_REPORT) = Some(report);
    match service_dispatcher::start(super::MANAGER_SERVICE, ffi_serve_main) {
        Ok(()) => Ok(()),
        Err(e) if is_console(&e) => run_manager(report),
        Err(e) => Err(io_error(e)),
    }
}

/// `pai-manager run-instance <id>` with llama-server's command `cmd` (or
/// the message of why there is none): as the service `service_name(id)`,
/// its output, or that message, in the instance's log; from a console in
/// the foreground, its exit code returned, a failure as its message.
pub fn run_instance(
    id: &str,
    cmd: Result<Command, String>,
    launch_failed: LaunchFailed,
) -> Result<i32, String> {
    let Some(name) = service_name(id) else {
        return foreground(cmd, &launch_failed);
    };
    *lock(&INSTANCE) = Some((id.to_string(), cmd, launch_failed));
    match service_dispatcher::start(name, ffi_instance_main) {
        Ok(()) => Ok(0),
        Err(e) => {
            let console = is_console(&e);
            // A dispatcher that failed never ran the service main.
            let (_, cmd, launch_failed) = lock(&INSTANCE).take().expect("set above");
            if console {
                foreground(cmd, &launch_failed)
            } else {
                Err(launch_failed(&io_error(e)))
            }
        }
    }
}

/// Run llama-server in the foreground, its exit code.
fn foreground(cmd: Result<Command, String>, launch_failed: &LaunchFailed) -> Result<i32, String> {
    let status = cmd?.status().map_err(|e| launch_failed(&e))?;
    Ok(status.code().unwrap_or(1))
}

/// Set up the instances' services, then serve the pipe: the SCM starts the
/// ones whose start type is automatic.
fn run_manager(report: Report) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    // The service has no console, so its records go to the file under the
    // framework's machine-wide directory. The service glue gets the same
    // sink: a restart's delayed start, which runs after its call was
    // answered and so renders no record of its own, writes its failure there.
    let log: Arc<dyn ActionLog + Send + Sync> = Arc::new(FileActionLog::new(manager_log_path()));
    let manager: Arc<WinManager> = Arc::new(
        Manager::new(
            ScmService::new(exe).with_log(log.clone()),
            TokenAuthority,
            SystemResolver,
        )
        .with_log(log),
    );
    for failure in manager.service().sync(&manager.list_instances()) {
        report(&failure);
    }
    // The instances read `presets@<id>.ini`: render the pool on disk before
    // the SCM starts an instance. A render that fails leaves the files as
    // they are: the next write of a pool file renders again.
    let _ = manager.render_all();
    pipe_server::serve(manager)
}

/// What ends a service's main.
enum End {
    /// The SCM asked it to stop.
    Stop,
    /// Its work failed with this Win32 error.
    Failed(u32),
}

fn serve_main(_arguments: Vec<OsString>) {
    let report = lock(&SERVE_REPORT).take().unwrap_or(|_| {});
    let (tx, rx) = mpsc::channel();
    let Ok(status) = register(super::MANAGER_SERVICE, tx.clone(), ends_manager) else {
        return;
    };
    set(
        &status,
        ServiceState::Running,
        ServiceExitCode::NO_ERROR,
        0,
        manager_controls_accepted,
    );
    std::thread::spawn(move || {
        let failed = run_manager(report).err();
        let code = failed.and_then(|e| e.raw_os_error()).unwrap_or(1) as u32;
        let _ = tx.send(End::Failed(code));
    });
    let exit = match rx.recv() {
        Ok(End::Failed(code)) => ServiceExitCode::Win32(code),
        Ok(End::Stop) | Err(_) => ServiceExitCode::NO_ERROR,
    };
    set(
        &status,
        ServiceState::Stopped,
        exit,
        0,
        manager_controls_accepted,
    );
}

fn instance_main(_arguments: Vec<OsString>) {
    let Some((id, cmd, launch_failed)) = lock(&INSTANCE).take() else {
        return;
    };
    let Some(name) = service_name(&id) else {
        return;
    };
    let (tx, rx) = mpsc::channel();
    let Ok(status) = register(&name, tx, ends_instance) else {
        return;
    };
    let failed = |status: &ServiceStatusHandle| {
        set(
            status,
            ServiceState::Stopped,
            exit_code(false, None),
            0,
            controls_accepted,
        );
    };
    // The log first, so a launch that fails says why in it.
    let opened = pai_core::paths::instance_log(&id)
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))
        .and_then(|log| super::open_instance_log(&log));
    let Ok(mut log) = opened else {
        return failed(&status);
    };
    // The snapshot target is read from the arguments this run spawns
    // llama-server with: the port and the folder of the engine that
    // runs, never a `server.ini` that may be ahead of it.
    let target = cmd.as_ref().ok().and_then(|cmd| {
        instance_save_target(&cmd.get_args().map(OsStr::to_os_string).collect::<Vec<_>>())
    });
    let spawned = cmd.and_then(|mut cmd| {
        let (out, err) = (log.try_clone(), log.try_clone());
        out.and_then(|out| cmd.stdout(out).stderr(err?).spawn())
            .map_err(|e| launch_failed(&e))
    });
    let mut child = match spawned {
        Ok(child) => child,
        Err(text) => {
            let _ = log
                .write_all(text.as_bytes())
                .and_then(|()| log.write_all(b"\n"));
            return failed(&status);
        }
    };
    set(
        &status,
        ServiceState::Running,
        ServiceExitCode::NO_ERROR,
        0,
        controls_accepted,
    );
    let (requested, code) = loop {
        if let Ok(Some(exit)) = child.try_wait() {
            break (false, exit.code());
        }
        if let Ok(End::Stop) = rx.recv_timeout(Duration::from_millis(500)) {
            set(
                &status,
                ServiceState::StopPending,
                ServiceExitCode::NO_ERROR,
                0,
                controls_accepted,
            );
            // The snapshots first, before the kill: the save runs on a
            // worker thread, within the core's one budget, while this
            // thread raises a checkpoint every STOP_TICK so the SCM
            // sees progress within every announced hint. The model
            // rows go to the instance's log, as the launch's output
            // does.
            if let Some((port, dir)) = target {
                let (done, progress) = mpsc::channel();
                std::thread::spawn(move || {
                    let _ = done.send(pai_core::slot_save::save_all(
                        port,
                        &dir,
                        pai_core::slot_save::SAVE_BUDGET,
                    ));
                });
                let mut tick = 0u32;
                let (saved, errors) = loop {
                    match progress.recv_timeout(STOP_TICK) {
                        Ok(answer) => break answer,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            tick += 1;
                            set(
                                &status,
                                ServiceState::StopPending,
                                ServiceExitCode::NO_ERROR,
                                stop_checkpoint(tick),
                                controls_accepted,
                            );
                        }
                        // The worker died: nothing to report, the stop
                        // goes on.
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            break (Vec::new(), Vec::new())
                        }
                    }
                };
                for t in &saved {
                    let _ = writeln!(
                        log,
                        "{}",
                        crate::strings::fill("snapshot-saved-row", &[&t.describe()])
                    );
                }
                for e in &errors {
                    let _ = writeln!(log, "{}", crate::strings::fill("snapshot-failed-row", &[e]));
                }
            }
            let _ = child.kill();
            let code = child.wait().ok().and_then(|exit| exit.code());
            break (true, code);
        }
    };
    set(
        &status,
        ServiceState::Stopped,
        exit_code(requested, code),
        0,
        controls_accepted,
    );
}

/// The SCM's exit code for [`exit_report`].
fn exit_code(requested: bool, child: Option<i32>) -> ServiceExitCode {
    match exit_report(requested, child) {
        (ERROR_SERVICE_SPECIFIC_ERROR, code) => ServiceExitCode::ServiceSpecific(code),
        (win32, _) => ServiceExitCode::Win32(win32),
    }
}

/// The control handler of service `name`: `ends` says which controls end its
/// run, and such a control is sent on `tx`. The rule comes from the caller
/// because the two services part there: an instance ends on the
/// Preshutdown of a system shutdown and saves then, the manager's own
/// service ends on the Stop only. The handler only signals, the SCM wants
/// it back at once (at the most 30 s, HandlerEx Remarks); the work belongs
/// to the service main.
fn register(
    name: &str,
    tx: mpsc::Sender<End>,
    ends: fn(u32) -> bool,
) -> windows_service::Result<ServiceStatusHandle> {
    service_control_handler::register(name, move |control| {
        if ends(control.raw_service_control_type()) {
            // Every control the caller counts as the end of the run
            // signals the same stop.
            let _ = tx.send(End::Stop);
            ServiceControlHandlerResult::NoError
        } else {
            match control {
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        }
    })
}

/// Report `state` to the SCM, `checkpoint` while a stop is pending: a
/// long stop must raise it within every announced hint. The
/// `controls_accepted` of a running service are the caller's `accept`
/// (the instance's and the manager's differ), and a service that is not
/// running reports none.
fn set(
    status: &ServiceStatusHandle,
    state: ServiceState,
    exit_code: ServiceExitCode,
    checkpoint: u32,
    accept: fn(bool) -> u32,
) {
    let pending = state == ServiceState::StopPending;
    let _ = status.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: ServiceControlAccept::from_bits_truncate(accept(
            state == ServiceState::Running,
        )),
        exit_code,
        checkpoint: if pending { checkpoint } else { 0 },
        wait_hint: if pending {
            STOP_WAIT_HINT
        } else {
            Duration::ZERO
        },
        process_id: None,
    });
}

fn is_console(e: &windows_service::Error) -> bool {
    matches!(e, windows_service::Error::Winapi(io) if io.raw_os_error() == Some(NOT_A_SERVICE))
}

fn io_error(e: windows_service::Error) -> std::io::Error {
    match e {
        windows_service::Error::Winapi(io) => io,
        other => std::io::Error::other(other),
    }
}

/// A poisoned lock is taken over: what it guards is a hand-over slot.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
