//! The Windows side of the manager (running llama-server as a service), its
//! decisions as OS-neutral functions so they are tested on every OS: the
//! named pipe's security descriptor, who may do what from the facts of the
//! client's token, the SCM service of each instance (name, account, command
//! line, start type), the SCM states and errors as the core's, the services
//! to create and delete for the instances on disk, and the instance's log
//! file. The Win32 calls that feed and use them are the `cfg(windows)` glue.

use super::service::{Action, Authority, Caller, ServiceError, UnitState};
use std::path::{Path, PathBuf};

#[cfg(windows)]
pub mod entry;
#[cfg(windows)]
pub mod logon;
#[cfg(windows)]
mod pipe_server;
#[cfg(windows)]
pub mod scm;

/// The manager's own service, registered by the MSI.
pub const MANAGER_SERVICE: &str = "private-ai-manager";

/// The named pipe's security descriptor (SDDL): network logons denied,
/// SYSTEM and Administrators full access, interactive users read and write
/// data only (`pai_core::client::pipe::CLIENT_ACCESS`), never
/// `FILE_CREATE_PIPE_INSTANCE`. The last ACE gives the same client rights to
/// `NT SERVICE\private-ai-bff`, the bff's account, by its service SID
/// ([MS-LSAT] 3.1.1.1.2: fixed by the service's name); its token is neither
/// interactive nor an administrator's, so it may only ask the read-only calls.
pub const PIPE_SDDL: &str = "D:P(D;;GA;;;NU)(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12008b;;;IU)\
    (A;;0x12008b;;;S-1-5-80-4088784262-1422927641-1363593751-33154763-2531830098)";

/// The service SID of `NT SERVICE\private-ai-bff`, the one [`PIPE_SDDL`]'s
/// last ACE names: the only caller `Authenticate` answers.
const BFF_SID: &str = "S-1-5-80-4088784262-1422927641-1363593751-33154763-2531830098";

/// `CreateNamedPipeW`'s pipe mode: a byte stream, blocking, remote clients
/// rejected.
pub const PIPE_MODE: u32 = PIPE_REJECT_REMOTE_CLIENTS;

/// `PIPE_REJECT_REMOTE_CLIENTS` (`PIPE_TYPE_BYTE`, `PIPE_READMODE_BYTE` and
/// `PIPE_WAIT` are 0).
const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x0000_0008;

/// The Administrators alias, `S-1-5-32-544`.
pub const SID_ADMINISTRATORS: &str = "S-1-5-32-544";

/// LocalSystem, `S-1-5-18`.
pub const SID_LOCAL_SYSTEM: &str = "S-1-5-18";

/// The interactive logon group, `S-1-5-4`.
pub const SID_INTERACTIVE: &str = "S-1-5-4";

/// `SID_AND_ATTRIBUTES.Attributes`: enabled for access checks.
pub const SE_GROUP_ENABLED: u32 = 0x0000_0004;

/// `SID_AND_ATTRIBUTES.Attributes`: deny-only (a UAC-filtered admin token's
/// Administrators).
pub const SE_GROUP_USE_FOR_DENY_ONLY: u32 = 0x0000_0010;

/// One group of the client's token: its SID string and attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenGroup {
    pub sid: String,
    pub attributes: u32,
}

/// What the manager needs from the client's token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenFacts {
    /// The user's SID string: the [`Caller`].
    pub sid: String,
    /// The interactive group is in the token, enabled.
    pub interactive: bool,
    /// The Administrators group is in the token, enabled or deny-only.
    pub admin_member: bool,
    /// The local group `pai` is in the token, enabled.
    pub pai_member: bool,
}

