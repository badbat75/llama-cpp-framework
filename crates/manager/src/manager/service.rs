//! The OS-neutral core of `pai-manager` (running llama-server as a service):
//! the instances, their config files in `paths::engine_config_dir()` and
//! their services, behind two seams. [`ServiceControl`] starts, stops and
//! reports the unit of an instance (systemd's D-Bus API on
//! `llama.cpp@<id>.service` on Linux, the SCM on Windows once written);
//! [`Authority`] decides whether a caller may do an action (polkit `CheckAuthorization`
//! with the caller's bus name as subject on Linux). Reads, the instance list
//! and status need no authorization; everything that changes something does,
//! and a refused caller changes nothing. A guarded call may name a
//! [`Principal`] a trusted caller vouches for (the bff's signed-in user): the
//! manager checks the caller, the name and that account's membership of `pai`
//! itself, and runs the action without asking polkit, whose subject is the
//! caller and not the user.

use crate::server_cfg::ServerConfig;
use pai_core::checks::FETCH_KEYS;
// The apply's digest (`pair_digest`): SHA-256 over the instance's rendered pair.
use sha2::{Digest as _, Sha256};
use std::net::IpAddr;

/// The render pass of a write, as the write sees it: the instances whose pair
/// the fit check refused, each with that check's keyed refusal, in id order. An
/// IO failure of a write is the write's own `Err` (`Manager::render_pass`).
type RenderPass = Result<Vec<(String, pai_core::checks::Refusal)>, ManagerError>;

/// A guarded action, one polkit action id each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Start, stop or restart an instance; read an instance file (it can
    /// hold a secret, `OpencodeApiKey`).
    Control,
    /// Write a config file.
    Configure,
}

impl Action {
    /// The polkit action id (agreed with the user).
    pub fn id(self) -> &'static str {
        match self {
            Action::Control => "org.privateai.manager.control",
            Action::Configure => "org.privateai.manager.configure",
        }
    }
}

/// Who asks: the caller's unique bus name on Linux (`:1.42`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller(pub String);

/// A user name a trusted caller vouches for: the signed-in user a guarded
/// call acts for. Never folded into [`Caller`], which stays the transport's
/// identity; the manager checks the name and the account behind it itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal(pub String);

/// The longest principal name the manager accepts: the bound of the caller
/// that vouches for one, the bff's own `MAX_USER_BYTES`
/// (`crates/bff/src/session.rs`).
const MAX_PRINCIPAL_BYTES: usize = 256;

/// Who asks for a guarded call: the transport's identity, and the principal
/// the caller vouches for. `principal` None is the caller's own call, exactly
/// as a desktop client makes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requester {
    pub caller: Caller,
    pub principal: Option<Principal>,
}

/// What a guarded core method takes: a [`Requester`], or a bare [`Caller`]
/// for a call with no principal. The transport faces send `&Requester`
/// (`Requester { caller, principal: None }` for the caller's own call); the
/// [`Caller`] bound is a request with no principal to send.
pub trait AsRequester {
    fn as_requester(&self) -> Requester;
}

impl AsRequester for Caller {
    fn as_requester(&self) -> Requester {
        Requester {
            caller: self.clone(),
            principal: None,
        }
    }
}

impl AsRequester for Requester {
    fn as_requester(&self) -> Requester {
        self.clone()
    }
}

/// Whether a caller may do an action. A failure of the check itself
/// (polkit unreachable) answers false.
pub trait Authority {
    fn is_authorized(&self, caller: &Caller, action: Action) -> bool;
    /// The same question asked without any interaction: one that would need
    /// a prompt (a polkit challenge) answers false.
    fn is_authorized_quietly(&self, caller: &Caller, action: Action) -> bool;
    /// Whether `caller` is the bff's own account (`private-ai-bff`), the
    /// only one [`Manager::authenticate`] answers. A failed lookup is false.
    fn is_bff(&self, caller: &Caller) -> bool;
}

/// A failure of the password check itself (PAM or `LogonUser` unusable), its
/// own message, which never holds the password. A wrong password or an
/// unknown user is no failure: `Ok(false)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordError(pub String);

/// The OS accounts: checks a user's password (PAM service `private-ai` on
/// Linux, `LogonUser` on Windows) and whether the user is an administrator.
/// A seam, so tests check no real password.
pub trait PasswordChecker {
    /// Whether `password` is `user`'s; an unknown user is `Ok(false)`.
    fn check(&self, user: &str, password: &str) -> Result<bool, PasswordError>;
    /// Whether `user` may configure: a member of the group `pai`, which the
    /// packages create (Linux: [`in_pai_group`] over the user's group names;
    /// Windows: the local group `pai`, looked up by name since its SID
    /// differs per machine).
    fn is_admin(&self, user: &str) -> bool;
}

/// Whether a user in `groups` (their group names) may configure on Linux:
/// a member of `pai`.
pub fn in_pai_group(groups: &[String]) -> bool {
    groups.iter().any(|g| g == ADMIN_GROUP)
}

/// The group whose members may configure, on Linux and Windows.
pub(crate) const ADMIN_GROUP: &str = "pai";

/// The system's accounts: PAM and the user's groups on Linux
/// (`accounts.rs`), `LogonUserW` and the local groups on Windows
/// (`windows/logon.rs`). Checked in a real run, never by tests.
pub struct SystemPasswords;

impl PasswordChecker for SystemPasswords {
    #[cfg(target_os = "linux")]
    fn check(&self, user: &str, password: &str) -> Result<bool, PasswordError> {
        super::accounts::check_password(user, password)
    }
    #[cfg(target_os = "linux")]
    fn is_admin(&self, user: &str) -> bool {
        in_pai_group(&super::accounts::group_names(user))
    }

    #[cfg(windows)]
    fn check(&self, user: &str, password: &str) -> Result<bool, PasswordError> {
        super::windows::logon::check_password(user, password)
    }
    #[cfg(windows)]
    fn is_admin(&self, user: &str) -> bool {
        super::windows::logon::in_local_group(user, ADMIN_GROUP)
    }

    /// No sign-in where the manager does not run (macOS).
    #[cfg(not(any(target_os = "linux", windows)))]
    fn check(&self, _user: &str, _password: &str) -> Result<bool, PasswordError> {
        Err(PasswordError("no password check on this OS".to_string()))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    fn is_admin(&self, _user: &str) -> bool {
        false
    }
}

/// The state of an instance's unit, systemd's `ActiveState` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitState {
    Active,
    Inactive,
    Failed,
    Activating,
    Deactivating,
}

/// A failure of the service manager, its own message (systemd's D-Bus error
/// or, once written, the SCM's), not shown as user text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceError(pub String);

/// The service manager, an instance addressed by its id.
pub trait ServiceControl {
    fn start(&self, id: &str) -> Result<(), ServiceError>;
    fn stop(&self, id: &str) -> Result<(), ServiceError>;
    fn restart(&self, id: &str) -> Result<(), ServiceError>;
    fn status(&self, id: &str) -> Result<UnitState, ServiceError>;
    /// Whether the instance's unit starts when the system starts.
    fn boot_enabled(&self, id: &str) -> Result<bool, ServiceError>;
    /// Make the instance's unit start when the system starts, or not.
    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ServiceError>;
    /// Take the instance's unit away: the `wants/` link and the unit itself, so
    /// a later instance of the same id starts clean. An id with no unit is
    /// success (the step is idempotent, the record of the instances is written
    /// after it).
    fn remove(&self, _id: &str) -> Result<(), ServiceError> {
        Err(ServiceError("this service manager has no Remove".into()))
    }
}

/// Why the manager refused or failed an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerError {
    /// The caller is not allowed this action; nothing was done.
    NotAuthorized,
    /// Not a valid instance id (`paths::valid_instance_id`).
    InvalidId,
    /// A valid id that is not a section of the record (`instances.ini`).
    NoInstance,
    /// Not one of the files the manager reads and writes ([`config_file`]).
    BadFileName,
    /// This host and port collide with the instance of this id.
    AddressInUse(String),
    /// The engine has no server profile of this name, so no
    /// `server@<name>.ini` to point an instance at.
    NoServerProfile(String),
    /// The pool has no instance profile of this name (`ARCHITECTURE.md`
    /// "Instance profiles").
    NoInstanceProfile(String),
    /// The server profile of this name is the one the instance of this id runs.
    ServerProfileInUse(String, String),
    /// A server profile with this name is already there: a create writes over
    /// nothing.
    ServerProfileExists(String),
    /// An instance profile with this name is already there: a create writes
    /// over nothing (`ARCHITECTURE.md` "Instance profiles").
    InstanceProfileExists(String),
    /// The instance profile of this name is the one the instance of this id
    /// runs.
    InstanceProfileInUse(String, String),
    /// This host name of the file being written does not resolve.
    UnresolvableHost(String),
    /// The service manager failed.
    Service(ServiceError),
    /// Reading or writing the file failed.
    Io(std::io::ErrorKind),
    /// The contents fail a check of `pai_core::checks`; nothing was written.
    Invalid(pai_core::checks::Refusal),
    /// A compare-and-swap write found the file no longer holding the text it
    /// was given to expect, or not there at all; nothing was written.
    Changed,
}

/// The error name and message `e` replies with, on every transport
/// (`org.privateai.Manager1.Error.<Kind>`). The message is protocol data, not
/// user text: the detail when the error has one (an instance id, a host, the
/// service manager's own message, the I/O error kind, a refusal's key and
/// values), else the kind's own name. The client shows its own text.
pub fn error_reply(e: &ManagerError) -> (String, String) {
    let (kind, message) = match e {
        ManagerError::NotAuthorized => ("NotAuthorized", "NotAuthorized".to_string()),
        ManagerError::InvalidId => ("InvalidId", "InvalidId".to_string()),
        ManagerError::NoInstance => ("NoInstance", "NoInstance".to_string()),
        ManagerError::BadFileName => ("BadFileName", "BadFileName".to_string()),
        ManagerError::AddressInUse(id) => ("AddressInUse", id.clone()),
        ManagerError::NoServerProfile(name) => ("NoServerProfile", name.clone()),
        ManagerError::NoInstanceProfile(name) => ("NoInstanceProfile", name.clone()),
        // The profile and the instance, split by the client at the separator
        // `Refusal::encode` uses.
        ManagerError::ServerProfileInUse(name, id) => {
            ("ServerProfileInUse", format!("{name}\u{1f}{id}"))
        }
        ManagerError::ServerProfileExists(name) => ("ServerProfileExists", name.clone()),
        // The instance profile refusals: the in-use one carries the profile
        // and the instance split at the separator, as the server profile's
        // does.
        ManagerError::InstanceProfileExists(name) => ("InstanceProfileExists", name.clone()),
        ManagerError::InstanceProfileInUse(name, id) => {
            ("InstanceProfileInUse", format!("{name}\u{1f}{id}"))
        }
        ManagerError::UnresolvableHost(host) => ("UnresolvableHost", host.clone()),
        ManagerError::Service(e) => ("Service", e.0.clone()),
        ManagerError::Io(kind) => ("Io", kind.to_string()),
        // `Refusal::encode`'s string: the client decodes it.
        ManagerError::Invalid(refusal) => ("Invalid", refusal.encode()),
        ManagerError::Changed => ("Changed", "Changed".to_string()),
    };
    (format!("org.privateai.Manager1.Error.{kind}"), message)
}

/// The error's own kind name, the same word `error_reply` puts in the
/// transport's error name, and the one an action record writes (quoted) for a
/// failed guarded call.
fn error_kind(e: &ManagerError) -> &'static str {
    match e {
        ManagerError::NotAuthorized => "NotAuthorized",
        ManagerError::InvalidId => "InvalidId",
        ManagerError::NoInstance => "NoInstance",
        ManagerError::BadFileName => "BadFileName",
        ManagerError::AddressInUse(_) => "AddressInUse",
        ManagerError::NoServerProfile(_) => "NoServerProfile",
        ManagerError::NoInstanceProfile(_) => "NoInstanceProfile",
        ManagerError::ServerProfileInUse(_, _) => "ServerProfileInUse",
        ManagerError::ServerProfileExists(_) => "ServerProfileExists",
        ManagerError::InstanceProfileExists(_) => "InstanceProfileExists",
        ManagerError::InstanceProfileInUse(_, _) => "InstanceProfileInUse",
        ManagerError::UnresolvableHost(_) => "UnresolvableHost",
        ManagerError::Service(_) => "Service",
        ManagerError::Io(_) => "Io",
        ManagerError::Invalid(_) => "Invalid",
        ManagerError::Changed => "Changed",
    }
}

/// The lowercase name `Status` answers for a unit state.
pub fn state_name(state: UnitState) -> &'static str {
    match state {
        UnitState::Active => "active",
        UnitState::Inactive => "inactive",
        UnitState::Failed => "failed",
        UnitState::Activating => "activating",
        UnitState::Deactivating => "deactivating",
    }
}

/// The log of who acted: the manager renders one record per guarded call
/// (start, stop, restart, read_file, write_file, set_boot_enabled), done or
/// refused, and hands the whole line to this seam. The record names the
/// action, the target (instance id or file name), the caller, the principal
/// (`-` when none) and the outcome, with every name written with Rust's
/// `{:?}` quoting, so nothing a name carries can forge a second record.
/// Never a password, never file contents. The format is literal text, not
/// a strings-file text: administrators and scripts read the records, like
/// the comments written into the served files.
pub trait ActionLog {
    /// Write one whole record line.
    fn record(&self, line: &str);
}

/// A shared sink is a log: the entry hands the same sink to the manager and
/// to the service glue (an `Arc`, so a thread can own a handle to it), and
/// both record through it.
impl<T: ActionLog + ?Sized> ActionLog for std::sync::Arc<T> {
    fn record(&self, line: &str) {
        (**self).record(line)
    }
}

/// The log [`Manager::new`] starts with: records nothing. The service entry
/// installs the OS's writer (stderr on Linux, the appended file on Windows)
/// with [`Manager::with_log`]; the SCM glue starts with it too
/// (`ScmService::new`), its writer set by `ScmService::with_log`.
pub struct NoActionLog;

impl ActionLog for NoActionLog {
    fn record(&self, _line: &str) {}
}

/// A file of the system config directory the manager serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigFile {
    /// `server@<profile>.ini`, the profile name valid.
    ServerProfile(String),
    /// `models.ini`.
    Models,
    /// `models.ini.disabled`.
    ModelsDisabled,
}

/// The file `name` names, a bare file name; None for any other name (a
/// path, `server.ini`, `settings.ini`, an invalid instance id).
pub fn config_file(name: &str) -> Option<ConfigFile> {
    match name {
        "models.ini" => Some(ConfigFile::Models),
        "models.ini.disabled" => Some(ConfigFile::ModelsDisabled),
        _ => {
            let id = name.strip_prefix("server@")?.strip_suffix(".ini")?;
            crate::paths::valid_instance_id(id).then(|| ConfigFile::ServerProfile(id.to_string()))
        }
    }
}

/// `assignments.ini`: which model profiles each instance is given. The model
/// data model retired it (`ARCHITECTURE.md`, "Instance profiles"): the entry
/// sections of an instance profile are the model list. A client reads it and
/// never writes it, and the manager's start removes it, with
/// [`MODELS_DISABLED_FILE`], once every instance of the record names an
/// instance profile.
pub const ASSIGNMENTS_FILE: &str = "assignments.ini";

/// `models.ini.disabled`: the switched-off model profiles. The model data model
/// retired it: a profile switched off sits in `models.ini` with no instance
/// profile naming it. A client reads it and never writes it, and the manager's
/// start removes it with [`ASSIGNMENTS_FILE`].
pub const MODELS_DISABLED_FILE: &str = "models.ini.disabled";

/// `instances.ini`: what an instance is, one section per instance with its
/// `InstanceProfile` and the `Hostname` and `Port` it listens on. The manager
/// owns it, so a client reads it and never writes it.
pub const INSTANCES_FILE: &str = "instances.ini";

/// `instance-profiles.ini`: the pool of instance profiles, one `[<p>]` section
/// each naming the server profile it runs, and one `[<p>/<m>]` section per
/// model profile the profile is given (`ARCHITECTURE.md`, "Instance profiles",
/// the placement and memory amendment). The manager owns it, so a client reads
/// it and never writes it.
pub const INSTANCE_PROFILES_FILE: &str = "instance-profiles.ini";

/// The suffix of the backup of a file the model data model migration moved,
/// carrying the release that had it as it stood: `models.ini.2.99.0`.
const BACKUP_SUFFIX: &str = ".2.99.0";

/// The engine this manager serves, the one every instance and every server
/// profile names: the only value `CreateServerProfile` and `DeleteServerProfile`
/// accept.
const ENGINE: &str = "llama.cpp";

/// `default`: the instance every install ships, the one the configurator
/// controls and the web UI calls the node's own. `RemoveInstance` refuses it;
/// every other id is the administrator's to create and remove.
const PROTECTED_INSTANCE: &str = "default";

/// The port a new instance starts looking for: the node's first instance port,
/// the one a node holding nothing from here up is given. Every port of the
/// node's bindings belongs to an instance it runs or to the web service, so a
/// create walks up from here until it finds one nothing holds
/// ([`proposed_port`]).
const FIRST_PROPOSED_PORT: u16 = 8581;

/// The last port a new instance is looked for at: the highest a socket carries.
/// The walk of [`proposed_port`] runs from [`FIRST_PROPOSED_PORT`] to here.
const LAST_PROPOSED_PORT: u16 = u16::MAX;

/// The `models_max` of the pool checks: the `load-on-startup` count is judged
/// per instance, against that instance's own `ModelsMax` and its own render,
/// so the pool has no cap of its own.
const NO_MODELS_MAX: Option<i32> = Some(i32::MAX);

/// The group an instance file belongs to on Unix: the instances run as it
/// (the sysusers.d entry of the Linux package), so they read their file
/// while other users do not.
pub const INSTANCE_GROUP: &str = "private-ai";

/// The mode a served file is written with on Unix: 0640 for an instance
/// file (it can hold `OpencodeApiKey`), 0644 for the model profile files.
pub fn file_mode(file: &ConfigFile) -> u32 {
    match file {
        ConfigFile::ServerProfile(_) => 0o640,
        ConfigFile::Models | ConfigFile::ModelsDisabled => 0o644,
    }
}

/// Gives a file the manager wrote its group, the owner unchanged (chown(2)
/// on Unix). A seam, so tests change no group and need no root.
pub trait FileGroup {
    /// Err `NotFound` when no group has this name.
    fn set_group(&self, path: &std::path::Path, group: &str) -> std::io::Result<()>;
}

/// The system's groups: the group looked up by name, then chown(2).
pub struct SystemGroup;

impl FileGroup for SystemGroup {
    #[cfg(unix)]
    fn set_group(&self, path: &std::path::Path, group: &str) -> std::io::Result<()> {
        // /etc/group's lines are `name:password:gid:members`.
        let groups = std::fs::read_to_string("/etc/group")?;
        let gid = groups
            .lines()
            .map(|line| line.split(':').collect::<Vec<_>>())
            .find(|fields| fields.len() >= 3 && fields[0] == group)
            .and_then(|fields| fields[2].parse::<u32>().ok())
            .ok_or(std::io::ErrorKind::NotFound)?;
        std::os::unix::fs::chown(path, None, Some(gid))
    }

    #[cfg(not(unix))]
    fn set_group(&self, _path: &std::path::Path, _group: &str) -> std::io::Result<()> {
        Ok(())
    }
}

/// The GPU memory a model profile's estimate is measured against: the node's
/// cards, each with its total. `None` when there is nothing to measure against
/// (no card, or a probe that failed), which skips the check (ARCHITECTURE.md
/// (b) 2). A seam, so a test answers any pool and runs no engine.
pub trait GpuTotals {
    fn gpus(&self) -> Option<Vec<pai_core::devices::Gpu>>;
    /// The node's compute devices as the probe's own lines, with the moment it
    /// ran: what a model profile distributes a model over, the CPU pseudo-device
    /// left out, the CUDA and Vulkan views of one card both kept, in the probe's
    /// order. `probed_at` `None` with no list when no probe answered, which is
    /// nothing to distribute and never an error.
    ///
    /// The reply also carries the host's inventory (R1, ARCHITECTURE.md (1) "The
    /// node inventory"): `ram_mib` and `cpu_threads`, read from the machine by
    /// the seam, the manager never reading it itself. A test hands the pair to
    /// its fake and so sees only figures it wrote: no test of the manager reads
    /// `/proc/meminfo`, `GlobalMemoryStatusEx` or the CPU count.
    fn devices(&self) -> pai_core::client::NodeDevices;
}

/// The node's GPU totals answered as nothing: no pool, so the memory check is
/// skipped, and no device list, so a page has nothing to distribute. The
/// default of [`Manager::new`], so a manager that wires no seam measures
/// nothing.
pub struct NoGpus;

impl GpuTotals for NoGpus {
    fn gpus(&self) -> Option<Vec<pai_core::devices::Gpu>> {
        None
    }
    fn devices(&self) -> pai_core::client::NodeDevices {
        // The host's inventory is read, not guessed, so a seam that reads
        // nothing answers 0 for both: no RAM to show, no thread to cap at.
        pai_core::client::NodeDevices {
            probed_at: None,
            ram_mib: 0,
            cpu_threads: 0,
            devices: Vec::new(),
        }
    }
}

/// The node's own facts the node-info read answers (AN3, `ARCHITECTURE.md`
/// "About and node info"): the operating system the host runs, the kernel's GPU
/// rows, and what the node's Vulkan read answered. The three readings are the
/// host's, so the manager itself opens no `os-release`, walks no `/sys` and
/// loads no Vulkan driver: a test hands its fake a fixture tree and a scripted
/// child, and a manager wired with no seam answers a hole and spawns nothing.
pub trait NodeFacts {
    /// The operating system: os-release's `NAME` and `VERSION_ID`, and the
    /// kernel's release (`uname(2)`), each `None` when the host answered none.
    fn os(&self) -> pai_core::host::OsInfo;
    /// The kernel's display devices, in bus id order, at most
    /// `pai_core::host::MAX_GPU_ROWS` of them.
    fn gpus(&self) -> Vec<pai_core::host::GpuDriver>;
    /// The node's Vulkan drivers as the child answered them, the answer kept
    /// for [`VULKAN_CACHE_SECONDS`].
    fn vulkan(&self) -> pai_core::vulkan::Probe;
}

/// The node's facts answered as nothing: no OS, no kernel row, and a Vulkan
/// read that gave nothing to trust. The default of [`Manager::new`], so a
/// manager that wires no seam spawns no child and answers a hole.
pub struct NoNodeFacts;

impl NodeFacts for NoNodeFacts {
    fn os(&self) -> pai_core::host::OsInfo {
        pai_core::host::OsInfo {
            name: None,
            version: None,
            kernel: None,
        }
    }
    fn gpus(&self) -> Vec<pai_core::host::GpuDriver> {
        Vec::new()
    }
    fn vulkan(&self) -> pai_core::vulkan::Probe {
        pai_core::vulkan::Probe {
            status: pai_core::vulkan::Status::Failed,
            devices: Vec::new(),
        }
    }
}

/// How long the engine probe of `--list-devices` is given before it is
/// killed (ARCHITECTURE.md (b) 2, "The GPUs").
pub const GPU_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// One probe, as the cache keeps it: the modification time the exe had when it
/// ran, the Unix second it ran, and the device lines it printed.
type ProbeCache = (
    Option<std::time::SystemTime>,
    u64,
    Vec<pai_core::devices::DeviceOption>,
);

/// One probe's answer: the Unix second it ran and the device lines it printed.
/// `None` when it answered nothing, which is a probe that failed: a spawn that
/// could not start, a probe killed at the bound, a block that named no device.
type ProbeAnswer = Option<(u64, Vec<pai_core::devices::DeviceOption>)>;

/// The node's GPU totals from the engine's own probe: `--list-devices` run
/// against `exe` as the manager, killed after `timeout`, the block read once and
/// kept as the engine printed it, and a probe that failed not cached
/// (ARCHITECTURE.md (b) 2: a probe that timed out, spawned badly or named no
/// device is answered as nothing, which skips the memory check and leaves the
/// device page empty; a node that named only the CPU pseudo-device is answered
/// with its time and an empty list, and is not cached either, "As built" (c)).
pub struct ProbedGpus {
    exe: std::path::PathBuf,
    timeout: std::time::Duration,
    /// The device lines the last probe parsed, with the modification time the
    /// exe had then and the second it ran: an engine upgrade replaces the file,
    /// and the next call probes again. A probe that named no device, and one
    /// that named only the CPU pseudo-device ("As built" (c)), leave nothing
    /// here: the node is read again at the next call, so the fix of the engine
    /// or of a GPU backend is picked up.
    cached: std::sync::Mutex<Option<ProbeCache>>,
    /// How many probes ran, and the answer of the last of them, failure
    /// included: a reader that queued behind a probe takes that probe's answer
    /// ("As built" (d)), so no reader pays a second bound and queued readers
    /// never run one probe each.
    last: std::sync::Mutex<(u64, ProbeAnswer)>,
    /// Held across the probe, so the reads that arrive together run one of it:
    /// a cold probe starts a `llama-server` on the node, and two at once
    /// contend for the cards they are there to read.
    probing: std::sync::Mutex<()>,
}

impl ProbedGpus {
    /// The probe of `exe --list-devices`, killed after `timeout`. A test
    /// names a script and a short bound.
    pub fn new(exe: impl Into<std::path::PathBuf>, timeout: std::time::Duration) -> Self {
        Self {
            exe: exe.into(),
            timeout,
            cached: std::sync::Mutex::new(None),
            last: std::sync::Mutex::new((0, None)),
            probing: std::sync::Mutex::new(()),
        }
    }

    /// The probe of a real node: the exe the launch uses, under
    /// [`GPU_PROBE_TIMEOUT`].
    pub fn for_node() -> Self {
        Self::new(
            crate::paths::llama_server_exe().unwrap_or_default(),
            GPU_PROBE_TIMEOUT,
        )
    }

    /// The cached device lines, when there is a probe of this very `exe` to
    /// answer with: a poisoned lock is taken over, every write leaves the
    /// entry whole.
    fn cached(
        &self,
        mtime: Option<std::time::SystemTime>,
    ) -> Option<(u64, Vec<pai_core::devices::DeviceOption>)> {
        let (at, probed, devices) = self
            .cached
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()?;
        (at == mtime).then_some((probed, devices))
    }

    /// How many probes ran so far: read before taking [`Self::probing`] and
    /// compared, once it is held, with the counter of that moment. A bigger
    /// counter means a probe ran while the caller waited, and its answer is the
    /// caller's.
    fn attempts(&self) -> u64 {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).0
    }

    /// The device lines of a probe, with the second it ran: the cache when the
    /// exe has not changed since it, else a probe run under [`Self::probing`],
    /// the cache read again first so the readers that queued behind it take its
    /// answer, and the answer of the probe that ran while the caller waited
    /// taken as it is, failure included ("As built" (d): a reader never waits
    /// past one bound and never runs a probe of its own behind a failed one).
    /// `None` when no probe answered: a spawn that failed, a probe killed at the
    /// bound, and a block that named no device, none of them cached.
    fn probe(&self) -> ProbeAnswer {
        let mtime = std::fs::metadata(&self.exe).and_then(|f| f.modified()).ok();
        if let Some(answer) = self.cached(mtime) {
            return Some(answer);
        }
        let seen = self.attempts();
        let _serial = self.probing.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(answer) = self.cached(mtime) {
            return Some(answer);
        }
        let (attempt, answered) = self.last.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if attempt > seen {
            return answered;
        }
        // The block is read out of stdout and stderr together (`proc`'s rule
        // for the llama-server probes), and the exit status is not asked: an
        // engine whose backend fails late exits non-zero with a block worth
        // reading. A block that names no device is a probe that failed.
        let parsed =
            pai_core::proc::run_hidden_probe_within(&self.exe, ["--list-devices"], self.timeout)
                .map(|out| pai_core::devices::parse(&pai_core::proc::combined_output(&out)))
                .unwrap_or_default();
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or_default();
        let answer = (!parsed.is_empty()).then_some((at, parsed));
        // A probe is kept only when it read a card of its own: a CPU-only node
        // answers its empty list with its time and is read again next time
        // ("As built" (c), the memory check's retry of 10b-3 restored), while a
        // partial failure (one backend down, one up) is kept until the exe
        // changes, because that node was read.
        if let Some((at, parsed)) = answer.clone() {
            if parsed.iter().any(|device| !device.is_cpu()) {
                *self.cached.lock().unwrap_or_else(|e| e.into_inner()) = Some((mtime, at, parsed));
            }
        }
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = (attempt + 1, answer.clone());
        answer
    }
}

/// The backend a device id stands for: the id with the digits the engine adds
/// to number the cards of one backend taken off its end, its case kept as
/// `device` names it (`ROCm0` is `ROCm`, `Vulkan1` is `Vulkan`).
fn backend_of_id(id: &str) -> String {
    id.trim_end_matches(|c: char| c.is_ascii_digit())
        .to_string()
}

