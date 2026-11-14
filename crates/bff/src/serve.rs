//! Serving the router until a stop: SIGTERM or Ctrl-C in the foreground,
//! the service manager's stop control under Windows' SCM (`windows`).

use std::future::Future;
use std::net::SocketAddr;

/// Why serving ended other than by a stop.
#[derive(Debug)]
pub enum ServeError {
    /// The tokio runtime could not be built.
    Runtime(std::io::Error),
    /// The address could not be bound.
    Bind(std::io::Error),
    /// Serving failed.
    Serve(std::io::Error),
}

/// Serve [`crate::router`] on `addr` until `stop` resolves, then let the
/// open requests finish and return.
pub fn serve(
    addr: SocketAddr,
    stop: impl Future<Output = ()> + Send + 'static,
) -> Result<(), ServeError> {
    // The IO driver also drives tokio's signals.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .map_err(ServeError::Runtime)?;
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(ServeError::Bind)?;
        axum::serve(listener, crate::router())
            .with_graceful_shutdown(stop)
            .await
            .map_err(ServeError::Serve)
    })
}

/// Resolves on Ctrl-C, and on Unix on SIGTERM too (how systemd stops a
/// unit). A signal that cannot be watched is left unwatched.
pub async fn stop_signal() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let term = async {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = term => {}
    }
}

/// `pai-bff service` under the Windows service dispatcher.
#[cfg(windows)]
pub mod windows {
    use super::{serve, ServeError};
    use std::ffi::OsString;
    use std::net::SocketAddr;
    use std::sync::Mutex;
    use std::time::Duration;
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{
        self, ServiceControlHandlerResult, ServiceStatusHandle,
    };
    use windows_service::{define_windows_service, service_dispatcher};

    /// The service's name, as the MSI installs it.
    const SERVICE: &str = "private-ai-bff";

    /// The dispatcher's error when the process was not started by the SCM.
    const NOT_A_SERVICE: i32 = 1063;

    /// Why `pai-bff service` did not run as a service.
    #[derive(Debug)]
    pub enum ServiceError {
        /// Not started by the SCM (a console).
        NotAService,
        /// The dispatcher failed.
        Dispatcher(std::io::Error),
    }

    /// The address the service main serves on.
    static ADDR: Mutex<Option<SocketAddr>> = Mutex::new(None);

    define_windows_service!(ffi_service_main, service_main);

    /// Hand the process to the SCM's dispatcher, the service serving on
    /// `addr` until its stop control; returns when the service stopped.
    pub fn run(addr: SocketAddr) -> Result<(), ServiceError> {
        *lock(&ADDR) = Some(addr);
        service_dispatcher::start(SERVICE, ffi_service_main).map_err(|e| match e {
            windows_service::Error::Winapi(io) if io.raw_os_error() == Some(NOT_A_SERVICE) => {
                ServiceError::NotAService
            }
            windows_service::Error::Winapi(io) => ServiceError::Dispatcher(io),
            other => ServiceError::Dispatcher(std::io::Error::other(other)),
        })
    }

    fn service_main(_arguments: Vec<OsString>) {
        let Some(addr) = lock(&ADDR).take() else {
            return;
        };
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let mut tx = Some(tx);
        let registered = service_control_handler::register(SERVICE, move |control| match control {
            ServiceControl::Stop => {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(());
                }
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        });
        let Ok(status) = registered else {
            return;
        };
        set(&status, ServiceState::Running, ServiceExitCode::NO_ERROR);
        let stop = async {
            let _ = rx.await;
        };
        let exit = match serve(addr, stop) {
            Ok(()) => ServiceExitCode::NO_ERROR,
            Err(ServeError::Runtime(e) | ServeError::Bind(e) | ServeError::Serve(e)) => {
                ServiceExitCode::Win32(e.raw_os_error().unwrap_or(1) as u32)
            }
        };
        set(&status, ServiceState::Stopped, exit);
    }

    /// Report `state` to the SCM; only a running service takes a stop.
    fn set(status: &ServiceStatusHandle, state: ServiceState, exit_code: ServiceExitCode) {
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
            wait_hint: Duration::ZERO,
            process_id: None,
        });
    }

    /// A poisoned lock is taken over: what it guards is a hand-over slot.
    fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }
}
