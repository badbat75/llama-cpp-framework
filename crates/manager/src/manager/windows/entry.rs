//! `serve` and `run-instance` as Windows services, under the SCM's
//! dispatcher; run from a console (the dispatcher refuses with
//! `ERROR_FAILED_SERVICE_CONTROLLER_CONNECT`), each runs in the foreground
//! instead.

use super::pipe_server::{self, WinManager};
use super::scm::ScmService;
use super::{exit_report, service_name, SyncFailure, TokenAuthority, ERROR_SERVICE_SPECIFIC_ERROR};
use crate::manager::service::{Manager, SystemResolver};
use std::ffi::OsString;
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
    let manager: Arc<WinManager> = Arc::new(Manager::new(
        ScmService::new(exe),
        TokenAuthority,
        SystemResolver,
    ));
    for failure in manager.service().sync(&manager.list_instances()) {
        report(&failure);
    }
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
    let Ok(status) = register(super::MANAGER_SERVICE, tx.clone()) else {
        return;
    };
    set(&status, ServiceState::Running, ServiceExitCode::NO_ERROR);
    std::thread::spawn(move || {
        let failed = run_manager(report).err();
        let code = failed.and_then(|e| e.raw_os_error()).unwrap_or(1) as u32;
        let _ = tx.send(End::Failed(code));
    });
    let exit = match rx.recv() {
        Ok(End::Failed(code)) => ServiceExitCode::Win32(code),
        Ok(End::Stop) | Err(_) => ServiceExitCode::NO_ERROR,
    };
    set(&status, ServiceState::Stopped, exit);
}

fn instance_main(_arguments: Vec<OsString>) {
    let Some((id, cmd, launch_failed)) = lock(&INSTANCE).take() else {
        return;
    };
    let Some(name) = service_name(&id) else {
        return;
    };
    let (tx, rx) = mpsc::channel();
    let Ok(status) = register(&name, tx) else {
        return;
    };
    let failed = |status: &ServiceStatusHandle| {
        set(status, ServiceState::Stopped, exit_code(false, None));
    };
    // The log first, so a launch that fails says why in it.
    let opened = pai_core::paths::instance_log(&id)
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))
        .and_then(|log| super::open_instance_log(&log));
    let Ok(mut log) = opened else {
        return failed(&status);
    };
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
    set(&status, ServiceState::Running, ServiceExitCode::NO_ERROR);
    let (requested, code) = loop {
        if let Ok(Some(exit)) = child.try_wait() {
            break (false, exit.code());
        }
        if let Ok(End::Stop) = rx.recv_timeout(Duration::from_millis(500)) {
            set(
                &status,
                ServiceState::StopPending,
                ServiceExitCode::NO_ERROR,
            );
            let _ = child.kill();
            let code = child.wait().ok().and_then(|exit| exit.code());
            break (true, code);
        }
    };
    set(&status, ServiceState::Stopped, exit_code(requested, code));
}

/// The SCM's exit code for [`exit_report`].
fn exit_code(requested: bool, child: Option<i32>) -> ServiceExitCode {
    match exit_report(requested, child) {
        (ERROR_SERVICE_SPECIFIC_ERROR, code) => ServiceExitCode::ServiceSpecific(code),
        (win32, _) => ServiceExitCode::Win32(win32),
    }
}

/// The control handler of service `name`: a stop is sent on `tx`.
fn register(name: &str, tx: mpsc::Sender<End>) -> windows_service::Result<ServiceStatusHandle> {
    service_control_handler::register(name, move |control| match control {
        ServiceControl::Stop => {
            let _ = tx.send(End::Stop);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })
}

/// Report `state` to the SCM; only a running service takes a stop.
fn set(status: &ServiceStatusHandle, state: ServiceState, exit_code: ServiceExitCode) {
    let pending = state == ServiceState::StopPending;
    let _ = status.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == ServiceState::Running {
            ServiceControlAccept::STOP
        } else {
            ServiceControlAccept::empty()
        },
        exit_code,
        checkpoint: 0,
        wait_hint: if pending {
            Duration::from_secs(10)
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
