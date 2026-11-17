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
    /// A valid id with no `server@<id>.ini`.
    NoInstance,
    /// Not one of the files the manager reads and writes ([`config_file`]).
    BadFileName,
    /// This host and port collide with the instance of this id.
    AddressInUse(String),
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
    /// `server@<id>.ini`, the id valid.
    Instance(String),
    /// `presets.ini`.
    Presets,
    /// `presets.ini.disabled`.
    PresetsDisabled,
}

/// The file `name` names, a bare file name; None for any other name (a
/// path, `server.ini`, `settings.ini`, an invalid instance id).
pub fn config_file(name: &str) -> Option<ConfigFile> {
    match name {
        "presets.ini" => Some(ConfigFile::Presets),
        "presets.ini.disabled" => Some(ConfigFile::PresetsDisabled),
        _ => {
            let id = name.strip_prefix("server@")?.strip_suffix(".ini")?;
            crate::paths::valid_instance_id(id).then(|| ConfigFile::Instance(id.to_string()))
        }
    }
}

/// `assignments.ini`: which presets each instance is given. The manager owns
/// it, so a client reads it and never writes it.
pub const ASSIGNMENTS_FILE: &str = "assignments.ini";

/// The `models_max` of the pool checks: the `load-on-startup` count is judged
/// per instance, against that instance's own `ModelsMax` and its own render,
/// so the pool has no cap of its own.
const NO_MODELS_MAX: Option<i32> = Some(i32::MAX);

/// The group an instance file belongs to on Unix: the instances run as it
/// (the sysusers.d entry of the Linux package), so they read their file
/// while other users do not.
pub const INSTANCE_GROUP: &str = "private-ai";

