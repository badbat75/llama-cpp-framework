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
mod tests {
    use super::*;
    use pai_core::client::pipe::CLIENT_ACCESS;

    // ── Start at boot: the service's start type ─────────────────────────
    // Values from windows-sys (Microsoft's metadata), meanings from
    // CreateServiceW / ChangeServiceConfigW's dwStartType
    // (learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-changeserviceconfigw).

    /// `SERVICE_AUTO_START` (2): started by the SCM during system startup.
    /// `SERVICE_DEMAND_START` (3): started only when asked, the start type
    /// a new instance's service is created with, so it starts disabled.
    #[test]
    fn the_start_at_boot_is_the_automatic_start_type_and_its_absence_manual() {
        assert_eq!(boot_start_type(true), 2, "SERVICE_AUTO_START");
        assert_eq!(boot_start_type(false), 3, "SERVICE_DEMAND_START");
        assert_eq!(boot_start_type(false), SERVICE_DEMAND_START);
        let spec = service_spec("default", Path::new(EXE)).expect("a valid id");
        assert_eq!(spec.start_type, boot_start_type(false), "created disabled");
    }

    /// Only an automatic start starts the service at boot (a delayed one is
    /// the same start type with a flag on the side); manual, disabled
    /// (`SERVICE_DISABLED`, 4) and the drivers' boot and system starts (0,
    /// 1) do not.
    #[test]
    fn only_the_automatic_start_type_reads_as_started_at_boot() {
        for (start_type, enabled) in [(2, true), (3, false), (4, false), (0, false), (1, false)] {
            assert_eq!(boot_enabled_of(start_type), enabled, "{start_type}");
        }
        for enabled in [true, false] {
            assert_eq!(boot_enabled_of(boot_start_type(enabled)), enabled);
        }
    }

    /// The Win32 side of the start at boot (scm.rs, compiled on Windows
    /// only, so pinned on its text): it reads the start type with
    /// `QueryServiceConfigW` and changes it through the two functions
    /// above. The spec applied again at every manager start (`sync`'s
    /// `reconfigure`) passes `SERVICE_NO_CHANGE` as the start type, so it
    /// never undoes the user's choice; only `CreateServiceW` takes the
    /// spec's (manual) start type.
    #[test]
    fn the_scm_backend_reads_and_changes_the_start_type_and_sync_keeps_it() {
        let scm = include_str!("windows/scm.rs");
        for want in [
            "QueryServiceConfigW",
            "boot_start_type(",
            "boot_enabled_of(",
        ] {
            assert!(scm.contains(want), "scm.rs does not use {want}");
        }
        let start = scm
            .find("fn reconfigure(")
            .expect("scm.rs keeps `reconfigure`");
        let body = &scm[start..];
        let body = &body[..body.find("\n}\n").expect("the end of reconfigure")];
        assert!(body.contains("SERVICE_NO_CHANGE"), "{body}");
        assert!(!body.contains("spec.start_type"), "{body}");
    }

    // ── The pipe's security descriptor ──────────────────────────────────

    /// One ACE of an SDDL DACL: (type, flags, rights, trustee).
    fn aces(sddl: &str) -> Vec<(String, String, String, String)> {
        let dacl = sddl
            .strip_prefix("D:")
            .unwrap_or_else(|| panic!("{sddl:?}"));
        let body = dacl.trim_start_matches(|c: char| c.is_ascii_uppercase());
        assert!(body.starts_with('(') && body.ends_with(')'), "{sddl:?}");
        body[1..body.len() - 1]
            .split(")(")
            .map(|ace| {
                let f: Vec<&str> = ace.split(';').collect();
                assert_eq!(f.len(), 6, "ace {ace:?}");
                assert_eq!((f[3], f[4]), ("", ""), "no object GUIDs: {ace:?}");
                (f[0].into(), f[1].into(), f[2].into(), f[5].into())
            })
            .collect()
    }

    #[test]
    fn the_pipe_denies_network_logons_first() {
        let aces = aces(PIPE_SDDL);
        assert_eq!(
            aces.first(),
            Some(&("D".into(), "".into(), "GA".into(), "NU".into())),
            "{PIPE_SDDL:?}"
        );
    }

    #[test]
    fn the_pipe_gives_system_and_administrators_full_access() {
        let aces = aces(PIPE_SDDL);
        for who in ["SY", "BA"] {
            assert!(
                aces.contains(&("A".into(), "".into(), "GA".into(), who.into())),
                "{who}: {PIPE_SDDL:?}"
            );
        }
    }

    /// Interactive users read and write data and nothing else: the rights
    /// pai-core's client asks for, without `FILE_CREATE_PIPE_INSTANCE`
    /// (0x4, which `FILE_GENERIC_WRITE` and `GW` include), so no user can
    /// create an instance of the manager's pipe.
    #[test]
    fn interactive_users_get_exactly_the_client_s_rights() {
        let aces = aces(PIPE_SDDL);
        let iu: Vec<_> = aces.iter().filter(|a| a.3 == "IU").collect();
        assert_eq!(iu.len(), 1, "{PIPE_SDDL:?}");
        let (kind, flags, rights, _) = iu[0];
        assert_eq!((kind.as_str(), flags.as_str()), ("A", ""));
        let mask = u32::from_str_radix(rights.trim_start_matches("0x"), 16)
            .unwrap_or_else(|_| panic!("rights as hex: {rights:?}"));
        assert_eq!(mask, CLIENT_ACCESS);
        assert_eq!(mask & 0x4, 0, "FILE_CREATE_PIPE_INSTANCE: {rights}");
    }

    #[test]
    fn nobody_else_is_on_the_pipe() {
        let trustees: Vec<String> = aces(PIPE_SDDL).into_iter().map(|a| a.3).collect();
        let bff = service_sid(BFF_SERVICE);
        assert_eq!(
            trustees,
            ["NU", "SY", "BA", "IU", bff.as_str()],
            "{PIPE_SDDL:?}"
        );
    }

    /// The bff's Windows service, as the MSI installs it and `pai-bff
    /// service` registers its control handler under.
    const BFF_SERVICE: &str = "private-ai-bff";