impl GpuTotals for ProbedGpus {
    fn gpus(&self) -> Option<Vec<pai_core::devices::Gpu>> {
        let (_, parsed) = self.probe()?;
        // The pool is a reduction of the probe's lines: the CUDA and Vulkan
        // views of one card folded into it, the integrated graphics and the
        // CPU left out. A node the probe read with no card of its own has
        // nothing to measure against.
        let pool = pai_core::devices::gpu_pool(&parsed);
        (!pool.is_empty()).then_some(pool)
    }
    fn devices(&self) -> pai_core::client::NodeDevices {
        // The host's inventory is read here, once per call, whatever the probe
        // did: the RAM and the CPUs are the node's, not the engine's, so a
        // probe that died leaves them answered (R1).
        let (ram_mib, cpu_threads) = (pai_core::host::ram_mib(), pai_core::host::cpu_threads());
        let Some((probed_at, parsed)) = self.probe() else {
            return pai_core::client::NodeDevices {
                probed_at: None,
                ram_mib,
                cpu_threads,
                devices: Vec::new(),
            };
        };
        pai_core::client::NodeDevices {
            probed_at: Some(probed_at),
            ram_mib,
            cpu_threads,
            devices: parsed
                .iter()
                .filter(|device| !device.is_cpu())
                .map(|device| pai_core::client::DeviceInfo {
                    id: device.id.clone(),
                    name: device.name.clone(),
                    backend: backend_of_id(&device.id),
                    total_mib: device.total_mib,
                    free_mib: device.free_mib,
                })
                .collect(),
        }
    }
}

/// How long the Vulkan child is given before it is killed (ARCHITECTURE.md
/// "About and node info", AN2: the child's 8 s, under the bff's 10 s for the
/// whole node read).
pub const VULKAN_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// How long one Vulkan answer is kept, success, `none` and `failed` alike: a
/// polled page spawns at most one child over the window, and a driver upgrade
/// shows within it (ARCHITECTURE.md "About and node info", "Cost and cache").
pub const VULKAN_CACHE_SECONDS: u64 = 600;

/// The subcommand the child is asked for: the hidden one of this same
/// executable, `pai-manager vulkan-probe`.
const VULKAN_PROBE_COMMAND: &str = "vulkan-probe";

/// The node's Vulkan drivers, read by a child of the manager's own executable
/// (`pai-manager vulkan-probe`), spawned under `run_hidden_probe_within`, its
/// document parsed by `pai_core::vulkan` and its answer kept for
/// [`VULKAN_CACHE_SECONDS`]. A child that hangs or dies is the child's, not the
/// manager's: the bound frees the caller and the answer is `Failed`. Callers
/// that arrive together share one run (the serial-probe pattern of
/// [`ProbedGpus`]).
pub struct VulkanProbe {
    exe: std::path::PathBuf,
    timeout: std::time::Duration,
    /// The answer of the last child, with the second `probe` was called at then:
    /// `None` while no child ran yet. The answer is kept whatever it is,
    /// `Failed` included: a hung ICD costs one child per window, not one per
    /// poll.
    cached: std::sync::Mutex<Option<(u64, pai_core::vulkan::Probe)>>,
    /// Held across the child, so the calls that arrive together run one of it:
    /// the child opens the loader and every ICD of the node, and two at once
    /// contend for the cards they are there to read.
    probing: std::sync::Mutex<()>,
}

impl VulkanProbe {
    /// A probe of `exe vulkan-probe`, killed after `timeout`: a test names a
    /// script and a short bound.
    pub fn new(exe: impl Into<std::path::PathBuf>, timeout: std::time::Duration) -> Self {
        Self {
            exe: exe.into(),
            timeout,
            cached: std::sync::Mutex::new(None),
            probing: std::sync::Mutex::new(()),
        }
    }

    /// The probe of a real node: this executable, under [`VULKAN_PROBE_TIMEOUT`].
    pub fn for_node() -> Self {
        Self::new(
            // The child is this same binary asked for its Vulkan drivers, so a
            // node carries its Vulkan with its manager and nothing is installed
            // beside it. A node where the path is unknown spawns no child, and
            // the read is `Failed` as any child that answers nothing.
            std::env::current_exe().unwrap_or_default(),
            VULKAN_PROBE_TIMEOUT,
        )
    }

    /// The node's Vulkan drivers as the child answered them: the cached answer
    /// when it is younger than [`VULKAN_CACHE_SECONDS`] of `now`, else one
    /// child run. `now` is the caller's clock (the unit `probed_at` uses), so a
    /// test moves the moment and runs no child.
    pub fn probe(&self, now: u64) -> pai_core::vulkan::Probe {
        // The window read off the serial lock first: a page polled inside it
        // takes the answer and never reaches a child.
        if let Some((ran_at, answer)) = self.kept() {
            if now.saturating_sub(ran_at) < VULKAN_CACHE_SECONDS {
                return answer;
            }
        }
        let _serial = self.probing.lock().unwrap_or_else(|e| e.into_inner());
        // The window read again with the lock held: the child that ran while
        // this caller waited for the lock is the caller's answer, one child for
        // every caller that queued behind it.
        if let Some((ran_at, answer)) = self.kept() {
            if now.saturating_sub(ran_at) < VULKAN_CACHE_SECONDS {
                return answer;
            }
        }
        // One run of the child: its document, read under the bound and capped by
        // `proc`, parsed by `pai_core::vulkan`. A child that cannot start, is
        // killed at the bound, dies on an ICD or prints text that is not the
        // document is `Failed`: the node's Vulkan is a hole in the page, never
        // an error the caller has to answer. The exit status is not asked: a
        // child that dies prints nothing, and an empty text is no document.
        let answer = match pai_core::proc::run_hidden_probe_within(
            &self.exe,
            [VULKAN_PROBE_COMMAND],
            self.timeout,
        ) {
            // The document is stdout alone: the child prints nothing else, and a
            // loader that logs does it on stderr, which is not part of it.
            Some(output) => pai_core::vulkan::parse(&String::from_utf8_lossy(&output.stdout)),
            None => pai_core::vulkan::Probe {
                status: pai_core::vulkan::Status::Failed,
                devices: Vec::new(),
            },
        };
        *self.cached.lock().unwrap_or_else(|e| e.into_inner()) = Some((now, answer.clone()));
        answer
    }

    /// The kept answer, whole: a poisoned lock is taken over, every write
    /// leaves the entry whole.
    fn kept(&self) -> Option<(u64, pai_core::vulkan::Probe)> {
        self.cached
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

/// The node's own facts, read from the host the manager runs on (AN3):
/// `pai_core::host` reads the operating system and the kernel's GPU rows, and
/// the Vulkan answer comes from the cached child of [`VulkanProbe`], so a page
/// polled inside the cache window spawns no child. A fact the host does not
/// answer is `None` and a child that answers nothing is `Failed`: the read
/// stays a hole in the page, never an error the caller has to answer.
pub struct HostNodeFacts {
    vulkan: VulkanProbe,
}

impl HostNodeFacts {
    /// The facts of this node: the Vulkan child is this executable, asked for
    /// its drivers under [`VULKAN_PROBE_TIMEOUT`].
    pub fn for_node() -> Self {
        Self {
            vulkan: VulkanProbe::for_node(),
        }
    }
}

impl NodeFacts for HostNodeFacts {
    fn os(&self) -> pai_core::host::OsInfo {
        // `/` is the real host: the root the fixture trees stand in place of.
        pai_core::host::os(std::path::Path::new("/"))
    }
    fn gpus(&self) -> Vec<pai_core::host::GpuDriver> {
        pai_core::host::gpu_drivers(std::path::Path::new("/"))
    }
    fn vulkan(&self) -> pai_core::vulkan::Probe {
        // The caller's clock, the same Unix second `probed_at` uses.
        self.vulkan.probe(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_secs())
                .unwrap_or_default(),
        )
    }
}

/// Replace `path` with `contents` atomically (a temp file beside it, synced,
/// then renamed over it). On Unix the temp file has `file_mode(file)` before
/// any byte is written and, for an instance file, its group set to
/// [`INSTANCE_GROUP`] through `group` before the rename. Any failure leaves
/// the old file as it was and no temp file.
pub fn write_served_file(
    path: &std::path::Path,
    file: &ConfigFile,
    contents: &str,
    group: &dyn FileGroup,
) -> std::io::Result<()> {
    let tmp = path.with_extension(
        path.extension()
            .map(|e| format!("{}.tmp", e.to_string_lossy()))
            .unwrap_or_else(|| "tmp".to_string()),
    );
    let write = || -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            options.mode(file_mode(file));
            let out = options.open(&tmp)?;
            // The umask can narrow the mode `open` gave, and a temp file
            // left over keeps its own: set it whole.
            out.set_permissions(std::fs::Permissions::from_mode(file_mode(file)))?;
            write_synced(out, contents)?;
            if let ConfigFile::ServerProfile(_) = file {
                group.set_group(&tmp, INSTANCE_GROUP)?;
            }
        }
        #[cfg(not(unix))]
        {
            let _ = (file, group);
            write_synced(options.open(&tmp)?, contents)?;
        }
        std::fs::rename(&tmp, path)
    };
    write().inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Write `contents` to `out` and flush them to the disk before the rename
/// (`pai_core::ini::atomic_write` says why).
fn write_synced(mut out: std::fs::File, contents: &str) -> std::io::Result<()> {
    std::io::Write::write_all(&mut out, contents.as_bytes())?;
    out.sync_all()
}

/// A host name the resolver could not turn into addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveError(pub String);

/// Turns a host name into its IP addresses. The real one is std's
/// `ToSocketAddrs`; tests use a fake and never touch DNS or /etc/hosts.
pub trait Resolver {
    fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, ResolveError>;
}

/// The system's resolver, std's `ToSocketAddrs`: every address of `host`,
/// each once, in the order the system gave them.
pub struct SystemResolver;

impl Resolver for SystemResolver {
    fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, ResolveError> {
        use std::net::ToSocketAddrs;
        let unresolved = || ResolveError(host.to_string());
        let mut ips: Vec<IpAddr> = Vec::new();
        for addr in (host, 0).to_socket_addrs().map_err(|_| unresolved())? {
            if !ips.contains(&addr.ip()) {
                ips.push(addr.ip());
            }
        }
        if ips.is_empty() {
            return Err(unresolved());
        }
        Ok(ips)
    }
}

/// What an instance binds: its port and the addresses of its `Hostname`
/// list, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub port: i32,
    pub ips: Vec<IpAddr>,
}

/// The binding of `cfg`, defaults applied (`port_or_default`, the hosts of
/// `hostname_or_default`): a literal IP is itself, without asking the
/// resolver; a name is resolved. Err names the first host that does not
/// resolve.
pub fn binding(cfg: &ServerConfig, resolver: &impl Resolver) -> Result<Binding, String> {
    let mut ips = Vec::new();
    for host in crate::net_ifaces::parse_hosts(&cfg.hostname_or_default()) {
        match host.parse::<IpAddr>() {
            Ok(ip) => ips.push(ip),
            Err(_) => ips.extend(resolver.resolve(&host).map_err(|_| host.clone())?),
        }
    }
    Ok(Binding {
        port: cfg.port_or_default(),
        ips,
    })
}

/// Whether two bindings collide: the same port and a common address, an
/// unspecified address (`0.0.0.0` or `::`) colliding with every address,
/// IPv4 or IPv6, on that port.
pub fn addresses_collide(a: &Binding, b: &Binding) -> bool {
    let wildcard = |b: &Binding| b.ips.iter().any(IpAddr::is_unspecified);
    a.port == b.port && (wildcard(a) || wildcard(b) || a.ips.iter().any(|ip| b.ips.contains(ip)))
}

/// The manager: the operations the D-Bus service / named pipe expose. It is
/// shared by every caller at once: only writes run one at a time.
pub struct Manager<S: ServiceControl, A: Authority, R: Resolver> {
    service: S,
    authority: A,
    resolver: R,
    /// Sets the group of a written instance file.
    group: Box<dyn FileGroup + Send + Sync>,
    /// Checks the passwords [`Manager::authenticate`] is given.
    passwords: Box<dyn PasswordChecker + Send + Sync>,
    /// The GPU totals the memory check of a model profile measure against.
    gpus: Box<dyn GpuTotals + Send + Sync>,
    /// The node's own facts the node-info read answers (AN3).
    node: Box<dyn NodeFacts + Send + Sync>,
    /// Records every guarded call, done or refused, through [`ActionLog`].
    action_log: Box<dyn ActionLog + Send + Sync>,
    /// How many entries the models-folder listing visits before it stops and
    /// answers `truncated` ([`MODELS_WALK_MAX_ENTRIES`], lowered by
    /// [`Self::with_walk_max_entries`]).
    walk_max_entries: usize,
    /// The bounds of the port walk of [`Manager::create_instance`]:
    /// [`FIRST_PROPOSED_PORT`] ..= [`LAST_PROPOSED_PORT`], narrowed by
    /// [`Self::with_port_search`].
    port_search: (u16, u16),
    /// Held by a write from its address check to its rename, so a check
    /// sees every write before it (and `write_served_file`'s one temp
    /// name per file is never shared). Taken only after the authorization:
    /// a caller polkit is still asking about holds up nobody.
    writing: std::sync::Mutex<()>,
    /// The instances the last render pass found not converged, in the pass's
    /// order, each with the keyed fit check that stopped its pair. Memory only
    /// (`ARCHITECTURE.md` "Convergence": not converged is derived, recomputed at
    /// the manager's start and at every write, and a file would add a second
    /// truth): a read answers the rows this pass left, so it measures the pair
    /// against no card (IP3c review 8). Written by the pass, under the `writing`
    /// lock, and read by [`Self::convergence`].
    not_converged: std::sync::Mutex<Vec<(String, pai_core::checks::Refusal)>>,
}

