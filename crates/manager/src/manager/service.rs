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
use std::net::IpAddr;

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
    /// The server profile of this name is the one the instance of this id runs.
    ServerProfileInUse(String, String),
    /// A server profile of this name is already there: a create writes over
    /// nothing.
    ServerProfileExists(String),
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
        // The profile and the instance, split by the client at the separator
        // `Refusal::encode` uses.
        ManagerError::ServerProfileInUse(name, id) => {
            ("ServerProfileInUse", format!("{name}\u{1f}{id}"))
        }
        ManagerError::ServerProfileExists(name) => ("ServerProfileExists", name.clone()),
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
        ManagerError::ServerProfileInUse(_, _) => "ServerProfileInUse",
        ManagerError::ServerProfileExists(_) => "ServerProfileExists",
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

/// `assignments.ini`: which model profiles each instance is given. The manager owns
/// it, so a client reads it and never writes it.
pub const ASSIGNMENTS_FILE: &str = "assignments.ini";

/// `instances.ini`: what an instance is, one section per instance with its
/// `ServerProfile`. The manager owns it, so a client reads it and never
/// writes it.
pub const INSTANCES_FILE: &str = "instances.ini";

/// The engine this manager serves, the one every instance and every server
/// profile names: the only value `CreateServerProfile` and `DeleteServerProfile`
/// accept.
const ENGINE: &str = "llama.cpp";

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
}

/// The node's GPU totals answered as nothing: no pool, so the memory check is
/// skipped. The default of [`Manager::new`], so a manager that wires no seam
/// measures nothing.
pub struct NoGpus;

impl GpuTotals for NoGpus {
    fn gpus(&self) -> Option<Vec<pai_core::devices::Gpu>> {
        None
    }
}

/// How long the engine probe of `--list-devices` is given before it is
/// killed (ARCHITECTURE.md (b) 2, "The GPUs").
pub const GPU_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The node's GPU totals from the engine's own probe: `--list-devices` run
/// against `exe` as the manager, killed after `timeout`, the block reduced
/// to the physical pool by `pai_core::devices`, cached for the manager's
/// life keyed by the exe's modification time, and a probe that failed not
/// cached (ARCHITECTURE.md (b) 2: a node with no card, a probe that timed
/// out, spawned badly or named no card is answered `None`, which skips the
/// memory check).
pub struct ProbedGpus {
    exe: std::path::PathBuf,
    timeout: std::time::Duration,
    /// The pool the last probe answered, with the modification time the exe had
    /// when it was read: an engine upgrade replaces the file, and the next call
    /// probes again. A probe that failed leaves nothing here, so the fix of the
    /// engine is picked up.
    cached: std::sync::Mutex<Option<(Option<std::time::SystemTime>, Vec<pai_core::devices::Gpu>)>>,
}

impl ProbedGpus {
    /// The probe of `exe --list-devices`, killed after `timeout`. A test
    /// names a script and a short bound.
    pub fn new(exe: impl Into<std::path::PathBuf>, timeout: std::time::Duration) -> Self {
        Self {
            exe: exe.into(),
            timeout,
            cached: std::sync::Mutex::new(None),
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
}

impl GpuTotals for ProbedGpus {
    fn gpus(&self) -> Option<Vec<pai_core::devices::Gpu>> {
        let mtime = std::fs::metadata(&self.exe).and_then(|f| f.modified()).ok();
        let cached = self
            .cached
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some((at, pool)) = cached {
            if at == mtime {
                return Some(pool);
            }
        }
        // The block is read out of stdout and stderr together (`proc`'s rule
        // for the llama-server probes), and the exit status is not asked: an
        // engine whose backend fails late exits non-zero with a block worth
        // reading. A pool with no card is a probe that failed.
        let pool =
            pai_core::proc::run_hidden_probe_within(&self.exe, ["--list-devices"], self.timeout)
                .map(|out| {
                    pai_core::devices::gpu_pool(&pai_core::devices::parse(
                        &pai_core::proc::combined_output(&out),
                    ))
                })
                .filter(|pool| !pool.is_empty())?;
        *self.cached.lock().unwrap_or_else(|e| e.into_inner()) = Some((mtime, pool.clone()));
        Some(pool)
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
    /// Records every guarded call, done or refused, through [`ActionLog`].
    action_log: Box<dyn ActionLog + Send + Sync>,
    /// Held by a write from its address check to its rename, so a check
    /// sees every write before it (and `write_served_file`'s one temp
    /// name per file is never shared). Taken only after the authorization:
    /// a caller polkit is still asking about holds up nobody.
    writing: std::sync::Mutex<()>,
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
            action_log: Box::new(NoActionLog),
            writing: std::sync::Mutex::new(()),
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

    /// This manager recording every guarded call, done or refused, through
    /// `log`, in place of the silent [`NoActionLog`].
    pub fn with_log(mut self, log: impl ActionLog + Send + Sync + 'static) -> Self {
        self.action_log = Box::new(log);
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
            self.check_and_write(file, name, contents)
        })
    }

    /// Replace file `name` only while it still holds `expected`, the whole
    /// text its caller read, byte for byte: equal bytes run `write_file`'s
    /// checks and write, a file that is missing or holds other bytes is
    /// [`ManagerError::Changed`] with nothing written. No `keep_api_key`: the
    /// caller sent the file as it read it, key line included. The principal and
    /// `configure` gates run first, as `write_file`'s, and the compare runs
    /// under the same `writing` lock as the write.
    pub fn write_file_if<W: AsRequester>(
        &self,
        who: &W,
        name: &str,
        contents: &str,
        expected: &str,
    ) -> Result<(), ManagerError> {
        self.write_guarded(who, name, "write_file_if", |file| {
            // The compare is the first thing under the lock, before any check
            // of `contents`: `expected` is the whole text the caller read, so
            // other bytes mean its read is stale and the write is refused.
            match std::fs::read_to_string(file_path(name)?) {
                Ok(text) if text == expected => self.check_and_write(file, name, contents),
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
    fn write_guarded<W: AsRequester, F>(
        &self,
        who: &W,
        name: &str,
        action: &str,
        body: F,
    ) -> Result<(), ManagerError>
    where
        F: FnOnce(&ConfigFile) -> Result<(), ManagerError>,
    {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
            self.authorize_as(&who, Action::Configure)?;
            let file = config_file(name).ok_or(ManagerError::BadFileName)?;
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
    fn check_and_write(
        &self,
        file: &ConfigFile,
        name: &str,
        contents: &str,
    ) -> Result<(), ManagerError> {
        check_contents(file, contents, &*self.gpus).map_err(ManagerError::Invalid)?;
        if let ConfigFile::ServerProfile(id) = file {
            self.check_address(id, contents)?;
        }
        let dir = crate::paths::engine_config_dir();
        let io = |e: std::io::Error| ManagerError::Io(e.kind());
        std::fs::create_dir_all(&dir).map_err(io)?;
        write_served_file(&dir.join(name), file, contents, &*self.group).map_err(io)?;
        // The write stands whatever the renders do: the render runs past a
        // failure, and the caller is told the first failure in id order.
        match self.render_all_locked().first() {
            Some((_, err)) => Err(err.clone()),
            None => Ok(()),
        }
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
    /// run first, so a refusal writes and renders nothing. Guarded as a write
    /// of a served file is: the principal and `configure` gates, the `writing`
    /// lock, and one action record for the whole call, done or refused.
    pub fn set_model_profile<W: AsRequester>(
        &self,
        who: &W,
        instance: &str,
        profile: &str,
        value: &str,
    ) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
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
            let today = render_of(instance, &enabled, &disabled);
            check_model_files_of(&text, instance, &enabled, &disabled, &today)
                .map_err(ManagerError::Invalid)?;
            // The memory the render this record leaves in effect, measured
            // against the render this instance has today: the record is what
            // this write moves, so the render to judge is built of the new
            // record, as the model-file check above does, and a model profile
            // this write does not make larger is judged on nothing.
            check_profile_memory_of(&text, instance, &enabled, &disabled, &today, &*self.gpus)
                .map_err(ManagerError::Invalid)?;
            let cap = profile_file(instance)
                .and_then(|file| std::fs::read_to_string(file).ok())
                .map(|contents| crate::server_cfg::from_text(&contents).models_max);
            if let Some(models_max) = cap {
                check_render_of(&text, instance, &enabled, &disabled, models_max)
                    .map_err(ManagerError::Invalid)?;
            }
            std::fs::create_dir_all(&dir).map_err(io)?;
            write_served_file(&path, &ConfigFile::Models, &text, &*self.group).map_err(io)?;
            // The write stands whatever the renders do, as a write of a
            // served file does: the caller is told the first failure in id
            // order.
            match self.render_all_locked().first() {
                Some((_, err)) => Err(err.clone()),
                None => Ok(()),
            }
        };
        let answer = run();
        self.record_action("set_model_profile", instance, Some(&who), &answer);
        answer
    }

    /// Points instance `instance` at the server profile `profile`: writes the
    /// `ServerProfile` key of its section of `instances.ini`, every other byte
    /// of the record kept. The instance's launch is the one that reads the
    /// profile, so the call touches no unit and renders nothing: a server
    /// profile carries no model profile text, and the render's name is the
    /// instance's. Each check answers before the write, and a refusal writes
    /// nothing: the id must be a section of the record (`InvalidId`,
    /// `NoInstance`; a record that cannot be read is `Io`, never the empty list
    /// `paths::instance_ids` answers for one), the profile must be one the
    /// engine has (`NoServerProfile`, naming it: a name that builds no file, or
    /// a file that is really absent; any other read failure is `Io`), no other
    /// instance may run it (`ServerProfileInUse`, naming the profile and that
    /// instance), the address the switch takes must be free
    /// (`AddressInUse`), and the new profile's `ModelsMax` must hold the loads
    /// the instance's render starts (`load-on-startup-over-max`): that render
    /// is cut by `assignments.ini`, and a record there that cannot be read is
    /// `Io` as the instance record is. The profile the instance's own section
    /// names is answered before the walk of the others: no conflict with
    /// itself, and the switch to it changes no byte. Guarded as a write of a served file is: the
    /// principal and `configure` gates, the `writing` lock, and one action
    /// record for the whole call, done or refused.
    pub fn set_server_profile<W: AsRequester>(
        &self,
        who: &W,
        instance: &str,
        profile: &str,
    ) -> Result<(), ManagerError> {
        let who = who.as_requester();
        let run = || -> Result<(), ManagerError> {
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
                return Ok(());
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
            // The new profile's cap, over the loads the instance's render
            // starts today: the switch changes no model profile, so the count
            // it is judged on is this instance's own render, cut by the
            // assignment record read here.
            let on_disk = |name: &str| -> Result<String, ManagerError> {
                match std::fs::read_to_string(dir.join(name)) {
                    Ok(text) => Ok(text),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
                    Err(e) => Err(io(e)),
                }
            };
            let enabled = on_disk("models.ini")?;
            let disabled = on_disk("models.ini.disabled")?;
            let assignments = on_disk(ASSIGNMENTS_FILE)?;
            check_render_of_full(
                &assignments,
                instance,
                &enabled,
                &disabled,
                crate::server_cfg::from_text(&contents).models_max,
            )
            .map_err(ManagerError::Invalid)?;
            std::fs::create_dir_all(&dir).map_err(io)?;
            let text = pai_core::ini::text::set_key(&record, instance, "ServerProfile", profile);
            write_served_file(
                &dir.join(INSTANCES_FILE),
                &ConfigFile::Models,
                &text,
                &*self.group,
            )
            .map_err(io)
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
                let profile =
                    profile_file(&section.id).and_then(|file| std::fs::read_to_string(file).ok());
                let (port, profile_tags) = match profile {
                    Some(text) => {
                        let cfg = crate::server_cfg::from_text(&text);
                        (
                            u16::try_from(cfg.port.unwrap_or(crate::server_cfg::DEFAULT_PORT))
                                .unwrap_or(0),
                            cfg.tags,
                        )
                    }
                    None => (0, Vec::new()),
                };
                InstanceInfo {
                    id: section.id.clone(),
                    engine: ENGINE.to_string(),
                    server_profile: section
                        .keys
                        .get("ServerProfile")
                        .map(|name| name.trim().to_string())
                        .filter(|name| !name.is_empty()),
                    port,
                    profile_tags,
                    instance_tags: list(section.keys.get("Tags")),
                }
            })
            .collect();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        rows
    }

    /// Every server profile the engine's directory holds: each
    /// `server@<id>.ini` whose name the id rule accepts, in id order, with the
    /// port and the tags of its `[Server]` section (no `Port` key is
    /// `server_cfg::DEFAULT_PORT`, as `instances` reads a profile) and the first
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
                        Some(ServerProfileInfo {
                            id: id.clone(),
                            engine: ENGINE.to_string(),
                            port,
                            tags,
                            instance: record_holder(&record, &id),
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
    pub fn render_failures(&self) -> Vec<(String, ManagerError)> {
        // The same lock a write holds, so a render never mixes with a write.
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
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
    /// is on disk, so it belongs with the write that changed it. A render
    /// that cannot be written does not stop the rest: every instance is
    /// rendered and every failure comes back, in id order.
    fn render_all_locked(&self) -> Vec<(String, ManagerError)> {
        let dir = crate::paths::engine_config_dir();
        let on_disk = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();
        let (enabled, disabled) = (on_disk("models.ini"), on_disk("models.ini.disabled"));
        let io = |e: std::io::Error| ManagerError::Io(e.kind());
        let mut failures = Vec::new();
        for id in crate::paths::instance_ids() {
            let path = match crate::paths::instance_models(&id) {
                Some(path) => path,
                None => continue,
            };
            // A render holds model profile text alone, so it carries the model profile
            // file's mode and needs no group of its own.
            let text = render_of(&id, &enabled, &disabled);
            if let Err(e) = write_served_file(&path, &ConfigFile::Models, &text, &*self.group) {
                failures.push((id, io(e)));
            }
        }
        failures
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
    /// server profile and the switch of an instance to another one. `skip`
    /// answers the instances to leave out of the walk, the ones that hold the
    /// address for the reason the check is about: the profile being saved, for
    /// a save, and the instance being switched, for a switch. An instance
    /// whose section names no profile, whose profile builds no path, or whose
    /// profile file cannot be read, is skipped too.
    fn check_binding<F>(&self, mine: &Binding, skip: F) -> Result<(), ManagerError>
    where
        F: Fn(&str) -> bool,
    {
        for other in crate::paths::instance_ids() {
            if skip(&other) {
                continue;
            }
            let Some(path) = profile_file(&other) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let theirs = binding(&crate::server_cfg::from_text(&text), &self.resolver);
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

/// The name the manager gives the web service when its address is in use: the
/// service's own name, the one the installer registers it under.
const BFF_NAME: &str = "private-ai-bff";

/// The file of the server profile the instance `id` names:
/// `server@<profile>.ini`, `None` when the section names no profile.
fn profile_file(id: &str) -> Option<std::path::PathBuf> {
    crate::paths::instance_ini(&crate::paths::instance_server_profile(id)?)
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

/// `engine` if this manager serves it ([`ENGINE`], the one engine of a node),
/// else the refusal naming the engine it was asked for.
fn known_engine(engine: &str) -> Result<(), ManagerError> {
    if engine == ENGINE {
        Ok(())
    } else {
        Err(one_value_refusal("unknown-engine", engine))
    }
}

/// The refusal of a rule that carries one value: the key, and the name the
/// rule refused.
fn one_value_refusal(key: &'static str, value: &str) -> ManagerError {
    ManagerError::Invalid(pai_core::checks::Refusal {
        key,
        values: vec![value.to_string()],
    })
}

/// The first section of the record text `text` that names the server profile
/// `profile`: the instance running it, `None` when no section does.
fn record_holder(text: &str, profile: &str) -> Option<String> {
    pai_core::ini::parse_sections(text)
        .into_iter()
        .find(|section| section.keys.get("ServerProfile").map(|name| name.trim()) == Some(profile))
        .map(|section| section.id)
}

/// The content checks of `file` for `contents`, against the files on disk:
/// the instance file against the Server rules and its own render, `models.ini`
/// and `models.ini.disabled` against the other model profile file and every
/// instance's render. A render is the pool cut by the instance's section of
/// `assignments.ini`, so the `load-on-startup` count is judged per instance,
/// against the `ModelsMax` of the server profile that instance's section of
/// `instances.ini` names. Both model profile files are judged for their model
/// files too: a model profile the write puts in effect on an instance names
/// files the node has, and a file that instance's render is already missing
/// is never re-judged. A refusal writes nothing.
fn check_contents(
    file: &ConfigFile,
    contents: &str,
    gpus: &dyn GpuTotals,
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
        ConfigFile::ServerProfile(name) => {
            // The Server rules alone: no pool caps them any more, the
            // instance's render does, against the `ModelsMax` of this text.
            // This text caps a pool only where an instance runs it, so a
            // profile no instance names loads nothing and is judged on nothing.
            checks::server_file(contents, "")?;
            let models_max = crate::server_cfg::from_text(contents).models_max;
            for id in crate::paths::instance_ids() {
                if crate::paths::instance_server_profile(&id).as_deref() == Some(name) {
                    check_render(&id, &enabled, &disabled, models_max)?;
                }
            }
            Ok(())
        }
        ConfigFile::Models => {
            let (old, off) = (on_disk("models.ini"), on_disk("models.ini.disabled"));
            checks::models_file(contents, &old, &off, NO_MODELS_MAX)?;
            checks::model_profile_ids_free(contents, &old, &off)?;
            check_renders(&enabled, &disabled)?;
            check_model_files(&on_disk(ASSIGNMENTS_FILE), &enabled, &disabled, &old, &off)?;
            // The memory the render puts in effect, after the model files: a
            // model profile whose file is not on the node has no header to
            // read, and the missing file is the news that matters (it is also
            // what makes this estimate give up).
            check_profile_memory(&enabled, &disabled, &old, &off, gpus)
        }
        ConfigFile::ModelsDisabled => {
            let (old, enabled_file) = (on_disk("models.ini.disabled"), on_disk("models.ini"));
            checks::models_disabled_file(contents, &old, &enabled_file)?;
            checks::model_profile_ids_free(contents, &old, &enabled_file)?;
            check_renders(&enabled, &disabled)?;
            check_model_files(
                &on_disk(ASSIGNMENTS_FILE),
                &enabled,
                &disabled,
                &enabled_file,
                &old,
            )?;
            check_profile_memory(&enabled, &disabled, &enabled_file, &old, gpus)
        }
    }
}

/// The text of instance `id`'s render `models@<id>.ini`, from the pool pair
/// and the `assignments.ini` on disk: what the checks judge and what
/// [`Manager::render_failures`] writes.
fn render_of(id: &str, enabled: &str, disabled: &str) -> String {
    let assignment =
        std::fs::read_to_string(crate::paths::engine_config_dir().join(ASSIGNMENTS_FILE))
            .ok()
            .and_then(|text| pai_core::ini::text::section_text(&text, id));
    pai_core::assign::render(enabled, disabled, assignment.as_deref())
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

/// The `load-on-startup` of instance `id`'s render against `models_max`, that
/// instance's own, over the assignment record `assignments` rather than the
/// one on disk: the render a write of the record is about to produce. Only a
/// render that starts MORE than the instance starts today is judged, so a step
/// down, or a count that stays where it is, lands. The refusal names the
/// instance, last.
fn check_render_of(
    assignments: &str,
    id: &str,
    enabled: &str,
    disabled: &str,
    models_max: Option<i32>,
) -> Result<(), pai_core::checks::Refusal> {
    let assignment = pai_core::ini::text::section_text(assignments, id);
    let render = pai_core::assign::render(enabled, disabled, assignment.as_deref());
    let count = pai_core::checks::startup_loads(&render);
    let today = pai_core::checks::startup_loads(&render_of(id, enabled, disabled));
    if count <= today {
        return Ok(());
    }
    pai_core::checks::startup_count(count, models_max).map_err(|mut refusal| {
        refusal.values.push(id.to_string());
        refusal
    })
}

/// The `load-on-startup` of instance `id`'s render against `models_max`, that
/// instance's own, over the assignment record `assignments` rather than the
/// one on disk: the render a server profile switch leaves. The whole count is
/// judged, so a switch to a lower cap is refused as one that starts more is:
/// the step rule of `check_render_of` is `SetModelProfile`'s, and a switch
/// changes no model profile, so it has no step to make. The refusal names the
/// instance, last.
fn check_render_of_full(
    assignments: &str,
    id: &str,
    enabled: &str,
    disabled: &str,
    models_max: Option<i32>,
) -> Result<(), pai_core::checks::Refusal> {
    let assignment = pai_core::ini::text::section_text(assignments, id);
    let render = pai_core::assign::render(enabled, disabled, assignment.as_deref());
    let count = pai_core::checks::startup_loads(&render);
    pai_core::checks::startup_count(count, models_max).map_err(|mut refusal| {
        refusal.values.push(id.to_string());
        refusal
    })
}

/// The `load-on-startup` of instance `id`'s render against `models_max`, that
/// instance's own. The refusal names the instance, last.
fn check_render(
    id: &str,
    enabled: &str,
    disabled: &str,
    models_max: Option<i32>,
) -> Result<(), pai_core::checks::Refusal> {
    let count = pai_core::checks::startup_loads(&render_of(id, enabled, disabled));
    pai_core::checks::startup_count(count, models_max).map_err(|mut refusal| {
        refusal.values.push(id.to_string());
        refusal
    })
}

/// Every instance's render against the `ModelsMax` of the server profile its
/// section names, in id order: the first render over its max refuses. An
/// instance whose section names no profile, or names one whose file cannot be
/// read, is skipped: its launch refuses it, and its render is not judged on
/// the defaults of a file that is not there.
fn check_renders(enabled: &str, disabled: &str) -> Result<(), pai_core::checks::Refusal> {
    for id in crate::paths::instance_ids() {
        let Some(path) = profile_file(&id) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        check_render(
            &id,
            enabled,
            disabled,
            crate::server_cfg::from_text(&text).models_max,
        )?;
    }
    Ok(())
}

/// The model profile keys whose `model` file the engine puts on the node
/// itself: `hf-repo` downloads it, `model-url` and `docker-repo` replace the
/// value with a file the engine fetches (llama.cpp `common/arg.cpp`). A model
/// profile carrying one of them names no file of the node, so the model-file
/// check looks at nothing for it.
const FETCH_KEYS: [&str; 3] = ["hf-repo", "model-url", "docker-repo"];

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
            let file = std::path::Path::new(&file);
            let path = if file.is_absolute() {
                file.to_path_buf()
            } else {
                cwd.join(file)
            };
            if !std::fs::metadata(&path).is_ok_and(|f| f.is_file()) {
                missing.push((section.id.clone(), path.to_string_lossy().into_owned()));
            }
        }
    }
    missing
}

// ── The model profile memory check (ARCHITECTURE.md step 10, item 13) ──────

/// The whole pool pair as it will stand after the write (`enabled`,
/// `disabled`) against the pair on disk (`old_enabled`, `old_disabled`): every
/// instance's render in id order, the first one with a model profile the write
/// makes over the node's memory refusing. The rule, the args of the refusal and
/// the GPU seam are [`check_render_memory`]'s.
fn check_profile_memory(
    enabled: &str,
    disabled: &str,
    old_enabled: &str,
    old_disabled: &str,
    gpus: &dyn GpuTotals,
) -> Result<(), pai_core::checks::Refusal> {
    for id in crate::paths::instance_ids() {
        // The render today is the pool on disk cut by the record on disk, as
        // [`check_model_files`] reads it.
        check_render_memory(
            &render_of(&id, enabled, disabled),
            &render_of(&id, old_enabled, old_disabled),
            &id,
            gpus,
        )?;
    }
    Ok(())
}

/// The memory of the render the record `assignments` leaves in effect on
/// instance `id`, against the render that instance has today (`today`: the pool
/// pair on disk and the record on disk, as [`check_model_files_of`] takes it).
/// The record is the whole of what this write moves, so the render to measure
/// is built of `assignments` and not read off the disk.
fn check_profile_memory_of(
    assignments: &str,
    id: &str,
    enabled: &str,
    disabled: &str,
    today: &str,
    gpus: &dyn GpuTotals,
) -> Result<(), pai_core::checks::Refusal> {
    let assignment = pai_core::ini::text::section_text(assignments, id);
    let render = pai_core::assign::render(enabled, disabled, assignment.as_deref());
    check_render_memory(&render, today, id, gpus)
}

/// The memory check of one instance's render `render`, named `id` in the
/// refusal, against the render that same instance has today (`today`): the
/// first model profile of `render`, in the render's order, that this write
/// makes larger AND whose own estimate is over the node's usable GPU memory
/// refuses with `model-profile-over-memory` and
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
/// The estimates come from the files and the GPU totals from the seam
/// [`GpuTotals`], which runs a probe: both renders are estimated and the growth
/// found first, and the seam is asked only once a section has grown. A node the
/// seam answers no pool for, an empty pool included, is measured on nothing.
fn check_render_memory(
    render: &str,
    today: &str,
    id: &str,
    gpus: &dyn GpuTotals,
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
    let grown: Vec<Estimated> = estimated_of(render, &cwd)
        .into_iter()
        .filter(|need| need.bytes > baseline(&need.id))
        .collect();
    if grown.is_empty() {
        return Ok(());
    }
    let Some(pool) = gpus.gpus() else {
        return Ok(());
    };
    let available = pai_core::devices::pool_usable(&pool);
    if available == 0 {
        return Ok(());
    }
    if let Some(need) = grown.into_iter().find(|need| need.bytes > available) {
        return Err(pai_core::checks::Refusal {
            key: "model-profile-over-memory",
            values: vec![
                need.id,
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
        let file = std::path::Path::new(&model);
        let path = if file.is_absolute() {
            file.to_path_buf()
        } else {
            cwd.join(file)
        };
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

/// Fakes of the two seams that record every call.
#[cfg(test)]
pub(crate) mod fakes {
    use super::*;
    use std::sync::Mutex;

    /// One recorded call to the service manager.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Call {
        Start(String),
        Stop(String),
        Restart(String),
        Status(String),
        BootEnabled(String),
        SetBootEnabled(String, bool),
    }

    /// A service manager that records calls, reports `state`, starts at boot
    /// the ids in `enabled` and fails every call for the ids in `failing`.
    pub struct FakeService {
        pub calls: Mutex<Vec<Call>>,
        pub state: UnitState,
        pub failing: Vec<String>,
        pub enabled: Mutex<std::collections::BTreeSet<String>>,
    }

    impl FakeService {
        pub fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                state: UnitState::Active,
                failing: Vec::new(),
                enabled: Mutex::new(Default::default()),
            }
        }

        fn record(&self, call: Call, id: &str) -> Result<(), ServiceError> {
            self.calls.lock().unwrap().push(call);
            if self.failing.iter().any(|f| f == id) {
                return Err(ServiceError(format!("llama.cpp@{id}.service failed")));
            }
            Ok(())
        }
    }

    impl ServiceControl for FakeService {
        fn start(&self, id: &str) -> Result<(), ServiceError> {
            self.record(Call::Start(id.into()), id)
        }
        fn stop(&self, id: &str) -> Result<(), ServiceError> {
            self.record(Call::Stop(id.into()), id)
        }
        fn restart(&self, id: &str) -> Result<(), ServiceError> {
            self.record(Call::Restart(id.into()), id)
        }
        fn status(&self, id: &str) -> Result<UnitState, ServiceError> {
            self.record(Call::Status(id.into()), id)?;
            Ok(self.state)
        }
        fn boot_enabled(&self, id: &str) -> Result<bool, ServiceError> {
            self.record(Call::BootEnabled(id.into()), id)?;
            Ok(self.enabled.lock().unwrap().contains(id))
        }
        fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ServiceError> {
            self.record(Call::SetBootEnabled(id.into(), enabled), id)?;
            let mut set = self.enabled.lock().unwrap();
            if enabled {
                set.insert(id.to_string());
            } else {
                set.remove(id);
            }
            Ok(())
        }
    }

    /// A resolver over a fixed table; any other name does not resolve.
    pub struct FakeResolver {
        pub table: Vec<(&'static str, Vec<IpAddr>)>,
        pub asked: Mutex<Vec<String>>,
    }

    impl FakeResolver {
        pub fn new(table: &[(&'static str, &[&str])]) -> Self {
            Self {
                table: table
                    .iter()
                    .map(|(name, ips)| {
                        (
                            *name,
                            ips.iter().map(|ip| ip.parse().expect("ip")).collect(),
                        )
                    })
                    .collect(),
                asked: Mutex::new(Vec::new()),
            }
        }

        /// The names the e2e uses: localhost, a LAN box with two names, a
        /// multi-homed one.
        pub fn lan() -> Self {
            Self::new(&[
                ("localhost", &["127.0.0.1"]),
                ("gpu-box", &["192.168.1.5"]),
                ("gpu-box.lan", &["192.168.1.5"]),
                ("multi-home", &["10.0.0.9", "192.168.1.5"]),
            ])
        }
    }

    impl Resolver for FakeResolver {
        fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, ResolveError> {
            self.asked.lock().unwrap().push(host.to_string());
            self.table
                .iter()
                .find(|(name, _)| *name == host)
                .map(|(_, ips)| ips.clone())
                .ok_or_else(|| ResolveError(host.to_string()))
        }
    }

    /// An authority that answers `allow` and records every question; the
    /// questions asked without interaction answer `quiet` (`allow` unless
    /// set with [`FakeAuthority::quietly`]) and are recorded apart.
    /// Only the caller set with [`FakeAuthority::bff`] is the bff's account
    /// (nobody, unless set); those questions are recorded in `asked_bff`.
    pub struct FakeAuthority {
        pub allow: bool,
        pub quiet: bool,
        pub bff: Option<Caller>,
        pub asked: Mutex<Vec<(Caller, Action)>>,
        pub asked_quietly: Mutex<Vec<(Caller, Action)>>,
        pub asked_bff: Mutex<Vec<Caller>>,
    }

    impl FakeAuthority {
        pub fn new(allow: bool) -> Self {
            Self {
                allow,
                quiet: allow,
                bff: None,
                asked: Mutex::new(Vec::new()),
                asked_quietly: Mutex::new(Vec::new()),
                asked_bff: Mutex::new(Vec::new()),
            }
        }

        /// This authority taking `caller` for the bff's account.
        pub fn bff(mut self, caller: &Caller) -> Self {
            self.bff = Some(caller.clone());
            self
        }

        /// This authority answering `quiet` to the questions asked without
        /// interaction: false is a caller polkit would challenge.
        pub fn quietly(mut self, quiet: bool) -> Self {
            self.quiet = quiet;
            self
        }
    }

    impl Authority for FakeAuthority {
        fn is_authorized(&self, caller: &Caller, action: Action) -> bool {
            self.asked.lock().unwrap().push((caller.clone(), action));
            self.allow
        }
        fn is_authorized_quietly(&self, caller: &Caller, action: Action) -> bool {
            self.asked_quietly
                .lock()
                .unwrap()
                .push((caller.clone(), action));
            self.quiet
        }
        fn is_bff(&self, caller: &Caller) -> bool {
            self.asked_bff.lock().unwrap().push(caller.clone());
            self.bff.as_ref() == Some(caller)
        }
    }

    /// The accounts of the authentication tests: each `(user, password,
    /// admin)`; any other user is unknown. `fail` makes every check fail
    /// with that message. Records each check (user and password, to pin that
    /// they arrive unchanged) and each admin question.
    /// The (user, password) pairs a [`FakePasswords`] was asked to check.
    pub type Checked = std::sync::Arc<Mutex<Vec<(String, String)>>>;

    pub struct FakePasswords {
        pub accounts: Vec<(&'static str, &'static str, bool)>,
        /// The accounts [`is_admin`](PasswordChecker::is_admin) answers true
        /// for, seeded from `accounts` and shared so a test can remove a
        /// member between two calls of one manager (the manager must not
        /// cache membership).
        pub admins: std::sync::Arc<Mutex<std::collections::BTreeSet<String>>>,
        pub fail: Option<String>,
        pub checked: Checked,
        pub asked_admin: std::sync::Arc<Mutex<Vec<String>>>,
    }

    impl FakePasswords {
        /// alice (an administrator) and bob (not one).
        pub fn new() -> Self {
            let accounts = vec![("alice", "s3cret-Pw", true), ("bob", "hunter2!", false)];
            let admins = accounts
                .iter()
                .filter(|(_, _, admin)| *admin)
                .map(|(u, _, _)| (*u).to_string())
                .collect();
            Self {
                accounts,
                admins: std::sync::Arc::new(Mutex::new(admins)),
                fail: None,
                checked: Default::default(),
                asked_admin: Default::default(),
            }
        }

        pub fn failing(message: &str) -> Self {
            Self {
                fail: Some(message.to_string()),
                ..Self::new()
            }
        }
    }

    impl PasswordChecker for FakePasswords {
        fn check(&self, user: &str, password: &str) -> Result<bool, PasswordError> {
            self.checked
                .lock()
                .unwrap()
                .push((user.to_string(), password.to_string()));
            if let Some(message) = &self.fail {
                return Err(PasswordError(message.clone()));
            }
            Ok(self
                .accounts
                .iter()
                .any(|(u, p, _)| *u == user && *p == password))
        }
        fn is_admin(&self, user: &str) -> bool {
            self.asked_admin.lock().unwrap().push(user.to_string());
            self.admins.lock().unwrap().contains(user)
        }
    }

    /// A group seam that changes no group: it records each call, with the
    /// file's mode at that moment (Unix; 0 elsewhere), and fails with
    /// `fail` when set.
    pub struct FakeGroup {
        pub fail: Option<std::io::ErrorKind>,
        pub calls: std::sync::Arc<Mutex<Vec<GroupCall>>>,
    }

    /// One recorded [`FakeGroup`] call.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GroupCall {
        pub path: std::path::PathBuf,
        pub group: String,
        pub mode: u32,
    }

    impl FakeGroup {
        pub fn new() -> Self {
            Self {
                fail: None,
                calls: Default::default(),
            }
        }

        pub fn failing(kind: std::io::ErrorKind) -> Self {
            Self {
                fail: Some(kind),
                ..Self::new()
            }
        }
    }

    impl FileGroup for FakeGroup {
        fn set_group(&self, path: &std::path::Path, group: &str) -> std::io::Result<()> {
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                std::fs::metadata(path).map_or(0, |m| m.permissions().mode() & 0o7777)
            };
            #[cfg(not(unix))]
            let mode = 0;
            self.calls.lock().unwrap().push(GroupCall {
                path: path.to_path_buf(),
                group: group.to_string(),
                mode,
            });
            match self.fail {
                Some(kind) => Err(kind.into()),
                None => Ok(()),
            }
        }
    }

    /// A GPU pool the memory check measures against: `gpus` answered as the
    /// node's cards, each written as `(device id, name, total MiB)`, and every
    /// answer recorded (lowered ids), so a test can pin how often the manager
    /// probed.
    pub struct FakeGpus {
        pub gpus: Option<Vec<(String, String, u64)>>,
        pub asked: std::sync::Arc<Mutex<Vec<String>>>,
    }

    impl FakeGpus {
        pub fn new(gpus: &[(&str, u64)]) -> Self {
            Self {
                gpus: Some(
                    gpus.iter()
                        .map(|(id, mib)| (id.to_string(), format!("NVIDIA GeForce {id}"), *mib))
                        .collect(),
                ),
                asked: std::sync::Arc::new(Mutex::new(Vec::new())),
            }
        }
        /// A node with no card: the seam answers `None`.
        pub fn none() -> Self {
            Self {
                gpus: None,
                asked: std::sync::Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl GpuTotals for FakeGpus {
        fn gpus(&self) -> Option<Vec<pai_core::devices::Gpu>> {
            self.gpus.as_ref().map(|list| {
                self.asked.lock().unwrap().push("gpus".to_string());
                list.iter()
                    .map(|(id, name, mib)| pai_core::devices::Gpu {
                        id: id.clone(),
                        name: name.clone(),
                        total_mib: *mib,
                    })
                    .collect()
            })
        }
    }

    /// An action log that keeps every record line the manager renders, so a
    /// test pins the records whole: one per guarded call, refusals included.
    #[derive(Default)]
    pub struct FakeActionLog {
        pub lines: std::sync::Arc<Mutex<Vec<String>>>,
    }

    impl FakeActionLog {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl ActionLog for FakeActionLog {
        fn record(&self, line: &str) {
            self.lines.lock().unwrap().push(line.to_string());
        }
    }

    /// The longest a gate holds a call, and the longest a test waits for one:
    /// a test that never opens its gate fails instead of hanging.
    pub const GATE_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

    /// A gate a fake seam holds its calls at until the test opens it, as
    /// polkit holds `CheckAuthorization` while a password prompt is up.
    pub struct Gate {
        /// How many calls arrived, and whether the gate is open.
        state: Mutex<(usize, bool)>,
        changed: std::sync::Condvar,
    }

    impl Gate {
        pub fn new() -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self {
                state: Mutex::new((0, false)),
                changed: std::sync::Condvar::new(),
            })
        }

        /// For the fake: count this call and wait for the gate, at most
        /// [`GATE_WAIT`]. Whether it was opened.
        pub fn pass(&self) -> bool {
            let mut state = self.state.lock().unwrap();
            state.0 += 1;
            self.changed.notify_all();
            let (state, _) = self
                .changed
                .wait_timeout_while(state, GATE_WAIT, |s| !s.1)
                .unwrap();
            state.1
        }

        /// For the test: whether `n` calls reached the gate within [`GATE_WAIT`].
        pub fn arrived(&self, n: usize) -> bool {
            let state = self.state.lock().unwrap();
            let (state, _) = self
                .changed
                .wait_timeout_while(state, GATE_WAIT, |s| s.0 < n)
                .unwrap();
            state.0 >= n
        }

        /// Let every held call, and every later one, through.
        pub fn open(&self) {
            self.state.lock().unwrap().1 = true;
            self.changed.notify_all();
        }
    }

    /// An authority that holds the questions about `gated` at `gate`, as
    /// polkit does while that user types a password, then answers from
    /// `answers` (a caller not listed is refused). Records every question.
    pub struct GateAuthority {
        pub gated: Caller,
        pub gate: std::sync::Arc<Gate>,
        pub answers: Vec<(Caller, bool)>,
        pub asked: Mutex<Vec<(Caller, Action)>>,
    }

    impl GateAuthority {
        pub fn new(
            gated: &Caller,
            gate: &std::sync::Arc<Gate>,
            answers: &[(&Caller, bool)],
        ) -> Self {
            Self {
                gated: gated.clone(),
                gate: gate.clone(),
                answers: answers.iter().map(|(c, a)| ((*c).clone(), *a)).collect(),
                asked: Mutex::new(Vec::new()),
            }
        }
    }

    impl Authority for GateAuthority {
        fn is_authorized(&self, caller: &Caller, action: Action) -> bool {
            self.asked.lock().unwrap().push((caller.clone(), action));
            if *caller == self.gated && !self.gate.pass() {
                return false;
            }
            self.answers.iter().any(|(c, allow)| c == caller && *allow)
        }
        /// Never held: a quiet question prompts nobody. Not recorded.
        fn is_authorized_quietly(&self, caller: &Caller, _action: Action) -> bool {
            self.answers.iter().any(|(c, allow)| c == caller && *allow)
        }
        /// Nobody is the bff here.
        fn is_bff(&self, _caller: &Caller) -> bool {
            false
        }
    }

    /// A [`FakeResolver`] that holds the first lookup of `host` at `gate`
    /// (a slow DNS answer); later lookups answer at once.
    pub struct GateResolver {
        pub inner: FakeResolver,
        pub host: &'static str,
        pub gate: std::sync::Arc<Gate>,
        held: std::sync::atomic::AtomicBool,
    }

    impl GateResolver {
        pub fn new(inner: FakeResolver, host: &'static str, gate: &std::sync::Arc<Gate>) -> Self {
            Self {
                inner,
                host,
                gate: gate.clone(),
                held: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    impl Resolver for GateResolver {
        fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, ResolveError> {
            use std::sync::atomic::Ordering;
            if host == self.host && !self.held.swap(true, Ordering::SeqCst) {
                self.gate.pass();
            }
            self.inner.resolve(host)
        }
    }
}

/// A file as read by a caller nothing was withheld from.
#[cfg(test)]
pub(crate) fn plain(text: &str) -> pai_core::client::ManagerFile {
    pai_core::client::ManagerFile {
        text: text.into(),
        withheld: false,
    }
}

/// The file `text` as read by a caller the API key was withheld from.
#[cfg(test)]
pub(crate) fn withheld(text: &str) -> pai_core::client::ManagerFile {
    pai_core::client::ManagerFile {
        text: text.into(),
        withheld: true,
    }
}

/// Run `f` on a thread of its own; its result arrives on the receiver, read
/// with `recv_timeout` so a call that never returns fails the test instead
/// of hanging it.
#[cfg(test)]
pub(crate) fn on_thread<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> std::sync::mpsc::Receiver<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx
}

/// The record of the instances, written by a fixture: `instances.ini` in the
/// engine's config directory, one section per id (`paths::instance_ids` reads
/// the sections), each naming the server profile of the same name, the key the
/// launch reads (`launch::instance_command`). The manager adds a section when
/// it creates an instance, and that is a later slice, so a fixture that wants
/// an instance hands it the record. The whole file is rewritten: a section is
/// dropped with the instance it stands for.
#[cfg(test)]
fn write_record(dir: &std::path::Path, ids: &[&str]) {
    std::fs::create_dir_all(dir).expect("engine config dir");
    std::fs::write(
        dir.join(INSTANCES_FILE),
        ids.iter()
            .map(|id| format!("[{id}]\nServerProfile = {id}\n"))
            .collect::<Vec<String>>()
            .join("\n"),
    )
    .expect(INSTANCES_FILE);
}

/// A fixture's model profile text with files the node has. A write that puts a
/// model profile in effect is checked for its model files, so a fixture whose
/// pool is in effect has to name files that exist: each `model` value becomes
/// an empty file under `dir/models` and is replaced by its path. The ids, the
/// other keys, the comments and the order of the text are kept.
#[cfg(test)]
pub(crate) fn real_model_files(dir: &std::path::Path, text: &str) -> String {
    let root = dir.join("models");
    std::fs::create_dir_all(&root).expect("models dir");
    let mut out = String::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("model") {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // What follows the first `;` or `#` is a comment (`ini::parse_sections`).
        let value = &value[..value.find([';', '#']).unwrap_or(value.len())];
        let value = value.trim();
        if value.is_empty() {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let name = std::path::Path::new(value)
            .file_name()
            .unwrap_or(std::ffi::OsStr::new("model"));
        let path = root.join(name);
        std::fs::write(&path, "").expect("model file");
        out.push_str("model = ");
        out.push_str(&path.to_string_lossy());
        out.push('\n');
    }
    out
}

/// The umask window of the mode pins: the constructor masks the process with
/// the mode it is given and keeps the previous umask, `Drop` restores it. The
/// umask is process-wide and the e2e phases run on parallel threads, so a
/// panic between the mask and the restore must not leave 0o077 on the process.
#[cfg(all(test, target_os = "linux"))]
struct UmaskGuard(nix::sys::stat::Mode);

#[cfg(all(test, target_os = "linux"))]
impl UmaskGuard {
    /// Masks the process with `mask`, answering the guard that restores the
    /// umask it found when it is dropped.
    fn mask(mask: nix::sys::stat::Mode) -> Self {
        Self(nix::sys::stat::umask(mask))
    }
}

#[cfg(all(test, target_os = "linux"))]
impl Drop for UmaskGuard {
    fn drop(&mut self) {
        nix::sys::stat::umask(self.0);
    }
}

/// E2E phase for the manager core, for this crate's e2e test to call: it sets
/// the process-wide `PRIVATE_AI_DATA_ROOT` redirect (`src/tests/mod.rs`).
/// Nothing here touches D-Bus, systemd, polkit or the GPUs: both seams are
/// fakes.
#[cfg(test)]
pub(crate) fn manager_e2e() {
    use fakes::{Call, FakeAuthority, FakeResolver, FakeService};
    use std::collections::BTreeSet;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let alice = Caller(":1.42".into());
    let files_in = |p: &std::path::Path| -> BTreeSet<String> {
        std::fs::read_dir(p)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    let set =
        |names: &[&str]| -> BTreeSet<String> { names.iter().map(|s| s.to_string()).collect() };
    // An instance is a section of the record, not a file in the directory: the
    // fixture hands the manager the record it reads (`write_record`).
    let record = |ids: &[&str]| write_record(&sys, ids);
    let bad_names = |sys: &std::path::Path| -> Vec<String> {
        vec![
            "../x".into(),
            "../models.ini".into(),
            "server.ini".into(),
            "settings.ini".into(),
            "server@Bad.ini".into(),
            "server@.ini".into(),
            "server@a_b.ini".into(),
            "server@default.ini.tmp".into(),
            "server@default".into(),
            "models.ini.bak".into(),
            "models@a.ini".into(),
            "models@default.ini".into(),
            "sub/models.ini".into(),
            "".into(),
            sys.join("models.ini").to_string_lossy().into_owned(),
            "/etc/passwd".into(),
            "Presets.ini".into(),
        ]
    };

    // ── A refused caller: nothing happens, not even the directory ────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8081\n",
                false
            ),
            Err(ManagerError::NotAuthorized),
            "refused write of an instance file"
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", "[m]\nmodel = /m/a.gguf\n", false),
            Err(ManagerError::NotAuthorized),
            "refused write of models.ini"
        );
        assert!(
            !sys.exists(),
            "a refused write must not create the directory"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure)
            ],
            "writes ask for the configure action, for this caller"
        );
    }

    // ── No directory yet: no instances, unknown ids refused ──────────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(m.list_instances(), Vec::<String>::new());
        assert_eq!(m.start(&alice, "default"), Err(ManagerError::NoInstance));
        assert_eq!(m.restart(&alice, "default"), Err(ManagerError::NoInstance));
        for bad in ["", "../x", "..", "a/b", "Default", "a_b", "-a", "x.ini"] {
            assert_eq!(
                m.status(bad),
                Err(ManagerError::InvalidId),
                "status {bad:?}"
            );
            assert_eq!(
                m.start(&alice, bad),
                Err(ManagerError::InvalidId),
                "start {bad:?}"
            );
            assert_eq!(
                m.stop(&alice, bad),
                Err(ManagerError::InvalidId),
                "stop {bad:?}"
            );
            assert_eq!(
                m.restart(&alice, bad),
                Err(ManagerError::InvalidId),
                "restart {bad:?}"
            );
        }
        assert_eq!(
            m.read_file(Some(&alice), "models.ini"),
            Ok(None),
            "missing file reads as None"
        );
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "no service call for a refused id"
        );
        assert!(!sys.exists(), "asking must not create the directory");
        // Stop and status need only a valid id: an instance whose file was
        // deleted while its unit runs stays visible and stoppable.
        assert_eq!(m.status("default"), Ok(UnitState::Active));
        assert_eq!(m.stop(&alice, "default"), Ok(()));
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![Call::Status("default".into()), Call::Stop("default".into())],
            "status and stop of a valid id with no file reach the service"
        );
        assert!(
            !sys.exists(),
            "stop and status must not create the directory"
        );
    }

    // ── Writes: the directory created, atomic, no temp file left ─────────
    let default_ini = "[Server]\nPort = 8081\nHostname = 127.0.0.1\n";
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n",
    );
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        // The instance the writes belong to: its section of the record, which
        // is what the manager's render pass writes a render for.
        record(&["default"]);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", default_ini, false),
            Ok(())
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &model_profiles, false),
            Ok(())
        );
        assert_eq!(
            m.write_file(
                &alice,
                "models.ini.disabled",
                &real_model_files(dir.path(), "[Old]\nmodel = /m/old.gguf\n"),
                false
            ),
            Ok(())
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).expect("written"),
            default_ini
        );
        assert_eq!(
            files_in(&sys),
            set(&[
                "instances.ini",
                "server@default.ini",
                "models.ini",
                "models.ini.disabled",
                "models@default.ini"
            ]),
            "exactly the files written, no temp file left"
        );
        // The record's section is what makes the instance: the listing reads it.
        assert_eq!(m.list_instances(), ["default"]);
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(default_ini)))
        );
        assert_eq!(
            m.read_file(Some(&alice), "models.ini"),
            Ok(Some(plain(&model_profiles)))
        );
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "a write calls no service"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Control),
            ],
            "writes ask for configure, the instance file's read for control, \
             the preset file's read for nothing"
        );

        // The render of every instance is checked against that instance's own
        // ModelsMax: two model profiles load on startup, default's max is 1,
        // so the pool is refused, the refusal naming the instance. The
        // assignments say the render holds the pool; the manager writes that
        // file in a later step, so the fixture writes it by hand.
        std::fs::write(sys.join("assignments.ini"), "[default]\n* = inherit\n")
            .expect("assignments.ini");
        std::fs::write(
            sys.join("server@default.ini"),
            format!("{default_ini}ModelsMax = 1\n"),
        )
        .expect("ModelsMax fixture");
        let two_loading = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
             [b]\nmodel = /m/b.gguf\nload-on-startup = true\n",
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &two_loading, false),
            Err(ManagerError::Invalid(pai_core::checks::Refusal {
                key: "load-on-startup-over-max",
                values: vec!["2".into(), "1".into(), "default".into()],
            })),
            "the render of default, not the pool, is over default's max"
        );
        assert_eq!(
            m.read_file(Some(&alice), "models.ini"),
            Ok(Some(plain(&model_profiles))),
            "the refused pool wrote nothing"
        );
        // The same pool is accepted once that instance's own max holds it.
        std::fs::write(
            sys.join("server@default.ini"),
            format!("{default_ini}ModelsMax = 2\n"),
        )
        .expect("ModelsMax fixture");
        assert_eq!(
            m.write_file(&alice, "models.ini", &two_loading, false),
            Ok(())
        );
        // The pool and the instance file the blocks that follow expect.
        assert_eq!(
            m.write_file(&alice, "server@default.ini", default_ini, false),
            Ok(())
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &model_profiles, false),
            Ok(())
        );

        // Any other name is refused, and nothing is created or read.
        let before = files_in(&sys);
        for name in bad_names(&sys) {
            assert_eq!(
                m.write_file(&alice, &name, "[Server]\n", false),
                Err(ManagerError::BadFileName),
                "write {name:?}"
            );
            assert_eq!(
                m.read_file(Some(&alice), &name),
                Err(ManagerError::BadFileName),
                "read {name:?}"
            );
        }
        assert_eq!(files_in(&sys), before, "a refused name writes nothing");
        assert!(
            !dir.path().join("x").exists(),
            "nothing escapes the directory"
        );
    }

    // ── Model profile reads, list and status ask no authorization ───────────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(m.list_instances(), ["default"]);
        assert_eq!(
            m.read_file(Some(&alice), "models.ini"),
            Ok(Some(plain(&model_profiles)))
        );
        assert_eq!(m.status("default"), Ok(UnitState::Active));
        assert!(
            m.authority().asked.lock().unwrap().is_empty(),
            "no question for reads"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![Call::Status("default".into())]
        );
    }

    // ── A refused caller cannot control an existing instance ─────────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(m.start(&alice, "default"), Err(ManagerError::NotAuthorized));
        assert_eq!(m.stop(&alice, "default"), Err(ManagerError::NotAuthorized));
        assert_eq!(
            m.restart(&alice, "default"),
            Err(ManagerError::NotAuthorized)
        );
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "no service call when refused"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(alice.clone(), Action::Control); 3],
            "start, stop and restart ask for the control action"
        );
        // A refused caller learns nothing about which ids exist.
        assert_eq!(m.start(&alice, "nope"), Err(ManagerError::NotAuthorized));
        assert_eq!(m.start(&alice, "../x"), Err(ManagerError::NotAuthorized));
        assert!(m.service().calls.lock().unwrap().is_empty());
        // Nor can it change a file.
        assert_eq!(
            m.write_file(&alice, "server@default.ini", "[Server]\nPort = 1\n", false),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.write_file(&alice, "server@new.ini", "[Server]\nPort = 1\n", false),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).expect("still there"),
            default_ini
        );
        assert!(!sys.join("server@new.ini").exists());
    }

    // ── Control of an existing instance goes to the service, by id ───────
    {
        let mut service = FakeService::new();
        service.state = UnitState::Failed;
        let m = Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
            .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(m.start(&alice, "default"), Ok(()));
        assert_eq!(m.stop(&alice, "default"), Ok(()));
        assert_eq!(m.restart(&alice, "default"), Ok(()));
        assert_eq!(m.status("default"), Ok(UnitState::Failed));
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![
                Call::Start("default".into()),
                Call::Stop("default".into()),
                Call::Restart("default".into()),
                Call::Status("default".into()),
            ]
        );
        // Another instance's file does not make an unknown id one.
        assert_eq!(m.start(&alice, "lan-2"), Err(ManagerError::NoInstance));
        assert_eq!(m.restart(&alice, "lan-2"), Err(ManagerError::NoInstance));
        assert_eq!(
            m.service().calls.lock().unwrap().len(),
            4,
            "no start or restart for an id with no file"
        );
        // Stop and status of it still reach the service.
        assert_eq!(m.stop(&alice, "lan-2"), Ok(()));
        assert_eq!(m.status("lan-2"), Ok(UnitState::Failed));
        assert_eq!(
            m.service().calls.lock().unwrap()[4..],
            [Call::Stop("lan-2".into()), Call::Status("lan-2".into())]
        );
    }

    // ── A service failure is reported as one ─────────────────────────────
    {
        let mut service = FakeService::new();
        service.failing = vec!["default".into()];
        let m = Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
            .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert!(
            matches!(m.start(&alice, "default"), Err(ManagerError::Service(_))),
            "start failure"
        );
        assert!(
            matches!(m.status("default"), Err(ManagerError::Service(_))),
            "status failure"
        );
    }

    // ── Address collisions between instance files ────────────────────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        // Every save below is of a profile an instance runs: a profile no
        // instance names binds nothing, so it is judged against nothing and
        // any address is free. Each refusal names the instance whose profile
        // holds the address.
        record(&["default", "lan-2"]);
        // The same address as `default` (127.0.0.1:8081): refused, naming it.
        assert_eq!(
            m.write_file(&alice, "server@lan-2.ini", default_ini, false),
            Err(ManagerError::AddressInUse("default".into()))
        );
        assert!(
            !sys.join("server@lan-2.ini").exists(),
            "refused: not written"
        );
        // The same port on another specific host: fine.
        let lan2 = "[Server]\nPort = 8081\nHostname = 192.168.1.5\n";
        assert_eq!(
            m.write_file(&alice, "server@lan-2.ini", lan2, false),
            Ok(())
        );
        record(&["default", "lan-2", "loop"]);
        // Host names are resolved: localhost is default's 127.0.0.1.
        assert_eq!(
            m.write_file(
                &alice,
                "server@loop.ini",
                "[Server]\nPort = 8081\nHostname = localhost\n",
                false
            ),
            Err(ManagerError::AddressInUse("default".into()))
        );
        assert!(
            !sys.join("server@loop.ini").exists(),
            "refused: not written"
        );
        // An instance whose profile file was never written (its own save was
        // refused) is skipped: only default and lan-2 hold an address.
        record(&["default", "lan-2", "loop", "named"]);
        // Two names of one address collide, whichever is written.
        assert_eq!(
            m.write_file(
                &alice,
                "server@named.ini",
                "[Server]\nPort = 8081\nHostname = gpu-box\n",
                false
            ),
            Err(ManagerError::AddressInUse("lan-2".into()))
        );
        record(&["default", "lan-2", "loop", "named", "multi-home"]);
        // A name with several addresses collides when one matches.
        assert_eq!(
            m.write_file(
                &alice,
                "server@multi-home.ini",
                "[Server]\nPort = 8081\nHostname = multi-home\n",
                false
            ),
            Err(ManagerError::AddressInUse("lan-2".into()))
        );
        // A host name that does not resolve: refused, naming it, not written.
        assert_eq!(
            m.write_file(
                &alice,
                "server@nowhere.ini",
                "[Server]\nPort = 8090\nHostname = nowhere.example\n",
                false
            ),
            Err(ManagerError::UnresolvableHost("nowhere.example".into()))
        );
        assert!(
            !sys.join("server@nowhere.ini").exists(),
            "refused: not written"
        );
        record(&["default", "lan-2", "loop", "named", "multi-home", "multi"]);
        // A host shared inside a Hostname list collides.
        assert_eq!(
            m.write_file(
                &alice,
                "server@multi.ini",
                "[Server]\nPort = 8081\nHostname = 10.0.0.1, 192.168.1.5\n",
                false
            ),
            Err(ManagerError::AddressInUse("lan-2".into()))
        );
        record(&[
            "default",
            "lan-2",
            "loop",
            "named",
            "multi-home",
            "multi",
            "wild",
        ]);
        // A wildcard host on the same port collides with every host.
        for wild in ["0.0.0.0", "::"] {
            let text = format!("[Server]\nPort = 8081\nHostname = {wild}\n");
            assert!(
                matches!(
                    m.write_file(&alice, "server@wild.ini", &text, false),
                    Err(ManagerError::AddressInUse(ref other)) if other == "default" || other == "lan-2"
                ),
                "wildcard {wild}"
            );
        }
        // A wildcard on another port is fine.
        assert_eq!(
            m.write_file(
                &alice,
                "server@wild.ini",
                "[Server]\nPort = 8082\nHostname = 0.0.0.0\n",
                false
            ),
            Ok(())
        );
        record(&["default", "lan-2", "wild", "late"]);
        // Then a specific host on that port collides with the wildcard.
        assert_eq!(
            m.write_file(
                &alice,
                "server@late.ini",
                "[Server]\nPort = 8082\nHostname = 10.0.0.1\n",
                false
            ),
            Err(ManagerError::AddressInUse("wild".into()))
        );
        // Rewriting an instance with its own address is fine.
        assert_eq!(
            m.write_file(&alice, "server@default.ini", default_ini, false),
            Ok(())
        );
        // Moving it onto another instance's address is refused, the file kept.
        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8081\nHostname = 192.168.1.5\n",
                false
            ),
            Err(ManagerError::AddressInUse("lan-2".into()))
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).expect("kept"),
            default_ini
        );
        // Defaults count: no Port key is the default port, no Hostname the
        // default host, so two bare files collide, and an explicit default
        // collides with an implicit one.
        assert_eq!(
            m.write_file(&alice, "server@bare.ini", "[Server]\n", false),
            Ok(())
        );
        record(&["default", "lan-2", "wild", "bare", "bare-2"]);
        assert_eq!(
            m.write_file(&alice, "server@bare-2.ini", "[Server]\n", false),
            Err(ManagerError::AddressInUse("bare".into()))
        );
        record(&["default", "lan-2", "wild", "bare", "bare-2", "explicit"]);
        let explicit = format!(
            "[Server]\nPort = {}\nHostname = localhost\n",
            crate::server_cfg::DEFAULT_PORT
        );
        assert_eq!(
            m.write_file(&alice, "server@explicit.ini", &explicit, false),
            Err(ManagerError::AddressInUse("bare".into()))
        );
        // Another instance whose host no longer resolves is skipped: it
        // does not block writing this one.
        std::fs::write(
            sys.join("server@stale.ini"),
            "[Server]\nPort = 8083\nHostname = gone.example\n",
        )
        .expect("stale fixture");
        // fresh is an instance too, so the check runs and walks stale's
        // unresolvable host: an unused profile would be accepted unread.
        record(&["default", "lan-2", "wild", "bare", "stale", "fresh"]);
        assert_eq!(
            m.write_file(
                &alice,
                "server@fresh.ini",
                "[Server]\nPort = 8083\nHostname = 10.0.0.1\n",
                false
            ),
            Ok(()),
            "an unresolvable other instance is skipped"
        );
        std::fs::remove_file(sys.join("server@stale.ini")).expect("drop stale");
        std::fs::remove_file(sys.join("server@fresh.ini")).expect("drop fresh");
        // Its section goes with it: the record is the listing.
        record(&["default", "lan-2", "wild", "bare"]);
        // models.ini has no address: never checked.
        assert_eq!(
            m.write_file(&alice, "models.ini", &model_profiles, false),
            Ok(())
        );
        assert_eq!(m.list_instances(), ["bare", "default", "lan-2", "wild"]);
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "a write calls no service"
        );
    }

    // ── A failed write leaves the old content ────────────────────────────
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        let before = files_in(&sys);
        std::fs::set_permissions(&sys, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        // Root writes into a read-only directory anyway: then there is no
        // failure to provoke.
        let probe = sys.join("probe");
        let enforced = std::fs::write(&probe, "").is_err();
        let _ = std::fs::remove_file(&probe);
        let result = m.write_file(
            &alice,
            "models.ini",
            &real_model_files(dir.path(), "[Other]\nmodel = /m/o.gguf\n"),
            false,
        );
        std::fs::set_permissions(&sys, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
        if enforced {
            assert!(
                matches!(result, Err(ManagerError::Io(_))),
                "write failure: {result:?}"
            );
            assert_eq!(
                std::fs::read_to_string(sys.join("models.ini")).expect("kept"),
                model_profiles
            );
            assert_eq!(files_in(&sys), before, "no temp file left");
        }
    }

    // ── Start at boot: read by anyone, changed under configure ──────────
    // systemd and the SCM start the enabled instances; the manager only
    // reads and changes which ones are.
    {
        let bob = Caller(":1.43".into());
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(
            m.boot_enabled("default"),
            Ok(false),
            "a new instance starts disabled"
        );
        assert_eq!(m.set_boot_enabled(&bob, "default", true), Ok(()));
        assert_eq!(m.boot_enabled("default"), Ok(true));
        assert_eq!(m.boot_enabled("lan-2"), Ok(false), "only default changed");
        assert_eq!(m.set_boot_enabled(&bob, "default", false), Ok(()));
        assert_eq!(m.boot_enabled("default"), Ok(false));
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![
                Call::BootEnabled("default".into()),
                Call::SetBootEnabled("default".into(), true),
                Call::BootEnabled("default".into()),
                Call::BootEnabled("lan-2".into()),
                Call::SetBootEnabled("default".into(), false),
                Call::BootEnabled("default".into()),
            ]
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(bob.clone(), Action::Configure); 2],
            "only the changes ask, for configure"
        );

        // An id with no server@<id>.ini: refused before the service
        // manager is asked, either way.
        m.service().calls.lock().unwrap().clear();
        assert_eq!(m.boot_enabled("lan-3"), Err(ManagerError::NoInstance));
        assert_eq!(
            m.set_boot_enabled(&bob, "lan-3", true),
            Err(ManagerError::NoInstance)
        );
        for bad in ["", "../x", "Default", "a_b"] {
            assert_eq!(m.boot_enabled(bad), Err(ManagerError::InvalidId), "{bad:?}");
            assert_eq!(
                m.set_boot_enabled(&bob, bad, true),
                Err(ManagerError::InvalidId),
                "{bad:?}"
            );
        }
        assert_eq!(*m.service().calls.lock().unwrap(), vec![]);
    }

    // ── Start at boot: a refused caller and a failing service manager ───
    {
        let bob = Caller(":1.43".into());
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(
            m.set_boot_enabled(&bob, "default", true),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.boot_enabled("default"),
            Ok(false),
            "the read needs no authorization, and the refusal changed nothing"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![Call::BootEnabled("default".into())],
            "a refused caller reaches no service manager"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(bob.clone(), Action::Configure)]
        );

        let mut service = FakeService::new();
        service.failing = vec!["default".into()];
        let m = Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
            .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert!(
            matches!(m.boot_enabled("default"), Err(ManagerError::Service(_))),
            "the service manager's failure"
        );
        assert!(matches!(
            m.set_boot_enabled(&bob, "default", true),
            Err(ManagerError::Service(_))
        ));
    }
}

/// E2E phase for the instance files' secret (`OpencodeApiKey`), for this
/// crate's e2e test to call (it sets the process-wide
/// `PRIVATE_AI_DATA_ROOT` redirect, `src/tests/mod.rs`): an authorized read
/// of an instance file, a model profile read with no caller, and the writes going
/// through the group seam with their modes.
#[cfg(test)]
pub(crate) fn manager_instance_files_e2e() {
    use fakes::{FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let alice = Caller(":1.42".into());
    let names = || -> std::collections::BTreeSet<String> {
        std::fs::read_dir(&sys)
            .expect("read_dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    };
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
    let fixtures = tempfile::tempdir().expect("tempdir");
    let model_profiles = real_model_files(
        fixtures.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    );

    // ── Writes go through the group seam: instance files only ────────────
    {
        let group = FakeGroup::new();
        let group_calls = group.calls.clone();
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(group);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", ini, false),
            Ok(())
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &model_profiles, false),
            Ok(())
        );
        let c = group_calls.lock().unwrap().clone();
        assert_eq!(c.len(), 1, "one group change, the instance file's: {c:?}");
        assert_eq!(c[0].group, INSTANCE_GROUP);
        assert_eq!(c[0].path.parent(), Some(sys.as_path()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode =
                |n: &str| std::fs::metadata(sys.join(n)).unwrap().permissions().mode() & 0o7777;
            assert_eq!(mode("server@default.ini"), 0o640);
            assert_eq!(mode("models.ini"), 0o644);
            assert_eq!(c[0].mode, 0o640);
        }

        // ── An authorized read of an instance file: asked, then read ─────
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(ini)))
        );
        assert_eq!(m.read_file(Some(&alice), "server@nope.ini"), Ok(None));
        // ── The model profile files need no caller at all ───────────────────────
        assert_eq!(
            m.read_file(None::<&Caller>, "models.ini"),
            Ok(Some(plain(&model_profiles)))
        );
        assert_eq!(
            m.read_file(None::<&Caller>, "models.ini.disabled"),
            Ok(None)
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Control),
                (alice.clone(), Action::Control),
            ],
            "the instance file reads ask for control, the preset reads nothing"
        );
    }

    // ── No group private-ai: the write refused, the old file kept ────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::failing(std::io::ErrorKind::NotFound));
        let before = names();
        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8090\n",
                false
            ),
            Err(ManagerError::Io(std::io::ErrorKind::NotFound))
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).unwrap(),
            ini,
            "the old file is kept"
        );
        assert_eq!(names(), before, "no temp file left");
        // The model profile files need no group: still written.
        assert_eq!(
            m.write_file(
                &alice,
                "models.ini.disabled",
                "[Old]\nmodel = /m/old.gguf\n",
                false
            ),
            Ok(())
        );
    }
}

/// E2E phase for the API key withheld from a reader who may not configure,
/// for this crate's e2e test to call (it sets the process-wide
/// `PRIVATE_AI_DATA_ROOT` redirect, `src/tests/mod.rs`). An instance file
/// needs only control to read, so a caller without configure (asked without
/// a prompt: a polkit challenge counts as no) gets the `OpencodeApiKey` line
/// blanked to `OpencodeApiKey =`, every other line as it was, and is told
/// the key was withheld; a caller with configure gets the file as saved.
/// The model profile files are never touched.
#[cfg(test)]
pub(crate) fn manager_withholds_api_key_e2e() {
    use fakes::{FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let put = |name: &str, text: &str| std::fs::write(sys.join(name), text).expect("write");
    let alice = Caller(":1.42".into());
    // `control` granted; `configure` asked quietly answers `configure`.
    let manager = |configure: bool| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(configure),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
    };
    let ini = "\
; Written by pai-config.
[Server]
Port = 8080
Hostname = 127.0.0.1
; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication
OpencodeBaseUrl = http://127.0.0.1:8080/v1
OpencodeApiKey = sk-local-3f9a
ModelsMax = 2
";
    let blanked = "\
; Written by pai-config.
[Server]
Port = 8080
Hostname = 127.0.0.1
; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication
OpencodeBaseUrl = http://127.0.0.1:8080/v1
OpencodeApiKey =
ModelsMax = 2
";
    put("server@default.ini", ini);

    // ── No configure: the value blanked, the rest as saved, withheld said ─
    {
        let m = manager(false);
        let read = m.read_file(Some(&alice), "server@default.ini");
        assert_eq!(read, Ok(Some(withheld(blanked))));
        let text = read.unwrap().unwrap().text;
        assert!(!text.contains("sk-local-3f9a"), "{text}");
        assert_eq!(
            crate::server_cfg::from_text(&text).opencode_api_key,
            None,
            "read back, the blanked file holds no key"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(alice.clone(), Action::Control)],
            "control asked as before; configure never asked with a prompt"
        );
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![(alice.clone(), Action::Configure)],
            "configure asked without a prompt"
        );
    }

    // ── Configure: the file as saved, nothing withheld, still no prompt ───
    {
        let m = manager(true);
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(ini)))
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(alice.clone(), Action::Control)],
            "an administrator's read prompts for nothing more than control"
        );
    }

    // ── Refused control: NotAuthorized, configure not even asked ──────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false).quietly(true),
            FakeResolver::lan(),
        );
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(*m.authority().asked_quietly.lock().unwrap(), vec![]);
    }

    // ── Nothing to withhold: no key, a blank key, only the commented hint ─
    for text in [
        "[Server]\nPort = 8080\n",
        "[Server]\nPort = 8080\nOpencodeApiKey =\n",
        "[Server]\nPort = 8080\nOpencodeApiKey =   \n",
        "[Server]\nPort = 8080\n; OpencodeApiKey = sk-xxx  ; blank = none\n",
        "",
    ] {
        put("server@lan-2.ini", text);
        let m = manager(false);
        assert_eq!(
            m.read_file(Some(&alice), "server@lan-2.ini"),
            Ok(Some(plain(text))),
            "nothing withheld from {text:?}"
        );
    }

    // ── Every key line blanked: a duplicate, an inline comment, CRLF ──────
    {
        let m = manager(false);
        put(
            "server@lan-2.ini",
            "[Server]\r\nOpencodeApiKey = sk-old-1111 ; rotated\r\nPort = 8081\r\nOpencodeApiKey = sk-new-2222\r\n",
        );
        assert_eq!(
            m.read_file(Some(&alice), "server@lan-2.ini"),
            Ok(Some(withheld(
                "[Server]\r\nOpencodeApiKey =\r\nPort = 8081\r\nOpencodeApiKey =\r\n"
            ))),
            "both key lines blanked, the inline comment with the value, the line ends kept"
        );
        // Spelled as the parser still reads it: spaces around, none at all.
        put(
            "server@lan-2.ini",
            "[Server]\n  OpencodeApiKey=sk-tight-3333\nPort = 8081\n",
        );
        let read = m
            .read_file(Some(&alice), "server@lan-2.ini")
            .unwrap()
            .unwrap();
        assert!(read.withheld, "{read:?}");
        assert!(!read.text.contains("sk-tight-3333"), "{read:?}");
        assert!(read.text.contains("\nPort = 8081\n"), "{read:?}");
        assert_eq!(
            crate::server_cfg::from_text(&read.text).opencode_api_key,
            None
        );
    }

    // ── A missing instance file is still None ────────────────────────────
    assert_eq!(
        manager(false).read_file(Some(&alice), "server@nope.ini"),
        Ok(None)
    );

    // ── The model profile files are never blanked, nor is configure asked ───────
    {
        let m = manager(false);
        let model_profiles = real_model_files(dir.path(), "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\nOpencodeApiKey = not-a-secret-here\n");
        put("models.ini", &model_profiles);
        put("models.ini.disabled", &model_profiles);
        assert_eq!(
            m.read_file(None::<&Caller>, "models.ini"),
            Ok(Some(plain(&model_profiles)))
        );
        assert_eq!(
            m.read_file(Some(&alice), "models.ini.disabled"),
            Ok(Some(plain(&model_profiles)))
        );
        assert_eq!(*m.authority().asked_quietly.lock().unwrap(), vec![]);
    }
}

/// E2E phase for a write that keeps the API key, for this crate's e2e test
/// to call (it sets the process-wide `PRIVATE_AI_DATA_ROOT` redirect,
/// `src/tests/mod.rs`). A caller the key was withheld from saves the file
/// it read, its key line blanked or gone: with `keep_api_key`, once the write
/// is authorized (configure, as every write), the manager puts the key it
/// holds back. The incoming `OpencodeApiKey` line takes the current value,
/// in place; with no such line, one is added (a client renders an unset key
/// as a commented hint only, so adding none would drop the key all the
/// same). With nothing held, or without `keep_api_key`, the text is written
/// as sent. The model profile files are never touched.
#[cfg(test)]
pub(crate) fn manager_keeps_api_key_e2e() {
    use fakes::{FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let put = |name: &str, text: &str| std::fs::write(sys.join(name), text).expect("write");
    let get = |name: &str| std::fs::read_to_string(sys.join(name)).unwrap_or_default();
    let alice = Caller(":1.42".into());
    let manager = |allow: bool| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(false),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
    };
    let key_of = |text: &str| crate::server_cfg::from_text(text).opencode_api_key;
    let key_lines = |text: &str| {
        text.lines()
            .filter(|l| {
                let t = l.trim();
                !t.starts_with([';', '#'])
                    && t.split_once('=')
                        .is_some_and(|(k, _)| k.trim() == "OpencodeApiKey")
            })
            .count()
    };
    let saved = "\
; Written by pai-config.
[Server]
Port = 8080
Hostname = 127.0.0.1
; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication
OpencodeBaseUrl = http://127.0.0.1:8080/v1
OpencodeApiKey = sk-local-3f9a
ModelsMax = 2
";
    // What a caller without configure read, then saved with Port changed.
    let blanked = "\
; Written by pai-config.
[Server]
Port = 8090
Hostname = 127.0.0.1
; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication
OpencodeBaseUrl = http://127.0.0.1:8080/v1
OpencodeApiKey =
ModelsMax = 2
";
    let kept = "\
; Written by pai-config.
[Server]
Port = 8090
Hostname = 127.0.0.1
; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication
OpencodeBaseUrl = http://127.0.0.1:8080/v1
OpencodeApiKey = sk-local-3f9a
ModelsMax = 2
";
    // A client's rendering of a config with no key: the hint, no key line.
    let no_line = "\
[Server]
Port = 8090
Hostname = 127.0.0.1
; OpencodeApiKey = sk-xxx  ; API key for proxy/gateway authentication (opencode.json apiKey + Claude Code); blank = none
ModelsMax = 2
";

    // ── The blanked line takes the held value, in place ──────────────────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", blanked, true),
            Ok(())
        );
        assert_eq!(get("server@default.ini"), kept);
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(alice.clone(), Action::Configure)],
            "configure asked, as for every write"
        );
    }

    // ── No key line at all: the held key is added, the rest as sent ──────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", no_line, true),
            Ok(())
        );
        let written = get("server@default.ini");
        assert!(
            written.starts_with(no_line),
            "the text as sent, first:\n{written}"
        );
        assert_eq!(key_of(&written), Some("sk-local-3f9a".into()), "{written}");
        assert_eq!(key_lines(&written), 1, "{written}");
        assert_eq!(crate::server_cfg::from_text(&written).port, Some(8090));
    }

    // ── keep_api_key wins over a value sent with it ──────────────────────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        let typed = blanked.replace("OpencodeApiKey =\n", "OpencodeApiKey = sk-typed-0000\n");
        assert_eq!(
            m.write_file(&alice, "server@default.ini", &typed, true),
            Ok(())
        );
        assert_eq!(get("server@default.ini"), kept);
    }

    // ── Without keep_api_key: as sent, the key dropped (as before) ───────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", blanked, false),
            Ok(())
        );
        assert_eq!(get("server@default.ini"), blanked);
        assert_eq!(key_of(&get("server@default.ini")), None);
    }

    // ── Nothing held: as sent, no line invented ──────────────────────────
    {
        let m = manager(true);
        for held in [
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nOpencodeApiKey =\n",
        ] {
            put("server@default.ini", held);
            for sent in [blanked, no_line] {
                assert_eq!(
                    m.write_file(&alice, "server@default.ini", sent, true),
                    Ok(())
                );
                assert_eq!(get("server@default.ini"), sent, "held {held:?}");
            }
        }
        // A new instance: no current file at all.
        assert_eq!(
            m.write_file(
                &alice,
                "server@lan-2.ini",
                no_line.replace("8090", "8091").as_str(),
                true
            ),
            Ok(())
        );
        assert_eq!(get("server@lan-2.ini"), no_line.replace("8090", "8091"));
        std::fs::remove_file(sys.join("server@lan-2.ini")).expect("remove lan-2");
    }

    // ── Refused: nothing written, the key still there ────────────────────
    {
        put("server@default.ini", saved);
        let m = manager(false);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", blanked, true),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(get("server@default.ini"), saved);
    }

    // ── The content checks still run, on what would be written ───────────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        let bad = blanked.replace("Port = 8090", "Port = 0");
        assert!(
            matches!(
                m.write_file(&alice, "server@default.ini", &bad, true),
                Err(ManagerError::Invalid(_))
            ),
            "a bad Port is refused with the key kept or not"
        );
        assert_eq!(get("server@default.ini"), saved);
    }

    // ── The held file cannot be read: refused, never written blanked ─────
    // An I/O error other than not-found (here permission denied) says
    // nothing about the key: writing the blanked text would erase it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = sys.join("server@default.ini");
        put("server@default.ini", saved);
        let mode = |m: u32| {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(m)).expect("chmod")
        };
        mode(0o000);
        // Root ignores the mode: only a real read failure tests anything.
        if std::fs::read_to_string(&path).is_ok() {
            mode(0o644);
            eprintln!("skipped: the unreadable-file case needs a non-root user");
        } else {
            let m = manager(true);
            let got = m.write_file(&alice, "server@default.ini", blanked, true);
            mode(0o644);
            assert_eq!(
                got,
                Err(ManagerError::Io(std::io::ErrorKind::PermissionDenied)),
                "refused on the read error"
            );
            assert_eq!(get("server@default.ini"), saved, "the file as it was");
        }
    }

    // ── A model profile file is written as sent ─────────────────────────────────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        let fixtures = tempfile::tempdir().expect("tempdir");
        let model_profiles = real_model_files(
            fixtures.path(),
            "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &model_profiles, true),
            Ok(())
        );
        assert_eq!(get("models.ini"), model_profiles);
    }
}

/// E2E phase for where the instance files live, for this crate's e2e test to
/// call (it sets the process-wide `PRIVATE_AI_DATA_ROOT` redirect,
/// `src/tests/mod.rs`): the engine's directory (`paths::engine_config_dir`,
/// `/etc/llama.cpp` on Linux), never the framework's
/// (`paths::system_config_dir`, `/etc/private-ai`, which keeps `bff.ini`).
/// Files an earlier layout left in the framework's directory are neither
/// read, listed, nor moved.
#[cfg(test)]
pub(crate) fn manager_engine_dir_e2e() {
    use fakes::{FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let engine = crate::paths::engine_config_dir();
    let framework = crate::paths::system_config_dir();
    let alice = Caller(":1.42".into());
    let names = |d: &std::path::Path| -> std::collections::BTreeSet<String> {
        std::fs::read_dir(d)
            .map(|r| {
                r.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    };

    // An earlier layout's files and the framework's own, in its directory.
    std::fs::create_dir_all(&framework).expect("framework dir");
    let stray_ini = "[Server]\nPort = 8090\nHostname = 127.0.0.1\n";
    let stray_model_profiles =
        real_model_files(dir.path(), "[stray]\nmodel = /models/stray.gguf\n");
    for (name, text) in [
        ("server@stray.ini", stray_ini),
        ("server@default.ini", stray_ini),
        ("models.ini", &stray_model_profiles),
        ("bff.ini", "[Bff]\nListen = 127.0.0.1:8590\n"),
    ] {
        std::fs::write(framework.join(name), text).expect("framework file");
    }
    let framework_before = names(&framework);

    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new());

    // ── Nothing of the framework's directory counts ───────────────────────
    assert_eq!(m.list_instances(), Vec::<String>::new(), "no instance yet");
    assert_eq!(m.read_file(None::<&Caller>, "models.ini"), Ok(None));
    assert_eq!(m.read_file(Some(&alice), "server@default.ini"), Ok(None));
    assert_eq!(
        m.start(&alice, "stray"),
        Err(ManagerError::NoInstance),
        "a file in the framework's directory is no instance"
    );
    assert!(m.service().calls.lock().unwrap().is_empty());

    // ── Writes land in the engine's directory ─────────────────────────────
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
    let fixtures = tempfile::tempdir().expect("tempdir");
    let model_profiles = real_model_files(
        fixtures.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    );
    let disabled = real_model_files(dir.path(), "[Old]\nmodel = /models/old.gguf\n");
    assert_eq!(
        m.write_file(&alice, "server@default.ini", ini, false),
        Ok(())
    );
    assert_eq!(
        m.write_file(&alice, "models.ini", &model_profiles, false),
        Ok(())
    );
    assert_eq!(
        m.write_file(&alice, "models.ini.disabled", &disabled, false),
        Ok(())
    );
    write_record(&engine, &["default"]);
    for (name, text) in [
        ("server@default.ini", ini),
        ("models.ini", &model_profiles),
        ("models.ini.disabled", &disabled),
    ] {
        assert_eq!(
            std::fs::read_to_string(engine.join(name)).ok().as_deref(),
            Some(text),
            "{name} in the engine's directory"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |n: &str| {
            std::fs::metadata(engine.join(n))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777
        };
        assert_eq!(mode("server@default.ini"), 0o640);
        assert_eq!(mode("models.ini"), 0o644);
        assert_eq!(mode("models.ini.disabled"), 0o644);
    }
    assert_eq!(names(&framework), framework_before, "nothing written there");
    for (name, text) in [
        ("server@default.ini", stray_ini),
        ("models.ini", &stray_model_profiles),
    ] {
        assert_eq!(
            std::fs::read_to_string(framework.join(name)).unwrap(),
            text,
            "{name}: the framework's directory is left as it was"
        );
    }

    // ── Reads and the list come from the engine's directory ───────────────
    assert_eq!(m.list_instances(), ["default"]);
    assert_eq!(
        m.read_file(None::<&Caller>, "models.ini"),
        Ok(Some(plain(&model_profiles)))
    );
    assert_eq!(
        m.read_file(Some(&alice), "server@default.ini"),
        Ok(Some(plain(ini)))
    );
    assert_eq!(m.start(&alice, "default"), Ok(()));

    // ── The address check reads the other instances there too ─────────────
    // server@stray.ini (port 8090) is in the framework's directory only: no
    // collision with it.
    assert_eq!(
        m.write_file(
            &alice,
            "server@lan-2.ini",
            "[Server]\nPort = 8090\nHostname = 127.0.0.1\n",
            false
        ),
        Ok(())
    );
    write_record(&engine, &["default", "lan-2"]);
    assert_eq!(m.list_instances(), ["default", "lan-2"]);
    assert!(
        !engine.join("server@stray.ini").exists(),
        "nothing moved from the framework's directory"
    );
}

/// E2E phase for the manager core while polkit waits for a user to
/// authenticate, for this crate's e2e test to call (it sets the process-wide
/// `PRIVATE_AI_DATA_ROOT` redirect, `src/tests/mod.rs`). A caller whose
/// authorization is pending holds up no other caller, reads or changes, and
/// gets its own answer once polkit gives it; writes still run one at a time,
/// so the address check of one sees the file another has just written.
#[cfg(test)]
pub(crate) fn manager_waits_e2e() {
    use fakes::{
        Call, FakeAuthority, FakeResolver, FakeService, Gate, GateAuthority, GateResolver,
        GATE_WAIT,
    };
    use std::sync::mpsc::Receiver;
    use std::sync::Arc;
    use std::time::Duration;

    /// The result on `rx`, which must come while another caller's question
    /// is still open at `gate`. On a timeout the gate is opened first, so
    /// the threads still waiting at it finish, then the test fails.
    fn answer<T>(gate: &Gate, what: &str, rx: Receiver<T>) -> T {
        let deadline = Duration::from_secs(2);
        rx.recv_timeout(deadline).unwrap_or_else(|_| {
            gate.open();
            panic!(
                "{what}: no answer within {deadline:?} while another caller's \
                 authorization was pending"
            )
        })
    }

    let alice = Caller(":1.42".into());
    let bob = Caller(":1.43".into());
    let carol = Caller(":1.44".into());
    let default_ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
    let fixtures = tempfile::tempdir().expect("tempdir");
    let model_profiles = real_model_files(
        fixtures.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    );

    // ── A pending authorization holds up no other caller ─────────────────
    {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        std::fs::create_dir_all(&sys).expect("system config dir");
        std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
        std::fs::write(sys.join("models.ini"), &model_profiles).expect("write presets");
        write_record(&sys, &["default"]);

        let gate = Gate::new();
        let m = Arc::new(
            Manager::new(
                FakeService::new(),
                GateAuthority::new(&alice, &gate, &[(&alice, true), (&bob, true)]),
                FakeResolver::lan(),
            )
            .with_group(crate::manager::service::fakes::FakeGroup::new()),
        );
        let pending = {
            let (m, alice) = (m.clone(), alice.clone());
            on_thread(move || m.start(&alice, "default"))
        };
        assert!(gate.arrived(1), "alice's start reached polkit");

        let status = {
            let m = m.clone();
            on_thread(move || m.status("default"))
        };
        assert_eq!(
            answer(&gate, "status", status),
            Ok(UnitState::Active),
            "status of the instance alice is starting"
        );
        let list = {
            let m = m.clone();
            on_thread(move || m.list_instances())
        };
        assert_eq!(answer(&gate, "list_instances", list), vec!["default"]);
        let read = {
            let m = m.clone();
            on_thread(move || m.read_file(None::<&Caller>, "models.ini"))
        };
        assert_eq!(
            answer(&gate, "read_file", read),
            Ok(Some(plain(&model_profiles)))
        );
        let stop = {
            let (m, bob) = (m.clone(), bob.clone());
            on_thread(move || m.stop(&bob, "default"))
        };
        assert_eq!(
            answer(&gate, "bob's stop", stop),
            Ok(()),
            "another caller's authorization is asked and answered"
        );
        let refused = {
            let (m, carol) = (m.clone(), carol.clone());
            on_thread(move || m.write_file(&carol, "models.ini", "[x]\nmodel = /m/x.gguf\n", false))
        };
        assert_eq!(
            answer(&gate, "carol's write", refused),
            Err(ManagerError::NotAuthorized),
            "a caller polkit refuses is refused at once"
        );
        let disabled = "[Qwen3.8-Flash-Next]\nmodel = /models/Qwen3.8-Flash-Next-IQ3_S.gguf\n";
        let write = {
            let (m, bob) = (m.clone(), bob.clone());
            on_thread(move || m.write_file(&bob, "models.ini.disabled", disabled, false))
        };
        assert_eq!(answer(&gate, "bob's write", write), Ok(()));

        assert!(
            pending.try_recv().is_err(),
            "alice's start is still waiting for polkit"
        );
        gate.open();
        let started = pending
            .recv_timeout(GATE_WAIT)
            .expect("alice's start returns once polkit answers");
        assert_eq!(started, Ok(()), "alice's own answer, after the others");
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![
                Call::Status("default".into()),
                Call::Stop("default".into()),
                Call::Start("default".into())
            ],
            "alice's start reaches systemd only after polkit answered"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![
                (alice.clone(), Action::Control),
                (bob.clone(), Action::Control),
                (carol.clone(), Action::Configure),
                (bob.clone(), Action::Configure)
            ]
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("models.ini")).unwrap(),
            model_profiles,
            "the refused write left models.ini"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("models.ini.disabled")).unwrap(),
            disabled
        );
    }

    // ── A pending caller refused in the end changes nothing ──────────────
    {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        std::fs::create_dir_all(&sys).expect("system config dir");
        std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
        // bob's start needs the record: a `server@default.ini` no section
        // names is no instance.
        write_record(&sys, &["default"]);

        let gate = Gate::new();
        let m = Arc::new(
            Manager::new(
                FakeService::new(),
                GateAuthority::new(&alice, &gate, &[(&alice, false), (&bob, true)]),
                FakeResolver::lan(),
            )
            .with_group(crate::manager::service::fakes::FakeGroup::new()),
        );
        let pending = {
            let (m, alice) = (m.clone(), alice.clone());
            on_thread(move || {
                m.write_file(&alice, "server@default.ini", "[Server]\nPort = 9\n", false)
            })
        };
        assert!(gate.arrived(1), "alice's write reached polkit");
        let start = {
            let (m, bob) = (m.clone(), bob.clone());
            on_thread(move || m.start(&bob, "default"))
        };
        assert_eq!(answer(&gate, "bob's start", start), Ok(()));
        gate.open();
        let refused = pending
            .recv_timeout(GATE_WAIT)
            .expect("alice's write returns once polkit answers");
        assert_eq!(refused, Err(ManagerError::NotAuthorized));
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).unwrap(),
            default_ini,
            "the refused write left the file"
        );
        assert_eq!(
            *m.service().calls.lock().unwrap(),
            vec![Call::Start("default".into())],
            "only bob's start reached systemd"
        );
    }

    // ── Writes run one at a time: a check sees the write before it ───────
    // The first write's address check is held at a slow host lookup (another
    // instance's name). The second, of a colliding address, must not slip in
    // between that check and the first write: one of them is written, the
    // other refused naming it, whichever went first.
    {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        std::fs::create_dir_all(&sys).expect("system config dir");
        std::fs::write(
            sys.join("server@slow.ini"),
            "[Server]\nPort = 9000\nHostname = slow-box\n",
        )
        .expect("write slow");
        // Both candidates of the race are in the record, so whichever write
        // lands second sees the other's file: a section whose file is missing
        // reads as the default binding, and 8081 on gpu-box is not that.
        write_record(&sys, &["lan-a", "lan-b", "slow"]);

        let gate = Gate::new();
        let resolver = GateResolver::new(
            FakeResolver::new(&[("gpu-box", &["192.168.1.5"]), ("slow-box", &["10.0.0.7"])]),
            "slow-box",
            &gate,
        );
        let m = Arc::new(
            Manager::new(FakeService::new(), FakeAuthority::new(true), resolver)
                .with_group(crate::manager::service::fakes::FakeGroup::new()),
        );
        let lan_a = "[Server]\nPort = 8081\nHostname = 192.168.1.5\n";
        let lan_b = "[Server]\nPort = 8081\nHostname = gpu-box\n";
        let first = {
            let (m, alice) = (m.clone(), alice.clone());
            on_thread(move || m.write_file(&alice, "server@lan-a.ini", lan_a, false))
        };
        assert!(
            gate.arrived(1),
            "the first write's address check reached the slow lookup"
        );
        let second = {
            let (m, bob) = (m.clone(), bob.clone());
            on_thread(move || m.write_file(&bob, "server@lan-b.ini", lan_b, false))
        };
        // Time for the second to run past the first, if it can.
        let early = second.recv_timeout(Duration::from_millis(500)).ok();
        gate.open();
        let first = first
            .recv_timeout(GATE_WAIT)
            .expect("the first write returns");
        let second = early
            .or_else(|| second.recv_timeout(GATE_WAIT).ok())
            .expect("the second write returns");
        let a = sys.join("server@lan-a.ini").is_file();
        let b = sys.join("server@lan-b.ini").is_file();
        match (&first, &second) {
            (Ok(()), Err(ManagerError::AddressInUse(other))) => {
                assert_eq!(other, "lan-a", "the second names the first");
                assert!(a && !b, "only lan-a written: {a} {b}");
                assert_eq!(
                    std::fs::read_to_string(sys.join("server@lan-a.ini")).unwrap(),
                    lan_a
                );
            }
            (Err(ManagerError::AddressInUse(other)), Ok(())) => {
                assert_eq!(other, "lan-b", "the first names the second");
                assert!(b && !a, "only lan-b written: {a} {b}");
            }
            _ => panic!(
                "two colliding writes ran at once, the second not seeing the first: \
                 first {first:?}, second {second:?}; lan-a written {a}, lan-b written {b}"
            ),
        }
    }
}

/// E2E phase for the content checks of `write_file`, for this crate's e2e
/// test to call (it sets the process-wide `PRIVATE_AI_DATA_ROOT` redirect,
/// `src/tests/mod.rs`). Invalid contents are refused with the check's
/// refusal after the authorization, and nothing is written.
#[cfg(test)]
pub(crate) fn manager_checks_e2e() {
    use fakes::{FakeAuthority, FakeResolver, FakeService};
    use pai_core::checks::Refusal;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let alice = Caller(":1.42".into());
    let manager = |allow: bool| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new())
    };
    let refusal = |key: &'static str, values: &[&str]| Refusal {
        key,
        values: values.iter().map(|v| v.to_string()).collect(),
    };
    let invalid =
        |key: &'static str, values: &[&str]| Err(ManagerError::Invalid(refusal(key, values)));
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let flagged = |ids: &[&str]| -> String {
        real_model_files(
            dir.path(),
            &ids.iter()
                .map(|id| format!("[{id}]\nmodel = /models/{id}.gguf\nload-on-startup = true\n"))
                .collect::<String>(),
        )
    };

    // ── A refused caller is told only that: no check runs first ──────────
    {
        let m = manager(false);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", "[Server]\nPort = 0\n", false),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", "[a b]\nmodel = /m/a.gguf\n", false),
            Err(ManagerError::NotAuthorized)
        );
        assert!(!sys.exists(), "nothing created");
    }

    // ── An invalid instance file: refused, nothing created ───────────────
    {
        let m = manager(true);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", "[Server]\nPort = 0\n", false),
            invalid("not-whole-number-in-range", &["Port", "0", "1", "65535"])
        );
        assert!(listing().is_empty(), "nothing written: {:?}", listing());
        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsDir = /models #1\n",
                false
            ),
            invalid("comment-markers", &["ModelsDir"])
        );
        assert!(listing().is_empty(), "nothing written: {:?}", listing());
        // The Server checks below the GUI: a bind list mixing 0.0.0.0 with
        // another address, a negative ModelsMax, a log level past 5.
        for (text, want) in [
            (
                "[Server]\nPort = 8081\nHostname = 0.0.0.0,127.0.0.1\n",
                invalid("bind-all-with-others", &[]),
            ),
            (
                "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = -1\n",
                invalid(
                    "not-whole-number-in-range",
                    &["ModelsMax", "-1", "0", "2147483647"],
                ),
            ),
            (
                "[Server]\nPort = 8081\nHostname = 127.0.0.1\nLogVerbosity = 7\n",
                invalid(
                    "not-whole-number-in-range",
                    &["LogVerbosity", "7", "0", "5"],
                ),
            ),
        ] {
            assert_eq!(
                m.write_file(&alice, "server@default.ini", text, false),
                want,
                "{text:?}"
            );
            assert!(listing().is_empty(), "nothing written: {:?}", listing());
        }
        assert_eq!(
            m.list_instances(),
            Vec::<String>::new(),
            "no instance added"
        );
    }

    // ── Valid files are written; then an invalid rewrite keeps the old ───
    let default_ini = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 2\n";
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\nctx-size = 172032\n",
    );
    {
        let m = manager(true);
        // default is an instance: its section of the record, before the writes
        // that make its render.
        write_record(&sys, &["default"]);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", default_ini, false),
            Ok(())
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &model_profiles, false),
            Ok(())
        );

        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMemoryMargin = -1\n",
                false
            ),
            invalid(
                "not-whole-number-in-range",
                &["ModelsMemoryMargin", "-1", "0", "2147483647"]
            )
        );
        assert_eq!(
            read("server@default.ini").as_deref(),
            Some(default_ini),
            "old kept"
        );

        let bad = format!("{model_profiles}[Other]\nmodel = C:\\Models #1\\o.gguf\n");
        assert_eq!(
            m.write_file(&alice, "models.ini", &bad, false),
            invalid("comment-markers", &["model"])
        );
        let bad = real_model_files(dir.path(), &format!(
            "{model_profiles}[Other]\nmodel = /models/o.gguf\nmoe-cache-mib = 4096\nsplit-mode = tensor\n"
        ));
        assert_eq!(
            m.write_file(&alice, "models.ini", &bad, false),
            invalid("moe-cache-with-tensor-split", &[])
        );
        assert_eq!(
            read("models.ini").as_deref(),
            Some(model_profiles.as_str()),
            "old kept"
        );
        assert_eq!(
            listing(),
            [
                "instances.ini",
                "models.ini",
                "models@default.ini",
                "server@default.ini"
            ],
            "no temp file left"
        );
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "a write calls no service"
        );
    }

    // ── load-on-startup against the instances' ModelsMax ─────────────────
    {
        let m = manager(true);
        // The assignments decide what each render loads, so they decide what
        // each ModelsMax has to hold. The manager writes the file in a later
        // step; the fixtures write it by hand.
        let a_none = "[default]\n* = inherit\n\n[lan-2]\n";
        let a_lan = "[default]\n* = inherit\n\n[lan-2]\na = inherit\n";
        let a_split = "[default]\nb = inherit\n\n[lan-2]\n* = inherit\n";
        std::fs::write(sys.join("assignments.ini"), a_none).expect("assignments.ini");
        // default's ModelsMax is 2, and default is an instance of the record.
        write_record(&sys, &["default"]);
        assert_eq!(
            m.write_file(&alice, "models.ini", &flagged(&["a", "b", "c"]), false),
            invalid("load-on-startup-over-max", &["3", "2", "default"]),
            "the refusal names the instance whose render is over its max"
        );
        assert_eq!(read("models.ini").as_deref(), Some(model_profiles.as_str()));
        assert_eq!(
            read("models@default.ini"),
            Some(pai_core::assign::render(&model_profiles, "", None)),
            "a refused write leaves the render as it was: it opens with the \
             generated header, so it is the pool rendered, never the pool text"
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &flagged(&["a", "b"]), false),
            Ok(())
        );
        // Lowering ModelsMax under them is refused too.
        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 1\n",
                false
            ),
            invalid("load-on-startup-over-max", &["2", "1", "default"])
        );
        assert_eq!(read("server@default.ini").as_deref(), Some(default_ini));
        // So is a new instance that could not start with what its own render
        // loads: lan-2 inherits a model profile and has no room for it.
        std::fs::write(sys.join("assignments.ini"), a_lan).expect("assignments.ini");
        // lan-2 joins the record: the render of a profile no instance runs is
        // judged against nothing, so its ModelsMax of 0 would be accepted.
        write_record(&sys, &["default", "lan-2"]);
        assert_eq!(
            m.write_file(
                &alice,
                "server@lan-2.ini",
                "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 0\n",
                false
            ),
            invalid("load-on-startup-unlimited", &["1", "lan-2"]),
            "the refusal names the instance whose render loads a preset with no ModelsMax"
        );
        assert!(read("server@lan-2.ini").is_none(), "not written");
        // The cross-instance minimum is gone: each instance is checked
        // against its own render. default is told one model profile, lan-2 all
        // three, and lan-2's own max of 3 holds them.
        std::fs::write(sys.join("assignments.ini"), a_split).expect("assignments.ini");
        assert_eq!(
            m.write_file(
                &alice,
                "server@lan-2.ini",
                "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 3\n",
                false
            ),
            Ok(())
        );
        write_record(&sys, &["default", "lan-2"]);
        assert_eq!(
            m.write_file(&alice, "models.ini", &flagged(&["a", "b", "c"]), false),
            Ok(()),
            "the pool has three presets and no instance is over its own max"
        );
        assert_eq!(
            read("models@default.ini"),
            Some(pai_core::assign::render(
                &flagged(&["a", "b", "c"]),
                "",
                pai_core::ini::text::section_text(a_split, "default").as_deref(),
            )),
            "default's render holds the one preset it is told"
        );
        assert_eq!(
            read("models@lan-2.ini"),
            Some(pai_core::assign::render(
                &flagged(&["a", "b", "c"]),
                "",
                pai_core::ini::text::section_text(a_split, "lan-2").as_deref(),
            )),
            "lan-2's render holds every preset of the pool"
        );
        assert_eq!(
            listing(),
            [
                "assignments.ini",
                "instances.ini",
                "models.ini",
                "models@default.ini",
                "models@lan-2.ini",
                "server@default.ini",
                "server@lan-2.ini"
            ],
            "one render per instance, beside the pool"
        );
    }

    // ── A broken section already on disk blocks no other write ───────────
    let broken = real_model_files(
        dir.path(),
        "[old]\nmodel = /models/old.gguf\nctx-size = abc\n",
    );
    {
        let m = manager(true);
        std::fs::write(sys.join("models.ini"), &broken).expect("hand-edited fixture");
        let added = real_model_files(
            dir.path(),
            &format!("{broken}[new]\nmodel = /models/new.gguf\n"),
        );
        assert_eq!(m.write_file(&alice, "models.ini", &added, false), Ok(()));
        assert_eq!(read("models.ini"), Some(added.clone()));
        let edited = real_model_files(
            dir.path(),
            &format!("{broken}[new]\nmodel = /models/new.gguf\nparallel = 0\n"),
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &edited, false),
            invalid(
                "not-whole-number-in-range",
                &["parallel", "0", "1", "2147483647"]
            )
        );
        assert_eq!(read("models.ini"), Some(added));
    }

    // ── models.ini.disabled: switching off is never refused ─────────────
    {
        let m = manager(true);
        // The broken model profile moves out of models.ini unchanged: accepted.
        assert_eq!(
            m.write_file(&alice, "models.ini.disabled", &broken, false),
            Ok(())
        );
        assert_eq!(read("models.ini.disabled"), Some(broken.clone()));
        // A disabled model profile edited now gets the model profile rules.
        let edited = real_model_files(
            dir.path(),
            &format!("{broken}[parked]\nmodel = /models/p.gguf\nfit-target = 2048MiB\n"),
        );
        assert_eq!(
            m.write_file(&alice, "models.ini.disabled", &edited, false),
            invalid("fit-target-format", &["2048MiB", "MiB"])
        );
        assert_eq!(read("models.ini.disabled"), Some(broken.clone()));
        // No startup count: llama-server never reads this file.
        let many = real_model_files(
            dir.path(),
            &format!("{broken}{}", flagged(&["a", "b", "c", "d", "e"])),
        );
        assert_eq!(
            m.write_file(&alice, "models.ini.disabled", &many, false),
            Ok(())
        );
        // Switching the broken one back on: new in models.ini, so checked.
        let enabled = real_model_files(dir.path(), "[new]\nmodel = /models/new.gguf\n");
        std::fs::write(sys.join("models.ini"), &enabled).expect("models.ini fixture");
        let on = real_model_files(dir.path(), &format!("{enabled}{broken}"));
        assert_eq!(
            m.write_file(&alice, "models.ini", &on, false),
            invalid(
                "not-whole-number-in-range",
                &["ctx-size", "abc", "0", "2147483647"]
            )
        );
    }

    // ── A model profile switched on with an inline comment: the disabled file is
    //    the other file check_contents hands to models_file ──────────────
    {
        let m = manager(true);
        let kept = real_model_files(dir.path(), "[kept]\nmodel = /models/kept.gguf\n");
        let parked = real_model_files(dir.path(), "[commented]\nmodel = /models/c.gguf ; Q8\n");
        std::fs::write(sys.join("models.ini"), &kept).expect("models.ini fixture");
        std::fs::write(sys.join("models.ini.disabled"), &parked).expect("disabled fixture");
        // The switch moves the section whole: new to models.ini, held
        // identically by the disabled file, so the commented line is known.
        let on = real_model_files(dir.path(), &format!("{kept}{parked}"));
        assert_eq!(m.write_file(&alice, "models.ini", &on, false), Ok(()));
        assert_eq!(
            read("models.ini").as_deref(),
            Some(on.as_str()),
            "the switch wrote through"
        );
    }

    // ── ctx-size = 0 (the context from the model) is writable ────────────
    {
        let m = manager(true);
        let zero = real_model_files(
            dir.path(),
            "[new]\nmodel = /models/new.gguf\nctx-size = 0\n",
        );
        assert_eq!(m.write_file(&alice, "models.ini", &zero, false), Ok(()));
        assert_eq!(read("models.ini"), Some(zero.clone()));
    }

    // ── One id across models.ini and models.ini.disabled ───────────────
    {
        let m = manager(true);
        let enabled = real_model_files(dir.path(), "[new]\nmodel = /models/new.gguf\n");
        let parked = real_model_files(
            dir.path(),
            "[Parked]\nmodel = /models/parked.gguf\nctx-size = 8192\n",
        );
        std::fs::write(sys.join("models.ini"), &enabled).expect("models.ini fixture");
        std::fs::write(sys.join("models.ini.disabled"), &parked).expect("disabled fixture");
        // A new enabled model profile named like a parked one, in another case.
        let namesake = real_model_files(
            dir.path(),
            &format!("{enabled}[parked]\nmodel = /models/other.gguf\n"),
        );
        assert_eq!(
            m.write_file(&alice, "models.ini", &namesake, false),
            invalid("model-profile-name-taken", &["parked"])
        );
        assert_eq!(read("models.ini"), Some(enabled.clone()), "old kept");
        // A new parked model profile named like an enabled one.
        let namesake = real_model_files(
            dir.path(),
            &format!("{parked}[NEW]\nmodel = /models/other.gguf\n"),
        );
        assert_eq!(
            m.write_file(&alice, "models.ini.disabled", &namesake, false),
            invalid("model-profile-name-taken", &["NEW"])
        );
        assert_eq!(read("models.ini.disabled"), Some(parked.clone()));
        // Switching the parked one on writes models.ini first: a move.
        let on = real_model_files(dir.path(), &format!("{enabled}{parked}"));
        assert_eq!(m.write_file(&alice, "models.ini", &on, false), Ok(()));
        assert_eq!(read("models.ini"), Some(on));
        assert_eq!(
            m.write_file(&alice, "models.ini.disabled", "", false),
            Ok(())
        );
    }

    // ── Floats, enums, JSON and min <= max: refused, the file kept ───────
    {
        let m = manager(true);
        let kept = real_model_files(dir.path(), "[kept]\nmodel = /models/kept.gguf\n");
        std::fs::write(sys.join("models.ini"), &kept).expect("models.ini fixture");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("disabled fixture");
        let with = |lines: &str| {
            real_model_files(
                dir.path(),
                &format!("{kept}[new]\nmodel = /models/new.gguf\n{lines}"),
            )
        };
        assert_eq!(
            m.write_file(&alice, "models.ini", &with("temp = NaN\n"), false),
            invalid("not-a-number", &["temp", "NaN"])
        );
        assert_eq!(
            m.write_file(
                &alice,
                "models.ini",
                &with("chat-template-kwargs = [1]\n"),
                false
            ),
            invalid("not-json-object", &["chat-template-kwargs"])
        );
        assert_eq!(
            m.write_file(
                &alice,
                "models.ini",
                &with("spec-draft-n-min = 5\nspec-draft-n-max = 4\n"),
                false
            ),
            invalid(
                "min-above-max",
                &["spec-draft-n-min", "5", "spec-draft-n-max", "4"]
            )
        );
        let enum_refused =
            m.write_file(&alice, "models.ini", &with("split-mode = banana\n"), false);
        assert!(
            matches!(&enum_refused, Err(ManagerError::Invalid(r)) if r.key == "not-one-of"),
            "{enum_refused:?}"
        );
        assert_eq!(
            m.write_file(&alice, "models.ini.disabled", &with("top-p = inf\n"), false),
            invalid("not-a-number", &["top-p", "inf"])
        );
        assert_eq!(
            read("models.ini").as_deref(),
            Some(kept.as_str()),
            "old kept"
        );
        assert_eq!(read("models.ini.disabled").as_deref(), Some(""), "old kept");
    }
}

/// The model profiles render of every instance: `assignments.ini` is read through
/// the file interface and never written by the client; the creation of an
/// instance and every write of a pool file write `models@<id>.ini`, which
/// holds the pool sections that instance's section of the assignments
/// selects, plus the global `[*]` section for every instance.
#[cfg(test)]
pub(crate) fn manager_assignments_e2e() {
    use fakes::{FakeAuthority, FakeResolver, FakeService};
    use pai_core::assign::render;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let alice = Caller(":1.42".into());
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(crate::manager::service::fakes::FakeGroup::new());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    // The two sections really differ, and no `*` sits on `[default]`: it
    // would assign every model profile of the pool to default, leaving nothing to
    // tell the two renders apart. default is told `a`, lan-2 is told `b`.
    let assignments = "[default]\na = inherit\n\n[lan-2]\nb = inherit\n";
    let section = |id: &str| -> String {
        pai_core::ini::text::section_text(assignments, id).unwrap_or_default()
    };
    let default_ini = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 2\n";
    // The global `[*]` section reaches the pool only as a hand edit: a
    // client write of it is refused, `*` is not a model profile id.
    let global = "[*]\ntemp = 0.7\n\n";
    let pool = real_model_files(
        dir.path(),
        "[a]\nmodel = /models/a.gguf\nload-on-startup = true\n\n[b]\nmodel = /models/b.gguf\n",
    );
    let off = real_model_files(dir.path(), "[z]\nmodel = /models/z.gguf\n");

    // ── assignments.ini: the client reads it, and never writes it ────────
    assert_eq!(
        m.read_file(Some(&alice), "assignments.ini"),
        Ok(None),
        "no assignments.ini is nothing to read"
    );
    std::fs::write(sys.join("assignments.ini"), assignments).expect("assignments.ini");
    assert_eq!(
        m.read_file(Some(&alice), "assignments.ini"),
        Ok(Some(plain(assignments))),
        "the assignments are read as they stand"
    );
    assert_eq!(
        m.write_file(&alice, "assignments.ini", "[default]\n* = off\n", false),
        Err(ManagerError::BadFileName),
        "the client does not write the assignments"
    );
    assert_eq!(
        m.write_file_if(&alice, "assignments.ini", "[default]\n* = off\n", ""),
        Err(ManagerError::BadFileName),
        "not even a compare-and-swap"
    );
    assert_eq!(
        read("assignments.ini").as_deref(),
        Some(assignments),
        "a refused write leaves the assignments as they were"
    );

    // ── The render of an instance is its section of the assignments ──────
    // The pool stands first, so the renders of the created instances hold
    // model profiles: default is told `a`, lan-2 is told `b`.
    write_record(&sys, &["default", "lan-2", "lan-3"]);
    assert_eq!(m.write_file(&alice, "models.ini", &pool, false), Ok(()));
    assert_eq!(
        m.write_file(&alice, "server@default.ini", default_ini, false),
        Ok(())
    );
    assert_eq!(
        read("models@default.ini"),
        Some(render(&pool, "", Some(section("default").as_str()))),
        "default's render on the instance's creation"
    );
    assert_eq!(
        m.write_file(
            &alice,
            "server@lan-2.ini",
            "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 3\n",
            false
        ),
        Ok(())
    );
    let lan2 = read("models@lan-2.ini").expect("lan-2's render");
    assert_eq!(
        lan2,
        render(&pool, "", Some(section("lan-2").as_str())),
        "lan-2's render is its own section"
    );
    assert!(
        lan2.contains("[b]") && !read("models@default.ini").unwrap().contains("[b]"),
        "the two renders differ: b is lan-2's alone"
    );
    assert!(
        read("models@default.ini").unwrap().contains("[a]") && !lan2.contains("[a]"),
        "and a is default's alone"
    );
    // An instance with no section gets the global section alone; the pool
    // has none until the hand edit below, so this render carries no model profile.
    assert_eq!(
        m.write_file(
            &alice,
            "server@lan-3.ini",
            "[Server]\nPort = 8083\nHostname = 127.0.0.1\n",
            false
        ),
        Ok(())
    );
    assert_eq!(
        read("models@lan-3.ini"),
        Some(render(&pool, "", None)),
        "no section: no preset of the pool"
    );
    assert_eq!(
        listing(),
        [
            "assignments.ini",
            "instances.ini",
            "models.ini",
            "models@default.ini",
            "models@lan-2.ini",
            "models@lan-3.ini",
            "server@default.ini",
            "server@lan-2.ini",
            "server@lan-3.ini",
        ],
        "one render per instance, and nothing else"
    );

    // ── Every write of a pool file re-renders every instance ─────────────
    // A client write of the global `[*]` section is refused: `*` is not a
    // model profile id. It arrives as a hand edit, and the next render takes it
    // from the file on disk.
    let on_disk = format!("{global}{pool}");
    assert!(
        matches!(
            m.write_file(&alice, "models.ini", &on_disk, false),
            Err(ManagerError::Invalid(r)) if r.key == "invalid-model-profile-id" && r.values == ["*"]
        ),
        "the client cannot write the global section"
    );
    assert_eq!(
        read("models.ini").as_deref(),
        Some(pool.as_str()),
        "the refused write changed nothing"
    );
    std::fs::write(sys.join("models.ini"), &on_disk).expect("hand-edited pool");
    assert_eq!(
        m.write_file(&alice, "models.ini.disabled", &off, false),
        Ok(())
    );
    assert_eq!(
        read("models@default.ini"),
        Some(render(&on_disk, &off, Some(section("default").as_str()))),
        "the render takes the global section from the pool file on disk"
    );
    assert_eq!(
        read("models@lan-3.ini"),
        Some(render(&on_disk, &off, None)),
        "the global section reaches every instance: lan-3 is the global section alone"
    );
    assert_eq!(m.write_file(&alice, "models.ini", &pool, false), Ok(()));
    assert_eq!(
        read("models@default.ini"),
        Some(render(&pool, &off, Some(section("default").as_str()))),
        "default keeps a alone: the pool's other presets are not its rows"
    );
    assert_eq!(
        read("models@lan-2.ini"),
        Some(render(&pool, &off, Some(section("lan-2").as_str()))),
        "lan-2 keeps b alone"
    );
    assert_eq!(
        read("models@lan-3.ini"),
        Some(render(&pool, &off, None)),
        "the client's write replaced the hand-edited global section"
    );
    let added = real_model_files(
        dir.path(),
        &format!("{pool}\n[c]\nmodel = /models/c.gguf\nload-on-startup = true\n"),
    );
    assert_eq!(m.write_file(&alice, "models.ini", &added, false), Ok(()));
    assert_eq!(
        read("models@default.ini"),
        Some(render(&added, &off, Some(section("default").as_str()))),
        "the second write of the pool re-renders"
    );

    // ── A refused write changes no render ────────────────────────────────
    let broken = real_model_files(
        dir.path(),
        &format!("{added}\n[bad]\nmodel = /models/bad.gguf\nctx-size = abc\n"),
    );
    let answer = m.write_file(&alice, "models.ini", &broken, false);
    assert!(
        matches!(answer, Err(ManagerError::Invalid(_))),
        "a pool with a broken section is refused: {answer:?}"
    );
    assert_eq!(
        read("models@default.ini"),
        Some(render(&added, &off, Some(section("default").as_str()))),
        "a refused write leaves every render as it was"
    );
    assert_eq!(
        listing(),
        [
            "assignments.ini",
            "instances.ini",
            "models.ini",
            "models.ini.disabled",
            "models@default.ini",
            "models@lan-2.ini",
            "models@lan-3.ini",
            "server@default.ini",
            "server@lan-2.ini",
            "server@lan-3.ini",
        ],
        "a render of an id is never a temp file left behind"
    );

    // ── The manager's start writes the render of every instance ──────────
    assert!(
        m.render_failures().is_empty(),
        "the renders are rewritten from what is on disk"
    );
    assert_eq!(
        read("models@default.ini"),
        Some(render(&added, &off, Some(section("default").as_str()))),
        "the start render agrees with the last write"
    );
}

/// E2E phase: `SetModelProfile`, the one verb that writes `assignments.ini`
/// (protocol 3, its `As` form and the same name over the pipe). The key of one
/// model profile goes in the instance's section and every model profile file of
/// every instance renders again from the pool and the assignments on disk:
/// `on`, `off` and `inherit` are the key's three values, `none` removes the
/// key, and `*`, the whole pool, takes `none` and `inherit` only. A call
/// answers before it writes: an id that is no instance (`InvalidId`,
/// `NoInstance`), a model profile the pool does not hold
/// (`no-model-profile-named`, not asked of `*`), a value outside the list, and
/// a render whose `load-on-startup` count is over the `ModelsMax` of the server
/// profile the instance's section names (`load-on-startup-over-max`, naming the
/// instance) each write and render nothing. The resource rule is one-sided:
/// only an assignment in effect on is judged, so `off`, `none` and a `*`
/// narrowed never refuse. Guarded as a write of a served file is: `configure`
/// first, the `As` form answered only for the bff's account and a current
/// member of `pai`, and one record of the action log per call, done or refused.
#[cfg(test)]
pub(crate) fn manager_set_model_profile_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use pai_core::assign::render;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    // The bff's account, its request for alice (a member of `pai` in
    // [`FakePasswords`]) and for bob (who is not), and a desktop caller of its
    // own.
    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let core = |allow: bool, passwords: FakePasswords, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(log)
    };
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let keys_of = |text: &str, id: &str, key: &str| -> Vec<String> {
        section(text, id)
            .lines()
            .filter(|line| line.trim_start().starts_with(&format!("{key} ")))
            .map(|line| line.to_string())
            .collect()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();

    // The pool, and the record and the server profiles the fixture hands the
    // manager: `a` loads on startup, `b` does not, `m` sits in the switched-off
    // file so a pool test sees both halves.
    let pool = real_model_files(
        dir.path(),
        "[a]\nmodel = /models/a.gguf\nload-on-startup = true\n\n[b]\nmodel = /models/b.gguf\n",
    );
    let off_pool = real_model_files(
        dir.path(),
        "[m]\nmodel = /models/m.gguf\nload-on-startup = true\n",
    );
    write_record(&sys, &["default", "lan-2"]);
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
    )
    .expect("server@default.ini");
    std::fs::write(
        sys.join("server@lan-2.ini"),
        "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
    )
    .expect("server@lan-2.ini");
    std::fs::write(sys.join("models.ini"), &pool).expect("models.ini");
    std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("models.ini.disabled");
    // The assignments as an earlier call left them: a comment, a key of its
    // own, a second section. The verb writes one key of one section and
    // touches none of the rest.
    let seeded =
        "# the manager's own record\n[default]\nb = off\nnote = keep me\n\n[lan-2]\n* = inherit\n";

    // ── The protocol these verbs came with stays 3 ────────────────────────
    assert_eq!(
        crate::manager::pipe::PROTOCOL,
        3,
        "the version `Version` reports: 3 came with the first assignment verb, and with `Instances` and `SetInstanceTags` (the protocol stays 3, so they are part of 3)"
    );

    // ── Each value writes its key, and re-renders every instance ─────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        std::fs::write(sys.join("assignments.ini"), seeded).expect("seed");

        // `on`: the key, the pool's section in the render, the rest kept.
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "on"),
            Ok(()),
            "a model profile of the pool, on"
        );
        let written = text_of("assignments.ini");
        assert_eq!(
            keys_of(&written, "default", "a"),
            vec!["a = on".to_string()],
            "one key for the model profile, at the value given"
        );
        assert!(
            keys_of(&written, "lan-2", "a").is_empty(),
            "the key is default's, not lan-2's"
        );
        assert!(
            written.contains("# the manager's own record"),
            "the comment of the file is kept"
        );
        assert!(
            written.contains("note = keep me"),
            "a key that is no model profile is kept"
        );
        assert!(
            written.contains("[lan-2]\n* = inherit\n"),
            "the section of another instance is kept as it was"
        );
        let d = text_of("models@default.ini");
        let l2 = text_of("models@lan-2.ini");
        assert_eq!(
            d,
            render(&pool, &off_pool, Some(&section(&written, "default"))),
            "default's render is the pool cut by its section, the one just written"
        );
        assert!(
            d.contains("[a]") && d.contains("load-on-startup = true"),
            "an `on` model profile is in the render the instance loads, its own keys with it"
        );
        assert_eq!(
            l2,
            render(&pool, &off_pool, Some(&section(&written, "lan-2"))),
            "the write re-renders every instance: lan-2 from its own section"
        );
        assert!(
            l2.contains("[a]"),
            "lan-2 keeps a: `* = inherit` is its own"
        );

        // `off`: the key, the section out of the render.
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "off"),
            Ok(()),
            "the same key, off"
        );
        let written = text_of("assignments.ini");
        assert_eq!(
            keys_of(&written, "default", "a"),
            vec!["a = off".to_string()],
            "the key is replaced, not added to"
        );
        let d = text_of("models@default.ini");
        assert_eq!(
            d,
            render(&pool, &off_pool, Some(&section(&written, "default"))),
            "the render follows the section"
        );
        assert!(
            !d.contains("[a]"),
            "an `off` model profile is out of the render: {d:?}"
        );

        // `inherit`: the key kept, the pool's own state decides.
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "inherit"),
            Ok(()),
            "the same key, inherit"
        );
        let written = text_of("assignments.ini");
        assert_eq!(
            keys_of(&written, "default", "a"),
            vec!["a = inherit".to_string()],
            "`inherit` is a value of the key, not its removal"
        );
        assert!(
            text_of("models@default.ini").contains("[a]"),
            "a model profile of the switched-on pool, inherited, is in the render"
        );

        // A second call with the same value writes one key, not two.
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "inherit"),
            Ok(()),
            "the same value again"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "a"),
            vec!["a = inherit".to_string()],
            "the key stays one"
        );

        // `none`: the key out, everything else as it was.
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "none"),
            Ok(()),
            "`none` is answered"
        );
        let written = text_of("assignments.ini");
        assert_eq!(
            section(&written, "default"),
            "[default]\nb = off\nnote = keep me\n\n",
            "`none` removes the key and leaves the section's other lines"
        );
        assert!(
            !text_of("models@default.ini").contains("[a]"),
            "a model profile with no key and no `*` is not rendered"
        );

        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_model_profile" target="default" caller=":1.42" principal=- outcome=ok"#,
                r#"action="set_model_profile" target="default" caller=":1.42" principal=- outcome=ok"#,
                r#"action="set_model_profile" target="default" caller=":1.42" principal=- outcome=ok"#,
                r#"action="set_model_profile" target="default" caller=":1.42" principal=- outcome=ok"#,
                r#"action="set_model_profile" target="default" caller=":1.42" principal=- outcome=ok"#,
            ],
            "one record per call: the verb's action, the instance as its target, the caller, no principal as a bare -, ok"
        );
    }

    // ── The global `*`: `none` and `inherit` only ─────────────────────────
    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("seed");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "*", "inherit"),
            Ok(()),
            "`*` takes inherit"
        );
        assert_eq!(
            section(&text_of("assignments.ini"), "default"),
            "[default]\n* = inherit\n",
            "the global key, written"
        );
        let d = text_of("models@default.ini");
        assert!(
            d.contains("[a]") && d.contains("[b]") && !d.contains("[m]"),
            "`* = inherit` is every model profile of the pool following its own file: the switched-off one stays out"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "*", "none"),
            Ok(()),
            "`*` takes none"
        );
        assert_eq!(
            section(&text_of("assignments.ini"), "default"),
            "[default]\n",
            "`none` removes the global key too"
        );

        // The pool check is not asked of `*`: with an empty pool the global key
        // is written, while one model profile is refused for want of a pool.
        std::fs::write(sys.join("models.ini"), "").expect("empty pool");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("empty pool");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "*", "inherit"),
            Ok(()),
            "`*` needs no model profile of the pool to exist"
        );
        assert!(
            matches!(
                m.set_model_profile(&desktop, "default", "a", "on"),
                Err(ManagerError::Invalid(r))
                    if r.key == "no-model-profile-named" && r.values == ["a"]
            ),
            "the same model profile, in an empty pool, is refused naming it"
        );
        std::fs::write(sys.join("models.ini"), &pool).expect("the pool back");

        // `on` and `off` under `*`: refused, with nothing written and no
        // render of their own.
        let seed = text_of("assignments.ini");
        let before = listing();
        let renders: Vec<(String, String)> = ["models@default.ini", "models@lan-2.ini"]
            .iter()
            .map(|name| (name.to_string(), text_of(name)))
            .collect();
        for value in ["on", "off"] {
            assert!(
                m.set_model_profile(&desktop, "default", "*", value)
                    .is_err(),
                "`*` refuses {value}"
            );
        }
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "a `*` refused writes nothing"
        );
        assert_eq!(listing(), before, "and creates no file");
        for (name, text) in renders {
            assert_eq!(text_of(&name), text, "and re-renders nothing: {name}");
        }
    }

    // ── A call that answers before it writes ──────────────────────────────
    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        // The fixture is made, not assumed: the block above leaves the
        // switched-off file empty and default's render built from a pool of
        // nothing. One call puts `a` in the record and in the render, the way
        // a real assignment does, so "rendered nothing" below has something to
        // keep.
        std::fs::write(sys.join("models.ini"), &pool).expect("pool back");
        std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("off pool back");
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("seed");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "on"),
            Ok(()),
            "the fixture: a call assigns a to default"
        );
        let seed = text_of("assignments.ini");
        let before = listing();
        let renders: Vec<(String, String)> = ["models@default.ini", "models@lan-2.ini"]
            .iter()
            .map(|name| (name.to_string(), text_of(name)))
            .collect();
        assert!(
            renders[0].1.contains("[a]"),
            "the fixture: a is in default's render before the refusals"
        );

        assert_eq!(
            m.set_model_profile(&desktop, "lan-3", "a", "on"),
            Err(ManagerError::NoInstance),
            "an id that is no section of the record"
        );
        for bad in ["", "../x", "Default", "a_b"] {
            assert_eq!(
                m.set_model_profile(&desktop, bad, "a", "on"),
                Err(ManagerError::InvalidId),
                "not an instance id at all: {bad:?}"
            );
        }
        assert!(
            matches!(
                m.set_model_profile(&desktop, "default", "zzz", "on"),
                Err(ManagerError::Invalid(r))
                    if r.key == "no-model-profile-named" && r.values == ["zzz"]
            ),
            "a model profile the pool does not hold, named"
        );
        for value in ["maybe", "ON", "", "on,off", "none "] {
            assert!(
                matches!(
                    m.set_model_profile(&desktop, "default", "a", value),
                    Err(ManagerError::Invalid(r))
                        if r.key == "model-profile-bad-value" && r.values == [value]
                ),
                "a value outside none, inherit, on, off is refused: {value:?}"
            );
        }
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "every refusal above wrote nothing"
        );
        assert_eq!(listing(), before, "and created no file");
        for (name, text) in renders {
            assert_eq!(text_of(&name), text, "and re-rendered nothing: {name}");
        }
    }

    // ── `ModelsMax` of the server profile the section names ───────────────
    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("ModelsMax 1");
        std::fs::write(sys.join("assignments.ini"), "[default]\n* = inherit\n").expect("seed");
        let seed = text_of("assignments.ini");
        let renders: Vec<(String, String)> = ["models@default.ini", "models@lan-2.ini"]
            .iter()
            .map(|name| (name.to_string(), text_of(name)))
            .collect();
        // `* = inherit` puts the switched-on half in effect, so `a` loads on
        // startup; switching on `m`, whose own section asks for it, is the
        // second one, over the max of one.
        let over = m.set_model_profile(&desktop, "default", "m", "on");
        assert!(
            matches!(
                over,
                Err(ManagerError::Invalid(ref r))
                    if r.key == "load-on-startup-over-max" && r.values == ["2", "1", "default"]
            ),
            "over the max, the instance named last, answered {over:?}"
        );
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "the refusal above max wrote nothing"
        );
        for (name, text) in renders {
            assert_eq!(text_of(&name), text, "and rendered nothing: {name}");
        }

        // The rule is one-sided: what leaves the render is never judged.
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\n* = inherit\nb = off\n",
        )
        .expect("b off");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "off"),
            Ok(()),
            "an assignment off is accepted whatever the max"
        );
        assert_eq!(
            section(&text_of("assignments.ini"), "default"),
            "[default]\n* = inherit\nb = off\na = off\n",
            "and its key landed"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "*", "none"),
            Ok(()),
            "a `*` removed, which leaves the whole pool out of the render, is accepted"
        );
        assert_eq!(
            section(&text_of("assignments.ini"), "default"),
            "[default]\nb = off\na = off\n",
            "the global key is out, the section's other keys kept"
        );
        assert_eq!(
            text_of("models@default.ini"),
            render(&pool, &off_pool, Some("[default]\nb = off\na = off\n")),
            "and the render follows: nothing of the pool is in effect"
        );
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("ModelsMax back");
    }

    // ── `ModelsMax` of the profile the section names, not of a file named as the id
    {
        // An instance whose server profile is not its own name: `lan` runs
        // `small`, and a `server@lan.ini` that no instance runs sits beside it
        // with a max of its own. Both directions are pinned: the max that
        // refuses is the profile's, and the max that lets the write land is
        // the profile's too.
        std::fs::write(
            sys.join(INSTANCES_FILE),
            "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = lan-2\n\n[lan]\nServerProfile = small\n",
        )
        .expect("instances.ini: lan runs the profile small");
        std::fs::write(
            sys.join("server@small.ini"),
            "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("server@small.ini");
        std::fs::write(
            sys.join("server@lan.ini"),
            "[Server]\nPort = 8083\nHostname = 127.0.0.1\nModelsMax = 8\n",
        )
        .expect("server@lan.ini, the file no instance runs");
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        std::fs::write(sys.join("assignments.ini"), "[lan]\n* = inherit\n").expect("seed");
        assert!(
            m.render_failures().is_empty(),
            "the record with a profile that is not the id renders every instance"
        );
        let seed = text_of("assignments.ini");
        let renders: Vec<(String, String)> =
            ["models@default.ini", "models@lan-2.ini", "models@lan.ini"]
                .iter()
                .map(|name| (name.to_string(), text_of(name)))
                .collect();
        // `* = inherit` puts `a` in effect, so switching on `m` is the second
        // load on startup: over `small`'s max of 1, refused, naming `lan` and
        // the max of 1. The 8 of `server@lan.ini` is not the number the check
        // reads.
        let over = m.set_model_profile(&desktop, "lan", "m", "on");
        assert!(
            matches!(
                over,
                Err(ManagerError::Invalid(ref r))
                    if r.key == "load-on-startup-over-max" && r.values == ["2", "1", "lan"]
            ),
            "the max of the profile the section names, answered {over:?}"
        );
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "the refusal wrote nothing to the record"
        );
        for (name, text) in &renders {
            assert_eq!(text_of(name), *text, "and rendered nothing: {name}");
        }

        // The reverse: `small` raised to 8 while the file named after the id
        // drops to 1. The same call lands, so the check cannot be reading the
        // id's file either.
        std::fs::write(
            sys.join("server@small.ini"),
            "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 8\n",
        )
        .expect("server@small.ini raised");
        std::fs::write(
            sys.join("server@lan.ini"),
            "[Server]\nPort = 8083\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("server@lan.ini lowered");
        assert_eq!(
            m.set_model_profile(&desktop, "lan", "m", "on"),
            Ok(()),
            "the profile's own max of 8 lets the second load on startup land"
        );
        assert_eq!(
            section(&text_of("assignments.ini"), "lan"),
            "[lan]\n* = inherit\nm = on\n",
            "the key landed in the section of the instance it was asked for"
        );
        assert_eq!(
            text_of("models@lan.ini"),
            render(&pool, &off_pool, Some("[lan]\n* = inherit\nm = on\n")),
            "and the render of the instance follows the record"
        );

        // The fixture's record and files back for the phases that follow.
        std::fs::remove_file(sys.join("server@small.ini")).expect("small out");
        std::fs::remove_file(sys.join("server@lan.ini")).expect("lan profile out");
        std::fs::remove_file(sys.join("models@lan.ini")).expect("lan render out");
        write_record(&sys, &["default", "lan-2"]);
    }

    // ── Guarded: `configure`, the principal, one record ──────────────────
    {
        // A caller without the right: NotAuthorized, nothing written.
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, FakePasswords::new(), log);
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("seed");
        let seed = text_of("assignments.ini");
        let before = listing();
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "on"),
            Err(ManagerError::NotAuthorized),
            "a caller outside `pai`, polkit refusing"
        );
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "a refused caller writes nothing"
        );
        assert_eq!(listing(), before, "and leaves no render of its own behind");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_model_profile" target="default" caller=":1.42" principal=- outcome=refused"#.to_string()
            ],
            "the refusal is recorded as one record too"
        );
    }
    {
        // The bff acting for a member: the pair grants the call, the record
        // names both.
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, p, log);
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("seed");
        assert_eq!(
            m.set_model_profile(&member, "default", "a", "on"),
            Ok(()),
            "the bff assigns for a member of `pai`"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "a"),
            vec!["a = on".to_string()],
            "for the member"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["alice".to_string()],
            "the membership of the principal, never of the caller"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            vec![bff.clone()],
            "the caller is checked first, at every call"
        );
        assert_eq!(
            m.authority().asked.lock().unwrap().len(),
            0,
            "polkit is never asked for a principal: its subject is the bff"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_model_profile" target="default" caller=":1.9" principal="alice" outcome=ok"#.to_string()
            ],
            "the record carries the caller and the principal"
        );

        // A principal outside `pai`: the call is refused and nothing is written.
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("seed");
        let seed = text_of("assignments.ini");
        assert_eq!(
            m.set_model_profile(&outsider, "default", "a", "on"),
            Err(ManagerError::NotAuthorized),
            "a right password of a caller outside `pai` is not enough"
        );
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "a refused principal writes nothing"
        );

        // A caller that is not the bff cannot name a principal at all.
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("seed");
        let seed = text_of("assignments.ini");
        let desktop_member = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.set_model_profile(&desktop_member, "default", "a", "on"),
            Err(ManagerError::NotAuthorized),
            "only the bff answers for a signed-in member"
        );
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "and a caller outside the bff writes nothing"
        );
    }

    // ── A file that cannot be read: the read's failure, and nothing written ─
    {
        let bytes_of = |name: &str| std::fs::read(sys.join(name)).expect("bytes");
        let kept_renders = || -> Vec<(String, String)> {
            ["models@default.ini", "models@lan-2.ini"]
                .iter()
                .map(|name| (name.to_string(), text_of(name)))
                .collect()
        };
        let restore_pool = || {
            std::fs::write(sys.join("models.ini"), &pool).expect("pool");
            std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("off pool");
        };

        // `assignments.ini` with bytes that are no text. A read taken as empty
        // here renders a record of one key and replaces the file with it: every
        // instance's assignments, gone.
        {
            let m = core(true, FakePasswords::new(), FakeActionLog::new());
            std::fs::write(
                sys.join("assignments.ini"),
                b"[default]\nb = off\nnote = keep me\n\xed\xa0\xbf\n",
            )
            .expect("broken record");
            let seed = bytes_of("assignments.ini");
            let before = listing();
            let renders = kept_renders();
            assert_eq!(
                m.set_model_profile(&desktop, "default", "a", "on"),
                Err(ManagerError::Io(std::io::ErrorKind::InvalidData)),
                "an assignments.ini that cannot be read answers the read's own kind, it is not an empty file"
            );
            assert_eq!(
                bytes_of("assignments.ini"),
                seed,
                "and its bytes are the bytes that were there: the atomic write never replaced them"
            );
            assert_eq!(
                listing(),
                before,
                "and the call left no file of its own behind: no temp, no render"
            );
            for (name, text) in renders {
                assert_eq!(text_of(&name), text, "and re-rendered nothing: {name}");
            }
        }

        // A directory at the record's path: the same answer, and the path
        // stays what it is.
        {
            let m = core(true, FakePasswords::new(), FakeActionLog::new());
            std::fs::remove_file(sys.join("assignments.ini")).expect("record out");
            std::fs::create_dir(sys.join("assignments.ini")).expect("record dir");
            let before = listing();
            let renders = kept_renders();
            assert_eq!(
                m.set_model_profile(&desktop, "default", "a", "on"),
                Err(ManagerError::Io(std::io::ErrorKind::IsADirectory)),
                "a directory at the record's path answers the read's own kind"
            );
            assert!(
                sys.join("assignments.ini").is_dir(),
                "the directory is still there: nothing was renamed over it"
            );
            assert_eq!(
                listing(),
                before,
                "and the call left no temp file of its own next to it"
            );
            for (name, text) in renders {
                assert_eq!(text_of(&name), text, "and re-rendered nothing: {name}");
            }
            std::fs::remove_dir(sys.join("assignments.ini")).expect("record dir out");
        }

        // A model profile file that cannot be read: the same. A pool read as
        // empty refuses a model profile of the real pool as
        // `no-model-profile-named`, and a `*` call renders every instance from
        // a pool of nothing.
        for (name, profile) in [("models.ini", "a"), ("models.ini.disabled", "m")] {
            let m = core(true, FakePasswords::new(), FakeActionLog::new());
            restore_pool();
            std::fs::write(sys.join("assignments.ini"), "[default]\n* = inherit\n").expect("seed");
            std::fs::write(sys.join(name), b"[a]\nmodel = \xff\n").expect("broken pool");
            let seed = bytes_of("assignments.ini");
            let before = listing();
            let renders = kept_renders();
            let answer = m.set_model_profile(&desktop, "default", profile, "on");
            assert_eq!(
                answer,
                Err(ManagerError::Io(std::io::ErrorKind::InvalidData)),
                "a {name} that cannot be read answers the read's own kind, for model profile {profile}"
            );
            assert!(
                !matches!(
                    &answer,
                    Err(ManagerError::Invalid(r)) if r.key == "no-model-profile-named"
                ),
                "an unreadable {name} is never the pool answering `no-model-profile-named`"
            );
            assert_eq!(
                bytes_of("assignments.ini"),
                seed,
                "and the unreadable {name} wrote nothing to the record"
            );
            assert_eq!(listing(), before, "and created no file: {name}");
            for (file, text) in renders {
                assert_eq!(text_of(&file), text, "and rendered nothing: {name}, {file}");
            }
        }

        // A record that is really absent is an empty one: the first assignment
        // of a fresh machine lands. The line endings of a new file are the INI
        // layer's own, so the section is compared with the endings folded.
        {
            let m = core(true, FakePasswords::new(), FakeActionLog::new());
            restore_pool();
            std::fs::remove_file(sys.join("assignments.ini")).expect("record out");
            assert_eq!(
                m.set_model_profile(&desktop, "default", "a", "on"),
                Ok(()),
                "a first assignment with no assignments.ini at all is answered"
            );
            let written = text_of("assignments.ini");
            assert_eq!(
                section(&written.replace("\r\n", "\n"), "default"),
                "[default]\na = on\n",
                "the key lands in the section of the instance it was asked for: {written:?}"
            );
            assert!(
                text_of("models@default.ini").contains("[a]"),
                "and the render follows the record that just appeared"
            );
        }
    }

    // ── Slice 7 review: the resource rule is a step rule, the id is exact,
    //    the record keeps the bytes it was not asked to write, the text ────

    {
        // The instance starts OVER its max: three model profiles that load on
        // startup are in effect, and the server profile its section names caps
        // them at one. The resource rule reads the step a call takes, so a call
        // that lowers the count lands even when the render it produces is
        // still over the max.
        let three = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                     [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                     [c]\nmodel = /m/c.gguf\nload-on-startup = true\n\n\
                     [d]\nmodel = /m/d.gguf\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("max 1");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &three).expect("three loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\n",
        )
        .expect("three on");
        assert!(
            m.render_failures().is_empty(),
            "the fixture renders every instance"
        );
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            3,
            "the fixture starts over the max of one"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "off"),
            Ok(()),
            "a step down lands: three loads to two, and two is still over one"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "a"),
            vec!["a = off".to_string()],
            "and its key landed"
        );
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            2,
            "the render followed the record"
        );
    }

    {
        // `none` removes a key, so it lowers the count too: it lands over the
        // max, exactly as `off` does.
        let three = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                     [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                     [c]\nmodel = /m/c.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("max 1");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &three).expect("three loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\n",
        )
        .expect("three on");
        assert!(
            m.render_failures().is_empty(),
            "the fixture renders every instance"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "b", "none"),
            Ok(()),
            "a key removed lands over the max: three loads to two, over one"
        );
        assert!(
            keys_of(&text_of("assignments.ini"), "default", "b").is_empty(),
            "the key is out of the record"
        );
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            2,
            "and out of the render"
        );
    }

    {
        // `*` narrowed from `inherit` to `none` leaves the whole pool out of
        // the render: it lands whatever the max.
        let three = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                     [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                     [c]\nmodel = /m/c.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("max 1");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &three).expect("three loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(sys.join("assignments.ini"), "[default]\n* = inherit\n").expect("inherit");
        assert!(
            m.render_failures().is_empty(),
            "the fixture renders every instance"
        );
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            3,
            "`* = inherit` puts all three in effect, over the max of one"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "*", "none"),
            Ok(()),
            "the `*` narrowed lands over the max"
        );
        assert!(
            keys_of(&text_of("assignments.ini"), "default", "*").is_empty(),
            "the global key is out"
        );
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            0,
            "and nothing of the pool is in effect"
        );
    }

    {
        // Assigning `on` a model profile that does NOT load on startup leaves
        // the count where it is, over the max: it lands, the count is what the
        // rule reads, not the key.
        let three = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                     [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                     [c]\nmodel = /m/c.gguf\nload-on-startup = true\n\n\
                     [d]\nmodel = /m/d.gguf\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("max 1");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &three).expect("three loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\n",
        )
        .expect("three on");
        assert!(
            m.render_failures().is_empty(),
            "the fixture renders every instance"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "d", "on"),
            Ok(()),
            "`on` of a profile that never loads on startup does not raise the count"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "d"),
            vec!["d = on".to_string()],
            "and its key landed"
        );
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            3,
            "the count is where it was, still over the max of one"
        );
    }

    {
        // The other side of the same fixture: `on` of a model profile that DOES
        // load on startup raises the count and is refused, writing nothing.
        let four = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                    [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                    [c]\nmodel = /m/c.gguf\nload-on-startup = true\n\n\
                    [e]\nmodel = /m/e.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
        )
        .expect("max 1");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &four).expect("four loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\n",
        )
        .expect("three on");
        assert!(
            m.render_failures().is_empty(),
            "the fixture renders every instance"
        );
        let seed = text_of("assignments.ini");
        let renders: Vec<(String, String)> = ["models@default.ini", "models@lan-2.ini"]
            .iter()
            .map(|name| (name.to_string(), text_of(name)))
            .collect();
        let over = m.set_model_profile(&desktop, "default", "e", "on");
        assert!(
            matches!(
                over,
                Err(ManagerError::Invalid(ref r))
                    if r.key == "load-on-startup-over-max" && r.values == ["4", "1", "default"]
            ),
            "the step that raises the count is refused, answered {over:?}"
        );
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "the refusal wrote nothing to the record"
        );
        for (name, text) in renders {
            assert_eq!(text_of(&name), text, "and rendered nothing: {name}");
        }
    }

    {
        // An instance whose section names no server profile has no file to read
        // a `ModelsMax` from, and the launch refuses it anyway: its render is
        // not judged on the llama.cpp default cap. Five loads land.
        let five = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                    [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                    [c]\nmodel = /m/c.gguf\nload-on-startup = true\n\n\
                    [d]\nmodel = /m/d.gguf\nload-on-startup = true\n\n\
                    [e]\nmodel = /m/e.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["lan-2"]);
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join(INSTANCES_FILE), "[default]\n").expect("no profile named");
        std::fs::write(sys.join("models.ini"), &five).expect("five loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\nd = on\n",
        )
        .expect("four on");
        assert!(
            m.render_failures().is_empty(),
            "a section naming no profile still renders"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "e", "on"),
            Ok(()),
            "the fifth load on startup lands: no profile read, no cap judged"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "e"),
            vec!["e = on".to_string()],
            "and its key landed"
        );
        assert_eq!(
            pai_core::checks::startup_loads(&text_of("models@default.ini")),
            5,
            "the render holds five, over the llama.cpp default of four"
        );
    }

    {
        // The section names a profile whose file is missing: nothing to read a
        // cap from, so the render is not judged. Five loads land.
        let five = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                    [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                    [c]\nmodel = /m/c.gguf\nload-on-startup = true\n\n\
                    [d]\nmodel = /m/d.gguf\nload-on-startup = true\n\n\
                    [e]\nmodel = /m/e.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["lan-2"]);
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(
            sys.join(INSTANCES_FILE),
            "[default]\nServerProfile = gone\n",
        )
        .expect("profile named");
        std::fs::remove_file(sys.join("server@gone.ini")).ok();
        std::fs::write(sys.join("models.ini"), &five).expect("five loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\nd = on\n",
        )
        .expect("four on");
        assert!(
            m.render_failures().is_empty(),
            "a section naming a missing profile still renders"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "e", "on"),
            Ok(()),
            "the fifth load on startup lands: the profile the section names is not there"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "e"),
            vec!["e = on".to_string()],
            "and its key landed"
        );
    }

    {
        // The section names a profile whose file cannot be read (no text): the
        // same, an unreadable file is not an empty one and holds no cap.
        let five = real_model_files(
            dir.path(),
            "[a]\nmodel = /m/a.gguf\nload-on-startup = true\n\n\
                    [b]\nmodel = /m/b.gguf\nload-on-startup = true\n\n\
                    [c]\nmodel = /m/c.gguf\nload-on-startup = true\n\n\
                    [d]\nmodel = /m/d.gguf\nload-on-startup = true\n\n\
                    [e]\nmodel = /m/e.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["lan-2"]);
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(
            sys.join(INSTANCES_FILE),
            "[default]\nServerProfile = broken\n",
        )
        .expect("profile named");
        std::fs::write(
            sys.join("server@broken.ini"),
            b"[Server]\nPort = 8090\n\xff\xfe\n",
        )
        .expect("unreadable profile");
        std::fs::write(sys.join("models.ini"), &five).expect("five loads");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(
            sys.join("assignments.ini"),
            "[default]\na = on\nb = on\nc = on\nd = on\n",
        )
        .expect("four on");
        assert!(
            m.render_failures().is_empty(),
            "a section naming an unreadable profile still renders"
        );
        assert_eq!(
            m.set_model_profile(&desktop, "default", "e", "on"),
            Ok(()),
            "the fifth load on startup lands: the profile the section names is unreadable"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "e"),
            vec!["e = on".to_string()],
            "and its key landed"
        );
    }

    {
        // The model profile's name is exact: llama.cpp knows a pool section in
        // its own spelling, so the pool's `[Qwen]` answers no call for `qwen`,
        // and the call leaves the record, and the renders, as they were.
        let qwen = real_model_files(
            dir.path(),
            "[Qwen]\nmodel = /m/qwen.gguf\nload-on-startup = true\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("default profile");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &qwen).expect("pool with Qwen");
        std::fs::write(sys.join("models.ini.disabled"), "").expect("no off file");
        std::fs::write(sys.join("assignments.ini"), "[default]\nQwen = off\n").expect("seed");
        assert!(
            m.render_failures().is_empty(),
            "the fixture renders every instance"
        );
        let seed = text_of("assignments.ini");
        let renders: Vec<(String, String)> = ["models@default.ini", "models@lan-2.ini"]
            .iter()
            .map(|name| (name.to_string(), text_of(name)))
            .collect();
        let answer = m.set_model_profile(&desktop, "default", "qwen", "on");
        assert!(
            matches!(
                answer,
                Err(ManagerError::Invalid(ref r))
                    if r.key == "no-model-profile-named" && r.values == ["qwen"]
            ),
            "the id is looked up exactly as it was spelled, answered {answer:?}"
        );
        assert_eq!(
            keys_of(&text_of("assignments.ini"), "default", "Qwen"),
            vec!["Qwen = off".to_string()],
            "the line in the pool's own spelling is untouched"
        );
        assert_eq!(
            text_of("assignments.ini"),
            seed,
            "and the call for the name the pool does not hold wrote nothing"
        );
        for (name, text) in renders {
            assert_eq!(text_of(&name), text, "and rendered nothing: {name}");
        }
    }

    {
        // The verb writes one key of one section: the blank line that separates
        // that section from the next one is not its to write.
        let seeded = "[default]\nb = off\n\n[lan-2]\n* = inherit\n";
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("default profile");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &pool).expect("pool");
        std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("off pool");
        std::fs::write(sys.join("assignments.ini"), seeded).expect("seed");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "on"),
            Ok(()),
            "a lands in default's section"
        );
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\nb = off\na = on\n\n[lan-2]\n* = inherit\n",
            "the blank line between the sections stayed"
        );
    }

    {
        // A CRLF record stays CRLF on every line the verb writes.
        let seeded = "[default]\r\nb = off\r\n\r\n[lan-2]\r\n* = inherit\r\n";
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("default profile");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &pool).expect("pool");
        std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("off pool");
        std::fs::write(sys.join("assignments.ini"), seeded).expect("CRLF seed");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "a", "on"),
            Ok(()),
            "a lands in the CRLF record"
        );
        let written = text_of("assignments.ini");
        assert!(
            written.contains("a = on\r\n"),
            "the key line the verb wrote is CRLF like the file: {written:?}"
        );
        let bytes = written.as_bytes();
        assert!(
            bytes
                .iter()
                .enumerate()
                .all(|(i, b)| *b != b'\n' || i > 0 && bytes[i - 1] == b'\r'),
            "no line of the record the verb wrote ends with a bare LF: {written:?}"
        );
        assert!(
            written.contains("[lan-2]\r\n* = inherit\r\n"),
            "the other section keeps its CRLF bytes: {written:?}"
        );
    }

    {
        // A record whose last section has no trailing newline keeps it that
        // way: the verb adds the key, not a terminator the file did not have.
        let seeded = "[default]\r\n* = inherit\r\n\r\n[lan-2]\r\nb = off";
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("default profile");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &pool).expect("pool");
        std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("off pool");
        std::fs::write(sys.join("assignments.ini"), seeded).expect("no final newline");
        assert_eq!(
            m.set_model_profile(&desktop, "lan-2", "a", "on"),
            Ok(()),
            "a lands in lan-2's section, the last one"
        );
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\r\n* = inherit\r\n\r\n[lan-2]\r\nb = off\r\na = on",
            "the file still ends on an unterminated line, and keeps the blank line"
        );
    }

    {
        // An instance the record has no section for: the verb adds one, and the
        // sections it did have keep their bytes.
        let seeded = "[default]\n* = inherit\n";
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("default profile");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile");
        std::fs::write(sys.join("models.ini"), &pool).expect("pool");
        std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("off pool");
        std::fs::write(sys.join("assignments.ini"), seeded).expect("seed");
        assert_eq!(
            m.set_model_profile(&desktop, "lan-2", "a", "on"),
            Ok(()),
            "lan-2 has no section of its own yet"
        );
        let written = text_of("assignments.ini");
        assert_eq!(
            section(&written, "lan-2"),
            "[lan-2]\na = on\n",
            "the section the record did not have is added: {written:?}"
        );
        assert_eq!(
            section(&written, "default"),
            "[default]\n* = inherit\n",
            "and default's section is the bytes it was: {written:?}"
        );
        assert!(
            text_of("models@lan-2.ini").contains("[a]"),
            "and the render of the instance follows the section that just appeared"
        );
    }

    {
        // The text a user reads for a value outside the list, byte for byte, in
        // both strings files: the CLI's and the GUI's.
        const BAD_VALUE: &str = "{0} is not a value here: a model profile takes none, inherit, on or off, and * takes none or inherit";
        for (file, source) in [
            ("cli.toml", include_str!("../../../core/strings/cli.toml")),
            (
                "en_US.toml",
                include_str!("../../../core/strings/en_US.toml"),
            ),
        ] {
            let all = crate::strings::parse(source);
            assert_eq!(
                all["model-profile-bad-value"], BAD_VALUE,
                "{file}: the text the refusal shows"
            );
        }
    }

    #[cfg(target_os = "linux")]
    {
        // The mode of the record the verb writes: the manager's config files
        // are read by every client, so a write lands them 0644 whatever the
        // process umask was, even over a file that was 0600. The umask is
        // process-wide, so it is masked only around the call and restored
        // before the first assert.
        use std::os::unix::fs::PermissionsExt;
        write_record(&sys, &["default", "lan-2"]);
        std::fs::write(
            sys.join("server@default.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("server@default.ini");
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("server@lan-2.ini");
        std::fs::write(sys.join("models.ini"), &pool).expect("models.ini");
        std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("models.ini.disabled");
        std::fs::write(sys.join("assignments.ini"), seeded).expect("seed");
        std::fs::set_permissions(
            sys.join("assignments.ini"),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("0600 assignments");
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let before = listing();
        let guard = UmaskGuard::mask(nix::sys::stat::Mode::from_bits_retain(0o077));
        let answer = m.set_model_profile(&desktop, "default", "a", "on");
        let old_mask = guard.0;
        drop(guard);
        assert_eq!(
            nix::sys::stat::umask(old_mask),
            old_mask,
            "the window restored the umask"
        );
        let mode = std::fs::metadata(sys.join("assignments.ini"))
            .ok()
            .map(|f| f.permissions().mode());
        let after = listing();
        assert_eq!(
            answer,
            Ok(()),
            "the fixture: a model profile of the pool, on, lands"
        );
        assert_eq!(
            mode.map(|f| f & 0o777),
            Some(0o644),
            "the record was 0600 and the process was masked 0o077: the written assignments.ini is 0644"
        );
        assert_eq!(
            after, before,
            "and the write left no temp file of its own beside it"
        );
    }
}

/// E2E phase: the model-file check (PLAN.md step 10 item 13, slice 9;
/// ARCHITECTURE.md (b) 3, "Which writes run them", item 10 of the slices
/// notes, and the key's name under "Renamed error keys"). A `model` path must
/// be a regular file on the node, every shard of a split model included; a
/// relative `model` is resolved against the working directory of the instance
/// (`launch::instance_current_dir`, the folder the engine starts in) and the
/// refusal names the resolved path, the one the check looked at; a model
/// profile with `hf-repo`, `model-url` or `docker-repo` is not checked, the
/// engine filling that file from the hub or fetching it itself; readability by
/// the instance's account is not checked at all. The manager refuses a file
/// where a write puts the model profile in
/// effect on an instance: a new assignment or override in effect on, the global
/// switch on, and a model profile save on every instance where it is in effect.
/// The refusal is `model-profile-model-missing` with `[model profile, instance,
/// path]`, the first file of the value that is missing (a shard named whole,
/// the first missing one), the instances answered in id order. A switch off, an
/// assignment removed and a `*` narrowed never fail it. A refusal writes
/// nothing and renders nothing.
///
/// Every fixture here leaves `ModelsMax` out of the server profiles, so the
/// load-on-startup rule has no cap to judge: `set_model_profile` and
/// `check_renders` skip an instance with none. Each refusal below can therefore
/// only be the model file's.
#[cfg(test)]
pub(crate) fn manager_model_file_check_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let models = dir.path().join("models");
    std::fs::create_dir_all(&models).expect("models dir");

    // The node's model files: `path` is the text a `model` key holds, `make`
    // puts one there, `rm` takes it away.
    let path = |name: &str| models.join(name).to_string_lossy().into_owned();
    let make = |name: &str| {
        std::fs::write(models.join(name), "gguf").expect("model file");
    };
    let rm = |name: &str| {
        let _ = std::fs::remove_file(models.join(name));
    };

    let bff = Caller(":1.9".into());
    let desktop = Caller(":1.42".into());
    let manager = || {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(FakePasswords::new())
        .with_log(FakeActionLog::new())
    };

    let read_opt = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let keys_of = |text: &str, id: &str, key: &str| -> Vec<String> {
        section(text, id)
            .lines()
            .filter(|line| line.trim_start().starts_with(&format!("{key} ")))
            .map(|line| line.to_string())
            .collect()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    // The picture a refusal must leave exactly as it found it: the bytes of the
    // record, of the two pool files and of the renders, a render that is not
    // there included, and the folder's own listing, which is what a file
    // created out of nothing changes.
    let listing_text = || format!("{}\n", listing().join("\n"));
    let seen = |name: &str| -> String {
        if name == "listing" {
            listing_text()
        } else {
            read_opt(name).unwrap_or_default()
        }
    };
    let snapshot = |names: &[&str]| -> Vec<(String, String)> {
        names
            .iter()
            .map(|name| name.to_string())
            .chain(std::iter::once("listing".to_string()))
            .map(|name| (name.clone(), seen(&name)))
            .collect()
    };
    let kept = |before: &[(String, String)]| {
        for (name, text) in before {
            assert_eq!(&seen(name), text, "the refusal wrote nothing: {name}");
        }
    };
    let pool_files = |on: &str, off: &str| {
        std::fs::write(sys.join("models.ini"), on).expect("models.ini");
        std::fs::write(sys.join("models.ini.disabled"), off).expect("models.ini.disabled");
    };
    let assignments = |text: &str| {
        std::fs::write(sys.join("assignments.ini"), text).expect("assignments.ini");
    };
    // The refusal this phase is about, its three args whole.
    let named = |answer: &Result<(), ManagerError>, profile: &str, instance: &str, file: &str| {
        matches!(
            answer,
            Err(ManagerError::Invalid(r))
                if r.key == "model-profile-model-missing"
                    && r.values == [profile.to_string(), instance.to_string(), file.to_string()]
        )
    };
    // The same refusal, its path compared as a path: a relative `model` is
    // looked for resolved against the instance's folder, so the string the
    // refusal names is a join, compared by its components, whichever separator
    // and spelling of the folder it carries.
    let named_at = |answer: &Result<(), ManagerError>,
                    profile: &str,
                    instance: &str,
                    want: &std::path::Path| {
        let same = |a: &str| {
            std::path::absolute(std::path::Path::new(a)).ok() == std::path::absolute(want).ok()
        };
        matches!(
            answer,
            Err(ManagerError::Invalid(r))
                if r.key == "model-profile-model-missing"
                    && r.values.len() == 3
                    && r.values[0] == profile
                    && r.values[1] == instance
                    && same(&r.values[2])
        )
    };

    write_record(&sys, &["default", "lan-2"]);
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
    )
    .expect("server@default.ini");
    std::fs::write(
        sys.join("server@lan-2.ini"),
        "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
    )
    .expect("server@lan-2.ini");
    const WATCH: [&str; 5] = [
        "assignments.ini",
        "models.ini",
        "models.ini.disabled",
        "models@default.ini",
        "models@lan-2.ini",
    ];
    let m = manager();

    // ── An assignment in effect on: the file has to be there ──────────────
    {
        make("ok.gguf");
        make("offok.gguf");
        // The switched-on pool holds `ok` (its file there) and `gone` (its file
        // not); the switched-off file holds `offok` (there) and `offgone` (not).
        let pool = format!(
            "[ok]\nmodel = {}\n\n[gone]\nmodel = {}\n",
            path("ok.gguf"),
            path("gone.gguf")
        );
        let off_pool = format!(
            "[offok]\nmodel = {}\n\n[offgone]\nmodel = {}\n",
            path("offok.gguf"),
            path("offgone.gguf")
        );
        pool_files(&pool, &off_pool);
        let seed = "[default]\ngone = off\n\n[lan-2]\ngone = off\n";

        // All three land in one report, so each case answers for itself.
        let mut refused: Vec<String> = Vec::new();
        for (profile, value, instance, file) in [
            ("gone", "on", "default", path("gone.gguf")),
            ("offgone", "on", "default", path("offgone.gguf")),
            ("gone", "inherit", "default", path("gone.gguf")),
        ] {
            assignments(seed);
            let before = snapshot(&WATCH);
            let answer = m.set_model_profile(&desktop, instance, profile, value);
            if !named(&answer, profile, instance, &file) {
                refused.push(format!(
                    "`{profile} = {value}` on {instance} puts a model profile with no file in effect, answered {answer:?}"
                ));
            }
            kept(&before);
        }
        assert!(
            refused.is_empty(),
            "a new assignment in effect on a model profile with no file is refused:\n{}",
            refused.join("\n")
        );

        // Never refused: `off` and `none` (the model profile leaves the render),
        // `inherit` of one the pool has switched off, and `on` of one whose
        // file is there.
        for (profile, value) in [
            ("gone", "off"),
            ("offgone", "inherit"),
            ("ok", "on"),
            ("offok", "on"),
        ] {
            assignments(seed);
            let answer = m.set_model_profile(&desktop, "default", profile, value);
            assert_eq!(
                answer,
                Ok(()),
                "`{profile} = {value}` is not in effect with a missing file, or its file is there"
            );
            assert!(
                text_of("assignments.ini").contains(&format!("{profile} = {value}")),
                "and its key landed: {profile} = {value}"
            );
        }

        // `none` takes the key of a model profile with no file away, and a `*`
        // narrowed leaves every such model profile out: neither fails.
        assignments(seed);
        assert_eq!(
            m.set_model_profile(&desktop, "default", "gone", "none"),
            Ok(()),
            "an assignment removed is never refused"
        );
        assert!(
            keys_of(&text_of("assignments.ini"), "default", "gone").is_empty(),
            "the key is out"
        );
        assignments("[default]\n* = inherit\n\n[lan-2]\n* = inherit\n");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "*", "none"),
            Ok(()),
            "a `*` narrowed is never refused, whatever the pool holds"
        );
        assert!(
            keys_of(&text_of("assignments.ini"), "default", "*").is_empty(),
            "the global key is out"
        );
    }

    // ── The file has to be a regular one: a directory, a FIFO, a symlink ──
    {
        let base = format!("[ok]\nmodel = {}\n", path("ok.gguf"));
        std::fs::create_dir(models.join("dir.gguf")).expect("a directory named .gguf");
        pool_files(&base, "");
        assignments("[default]\n* = inherit\n\n[lan-2]\n* = inherit\n");

        // A directory is not a model file.
        let with = format!("{base}\n[dir]\nmodel = {}\n", path("dir.gguf"));
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            named(&answer, "dir", "default", &path("dir.gguf")),
            "a directory in `model` is refused as a file that is not there: {answer:?}"
        );
        kept(&before);

        // A FIFO is not a model file either: reading it would block, so it is
        // refused as a file that is not there.
        #[cfg(target_os = "linux")]
        {
            let fifo = models.join("fifo.gguf");
            nix::sys::stat::mknod(
                &fifo,
                nix::sys::stat::SFlag::S_IFIFO,
                nix::sys::stat::Mode::from_bits_truncate(0o644),
                0,
            )
            .expect("mkfifo");
            let with = format!("{base}\n[pipes]\nmodel = {}\n", path("fifo.gguf"));
            let before = snapshot(&WATCH);
            let answer = m.write_file(&desktop, "models.ini", &with, false);
            assert!(
                named(&answer, "pipes", "default", &path("fifo.gguf")),
                "a FIFO in `model` is refused as a file that is not there: {answer:?}"
            );
            kept(&before);
        }

        // A symlink to a regular file is that file: the save lands.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(models.join("ok.gguf"), models.join("link.gguf"))
                .expect("symlink");
            let with = format!("{base}\n[link]\nmodel = {}\n", path("link.gguf"));
            assert_eq!(
                m.write_file(&desktop, "models.ini", &with, false),
                Ok(()),
                "a symlink to a regular file is the file"
            );
            assert_eq!(text_of("models.ini"), with, "and the save landed");
        }
    }

    // ── A split model: every shard, the first missing one named ───────────
    {
        make("big-00001-of-00003.gguf");
        make("big-00003-of-00003.gguf");
        let base = format!("[ok]\nmodel = {}\n", path("ok.gguf"));
        let with = format!(
            "{base}\n[big]\nmodel = {}\n",
            path("big-00001-of-00003.gguf")
        );
        pool_files(&base, "");

        // Shard 2 is not there: the refusal names it, the instance where the
        // model profile is in effect.
        assignments("[default]\n* = inherit\n\n[lan-2]\nbig = off\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            named(&answer, "big", "default", &path("big-00002-of-00003.gguf")),
            "the shard that is missing is named whole: {answer:?}"
        );
        kept(&before);

        // Shard 3 missing and 2 there: the refusal names 3.
        make("big-00002-of-00003.gguf");
        rm("big-00003-of-00003.gguf");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            named(&answer, "big", "default", &path("big-00003-of-00003.gguf")),
            "the first shard that is missing, wherever it falls: {answer:?}"
        );
        kept(&before);

        // Every shard there: the save lands.
        make("big-00003-of-00003.gguf");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &with, false),
            Ok(()),
            "a split model with every shard is a file that is there"
        );
        assert_eq!(text_of("models.ini"), with, "and the save landed");

        // The instance named is the one whose render holds the model profile,
        // and where more than one does, the first in id order. The save lands
        // with every shard above, so re-seed the pool: the case below must
        // really ADD the model profile whose shard is missing, not re-write
        // one the pool already holds.
        pool_files(&base, "");
        rm("big-00002-of-00003.gguf");
        assignments("[default]\nbig = off\n\n[lan-2]\n* = inherit\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            named(&answer, "big", "lan-2", &path("big-00002-of-00003.gguf")),
            "in effect on lan-2 alone: lan-2 is named: {answer:?}"
        );
        kept(&before);

        pool_files(&base, "");
        assignments("[default]\n* = inherit\n\n[lan-2]\n* = inherit\n");
        rm("big-00002-of-00003.gguf");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            named(&answer, "big", "default", &path("big-00002-of-00003.gguf")),
            "in effect on both: the first instance in id order answers: {answer:?}"
        );
        kept(&before);
        make("big-00002-of-00003.gguf");
    }

    // ── `hf-repo`, `model-url`, `docker-repo`: that file is not the node's ─
    {
        // The engine fills `model` from the hub (`--hf-repo`) or replaces it
        // with a file it fetches itself (`--model-url`, `--docker-repo`:
        // llama.cpp `common/arg.cpp` 427-441 and 480-535 in v1.0.0). The file
        // the model profile names is therefore not on the node at save time,
        // so a model profile carrying any of the three keys is not checked.
        let base = format!("[base]\nmodel = {}\n", path("ok.gguf"));
        pool_files(&base, "");
        assignments("[default]\n* = inherit\n\n[lan-2]\n* = inherit\n");
        let fetches: [(&str, &str, &str); 3] = [
            ("hf", "hf-repo", "theos/Big-8B-Q4"),
            ("url", "model-url", "https://example.test/Big-8B-Q4.gguf"),
            ("docker", "docker-repo", "ggmlorg/llama:server"),
        ];

        for (id, key, value) in fetches {
            // Its `model` file is there: the key adds no file to look for.
            let with = format!(
                "{base}\n[{id}]\nmodel = {}\n{key} = {value}\n",
                path("ok.gguf")
            );
            assert_eq!(
                m.write_file(&desktop, "models.ini", &with, false),
                Ok(()),
                "`{key}` with its `model` file there lands"
            );
            assert_eq!(text_of("models.ini"), with, "and the save landed: {key}");

            // Its `model` file is not there: the engine is the one putting it
            // there, so the check looks at nothing and the save lands.
            let with = format!(
                "{base}\n[{id}]\nmodel = {}\n{key} = {value}\n",
                path(&format!("{id}-gone.gguf"))
            );
            assert_eq!(
                m.write_file(&desktop, "models.ini", &with, false),
                Ok(()),
                "`{key}` with a `model` that is not there is not checked: the engine fetches that file"
            );
            assert_eq!(text_of("models.ini"), with, "and the save landed: {key}");
        }

        // The skip is the keyed model profile's own: another one in the same
        // save, with none of the three keys, is checked as always.
        let with = format!(
            "{base}\n[hf]\nmodel = {}\nhf-repo = theos/Big-8B-Q4\n\n[plain]\nmodel = {}\n",
            path("hf-gone.gguf"),
            path("plain-gone.gguf")
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            named(&answer, "plain", "default", &path("plain-gone.gguf")),
            "`[hf]` is skipped, `[plain]` beside it is refused: {answer:?}"
        );
        kept(&before);

        // A model profile with `hf-repo` and no `model` at all is never refused
        // by this check: it names no file. It is the `model` rule's own, the key
        // the manager answers today.
        let with = format!("{base}\n[repo]\nhf-repo = theos/Big-8B-Q4\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &with, false);
        assert!(
            matches!(&answer, Err(ManagerError::Invalid(r))
                if r.key == "model-profile-no-model" && r.values == ["repo"]),
            "`hf-repo` alone is the `model` rule's, never the model file's: {answer:?}"
        );
        kept(&before);
    }

    // ── A model profile save: only where it is in effect ──────────────────
    {
        let on_disk = format!("[ok]\nmodel = {}\n", path("ok.gguf"));
        let save = format!("{on_disk}\n[new]\nmodel = {}\n", path("new.gguf"));
        pool_files(&on_disk, "");

        // In effect on no instance: every instance has it `off`, so the save
        // lands with the file missing.
        assignments("[default]\nnew = off\n\n[lan-2]\nnew = off\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &save, false),
            Ok(()),
            "a save whose only new model profile is in effect nowhere lands"
        );
        assert_eq!(text_of("models.ini"), save, "and landed");

        // In effect on one instance: that instance is named. The save above
        // landed, so re-seed the pool: the case below must really ADD the
        // model profile whose file is missing.
        pool_files(&on_disk, "");
        assignments("[default]\nnew = off\n\n[lan-2]\n* = inherit\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &save, false);
        assert!(
            named(&answer, "new", "lan-2", &path("new.gguf")),
            "the save puts the model profile in effect on lan-2: {answer:?}"
        );
        kept(&before);

        // The switch off, on both sides: taking the model profile out of the
        // switched-on file, and putting it into the switched-off one, are never
        // refused, not even while the assignment keeps it on. The state a real
        // switch off starts from, seeded directly: the model profile in the
        // switched-on file, its file not there, in effect on `default`.
        pool_files(&save, "");
        assignments("[default]\nnew = on\n");
        let answer = m.write_file(
            &desktop,
            "models.ini.disabled",
            &format!("[new]\nmodel = {}\n", path("new.gguf")),
            false,
        );
        assert_eq!(
            answer,
            Ok(()),
            "a switch off never fails a resource check, whatever the file: {answer:?}"
        );
        assert_eq!(
            text_of("models.ini.disabled"),
            format!("[new]\nmodel = {}\n", path("new.gguf")),
            "and landed"
        );
        assignments("[default]\nnew = inherit\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &on_disk, false),
            Ok(()),
            "the other half of the switch, the model profile out of the switched-on file, lands"
        );
        assert_eq!(text_of("models.ini"), on_disk, "and landed");
    }

    // ── The global switch on ──────────────────────────────────────────────
    {
        let on_disk = format!("[ok]\nmodel = {}\n", path("ok.gguf"));
        let off = format!("[sw]\nmodel = {}\n", path("sw.gguf"));
        let on = format!("{on_disk}\n[sw]\nmodel = {}\n", path("sw.gguf"));
        pool_files(&on_disk, &off);

        // The model profile sits switched off with its file missing, assigned to
        // both instances with no override of the switch. Switching it on, the
        // write into the switched-on file, is refused on the first of them.
        assignments("[default]\nsw = inherit\n\n[lan-2]\nsw = inherit\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &on, false);
        assert!(
            named(&answer, "sw", "default", &path("sw.gguf")),
            "the global switch on, in effect on every instance that does not say off: {answer:?}"
        );
        kept(&before);

        // The same switch where every instance says off lands, and with it the
        // second write of the switch, the model profile out of the switched-off
        // file.
        assignments("[default]\nsw = off\n\n[lan-2]\nsw = off\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &on, false),
            Ok(()),
            "the switch on lands where every instance has the model profile off"
        );
        assert_eq!(
            m.write_file(&desktop, "models.ini.disabled", "", false),
            Ok(()),
            "and so does the model profile leaving the switched-off file"
        );
    }

    // ── A relative `model`: the instance's own folder is where it lives ───
    {
        // The engine resolves a relative `model` against the working directory
        // of the process it launches, `launch::instance_current_dir` (the
        // engine's `state` directory on Windows, the data root elsewhere: the
        // folder the router and its children inherit). The check resolves the
        // value there, and the path it names is the one it looked at, the
        // resolved one, not the value as written.
        let cwd = crate::launch::instance_current_dir(
            cfg!(windows),
            &crate::paths::data_root(),
            &crate::paths::instance_state_dir(),
        );
        std::fs::create_dir_all(cwd.join("Qwen")).expect("the instance's folder");
        let on_disk = format!("[ok]\nmodel = {}\n", path("ok.gguf"));
        pool_files(&on_disk, "");
        assignments("[default]\n* = inherit\n\n[lan-2]\n* = inherit\n");

        // The file is in that folder: the save lands.
        std::fs::write(cwd.join("Qwen/Big-8B-Q4.gguf"), "gguf").expect("model file");
        let rel = format!("{on_disk}\n[rel]\nmodel = Qwen/Big-8B-Q4.gguf\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &rel, false),
            Ok(()),
            "a relative `model` whose file is in the instance's folder lands"
        );
        assert_eq!(text_of("models.ini"), rel, "and the save landed");

        // The file is not there: refused, naming the resolved path.
        let gone = format!("{on_disk}\n[rel]\nmodel = Qwen/Gone-8B-Q4.gguf\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &gone, false);
        assert!(
            named_at(&answer, "rel", "default", &cwd.join("Qwen/Gone-8B-Q4.gguf")),
            "a relative `model` missing from the instance's folder is refused, naming the path the check looked at: {answer:?}"
        );
        kept(&before);
    }

    // ── An instance whose server profile is not there ─────────────────────
    {
        // `ghost` runs a server profile whose file is missing, so its launch
        // refuses it and the load-on-startup rule skips it (no cap to read).
        // The model-file check has no cap, so it runs on `ghost`'s render too.
        std::fs::write(
            sys.join(INSTANCES_FILE),
            "[default]\nServerProfile = default\n\n[ghost]\nServerProfile = ghost\n",
        )
        .expect("instances.ini: ghost has no profile file");
        let on_disk = format!("[ok]\nmodel = {}\n", path("ok.gguf"));
        let save = format!("{on_disk}\n[lost]\nmodel = {}\n", path("lost.gguf"));
        pool_files(&on_disk, "");
        let watch: Vec<&str> = WATCH
            .iter()
            .copied()
            .chain(std::iter::once("models@ghost.ini"))
            .collect();

        assignments("[default]\nlost = off\n\n[ghost]\n* = inherit\n");
        let before = snapshot(&watch);
        let answer = m.write_file(&desktop, "models.ini", &save, false);
        assert!(
            named(&answer, "lost", "ghost", &path("lost.gguf")),
            "in effect on the instance with no profile file: that instance answers: {answer:?}"
        );
        kept(&before);

        pool_files(&save, "");
        assignments("[default]\nlost = off\n\n[ghost]\nlost = off\n");
        let before = snapshot(&watch);
        let answer = m.set_model_profile(&desktop, "ghost", "lost", "on");
        assert!(
            named(&answer, "lost", "ghost", &path("lost.gguf")),
            "an assignment on an instance with no profile file is refused as any other: {answer:?}"
        );
        kept(&before);

        write_record(&sys, &["default", "lan-2"]);
    }

    // ── A broken profile in effect: the check refuses only what the write adds ──
    {
        // ARCHITECTURE.md (b) "Which writes run them": "A switch off, an
        // assignment removed, a `*` narrowed never fail a resource check", so a
        // check refuses what the write ADDS and never a file a model profile
        // already in effect lost earlier. Here `b` sits in effect on `default`
        // and its file is deleted after it was assigned, so every write that
        // touches only another model profile must land, and the two that add a
        // missing file answer naming the model profile they add, never `b`.
        let section = |id: &str, file: &str, extra: &str| {
            format!("[{id}]\nmodel = {}\n{extra}\n", path(file))
        };
        let b_sec = section("b", "b.gguf", "");
        let a_sec = section("a", "a.gguf", "ctx-size = 4096");
        let c_sec = section("c", "c.gguf", "");
        let d_sec = section("d", "d.gguf", "");
        let e_sec = section("e", "e.gguf", "");
        let pool = format!("{b_sec}\n{a_sec}\n{c_sec}\n{d_sec}\n{e_sec}");
        // `b`, `a` are in effect on `default` by their own key, `c` through
        // `* = inherit`; `d` and `e` are off. `lan-2` runs `a` alone.
        let seeded =
            "[default]\n* = inherit\na = on\nb = on\nd = off\ne = off\n\n[lan-2]\na = on\n";
        // The picture before each write: `b`'s file gone (deleted after the
        // assignment landed), every other file of the pool there.
        let seed = || {
            make("a.gguf");
            make("c.gguf");
            make("d.gguf");
            rm("b.gguf");
            rm("e.gguf");
            pool_files(&pool, "");
            assignments(seeded);
        };
        // The snapshot that is not there, told by name, so a case that wrote
        // something reports it and the next case still runs.
        let changed = |before: &[(String, String)]| -> Option<String> {
            before
                .iter()
                .find(|(name, text)| seen(name) != *text)
                .map(|(name, _)| name.clone())
        };
        let mut fails: Vec<String> = Vec::new();

        // (a) An assignment removed, and an override switched off, on a model
        // profile that is not the broken one.
        // S9B-PIN-1 BEGIN
        {
            seed();
            let answer = m.set_model_profile(&desktop, "default", "a", "none");
            if answer != Ok(()) {
                fails.push(format!(
                    "(a) `a = none` while `b` is broken in effect lands: {answer:?}"
                ));
            } else if !keys_of(&text_of("assignments.ini"), "default", "a").is_empty() {
                fails.push("(a) and the key `a` is out of the section".to_string());
            }
        }
        // S9B-PIN-1 END
        // S9B-PIN-2 BEGIN
        {
            seed();
            let answer = m.set_model_profile(&desktop, "default", "a", "off");
            if answer != Ok(()) {
                fails.push(format!(
                    "(a) `a = off` while `b` is broken in effect lands: {answer:?}"
                ));
            } else if !text_of("assignments.ini").contains("a = off") {
                fails.push("(a) and the key `a = off` landed".to_string());
            }
        }
        // S9B-PIN-2 END

        // (b) The global switch narrowed: `*` out, which takes model profiles
        // out of the render and adds none.
        // S9B-PIN-3 BEGIN
        {
            seed();
            let answer = m.set_model_profile(&desktop, "default", "*", "none");
            if answer != Ok(()) {
                fails.push(format!(
                    "(b) `* = none` while `b` is broken in effect lands: {answer:?}"
                ));
            } else if !keys_of(&text_of("assignments.ini"), "default", "*").is_empty() {
                fails.push("(b) and the global key is out".to_string());
            }
        }
        // S9B-PIN-3 END

        // (c) A save of `models.ini` that edits one non-model key of another
        // model profile, and (d) the switch off of a third one: the two writes
        // of the switch, the model profile out of the switched-on file and into
        // the switched-off one.
        // S9B-PIN-4 BEGIN
        {
            seed();
            let edit = pool.replace("ctx-size = 4096", "ctx-size = 8192");
            let answer = m.write_file(&desktop, "models.ini", &edit, false);
            if answer != Ok(()) {
                fails.push(format!(
                    "(c) a save of `models.ini` that edits only `a`'s ctx-size lands: {answer:?}"
                ));
            } else if text_of("models.ini") != edit {
                fails.push("(c) and the save landed".to_string());
            }
        }
        // S9B-PIN-4 END
        // S9B-PIN-5 BEGIN
        {
            seed();
            let no_c = format!("{b_sec}\n{a_sec}\n{d_sec}\n{e_sec}");
            let answer = m.write_file(&desktop, "models.ini", &no_c, false);
            if answer != Ok(()) {
                fails.push(format!(
                    "(d) the switch off of `c`: `c` out of `models.ini` lands: {answer:?}"
                ));
            } else {
                let off = format!("[c]\nmodel = {}\n", path("c.gguf"));
                let second = m.write_file(&desktop, "models.ini.disabled", &off, false);
                if second != Ok(()) {
                    fails.push(format!(
                        "(d) the switch off of `c`: `c` into `models.ini.disabled` lands: {second:?}"
                    ));
                }
            }
        }
        // S9B-PIN-5 END

        // (e) The switch on of a model profile whose file IS there, beside the
        // broken one.
        // S9B-PIN-6 BEGIN
        {
            seed();
            let answer = m.set_model_profile(&desktop, "default", "d", "on");
            if answer != Ok(()) {
                fails.push(format!(
                    "(e) `d = on`, its file there, while `b` is broken lands: {answer:?}"
                ));
            } else if !text_of("assignments.ini").contains("d = on") {
                fails.push("(e) and the key `d = on` landed".to_string());
            }
        }
        // S9B-PIN-6 END

        // (f) and (g) stay refused: the write that ADDS a model profile with no
        // file, and the save that points a model profile at a file that is not
        // there. Each names the model profile the write touches, not `b`, and
        // writes nothing.
        // S9B-PIN-7 BEGIN
        {
            seed();
            let before = snapshot(&WATCH);
            let answer = m.set_model_profile(&desktop, "default", "e", "on");
            if !named(&answer, "e", "default", &path("e.gguf")) {
                fails.push(format!(
                    "(f) `e = on` refused naming `e` while `b` is broken: {answer:?}"
                ));
            }
            if let Some(what) = changed(&before) {
                fails.push(format!("(f) the refusal wrote nothing: {what} changed"));
            }
        }
        // S9B-PIN-7 END
        // S9B-PIN-8 BEGIN
        {
            seed();
            let before = snapshot(&WATCH);
            let a_gone = pool.replace(
                &format!("model = {}\nctx-size", path("a.gguf")),
                &format!("model = {}\nctx-size", path("a-gone.gguf")),
            );
            let answer = m.write_file(&desktop, "models.ini", &a_gone, false);
            if !named(&answer, "a", "default", &path("a-gone.gguf")) {
                fails.push(format!(
                    "(g) a save pointing `a` at a missing file refused naming `a`: {answer:?}"
                ));
            }
            if let Some(what) = changed(&before) {
                fails.push(format!("(g) the refusal wrote nothing: {what} changed"));
            }
        }
        // S9B-PIN-8 END

        assert!(
            fails.is_empty(),
            "a resource check refuses only what the write adds, never a model profile already \
             in effect with a missing file:\n{}",
            fails.join("\n")
        );
    }

    // ── A switched-off model profile in effect: the disabled file is checked ─
    //
    // `assign::render` takes BOTH pool files, so a model profile that sits in
    // `models.ini.disabled` is in effect on every instance whose section of
    // `assignments.ini` says `on` for it. A save of the switched-off file
    // changes that instance's render, so it is judged by the model-file check
    // exactly as a save of `models.ini` is: it refuses what the write ADDS, on
    // every render the write moves, and nothing else.
    // S9C-PIN-1 BEGIN
    {
        // The switched-on file holds `ok`, the switched-off one `p` and `q`,
        // every file there. `default` has `p = on`, `lan-2` has `p = off`, and
        // no instance says anything about `q`.
        let sec = |id: &str, file: &str, ctx: &str| {
            format!("[{id}]\nmodel = {}\nctx-size = {ctx}\n", path(file))
        };
        let on = sec("ok", "ok.gguf", "4096");
        let off = format!(
            "{}\n{}",
            sec("p", "p.gguf", "4096"),
            sec("q", "q.gguf", "1024")
        );
        let seed = || {
            make("ok.gguf");
            make("p.gguf");
            make("q.gguf");
            pool_files(&on, &off);
            assignments("[default]\np = on\n\n[lan-2]\np = off\n");
        };

        seed();
        // The save points `p`, which `default` has `on`, at a file that is not
        // there: `default`'s render gains a model file the node does not have,
        // so the save is refused naming `p`, `default` and that path, and
        // writes nothing.
        let save = off.replace(
            &format!("model = {}\nctx-size", path("p.gguf")),
            &format!("model = {}\nctx-size", path("p-gone.gguf")),
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini.disabled", &save, false);
        assert!(
            named(&answer, "p", "default", &path("p-gone.gguf")),
            "a save of `models.ini.disabled` that puts a model profile with no file in effect on an instance that has it `on` is refused: {answer:?}"
        );
        kept(&before);
    }
    // S9C-PIN-1 END
    // S9C-PIN-2 BEGIN
    {
        let sec = |id: &str, file: &str, ctx: &str| {
            format!("[{id}]\nmodel = {}\nctx-size = {ctx}\n", path(file))
        };
        let on = sec("ok", "ok.gguf", "4096");
        let off = format!(
            "{}\n{}",
            sec("p", "p.gguf", "4096"),
            sec("q", "q.gguf", "1024")
        );
        let seed = || {
            make("ok.gguf");
            make("p.gguf");
            make("q.gguf");
            pool_files(&on, &off);
            assignments("[default]\np = on\n\n[lan-2]\np = off\n");
        };

        seed();
        // The same save for `q`: no instance has it `on`, so it is in no
        // render, its missing file is in effect nowhere, and the save lands.
        let save = off.replace(
            &format!("model = {}\nctx-size", path("q.gguf")),
            &format!("model = {}\nctx-size", path("q-gone.gguf")),
        );
        let answer = m.write_file(&desktop, "models.ini.disabled", &save, false);
        assert_eq!(
            answer,
            Ok(()),
            "a switched-off model profile no instance has `on` is in no render, so a missing file is not in effect anywhere: {answer:?}"
        );
        assert_eq!(text_of("models.ini.disabled"), save, "and the save landed");
    }
    // S9C-PIN-2 END
    // S9C-PIN-3 BEGIN
    {
        let sec = |id: &str, file: &str, ctx: &str| {
            format!("[{id}]\nmodel = {}\nctx-size = {ctx}\n", path(file))
        };
        let on = sec("ok", "ok.gguf", "4096");
        let off = format!(
            "{}\n{}",
            sec("p", "p.gguf", "4096"),
            sec("q", "q.gguf", "1024")
        );
        let seed = || {
            make("ok.gguf");
            make("p.gguf");
            make("q.gguf");
            pool_files(&on, &off);
            assignments("[default]\np = on\n\n[lan-2]\np = off\n");
        };

        // `p` sits in effect on `default` with its file gone, taken away after
        // the assignment landed. The check refuses only what a write adds, so a
        // save of the switched-off file that touches another key lands: first
        // a key of `p` itself, then a key of `q` beside the broken one.
        seed();
        rm("p.gguf");
        let edit_p = off.replace("ctx-size = 4096", "ctx-size = 8192");
        let answer = m.write_file(&desktop, "models.ini.disabled", &edit_p, false);
        assert_eq!(
            answer,
            Ok(()),
            "a save of the switched-off file that edits only `ctx-size` of a model profile already in effect with a missing file lands: {answer:?}"
        );
        assert_eq!(
            text_of("models.ini.disabled"),
            edit_p,
            "and the save landed"
        );

        let edit_q = edit_p.replace("ctx-size = 1024", "ctx-size = 2048");
        let second = m.write_file(&desktop, "models.ini.disabled", &edit_q, false);
        assert_eq!(
            second,
            Ok(()),
            "a save of the switched-off file that edits only another model profile, beside one in effect with a missing file, lands: {second:?}"
        );
        assert_eq!(
            text_of("models.ini.disabled"),
            edit_q,
            "and the save landed"
        );
    }
    // S9C-PIN-3 END
}

/// A GGUF v3 header for the memory fixtures: a `llama` model of two layers
/// trained on `ctx_len` tokens, with the KV layout the reader of
/// `pai_core::gguf_shape` knows (the head counts written below), and one
/// tensor of `bytes` per `(name, bytes)`, laid out as llama.cpp does (each
/// aligned to 32). A copy of `pai-core`'s `gguf_shape::tests::gguf` and
/// `write_sparse`: a test module imports no other test module, and the bytes
/// both check are the same.
#[cfg(test)]
fn write_gguf(path: &std::path::Path, ctx_len: u32, tensors: &[(&str, u64)]) {
    let put_str = |out: &mut Vec<u8>, s: &str| {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    };
    let put_u32 = |out: &mut Vec<u8>, x: u32| out.extend_from_slice(&x.to_le_bytes());
    let put_u64 = |out: &mut Vec<u8>, x: u64| out.extend_from_slice(&x.to_le_bytes());
    let mut out = b"GGUF".to_vec();
    put_u32(&mut out, 3);
    put_u64(&mut out, tensors.len() as u64);
    put_u64(&mut out, 8);
    for (k, v) in [
        ("general.architecture", None),
        ("llama.context_length", Some(ctx_len)),
        ("llama.block_count", Some(2)),
        ("llama.embedding_length", Some(2048)),
        ("llama.attention.head_count", Some(32)),
        ("llama.attention.head_count_kv", Some(4)),
        ("llama.attention.key_length", Some(128)),
        ("llama.attention.value_length", Some(128)),
    ] {
        put_str(&mut out, k);
        match v {
            None => {
                put_u32(&mut out, 8);
                put_str(&mut out, "llama");
            }
            Some(x) => {
                put_u32(&mut out, 4);
                put_u32(&mut out, x);
            }
        }
    }
    let mut off = 0u64;
    for (name, bytes) in tensors {
        put_str(&mut out, name);
        put_u32(&mut out, 2);
        put_u64(&mut out, 4096);
        put_u64(&mut out, bytes / 4096);
        put_u32(&mut out, 12);
        put_u64(&mut out, off);
        off += bytes.div_ceil(32) * 32;
    }
    let data_start = (out.len() as u64).div_ceil(32) * 32;
    std::fs::write(path, out).expect("gguf header");
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open gguf")
        .set_len(data_start + off)
        .expect("sparse gguf");
}

/// E2E phase: the model profile memory check (PLAN.md step 10 item 13, slice
/// 10b-2; ARCHITECTURE.md (b) 1, 2, 3). The manager measures a write of the
/// model profile pool against the node's GPU memory with the pure estimate of
/// `pai_core::gguf_shape` (slice 10b-1), over every instance's render in id
/// order, and refuses with `model-profile-over-memory` and
/// `[model profile, instance, needed MiB rounded up, available MiB rounded
/// down]` when ONE model profile is over the node's usable memory AND this
/// write makes that profile larger than it stands in today's render: the need
/// named is that one profile's estimate, never a sum over the render (slice
/// 10b). The assignment verb `set_model_profile` measures the render its
/// record leaves in effect, and the GPU seam is asked only once a section has
/// grown. The estimate reads four keys,
/// `model`, `ctx-size`, `cache-type-k` and `cache-type-v`, with `[*]` filling
/// a key a section has none of, the trained context standing in for a
/// `ctx-size` that is absent or 0, and f16 standing in for a cache type that
/// is absent. A model profile with `hf-repo`, `model-url` or `docker-repo`,
/// one with no `model`, and one whose `model` is not a GGUF the reader knows,
/// are judged on nothing; a node the GPU seam answers no pool for, an empty
/// pool included, is measured on nothing. The refusal writes nothing and
/// renders nothing.
///
/// The pool of every fixture is a two-layer `llama` (`write_gguf`) whose
/// weights and KV terms are both far over the small card, and the cards are
/// sized from the estimate read off that file with the same pure API the
/// manager uses, so no figure here restates the arithmetic of 10b-1.
#[cfg(test)]
pub(crate) fn manager_memory_check_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGpus, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };
    use pai_core::gguf_shape::{full_gpu_bytes, local_shape, GPU_RESERVE, MIB};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let models = dir.path().join("models");
    std::fs::create_dir_all(&models).expect("models dir");

    let model = models.join("qwen.gguf");
    write_gguf(
        &model,
        262_144,
        &[
            ("token_embd.weight", 200 << 20),
            ("blk.0.attn_q.weight", 300 << 20),
            ("blk.1.attn_q.weight", 300 << 20),
            ("output.weight", 200 << 20),
        ],
    );
    std::fs::write(models.join("notes.txt"), "not a model").expect("notes.txt");
    let path = model.to_string_lossy().into_owned();
    let txt = models.join("notes.txt").to_string_lossy().into_owned();

    // What the model needs, read from the file with the pure estimate: the
    // phase sizes its cards and its refusals with these, never with figures it
    // worked out by hand.
    let shape = local_shape(&model).expect("fixture shape");
    let trained = shape.trained_ctx;
    let need = |ctx: u64, k: &str, v: &str| full_gpu_bytes(&shape, ctx, k, v);
    let mib_up = |bytes: u64| (bytes.div_ceil(MIB)).to_string();
    let mib_down = |bytes: u64| (bytes / MIB).to_string();
    // Every card carries the reserve the estimate keeps free (`GPU_RESERVE`),
    // so its usable memory is what the test adds to the total.
    let reserve = GPU_RESERVE / MIB;
    let small = need(1024, "f16", "f16") / MIB + reserve + 32;
    let big = need(trained, "f16", "f16") / MIB + reserve + 32;
    // A card sitting between the two contexts the growth case moves between:
    // it fits the model at 131072 tokens and not at the trained one.
    let mid = need(131_072, "f16", "f16") / MIB + reserve + 32;
    let usable = |total_mib: u64| {
        pai_core::devices::pool_usable(&[pai_core::devices::Gpu {
            id: "CUDA0".to_string(),
            name: "NVIDIA GeForce CUDA0".to_string(),
            total_mib,
        }])
    };

    let bff = Caller(":1.9".into());
    let desktop = Caller(":1.42".into());
    let manager_on = |gpus: FakeGpus| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(FakePasswords::new())
        .with_log(FakeActionLog::new())
        .with_gpus(gpus)
    };

    let read_opt = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let seen = |name: &str| -> String {
        if name == "listing" {
            format!("{}\n", listing().join("\n"))
        } else {
            read_opt(name).unwrap_or_default()
        }
    };
    let snapshot = |names: &[&str]| -> Vec<(String, String)> {
        names
            .iter()
            .map(|name| name.to_string())
            .chain(std::iter::once("listing".to_string()))
            .map(|name| (name.clone(), seen(&name)))
            .collect()
    };
    let kept = |before: &[(String, String)]| {
        for (name, text) in before {
            assert_eq!(&seen(name), text, "the refusal wrote nothing: {name}");
        }
    };
    let pool_files = |on: &str, off: &str| {
        std::fs::write(sys.join("models.ini"), on).expect("models.ini");
        std::fs::write(sys.join("models.ini.disabled"), off).expect("models.ini.disabled");
    };
    let assignments = |text: &str| {
        std::fs::write(sys.join("assignments.ini"), text).expect("assignments.ini");
    };
    // The refusal this phase is about, its four args whole.
    let over = |answer: &Result<(), ManagerError>,
                profile: &str,
                instance: &str,
                needed: u64,
                available: u64| {
        matches!(
            answer,
            Err(ManagerError::Invalid(r))
                if r.key == "model-profile-over-memory"
                    && r.values == [
                        profile.to_string(),
                        instance.to_string(),
                        mib_up(needed),
                        mib_down(available)
                    ]
        )
    };

    write_record(&sys, &["default"]);
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
    )
    .expect("server@default.ini");
    const WATCH: [&str; 5] = [
        "assignments.ini",
        "models.ini",
        "models.ini.disabled",
        "models@default.ini",
        "models@a-lan.ini",
    ];

    // The pool texts, one profile of the fixture model.
    let on_text = format!("[qwen]\nmodel = {path}\nctx-size = {trained}\n");
    let fits_text = format!("[qwen]\nmodel = {path}\nctx-size = 1024\n");

    // The cards are sized from the estimate of the fixture, and the phase
    // depends on them sitting between the model's needs: pinned here, so a
    // refusal that does not come means the check, not a fixture that stopped
    // being over its card.
    assert_eq!(
        local_shape(&model).map(|s| s.kv_f16_per_token.is_some()),
        Ok(true),
        "the fixture's KV layout is one the reader knows: with no KV term every \
         context and cache-type case below would measure the same bytes"
    );
    assert!(
        need(1024, "f16", "f16") <= usable(small),
        "the small card fits the model at 1024 tokens"
    );
    assert!(
        need(trained, "f16", "f16") > usable(small),
        "the small card does not fit it at the context it was trained on"
    );
    assert!(
        need(trained, "f16", "f16") <= usable(big),
        "the big card fits it at the trained context, f16 on both sides"
    );
    assert!(
        need(131_072, "f16", "f16") <= usable(mid),
        "the mid card fits it at 131072 tokens"
    );
    assert!(
        need(trained, "f16", "f16") > usable(mid),
        "the mid card does not fit it at the trained context: the growth case is refused \
         on this card and lands below it"
    );
    assert!(
        need(trained, "f32", "f32") > usable(big),
        "the big card does not fit it with an f32 KV cache"
    );

    // ── Over the node: refused, on an instance that loads it ───────────────
    {
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &on_text, false);
        assert!(
            over(
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "a model profile that does not fit the node's GPU memory is refused, naming \
             itself, the instance, the MiB it needs and the MiB the node has: {answer:?}"
        );
        kept(&before);
    }

    // ── The same profile, sized to fit: it lands ──────────────────────────
    {
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        assert_eq!(
            m.write_file(&desktop, "models.ini", &fits_text, false),
            Ok(()),
            "the same model profile at 1024 tokens fits the card and lands"
        );
        assert_eq!(text_of("models.ini"), fits_text, "the save landed");
    }

    // ── The four keys the estimate reads ─────────────────────────────────
    {
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        // No `ctx-size` at all: the trained context, so this is the same need
        // as the one refused above.
        let bare = format!("[qwen]\nmodel = {path}\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &bare, false);
        assert!(
            over(
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "a `ctx-size` that is absent is the context the model was trained on: {answer:?}"
        );
        kept(&before);

        // `ctx-size = 0` is the same: 0 is llama.cpp's "unset".
        let zero = format!("[qwen]\nmodel = {path}\nctx-size = 0\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &zero, false);
        assert!(
            over(
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "a `ctx-size` of 0 is the trained context too: {answer:?}"
        );
        kept(&before);

        // No cache type keys at all: f16 on both sides, the engine's default,
        // which is what the card sized for f16 fits.
        let m_big = manager_on(FakeGpus::new(&[("CUDA0", big)]));
        assert_eq!(
            m_big.write_file(&desktop, "models.ini", &on_text, false),
            Ok(()),
            "cache types absent are f16: on the card sized for f16 this lands, \
             and one bit wider on either side does not"
        );
    }

    // ── A cache type makes it grow: refused ──────────────────────────────
    {
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", big)]));
        let f32 = format!(
            "[qwen]\nmodel = {path}\nctx-size = {trained}\ncache-type-k = f32\ncache-type-v = f32\n"
        );
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &f32, false);
        assert!(
            over(
                &answer,
                "qwen",
                "default",
                need(trained, "f32", "f32"),
                usable(big)
            ),
            "an f32 KV cache on both sides doubles the KV term and does not fit: {answer:?}"
        );
        kept(&before);
    }

    // ── `[*]` fills a key the section has none of ────────────────────────
    //
    // A client cannot write the global section (a client's `[*]` is refused
    // `invalid-model-profile-id`, pinned in the assignments phase), so it
    // reaches the pool as a hand edit of `models.ini` and a save that keeps it
    // byte for byte. The cascade is pinned on the cache types: the card fits
    // the model at f16 and not at f32, so a section that writes no cache key
    // is measured on `[*]`'s f32.
    {
        // The global section, f32 on both sides, hand-edited into the
        // switched-on file, and the model profile that says nothing about the
        // cache sitting switched off: today's render holds no model profile,
        // so the need the write puts in effect is all this write's.
        let global = "[*]\ncache-type-k = f32\ncache-type-v = f32\n";
        pool_files(global, &on_text);
        assignments("[default]\nqwen = inherit\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", big)]));

        // The switch on: the section moves to the switched-on file, the
        // `[*]` of the file kept byte for byte as a save keeps it.
        let switch_on = format!("{global}{on_text}");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &switch_on, false);
        assert!(
            over(
                &answer,
                "qwen",
                "default",
                need(trained, "f32", "f32"),
                usable(big)
            ),
            "the cache types of `[*]` are the need of a model profile that writes none, on \
             a card that fits it at f16 and not at f32: {answer:?}"
        );
        kept(&before);

        // The section's own keys win over `[*]`'s: the same model profile
        // f16 of its own lands on the card `[*]`'s f32 does not fit. The pool
        // is hand-edited to the switched-on state, so the save is a plain edit
        // of a model profile that is in effect, not a switch.
        pool_files(&format!("{global}{on_text}"), "");
        let own = format!(
            "[qwen]\nmodel = {path}\nctx-size = {trained}\ncache-type-k = f16\ncache-type-v = f16\n"
        );
        let save = format!("{global}{own}");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &save, false),
            Ok(()),
            "a model profile's own cache types replace `[*]`'s, and fit the card: {save:?}"
        );
        assert_eq!(text_of("models.ini"), save, "the save landed");

        // The same rule, from the other side, as a refusal: `[*]` is f16 and
        // the section's own keys are f32, so the render is over the card. An
        // implementation that let `[*]` win over a key the section writes
        // lands this save.
        let f16 = "[*]\ncache-type-k = f16\ncache-type-v = f16\n";
        pool_files(&format!("{f16}{on_text}"), "");
        let own32 = own.replace("f16", "f32");
        let wider = format!("{f16}{own32}");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &wider, false);
        assert!(
            over(
                &answer,
                "qwen",
                "default",
                need(trained, "f32", "f32"),
                usable(big)
            ),
            "the section's own f32 cache types, not `[*]`'s f16, are the need: {answer:?}"
        );
        kept(&before);
    }

    // ── Judged on nothing ────────────────────────────────────────────────
    {
        pool_files("", "");
        assignments("[default]\nqwen = on\nhf = on\nnotes = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        // The engine fills a `hf-repo` model itself: not judged.
        let fetch = "[hf]\nmodel = /models/never-fetched.gguf\nhf-repo = theos/Big-8B-Q4\n";
        assert_eq!(
            m.write_file(&desktop, "models.ini", fetch, false),
            Ok(()),
            "a model profile with `hf-repo` names no file of the node, and is not measured"
        );

        // A `model` that is not a GGUF has no shape to measure: not judged,
        // and it does not blind the check for the profile beside it.
        let notes = format!("[notes]\nmodel = {txt}\nctx-size = {trained}\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &notes, false),
            Ok(()),
            "a `model` that is not a GGUF is measured on nothing and lands"
        );
        let both = format!("{notes}{on_text}");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &both, false);
        assert!(
            over(&answer, "qwen", "default", need(trained, "f16", "f16"), usable(small)),
            "an unmeasurable model profile beside one that does not fit does not hide it: {answer:?}"
        );
        kept(&before);
    }

    // ── A switch off, and an assignment that loads nothing ───────────────
    {
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        // Switching the over-memory model profile off: it goes to the disabled
        // file, where llama-server never reads it.
        pool_files("", "");
        assignments("[default]\nqwen = off\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini.disabled", &on_text, false),
            Ok(()),
            "a model profile switched off is in no render and lands"
        );
        assert_eq!(text_of("models.ini.disabled"), on_text, "the switch landed");

        // In the pool, assigned to nobody.
        pool_files(&on_text, "");
        assignments("[default]\nqwen = off\n");
        assert_eq!(
            m.write_file(&desktop, "models.ini", &on_text, false),
            Ok(()),
            "an over-memory model profile no instance loads is in no render and lands"
        );
    }

    // ── A switched-off model profile in effect: the disabled file is measured
    //
    // `assign::render` takes BOTH pool files, so a model profile sitting in
    // `models.ini.disabled` is in effect on every instance whose section of
    // `assignments.ini` says `on` for it. A save of that switched-off file
    // moves that instance's render, so the memory check judges it exactly as
    // it judges a save of `models.ini` (the model-file check of slice 9 does).
    {
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        // The write puts the over-memory model profile in effect on `default`.
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini.disabled", &on_text, false);
        assert!(
            over(
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "a save of `models.ini.disabled` that puts an over-memory model profile in effect \
             on an instance that has it `on` is refused: {answer:?}"
        );
        kept(&before);

        // The same write, sized to fit the card, lands in the same file.
        assert_eq!(
            m.write_file(&desktop, "models.ini.disabled", &fits_text, false),
            Ok(()),
            "a switched-off model profile that fits the node lands in the switched-off file"
        );
        assert_eq!(text_of("models.ini.disabled"), fits_text, "the save landed");
    }

    // ── The growth rule: a save that changes nothing of the need lands ────
    {
        pool_files(&on_text, "");
        assignments("[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        // The section on disk is already over; this save adds one key of it and
        // leaves the `model`, `ctx-size` and cache keys byte for byte.
        let edited = on_text.replace(
            &format!("ctx-size = {trained}\n"),
            &format!("ctx-size = {trained}\ntemperature = 0.5\n"),
        );
        assert_eq!(
            m.write_file(&desktop, "models.ini", &edited, false),
            Ok(()),
            "a save of another key of a model profile that is already over memory is not \
             refused: the need it leaves is today's"
        );
        assert_eq!(text_of("models.ini"), edited, "the save landed");

        // The same save again, every byte of the model profile as it stands on
        // disk: today's render has that very section, so the need it leaves is
        // today's and the write is not this profile's news. It lands, even
        // though the profile is over the card.
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &edited, false);
        assert_eq!(
            answer,
            Ok(()),
            "a save that changes no byte of a model profile that is already over memory is \
             not a need this write makes: {answer:?}"
        );
        kept(&before);

        // A save that raises `ctx-size` of that same profile is refused: the
        // need grew. The card is the mid one, which fits today's context and
        // not the trained one, so only the wider context is over it.
        let m_mid = manager_on(FakeGpus::new(&[("CUDA0", mid)]));
        let grown = format!("[qwen]\nmodel = {path}\nctx-size = 131072\n");
        pool_files(&grown, "");
        assignments("[default]\nqwen = on\n");
        let wider = format!("[qwen]\nmodel = {path}\nctx-size = {trained}\n");
        let before = snapshot(&WATCH);
        let answer = m_mid.write_file(&desktop, "models.ini", &wider, false);
        assert!(
            over(
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(mid)
            ),
            "raising `ctx-size` past what the node has is refused: {answer:?}"
        );
        kept(&before);

        // The same profile at the context the mid card has room for, from an
        // empty pool: it lands.
        pool_files("", "");
        assert_eq!(
            m_mid.write_file(&desktop, "models.ini", &grown, false),
            Ok(()),
            "the context the mid card fits, put in effect by the same write, lands"
        );
        assert_eq!(text_of("models.ini"), grown, "the save landed");
    }

    // ── A node with nothing to measure against ───────────────────────────
    {
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        // The seam answers no pool (no card, a probe that failed).
        let m = manager_on(FakeGpus::none());
        assert_eq!(
            m.write_file(&desktop, "models.ini", &on_text, false),
            Ok(()),
            "a node the GPU seam answers no pool for is not measured"
        );
        assert_eq!(text_of("models.ini"), on_text, "the save landed");

        // The seam answers a pool with no card: the same.
        pool_files("", "");
        let m = manager_on(FakeGpus::new(&[]));
        assert_eq!(
            m.write_file(&desktop, "models.ini", &on_text, false),
            Ok(()),
            "a pool with no card is not measured either"
        );
        assert_eq!(text_of("models.ini"), on_text, "the save landed");
    }

    // ── Whom the refusal names ───────────────────────────────────────────
    {
        // Two instances, the profile in effect on both: the first in id order
        // answers.
        write_record(&sys, &["default", "a-lan"]);
        std::fs::write(
            sys.join("server@a-lan.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("server@a-lan.ini");
        // The pool is empty on disk, so every model profile the write puts in
        // effect is a need this write brings, on both instances.
        pool_files("", "");
        assignments("[a-lan]\nqwen = on\n\n[default]\nqwen = on\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &on_text, false);
        assert!(
            over(
                &answer,
                "qwen",
                "a-lan",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "the instances are answered in id order: `a-lan` comes first: {answer:?}"
        );
        kept(&before);

        // Two model profiles in one render, one over the card on its own:
        // the refusal names that one profile with its OWN need, never the
        // render's total (slice 10b: `needed` is one profile's estimate).
        // `beta` needs a key of its own to be in effect on `a-lan`
        // (`assign::render` renders a model profile with no key and no `*`
        // nowhere).
        assignments("[a-lan]\nqwen = on\nbeta = on\n\n[default]\nqwen = on\n");
        let two = format!("{on_text}[beta]\nmodel = {path}\nctx-size = 1024\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &two, false);
        // 10B-PIN-1 BEGIN
        assert!(
            over(
                &answer,
                "qwen",
                "a-lan",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "the profile that is over on its own answers with its own need, not the sum of \
             the render: {answer:?}"
        );
        // 10B-PIN-1 END
        kept(&before);

        // The assignments' global `*` puts every model profile of the pool in
        // effect: the check measures that render as it measures one built of
        // explicit keys. (The switch of the whole pool is a key of the record,
        // never a `[*]` section of the pool a client writes: the client cannot
        // write `[*]` at all.)
        write_record(&sys, &["default"]);
        pool_files("", "");
        assignments("[default]\n* = inherit\n");
        let before = snapshot(&WATCH);
        let answer = m.write_file(&desktop, "models.ini", &on_text, false);
        assert!(
            over(
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "a pool in effect under the assignment's `* = inherit` is measured: {answer:?}"
        );
        kept(&before);
    }

    // ── The assignment: the render this record leaves is measured ────
    //
    // `set_model_profile` changes `assignments.ini`, not the pool: the render
    // to measure is the one the record LEAVES in effect, not the one on disk.
    // The write passes today's render as both sides, so the check sees an
    // unchanged render and an assignment that puts an over-memory profile in
    // effect lands: this is the rule slice 10b adds.
    {
        write_record(&sys, &["default"]);
        pool_files(&on_text, "");
        assignments("[default]\nqwen = off\n");
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        // 10B-PIN-2 BEGIN
        let before = snapshot(&WATCH);
        let answer = m.set_model_profile(&desktop, "default", "qwen", "on");
        assert!(
            over(
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "assigning a model profile that is in the pool and does not fit the card is \
             refused, naming the profile and the instance whose render grows: {answer:?}"
        );
        kept(&before);
        // 10B-PIN-2 END

        // The switch off lands: the profile is in effect over the card (the
        // record seeded by hand, as a GPUs swap leaves it), and switching it
        // off shrinks the render: this write adds no need.
        assignments("[default]\nqwen = on\n");
        assert_eq!(
            m.set_model_profile(&desktop, "default", "qwen", "off"),
            Ok(()),
            "assigning `off` to a model profile that is over the card lands: the render \
             shrinks, so the write is judged on no growth"
        );
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\nqwen = off\n",
            "the switch landed in the record"
        );

        // 10B-PIN-2b BEGIN. The count pin of the same assignment, self-contained:
        // its own pool, record and manager, because a count pin must make the
        // call it counts (the pin above's own copy of this call is inside its
        // block, and a proof run that deletes that block makes a count over it
        // vacuous). It accepts either answer, so it is green today, when the
        // check probes the GPUs before it judges anything: a guard that the
        // fixed write probes exactly once for a render that grows.
        pool_files(&on_text, "");
        assignments("[default]\nqwen = off\n");
        let gpus = FakeGpus::new(&[("CUDA0", small)]);
        let asked = gpus.asked.clone();
        let m_grow = manager_on(gpus);
        let _ = m_grow.set_model_profile(&desktop, "default", "qwen", "on");
        let asks = asked.lock().unwrap().clone();
        assert_eq!(
            asks,
            vec!["gpus".to_string()],
            "the assignment that puts an over-memory model profile in effect asks the GPUs \
             once, the render it leaves is judged: {asks:?}"
        );
        // 10B-PIN-2b END
    }

    // ── The need is each profile's own, never a sum over the render ──
    //
    // A second fixture sized from the same pure estimate: one profile of it
    // is about 60 % of the small card's usable memory, so two of them in one
    // render together exceed the card while each of them alone fits. The sum
    // rule refuses that save; the per-profile rule lands it.
    {
        write_record(&sys, &["default"]);
        pool_files("", "");
        assignments("[default]\nsixtya = on\nsixtyb = on\n");
        let half = models.join("half.gguf");
        let target = usable(small) * 3 / 5;
        write_gguf(&half, 262_144, &[("output.weight", target)]);
        let hs = local_shape(&half).expect("half fixture shape");
        // The KV term is the model's shape, not its bytes: taken out of the
        // target, what is left is the weight tensor (aligned to the 4096 the
        // header counts).
        let kv = full_gpu_bytes(&hs, 1024, "f16", "f16") - full_gpu_bytes(&hs, 1, "f16", "f16");
        let bytes = (target - kv) / 4096 * 4096;
        write_gguf(&half, 262_144, &[("output.weight", bytes)]);
        let hs = local_shape(&half).expect("the resized half fixture shape");
        let half_need = full_gpu_bytes(&hs, 1024, "f16", "f16");
        // 10B-PIN-3 BEGIN
        assert!(
            half_need <= usable(small) && 2 * half_need > usable(small),
            "the fixture: one profile of `half.gguf` fits the small card and two of them \
             together do not: {half_need} against {}",
            usable(small)
        );
        let half_path = half.to_string_lossy().into_owned();
        let two_fit = format!(
            "[sixtya]\nmodel = {half_path}\nctx-size = 1024\n\n[sixtyb]\nmodel = {half_path}\nctx-size = 1024\n"
        );
        let m = manager_on(FakeGpus::new(&[("CUDA0", small)]));
        let answer = m.write_file(&desktop, "models.ini", &two_fit, false);
        assert_eq!(
            answer,
            Ok(()),
            "two model profiles each at about 60 % of the card land in one write: each fits \
             on its own, the render's sum is not the rule: {answer:?}"
        );
        assert_eq!(text_of("models.ini"), two_fit, "the save landed");
        // 10B-PIN-3 END
    }

    // ── The GPUs are asked only when a section grew ──────────────────
    //
    // The estimate comes from the files, the GPUs from a probe: a write that
    // grows no profile never runs the probe (slice 10b, "The GPUs"). The
    // count is the seam's own log (`FakeGpus::asked`), one per manager.
    {
        write_record(&sys, &["default"]);
        pool_files(&on_text, "");
        assignments("[default]\nqwen = on\n");

        // A save of an unrelated key of a profile that stays as it is: the
        // profile is over the card, the need does not grow, so the probe is
        // not asked at all.
        let gpus = FakeGpus::new(&[("CUDA0", small)]);
        let asked = gpus.asked.clone();
        let m = manager_on(gpus);
        let edited = on_text.replace(
            &format!("ctx-size = {trained}\n"),
            &format!("ctx-size = {trained}\ntemperature = 0.5\n"),
        );
        assert_eq!(
            m.write_file(&desktop, "models.ini", &edited, false),
            Ok(()),
            "fixture: the unchanged-over profile lands (the count below is the pin)"
        );
        // 10B-PIN-4 BEGIN
        let asks = asked.lock().unwrap().clone();
        assert!(
            asks.is_empty(),
            "a save of an unrelated key of an unchanged profile asks the GPUs 0 times: {asks:?}"
        );
        // 10B-PIN-4 END

        // The switch off: the render shrinks, no section grew, so the probe
        // is not asked at all.
        let gpus = FakeGpus::new(&[("CUDA0", small)]);
        let asked = gpus.asked.clone();
        let m = manager_on(gpus);
        assert_eq!(
            m.set_model_profile(&desktop, "default", "qwen", "off"),
            Ok(()),
            "fixture: the switch off lands (the count below is the pin)"
        );
        // 10B-PIN-5 BEGIN
        let asks = asked.lock().unwrap().clone();
        assert!(
            asks.is_empty(),
            "a switch off asks the GPUs 0 times: {asks:?}"
        );
        // 10B-PIN-5 END

        // A save that grows a profile: the probe is asked, once. Green today
        // (the check asks the seam first whatever it judges): a guard.
        pool_files("", "");
        assignments("[default]\nqwen = on\n");
        let gpus = FakeGpus::new(&[("CUDA0", small)]);
        let asked = gpus.asked.clone();
        let m = manager_on(gpus);
        let answer = m.write_file(&desktop, "models.ini", &on_text, false);
        assert!(
            over(
                &answer,
                "qwen",
                "default",
                need(trained, "f16", "f16"),
                usable(small)
            ),
            "fixture: the growing save is refused (the count below is the pin): {answer:?}"
        );
        // 10B-PIN-6 BEGIN
        let asks = asked.lock().unwrap().clone();
        assert_eq!(
            asks,
            vec!["gpus".to_string()],
            "a save that grows a profile asks the GPUs once"
        );
        // 10B-PIN-6 END
    }
}
/// The tags of an instance, the two lists apart: the `Tags` key of the
/// instance's own section of `instances.ini`, and the `Tags` key of the server
/// profile its section names. `Instances` answers both, beside the id, the
/// engine and the port, to any caller as the list of ids is; `SetInstanceTags`
/// replaces the record's own list, guarded as a write of a served file is:
/// `configure` first, the `As` form answered only for the bff's account and a
/// current member of `pai`, and one record of the action log per call, done or
/// refused. A list the rule refuses (`server-bad-tags`) answers before the
/// record is read and writes nothing, and so does an id that is no instance.
/// The lists are metadata: the manager passes none of them to the engine.
#[cfg(test)]
pub(crate) fn manager_instance_tags_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let core = |allow: bool, passwords: FakePasswords, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(log)
    };
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let keys_of = |text: &str, id: &str, key: &str| -> Vec<String> {
        section(text, id)
            .lines()
            .filter(|line| line.trim_start().starts_with(&format!("{key} ")))
            .map(|line| line.trim_end().to_string())
            .collect()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();
    let tags = |list: &[&str]| -> Vec<String> { list.iter().map(|t| t.to_string()).collect() };
    let refused = |reason: &str, list: &str| -> Result<(), ManagerError> {
        Err(ManagerError::Invalid(pai_core::checks::Refusal {
            key: "server-bad-tags",
            values: vec![
                reason.to_string(),
                list.to_string(),
                pai_core::checks::TAGS_MAX.to_string(),
                pai_core::checks::TAG_MIN.to_string(),
                pai_core::checks::TAG_MAX.to_string(),
            ],
        }))
    };
    // The record: `default`'s profile carries the tags, `gpu-1` carries a list
    // of its own, hand-spaced, beside a comment and a key of its own, and
    // `lan-2` points at `default`'s profile: the row must carry that profile's
    // port and tags, never the ones of the file named after the instance.
    const SEEDED: &str = "# the manager's own record\n[default]\nServerProfile = default\n\n[gpu-1]\nServerProfile = gpu-1\nTags = gpu , edge \n; a note\nnote = keep me\n\n[lan-2]\nServerProfile = default\nTags = lan\n";
    std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\nTags = edge,v2\n",
    )
    .expect("default profile");
    std::fs::write(
        sys.join("server@gpu-1.ini"),
        "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
    )
    .expect("gpu-1 profile");
    // A file named after `lan-2`, holding a port and tags no instance runs:
    // a render of a profile the launch never reads, and no row of `Instances`.
    std::fs::write(
        sys.join("server@lan-2.ini"),
        "[Server]\nPort = 8099\nHostname = 127.0.0.1\nTags = unused,file\n",
    )
    .expect("lan-2 render");

    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let rows = m.instances();
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["default", "gpu-1", "lan-2"],
            "the record's order, every section of it"
        );
        assert_eq!(rows[0].engine, "llama.cpp");
        assert_eq!(
            rows[0].server_profile.as_deref(),
            Some("default"),
            "the server profile the section names, the name of the file the launch reads"
        );
        assert_eq!(
            rows[0].port, 8080,
            "the port of the profile the section names"
        );
        assert_eq!(
            rows[0].profile_tags,
            tags(&["edge", "v2"]),
            "the profile's list, in the order it was written"
        );
        assert_eq!(
            rows[0].instance_tags,
            Vec::<String>::new(),
            "a section with no Tags key"
        );
        assert_eq!(
            rows[1].profile_tags,
            Vec::<String>::new(),
            "a profile with no Tags key"
        );
        assert_eq!(
            rows[1].instance_tags,
            tags(&["gpu", "edge"]),
            "the hand-spaced list, trimmed, in order"
        );
        assert_eq!(rows[1].port, 8081);
        assert_eq!(
            rows[1].server_profile.as_deref(),
            Some("gpu-1"),
            "the profile of its own section"
        );
        assert_eq!(
            rows[2].id, "lan-2",
            "the record's order, every section of it"
        );
        assert_eq!(
            rows[2].server_profile.as_deref(),
            Some("default"),
            "the name the section names, which is not the instance's id: the \
             client that reads the server file of lan-2 reads server@default.ini, \
             and server@lan-2.ini is a file no instance runs"
        );
        assert_eq!(
            rows[2].port, 8080,
            "the port of the profile the section names, not the one of the file named after the instance"
        );
        assert_eq!(
            rows[2].profile_tags,
            tags(&["edge", "v2"]),
            "the tags of that profile, not the ones of the file named after the instance"
        );
        assert_eq!(
            rows[2].instance_tags,
            tags(&["lan"]),
            "the section's own list beside the profile's"
        );
    }

    {
        // A section naming a profile whose file is not there: the read of a
        // file that is not there is no tags and no port, not a failure that
        // drops the whole list, and never the default port: 0 is what the web
        // UI shows as "no port".
        std::fs::write(
            sys.join(INSTANCES_FILE),
            format!("{SEEDED}\n[lan-3]\nServerProfile = gone\n"),
        )
        .expect(INSTANCES_FILE);
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let rows = m.instances();
        assert_eq!(rows.len(), 4, "the section of a missing profile is listed");
        assert_eq!(rows[3].id, "lan-3");
        assert_eq!(
            rows[3].server_profile.as_deref(),
            Some("gone"),
            "the name is the section's, and a missing file does not hide it: the \
             client is told where to write, and the file appears when it does"
        );
        assert_eq!(rows[3].profile_tags, Vec::<String>::new());
        assert_eq!(rows[3].port, 0, "no profile file is no port");
    }

    {
        // A section naming no profile at all: no name to give, answered as no
        // profile, not as a name invented from the id.
        std::fs::write(sys.join(INSTANCES_FILE), "[bare]\nPort = 1\n").expect(INSTANCES_FILE);
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let rows = m.instances();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "bare");
        assert_eq!(
            rows[0].server_profile, None,
            "a section naming no profile names no file"
        );
        assert_eq!(rows[0].port, 0, "no profile is no port");
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        let before = listing();

        assert_eq!(
            m.set_instance_tags(&desktop, "default", &tags(&["edge", "a-1"])),
            Ok(()),
            "a list of the rule's own shape"
        );
        let written = text_of(INSTANCES_FILE);
        assert_eq!(
            keys_of(&written, "default", "Tags"),
            vec!["Tags = edge,a-1".to_string()],
            "one key, the list joined with a comma"
        );
        assert_eq!(
            section(&written, "gpu-1"),
            section(SEEDED, "gpu-1"),
            "the section of another instance, its comment and its own key, as they were: {written:?}"
        );
        assert_eq!(
            m.instances()[0].instance_tags,
            tags(&["edge", "a-1"]),
            "the write reads back, in order"
        );

        assert_eq!(
            m.set_instance_tags(&desktop, "default", &tags(&["x"])),
            Ok(())
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "Tags"),
            vec!["Tags = x".to_string()],
            "one key after two writes, not two"
        );

        // The empty list clears the record's tags: the key stays, with nothing
        // in it, and reads back as no tags.
        assert_eq!(m.set_instance_tags(&desktop, "default", &[]), Ok(()));
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "Tags"),
            vec!["Tags =".to_string()],
            "an empty list keeps the key, empty"
        );
        assert_eq!(
            m.instances()[0].instance_tags,
            Vec::<String>::new(),
            "the empty key is no tags"
        );

        // The limits are inclusive: 8 tags, each 32.
        let at_limits: Vec<String> = (0..8).map(|i| format!("{i}{}", "t".repeat(31))).collect();
        assert_eq!(
            m.set_instance_tags(&desktop, "default", &at_limits),
            Ok(()),
            "the two limits at once"
        );
        assert_eq!(m.instances()[0].instance_tags, at_limits);
        assert_eq!(
            listing(),
            before,
            "the verb writes the record and no file of its own"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_instance_tags" target="default" caller=":1.42" principal=- outcome=ok"#
                    .to_string();
                4
            ],
            "one record per call: the verb's action, the instance as its target, the caller, no principal as a bare -, ok"
        );
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        let seed = text_of(INSTANCES_FILE);
        let before = listing();

        assert_eq!(
            m.set_instance_tags(&desktop, "lan-3", &tags(&["gpu"])),
            Err(ManagerError::NoInstance),
            "an id that is no section of the record"
        );
        for id in ["", "../x", "Default", "a_b"] {
            assert_eq!(
                m.set_instance_tags(&desktop, id, &tags(&["gpu"])),
                Err(ManagerError::InvalidId),
                "not an instance id at all: {id:?}"
            );
        }

        let long = "t".repeat(33);
        let many: Vec<String> = (0..9).map(|i| format!("t{i}")).collect();
        for (list, reason) in [
            (tags(&[""]), "empty tag"),
            (tags(&[&long]), "too long"),
            (tags(&["Edge"]), "bad characters"),
            (tags(&["a b"]), "bad characters"),
            (tags(&["ok", "ok"]), "the same tag twice"),
            (many, "too many tags"),
        ] {
            let joined = list.join(",");
            assert_eq!(
                m.set_instance_tags(&desktop, "default", &list),
                refused(reason, &joined),
                "{list:?}"
            );
        }
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "every refusal above wrote nothing"
        );
        assert_eq!(listing(), before, "and created no file");
        let mut want = vec![
            r#"action="set_instance_tags" target="lan-3" caller=":1.42" principal=- outcome="NoInstance""#.to_string(),
            r#"action="set_instance_tags" target="" caller=":1.42" principal=- outcome="InvalidId""#.to_string(),
            r#"action="set_instance_tags" target="../x" caller=":1.42" principal=- outcome="InvalidId""#.to_string(),
            r#"action="set_instance_tags" target="Default" caller=":1.42" principal=- outcome="InvalidId""#.to_string(),
            r#"action="set_instance_tags" target="a_b" caller=":1.42" principal=- outcome="InvalidId""#.to_string(),
        ];
        want.extend(vec![
            r#"action="set_instance_tags" target="default" caller=":1.42" principal=- outcome="Invalid""#.to_string();
            6
        ]);
        assert_eq!(
            records(&lines),
            want,
            "one record per refused call, the refusals of the rule all as Invalid"
        );
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, FakePasswords::new(), log);
        let seed = text_of(INSTANCES_FILE);
        assert_eq!(
            m.set_instance_tags(&desktop, "default", &tags(&["gpu"])),
            Err(ManagerError::NotAuthorized),
            "a caller the authority refuses"
        );
        assert_eq!(text_of(INSTANCES_FILE), seed, "the refusal wrote nothing");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_instance_tags" target="default" caller=":1.42" principal=- outcome=refused"#
                    .to_string()
            ],
            "the guard's refusal is recorded"
        );

        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        assert_eq!(
            m.set_instance_tags(&member, "default", &tags(&["gpu"])),
            Ok(()),
            "the bff's account, for a member of `pai`"
        );
        assert_eq!(
            m.set_instance_tags(&outsider, "default", &tags(&["gpu"])),
            Err(ManagerError::NotAuthorized),
            "the bff's account, for a caller outside `pai`"
        );
        let vouching = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.set_instance_tags(&vouching, "default", &tags(&["gpu"])),
            Err(ManagerError::NotAuthorized),
            "a caller that is not the bff vouches for nobody"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "Tags"),
            vec!["Tags = gpu".to_string()],
            "only the accepted call wrote"
        );
    }
}
/// Creating and deleting the node's server profiles, the files beside the
/// instances (ARCHITECTURE.md, slice 14). `CreateServerProfile(engine, profile,
/// port)` writes `server@<profile>.ini` the way a save writes it (`render_for`
/// with the directories left unset, mode 0640, group [`INSTANCE_GROUP`], port 0
/// no port), and overwrites nothing: any entry already at that name, a dangling
/// link included (the check is `symlink_metadata`), is the new
/// `ServerProfileExists`. `DeleteServerProfile` removes a file no section of
/// `instances.ini` names, refuses one a section names (`ServerProfileInUse`,
/// naming it) and one that is no file (`NoServerProfile`, a name the id rule
/// refuses included), and answers the record read's own `Io` rather than
/// "unused" when `instances.ini` cannot be read. `ServerProfiles` lists the
/// files, sorted, to any caller as `Instances` does: id, engine, port, tags and
/// the first section naming it, and nothing of the rest of the file. Both verbs
/// are guarded as a write of a served file is: `configure` first, the `As` form
/// answered only for the bff's account and a current member of `pai`, and one
/// record of the action log per call, done or refused.
#[cfg(test)]
pub(crate) fn manager_server_profile_crud_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let core_in = |allow: bool, group: FakeGroup, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(group)
        .with_passwords(FakePasswords::new())
        .with_log(log)
    };
    let core = |allow: bool, log: FakeActionLog| core_in(allow, FakeGroup::new(), log);
    let profile_path = |name: &str| sys.join(format!("server@{name}.ini"));
    let profile = |name: &str, text: &str| {
        std::fs::write(profile_path(name), text).expect("server profile");
    };
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let bytes_of = |name: &str| std::fs::read(sys.join(name)).expect("bytes");
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let keys_of = |text: &str, id: &str, key: &str| -> Vec<String> {
        section(text, id)
            .lines()
            .filter(|line| line.trim_start().starts_with(&format!("{key} ")))
            .map(|line| line.trim_end().to_string())
            .collect()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();
    let tags = |list: &[&str]| -> Vec<String> { list.iter().map(|t| t.to_string()).collect() };
    // The file a create is meant to write: the same render a save of a server
    // config with that port writes, its directories left unset.
    let render = |port: Option<i32>| -> String {
        pai_core::server_cfg::render_for(
            &pai_core::server_cfg::ServerConfig {
                port,
                ..Default::default()
            },
            true,
        )
    };
    let exists = |name: &str| -> Result<(), ManagerError> {
        Err(ManagerError::ServerProfileExists(name.to_string()))
    };
    let no_profile = |name: &str| -> Result<(), ManagerError> {
        Err(ManagerError::NoServerProfile(name.to_string()))
    };
    let in_use = |name: &str, id: &str| -> Result<(), ManagerError> {
        Err(ManagerError::ServerProfileInUse(
            name.to_string(),
            id.to_string(),
        ))
    };
    let refused = |key: &'static str, value: &str| -> Result<(), ManagerError> {
        Err(ManagerError::Invalid(pai_core::checks::Refusal {
            key,
            values: vec![value.to_string()],
        }))
    };

    // The record: `default` and `lan-2` name a profile of their own, `lan-3`
    // names a file that is not there (a section is no row of the list of files).
    const SEEDED: &str = "# the manager's own record\n[default]\nServerProfile = default\n; a note\nnote = keep me\n\n[lan-2]\nServerProfile = lan-2\n\n[lan-3]\nServerProfile = gone\n";
    std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
    profile(
        "default",
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\nTags = edge,v2\n",
    );
    profile("lan-2", "[Server]\nPort = 8081\nHostname = 127.0.0.1\n");
    profile("noport", "[Server]\nHostname = 127.0.0.1\n");
    profile(
        "secret",
        "[Server]\nPort = 8090\nOpencodeApiKey = sk-local-3f9a\n",
    );
    // A file whose name the engine refuses, and a file that is no profile at
    // all: neither is a row of the list.
    std::fs::write(sys.join("server@X.ini"), "[Server]\nPort = 1\n").expect("bad name");
    std::fs::write(sys.join("stray.ini"), "[Server]\nPort = 2\n").expect("stray");

    // ── The list ──────────────────────────────────────────────────────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        let rows = m.server_profiles();
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["default", "lan-2", "noport", "secret"],
            "the files whose name is a profile, sorted: `gone` is a section with \
             no file, `server@X.ini` a name the engine refuses, `stray.ini` no profile"
        );
        assert_eq!(
            rows[0].engine, "llama.cpp",
            "the one engine the manager serves"
        );
        assert_eq!(rows[0].port, 8080);
        assert_eq!(
            rows[0].tags,
            tags(&["edge", "v2"]),
            "the profile's own tags"
        );
        assert_eq!(
            rows[0].instance.as_deref(),
            Some("default"),
            "the section that names it"
        );
        assert_eq!(
            rows[1].tags,
            Vec::<String>::new(),
            "a file with no Tags key"
        );
        assert_eq!(
            rows[2].port,
            u16::try_from(pai_core::server_cfg::DEFAULT_PORT).expect("DEFAULT_PORT"),
            "a file with no Port is the default port, as Instances reads it"
        );
        assert_eq!(
            rows[2].instance, None,
            "a profile no section names serves no instance"
        );
        let line = format!("{rows:?}");
        assert!(
            !line.contains("sk-local-3f9a"),
            "the row carries the port and the tags, and nothing else of the file: {line}"
        );
        assert_eq!(
            records(&lines),
            Vec::<String>::new(),
            "the list is a read: no action record"
        );

        let m = core(false, FakeActionLog::new());
        assert_eq!(
            m.server_profiles().len(),
            4,
            "a caller the authority refuses reads the list as any other: the list is \
             the class of data Instances answers to anyone"
        );
    }

    // ── A create writes the file a save writes ─────────────────────────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let group = FakeGroup::new();
        let calls = group.calls.clone();
        let m = core_in(true, group, log);
        let before = listing();
        let seed = text_of(INSTANCES_FILE);
        let default_bytes = bytes_of("server@default.ini");

        assert_eq!(
            m.create_server_profile(&desktop, "llama.cpp", "big", Some(8123)),
            Ok(()),
            "a name that is nothing yet, of the engine the manager serves"
        );
        let written = text_of("server@big.ini");
        assert_eq!(
            written,
            render(Some(8123)),
            "the file a save of a server config with that port writes, directories \
             left unset, and nothing beside it"
        );
        assert_eq!(
            keys_of(&written, "Server", "Port"),
            vec!["Port = 8123".to_string()],
            "the port of the call, as one key"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "a create writes no instance record: the file serves nothing until a section names it"
        );
        let mut want = before.clone();
        want.push("server@big.ini".to_string());
        want.sort();
        assert_eq!(listing(), want, "one new file, and no temp file left");
        let g = calls.lock().unwrap().clone();
        assert_eq!(g.len(), 1, "the group set once, on the way in");
        assert_eq!(g[0].group, INSTANCE_GROUP, "the group of a served file");
        assert_eq!(
            g[0].path.file_name().and_then(|n| n.to_str()),
            Some("server@big.ini.tmp"),
            "the group is set on the temp file, before the rename"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(profile_path("big"))
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o7777,
                0o640,
                "the mode of a served profile"
            );
            assert_eq!(
                g[0].mode, 0o640,
                "the temp file had the mode when its group was set: the key of a \
                 profile is never world readable"
            );
        }
        assert_eq!(
            m.server_profiles()
                .iter()
                .find(|r| r.id == "big")
                .map(|r| (r.port, r.instance.clone())),
            Some((8123u16, None)),
            "the new file is a row, and an unused one"
        );

        assert_eq!(
            m.create_server_profile(&desktop, "llama.cpp", "zero", None),
            Ok(()),
            "port 0 on the wire is no port"
        );
        assert_eq!(
            text_of("server@zero.ini"),
            render(None),
            "the file of no port"
        );
        assert_eq!(
            keys_of(&text_of("server@zero.ini"), "Server", "Port"),
            Vec::<String>::new(),
            "no Port key at all"
        );

        // The never-overwrite: a name that is anything already is refused.
        assert_eq!(
            m.create_server_profile(&desktop, "llama.cpp", "big", Some(9001)),
            exists("big"),
            "a second create of the same name"
        );
        assert_eq!(
            m.create_server_profile(&desktop, "llama.cpp", "default", Some(9001)),
            exists("default"),
            "a name that is an existing profile with other bytes"
        );
        assert_eq!(
            bytes_of("server@default.ini"),
            default_bytes,
            "the bytes of the file that was there stay"
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(sys.join("nowhere.ini"), profile_path("ghost"))
                .expect("symlink");
            assert_eq!(
                m.create_server_profile(&desktop, "llama.cpp", "ghost", Some(8123)),
                exists("ghost"),
                "a dangling link is an entry: the create would write through it"
            );
            assert!(
                !sys.join("nowhere.ini").exists(),
                "and the link's target was not created"
            );
            assert!(
                std::fs::symlink_metadata(profile_path("ghost"))
                    .expect("symlink_metadata")
                    .file_type()
                    .is_symlink(),
                "the link itself is still a link"
            );
        }

        // The `mut` is only used on unix, where the dangling link's record is
        // pushed.
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut want = vec![
            r#"action="create_server_profile" target="big" caller=":1.42" principal=- outcome=ok"#
                .to_string(),
            r#"action="create_server_profile" target="zero" caller=":1.42" principal=- outcome=ok"#
                .to_string(),
            r#"action="create_server_profile" target="big" caller=":1.42" principal=- outcome="ServerProfileExists""#.to_string(),
            r#"action="create_server_profile" target="default" caller=":1.42" principal=- outcome="ServerProfileExists""#.to_string(),
        ];
        #[cfg(unix)]
        want.push(
            r#"action="create_server_profile" target="ghost" caller=":1.42" principal=- outcome="ServerProfileExists""#
                .to_string(),
        );
        assert_eq!(
            records(&lines),
            want,
            "one record per call, the profile as its target"
        );
    }

    // ── A create the rules refuses writes nothing, anywhere ───────────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        let before = listing();
        let seed = text_of(INSTANCES_FILE);
        let long = "t".repeat(33);
        for (engine, name) in [
            ("vllm", "fine-name"),
            ("llama.cpp", "../x"),
            ("llama.cpp", "X"),
            ("llama.cpp", "a_b"),
            ("llama.cpp", ""),
            ("llama.cpp", long.as_str()),
        ] {
            let want = if engine != "llama.cpp" {
                refused("unknown-engine", engine)
            } else {
                refused("server-profile-bad-id", name)
            };
            assert_eq!(
                m.create_server_profile(&desktop, engine, name, Some(8123)),
                want,
                "create of engine {engine:?}, profile {name:?}"
            );
        }
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "a refused create writes no record"
        );
        assert_eq!(listing(), before, "and creates nothing, anywhere");
        assert!(
            !sys.join("x.ini").exists() && !dir.path().join("x.ini").exists(),
            "the name rule holds: `../x` builds no path outside the engine's directory"
        );
        let want = vec![
            r#"action="create_server_profile" target="fine-name" caller=":1.42" principal=- outcome="Invalid""#.to_string(),
            r#"action="create_server_profile" target="../x" caller=":1.42" principal=- outcome="Invalid""#.to_string(),
            r#"action="create_server_profile" target="X" caller=":1.42" principal=- outcome="Invalid""#.to_string(),
            r#"action="create_server_profile" target="a_b" caller=":1.42" principal=- outcome="Invalid""#.to_string(),
            r#"action="create_server_profile" target="" caller=":1.42" principal=- outcome="Invalid""#.to_string(),
            format!(
                r#"action="create_server_profile" target="{long}" caller=":1.42" principal=- outcome="Invalid""#
            ),
        ];
        assert_eq!(
            records(&lines),
            want,
            "one record per refused call, both rules as Invalid"
        );
    }

    // ── A delete: unused goes, in use stays ───────────────────────────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        let default_bytes = bytes_of("server@default.ini");

        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "default"),
            in_use("default", "default"),
            "the instance whose section names it"
        );
        assert_eq!(
            bytes_of("server@default.ini"),
            default_bytes,
            "the file of an in-use profile stays, byte for byte"
        );
        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "lan-2"),
            in_use("lan-2", "lan-2"),
            "the same for the other instance"
        );
        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "noport"),
            Ok(()),
            "a profile no section names"
        );
        assert!(!profile_path("noport").exists(), "its file is gone");
        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "gone"),
            no_profile("gone"),
            "a section naming a file that is not there: the refusal is about the file"
        );
        for name in ["nope", "../x", "X", "a_b", ""] {
            assert_eq!(
                m.delete_server_profile(&desktop, "llama.cpp", name),
                no_profile(name),
                "a name with no file, and a name the rule refuses, answer the same: {name:?}"
            );
        }
        assert!(
            profile_path("X").exists(),
            "a name the rule refuses is refused before the file is looked at"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            SEEDED,
            "a delete writes no record: a section is left naming a file, never the other way"
        );
        let mut want = vec![
            r#"action="delete_server_profile" target="default" caller=":1.42" principal=- outcome="ServerProfileInUse""#.to_string(),
            r#"action="delete_server_profile" target="lan-2" caller=":1.42" principal=- outcome="ServerProfileInUse""#.to_string(),
            r#"action="delete_server_profile" target="noport" caller=":1.42" principal=- outcome=ok"#
                .to_string(),
            r#"action="delete_server_profile" target="gone" caller=":1.42" principal=- outcome="NoServerProfile""#.to_string(),
        ];
        want.extend(
            ["nope", "../x", "X", "a_b", ""]
                .iter()
                .map(|name| {
                    format!(
                        r#"action="delete_server_profile" target="{name}" caller=":1.42" principal=- outcome="NoServerProfile""#
                    )
                }),
        );
        assert_eq!(records(&lines), want, "one record per call");
    }

    // ── A profile two sections name: the first of them ────────────────────
    {
        std::fs::write(
            sys.join(INSTANCES_FILE),
            "[default]\nServerProfile = shared\n\n[lan-3]\nServerProfile = shared\n",
        )
        .expect(INSTANCES_FILE);
        profile("shared", "[Server]\nPort = 8091\nHostname = 127.0.0.1\n");
        let m = core(true, FakeActionLog::new());
        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "shared"),
            in_use("shared", "default"),
            "refused for the first section that names it"
        );
        assert_eq!(
            m.server_profiles()
                .iter()
                .find(|r| r.id == "shared")
                .and_then(|r| r.instance.clone()),
            Some("default".to_string()),
            "and the list names that same section"
        );
    }

    // ── A record the manager cannot read is never answered "unused" ───────
    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        profile("keepme", "[Server]\nPort = 8092\nHostname = 127.0.0.1\n");
        let kept = bytes_of("server@keepme.ini");
        // Bytes that are no text: the read of the record fails, and "no section
        // names it" would delete a profile a running instance reads.
        std::fs::write(
            sys.join(INSTANCES_FILE),
            b"[default]\nServerProfile = default\n\xed\xa0\xbf\n",
        )
        .expect("broken record");
        let m = core(true, FakeActionLog::new());
        let e = m.delete_server_profile(&desktop, "llama.cpp", "keepme");
        assert!(
            matches!(e, Err(ManagerError::Io(_))),
            "the read's own failure, not \"unused\": {e:?}"
        );
        assert_eq!(bytes_of("server@keepme.ini"), kept, "the file stays");

        std::fs::remove_file(sys.join(INSTANCES_FILE)).expect("no record");
        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "keepme"),
            Ok(()),
            "a record that is not there is no section naming it: the profile is unused"
        );
        assert!(!profile_path("keepme").exists(), "and its file goes");
    }

    // ── A caller without configure, and the `As` form ─────────────────────
    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        profile("spare", "[Server]\nPort = 8093\nHostname = 127.0.0.1\n");
        let spare = bytes_of("server@spare.ini");
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, log);
        let before = listing();
        assert_eq!(
            m.create_server_profile(&desktop, "llama.cpp", "new-one", Some(8123)),
            Err(ManagerError::NotAuthorized),
            "a caller the authority refuses for configure"
        );
        assert_eq!(
            m.delete_server_profile(&desktop, "llama.cpp", "spare"),
            Err(ManagerError::NotAuthorized),
            "the same for a delete"
        );
        // The rows, by name, never a count: this phase's own blocks put files
        // in the tree, so any absolute number reds on a row a block adds. What
        // is pinned is the set of names this phase leaves: the profiles it
        // seeded and kept, the ones its creates wrote (`big`, `zero`, `shared`,
        // `spare`), `ghost` the dangling link (the list is of entries, as the
        // create's `symlink_metadata` check is), and `noport` absent, the
        // profile whose delete landed.
        let rows = m.server_profiles();
        let ids = rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>();
        let mut want = vec![
            "big", "default", "lan-2", "secret", "shared", "spare", "zero",
        ];
        #[cfg(unix)]
        want.insert(2, "ghost");
        assert_eq!(
            ids, want,
            "the list is answered to a caller the authority refuses for configure, in \
             full: it is a read, the gate does not shrink it"
        );
        assert!(
            ids.contains(&"spare"),
            "the profile this block wrote is one of its rows"
        );
        assert!(
            !ids.contains(&"noport"),
            "the profile an earlier block deleted is not"
        );
        assert_eq!(
            rows,
            core(true, FakeActionLog::new()).server_profiles(),
            "the same rows an authorized caller sees: the list is the class of data \
             Instances answers to anyone, and a fresh log records no read"
        );
        assert_eq!(listing(), before, "a refused caller creates nothing");
        assert_eq!(bytes_of("server@spare.ini"), spare, "and removes nothing");
        assert_eq!(text_of(INSTANCES_FILE), SEEDED, "and writes no record");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="create_server_profile" target="new-one" caller=":1.42" principal=- outcome=refused"#.to_string(),
                r#"action="delete_server_profile" target="spare" caller=":1.42" principal=- outcome=refused"#.to_string(),
            ],
            "the guard's refusals are recorded, the read of the list is not"
        );
    }
    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, log);
        let vouching = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.create_server_profile(&member, "llama.cpp", "alice-p", Some(8123)),
            Ok(()),
            "the bff's account, for a member of `pai`"
        );
        assert_eq!(
            m.delete_server_profile(&member, "llama.cpp", "alice-p"),
            Ok(()),
            "and the member's delete of it"
        );
        assert_eq!(
            m.create_server_profile(&outsider, "llama.cpp", "bob-p", Some(8123)),
            Err(ManagerError::NotAuthorized),
            "the bff's account, for a caller outside `pai`"
        );
        assert_eq!(
            m.delete_server_profile(&outsider, "llama.cpp", "noport"),
            Err(ManagerError::NotAuthorized),
            "the same for a delete"
        );
        assert_eq!(
            m.create_server_profile(&vouching, "llama.cpp", "x", Some(8123)),
            Err(ManagerError::NotAuthorized),
            "a caller that is not the bff vouches for nobody"
        );
        assert_eq!(
            m.delete_server_profile(&vouching, "llama.cpp", "lan-2"),
            Err(ManagerError::NotAuthorized),
            "and cannot delete through one either"
        );
        assert!(
            !profile_path("bob-p").exists() && !profile_path("x").exists(),
            "the refused `As` calls created nothing"
        );
        // The tree's own profiles survive the refusals, pinned by name, never
        // by a count of the directory (this phase's blocks write and remove
        // files): `lan-2` is the target of the refused vouching delete, an
        // in-use profile of the seeded record; `spare` is a profile the tree
        // holds that no refused call names.
        assert!(
            profile_path("spare").exists() && profile_path("lan-2").exists(),
            "and removed nothing"
        );
        assert!(
            !profile_path("alice-p").exists(),
            "the two accepted calls did: the create, then the delete"
        );
        let got = records(&lines);
        assert_eq!(got.len(), 6, "one record per call");
        for (line, want) in [
            (
                got[0].as_str(),
                r#"action="create_server_profile" target="alice-p" caller=":1.9""#,
            ),
            (
                got[1].as_str(),
                r#"action="delete_server_profile" target="alice-p" caller=":1.9""#,
            ),
            (
                got[2].as_str(),
                r#"action="create_server_profile" target="bob-p" caller=":1.9""#,
            ),
            (
                got[3].as_str(),
                r#"action="delete_server_profile" target="noport" caller=":1.9""#,
            ),
            (
                got[4].as_str(),
                r#"action="create_server_profile" target="x" caller=":1.42""#,
            ),
            (
                got[5].as_str(),
                r#"action="delete_server_profile" target="lan-2" caller=":1.42""#,
            ),
        ] {
            assert!(line.starts_with(want), "{line} is not the record of {want}");
        }
        assert_eq!(
            got.iter()
                .map(|l| l.rsplit_once("outcome=").expect("outcome").1.to_string())
                .collect::<Vec<_>>(),
            vec!["ok", "ok", "refused", "refused", "refused", "refused"],
            "the accepted calls ok, every `As` refusal recorded as refused"
        );
    }
}
/// the node's server profiles (protocol 3, its `As` form and the same name over
/// the pipe). It writes one key, the `ServerProfile` of the instance's section
/// of `instances.ini`, and the instance's next launch reads the profile the key
/// names. The profile a switch aims at is unused at that moment, so the port
/// check of a profile save (`check_address`, which judges a profile no instance
/// runs against nothing) judges nothing here: the switch judges the target's
/// address against every other instance's profile and against the web service's,
/// leaving out the profile the switching instance runs now. A call answers
/// before it writes: an id that is no instance (`InvalidId`, `NoInstance`), a
/// profile the engine has none of (`NoServerProfile`, naming it), a profile
/// another instance runs (`ServerProfileInUse`, naming the profile and that
/// instance), an address the switch would take (`AddressInUse`, naming the
/// instance, or the web service as itself) and a `ModelsMax` under the loads the
/// instance's render starts (`load-on-startup-over-max`, naming the instance)
/// each write nothing, and a file that cannot be read is answered its own kind
/// rather than as an empty one. Guarded as a write of a served file is:
/// `configure` first, the `As` form answered only for the bff's account and a
/// current member of `pai`, and one record of the action log per call, done or
/// refused.
#[cfg(test)]
pub(crate) fn manager_set_server_profile_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let framework = crate::paths::system_config_dir();
    std::fs::create_dir_all(&framework).expect("framework config dir");

    // The bff's account, its request for alice (a member of `pai` in
    // [`FakePasswords`]) and for bob (who is not), and a desktop caller of its
    // own.
    let bff = Caller(":1.9".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());
    let core = |allow: bool, passwords: FakePasswords, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(log)
    };
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let bytes_of = |name: &str| std::fs::read(sys.join(name)).expect("bytes");
    let section =
        |text: &str, id: &str| pai_core::ini::text::section_text(text, id).unwrap_or_default();
    let keys_of = |text: &str, id: &str, key: &str| -> Vec<String> {
        section(text, id)
            .lines()
            .filter(|line| line.trim_start().starts_with(&format!("{key} ")))
            .map(|line| line.to_string())
            .collect()
    };
    let listing = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();
    let profile = |name: &str, text: &str| {
        std::fs::write(sys.join(format!("server@{name}.ini")), text).expect("server profile");
    };
    let no_profile = |name: &str| -> Result<(), ManagerError> {
        Err(ManagerError::NoServerProfile(name.to_string()))
    };
    let in_use = |name: &str, id: &str| -> Result<(), ManagerError> {
        Err(ManagerError::ServerProfileInUse(
            name.to_string(),
            id.to_string(),
        ))
    };
    let taken = |who: &str| -> Result<(), ManagerError> {
        Err(ManagerError::AddressInUse(who.to_string()))
    };
    let over_max = |count: &str, max: &str, id: &str| -> Result<(), ManagerError> {
        Err(ManagerError::Invalid(pai_core::checks::Refusal {
            key: "load-on-startup-over-max",
            values: vec![count.to_string(), max.to_string(), id.to_string()],
        }))
    };
    // The model profile renders of the two instances: a server profile switch
    // writes no model profile text, so these bytes must not move.
    let renders = || -> Vec<(String, String)> {
        ["models@default.ini", "models@lan-2.ini"]
            .iter()
            .map(|name| (name.to_string(), text_of(name)))
            .collect()
    };
    // The instance's launch, read after a switch: `flag` is the value of a flag
    // in its argument list, `None` when the launch carries none.
    let exe = dir
        .path()
        .join("bin")
        .join(crate::paths::binary_name("llama-server"));
    let launch = |id: &str| {
        crate::launch::instance_command(id, &exe).expect("the instance has a profile to launch")
    };
    let flag = |cmd: &std::process::Command, name: &str| -> Option<String> {
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let i = args.iter().position(|a| a == name)?;
        args.get(i + 1).cloned()
    };

    // The record, seeded the way an earlier write leaves it: a comment, a key
    // of its own, a `;` comment and a blank line in front of the next section.
    // The verb writes one key of one section, so the file it writes is compared
    // byte for byte with these bytes.
    const SEEDED: &str = "# the manager's own record\n[default]\nServerProfile = default\n; a note of its own\nnote = keep me\n\n[lan-2]\nServerProfile = lan-2\n";
    std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
    // The server profiles. `own` shares `default`'s 8080 (one in use, one
    // spare), `clash` shares `lan-2`'s 8081 (spare), `big` sits on the free
    // 8082, `on-bff` on the web service's default port, `tiny` caps the pool at
    // 1, `roomy` at 3.
    profile("default", "[Server]\nPort = 8080\nHostname = 127.0.0.1\n");
    profile("lan-2", "[Server]\nPort = 8081\nHostname = 127.0.0.1\n");
    profile("own", "[Server]\nPort = 8080\nHostname = 127.0.0.1\n");
    profile("clash", "[Server]\nPort = 8081\nHostname = 127.0.0.1\n");
    profile("big", "[Server]\nPort = 8082\nHostname = 127.0.0.1\n");
    profile("on-bff", "[Server]\nPort = 8590\nHostname = 127.0.0.1\n");
    profile(
        "tiny",
        "[Server]\nPort = 8083\nHostname = 127.0.0.1\nModelsMax = 1\n",
    );
    profile(
        "roomy",
        "[Server]\nPort = 8084\nHostname = 127.0.0.1\nModelsMax = 3\n",
    );
    // The pool: three model profiles that load on startup and one that does
    // not. The switched-off half asks for a load too, so the count a check
    // reads is a render's, never the pool's.
    let pool = real_model_files(dir.path(), "[a]\nmodel = /models/a.gguf\nload-on-startup = true\n\n[b]\nmodel = /models/b.gguf\nload-on-startup = true\n\n[c]\nmodel = /models/c.gguf\nload-on-startup = true\n\n[d]\nmodel = /models/d.gguf\n");
    let off_pool = real_model_files(
        dir.path(),
        "[m]\nmodel = /models/m.gguf\nload-on-startup = true\n",
    );
    std::fs::write(sys.join("models.ini"), &pool).expect("models.ini");
    std::fs::write(sys.join("models.ini.disabled"), &off_pool).expect("models.ini.disabled");
    // The assignments, per instance: `default` inherits the pool, so its render
    // starts three model profiles; `lan-2` switches all three off, so its
    // render starts none. The same profile caps one and caps nothing for the
    // other, which is what the per-instance rule is for.
    std::fs::write(
        sys.join("assignments.ini"),
        "[default]\n* = inherit\n\n[lan-2]\n* = inherit\na = off\nb = off\nc = off\n",
    )
    .expect("assignments.ini");

    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        assert!(
            m.render_failures().is_empty(),
            "the fixture's record renders every instance"
        );
        assert_eq!(
            flag(&launch("default"), "--port").as_deref(),
            Some("8080"),
            "the fixture: the launch reads the profile the section names"
        );
        let before = listing();
        let kept = renders();

        assert_eq!(
            m.set_server_profile(&desktop, "default", "big"),
            Ok(()),
            "a spare profile on a port nobody runs"
        );
        let written = text_of(INSTANCES_FILE);
        assert_eq!(
            written,
            SEEDED.replace("ServerProfile = default", "ServerProfile = big"),
            "one key of one section: every other byte of the record, the comment, the `;` comment, the key of its own and the blank line in front of the next section, as it was"
        );
        assert_eq!(
            keys_of(&written, "default", "ServerProfile"),
            vec!["ServerProfile = big".to_string()],
            "the key is replaced, not added to"
        );
        assert_eq!(
            section(&written, "lan-2"),
            "[lan-2]\nServerProfile = lan-2\n",
            "the section of another instance is kept as it was"
        );
        let cmd = launch("default");
        assert_eq!(
            flag(&cmd, "--port").as_deref(),
            Some("8082"),
            "the next launch of the instance is the new profile's port"
        );
        assert_eq!(
            flag(&cmd, "--models-preset").as_deref(),
            Some(sys.join("models@default.ini").to_string_lossy().as_ref()),
            "and its model profiles stay the render of the instance id, not of the profile"
        );

        // A profile that caps the pool: its own key reaches the launch.
        assert_eq!(
            m.set_server_profile(&desktop, "default", "roomy"),
            Ok(()),
            "a cap equal to the loads the instance starts is not over it"
        );
        let cmd = launch("default");
        assert_eq!(
            flag(&cmd, "--port").as_deref(),
            Some("8084"),
            "the second switch landed"
        );
        assert_eq!(
            flag(&cmd, "--models-max").as_deref(),
            Some("3"),
            "the new profile's cap, read from its file"
        );

        // Back to the profile it started on.
        assert_eq!(
            m.set_server_profile(&desktop, "default", "default"),
            Ok(()),
            "a switch back"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "ServerProfile"),
            vec!["ServerProfile = default".to_string()],
            "one key after three switches, not three"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            SEEDED,
            "the record is the bytes it was before the switches"
        );
        assert_eq!(listing(), before, "the verb creates no file of its own");
        for (name, text) in kept {
            assert_eq!(
                text_of(&name),
                text,
                "and renders nothing: a server profile carries no model profile text, and the render's name is the instance's, not the profile's: {name}"
            );
        }
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_server_profile" target="default" caller=":1.42" principal=- outcome=ok"#.to_string();
                3
            ],
            "one record per call: the verb's action, the instance as its target, the caller, no principal as a bare -, ok"
        );
    }

    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let seed = text_of(INSTANCES_FILE);
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "lan-3", "big"),
            Err(ManagerError::NoInstance),
            "an id that is no section of the record"
        );
        for bad in ["", "../x", "Default", "a_b"] {
            assert_eq!(
                m.set_server_profile(&desktop, bad, "big"),
                Err(ManagerError::InvalidId),
                "not an instance id at all: {bad:?}"
            );
        }
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "every refusal above wrote nothing"
        );
        assert_eq!(listing(), before, "and created no file");
    }

    {
        // A file outside the engine's directory, at the place a naive join of
        // the name `../x` lands. It stays the fixture's, so the refusals below
        // built no path.
        std::fs::write(dir.path().join("x.ini"), b"not a profile\n").expect("outside marker");
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let seed = text_of(INSTANCES_FILE);
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "nope"),
            no_profile("nope"),
            "no `server@nope.ini` anywhere: the name the caller asked for comes back"
        );
        for bad in ["", "../x", "a/b", "Big", "x.ini"] {
            assert_eq!(
                m.set_server_profile(&desktop, "default", bad),
                no_profile(bad),
                "a name that builds no profile file, named back: {bad:?}"
            );
        }
        assert_eq!(
            std::fs::read(dir.path().join("x.ini")).expect("outside marker"),
            b"not a profile\n",
            "a name that builds no profile built no path beside it"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "every refusal above wrote nothing"
        );
        assert_eq!(listing(), before, "and created no file");
    }

    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let seed = text_of(INSTANCES_FILE);
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "lan-2"),
            in_use("lan-2", "lan-2"),
            "a profile another instance runs, naming the profile and that instance"
        );
        assert_eq!(
            m.set_server_profile(&desktop, "lan-2", "default"),
            in_use("default", "default"),
            "the same rule read from the other section"
        );
        // The profile the instance already runs is no conflict with itself: the
        // switching instance is out of the walk.
        // The answer comes at once, from a write of nothing: a write of this
        // record is an atomic rename that replaces the file, so the record that
        // is still the same inode is the record the call never rewrote.
        #[cfg(unix)]
        let ino = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(sys.join(INSTANCES_FILE))
                .expect("the record")
                .ino()
        };
        assert_eq!(
            m.set_server_profile(&desktop, "default", "default"),
            Ok(()),
            "the profile it already runs is no conflict with itself"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                std::fs::metadata(sys.join(INSTANCES_FILE))
                    .expect("the record")
                    .ino(),
                ino,
                "the profile it runs is answered at once: the record is the file that was there, not a copy written over it"
            );
        }
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "the refusals above wrote nothing, and the accepted call changed no byte: {seed:?}"
        );
        assert_eq!(listing(), before, "and neither created a file");
    }

    {
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let seed = text_of(INSTANCES_FILE);
        // `clash` is spare and sits on lan-2's 8081. A profile no instance runs
        // binds nothing, so its own save is judged against nothing: the switch
        // is where this address is refused.
        assert_eq!(
            m.set_server_profile(&desktop, "default", "clash"),
            taken("lan-2"),
            "a spare profile on the port of another instance, naming that instance"
        );
        // The web service: no `bff.ini` is its default address, 127.0.0.1:8590.
        std::fs::remove_file(framework.join("bff.ini")).ok();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "on-bff"),
            taken(BFF_NAME),
            "the web service's address, named as itself"
        );
        // Its file names the address it holds: move the service to 9999 and
        // 8590 is free, while 9999 is the service's.
        std::fs::write(
            framework.join("bff.ini"),
            "[Bff]\nListen = 127.0.0.1:9999\n",
        )
        .expect("bff.ini");
        profile("on-listen", "[Server]\nPort = 9999\nHostname = 127.0.0.1\n");
        // The tree is snapshotted after this block's own seeds: the pin below
        // says the refusals added no file of their own, and `server@on-listen.ini`
        // is a seed of this block, not one of theirs.
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "on-listen"),
            taken(BFF_NAME),
            "the address `bff.ini` names is the one the service holds"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "every refusal above wrote nothing"
        );
        assert_eq!(
            listing(),
            before,
            "and left no file of its own beside the record"
        );
        assert_eq!(
            m.set_server_profile(&desktop, "default", "on-bff"),
            Ok(()),
            "the service's default address is free once its file moved"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed.replace("ServerProfile = default", "ServerProfile = on-bff"),
            "the accepted switch is the only key that moved"
        );
        std::fs::remove_file(framework.join("bff.ini")).ok();
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        // `own` is spare and holds 8080, which `default` runs right now: the
        // switch binds nothing new, so it lands.
        assert_eq!(
            m.set_server_profile(&desktop, "default", "own"),
            Ok(()),
            "a spare profile on the port the instance already runs"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "ServerProfile"),
            vec!["ServerProfile = own".to_string()],
            "and the key is the new profile"
        );
        // Moved: `default` runs `own` now, so lan-2's 8081 is back in the walk.
        let seed = text_of(INSTANCES_FILE);
        assert_eq!(
            m.set_server_profile(&desktop, "default", "clash"),
            taken("lan-2"),
            "the instance's current profile is `own` now: lan-2 is not left out"
        );
        assert_eq!(text_of(INSTANCES_FILE), seed, "the refusal wrote nothing");
        // An instance whose profile cannot be read is skipped, as a profile
        // save skips it: nothing is judged on the defaults of a file that
        // cannot be read, so 8081 is nobody's and `clash` lands.
        std::fs::write(
            sys.join("server@lan-2.ini"),
            b"[Server]\nPort = 8081\n\xed\xa0\xbf\n",
        )
        .expect("unreadable lan-2 profile");
        assert_eq!(
            m.set_server_profile(&desktop, "default", "clash"),
            Ok(()),
            "an instance whose profile cannot be read binds no address"
        );
        std::fs::write(
            sys.join("server@lan-2.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("lan-2 profile back");
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        assert!(
            m.render_failures().is_empty(),
            "the record's renders are rewritten from what is on disk"
        );
        let kept = renders();
        // `default`'s render starts three model profiles, `tiny` caps them at 1.
        // The switch changes no model profile, so the count it is judged on is
        // the one the instance starts today: a step to a lower cap is judged
        // all the same, which is the full rule of (b') 3 at the switch, not the
        // step rule `SetModelProfile` uses (that one lets a count that does not
        // rise through).
        assert_eq!(
            m.set_server_profile(&desktop, "default", "tiny"),
            over_max("3", "1", "default"),
            "the new profile's cap under the loads the instance starts, the instance named last, and the count is the render's, not the pool's four"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            SEEDED,
            "the refusal over the max wrote nothing"
        );
        for (name, text) in kept {
            assert_eq!(text_of(&name), text, "and rendered nothing: {name}");
        }
        // The cap is each instance's own render's: `lan-2` switched its three
        // off, so `tiny` caps nothing for it, while it refuses `default`.
        assert_eq!(
            m.set_server_profile(&desktop, "lan-2", "tiny"),
            Ok(()),
            "the same profile, for the instance whose render starts nothing"
        );
        // A profile with no `ModelsMax` caps nothing.
        assert_eq!(
            m.set_server_profile(&desktop, "default", "big"),
            Ok(()),
            "no ModelsMax is no cap"
        );
        // A cap raised to the count lands: the rule is a cap check, judged on
        // what the instance starts.
        profile(
            "tiny",
            "[Server]\nPort = 8083\nHostname = 127.0.0.1\nModelsMax = 3\n",
        );
        assert_eq!(
            m.set_server_profile(&desktop, "lan-2", "roomy"),
            Ok(()),
            "and the instance on the raised profile moves on to the cap of 3"
        );
        let written = text_of(INSTANCES_FILE);
        assert_eq!(
            section(&written, "default"),
            "[default]\nServerProfile = big\n; a note of its own\nnote = keep me\n\n",
            "the switching sections, each its own: {written:?}"
        );
        assert_eq!(
            section(&written, "lan-2"),
            "[lan-2]\nServerProfile = roomy\n",
            "the switch that landed for the refused instance's own id, in its own section, its only key the profile: {written:?}"
        );
        assert_eq!(
            written,
            SEEDED
                .replace("ServerProfile = default", "ServerProfile = big")
                .replace("ServerProfile = lan-2", "ServerProfile = roomy"),
            "the record as it stood, only the two `ServerProfile` keys changed: the leading comment, the `;` comment, the key of a section's own and the blank line in front of the next section are the bytes they were, and the refusal over the cap moved nothing: {written:?}"
        );
        // The `a`/`b`/`c` keys are the assignments' and stay there: this verb
        // writes the record, never the assignments.
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\n* = inherit\n\n[lan-2]\n* = inherit\na = off\nb = off\nc = off\n",
            "the assignments, where those keys live, are the bytes they were"
        );
    }

    {
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        // The target profile with bytes that are no text. A profile read as
        // empty lands as a switch to a profile with no port and no cap, so a
        // file the manager cannot read must never be answered as an empty one.
        profile("big", "[Server]\nPort = 8082\nHostname = 127.0.0.1\n");
        std::fs::write(
            sys.join("server@big.ini"),
            b"[Server]\nPort = 8082\n\xed\xa0\xbf\n",
        )
        .expect("broken profile");
        let record = bytes_of(INSTANCES_FILE);
        let broken = bytes_of("server@big.ini");
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "big"),
            Err(ManagerError::Io(std::io::ErrorKind::InvalidData)),
            "a profile that cannot be read answers the read's own kind"
        );
        assert_eq!(
            bytes_of(INSTANCES_FILE),
            record,
            "the record is the bytes that were there: the atomic write never replaced them"
        );
        assert_eq!(
            bytes_of("server@big.ini"),
            broken,
            "and the profile the call read keeps its bytes"
        );
        assert_eq!(
            listing(),
            before,
            "and the call left no file of its own behind: no temp, no render"
        );
        // A directory at the profile's path: the same answer, and the path
        // stays what it is.
        std::fs::remove_file(sys.join("server@big.ini")).expect("big out");
        std::fs::create_dir(sys.join("server@big.ini")).expect("big dir");
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "big"),
            Err(ManagerError::Io(std::io::ErrorKind::IsADirectory)),
            "a directory at the profile's path answers the read's own kind"
        );
        assert!(
            sys.join("server@big.ini").is_dir(),
            "the directory is still there: nothing was renamed over it"
        );
        assert_eq!(
            listing(),
            before,
            "and the call left no temp file of its own next to it"
        );
        std::fs::remove_dir(sys.join("server@big.ini")).expect("big dir out");
        profile("big", "[Server]\nPort = 8082\nHostname = 127.0.0.1\n");
        // The record itself unreadable: the read's own kind, not the empty list
        // `paths::instance_ids` answers for a file it cannot read, and the
        // record is never replaced with one the manager made up.
        std::fs::write(
            sys.join(INSTANCES_FILE),
            b"[default]\nServerProfile = default\n\xed\xa0\xbf\n",
        )
        .expect("broken record");
        let record = bytes_of(INSTANCES_FILE);
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "big"),
            Err(ManagerError::Io(std::io::ErrorKind::InvalidData)),
            "a record that cannot be read answers its read's own kind, it is not a record of no instances"
        );
        assert_eq!(
            bytes_of(INSTANCES_FILE),
            record,
            "and its bytes are the bytes that were there"
        );
        assert_eq!(
            listing(),
            before,
            "and the call left no file of its own behind"
        );
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
    }

    {
        // The cap check judges the instance's render, and the render is cut by
        // `assignments.ini`. A record there that cannot be read is not an empty
        // one: answering it as if nothing were assigned lets through a switch
        // the same call refuses while the file can be read.
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let assignments =
            "[default]\n* = inherit\n\n[lan-2]\n* = inherit\na = off\nb = off\nc = off\n";
        std::fs::write(sys.join("assignments.ini"), assignments).expect("assignments.ini");
        profile(
            "tiny",
            "[Server]\nPort = 8083\nHostname = 127.0.0.1\nModelsMax = 1\n",
        );
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        // The fixture: with the record readable the switch is over the cap.
        assert_eq!(
            m.set_server_profile(&desktop, "default", "tiny"),
            over_max("3", "1", "default"),
            "the fixture: the same switch is refused while the assignments can be read"
        );
        // Bytes that are no text: the read's own kind, and the record stays.
        std::fs::write(
            sys.join("assignments.ini"),
            b"[default]\n* = inherit\n\xed\xa0\xbf\n",
        )
        .expect("broken assignments");
        let record = bytes_of(INSTANCES_FILE);
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "tiny"),
            Err(ManagerError::Io(std::io::ErrorKind::InvalidData)),
            "an assignments.ini that cannot be read answers the read's own kind, not the cap of a render cut by no assignment"
        );
        assert_eq!(
            bytes_of(INSTANCES_FILE),
            record,
            "the record is the bytes that were there: the refusal wrote nothing"
        );
        assert_eq!(
            listing(),
            before,
            "and the call left no file of its own behind: no temp, no render"
        );
        // A directory at the record's path: the read's own kind, the same.
        std::fs::remove_file(sys.join("assignments.ini")).expect("assignments out");
        std::fs::create_dir(sys.join("assignments.ini")).expect("assignments dir");
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "tiny"),
            Err(ManagerError::Io(std::io::ErrorKind::IsADirectory)),
            "a directory at the assignments path answers the read's own kind"
        );
        assert_eq!(
            bytes_of(INSTANCES_FILE),
            record,
            "and the record keeps its bytes"
        );
        assert_eq!(
            listing(),
            before,
            "and the call left no file of its own behind"
        );
        std::fs::remove_dir(sys.join("assignments.ini")).expect("assignments dir out");
        std::fs::write(sys.join("assignments.ini"), assignments).expect("assignments.ini");
    }

    {
        // Two instances naming one server profile by hand: each asking for the
        // profile its own section already names is no conflict with the other,
        // whichever of them sorts first, and the switch to it changes no byte.
        let twin = "# the record as an editor left it\n[default]\nServerProfile = twin\n\n[zulu]\nServerProfile = twin\n";
        std::fs::write(sys.join(INSTANCES_FILE), twin).expect(INSTANCES_FILE);
        profile("twin", "[Server]\nPort = 8090\nHostname = 127.0.0.1\n");
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let record = bytes_of(INSTANCES_FILE);
        assert_eq!(
            m.set_server_profile(&desktop, "default", "twin"),
            Ok(()),
            "the instance whose section sorts first asks for the profile it already names"
        );
        assert_eq!(
            m.set_server_profile(&desktop, "zulu", "twin"),
            Ok(()),
            "so does the one whose section sorts last: it names that profile too, the first instance is not the holder for it"
        );
        assert_eq!(
            bytes_of(INSTANCES_FILE),
            record,
            "the switch to the profile a section already names changes no byte"
        );
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
    }

    #[cfg(target_os = "linux")]
    {
        // The mode of the record the verb writes: the manager's config files
        // are read by every client, so a write lands them 0644 whatever the
        // process umask was, even over a file that was 0600. The umask is
        // process-wide, so it is masked only around the call and restored
        // before the first assert.
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        std::fs::set_permissions(
            sys.join(INSTANCES_FILE),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("0600 record");
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        let before = listing();
        let guard = UmaskGuard::mask(nix::sys::stat::Mode::from_bits_retain(0o077));
        let answer = m.set_server_profile(&desktop, "default", "big");
        let old_mask = guard.0;
        drop(guard);
        assert_eq!(
            nix::sys::stat::umask(old_mask),
            old_mask,
            "the window restored the umask"
        );
        let mode = std::fs::metadata(sys.join(INSTANCES_FILE))
            .ok()
            .map(|f| f.permissions().mode());
        let after = listing();
        assert_eq!(answer, Ok(()), "the fixture: the switch lands");
        assert_eq!(
            mode.map(|f| f & 0o777),
            Some(0o644),
            "the record was 0600 and the process was masked 0o077: the written instances.ini is 0644"
        );
        assert_eq!(
            after, before,
            "and the write left no temp file of its own beside it"
        );
    }

    {
        // A caller without the right: NotAuthorized, nothing written.
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, FakePasswords::new(), log);
        let seed = text_of(INSTANCES_FILE);
        let before = listing();
        assert_eq!(
            m.set_server_profile(&desktop, "default", "big"),
            Err(ManagerError::NotAuthorized),
            "a caller outside `pai`, polkit refusing"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "a refused caller writes nothing"
        );
        assert_eq!(listing(), before, "and leaves no file of its own behind");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_server_profile" target="default" caller=":1.42" principal=- outcome=refused"#.to_string()
            ],
            "the refusal is recorded as one record too"
        );
    }
    {
        // The bff acting for a member: the pair grants the call, the record
        // names both.
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, p, log);
        assert_eq!(
            m.set_server_profile(&member, "default", "big"),
            Ok(()),
            "the bff switches the server profile for a member of `pai`"
        );
        assert_eq!(
            keys_of(&text_of(INSTANCES_FILE), "default", "ServerProfile"),
            vec!["ServerProfile = big".to_string()],
            "for the member"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["alice".to_string()],
            "the membership of the principal, never of the caller"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            vec![bff.clone()],
            "the caller is checked first, at every call"
        );
        assert_eq!(
            m.authority().asked.lock().unwrap().len(),
            0,
            "polkit is never asked for a principal: its subject is the bff"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="set_server_profile" target="default" caller=":1.9" principal="alice" outcome=ok"#.to_string()
            ],
            "the record carries the caller and the principal"
        );

        // A principal outside `pai`: the call is refused and nothing is written.
        std::fs::write(sys.join(INSTANCES_FILE), SEEDED).expect(INSTANCES_FILE);
        let seed = text_of(INSTANCES_FILE);
        assert_eq!(
            m.set_server_profile(&outsider, "default", "own"),
            Err(ManagerError::NotAuthorized),
            "a right password of a caller outside `pai` is not enough"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "a refused principal writes nothing"
        );

        // A caller that is not the bff cannot name a principal at all.
        let desktop_member = Requester {
            caller: desktop.clone(),
            principal: Some(Principal("alice".into())),
        };
        assert_eq!(
            m.set_server_profile(&desktop_member, "default", "own"),
            Err(ManagerError::NotAuthorized),
            "only the bff answers for a signed-in member"
        );
        assert_eq!(
            text_of(INSTANCES_FILE),
            seed,
            "and a caller outside the bff writes nothing"
        );
    }
}

/// The instance record: `instances.ini`, one section per instance. The list
/// comes from its sections: a `server@<profile>.ini` no section names is no
/// instance, and a section whose profile file is missing is an instance. A
/// client reads the file like `assignments.ini` and can never write it: every
/// write is refused as a bad name and the file stays byte for byte.
#[cfg(test)]
pub(crate) fn manager_instances_file_e2e() {
    use fakes::{FakeActionLog, FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let alice = Caller(":1.42".into());
    let log = FakeActionLog::new();
    let lines = log.lines.clone();
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new())
    .with_log(log);
    // The path is joined here, not through `file_path`: that resolves a
    // served name, and `instances.ini` is served by the read today, so a
    // phase that asked it would stop at its first line.
    let instances = sys.join(INSTANCES_FILE);
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();

    // The record: default on profile default, lan-2 on profile big. big is a
    // profile no instance names, so it serves nothing.
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
    std::fs::write(sys.join("server@default.ini"), ini).expect("server profile");
    std::fs::write(sys.join("server@big.ini"), ini).expect("spare profile");
    std::fs::write(sys.join("models.ini"), "[M1]\nmodel = /models/m1.gguf\n").expect("models.ini");
    let record = "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = big\n";
    std::fs::write(&instances, record).expect("instances.ini");

    // The list is the sections: lan-2 is an instance with no server@lan-2.ini,
    // the profiles default and big alone list nothing beside it.
    assert_eq!(
        m.list_instances(),
        vec!["default".to_string(), "lan-2".to_string()],
        "the sections of instances.ini are the instances"
    );

    // A client reads it like assignments.ini: the text as it stands, with and
    // without an identity. It is no `ConfigFile`, so the gates a server
    // profile read runs never see it.
    assert_eq!(
        m.read_file(None::<&Caller>, INSTANCES_FILE),
        Ok(Some(plain(record))),
        "the file, to a caller-less client, as assignments.ini"
    );
    assert_eq!(
        m.read_file(Some(&alice), INSTANCES_FILE),
        Ok(Some(plain(record))),
        "the file, to a caller"
    );
    assert_eq!(
        config_file(INSTANCES_FILE),
        None,
        "the file is served by the read, not as a config file"
    );

    // A client never writes it: refused as a bad name, through the plain and
    // the compare-and-swap write, forced or not, the file untouched.
    let swapped = "[default]\nServerProfile = big\n";
    let base = lines.lock().unwrap().len();
    assert_eq!(
        m.write_file(&alice, INSTANCES_FILE, swapped, false),
        Err(ManagerError::BadFileName)
    );
    assert_eq!(
        m.write_file_if(&alice, INSTANCES_FILE, swapped, record),
        Err(ManagerError::BadFileName),
        "not through the compare and swap write either"
    );
    for force in [false, true] {
        assert_eq!(
            m.write_file(&alice, INSTANCES_FILE, swapped, force),
            Err(ManagerError::BadFileName),
            "keep_api_key {force}"
        );
    }
    let recorded: Vec<String> = lines.lock().unwrap()[base..].to_vec();
    assert_eq!(
        recorded.len(),
        4,
        "each refused write records one line: {recorded:?}"
    );
    for line in recorded.iter() {
        assert!(
            line.starts_with("action=\"write_file\"")
                || line.starts_with("action=\"write_file_if\""),
            "a write record: {line}"
        );
        assert!(
            line.contains("target=\"instances.ini\""),
            "the refused write names the record: {line}"
        );
        assert!(
            line.contains("outcome=\"BadFileName\""),
            "refused as a bad name: {line}"
        );
    }
    assert_eq!(
        read(INSTANCES_FILE).as_deref(),
        Some(record),
        "a refused write leaves the record"
    );

    // The record gone: the profiles alone are no instances, and the read of a
    // missing record is no content, as for assignments.ini.
    std::fs::remove_file(&instances).expect("remove the record");
    assert_eq!(
        m.list_instances(),
        Vec::<String>::new(),
        "the sections are the instances: none without the file"
    );
    assert_eq!(
        m.read_file(Some(&alice), INSTANCES_FILE),
        Ok(None),
        "the file missing: no content"
    );
}

/// The verbs that act on an instance ask whether the instance exists: that is
/// its section of `instances.ini`, not a `server@<id>.ini` named after the id.
/// So a section naming another profile is an instance whose settings file is
/// `server@<profile>.ini`; an id with no section is no instance even when a
/// file named after it stands in the directory; and a section naming a profile
/// whose file is missing IS an instance for the manager, because the file is
/// the launch's check, made when the unit starts (its refusal is
/// `launch::instance_command`'s own `NoConfig`, pinned in `launch.rs`).
#[cfg(test)]
pub(crate) fn manager_existing_id_e2e() {
    use fakes::{Call, FakeAuthority, FakeGroup, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let alice = Caller(":1.42".into());
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new());
    // The recorded service calls, so a pin can say which verb reached the
    // service manager and under which name. Nothing real is started.
    let taken = || -> Vec<Call> { m.service().calls.lock().unwrap().clone() };
    // The four verbs that ask whether the instance exists (stop and status
    // ask only for a valid id, pinned by `manager_e2e`).
    type VerbAnswers = (
        Result<(), ManagerError>,
        Result<(), ManagerError>,
        Result<bool, ManagerError>,
        Result<(), ManagerError>,
    );
    let verbs = |id: &str| {
        (
            m.start(&alice, id),
            m.restart(&alice, id),
            m.boot_enabled(id),
            m.set_boot_enabled(&alice, id, true),
        )
    };
    let no_instance = || -> VerbAnswers {
        (
            Err(ManagerError::NoInstance),
            Err(ManagerError::NoInstance),
            Err(ManagerError::NoInstance),
            Err(ManagerError::NoInstance),
        )
    };
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
    let record = |text: &str| {
        std::fs::write(sys.join(INSTANCES_FILE), text).expect(INSTANCES_FILE);
    };

    // The record names `default` on the profile `big`, and only
    // `server@big.ini` is on disk: default is an instance, and its unit is
    // the one named by the id.
    std::fs::write(sys.join("server@big.ini"), ini).expect("server@big.ini");
    record("[default]\nServerProfile = big\n");
    assert_eq!(
        m.list_instances(),
        ["default"],
        "the section is the instance"
    );
    assert_eq!(
        m.boot_enabled("default"),
        Ok(false),
        "a section naming a profile with no server@<id>.ini: not NoInstance"
    );
    assert_eq!(
        m.set_boot_enabled(&alice, "default", true),
        Ok(()),
        "the start at boot of that instance"
    );
    assert_eq!(m.boot_enabled("default"), Ok(true), "and it reads back");
    assert_eq!(
        m.start(&alice, "default"),
        Ok(()),
        "start of default, whose section names `big`: not NoInstance"
    );
    assert_eq!(
        m.restart(&alice, "default"),
        Ok(()),
        "restart of default: not NoInstance"
    );
    assert_eq!(
        taken(),
        vec![
            Call::BootEnabled("default".into()),
            Call::SetBootEnabled("default".into(), true),
            Call::BootEnabled("default".into()),
            Call::Start("default".into()),
            Call::Restart("default".into()),
        ],
        "the verbs reach the service under the instance id: the profile never names a unit"
    );

    // An id with no section is no instance, even with a `server@<id>.ini` in
    // the directory: the record is the fact, the file names nothing.
    std::fs::write(sys.join("server@orphan.ini"), ini).expect("server@orphan.ini");
    let asked = taken().len();
    assert_eq!(
        m.list_instances(),
        ["default"],
        "a profile file no section names is no instance"
    );
    assert_eq!(
        verbs("orphan"),
        no_instance(),
        "start, restart, boot_enabled and set_boot_enabled of an id with no section"
    );
    assert_eq!(
        taken().len(),
        asked,
        "a refused verb reaches no service call"
    );

    // A section naming a profile whose file is missing: the manager has the
    // instance, so the verbs reach the service. The missing file is refused by
    // the launch, when the unit starts and reads the profile (`launch.rs`:
    // `NoConfig` naming `server@huge.ini`).
    record("[default]\nServerProfile = huge\n");
    assert!(
        !sys.join("server@huge.ini").exists(),
        "fixture sanity: the profile has no file"
    );
    assert_eq!(
        m.start(&alice, "default"),
        Ok(()),
        "a section naming a profile with no file passes the existence check"
    );
    assert_eq!(
        &taken()[asked..],
        &[Call::Start("default".into())][..],
        "the start reaches the service manager: nothing is refused here"
    );
    // A section with no `ServerProfile` key is an instance the same way (the
    // listing reads sections, and the launch is the one that refuses a section
    // naming no profile, pinned in `launch.rs`).
    record("[default]\n");
    assert_eq!(
        m.start(&alice, "default"),
        Ok(()),
        "a section naming no profile is an instance for the manager"
    );

    // A name that is no instance id stays InvalidId, whatever the record holds
    // and whatever file stands in the directory: the id is checked first.
    record(
        "[Big]\nServerProfile = big\n\n[a/b]\nServerProfile = big\n\n[../x]\nServerProfile = big\n",
    );
    std::fs::write(sys.join("server@Big.ini"), ini).expect("server@Big.ini");
    for bad in ["Big", "a/b", "../x", ""] {
        assert_eq!(
            m.start(&alice, bad),
            Err(ManagerError::InvalidId),
            "start {bad:?}"
        );
        assert_eq!(
            m.restart(&alice, bad),
            Err(ManagerError::InvalidId),
            "restart {bad:?}"
        );
        assert_eq!(
            m.boot_enabled(bad),
            Err(ManagerError::InvalidId),
            "boot_enabled {bad:?}"
        );
        assert_eq!(
            m.set_boot_enabled(&alice, bad, false),
            Err(ManagerError::InvalidId),
            "set_boot_enabled {bad:?}"
        );
    }

    // The record gone: every id is no instance, and a file named after one
    // stands in for nothing.
    std::fs::remove_file(sys.join(INSTANCES_FILE)).expect("remove the record");
    assert!(
        sys.join("server@orphan.ini").is_file(),
        "fixture sanity: the file is still there"
    );
    assert_eq!(
        verbs("orphan"),
        no_instance(),
        "no record: the file named after the id names no instance"
    );
}

/// E2E phase: the port check and the `ModelsMax` check follow the profile an
/// instance names in `instances.ini`, not the file named after its id. So two
/// profiles no instance uses share a port freely, a profile in use collides
/// with the profile of another instance and the refusal names the INSTANCE, an
/// instance whose profile cannot be read is skipped and never judged on the
/// defaults of a missing file, and a file named after an id that is not that
/// instance's profile is never read. Instances `default` and `lan` run the
/// profiles `big` and `small`: the names differ on purpose.
#[cfg(test)]
pub(crate) fn manager_profiles_in_use_e2e() {
    use fakes::{FakeAuthority, FakeGroup, FakeResolver, FakeService};
    use pai_core::checks::Refusal;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let framework = crate::paths::system_config_dir();
    let alice = Caller(":1.42".into());
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let record = |text: &str| {
        std::fs::write(sys.join(INSTANCES_FILE), text).expect(INSTANCES_FILE);
    };
    let profile = |name: &str, text: &str| {
        std::fs::write(sys.join(format!("server@{name}.ini")), text).expect("server profile");
    };
    let over_max = |count: &str, max: &str, id: &str| {
        Err(ManagerError::Invalid(Refusal {
            key: "load-on-startup-over-max",
            values: vec![count.to_string(), max.to_string(), id.to_string()],
        }))
    };
    let flagged = |ids: &[&str]| -> String {
        real_model_files(
            dir.path(),
            &ids.iter()
                .map(|id| format!("[{id}]\nmodel = /models/{id}.gguf\nload-on-startup = true\n"))
                .collect::<String>(),
        )
    };
    // default on big (8081), lan on small (8082).
    let in_use = "[default]\nServerProfile = big\n\n[lan]\nServerProfile = small\n";
    let big_8081 = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 8\n";
    record(in_use);
    profile("big", big_8081);
    profile(
        "small",
        "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 8\n",
    );

    // ── A profile no instance uses is checked against nothing ─────────────
    for name in ["spare-a", "spare-b"] {
        assert_eq!(
            m.write_file(
                &alice,
                &format!("server@{name}.ini"),
                "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
                false
            ),
            Ok(()),
            "{name}: two unused profiles with one port both save, they are \
             alternatives for one instance"
        );
    }
    assert_eq!(
        m.write_file(
            &alice,
            "server@spare-c.ini",
            "[Server]\nPort = 8082\nHostname = 127.0.0.1\n",
            false
        ),
        Ok(()),
        "an unused profile may share the port of a profile in use"
    );

    // ── A profile in use collides with another instance's profile ─────────
    assert_eq!(
        m.write_file(
            &alice,
            "server@big.ini",
            "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 8\n",
            false
        ),
        Err(ManagerError::AddressInUse("lan".into())),
        "the refusal names the INSTANCE that runs the colliding profile, \
         never the profile's name"
    );
    assert_eq!(
        read("server@big.ini").as_deref(),
        Some(big_8081),
        "a refused save writes nothing"
    );
    assert_eq!(
        m.write_file(
            &alice,
            "server@small.ini",
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 8\n",
            false
        ),
        Err(ManagerError::AddressInUse("default".into())),
        "the other way round names default, the instance running big"
    );
    // An instance never collides with itself, and its id is not the profile's
    // name: rewriting big with the address default already binds is its own.
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "big is the profile in use by default: it is not a second address"
    );

    // ── A section that cannot name a readable profile is skipped ──────────
    // ghost's file is missing, noprofile has no key, bogus an invalid value:
    // none of them is judged on the defaults of a missing file, so none of
    // them (all of them on the built-in default address) holds a save whose
    // port is that default. The save is of big, a profile in use, so the
    // check runs and walks those sections.
    record(&format!(
        "{in_use}\n[ghost]\nServerProfile = ghost\n\n[noprofile]\n\n[bogus]\nServerProfile = ../x\n"
    ));
    let on_default_port = format!(
        "[Server]\nPort = {}\nHostname = 127.0.0.1\nModelsMax = 8\n",
        crate::server_cfg::DEFAULT_PORT
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", &on_default_port, false),
        Ok(()),
        "a missing profile file, a section with no ServerProfile and one with \
         an invalid value refuse nothing: none is judged on the defaults of a \
         missing file"
    );
    // The same port on a profile no instance uses: accepted as well.
    assert_eq!(
        m.write_file(&alice, "server@fresh.ini", &on_default_port, false),
        Ok(()),
        "and an unused profile is checked against nothing at all"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big back on 8081"
    );

    // ── A file named after an instance, when its profile is another one ───
    // lan runs small on 8091; server@lan.ini stands on 8090 and is no part of
    // lan, so a profile in use may take 8090.
    profile(
        "small",
        "[Server]\nPort = 8091\nHostname = 127.0.0.1\nModelsMax = 8\n",
    );
    profile("lan", "[Server]\nPort = 8090\nHostname = 127.0.0.1\n");
    assert_eq!(
        m.write_file(
            &alice,
            "server@big.ini",
            "[Server]\nPort = 8090\nHostname = 127.0.0.1\nModelsMax = 8\n",
            false
        ),
        Ok(()),
        "server@lan.ini is not lan's profile: it is never read"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big back on 8081"
    );

    // ── The bff's own listen address ──────────────────────────────────────
    std::fs::create_dir_all(&framework).expect("framework config dir");
    std::fs::write(
        framework.join("bff.ini"),
        "[Bff]\nListen = 127.0.0.1:8590\n",
    )
    .expect("bff.ini");
    assert_eq!(
        m.write_file(
            &alice,
            "server@big.ini",
            "[Server]\nPort = 8590\nHostname = 127.0.0.1\nModelsMax = 8\n",
            false
        ),
        Err(ManagerError::AddressInUse("private-ai-bff".into())),
        "the bff's listen address holds its port against a profile in use, \
         named as itself"
    );
    assert_eq!(
        read("server@big.ini").as_deref(),
        Some(big_8081),
        "a refused save writes nothing"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "a profile in use on another port saves with bff.ini in place"
    );
    // The address the bff serves, as `crates/bff/src/listen.rs` reads the file.
    let on_bff_port = "[Server]\nPort = 8590\nHostname = 127.0.0.1\nModelsMax = 8\n";

    // No bff.ini at all, and one whose [Bff] section sets no Listen, both leave
    // the bff on its default 127.0.0.1:8590: that address holds its port.
    std::fs::remove_file(framework.join("bff.ini")).expect("remove bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@big.ini", on_bff_port, false),
        Err(ManagerError::AddressInUse("private-ai-bff".into())),
        "no bff.ini: the bff is on 127.0.0.1:8590 by default and holds it \
         against a profile in use"
    );
    std::fs::write(framework.join("bff.ini"), "[Bff]\nHosts = box.local\n").expect("bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@big.ini", on_bff_port, false),
        Err(ManagerError::AddressInUse("private-ai-bff".into())),
        "a bff.ini that sets no Listen leaves the same default address: the \
         same refusal"
    );

    // The wildcard binds every address, so a bff on 0.0.0.0:8590 collides with
    // a profile bound to 127.0.0.1:8590 (`addresses_collide`); a bff bound to
    // 127.0.0.1:8590 leaves the same port on another address free.
    std::fs::write(framework.join("bff.ini"), "[Bff]\nListen = 0.0.0.0:8590\n").expect("bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@big.ini", on_bff_port, false),
        Err(ManagerError::AddressInUse("private-ai-bff".into())),
        "a bff on 0.0.0.0:8590 collides with a profile in use on \
         127.0.0.1:8590"
    );
    assert_eq!(
        read("server@big.ini").as_deref(),
        Some(big_8081),
        "a refused save writes nothing"
    );
    std::fs::write(
        framework.join("bff.ini"),
        "[Bff]\nListen = 127.0.0.1:8590\n",
    )
    .expect("bff.ini");
    assert_eq!(
        m.write_file(
            &alice,
            "server@big.ini",
            "[Server]\nPort = 8590\nHostname = 192.168.1.5\nModelsMax = 8\n",
            false
        ),
        Ok(()),
        "the bff on 127.0.0.1:8590 does not hold 192.168.1.5:8590"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big back on 8081"
    );

    // A Listen the bff will not start on, and a bff.ini that cannot be read,
    // refuse no save: the bff answers those by not starting, the manager has
    // no address to hold. An empty value is no address and not the default.
    std::fs::write(
        framework.join("bff.ini"),
        "[Bff]\nListen = box.local:8590\n",
    )
    .expect("bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@big.ini", on_bff_port, false),
        Ok(()),
        "a Listen that is no ip:port, a host name: the save lands"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big back on 8081"
    );
    std::fs::write(framework.join("bff.ini"), "[Bff]\nListen =\n").expect("bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@big.ini", on_bff_port, false),
        Ok(()),
        "an empty Listen is no address and not the default: the save lands"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big back on 8081"
    );
    // The file is a directory: reading it fails, and the manager asks nothing
    // of a file it cannot read.
    std::fs::remove_file(framework.join("bff.ini")).expect("remove bff.ini");
    std::fs::create_dir_all(framework.join("bff.ini")).expect("directory as bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@big.ini", on_bff_port, false),
        Ok(()),
        "a bff.ini that cannot be read refuses no save"
    );
    assert_eq!(
        read("server@big.ini").as_deref(),
        Some(on_bff_port),
        "and it lands"
    );
    std::fs::remove_dir(framework.join("bff.ini")).expect("remove the directory");

    // The bff's port is free for a profile no instance uses.
    std::fs::write(
        framework.join("bff.ini"),
        "[Bff]\nListen = 127.0.0.1:8590\n",
    )
    .expect("bff.ini");
    assert_eq!(
        m.write_file(&alice, "server@spare-d.ini", on_bff_port, false),
        Ok(()),
        "an unused profile on the bff's port saves: it is checked against \
         nothing at all"
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big back on 8081, the bff's port left on an unused profile"
    );

    // ── ModelsMax of a profile in use ─────────────────────────────────────
    std::fs::write(
        sys.join("assignments.ini"),
        "[default]\n* = inherit\n\n[lan]\n",
    )
    .expect("assignments.ini");
    std::fs::write(sys.join("models.ini"), flagged(&["a", "b"])).expect("models.ini");
    assert_eq!(
        m.write_file(
            &alice,
            "server@big.ini",
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 1\n",
            false
        ),
        over_max("2", "1", "default"),
        "lowered under what default loads: refused, naming the INSTANCE"
    );
    assert_eq!(
        read("server@big.ini").as_deref(),
        Some(big_8081),
        "a refused save writes nothing"
    );
    assert_eq!(
        m.write_file(
            &alice,
            "server@spare-a.ini",
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 1\n",
            false
        ),
        Ok(()),
        "the same save on an unused profile: no instance loads anything from it"
    );

    // ── A model-profile write is judged against the profile the instance names
    // big goes to a max of 2 (default loads two: legal), and a misleading
    // server@default.ini (max 1) stands beside it: the profile is the cap.
    assert_eq!(
        m.write_file(
            &alice,
            "server@big.ini",
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 2\n",
            false
        ),
        Ok(()),
        "fixture: big's max down to the two loads default makes"
    );
    profile(
        "default",
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\nModelsMax = 1\n",
    );
    assert_eq!(
        m.write_file(&alice, "models.ini", &flagged(&["a", "b"]), false),
        Ok(()),
        "big has room for the two default loads: the file named after the id is not read"
    );
    assert_eq!(
        m.write_file(&alice, "models.ini", &flagged(&["a", "b", "c"]), false),
        over_max("3", "2", "default"),
        "the max the refusal carries is the profile's, and it names the instance"
    );
    assert_eq!(
        read("models.ini").as_deref(),
        Some(flagged(&["a", "b"]).as_str()),
        "a refused pool write leaves the file it had"
    );
    // The other pool file is judged the same way, against the profile's max.
    assert_eq!(
        m.write_file(&alice, "models.ini.disabled", &flagged(&["f"]), false),
        Ok(()),
        "the off pool of an instance running big (max 2) fits: the id-named \
         file's max 1 is not read"
    );

    // ── An instance whose profile cannot be read is skipped ───────────────
    // ghost's profile file is missing and broken names an invalid one: with
    // five loads in the pool neither is judged on the defaults of a missing
    // file, so neither refuses the write on its own account.
    record(
        "[default]\nServerProfile = big\n\n[ghost]\nServerProfile = ghost\n\n\
         [broken]\nServerProfile = ../x\n",
    );
    assert_eq!(
        m.write_file(&alice, "server@big.ini", big_8081, false),
        Ok(()),
        "fixture: big's max back to 8, which fits the five loads below"
    );
    std::fs::write(
        sys.join("assignments.ini"),
        "[default]\n* = inherit\n\n[ghost]\n* = inherit\n\n[broken]\n* = inherit\n",
    )
    .expect("assignments.ini");
    assert_eq!(
        m.write_file(
            &alice,
            "models.ini",
            &flagged(&["a", "b", "c", "d", "e"]),
            false
        ),
        Ok(()),
        "an instance with no readable profile refuses nothing"
    );
    assert_eq!(
        read("models.ini").as_deref(),
        Some(flagged(&["a", "b", "c", "d", "e"]).as_str()),
        "and it lands"
    );

    // ── Two instances run one profile: it is one binding, not two ────────
    // a and b both name shared (8081), c names other (8082). They are the
    // same address, so they never collide with each other; c is the one
    // holding 8082. The two pools of the record above are left behind.
    record(
        "[a]\nServerProfile = shared\n\n[b]\nServerProfile = shared\n\n\
         [c]\nServerProfile = other\n",
    );
    let shared_8081 = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 8\n";
    profile("shared", shared_8081);
    profile(
        "other",
        "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 8\n",
    );
    std::fs::write(
        sys.join("assignments.ini"),
        "[a]\n* = inherit\n\n[b]\n* = inherit\n\n[c]\n* = inherit\n",
    )
    .expect("assignments.ini");
    std::fs::write(sys.join("models.ini"), flagged(&["m1", "m2"])).expect("models.ini");
    assert_eq!(
        m.write_file(&alice, "server@shared.ini", shared_8081, false),
        Ok(()),
        "a and b run shared on 127.0.0.1:8081: one binding, they are never \
         two colliding addresses"
    );
    assert_eq!(
        m.write_file(
            &alice,
            "server@shared.ini",
            "[Server]\nPort = 8082\nHostname = 127.0.0.1\nModelsMax = 8\n",
            false
        ),
        Err(ManagerError::AddressInUse("c".into())),
        "shared on other's port: the refusal names c, the instance running \
         other, and never a or b, which run the profile being saved"
    );
    assert_eq!(
        read("server@shared.ini").as_deref(),
        Some(shared_8081),
        "a refused save writes nothing"
    );

    // ── One profile, two instances: each instance's own load count ────────
    // Both a and b inherit the pool, so each loads two model profiles on
    // startup: a max of 1 is under the count of each and is refused naming
    // the instance whose render is over (a first, in id order).
    let shared_max_1 = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 1\n";
    assert_eq!(
        m.write_file(&alice, "server@shared.ini", shared_max_1, false),
        over_max("2", "1", "a"),
        "under what a loads: refused naming a"
    );
    assert_eq!(
        read("server@shared.ini").as_deref(),
        Some(shared_8081),
        "a refused save writes nothing"
    );
    // a given one of the two, b left with both: the same save names b.
    std::fs::write(
        sys.join("assignments.ini"),
        "[a]\nm1 = inherit\n\n[b]\n* = inherit\n",
    )
    .expect("assignments.ini");
    assert_eq!(
        m.write_file(&alice, "server@shared.ini", shared_max_1, false),
        over_max("2", "1", "b"),
        "under what b loads: refused naming b, the other instance running \
         the same profile"
    );

    // ── An unused profile is still the file's own content ─────────────────
    // No instance names spare-x, so it binds nothing and caps nothing; a
    // Hostname that does not resolve is a content error of the file, whoever
    // uses it: the save stays refused, naming the host.
    assert_eq!(
        m.write_file(
            &alice,
            "server@spare-x.ini",
            "[Server]\nPort = 8095\nHostname = nowhere.example\n",
            false
        ),
        Err(ManagerError::UnresolvableHost("nowhere.example".into())),
        "an unused profile whose Hostname does not resolve is refused"
    );
    assert!(
        !sys.join("server@spare-x.ini").exists(),
        "refused: not written"
    );
}

/// A render that cannot be written (its target is a directory, the disk is
/// full): the render does not stop there and it is not silent. Every instance
/// after it in id order is rendered, each failure comes back with its
/// instance, and the manager's start reports each one, naming the instance.
#[cfg(test)]
pub(crate) fn manager_render_failure_e2e() {
    use fakes::{FakeActionLog, FakeAuthority, FakeGroup, FakeResolver, FakeService};
    use pai_core::assign::render;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let alice = Caller(":1.42".into());
    let log = FakeActionLog::new();
    let lines = log.lines.clone();
    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new())
    .with_log(log);
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).ok();
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();
    let listing = |dir: &std::path::Path| -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    // Three instances: the id order the render walks is default, lan-2,
    // lan-3, so lan-3 sits after the two renders broken below. Each section
    // is told its own model profile, and lan-3 is told a model profile only the later pool
    // has: its render says, in its own text, whether the render ran.
    let assignments = "[default]\na = inherit\n\n[lan-2]\nb = inherit\n\n[lan-3]\nd = inherit\n";
    let section = |id: &str| -> String {
        pai_core::ini::text::section_text(assignments, id).unwrap_or_default()
    };
    let pool = real_model_files(dir.path(), "[a]\nmodel = /models/a.gguf\n\n[b]\nmodel = /models/b.gguf\n\n[c]\nmodel = /models/c.gguf\n");
    // The pool a hand edit puts on disk, and the one a client write puts
    // there: d arrives with the hand edit, e with the write.
    let hand = real_model_files(
        dir.path(),
        &format!("{pool}\n[d]\nmodel = /models/d.gguf\n"),
    );
    let written = real_model_files(
        dir.path(),
        &format!("{hand}\n[e]\nmodel = /models/e.gguf\n"),
    );

    std::fs::write(sys.join("assignments.ini"), assignments).expect("assignments.ini");
    write_record(&sys, &["default", "lan-2", "lan-3"]);
    assert_eq!(m.write_file(&alice, "models.ini", &pool, false), Ok(()));
    for (id, port) in [("default", 8081u16), ("lan-2", 8082), ("lan-3", 8083)] {
        let ini = format!("[Server]\nPort = {port}\nHostname = 127.0.0.1\n");
        assert_eq!(
            m.write_file(&alice, &format!("server@{id}.ini"), &ini, false),
            Ok(()),
            "{id} created"
        );
    }
    assert_eq!(
        read("models@lan-3.ini"),
        Some(render(&pool, "", Some(section("lan-3").as_str()))),
        "every instance rendered, before anything is broken"
    );

    // Two renders cannot land, each in the place of a file: the atomic write
    // cannot rename a file onto a directory. A mode change is no fixture, the
    // service runs as root and root ignores modes.
    std::fs::write(sys.join("models.ini"), &hand).expect("hand-edited pool");
    for id in ["default", "lan-2"] {
        let path = sys.join(format!("models@{id}.ini"));
        std::fs::rename(&path, sys.join(format!("models@{id}.ini.held")))
            .expect("move the render aside");
        std::fs::create_dir(&path).expect("directory in a render's place");
    }

    // ── The start render: the instances after a failure are rendered ─────
    let failures = m.render_failures();
    let ids: Vec<String> = failures.iter().map(|(id, _)| id.clone()).collect();
    assert_eq!(
        ids,
        vec!["default".to_string(), "lan-2".to_string()],
        "one failure per instance whose render did not land, in id order, \
         the first failure first: {failures:?}"
    );
    assert!(
        failures
            .iter()
            .all(|(_, err)| matches!(err, ManagerError::Io(_))),
        "a render that cannot be written is an I/O failure: {failures:?}"
    );
    assert_eq!(
        read("models@lan-3.ini"),
        Some(render(&hand, "", Some(section("lan-3").as_str()))),
        "lan-3, after both failures in id order, holds the pool on disk"
    );
    for id in ["default", "lan-2"] {
        let path = sys.join(format!("models@{id}.ini"));
        assert!(
            path.is_dir(),
            "the directory that blocked {id} stands: {path:?}"
        );
        assert_eq!(
            listing(&path),
            Vec::<String>::new(),
            "a render that failed writes nothing inside the directory that blocked it"
        );
    }
    assert!(
        !listing(&sys).iter().any(|name| name.contains(".tmp")),
        "a failed render leaves no temp file: {:?}",
        listing(&sys)
    );

    // ── The manager's start reports each failure, naming the instance ────
    let before = records(&lines).len();
    m.report_render_failures(&failures);
    let recs = records(&lines);
    let reported: Vec<&str> = recs[before..].iter().map(String::as_str).collect();
    assert_eq!(
        reported.len(),
        2,
        "one line per failed render: {reported:?}"
    );
    for id in ["default", "lan-2"] {
        let named: Vec<&&str> = reported.iter().filter(|line| line.contains(id)).collect();
        assert_eq!(named.len(), 1, "exactly one line names {id}: {reported:?}");
        assert!(
            named[0].contains("render") && named[0].contains("Io"),
            "the line names the render and how it failed: {:?}",
            named[0]
        );
    }
    assert!(
        reported.iter().all(|line| !line.contains("lan-3")),
        "an instance that was rendered is not reported: {reported:?}"
    );

    // ── A write of a pool file: it lands, the render runs past a failure ─
    // The caller is told the render failed, with the first failure in id
    // order; the write itself stands, and every instance after the failures
    // is rendered with it.
    let answer = m.write_file(&alice, "models.ini", &written, false);
    assert_eq!(
        answer,
        Err(failures[0].1.clone()),
        "the caller is told the first render failure"
    );
    assert_eq!(
        read("models.ini").as_deref(),
        Some(written.as_str()),
        "the write itself landed: the render that failed is not its failure"
    );
    assert_eq!(
        read("models@lan-3.ini"),
        Some(render(&written, "", Some(section("lan-3").as_str()))),
        "the write's render runs past both failures"
    );

    // ── A start where every render lands reports nothing ─────────────────
    for id in ["default", "lan-2"] {
        std::fs::remove_dir(sys.join(format!("models@{id}.ini"))).expect("remove the blocker");
    }
    let clean = m.render_failures();
    assert_eq!(
        clean,
        Vec::<(String, ManagerError)>::new(),
        "with every render landing, nothing failed: {clean:?}"
    );
    let before_clean = records(&lines).len();
    m.report_render_failures(&clean);
    assert_eq!(
        records(&lines).len(),
        before_clean,
        "a start with no failed render records nothing"
    );
    assert_eq!(
        read("models@default.ini"),
        Some(render(&written, "", Some(section("default").as_str()))),
        "default's render lands once its path is free"
    );
}

/// E2E phase for acting for a signed-in user in the core: a trusted caller
/// (the bff's account) naming a `pai` member, over the files a guarded call
/// touches, so it redirects the data root (`src/tests/mod.rs`). Every seam is
/// a fake: no polkit, no systemd, no real account is asked.
#[cfg(test)]
pub(crate) fn manager_principal_e2e() {
    use fakes::{Call, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n",
    );
    let other = real_model_files(
        dir.path(),
        "[Qwen3.8-32B]\nmodel = /m/Qwen3.8-32B-Q4_K_M.gguf\n",
    );
    // A model profile text no block of this phase writes on purpose: a refused write
    // of it is visible as the file on disk staying as `other`.
    let third = real_model_files(
        dir.path(),
        "[Qwen3.8-14B]\nmodel = /m/Qwen3.8-14B-Q4_K_M.gguf\n",
    );
    std::fs::write(sys.join("server@default.ini"), ini).expect("instance file");
    std::fs::write(sys.join("models.ini"), &model_profiles).expect("presets file");
    write_record(&sys, &["default"]);
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");

    // The bff's account (a bus name here, the service SID on Windows) and its
    // request for alice, a member of `pai` in [`FakePasswords`].
    let bff = Caller(":1.9".into());
    let alice = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let bob = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let core = |allow: bool, quiet: bool, passwords: FakePasswords| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(quiet).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
    };
    let log = |m: &Manager<FakeService, FakeAuthority, FakeResolver>| {
        m.service().calls.lock().unwrap().clone()
    };
    let polkit = |m: &Manager<FakeService, FakeAuthority, FakeResolver>| {
        m.authority().asked.lock().unwrap().clone()
    };
    let polkit_quiet = |m: &Manager<FakeService, FakeAuthority, FakeResolver>| {
        m.authority().asked_quietly.lock().unwrap().clone()
    };

    // ── The bff acting for a member: the right is the pair ──────────────
    // The bff's own polkit rights are allowed here and irrelevant: the pair
    // (the bff's account, a current `pai` member) is what grants the call, so
    // polkit is never asked, not even about the action the call needs.
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        assert_eq!(
            m.start(&alice, "default"),
            Ok(()),
            "the bff starts an instance for a member"
        );
        assert_eq!(
            m.stop(&alice, "default"),
            Ok(()),
            "the bff stops an instance for a member"
        );
        assert_eq!(
            m.restart(&alice, "default"),
            Ok(()),
            "the bff restarts an instance for a member"
        );
        assert_eq!(
            m.set_boot_enabled(&alice, "default", true),
            Ok(()),
            "the bff sets start at boot for a member"
        );
        assert_eq!(
            log(&m),
            vec![
                Call::Start("default".into()),
                Call::Stop("default".into()),
                Call::Restart("default".into()),
                Call::SetBootEnabled("default".into(), true)
            ],
            "every call reaches the service manager"
        );
        assert_eq!(
            polkit(&m),
            vec![],
            "polkit is never asked for a principal: its subject is the bff"
        );
        assert_eq!(
            polkit_quiet(&m),
            vec![],
            "not even the question asked without a prompt"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            vec![bff.clone(); 4],
            "the caller is checked first, at every call"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["alice".to_string(); 4],
            "the membership of the principal, never of the caller"
        );
    }

    // ── A principal's read is never withheld ─────────────────────────────
    // Here the bff is one polkit would challenge for `configure` (quiet
    // false): the member's read of an instance file still carries the key.
    {
        let m = core(true, false, FakePasswords::new());
        let read = m.read_file(Some(&alice), "server@default.ini");
        assert_eq!(
            read,
            Ok(Some(plain(ini))),
            "a principal's read of an instance file holds its key"
        );
        let file = read.unwrap().expect("read");
        assert!(!file.withheld, "nothing withheld from a member's read");
        assert!(
            file.text.contains("sk-local-3f9a"),
            "the key blanked: {file:?}"
        );
        assert_eq!(
            polkit_quiet(&m),
            vec![],
            "configure is not asked to decide a principal's read"
        );
        assert_eq!(polkit(&m), vec![], "control is not asked either");
    }

    // ── A principal call needs no right of the bff's own ───────────────
    // This bff has no polkit right at all: a member principal writes.
    {
        let m = core(false, false, FakePasswords::new());
        assert_eq!(
            m.write_file(&alice, "models.ini", &other, false),
            Ok(()),
            "granting the bff `configure` for this is what the pair replaces"
        );
        assert_eq!(text_of("models.ini"), other, "the write landed");
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(ini))),
            "the write is not the only call a member's principal opens"
        );
        assert_eq!(polkit(&m), vec![], "no polkit right of the bff is asked");
    }

    // ── The bff naming a non-member: refused, nothing done ─────────────
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        assert_eq!(
            m.start(&bob, "default"),
            Err(ManagerError::NotAuthorized),
            "bob is no `pai` member"
        );
        assert_eq!(
            m.write_file(&bob, "models.ini", &model_profiles, false),
            Err(ManagerError::NotAuthorized),
            "writing needs the same membership"
        );
        assert_eq!(
            m.read_file(Some(&bob), "server@default.ini"),
            Err(ManagerError::NotAuthorized),
            "a non-member reads nothing, not even a blanked file"
        );
        assert_eq!(
            log(&m),
            vec![],
            "the refused start reached no service manager"
        );
        assert_eq!(
            text_of("models.ini"),
            other,
            "the refused write changed no file"
        );
        assert_eq!(polkit(&m), vec![], "polkit decides nothing here");
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["bob".to_string(); 3],
            "the membership is asked once per call"
        );
    }

    // ── Fail closed: a member removed between two calls ────────────────
    {
        let p = FakePasswords::new();
        let admins = p.admins.clone();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        assert_eq!(
            m.start(&alice, "default"),
            Ok(()),
            "a member's first call works"
        );
        admins.lock().unwrap().remove("alice");
        assert_eq!(
            m.start(&alice, "default"),
            Err(ManagerError::NotAuthorized),
            "the next call is refused: the manager caches no membership"
        );
        assert_eq!(
            log(&m),
            vec![Call::Start("default".into())],
            "the refused call started nothing"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["alice".to_string(); 2],
            "membership looked up again at the second call"
        );
    }

    // ── A caller that is not the bff naming a member: refused, unwritten ──
    // The three real transports' identities that are not the bff's account: a
    // D-Bus bus name, the LocalSystem SID, and the pipe's caller with no
    // identity. The pair is the only right a principal call has, so naming a
    // member grants these nothing, and the refusal answers before the account
    // behind the name is looked up.
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        let outsiders = [
            Caller(":1.42".into()),
            Caller("S-1-5-18".into()),
            Caller(String::new()),
        ];
        for caller in outsiders.clone() {
            let who = Requester {
                caller,
                principal: Some(Principal("alice".into())),
            };
            assert_eq!(
                m.write_file(&who, "models.ini", &third, false),
                Err(ManagerError::NotAuthorized),
                "{who:?}: only the bff vouches for a user"
            );
        }
        assert_eq!(
            text_of("models.ini"),
            other,
            "no caller outside the bff wrote the preset file"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            Vec::<String>::new(),
            "the caller is refused before the account behind the name is looked up"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            outsiders.to_vec(),
            "the caller is the question asked, once per call"
        );
        assert_eq!(
            polkit(&m),
            vec![],
            "the gate, not polkit, is what refused these writes"
        );
    }

    // ── The bff naming an illegible name: refused, unwritten ─────────────
    // No name at all, and a name carrying a NUL, the character that would
    // split one record into two wherever the manager writes who acted. The
    // shape of the name answers, so the accounts are never asked: a name that
    // is no name is never answered from them.
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        for name in ["", "ali\u{0}ce"] {
            assert_eq!(
                m.write_file(
                    &Requester {
                        caller: bff.clone(),
                        principal: Some(Principal(name.into())),
                    },
                    "models.ini",
                    &third,
                    false
                ),
                Err(ManagerError::NotAuthorized),
                "{name:?}: an illegible name is refused"
            );
        }
        assert_eq!(
            text_of("models.ini"),
            other,
            "an illegible name wrote nothing"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            Vec::<String>::new(),
            "the name is checked for shape before the account behind it is looked up"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            vec![bff.clone(); 2],
            "the caller is the bff here: the name is what fails"
        );
        assert_eq!(polkit(&m), vec![], "polkit is not asked for a refused name");
    }

    // ── A model profile file read is gated too ──────────────────────────────────
    // The model profile files need no authorization of the caller, so the principal
    // gate is all that stands between these requests and the file: a
    // non-member's read and a caller outside the bff's read answer nothing.
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        assert_eq!(
            m.read_file(Some(&bob), "models.ini"),
            Err(ManagerError::NotAuthorized),
            "a non-member reads no preset file, not even a blanked one"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["bob".to_string()],
            "the membership is what refused this read"
        );
        assert_eq!(
            polkit(&m),
            vec![],
            "polkit decides nothing about a preset file"
        );
    }
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        assert_eq!(
            m.read_file(
                Some(&Requester {
                    caller: Caller(":1.42".into()),
                    principal: Some(Principal("alice".into())),
                }),
                "models.ini"
            ),
            Err(ManagerError::NotAuthorized),
            "a caller outside the bff reads nothing as alice"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            Vec::<String>::new(),
            "a caller outside the bff never reaches the membership lookup"
        );
        assert_eq!(
            m.read_file(Some(&alice), "models.ini"),
            Ok(Some(plain(&other))),
            "the member the bff names does read it: the gate refuses these calls, not all of them"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["alice".to_string()],
            "the read that is answered is the one that looked the membership up"
        );
    }
}

/// E2E phase for the log of who acted: every guarded call (start, stop,
/// restart, read_file, write_file, set_boot_enabled), done or refused,
/// writes exactly one record through the `ActionLog` seam installed with
/// `Manager::with_log`, naming the action, the target (instance id or file
/// name), the caller, the principal (`-` when none) and the outcome. Values
/// are written with Rust's `{:?}` quoting, so a caller or principal
/// carrying a quote, a newline or a NUL cannot forge a second record. Never
/// a password, never file contents. Unguarded calls record nothing. The
/// record format is literal text, as the comments written into the served
/// files: administrators and scripts read it.
#[cfg(test)]
pub(crate) fn manager_action_log_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n",
    );
    let other = real_model_files(
        dir.path(),
        "[Qwen3.8-32B]\nmodel = /m/Qwen3.8-32B-Q4_K_M.gguf\n",
    );
    std::fs::write(sys.join("server@default.ini"), ini).expect("instance file");
    std::fs::write(sys.join("models.ini"), &model_profiles).expect("presets file");
    write_record(&sys, &["default"]);
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");

    let bff = Caller(":1.9".into());
    let alice = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let bob = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let desktop = Caller(":1.42".into());

    // A manager wired to a fresh fake log: the records land in the `lines`
    // handle the test keeps beside it.
    let core = |allow: bool, passwords: FakePasswords, service: FakeService, log: FakeActionLog| {
        Manager::new(
            service,
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(log)
    };
    let records =
        |lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>| lines.lock().unwrap().clone();

    // ── One record per guarded call done, in the pinned format ──────────
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), FakeService::new(), log);
        assert_eq!(m.start(&alice, "default"), Ok(()));
        assert_eq!(m.stop(&alice, "default"), Ok(()));
        assert_eq!(m.restart(&alice, "default"), Ok(()));
        assert_eq!(
            m.set_boot_enabled(&alice, "default", true),
            Ok(()),
            "start at boot is a guarded call too"
        );
        assert_eq!(m.write_file(&alice, "models.ini", &other, false), Ok(()));
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(ini)))
        );
        assert_eq!(
            m.read_file(Some(&alice), "models.ini"),
            Ok(Some(plain(&other))),
            "the preset file needs no right, yet the read of it is recorded"
        );
        let recs = records(&lines);
        let got: Vec<&str> = recs.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            vec![
                r#"action="start" target="default" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="stop" target="default" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="restart" target="default" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="set_boot_enabled" target="default" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file" target="models.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="read_file" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="read_file" target="models.ini" caller=":1.9" principal="alice" outcome=ok"#,
            ],
            "one record per guarded call: action, target (id or file name), caller, principal, outcome, in this order, all values {{:?}}-quoted, ok unquoted"
        );
        for line in records(&lines) {
            assert!(!line.contains('\n'), "a record is one line: {line:?}");
            assert!(!line.contains('\0'), "a record carries no NUL: {line:?}");
        }
    }

    // ── Refusals and failures: one record each, refused or the error name ─
    {
        let mut service = FakeService::new();
        service.failing = vec!["default".into()];
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), service, log);
        assert_eq!(m.start(&bob, "default"), Err(ManagerError::NotAuthorized));
        assert_eq!(
            m.write_file(&bob, "models.ini", &model_profiles, false),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.read_file(Some(&bob), "server@default.ini"),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.start(&alice, "ghost"),
            Err(ManagerError::NoInstance),
            "a valid id with no server@ghost.ini"
        );
        assert_eq!(
            m.start(&alice, "../x"),
            Err(ManagerError::InvalidId),
            "not an id at all"
        );
        assert_eq!(
            m.write_file(&alice, "nope.ini", &model_profiles, false),
            Err(ManagerError::BadFileName)
        );
        let failed = m
            .start(&alice, "default")
            .expect_err("the fake service fails");
        assert!(
            matches!(failed, ManagerError::Service(_)),
            "the service manager's own failure: {failed:?}"
        );
        // logtest joins the record: a save of a profile no instance runs is
        // judged against nothing, so the collision with default would pass.
        write_record(&sys, &["default", "logtest"]);
        assert_eq!(
            m.write_file(&alice, "server@logtest.ini", ini, false),
            Err(ManagerError::AddressInUse("default".into())),
            "the new instance collides with default on 127.0.0.1:8080"
        );
        assert_eq!(
            m.read_file(None::<&Caller>, "server@default.ini"),
            Err(ManagerError::NotAuthorized),
            "a request with no requester reads nothing"
        );
        let recs = records(&lines);
        let got: Vec<&str> = recs.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            vec![
                r#"action="start" target="default" caller=":1.9" principal="bob" outcome=refused"#,
                r#"action="write_file" target="models.ini" caller=":1.9" principal="bob" outcome=refused"#,
                r#"action="read_file" target="server@default.ini" caller=":1.9" principal="bob" outcome=refused"#,
                r#"action="start" target="ghost" caller=":1.9" principal="alice" outcome="NoInstance""#,
                r#"action="start" target="../x" caller=":1.9" principal="alice" outcome="InvalidId""#,
                r#"action="write_file" target="nope.ini" caller=":1.9" principal="alice" outcome="BadFileName""#,
                r#"action="start" target="default" caller=":1.9" principal="alice" outcome="Service""#,
                r#"action="write_file" target="server@logtest.ini" caller=":1.9" principal="alice" outcome="AddressInUse""#,
                r#"action="read_file" target="server@default.ini" caller=- principal=- outcome=refused"#,
            ],
            "a refusal is recorded too: `refused` for NotAuthorized, the error's own name (quoted) for a failure, with the target as requested and `caller=-` for a request with no requester"
        );
    }
    // The record is one instance again: the blocks below list the instances.
    write_record(&sys, &["default"]);

    // ── The caller's own call: recorded with `principal=-` ───────────────
    // The files here are as the blocks above left them: `models.ini` holds
    // `other`, the text the first block wrote (this block runs after it, in
    // this one phase).
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), FakeService::new(), log);
        assert_eq!(m.start(&desktop, "default"), Ok(()));
        assert_eq!(m.stop(&desktop, "default"), Ok(()));
        assert_eq!(
            m.read_file(Some(&desktop), "server@default.ini"),
            Ok(Some(plain(ini))),
            "a desktop reader here may configure, so nothing is withheld"
        );
        assert_eq!(
            m.read_file(Some(&desktop), "models.ini"),
            Ok(Some(plain(&other))),
            "a preset file needs no right, yet it is recorded like any guarded call"
        );
        let recs = records(&lines);
        let got: Vec<&str> = recs.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            vec![
                r#"action="start" target="default" caller=":1.42" principal=- outcome=ok"#,
                r#"action="stop" target="default" caller=":1.42" principal=- outcome=ok"#,
                r#"action="read_file" target="server@default.ini" caller=":1.42" principal=- outcome=ok"#,
                r#"action="read_file" target="models.ini" caller=":1.42" principal=- outcome=ok"#,
            ],
            "a call with no principal is recorded with `principal=-`"
        );
    }
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, FakePasswords::new(), FakeService::new(), log);
        assert_eq!(
            m.start(&desktop, "default"),
            Err(ManagerError::NotAuthorized),
            "polkit refuses this caller"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="start" target="default" caller=":1.42" principal=- outcome=refused"#
                    .to_string()
            ],
            "the refused call of a caller with no principal is recorded, refused"
        );
    }

    // ── Never a password, never file contents ────────────────────────────
    // The text written and read here carries a marker: the write really
    // lands (the file on disk proves the record below exists for a real
    // write), and no record names the marker.
    {
        let secret = "sk-LOG-NEVER-9d21";
        let secret_model_profiles = real_model_files(
            dir.path(),
            &format!("[Qwen3.8-27B]\nmodel = /m/{secret}.gguf\n"),
        );
        let secret_ini = ini.replace("sk-local-3f9a", secret);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), FakeService::new(), log);
        assert_eq!(
            m.write_file(&alice, "models.ini", &secret_model_profiles, false),
            Ok(())
        );
        assert_eq!(
            m.write_file(&alice, "server@default.ini", &secret_ini, false),
            Ok(())
        );
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(&secret_ini)))
        );
        assert_eq!(
            m.write_file(&bob, "models.ini", &secret_model_profiles, false),
            Err(ManagerError::NotAuthorized),
            "the refused write of a secret is a record as well"
        );
        assert!(
            text_of("server@default.ini").contains(secret),
            "the write of the secret landed: its record is below"
        );
        let recs = records(&lines);
        let got: Vec<&str> = recs.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            vec![
                r#"action="write_file" target="models.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="read_file" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file" target="models.ini" caller=":1.9" principal="bob" outcome=refused"#,
            ]
        );
        for line in records(&lines) {
            assert!(
                !line.contains(secret),
                "a record carries the target and the outcome, never the text: {line:?}"
            );
        }
    }

    // ── A record cannot be forged: names are written {:?}-quoted ─────────
    // Each hostile value runs ONE guarded call on a manager of its own: the
    // fake log must hold exactly one record, on one line, with the value
    // written whole (Rust's own escaping, pinned by formatting the same
    // value here).
    {
        let cases: Vec<(Caller, Option<Principal>, &str)> = vec![
            (bff.clone(), Some(Principal("ali\"ce".into())), "refused"),
            (bff.clone(), Some(Principal("ali\nce".into())), "refused"),
            (bff.clone(), Some(Principal("ali\u{0}ce".into())), "refused"),
            (Caller(":1.9\"x".into()), None, "ok"),
            (Caller(":1.9\nx".into()), None, "ok"),
            (Caller(":1.9\u{0}x".into()), None, "ok"),
        ];
        for (caller, principal, outcome) in cases {
            let who = Requester {
                caller: caller.clone(),
                principal: principal.clone(),
            };
            let log = FakeActionLog::new();
            let lines = log.lines.clone();
            let m = core(true, FakePasswords::new(), FakeService::new(), log);
            let answer = m.start(&who, "default");
            if outcome == "ok" {
                assert_eq!(answer, Ok(()), "{who:?}: the call itself runs");
            } else {
                assert_eq!(
                    answer,
                    Err(ManagerError::NotAuthorized),
                    "{who:?}: the principal gate refuses"
                );
            }
            let recs = records(&lines);
            assert_eq!(
                recs.len(),
                1,
                "{who:?}: exactly one record, a hostile name forged no second"
            );
            let line = &recs[0];
            assert!(!line.contains('\n'), "{who:?}: one line: {line:?}");
            assert!(!line.contains('\0'), "{who:?}: no raw NUL: {line:?}");
            assert!(
                line.contains(&format!("caller={:?}", caller.0)),
                "the caller written {{:?}}-quoted in: {line}"
            );
            if let Some(named) = &principal {
                assert!(
                    line.contains(&format!("principal={:?}", named.0)),
                    "the principal written {{:?}}-quoted in: {line}"
                );
            }
            assert!(
                line.ends_with(&format!("outcome={outcome}")),
                "the record ends with the outcome: {line}"
            );
        }
        // The quote is no control character: that principal is a well formed
        // name, so the refusal is the account lookup, and the record shows
        // the name exactly as asked, escaped inside the quoted value.
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, p, FakeService::new(), log);
        assert_eq!(
            m.start(
                &Requester {
                    caller: bff.clone(),
                    principal: Some(Principal("ali\"ce".into())),
                },
                "default"
            ),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["ali\"ce".to_string()],
            "refused as an unknown account, not as an illegible name"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="start" target="default" caller=":1.9" principal="ali\"ce" outcome=refused"#
                    .to_string()
            ],
            "the quote is escaped inside the quoted value: one line, one record"
        );
    }

    // ── Unguarded calls record nothing ───────────────────────────────────
    // The list, the reads, the sign-in: no record, so the sign-in password
    // is nowhere in the log. `Version` is answered by the transport faces
    // (manager/pipe.rs, manager/dbus.rs), not by the core, and reaches no
    // record.
    {
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), FakeService::new(), log);
        assert_eq!(m.list_instances(), vec!["default".to_string()]);
        assert_eq!(m.status("default"), Ok(UnitState::Active));
        assert_eq!(m.status("../x"), Err(ManagerError::InvalidId));
        assert_eq!(m.boot_enabled("default"), Ok(false));
        assert_eq!(
            m.authenticate(&bff, "alice", "s3cret-Pw"),
            Ok((true, true)),
            "the sign-in that succeeds"
        );
        assert_eq!(
            m.authenticate(&desktop, "alice", "s3cret-Pw"),
            Err(ManagerError::NotAuthorized),
            "the sign-in refused to a caller that is not the bff"
        );
        assert_eq!(
            records(&lines),
            Vec::<String>::new(),
            "no record for a call that is not guarded, so no password ever reaches the log"
        );
        for line in records(&lines) {
            assert!(
                !line.contains("s3cret-Pw"),
                "the sign-in password is never in a record: {line:?}"
            );
        }
    }

    // ── A literal format, not a strings-file text ────────────────────────
    // Administrators and scripts read the records, like the comments written
    // into the served files, so the rendering is literal text in this file's
    // production code (before the first `#[cfg(test)]`: the fakes, the phases
    // and this test are cut away) and asks no strings file.
    {
        let source = std::include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/manager/service.rs"
        ));
        let production = source
            .split("\n#[cfg(test)]\n")
            .next()
            .expect("production text of service.rs");
        for field in [
            concat!("act", "ion="),
            concat!("tar", "get="),
            concat!("call", "er="),
            concat!("prin", "cipal="),
            concat!("outc", "ome="),
        ] {
            assert!(
                production.contains(field),
                "the record renders {field:?} as literal text, in the code that builds the line"
            );
        }
        for call in [concat!("strings", "::t("), concat!("strings", "::fill(")] {
            assert!(
                !production.contains(call),
                "the record is not a strings-file text: {call:?} has no place in this file"
            );
        }
    }

    // ── The file sink under the data root: the path and the records ──────
    // The Windows service appends its records to a file (Linux writes stderr,
    // the journal reads that). Its path is derived from the framework's
    // machine-wide directory, taken through `paths::system_config_dir`, so the
    // redirect this phase set keeps the file inside this temp tree: the real
    // machine tree is never touched. One line per guarded call, appended, as
    // the sinks' own tests pin for a plain `Write`.
    {
        use super::action_log::{manager_log_path, FileActionLog, MANAGER_LOG_FILE};

        let path = manager_log_path();
        assert!(
            path.starts_with(dir.path()),
            "the record file is inside the redirected data root, never the real machine \
             tree: {path:?}"
        );
        assert_eq!(
            path,
            dir.path().join("logs").join(MANAGER_LOG_FILE),
            "a `logs` directory beside the framework's machine directory (its config dir \
             is `<root>/system-config`, so the root is its parent), named after the service"
        );

        // A manager whose log is the file sink, at the path above.
        let file_core = |allow: bool| {
            Manager::new(
                FakeService::new(),
                FakeAuthority::new(allow).quietly(true).bff(&bff),
                FakeResolver::lan(),
            )
            .with_log(FileActionLog::new(&path))
        };

        let m = file_core(true);
        assert_eq!(m.stop(&desktop, "default"), Ok(()));
        assert_eq!(m.restart(&desktop, "default"), Ok(()));
        let text = std::fs::read_to_string(&path).expect("the file the sink created");
        assert_eq!(
            text,
            "action=\"stop\" target=\"default\" caller=\":1.42\" principal=- outcome=ok\n\
             action=\"restart\" target=\"default\" caller=\":1.42\" principal=- outcome=ok\n",
            "one record per guarded call, whole lines, appended in order"
        );

        // A refusal reaches the file too, as the line the core renders for it.
        let m = file_core(false);
        assert_eq!(
            m.stop(&desktop, "default"),
            Err(ManagerError::NotAuthorized),
            "polkit refuses this caller"
        );
        let text = std::fs::read_to_string(&path).expect("the file after the refusal");
        assert_eq!(
            text.lines().count(),
            3,
            "the refusal is one more line: {text:?}"
        );
        assert_eq!(
            text.lines().last(),
            Some(r#"action="stop" target="default" caller=":1.42" principal=- outcome=refused"#),
            "the refusal is recorded with the answer the caller got: {text:?}"
        );

        // A later run opens the same path and appends: nothing is truncated.
        let m = file_core(true);
        assert_eq!(m.stop(&desktop, "default"), Ok(()));
        let text = std::fs::read_to_string(&path).expect("the file after the second run");
        assert_eq!(
            text.lines().count(),
            4,
            "a fresh sink over the same path appends, it never truncates: {text:?}"
        );
        assert!(
            text.starts_with(
                "action=\"stop\" target=\"default\" caller=\":1.42\" principal=- outcome=ok\n\
                 action=\"restart\""
            ),
            "the first run's lines are still there: {text:?}"
        );
        assert!(
            !text.contains("sk-local-3f9a"),
            "a record names the file it touched, never its contents: {text:?}"
        );

        // A record that cannot be written is lost and the action stands: the
        // path's parent is a regular file, so no log file can be made.
        let blocker = dir.path().join("not-a-dir");
        std::fs::write(&blocker, "not a directory").expect("write the blocker");
        let blocked = blocker.join(MANAGER_LOG_FILE);
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_log(FileActionLog::new(&blocked));
        assert_eq!(
            m.stop(&desktop, "default"),
            Ok(()),
            "the record is lost, the stop stands"
        );
        assert!(
            !blocked.exists() && blocker.is_file(),
            "the sink created nothing over an unwritable path: {blocker:?}"
        );
    }
}

/// E2E phase for the compare-and-swap write (`write_file_if`), for this
/// crate's e2e test to call (it sets the process-wide `PRIVATE_AI_DATA_ROOT`
/// redirect, `src/tests/mod.rs`). `expected` is the whole text the caller
/// read: equal bytes run `write_file`'s checks and its write; a file that is
/// missing, or holds any other bytes, is `Changed` with nothing written. Each
/// block redirects a data root of its own, so it reads only the files it
/// wrote. The seams are fakes: no D-Bus, systemd, polkit or DNS.
#[cfg(test)]
pub(crate) fn manager_write_file_if_e2e() {
    use fakes::{
        FakeActionLog, FakeAuthority, FakeGroup, FakePasswords, FakeResolver, FakeService, Gate,
        GateResolver, GATE_WAIT,
    };
    use pai_core::checks::Refusal;
    use std::sync::Arc;
    use std::time::Duration;

    let bff = Caller(":1.9".into());
    let desktop = Caller(":1.42".into());
    let member = Requester {
        caller: bff.clone(),
        principal: Some(Principal("alice".into())),
    };
    let outsider = Requester {
        caller: bff.clone(),
        principal: Some(Principal("bob".into())),
    };
    let fixtures = tempfile::tempdir().expect("tempdir");
    let model_profiles = real_model_files(
        fixtures.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\nctx-size = 172032\n",
    );
    let other = real_model_files(
        fixtures.path(),
        "[Qwen3.8-32B]\nmodel = /models/Qwen3.8-32B-Q4_K_M.gguf\n",
    );
    let default_ini = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 2\n";
    let invalid = |key: &'static str, values: &[&str]| {
        Err(ManagerError::Invalid(Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        }))
    };

    /// A data root of its own, holding `files`: a block never reads another
    /// block's writes.
    fn root(files: &[(&str, &str)]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        std::fs::create_dir_all(&sys).expect("engine config dir");
        for (name, text) in files {
            std::fs::write(sys.join(name), text).expect("fixture file");
        }
        (dir, sys)
    }
    /// The names in the directory, sorted: a refused write leaves it as it was,
    /// with no temp file added.
    fn listing(sys: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(sys)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
    fn text_of(sys: &std::path::Path, name: &str) -> String {
        std::fs::read_to_string(sys.join(name)).expect("file on disk")
    }
    fn records(lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Vec<String> {
        lines.lock().unwrap().clone()
    }
    // A manager whose caller's own calls are gated by polkit `allow`, with
    // a fake password checker for the principal checks and a fake log.
    let core = |allow: bool, passwords: FakePasswords, log: FakeActionLog| {
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(allow).quietly(true).bff(&bff),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords)
        .with_log(log)
    };

    // ── The gates run first, as write_file's: the read is never reached ──
    // The served files here are directories: a read of one fails, so a
    // compare that ran before the gate would answer `Io`, not the refusal.
    {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        let names = ["models.ini", "server@default.ini"];
        for name in names {
            std::fs::create_dir_all(sys.join(name)).expect("unreadable fixture");
        }
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, FakePasswords::new(), log);
        for name in names {
            assert_eq!(
                m.write_file_if(&desktop, name, &other, &model_profiles),
                Err(ManagerError::NotAuthorized),
                "{name}: the refusal write_file gives a caller without configure"
            );
        }
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(desktop.clone(), Action::Configure); 2],
            "configure is asked once per call, before anything is read"
        );
        assert!(
            m.authority().asked_quietly.lock().unwrap().is_empty(),
            "a write never asks the question made without a prompt"
        );
        assert_eq!(
            records(&lines),
            names
                .iter()
                .map(|name| format!(
                    "action=\"write_file_if\" target={name:?} caller=\":1.42\" principal=- outcome=refused"
                ))
                .collect::<Vec<String>>(),
            "one record per refused call, naming the file as it was asked for"
        );
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "a refused write reaches no service manager"
        );
        for name in names {
            assert!(
                std::fs::read_to_string(sys.join(name)).is_err(),
                "the fixture is still the unreadable directory: {name}"
            );
        }
        assert_eq!(
            listing(&sys),
            names,
            "the refused calls added nothing, no temp file"
        );
    }

    // ── Equal bytes: written, and checked as write_file checks ───────────
    {
        let (dir, sys) = root(&[
            ("models.ini", &model_profiles),
            ("server@default.ini", default_ini),
        ]);
        write_record(&sys, &["default"]);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        // A marker in the text written and one in the text expected: the
        // record names the file, never what is in it.
        let marker = "sk-CAS-NEVER-4b71";
        let newer = format!("{default_ini}OpencodeApiKey = {marker}\n");
        assert_eq!(
            m.write_file_if(&desktop, "server@default.ini", &newer, default_ini),
            Ok(()),
            "the file holds the text the caller expected: written"
        );
        assert_eq!(
            text_of(&sys, "server@default.ini"),
            newer,
            "the write landed as sent"
        );
        assert_eq!(
            m.read_file(Some(&desktop), "server@default.ini"),
            Ok(Some(plain(&newer))),
            "the write reads back whole"
        );
        assert_eq!(
            listing(&sys),
            [
                "instances.ini",
                "models.ini",
                "models@default.ini",
                "server@default.ini"
            ],
            "exactly the files written (the record of the instance, the pool, and \
             the server write's render of the instance's presets file), no temp file left"
        );
        assert!(
            m.service().calls.lock().unwrap().is_empty(),
            "a write calls no service manager"
        );
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![
                (desktop.clone(), Action::Configure),
                (desktop.clone(), Action::Control),
            ],
            "configure for the write, control for the read of the instance file"
        );
        // The checks of write_file run on the text written: an equal
        // expected does not skip them, and the old file is kept.
        assert_eq!(
            m.write_file_if(
                &desktop,
                "server@default.ini",
                "[Server]\nPort = 0\nHostname = 127.0.0.1\n",
                &newer
            ),
            invalid("not-whole-number-in-range", &["Port", "0", "1", "65535"]),
            "an equal expected does not skip the content checks"
        );
        assert_eq!(
            text_of(&sys, "server@default.ini"),
            newer,
            "the refused text left the file"
        );
        let bad_model_profiles = real_model_files(dir.path(), &format!(
            "{model_profiles}[Other]\nmodel = /models/o.gguf\nmoe-cache-mib = 4096\nsplit-mode = tensor\n"
        ));
        assert_eq!(
            m.write_file_if(&desktop, "models.ini", &bad_model_profiles, &model_profiles),
            invalid("moe-cache-with-tensor-split", &[]),
            "the preset file's checks run too"
        );
        assert_eq!(
            text_of(&sys, "models.ini"),
            model_profiles,
            "the refused text left the file"
        );
        // The compare is the first thing under the writing lock: a stale
        // expected is Changed, whatever the contents would have failed on.
        assert_eq!(
            m.write_file_if(&desktop, "models.ini", &bad_model_profiles, default_ini),
            Err(ManagerError::Changed),
            "the compare answers before any check runs"
        );
        assert_eq!(
            text_of(&sys, "models.ini"),
            model_profiles,
            "nothing written for a stale expected"
        );
        let recs = records(&lines);
        let got: Vec<&str> = recs.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            vec![
                r#"action="write_file_if" target="server@default.ini" caller=":1.42" principal=- outcome=ok"#,
                r#"action="read_file" target="server@default.ini" caller=":1.42" principal=- outcome=ok"#,
                r#"action="write_file_if" target="server@default.ini" caller=":1.42" principal=- outcome="Invalid""#,
                r#"action="write_file_if" target="models.ini" caller=":1.42" principal=- outcome="Invalid""#,
                r#"action="write_file_if" target="models.ini" caller=":1.42" principal=- outcome="Changed""#,
            ],
            "one record per call, under the method's own name, the outcome the error's own name"
        );
        for line in records(&lines) {
            assert!(
                !line.contains(marker),
                "a record never carries the text written: {line:?}"
            );
            assert!(
                !line.contains("Qwen3.8-27B"),
                "a record never carries the text expected: {line:?}"
            );
            assert!(!line.contains('\n'), "a record is one line: {line:?}");
        }
    }

    // ── Any other bytes: Changed, nothing written ────────────────────────
    {
        let (_dir, sys) = root(&[
            ("models.ini", &model_profiles),
            ("server@default.ini", default_ini),
        ]);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        let before = listing(&sys);
        // Every one of these is the file's own text with a byte added,
        // dropped, moved or translated: the compare is byte for byte, not a
        // comparison of a normalized form.
        let stale: Vec<String> = vec![
            other.to_string(),
            model_profiles.trim_end().to_string(),
            format!("{model_profiles} "),
            model_profiles[..model_profiles.len() - 1].to_string(),
            format!("; note\n{model_profiles}"),
            model_profiles.replace('\n', "\r\n"),
            model_profiles.replace("172032", "172033"),
            model_profiles.to_uppercase(),
        ];
        for expected in &stale {
            assert_eq!(
                m.write_file_if(&desktop, "models.ini", &other, expected),
                Err(ManagerError::Changed),
                "expected {expected:?} against the file on disk"
            );
        }
        assert_eq!(
            text_of(&sys, "models.ini"),
            model_profiles,
            "the refused writes changed nothing"
        );
        assert_eq!(listing(&sys), before, "nothing added, no temp file");
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            vec![(desktop.clone(), Action::Configure); stale.len()],
            "the gate is asked for every call, before the read"
        );
        let recs = records(&lines);
        assert_eq!(
            recs.first().map(String::as_str),
            Some(
                r#"action="write_file_if" target="models.ini" caller=":1.42" principal=- outcome="Changed""#
            ),
            "the record of a refused compare-and-swap: the outcome is Changed"
        );
        assert_eq!(
            recs.iter().filter(|line| line.contains("Changed")).count(),
            stale.len(),
            "one record per refused call"
        );
        // The file's own bytes are accepted: the table above is not green
        // because every compare refuses.
        assert_eq!(
            m.write_file_if(&desktop, "models.ini", &other, &model_profiles),
            Ok(()),
            "the expected text is the file's own: written"
        );
        assert_eq!(text_of(&sys, "models.ini"), other, "the write landed");
    }

    // ── A missing file: Changed, and nothing is created ──────────────────
    {
        let (dir, sys) = root(&[("models.ini", &model_profiles)]);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        let disabled = real_model_files(dir.path(), "[Old]\nmodel = /models/old.gguf\n");
        assert_eq!(
            m.write_file_if(&desktop, "models.ini.disabled", &disabled, &model_profiles),
            Err(ManagerError::Changed),
            "the caller expects a file that is not there"
        );
        assert_eq!(
            m.write_file_if(&desktop, "models.ini.disabled", &disabled, &disabled),
            Err(ManagerError::Changed),
            "an expected equal to the contents does not make the file exist"
        );
        assert!(
            !sys.join("models.ini.disabled").exists(),
            "a refused compare-and-swap creates no file"
        );
        assert_eq!(listing(&sys), ["models.ini"], "nothing created");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="write_file_if" target="models.ini.disabled" caller=":1.42" principal=- outcome="Changed""#
                    .to_string(),
                r#"action="write_file_if" target="models.ini.disabled" caller=":1.42" principal=- outcome="Changed""#
                    .to_string(),
            ],
            "a missing file is recorded as Changed, not as a failure"
        );
    }
    // The directory is not there at all: the compare reads, it creates nothing.
    {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        let m = core(true, FakePasswords::new(), FakeActionLog::new());
        assert_eq!(
            m.write_file_if(&desktop, "models.ini", &model_profiles, &model_profiles),
            Err(ManagerError::Changed),
            "no file to compare with"
        );
        assert!(!sys.exists(), "the refused write created no directory");
    }

    // ── The principal form: the pair is the right, the compare is byte for byte
    {
        let ini = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
        let (_dir, sys) = root(&[("models.ini", &model_profiles), ("server@default.ini", ini)]);
        let passwords = FakePasswords::new();
        let asked_admin = passwords.asked_admin.clone();
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        // This bff has no polkit right of its own: the pair does the work.
        let m = core(false, passwords, log);
        let read = m
            .read_file(Some(&member), "server@default.ini")
            .expect("a member's read")
            .expect("the file exists");
        assert_eq!(
            read.text, ini,
            "a member's read carries the key, so the text to expect is the file's own"
        );
        assert_eq!(
            m.write_file_if(
                &member,
                "server@default.ini",
                &format!("{ini}ModelsMax = 2\n"),
                &read.text
            ),
            Ok(()),
            "the text a member read, unblanked, is what the compare accepts"
        );
        assert!(
            text_of(&sys, "server@default.ini").contains("sk-local-3f9a"),
            "the key line the caller sent is written as sent, with no key flag"
        );
        assert_eq!(
            m.write_file_if(&member, "models.ini", &other, &model_profiles),
            Ok(()),
            "a member's compare-and-swap lands with the caller allowed nothing"
        );
        assert_eq!(text_of(&sys, "models.ini"), other, "the write landed");
        assert_eq!(
            m.write_file_if(
                &member,
                "models.ini",
                &model_profiles,
                "[Qwen3.8-9B]\nmodel = /m/x.gguf\n"
            ),
            Err(ManagerError::Changed),
            "a member's stale expected is refused as anyone's"
        );
        assert_eq!(
            text_of(&sys, "models.ini"),
            other,
            "the refused write changed no file"
        );
        assert_eq!(
            m.write_file_if(&outsider, "models.ini", &model_profiles, &other),
            Err(ManagerError::NotAuthorized),
            "a non-member is refused as write_file refuses it"
        );
        assert_eq!(
            text_of(&sys, "models.ini"),
            other,
            "nothing written for a non-member"
        );
        // A caller that is not the bff, naming a member: refused, and the
        // compare never runs (the expected text is the file's own, so a
        // compare that ran would write).
        assert_eq!(
            m.write_file_if(
                &Requester {
                    caller: desktop.clone(),
                    principal: Some(Principal("alice".into())),
                },
                "models.ini",
                &model_profiles,
                &other
            ),
            Err(ManagerError::NotAuthorized),
            "only the bff's account vouches for a user"
        );
        assert_eq!(text_of(&sys, "models.ini"), other, "nothing written");
        assert_eq!(
            *m.authority().asked.lock().unwrap(),
            Vec::<(Caller, Action)>::new(),
            "polkit decides nothing about a principal"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec![
                "alice".to_string(),
                "alice".to_string(),
                "alice".to_string(),
                "alice".to_string(),
                "bob".to_string()
            ],
            "the membership is asked once per call the bff vouches for, of the \
             principal named; the caller that is not the bff is refused before \
             the lookup (check_principal asks is_bff first)"
        );
        assert_eq!(
            records(&lines),
            vec![
                r#"action="read_file" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file_if" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file_if" target="models.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file_if" target="models.ini" caller=":1.9" principal="alice" outcome="Changed""#,
                r#"action="write_file_if" target="models.ini" caller=":1.9" principal="bob" outcome=refused"#,
                r#"action="write_file_if" target="models.ini" caller=":1.42" principal="alice" outcome=refused"#,
            ],
            "a principal's compare-and-swap is recorded with its caller and its principal"
        );
    }

    // ── The compare runs under the writing lock ──────────────────────────
    // The first write is held inside the lock at a slow host lookup. The
    // second is a compare-and-swap whose expected is the file as it is on disk
    // now: it must wait for the lock, find the text it expected gone and
    // answer Changed, leaving the first write on disk. A compare outside the
    // lock writes over the first.
    {
        let held = "[Server]\nPort = 9000\nHostname = slow-box\n";
        let moved = "[Server]\nPort = 9002\nHostname = slow-box\n";
        let late = "[Server]\nPort = 9003\nHostname = slow-box\n";
        let (_dir, sys) = root(&[("server@slow.ini", held)]);
        write_record(&sys, &["slow"]);
        let gate = Gate::new();
        let resolver = GateResolver::new(
            FakeResolver::new(&[("slow-box", &["10.0.0.7"])]),
            "slow-box",
            &gate,
        );
        let m = Arc::new(
            Manager::new(FakeService::new(), FakeAuthority::new(true), resolver)
                .with_group(FakeGroup::new()),
        );
        let first = {
            let (m, who) = (m.clone(), desktop.clone());
            on_thread(move || m.write_file(&who, "server@slow.ini", moved, false))
        };
        assert!(
            gate.arrived(1),
            "the first write's address check reached the slow lookup"
        );
        let second = {
            let (m, who) = (m.clone(), desktop.clone());
            on_thread(move || m.write_file_if(&who, "server@slow.ini", late, held))
        };
        // Time enough for the second to write, if it compared before the lock.
        let early = second.recv_timeout(Duration::from_millis(500));
        gate.open();
        let first = first
            .recv_timeout(GATE_WAIT)
            .expect("the first write returns once the lookup answers");
        let second = second
            .recv_timeout(GATE_WAIT)
            .expect("the compare-and-swap returns once it has the lock");
        assert!(
            !matches!(early, Ok(Ok(()))),
            "the compare-and-swap finished while the first write held the lock: {early:?}"
        );
        assert_eq!(first, Ok(()), "the first write lands");
        assert_eq!(
            second,
            Err(ManagerError::Changed),
            "the write it waited for made its expected text stale"
        );
        assert_eq!(
            text_of(&sys, "server@slow.ini"),
            moved,
            "the first write is what stands on disk"
        );
        assert_eq!(
            listing(&sys),
            ["instances.ini", "models@slow.ini", "server@slow.ini"],
            "no temp file left; the landed write renders the instance's presets file"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::fakes::FakeResolver;
    use super::*;

    fn cfg(port: Option<i32>, host: Option<&str>) -> ServerConfig {
        ServerConfig {
            port,
            hostname: host.map(str::to_string),
            ..Default::default()
        }
    }

    /// With no live key line in the incoming text, the held key goes inside
    /// `[Server]`, not at the end of the file: a hand-edited file with
    /// another section after `[Server]` would otherwise put it in that
    /// section, where nothing reads it, and the key is lost.
    #[test]
    fn a_kept_key_with_no_line_goes_into_the_server_section() {
        let key = "sk-local-3f9a";
        for sent in [
            "[Server]\nPort = 8090\nModelsMax = 2\n\n[Extra]\nFoo = 1\n",
            "; Hand-edited.\n[Server]\nPort = 8090\n; OpencodeApiKey = sk-xxx  ; hint\n[Extra]\nFoo = 1\n",
            "[Server]\r\nPort = 8090\r\n\r\n[Extra]\r\nFoo = 1\r\n",
        ] {
            let out = keep_held_api_key(sent, key);
            let cfg = crate::server_cfg::from_text(&out);
            assert_eq!(cfg.opencode_api_key.as_deref(), Some(key), "{out:?}");
            assert_eq!(cfg.port, Some(8090), "{out:?}");
            let line = out.find("OpencodeApiKey = sk-local").expect("key line");
            let extra = out.find("[Extra]").expect("[Extra] kept");
            assert!(line < extra, "the key before [Extra]: {out:?}");
            assert!(
                out[extra..].trim_end().ends_with("Foo = 1"),
                "[Extra] as sent: {out:?}"
            );
            assert_eq!(
                out.lines()
                    .filter(|l| l.trim_start().starts_with("OpencodeApiKey ="))
                    .count(),
                1,
                "{out:?}"
            );
        }
        // Only [Server]: the line follows its last key, as before.
        assert_eq!(
            keep_held_api_key("[Server]\nPort = 8090\n", key),
            "[Server]\nPort = 8090\nOpencodeApiKey = sk-local-3f9a\n"
        );
    }

    #[test]
    fn action_ids_are_the_agreed_polkit_ids() {
        assert_eq!(Action::Control.id(), "org.privateai.manager.control");
        assert_eq!(Action::Configure.id(), "org.privateai.manager.configure");
    }

    #[test]
    fn config_file_accepts_the_served_files() {
        assert_eq!(
            config_file("server@default.ini"),
            Some(ConfigFile::ServerProfile("default".into()))
        );
        assert_eq!(
            config_file("server@lan-2.ini"),
            Some(ConfigFile::ServerProfile("lan-2".into()))
        );
        assert_eq!(config_file("models.ini"), Some(ConfigFile::Models));
        assert_eq!(
            config_file("models.ini.disabled"),
            Some(ConfigFile::ModelsDisabled)
        );
    }

    #[test]
    fn config_file_refuses_every_other_name() {
        for name in [
            "",
            "../x",
            "../models.ini",
            "./models.ini",
            "sub/models.ini",
            "sub\\models.ini",
            "/etc/private-ai/models.ini",
            "C:\\ProgramData\\private-ai\\config\\models.ini",
            "server.ini",
            "settings.ini",
            "server@Bad.ini",
            "server@.ini",
            "server@a_b.ini",
            "server@../x.ini",
            "server@default.ini.tmp",
            "server@default",
            "models.ini.bak",
            "models.ini.tmp",
            "Presets.ini",
            "models.ini ",
        ] {
            assert_eq!(config_file(name), None, "name {name:?}");
        }
    }

    // ── Instance files hold a secret: reads need control ───────────────
    // Refused before any path is resolved, so these never reach `paths::`.

    fn core(
        allow: bool,
    ) -> Manager<super::fakes::FakeService, super::fakes::FakeAuthority, FakeResolver> {
        Manager::new(
            super::fakes::FakeService::new(),
            super::fakes::FakeAuthority::new(allow),
            FakeResolver::lan(),
        )
        .with_group(super::fakes::FakeGroup::new())
    }

    fn asked(
        m: &Manager<super::fakes::FakeService, super::fakes::FakeAuthority, FakeResolver>,
    ) -> Vec<(Caller, Action)> {
        m.authority().asked.lock().unwrap().clone()
    }

    /// An instance file can hold `OpencodeApiKey`: reading it asks the
    /// authority for control, with the caller; a refusal is NotAuthorized
    /// and nothing is read, whether the instance exists or not (a refused
    /// caller learns nothing about which ids exist).
    #[test]
    fn reading_an_instance_file_asks_control_and_a_refusal_reads_nothing() {
        let alice = Caller(":1.42".into());
        let m = core(false);
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.read_file(Some(&alice), "server@nope.ini"),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(asked(&m), vec![(alice.clone(), Action::Control); 2]);
    }

    /// A transport with no identity for the call (a D-Bus call with no
    /// sender, a pipe connection with no token) gets no instance file, and
    /// nobody is asked, even an authority that would allow it.
    #[test]
    fn reading_an_instance_file_with_no_caller_is_refused_unasked() {
        let m = core(true);
        assert_eq!(
            m.read_file(None::<&Caller>, "server@default.ini"),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(asked(&m), vec![]);
    }

    /// A name that is no served file is refused as one before anybody is
    /// asked, with or without a caller.
    #[test]
    fn a_read_of_a_bad_name_asks_nobody() {
        let alice = Caller(":1.42".into());
        let m = core(false);
        for name in ["server.ini", "../server@default.ini", "server@Bad.ini"] {
            assert_eq!(
                m.read_file(Some(&alice), name),
                Err(ManagerError::BadFileName),
                "{name}"
            );
            assert_eq!(
                m.read_file(None::<&Caller>, name),
                Err(ManagerError::BadFileName),
                "{name}"
            );
        }
        assert_eq!(asked(&m), vec![]);
    }

    // ── Compare-and-swap writes: gates first, as write_file's ───────────
    // These resolve no path: every refusal lands before a path is built,
    // as write_file's do, so these unit tests never reach `paths::`.

    /// A caller without the configure right gets write_file's own refusal,
    /// asked once for configure, never the question made without a prompt,
    /// and the call reaches nothing past the gate.
    #[test]
    fn a_compare_and_swap_write_without_configure_is_the_same_refusal() {
        let alice = Caller(":1.42".into());
        let m = core(false);
        assert_eq!(
            m.write_file_if(&alice, "models.ini", "[X]\n", "[Y]\n"),
            Err(ManagerError::NotAuthorized),
            "the same refusal write_file gives"
        );
        assert_eq!(
            asked(&m),
            vec![(alice.clone(), Action::Configure)],
            "configure is asked once, before the expected text is read"
        );
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![],
            "a write never asks the question made without a prompt"
        );
        assert_eq!(*m.service().calls.lock().unwrap(), vec![]);
    }

    /// A name that is no served file is refused as one, the gate asked
    /// first: NotAuthorized where write_file is refused, BadFileName where
    /// the gate allows, the gate asked once per call either way.
    #[test]
    fn a_compare_and_swap_write_of_a_bad_name_is_refused_as_write_files() {
        let alice = Caller(":1.42".into());
        let m = core(false);
        for name in ["server.ini", "../models.ini", "models.ini.tmp"] {
            assert_eq!(
                m.write_file_if(&alice, name, "[X]\n", "[Y]\n"),
                Err(ManagerError::NotAuthorized),
                "{name}"
            );
        }
        assert_eq!(
            asked(&m),
            vec![(alice.clone(), Action::Configure); 3],
            "the gate runs before the name is looked at"
        );
        let m = core(true);
        for name in ["server.ini", "../models.ini", "models.ini.tmp"] {
            assert_eq!(
                m.write_file_if(&alice, name, "[X]\n", "[Y]\n"),
                Err(ManagerError::BadFileName),
                "{name}"
            );
        }
        assert_eq!(asked(&m), vec![(alice.clone(), Action::Configure); 3]);
        assert_eq!(*m.service().calls.lock().unwrap(), vec![]);
    }

    // ── Start at boot ───────────────────────────────────────────────────
    // Refused before any path is resolved, so these never reach `paths::`.

    /// Enabling an instance at boot is systemd's own `enable`, which asks for
    /// an administrator: the configure action. A refused caller is asked
    /// once, with its identity, and nothing reaches the service manager,
    /// whatever the id.
    #[test]
    fn setting_the_start_at_boot_asks_configure_and_a_refusal_changes_nothing() {
        let alice = Caller(":1.42".into());
        let m = core(false);
        for (id, on) in [("default", true), ("default", false), ("../x", true)] {
            assert_eq!(
                m.set_boot_enabled(&alice, id, on),
                Err(ManagerError::NotAuthorized),
                "{id} {on}"
            );
        }
        assert_eq!(asked(&m), vec![(alice.clone(), Action::Configure); 3]);
        assert_eq!(*m.service().calls.lock().unwrap(), vec![]);
    }

    /// The read is open to anyone, like Status: an invalid id is refused as
    /// one, nobody asked, the service manager untouched.
    #[test]
    fn reading_the_start_at_boot_of_an_invalid_id_asks_nobody() {
        let m = core(false);
        for bad in ["", "../x", "Default", "a_b"] {
            assert_eq!(m.boot_enabled(bad), Err(ManagerError::InvalidId), "{bad:?}");
        }
        assert_eq!(asked(&m), vec![]);
        assert_eq!(*m.service().calls.lock().unwrap(), vec![]);
    }

    /// systemd and the SCM start the enabled instances at boot: the manager
    /// starts none at its own start, on either OS.
    #[test]
    fn the_manager_starts_no_instance_at_its_own_start() {
        let old = concat!("start_all", "_at_boot");
        for (file, text) in [
            ("service.rs", include_str!("service.rs")),
            ("dbus.rs", include_str!("dbus.rs")),
            ("windows/entry.rs", include_str!("windows/entry.rs")),
        ] {
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            assert!(!code.contains(old), "{file} still names {old}");
        }
    }

    /// The manager's start reports the renders that did not land, on both
    /// OSes: the failures reach the report, they are not thrown away.
    #[test]
    fn the_start_reports_the_renders_that_did_not_land() {
        for (name, text) in [
            ("dbus.rs", include_str!("dbus.rs")),
            ("windows/entry.rs", include_str!("windows/entry.rs")),
        ] {
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            assert!(
                code.contains("report_render_failures"),
                "{name}: the start reports the renders that did not land"
            );
        }
    }

    /// The rename of the release: `server@<id>.ini` is a server profile, so
    /// the served file is `ConfigFile::ServerProfile`, and the instance
    /// record is the `instances.ini` constant, not a name typed at each use.
    #[test]
    fn the_served_server_file_is_a_server_profile() {
        let code = std::include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/manager/service.rs"
        ))
        .split("\n#[cfg(test)]\n")
        .next()
        .unwrap_or_default();
        assert!(
            code.contains("ConfigFile::ServerProfile"),
            "the served file is named for the profile"
        );
        assert!(
            !code.contains("ConfigFile::Instance"),
            "the instance is the section of instances.ini, not a file"
        );
        assert!(
            code.contains("INSTANCES_FILE"),
            "the record is named by one constant"
        );
    }

    // ── Instance files: mode 0640, group private-ai ─────────────────────

    /// The instance file can hold a secret: 0640, read by root and the
    /// instances' group. The model profile files hold none: 0644, as before.
    #[test]
    fn file_mode_is_0640_for_instance_files_and_0644_for_the_model_profiles() {
        assert_eq!(
            file_mode(&ConfigFile::ServerProfile("default".into())),
            0o640
        );
        assert_eq!(file_mode(&ConfigFile::ServerProfile("lan-2".into())), 0o640);
        assert_eq!(file_mode(&ConfigFile::Models), 0o644);
        assert_eq!(file_mode(&ConfigFile::ModelsDisabled), 0o644);
    }

    /// The group is the service user's, as the Linux package's sysusers.d
    /// entry names it (xtask `system_files::sysusers`).
    #[test]
    fn the_instance_group_is_the_service_users() {
        assert_eq!(INSTANCE_GROUP, "private-ai");
    }

    #[cfg(unix)]
    mod unix_files {
        use super::super::fakes::{FakeGroup, GroupCall};
        use super::super::*;
        use std::collections::BTreeSet;
        use std::io::ErrorKind;
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        use std::path::Path;

        fn mode(p: &Path) -> u32 {
            std::fs::metadata(p).expect("metadata").permissions().mode() & 0o7777
        }

        fn names(dir: &Path) -> BTreeSet<String> {
            std::fs::read_dir(dir)
                .expect("read_dir")
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        }

        fn calls(g: &FakeGroup) -> Vec<GroupCall> {
            g.calls.lock().unwrap().clone()
        }

        const INI: &str = "[Server]\nPort = 8080\nOpencodeApiKey = sk-local-3f9a\n";

        /// A new instance file: 0640 from the start (the temp file had the
        /// mode when the group was set, so the secret was never world
        /// readable), its group set to private-ai on the temp file before
        /// the rename, no temp file left.
        #[test]
        fn an_instance_file_is_written_0640_with_its_group_set_before_the_rename() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("server@default.ini");
            let g = FakeGroup::new();
            write_served_file(&path, &ConfigFile::ServerProfile("default".into()), INI, &g)
                .expect("write");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), INI);
            assert_eq!(mode(&path), 0o640);
            let c = calls(&g);
            assert_eq!(c.len(), 1, "one group change: {c:?}");
            assert_eq!(c[0].group, "private-ai");
            assert_eq!(
                c[0].mode, 0o640,
                "the temp file was 0640 when its group was set"
            );
            assert_ne!(
                c[0].path, path,
                "the group is set on the temp file, before the rename"
            );
            assert_eq!(
                c[0].path.parent(),
                Some(dir.path()),
                "the temp file is beside it"
            );
            assert!(!c[0].path.exists(), "the temp file was renamed");
            assert_eq!(
                names(dir.path()),
                BTreeSet::from(["server@default.ini".to_string()])
            );
        }

        /// A file the previous version wrote 0644 becomes 0640 on its next
        /// write: the rename replaces it with the new file.
        #[test]
        fn a_0644_instance_file_is_0640_after_its_next_write() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("server@default.ini");
            std::fs::write(&path, "[Server]\nPort = 8081\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            let g = FakeGroup::new();
            write_served_file(&path, &ConfigFile::ServerProfile("default".into()), INI, &g)
                .expect("write");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), INI);
            assert_eq!(mode(&path), 0o640);
            assert_eq!(calls(&g).len(), 1);
        }

        /// The model profile files hold no secret: 0644, whatever was there, and
        /// their group is left alone (root's, the manager's own).
        #[test]
        fn model_profile_files_are_written_0644_and_keep_their_group() {
            let dir = tempfile::tempdir().expect("tempdir");
            let model_profiles = dir.path().join("models.ini");
            std::fs::write(&model_profiles, "[Old]\nmodel = /m/old.gguf\n").unwrap();
            std::fs::set_permissions(&model_profiles, std::fs::Permissions::from_mode(0o600))
                .unwrap();
            let g = FakeGroup::new();
            let text = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
            write_served_file(&model_profiles, &ConfigFile::Models, text, &g)
                .expect("model-profiles");
            let disabled = dir.path().join("models.ini.disabled");
            write_served_file(&disabled, &ConfigFile::ModelsDisabled, text, &g).expect("disabled");
            assert_eq!(std::fs::read_to_string(&model_profiles).unwrap(), text);
            assert_eq!(std::fs::read_to_string(&disabled).unwrap(), text);
            assert_eq!(mode(&model_profiles), 0o644);
            assert_eq!(mode(&disabled), 0o644);
            assert_eq!(calls(&g), vec![], "no group change for a preset file");
            assert_eq!(
                names(dir.path()),
                BTreeSet::from(["models.ini".to_string(), "models.ini.disabled".to_string()])
            );
        }

        /// No group private-ai (the package's sysusers.d entry did not run):
        /// a 0640 root:root file would stop the instance from reading its
        /// config at the next start, and 0644 would show the secret, so the
        /// write is refused with the seam's NotFound, the old file kept and
        /// no temp file left.
        #[test]
        fn a_missing_group_refuses_the_write_and_keeps_the_old_file() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("server@default.ini");
            let old = "[Server]\nPort = 8081\n";
            std::fs::write(&path, old).unwrap();
            let g = FakeGroup::failing(ErrorKind::NotFound);
            let e = write_served_file(&path, &ConfigFile::ServerProfile("default".into()), INI, &g)
                .expect_err("a missing group refuses the write");
            assert_eq!(e.kind(), ErrorKind::NotFound);
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                old,
                "the old file is kept"
            );
            assert_eq!(calls(&g).len(), 1);
            assert_eq!(
                names(dir.path()),
                BTreeSet::from(["server@default.ini".to_string()])
            );

            // A new instance: nothing created at all.
            let new = dir.path().join("server@lan-2.ini");
            let e = write_served_file(&new, &ConfigFile::ServerProfile("lan-2".into()), INI, &g)
                .expect_err("refused");
            assert_eq!(e.kind(), ErrorKind::NotFound);
            assert!(!new.exists());
            assert_eq!(
                names(dir.path()),
                BTreeSet::from(["server@default.ini".to_string()])
            );
        }

        /// A write that cannot create its temp file fails and leaves the
        /// old file and the directory as they were.
        #[test]
        fn a_failed_write_leaves_the_old_file_and_no_temp() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("server@default.ini");
            let old = "[Server]\nPort = 8081\n";
            std::fs::write(&path, old).unwrap();
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
            let probe = std::fs::write(dir.path().join("probe"), "x");
            let g = FakeGroup::new();
            let result =
                write_served_file(&path, &ConfigFile::ServerProfile("default".into()), INI, &g);
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
            if probe.is_ok() {
                eprintln!("skipped: the directory's mode does not stop this user (root)");
                return;
            }
            assert!(result.is_err(), "the write must fail: {result:?}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), old);
            assert_eq!(
                names(dir.path()),
                BTreeSet::from(["server@default.ini".to_string()])
            );
            assert_eq!(
                calls(&g),
                vec![],
                "no group change for a file never written"
            );
        }

        /// The group of `name` and its gid, from the system's group
        /// database (`getent group`): read only.
        fn gid_of(name: &str) -> u32 {
            let out = std::process::Command::new("getent")
                .args(["group", name])
                .output()
                .expect("getent");
            let line = String::from_utf8(out.stdout).expect("utf-8");
            line.trim()
                .split(':')
                .nth(2)
                .expect("gid field")
                .parse()
                .expect("gid")
        }

        /// The real seam: an unknown group is NotFound and the file's group
        /// is unchanged.
        #[test]
        fn the_system_group_seam_refuses_an_unknown_group_with_not_found() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("server@default.ini");
            std::fs::write(&path, INI).unwrap();
            let before = std::fs::metadata(&path).unwrap().gid();
            let e = SystemGroup
                .set_group(&path, "pai-no-such-group-3f9a")
                .expect_err("unknown group");
            assert_eq!(e.kind(), ErrorKind::NotFound, "{e:?}");
            assert_eq!(std::fs::metadata(&path).unwrap().gid(), before);
        }

        /// The real seam on a group this user belongs to (chown(2) lets an
        /// owner give a file any group it is a member of, no root needed):
        /// a supplementary group when there is one, so the change shows.
        #[test]
        fn the_system_group_seam_sets_a_group_of_this_user() {
            let out = std::process::Command::new("id")
                .arg("-Gn")
                .output()
                .expect("id -Gn");
            let groups: Vec<String> = String::from_utf8(out.stdout)
                .expect("utf-8")
                .split_whitespace()
                .map(str::to_string)
                .collect();
            let group = groups.get(1).or(groups.first()).expect("a group").clone();
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("server@default.ini");
            std::fs::write(&path, INI).unwrap();
            let owner = std::fs::metadata(&path).unwrap().uid();
            SystemGroup.set_group(&path, &group).expect("set_group");
            let m = std::fs::metadata(&path).unwrap();
            assert_eq!(m.gid(), gid_of(&group), "group {group}");
            assert_eq!(m.uid(), owner, "the owner is unchanged");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), INI);
        }
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().expect("ip")
    }

    fn bind(port: i32, ips: &[&str]) -> Binding {
        Binding {
            port,
            ips: ips.iter().map(|s| ip(s)).collect(),
        }
    }

    /// The binding of a config through the LAN fake resolver, or the name
    /// that did not resolve.
    fn resolved(port: Option<i32>, host: Option<&str>) -> Result<Binding, String> {
        binding(&cfg(port, host), &FakeResolver::lan())
    }

    fn collide(a: (Option<i32>, Option<&str>), b: (Option<i32>, Option<&str>)) -> bool {
        let a = resolved(a.0, a.1).expect("a resolves");
        let b = resolved(b.0, b.1).expect("b resolves");
        addresses_collide(&a, &b)
    }

    #[test]
    fn binding_refuses_a_host_that_does_not_resolve() {
        assert_eq!(
            resolved(Some(8081), Some("nowhere.example")),
            Err("nowhere.example".into())
        );
        // The first unresolvable host of a list is named.
        assert_eq!(
            resolved(Some(8081), Some("127.0.0.1, gone.example, nowhere.example")),
            Err("gone.example".into())
        );
    }

    #[test]
    fn binding_takes_literal_ips_as_themselves_without_resolving() {
        let r = FakeResolver::new(&[]);
        assert_eq!(
            binding(&cfg(Some(8081), Some("127.0.0.1, ::1, 0.0.0.0, ::")), &r),
            Ok(bind(8081, &["127.0.0.1", "::1", "0.0.0.0", "::"]))
        );
        assert!(
            r.asked.lock().unwrap().is_empty(),
            "a literal IP is not resolved"
        );
    }

    #[test]
    fn binding_resolves_names_and_applies_defaults() {
        let r = FakeResolver::lan();
        assert_eq!(
            binding(&cfg(Some(8081), Some("multi-home")), &r),
            Ok(bind(8081, &["10.0.0.9", "192.168.1.5"]))
        );
        // No Port and no Hostname: the default port on localhost, resolved.
        assert_eq!(
            binding(&cfg(None, None), &r),
            Ok(bind(crate::server_cfg::DEFAULT_PORT, &["127.0.0.1"]))
        );
        assert_eq!(
            binding(&cfg(None, Some("   ")), &r),
            Ok(bind(crate::server_cfg::DEFAULT_PORT, &["127.0.0.1"])),
            "a blank Hostname is localhost"
        );
    }

    #[test]
    fn same_port_and_a_common_address_collide() {
        let a = bind(8081, &["127.0.0.1"]);
        assert!(addresses_collide(&a, &a.clone()));
        assert!(addresses_collide(
            &bind(8081, &["10.0.0.1", "192.168.1.5"]),
            &bind(8081, &["192.168.1.5"])
        ));
        assert!(addresses_collide(
            &bind(8081, &["::1"]),
            &bind(8081, &["::1"])
        ));
    }

    #[test]
    fn different_port_or_disjoint_addresses_do_not_collide() {
        assert!(!addresses_collide(
            &bind(8081, &["127.0.0.1"]),
            &bind(8082, &["127.0.0.1"])
        ));
        assert!(!addresses_collide(
            &bind(8081, &["127.0.0.1"]),
            &bind(8081, &["192.168.1.5"])
        ));
        assert!(!addresses_collide(
            &bind(8081, &["127.0.0.1"]),
            &bind(8081, &["::1"])
        ));
        assert!(!addresses_collide(
            &bind(8081, &["0.0.0.0"]),
            &bind(8082, &["0.0.0.0"])
        ));
    }

    #[test]
    fn a_wildcard_collides_with_every_address_on_its_port() {
        // Both wildcards, IPv4 and IPv6 alike: `::` is dual-stack on Linux by
        // default, and `0.0.0.0` is pinned the same way to stay on the safe
        // side.
        for wild in ["0.0.0.0", "::"] {
            for other in [
                "127.0.0.1",
                "192.168.1.5",
                "::1",
                "fe80::1",
                "0.0.0.0",
                "::",
            ] {
                let w = bind(8081, &[wild]);
                let o = bind(8081, &[other]);
                assert!(addresses_collide(&w, &o), "{wild} vs {other}");
                assert!(addresses_collide(&o, &w), "{other} vs {wild}");
            }
        }
        assert!(addresses_collide(
            &bind(8081, &["10.0.0.1", "0.0.0.0"]),
            &bind(8081, &["192.168.1.5"])
        ));
    }

    #[test]
    fn resolved_names_collide_by_address() {
        // localhost is 127.0.0.1.
        assert!(collide(
            (Some(8081), Some("localhost")),
            (Some(8081), Some("127.0.0.1"))
        ));
        assert!(collide(
            (Some(8081), Some("127.0.0.1")),
            (Some(8081), Some("localhost"))
        ));
        // The default host is localhost.
        assert!(collide(
            (None, None),
            (Some(crate::server_cfg::DEFAULT_PORT), Some("127.0.0.1"))
        ));
        // Two different names of one address.
        assert!(collide(
            (Some(8081), Some("gpu-box")),
            (Some(8081), Some("gpu-box.lan"))
        ));
        assert!(collide(
            (Some(8081), Some("gpu-box")),
            (Some(8081), Some("192.168.1.5"))
        ));
        // A name with several addresses, one matching.
        assert!(collide(
            (Some(8081), Some("multi-home")),
            (Some(8081), Some("gpu-box"))
        ));
        assert!(collide(
            (Some(8081), Some("multi-home")),
            (Some(8081), Some("10.0.0.9"))
        ));
        // No common address.
        assert!(!collide(
            (Some(8081), Some("localhost")),
            (Some(8081), Some("gpu-box"))
        ));
        // Names on different ports never collide.
        for (a, b) in [
            ("localhost", "127.0.0.1"),
            ("gpu-box", "gpu-box.lan"),
            ("multi-home", "gpu-box"),
            ("0.0.0.0", "localhost"),
        ] {
            assert!(
                !collide((Some(8081), Some(a)), (Some(8082), Some(b))),
                "{a} vs {b}"
            );
        }
    }

    #[test]
    fn defaults_count_in_a_collision() {
        assert!(collide((None, None), (None, None)));
        assert!(collide(
            (None, None),
            (Some(crate::server_cfg::DEFAULT_PORT), Some("localhost"))
        ));
        assert!(!collide((None, None), (Some(8081), None)));
        assert!(!collide((None, None), (None, Some("192.168.1.5"))));
    }

    // ── Authenticate: the bff's sign-in, checked by the manager ────────
    // No file is read, so these never reach `paths::`.

    type AuthCore = Manager<super::fakes::FakeService, super::fakes::FakeAuthority, FakeResolver>;

    /// The bff's account as the core sees it (a bus name here; on Windows the
    /// service SID of `NT SERVICE\private-ai-bff`).
    fn bff() -> Caller {
        Caller(":1.9".into())
    }

    /// A manager whose authority takes [`bff`] for the bff's account, answering
    /// `allow` to every polkit question, over `passwords`.
    fn auth_core(allow: bool, passwords: super::fakes::FakePasswords) -> AuthCore {
        Manager::new(
            super::fakes::FakeService::new(),
            super::fakes::FakeAuthority::new(allow).bff(&bff()),
            FakeResolver::lan(),
        )
        .with_group(super::fakes::FakeGroup::new())
        .with_passwords(passwords)
    }

    /// The password checker's logs: (checked, asked_admin).
    type AuthLogs = (
        std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
        std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    );

    fn passwords() -> (super::fakes::FakePasswords, AuthLogs) {
        let p = super::fakes::FakePasswords::new();
        let logs = (p.checked.clone(), p.asked_admin.clone());
        (p, logs)
    }

    fn checked(logs: &AuthLogs) -> Vec<(String, String)> {
        logs.0.lock().unwrap().clone()
    }

    fn asked_admin(logs: &AuthLogs) -> Vec<String> {
        logs.1.lock().unwrap().clone()
    }

    /// Only the bff's account may ask: any other caller, one polkit would
    /// allow everything included, is NotAuthorized and no password is
    /// checked; nor is the empty caller (a pipe connection with no token).
    #[test]
    fn authenticate_answers_only_the_bff() {
        for caller in [
            Caller(":1.42".into()),
            Caller(String::new()),
            Caller("S-1-5-18".into()),
        ] {
            let (p, logs) = passwords();
            let m = auth_core(true, p);
            assert_eq!(
                m.authenticate(&caller, "alice", "s3cret-Pw"),
                Err(ManagerError::NotAuthorized),
                "{caller:?}"
            );
            assert_eq!(checked(&logs), vec![], "{caller:?}: a password checked");
            assert_eq!(asked_admin(&logs), Vec::<String>::new(), "{caller:?}");
            assert_eq!(
                *m.authority().asked_bff.lock().unwrap(),
                vec![caller.clone()],
                "the authority is asked whether {caller:?} is the bff"
            );
        }
    }

    /// The bff's own polkit rights do not matter: it is answered for being
    /// the bff, and nothing asks polkit about it.
    #[test]
    fn authenticate_asks_polkit_nothing() {
        let (p, logs) = passwords();
        let m = auth_core(false, p);
        assert_eq!(
            m.authenticate(&bff(), "alice", "s3cret-Pw"),
            Ok((true, true))
        );
        assert_eq!(asked(&m), vec![]);
        assert_eq!(*m.authority().asked_quietly.lock().unwrap(), vec![]);
        assert_eq!(checked(&logs).len(), 1);
    }

    /// An empty user or password is no: (false, false), the checker never
    /// asked (PAM would take an empty password for some accounts).
    #[test]
    fn an_empty_user_or_password_is_no_and_asks_no_checker() {
        for (user, password) in [("", "s3cret-Pw"), ("alice", ""), ("", "")] {
            let (p, logs) = passwords();
            let m = auth_core(true, p);
            assert_eq!(
                m.authenticate(&bff(), user, password),
                Ok((false, false)),
                "{user:?}"
            );
            assert_eq!(checked(&logs), vec![], "{user:?}");
            assert_eq!(asked_admin(&logs), Vec::<String>::new(), "{user:?}");
        }
    }

    /// A wrong password and an unknown user get the same answer, and the
    /// group membership of nobody is looked up.
    #[test]
    fn a_wrong_password_and_an_unknown_user_are_the_same_no() {
        for (user, password) in [
            ("alice", "wrong"),
            ("alice", "S3CRET-PW"),
            ("bob", "s3cret-Pw"),
            ("mallory", "s3cret-Pw"),
            ("mallory", "anything"),
        ] {
            let (p, logs) = passwords();
            let m = auth_core(true, p);
            assert_eq!(
                m.authenticate(&bff(), user, password),
                Ok((false, false)),
                "{user:?} / {password:?}"
            );
            assert_eq!(
                checked(&logs),
                vec![(user.to_string(), password.to_string())]
            );
            assert_eq!(
                asked_admin(&logs),
                Vec::<String>::new(),
                "{user:?}: membership looked up after a failed check"
            );
        }
    }

    /// The right password: ok, and whether the user may configure, asked
    /// for that user only after the check.
    #[test]
    fn a_right_password_says_whether_the_user_may_configure() {
        for (user, password, admin) in [("alice", "s3cret-Pw", true), ("bob", "hunter2!", false)] {
            let (p, logs) = passwords();
            let m = auth_core(true, p);
            assert_eq!(
                m.authenticate(&bff(), user, password),
                Ok((true, admin)),
                "{user}"
            );
            assert_eq!(
                checked(&logs),
                vec![(user.to_string(), password.to_string())]
            );
            assert_eq!(asked_admin(&logs), vec![user.to_string()]);
        }
    }

    /// The user and password reach the checker exactly as sent: nothing
    /// trimmed or folded (a password may start or end with a space).
    #[test]
    fn the_user_and_password_reach_the_checker_unchanged() {
        let (p, logs) = passwords();
        let m = auth_core(true, p);
        let _ = m.authenticate(&bff(), " alice", " s3cret-Pw ");
        let _ = m.authenticate(&bff(), "Alice", "pässwörd\u{1f511}");
        assert_eq!(
            checked(&logs),
            vec![
                (" alice".to_string(), " s3cret-Pw ".to_string()),
                ("Alice".to_string(), "pässwörd\u{1f511}".to_string()),
            ]
        );
    }

    /// A checker that fails (PAM unusable) is a Service error with its own
    /// message, not a no: the bff can tell "try again later" from "wrong
    /// password". The password is in neither the error nor its reply.
    #[test]
    fn a_failing_checker_is_a_service_error_without_the_password() {
        let failure =
            "pam_authenticate: Authentication service cannot retrieve authentication info";
        let p = super::fakes::FakePasswords::failing(failure);
        let logs = p.asked_admin.clone();
        let m = auth_core(true, p);
        let result = m.authenticate(&bff(), "alice", "s3cret-Pw");
        assert_eq!(
            result,
            Err(ManagerError::Service(ServiceError(failure.into())))
        );
        let (name, message) = error_reply(&result.clone().unwrap_err());
        for text in [format!("{result:?}"), name, message] {
            assert!(!text.contains("s3cret-Pw"), "the password in {text:?}");
        }
        assert_eq!(*logs.lock().unwrap(), Vec::<String>::new());
    }

    /// No answer carries the password, whatever the outcome.
    #[test]
    fn no_answer_carries_the_password() {
        for (caller, user, password) in [
            (bff(), "alice", "s3cret-Pw"),
            (bff(), "alice", "wrong-Pw-xyz"),
            (bff(), "mallory", "wrong-Pw-xyz"),
            (Caller(":1.42".into()), "alice", "s3cret-Pw"),
        ] {
            let (p, _) = passwords();
            let m = auth_core(true, p);
            let result = m.authenticate(&caller, user, password);
            let mut texts = vec![format!("{result:?}")];
            if let Err(e) = &result {
                let (name, message) = error_reply(e);
                texts.extend([name, message]);
            }
            for text in texts {
                assert!(!text.contains(password), "{password:?} in {text:?}");
            }
        }
    }

    /// Linux: only a member of `pai`, the group the packages create, may
    /// configure; `sudo` and `wheel` give nothing here. Group names are
    /// case-sensitive and matched whole.
    #[test]
    fn only_a_pai_member_may_configure() {
        let groups = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        for yes in [
            &["pai"][..],
            &["alice", "adm", "pai", "plugdev"],
            &["sudo", "wheel", "pai"],
        ] {
            assert!(in_pai_group(&groups(yes)), "{yes:?}");
        }
        for no in [
            &[][..],
            &["alice", "users", "adm", "plugdev"],
            &["sudo"],
            &["wheel"],
            &["sudo", "wheel", "root"],
            &["Pai"],
            &["PAI"],
            &["pai2"],
            &["pai-users"],
            &[" pai"],
            &["pai "],
            &["admin"],
            &["Administrators"],
            &["private-ai"],
            &[""],
        ] {
            assert!(!in_pai_group(&groups(no)), "{no:?}");
        }
    }

    // ── Acting for a signed-in user: the caller, the name, the membership ─
    // Every call here is refused before a path is resolved and no file is
    // written, so these never reach `paths::`. The e2e phase
    // `manager_principal_e2e` pins the calls that run, with the files.

    /// A request from `caller` vouching for `principal`.
    fn request(caller: &Caller, principal: &str) -> Requester {
        Requester {
            caller: caller.clone(),
            principal: Some(Principal(principal.to_string())),
        }
    }

    /// Every guarded call of `m` for `who`, in the order (start, stop,
    /// restart, start at boot, read an instance file). Writing a file needs
    /// the data root redirect, so the e2e phase pins that one.
    fn guarded_calls(m: &AuthCore, who: &Requester) -> Vec<Result<(), ManagerError>> {
        vec![
            m.start(who, "default"),
            m.stop(who, "default"),
            m.restart(who, "default"),
            m.set_boot_enabled(who, "default", true),
            m.read_file(Some(who), "server@default.ini").map(|_| ()),
        ]
    }

    /// A request with no principal is the caller's own call, exactly as
    /// today: the same polkit questions in the same order, with the caller,
    /// no account lookup, and no question about whose account the bff's is.
    #[test]
    fn a_request_with_no_principal_is_the_callers_own() {
        let alice = Caller(":1.42".into());
        let who = Requester {
            caller: alice.clone(),
            principal: None,
        };
        let (p, logs) = passwords();
        let m = auth_core(true, p);
        let _ = guarded_calls(&m, &who);
        assert_eq!(
            asked(&m),
            vec![
                (alice.clone(), Action::Control),
                (alice.clone(), Action::Control),
                (alice.clone(), Action::Control),
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Control),
            ],
            "the calls for the action, with the caller, as today"
        );
        assert_eq!(
            *m.authority().asked_quietly.lock().unwrap(),
            vec![(alice, Action::Configure)],
            "the read asks configure without a prompt, as today"
        );
        assert_eq!(
            asked_admin(&logs),
            Vec::<String>::new(),
            "no account is looked up for a caller's own call"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            Vec::<Caller>::new(),
            "a call with no principal is never checked against the bff"
        );
    }

    /// Only the bff's account may name a principal. A caller polkit would
    /// allow everything, and the empty caller of a pipe connection with no
    /// token, are refused the principal: NotAuthorized, the membership of the
    /// name not looked up, nothing asked beyond the caller, and the call
    /// refused rather than run as the caller's own.
    #[test]
    fn a_principal_named_by_a_caller_that_is_not_the_bff_is_refused_unasked() {
        for caller in [
            Caller(":1.42".into()),
            Caller(String::new()),
            Caller("S-1-5-18".into()),
        ] {
            let (p, logs) = passwords();
            let m = auth_core(true, p);
            let who = request(&caller, "alice");
            assert_eq!(
                guarded_calls(&m, &who),
                vec![Err(ManagerError::NotAuthorized); 5],
                "{caller:?} naming alice"
            );
            assert_eq!(
                asked(&m),
                vec![],
                "{caller:?}: polkit asked, so the call ran as the caller"
            );
            assert_eq!(
                *m.authority().asked_quietly.lock().unwrap(),
                vec![],
                "{caller:?}"
            );
            assert_eq!(
                asked_admin(&logs),
                Vec::<String>::new(),
                "{caller:?}: a membership looked up for a caller that is not the bff"
            );
            assert_eq!(
                *m.service().calls.lock().unwrap(),
                vec![],
                "{caller:?}: something reached the service manager"
            );
            assert_eq!(
                *m.authority().asked_bff.lock().unwrap(),
                vec![caller.clone(); 5],
                "the caller is checked first, once per call"
            );
        }
    }

    /// The bff naming an account that is no member of `pai` (bob, an ordinary
    /// account), an unknown account, or root: refused, with the membership of
    /// the name asked once per call and nothing else done. A failed lookup is
    /// no membership, and being the machine's superuser is not one either.
    #[test]
    fn a_principal_who_is_not_a_member_is_refused_and_nothing_is_done() {
        for user in ["bob", "mallory", "root"] {
            let (p, logs) = passwords();
            let m = auth_core(true, p);
            let who = request(&bff(), user);
            assert_eq!(
                guarded_calls(&m, &who),
                vec![Err(ManagerError::NotAuthorized); 5],
                "{user}"
            );
            assert_eq!(
                asked_admin(&logs),
                vec![user.to_string(); 5],
                "{user}: the membership is asked once per call"
            );
            assert_eq!(asked(&m), vec![], "{user}: polkit asked");
            assert_eq!(
                *m.authority().asked_quietly.lock().unwrap(),
                vec![],
                "{user}"
            );
            assert_eq!(
                *m.service().calls.lock().unwrap(),
                vec![],
                "{user}: a call reached the service manager"
            );
        }
    }

    /// A name the manager will not put in a record: no account is looked up,
    /// nothing is asked beyond the caller, nothing runs. The length bound is
    /// the bff's own, `MAX_USER_BYTES` in `crates/bff/src/session.rs`.
    #[test]
    fn an_illegible_principal_name_is_refused_with_nothing_asked() {
        let long = "a".repeat(257);
        for name in [
            "",
            "a\0b",
            "ali\nce",
            "alice\n",
            "alice\r\n",
            "ali\tce",
            "ali\u{7f}ce",
            &long,
        ] {
            let (p, logs) = passwords();
            let m = auth_core(true, p);
            let who = request(&bff(), name);
            assert_eq!(
                guarded_calls(&m, &who),
                vec![Err(ManagerError::NotAuthorized); 5],
                "{name:?}"
            );
            assert_eq!(
                asked_admin(&logs),
                Vec::<String>::new(),
                "{name:?}: the name reached the account lookup"
            );
            assert_eq!(asked(&m), vec![], "{name:?}: polkit asked");
            assert_eq!(
                *m.service().calls.lock().unwrap(),
                vec![],
                "{name:?}: a call reached the service manager"
            );
            assert_eq!(
                *m.authority().asked_bff.lock().unwrap(),
                vec![bff(); 5],
                "{name:?}: the caller is checked first, once per call"
            );
        }
    }

    /// 256 bytes, the longest the bff sends, is well formed: what refuses it
    /// is the membership, asked for exactly that name, not the name.
    #[test]
    fn a_256_byte_principal_name_is_well_formed() {
        let name = "a".repeat(256);
        let (p, logs) = passwords();
        let m = auth_core(true, p);
        let who = request(&bff(), &name);
        assert_eq!(
            guarded_calls(&m, &who),
            vec![Err(ManagerError::NotAuthorized); 5]
        );
        assert_eq!(
            asked_admin(&logs),
            vec![name.clone(); 5],
            "asked as a name, and unknown as an account"
        );
        assert_eq!(asked(&m), vec![]);
    }
    /// The bff naming a non-member, and a caller that is not the bff naming
    /// a member: `write_file_if` is refused exactly as the other guarded
    /// calls are, with the membership of the name asked (or not) and the
    /// file never read.
    #[test]
    fn a_compare_and_swap_write_answers_to_the_principal_gates() {
        let (p, logs) = passwords();
        let m = auth_core(true, p);
        assert_eq!(
            m.write_file_if(&request(&bff(), "bob"), "models.ini", "[X]\n", "[Y]\n"),
            Err(ManagerError::NotAuthorized),
            "a non-member is refused as the other guarded calls refuse it"
        );
        assert_eq!(
            asked_admin(&logs),
            vec!["bob".to_string()],
            "the membership is asked once, of the principal named"
        );
        assert_eq!(
            m.write_file_if(
                &request(&Caller(":1.42".into()), "alice"),
                "models.ini",
                "[X]\n",
                "[Y]\n"
            ),
            Err(ManagerError::NotAuthorized),
            "only the bff's account vouches for a user"
        );
        assert_eq!(
            asked_admin(&logs),
            vec!["bob".to_string()],
            "a caller that is not the bff gets no account looked up"
        );
        assert_eq!(
            asked(&m),
            vec![],
            "polkit decides nothing about a principal"
        );
        assert_eq!(
            *m.authority().asked_bff.lock().unwrap(),
            vec![bff(), Caller(":1.42".into())],
            "the caller is checked against the bff, once per call"
        );
        assert_eq!(*m.service().calls.lock().unwrap(), vec![]);
    }

    /// `Changed` answers with its own transport name and the kind's own
    /// word as message, and `error_kind` gives the same word the action
    /// record writes: the bff and the logs pin it.
    #[test]
    fn the_changed_refusal_has_its_own_error_name() {
        let e = ManagerError::Changed;
        assert_eq!(
            error_reply(&e),
            (
                "org.privateai.Manager1.Error.Changed".to_string(),
                "Changed".to_string()
            )
        );
        assert_eq!(error_kind(&e), "Changed");
    }

    /// The method's shape, pinned in the production text: the caller's
    /// whole text to compare (`expected`) and the text to write
    /// (`contents`), and no `keep_api_key`: the caller sent the file as it
    /// read it, key line included.
    #[test]
    fn write_file_if_takes_expected_and_contents_and_no_key_flag() {
        let source = std::include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/manager/service.rs"
        ));
        let production = source
            .split("\n#[cfg(test)]\n")
            .next()
            .expect("production text of service.rs");
        let at = production
            .find("pub fn write_file_if")
            .expect("the method is there to be tested");
        let signature = &production[at..at + production[at..].find('{').expect("a body")];
        for param in ["who: &W", "name: &str", "contents: &str", "expected: &str"] {
            assert!(signature.contains(param), "the signature lacks {param:?}");
        }
        assert!(
            !signature.contains("keep_api_key"),
            "the compare-and-swap takes no key flag"
        );
    }
}