    /// The SID of service `name`, as Windows derives it ([MS-LSAT] 3.1.1.1.2,
    /// "Configurable Translation Database and Corresponding View"): the name
    /// upper-cased in UTF-16, its SHA-1 read as five unsigned 32-bit
    /// integers, `S-1-5-80-h0-h1-h2-h3-h4`. A virtual account
    /// `NT SERVICE\<name>` has it as its user SID.
    fn service_sid(name: &str) -> String {
        use sha1::{Digest, Sha1};
        let utf16: Vec<u8> = name
            .to_uppercase()
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let hash = Sha1::digest(&utf16);
        let words: Vec<String> = hash
            .chunks(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]).to_string())
            .collect();
        format!("S-1-5-80-{}", words.join("-"))
    }

    /// The helper against known SIDs: the example in [MS-LSAT] 3.1.1.1.2
    /// (ALG) and `NT SERVICE\TrustedInstaller`, whose SID owns the system's
    /// files; the case of the name does not matter.
    #[test]
    fn the_service_sid_rule_gives_the_documented_sids() {
        assert_eq!(
            service_sid("ALG"),
            "S-1-5-80-2387347252-3645287876-2469496166-3824418187-3586569773"
        );
        assert_eq!(
            service_sid("TrustedInstaller"),
            "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464"
        );
        assert_eq!(service_sid("alg"), service_sid("ALG"));
    }

    /// The bff runs as the virtual account `NT SERVICE\private-ai-bff`,
    /// which is not interactive: the pipe gives its SID (the service SID,
    /// fixed by the service's name, so a constant in the descriptor) the
    /// client's rights and nothing more, so it can connect and ask the calls
    /// that need no authorization.
    #[test]
    fn the_bff_s_account_gets_exactly_the_client_s_rights() {
        let bff = service_sid(BFF_SERVICE);
        let aces = aces(PIPE_SDDL);
        let mine: Vec<_> = aces.iter().filter(|a| a.3 == bff).collect();
        assert_eq!(mine.len(), 1, "one ACE for {bff}: {PIPE_SDDL:?}");
        let (kind, flags, rights, _) = mine[0];
        assert_eq!((kind.as_str(), flags.as_str()), ("A", ""));
        let mask = u32::from_str_radix(rights.trim_start_matches("0x"), 16)
            .unwrap_or_else(|_| panic!("rights as hex: {rights:?}"));
        assert_eq!(mask, CLIENT_ACCESS);
        assert_eq!(mask & 0x4, 0, "FILE_CREATE_PIPE_INSTANCE: {rights}");
    }

    /// Connecting is all the bff gets: its token (the service SID as user,
    /// SERVICE and ALL SERVICES, no INTERACTIVE, no Administrators) may
    /// neither control nor configure, so through the pipe it reads the
    /// instances and their states only.
    #[test]
    fn the_bff_s_token_may_neither_control_nor_configure() {
        let bff = service_sid(BFF_SERVICE);
        let mut groups = common();
        groups.push(group("S-1-5-6", 0x7)); // SERVICE
        groups.push(group("S-1-5-80-0", 0x7)); // NT SERVICE\ALL SERVICES
        let facts = TokenFacts::from_groups(&bff, &groups, Some(PAI));
        assert_eq!(
            facts,
            TokenFacts {
                sid: bff.clone(),
                interactive: false,
                admin_member: false,
                pai_member: false,
            }
        );
        for action in [Action::Control, Action::Configure] {
            assert!(!allowed(&facts, action), "{action:?}");
            let authorized = TokenAuthority::with_caller(facts.clone(), |c| {
                TokenAuthority.is_authorized(c, action)
            });
            assert!(!authorized, "{action:?} through the authority");
        }
    }

    /// The bff's token as the pipe sees it: its service SID, SERVICE and
    /// ALL SERVICES, no INTERACTIVE, no Administrators.
    fn bff_facts() -> TokenFacts {
        let mut groups = common();
        groups.push(group("S-1-5-6", 0x7));
        groups.push(group("S-1-5-80-0", 0x7));
        TokenFacts::from_groups(&service_sid(BFF_SERVICE), &groups, Some(PAI))
    }

    /// The connection of `NT SERVICE\private-ai-bff` is the bff, by its
    /// service SID, the only caller `Authenticate` answers.
    #[test]
    fn the_bff_s_connection_is_the_bff() {
        let is_bff = TokenAuthority::with_caller(bff_facts(), |c| TokenAuthority.is_bff(c));
        assert!(is_bff);
    }

    /// Any other connection is not: a user, an administrator, SYSTEM, another
    /// service's account; nor a caller other than the connection's (the
    /// bff's SID named by another connection), nor any caller outside a
    /// connection or on another thread.
    #[test]
    fn no_other_connection_is_the_bff() {
        let bff = Caller(service_sid(BFF_SERVICE));
        for f in [
            facts(true, false, false),
            facts(true, true, false),
            TokenFacts::from_groups("S-1-5-18", &common(), Some(PAI)),
            TokenFacts::from_groups(&service_sid("private-ai-manager"), &common(), Some(PAI)),
            TokenFacts::from_groups(&service_sid("llama.cpp_default"), &common(), Some(PAI)),
        ] {
            let sid = f.sid.clone();
            let (own, forged) = TokenAuthority::with_caller(f, |c| {
                (TokenAuthority.is_bff(c), TokenAuthority.is_bff(&bff))
            });
            assert!(!own, "{sid} taken for the bff");
            assert!(!forged, "{sid}'s connection naming the bff's SID");
        }
        assert!(!TokenAuthority.is_bff(&bff), "outside any connection");
        TokenAuthority::with_caller(bff_facts(), |caller| {
            let caller = caller.clone();
            let seen = std::thread::spawn(move || TokenAuthority.is_bff(&caller))
                .join()
                .expect("thread");
            assert!(!seen, "another thread has no connection");
            assert!(!TokenAuthority.is_bff(&Caller(String::new())));
        });
    }

    #[test]
    fn the_pipe_is_a_local_byte_stream() {
        const PIPE_TYPE_MESSAGE: u32 = 0x4;
        const PIPE_READMODE_MESSAGE: u32 = 0x2;
        const PIPE_NOWAIT: u32 = 0x1;
        const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x8;
        assert_eq!(PIPE_MODE, PIPE_REJECT_REMOTE_CLIENTS, "{PIPE_MODE:#x}");
        assert_eq!(
            PIPE_MODE & (PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_NOWAIT),
            0
        );
    }

    // ── Who may do what ──────────────────────────────────────────────────

    const USER: &str = "S-1-5-21-1004336348-1177238915-682003330-1001";

    fn group(sid: &str, attributes: u32) -> TokenGroup {
        TokenGroup {
            sid: sid.into(),
            attributes,
        }
    }

    /// Groups as a real token holds them: Everyone, Users, Authenticated
    /// Users, the logon SID, Local (attributes 0x7: mandatory, enabled by
    /// default, enabled).
    fn common() -> Vec<TokenGroup> {
        vec![
            group("S-1-1-0", 0x7),
            group("S-1-5-32-545", 0x7),
            group("S-1-5-11", 0x7),
            group("S-1-5-5-0-389150", 0xC000_0007),
            group("S-1-2-0", 0x7),
        ]
    }

    fn facts(interactive: bool, admin_member: bool, pai_member: bool) -> TokenFacts {
        TokenFacts {
            sid: USER.into(),
            interactive,
            admin_member,
            pai_member,
        }
    }

    #[test]
    fn the_well_known_sids() {
        assert_eq!(SID_ADMINISTRATORS, "S-1-5-32-544");
        assert_eq!(SID_INTERACTIVE, "S-1-5-4");
    }

    #[test]
    fn a_token_with_no_groups_is_nobody() {
        assert_eq!(
            TokenFacts::from_groups(USER, &[], Some(PAI)),
            facts(false, false, false)
        );
        assert_eq!(
            TokenFacts::from_groups(USER, &common(), Some(PAI)),
            facts(false, false, false)
        );
    }

    #[test]
    fn a_network_logon_is_not_interactive() {
        let mut groups = common();
        groups.push(group("S-1-5-2", 0x7));
        assert_eq!(
            TokenFacts::from_groups(USER, &groups, Some(PAI)),
            facts(false, false, false)
        );
    }

    #[test]
    fn a_standard_user_at_the_console_is_interactive() {
        let mut groups = common();
        groups.push(group(SID_INTERACTIVE, 0x7));
        assert_eq!(
            TokenFacts::from_groups(USER, &groups, Some(PAI)),
            facts(true, false, false)
        );
    }

    /// A deny-only INTERACTIVE (a restricted token) grants nothing.
    #[test]
    fn a_deny_only_interactive_group_does_not_count() {
        let mut groups = common();
        groups.push(group(SID_INTERACTIVE, SE_GROUP_USE_FOR_DENY_ONLY));
        assert_eq!(
            TokenFacts::from_groups(USER, &groups, Some(PAI)),
            facts(false, false, false)
        );
    }

    /// The owner of a personal PC, not elevated: UAC's filtered token holds
    /// Administrators deny-only (0x10); still a member.
    #[test]
    fn a_filtered_admin_token_is_an_admin_member() {
        let mut groups = common();
        groups.push(group(SID_INTERACTIVE, 0x7));
        groups.push(group(SID_ADMINISTRATORS, SE_GROUP_USE_FOR_DENY_ONLY));
        assert_eq!(
            TokenFacts::from_groups(USER, &groups, Some(PAI)),
            facts(true, true, false)
        );
    }

    /// Elevated: Administrators enabled and owner (0xF).
    #[test]
    fn an_elevated_admin_token_is_an_admin_member() {
        let mut groups = common();
        groups.push(group(SID_ADMINISTRATORS, 0xF));
        groups.push(group(SID_INTERACTIVE, 0x7));
        assert_eq!(
            TokenFacts::from_groups(USER, &groups, Some(PAI)),
            facts(true, true, false)
        );
    }

    /// The SID string is compared whole: a domain group whose SID merely
    /// starts like the alias is not it.
    #[test]
    fn a_look_alike_sid_is_not_administrators() {
        let mut groups = common();
        groups.push(group("S-1-5-32-5440", 0x7));
        groups.push(group("S-1-5-41", 0x7));
        assert_eq!(
            TokenFacts::from_groups(USER, &groups, Some(PAI)),
            facts(false, false, false)
        );
    }

    #[test]
    fn the_user_sid_is_kept() {
        assert_eq!(
            TokenFacts::from_groups("S-1-5-18", &[], Some(PAI)).sid,
            "S-1-5-18"
        );
    }

    /// The SID of a local group `pai` as `LookupAccountNameW` gives it: the
    /// machine's domain SID and a RID of its own (1000 and up).
    const PAI: &str = "S-1-5-21-1004336348-1177238915-682003330-1009";

    /// The computer name `GetComputerNameW` answers for the fixture machine.
    const COMPUTER: &str = "WORKSTATION-7";

    #[test]
    fn the_account_use_values_are_the_windows_ones() {
        assert_eq!(SID_TYPE_USER, 1);
        assert_eq!(SID_TYPE_GROUP, 2);
        assert_eq!(SID_TYPE_DOMAIN, 3);
        assert_eq!(SID_TYPE_ALIAS, 4);
        assert_eq!(SID_TYPE_WELL_KNOWN_GROUP, 5);
        assert_eq!(SID_TYPE_DELETED_ACCOUNT, 6);
        assert_eq!(SID_TYPE_INVALID, 7);
        assert_eq!(SID_TYPE_UNKNOWN, 8);
        assert_eq!(SID_TYPE_COMPUTER, 9);
        assert_eq!(SID_TYPE_LABEL, 10);
        assert_eq!(SID_TYPE_LOGON_SESSION, 11);
    }

    /// The looked-up SID is this machine's `pai` only when the account is a
    /// local group (an alias) of this computer: everything the name search
    /// finds beyond the local computer (its order: local, primary domain,
    /// trusted domains) is refused.
    #[test]
    fn the_pai_sid_is_accepted_only_for_a_local_alias_of_this_computer() {
        // The local group `pai`: an alias, whose domain is the computer.
        assert_eq!(
            local_alias_sid(SID_TYPE_ALIAS, COMPUTER, COMPUTER, PAI),
            Some(PAI)
        );
        // Windows compares names case-insensitively: `LookupAccountNameW`
        // reports the domain upper-case, `GetComputerNameW` answers with
        // the name as it was set.
        assert_eq!(
            local_alias_sid(SID_TYPE_ALIAS, "workstation-7", COMPUTER, PAI),
            Some(PAI)
        );
        assert_eq!(
            local_alias_sid(SID_TYPE_ALIAS, COMPUTER, "workstation-7", PAI),
            Some(PAI)
        );
        // An alias of another computer or domain (a trusted domain's local
        // group): its members are not this machine's.
        assert_eq!(
            local_alias_sid(SID_TYPE_ALIAS, "CORP", COMPUTER, PAI),
            None,
            "alias of another domain"
        );
        // A domain group named `pai` (SidTypeGroup), in the domain, and as
        // an account type on this computer's name: not an alias.
        assert_eq!(
            local_alias_sid(SID_TYPE_GROUP, "CORP", COMPUTER, PAI),
            None,
            "domain global group"
        );
        assert_eq!(
            local_alias_sid(SID_TYPE_GROUP, COMPUTER, COMPUTER, PAI),
            None,
            "a global group is not an alias"
        );
        // A user named `pai`, local or the domain's (SidTypeUser).
        assert_eq!(
            local_alias_sid(SID_TYPE_USER, COMPUTER, COMPUTER, PAI),
            None,
            "local user"
        );
        assert_eq!(
            local_alias_sid(SID_TYPE_USER, "CORP", COMPUTER, PAI),
            None,
            "domain user"
        );
        // Every other account type, and an account with no domain reported.
        for use_for in [
            SID_TYPE_DOMAIN,
            SID_TYPE_WELL_KNOWN_GROUP,
            SID_TYPE_DELETED_ACCOUNT,
            SID_TYPE_INVALID,
            SID_TYPE_UNKNOWN,
            SID_TYPE_COMPUTER,
            SID_TYPE_LABEL,
            SID_TYPE_LOGON_SESSION,
            0,
        ] {
            assert_eq!(
                local_alias_sid(use_for, COMPUTER, COMPUTER, PAI),
                None,
                "account type {use_for}"
            );
        }
        assert_eq!(local_alias_sid(SID_TYPE_ALIAS, "", COMPUTER, PAI), None);
    }

    /// End to end through the token's facts: with no local `pai`, the
    /// domain's group (or a user) named `pai` that the name search finds is
    /// refused, so a token holding its SID grants no `configure`; the same
    /// token with the local alias accepted does configure.
    #[test]
    fn a_domain_account_named_pai_makes_nobody_a_member() {
        let mut groups = common();
        groups.push(group(SID_INTERACTIVE, 0x7));
        groups.push(group(PAI, 0x7));
        for (use_for, domain) in [
            (SID_TYPE_GROUP, "CORP"),
            (SID_TYPE_ALIAS, "CORP"),
            (SID_TYPE_USER, "CORP"),
            (SID_TYPE_USER, COMPUTER),
        ] {
            let pai = local_alias_sid(use_for, domain, COMPUTER, PAI);
            let f = TokenFacts::from_groups(USER, &groups, pai);
            assert!(!f.pai_member, "{use_for} in {domain:?}: {f:?}");
            assert!(
                !allowed(&f, Action::Configure),
                "{use_for} in {domain:?}: {f:?}"
            );
        }
        let pai = local_alias_sid(SID_TYPE_ALIAS, &COMPUTER.to_lowercase(), COMPUTER, PAI);
        let f = TokenFacts::from_groups(USER, &groups, pai);
        assert!(f.pai_member, "{f:?}");
        assert!(allowed(&f, Action::Configure), "{f:?}");
    }

    /// The group's SID is looked up from its name (`LookupAccountNameW`) for
    /// each connection, not cached at the manager's start: a group created
    /// after the start, or deleted and created again (a new SID), counts at
    /// once. The name is the manager's one (`ADMIN_GROUP`, `pai`), the same
    /// `Authenticate` checks. The answer counts only through
    /// [`local_alias_sid`]: a local alias of this computer.
    #[test]
    fn the_pipe_looks_the_pai_group_up_by_name_for_each_connection() {
        let src = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/manager/windows/pipe_server.rs"),
        )
        .expect("pipe_server.rs");
        let start = src.find("fn client_facts(").expect("fn client_facts");
        let body = &src[start..];
        let end = body.find("\n}\n").expect("end of client_facts");
        let body = &body[..end];
        assert!(
            src.contains("LookupAccountNameW"),
            "pipe_server.rs never calls LookupAccountNameW"
        );
        assert!(
            body.contains("ADMIN_GROUP"),
            "client_facts does not name the group by ADMIN_GROUP:\n{body}"
        );
        assert!(
            !body.contains("from_groups(&sid, &groups, None)"),
            "client_facts passes no pai SID:\n{body}"
        );
        assert!(
            !src.contains("OnceLock") && !src.contains("LazyLock") && !src.contains("static PAI"),
            "the pai SID is cached"
        );
        let gs = src.find("fn group_sid(").expect("fn group_sid");
        let rest = &src[gs..];
        let gs = &rest[..rest.find("\n}\n").expect("end of group_sid")];
        assert!(
            gs.contains("local_alias_sid("),
            "group_sid hands over the looked-up SID without filtering it \
             through local_alias_sid:\n{gs}"
        );
    }

    /// A token holding `pai` enabled (a local group in a user's token:
    /// mandatory, enabled by default, enabled) is a member, at the console
    /// or not; nothing else changes.
    #[test]
    fn a_token_holding_pai_is_a_pai_member() {
        let mut groups = common();
        groups.push(group(PAI, 0x7));
        assert_eq!(
            TokenFacts::from_groups(USER, &groups, Some(PAI)),
            facts(false, false, true)
        );
        groups.push(group(SID_INTERACTIVE, 0x7));
        assert_eq!(
            TokenFacts::from_groups(USER, &groups, Some(PAI)),
            facts(true, false, true)
        );
    }

    /// A deny-only `pai` (a restricted token) grants nothing, as a deny-only
    /// INTERACTIVE grants nothing.
    #[test]
    fn a_deny_only_pai_group_does_not_count() {
        let mut groups = common();
        groups.push(group(PAI, SE_GROUP_USE_FOR_DENY_ONLY));
        assert_eq!(
            TokenFacts::from_groups(USER, &groups, Some(PAI)),
            facts(false, false, false)
        );
        let mut groups = common();
        groups.push(group(PAI, 0x7 | SE_GROUP_USE_FOR_DENY_ONLY));
        assert!(!TokenFacts::from_groups(USER, &groups, Some(PAI)).pai_member);
    }

    /// No group `pai` on the machine (deleted, or the lookup failed): nobody
    /// is a member, whatever the token holds.
    #[test]
    fn no_pai_group_makes_nobody_a_member() {
        let mut groups = common();
        groups.push(group(PAI, 0x7));
        groups.push(group(SID_ADMINISTRATORS, 0xF));
        let f = TokenFacts::from_groups(USER, &groups, None);
        assert!(!f.pai_member, "{f:?}");
        assert!(!allowed(&f, Action::Configure), "{f:?}");
    }

    /// The SID is compared whole: a group whose SID starts like `pai`'s, or
    /// another local group of the same machine, is not `pai`; nor is the
    /// Administrators alias.
    #[test]
    fn a_look_alike_sid_is_not_pai() {
        let mut groups = common();
        groups.push(group(&format!("{PAI}0"), 0x7));
        groups.push(group("S-1-5-21-1004336348-1177238915-682003330-1010", 0x7));
        groups.push(group(SID_ADMINISTRATORS, 0xF));
        let f = TokenFacts::from_groups(USER, &groups, Some(PAI));
        assert!(!f.pai_member, "{f:?}");
        assert!(!allowed(&f, Action::Configure), "{f:?}");
    }

    #[test]
    fn nobody_may_do_anything() {
        for action in [Action::Control, Action::Configure] {
            assert!(!allowed(&facts(false, false, false), action), "{action:?}");
        }
    }

    #[test]
    fn a_standard_user_controls_but_never_configures() {
        assert!(allowed(&facts(true, false, false), Action::Control));
        assert!(!allowed(&facts(true, false, false), Action::Configure));
    }

    /// An administrator still controls, but configures only as a member of
    /// `pai` (step 10 slice 6b part 2b): Administrators alone, filtered or
    /// elevated, no longer configures.
    #[test]
    fn an_admin_member_controls_but_configures_only_in_pai() {
        for interactive in [true, false] {
            let f = facts(interactive, true, false);
            assert!(allowed(&f, Action::Control), "{f:?}");
            assert!(!allowed(&f, Action::Configure), "{f:?}");
            let f = facts(interactive, true, true);
            assert!(allowed(&f, Action::Configure), "{f:?}");
        }
    }

    /// A member of `pai` configures, at the console or not, administrator
    /// or not; configuring includes controlling.
    #[test]
    fn a_pai_member_configures_and_controls() {
        for (interactive, admin) in [(true, false), (false, false), (true, true), (false, true)] {
            let f = facts(interactive, admin, true);
            assert!(allowed(&f, Action::Configure), "{f:?}");
            assert!(allowed(&f, Action::Control), "{f:?}");
        }
    }

    /// The real tokens end to end: a standard user at the console added to
    /// `pai` configures; the owner of the PC (filtered or elevated) who is
    /// not in `pai` does not.
    #[test]
    fn configure_follows_the_pai_group_in_the_token() {
        let mut member = common();
        member.push(group(SID_INTERACTIVE, 0x7));
        member.push(group(PAI, 0x7));
        let member = TokenFacts::from_groups(USER, &member, Some(PAI));
        assert!(allowed(&member, Action::Configure), "{member:?}");
        for admin in [SE_GROUP_USE_FOR_DENY_ONLY, 0xF] {
            let mut owner = common();
            owner.push(group(SID_INTERACTIVE, 0x7));
            owner.push(group(SID_ADMINISTRATORS, admin));
            let owner = TokenFacts::from_groups(USER, &owner, Some(PAI));
            assert!(allowed(&owner, Action::Control), "{owner:?}");
            assert!(!allowed(&owner, Action::Configure), "{owner:?}");
        }
    }

    /// LocalSystem (the manager's own account, the MSI's deferred custom
    /// actions) configures as it did through Administrators, like root on
    /// Linux; its token is not in `pai`.
    #[test]
    fn local_system_still_configures() {
        let mut groups = common();
        groups.push(group(SID_ADMINISTRATORS, 0xE)); // enabled by default, enabled, owner
        let system = TokenFacts::from_groups(SID_LOCAL_SYSTEM, &groups, Some(PAI));
        assert!(!system.pai_member, "{system:?}");
        assert!(allowed(&system, Action::Configure), "{system:?}");
        assert!(allowed(&system, Action::Control), "{system:?}");
        let system = TokenFacts::from_groups(SID_LOCAL_SYSTEM, &groups, None);
        assert!(
            allowed(&system, Action::Configure),
            "no pai group: {system:?}"
        );
    }

    /// Only the SID itself is LocalSystem: a user whose token merely holds
    /// SYSTEM as a group (or a look-alike SID) is not.
    #[test]
    fn only_the_local_system_user_sid_is_local_system() {
        assert_eq!(SID_LOCAL_SYSTEM, "S-1-5-18");
        let mut groups = common();
        groups.push(group(SID_LOCAL_SYSTEM, 0x7));
        let f = TokenFacts::from_groups(USER, &groups, Some(PAI));
        assert!(!allowed(&f, Action::Configure), "{f:?}");
        let f = TokenFacts::from_groups("S-1-5-180", &common(), Some(PAI));
        assert!(!allowed(&f, Action::Configure), "{f:?}");
    }

    #[test]
    fn no_connection_on_this_thread_is_refused() {
        for action in [Action::Control, Action::Configure] {
            assert!(!TokenAuthority.is_authorized(&Caller(USER.into()), action));
        }
    }

    #[test]
    fn the_connection_s_caller_is_its_sid_and_decides() {
        let (control, configure) =
            TokenAuthority::with_caller(facts(true, false, false), |caller| {
                assert_eq!(caller, &Caller(USER.into()));
                (
                    TokenAuthority.is_authorized(caller, Action::Control),
                    TokenAuthority.is_authorized(caller, Action::Configure),
                )
            });
        assert_eq!((control, configure), (true, false));
    }

    /// Windows never prompts: the quiet question answers as the plain one,
    /// from the token (configure is a member of `pai`, not of
    /// Administrators), and a caller other than the connection's, or no
    /// connection, is nobody.
    #[test]
    fn the_quiet_question_answers_from_the_token_as_the_plain_one() {
        for (interactive, admin, pai) in [
            (true, false, false),
            (true, true, false),
            (false, true, false),
            (false, false, false),
            (true, false, true),
            (false, false, true),
            (true, true, true),
        ] {
            let f = facts(interactive, admin, pai);
            let configure = TokenAuthority::with_caller(f.clone(), |caller| {
                TokenAuthority.is_authorized_quietly(caller, Action::Configure)
            });
            assert_eq!(configure, pai, "quiet configure is pai membership: {f:?}");
            let (quiet, plain) = TokenAuthority::with_caller(f.clone(), |caller| {
                let q = [Action::Control, Action::Configure]
                    .map(|a| TokenAuthority.is_authorized_quietly(caller, a));
                let p = [Action::Control, Action::Configure]
                    .map(|a| TokenAuthority.is_authorized(caller, a));
                (q, p)
            });
            assert_eq!(quiet, plain, "{f:?}");
        }
        let other = Caller("S-1-5-21-1-2-3-500".into());
        let refused = TokenAuthority::with_caller(facts(true, true, true), |_| {
            !TokenAuthority.is_authorized_quietly(&other, Action::Configure)
        });
        assert!(refused, "another caller than the connection's");
        assert!(
            !TokenAuthority.is_authorized_quietly(&Caller(USER.into()), Action::Configure),
            "no connection on this thread"
        );
    }

    /// Another caller than the connection's (a stale or forged one) is
    /// nobody.
    #[test]
    fn another_caller_than_the_connection_s_is_refused() {
        let other = Caller("S-1-5-21-1-2-3-500".into());
        let refused = TokenAuthority::with_caller(facts(true, true, false), |_| {
            !TokenAuthority.is_authorized(&other, Action::Control)
                && !TokenAuthority.is_authorized(&Caller(String::new()), Action::Control)
        });
        assert!(refused);
    }

    #[test]
    fn the_connection_is_cleared_when_it_ends() {
        TokenAuthority::with_caller(facts(true, true, false), |_| {});
        assert!(!TokenAuthority.is_authorized(&Caller(USER.into()), Action::Control));
        let panicked = std::panic::catch_unwind(|| {
            TokenAuthority::with_caller(facts(true, true, false), |_| panic!("in the call"))
        });
        assert!(panicked.is_err());
        assert!(!TokenAuthority.is_authorized(&Caller(USER.into()), Action::Configure));
    }

    /// Each connection thread sees its own caller only.
    #[test]
    fn connections_on_other_threads_are_apart() {
        TokenAuthority::with_caller(facts(true, true, false), |caller| {
            let caller = caller.clone();
            let seen = std::thread::spawn(move || {
                TokenAuthority.is_authorized(&caller, Action::Configure)
            })
            .join()
            .expect("thread");
            assert!(!seen, "another thread has no connection");
        });
    }

    // ── The SCM service of an instance ──────────────────────────────────

    #[test]
    fn the_manager_s_own_service() {
        assert_eq!(MANAGER_SERVICE, "private-ai-manager");
    }

    #[test]
    fn invalid_ids_have_no_service() {
        for bad in ["", "Default", "a/b", "a\\b", "../x", "a@b", "a b"] {
            assert_eq!(service_name(bad), None, "{bad:?}");
            assert_eq!(service_spec(bad, Path::new("pai-manager.exe")), None);
        }
    }

    #[test]
    fn the_service_is_named_after_the_instance_with_an_underscore() {
        assert_eq!(
            service_name("default").as_deref(),
            Some("llama.cpp_default")
        );
        assert_eq!(service_name("lan-2").as_deref(), Some("llama.cpp_lan-2"));
    }

    #[test]
    fn foreign_services_are_no_instance() {
        for name in [
            "",
            "llama.cpp",
            "llama.cpp_",
            "llama.cpp__",
            "llama.cpp__default",
            "llama.cpp_a_b",
            "llama.cpp_a@b",
            "llama.cpp-default",
            "xllama.cpp_default",
            // The Linux unit's name is not the Windows service's.
            "llama.cpp@default",
            "llama.cpp@lan-2",
            "private-ai-manager",
            "Spooler",
        ] {
            assert_eq!(instance_of_service(name), None, "{name:?}");
        }
    }

    /// No id holds a `_` (`paths::valid_instance_id`) and the prefix is
    /// fixed, so every service name reads back as the one id it was made of.
    #[test]
    fn the_service_name_reads_back_as_its_id() {
        for id in ["default", "lan-2", "8080", "a", "0-1-2", "x-"] {
            let name = service_name(id).expect("a valid id");
            assert_eq!(instance_of_service(&name).as_deref(), Some(id), "{name}");
        }
    }

    #[test]
    fn our_services_name_their_instance_in_any_case() {
        assert_eq!(
            instance_of_service("llama.cpp_default").as_deref(),
            Some("default")
        );
        // The SCM compares names case-insensitively: a service a user
        // created as LLAMA.CPP_LAN-2 is the one of instance lan-2.
        assert_eq!(
            instance_of_service("LLAMA.CPP_LAN-2").as_deref(),
            Some("lan-2")
        );
    }

    const EXE: &str = r"C:\Program Files\private-ai\bin\pai-manager.exe";

    #[test]
    fn the_service_runs_run_instance_on_demand_as_its_virtual_account() {
        let spec = service_spec("lan-2", Path::new(EXE)).expect("a valid id");
        assert_eq!(
            spec,
            ServiceSpec {
                name: "llama.cpp_lan-2".into(),
                display_name: "llama.cpp server lan-2 (Private AI Framework)".into(),
                executable: PathBuf::from(EXE),
                arguments: vec!["run-instance".into(), "lan-2".into()],
                account: r"NT SERVICE\llama.cpp_lan-2".into(),
                service_type: SERVICE_WIN32_OWN_PROCESS,
                start_type: SERVICE_DEMAND_START,
            }
        );
    }

    /// The arguments are the ones `parse_args` reads back.
    #[test]
    fn the_service_s_arguments_parse_as_run_instance() {
        let spec = service_spec("default", Path::new(EXE)).expect("a valid id");
        assert_eq!(
            crate::manager::parse_args(&spec.arguments),
            Ok(crate::manager::ManagerCommand::RunInstance(
                "default".into()
            ))
        );
    }

    /// CreateServiceW: a path with a space must be quoted.
    #[test]
    fn the_image_path_quotes_the_executable() {
        let spec = service_spec("default", Path::new(EXE)).expect("a valid id");
        assert_eq!(
            image_path(&spec),
            r#""C:\Program Files\private-ai\bin\pai-manager.exe" run-instance default"#
        );
        let spec = service_spec("lan-2", Path::new(r"D:\pai\pai-manager.exe")).expect("id");
        assert_eq!(
            image_path(&spec),
            r#""D:\pai\pai-manager.exe" run-instance lan-2"#
        );
    }

    // ── SCM states and errors ────────────────────────────────────────────

    const STOPPED: u32 = 1;
    const START_PENDING: u32 = 2;
    const STOP_PENDING: u32 = 3;
    const RUNNING: u32 = 4;
    const CONTINUE_PENDING: u32 = 5;
    const PAUSE_PENDING: u32 = 6;
    const PAUSED: u32 = 7;
    const ERROR_SERVICE_SPECIFIC_ERROR: u32 = 1066;

    #[test]
    fn a_service_that_stopped_with_an_error_failed() {
        assert_eq!(unit_state(STOPPED, 1067, 0), UnitState::Failed);
        assert_eq!(
            unit_state(STOPPED, ERROR_SERVICE_SPECIFIC_ERROR, 1),
            UnitState::Failed
        );
        assert_eq!(
            unit_state(STOPPED, ERROR_SERVICE_SPECIFIC_ERROR, 0xC000_0005),
            UnitState::Failed
        );
    }

    #[test]
    fn a_service_that_stopped_cleanly_is_inactive() {
        assert_eq!(unit_state(STOPPED, 0, 0), UnitState::Inactive);
        assert_eq!(
            unit_state(STOPPED, 0, 7),
            UnitState::Inactive,
            "ignored without 1066"
        );
        assert_eq!(
            unit_state(STOPPED, ERROR_SERVICE_SPECIFIC_ERROR, 0),
            UnitState::Inactive
        );
    }

    /// The manager creates an instance's service at its start without
    /// starting it; until a start since the last boot, the SCM reports it
    /// stopped with ERROR_SERVICE_NEVER_STARTED (1077, "No attempts to start
    /// the service have been made since the last boot"): nothing failed.
    #[test]
    fn a_service_never_started_is_inactive() {
        const ERROR_SERVICE_NEVER_STARTED: u32 = 1077;
        assert_eq!(
            unit_state(STOPPED, ERROR_SERVICE_NEVER_STARTED, 0),
            UnitState::Inactive
        );
    }

    #[test]
    fn scm_states_map_to_unit_states() {
        assert_eq!(unit_state(START_PENDING, 0, 0), UnitState::Activating);
        assert_eq!(unit_state(CONTINUE_PENDING, 0, 0), UnitState::Activating);
        assert_eq!(unit_state(RUNNING, 0, 0), UnitState::Active);
        assert_eq!(unit_state(PAUSED, 0, 0), UnitState::Active);
        assert_eq!(unit_state(STOP_PENDING, 0, 0), UnitState::Deactivating);
        assert_eq!(unit_state(PAUSE_PENDING, 0, 0), UnitState::Deactivating);
        // The exit code is only read once stopped.
        assert_eq!(unit_state(RUNNING, 1067, 3), UnitState::Active);
    }

    #[test]
    fn an_unknown_scm_state_reads_as_inactive() {
        for state in [0, 8, u32::MAX] {
            assert_eq!(unit_state(state, 0, 0), UnitState::Inactive, "{state}");
        }
    }

    #[test]
    fn a_requested_stop_is_a_clean_stop() {
        for child in [Some(0), Some(1), Some(-1), None] {
            assert_eq!(exit_report(true, child), (0, 0), "{child:?}");
        }
    }

    #[test]
    fn llama_server_exiting_by_itself_reports_its_code() {
        assert_eq!(exit_report(false, Some(0)), (0, 0));
        assert_eq!(
            exit_report(false, Some(1)),
            (ERROR_SERVICE_SPECIFIC_ERROR, 1)
        );
        // An NTSTATUS crash code (access violation) is kept bit for bit.
        assert_eq!(
            exit_report(false, Some(0xC000_0005_u32 as i32)),
            (ERROR_SERVICE_SPECIFIC_ERROR, 0xC000_0005)
        );
        // It never ran: a failure, with no code of its own.
        assert_eq!(exit_report(false, None), (ERROR_SERVICE_SPECIFIC_ERROR, 1));
    }

    /// What run-instance reports is what Status reads back.
    #[test]
    fn the_exit_report_reads_back_as_the_unit_state() {
        let state = |requested, child| {
            let (win32, specific) = exit_report(requested, child);
            unit_state(STOPPED, win32, specific)
        };
        assert_eq!(state(false, Some(1)), UnitState::Failed);
        assert_eq!(state(false, None), UnitState::Failed);
        assert_eq!(state(false, Some(0)), UnitState::Inactive);
        assert_eq!(state(true, Some(1)), UnitState::Inactive);
    }

    /// A failure of the SCM itself names no instance's service: no
    /// `llama.cpp_` with an empty id, the Win32 error as `scm_error` names it.
    #[test]
    fn an_scm_wide_error_names_no_instance_service() {
        for (code, name) in [
            (5, "ERROR_ACCESS_DENIED"),
            (1053, "ERROR_SERVICE_REQUEST_TIMEOUT"),
            (1072, "ERROR_SERVICE_MARKED_FOR_DELETE"),
        ] {
            assert_eq!(
                scm_wide_error(code),
                ServiceError(format!("service control manager: {name} ({code})"))
            );
        }
        assert_eq!(
            scm_wide_error(87),
            ServiceError("service control manager: Win32 error 87".into())
        );
        assert!(!scm_wide_error(5).0.contains(SERVICE_PREFIX));
    }

    /// The code of `scm.rs` (`cfg(windows)`, only type-checked on Linux),
    /// whole-line comments left out.
    fn scm_code() -> String {
        include_str!("windows/scm.rs")
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// `sync` reports a failure of the SCM itself as its own kind, never as
    /// the failure of an instance with an empty id ("instance ``").
    #[test]
    fn sync_reports_an_scm_failure_apart_from_the_instances() {
        let code = scm_code();
        assert!(
            !code.contains("scm_error(\"\""),
            "an SCM failure as an instance with an empty id"
        );
        assert!(!code.contains("(String::new(), "), "an empty instance id");
        assert!(
            code.contains("-> Vec<SyncFailure>"),
            "sync returns its failures as SyncFailure"
        );
        assert!(
            code.matches("SyncFailure::Scm(scm_wide_error(").count() >= 2,
            "opening the SCM and listing its services are SCM-wide failures"
        );
        assert!(code.contains("SyncFailure::Instance("));
    }

    /// `serve`'s doc says what the manager reports at its start: the
    /// services it could not set up; it starts no instance (the SCM starts
    /// the enabled ones).
    #[test]
    fn serve_s_doc_says_set_up_not_started() {
        let entry = include_str!("windows/entry.rs");
        let at = entry.find("pub fn serve(").expect("serve");
        let doc: Vec<&str> = entry[..at]
            .lines()
            .rev()
            .take_while(|l| l.trim_start().starts_with("///"))
            .collect();
        assert!(!doc.is_empty(), "serve has no doc comment");
        let doc = doc.into_iter().rev().collect::<Vec<_>>().join("\n");
        assert!(!doc.contains("started"), "{doc}");
        assert!(doc.contains("set up"), "{doc}");
    }

    #[test]
    fn scm_errors_name_the_service_and_the_error() {
        for (code, name) in [
            (5, "ERROR_ACCESS_DENIED"),
            (1053, "ERROR_SERVICE_REQUEST_TIMEOUT"),
            (1058, "ERROR_SERVICE_DISABLED"),
            (1060, "ERROR_SERVICE_DOES_NOT_EXIST"),
            (1064, "ERROR_EXCEPTION_IN_SERVICE"),
            (1069, "ERROR_SERVICE_LOGON_FAILED"),
            (1072, "ERROR_SERVICE_MARKED_FOR_DELETE"),
        ] {
            assert_eq!(
                scm_error("lan-2", code),
                ServiceError(format!("llama.cpp_lan-2: {name} ({code})"))
            );
        }
        assert_eq!(
            scm_error("default", 87),
            ServiceError("llama.cpp_default: Win32 error 87".into())
        );
    }

    #[test]
    fn start_failures_are_errors_but_already_running() {
        for code in [5, 1053, 1058, 1060, 1069, 1072] {
            assert_eq!(
                start_failure("default", code),
                Err(scm_error("default", code))
            );
        }
        assert_eq!(start_failure("default", 1056), Ok(()));
    }

    #[test]
    fn stop_failures_are_errors_but_not_running_or_no_service() {
        for code in [5, 1053, 1061] {
            assert_eq!(
                stop_failure("default", code),
                Err(scm_error("default", code))
            );
        }
        assert_eq!(stop_failure("default", 1062), Ok(()));
        assert_eq!(stop_failure("default", 1060), Ok(()));
    }

    #[test]
    fn status_failures_are_errors_but_no_service() {
        for code in [5, 6, 1072] {
            assert_eq!(
                status_failure("default", code),
                Err(scm_error("default", code))
            );
        }
        assert_eq!(status_failure("default", 1060), Ok(UnitState::Inactive));
    }

    // ── The services for the instances on disk ──────────────────────────

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn nothing_on_disk_and_no_service_plans_nothing() {
        assert_eq!(service_plan(&[], &[]), ServicePlan::default());
        let foreign = ids(&[
            "Spooler",
            "private-ai-manager",
            "llama.cpp",
            "llama.cpp_Bad_id",
        ]);
        assert_eq!(service_plan(&[], &foreign), ServicePlan::default());
    }

    /// What `uninstall-services` applies (`ScmService::sync` with no
    /// instance): every instance's service goes, whatever its case, and
    /// nobody else's, the manager's own included (the MSI removes that one).
    #[test]
    fn with_no_instance_every_instance_service_is_deleted() {
        let services = ids(&[
            "Spooler",
            "private-ai-manager",
            "llama.cpp_default",
            "LLAMA.CPP_LAN-2",
            "llama.cpp",
            "llama.cpp_Bad_id",
        ]);
        assert_eq!(
            service_plan(&[], &services),
            ServicePlan {
                delete: ids(&["default", "lan-2"]),
                ..ServicePlan::default()
            }
        );
    }

    #[test]
    fn a_new_instance_gets_its_service() {
        let plan = service_plan(&ids(&["default", "lan-2"]), &ids(&["Spooler"]));
        assert_eq!(
            plan,
            ServicePlan {
                create: ids(&["default", "lan-2"]),
                ..ServicePlan::default()
            }
        );
    }

    #[test]
    fn a_removed_instance_loses_its_service() {
        let plan = service_plan(
            &ids(&["default"]),
            &ids(&["llama.cpp_default", "llama.cpp_old", "Spooler"]),
        );
        assert_eq!(
            plan,
            ServicePlan {
                keep: ids(&["default"]),
                delete: ids(&["old"]),
                ..ServicePlan::default()
            }
        );
    }

    #[test]
    fn a_service_in_another_case_is_the_instance_s() {
        let plan = service_plan(&ids(&["lan-2"]), &ids(&["LLAMA.CPP_LAN-2"]));
        assert_eq!(
            plan,
            ServicePlan {
                keep: ids(&["lan-2"]),
                ..ServicePlan::default()
            }
        );
    }

    /// A crafted id that reached the list is never turned into a service.
    #[test]
    fn an_invalid_id_on_disk_is_never_created() {
        let plan = service_plan(&ids(&["default", "../x", "Upper"]), &[]);
        assert_eq!(plan.create, ids(&["default"]));
    }

    // ── The instance's log ───────────────────────────────────────────────

    #[test]
    fn the_previous_log_sits_beside_the_log() {
        let log = Path::new("logs").join("llama-server@lan-2.log");
        assert_eq!(
            previous_log(&log),
            Path::new("logs").join("llama-server@lan-2.previous.log")
        );
    }

    #[test]
    fn a_log_whose_directory_cannot_exist_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("logs");
        std::fs::write(&file, "a file, not a directory").expect("fixture");
        let log = file.join("llama-server@default.log");
        assert!(open_instance_log(&log).is_err());
        assert_eq!(
            std::fs::read_to_string(&file).expect("still there"),
            "a file, not a directory"
        );
    }

    /// A directory the service cannot write: the error, and the logs of the
    /// runs before untouched.
    #[cfg(unix)]
    #[test]
    fn an_unwritable_log_directory_fails_and_keeps_both_logs() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let logs = dir.path().join("logs");
        std::fs::create_dir(&logs).expect("logs");
        let log = logs.join("llama-server@default.log");
        std::fs::write(&log, "last run\n").expect("log");
        std::fs::write(previous_log(&log), "run before\n").expect("previous");
        std::fs::set_permissions(&logs, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        let probe = std::fs::write(logs.join("probe"), "");
        let result = open_instance_log(&log);
        std::fs::set_permissions(&logs, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        if probe.is_ok() {
            eprintln!("skipped: running as root, the mode does not stop a write");
            return;
        }
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&log).expect("log"), "last run\n");
        assert_eq!(
            std::fs::read_to_string(previous_log(&log)).expect("previous"),
            "run before\n"
        );
        let mut names: Vec<_> = std::fs::read_dir(&logs)
            .expect("list")
            .map(|e| e.expect("entry").file_name())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "llama-server@default.log",
                "llama-server@default.previous.log"
            ]
        );
    }

    #[test]
    fn the_first_run_creates_the_directory_and_an_empty_log() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("tempdir");
        let log = dir.path().join("logs").join("llama-server@default.log");
        let mut file = open_instance_log(&log).expect("opened");
        assert_eq!(std::fs::read(&log).expect("created"), b"");
        assert!(!previous_log(&log).exists());
        file.write_all(b"build: 1234\n").expect("write");
        drop(file);
        assert_eq!(std::fs::read_to_string(&log).expect("log"), "build: 1234\n");
    }

    #[test]
    fn the_last_run_becomes_the_previous_one() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("tempdir");
        let log = dir.path().join("llama-server@default.log");
        std::fs::write(&log, "last run\n").expect("log");
        std::fs::write(previous_log(&log), "run before\n").expect("previous");
        let mut file = open_instance_log(&log).expect("opened");
        file.write_all(b"this run\n").expect("write");
        drop(file);
        assert_eq!(std::fs::read_to_string(&log).expect("log"), "this run\n");
        assert_eq!(
            std::fs::read_to_string(previous_log(&log)).expect("previous"),
            "last run\n"
        );
    }

    /// A run that wrote nothing (a start that failed at once) does not push
    /// the useful previous log out.
    #[test]
    fn an_empty_last_run_keeps_the_previous_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = dir.path().join("llama-server@default.log");
        std::fs::write(&log, "").expect("log");
        std::fs::write(previous_log(&log), "the crash\n").expect("previous");
        drop(open_instance_log(&log).expect("opened"));
        assert_eq!(std::fs::read(&log).expect("log"), b"");
        assert_eq!(
            std::fs::read_to_string(previous_log(&log)).expect("previous"),
            "the crash\n"
        );
    }
}