impl<S: ServiceControl, A: Authority, R: Resolver> Manager<S, A, R> {
    pub fn new(service: S, authority: A, resolver: R) -> Self {
        Self {
            service,
            authority,
            resolver,
            group: Box::new(SystemGroup),
            passwords: Box::new(SystemPasswords),
            gpus: Box::new(NoGpus),
            node: Box::new(NoNodeFacts),
            action_log: Box::new(NoActionLog),
            walk_max_entries: MODELS_WALK_MAX_ENTRIES,
            port_search: (FIRST_PROPOSED_PORT, LAST_PROPOSED_PORT),
            writing: std::sync::Mutex::new(()),
            not_converged: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// This manager with `group` setting the group of the instance files it
    /// writes, in place of [`SystemGroup`].
    pub fn with_group(mut self, group: impl FileGroup + Send + Sync + 'static) -> Self {
        self.group = Box::new(group);
        self
    }

    /// This manager with `passwords` checking the passwords, in place of
    /// [`SystemPasswords`].
    pub fn with_passwords(
        mut self,
        passwords: impl PasswordChecker + Send + Sync + 'static,
    ) -> Self {
        self.passwords = Box::new(passwords);
        self
    }

    /// This manager measuring the model profile memory check against the cards
    /// [`GpuTotals::gpus`] answers, in place of [`NoGpus`].
    pub fn with_gpus(mut self, gpus: impl GpuTotals + Send + Sync + 'static) -> Self {
        self.gpus = Box::new(gpus);
        self
    }

    /// This manager answering the node-info read (AN3) from `node`, in place of
    /// [`NoNodeFacts`].
    pub fn with_node_facts(mut self, node: impl NodeFacts + Send + Sync + 'static) -> Self {
        self.node = Box::new(node);
        self
    }

    /// This manager recording every guarded call, done or refused, through
    /// `log`, in place of the silent [`NoActionLog`].
    pub fn with_log(mut self, log: impl ActionLog + Send + Sync + 'static) -> Self {
        self.action_log = Box::new(log);
        self
    }

    /// This manager stopping its models-folder listing after `entries` entries
    /// visited, in place of [`MODELS_WALK_MAX_ENTRIES`].
    pub fn with_walk_max_entries(mut self, entries: usize) -> Self {
        self.walk_max_entries = entries;
        self
    }

    /// This manager proposing the port of a new instance only inside
    /// `first ..= last`, in place of [`FIRST_PROPOSED_PORT`] ..=
    /// [`LAST_PROPOSED_PORT`].
    pub fn with_port_search(mut self, first: u16, last: u16) -> Self {
        self.port_search = (first, last);
        self
    }

    pub fn service(&self) -> &S {
        &self.service
    }

    pub fn authority(&self) -> &A {
        &self.authority
    }

    pub fn resolver(&self) -> &R {
        &self.resolver
    }

    /// The instance ids, `paths::instance_ids()`.
    pub fn list_instances(&self) -> Vec<String> {
        crate::paths::instance_ids()
    }

    /// The unit state of instance `id`: a valid id is enough, so a unit whose
    /// file was deleted stays visible.
    pub fn status(&self, id: &str) -> Result<UnitState, ManagerError> {
        valid_id(id)?;
        self.service.status(id).map_err(ManagerError::Service)
    }

    pub fn start<W: AsRequester>(&self, who: &W, id: &str) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Control)?;
            existing_id(id)?;
            has_profiles(id)?;
            self.service.start(id).map_err(ManagerError::Service)
        };
        let answer = run();
        self.record_action("start", id, Some(&who), &answer);
        answer
    }

    pub fn stop<W: AsRequester>(&self, who: &W, id: &str) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Control)?;
            valid_id(id)?;
            self.service.stop(id).map_err(ManagerError::Service)
        };
        let answer = run();
        self.record_action("stop", id, Some(&who), &answer);
        answer
    }

    pub fn restart<W: AsRequester>(&self, who: &W, id: &str) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Control)?;
            existing_id(id)?;
            has_profiles(id)?;
            self.service.restart(id).map_err(ManagerError::Service)
        };
        let answer = run();
        self.record_action("restart", id, Some(&who), &answer);
        answer
    }

    /// The contents of file `name`, None when it does not exist. An instance
    /// file needs the control authorization: no requester (a transport with
    /// no identity for the call) or a refused one is NotAuthorized, nothing
    /// read; the model profile files, [`ASSIGNMENTS_FILE`] and [`INSTANCES_FILE`]
    /// need none, and their
    /// text is read whole. A reader of an instance file who
    /// may not configure (asked without a prompt) gets `OpencodeApiKey`
    /// blanked ([`withhold_api_key`]). A request naming a principal is gated
    /// on that principal alone, whatever the file, and its read is never
    /// withheld: a member of `pai` may configure.
    pub fn read_file<W: AsRequester>(
        &self,
        who: Option<&W>,
        name: &str,
    ) -> Result<Option<pai_core::client::ManagerFile>, ManagerError> {
        let who = who.map(AsRequester::as_requester);
        let run = || -> Result<Option<pai_core::client::ManagerFile>, ManagerError> {
            // A request naming a principal is gated first, whatever the file:
            // the pair (the bff's account, a current member) is its only right.
            if let Some(who) = &who {
                self.gate_principal(who)?;
            }
            // [`ASSIGNMENTS_FILE`] and [`INSTANCES_FILE`] are served for
            // reading only: neither is a `ConfigFile`, so the gate below never
            // sees it and the write gate refuses it by name.
            if config_file(name).is_none() && !read_only_file(name) {
                return Err(ManagerError::BadFileName);
            }
            let mut may_configure = true;
            if let Some(ConfigFile::ServerProfile(_)) = config_file(name) {
                match &who {
                    // Gated above: polkit is asked for neither action, and a
                    // member of `pai` may configure, so nothing is withheld.
                    Some(who) if who.principal.is_some() => {}
                    Some(who) => {
                        self.authorize(&who.caller, Action::Control)?;
                        may_configure = self
                            .authority
                            .is_authorized_quietly(&who.caller, Action::Configure);
                    }
                    None => return Err(ManagerError::NotAuthorized),
                }
            }
            let path = file_path(name)?;
            match std::fs::read_to_string(path) {
                Ok(text) if may_configure => Ok(Some(pai_core::client::ManagerFile {
                    text,
                    withheld: false,
                })),
                Ok(text) => Ok(Some(withhold_api_key(&text))),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(ManagerError::Io(e.kind())),
            }
        };
        let answer = run();
        self.record_action("read_file", name, who.as_ref(), &answer);
        answer
    }

    /// Replace file `name` atomically, creating the directory. A
    /// `server@<profile>.ini` is a server profile, not an instance: the
    /// instances are the sections of `instances.ini`, and no write of a file
    /// adds one. With `keep_api_key`, an instance file keeps the
    /// `OpencodeApiKey` its current file holds ([`keep_held_api_key`]); the
    /// checks run on the text written.
    pub fn write_file<W: AsRequester>(
        &self,
        who: &W,
        name: &str,
        contents: &str,
        keep_api_key: bool,
    ) -> Result<(), ManagerError> {
        self.write_guarded(who, name, "write_file", |file| {
            let kept;
            let contents = match file {
                ConfigFile::ServerProfile(_) if keep_api_key => {
                    // An unreadable file says nothing about the key: refused,
                    // since writing the text as sent could erase it.
                    let held = match std::fs::read_to_string(file_path(name)?) {
                        Ok(t) => crate::server_cfg::from_text(&t).opencode_api_key,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                        Err(e) => return Err(ManagerError::Io(e.kind())),
                    };
                    match held {
                        Some(key) => {
                            kept = keep_held_api_key(contents, &key);
                            kept.as_str()
                        }
                        None => contents,
                    }
                }
                _ => contents,
            };
            self.check_and_write(file, name, contents, MODEL_FILES_JUDGE, PLACEMENT_JUDGE)
                // The plain write answers nothing: its caller sent no read to
                // compare, so no client is waiting on the states of the render
                // (IP3c).
                .map(|_| ())
        })
    }

    /// Replace file `name` only while it still holds `expected`, the whole
    /// text its caller read, byte for byte: equal bytes run `write_file`'s
    /// checks and write, a file that is missing or holds other bytes is
    /// [`ManagerError::Changed`] with nothing written. No `keep_api_key`: the
    /// caller sent the file as it read it, key line included. The principal and
    /// `configure` gates run first, as `write_file`'s, and the compare runs
    /// under the same `writing` lock as the write. The write answers the states
    /// it left ([`Affected`], IP3c): the page that saved a model, a server or an
    /// instance profile reads them to offer the restart.
    pub fn write_file_if<W: AsRequester>(
        &self,
        who: &W,
        name: &str,
        contents: &str,
        expected: &str,
    ) -> Result<Affected, ManagerError> {
        self.write_guarded(who, name, "write_file_if", |file| {
            // The compare is the first thing under the lock, before any check
            // of `contents`: `expected` is the whole text the caller read, so
            // other bytes mean its read is stale and the write is refused.
            match std::fs::read_to_string(file_path(name)?) {
                Ok(text) if text == expected => {
                    self.check_and_write(file, name, contents, MODEL_FILES_JUDGE, PLACEMENT_JUDGE)
                        // The compare-and-swap answers the states it left: its
                        // caller read the file and is waiting on the rows
                        // ([`Affected`], IP3c), read from the pass this write
                        // ran.
                        .and_then(|pass| Ok(self.affected_of(&pass, self.drift_running()?)))
                }
                Ok(_) => Err(ManagerError::Changed),
                // A file that is not there is the same refusal: the caller's
                // read has nothing to match. A read that failed for another
                // reason says nothing about the bytes, so it is refused as
                // `write_file`'s read of the held key is: writing blind could
                // replace text the caller never saw.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(ManagerError::Changed),
                Err(e) => Err(ManagerError::Io(e.kind())),
            }
        })
    }

    /// The shape every write of a served file has, the one `write_file` and
    /// `write_file_if` share: the `configure` gate of `who` (its principal
    /// first, polkit only for a call of the caller's own), the name, `body`
    /// with that file under the `writing` lock, and one action record under
    /// `action` for the whole call, done or refused. A refused gate or name
    /// reads nothing, so it writes nothing.
    fn write_guarded<W: AsRequester, F, T>(
        &self,
        who: &W,
        name: &str,
        action: &str,
        body: F,
    ) -> Result<T, ManagerError>
    where
        F: FnOnce(&ConfigFile) -> Result<T, ManagerError>,
    {
        let who = who.as_requester();
        let run = || -> Result<T, ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            let file = config_file(name).ok_or(ManagerError::BadFileName)?;
            // The two files the model data model retires (`ARCHITECTURE.md`,
            // "Instance profiles"): the instance profiles and their entry
            // sections are the model list, and the record is the address, so
            // `assignments.ini` and the file of the switched-off model profiles
            // are read by nobody, and the manager's start removes them. A client
            // reads them and never writes them: `config_file` answers no served
            // file for `assignments.ini`, and the switched-off file is the
            // served file that is retired.
            if name == MODELS_DISABLED_FILE {
                return Err(ManagerError::BadFileName);
            }
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            body(&file)
        };
        let answer = run();
        self.record_action(action, name, Some(&who), &answer);
        answer
    }

    /// The checks `contents` must pass for `file`, and the atomic write of it:
    /// [`check_contents`], the address check of an instance file, then the
    /// write, creating the directory, then the render of every instance from
    /// what is on disk now. Taken under the `writing` lock, by the body of
    /// [`Self::write_guarded`], so the compare of a compare-and-swap, the write
    /// it authorizes and the renders it causes are one atomic step.
    ///
    /// `judge_model_files` is [`MODEL_FILES_JUDGE`] for every write, and
    /// [`MODEL_FILES_SKIP`] for the create of a model profile, whose section is
    /// listed in no entry: the model file it names is judged when an entry lists
    /// it. `judge_placement` is [`PLACEMENT_JUDGE`] for the save of a pool, and
    /// [`PLACEMENT_SKIP`] for the create and the delete, which judge the
    /// section they write and leave the sections they do not.
    fn check_and_write(
        &self,
        file: &ConfigFile,
        name: &str,
        contents: &str,
        judge_model_files: bool,
        judge_placement: bool,
    ) -> RenderPass {
        check_contents(file, contents, judge_model_files, judge_placement)
            .map_err(ManagerError::Invalid)?;
        if let ConfigFile::ServerProfile(id) = file {
            self.check_address(id, contents)?;
        }
        let dir = crate::paths::engine_config_dir();
        let io = |e: std::io::Error| ManagerError::Io(e.kind());
        std::fs::create_dir_all(&dir).map_err(io)?;
        write_served_file(&dir.join(name), file, contents, &*self.group).map_err(io)?;
        // The write stands whatever the renders do: the render runs past a
        // failure, and the caller is told the first IO failure in id order. A
        // pair the fit check refused is not the write's failure: the save
        // landed, that instance kept its last good pair, and the rows report it
        // (IP3c). The pass's rows go to the caller: the save that ran this write
        // answers them, and renders nothing a second time.
        self.render_pass()
    }

    /// Whether instance `id` starts when the system starts. No authorization,
    /// like `status`; a valid id that is not a section of the record
    /// (`instances.ini`) is NoInstance.
    pub fn boot_enabled(&self, id: &str) -> Result<bool, ManagerError> {
        existing_id(id)?;
        self.service.boot_enabled(id).map_err(ManagerError::Service)
    }

    /// Make instance `id` start when the system starts, or not: the
    /// configure authorization, asked first (systemd's own enabling asks for
    /// an administrator); a refused caller changes nothing.
    pub fn set_boot_enabled<W: AsRequester>(
        &self,
        who: &W,
        id: &str,
        enabled: bool,
    ) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            existing_id(id)?;
            self.service
                .set_boot_enabled(id, enabled)
                .map_err(ManagerError::Service)
        };
        let answer = run();
        self.record_action("set_boot_enabled", id, Some(&who), &answer);
        answer
    }

    /// Assigns the model profile `profile` to instance `instance` as `value`:
    /// `on`, `off` (the explicit overrides), `inherit` (the key kept, the
    /// pool's own state) or `none` (the key removed). `*` is every model
    /// profile of the pool and takes `none` and `inherit` only. The key goes
    /// in the instance's section of `assignments.ini`, and every model profile
    /// file of every instance renders again from what is on disk: the checks
    /// run first, so a refusal writes and renders nothing. The render this
    /// record leaves is judged for its model files: only a file the record ADDS
    /// and this node does not have refuses (IP3c review 4). What the render
    /// needs of the cards, of the `ModelsMax` of the server profile and of the
    /// GPUs its model profiles name is the fit check of the render pass (IP3c,
    /// `ARCHITECTURE.md` "Convergence"): the write stands, and the instance the
    /// check stops keeps its last good pair. Guarded as a write
    /// of a served file is: the principal and `configure` gates, the `writing`
    /// lock, and one action record for the whole call, done or refused.
    pub fn set_model_profile<W: AsRequester>(
        &self,
        who: &W,
        instance: &str,
        profile: &str,
        value: &str,
    ) -> Result<Affected, ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<Affected, ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            // The value first: `*` is the pool and has no override of its own.
            let value = match value {
                "on" | "off" if profile != "*" => value,
                "inherit" | "none" => value,
                _ => {
                    return Err(ManagerError::Invalid(pai_core::checks::Refusal {
                        key: "model-profile-bad-value",
                        values: vec![value.to_string()],
                    }))
                }
            };
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            // The instance record is judged under this same lock, as the
            // record this call writes.
            existing_id(instance)?;
            let dir = crate::paths::engine_config_dir();
            let io = |e: std::io::Error| ManagerError::Io(e.kind());
            // A file that is really absent is an empty one; any other read
            // failure answers its own kind before anything is written, so a
            // record that cannot be read is never replaced with an empty one.
            let on_disk = |name: &str| -> Result<String, ManagerError> {
                match std::fs::read_to_string(dir.join(name)) {
                    Ok(text) => Ok(text),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
                    Err(e) => Err(ManagerError::Io(e.kind())),
                }
            };
            let enabled = on_disk("models.ini")?;
            let disabled = on_disk("models.ini.disabled")?;
            // The model profile is of the pool, in either file; the pool's
            // global `*` is asked of no id, it names the whole pool.
            if profile != "*" && !pool_holds(&enabled, profile) && !pool_holds(&disabled, profile) {
                return Err(ManagerError::Invalid(pai_core::checks::Refusal {
                    key: "no-model-profile-named",
                    values: vec![profile.to_string()],
                }));
            }
            // The record with the key as asked, judged before it is written:
            // the render it produces is the one the instance loads, against
            // the `ModelsMax` of the server profile its section names. A
            // section that names no profile, or names one whose file is
            // missing or cannot be read, has no cap to judge, as
            // `check_renders` does: the launch refuses that instance, and a
            // render is never judged on the defaults of a file that is not
            // there.
            let path = dir.join(ASSIGNMENTS_FILE);
            let current = on_disk(ASSIGNMENTS_FILE)?;
            let text = with_profile(&current, instance, profile, value);
            // The model files the render this record leaves names, judged
            // against the render this instance has today: only a file the
            // record ADDS refuses. The check has no cap to read, so it runs on
            // this instance, the one whose profile file is missing included.
            // Model files present stay an intrinsic check of the node (IP3c
            // review 4): a shard the node does not have is the news the
            // assignment answers, on every instance that renders the profile.
            let today = render_of(instance, &enabled, &disabled);
            check_model_files_of(&text, instance, &enabled, &disabled, &today)
                .map_err(ManagerError::Invalid)?;
            // The memory the render needs, the cards its model profiles name
            // and its `load-on-startup` count are not judged here (IP3c): they
            // are the fit checks of one instance, the render pass judges them
            // on the render this record leaves, and the instance they stop
            // keeps its last good pair.
            std::fs::create_dir_all(&dir).map_err(io)?;
            write_served_file(&path, &ConfigFile::Models, &text, &*self.group).map_err(io)?;
            // The write stands whatever the renders do, as a write of a
            // served file does: the caller is told the first IO failure in id
            // order. A pair the fit check refused is no failure of the write
            // (IP3c). The rows the pass left are the answer.
            self.render_pass()
                .and_then(|pass| Ok(self.affected_of(&pass, self.drift_running()?)))
        };
        let answer = run();
        self.record_action("set_model_profile", instance, Some(&who), &answer);
        answer
    }

    /// Writes a new model profile `profile` at the end of `models.ini`, the one
    /// pool, never over a name the pool holds in any letter case. The source is
    /// one of two: `values` names the model, with `model` alone or `hf-repo` and
    /// its optional `hf-file`, and becomes the section's own lines sorted by
    /// key; `from` names a profile of the pool, whose section's stay keys the
    /// new one takes under its own header, and takes with it the `values` that
    /// only override a line, the clone carrying its model with it. A key of the
    /// entry ([`pai_core::checks::PLACEMENT_KEYS`]) is left out of the copy: it
    /// belongs to the entry of the instance profile that lists the model, and a
    /// new profile is listed in none. Every check answers before a byte is
    /// written, in this order: a value that is blank, carries a control
    /// character, a space at an end or a comment marker, or a key that is no
    /// key of the INI (`model-profile-bad-text` and `comment-markers`, each
    /// naming the key), the shape of the source (`model-profile-bad-source`),
    /// the id rule (`invalid-model-profile-id`), a name the pool holds
    /// (`model-profile-name-taken`), a value naming a key of the entry
    /// (`pai_core::checks::PLACEMENT_KEYS`, `model-profile-placement-key`,
    /// naming the key), and a source that is no pool id
    /// (`no-model-profile-named`, naming the source). Nothing renders: the new
    /// profile is listed in no entry, so it joins no instance's render, and the
    /// write changes no render. The new section
    /// takes the line ending of the file it lands in, the CRLF of a file the
    /// install never wrote. Guarded as a write of a
    /// served file is: `configure` first, the `As` form answered only to the
    /// bff's account for a current member of `pai`, one record of the action log
    /// per call.
    pub fn create_model_profile<W: AsRequester>(
        &self,
        who: &W,
        profile: &str,
        from: &str,
        values: &[(String, String)],
    ) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            // The values, the source and the id are judged before the lock and
            // before a file is read: they need nothing of the node.
            for (key, value) in values {
                create_key_text(key).map_err(ManagerError::Invalid)?;
                create_value_text(key, value).map_err(ManagerError::Invalid)?;
            }
            if !create_source_shape(from, values) {
                return Err(one_value_refusal("model-profile-bad-source", profile));
            }
            pai_core::checks::model_profile_id(profile).map_err(ManagerError::Invalid)?;
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            let io = |e: std::io::Error| ManagerError::Io(e.kind());
            let dir = crate::paths::engine_config_dir();
            // A file that is really absent is an empty one; any other read
            // failure answers its own kind, so a pool that cannot be read is
            // never replaced with an empty one.
            let on_disk = |name: &str| -> Result<String, ManagerError> {
                match std::fs::read_to_string(dir.join(name)) {
                    Ok(text) => Ok(text),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
                    Err(e) => Err(ManagerError::Io(e.kind())),
                }
            };
            let pool = on_disk("models.ini")?;
            let holds = |text: &str| pai_core::ini::text::section_text(text, profile).is_some();
            if holds(&pool) {
                return Err(one_value_refusal("model-profile-name-taken", profile));
            }
            // A key of the entry among the values is refused here, before the
            // source is read: the new profile is listed in no entry, so such a
            // key would put a second source of where the weights go in the
            // pool, where the migration took it out of. A pick takes the three
            // keys of the model file only, so this reaches a clone's overrides.
            if let Some((key, _)) = values
                .iter()
                .find(|(key, _)| pai_core::checks::placement_key(key))
            {
                return Err(one_value_refusal("model-profile-placement-key", key));
            }
            // The section to write, at the end of the pool: the source's stay
            // lines under the new header, the call's keys after them. A clone
            // takes the line ending of the source's own text, the keys that of
            // the file being written, the CRLF of a file the install never
            // wrote.
            let eol = pai_core::ini::detect_eol(&pool);
            let mut section = String::new();
            if !from.is_empty() {
                let source = pai_core::ini::text::section_text(&pool, from)
                    .ok_or_else(|| one_value_refusal("no-model-profile-named", from))?;
                section.push_str(&cloned_section(&source, profile));
            }
            if !values.is_empty() {
                if from.is_empty() {
                    section.push_str(&format!("[{profile}]{eol}"));
                }
                let mut keys: Vec<&(String, String)> = values.iter().collect();
                keys.sort_by(|a, b| a.0.cmp(&b.0));
                for (key, value) in keys {
                    section.push_str(&format!("{key} = {value}{eol}"));
                }
            }
            let mut after = pool;
            if !after.is_empty() && !after.ends_with('\n') {
                after.push_str(eol);
            }
            after.push_str(&section);
            // The content checks of a pool write, then the write itself, and no
            // render: the new profile is listed in no entry, so it is in no
            // instance's render, and the write changes no render (its model
            // file, its devices and its memory are judged when an entry lists
            // it). A render pass here would rewrite renders this call has no
            // part in.
            check_contents(
                &ConfigFile::Models,
                &after,
                MODEL_FILES_SKIP,
                PLACEMENT_SKIP,
            )
            .map_err(ManagerError::Invalid)?;
            std::fs::create_dir_all(&dir).map_err(&io)?;
            write_served_file(
                &dir.join("models.ini"),
                &ConfigFile::Models,
                &after,
                &*self.group,
            )
            .map_err(&io)?;
            Ok(())
        };
        let answer = run();
        self.record_action("create_model_profile", profile, Some(&who), &answer);
        answer
    }

    /// Removes the model profile `profile` from `models.ini`, the one pool: only
    /// a profile no instance profile's entry `[<profile>/<model>]` lists. Every
    /// check answers before a byte is written, in this order: the id rule answers
    /// nothing of its own here, so no id and `*` are `no-model-profile-named
    /// [profile]`, with no file read and no path built; the pool of instance
    /// profiles is read next, so a file that cannot be read is
    /// [`ManagerError::Io`], never the empty list of a node with no profile; a
    /// name the pool has no section for is the same refusal, a key of the retired
    /// `assignments.ini` or a section of the retired `models.ini.disabled` being
    /// no proof it exists; and a name an entry lists, in any letter case, is
    /// `model-profile-assigned [profile, "<a>, <b>"]`, naming every instance
    /// profile that lists it, in the pool's own order, each once: the model sits
    /// in those profiles' renders, and a delete would leave an entry naming
    /// nothing. The write takes every section of the id, repeated in any letter
    /// case, out of the pool, through the content checks of a save: a file whose
    /// last section is removed stands empty. Nothing else moves, and the renders
    /// that move are those of the instances running a profile that lists the
    /// model. Guarded as a write of a served file is: `configure` first, the
    /// `As` form answered only to the bff's account for a current member of
    /// `pai`, one record of the action log per call.
    pub fn delete_model_profile<W: AsRequester>(
        &self,
        who: &W,
        profile: &str,
    ) -> Result<Affected, ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<Affected, ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            let dir = crate::paths::engine_config_dir();
            let on_disk = |name: &str| -> Result<String, ManagerError> {
                match std::fs::read_to_string(dir.join(name)) {
                    Ok(text) => Ok(text),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
                    Err(e) => Err(ManagerError::Io(e.kind())),
                }
            };
            // The id is judged before any file, and answers the pool's refusal
            // of its own: no id and `*` included, so no path is built from it.
            if profile.is_empty()
                || profile == "*"
                || !pai_core::checks::valid_model_profile_id(profile)
            {
                return Err(one_value_refusal("no-model-profile-named", profile));
            }
            // The entries are the ground of the refusal, so they are read before
            // the pool: their file unreadable is the read's own kind, never the
            // "unused" of an empty list.
            let profiles = self.pool_text()?;
            let text = on_disk("models.ini")?;
            if pai_core::ini::text::section_text(&text, profile).is_none() {
                return Err(one_value_refusal("no-model-profile-named", profile));
            }
            let holders = entry_profile_owners(&profiles, profile);
            if !holders.is_empty() {
                return Err(ManagerError::Invalid(pai_core::checks::Refusal {
                    key: "model-profile-assigned",
                    values: vec![profile.to_string(), holders.join(", ")],
                }));
            }
            let mut after = text;
            while let Some(text) = pai_core::ini::text::delete_section(&after, profile) {
                after = text;
            }
            self.check_and_write(
                &ConfigFile::Models,
                "models.ini",
                &after,
                MODEL_FILES_JUDGE,
                PLACEMENT_SKIP,
            )
            // The delete is a save that re-renders, so it answers the apply like
            // every one of them (IP4a): the running instances the write left
            // drifting, beside the rows of every instance whose pair the write
            // did not leave ([`Self::affected_of`]).
            .and_then(|pass| Ok(self.affected_of(&pass, self.drift_running()?)))
        };
        let answer = run();
        self.record_action("delete_model_profile", profile, Some(&who), &answer);
        answer
    }

    /// Points instance `instance` at the server profile `profile`: writes the
    /// `ServerProfile` key of its section of `instances.ini`, every other byte
    /// of the record kept (of the section of `instance-profiles.ini` the record
    /// names as its instance profile, once the migration has written the
    /// record). The instance's launch is the one that reads the profile, so the
    /// call touches no unit: a server profile carries no model profile text, and
    /// the render's name is the instance's. Each check answers before the write,
    /// and a refusal writes nothing: the id must be a section of the record
    /// (`InvalidId`, `NoInstance`; a record that cannot be read is `Io`, never
    /// the empty list `paths::instance_ids` answers for one), the profile must be
    /// one the engine has (`NoServerProfile`, naming it: a name that builds no
    /// file, or a file that is really absent; any other read failure is `Io`), no
    /// other instance may run it (`ServerProfileInUse`, naming the profile and
    /// that instance), and the address the switch takes must be free
    /// (`AddressInUse`). What the profile's cap, cards and devices do to the
    /// instance's render is judged there, not here: the switch re-renders, and
    /// the render pass's fit checks stop the instance they stop, which keeps its
    /// last good pair (IP3c, `ARCHITECTURE.md` "Convergence"). The profile the
    /// instance's own section names is answered before the walk of the others: no
    /// conflict with itself, and the switch to it changes no byte. The answer is
    /// the states the render leaves ([`Affected`], IP3c). Guarded as a write of
    /// a served file is: the principal and `configure` gates, the `writing`
    /// lock, and one action record for the whole call, done or refused.
    pub fn set_server_profile<W: AsRequester>(
        &self,
        who: &W,
        instance: &str,
        profile: &str,
    ) -> Result<Affected, ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<Affected, ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            valid_id(instance)?;
            let dir = crate::paths::engine_config_dir();
            let io = |e: std::io::Error| ManagerError::Io(e.kind());
            // A file that is really absent is an empty one; any other read
            // failure answers its own kind before anything is written, as in
            // `set_model_profile`. The record first: it is the one this call
            // writes, and an instance is a section of it.
            let record = read_record(&dir.join(INSTANCES_FILE)).map_err(io)?;
            if !pai_core::ini::parse_sections(&record)
                .iter()
                .any(|section| section.id == instance)
            {
                return Err(ManagerError::NoInstance);
            }
            // The target's file. `paths::instance_ini` answers nothing for a
            // name that builds no profile path, and the engine's directory is
            // the only place a name is looked up: both are the profile the
            // engine has none of, said with the name the caller asked for.
            let path = crate::paths::instance_ini(profile)
                .ok_or_else(|| ManagerError::NoServerProfile(profile.to_string()))?;
            let contents = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err(ManagerError::NoServerProfile(profile.to_string()));
                }
                Err(e) => return Err(io(e)),
            };
            // One instance per server profile: the profile of another instance
            // is that instance's address and its cap. The profile the
            // instance's own section names is answered before that walk: it is
            // no conflict with itself, and the switch to it changes no byte.
            if crate::paths::instance_server_profile(instance).as_deref() == Some(profile) {
                // The answer of a save that re-renders, with nothing moved: the
                // switch changed no byte, so no running instance drifts, and the
                // rows are the ones the files already give (IP3c).
                return self.affected();
            }
            if let Some(holder) = profile_holder(profile) {
                return Err(ManagerError::ServerProfileInUse(
                    profile.to_string(),
                    holder,
                ));
            }
            // The address the switch takes. The target is unused at this
            // moment, so a save of it is judged against nothing: the switch is
            // where its address meets the instances that run, and the web
            // service. This instance's own current profile is out of the walk,
            // so a profile on the port the instance already runs lands.
            let mine = binding(&crate::server_cfg::from_text(&contents), &self.resolver)
                .map_err(ManagerError::UnresolvableHost)?;
            self.check_binding(&mine, |id| id == instance)?;
            // What the new profile's cap, cards and binding do to the instance's
            // render are not judged here (IP3c): they are the fit checks of that
            // one instance, the render pass judges them on the render the switch
            // leaves, and the instance they stop keeps its last good pair. The
            // switch itself lands, as the record write it is. The profile's own
            // content is checked at its save, its address above, where it meets
            // the instances that run.
            std::fs::create_dir_all(&dir).map_err(io)?;
            // A record the migration wrote names an instance profile, and the
            // switch moves that profile's `ServerProfile` key in the pool: the
            // record keeps its instance profile and its address. A record the
            // migration has not written names the server profile itself, and the
            // switch writes that key of the section, as it did.
            let (file, text) = if record_is_migrated(&record) {
                let profiles = read_record(&dir.join(INSTANCE_PROFILES_FILE)).map_err(io)?;
                let target =
                    instance_profile_of(&record, instance).ok_or(ManagerError::NoInstance)?;
                (
                    INSTANCE_PROFILES_FILE,
                    pai_core::ini::text::set_key(&profiles, &target, "ServerProfile", profile),
                )
            } else {
                (
                    INSTANCES_FILE,
                    pai_core::ini::text::set_key(&record, instance, "ServerProfile", profile),
                )
            };
            write_served_file(&dir.join(file), &ConfigFile::Models, &text, &*self.group)
                .map_err(io)?;
            // The switch moves the server config the instance launches with, so
            // it re-renders the pair: `run-instance` reads `launch@<id>.ini` in
            // place of resolving the record, and a launch left at the old
            // profile's port would boot the instance on the port it just left.
            // The model half of the pair is unchanged, so the pass writes bytes
            // and asks the GPUs nothing (IP3c). The rows the pass left are the
            // answer.
            self.render_pass()
                .and_then(|pass| Ok(self.affected_of(&pass, self.drift_running()?)))
        };
        let answer = run();
        self.record_action("set_server_profile", instance, Some(&who), &answer);
        answer
    }

    /// Replaces the `Tags` key of the instance's own section of the record:
    /// the record's tags, which the web UI shows beside the tags of the
    /// server profile the instance points at. Guarded as `set_server_profile`
    /// is: the principal and the `configure` gate, the `writing` lock, and one
    /// action record for the whole call, done or refused. The rule is
    /// `pai_core::checks::tags_rule`, applied to the list before the record is
    /// read: a list it refuses leaves every file untouched.
    pub fn set_instance_tags<W: AsRequester>(
        &self,
        who: &W,
        instance: &str,
        tags: &[String],
    ) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            valid_id(instance)?;
            pai_core::checks::tags_rule(tags).map_err(ManagerError::Invalid)?;
            let dir = crate::paths::engine_config_dir();
            let io = |e: std::io::Error| ManagerError::Io(e.kind());
            let record = read_record(&dir.join(INSTANCES_FILE)).map_err(io)?;
            if !pai_core::ini::parse_sections(&record)
                .iter()
                .any(|section| section.id == instance)
            {
                return Err(ManagerError::NoInstance);
            }
            let text = pai_core::ini::text::set_key(&record, instance, "Tags", &tags.join(","));
            write_served_file(
                &dir.join(INSTANCES_FILE),
                &ConfigFile::Models,
                &text,
                &*self.group,
            )
            .map_err(io)
        };
        let answer = run();
        self.record_action("set_instance_tags", instance, Some(&who), &answer);
        answer
    }

    /// Replaces the `Label` key of the instance's own section of the record:
    /// the text the web UI shows beside the id (IL1). Guarded as
    /// `set_instance_tags` is: the principal and the `configure` gate, the
    /// `writing` lock, and one action record for the whole call, done or
    /// refused. The rule is `pai_core::checks::instance_label`, applied before
    /// the record is read; the text lands trimmed by that very rule, spaces
    /// and tabs at the ends and nothing else, and the empty text, after the
    /// trim, REMOVES the key (never an empty key), so the reader sees no label
    /// and the UI falls back to the engine and the port. A label equal to
    /// ANOTHER instance's, ignoring letter case, is refused
    /// `instance-label-taken` naming who holds it, judged under the write
    /// lock; the section being written is never the one the rule compares
    /// against, so an instance changing the letter case of its own label
    /// relabels itself. The label is nothing the engine reads: a running
    /// instance is relabelled without a stop, and nothing is rendered.
    pub fn set_instance_label<W: AsRequester>(
        &self,
        who: &W,
        instance: &str,
        label: &str,
    ) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            valid_id(instance)?;
            pai_core::checks::instance_label(label).map_err(ManagerError::Invalid)?;
            let label = label.trim_matches([' ', '\t']);
            let dir = crate::paths::engine_config_dir();
            let io = |e: std::io::Error| ManagerError::Io(e.kind());
            let record = read_record(&dir.join(INSTANCES_FILE)).map_err(io)?;
            let sections = pai_core::ini::parse_sections(&record);
            if !sections.iter().any(|section| section.id == instance) {
                return Err(ManagerError::NoInstance);
            }
            // Unique on the node, ignoring letter case: two rows with one
            // name defeat the point of a name. Not judged against the
            // fallback text, nor against ids. The section being written is
            // never one of the ones compared: an instance changing the letter
            // case of its own label relabels itself, it does not clash with
            // the text it already carries, and that text stays taken for every
            // other instance.
            if !label.is_empty() {
                let taken = label.to_lowercase();
                if let Some(other) = sections.iter().find(|section| {
                    section.id != instance
                        && section
                            .keys
                            .get("Label")
                            .is_some_and(|held| held.trim().to_lowercase() == taken)
                }) {
                    return Err(ManagerError::Invalid(pai_core::checks::Refusal {
                        key: "instance-label-taken",
                        values: vec![label.to_string(), other.id.clone()],
                    }));
                }
            }
            let text = if label.is_empty() {
                pai_core::ini::text::remove_key(&record, instance, "Label")
            } else {
                pai_core::ini::text::set_key(&record, instance, "Label", label)
            };
            write_served_file(
                &dir.join(INSTANCES_FILE),
                &ConfigFile::Models,
                &text,
                &*self.group,
            )
            .map_err(io)
        };
        let answer = run();
        self.record_action("set_instance_label", instance, Some(&who), &answer);
        answer
    }

    /// Writes a new server profile of the engine `engine`: `server@<profile>.ini`
    /// rendered from the server config's defaults with `port` (`None` leaves the
    /// file no `Port` key, so the default port answers), written as a save
    /// writes it ([`write_served_file`]: mode [`file_mode`], group
    /// [`INSTANCE_GROUP`]). Nothing is ever overwritten: any entry already at
    /// that name, a dangling link included (the check is
    /// [`std::fs::symlink_metadata`], so the link's target is never created), is
    /// [`ManagerError::ServerProfileExists`], and the bytes that are there stay.
    /// The new file is judged by no address and no cap: it binds nothing (the
    /// design of `check_address`: a profile no instance runs is judged against
    /// nothing) and loads no model. The check for an entry and the write are
    /// safe together because every manager write holds `writing`. Guarded as a
    /// write of a served file is: the principal and `configure` gates, the
    /// `writing` lock, and one action record for the whole call, done or
    /// refused, with the profile as its target.
    pub fn create_server_profile<W: AsRequester>(
        &self,
        who: &W,
        engine: &str,
        profile: &str,
        port: Option<u16>,
    ) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            known_engine(engine)?;
            // The name rule `config_file` applies to `server@<x>.ini`: no name
            // builds a path outside the engine's directory.
            let path = crate::paths::instance_ini(profile)
                .ok_or_else(|| one_value_refusal("server-profile-bad-id", profile))?;
            if std::fs::symlink_metadata(&path).is_ok() {
                return Err(ManagerError::ServerProfileExists(profile.to_string()));
            }
            let io = |e: std::io::Error| ManagerError::Io(e.kind());
            std::fs::create_dir_all(crate::paths::engine_config_dir()).map_err(io)?;
            let cfg = crate::server_cfg::ServerConfig {
                port: port.map(i32::from),
                ..Default::default()
            };
            write_served_file(
                &path,
                &ConfigFile::ServerProfile(profile.to_string()),
                &crate::server_cfg::render_for(&cfg, true),
                &*self.group,
            )
            .map_err(io)
        };
        let answer = run();
        self.record_action("create_server_profile", profile, Some(&who), &answer);
        answer
    }

    /// Removes the server profile `profile` of the engine `engine`, when no
    /// instance points at it. A name the id rule refuses and a name with no
    /// file answer [`ManagerError::NoServerProfile`] (the name first, so a bad
    /// name never looks at a file); the record is read with [`read_record`], so
    /// a record that is not there names no instance and a record that cannot be
    /// read is [`ManagerError::Io`] rather than "unused": a profile a running
    /// instance reads is never removed on a read failure. The first section
    /// naming it is [`ManagerError::ServerProfileInUse`], naming that instance,
    /// and the file stays. Guarded as a write of a served file is: the
    /// principal and `configure` gates, the `writing` lock, and one action
    /// record for the whole call, done or refused, with the profile as its
    /// target. `default` is no special case: in use it is refused like any
    /// other, and unused it serves nothing.
    pub fn delete_server_profile<W: AsRequester>(
        &self,
        who: &W,
        engine: &str,
        profile: &str,
    ) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            known_engine(engine)?;
            let io = |e: std::io::Error| ManagerError::Io(e.kind());
            let no_profile = || ManagerError::NoServerProfile(profile.to_string());
            let path = crate::paths::instance_ini(profile).ok_or_else(no_profile)?;
            if std::fs::symlink_metadata(&path).is_err() {
                return Err(no_profile());
            }
            let record =
                read_record(&crate::paths::engine_config_dir().join(INSTANCES_FILE)).map_err(io)?;
            if let Some(holder) = record_holder(&record, profile) {
                return Err(ManagerError::ServerProfileInUse(
                    profile.to_string(),
                    holder,
                ));
            }
            std::fs::remove_file(&path).map_err(io)
        };
        let answer = run();
        self.record_action("delete_server_profile", profile, Some(&who), &answer);
        answer
    }

    /// Copies the server profile `from` of the engine `engine` to a new
    /// profile `profile`, byte for byte. Guarded as a write of a served
    /// file is. The id and target checks are the create's, in its order; the
    /// source's file is read whole and written through the same temp file and
    /// rename, so the copy lands complete. The API key line rides with the
    /// bytes: the manager never hands a key to a client, and a client that
    /// cannot read a file cannot read the copy. No render, no record, no unit
    /// and no state: an instance running on the source keeps running on it.
    pub fn copy_server_profile<W: AsRequester>(
        &self,
        who: &W,
        engine: &str,
        from: &str,
        profile: &str,
    ) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            known_engine(engine)?;
            let io = |e: std::io::Error| ManagerError::Io(e.kind());
            // The name rule `config_file` applies to `server@<x>.ini`: no name
            // builds a path outside the engine's directory, and the target
            // check that follows is the create's.
            let path = crate::paths::instance_ini(profile)
                .ok_or_else(|| one_value_refusal("server-profile-bad-id", profile))?;
            if std::fs::symlink_metadata(&path).is_ok() {
                return Err(ManagerError::ServerProfileExists(profile.to_string()));
            }
            // A source name the rule refuses is a source with no file: the
            // delete answers it the same way.
            let source = crate::paths::instance_ini(from)
                .ok_or_else(|| ManagerError::NoServerProfile(from.to_string()))?;
            let bytes = std::fs::read(&source).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => ManagerError::NoServerProfile(from.to_string()),
                other => ManagerError::Io(other),
            })?;
            // The bytes are text: the served-file write takes a text, and a
            // source that stopped being text is not a served file.
            let text = String::from_utf8(bytes)
                .map_err(|_| ManagerError::Io(std::io::ErrorKind::InvalidData))?;
            write_served_file(
                &path,
                &ConfigFile::ServerProfile(profile.to_string()),
                &text,
                &*self.group,
            )
            .map_err(io)
        };
        let answer = run();
        self.record_action("copy_server_profile", profile, Some(&who), &answer);
        answer
    }

    // ── The instance profile verbs ────────────────────────────────────────
    // The pool of instance profiles, its entries, the instance's profile and
    // its address (`ARCHITECTURE.md` "Instance profiles", slice IP3). A write
    // of the pool or of the record is a write of a served file: the principal
    // gate and `configure`, the `writing` lock, one action record for the
    // whole call, and the render of every instance from what is on disk. The
    // two reads take the same lock, so the text a read hands back as its
    // revision is one no write is changing.

    /// The node's instance profiles, in the pool's own order: each one its id,
    /// the server profile its section names, and the model profiles its entry
    /// sections give it. The rows carry no revision: the pool is one text, so
    /// only [`Self::instance_profile`] answers it. A read: no action record, no
    /// service manager, no byte written.
    pub fn instance_profiles<W: AsRequester>(
        &self,
        who: &W,
    ) -> Result<Vec<pai_core::client::InstanceProfileInfo>, ManagerError> {
        let who = who.as_requester();
        self.authorize_as(&who, Action::Configure)?;
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        Ok(profile_rows(&self.pool_text()?))
    }

    /// The instance profile `profile`, with the text of `instance-profiles.ini`
    /// as its revision: the pair a page of the pool sends back to
    /// [`Self::set_instance_profile`], which refuses when the pool no longer
    /// holds it. A profile the pool has no section for is
    /// [`ManagerError::NoInstanceProfile`], naming it. A read, as
    /// [`Self::instance_profiles`].
    pub fn instance_profile<W: AsRequester>(
        &self,
        who: &W,
        profile: &str,
    ) -> Result<pai_core::client::InstanceProfileInfo, ManagerError> {
        let who = who.as_requester();
        self.authorize_as(&who, Action::Configure)?;
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        let text = self.pool_text()?;
        let row = profile_rows(&text)
            .into_iter()
            .find(|row| row.id == profile)
            .ok_or_else(|| ManagerError::NoInstanceProfile(profile.to_string()))?;
        Ok(pai_core::client::InstanceProfileInfo {
            revision: text,
            ..row
        })
    }

    /// Writes a new instance profile `profile` running the server profile
    /// `server_profile`: the section is appended in the pool's own shape
    /// ([`pai_core::instance_profiles::add_profile`]), every byte before it the
    /// file's. The checks answer first: an id the rule refuses is the keyed
    /// `instance-profile-bad-id`, a name the pool has is
    /// [`ManagerError::InstanceProfileExists`], a server profile the engine has
    /// no file for is [`ManagerError::NoServerProfile`]. The new profile is
    /// switched on and runs nothing: it writes no record, and no instance
    /// renders it.
    pub fn create_instance_profile<W: AsRequester>(
        &self,
        who: &W,
        profile: &str,
        server_profile: &str,
    ) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            if !crate::paths::valid_instance_id(profile) {
                return Err(one_value_refusal("instance-profile-bad-id", profile));
            }
            let text = self.pool_text()?;
            if profile_rows(&text).iter().any(|row| row.id == profile) {
                return Err(ManagerError::InstanceProfileExists(profile.to_string()));
            }
            let no_profile = || ManagerError::NoServerProfile(server_profile.to_string());
            let path = crate::paths::instance_ini(server_profile).ok_or_else(no_profile)?;
            if std::fs::symlink_metadata(&path).is_err() {
                return Err(no_profile());
            }
            self.write_pool(
                &text,
                &pai_core::instance_profiles::add_profile(&text, profile, server_profile),
            )
            .map(|_| ())
        };
        let answer = run();
        self.record_action("create_instance_profile", profile, Some(&who), &answer);
        answer
    }

    /// Removes the instance profile `profile`, with its entry sections: the
    /// entry sections are cut out first, then the profile's own section, every
    /// other byte of the file the file's own. A profile the pool has no section
    /// for is [`ManagerError::NoInstanceProfile`]. A profile is in use, and is
    /// [`ManagerError::InstanceProfileInUse`] naming the profile and the
    /// instance, two ways: an instance's record section names it, or the profile
    /// carries the id of an instance the record has, the profile of that
    /// instance's own, and the refusal names the profile the instance runs
    /// today. A profile in use is removed by switching its instances away and
    /// removing them first. The record is never written.
    pub fn delete_instance_profile<W: AsRequester>(
        &self,
        who: &W,
        profile: &str,
    ) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            let io = |e: std::io::Error| ManagerError::Io(e.kind());
            let text = self.pool_text()?;
            let row = profile_rows(&text)
                .into_iter()
                .find(|row| row.id == profile)
                .ok_or_else(|| ManagerError::NoInstanceProfile(profile.to_string()))?;
            let record =
                read_record(&crate::paths::engine_config_dir().join(INSTANCES_FILE)).map_err(io)?;
            // In use, in each of the two ways a profile is in use: an instance's
            // record section names it, and the profile carries the id of an
            // instance the record has, the profile the migration gave that
            // instance of its own. The refusal names the profile and the
            // instance, so the page can point at the instance and at the profile
            // it really runs.
            if let Some(holder) = instance_profile_holder(&record, profile) {
                return Err(ManagerError::InstanceProfileInUse(
                    profile.to_string(),
                    holder,
                ));
            }
            if let Some(owner) = instance_owning_profile(&record, profile) {
                let runs = instance_profile_of(&record, &owner)
                    .or_else(|| record_key(&record, &owner, "ServerProfile"))
                    .unwrap_or_else(|| profile.to_string());
                return Err(ManagerError::InstanceProfileInUse(runs, owner));
            }
            // The entry sections first, then the profile's own: every section
            // cut here is one the parse of the text returned listed, and the
            // pool is not moving under the `writing` lock.
            let mut next = text.clone();
            for entry in &row.entries {
                next = cut_section(&next, &format!("{profile}/{entry}"))?;
            }
            let next = cut_section(&next, profile)?;
            self.write_pool(&text, &next).map(|_| ())
        };
        let answer = run();
        self.record_action("delete_instance_profile", profile, Some(&who), &answer);
        answer
    }

    /// Copies the instance profile `from` to a new profile `profile`, with
    /// its entry sections. Guarded as a write of a served file is. The id
    /// and target checks are the create's, in its order; the source's own
    /// section is copied whole with its keys, its header renamed, and each
    /// entry section is copied whole, raw, in the source's order: the entry's
    /// keys are its state, and a copy carries it. No render, no record, no
    /// unit and no state: the copy names no instance, and an instance running
    /// the source keeps running it.
    pub fn copy_instance_profile<W: AsRequester>(
        &self,
        who: &W,
        from: &str,
        profile: &str,
    ) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            if !crate::paths::valid_instance_id(profile) {
                return Err(one_value_refusal("instance-profile-bad-id", profile));
            }
            let text = self.pool_text()?;
            let rows = profile_rows(&text);
            if rows.iter().any(|row| row.id == profile) {
                return Err(ManagerError::InstanceProfileExists(profile.to_string()));
            }
            let row = rows
                .iter()
                .find(|row| row.id == from)
                .ok_or_else(|| ManagerError::NoInstanceProfile(from.to_string()))?;
            // The source's sections, cut from no text: the copy appends to the
            // pool text it read under this lock, and every read of the pool
            // takes the same lock, so no write moves that text. The profile's
            // own section first, then its entries in the source's order.
            let mut next = text.clone();
            let own = pai_core::ini::text::section_text(&text, from)
                .ok_or(ManagerError::NoInstanceProfile(from.to_string()))?;
            next = append_pool_section(&next, &renamed_section(&own, profile));
            for entry in &row.entries {
                let body = pai_core::ini::text::section_text(&text, &format!("{from}/{entry}"))
                    .ok_or(ManagerError::Io(std::io::ErrorKind::UnexpectedEof))?;
                let name = format!("{profile}/{entry}");
                next = append_pool_section(&next, &renamed_section(&body, &name));
            }
            // No render: the new profile runs no instance, and the instance
            // that runs the source keeps running it.
            self.write_pool_quiet(&next)
        };
        let answer = run();
        self.record_action("copy_instance_profile", profile, Some(&who), &answer);
        answer
    }

    /// Gives the instance profile `profile` the model profile `model`: an
    /// empty `[<profile>/<model>]` entry appended in the pool's own shape
    /// ([`pai_core::instance_profiles::add_entry`]), answered with the running
    /// instances of the profile whose pair the write moved off the pair their
    /// launch recorded (IP3c). A profile the pool has no section for is
    /// [`ManagerError::NoInstanceProfile`], a model profile the pool has no
    /// section for is the keyed `no-model-profile-named`, naming the model, and
    /// a model the profile lists already is the keyed
    /// `instance-profile-model-listed`, naming the profile and the model: the
    /// entry's keys are its state, and the page adds what is not there. Every
    /// instance running the profile renders again, so the model lands in its
    /// render.
    pub fn add_instance_profile_model<W: AsRequester>(
        &self,
        who: &W,
        profile: &str,
        model: &str,
    ) -> Result<Vec<String>, ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<Vec<String>, ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            let text = self.pool_text()?;
            if !profile_rows(&text).iter().any(|row| row.id == profile) {
                return Err(ManagerError::NoInstanceProfile(profile.to_string()));
            }
            let io = |e: std::io::Error| ManagerError::Io(e.kind());
            let pool =
                read_record(&crate::paths::engine_config_dir().join("models.ini")).map_err(io)?;
            if !pool_holds(&pool, model) {
                return Err(one_value_refusal("no-model-profile-named", model));
            }
            // The profile lists the model already: the add is refused on its
            // own conflict, and no byte is written.
            if entry_models(&text, profile)
                .iter()
                .any(|entry| entry == model)
            {
                return Err(two_value_refusal(
                    "instance-profile-model-listed",
                    profile,
                    model,
                ));
            }
            // The instances the write touches: the ones running this profile
            // whose pair the write moved off the pair their launch recorded, in
            // the record's own order, beside the instances whose pair the fit
            // check refused (IP3c). The list is read from what the write left, so
            // no unit is asked for a pair that moved.
            self.write_pool(
                &text,
                &pai_core::instance_profiles::add_entry(&text, profile, model),
            )
            .and_then(|_| self.drift_running_using_profile(profile))
        };
        let answer = run();
        self.record_action("add_instance_profile_model", profile, Some(&who), &answer);
        answer
    }

    /// Takes the model profile `model` away from the instance profile
    /// `profile`: its entry section goes, every other byte of the file the
    /// file's own, and the answer is the running instances of the profile whose
    /// pair the write moved off the pair their launch recorded (IP3c). A profile
    /// the pool has no section for is
    /// [`ManagerError::NoInstanceProfile`]; an entry the profile does not list
    /// is the keyed `no-instance-profile-model`, naming the profile and the
    /// model. Every instance running the profile renders again, so the model
    /// leaves its render.
    pub fn remove_instance_profile_model<W: AsRequester>(
        &self,
        who: &W,
        profile: &str,
        model: &str,
    ) -> Result<Vec<String>, ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<Vec<String>, ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            let text = self.pool_text()?;
            if !profile_rows(&text).iter().any(|row| row.id == profile) {
                return Err(ManagerError::NoInstanceProfile(profile.to_string()));
            }
            let section = format!("{profile}/{model}");
            let next = match pai_core::ini::text::delete_section(&text, &section) {
                Some(next) => next,
                // No entry of that model: the profile does not list it, so the
                // remove is refused on its own conflict.
                None => {
                    return Err(two_value_refusal(
                        "no-instance-profile-model",
                        profile,
                        model,
                    ))
                }
            };
            self.write_pool(&text, &next)
                .and_then(|_| self.drift_running_using_profile(profile))
        };
        let answer = run();
        self.record_action(
            "remove_instance_profile_model",
            profile,
            Some(&who),
            &answer,
        );
        answer
    }

    /// Writes the keys of the entry `[profile/model]` of `instance-profiles.ini`
    /// as `values` sent them, and answers the running instances that use
    /// `profile`: the form checks of the moved keys ride this write
    /// (`ARCHITECTURE.md` "Instance profiles"), the fit of the render they move is
    /// every instance's own ("Convergence": the entry that stops one instance's
    /// render lands, and that instance keeps its last good pair). The entry holds
    /// exactly the keys sent, in the order they were sent: the keys it carried and
    /// are not sent are gone, the model profile's own keys, which the entry does
    /// not name, are not touched, and the entry stands directly under the
    /// profile's own section.
    ///
    /// Every check answers before a byte is written, in this order: a profile
    /// the pool has no section for is [`ManagerError::NoInstanceProfile`], a
    /// pool no longer holding `revision`, the text the caller read, is
    /// [`ManagerError::Changed`], an entry the profile does not list is the
    /// keyed `no-instance-profile-model` naming the profile and the model, a key
    /// that stays in the model profile is the keyed `model-profile-stay-key`
    /// naming it, and the value rules of the keys
    /// (`not-whole-number-in-range`, `tensor-split-format`,
    /// `override-no-device`, the enums and the numbers, [`pai_core::checks`]'s
    /// own). Guarded as every write of a served file is: `configure` first, the
    /// `As` form answered only to the bff's account for a current member of
    /// `pai`, one record of the action log per call.
    /// Answers [`Affected`], as every save that re-renders does: the running
    /// instances the write left drifting, beside the rows of every instance whose
    /// pair the write did not leave ([`Self::affected_of`], IP3c). The save names
    /// no instance (IP4): the entry belongs to the profile, the page's revision
    /// is the pool's text, and every instance running the profile renders again.
    pub fn set_instance_profile_model<W: AsRequester>(
        &self,
        who: &W,
        profile: &str,
        model: &str,
        values: &[(String, String)],
        revision: &str,
    ) -> Result<Affected, ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<Affected, ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            let text = self.pool_text()?;
            let row = profile_rows(&text)
                .into_iter()
                .find(|row| row.id == profile)
                .ok_or_else(|| ManagerError::NoInstanceProfile(profile.to_string()))?;
            if revision != text {
                return Err(ManagerError::Changed);
            }
            if !row.entries.iter().any(|entry| entry == model) {
                return Err(two_value_refusal(
                    "no-instance-profile-model",
                    profile,
                    model,
                ));
            }
            check_entry_values(values).map_err(ManagerError::Invalid)?;
            // The entry as the page sent it, written whole: every key it
            // carried and is not sent is gone, and the entry holds exactly what
            // the page sent, in the order it sent it.
            let next = write_entry_section(&text, profile, model, values);
            // The entry's values re-render every instance whose profile names
            // the model: the answer is the running ones the write left drifting,
            // beside the rows the write's pass left (IP3c).
            self.write_pool(&text, &next).and_then(|pass| {
                Ok(self.affected_of(&pass, self.drift_running_using_profile(profile)?))
            })
        };
        let answer = run();
        self.record_action(
            "set_instance_profile_model",
            &format!("{profile}/{model}"),
            Some(&who),
            &answer,
        );
        answer
    }

    /// Points the instance profile `profile` at the server profile
    /// `server_profile`, and answers [`Affected`] as every save that re-renders
    /// does (IP4a): the running instances the switch left running a pair that is
    /// not the pair on disk, beside the rows of the instances whose render the
    /// switch stopped ([`Self::affected_of`], IP3c). The switch moves the ground
    /// of every entry of the profile at once, so every instance running it
    /// renders again, and the fit of each render is that instance's own
    /// (`ARCHITECTURE.md` "Convergence": a switch that takes a card an entry
    /// names away lands, and the instances whose render stops on the card keep
    /// their last good pair). A profile the pool has no section for is
    /// [`ManagerError::NoInstanceProfile`], a server profile the node has no
    /// file for is [`ManagerError::NoServerProfile`]. The switch carries no
    /// revision: it is one set-like change, refused on its own conflict.
    /// Guarded as every write of a served file is.
    pub fn set_instance_profile_server<W: AsRequester>(
        &self,
        who: &W,
        profile: &str,
        server_profile: &str,
    ) -> Result<Affected, ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<Affected, ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            let text = self.pool_text()?;
            if !profile_rows(&text).iter().any(|row| row.id == profile) {
                return Err(ManagerError::NoInstanceProfile(profile.to_string()));
            }
            let no_profile = || ManagerError::NoServerProfile(server_profile.to_string());
            let path = crate::paths::instance_ini(server_profile).ok_or_else(no_profile)?;
            if std::fs::symlink_metadata(&path).is_err() {
                return Err(no_profile());
            }
            self.write_pool(
                &text,
                &pai_core::ini::text::set_key(&text, profile, "ServerProfile", server_profile),
            )
            .and_then(|pass| {
                Ok(self.affected_of(&pass, self.drift_running_using_profile(profile)?))
            })
        };
        let answer = run();
        self.record_action("set_instance_profile_server", profile, Some(&who), &answer);
        answer
    }

    /// Points the instance `instance` at the instance profile `profile`: the
    /// `InstanceProfile` key of its section of the record, every other key of
    /// the section and every other section kept. The checks answer in this
    /// order: an instance the record has no section for is
    /// [`ManagerError::NoInstance`], a profile the pool has no section for is
    /// [`ManagerError::NoInstanceProfile`], and a pool no longer holding
    /// `revision`, the text the caller read, is [`ManagerError::Changed`]. The
    /// pool is never written: the switch moves the record's pointer, and every
    /// instance renders again from what is on disk.
    pub fn set_instance_profile<W: AsRequester>(
        &self,
        who: &W,
        instance: &str,
        profile: &str,
        revision: &str,
    ) -> Result<Affected, ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<Affected, ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            let record = self.read_record_locked()?;
            if !pai_core::ini::parse_sections(&record)
                .iter()
                .any(|section| section.id == instance)
            {
                return Err(ManagerError::NoInstance);
            }
            let pool = self.pool_text()?;
            if !profile_rows(&pool).iter().any(|row| row.id == profile) {
                return Err(ManagerError::NoInstanceProfile(profile.to_string()));
            }
            if pool != revision {
                return Err(ManagerError::Changed);
            }
            let text =
                pai_core::ini::text::set_key(&record, instance, INSTANCE_PROFILE_KEY, profile);
            // The record carries the instance profile and the address of every
            // instance, so the write re-renders them all: the answer is the
            // states it left (IP3c), read from the pass it ran.
            self.write_record(&text)
                .and_then(|pass| Ok(self.affected_of(&pass, self.drift_running()?)))
        };
        let answer = run();
        self.record_action("set_instance_profile", instance, Some(&who), &answer);
        answer
    }

    /// Sets the address of the instance `instance` to `host` and `port`: the
    /// record's `Hostname` and `Port` keys of its section, every other key of
    /// the section and every other section kept. The empty host removes the
    /// key, leaving the server profile's bind; the port travels as text, as
    /// every address text on the wire. The checks answer in this order: an
    /// instance the record has no section for is [`ManagerError::NoInstance`],
    /// a record no longer holding `revision` is [`ManagerError::Changed`], a
    /// port outside [`pai_core::checks::PORT`] is the keyed
    /// `not-whole-number-in-range`, a host carrying a control character is
    /// [`ManagerError::UnresolvableHost`], and an address another instance or
    /// the web service holds is [`ManagerError::AddressInUse`] naming who holds it
    /// ([`Self::check_binding`], the instance itself out of the walk: it moves
    /// its own address). The instance's render is written from what is on disk.
    pub fn set_instance_address<W: AsRequester>(
        &self,
        who: &W,
        instance: &str,
        host: &str,
        port: &str,
        revision: &str,
    ) -> Result<Affected, ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<Affected, ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            let record = self.read_record_locked()?;
            if !pai_core::ini::parse_sections(&record)
                .iter()
                .any(|section| section.id == instance)
            {
                return Err(ManagerError::NoInstance);
            }
            if record != revision {
                return Err(ManagerError::Changed);
            }
            let port =
                pai_core::ini::parse_int_in(port, &pai_core::checks::PORT).ok_or_else(|| {
                    ManagerError::Invalid(pai_core::checks::Refusal {
                        key: "not-whole-number-in-range",
                        values: vec![
                            "Port".to_string(),
                            port.trim().to_string(),
                            pai_core::checks::PORT.start().to_string(),
                            pai_core::checks::PORT.end().to_string(),
                        ],
                    })
                })?;
            let host = host.trim();
            // A host is written as the record's `Hostname`, so a control
            // character in it forges a line of the file: it is no host of this
            // node, and the answer lands before anything is written.
            if host.chars().any(char::is_control) {
                return Err(ManagerError::UnresolvableHost(host.to_string()));
            }
            // The binding the instance would have after the write: its own
            // server profile's address, as `instance_cfg` reads it, with the
            // `Hostname` and `Port` this verb writes over it. An empty host
            // removes the record's key, leaving the profile's bind.
            let mut cfg = crate::server_cfg::from_text(
                &profile_file(instance)
                    .and_then(|file| std::fs::read_to_string(file).ok())
                    .unwrap_or_default(),
            );
            if !host.is_empty() {
                cfg.hostname = Some(host.to_string());
            }
            cfg.port = Some(port);
            let mine = binding(&cfg, &self.resolver).map_err(ManagerError::UnresolvableHost)?;
            self.check_binding(&mine, |id| id == instance)?;
            let text = pai_core::ini::text::set_key(&record, instance, "Port", &port.to_string());
            let text = if host.is_empty() {
                pai_core::ini::text::remove_key(&text, instance, "Hostname")
            } else {
                pai_core::ini::text::set_key(&text, instance, "Hostname", host)
            };
            // The record carries the instance profile and the address of every
            // instance, so the write re-renders them all: the answer is the
            // states it left (IP3c), read from the pass it ran.
            self.write_record(&text)
                .and_then(|pass| Ok(self.affected_of(&pass, self.drift_running()?)))
        };
        let answer = run();
        self.record_action("set_instance_address", instance, Some(&who), &answer);
        answer
    }

    /// The text of `instances.ini`, read as [`read_record`] does: a file that
    /// is not there reads as an empty one, and a read that fails for any other
    /// reason is the verb's own answer before anything is written. Taken under
    /// the `writing` lock, as every write of the record is.
    fn read_record_locked(&self) -> Result<String, ManagerError> {
        read_record(&crate::paths::engine_config_dir().join(INSTANCES_FILE))
            .map_err(|e| ManagerError::Io(e.kind()))
    }

    /// The text of `instance-profiles.ini`, as [`read_pool`] reads it.
    fn pool_text(&self) -> Result<String, ManagerError> {
        read_pool().map_err(|e| ManagerError::Io(e.kind()))
    }

    /// The instances a write of the instance profile `profile` answers as
    /// `running` (IP3c, `ARCHITECTURE.md` "Convergence"): the ones of the record
    /// running that profile, in the record's own order, the write left running a
    /// pair that is not the pair their launch recorded. The compare comes BEFORE
    /// the state is asked, so a write asks the service manager for nothing past a
    /// pair that moved (the IP3c review): an instance that recorded no pair
    /// applied nothing, and one that stands on its record drifts by no
    /// definition. Call it after the write, on the pair the write left.
    fn drift_running_using_profile(&self, profile: &str) -> Result<Vec<String>, ManagerError> {
        let record = self.read_record_locked()?;
        Ok(pai_core::ini::parse_sections(&record)
            .into_iter()
            .map(|section| section.id)
            .filter(|id| instance_profile_of(&record, id).as_deref() == Some(profile))
            .filter(|id| self.pair_moved(id))
            .filter(|id| {
                self.service
                    .status(id)
                    .is_ok_and(|state| state == UnitState::Active)
            })
            .collect())
    }

    /// Writes the instance profile pool `text` and renders nothing: the
    /// served-file write on its own, for the one pool write that can change no
    /// instance's render, [`Self::copy_instance_profile`]: its new sections are
    /// a profile no instance runs.
    fn write_pool_quiet(&self, text: &str) -> Result<(), ManagerError> {
        let io = |e: std::io::Error| ManagerError::Io(e.kind());
        let dir = crate::paths::engine_config_dir();
        std::fs::create_dir_all(&dir).map_err(io)?;
        write_served_file(
            &dir.join(INSTANCE_PROFILES_FILE),
            &ConfigFile::Models,
            text,
            &*self.group,
        )
        .map_err(io)
    }

    /// Writes the instance profile pool `text` over the text `before` the file
    /// holds, then renders every instance from what is on disk: the entry
    /// sections of a profile ARE its model list, so a pool write changes what
    /// the instances running it render. A text that did not change writes no
    /// byte and renders nothing. The write stands whatever the renders do, as
    /// [`Self::check_and_write`]'s does: the caller is told the first failure
    /// in id order.
    fn write_pool(&self, before: &str, text: &str) -> RenderPass {
        if text == before {
            return Ok(Vec::new());
        }
        self.write_served(INSTANCE_PROFILES_FILE, text)
    }

    /// Writes the record `instances.ini` whole, then renders every instance:
    /// the record carries the address an instance listens on and the instance
    /// profile it runs, so a record write changes what the instance renders.
    fn write_record(&self, text: &str) -> RenderPass {
        self.write_served(INSTANCES_FILE, text)
    }

    /// The write every served file of the engine's config directory shares:
    /// the directory made, the file written as [`write_served_file`] writes it,
    /// and the render of every instance after it.
    fn write_served(&self, name: &str, text: &str) -> RenderPass {
        let io = |e: std::io::Error| ManagerError::Io(e.kind());
        let dir = crate::paths::engine_config_dir();
        std::fs::create_dir_all(&dir).map_err(io)?;
        write_served_file(&dir.join(name), &ConfigFile::Models, text, &*self.group).map_err(io)?;
        self.render_pass()
    }

    /// Creates the instance `id` of the engine `engine`, running the server
    /// profile `server_profile`. Nothing is asked of the service manager: the
    /// Linux unit template and the Windows `sync` serve an id the record names,
    /// and a new id has no wants link, so it does not start at boot. Every
    /// check answers before a byte is written, in this order: the engine
    /// ([`known_engine`]), the id rule (a `../x` id writes and deletes nothing,
    /// so it is judged before any path is built), the record read with
    /// [`read_record`] (a record that cannot be read is [`ManagerError::Io`],
    /// never an empty one), a record section with the id (`instance-exists`),
    /// the pool read with [`read_pool`] and the instance profile: a name the
    /// pool has no section for is [`ManagerError::NoInstanceProfile`] (IP4b: the
    /// third name is an INSTANCE profile), and the server profile that
    /// profile's section names is judged as it was: a name the id rule refuses
    /// or one with no `server@<name>.ini` is [`ManagerError::NoServerProfile`], a
    /// profile a section of the record runs at the profile's own address is
    /// [`ManagerError::ServerProfileInUse`] naming that instance, and the
    /// profile's own address is judged against those instances and against the
    /// web service ([`Self::check_binding`]; the new instance is out of the walk
    /// because the record does not name it yet, and an instance with a
    /// `Hostname` or a `Port` in its section is out too, because it binds an
    /// address of its own: that is why any number of instances may run one
    /// instance profile, each on a port of its own), and the port the node
    /// proposes ([`proposed_port`]), a node holding every port of its range
    /// refused `no-free-port`. Then, in this order: the
    /// `assignments.ini` section of an id used before, so the new instance is
    /// assigned nothing; its render `models@<id>.ini`, of the pool as it is on
    /// disk; and the record last, naming the instance profile and
    /// carrying the `Port` proposed and no `Hostname`, so the profile's bind
    /// stands, and the render pass every write runs, which
    /// gives the new instance the `launch@<id>.ini` half of its pair beside the
    /// record's address (IP3c), a pair the pass cannot write being the create's
    /// own failure. The port is answered
    /// before any byte is written, and is the call's answer (IP4b), so the
    /// caller knows the address the instance is at without reading the record. A
    /// create that wrote the render and failed on the record leaves a file no
    /// instance reads, never an instance with no render. Guarded as a write of
    /// a served file is: the principal gate and `configure`, the `writing`
    /// lock, one action record for the whole call.
    pub fn create_instance<W: AsRequester>(
        &self,
        who: &W,
        engine: &str,
        id: &str,
        instance_profile: &str,
    ) -> Result<u16, ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<u16, ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            known_engine(engine)?;
            if !crate::paths::valid_instance_id(id) {
                return Err(one_value_refusal("instance-bad-id", id));
            }
            let io = |e: std::io::Error| ManagerError::Io(e.kind());
            let dir = crate::paths::engine_config_dir();
            let record = read_record(&dir.join(INSTANCES_FILE)).map_err(io)?;
            if pai_core::ini::parse_sections(&record)
                .iter()
                .any(|section| section.id == id)
            {
                return Err(one_value_refusal("instance-exists", id));
            }
            // The third name is an INSTANCE profile (IP4b): the pool's section
            // for it names the server profile the instance runs, one step
            // behind. A name the pool has no section for, and a section naming
            // no server profile, leave the create with nothing to run.
            let pool = read_pool().map_err(io)?;
            let found = pai_core::instance_profiles::parse(&pool)
                .into_iter()
                .find(|profile| profile.id == instance_profile)
                .ok_or_else(|| ManagerError::NoInstanceProfile(instance_profile.to_string()))?;
            let server_profile = found
                .server_profile
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty())
                .ok_or_else(|| ManagerError::NoServerProfile(instance_profile.to_string()))?;
            let no_profile = || ManagerError::NoServerProfile(server_profile.clone());
            let path = crate::paths::instance_ini(&server_profile).ok_or_else(no_profile)?;
            if std::fs::symlink_metadata(&path).is_err() {
                return Err(no_profile());
            }
            if let Some(holder) = record_profile_owner(&record, &server_profile) {
                return Err(ManagerError::ServerProfileInUse(
                    server_profile.clone(),
                    holder,
                ));
            }
            let contents = std::fs::read_to_string(&path).map_err(io)?;
            let mine = binding(&crate::server_cfg::from_text(&contents), &self.resolver)
                .map_err(ManagerError::UnresolvableHost)?;
            // The profile's address is judged against the instances that bind
            // it, the ones taking their address from a server profile. An
            // instance with a `Hostname` or a `Port` in its record section binds
            // an address of its own, so it is out of the walk: instances share a
            // server profile by sitting on their own ports, which is why the
            // create proposes one.
            self.check_binding(&mine, |other| record_owns_address(&record, other))?;
            // The port answered before any write: a node holding every port of
            // the range refuses, and the create leaves no file behind.
            let port = proposed_port(self.port_search.0, self.port_search.1)?;
            // A section an id used before left behind: the new instance is
            // assigned nothing, so its render starts no model profile.
            pai_core::ini::delete_section(&dir.join(ASSIGNMENTS_FILE), id).map_err(io)?;
            let on_disk = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();
            let render = pai_core::assign::render(
                &on_disk("models.ini"),
                &on_disk("models.ini.disabled"),
                None,
            );
            write_served_file(
                &crate::paths::instance_models(id).ok_or(ManagerError::InvalidId)?,
                &ConfigFile::Models,
                &render,
                &*self.group,
            )
            .map_err(io)?;
            // The record last, and appended: every byte of the file, the notes
            // of the other sections included, stays as it was. The new instance
            // is given an address of the node's own, never the address of the
            // server profile it runs, which several instances share: the first
            // port from [`FIRST_PROPOSED_PORT`] up that no section of the record
            // holds. `SetInstanceAddress` is the verb that moves it later.
            append_record(
                &dir.join(INSTANCES_FILE),
                &record,
                id,
                instance_profile,
                port,
            )
            .map_err(io)?;
            // The pair the new instance converges to (IP3c): the record names it
            // now, so the pass every write runs gives it both files, the launch
            // carrying the record's address. Every other instance's pair stands
            // and is left byte for byte; a pair the pass cannot write is the
            // create's own failure.
            let pass = self.render_all_locked();
            if let Some(err) = self.write_failure(&pass) {
                return Err(err);
            }
            Ok(port)
        };
        let answer = run();
        self.record_action("create_instance", id, Some(&who), &answer);
        answer
    }

    /// Removes the instance `id`: its unit, its `assignments.ini` section, the
    /// whole pair it renders (`models@<id>.ini` beside `launch@<id>.ini`) with
    /// the `applied` record its launch left in its state folder, and its record
    /// section, in that order, the record last (IP3c: the pair is ONE state, so
    /// the removal takes both of its files, and a recreation of the id starts
    /// with no record of the pair the old install applied). The server profile
    /// it runs, the pool, its state directory with the snapshots it holds and
    /// every other instance's pair stay. The checks answer before anything is
    /// removed:
    /// the id rule ([`ManagerError::InvalidId`], before any path), the record
    /// read with [`read_record`] (a record that cannot be read is
    /// [`ManagerError::Io`], one with no section of the id
    /// [`ManagerError::NoInstance`]), [`PROTECTED_INSTANCE`] (even stopped), and
    /// the unit's state read from the service manager: anything but inactive or
    /// failed is `instance-running`, and a state that cannot be read is the
    /// service manager's own failure, never a state seen stopped. Every step is
    /// idempotent, so a remove that failed partway leaves the instance listed
    /// and stopped, and the next one finishes it. Guarded as a write of a
    /// served file is: the principal gate and `configure`, the `writing` lock,
    /// one action record for the whole call.
    pub fn remove_instance<W: AsRequester>(&self, who: &W, id: &str) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            // A poisoned lock is taken over: it guards no data of its own.
            let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
            valid_id(id)?;
            let io = |e: std::io::Error| ManagerError::Io(e.kind());
            let dir = crate::paths::engine_config_dir();
            let record = read_record(&dir.join(INSTANCES_FILE)).map_err(io)?;
            if !pai_core::ini::parse_sections(&record)
                .iter()
                .any(|section| section.id == id)
            {
                return Err(ManagerError::NoInstance);
            }
            if id == PROTECTED_INSTANCE {
                return Err(one_value_refusal("instance-protected", id));
            }
            let state = self.service.status(id).map_err(ManagerError::Service)?;
            if !matches!(state, UnitState::Inactive | UnitState::Failed) {
                return Err(one_value_refusal("instance-running", id));
            }
            self.service.remove(id).map_err(ManagerError::Service)?;
            pai_core::ini::delete_section(&dir.join(ASSIGNMENTS_FILE), id).map_err(io)?;
            // The whole pair, and the record of the pair the last launch applied
            // (IP3c). A file of the three missing is the removal already done.
            let mut pair = vec![
                crate::paths::instance_models(id).ok_or(ManagerError::InvalidId)?,
                crate::launch::instance_launch_ini(id).ok_or(ManagerError::InvalidId)?,
            ];
            pair.extend(applied_file(id));
            for path in pair {
                match std::fs::remove_file(&path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    done => done.map_err(io),
                }?;
            }
            pai_core::ini::delete_section(&dir.join(INSTANCES_FILE), id).map_err(io)?;
            Ok(())
        };
        let answer = run();
        self.record_action("remove_instance", id, Some(&who), &answer);
        answer
    }

    /// Every instance the record lists, in id order as `list_instances`
    /// answers it, each with the server profile its section names (the
    /// `ServerProfile` of the section, verbatim: the file the launch reads is
    /// `server@<name>.ini`), the port and the tags of that profile, and the
    /// tags of its own section. A
    /// section naming no profile answers no name; a profile whose file is not
    /// there answers port 0 and no profile tags: the web
    /// UI shows 0 as "no port", and a read of a missing file is not a failure
    /// that drops the list. Answered to any caller, as `list_instances`
    /// is: the tags are what the web UI shows, and the manager passes none of
    /// them to the engine.
    pub fn instances(&self) -> Vec<pai_core::client::InstanceInfo> {
        use pai_core::client::InstanceInfo;
        let dir = crate::paths::engine_config_dir();
        let record = read_record(&dir.join(INSTANCES_FILE)).unwrap_or_default();
        let list = |value: Option<&String>| {
            value
                .map(|v| {
                    v.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut rows: Vec<InstanceInfo> = pai_core::ini::parse_sections(&record)
            .iter()
            .map(|section| {
                let (port, profile_tags) = match instance_cfg(&section.id) {
                    Some(cfg) => (u16::try_from(cfg.port_or_default()).unwrap_or(0), cfg.tags),
                    None => (0, Vec::new()),
                };
                let (converged, reason) = self.convergence(&section.id);
                InstanceInfo {
                    id: section.id.clone(),
                    engine: ENGINE.to_string(),
                    server_profile: crate::paths::instance_server_profile(&section.id),
                    port,
                    profile_tags,
                    instance_tags: list(section.keys.get("Tags")),
                    // No `Label` key, or a blank one, is no label: the UI
                    // falls back to the engine and the port of this row.
                    label: section
                        .keys
                        .get("Label")
                        .map(|v| v.trim().to_string())
                        .filter(|v| !v.is_empty()),
                    // The pair of the apply (IP3c): converged and its reason are
                    // the rows the last render pass left in memory, so the read
                    // measures the pair against no card (review 8). The drift is
                    // the pair on disk against the `applied` record its launch
                    // left (IP3c review 4), a compare of files that reaches no
                    // service manager and asks the GPU probe nothing.
                    converged,
                    reason: reason.map(|refusal| pai_core::client::NotConverged {
                        key: refusal.key.to_string(),
                        args: refusal.values,
                    }),
                    pending_restart: self.pending_restart(&section.id),
                    // The two fields appended to the row (IP4b): the instance
                    // profile the section names, and the host its section sets.
                    // A section with neither, as a migration of an older record
                    // leaves them, answers none.
                    instance_profile: instance_profile_of(&record, &section.id),
                    hostname: section
                        .keys
                        .get("Hostname")
                        .map(|v| v.trim().to_string())
                        .filter(|v| !v.is_empty()),
                }
            })
            .collect();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        rows
    }

    /// Every server profile the engine's directory holds: each
    /// `server@<id>.ini` whose name the id rule accepts, in id order, with the
    /// port and the tags of its `[Server]` section (the port as the record of
    /// the instance running the profile holds it, the address the migration
    /// moved out of the file, as `instances` reads it; no `Port` in the record
    /// and no `Port` in the file is `server_cfg::DEFAULT_PORT`) and the first
    /// section of `instances.ini` naming it, `None` when no section does: the
    /// profile serves nothing. A section naming a file that is not there is no
    /// row, the list being of files. Answered to any caller, as `instances` is:
    /// it names ports, tags and the unused profiles of a node, the class of data
    /// `instances` already gives for the used ones, and reads nothing past the
    /// `[Server]` keys, so a key the file holds stays behind a read of it.
    pub fn server_profiles(&self) -> Vec<pai_core::client::ServerProfileInfo> {
        use pai_core::client::ServerProfileInfo;
        let dir = crate::paths::engine_config_dir();
        let record = read_record(&dir.join(INSTANCES_FILE)).unwrap_or_default();
        let mut rows: Vec<ServerProfileInfo> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|entry| {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        let id = match config_file(&name) {
                            Some(ConfigFile::ServerProfile(id)) => id,
                            _ => return None,
                        };
                        // A file that cannot be read answers as `instances`
                        // answers a profile whose file is not there.
                        let cfg = std::fs::read_to_string(dir.join(&name))
                            .ok()
                            .map(|text| crate::server_cfg::from_text(&text));
                        let (port, tags) = match cfg {
                            Some(cfg) => (
                                u16::try_from(cfg.port.unwrap_or(crate::server_cfg::DEFAULT_PORT))
                                    .unwrap_or(0),
                                cfg.tags,
                            ),
                            None => (0, Vec::new()),
                        };
                        let instance = record_holder(&record, &id);
                        // The port as the record of the instance running the
                        // profile holds it: the migration moves the address
                        // into the record and takes it out of the profile file,
                        // so a list reading the file alone answers
                        // `server_cfg::DEFAULT_PORT` of a migrated install. A
                        // profile no instance uses, and a record carrying no
                        // `Port`, keep the file's own answer.
                        let port = instance
                            .as_deref()
                            .and_then(|holder| crate::paths::instance_address(holder).1)
                            .and_then(|port| u16::try_from(port).ok())
                            .unwrap_or(port);
                        Some(ServerProfileInfo {
                            id: id.clone(),
                            engine: ENGINE.to_string(),
                            port,
                            tags,
                            instance,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        rows
    }

    /// Writes the render of every instance and answers with the instances
    /// whose render did not land, in id order, each with the failure that
    /// stopped it: empty when every render landed. The manager's start calls
    /// this and hands the result to [`Self::report_render_failures`], so a
    /// render that failed at the start is not silent.
    ///
    /// Every instance the record lists is rendered, an instance given no model
    /// profile included: its render is the pool's global section and no model
    /// profile section, the file a new instance assigned nothing is given
    /// (`ARCHITECTURE.md`, "Instance profiles", the placement and memory
    /// amendment).
    pub fn render_failures(&self) -> Vec<(String, ManagerError)> {
        // The same lock a write holds, so a render never mixes with a write.
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        // The start renders every instance against the pair on disk, its last
        // good pair: sources that grew while the manager was away keep their
        // bytes and are reported, a pair that stands is skipped, so a start over
        // a tree that stands writes no byte and asks the GPUs for nothing
        // (IP3c, IP2 review D1).
        self.render_all_locked()
    }

    /// One record per render that did not land, in the action log's own
    /// format: the action is `render`, the target is the instance, the caller
    /// and the principal are the bare `-` (this is the manager acting for
    /// itself at its start, no one asked it), the outcome is the failure's
    /// kind name, quoted as a failed guarded call quotes it. The line goes to
    /// the log the service installed (on Linux stderr, which the unit sends
    /// to the journal; on Windows the service's log file). A log, like the
    /// action records: the manager's own words, not user text.
    pub fn report_render_failures(&self, failures: &[(String, ManagerError)]) {
        for (id, err) in failures {
            self.action_log.record(&format!(
                "action=\"render\" target={id:?} caller=- principal=- outcome={:?}",
                error_kind(err)
            ));
        }
    }

    /// The manager's start, before any service answers: migrate the model
    /// data model of an install of the old files (`pai_core::migrate_model`,
    /// with its `<name>.2.99.0` backups, the writes in the crash-safe order
    /// and the stray `assignments.ini` rule), reporting what did not land, then
    /// write the render of every instance and answer with the ones that did not
    /// land, for the caller to report as it logs (Linux `dbus::serve`, Windows
    /// `windows::entry`).
    pub fn start_up(&self) -> Vec<(String, ManagerError)> {
        self.report_migrations(&self.migrate_model());
        self.render_failures()
    }

    /// The model data model migration of [`pai_core::migrate_model`], taken
    /// under the `writing` lock a write holds, so no service answers between
    /// two of its writes and none reads a half-migrated file. Answers the
    /// failures, in the order they happened, empty when the start migrated the
    /// tree (a tree already migrated answers none).
    fn migrate_model(&self) -> Vec<MigrationFailure> {
        let _guard = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        self.migrate_model_files()
    }

    /// The migration over the engine's config folder: every file read once,
    /// [`pai_core::migrate_model`] answering the new texts, and every write
    /// through the manager's own writer, so a migrated file keeps the mode of
    /// the file it replaces and the group of an instance file. The writes go in
    /// the order `migrate_model` documents, and the two files the new shape has
    /// no place for go last, once the record is the migration's own. A read that
    /// fails stops the migration with the reason: the tree is left as it stands
    /// and the next start reads it again, and a tree that is already migrated
    /// answers the same bytes it has.
    fn migrate_model_files(&self) -> Vec<MigrationFailure> {
        let dir = crate::paths::engine_config_dir();
        let mut failures: Vec<MigrationFailure> = Vec::new();
        let Some(instances) = read_tree(&dir, INSTANCES_FILE, &mut failures) else {
            return failures;
        };
        let Some(assignments) = read_tree(&dir, ASSIGNMENTS_FILE, &mut failures) else {
            return failures;
        };
        let Some(models) = read_tree(&dir, "models.ini", &mut failures) else {
            return failures;
        };
        let Some(models_disabled) = read_tree(&dir, "models.ini.disabled", &mut failures) else {
            return failures;
        };
        let Some(instance_profiles) = read_tree(&dir, INSTANCE_PROFILES_FILE, &mut failures) else {
            return failures;
        };
        // Every server profile the instances run, read as it stands: the
        // migration answers each one with its address out. A profile whose file
        // is not there is left out: the migration creates no file.
        let mut server_profiles: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        for name in server_profile_names() {
            let file = format!("server@{name}.ini");
            match std::fs::read_to_string(dir.join(&file)) {
                Ok(text) => {
                    server_profiles.insert(name, text);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    failures.push(MigrationFailure {
                        name: file,
                        error: e.kind(),
                    });
                    return failures;
                }
            }
        }
        let old = pai_core::migrate_model::OldFiles {
            models,
            models_disabled,
            instance_profiles,
            assignments,
            instances,
            server_profiles,
        };
        let new = pai_core::migrate_model::migrate_model(&old);
        // The two files the new shape has no place for: the record the migration
        // wrote names an instance profile for every instance, so the address sits
        // in it and the pool is merged. A record with no section names no instance
        // and is migrated the same: a stray `assignments.ini` is a file nothing
        // reads.
        let retired: &[&str] = if record_is_migrated(&new.instances) {
            &[ASSIGNMENTS_FILE, "models.ini.disabled"]
        } else {
            &[]
        };
        // The backup of every file this start changes, written before any of
        // them is written, so the install's own text survives the migration.
        // Each one carries the text the start writes for it, and a file that
        // already holds it is no old-shape file: the migration is idempotent, so
        // a start of a migrated tree reads the new shape and answers the same
        // bytes, and `instance-profiles.ini` has no old text at all (IP2 review
        // D1). The two retired files are changed by being gone, so they are
        // listed with no text of theirs.
        let mut changes: Vec<(String, Option<&str>)> = vec![
            (
                INSTANCE_PROFILES_FILE.to_string(),
                Some(&new.instance_profiles),
            ),
            (INSTANCES_FILE.to_string(), Some(&new.instances)),
            ("models.ini".to_string(), Some(&new.models)),
        ];
        changes.extend(
            new.server_profiles
                .iter()
                .map(|(name, text)| (format!("server@{name}.ini"), Some(text.as_str()))),
        );
        changes.extend(retired.iter().map(|name| (name.to_string(), None)));
        for (name, text) in &changes {
            if let Err(e) = ensure_backup(&dir, name, *text) {
                failures.push(MigrationFailure {
                    name: name.clone(),
                    error: e.kind(),
                });
            }
        }
        if failures.is_empty() {
            // The writes, in the order `migrate_model` documents: the pool of
            // instance profiles, the record, the merged pool of model profiles,
            // every server profile with the address out of it.
            write_migrated(
                &dir,
                &ConfigFile::Models,
                INSTANCE_PROFILES_FILE,
                &new.instance_profiles,
                &*self.group,
                &mut failures,
            );
            write_migrated(
                &dir,
                &ConfigFile::Models,
                INSTANCES_FILE,
                &new.instances,
                &*self.group,
                &mut failures,
            );
            write_migrated(
                &dir,
                &ConfigFile::Models,
                "models.ini",
                &new.models,
                &*self.group,
                &mut failures,
            );
            for (name, text) in &new.server_profiles {
                write_migrated(
                    &dir,
                    &ConfigFile::ServerProfile(name.clone()),
                    &format!("server@{name}.ini"),
                    text,
                    &*self.group,
                    &mut failures,
                );
            }
        }
        // The removals, last: the record, the instance profiles and the pool are
        // written, so `assignments.ini` and `models.ini.disabled` are files
        // nothing reads. A file that is not there is already gone.
        if failures.is_empty() {
            for name in retired {
                match std::fs::remove_file(dir.join(name)) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => failures.push(MigrationFailure {
                        name: (*name).to_string(),
                        error: e.kind(),
                    }),
                }
            }
        }
        failures
    }

    /// One record for each migration failure that stopped the start, so a tree
    /// left half-migrated is not silent: the action is `migrate`, the target
    /// the file, the outcome the io kind. The manager acts for itself at its
    /// start, so caller and principal are the bare `-`, as with a render
    /// failure. A log, like the action records: the manager's own words, not
    /// user text.
    fn report_migrations(&self, failures: &[MigrationFailure]) {
        for failure in failures {
            self.action_log.record(&format!(
                "action=\"migrate\" target={:?} caller=- principal=- outcome={:?}",
                failure.name, failure.error
            ));
        }
    }

    /// One record for a start that planned no services, in either of two
    /// cases: the instance record ([`INSTANCES_FILE`]) could not be read, or
    /// it read whole and named no instance (an empty one, one cut
    /// mid-header, one of comments). Both plan nothing, so nothing was
    /// synced: a sync against an empty list deletes the service of every
    /// instance, which a read that failed must never do, and a record that
    /// names none is treated like it. The action is `sync`, the target the
    /// record's name, the outcome the io kind the caller passes (a listing
    /// that read whole passes `InvalidData`, as no read failed); caller and
    /// principal are the bare `-` (the manager acting for itself at its
    /// start), as with a render failure. A log, like the action records: the
    /// manager's own words, not user text.
    pub fn sync_report(&self, why: &std::io::Error) {
        self.action_log.record(&format!(
            "action=\"sync\" target={INSTANCES_FILE:?} caller=- principal=- outcome={:?}",
            why.kind()
        ));
    }

    /// The render of every instance, taken under the `writing` lock by
    /// [`Self::render_failures`] and by a write: a render is the pool as it
    /// is on disk, so it belongs with the write that changed it. One record
    /// comes back per instance whose pair did not land, in the record's order:
    /// the fit check that refused it (`ManagerError::Invalid`) or the IO error
    /// that stopped the write; a render that cannot be written does not stop
    /// the rest. The render writes the instance's pair, `models@<id>.ini` and
    /// `launch@<id>.ini`, together (`ARCHITECTURE.md` "Convergence: idempotent
    /// apply, drift and the last good render"): a pair whose text the pass does
    /// not change is not written, and an instance whose pair the fit check
    /// refused keeps the bytes it has, reported not converged with that
    /// check's reason. Each instance's fit checks measure the render against
    /// its pair on disk, the last good one (a fit failure writes neither file,
    /// so what stands on disk is the state the instance is running towards): a
    /// write's pass and the start's pass judge the same way, so a save that
    /// grows nothing over the pair that stands asks the GPU probe for nothing,
    /// and a render that grew past the pair while the manager was away keeps
    /// its bytes and is reported not converged. The pass's keyed refusals are
    /// kept in memory, the rows a read answers until the next pass measures the
    /// tree again (IP3c review 8).
    fn render_all_locked(&self) -> Vec<(String, ManagerError)> {
        let dir = crate::paths::engine_config_dir();
        let on_disk = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();
        let io = |e: std::io::Error| ManagerError::Io(e.kind());
        let record = on_disk(INSTANCES_FILE);
        // The shape of the record, which decides below whether a render that
        // already holds its text is rewritten.
        let migrated = record_is_migrated(&record);
        let mut failures = Vec::new();
        for id in crate::paths::instance_ids() {
            let (path, launch_path) = match (
                crate::paths::instance_models(&id),
                crate::launch::instance_launch_ini(&id),
            ) {
                (Some(path), Some(launch_path)) => (path, launch_path),
                _ => continue,
            };
            // The pair the instance converges to, as its launch reads it. The
            // record naming no server profile the engine has a file for gives no
            // launch: the pass writes no launch file, and the launch reads the
            // profile itself, as it did before the first render.
            let pair = (render_of_id(&id, &record), launch_render(&id));
            // A pair that already holds the text the pass gives stands where it
            // is: the start renders every instance every time, and a start of a
            // migrated install changes no byte and no mtime (IP2 review D1). The
            // launch file is part of the pair, so it is part of that rule. A
            // tree the migration has not written is rendered as the manager
            // always rendered it (IP2 review D4b).
            let same = pair_stands(&id, &pair);
            if migrated && same {
                continue;
            }
            // The fit checks of this one instance's pair: a render that does not
            // fit on this instance's cards keeps its bytes, and the instance is
            // not converged. The same write lands on the instance that fits
            // (IP3c: the fit checks are per instance, the checks of the entry
            // itself are the whole save's). `today` is the whole pair on disk,
            // the launch with the model list: the cards and the cap the pair
            // stands on are the ones its own launch names.
            let today = (
                std::fs::read_to_string(&path).unwrap_or_default(),
                std::fs::read_to_string(&launch_path).ok(),
            );
            if let Err(refusal) = self.pair_fit(&id, &pair, &today) {
                failures.push((id, ManagerError::Invalid(refusal)));
                continue;
            }
            // A render holds model profile text alone, so it carries the model profile
            // file's mode and needs no group of its own. The launch is a server
            // config: it carries the server profile's mode and group, the mode
            // the instance's account can read.
            let written = write_served_file(&path, &ConfigFile::Models, &pair.0, &*self.group)
                .and_then(|_| match &pair.1 {
                    Some(launch) => write_served_file(
                        &launch_path,
                        &ConfigFile::ServerProfile(id.clone()),
                        launch,
                        &*self.group,
                    ),
                    None => Ok(()),
                });
            if let Err(e) = written {
                failures.push((id, io(e)));
            }
        }
        // The rows the reads answer until the next pass: this pass's keyed
        // refusals, in its order. An instance the pass skipped, its pair
        // already standing, is in no row, so the read says it converged
        // (IP3c review 8). The IO failures are not rows: the write's own
        // failure, as [`Self::write_failure`] reads them.
        *self.not_converged.lock().unwrap_or_else(|e| e.into_inner()) = failures
            .iter()
            .filter_map(|(id, err)| match err {
                ManagerError::Invalid(refusal) => Some((id.clone(), refusal.clone())),
                _ => None,
            })
            .collect();
        failures
    }

    /// The convergence of instance `id`: the keyed refusal the last render pass
    /// left for it, and `converged` when it left none. Read from memory, measured
    /// nowhere here (`ARCHITECTURE.md` "Convergence": not converged is derived,
    /// recomputed at the manager's start and at every write): the memory check
    /// runs the engine's probe, and the render of a not-converged instance is
    /// larger than the pair that stands, so a read that measured again would ask
    /// a card for every one of them (IP3c review 8). The pass writes the rows at
    /// the start and at every write, so a manager that starts again over the same
    /// tree answers the same rows before any read.
    fn convergence(&self, id: &str) -> (bool, Option<pai_core::checks::Refusal>) {
        let rows = self.not_converged.lock().unwrap_or_else(|e| e.into_inner());
        match rows.iter().find(|(row, _)| row == id) {
            Some((_, refusal)) => (false, Some(refusal.clone())),
            None => (true, None),
        }
    }

    /// Whether the pair instance `id` runs is not the pair on disk: its launch
    /// recorded the digest of the pair it started with in [`applied_file`], and
    /// the files digest differently. An instance with no record of what it
    /// applied runs a pair this install never recorded, so it drifts too, until
    /// its next start; an id with no readable pair drifts by no definition
    /// (`ARCHITECTURE.md` "Convergence", IP3c). A compare of files, asking
    /// NOTHING of the service manager: the drift is what the files say, so every
    /// read of it (the bus's rows, the pipe's `ListInstances`, the plain list)
    /// answers it and reaches no unit. A pair that drifted and is running wants
    /// the restart now; a pair that drifted and is stopped wants it at its next
    /// start, which is the same row.
    pub fn pending_restart(&self, id: &str) -> bool {
        match (applied_digest(id), pair_digest(id)) {
            (Some(applied), Some(pair)) => applied != pair,
            (None, Some(_)) => true,
            _ => false,
        }
    }

    /// Whether a write moved instance `id` off the pair its launch recorded: the
    /// two digests, with a record REQUIRED. An instance that never recorded a
    /// pair applied nothing, so no write moved it to a restart and its unit's
    /// state is worth no ask; the drift of [`Self::pending_restart`] is the
    /// wider reading of the same files, the row's (IP3c review 4).
    fn pair_moved(&self, id: &str) -> bool {
        match (applied_digest(id), pair_digest(id)) {
            (Some(applied), Some(pair)) => applied != pair,
            _ => false,
        }
    }

    /// The fit checks of instance `id`'s pair: the memory the render needs on
    /// the cards its launch names, the `load-on-startup` count of its model
    /// list against the `ModelsMax` of its launch, and the GPUs its model
    /// profiles name, all judged against the pair on disk (`today`). Each side
    /// is measured on ITS OWN cards and cap, the ones its launch carries: the
    /// pair on disk is the last good one, so it fits the cards it was rendered
    /// for, and a switch that narrows those cards, lowers that cap, or puts a
    /// named GPU outside is the news that stops the pair (`ARCHITECTURE.md`
    /// "Convergence", IP3c review: fit is judged against the pair on disk). The
    /// model files are not judged here: the save that moves them judges them,
    /// and an instance whose model file vanished is no reason to stop rendering
    /// it (the boot render converges a tree whose files are missing). A pair the
    /// pass leaves alone grows nothing and names nothing new, so it is judged on
    /// nothing.
    fn pair_fit(
        &self,
        id: &str,
        pair: &(String, Option<String>),
        today: &(String, Option<String>),
    ) -> Result<(), pai_core::checks::Refusal> {
        check_render_memory(
            &pair.0,
            &today.0,
            id,
            &*self.gpus,
            &pair_devices(today, id),
            &pair_devices(pair, id),
        )?;
        check_pair_startup_count(
            &pair.0,
            &today.0,
            pair_cap(pair, id),
            pair_cap(today, id),
            id,
        )?;
        check_pair_devices(
            &pair.0,
            &today.0,
            id,
            &pair_devices(today, id),
            &pair_devices(pair, id),
        )
    }

    /// The failures of a render pass that stop the write: an IO failure of a
    /// render, the first in id order. A pair the fit check refused is not the
    /// write's failure: the save landed, that instance kept its last good pair,
    /// and the answer names it (`Self::write_pool`).
    fn write_failure(&self, failures: &[(String, ManagerError)]) -> Option<ManagerError> {
        failures
            .iter()
            .map(|(_, err)| err)
            .find(|err| !matches!(err, ManagerError::Invalid(_)))
            .cloned()
    }

    /// The render pass: the instances whose pair the fit check refused, keyed
    /// and in id order, and the write's own failure when the write of a pair
    /// failed, the first in id order as a write always answered. Each
    /// instance's fit checks measure the render against its pair on disk
    /// ([`Self::render_all_locked`]).
    fn render_pass(&self) -> RenderPass {
        let pass = self.render_all_locked();
        match self.write_failure(&pass) {
            Some(err) => Err(err),
            None => Ok(pass
                .into_iter()
                .filter_map(|(id, err)| match err {
                    ManagerError::Invalid(refusal) => Some((id, refusal)),
                    _ => None,
                })
                .collect()),
        }
    }

    /// The instances of this node that are not converged: one row per instance
    /// whose pair the render refused, in id order (`ARCHITECTURE.md`
    /// "Convergence", IP3c). A write's answer carries them beside the running
    /// instances it moved, and the rows of `instances` carry the same keyed
    /// reason. Derived by a render pass over the files, never kept. The pass's
    /// own failure is the caller's, as a write's is: a pair that could not be
    /// read or written is not a pair that stands. Taken under the `writing` lock
    /// the caller holds, as every save is: a save's answer reads the rows the
    /// pass it just ran left, on the same thread.
    fn not_converged_locked(&self) -> Result<Vec<NotConverged>, ManagerError> {
        Ok(not_converged_of(&self.render_pass()?))
    }

    /// [`Self::drift_running`] beside the rows of [`Self::not_converged_locked`]:
    /// the answer of a save that re-renders ([`Affected`], IP3c). Taken under the
    /// `writing` lock, as every save is, so the rows speak of the pair the save's
    /// pass left. The stream and the bus answer it to every save that re-renders,
    /// the pool writes included.
    pub fn affected(&self) -> Result<Affected, ManagerError> {
        Ok(Affected {
            running: self.drift_running()?,
            not_converged: self.not_converged_locked()?,
        })
    }

    /// The answer of a save that re-renders, from the render pass the save
    /// itself ran (IP3c): the rows of that pass, beside `running`, the
    /// instances the save answers as moved. A save takes its rows from the pass
    /// it left the files with, never from a pass run again after it: the second
    /// pass renders what the first wrote and can disagree with it. The pass's own
    /// failure is the save's, as a write's is, so the rows arrive unwrapped.
    /// Taken under the `writing` lock, as every save is.
    fn affected_of(
        &self,
        failures: &[(String, pai_core::checks::Refusal)],
        running: Vec<String>,
    ) -> Affected {
        Affected {
            running,
            not_converged: not_converged_of(failures),
        }
    }

    /// The instances a save that re-renders answers as `running` (IP3c,
    /// `ARCHITECTURE.md` "Convergence"): the ones of this node that recorded the
    /// pair their launch started with, are up, and now run a pair that is not the
    /// pair on disk, in the record's order. The compare comes first: only an
    /// instance whose pair moved off its record is asked of the service manager,
    /// so a save that changed no pair touches no unit, and an instance that
    /// recorded no pair applied nothing, so no save moved it to a restart and its
    /// unit is asked for nothing (review 4). Exactly the instances the page offers
    /// a restart to, and the twin of
    /// [`Self::drift_running_using_profile`], which walks one profile. The
    /// rows beside them come from the render pass, read from the files the save
    /// left, never kept.
    fn drift_running(&self) -> Result<Vec<String>, ManagerError> {
        let record = self.read_record_locked()?;
        Ok(pai_core::ini::parse_sections(&record)
            .into_iter()
            .map(|section| section.id)
            .filter(|id| self.pair_moved(id))
            .filter(|id| {
                self.service
                    .status(id)
                    .is_ok_and(|state| state == UnitState::Active)
            })
            .collect())
    }

    /// The node's compute devices, from the engine's own probe, with the moment
    /// it ran: the list a model profile page distributes a model over. The
    /// host's inventory travels with it (`NodeDevices`'s `ram_mib` and
    /// `cpu_threads`, read from the machine by the seam, R1). A read,
    /// so it records no action, reaches no service manager and writes no file;
    /// the probe runs as the manager, which is why the right is asked before it.
    /// A call for the caller's own asks `control` of polkit WITHOUT interaction
    /// ([`Authority::is_authorized_quietly`], the way a read that can prompt for
    /// nothing is asked): a device list is worth no challenge over the user's own
    /// session, so a caller polkit would challenge is refused. A call naming a
    /// principal is the `As` form, gated on the pair alone, as every other `As`
    /// verb. Either refusal is [`ManagerError::NotAuthorized`] with the probe
    /// never run: a caller with no right must not make the manager spawn the
    /// engine.
    pub fn devices(&self, who: &Requester) -> Result<pai_core::client::NodeDevices, ManagerError> {
        self.authorize_quietly(who, Action::Control)?;
        Ok(self.gpus.devices())
    }

    /// The node's own info (AN3, `ARCHITECTURE.md` "About and node info"): its
    /// operating system and kernel, what its Vulkan read answered, and one row
    /// per GPU with the kernel's driver and the Vulkan one joined onto it. The
    /// gate is the gate of [`Self::devices`], because this read may spawn the
    /// Vulkan child: the caller's own read is `control` asked WITHOUT
    /// interaction, the `As` form is the bff's account vouching for a current
    /// member of `pai`, and every refusal is [`ManagerError::NotAuthorized`]
    /// with no child spawned and no host read: a caller with no right must not
    /// make the manager spawn the child. A read, so it records no action and
    /// reaches no service manager. The three parts are read in the page's
    /// order, the OS, the kernel's rows, the child's answer, and the join is
    /// the core's ([`pai_core::vulkan::join`]), so the director's node gets
    /// the same rows; a Vulkan read of `none` or `failed` joins nothing and
    /// keeps the kernel's rows, which is a hole in the page, never an error.
    pub fn node_info(
        &self,
        who: &Requester,
    ) -> Result<pai_core::client::NodeInfoReply, ManagerError> {
        self.authorize_quietly(who, Action::Control)?;
        let os = self.node.os();
        let kernel_rows = self.node.gpus();
        let probe = self.node.vulkan();
        Ok(pai_core::client::NodeInfoReply {
            os,
            vulkan: probe.status,
            gpus: pai_core::vulkan::join(&kernel_rows, &probe),
        })
    }

    /// M3 of "Choosing the model file on the web" (`ARCHITECTURE.md` (4) and
    /// "One models folder for every engine" (3)): the node's one models
    /// folder (`paths::instance_models_dir()`) as the picker's file list. The
    /// gate is the gate of [`Self::devices`]: the caller's own read is
    /// `control` asked WITHOUT interaction, the `As` form is the bff's account
    /// vouching for a current member of `pai`, and every refusal is
    /// [`ManagerError::NotAuthorized`] with the folder never opened.
    ///
    ///
    /// The folder is opened only for a caller the gate lets through. A folder
    /// that is missing, is not a folder, or cannot be read answers no rows and
    /// the keyed `models-dir-unreadable` naming it, INSIDE a success: the
    /// picker shows an empty list with a text, the call does not fail. A
    /// folder below it that cannot be opened is skipped, its files simply
    /// absent. The walk is the folder's own ([`walk_models_dir`]), its bound of
    /// entries visited being this manager's [`Self::walk_max_entries`]: no
    /// service manager is reached, no file is written, no action recorded.
    pub fn model_files(
        &self,
        who: &Requester,
    ) -> Result<pai_core::client::ModelFilesReply, ManagerError> {
        self.authorize_quietly(who, Action::Control)?;
        Ok(walk_models_dir(
            &crate::paths::instance_models_dir(),
            self.walk_max_entries,
        ))
    }

    /// L1 of "Layers, MoE and tensor placement on the web model profile page"
    /// (ARCHITECTURE.md (1)): the layer facts of ONE saved model profile, by
    /// its id. The gate is the gate of reading `models.ini`
    /// ([`Self::authorize_quietly`] with `control`): the caller's own read is
    /// polkit asked quietly, the `As` form is the bff's account vouching for a
    /// current member of `pai`, and every refusal reads nothing. The section is
    /// looked for in `models.ini`, else in `models.ini.disabled` (a switched-off
    /// profile has sliders too); no section, and `*`, answer the keyed
    /// `no-model-profile-named`. `model` resolves as `model_path` resolves it,
    /// against the instances' working directory, and the key the section lacks
    /// is the pool's `[*]` one, key by key, as `estimated_of` reads it. The
    /// outcomes are all a SUCCESS of the call, with `info` null and a keyed
    /// `reason` (the outcome table of the design); a shard set declaring more
    /// shards than [`MAX_MODEL_SHARDS`] is `model-info-unreadable` naming the
    /// model, judged from the name with no shard opened. The reader's own text
    /// never travels, so no byte of a file it refused reaches the reply.
    /// `info` carries the tensor list of 2j (J1) beside the counts: every
    /// tensor of every shard, each shard's tensors in data-offset order, shard
    /// after shard, cut at the first row past either bound,
    /// [`pai_core::gguf_shape::MAX_TENSOR_ROWS`] rows or
    /// [`pai_core::gguf_shape::MAX_TENSOR_LIST_BYTES`] of their JSON, with
    /// `tensors_truncated` saying the model has more than the list carries.
    pub fn model_info(
        &self,
        who: &Requester,
        profile: &str,
    ) -> Result<pai_core::client::ModelInfoReply, ManagerError> {
        use pai_core::client::{ModelFacts, ModelInfoReply, Reason, TensorRow};
        self.authorize_quietly(who, Action::Control)?;
        let reason = |key: &str, args: Vec<String>| ModelInfoReply {
            model: None,
            info: None,
            reason: Some(Reason {
                key: key.into(),
                args,
            }),
        };
        let named = |name: &str| -> Vec<pai_core::ini::Section> {
            std::fs::read_to_string(crate::paths::engine_config_dir().join(name))
                .map(|text| pai_core::ini::parse_sections(&text))
                .unwrap_or_default()
        };
        // The section, in the pool that has it: the enabled file first, the
        // switched-off one next.
        let enabled = named("models.ini");
        let disabled = named("models.ini.disabled");
        let (sections, section) = match profile_section(&enabled, profile)
            .map(|s| (enabled.as_slice(), s))
            .or_else(|| profile_section(&disabled, profile).map(|s| (disabled.as_slice(), s)))
        {
            Some(found) => found,
            None => return Err(one_value_refusal("no-model-profile-named", profile)),
        };
        let own = |key: &str| {
            section
                .keys
                .get(key)
                .map(String::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        // A fetch key of the section names no file of the node: the answer is
        // "fetched", and no file is opened.
        if let Some(key) = FETCH_KEYS.iter().find(|key| own(key).is_some()) {
            return Ok(reason("model-info-fetched", vec![(*key).to_string()]));
        }
        // The section's own `model`, else the pool's `[*]` one: the cascade is
        // PER KEY, exactly as `estimated_of` resolves it.
        let global = sections.iter().find(|s| s.id == "*");
        let get = |key: &str| {
            own(key).or_else(|| {
                global
                    .as_ref()
                    .and_then(|g| g.keys.get(key))
                    .map(String::as_str)
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
            })
        };
        let Some(model) = get("model") else {
            return Ok(reason("model-profile-no-model", vec![profile.to_string()]));
        };
        let cwd = crate::launch::instance_current_dir(
            cfg!(windows),
            &crate::paths::data_root(),
            &crate::paths::instance_state_dir(),
        );
        let path = model_path(&model, &cwd);
        // The shard set first, from the name alone: a split model declaring
        // more shards than [`MAX_MODEL_SHARDS`] is `unreadable`, naming the
        // model, and nothing past the first shard is opened. Then a shard that
        // is not there answers `missing` naming that shard, and a path that
        // names no regular file answers `unreadable`, before the reader turns
        // either into its own text.
        let shards = pai_core::gguf_shape::shard_set(&path);
        if shards.len() > MAX_MODEL_SHARDS {
            return Ok(reason(
                "model-info-unreadable",
                vec![path.to_string_lossy().into_owned()],
            ));
        }
        for shard in shards {
            let named = shard.to_string_lossy().into_owned();
            match std::fs::metadata(&shard) {
                Err(_) => return Ok(reason("model-info-missing", vec![named])),
                Ok(meta) if !meta.is_file() => {
                    return Ok(reason("model-info-unreadable", vec![named]))
                }
                Ok(_) => {}
            }
        }
        match pai_core::gguf_shape::read_shards(&path) {
            Ok(shards) => {
                // The tensor list of 2j (J1): every tensor of every shard, each
                // shard's tensors in data-offset order, shard after shard, cut
                // at the first row past either bound, with the flag saying the
                // list is not the whole model.
                let (tensors, tensors_truncated) = match pai_core::gguf_shape::tensor_rows(&shards)
                {
                    Ok(rows) => rows,
                    Err(_) => {
                        return Ok(reason(
                            "model-info-unreadable",
                            vec![path.to_string_lossy().into_owned()],
                        ))
                    }
                };
                match pai_core::gguf_shape::shape_of(&shards) {
                    Ok(shape) => Ok(ModelInfoReply {
                        model: Some(path.to_string_lossy().into_owned()),
                        info: Some(ModelFacts {
                            block_count: shape.block_count,
                            moe: shape.moe,
                            expert_count: shape.expert_count,
                            expert_used: shape.expert_used_count,
                            tensors: tensors
                                .into_iter()
                                .map(|(name, bytes)| TensorRow { name, bytes })
                                .collect(),
                            tensors_truncated,
                        }),
                        reason: None,
                    }),
                    // The reader's text is dropped: the reply names the path,
                    // never what the file's bytes spelled.
                    Err(_) => Ok(reason(
                        "model-info-unreadable",
                        vec![path.to_string_lossy().into_owned()],
                    )),
                }
            }
            // The reader's text is dropped: the reply names the path, never
            // what the file's bytes spelled.
            Err(_) => Ok(reason(
                "model-info-unreadable",
                vec![path.to_string_lossy().into_owned()],
            )),
        }
    }

    /// The gate of a read that may prompt for nothing: the principal gate first,
    /// then polkit asked quietly, and for a call that vouches for a user nothing
    /// of polkit at all (the pair is its right, and polkit's subject is the
    /// caller, never the user the caller vouches for).
    fn authorize_quietly(&self, who: &Requester, action: Action) -> Result<(), ManagerError> {
        self.gate_principal(who)?;
        if who.principal.is_some() {
            return Ok(());
        }
        if self.authority.is_authorized_quietly(&who.caller, action) {
            Ok(())
        } else {
            Err(ManagerError::NotAuthorized)
        }
    }

    /// Whether `password` is `user`'s, and whether that user may configure:
    /// (ok, can_configure). Answers only the bff's account
    /// ([`Authority::is_bff`]), anyone else NotAuthorized with nothing
    /// checked. An empty user or password, a wrong password and an unknown
    /// user are all (false, false).
    pub fn authenticate(
        &self,
        caller: &Caller,
        user: &str,
        password: &str,
    ) -> Result<(bool, bool), ManagerError> {
        if !self.authority.is_bff(caller) {
            return Err(ManagerError::NotAuthorized);
        }
        if user.is_empty() || password.is_empty() {
            return Ok((false, false));
        }
        match self.passwords.check(user, password) {
            Ok(true) => Ok((true, self.passwords.is_admin(user))),
            Ok(false) => Ok((false, false)),
            Err(PasswordError(message)) => Err(ManagerError::Service(ServiceError(message))),
        }
    }

    /// The log of one guarded call: renders the record and hands the whole
    /// line to [`ActionLog::record`], once per call, done or refused. The
    /// record names the `action` (the method's own name), the `target` (the
    /// instance id or file name as it was asked for), the caller, the principal
    /// (a bare `-` when absent) and `answer`'s outcome: `ok` when the call ran,
    /// `refused` for [`ManagerError::NotAuthorized`], the error's own name for
    /// any other failure. Each name is written with Rust's `{:?}` quoting, so a
    /// caller or principal carrying a quote, a newline or a NUL forges no second
    /// record. Never a password, never file contents: a write names only the
    /// file it wrote.
    fn record_action<T>(
        &self,
        action: &str,
        target: &str,
        who: Option<&Requester>,
        answer: &Result<T, ManagerError>,
    ) {
        let quoted = |name: Option<&str>| match name {
            Some(name) => format!("{name:?}"),
            None => "-".to_string(),
        };
        let outcome = match answer {
            Ok(_) => "ok".to_string(),
            Err(ManagerError::NotAuthorized) => "refused".to_string(),
            Err(e) => format!("{:?}", error_kind(e)),
        };
        let caller = quoted(who.map(|who| who.caller.0.as_str()));
        let principal = quoted(
            who.and_then(|who| who.principal.as_ref())
                .map(|principal| principal.0.as_str()),
        );
        self.action_log.record(&format!(
            "action={action:?} target={target:?} caller={caller} principal={principal} outcome={outcome}"
        ));
    }

    fn authorize(&self, caller: &Caller, action: Action) -> Result<(), ManagerError> {
        if self.authority.is_authorized(caller, action) {
            Ok(())
        } else {
            Err(ManagerError::NotAuthorized)
        }
    }

    /// The gate of a guarded call: the principal gate first, then, for the
    /// caller's own call (`principal` None), polkit, exactly as today. A call
    /// naming a principal needs no right of the caller's at all (the bff keeps
    /// none: the right is the pair), so polkit is asked for nothing, not even
    /// about the action: polkit's subject is the caller, never the user the
    /// caller vouches for. Either refusal is [`ManagerError::NotAuthorized`]
    /// before any path is resolved, so a refused call reads and writes nothing.
    fn authorize_as(&self, who: &Requester, action: Action) -> Result<(), ManagerError> {
        self.gate_principal(who)?;
        if who.principal.is_none() {
            return self.authorize(&who.caller, action);
        }
        Ok(())
    }

    /// [`Self::check_principal`] when `who` vouches for a user, nothing when
    /// the call is the caller's own. A request naming a principal is gated
    /// before its action, whatever that action needs: the model profile files are
    /// open to every caller, and a call naming a user the manager refuses is
    /// refused whole, never answered as the caller's own.
    fn gate_principal(&self, who: &Requester) -> Result<(), ManagerError> {
        match &who.principal {
            Some(principal) => self.check_principal(&who.caller, principal),
            None => Ok(()),
        }
    }

    /// The checks a call that names a principal passes before its action
    /// runs, in this order, each failure [`ManagerError::NotAuthorized`] with
    /// nothing done and the later check not asked:
    ///
    /// 1. the caller is the bff's account ([`Authority::is_bff`]), the only
    ///    one vouching for a user; a refusal is never downgraded to a call as
    ///    the caller's own,
    /// 2. the name is well formed ([`principal_name_is_well_formed`]),
    /// 3. the account behind the name is a member of `pai` at this call
    ///    ([`PasswordChecker::is_admin`], the same lookup `authenticate`
    ///    uses): asked every call, never cached here, never taken from the
    ///    caller, so a member removed between two calls is refused at the
    ///    next one.
    fn check_principal(&self, caller: &Caller, principal: &Principal) -> Result<(), ManagerError> {
        if !self.authority.is_bff(caller) {
            return Err(ManagerError::NotAuthorized);
        }
        if !principal_name_is_well_formed(&principal.0) {
            return Err(ManagerError::NotAuthorized);
        }
        if !self.passwords.is_admin(&principal.0) {
            return Err(ManagerError::NotAuthorized);
        }
        Ok(())
    }

    /// Refuse the save of the server profile `name` when an instance runs it
    /// and its address collides with the profile of another instance, or with
    /// the address the web service listens at: the refusal names the instance,
    /// the service is named as itself. A profile no instance runs binds
    /// nothing, so it is judged against nothing: two spare profiles, or a spare
    /// and one in use, share an address. Its own host is resolved all the
    /// same, before that judgment: a host of this text that does not resolve is
    /// its own refusal, spare profile or not. An instance whose section names
    /// no profile, or names one whose file cannot be read, is skipped: its
    /// launch refuses it, and a save is never judged on the defaults of a file
    /// that is not there.
    fn check_address(&self, name: &str, contents: &str) -> Result<(), ManagerError> {
        let cfg = crate::server_cfg::from_text(contents);
        let mine = binding(&cfg, &self.resolver).map_err(ManagerError::UnresolvableHost)?;
        if !profile_in_use(name) {
            return Ok(());
        }
        self.check_binding(&mine, |other| {
            crate::paths::instance_server_profile(other).as_deref() == Some(name)
        })
    }

    /// Refuse when the binding `mine` collides with the address of another
    /// instance: the walk every check of an address shares, the save of a
    /// server profile, the switch of an instance to another one, the write of
    /// an instance's address and the create of an instance. `skip`
    /// answers the instances to leave out of the walk, the ones that hold the
    /// address for the reason the check is about: the profile being saved, for
    /// a save, and the instance being switched, for a switch. An instance
    /// whose section names no profile, whose profile builds no path, or whose
    /// profile file cannot be read, is skipped too. The walk is
    /// [`crate::paths::instance_ids`], the ids a section of the record carries
    /// and the manager accepts: a section whose name is no instance id is
    /// never run (`launch::instance_command`'s `InvalidId`), so it holds no
    /// address.
    fn check_binding<F>(&self, mine: &Binding, skip: F) -> Result<(), ManagerError>
    where
        F: Fn(&str) -> bool,
    {
        for other in crate::paths::instance_ids() {
            if skip(&other) {
                continue;
            }
            // The address the instance listens on: its server profile's, with
            // the record's `Hostname` and `Port` over it, the way the launch
            // binds (`ARCHITECTURE.md`, "Instance profiles").
            let Some(cfg) = instance_cfg(&other) else {
                continue;
            };
            let theirs = binding(&cfg, &self.resolver);
            if theirs.is_ok_and(|b| addresses_collide(mine, &b)) {
                return Err(ManagerError::AddressInUse(other));
            }
        }
        // The web service's address: no `bff.ini` is its default, a file that
        // cannot be read or names no address is an address nobody holds.
        if let Some(addr) = pai_core::bff_cfg::listen() {
            let theirs = Binding {
                port: addr.port() as i32,
                ips: vec![addr.ip()],
            };
            if addresses_collide(mine, &theirs) {
                return Err(ManagerError::AddressInUse(BFF_NAME.to_string()));
            }
        }
        Ok(())
    }
}

/// How deep [`walk_models_dir`] descends below the models folder: a file in
/// the folder itself is at depth 0, one under six folders is at depth 6 and is
/// the deepest listed. The layout the picker reads is one category folder with
/// a tree of the download tool's under it.
const MODELS_WALK_MAX_DEPTH: usize = 6;

/// How many rows [`walk_models_dir`] lists before it stops and says so
/// (`truncated`): a folder of a model farm is no page of ten thousand rows.
const MODELS_WALK_MAX_FILES: usize = 5000;

/// How many entries [`walk_models_dir`] visits before it stops and says so
/// (`truncated`), counting every entry it looks at and not only the rows it
/// keeps: the service's account reads the tree, so a bound on the rows alone
/// leaves a huge tree of sidecars read in full on every request. A test lowers
/// it through [`Manager::with_walk_max_entries`].
const MODELS_WALK_MAX_ENTRIES: usize = 50_000;

/// The shard facts a `.gguf` name carries: the number of shards it declares (1
/// for a file whose name is no shard) and whether the file is the FIRST shard
/// (a single file is its own first). Both are read out of the name with no
/// shard opened, the way [`MAX_MODEL_SHARDS`] judges a shard set.
fn shards_of(name: &str) -> (u64, bool) {
    use pai_core::gguf_shape::split_shard_suffix;
    let one = || (1, true);
    // The extension is the shard suffix's own, matched any case as the scan
    // matches it, so a `.Gguf` shard is judged like a `.gguf` one.
    let Some(cut) = name.len().checked_sub(5) else {
        return one();
    };
    if !name.is_char_boundary(cut) || !name[cut..].eq_ignore_ascii_case(".gguf") {
        return one();
    }
    let stem = &name[..cut];
    // `split_shard_suffix` has read the index as five digits and the total as
    // five digits, so the total needs no second check.
    let Some((_, index)) = split_shard_suffix(stem) else {
        return one();
    };
    let Some((_, total)) = stem.rsplit_once("-of-") else {
        return one();
    };
    (total.parse().unwrap_or(1).max(1), index == "00001")
}

/// The `category` of a model file: the first subfolder of `path` under `dir`
/// (the GUI's `model_scan::Category::subdir` layout: `models`, `mmprojs`,
/// `mtps`, `dflashs`), `None` for a file that sits in `dir` itself.
fn models_category(dir: &std::path::Path, path: &std::path::Path) -> Option<String> {
    use std::path::Component;
    let mut parts = path.strip_prefix(dir).ok()?.components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(name)), Some(_)) => Some(name.to_string_lossy().into_owned()),
        _ => None,
    }
}

/// Every `.gguf` file under `dir`, as the picker's rows: a split model once,
/// as its first shard with the shard count its name declares, each row's
/// `category` the first subfolder under `dir`. Symlinked folders are not
/// followed (a linked folder is another folder's files, and the models folder
/// is the only one the caller has a right to), a folder whose name begins with
/// `.` is skipped (a download tool's staging area is no model), the descent
/// stops at [`MODELS_WALK_MAX_DEPTH`] folders, the entries visited at
/// `max_entries` (every entry the walk looks at, folders and files it keeps no
/// row of included, the bound's reason at [`MODELS_WALK_MAX_ENTRIES`]) and the
/// rows at [`MODELS_WALK_MAX_FILES`], `truncated` saying the last two. A folder
/// that is missing, is not a folder, or cannot be opened answers no rows and
/// the keyed `models-dir-unreadable` naming it; a folder below it that cannot
/// be opened is skipped, its files simply absent.
fn walk_models_dir(dir: &std::path::Path, max_entries: usize) -> pai_core::client::ModelFilesReply {
    let text = dir.to_string_lossy().into_owned();
    let mut out = pai_core::client::ModelFilesReply {
        dir: text.clone(),
        files: Vec::new(),
        truncated: false,
        error: None,
    };
    if !std::fs::metadata(dir).is_ok_and(|m| m.is_dir()) {
        out.error = Some(pai_core::client::Reason {
            key: "models-dir-unreadable".into(),
            args: vec![text],
        });
        return out;
    }
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    let mut visited = 0usize;
    while let Some((folder, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            // The bound counts the entries visited, so a tree the walk keeps no
            // row of spends it too: the reading is the service's own account.
            if visited == max_entries {
                out.truncated = true;
                return out;
            }
            visited += 1;
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if file_type.is_dir() {
                if depth == MODELS_WALK_MAX_DEPTH || name.starts_with('.') {
                    continue;
                }
                stack.push((entry.path(), depth + 1));
                continue;
            }
            // A symlink, a fifo or a device is no model file, and a file of
            // another extension is no model file, whatever its folder.
            if !file_type.is_file() || !name.to_lowercase().ends_with(".gguf") {
                continue;
            }
            // A split model is the first shard's row, so the row is the same
            // whichever shard the folder hands over first, and a set missing
            // its first shard, which no loader opens, is listed nowhere.
            let (shards, first) = shards_of(&name);
            if !first {
                continue;
            }
            if out.files.len() == MODELS_WALK_MAX_FILES {
                out.truncated = true;
                return out;
            }
            let path = entry.path();
            out.files.push(pai_core::client::ModelFileRow {
                path: path.to_string_lossy().into_owned(),
                size: entry.metadata().map(|m| m.len()).unwrap_or(0),
                shards,
                category: models_category(dir, &path),
            });
        }
    }
    out
}

/// The name the manager gives the web service when its address is in use: the
/// service's own name, the one the installer registers it under.
const BFF_NAME: &str = "private-ai-bff";

/// The file of the server profile the instance `id` names:
/// `server@<profile>.ini`, `None` when the section names no profile.
fn profile_file(id: &str) -> Option<std::path::PathBuf> {
    crate::paths::instance_ini(&crate::paths::instance_server_profile(id)?)
}

/// The server config of the instance `id`, as its launch reads it: the text of
/// the server profile its instance profile names, with the `Hostname` and
/// `Port` of its section of the record over it (`ARCHITECTURE.md`, "Instance
/// profiles", where the model data model moved the address: the same
/// `launch::with_record_address` the launch reads). `None` when the
/// instance names no profile, names one that builds no path, or that file
/// cannot be read.
fn instance_cfg(id: &str) -> Option<ServerConfig> {
    let cfg = crate::server_cfg::from_text(&std::fs::read_to_string(profile_file(id)?).ok()?);
    Some(crate::launch::with_record_address(cfg, id))
}

/// The `[Server] Devices` of the server profile instance `id` names, as the
/// memory check measures the instance's cards: `""` when the section names no
/// profile, names one the engine has no file for, or that file cannot be read:
/// every card of the node, the measure of a profile that carries no `Devices`.
fn instance_devices(id: &str) -> String {
    // A profile that cannot be read reads as every card, like a missing one.
    profile_file(id)
        .and_then(|file| std::fs::read_to_string(file).ok())
        .map(|contents| crate::server_cfg::from_text(&contents).devices)
        .unwrap_or_default()
        .unwrap_or_default()
}

/// Whether any instance runs the server profile `name`: the profiles the
/// sections name are what binds an address and what caps a render.
fn profile_in_use(name: &str) -> bool {
    profile_holder(name).is_some()
}

/// The instance running the server profile `name`, when one does: the first
/// section of `instances.ini` that names it.
fn profile_holder(name: &str) -> Option<String> {
    crate::paths::instance_ids()
        .into_iter()
        .find(|id| crate::paths::instance_server_profile(id).as_deref() == Some(name))
}

/// The text of the record file `path`: `instances.ini`, read whole so a write
/// of it keeps every other byte. A file that is not there reads as empty:
/// llama.cpp's MSI copies it at install, and an install that has not is a tree
/// with no instance.
fn read_record(path: &std::path::Path) -> std::io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

/// Adds the section of the new instance to the record `path`, over the record
/// text `existing` that is there: the header, its instance profile and the
/// `Port` `port` it listens on, appended after every byte, so the notes of the
/// other sections and the file's own spacing are untouched. A record whose
/// last line has no ending gets a line ending of the file's own style first,
/// so the section it ends with keeps the keys it names and the new header
/// starts a line of its own. A record the install never copied is created by
/// this. The section names no server profile and no `Hostname`: the instance
/// profile is one step above the server profile (IP4b), and the address comes
/// from the `Port` alone.
fn append_record(
    path: &std::path::Path,
    existing: &str,
    id: &str,
    instance_profile: &str,
    port: u16,
) -> std::io::Result<()> {
    use std::io::Write;
    let mut record = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        record.write_all(pai_core::ini::detect_eol(existing).as_bytes())?;
    }
    record.write_all(
        format!("[{id}]\n{INSTANCE_PROFILE_KEY} = {instance_profile}\nPort = {port}\n").as_bytes(),
    )
}

/// The port [`create_instance`](Manager::create_instance) gives a new instance:
/// the first port of `first ..= last` the node holds nothing on. The ports the
/// node holds are the address of every instance the record names with an id the
/// manager accepts ([`crate::paths::instance_ids`]), read as its launch reads it
/// ([`instance_cfg`]: the server profile's, with the record's `Hostname` and
/// `Port` over it), so an instance whose section names no `Port` holds its
/// server profile's port, and the web service's own `Listen` port
/// ([`pai_core::bff_cfg::listen`]). A port outside the range is no answer, so a
/// node holding every port of it is refused `no-free-port`, naming the two ends
/// of the range it walked; the walk is bounded by `last`, it never runs past it.
fn proposed_port(first: u16, last: u16) -> Result<u16, ManagerError> {
    let mut held: Vec<u16> = Vec::new();
    for id in crate::paths::instance_ids() {
        if let Some(cfg) = instance_cfg(&id) {
            // A `Port` a hand edit wrote outside a port (0, 70000) holds
            // nothing, as a value that is no number at all does.
            if let Ok(port) = u16::try_from(cfg.port_or_default()) {
                held.push(port);
            }
        }
    }
    if let Some(addr) = pai_core::bff_cfg::listen() {
        held.push(addr.port());
    }
    (first..=last)
        .find(|port| !held.contains(port))
        .ok_or_else(|| {
            ManagerError::Invalid(pai_core::checks::Refusal {
                key: "no-free-port",
                values: vec![first.to_string(), last.to_string()],
            })
        })
}

/// `engine` if this manager serves it ([`ENGINE`], the one engine of a node),
/// else the refusal naming the engine it was asked for.
fn known_engine(engine: &str) -> Result<(), ManagerError> {
    if engine == ENGINE {
        Ok(())
    } else {
        Err(one_value_refusal("unknown-engine", engine))
    }
}

/// True when `from` and `values` are a source [`Manager::create_model_profile`]
/// can build a section from. A pick: `from` empty and `values` naming the model
/// with `model` alone, or `hf-repo` with `hf-file` beside it, no key twice, and
/// any other key a line of the new profile of its own (a key of the entry is
/// refused by its own rule). A clone: `from` a name, and `values` only lines
/// that override one of the clone's, so none of the three naming a model of its
/// own, none twice. Exactly one of the two sources, the empty pair and the pair
/// at once both refused. The shape answers before a value is read.
fn create_source_shape(from: &str, values: &[(String, String)]) -> bool {
    let mut seen = [false; 3];
    for (key, _) in values {
        let i = match key.as_str() {
            "model" => 0,
            "hf-repo" => 1,
            "hf-file" => 2,
            // A line of the new profile of its own, `temp` beside its model,
            // on a pick as on a clone: the page sends the model profile's keys.
            _ => continue,
        };
        // A pick takes each key once; a clone takes no key that names a model.
        if !from.is_empty() || seen[i] {
            return false;
        }
        seen[i] = true;
    }
    !from.is_empty() || ((seen[0] || seen[1]) && !(seen[0] && seen[1]))
}

/// The refusal of one key of a new model profile's source: the key is written
/// as the left side of a line, so a key that is empty, carries a control
/// character, white space, `=`, `[`, `]`, `;` or `#` is no key of the INI: it
/// forges a line, a comment marker or a whole section, out of the reach of the
/// name, id and source checks. It names the key, as a bad value does.
fn create_key_text(key: &str) -> Result<(), pai_core::checks::Refusal> {
    if key.is_empty()
        || key.chars().any(|c| {
            c.is_control() || c.is_whitespace() || matches!(c, '=' | '[' | ']' | ';' | '#')
        })
    {
        return Err(pai_core::checks::Refusal {
            key: "model-profile-bad-text",
            values: vec![key.to_string()],
        });
    }
    Ok(())
}

/// The refusal of one value of a new model profile's source: the INI format has
/// no escaping and a line ends at a newline, so a value that is blank, carries
/// a space at an end or carries a control character names the key it sits under;
/// a `;` or `#` is the existing `comment-markers` rule, naming the same key.
fn create_value_text(key: &str, value: &str) -> Result<(), pai_core::checks::Refusal> {
    if value.is_empty() || value.trim() != value || value.chars().any(|c| c.is_control()) {
        return Err(pai_core::checks::Refusal {
            key: "model-profile-bad-text",
            values: vec![key.to_string()],
        });
    }
    pai_core::checks::comment_markers(key, value)
}

/// The section the clone of `source` writes under the id `id`: the source's own
/// header replaced and every other line kept exactly as the file holds it,
/// comment lines included, the blank separator lines that end the section of
/// the middle of the file left out, and one line ending added at the end. The
/// line ending is the source's own, the one its header carries, so a clone of a
/// section of an LF file is an LF section. A key line of the entry is left out:
/// the placement and memory keys ([`pai_core::checks::PLACEMENT_KEYS`]) belong
/// to the entry of the instance profile that lists the model, and a clone is
/// listed in no entry, so copying one would put a second source of where the
/// weights go in the pool.
fn cloned_section(source: &str, id: &str) -> String {
    let eol = eol_of(source);
    let rest = match source.split_once('\n') {
        Some((_, rest)) => rest,
        None => "",
    };
    let mut lines: Vec<&str> = rest
        .split(eol)
        .filter(|line| !is_entry_key_line(line))
        .collect();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return format!("[{id}]{eol}");
    }
    format!("[{id}]{eol}{}{eol}", lines.join(eol))
}

/// Whether `line`, a line of a model profile section, is the key line of a key
/// of the entry ([`pai_core::checks::PLACEMENT_KEYS`]): its key, read as the INI
/// reads it, is one of them. A blank line, a comment and a line with no `=` are
/// no key line.
fn is_entry_key_line(line: &str) -> bool {
    let line = line.trim();
    if line.starts_with(';') || line.starts_with('#') {
        return false;
    }
    match line.split_once('=') {
        Some((key, _)) => pai_core::checks::placement_key(key.trim()),
        None => false,
    }
}

/// The line ending of `source`, a section's text taken from a file: the one its
/// header carries, CRLF when the text has no line ending at all.
fn eol_of(source: &str) -> &'static str {
    match source.split_once('\n') {
        Some((line, _)) if line.ends_with('\r') => "\r\n",
        Some(_) => "\n",
        None => "\r\n",
    }
}