impl TokenFacts {
    /// The facts of a token of user `sid` with `groups` (`TokenUser`,
    /// `TokenGroups`); `pai` is the SID of the local group `pai`, None when
    /// it was not found.
    pub fn from_groups(sid: &str, groups: &[TokenGroup], pai: Option<&str>) -> TokenFacts {
        let has = |wanted: &str, mask: u32| {
            groups
                .iter()
                .any(|g| g.sid == wanted && g.attributes & mask != 0)
        };
        // A group counts when it is enabled and not deny-only, as INTERACTIVE
        // does: a UAC-filtered token's Administrators is deny-only, and a
        // restricted token's `pai` is.
        let enabled = |wanted: &str| {
            groups.iter().any(|g| {
                g.sid == wanted
                    && g.attributes & SE_GROUP_ENABLED != 0
                    && g.attributes & SE_GROUP_USE_FOR_DENY_ONLY == 0
            })
        };
        TokenFacts {
            sid: sid.to_string(),
            interactive: enabled(SID_INTERACTIVE),
            admin_member: has(
                SID_ADMINISTRATORS,
                SE_GROUP_ENABLED | SE_GROUP_USE_FOR_DENY_ONLY,
            ),
            pai_member: pai.is_some_and(enabled),
        }
    }
}

/// Whether a caller with `facts` may do `action`: control is any interactive
/// user (an administrator, or a member of `pai`), configure a member of the
/// local group `pai` (what `Authenticate` checks on the web sign-in), plus
/// LocalSystem, whose own token holds no group of the machine.
pub fn allowed(facts: &TokenFacts, action: Action) -> bool {
    match action {
        Action::Control => facts.interactive || facts.admin_member || facts.pai_member,
        Action::Configure => facts.pai_member || facts.sid == SID_LOCAL_SYSTEM,
    }
}

/// `SID_NAME_USE`: the account type `LookupAccountNameW` reports for the
/// name it found (values from the windows-sys source, meanings from
/// learn.microsoft.com/en-us/windows/win32/api/winnt/ne-winnt-sid_name_use).
/// Only [`SID_TYPE_ALIAS`], a local group (an alias), is an account `pai`
/// may be.
pub const SID_TYPE_USER: i32 = 1;
/// See [`SID_TYPE_USER`].
pub const SID_TYPE_GROUP: i32 = 2;
/// See [`SID_TYPE_USER`].
pub const SID_TYPE_DOMAIN: i32 = 3;
/// See [`SID_TYPE_USER`]: a local group.
pub const SID_TYPE_ALIAS: i32 = 4;
/// See [`SID_TYPE_USER`].
pub const SID_TYPE_WELL_KNOWN_GROUP: i32 = 5;
/// See [`SID_TYPE_USER`].
pub const SID_TYPE_DELETED_ACCOUNT: i32 = 6;
/// See [`SID_TYPE_USER`].
pub const SID_TYPE_INVALID: i32 = 7;
/// See [`SID_TYPE_USER`].
pub const SID_TYPE_UNKNOWN: i32 = 8;
/// See [`SID_TYPE_USER`].
pub const SID_TYPE_COMPUTER: i32 = 9;
/// See [`SID_TYPE_USER`].
pub const SID_TYPE_LABEL: i32 = 10;
/// See [`SID_TYPE_USER`].
pub const SID_TYPE_LOGON_SESSION: i32 = 11;

/// The SID string `sid` of the account `LookupAccountNameW` found for the
/// group's name, accepted only when that account is a local group of this
/// computer: its type `use_for` is [`SID_TYPE_ALIAS`] and the domain the
/// call reported, `domain`, is this computer's name `computer`, compared
/// case-insensitively as Windows compares names. None for anything else.
///
/// A bare name with no system named is searched on the local computer,
/// then the primary domain, then its trusted domains (`LookupAccountNameW`:
/// learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-lookupaccountnamew),
/// so when the machine has no local `pai` a domain group, a domain user, or
/// a group of a trusted domain answers the search. Their members are not
/// this machine's `pai`: taking their SID would hand `configure` to
/// accounts nobody added.
pub fn local_alias_sid<'a>(
    use_for: i32,
    domain: &str,
    computer: &str,
    sid: &'a str,
) -> Option<&'a str> {
    (use_for == SID_TYPE_ALIAS && domain.eq_ignore_ascii_case(computer)).then_some(sid)
}

