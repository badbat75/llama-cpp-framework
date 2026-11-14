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
mod pipe_server;
#[cfg(windows)]
pub mod scm;

/// The manager's own service, registered by the MSI.
pub const MANAGER_SERVICE: &str = "private-ai-manager";

/// The named pipe's security descriptor (SDDL): network logons denied,
/// SYSTEM and Administrators full access, interactive users read and write
/// data only (`pai_core::client::pipe::CLIENT_ACCESS`), never
/// `FILE_CREATE_PIPE_INSTANCE`.
pub const PIPE_SDDL: &str = "D:P(D;;GA;;;NU)(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12008b;;;IU)";

/// `CreateNamedPipeW`'s pipe mode: a byte stream, blocking, remote clients
/// rejected.
pub const PIPE_MODE: u32 = PIPE_REJECT_REMOTE_CLIENTS;

/// `PIPE_REJECT_REMOTE_CLIENTS` (`PIPE_TYPE_BYTE`, `PIPE_READMODE_BYTE` and
/// `PIPE_WAIT` are 0).
const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x0000_0008;

/// The Administrators alias, `S-1-5-32-544`.
pub const SID_ADMINISTRATORS: &str = "S-1-5-32-544";

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
}

impl TokenFacts {
    /// The facts of a token of user `sid` with `groups` (`TokenUser`,
    /// `TokenGroups`).
    pub fn from_groups(sid: &str, groups: &[TokenGroup]) -> TokenFacts {
        let has = |wanted: &str, mask: u32| {
            groups
                .iter()
                .any(|g| g.sid == wanted && g.attributes & mask != 0)
        };
        let interactive = groups.iter().any(|g| {
            g.sid == SID_INTERACTIVE
                && g.attributes & SE_GROUP_ENABLED != 0
                && g.attributes & SE_GROUP_USE_FOR_DENY_ONLY == 0
        });
        TokenFacts {
            sid: sid.to_string(),
            interactive,
            admin_member: has(
                SID_ADMINISTRATORS,
                SE_GROUP_ENABLED | SE_GROUP_USE_FOR_DENY_ONLY,
            ),
        }
    }
}