/// The ids, in the order of the instance profile pool `text`, of the profiles
/// whose entry sections list the model profile `model`, in any letter case. A
/// profile is named once, whatever the number of its entries: an entry is the
/// model's place in the profile, and the profile is what its page changes to
/// take the model out.
fn entry_profile_owners(text: &str, model: &str) -> Vec<String> {
    let mut owners: Vec<String> = Vec::new();
    for profile in pai_core::instance_profiles::parse(text) {
        if !owners.contains(&profile.id)
            && profile
                .entries
                .iter()
                .any(|entry| entry.model.eq_ignore_ascii_case(model))
        {
            owners.push(profile.id);
        }
    }
    owners
}

/// The refusal of a rule that carries one value: the key, and the name the
/// rule refused.
fn one_value_refusal(key: &'static str, value: &str) -> ManagerError {
    ManagerError::Invalid(pai_core::checks::Refusal {
        key,
        values: vec![value.to_string()],
    })
}

/// The refusal of a rule that carries two: the key, and the two names the rule
/// refused, the pair an entry is (`instance-profile-model-listed`,
/// `no-instance-profile-model`).
fn two_value_refusal(key: &'static str, first: &str, second: &str) -> ManagerError {
    ManagerError::Invalid(pai_core::checks::Refusal {
        key,
        values: vec![first.to_string(), second.to_string()],
    })
}