/// The Windows [`Authority`]: the facts of the connection served on this
/// thread, set by [`TokenAuthority::with_caller`]. Outside it, or for a
/// caller other than that connection's, nobody is authorized.
pub struct TokenAuthority;

thread_local! {
    /// The facts of the connection this thread serves.
    static CONNECTION: std::cell::RefCell<Option<TokenFacts>> = const { std::cell::RefCell::new(None) };
}

/// Clears this thread's connection when dropped, on return or unwind.
struct ClearConnection;

impl Drop for ClearConnection {
    fn drop(&mut self) {
        CONNECTION.with(|c| c.borrow_mut().take());
    }
}

impl TokenAuthority {
    /// Run `f` with the caller of `facts` (its SID) as this thread's
    /// connection, cleared again when `f` returns or panics.
    pub fn with_caller<T>(facts: TokenFacts, f: impl FnOnce(&Caller) -> T) -> T {
        let caller = Caller(facts.sid.clone());
        CONNECTION.with(|c| *c.borrow_mut() = Some(facts));
        let _clear = ClearConnection;
        f(&caller)
    }
}

impl Authority for TokenAuthority {
    /// Windows never prompts: the same answer as [`Self::is_authorized`].
    fn is_authorized_quietly(&self, caller: &Caller, action: Action) -> bool {
        self.is_authorized(caller, action)
    }

    fn is_authorized(&self, caller: &Caller, action: Action) -> bool {
        CONNECTION.with(|c| {
            c.borrow()
                .as_ref()
                .is_some_and(|facts| facts.sid == caller.0 && allowed(facts, action))
        })
    }

    /// The connection's own caller, whose SID is the bff's service SID.
    fn is_bff(&self, caller: &Caller) -> bool {
        CONNECTION.with(|c| {
            c.borrow()
                .as_ref()
                .is_some_and(|facts| facts.sid == caller.0 && facts.sid == BFF_SID)
        })
    }
}

/// The SCM service name of instance `id` (`llama.cpp_<id>`), None for an
/// invalid id.
pub fn service_name(id: &str) -> Option<String> {
    pai_core::paths::valid_instance_id(id).then(|| format!("{SERVICE_PREFIX}{id}"))
}

/// The SCM name of every instance's service, before the id: the one place
/// that holds the separator, so changing it is this line.
const SERVICE_PREFIX: &str = "llama.cpp_";

/// The instance id an SCM service name stands for, None when it is not one
/// of ours. Service names compare case-insensitively.
pub fn instance_of_service(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let id = lower.strip_prefix(SERVICE_PREFIX)?;
    pai_core::paths::valid_instance_id(id).then(|| id.to_string())
}

/// `SERVICE_WIN32_OWN_PROCESS`.
pub const SERVICE_WIN32_OWN_PROCESS: u32 = 0x0000_0010;
/// `SERVICE_DEMAND_START`.
pub const SERVICE_DEMAND_START: u32 = 0x0000_0003;
/// `SERVICE_AUTO_START`.
pub const SERVICE_AUTO_START: u32 = 0x0000_0002;

/// The SCM service of an instance, what `CreateServiceW` /
/// `ChangeServiceConfigW` are given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSpec {
    pub name: String,
    pub display_name: String,
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    /// `lpServiceStartName`: the virtual account, no password.
    pub account: String,
    pub service_type: u32,
    /// Given to `CreateServiceW` only: a new instance starts disabled.
    pub start_type: u32,
    /// The preshutdown timeout in milliseconds, the
    /// `SERVICE_CONFIG_PRESHUTDOWN_INFO`'s `dwPreshutdownTimeout`: how long
    /// the SCM leaves the service for the snapshot save of a reboot,
    /// [`PRESHUTDOWN_TIMEOUT`] in the SCM's unit.
    pub preshutdown_timeout_ms: u32,
}