/// Whether a caller with `facts` may do `action`: control is any interactive
/// user (or an administrator), configure a member of Administrators.
pub fn allowed(facts: &TokenFacts, action: Action) -> bool {
    match action {
        Action::Control => facts.interactive || facts.admin_member,
        Action::Configure => facts.admin_member,
    }
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
    fn is_authorized(&self, caller: &Caller, action: Action) -> bool {
        CONNECTION.with(|c| {
            c.borrow()
                .as_ref()
                .is_some_and(|facts| facts.sid == caller.0 && allowed(facts, action))
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
        start_type: SERVICE_DEMAND_START,
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

/// The core's state of a service from its `SERVICE_STATUS`.
pub fn unit_state(current_state: u32, win32_exit_code: u32, service_exit_code: u32) -> UnitState {
    match current_state {
        SERVICE_STOPPED => match (win32_exit_code, service_exit_code) {
            (0, _) | (ERROR_SERVICE_SPECIFIC_ERROR, 0) => UnitState::Inactive,
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
    let name = match code {
        5 => "ERROR_ACCESS_DENIED",
        1053 => "ERROR_SERVICE_REQUEST_TIMEOUT",
        1058 => "ERROR_SERVICE_DISABLED",
        ERROR_SERVICE_DOES_NOT_EXIST => "ERROR_SERVICE_DOES_NOT_EXIST",
        1064 => "ERROR_EXCEPTION_IN_SERVICE",
        1069 => "ERROR_SERVICE_LOGON_FAILED",
        1072 => "ERROR_SERVICE_MARKED_FOR_DELETE",
        _ => return ServiceError(format!("{SERVICE_PREFIX}{id}: Win32 error {code}")),
    };
    ServiceError(format!("{SERVICE_PREFIX}{id}: {name} ({code})"))
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
        assert_eq!(trustees, ["NU", "SY", "BA", "IU"], "{PIPE_SDDL:?}");
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

    fn facts(interactive: bool, admin_member: bool) -> TokenFacts {
        TokenFacts {
            sid: USER.into(),
            interactive,
            admin_member,
        }
    }

    #[test]
    fn the_well_known_sids() {
        assert_eq!(SID_ADMINISTRATORS, "S-1-5-32-544");
        assert_eq!(SID_INTERACTIVE, "S-1-5-4");
    }

    #[test]
    fn a_token_with_no_groups_is_nobody() {
        assert_eq!(TokenFacts::from_groups(USER, &[]), facts(false, false));
        assert_eq!(
            TokenFacts::from_groups(USER, &common()),
            facts(false, false)
        );
    }

    #[test]
    fn a_network_logon_is_not_interactive() {
        let mut groups = common();
        groups.push(group("S-1-5-2", 0x7));
        assert_eq!(TokenFacts::from_groups(USER, &groups), facts(false, false));
    }

    #[test]
    fn a_standard_user_at_the_console_is_interactive() {
        let mut groups = common();
        groups.push(group(SID_INTERACTIVE, 0x7));
        assert_eq!(TokenFacts::from_groups(USER, &groups), facts(true, false));
    }

    /// A deny-only INTERACTIVE (a restricted token) grants nothing.
    #[test]
    fn a_deny_only_interactive_group_does_not_count() {
        let mut groups = common();
        groups.push(group(SID_INTERACTIVE, SE_GROUP_USE_FOR_DENY_ONLY));
        assert_eq!(TokenFacts::from_groups(USER, &groups), facts(false, false));
    }

    /// The owner of a personal PC, not elevated: UAC's filtered token holds
    /// Administrators deny-only (0x10); still a member.
    #[test]
    fn a_filtered_admin_token_is_an_admin_member() {
        let mut groups = common();
        groups.push(group(SID_INTERACTIVE, 0x7));
        groups.push(group(SID_ADMINISTRATORS, SE_GROUP_USE_FOR_DENY_ONLY));
        assert_eq!(TokenFacts::from_groups(USER, &groups), facts(true, true));
    }

    /// Elevated: Administrators enabled and owner (0xF).
    #[test]
    fn an_elevated_admin_token_is_an_admin_member() {
        let mut groups = common();
        groups.push(group(SID_ADMINISTRATORS, 0xF));
        groups.push(group(SID_INTERACTIVE, 0x7));
        assert_eq!(TokenFacts::from_groups(USER, &groups), facts(true, true));
    }

    /// The SID string is compared whole: a domain group whose SID merely
    /// starts like the alias is not it.
    #[test]
    fn a_look_alike_sid_is_not_administrators() {
        let mut groups = common();
        groups.push(group("S-1-5-32-5440", 0x7));
        groups.push(group("S-1-5-41", 0x7));
        assert_eq!(TokenFacts::from_groups(USER, &groups), facts(false, false));
    }

    #[test]
    fn the_user_sid_is_kept() {
        assert_eq!(TokenFacts::from_groups("S-1-5-18", &[]).sid, "S-1-5-18");
    }

    #[test]
    fn nobody_may_do_anything() {
        for action in [Action::Control, Action::Configure] {
            assert!(!allowed(&facts(false, false), action), "{action:?}");
        }
    }

    #[test]
    fn a_standard_user_controls_but_never_configures() {
        assert!(allowed(&facts(true, false), Action::Control));
        assert!(!allowed(&facts(true, false), Action::Configure));
    }

    #[test]
    fn an_admin_member_controls_and_configures() {
        for interactive in [true, false] {
            let f = facts(interactive, true);
            assert!(allowed(&f, Action::Control), "{f:?}");
            assert!(allowed(&f, Action::Configure), "{f:?}");
        }
    }

    #[test]
    fn no_connection_on_this_thread_is_refused() {
        for action in [Action::Control, Action::Configure] {
            assert!(!TokenAuthority.is_authorized(&Caller(USER.into()), action));
        }
    }

    #[test]
    fn the_connection_s_caller_is_its_sid_and_decides() {
        let (control, configure) = TokenAuthority::with_caller(facts(true, false), |caller| {
            assert_eq!(caller, &Caller(USER.into()));
            (
                TokenAuthority.is_authorized(caller, Action::Control),
                TokenAuthority.is_authorized(caller, Action::Configure),
            )
        });
        assert_eq!((control, configure), (true, false));
    }

    /// Another caller than the connection's (a stale or forged one) is
    /// nobody.
    #[test]
    fn another_caller_than_the_connection_s_is_refused() {
        let other = Caller("S-1-5-21-1-2-3-500".into());
        let refused = TokenAuthority::with_caller(facts(true, true), |_| {
            !TokenAuthority.is_authorized(&other, Action::Control)
                && !TokenAuthority.is_authorized(&Caller(String::new()), Action::Control)
        });
        assert!(refused);
    }

    #[test]
    fn the_connection_is_cleared_when_it_ends() {
        TokenAuthority::with_caller(facts(true, true), |_| {});
        assert!(!TokenAuthority.is_authorized(&Caller(USER.into()), Action::Control));
        let panicked = std::panic::catch_unwind(|| {
            TokenAuthority::with_caller(facts(true, true), |_| panic!("in the call"))
        });
        assert!(panicked.is_err());
        assert!(!TokenAuthority.is_authorized(&Caller(USER.into()), Action::Configure));
    }

    /// Each connection thread sees its own caller only.
    #[test]
    fn connections_on_other_threads_are_apart() {
        TokenAuthority::with_caller(facts(true, true), |caller| {
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