/// The checks of the entry keys a save sends, the checks of the keys the model
/// profile gave up to the entry (`ARCHITECTURE.md` "Instance profiles", the
/// placement and memory amendment): a key that STAYS in the model profile is
/// the keyed `model-profile-stay-key`, naming it, and every value is judged by
/// the rule of its key as the model profile's own save judges it, the ranges of
/// [`pai_core::checks::MODEL_PROFILE_INT_KEYS`], the tensor rule, the tensor
/// rules' devices, the device lists, the floats, the enums and the pairs.
fn check_entry_values(values: &[(String, String)]) -> Result<(), pai_core::checks::Refusal> {
    for (key, value) in values {
        if !pai_core::checks::placement_key(key) {
            return Err(pai_core::checks::Refusal {
                key: "model-profile-stay-key",
                values: vec![key.clone()],
            });
        }
        if let Some(range) = pai_core::checks::model_profile_int_range(key) {
            pai_core::checks::whole_number(key, value, &range)?;
        }
        if key == "tensor-split" {
            pai_core::checks::tensor_split(value)?;
        }
        if key == "override-tensor" {
            pai_core::checks::override_tensor(value, true)?;
        }
        if pai_core::checks::DEVICE_LIST_KEYS.contains(&key.as_str()) {
            pai_core::checks::device_list(key, value)?;
        }
    }
    pai_core::checks::model_profile_values(|key| {
        values
            .iter()
            .find(|(sent, _)| sent == key)
            .map(|(_, value)| value.clone())
    })
}