/// The service of instance `id` run by `exe` (`pai-manager.exe`), None for an
/// invalid id.
pub fn service_spec(id: &str, exe: &Path) -> Option<ServiceSpec> {
    let name = service_name(id)?;
    Some(ServiceSpec {
        display_name: format!("llama.cpp server {id} (Private AI Framework)"),
        executable: exe.to_path_buf(),
        arguments: vec!["run-instance".into(), id.into()],
        account: format!(r"NT SERVICE\{name}"),
        service_type: SERVICE_WIN32_OWN_PROCESS,
        start_type: boot_start_type(false),
        name,
        preshutdown_timeout_ms: PRESHUTDOWN_TIMEOUT.as_millis() as u32,
    })
}

/// The service's command line as the SCM keeps it (`lpBinaryPathName`):
/// the executable quoted, then the arguments.
pub fn image_path(spec: &ServiceSpec) -> String {
    let mut line = format!("\"{}\"", spec.executable.display());
    for arg in &spec.arguments {
        line.push(' ');
        line.push_str(arg);
    }
    line
}

const SERVICE_STOPPED: u32 = 1;
const SERVICE_START_PENDING: u32 = 2;
const SERVICE_STOP_PENDING: u32 = 3;
const SERVICE_RUNNING: u32 = 4;
const SERVICE_CONTINUE_PENDING: u32 = 5;
const SERVICE_PAUSE_PENDING: u32 = 6;
const SERVICE_PAUSED: u32 = 7;
/// `dwWin32ExitCode` when `dwServiceSpecificExitCode` holds the code.
pub const ERROR_SERVICE_SPECIFIC_ERROR: u32 = 1066;
const ERROR_SERVICE_ALREADY_RUNNING: u32 = 1056;
const ERROR_SERVICE_DOES_NOT_EXIST: u32 = 1060;
const ERROR_SERVICE_NOT_ACTIVE: u32 = 1062;
/// `dwWin32ExitCode` of a service not started since the last boot.
const ERROR_SERVICE_NEVER_STARTED: u32 = 1077;

/// The core's state of a service from its `SERVICE_STATUS`.
pub fn unit_state(current_state: u32, win32_exit_code: u32, service_exit_code: u32) -> UnitState {
    match current_state {
        SERVICE_STOPPED => match (win32_exit_code, service_exit_code) {
            (0, _) | (ERROR_SERVICE_SPECIFIC_ERROR, 0) => UnitState::Inactive,
            // Created but never started since the boot: nothing failed.
            (ERROR_SERVICE_NEVER_STARTED, _) => UnitState::Inactive,
            _ => UnitState::Failed,
        },
        SERVICE_START_PENDING | SERVICE_CONTINUE_PENDING => UnitState::Activating,
        SERVICE_RUNNING | SERVICE_PAUSED => UnitState::Active,
        SERVICE_STOP_PENDING | SERVICE_PAUSE_PENDING => UnitState::Deactivating,
        _ => UnitState::Inactive,
    }
}

/// What `run-instance` reports as `dwWin32ExitCode` and
/// `dwServiceSpecificExitCode` when it stops: `requested` when the SCM asked
/// it to stop, `child` llama-server's exit code (None: it never ran).
pub fn exit_report(requested: bool, child: Option<i32>) -> (u32, u32) {
    match (requested, child) {
        (true, _) | (false, Some(0)) => (0, 0),
        // The code bit for bit: an NTSTATUS crash code is negative as i32.
        (false, Some(code)) => (ERROR_SERVICE_SPECIFIC_ERROR, code as u32),
        (false, None) => (ERROR_SERVICE_SPECIFIC_ERROR, 1),
    }
}

// ── Snapshots at the instance's own stop ───────────────────────────────
// `run-instance` writes the slot snapshots before it kills llama-server,
// on a Stop and on a Preshutdown. The values below and their meanings:
// SERVICE_STATUS (learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_status);
// the shutdown timings come from the HandlerEx Remarks
// (learn.microsoft.com/en-us/windows/win32/api/winsvc/nc-winsvc-lphandler_function_ex).

