//! [`ScmService`]: the core's [`ServiceControl`] over the Service Control
//! Manager, each instance the service `llama.cpp_<id>` ([`service_spec`]).
//! The decisions are the OS-neutral functions of the parent module; this is
//! only the Win32 calls.

use super::{
    image_path, scm_error, service_name, service_plan, service_spec, start_failure, status_failure,
    stop_failure, unit_state, ServiceSpec,
};
use crate::manager::service::{ServiceControl, ServiceError, UnitState};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_MORE_DATA};
use windows_sys::Win32::Storage::FileSystem::DELETE;
use windows_sys::Win32::System::Services::{
    ChangeServiceConfigW, CloseServiceHandle, ControlService, CreateServiceW, DeleteService,
    EnumServicesStatusExW, OpenSCManagerW, OpenServiceW, QueryServiceStatus, StartServiceW,
    ENUM_SERVICE_STATUS_PROCESSW, SC_ENUM_PROCESS_INFO, SC_HANDLE, SC_MANAGER_CONNECT,
    SC_MANAGER_CREATE_SERVICE, SC_MANAGER_ENUMERATE_SERVICE, SERVICE_CHANGE_CONFIG,
    SERVICE_CONTROL_STOP, SERVICE_ERROR_NORMAL, SERVICE_QUERY_STATUS, SERVICE_START,
    SERVICE_STATE_ALL, SERVICE_STATUS, SERVICE_STOP, SERVICE_WIN32,
};

/// `ERROR_SERVICE_DOES_NOT_EXIST`: open the service to create it.
const NO_SERVICE: u32 = 1060;
/// `ERROR_INVALID_PARAMETER`: an id the core should have refused.
const INVALID: u32 = 87;
/// `ERROR_SERVICE_REQUEST_TIMEOUT`: a restart whose stop never ended.
const TIMEOUT: u32 = 1053;
/// How long a restart waits for the service to stop.
const STOP_WAIT: Duration = Duration::from_secs(30);

/// The SCM, its instance services run by `exe` (`pai-manager.exe`).
pub struct ScmService {
    exe: PathBuf,
}

impl ScmService {
    pub fn new(exe: PathBuf) -> Self {
        Self { exe }
    }

    /// Match the services to `instances` (the ids on disk): create the
    /// missing ones, apply the spec again to the others, stop and delete
    /// ours that have no instance. The failures, by id.
    pub fn sync(&self, instances: &[String]) -> Vec<(String, ServiceError)> {
        let access = SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE | SC_MANAGER_ENUMERATE_SERVICE;
        let scm = match open_scm(access) {
            Ok(scm) => scm,
            Err(code) => return vec![(String::new(), scm_error("", code))],
        };
        let services = match service_names(&scm) {
            Ok(names) => names,
            Err(code) => return vec![(String::new(), scm_error("", code))],
        };
        let plan = service_plan(instances, &services);
        let mut failures = Vec::new();
        for id in &plan.create {
            if let Err(e) = self.spec(id).and_then(|spec| {
                create(&scm, &spec, SERVICE_QUERY_STATUS).map_err(|c| scm_error(id, c))
            }) {
                failures.push((id.clone(), e));
            }
        }
        for id in &plan.keep {
            if let Err(e) = self.spec(id).and_then(|spec| {
                open_service(&scm, &spec.name, SERVICE_CHANGE_CONFIG)
                    .and_then(|service| reconfigure(&service, &spec))
                    .map_err(|c| scm_error(id, c))
            }) {
                failures.push((id.clone(), e));
            }
        }
        for id in &plan.delete {
            let _ = self.stop(id);
            let deleted = service_name(id)
                .ok_or(INVALID)
                .and_then(|name| open_service(&scm, &name, DELETE))
                .and_then(|service| ok(unsafe { DeleteService(service.0) }));
            if let Err(code) = deleted {
                failures.push((id.clone(), scm_error(id, code)));
            }
        }
        failures
    }

    fn spec(&self, id: &str) -> Result<ServiceSpec, ServiceError> {
        service_spec(id, &self.exe).ok_or_else(|| scm_error(id, INVALID))
    }
}

impl ServiceControl for ScmService {
    /// Start the service, created first when it does not exist yet.
    fn start(&self, id: &str) -> Result<(), ServiceError> {
        let spec = self.spec(id)?;
        let scm = open_scm(SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE)
            .map_err(|c| scm_error(id, c))?;
        let access = SERVICE_START | SERVICE_QUERY_STATUS;
        let service = match open_service(&scm, &spec.name, access) {
            Err(NO_SERVICE) => create(&scm, &spec, access),
            opened => opened,
        }
        .map_err(|c| scm_error(id, c))?;
        // SAFETY: a valid service handle, no arguments.
        match ok(unsafe { StartServiceW(service.0, 0, null()) }) {
            Ok(()) => Ok(()),
            Err(code) => start_failure(id, code),
        }
    }

    fn stop(&self, id: &str) -> Result<(), ServiceError> {
        let name = service_name(id).ok_or_else(|| scm_error(id, INVALID))?;
        let stopped = open_scm(SC_MANAGER_CONNECT)
            .and_then(|scm| open_service(&scm, &name, SERVICE_STOP))
            .and_then(|service| {
                let mut status = SERVICE_STATUS::default();
                // SAFETY: a valid service handle and status out-parameter.
                ok(unsafe { ControlService(service.0, SERVICE_CONTROL_STOP, &mut status) })
            });
        match stopped {
            Ok(()) => Ok(()),
            Err(code) => stop_failure(id, code),
        }
    }