/// The pool text `text` with the entry `[profile/model]` written whole: its
/// header and `keys` in the order they were sent, standing directly under the
/// profile's own `[profile]` section, the blank line that separated the two
/// dropped (the entry is part of that section's list, not a section of its
/// own). Every other byte of the file is the file's own. The caller refuses an
/// entry the profile does not list, so the section is there.
fn write_entry_section(
    text: &str,
    profile: &str,
    model: &str,
    keys: &[(String, String)],
) -> String {
    let eol = pai_core::ini::detect_eol(text);
    let section = format!("{profile}/{model}");
    let body = match pai_core::ini::text::section_text(text, &section) {
        Some(body) => body,
        None => return text.to_string(),
    };
    let at = text.find(&body).unwrap_or(0);
    // The blank line goes only when the profile's own text runs up to here.
    let from = match pai_core::ini::text::section_text(text, profile) {
        Some(row) if text.find(&row).is_some_and(|start| start + row.len() == at) => {
            blank_lines_before(text, at)
        }
        _ => at,
    };
    let mut next = String::with_capacity(text.len());
    next.push_str(&text[..from]);
    next.push_str(&format!("[{section}]{eol}"));
    for (key, value) in keys {
        next.push_str(&format!("{key} = {value}{eol}"));
    }
    next.push_str(&text[at + body.len()..]);
    next
}