/// `dwControlsAccepted`: the service may be stopped.
pub const SERVICE_ACCEPT_STOP: u32 = 0x1;
/// `dwControlsAccepted`: the service is notified at system shutdown. Not
/// accepted: it is not the save path (see [`controls_accepted`]). The
/// shutdown bit is 0x4, 0x2 is the pause/continue bit, which no service
/// here takes.
pub const SERVICE_ACCEPT_SHUTDOWN: u32 = 0x4;
/// `dwControlsAccepted`: the service takes part in the system shutdown
/// before the shutdown begins, on its own timeout.
pub const SERVICE_ACCEPT_PRESHUTDOWN: u32 = 0x100;
/// The control code for a stop.
pub const SERVICE_CONTROL_STOP: u32 = 0x1;
/// The control code for a status poll.
pub const SERVICE_CONTROL_INTERROGATE: u32 = 0x4;
/// The control code for the shutdown notification.
pub const SERVICE_CONTROL_SHUTDOWN: u32 = 0x5;
/// The control code for the pre-shutdown notification.
pub const SERVICE_CONTROL_PRESHUTDOWN: u32 = 0xF;

/// The `dwControlsAccepted` of an instance service: a running instance
/// accepts the Stop, and the Preshutdown that saves the snapshots of a
/// reboot. Plain shutdown is not accepted: the SCM gives it about 20 s
/// (HandlerEx Remarks), too short for a snapshot write, so the reboot
/// path is the preshutdown, bounded by the service's own timeout
/// ([`PRESHUTDOWN_TIMEOUT`]). A service that is not running accepts
/// nothing.
pub fn controls_accepted(running: bool) -> u32 {
    if running {
        SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_PRESHUTDOWN
    } else {
        0
    }
}

/// Whether control `control` ends the instance's run: the Stop, and the
/// Preshutdown of a system shutdown. An ended run must save the snapshots
/// before it kills llama-server.
pub fn ends_instance(control: u32) -> bool {
    control == SERVICE_CONTROL_STOP || control == SERVICE_CONTROL_PRESHUTDOWN
}

/// The `dwControlsAccepted` of the manager's own service: a running one
/// accepts the Stop and nothing else. The Preshutdown is the instances'
/// save path only ([`controls_accepted`]): a manager service that takes it
/// is stopped at a reboot, before or while the instances write their
/// snapshots. A service that is not running accepts nothing.
pub fn manager_controls_accepted(running: bool) -> u32 {
    if running {
        SERVICE_ACCEPT_STOP
    } else {
        0
    }
}

/// Whether control `control` ends the manager's own run: the Stop only, not
/// the Preshutdown that ends an instance ([`ends_instance`]).
pub fn ends_manager(control: u32) -> bool {
    control == SERVICE_CONTROL_STOP
}

/// The `dwWaitHint` announced while the save runs: the SCM may expect a
/// new checkpoint within every hint announced ([`stop_checkpoint`]).
pub const STOP_WAIT_HINT: std::time::Duration = std::time::Duration::from_secs(10);

/// The moment of a new checkpoint while the save runs: well inside the
/// announced hint ([`STOP_WAIT_HINT`]), so progress lands in time.
pub const STOP_TICK: std::time::Duration = std::time::Duration::from_secs(2);

/// The `dwCheckPoint` at tick `tick`: it must strictly grow, a value that
/// stands still reads as a service that stopped making progress.
pub fn stop_checkpoint(tick: u32) -> u32 {
    tick.saturating_add(1)
}

/// The preshutdown timeout (`SERVICE_CONFIG_PRESHUTDOWN_INFO`) of the
/// instance services: above the save's one budget
/// (`pai_core::slot_save::SAVE_BUDGET`), so llama-server keeps time to
/// exit after a save that runs to the end of its budget.
pub const PRESHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// How long a restart's delayed start waits for the service to stop before
/// starting it again: the stop saves the snapshots first, so this must
/// outlast the save's one budget (`pai_core::slot_save::SAVE_BUDGET`) and
/// never be shorter than the preshutdown timeout ([`PRESHUTDOWN_TIMEOUT`])
/// that bounds the same save on the reboot path (ARCHITECTURE.md, "Budget").
pub const RESTART_STOP_WAIT: std::time::Duration = std::time::Duration::from_secs(300);