/// The mode a served file is written with on Unix: 0640 for an instance
/// file (it can hold `OpencodeApiKey`), 0644 for the preset files.
pub fn file_mode(file: &ConfigFile) -> u32 {
    match file {
        ConfigFile::Instance(_) => 0o640,
        ConfigFile::Presets | ConfigFile::PresetsDisabled => 0o644,
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
            if let ConfigFile::Instance(_) = file {
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
    /// read; the preset files and [`ASSIGNMENTS_FILE`] need none, and their
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
            // [`ASSIGNMENTS_FILE`] is served for reading only: it is not a
            // `ConfigFile`, so the gate below never sees it and the write
            // gate refuses it by name.
            if config_file(name).is_none() && name != ASSIGNMENTS_FILE {
                return Err(ManagerError::BadFileName);
            }
            let mut may_configure = true;
            if let Some(ConfigFile::Instance(_)) = config_file(name) {
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

    /// Replace file `name` atomically, creating the directory; a new
    /// `server@<id>.ini` adds instance `id`. With `keep_api_key`, an
    /// instance file keeps the `OpencodeApiKey` its current file holds
    /// ([`keep_held_api_key`]); the checks run on the text written.
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
                ConfigFile::Instance(_) if keep_api_key => {
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
        check_contents(file, contents).map_err(ManagerError::Invalid)?;
        if let ConfigFile::Instance(id) = file {
            self.check_address(id, contents)?;
        }
        let dir = crate::paths::engine_config_dir();
        let io = |e: std::io::Error| ManagerError::Io(e.kind());
        std::fs::create_dir_all(&dir).map_err(io)?;
        write_served_file(&dir.join(name), file, contents, &*self.group).map_err(io)?;
        self.render_all_locked()
    }

    /// Whether instance `id` starts when the system starts. No authorization,
    /// like `status`; a valid id with no `server@<id>.ini` is NoInstance.
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

    /// Writes the render `presets@<id>.ini` of every instance from the pool,
    /// the switched-off pool and `assignments.ini`. The manager calls it at
    /// its start and after every write of a pool file.
    pub fn render_all(&self) -> Result<(), ManagerError> {
        // The same lock a write holds, so a render never mixes with a write.
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        self.render_all_locked()
    }

    /// The render of every instance, taken under the `writing` lock by
    /// [`Self::render_all`] and by a write: a render is the pool as it is on
    /// disk, so it belongs with the write that changed it.
    fn render_all_locked(&self) -> Result<(), ManagerError> {
        let dir = crate::paths::engine_config_dir();
        let on_disk = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();
        let (enabled, disabled) = (on_disk("presets.ini"), on_disk("presets.ini.disabled"));
        let io = |e: std::io::Error| ManagerError::Io(e.kind());
        for id in crate::paths::instance_ids() {
            let path = match crate::paths::instance_presets(&id) {
                Some(path) => path,
                None => continue,
            };
            // A render holds preset text alone, so it carries the preset
            // file's mode and needs no group of its own.
            let text = render_of(&id, &enabled, &disabled);
            write_served_file(&path, &ConfigFile::Presets, &text, &*self.group).map_err(io)?;
        }
        Ok(())
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
    /// before its action, whatever that action needs: the preset files are
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

    /// Refuse `contents` for instance `id` when its address collides with
    /// another instance's; an instance whose host does not resolve is skipped.
    fn check_address(&self, id: &str, contents: &str) -> Result<(), ManagerError> {
        let cfg = crate::server_cfg::from_text(contents);
        let mine = binding(&cfg, &self.resolver).map_err(ManagerError::UnresolvableHost)?;
        for other in crate::paths::instance_ids() {
            if other == id {
                continue;
            }
            let Some(path) = crate::paths::instance_ini(&other) else {
                continue;
            };
            let theirs = binding(&crate::server_cfg::load_from(&path), &self.resolver);
            if theirs.is_ok_and(|b| addresses_collide(&mine, &b)) {
                return Err(ManagerError::AddressInUse(other));
            }
        }
        Ok(())
    }
}

/// The content checks of `file` for `contents`, against the files on disk:
/// the instance file against the Server rules and its own render, `presets.ini`
/// and `presets.ini.disabled` against the other preset file and every
/// instance's render. A render is the pool cut by the instance's section of
/// `assignments.ini`, so the `load-on-startup` count is judged per instance,
/// against that instance's `ModelsMax`. A refusal writes nothing.
fn check_contents(file: &ConfigFile, contents: &str) -> Result<(), pai_core::checks::Refusal> {
    use pai_core::checks;
    let dir = crate::paths::engine_config_dir();
    let on_disk = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();
    // The pool the renders are made of once this write lands.
    let enabled = match file {
        ConfigFile::Presets => contents.to_string(),
        _ => on_disk("presets.ini"),
    };
    let disabled = match file {
        ConfigFile::PresetsDisabled => contents.to_string(),
        _ => on_disk("presets.ini.disabled"),
    };
    match file {
        ConfigFile::Instance(id) => {
            // The Server rules alone: no pool caps them any more, the
            // instance's render does, against the `ModelsMax` of this text.
            checks::server_file(contents, "")?;
            let models_max = crate::server_cfg::from_text(contents).models_max;
            check_render(id, &enabled, &disabled, models_max)
        }
        ConfigFile::Presets => {
            let (old, off) = (on_disk("presets.ini"), on_disk("presets.ini.disabled"));
            checks::presets_file(contents, &old, &off, NO_MODELS_MAX)?;
            checks::preset_ids_free(contents, &old, &off)?;
            check_renders(&enabled, &disabled)
        }
        ConfigFile::PresetsDisabled => {
            let (old, enabled_file) = (on_disk("presets.ini.disabled"), on_disk("presets.ini"));
            checks::presets_disabled_file(contents, &old, &enabled_file)?;
            checks::preset_ids_free(contents, &old, &enabled_file)?;
            check_renders(&enabled, &disabled)
        }
    }
}

/// The text of instance `id`'s render `presets@<id>.ini`, from the pool pair
/// and the `assignments.ini` on disk: what the checks judge and what
/// [`Manager::render_all`] writes.
fn render_of(id: &str, enabled: &str, disabled: &str) -> String {
    let assignment =
        std::fs::read_to_string(crate::paths::engine_config_dir().join(ASSIGNMENTS_FILE))
            .ok()
            .and_then(|text| pai_core::ini::text::section_text(&text, id));
    pai_core::assign::render(enabled, disabled, assignment.as_deref())
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

/// Every instance's render against its own `ModelsMax`, in id order: the
/// first render over its max refuses.
fn check_renders(enabled: &str, disabled: &str) -> Result<(), pai_core::checks::Refusal> {
    for id in crate::paths::instance_ids() {
        let path = match crate::paths::instance_ini(&id) {
            Some(path) => path,
            None => continue,
        };
        check_render(
            &id,
            enabled,
            disabled,
            crate::server_cfg::load_from(&path).models_max,
        )?;
    }
    Ok(())
}

/// `id` if it is a valid instance id.
fn valid_id(id: &str) -> Result<(), ManagerError> {
    if crate::paths::valid_instance_id(id) {
        Ok(())
    } else {
        Err(ManagerError::InvalidId)
    }
}

/// `id` if it is a valid id with its `server@<id>.ini`.
fn existing_id(id: &str) -> Result<(), ManagerError> {
    let path = crate::paths::instance_ini(id).ok_or(ManagerError::InvalidId)?;
    if path.is_file() {
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

/// The path of served file `name` in the engine's config directory: a
/// `ConfigFile` and [`ASSIGNMENTS_FILE`], nothing else.
fn file_path(name: &str) -> Result<std::path::PathBuf, ManagerError> {
    if config_file(name).is_none() && name != ASSIGNMENTS_FILE {
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
    let bad_names = |sys: &std::path::Path| -> Vec<String> {
        vec![
            "../x".into(),
            "../presets.ini".into(),
            "server.ini".into(),
            "settings.ini".into(),
            "server@Bad.ini".into(),
            "server@.ini".into(),
            "server@a_b.ini".into(),
            "server@default.ini.tmp".into(),
            "server@default".into(),
            "presets.ini.bak".into(),
            "presets@a.ini".into(),
            "presets@default.ini".into(),
            "sub/presets.ini".into(),
            "".into(),
            sys.join("presets.ini").to_string_lossy().into_owned(),
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
            m.write_file(&alice, "presets.ini", "[m]\nmodel = /m/a.gguf\n", false),
            Err(ManagerError::NotAuthorized),
            "refused write of presets.ini"
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
            m.read_file(Some(&alice), "presets.ini"),
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
    let presets = "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n";
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(
            m.write_file(&alice, "server@default.ini", default_ini, false),
            Ok(())
        );
        assert_eq!(m.write_file(&alice, "presets.ini", presets, false), Ok(()));
        assert_eq!(
            m.write_file(
                &alice,
                "presets.ini.disabled",
                "[Old]\nmodel = /m/old.gguf\n",
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
                "server@default.ini",
                "presets.ini",
                "presets.ini.disabled",
                "presets@default.ini"
            ]),
            "exactly the files written, no temp file left"
        );
        // A new instance file is how an instance is added.
        assert_eq!(m.list_instances(), ["default"]);
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(default_ini)))
        );
        assert_eq!(
            m.read_file(Some(&alice), "presets.ini"),
            Ok(Some(plain(presets)))
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

    // ── Preset reads, list and status ask no authorization ───────────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new());
        assert_eq!(m.list_instances(), ["default"]);
        assert_eq!(
            m.read_file(Some(&alice), "presets.ini"),
            Ok(Some(plain(presets)))
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
        assert_eq!(
            m.write_file(&alice, "server@bare-2.ini", "[Server]\n", false),
            Err(ManagerError::AddressInUse("bare".into()))
        );
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
        // presets.ini has no address: never checked.
        assert_eq!(m.write_file(&alice, "presets.ini", presets, false), Ok(()));
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
        let result = m.write_file(&alice, "presets.ini", "[Other]\nmodel = /m/o.gguf\n", false);
        std::fs::set_permissions(&sys, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
        if enforced {
            assert!(
                matches!(result, Err(ManagerError::Io(_))),
                "write failure: {result:?}"
            );
            assert_eq!(
                std::fs::read_to_string(sys.join("presets.ini")).expect("kept"),
                presets
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
/// of an instance file, a preset read with no caller, and the writes going
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
    let presets = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";

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
        assert_eq!(m.write_file(&alice, "presets.ini", presets, false), Ok(()));
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
            assert_eq!(mode("presets.ini"), 0o644);
            assert_eq!(c[0].mode, 0o640);
        }

        // ── An authorized read of an instance file: asked, then read ─────
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(ini)))
        );
        assert_eq!(m.read_file(Some(&alice), "server@nope.ini"), Ok(None));
        // ── The preset files need no caller at all ───────────────────────
        assert_eq!(
            m.read_file(None::<&Caller>, "presets.ini"),
            Ok(Some(plain(presets)))
        );
        assert_eq!(
            m.read_file(None::<&Caller>, "presets.ini.disabled"),
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
        // The preset files need no group: still written.
        assert_eq!(
            m.write_file(
                &alice,
                "presets.ini.disabled",
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
/// The preset files are never touched.
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

    // ── The preset files are never blanked, nor is configure asked ───────
    {
        let m = manager(false);
        let presets = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\nOpencodeApiKey = not-a-secret-here\n";
        put("presets.ini", presets);
        put("presets.ini.disabled", presets);
        assert_eq!(
            m.read_file(None::<&Caller>, "presets.ini"),
            Ok(Some(plain(presets)))
        );
        assert_eq!(
            m.read_file(Some(&alice), "presets.ini.disabled"),
            Ok(Some(plain(presets)))
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
/// as sent. The preset files are never touched.
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

    // ── A preset file is written as sent ─────────────────────────────────
    {
        put("server@default.ini", saved);
        let m = manager(true);
        let presets = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
        assert_eq!(m.write_file(&alice, "presets.ini", presets, true), Ok(()));
        assert_eq!(get("presets.ini"), presets);
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
    let stray_presets = "[stray]\nmodel = /models/stray.gguf\n";
    for (name, text) in [
        ("server@stray.ini", stray_ini),
        ("server@default.ini", stray_ini),
        ("presets.ini", stray_presets),
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
    assert_eq!(m.read_file(None::<&Caller>, "presets.ini"), Ok(None));
    assert_eq!(m.read_file(Some(&alice), "server@default.ini"), Ok(None));
    assert_eq!(
        m.start(&alice, "stray"),
        Err(ManagerError::NoInstance),
        "a file in the framework's directory is no instance"
    );
    assert!(m.service().calls.lock().unwrap().is_empty());

    // ── Writes land in the engine's directory ─────────────────────────────
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
    let presets = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
    let disabled = "[Old]\nmodel = /models/old.gguf\n";
    assert_eq!(
        m.write_file(&alice, "server@default.ini", ini, false),
        Ok(())
    );
    assert_eq!(m.write_file(&alice, "presets.ini", presets, false), Ok(()));
    assert_eq!(
        m.write_file(&alice, "presets.ini.disabled", disabled, false),
        Ok(())
    );
    for (name, text) in [
        ("server@default.ini", ini),
        ("presets.ini", presets),
        ("presets.ini.disabled", disabled),
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
        assert_eq!(mode("presets.ini"), 0o644);
        assert_eq!(mode("presets.ini.disabled"), 0o644);
    }
    assert_eq!(names(&framework), framework_before, "nothing written there");
    for (name, text) in [
        ("server@default.ini", stray_ini),
        ("presets.ini", stray_presets),
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
        m.read_file(None::<&Caller>, "presets.ini"),
        Ok(Some(plain(presets)))
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
    let presets = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";

    // ── A pending authorization holds up no other caller ─────────────────
    {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::paths::redirect_for_tests(dir.path());
        let sys = crate::paths::engine_config_dir();
        std::fs::create_dir_all(&sys).expect("system config dir");
        std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
        std::fs::write(sys.join("presets.ini"), presets).expect("write presets");

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
            on_thread(move || m.read_file(None::<&Caller>, "presets.ini"))
        };
        assert_eq!(answer(&gate, "read_file", read), Ok(Some(plain(presets))));
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
            on_thread(move || {
                m.write_file(&carol, "presets.ini", "[x]\nmodel = /m/x.gguf\n", false)
            })
        };
        assert_eq!(
            answer(&gate, "carol's write", refused),
            Err(ManagerError::NotAuthorized),
            "a caller polkit refuses is refused at once"
        );
        let disabled = "[Qwen3.8-Flash-Next]\nmodel = /models/Qwen3.8-Flash-Next-IQ3_S.gguf\n";
        let write = {
            let (m, bob) = (m.clone(), bob.clone());
            on_thread(move || m.write_file(&bob, "presets.ini.disabled", disabled, false))
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
            std::fs::read_to_string(sys.join("presets.ini")).unwrap(),
            presets,
            "the refused write left presets.ini"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("presets.ini.disabled")).unwrap(),
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
        ids.iter()
            .map(|id| format!("[{id}]\nmodel = /models/{id}.gguf\nload-on-startup = true\n"))
            .collect()
    };

    // ── A refused caller is told only that: no check runs first ──────────
    {
        let m = manager(false);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", "[Server]\nPort = 0\n", false),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.write_file(&alice, "presets.ini", "[a b]\nmodel = /m/a.gguf\n", false),
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
    let presets = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\nctx-size = 172032\n";
    {
        let m = manager(true);
        assert_eq!(
            m.write_file(&alice, "server@default.ini", default_ini, false),
            Ok(())
        );
        assert_eq!(m.write_file(&alice, "presets.ini", presets, false), Ok(()));

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

        let bad = format!("{presets}[Other]\nmodel = C:\\Models #1\\o.gguf\n");
        assert_eq!(
            m.write_file(&alice, "presets.ini", &bad, false),
            invalid("comment-markers", &["model"])
        );
        let bad = format!(
            "{presets}[Other]\nmodel = /models/o.gguf\nmoe-cache-mib = 4096\nsplit-mode = tensor\n"
        );
        assert_eq!(
            m.write_file(&alice, "presets.ini", &bad, false),
            invalid("moe-cache-with-tensor-split", &[])
        );
        assert_eq!(read("presets.ini").as_deref(), Some(presets), "old kept");
        assert_eq!(
            listing(),
            ["presets.ini", "presets@default.ini", "server@default.ini"],
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
        // default's ModelsMax is 2.
        assert_eq!(
            m.write_file(&alice, "presets.ini", &flagged(&["a", "b", "c"]), false),
            invalid("load-on-startup-over-max", &["3", "2", "default"]),
            "the refusal names the instance whose render is over its max"
        );
        assert_eq!(read("presets.ini").as_deref(), Some(presets));
        assert_eq!(
            read("presets@default.ini"),
            Some(pai_core::assign::render(presets, "", None)),
            "a refused write leaves the render as it was: it opens with the \
             generated header, so it is the pool rendered, never the pool text"
        );
        assert_eq!(
            m.write_file(&alice, "presets.ini", &flagged(&["a", "b"]), false),
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
        // loads: lan-2 inherits a preset and has no room for it.
        std::fs::write(sys.join("assignments.ini"), a_lan).expect("assignments.ini");
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
        // against its own render. default is told one preset, lan-2 all
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
        assert_eq!(
            m.write_file(&alice, "presets.ini", &flagged(&["a", "b", "c"]), false),
            Ok(()),
            "the pool has three presets and no instance is over its own max"
        );
        assert_eq!(
            read("presets@default.ini"),
            Some(pai_core::assign::render(
                &flagged(&["a", "b", "c"]),
                "",
                pai_core::ini::text::section_text(a_split, "default").as_deref(),
            )),
            "default's render holds the one preset it is told"
        );
        assert_eq!(
            read("presets@lan-2.ini"),
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
                "presets.ini",
                "presets@default.ini",
                "presets@lan-2.ini",
                "server@default.ini",
                "server@lan-2.ini"
            ],
            "one render per instance, beside the pool"
        );
    }

    // ── A broken section already on disk blocks no other write ───────────
    let broken = "[old]\nmodel = /models/old.gguf\nctx-size = abc\n";
    {
        let m = manager(true);
        std::fs::write(sys.join("presets.ini"), broken).expect("hand-edited fixture");
        let added = format!("{broken}[new]\nmodel = /models/new.gguf\n");
        assert_eq!(m.write_file(&alice, "presets.ini", &added, false), Ok(()));
        assert_eq!(read("presets.ini"), Some(added.clone()));
        let edited = format!("{broken}[new]\nmodel = /models/new.gguf\nparallel = 0\n");
        assert_eq!(
            m.write_file(&alice, "presets.ini", &edited, false),
            invalid(
                "not-whole-number-in-range",
                &["parallel", "0", "1", "2147483647"]
            )
        );
        assert_eq!(read("presets.ini"), Some(added));
    }

    // ── presets.ini.disabled: switching off is never refused ─────────────
    {
        let m = manager(true);
        // The broken preset moves out of presets.ini unchanged: accepted.
        assert_eq!(
            m.write_file(&alice, "presets.ini.disabled", broken, false),
            Ok(())
        );
        assert_eq!(read("presets.ini.disabled").as_deref(), Some(broken));
        // A disabled preset edited now gets the preset rules.
        let edited = format!("{broken}[parked]\nmodel = /models/p.gguf\nfit-target = 2048MiB\n");
        assert_eq!(
            m.write_file(&alice, "presets.ini.disabled", &edited, false),
            invalid("fit-target-format", &["2048MiB", "MiB"])
        );
        assert_eq!(read("presets.ini.disabled").as_deref(), Some(broken));
        // No startup count: llama-server never reads this file.
        let many = format!("{broken}{}", flagged(&["a", "b", "c", "d", "e"]));
        assert_eq!(
            m.write_file(&alice, "presets.ini.disabled", &many, false),
            Ok(())
        );
        // Switching the broken one back on: new in presets.ini, so checked.
        let enabled = "[new]\nmodel = /models/new.gguf\n";
        std::fs::write(sys.join("presets.ini"), enabled).expect("presets.ini fixture");
        let on = format!("{enabled}{broken}");
        assert_eq!(
            m.write_file(&alice, "presets.ini", &on, false),
            invalid(
                "not-whole-number-in-range",
                &["ctx-size", "abc", "0", "2147483647"]
            )
        );
    }

    // ── A preset switched on with an inline comment: the disabled file is
    //    the other file check_contents hands to presets_file ──────────────
    {
        let m = manager(true);
        let kept = "[kept]\nmodel = /models/kept.gguf\n";
        let parked = "[commented]\nmodel = /models/c.gguf ; Q8\n";
        std::fs::write(sys.join("presets.ini"), kept).expect("presets.ini fixture");
        std::fs::write(sys.join("presets.ini.disabled"), parked).expect("disabled fixture");
        // The switch moves the section whole: new to presets.ini, held
        // identically by the disabled file, so the commented line is known.
        let on = format!("{kept}{parked}");
        assert_eq!(m.write_file(&alice, "presets.ini", &on, false), Ok(()));
        assert_eq!(
            read("presets.ini").as_deref(),
            Some(on.as_str()),
            "the switch wrote through"
        );
    }

    // ── ctx-size = 0 (the context from the model) is writable ────────────
    {
        let m = manager(true);
        let zero = "[new]\nmodel = /models/new.gguf\nctx-size = 0\n";
        assert_eq!(m.write_file(&alice, "presets.ini", zero, false), Ok(()));
        assert_eq!(read("presets.ini").as_deref(), Some(zero));
    }

    // ── One id across presets.ini and presets.ini.disabled ───────────────
    {
        let m = manager(true);
        let enabled = "[new]\nmodel = /models/new.gguf\n";
        let parked = "[Parked]\nmodel = /models/parked.gguf\nctx-size = 8192\n";
        std::fs::write(sys.join("presets.ini"), enabled).expect("presets.ini fixture");
        std::fs::write(sys.join("presets.ini.disabled"), parked).expect("disabled fixture");
        // A new enabled preset named like a parked one, in another case.
        let namesake = format!("{enabled}[parked]\nmodel = /models/other.gguf\n");
        assert_eq!(
            m.write_file(&alice, "presets.ini", &namesake, false),
            invalid("preset-name-taken", &["parked"])
        );
        assert_eq!(read("presets.ini").as_deref(), Some(enabled), "old kept");
        // A new parked preset named like an enabled one.
        let namesake = format!("{parked}[NEW]\nmodel = /models/other.gguf\n");
        assert_eq!(
            m.write_file(&alice, "presets.ini.disabled", &namesake, false),
            invalid("preset-name-taken", &["NEW"])
        );
        assert_eq!(read("presets.ini.disabled").as_deref(), Some(parked));
        // Switching the parked one on writes presets.ini first: a move.
        let on = format!("{enabled}{parked}");
        assert_eq!(m.write_file(&alice, "presets.ini", &on, false), Ok(()));
        assert_eq!(read("presets.ini"), Some(on));
        assert_eq!(
            m.write_file(&alice, "presets.ini.disabled", "", false),
            Ok(())
        );
    }

    // ── Floats, enums, JSON and min <= max: refused, the file kept ───────
    {
        let m = manager(true);
        let kept = "[kept]\nmodel = /models/kept.gguf\n";
        std::fs::write(sys.join("presets.ini"), kept).expect("presets.ini fixture");
        std::fs::write(sys.join("presets.ini.disabled"), "").expect("disabled fixture");
        let with = |lines: &str| format!("{kept}[new]\nmodel = /models/new.gguf\n{lines}");
        assert_eq!(
            m.write_file(&alice, "presets.ini", &with("temp = NaN\n"), false),
            invalid("not-a-number", &["temp", "NaN"])
        );
        assert_eq!(
            m.write_file(
                &alice,
                "presets.ini",
                &with("chat-template-kwargs = [1]\n"),
                false
            ),
            invalid("not-json-object", &["chat-template-kwargs"])
        );
        assert_eq!(
            m.write_file(
                &alice,
                "presets.ini",
                &with("spec-draft-n-min = 5\nspec-draft-n-max = 4\n"),
                false
            ),
            invalid(
                "min-above-max",
                &["spec-draft-n-min", "5", "spec-draft-n-max", "4"]
            )
        );
        let enum_refused =
            m.write_file(&alice, "presets.ini", &with("split-mode = banana\n"), false);
        assert!(
            matches!(&enum_refused, Err(ManagerError::Invalid(r)) if r.key == "not-one-of"),
            "{enum_refused:?}"
        );
        assert_eq!(
            m.write_file(
                &alice,
                "presets.ini.disabled",
                &with("top-p = inf\n"),
                false
            ),
            invalid("not-a-number", &["top-p", "inf"])
        );
        assert_eq!(read("presets.ini").as_deref(), Some(kept), "old kept");
        assert_eq!(
            read("presets.ini.disabled").as_deref(),
            Some(""),
            "old kept"
        );
    }
}

/// The presets render of every instance: `assignments.ini` is read through
/// the file interface and never written by the client; the creation of an
/// instance and every write of a pool file write `presets@<id>.ini`, which
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
    // would assign every preset of the pool to default, leaving nothing to
    // tell the two renders apart. default is told `a`, lan-2 is told `b`.
    let assignments = "[default]\na = inherit\n\n[lan-2]\nb = inherit\n";
    let section = |id: &str| -> String {
        pai_core::ini::text::section_text(assignments, id).unwrap_or_default()
    };
    let default_ini = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsMax = 2\n";
    // The global `[*]` section reaches the pool only as a hand edit: a
    // client write of it is refused, `*` is not a preset id.
    let global = "[*]\ntemp = 0.7\n\n";
    let pool =
        "[a]\nmodel = /models/a.gguf\nload-on-startup = true\n\n[b]\nmodel = /models/b.gguf\n";
    let off = "[z]\nmodel = /models/z.gguf\n";

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
    // presets: default is told `a`, lan-2 is told `b`.
    assert_eq!(m.write_file(&alice, "presets.ini", pool, false), Ok(()));
    assert_eq!(
        m.write_file(&alice, "server@default.ini", default_ini, false),
        Ok(())
    );
    assert_eq!(
        read("presets@default.ini"),
        Some(render(pool, "", Some(section("default").as_str()))),
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
    let lan2 = read("presets@lan-2.ini").expect("lan-2's render");
    assert_eq!(
        lan2,
        render(pool, "", Some(section("lan-2").as_str())),
        "lan-2's render is its own section"
    );
    assert!(
        lan2.contains("[b]") && !read("presets@default.ini").unwrap().contains("[b]"),
        "the two renders differ: b is lan-2's alone"
    );
    assert!(
        read("presets@default.ini").unwrap().contains("[a]") && !lan2.contains("[a]"),
        "and a is default's alone"
    );
    // An instance with no section gets the global section alone; the pool
    // has none until the hand edit below, so this render carries no preset.
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
        read("presets@lan-3.ini"),
        Some(render(pool, "", None)),
        "no section: no preset of the pool"
    );
    assert_eq!(
        listing(),
        [
            "assignments.ini",
            "presets.ini",
            "presets@default.ini",
            "presets@lan-2.ini",
            "presets@lan-3.ini",
            "server@default.ini",
            "server@lan-2.ini",
            "server@lan-3.ini",
        ],
        "one render per instance, and nothing else"
    );

    // ── Every write of a pool file re-renders every instance ─────────────
    // A client write of the global `[*]` section is refused: `*` is not a
    // preset id. It arrives as a hand edit, and the next render takes it
    // from the file on disk.
    let on_disk = format!("{global}{pool}");
    assert!(
        matches!(
            m.write_file(&alice, "presets.ini", &on_disk, false),
            Err(ManagerError::Invalid(r)) if r.key == "invalid-preset-id" && r.values == ["*"]
        ),
        "the client cannot write the global section"
    );
    assert_eq!(
        read("presets.ini").as_deref(),
        Some(pool),
        "the refused write changed nothing"
    );
    std::fs::write(sys.join("presets.ini"), &on_disk).expect("hand-edited pool");
    assert_eq!(
        m.write_file(&alice, "presets.ini.disabled", off, false),
        Ok(())
    );
    assert_eq!(
        read("presets@default.ini"),
        Some(render(&on_disk, off, Some(section("default").as_str()))),
        "the render takes the global section from the pool file on disk"
    );
    assert_eq!(
        read("presets@lan-3.ini"),
        Some(render(&on_disk, off, None)),
        "the global section reaches every instance: lan-3 is the global section alone"
    );
    assert_eq!(m.write_file(&alice, "presets.ini", pool, false), Ok(()));
    assert_eq!(
        read("presets@default.ini"),
        Some(render(pool, off, Some(section("default").as_str()))),
        "default keeps a alone: the pool's other presets are not its rows"
    );
    assert_eq!(
        read("presets@lan-2.ini"),
        Some(render(pool, off, Some(section("lan-2").as_str()))),
        "lan-2 keeps b alone"
    );
    assert_eq!(
        read("presets@lan-3.ini"),
        Some(render(pool, off, None)),
        "the client's write replaced the hand-edited global section"
    );
    let added = format!("{pool}\n[c]\nmodel = /models/c.gguf\nload-on-startup = true\n");
    assert_eq!(m.write_file(&alice, "presets.ini", &added, false), Ok(()));
    assert_eq!(
        read("presets@default.ini"),
        Some(render(&added, off, Some(section("default").as_str()))),
        "the second write of the pool re-renders"
    );

    // ── A refused write changes no render ────────────────────────────────
    let broken = format!("{added}\n[bad]\nmodel = /models/bad.gguf\nctx-size = abc\n");
    let answer = m.write_file(&alice, "presets.ini", &broken, false);
    assert!(
        matches!(answer, Err(ManagerError::Invalid(_))),
        "a pool with a broken section is refused: {answer:?}"
    );
    assert_eq!(
        read("presets@default.ini"),
        Some(render(&added, off, Some(section("default").as_str()))),
        "a refused write leaves every render as it was"
    );
    assert_eq!(
        listing(),
        [
            "assignments.ini",
            "presets.ini",
            "presets.ini.disabled",
            "presets@default.ini",
            "presets@lan-2.ini",
            "presets@lan-3.ini",
            "server@default.ini",
            "server@lan-2.ini",
            "server@lan-3.ini",
        ],
        "a render of an id is never a temp file left behind"
    );

    // ── The manager's start writes the render of every instance ──────────
    // Last: the stub panics until the coder lands.
    assert_eq!(
        m.render_all(),
        Ok(()),
        "the renders are rewritten from what is on disk"
    );
    assert_eq!(
        read("presets@default.ini"),
        Some(render(&added, off, Some(section("default").as_str()))),
        "the start render agrees with the last write"
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
    let presets = "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n";
    let other = "[Qwen3.8-32B]\nmodel = /m/Qwen3.8-32B-Q4_K_M.gguf\n";
    // A preset text no block of this phase writes on purpose: a refused write
    // of it is visible as the file on disk staying as `other`.
    let third = "[Qwen3.8-14B]\nmodel = /m/Qwen3.8-14B-Q4_K_M.gguf\n";
    std::fs::write(sys.join("server@default.ini"), ini).expect("instance file");
    std::fs::write(sys.join("presets.ini"), presets).expect("presets file");
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
            m.write_file(&alice, "presets.ini", other, false),
            Ok(()),
            "granting the bff `configure` for this is what the pair replaces"
        );
        assert_eq!(text_of("presets.ini"), other, "the write landed");
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
            m.write_file(&bob, "presets.ini", presets, false),
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
            text_of("presets.ini"),
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
                m.write_file(&who, "presets.ini", third, false),
                Err(ManagerError::NotAuthorized),
                "{who:?}: only the bff vouches for a user"
            );
        }
        assert_eq!(
            text_of("presets.ini"),
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
                    "presets.ini",
                    third,
                    false
                ),
                Err(ManagerError::NotAuthorized),
                "{name:?}: an illegible name is refused"
            );
        }
        assert_eq!(
            text_of("presets.ini"),
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

    // ── A preset file read is gated too ──────────────────────────────────
    // The preset files need no authorization of the caller, so the principal
    // gate is all that stands between these requests and the file: a
    // non-member's read and a caller outside the bff's read answer nothing.
    {
        let p = FakePasswords::new();
        let asked_admin = p.asked_admin.clone();
        let m = core(true, true, p);
        assert_eq!(
            m.read_file(Some(&bob), "presets.ini"),
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
                "presets.ini"
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
            m.read_file(Some(&alice), "presets.ini"),
            Ok(Some(plain(other))),
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
    let presets = "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n";
    let other = "[Qwen3.8-32B]\nmodel = /m/Qwen3.8-32B-Q4_K_M.gguf\n";
    std::fs::write(sys.join("server@default.ini"), ini).expect("instance file");
    std::fs::write(sys.join("presets.ini"), presets).expect("presets file");
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
        assert_eq!(m.write_file(&alice, "presets.ini", other, false), Ok(()));
        assert_eq!(
            m.read_file(Some(&alice), "server@default.ini"),
            Ok(Some(plain(ini)))
        );
        assert_eq!(
            m.read_file(Some(&alice), "presets.ini"),
            Ok(Some(plain(other))),
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
                r#"action="write_file" target="presets.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="read_file" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="read_file" target="presets.ini" caller=":1.9" principal="alice" outcome=ok"#,
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
            m.write_file(&bob, "presets.ini", presets, false),
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
            m.write_file(&alice, "nope.ini", presets, false),
            Err(ManagerError::BadFileName)
        );
        let failed = m
            .start(&alice, "default")
            .expect_err("the fake service fails");
        assert!(
            matches!(failed, ManagerError::Service(_)),
            "the service manager's own failure: {failed:?}"
        );
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
                r#"action="write_file" target="presets.ini" caller=":1.9" principal="bob" outcome=refused"#,
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

    // ── The caller's own call: recorded with `principal=-` ───────────────
    // The files here are as the blocks above left them: `presets.ini` holds
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
            m.read_file(Some(&desktop), "presets.ini"),
            Ok(Some(plain(other))),
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
                r#"action="read_file" target="presets.ini" caller=":1.42" principal=- outcome=ok"#,
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
        let secret_presets = format!("[Qwen3.8-27B]\nmodel = /m/{secret}.gguf\n");
        let secret_ini = ini.replace("sk-local-3f9a", secret);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), FakeService::new(), log);
        assert_eq!(
            m.write_file(&alice, "presets.ini", &secret_presets, false),
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
            m.write_file(&bob, "presets.ini", &secret_presets, false),
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
                r#"action="write_file" target="presets.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="read_file" target="server@default.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file" target="presets.ini" caller=":1.9" principal="bob" outcome=refused"#,
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
    let presets = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\nctx-size = 172032\n";
    let other = "[Qwen3.8-32B]\nmodel = /models/Qwen3.8-32B-Q4_K_M.gguf\n";
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
        let names = ["presets.ini", "server@default.ini"];
        for name in names {
            std::fs::create_dir_all(sys.join(name)).expect("unreadable fixture");
        }
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(false, FakePasswords::new(), log);
        for name in names {
            assert_eq!(
                m.write_file_if(&desktop, name, other, presets),
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
        let (_dir, sys) = root(&[
            ("presets.ini", presets),
            ("server@default.ini", default_ini),
        ]);
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
            ["presets.ini", "presets@default.ini", "server@default.ini"],
            "exactly the files written (the server write renders the instance's \
             presets file), no temp file left"
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
        let bad_presets = format!(
            "{presets}[Other]\nmodel = /models/o.gguf\nmoe-cache-mib = 4096\nsplit-mode = tensor\n"
        );
        assert_eq!(
            m.write_file_if(&desktop, "presets.ini", &bad_presets, presets),
            invalid("moe-cache-with-tensor-split", &[]),
            "the preset file's checks run too"
        );
        assert_eq!(
            text_of(&sys, "presets.ini"),
            presets,
            "the refused text left the file"
        );
        // The compare is the first thing under the writing lock: a stale
        // expected is Changed, whatever the contents would have failed on.
        assert_eq!(
            m.write_file_if(&desktop, "presets.ini", &bad_presets, default_ini),
            Err(ManagerError::Changed),
            "the compare answers before any check runs"
        );
        assert_eq!(
            text_of(&sys, "presets.ini"),
            presets,
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
                r#"action="write_file_if" target="presets.ini" caller=":1.42" principal=- outcome="Invalid""#,
                r#"action="write_file_if" target="presets.ini" caller=":1.42" principal=- outcome="Changed""#,
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
            ("presets.ini", presets),
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
            presets.trim_end().to_string(),
            format!("{presets} "),
            presets[..presets.len() - 1].to_string(),
            format!("; note\n{presets}"),
            presets.replace('\n', "\r\n"),
            presets.replace("172032", "172033"),
            presets.to_uppercase(),
        ];
        for expected in &stale {
            assert_eq!(
                m.write_file_if(&desktop, "presets.ini", other, expected),
                Err(ManagerError::Changed),
                "expected {expected:?} against the file on disk"
            );
        }
        assert_eq!(
            text_of(&sys, "presets.ini"),
            presets,
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
                r#"action="write_file_if" target="presets.ini" caller=":1.42" principal=- outcome="Changed""#
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
            m.write_file_if(&desktop, "presets.ini", other, presets),
            Ok(()),
            "the expected text is the file's own: written"
        );
        assert_eq!(text_of(&sys, "presets.ini"), other, "the write landed");
    }

    // ── A missing file: Changed, and nothing is created ──────────────────
    {
        let (_dir, sys) = root(&[("presets.ini", presets)]);
        let log = FakeActionLog::new();
        let lines = log.lines.clone();
        let m = core(true, FakePasswords::new(), log);
        let disabled = "[Old]\nmodel = /models/old.gguf\n";
        assert_eq!(
            m.write_file_if(&desktop, "presets.ini.disabled", disabled, presets),
            Err(ManagerError::Changed),
            "the caller expects a file that is not there"
        );
        assert_eq!(
            m.write_file_if(&desktop, "presets.ini.disabled", disabled, disabled),
            Err(ManagerError::Changed),
            "an expected equal to the contents does not make the file exist"
        );
        assert!(
            !sys.join("presets.ini.disabled").exists(),
            "a refused compare-and-swap creates no file"
        );
        assert_eq!(listing(&sys), ["presets.ini"], "nothing created");
        assert_eq!(
            records(&lines),
            vec![
                r#"action="write_file_if" target="presets.ini.disabled" caller=":1.42" principal=- outcome="Changed""#
                    .to_string(),
                r#"action="write_file_if" target="presets.ini.disabled" caller=":1.42" principal=- outcome="Changed""#
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
            m.write_file_if(&desktop, "presets.ini", presets, presets),
            Err(ManagerError::Changed),
            "no file to compare with"
        );
        assert!(!sys.exists(), "the refused write created no directory");
    }

    // ── The principal form: the pair is the right, the compare is byte for byte
    {
        let ini = "[Server]\nPort = 8081\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
        let (_dir, sys) = root(&[("presets.ini", presets), ("server@default.ini", ini)]);
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
            m.write_file_if(&member, "presets.ini", other, presets),
            Ok(()),
            "a member's compare-and-swap lands with the caller allowed nothing"
        );
        assert_eq!(text_of(&sys, "presets.ini"), other, "the write landed");
        assert_eq!(
            m.write_file_if(
                &member,
                "presets.ini",
                presets,
                "[Qwen3.8-9B]\nmodel = /m/x.gguf\n"
            ),
            Err(ManagerError::Changed),
            "a member's stale expected is refused as anyone's"
        );
        assert_eq!(
            text_of(&sys, "presets.ini"),
            other,
            "the refused write changed no file"
        );
        assert_eq!(
            m.write_file_if(&outsider, "presets.ini", presets, other),
            Err(ManagerError::NotAuthorized),
            "a non-member is refused as write_file refuses it"
        );
        assert_eq!(
            text_of(&sys, "presets.ini"),
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
                "presets.ini",
                presets,
                other
            ),
            Err(ManagerError::NotAuthorized),
            "only the bff's account vouches for a user"
        );
        assert_eq!(text_of(&sys, "presets.ini"), other, "nothing written");
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
                r#"action="write_file_if" target="presets.ini" caller=":1.9" principal="alice" outcome=ok"#,
                r#"action="write_file_if" target="presets.ini" caller=":1.9" principal="alice" outcome="Changed""#,
                r#"action="write_file_if" target="presets.ini" caller=":1.9" principal="bob" outcome=refused"#,
                r#"action="write_file_if" target="presets.ini" caller=":1.42" principal="alice" outcome=refused"#,
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
            ["presets@slow.ini", "server@slow.ini"],
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
            Some(ConfigFile::Instance("default".into()))
        );
        assert_eq!(
            config_file("server@lan-2.ini"),
            Some(ConfigFile::Instance("lan-2".into()))
        );
        assert_eq!(config_file("presets.ini"), Some(ConfigFile::Presets));
        assert_eq!(
            config_file("presets.ini.disabled"),
            Some(ConfigFile::PresetsDisabled)
        );
    }

    #[test]
    fn config_file_refuses_every_other_name() {
        for name in [
            "",
            "../x",
            "../presets.ini",
            "./presets.ini",
            "sub/presets.ini",
            "sub\\presets.ini",
            "/etc/private-ai/presets.ini",
            "C:\\ProgramData\\private-ai\\config\\presets.ini",
            "server.ini",
            "settings.ini",
            "server@Bad.ini",
            "server@.ini",
            "server@a_b.ini",
            "server@../x.ini",
            "server@default.ini.tmp",
            "server@default",
            "presets.ini.bak",
            "presets.ini.tmp",
            "Presets.ini",
            "presets.ini ",
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
            m.write_file_if(&alice, "presets.ini", "[X]\n", "[Y]\n"),
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
        for name in ["server.ini", "../presets.ini", "presets.ini.tmp"] {
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
        for name in ["server.ini", "../presets.ini", "presets.ini.tmp"] {
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

    // ── Instance files: mode 0640, group private-ai ─────────────────────

    /// The instance file can hold a secret: 0640, read by root and the
    /// instances' group. The preset files hold none: 0644, as before.
    #[test]
    fn file_mode_is_0640_for_instance_files_and_0644_for_the_presets() {
        assert_eq!(file_mode(&ConfigFile::Instance("default".into())), 0o640);
        assert_eq!(file_mode(&ConfigFile::Instance("lan-2".into())), 0o640);
        assert_eq!(file_mode(&ConfigFile::Presets), 0o644);
        assert_eq!(file_mode(&ConfigFile::PresetsDisabled), 0o644);
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
            write_served_file(&path, &ConfigFile::Instance("default".into()), INI, &g)
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
            write_served_file(&path, &ConfigFile::Instance("default".into()), INI, &g)
                .expect("write");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), INI);
            assert_eq!(mode(&path), 0o640);
            assert_eq!(calls(&g).len(), 1);
        }

        /// The preset files hold no secret: 0644, whatever was there, and
        /// their group is left alone (root's, the manager's own).
        #[test]
        fn preset_files_are_written_0644_and_keep_their_group() {
            let dir = tempfile::tempdir().expect("tempdir");
            let presets = dir.path().join("presets.ini");
            std::fs::write(&presets, "[Old]\nmodel = /m/old.gguf\n").unwrap();
            std::fs::set_permissions(&presets, std::fs::Permissions::from_mode(0o600)).unwrap();
            let g = FakeGroup::new();
            let text = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
            write_served_file(&presets, &ConfigFile::Presets, text, &g).expect("presets");
            let disabled = dir.path().join("presets.ini.disabled");
            write_served_file(&disabled, &ConfigFile::PresetsDisabled, text, &g).expect("disabled");
            assert_eq!(std::fs::read_to_string(&presets).unwrap(), text);
            assert_eq!(std::fs::read_to_string(&disabled).unwrap(), text);
            assert_eq!(mode(&presets), 0o644);
            assert_eq!(mode(&disabled), 0o644);
            assert_eq!(calls(&g), vec![], "no group change for a preset file");
            assert_eq!(
                names(dir.path()),
                BTreeSet::from([
                    "presets.ini".to_string(),
                    "presets.ini.disabled".to_string()
                ])
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
            let e = write_served_file(&path, &ConfigFile::Instance("default".into()), INI, &g)
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
            let e = write_served_file(&new, &ConfigFile::Instance("lan-2".into()), INI, &g)
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
            let result = write_served_file(&path, &ConfigFile::Instance("default".into()), INI, &g);
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
            m.write_file_if(&request(&bff(), "bob"), "presets.ini", "[X]\n", "[Y]\n"),
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
                "presets.ini",
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