/// The offset of the section header at `at`, with the empty lines in front of
/// it dropped, in the file's own line endings.
fn blank_lines_before(text: &str, at: usize) -> usize {
    let bytes = text.as_bytes();
    let mut end = at;
    loop {
        if end >= 2 && bytes[end - 1] == b'\n' && bytes[end - 2] == b'\n' {
            end -= 1;
        } else if end >= 4
            && bytes[end - 1] == b'\n'
            && bytes[end - 2] == b'\r'
            && bytes[end - 3] == b'\n'
            && bytes[end - 4] == b'\r'
        {
            end -= 2;
        } else {
            return end;
        }
    }
}

/// The first section of the record text `text` that runs the server profile
/// `profile`: the instance running it, `None` when no section does. From the
/// model data model a section runs a profile one step behind, through its
/// instance profile, so this is the same rule the launch reads
/// ([`pai_core::paths::instance_server_profile`]).
fn record_holder(text: &str, profile: &str) -> Option<String> {
    pai_core::ini::parse_sections(text)
        .into_iter()
        .map(|section| section.id)
        .find(|id| crate::paths::instance_server_profile(id).as_deref() == Some(profile))
}

/// Whether the section of the instance `id` in the record text `text` carries an
/// address of its own: a `Hostname` or a `Port` key with a value. The migration
/// writes both out of the server profile, and [`proposed_port`] feeds the
/// `Port` of a new section, so such an instance binds an address nobody else
/// holds, and its server profile's address is not its address.
fn record_owns_address(text: &str, id: &str) -> bool {
    record_key(text, id, "Hostname").is_some() || record_key(text, id, "Port").is_some()
}

/// The first section of the record text `text` that runs the server profile
/// `profile` AT the profile's own address: it names `profile` in its own
/// `ServerProfile` key and carries no address of its own. That is the instance
/// a create collides with: the record shape that binds the profile's
/// `Hostname` and `Port`, so a second instance of that shape would bind the very
/// same address. A section that runs a profile through its instance profile, or
/// carries a `Hostname` or a `Port` of its own, has an address of its own, so
/// the profile is there to share: "any number of instances on one node may
/// share an instance profile or a server profile, and `AddressInUse` is judged
/// on the records alone" (`ARCHITECTURE.md`, "Instance profiles", the port
/// option P1).
fn record_profile_owner(text: &str, profile: &str) -> Option<String> {
    pai_core::ini::parse_sections(text)
        .into_iter()
        .find(|section| {
            let held = |key: &str| section.keys.get(key).is_some_and(|v| !v.trim().is_empty());
            section
                .keys
                .get("ServerProfile")
                .is_some_and(|name| name.trim() == profile)
                && !(held("Hostname") || held("Port"))
        })
        .map(|section| section.id)
}

/// The `judge_model_files` of [`check_contents`] and [`Manager::check_and_write`]:
/// a write that judges the model files of the model profiles it puts in effect.
const MODEL_FILES_JUDGE: bool = true;
/// A write that leaves that check out: the create of a model profile, whose
/// profile is listed in no entry, names a model file judged when an entry lists
/// it and puts the profile in a render.
const MODEL_FILES_SKIP: bool = false;

/// The `judge_placement` of [`check_contents`] and [`Manager::check_and_write`]:
/// a write of `models.ini` that judges the whole pool for a placement or memory
/// key left in a model profile section, the key the migration moved into the
/// instance profile's entry: the save of the pool, which writes every section.
const PLACEMENT_JUDGE: bool = true;
/// A write that leaves that check out: the create of a model profile appends a
/// section it has judged key by key, and the delete takes a section out. A key
/// of a section neither verb writes is the node's own text, and the save that
/// writes that section is where it is judged.
const PLACEMENT_SKIP: bool = false;

/// The content checks of `file` for `contents`, against the files on disk: the
/// instance file against the Server rules, `models.ini` and
/// `models.ini.disabled` against the other model profile file, and both model
/// profile files for their model files: a model profile the write puts in
/// effect on an instance names files the node has, and a file that instance's
/// render is already missing is never re-judged; `judge_model_files` is
/// [`MODEL_FILES_SKIP`] where a write lands a profile that is switched off and
/// unassigned, whose file is judged at the switch-on or the assignment that
/// makes it render. Nothing of the fit is here (IP3c, `ARCHITECTURE.md`
/// "Convergence"): the memory the render needs, the GPUs its model profiles
/// name and its `load-on-startup` count are judged per instance, on the render
/// the pass writes ([`Manager::pair_fit`]), and the instance a check stops
/// keeps its last good pair. A refusal of what is here writes nothing.
fn check_contents(
    file: &ConfigFile,
    contents: &str,
    judge_model_files: bool,
    judge_placement: bool,
) -> Result<(), pai_core::checks::Refusal> {
    use pai_core::checks;
    let dir = crate::paths::engine_config_dir();
    let on_disk = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();
    // The pool the renders are made of once this write lands.
    let enabled = match file {
        ConfigFile::Models => contents.to_string(),
        _ => on_disk("models.ini"),
    };
    let disabled = match file {
        ConfigFile::ModelsDisabled => contents.to_string(),
        _ => on_disk("models.ini.disabled"),
    };
    match file {
        ConfigFile::ServerProfile(_) => {
            // The Server rules alone: no pool caps them any more, the
            // instance's render does, against the `ModelsMax` of this text.
            // This text caps a pool only where an instance runs it, so a
            // profile no instance names loads nothing and is judged on nothing.
            checks::server_file(contents, "")?;
            Ok(())
        }
        ConfigFile::Models => {
            let (old, off) = (on_disk("models.ini"), on_disk(MODELS_DISABLED_FILE));
            checks::models_file(contents, &old, &off, NO_MODELS_MAX)?;
            // A placement or memory key left in a model profile section lives in
            // the instance profile's entry section (`ARCHITECTURE.md`, "Instance
            // profiles", the placement and memory amendment): from the migration
            // on, a pool that carries one is a second source of truth for where
            // the weights go. A tree the migration has not written keeps its
            // keys in the pool, which is where they were. `judge_placement`
            // leaves the check out where the write does not touch the section
            // carrying the key.
            if judge_placement && record_is_migrated(&on_disk(INSTANCES_FILE)) {
                checks::model_profile_placement(contents)?;
            }
            checks::model_profile_ids_free(contents, &old, &off)?;
            // The model files the write puts in effect stay an intrinsic check of
            // the save (IP3c): the pool is what names a file, and a shard that is
            // not on the node is the news the save answers, on every instance that
            // renders the profile. The render the pass writes is never judged for
            // them (`pair_fit`): a node whose model folder loses a file while the
            // instance runs converges to the pair it has.
            if judge_model_files {
                check_model_files(&assignments_all(), &enabled, &disabled, &old, &off)?;
            }
            Ok(())
        }
        ConfigFile::ModelsDisabled => {
            let (old, enabled_file) = (on_disk("models.ini.disabled"), on_disk("models.ini"));
            checks::models_disabled_file(contents, &old, &enabled_file)?;
            checks::model_profile_ids_free(contents, &old, &enabled_file)?;
            // The model files, as the model profile pool's save judges them.
            if judge_model_files {
                check_model_files(&assignments_all(), &enabled, &disabled, &enabled_file, &old)?;
            }
            Ok(())
        }
    }
}

/// The GPUs of the render `render` against the cards instance `id` names
/// (`devices_today` the cards it has today, `devices_new` the ones it is about
/// to have): the first model profile naming a card outside, in the render's
/// order, that `today` did not already name outside refuses
/// (`model-profile-device-outside`, the check of L4). The render's own text is
/// judged, the way the apply writes it: from the migration on a render carries
/// the entry's placement keys, which no assignment row holds.
fn check_pair_devices(
    render: &str,
    today: &str,
    id: &str,
    devices_today: &str,
    devices_new: &str,
) -> Result<(), pai_core::checks::Refusal> {
    let was = outside_devices(today, devices_today);
    if let Some((profile, key, device)) = outside_devices(render, devices_new)
        .into_iter()
        .find(|named| !was.contains(named))
    {
        return Err(pai_core::checks::Refusal {
            key: "model-profile-device-outside",
            values: vec![profile, id.to_string(), key, device],
        });
    }
    Ok(())
}

/// Why one file of the model data model migration did not land: its name in the
/// engine's config folder, and the io kind of the read, the write or the delete
/// that stopped it.
struct MigrationFailure {
    name: String,
    error: std::io::ErrorKind,
}

/// The key every record section carries once the migration has written it: the
/// instance profile the instance runs. Its server profile is one step behind,
/// in that profile's section of `INSTANCE_PROFILES_FILE`.
const INSTANCE_PROFILE_KEY: &str = "InstanceProfile";

/// The instance profile the instance `id` runs: the `InstanceProfile` its
/// section of the record `record` names. A record the migration has not written
/// names none, and the instance is read the way it was before: its section of
/// `assignments.ini`, its `ServerProfile` key for the address.
fn instance_profile_of(record: &str, id: &str) -> Option<String> {
    record_key(record, id, INSTANCE_PROFILE_KEY)
}

/// The instance profile pool `instance-profiles.ini`, read as [`read_record`]
/// reads the record: a file the install has none of reads as an empty one, and
/// every other failure is the verb's own answer before anything is written.
/// Taken under the `writing` lock, as every write of the pool is.
fn read_pool() -> std::io::Result<String> {
    read_record(&crate::paths::engine_config_dir().join(INSTANCE_PROFILES_FILE))
}

/// The pool text `text` as the rows the manager answers: each profile its id,
/// the server profile its section names (blank when it names none), and the
/// model profiles of its entry sections, in the file's own order. The rows
/// carry no revision: [`Manager::instance_profile`] answers the text of the
/// whole file, the pair a page sends back to a switch.
fn profile_rows(text: &str) -> Vec<pai_core::client::InstanceProfileInfo> {
    pai_core::instance_profiles::parse(text)
        .into_iter()
        .map(|profile| pai_core::client::InstanceProfileInfo {
            id: profile.id,
            server_profile: profile.server_profile.unwrap_or_default(),
            entries: profile
                .entries
                .into_iter()
                .map(|entry| entry.model)
                .collect(),
            revision: String::new(),
        })
        .collect()
}

/// The first instance whose record section runs the instance profile
/// `profile`: the holder that keeps a profile from being deleted. The record is
/// walked in its own order, so a profile two instances run is named with the
/// first of them.
fn instance_profile_holder(record: &str, profile: &str) -> Option<String> {
    pai_core::ini::parse_sections(record)
        .into_iter()
        .find(|section| {
            section
                .keys
                .get(INSTANCE_PROFILE_KEY)
                .is_some_and(|held| held.trim() == profile)
        })
        .map(|section| section.id)
}

/// The instance whose record section carries the id `profile`: the profile of
/// an instance's own, the one the migration wrote for every instance from its
/// id (`migrate_model`). It is that instance's profile in the strongest sense,
/// so a delete of it is refused while the instance is listed, and the refusal
/// names the profile the instance runs today, which is what the caller has to
/// know to finish the job.
fn instance_owning_profile(record: &str, profile: &str) -> Option<String> {
    pai_core::ini::parse_sections(record)
        .into_iter()
        .find(|section| section.id == profile)
        .map(|section| section.id)
}

/// The text `text` with the section `section` cut out, every other byte kept
/// ([`pai_core::ini::text::delete_section`]). Every section cut through here is
/// one the parse of the same text listed under the `writing` lock, so a `None`
/// is a file that moved under the lock: the verb's own answer, as a read
/// failure is.
fn cut_section(text: &str, section: &str) -> Result<String, ManagerError> {
    pai_core::ini::text::delete_section(text, section)
        .ok_or(ManagerError::Io(std::io::ErrorKind::NotFound))
}

/// The raw text of one section `section` (a header line and the lines after
/// it) appended to the pool text `text` at the end, in the pool's own shape:
/// a blank line, the section, the file's line ending.
fn append_pool_section(text: &str, section: &str) -> String {
    let eol = pai_core::ini::detect_eol(text);
    let mut out = text.trim_end().to_string();
    out.push_str(eol);
    out.push_str(eol);
    out.push_str(section.trim_end_matches(['\r', '\n']));
    out.push_str(eol);
    out
}

/// The raw text of a section `body` with its header line rewritten to
/// `[name]`, in the header line's own line ending: the keys, comments and
/// order after the header are the source's, byte for byte.
fn renamed_section(body: &str, name: &str) -> String {
    match body.split_once('\n') {
        Some((line, rest)) => {
            let eol = if line.ends_with('\r') { "\r\n" } else { "\n" };
            format!("[{name}]{eol}{rest}")
        }
        None => format!("[{name}]"),
    }
}

/// The key `key` of the section `section` of the text `text`, trimmed, `None`
/// when the section is absent, the key is absent, or its value is blank.
fn record_key(text: &str, section: &str, key: &str) -> Option<String> {
    pai_core::ini::section_keys(text, section)
        .into_iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// The model profile ids of the entry sections of the instance profile
/// `profile`: its `[<profile>/<m>]` sections, in the file's own order. The entry
/// sections ARE the model list, so this is the list of model profiles the
/// instance is given, and an empty list is an instance given nothing.
fn entry_models(profiles: &str, profile: &str) -> Vec<String> {
    let head = format!("{profile}/");
    pai_core::ini::parse_sections(profiles)
        .into_iter()
        .filter_map(|section| section.id.strip_prefix(&head).map(|id| id.to_string()))
        .collect()
}

/// The text of instance `id`'s render `models@<id>.ini`, from the pool `pool`
/// and the instance profile `profile` of the pool of profiles `profiles`: the
/// entry sections are the model profiles the instance is given, each one the
/// pool's section with the entry's placement and memory keys over it
/// (`ARCHITECTURE.md`, "Instance profiles", the placement and memory
/// amendment). The render of a ROCm instance carries the load rule of
/// `ARCHITECTURE.md` "ROCm instances load models with `load-mode = none`"
/// (GT5a), added by [`with_rocm_load_mode`].
fn render_instance(id: &str, pool: &str, profiles: &str, profile: &str) -> String {
    with_rocm_load_mode(
        id,
        pai_core::assign::render_profile(pool, profiles, profile),
    )
}

/// The record is in the new shape: every section of it names its instance
/// profile, the key the migration wrote in place of `ServerProfile`.
/// `pai_core::migrate_model` answers the record and the instance profiles
/// unchanged from that point on; the manager reads the same rule as the
/// leave-ahead of `assignments.ini` and `models.ini.disabled`, the two files the
/// new shape has no place for: the address an instance listens on sits in the
/// record before any server profile gives it up, so the two go once the record
/// is the migration's own. A record with no section names no instance to move:
/// migrated, and a stray `assignments.ini` is a file nothing reads.
fn record_is_migrated(record: &str) -> bool {
    pai_core::ini::parse_sections(record)
        .iter()
        .all(|section| section.keys.contains_key(INSTANCE_PROFILE_KEY))
}

/// The backup of a file the migration changes, `<name>.2.99.0` of `<name>`,
/// written once: a backup that is already there is kept as it is, so the
/// install's own text survives every later start, and a file that is not there
/// needs no backup of its own. `new` is the text this start writes for the
/// file, `None` when the start takes the file away: a file that already holds
/// what the start writes is left where it is, so no start backs up the new
/// shape as the install's own text (IP2 review D1). The copy goes to a temp
/// name and is renamed into place, so a start that dies mid-copy leaves no
/// truncated backup for the next start to keep as the install's text, and a
/// copy that fails leaves no entry at all (IP2 review D3).
fn ensure_backup(dir: &std::path::Path, name: &str, new: Option<&str>) -> std::io::Result<()> {
    let backup = dir.join(format!("{name}{BACKUP_SUFFIX}"));
    if backup.exists() {
        return Ok(());
    }
    let live = dir.join(name);
    let text = match std::fs::read_to_string(&live) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if Some(text.as_str()) == new {
        return Ok(());
    }
    let tmp = dir.join(format!("{name}{BACKUP_SUFFIX}.tmp"));
    std::fs::copy(&live, &tmp)
        .and_then(|_| std::fs::rename(&tmp, &backup))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
}

/// The text of the file `name` of the engine's config folder, an empty text when
/// the file is missing. `None` when the read failed for any other reason, the
/// reason answered in `failures`: a tree that cannot be read is left as it
/// stands, and the start says so.
fn read_tree(
    dir: &std::path::Path,
    name: &str,
    failures: &mut Vec<MigrationFailure>,
) -> Option<String> {
    match std::fs::read_to_string(dir.join(name)) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(String::new()),
        Err(e) => {
            failures.push(MigrationFailure {
                name: name.to_string(),
                error: e.kind(),
            });
            None
        }
    }
}

/// One migrated file, written as a served file: the writer of
/// [`Manager::write_file`], so a migrated file keeps the mode of the file it
/// replaces, and the group of an instance file. A file whose bytes already
/// match is skipped: the migration is idempotent, so a start of a migrated
/// tree rewrites nothing, and a file the start does not change keeps its own
/// mtime (IP2 review D1). A write that failed is answered in `failures`, in
/// order with the rest.
fn write_migrated(
    dir: &std::path::Path,
    file: &ConfigFile,
    name: &str,
    text: &str,
    group: &dyn FileGroup,
    failures: &mut Vec<MigrationFailure>,
) {
    if std::fs::read_to_string(dir.join(name)).is_ok_and(|now| now == text) {
        return;
    }
    if let Err(e) = write_served_file(&dir.join(name), file, text, group) {
        failures.push(MigrationFailure {
            name: name.to_string(),
            error: e.kind(),
        });
    }
}

/// The server profile names the instances run, each one once, in name order:
/// the files the migration takes its address out of. The names are the rule the
/// launch reads (`paths::instance_server_profile`): the `ServerProfile` of the
/// record's own section while the record is unmigrated, and from the migration
/// on the `ServerProfile` of the instance profile the record names, so a tree
/// that stopped with the record written still names the profiles it has to
/// strip (IP2 review D4a).
fn server_profile_names() -> Vec<String> {
    let mut names: Vec<String> = pai_core::paths::instance_ids()
        .iter()
        .filter_map(|id| crate::paths::instance_server_profile(id))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The text of instance `id`'s render `models@<id>.ini`, from the pool pair and
/// the model list the instance is given: what the checks judge. From the model
/// data model the entry sections of the instance profile ARE that list, so
/// [`assignment_of`] reads the record and `instance-profiles.ini` and answers
/// them spelled as `assignments.ini` spelled them; before the migration that
/// file answers the instance's section as it stands. The render of a ROCm
/// instance carries the load rule of `ARCHITECTURE.md` "ROCm instances load
/// models with `load-mode = none`" (GT5a), added by [`with_rocm_load_mode`].
fn render_of(id: &str, enabled: &str, disabled: &str) -> String {
    with_rocm_load_mode(
        id,
        pai_core::assign::render(enabled, disabled, assignment_of(id).as_deref()),
    )
}

/// The model list of instance `id`, spelled as `assignments.ini` spells it: a
/// `[<id>]` section with one `on` key per model profile the instance is given.
/// The entry sections of the instance profile the record names are that list
/// (`ARCHITECTURE.md`, "Instance profiles", the placement and memory
/// amendment), no entry included, which is an instance given nothing. A record
/// the migration has not written names no instance profile, and `assignments`
/// answers the instance's section as it stands; a record with no section for
/// `id` is an instance given nothing too.
fn assignment_text(record: &str, profiles: &str, assignments: &str, id: &str) -> Option<String> {
    if let Some(profile) = instance_profile_of(record, id) {
        let mut text = format!("[{id}]\n");
        for model in entry_models(profiles, &profile) {
            text.push_str(&format!("{model} = on\n"));
        }
        return Some(text);
    }
    pai_core::ini::text::section_text(assignments, id)
}

/// The three files the model lists are read from: the record, the instance
/// profile pool, and `assignments.ini`. A file that is not there is an empty one.
fn assignment_files() -> (String, String, String) {
    let dir = crate::paths::engine_config_dir();
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();
    (
        read(INSTANCES_FILE),
        read(INSTANCE_PROFILES_FILE),
        read(ASSIGNMENTS_FILE),
    )
}

/// [`assignment_text`] over the files on disk.
fn assignment_of(id: &str) -> Option<String> {
    let (record, profiles, assignments) = assignment_files();
    assignment_text(&record, &profiles, &assignments, id)
}

/// Every instance's model list as one `assignments.ini`: [`assignment_text`] for
/// each section of the record, in the record's own order. The checks that judge
/// every instance's render take the model list in this shape, so they read the
/// entry sections of the instance profiles from the migration on, and the file
/// itself before it.
fn assignments_all() -> String {
    let (record, profiles, assignments) = assignment_files();
    let mut text = String::new();
    for section in pai_core::ini::parse_sections(&record) {
        if let Some(assignment) = assignment_text(&record, &profiles, &assignments, &section.id) {
            text.push_str(&assignment);
        }
    }
    text
}

/// The render `text` of instance `id`, with `load-mode = none` in its `[*]`
/// section when that section carries no `load-mode` of its own
/// (`ARCHITECTURE.md` "ROCm instances load models with `load-mode = none`",
/// slice GT5a): on ROCm an mmapped model's weights are pinned by the runtime,
/// and `none` loads them without that. The line lives in the render alone: the
/// pool files are never written, so a profile moved to a non-ROCm instance
/// loses it and the pages keep showing the pool's own value. An explicit pool
/// `[*]` value is copied as it is, and a model section's own key is left
/// alone: llama-server puts a model section over `[*]`, and the launch passes
/// no `--load-mode` (`launch.rs` keeps the key per preset, so the command line
/// never overrides a profile's choice). Linux only: every other system's
/// render is unchanged.
#[cfg(target_os = "linux")]
fn with_rocm_load_mode(id: &str, text: String) -> String {
    if !instance_is_rocm(id) || render_has_load_mode(&text) {
        return text;
    }
    if pai_core::ini::text::section_text(&text, "*").is_some() {
        return pai_core::ini::text::set_key(&text, "*", "load-mode", "none");
    }
    // The pool has no `[*]`: the render opens one after its header line, where
    // the pool's own `[*]` sits when it has one, in the render's line ending
    // and with the blank line that separates two sections.
    let eol = pai_core::ini::detect_eol(&text);
    let head = text.find('\n').map(|i| i + 1).unwrap_or(text.len());
    format!(
        "{}[*]{eol}load-mode = none{eol}{eol}{}",
        &text[..head],
        &text[head..]
    )
}

/// The same render on the other systems, unchanged
/// (`ARCHITECTURE.md`, the section above): the rule is Linux's.
#[cfg(not(target_os = "linux"))]
fn with_rocm_load_mode(_id: &str, text: String) -> String {
    text
}

/// Whether the render `text` carries a `load-mode` in its `[*]` section: the
/// pool's own value, copied byte for byte by `pai_core::assign::render`, is
/// kept as it stands. A key in any letter case, the way the reader reads one.
#[cfg(target_os = "linux")]
fn render_has_load_mode(text: &str) -> bool {
    pai_core::ini::parse_sections(text)
        .into_iter()
        .find(|section| section.id == "*")
        .map(|section| {
            section
                .keys
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case("load-mode"))
        })
        .unwrap_or(false)
}