/// The record a restart's delayed start hands to the log of who acted when
/// the start fails: the action, the instance and the failure, every name
/// written with Rust's `{:?}` quoting, so nothing a name carries can forge
/// a second record. The manager's own records of who acted
/// (`manager/service.rs`) are the same shape, without caller and principal:
/// the delayed thread carries no request. Its own action name,
/// `restart_start`: the manager has already recorded the `restart` call it
/// answered, so a delayed start that fails must not read as a second
/// restart of the same instance.
pub fn restart_failure_line(id: &str, detail: &str) -> String {
    format!(r#"action="restart_start" target={id:?} outcome={detail:?}"#)
}

/// The restarts asked of the SCM, per instance, so a stop of an instance is
/// not undone by a restart of it that is still waiting to start it again.
/// One generation per instance: [`RestartGate::begin`] opens a restart,
/// [`RestartGate::cancel`] (a stop) makes every restart of that instance
/// stale, and the delayed start of a restart calls
/// [`RestartGate::is_current`] with the generation it was given, before it
/// starts the service. Held as an `Arc` by the SCM glue, so the manager's
/// calls and the thread of a delayed start ask the same generations.
#[derive(Default)]
pub struct RestartGate {
    generations: std::sync::Mutex<std::collections::HashMap<String, u64>>,
}

impl RestartGate {
    /// A restart of instance `id` begins: the id's generation goes up, and
    /// the new one is returned for the delayed start to ask about later.
    pub fn begin(&self, id: &str) -> u64 {
        self.bump(id)
    }

    /// Instance `id` was stopped: its generation goes up, so every restart
    /// of it that is still waiting is no longer current.
    pub fn cancel(&self, id: &str) {
        self.bump(id);
    }

    /// Whether `generation` is still the current generation of instance
    /// `id`: only a current restart may start the service again. An id with
    /// no generation was never restarted, so nothing is current for it.
    pub fn is_current(&self, id: &str, generation: u64) -> bool {
        self.generations()
            .get(id)
            .is_some_and(|current| *current == generation)
    }

    /// The id's generation, one higher than it was.
    fn bump(&self, id: &str) -> u64 {
        let mut generations = self.generations();
        let next = *generations.get(id).unwrap_or(&0) + 1;
        generations.insert(id.to_string(), next);
        next
    }

    /// The generations. A panic that poisoned the lock is taken over: the
    /// map holds `u64`s and keeps no invariant to break, and a gate that
    /// refuses to answer would leave a restart hanging on a stop that is
    /// already over.
    fn generations(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, u64>> {
        self.generations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The snapshot target of the arguments `run-instance` spawned
/// llama-server with (`Command::get_args`): the port and the folder of
/// the running engine, through `pai_core::slot_save::slot_target`. The
/// read is lossy: an `OsString` may hold bytes that are not UTF-8, and
/// the replacement character stands for them.
pub fn instance_save_target(args: &[std::ffi::OsString]) -> Option<(u16, PathBuf)> {
    let args: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    pai_core::slot_save::slot_target(&args)
}

/// An SCM failure of instance `id`'s service as the core's error: the
/// service name, the Win32 error's name and its code. Protocol data, not
/// user text.
pub fn scm_error(id: &str, code: u32) -> ServiceError {
    ServiceError(format!("{SERVICE_PREFIX}{id}: {}", win32_error(code)))
}

/// A failure of the SCM itself (opening it, listing its services), tied to
/// no instance: the Win32 error as `scm_error` names it.
pub fn scm_wide_error(code: u32) -> ServiceError {
    ServiceError(format!("service control manager: {}", win32_error(code)))
}

/// The Win32 error `code` by its name and code, or its bare code.
fn win32_error(code: u32) -> String {
    let name = match code {
        5 => "ERROR_ACCESS_DENIED",
        1053 => "ERROR_SERVICE_REQUEST_TIMEOUT",
        1058 => "ERROR_SERVICE_DISABLED",
        ERROR_SERVICE_DOES_NOT_EXIST => "ERROR_SERVICE_DOES_NOT_EXIST",
        1064 => "ERROR_EXCEPTION_IN_SERVICE",
        1069 => "ERROR_SERVICE_LOGON_FAILED",
        1072 => "ERROR_SERVICE_MARKED_FOR_DELETE",
        _ => return format!("Win32 error {code}"),
    };
    format!("{name} ({code})")
}

/// What `scm::ScmService::sync` could not do: the SCM itself, or one
/// instance's service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncFailure {
    Scm(ServiceError),
    Instance(String, ServiceError),
}

/// `StartService` failed with `code`: already running is a success.
pub fn start_failure(id: &str, code: u32) -> Result<(), ServiceError> {
    match code {
        ERROR_SERVICE_ALREADY_RUNNING => Ok(()),
        _ => Err(scm_error(id, code)),
    }
}

/// `ControlService(STOP)` or opening the service failed with `code`: not
/// running, or no service at all, is a success.
pub fn stop_failure(id: &str, code: u32) -> Result<(), ServiceError> {
    match code {
        ERROR_SERVICE_NOT_ACTIVE | ERROR_SERVICE_DOES_NOT_EXIST => Ok(()),
        _ => Err(scm_error(id, code)),
    }
}

/// Opening or querying the service failed with `code`: no service reads as
/// inactive, as a unit systemd has not loaded.
pub fn status_failure(id: &str, code: u32) -> Result<UnitState, ServiceError> {
    match code {
        ERROR_SERVICE_DOES_NOT_EXIST => Ok(UnitState::Inactive),
        _ => Err(scm_error(id, code)),
    }
}

/// The start type (`dwStartType`) that makes an instance's service start
/// when the system starts, or not.
pub fn boot_start_type(enabled: bool) -> u32 {
    if enabled {
        SERVICE_AUTO_START
    } else {
        SERVICE_DEMAND_START
    }
}

/// Whether a service of start type `start_type` starts when the system
/// starts.
pub fn boot_enabled_of(start_type: u32) -> bool {
    start_type == SERVICE_AUTO_START
}

/// The SCM changes that match the services to the instances on disk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServicePlan {
    /// Instances with no service: create it.
    pub create: Vec<String>,
    /// Instances with a service: apply the spec again (the install
    /// directory may have moved).
    pub keep: Vec<String>,
    /// Services of ours with no instance: stop and delete them.
    pub delete: Vec<String>,
}

/// The plan for `instances` (ids on disk) against `services` (every SCM
/// service name); services that are not ours are left alone.
pub fn service_plan(instances: &[String], services: &[String]) -> ServicePlan {
    let ours: Vec<String> = services
        .iter()
        .filter_map(|s| instance_of_service(s))
        .collect();
    let mut plan = ServicePlan::default();
    for id in instances {
        if !pai_core::paths::valid_instance_id(id) {
            continue;
        }
        if ours.contains(id) {
            plan.keep.push(id.clone());
        } else {
            plan.create.push(id.clone());
        }
    }
    for id in ours {
        if !instances.contains(&id) && !plan.delete.contains(&id) {
            plan.delete.push(id);
        }
    }
    plan
}

/// Where the previous run's log of `log` is kept (`<stem>.previous.log`).
pub fn previous_log(log: &Path) -> PathBuf {
    let stem = log.file_stem().unwrap_or_default().to_string_lossy();
    log.with_file_name(format!("{stem}.previous.log"))
}

/// Open `log` for a new run of the instance: its directory created, a
/// non-empty log of the run before moved to [`previous_log`] (replacing
/// the older one), then an empty file. A failure leaves both files as they
/// were.
pub fn open_instance_log(log: &Path) -> std::io::Result<std::fs::File> {
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if std::fs::metadata(log).is_ok_and(|m| m.len() > 0) {
        std::fs::rename(log, previous_log(log))?;
    }
    std::fs::File::create(log)
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