    /// Stop, wait until the service has stopped, start.
    fn restart(&self, id: &str) -> Result<(), ServiceError> {
        self.stop(id)?;
        let deadline = Instant::now() + STOP_WAIT;
        while matches!(
            self.status(id)?,
            UnitState::Active | UnitState::Activating | UnitState::Deactivating
        ) {
            if Instant::now() >= deadline {
                return Err(scm_error(id, TIMEOUT));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        self.start(id)
    }

    fn status(&self, id: &str) -> Result<UnitState, ServiceError> {
        let name = service_name(id).ok_or_else(|| scm_error(id, INVALID))?;
        let queried = open_scm(SC_MANAGER_CONNECT)
            .and_then(|scm| open_service(&scm, &name, SERVICE_QUERY_STATUS))
            .and_then(|service| {
                let mut status = SERVICE_STATUS::default();
                // SAFETY: a valid service handle and status out-parameter.
                ok(unsafe { QueryServiceStatus(service.0, &mut status) }).map(|()| status)
            });
        match queried {
            Ok(s) => Ok(unit_state(
                s.dwCurrentState,
                s.dwWin32ExitCode,
                s.dwServiceSpecificExitCode,
            )),
            Err(code) => status_failure(id, code),
        }
    }
}

/// An SCM handle, closed when dropped.
struct Handle(SC_HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle this module opened, closed once.
        unsafe { CloseServiceHandle(self.0) };
    }
}

fn last_error() -> u32 {
    // SAFETY: no preconditions.
    unsafe { GetLastError() }
}

fn ok(result: windows_sys::core::BOOL) -> Result<(), u32> {
    if result == 0 {
        Err(last_error())
    } else {
        Ok(())
    }
}

fn handle(h: SC_HANDLE) -> Result<Handle, u32> {
    if h.is_null() {
        Err(last_error())
    } else {
        Ok(Handle(h))
    }
}

fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

fn open_scm(access: u32) -> Result<Handle, u32> {
    // SAFETY: the local machine's active database.
    handle(unsafe { OpenSCManagerW(null(), null(), access) })
}

fn open_service(scm: &Handle, name: &str, access: u32) -> Result<Handle, u32> {
    let name = wide(name);
    // SAFETY: a valid SCM handle and a NUL-terminated name.
    handle(unsafe { OpenServiceW(scm.0, name.as_ptr(), access) })
}

/// `CreateServiceW` with `spec`: the virtual account takes no password.
fn create(scm: &Handle, spec: &ServiceSpec, access: u32) -> Result<Handle, u32> {
    let (name, display) = (wide(&spec.name), wide(&spec.display_name));
    let (path, account) = (wide(image_path(spec)), wide(&spec.account));
    // SAFETY: a valid SCM handle; every string NUL-terminated and alive
    // for the call; the optional ones null.
    handle(unsafe {
        CreateServiceW(
            scm.0,
            name.as_ptr(),
            display.as_ptr(),
            access,
            spec.service_type,
            spec.start_type,
            SERVICE_ERROR_NORMAL,
            path.as_ptr(),
            null(),
            null_mut(),
            null(),
            account.as_ptr(),
            null(),
        )
    })
}

/// `ChangeServiceConfigW` with `spec` (the password left as it is).
fn reconfigure(service: &Handle, spec: &ServiceSpec) -> Result<(), u32> {
    let (display, path, account) = (
        wide(&spec.display_name),
        wide(image_path(spec)),
        wide(&spec.account),
    );
    // SAFETY: a valid service handle; strings as in `create`.
    ok(unsafe {
        ChangeServiceConfigW(
            service.0,
            spec.service_type,
            spec.start_type,
            SERVICE_ERROR_NORMAL,
            path.as_ptr(),
            null(),
            null_mut(),
            null(),
            account.as_ptr(),
            null(),
            display.as_ptr(),
        )
    })
}

/// The name of every Win32 service (`EnumServicesStatusExW`, resumed
/// while it answers `ERROR_MORE_DATA`).
fn service_names(scm: &Handle) -> Result<Vec<String>, u32> {
    let mut names = Vec::new();
    let mut resume = 0u32;
    let mut bytes = 64 * 1024;
    loop {
        // u64s: the entries hold pointers and need their alignment.
        let mut buffer = vec![0u64; (bytes as usize).div_ceil(8)];
        let (mut needed, mut count) = (0u32, 0u32);
        // SAFETY: a valid SCM handle with ENUMERATE_SERVICE; the buffer is
        // as long as said, aligned for the entries.
        let done = unsafe {
            EnumServicesStatusExW(
                scm.0,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut needed,
                &mut count,
                &mut resume,
                null(),
            )
        } != 0;
        if !done {
            let code = last_error();
            if code != ERROR_MORE_DATA {
                return Err(code);
            }
        }
        // SAFETY: the call wrote `count` entries at the buffer's start, their
        // names NUL-terminated strings inside the buffer.
        let entries = unsafe {
            std::slice::from_raw_parts(
                buffer.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>(),
                count as usize,
            )
        };
        for entry in entries {
            names.push(unsafe { from_wide(entry.lpServiceName) });
        }
        if done {
            return Ok(names);
        }
        bytes = needed.max(bytes);
    }
}

/// A NUL-terminated UTF-16 string as a `String`.
///
/// # Safety
/// `p` points to a NUL-terminated UTF-16 string.
unsafe fn from_wide(p: *const u16) -> String {
    let mut len = 0;
    while *p.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}