/// Whether instance `id` is a ROCm instance (`ARCHITECTURE.md`, the section
/// above): its server profile's `Devices` names a device whose backend is
/// `rocm`, or leaves `Devices` unset (every card of the node, the reading of
/// [`instance_devices`]) while the engine's installed backends hold `rocm`.
/// `Devices = none` (a CPU-only profile) and a list of only Vulkan or CUDA
/// cards are not. The engine's own device probe is not asked: it spawns the
/// engine at every render, for an answer these two files already give.
#[cfg(target_os = "linux")]
fn instance_is_rocm(id: &str) -> bool {
    let devices = instance_devices(id);
    if devices.is_empty() {
        return engine_has_rocm();
    }
    devices
        .split(',')
        .any(|device| pai_core::devices::backend_of(device) == "rocm")
}

/// Whether the engine installed on this node has a `rocm` runtime: the version
/// files of its package (`pai_core::versions::installed`, one `llama.cpp-<name>`
/// file per runtime), the same read the web UI's node page makes
/// ([`pai_core::bff_cfg::versions_dir`]). Where the install ships no such
/// folder (a tree build, and a test redirecting the data root) the files are
/// read beside the pool files.
#[cfg(target_os = "linux")]
fn engine_has_rocm() -> bool {
    let dir = pai_core::bff_cfg::versions_dir()
        .unwrap_or_else(|| crate::paths::engine_config_dir().join("versions"));
    pai_core::versions::installed(&dir)
        .into_iter()
        .flat_map(|engine| engine.backends)
        .any(|backend| backend.name == "rocm")
}

/// The record `seed` with the key of model profile `profile` in the section of
/// `instance` set to `value`, and removed for `none`. The section is created
/// when the instance has none, with the file's own line ending. Every other
/// byte of `seed` is kept: the comments, the keys that are no model profile,
/// and the blank line in front of the next section's header.
fn with_profile(seed: &str, instance: &str, profile: &str, value: &str) -> String {
    if value == "none" {
        return pai_core::ini::text::remove_key(seed, instance, profile);
    }
    if pai_core::ini::text::section_text(seed, instance).is_some() {
        return pai_core::ini::text::set_key(seed, instance, profile, value);
    }
    // The instance has no section: the new one carries its own header (a key
    // list appended without one reads back as an empty section), and is
    // appended as it is, with no blank line of its own.
    let eol = pai_core::ini::detect_eol(seed);
    let mut out = seed.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(eol);
    }
    out.push_str(&format!("[{instance}]{eol}{profile} = {value}{eol}"));
    out
}

/// Whether the pool `text` holds a model profile `id`: a section of that exact
/// name, the spelling llama.cpp knows the model profile by.
fn pool_holds(text: &str, id: &str) -> bool {
    pai_core::ini::parse_sections(text)
        .iter()
        .any(|section| section.id == id)
}

/// The render of instance `id` over the record `record`: the entry sections of
/// the instance profile the record names, once the migration has written one,
/// and the pool pair cut by `assignments.ini` before that.
fn render_of_id(id: &str, record: &str) -> String {
    let dir = crate::paths::engine_config_dir();
    let pool = std::fs::read_to_string(dir.join("models.ini")).unwrap_or_default();
    match instance_profile_of(record, id) {
        Some(profile) => render_instance(
            id,
            &pool,
            &std::fs::read_to_string(dir.join(INSTANCE_PROFILES_FILE)).unwrap_or_default(),
            &profile,
        ),
        None => render_of(
            id,
            &pool,
            &std::fs::read_to_string(dir.join(MODELS_DISABLED_FILE)).unwrap_or_default(),
        ),
    }
}

/// The text of `launch@<id>.ini`: the server config instance `id` launches
/// with, the profile its record resolves with the record's host and port over it
/// (`instance_cfg`, the same `launch::with_record_address` the launch reads, L1:
/// the launch and the render name the same address). `None` when the record
/// names no profile, or names one the engine has no readable file for: the pass
/// writes no launch for that instance.
fn launch_render(id: &str) -> Option<String> {
    Some(crate::server_cfg::render_for(&instance_cfg(id)?, true))
}

/// Whether the pair files of instance `id` already hold `pair` (the render and
/// the launch the pass gives): a pair that stands is written by no pass, changes
/// no byte and no mtime, and is the instance converged (IP3c). A launch with
/// nothing to write, and an id with no file of its own, put nothing of
/// themselves into the pair.
fn pair_stands(id: &str, pair: &(String, Option<String>)) -> bool {
    let on_disk =
        |path: Option<std::path::PathBuf>| path.and_then(|path| std::fs::read_to_string(path).ok());
    let launch = on_disk(crate::launch::instance_launch_ini(id));
    on_disk(crate::paths::instance_models(id)).as_deref() == Some(pair.0.as_str())
        && match &pair.1 {
            Some(want) => launch.as_deref() == Some(want.as_str()),
            None => true,
        }
}

/// The file a launched instance records the pair it started with in: `applied`
/// in its own state folder, which the instance's account writes and the manager
/// reads (`ARCHITECTURE.md` "Convergence", IP3c).
pub const APPLIED_FILE: &str = "applied";

/// `applied` in the state folder of instance `id`. None for an invalid id.
pub fn applied_file(id: &str) -> Option<std::path::PathBuf> {
    crate::paths::valid_instance_id(id).then(|| {
        crate::paths::instance_state_dir()
            .join(id)
            .join(APPLIED_FILE)
    })
}

/// The digest of the pair instance `id` applies: the bytes of `models@<id>.ini`
/// followed by those of `launch@<id>.ini`, SHA-256 in lowercase hex. The shape
/// IP3d's reconcile digest keeps. None when either file cannot be read.
pub fn pair_digest(id: &str) -> Option<String> {
    let models = std::fs::read(crate::paths::instance_models(id)?).ok()?;
    let launch = std::fs::read(crate::launch::instance_launch_ini(id)?).unwrap_or_default();
    let mut digest = Sha256::new();
    digest.update(&models);
    digest.update(&launch);
    Some(hex(&digest.finalize()))
}

/// The digest the last launch of instance `id` recorded, as lowercase hex. None
/// when there is no record.
pub fn applied_digest(id: &str) -> Option<String> {
    let text = std::fs::read_to_string(applied_file(id)?).ok()?;
    let hex = text.trim().to_ascii_lowercase();
    (!hex.is_empty()).then_some(hex)
}

/// Records the pair the launch of instance `id` starts with: the digest of its
/// rendered pair, in `applied` under the instance's state folder. The launch
/// reports a failure and starts anyway: the file belongs to the instance's own
/// account, and a node where that account cannot write it has no drift to
/// report, which is no reason to refuse a launch (`ARCHITECTURE.md`
/// "Convergence", IP3c).
pub fn write_applied(id: &str) -> std::io::Result<()> {
    let (file, digest) = applied_file(id)
        .zip(pair_digest(id))
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&file, digest)
}

/// The lowercase hex of the digest bytes, the shape every digest file carries.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// One instance a render pass refused: the instance, and the keyed check that
/// stopped its pair, with the arguments that check carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotConverged {
    pub instance: String,
    pub key: String,
    pub args: Vec<String>,
}

/// What a save that re-renders answers (`ARCHITECTURE.md` "Convergence", IP3c):
/// the instances the save moved, and the instances it leaves not converged. The
/// first are the ones the page offers a restart to, the second the ones whose
/// pair the save's render refused, each with that refusal's key, beside them.
/// Both are derived from the files the save left, never kept, the way the rows
/// of `instances` are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Affected {
    pub running: Vec<String>,
    pub not_converged: Vec<NotConverged>,
}

/// The render pass's refusals as the answer's rows, in the pass's order.
fn not_converged_of(pass: &[(String, pai_core::checks::Refusal)]) -> Vec<NotConverged> {
    pass.iter()
        .map(|(id, refusal)| NotConverged {
            instance: id.clone(),
            key: refusal.key.to_string(),
            args: refusal.values.clone(),
        })
        .collect()
}

/// The (model profile, key, device) triples of the render `render` naming a GPU
/// outside the `Devices` value `devices`, in the render's order: every section
/// of the render's own, the `*` global section skipped, judged by
/// [`pai_core::checks::devices_outside`] on the section's device key lines, the
/// `override-tensor` rule line with them (J1, the key on the web), with the
/// render's `[*]` lines before them, the way llama.cpp cascades the global
/// section into every model profile.
fn outside_devices(render: &str, devices: &str) -> Vec<(String, String, String)> {
    let sections = pai_core::ini::parse_sections(render);
    let global = sections.iter().find(|section| section.id == "*");
    let mut out = Vec::new();
    for section in &sections {
        if section.id == "*" {
            continue;
        }
        // The cascaded line first and the section's own after it: the reader
        // keeps the last line of a key.
        let mut keys = String::new();
        let named = pai_core::checks::DEVICE_LIST_KEYS[1..]
            .iter()
            .copied()
            .chain(std::iter::once("override-tensor"));
        for key in named {
            for from in [global, Some(section)] {
                if let Some(value) = from
                    .and_then(|s| s.keys.get(key))
                    .map(String::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    keys.push_str(&format!("{key} = {value}\n"));
                }
            }
        }
        if let Some((key, device)) = pai_core::checks::devices_outside(&keys, devices) {
            out.push((section.id.clone(), key, device));
        }
    }
    out
}

/// The count `count` against `models_max`, as a check of instance `id`: the
/// check's own keyed refusal, with the instance it stopped named last.
fn startup_refusal(
    count: usize,
    models_max: Option<i32>,
    id: &str,
) -> Result<(), pai_core::checks::Refusal> {
    pai_core::checks::startup_count(count, models_max).map_err(|mut refusal| {
        refusal.values.push(id.to_string());
        refusal
    })
}

/// The `load-on-startup` count of the render `render` of instance `id`, grown
/// over the count of the render on disk (`today`), against the `ModelsMax` the
/// pair names (`cap`: the cap the launch carries, the pair on disk measured on
/// its own launch, `cap_today`). The count is one instance's own, its model
/// list and its cap, so it is a fit check of that instance and never a check of
/// the save (`ARCHITECTURE.md` "Convergence", IP3c review 7): the write that
/// grows one instance's list over that instance's cap lands, and the instance
/// that fits is written. The growth rule of every fit check: a render that
/// already stands over the cap is no news, so the save that shrinks it lands.
/// The count that SHRINKS is never news: a step down lands even when the list
/// it leaves is still over the cap, which is what the step rule of
/// `SetModelProfile` always allowed. The count that STANDS is news when the cap
/// is news: the pair on disk ran under no cap of its own (`cap_today` is none,
/// a first render, a profile whose key is commented) and the pair now names
/// one, which is what a switch to a capped profile brings. A save of the server
/// profile the instance runs, lowering the cap the pair stands under, is judged
/// on its growth only and so lands (the growth rule, the same one
/// `manager_checks_e2e` pins). A pair with no cap to be judged on now (no
/// launch and no readable server profile, the unlimited pool) is judged on
/// nothing.
fn check_pair_startup_count(
    render: &str,
    today: &str,
    cap: Option<i32>,
    cap_today: Option<i32>,
    id: &str,
) -> Result<(), pai_core::checks::Refusal> {
    let Some(cap) = cap else {
        return Ok(());
    };
    let count = pai_core::checks::startup_loads(render);
    let standing = pai_core::checks::startup_loads(today);
    if count < standing || (count == standing && cap_today.is_some()) {
        return Ok(());
    }
    startup_refusal(count, Some(cap), id)
}

/// The cards the pair `pair` names: the `Devices` of its launch text, the pair
/// being rendered and the pair on disk each measured on their own (IP3c). A
/// pair with no launch half, and a launch whose server profile the record
/// cannot be read for, name every card, as the launch and `instance_devices`
/// read them.
fn pair_devices(pair: &(String, Option<String>), id: &str) -> String {
    match &pair.1 {
        Some(text) => crate::server_cfg::from_text(text)
            .devices
            .unwrap_or_default(),
        None => instance_devices(id),
    }
}

/// The `ModelsMax` the pair `pair` names, as [`pair_devices`] reads its cards:
/// the cap the launch carries, `None` when the pair has no launch half and the
/// record names no readable server profile, or the key is commented (the
/// unlimited pool): the instance has nothing to be judged on.
fn pair_cap(pair: &(String, Option<String>), id: &str) -> Option<i32> {
    match &pair.1 {
        Some(text) => crate::server_cfg::from_text(text).models_max,
        None => profile_file(id)
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| crate::server_cfg::from_text(&text).models_max),
    }
}

/// The model files of every instance's render, in id order: the first render
/// naming a file the write ADDS refuses. `assignments`, `enabled` and
/// `disabled` are the record and the pool pair the write leaves; `old` and
/// `off` are the pool pair on disk, which with the record on disk (read by
/// `render_of`) is the render today.
fn check_model_files(
    assignments: &str,
    enabled: &str,
    disabled: &str,
    old: &str,
    off: &str,
) -> Result<(), pai_core::checks::Refusal> {
    for id in crate::paths::instance_ids() {
        check_model_files_of(
            assignments,
            &id,
            enabled,
            disabled,
            &render_of(&id, old, off),
        )?;
    }
    Ok(())
}

/// A `model` value as the instance sees it: absolute as it stands, else under
/// `cwd`, the folder the instance inherits. The one resolution the GPU memory
/// check and `ModelInfo` share, so the sliders and the check read the same
/// file.
fn model_path(model: &str, cwd: &std::path::Path) -> std::path::PathBuf {
    let file = std::path::Path::new(model);
    if file.is_absolute() {
        file.to_path_buf()
    } else {
        cwd.join(file)
    }
}

/// The shard cap of [`Manager::model_info`]'s read: a split model whose name
/// declares more shards than this is refused as `model-info-unreadable` naming
/// the model, with no shard opened. The bound is the design's, "Bounds and
/// caching" in ARCHITECTURE.md.
const MAX_MODEL_SHARDS: usize = 256;

/// The pool's section of the model profile `profile`, None when the pool has
/// none. `*` is the pool's global section, never a profile.
fn profile_section<'a>(
    sections: &'a [pai_core::ini::Section],
    profile: &str,
) -> Option<&'a pai_core::ini::Section> {
    sections.iter().find(|s| s.id == profile && s.id != "*")
}

/// The model files instance `id`'s render names, over the pool pair and the
/// assignment record `assignments`: the render the write is about to produce.
/// Every model profile in effect has a `model` whose every file (every shard
/// of a split one) is a regular file: the `metadata` call follows a symlink,
/// so a link to a file is that file, while a directory, a FIFO or a device is
/// a file that is not there. A value that is not absolute is looked for in
/// `launch::instance_current_dir`, the folder the router and its children
/// inherit, and the refusal names the path the check looked at, the resolved
/// one. The refusal is `model-profile-model-missing` with
/// `[model profile, instance, path]`. The check reads no server profile, so it
/// runs on an instance whose profile file is missing too: its launch is
/// refused for the profile, and a model file is a fact about the node.
///
/// The check refuses only a file the write ADDS: a (model profile, path) pair
/// that the instance's render today (`today`: the render of the pool pair and
/// the record on disk) is already missing is left alone, so a switch off, an
/// assignment removed, a `*` narrowed, or a save that touches only another
/// model profile, never fails on a file a model profile already in effect lost
/// (ARCHITECTURE.md (b), "Which writes run them"). The first missing pair of
/// the new render that is not missing today refuses, the instances answered in
/// id order.
fn check_model_files_of(
    assignments: &str,
    id: &str,
    enabled: &str,
    disabled: &str,
    today: &str,
) -> Result<(), pai_core::checks::Refusal> {
    let assignment = pai_core::ini::text::section_text(assignments, id);
    let render = pai_core::assign::render(enabled, disabled, assignment.as_deref());
    let cwd = crate::launch::instance_current_dir(
        cfg!(windows),
        &crate::paths::data_root(),
        &crate::paths::instance_state_dir(),
    );
    let was = missing_model_files(today, &cwd);
    if let Some((profile, path)) = missing_model_files(&render, &cwd)
        .iter()
        .find(|pair| !was.contains(pair))
    {
        return Err(pai_core::checks::Refusal {
            key: "model-profile-model-missing",
            values: vec![profile.clone(), id.to_string(), path.clone()],
        });
    }
    Ok(())
}

/// The model files the render `render` names that are not on the node, as the
/// (model profile, path) pair of each, in the order the check looks at them:
/// the sections in the render's order, and for each the files of its `model`
/// value in the order [`pai_core::checks::model_files_of`] lists them, the
/// path the check looked at. A model profile carrying one of [`FETCH_KEYS`] is
/// skipped, the engine putting that file on the node itself.
fn missing_model_files(render: &str, cwd: &std::path::Path) -> Vec<(String, String)> {
    let mut missing = Vec::new();
    for section in pai_core::ini::parse_sections(render) {
        let get = |key: &str| section.keys.get(key).map(String::as_str);
        if FETCH_KEYS
            .iter()
            .any(|key| get(key).is_some_and(|v| !v.trim().is_empty()))
        {
            continue;
        }
        for file in pai_core::checks::model_files_of(get("model").unwrap_or_default()) {
            let path = model_path(&file, cwd);
            if !std::fs::metadata(&path).is_ok_and(|f| f.is_file()) {
                missing.push((section.id.clone(), path.to_string_lossy().into_owned()));
            }
        }
    }
    missing
}

// ── The model profile memory check (ARCHITECTURE.md step 10, item 13) ──────

/// The memory check of one instance's render `render`, named `id` in the
/// refusal, against the render that same instance has on disk (`today`): the
/// first model profile of `render`, in the render's order, that this pair
/// makes larger AND whose own estimate is over the usable GPU memory of the
/// cards the pair's launch names (`devices_new`; R3: `Devices`, every card
/// when the key is absent, nothing for `none` or a `Devices` whose ids the
/// probe does not know) refuses with `model-profile-over-memory` and
/// `[model profile, instance, needed MiB, available MiB]`, the need rounded up
/// to a MiB and the memory the node has rounded down to one
/// (ARCHITECTURE.md (b) 1, 2 and 3). The need is that one model profile's
/// estimate, never a sum over the render: two model profiles that each fit the
/// node land in one write.
///
/// A section grew when its estimate is larger than the estimate of the section
/// of the same id in `today` (0 when `today` has no such section, so the whole
/// of its need is this write's). A save of a key the estimate does not read, a
/// save that changes no byte, a switch off and an `[*]` that changes nothing
/// grow nothing, so a model profile that stands over the node's memory today
/// (GPUs swapped, a hand edit) is never refused by a write beside it.
///
/// The cards narrowing is news of its own: the pair on disk fits the cards its
/// own launch names (`devices_today`), so a pair that names fewer of them puts
/// a need that fits today over the memory the instance has, and a need that
/// fits the new memory is no news. Each side is measured on ITS OWN cards, the
/// pair being rendered on the ones its launch carries and the pair on disk on
/// the ones its launch carries (`ARCHITECTURE.md` "Convergence", IP3c review:
/// fit is judged against the pair on disk).
///
/// The estimates come from the files and the GPU totals from the seam
/// [`GpuTotals`], which runs a probe: both renders are estimated and the growth
/// found first, and the seam is asked only once a section has grown or the
/// cards narrowed. A node the seam answers no pool for, an empty pool
/// included, is measured on nothing, and so is an instance whose server profile
/// names no card the probe knows.
fn check_render_memory(
    render: &str,
    today: &str,
    id: &str,
    gpus: &dyn GpuTotals,
    devices_today: &str,
    devices_new: &str,
) -> Result<(), pai_core::checks::Refusal> {
    let cwd = crate::launch::instance_current_dir(
        cfg!(windows),
        &crate::paths::data_root(),
        &crate::paths::instance_state_dir(),
    );
    let was = estimated_of(today, &cwd);
    // Today's estimate of the same model profile, by section id: a
    // byte-identical section estimates the same, so it is no growth.
    let baseline = |section: &str| {
        was.iter()
            .find(|was| was.id == section)
            .map(|was| was.bytes)
            .unwrap_or(0)
    };
    let need = estimated_of(render, &cwd);
    let grown: Vec<&Estimated> = need
        .iter()
        .filter(|need| need.bytes > baseline(&need.id))
        .collect();
    // The seam runs the probe, so it is asked only when there is something to
    // measure: a render with no estimate measures nothing on any card, and a
    // pair whose launch names the cards the pair on disk names measures nothing
    // when no section grew.
    if need.is_empty() || (grown.is_empty() && devices_today.trim() == devices_new.trim()) {
        return Ok(());
    }
    let Some(pool) = gpus.gpus() else {
        return Ok(());
    };
    let available = pai_core::devices::devices_usable(&pool, devices_new);
    if available == 0 {
        return Ok(());
    }
    let narrowed = pai_core::devices::devices_usable(&pool, devices_today) > available;
    let judged: Vec<&Estimated> = if narrowed {
        need.iter().collect()
    } else {
        grown
    };
    if let Some(need) = judged.into_iter().find(|need| need.bytes > available) {
        return Err(pai_core::checks::Refusal {
            key: "model-profile-over-memory",
            values: vec![
                need.id.clone(),
                id.to_string(),
                (need.bytes.div_ceil(pai_core::gguf_shape::MIB)).to_string(),
                (available / pai_core::gguf_shape::MIB).to_string(),
            ],
        });
    }
    Ok(())
}

/// One model profile of a render that has an estimate.
struct Estimated {
    id: String,
    bytes: u64,
}

/// The estimate of every model profile the render `render` puts in effect, in
/// the render's order: a section of the render's own `[*]`, one with a
/// [`FETCH_KEYS`] key, one naming no `model`, and one whose `model` is not a
/// GGUF [`pai_core::gguf_shape::local_shape`] can read, have none. The four
/// keys are the section's own, else the `[*]` section's, llama.cpp cascading
/// the global section into every model profile; a `ctx-size` that is absent,
/// `0` or not an integer is the context the model was trained on, `REF_CTX`
/// when the header gives none, and a cache type that is absent is `f16`, the
/// engine's default. The `model` is resolved as in the model-file check:
/// absolute, else in `cwd`, the folder the instance inherits.
fn estimated_of(render: &str, cwd: &std::path::Path) -> Vec<Estimated> {
    let global = pai_core::ini::parse_sections(render)
        .into_iter()
        .find(|section| section.id == "*");
    let mut out = Vec::new();
    for section in pai_core::ini::parse_sections(render) {
        if section.id == "*" {
            continue;
        }
        let own = |key: &str| {
            section
                .keys
                .get(key)
                .map(String::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        // The section's own key, else the render's `[*]` one.
        let get = |key: &str| {
            own(key).or_else(|| {
                global
                    .as_ref()
                    .and_then(|g| g.keys.get(key))
                    .map(String::as_str)
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
            })
        };
        if FETCH_KEYS.iter().any(|key| own(key).is_some()) {
            continue;
        }
        let Some(model) = get("model") else {
            continue;
        };
        let path = model_path(&model, cwd);
        let Some(shape) = pai_core::gguf_shape::local_shape(&path).ok() else {
            continue;
        };
        // `0` is llama.cpp's "unset", and a value that is not a number leaves
        // the reading of the key to the launch: both are the context the model
        // was trained on, `REF_CTX` when the header gives none.
        let ctx = get("ctx-size")
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|ctx| *ctx != 0)
            .unwrap_or(shape.trained_ctx);
        let ctx = if ctx == 0 {
            pai_core::gguf_shape::REF_CTX
        } else {
            ctx
        };
        let full = pai_core::gguf_shape::full_gpu_bytes(
            &shape,
            ctx,
            get("cache-type-k").as_deref().unwrap_or("f16"),
            get("cache-type-v").as_deref().unwrap_or("f16"),
        );
        out.push(Estimated {
            id: section.id,
            bytes: full,
        });
    }
    out
}

/// `id` if it is a valid instance id.
fn valid_id(id: &str) -> Result<(), ManagerError> {
    if crate::paths::valid_instance_id(id) {
        Ok(())
    } else {
        Err(ManagerError::InvalidId)
    }
}

/// Refuse a start of the instance `id` when its render has no model profile to
/// load: the launch hands llama-server the render as `--models-preset`, and a
/// preset file naming no model profile starts a server with nothing to serve. A
/// render that is not there at all is the same refusal, the instance assigned no
/// model profile yet. `Stop` is never refused: an instance is always stoppable,
/// and a `Restart` refused here stops nothing.
fn has_profiles(id: &str) -> Result<(), ManagerError> {
    let path = crate::paths::instance_models(id).ok_or(ManagerError::InvalidId)?;
    if pai_core::launch::has_model_profiles_in(&path) {
        Ok(())
    } else {
        Err(one_value_refusal("instance-no-model-profiles", id))
    }
}

/// `id` if it is a valid id and a section of the record (`paths::instance_ids`).
/// The record is the instance, not a file named after the id: a section naming
/// a server profile whose `server@<profile>.ini` is missing IS an instance,
/// because the profile is read by the launch when the unit starts (its refusal
/// is `launch::instance_command`'s `NoConfig`); and a `server@<id>.ini` no
/// section names is no instance. An id that is not a valid instance id is
/// InvalidId, answered first.
fn existing_id(id: &str) -> Result<(), ManagerError> {
    valid_id(id)?;
    if crate::paths::instance_ids().iter().any(|name| name == id) {
        Ok(())
    } else {
        Err(ManagerError::NoInstance)
    }
}

/// Whether `name` is a user name the manager takes for a principal: 1 to
/// [`MAX_PRINCIPAL_BYTES`] bytes, with no control character. The name is
/// written whole where the manager records who acted, so it carries no NUL
/// and no line break that would read as a second record.
fn principal_name_is_well_formed(name: &str) -> bool {
    !name.is_empty() && name.len() <= MAX_PRINCIPAL_BYTES && !name.chars().any(char::is_control)
}

/// The files of the machine's config a manager client may READ, and never
/// write: `assignments.ini` and `instances.ini` are both the manager's own
/// records, written only by the manager; each comes with the engine's
/// package where it is missing, and the manager is the one that keeps it
/// afterwards. A client reads either; a write of one is refused as a bad
/// name.
fn read_only_file(name: &str) -> bool {
    name == ASSIGNMENTS_FILE || name == INSTANCES_FILE
}

/// The path of served file `name` in the engine's config directory: a
/// `ConfigFile` and [`read_only_file`], nothing else.
fn file_path(name: &str) -> Result<std::path::PathBuf, ManagerError> {
    if config_file(name).is_none() && !read_only_file(name) {
        return Err(ManagerError::BadFileName);
    }
    Ok(crate::paths::engine_config_dir().join(name))
}

/// `text` with every `OpencodeApiKey` line that holds a value rewritten as
/// `OpencodeApiKey =` (its inline comment dropped with it, the line end
/// kept); comment lines and every other line as they were. `withheld` says
/// whether a value was blanked. The value is read as `pai_core::ini` reads
/// it: up to the first `;` or `#`, trimmed.
fn withhold_api_key(text: &str) -> pai_core::client::ManagerFile {
    const KEY: &str = "OpencodeApiKey";
    let mut withheld = false;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let t = body.trim();
        let holds_value = !t.starts_with([';', '#'])
            && t.split_once('=').is_some_and(|(k, v)| {
                k.trim() == KEY && !v.split([';', '#']).next().unwrap_or("").trim().is_empty()
            });
        if holds_value {
            withheld = true;
            out.push_str(KEY);
            out.push_str(" =");
            out.push_str(&line[body.len()..]);
        } else {
            out.push_str(line);
        }
    }
    pai_core::client::ManagerFile {
        text: out,
        withheld,
    }
}

/// `contents` with every live `OpencodeApiKey` line (not a comment) set to
/// `OpencodeApiKey = <key>`, its line end kept; with no such line, one is
/// added inside `[Server]`, after its last key line (right after the header
/// when it has none), in the file's line end style; appended when there is
/// no `[Server]` (a client renders an unset key as a commented hint only).
fn keep_held_api_key(contents: &str, key: &str) -> String {
    const KEY: &str = "OpencodeApiKey";
    let mut found = false;
    let mut out = String::with_capacity(contents.len() + key.len());
    // Byte offset in `out` just past [Server]'s last key line, or its header.
    let mut insert_at = None;
    let mut in_server = false;
    for line in contents.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let t = body.trim();
        let is_comment = t.starts_with([';', '#']);
        let is_key = !is_comment && t.split_once('=').is_some_and(|(k, _)| k.trim() == KEY);
        if is_key {
            found = true;
            out.push_str(&format!("{KEY} = {key}"));
            out.push_str(&line[body.len()..]);
        } else {
            out.push_str(line);
        }
        if t.starts_with('[') {
            in_server = t == "[Server]";
            if in_server {
                insert_at = Some(out.len());
            }
        } else if in_server && !is_comment && t.contains('=') {
            insert_at = Some(out.len());
        }
    }
    if !found {
        let eol = if contents.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let new_line = format!("{KEY} = {key}{eol}");
        match insert_at {
            Some(at) => {
                if !out[..at].ends_with('\n') {
                    out.insert_str(at, eol);
                    out.insert_str(at + eol.len(), &new_line);
                } else {
                    out.insert_str(at, &new_line);
                }
            }
            None => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push_str(eol);
                }
                out.push_str(&new_line);
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
// The items the moved file declares at its top level: every
// caller names them here, as it always did.
#[cfg(test)]
pub(crate) use tests::*;
