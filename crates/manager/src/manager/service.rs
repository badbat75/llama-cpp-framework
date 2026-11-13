//! The OS-neutral core of `private-ai-manager` (running llama-server as a service):
//! the instances, their config files in `paths::system_config_dir()` and
//! their services, behind two seams. [`ServiceControl`] starts, stops and
//! reports the unit of an instance (systemd's D-Bus API on
//! `llama.cpp@<id>.service` on Linux, the SCM on Windows); [`Authority`]
//! decides whether a caller may do an action (polkit `CheckAuthorization`
//! with the caller's bus name as subject on Linux). Reads, the instance list
//! and status need no authorization; everything that changes something does,
//! and a refused caller changes nothing.

use crate::server_cfg::ServerConfig;
use std::net::IpAddr;

/// A guarded action, one polkit action id each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Start, stop or restart an instance.
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

/// Whether a caller may do an action. A failure of the check itself
/// (polkit unreachable) answers false.
pub trait Authority {
    fn is_authorized(&self, caller: &Caller, action: Action) -> bool;
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
/// or the SCM's), not shown as user text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceError(pub String);

/// The service manager, an instance addressed by its id.
pub trait ServiceControl {
    fn start(&self, id: &str) -> Result<(), ServiceError>;
    fn stop(&self, id: &str) -> Result<(), ServiceError>;
    fn restart(&self, id: &str) -> Result<(), ServiceError>;
    fn status(&self, id: &str) -> Result<UnitState, ServiceError>;
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

/// The manager: the operations the D-Bus service / named pipe expose.
pub struct Manager<S: ServiceControl, A: Authority, R: Resolver> {
    service: S,
    authority: A,
    resolver: R,
}

impl<S: ServiceControl, A: Authority, R: Resolver> Manager<S, A, R> {
    pub fn new(service: S, authority: A, resolver: R) -> Self {
        Self {
            service,
            authority,
            resolver,
        }
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

    pub fn start(&self, caller: &Caller, id: &str) -> Result<(), ManagerError> {
        self.authorize(caller, Action::Control)?;
        existing_id(id)?;
        self.service.start(id).map_err(ManagerError::Service)
    }

    pub fn stop(&self, caller: &Caller, id: &str) -> Result<(), ManagerError> {
        self.authorize(caller, Action::Control)?;
        valid_id(id)?;
        self.service.stop(id).map_err(ManagerError::Service)
    }

    pub fn restart(&self, caller: &Caller, id: &str) -> Result<(), ManagerError> {
        self.authorize(caller, Action::Control)?;
        existing_id(id)?;
        self.service.restart(id).map_err(ManagerError::Service)
    }

    /// The contents of file `name`, None when it does not exist.
    pub fn read_file(&self, name: &str) -> Result<Option<String>, ManagerError> {
        let path = file_path(name)?;
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ManagerError::Io(e.kind())),
        }
    }

    /// Replace file `name` atomically, creating the directory; a new
    /// `server@<id>.ini` adds instance `id`.
    pub fn write_file(
        &self,
        caller: &Caller,
        name: &str,
        contents: &str,
    ) -> Result<(), ManagerError> {
        self.authorize(caller, Action::Configure)?;
        let file = config_file(name).ok_or(ManagerError::BadFileName)?;
        if let ConfigFile::Instance(id) = &file {
            self.check_address(id, contents)?;
        }
        let dir = crate::paths::system_config_dir();
        let io = |e: std::io::Error| ManagerError::Io(e.kind());
        std::fs::create_dir_all(&dir).map_err(io)?;
        crate::ini::atomic_write(&dir.join(name), contents).map_err(io)
    }

    /// Start every instance at the service's own start, past any failure:
    /// the failures, by id. No authorization: the service itself asks.
    pub fn start_all_at_boot(&self) -> Vec<(String, ManagerError)> {
        crate::paths::instance_ids()
            .into_iter()
            .filter_map(|id| match self.service.start(&id) {
                Ok(()) => None,
                Err(e) => Some((id, ManagerError::Service(e))),
            })
            .collect()
    }

    fn authorize(&self, caller: &Caller, action: Action) -> Result<(), ManagerError> {
        if self.authority.is_authorized(caller, action) {
            Ok(())
        } else {
            Err(ManagerError::NotAuthorized)
        }
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

/// The path of served file `name` in the system config directory.
fn file_path(name: &str) -> Result<std::path::PathBuf, ManagerError> {
    config_file(name).ok_or(ManagerError::BadFileName)?;
    Ok(crate::paths::system_config_dir().join(name))
}

/// Fakes of the two seams that record every call.
#[cfg(test)]
pub(crate) mod fakes {
    use super::*;
    use std::cell::RefCell;

    /// One recorded call to the service manager.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Call {
        Start(String),
        Stop(String),
        Restart(String),
        Status(String),
    }

    /// A service manager that records calls, reports `state` and fails every
    /// call for the ids in `failing`.
    pub struct FakeService {
        pub calls: RefCell<Vec<Call>>,
        pub state: UnitState,
        pub failing: Vec<String>,
    }

    impl FakeService {
        pub fn new() -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                state: UnitState::Active,
                failing: Vec::new(),
            }
        }

        fn record(&self, call: Call, id: &str) -> Result<(), ServiceError> {
            self.calls.borrow_mut().push(call);
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
    }

    /// A resolver over a fixed table; any other name does not resolve.
    pub struct FakeResolver {
        pub table: Vec<(&'static str, Vec<IpAddr>)>,
        pub asked: RefCell<Vec<String>>,
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
                asked: RefCell::new(Vec::new()),
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
            self.asked.borrow_mut().push(host.to_string());
            self.table
                .iter()
                .find(|(name, _)| *name == host)
                .map(|(_, ips)| ips.clone())
                .ok_or_else(|| ResolveError(host.to_string()))
        }
    }

    /// An authority that answers `allow` and records every question.
    pub struct FakeAuthority {
        pub allow: bool,
        pub asked: RefCell<Vec<(Caller, Action)>>,
    }

    impl FakeAuthority {
        pub fn new(allow: bool) -> Self {
            Self {
                allow,
                asked: RefCell::new(Vec::new()),
            }
        }
    }

    impl Authority for FakeAuthority {
        fn is_authorized(&self, caller: &Caller, action: Action) -> bool {
            self.asked.borrow_mut().push((caller.clone(), action));
            self.allow
        }
    }
}

/// E2E phase for the manager core, for this crate's e2e test to call: it sets
/// the process-wide `LLAMA_CPP_CONFIG_DATA_ROOT` redirect (`src/tests/mod.rs`).
/// Nothing here touches D-Bus, systemd, polkit or the GPUs: both seams are
/// fakes.
#[cfg(test)]
pub(crate) fn manager_e2e() {
    use fakes::{Call, FakeAuthority, FakeResolver, FakeService};
    use std::collections::BTreeSet;

    let dir = tempfile::tempdir().expect("tempdir");
    std::env::set_var("LLAMA_CPP_CONFIG_DATA_ROOT", dir.path());
    let sys = crate::paths::system_config_dir();
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
        );
        assert_eq!(
            m.write_file(&alice, "server@default.ini", "[Server]\nPort = 8081\n"),
            Err(ManagerError::NotAuthorized),
            "refused write of an instance file"
        );
        assert_eq!(
            m.write_file(&alice, "presets.ini", "[m]\nmodel = /m/a.gguf\n"),
            Err(ManagerError::NotAuthorized),
            "refused write of presets.ini"
        );
        assert!(
            !sys.exists(),
            "a refused write must not create the directory"
        );
        assert_eq!(
            *m.authority().asked.borrow(),
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
        );
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
            m.read_file("presets.ini"),
            Ok(None),
            "missing file reads as None"
        );
        assert!(
            m.service().calls.borrow().is_empty(),
            "no service call for a refused id"
        );
        assert!(!sys.exists(), "asking must not create the directory");
        // Stop and status need only a valid id: an instance whose file was
        // deleted while its unit runs stays visible and stoppable.
        assert_eq!(m.status("default"), Ok(UnitState::Active));
        assert_eq!(m.stop(&alice, "default"), Ok(()));
        assert_eq!(
            *m.service().calls.borrow(),
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
        );
        assert_eq!(
            m.write_file(&alice, "server@default.ini", default_ini),
            Ok(())
        );
        assert_eq!(m.write_file(&alice, "presets.ini", presets), Ok(()));
        assert_eq!(
            m.write_file(
                &alice,
                "presets.ini.disabled",
                "[Old]\nmodel = /m/old.gguf\n"
            ),
            Ok(())
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).expect("written"),
            default_ini
        );
        assert_eq!(
            files_in(&sys),
            set(&["server@default.ini", "presets.ini", "presets.ini.disabled"]),
            "exactly the files written, no temp file left"
        );
        // A new instance file is how an instance is added.
        assert_eq!(m.list_instances(), ["default"]);
        assert_eq!(
            m.read_file("server@default.ini"),
            Ok(Some(default_ini.into()))
        );
        assert_eq!(m.read_file("presets.ini"), Ok(Some(presets.into())));
        assert!(
            m.service().calls.borrow().is_empty(),
            "a write calls no service"
        );
        assert!(
            m.authority()
                .asked
                .borrow()
                .iter()
                .all(|(c, a)| c == &alice && *a == Action::Configure),
            "writes ask for configure"
        );

        // Any other name is refused, and nothing is created or read.
        let before = files_in(&sys);
        for name in bad_names(&sys) {
            assert_eq!(
                m.write_file(&alice, &name, "[Server]\n"),
                Err(ManagerError::BadFileName),
                "write {name:?}"
            );
            assert_eq!(
                m.read_file(&name),
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

    // ── Reads, list and status ask no authorization ──────────────────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false),
            FakeResolver::lan(),
        );
        assert_eq!(m.list_instances(), ["default"]);
        assert_eq!(m.read_file("presets.ini"), Ok(Some(presets.into())));
        assert_eq!(m.status("default"), Ok(UnitState::Active));
        assert!(
            m.authority().asked.borrow().is_empty(),
            "no question for reads"
        );
        assert_eq!(
            *m.service().calls.borrow(),
            vec![Call::Status("default".into())]
        );
    }

    // ── A refused caller cannot control an existing instance ─────────────
    {
        let m = Manager::new(
            FakeService::new(),
            FakeAuthority::new(false),
            FakeResolver::lan(),
        );
        assert_eq!(m.start(&alice, "default"), Err(ManagerError::NotAuthorized));
        assert_eq!(m.stop(&alice, "default"), Err(ManagerError::NotAuthorized));
        assert_eq!(
            m.restart(&alice, "default"),
            Err(ManagerError::NotAuthorized)
        );
        assert!(
            m.service().calls.borrow().is_empty(),
            "no service call when refused"
        );
        assert_eq!(
            *m.authority().asked.borrow(),
            vec![(alice.clone(), Action::Control); 3],
            "start, stop and restart ask for the control action"
        );
        // A refused caller learns nothing about which ids exist.
        assert_eq!(m.start(&alice, "nope"), Err(ManagerError::NotAuthorized));
        assert_eq!(m.start(&alice, "../x"), Err(ManagerError::NotAuthorized));
        assert!(m.service().calls.borrow().is_empty());
        // Nor can it change a file.
        assert_eq!(
            m.write_file(&alice, "server@default.ini", "[Server]\nPort = 1\n"),
            Err(ManagerError::NotAuthorized)
        );
        assert_eq!(
            m.write_file(&alice, "server@new.ini", "[Server]\nPort = 1\n"),
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
        let m = Manager::new(service, FakeAuthority::new(true), FakeResolver::lan());
        assert_eq!(m.start(&alice, "default"), Ok(()));
        assert_eq!(m.stop(&alice, "default"), Ok(()));
        assert_eq!(m.restart(&alice, "default"), Ok(()));
        assert_eq!(m.status("default"), Ok(UnitState::Failed));
        assert_eq!(
            *m.service().calls.borrow(),
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
            m.service().calls.borrow().len(),
            4,
            "no start or restart for an id with no file"
        );
        // Stop and status of it still reach the service.
        assert_eq!(m.stop(&alice, "lan-2"), Ok(()));
        assert_eq!(m.status("lan-2"), Ok(UnitState::Failed));
        assert_eq!(
            m.service().calls.borrow()[4..],
            [Call::Stop("lan-2".into()), Call::Status("lan-2".into())]
        );
    }

    // ── A service failure is reported as one ─────────────────────────────
    {
        let mut service = FakeService::new();
        service.failing = vec!["default".into()];
        let m = Manager::new(service, FakeAuthority::new(true), FakeResolver::lan());
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
        );
        // The same address as `default` (127.0.0.1:8081): refused, naming it.
        assert_eq!(
            m.write_file(&alice, "server@lan-2.ini", default_ini),
            Err(ManagerError::AddressInUse("default".into()))
        );
        assert!(
            !sys.join("server@lan-2.ini").exists(),
            "refused: not written"
        );
        // The same port on another specific host: fine.
        let lan2 = "[Server]\nPort = 8081\nHostname = 192.168.1.5\n";
        assert_eq!(m.write_file(&alice, "server@lan-2.ini", lan2), Ok(()));
        // Host names are resolved: localhost is default's 127.0.0.1.
        assert_eq!(
            m.write_file(
                &alice,
                "server@loop.ini",
                "[Server]\nPort = 8081\nHostname = localhost\n"
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
                "[Server]\nPort = 8081\nHostname = gpu-box\n"
            ),
            Err(ManagerError::AddressInUse("lan-2".into()))
        );
        // A name with several addresses collides when one matches.
        assert_eq!(
            m.write_file(
                &alice,
                "server@multi-home.ini",
                "[Server]\nPort = 8081\nHostname = multi-home\n"
            ),
            Err(ManagerError::AddressInUse("lan-2".into()))
        );
        // A host name that does not resolve: refused, naming it, not written.
        assert_eq!(
            m.write_file(
                &alice,
                "server@nowhere.ini",
                "[Server]\nPort = 8090\nHostname = nowhere.example\n"
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
                "[Server]\nPort = 8081\nHostname = 10.0.0.1, 192.168.1.5\n"
            ),
            Err(ManagerError::AddressInUse("lan-2".into()))
        );
        // A wildcard host on the same port collides with every host.
        for wild in ["0.0.0.0", "::"] {
            let text = format!("[Server]\nPort = 8081\nHostname = {wild}\n");
            assert!(
                matches!(
                    m.write_file(&alice, "server@wild.ini", &text),
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
                "[Server]\nPort = 8082\nHostname = 0.0.0.0\n"
            ),
            Ok(())
        );
        // Then a specific host on that port collides with the wildcard.
        assert_eq!(
            m.write_file(
                &alice,
                "server@late.ini",
                "[Server]\nPort = 8082\nHostname = 10.0.0.1\n"
            ),
            Err(ManagerError::AddressInUse("wild".into()))
        );
        // Rewriting an instance with its own address is fine.
        assert_eq!(
            m.write_file(&alice, "server@default.ini", default_ini),
            Ok(())
        );
        // Moving it onto another instance's address is refused, the file kept.
        assert_eq!(
            m.write_file(
                &alice,
                "server@default.ini",
                "[Server]\nPort = 8081\nHostname = 192.168.1.5\n"
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
            m.write_file(&alice, "server@bare.ini", "[Server]\n"),
            Ok(())
        );
        assert_eq!(
            m.write_file(&alice, "server@bare-2.ini", "[Server]\n"),
            Err(ManagerError::AddressInUse("bare".into()))
        );
        let explicit = format!(
            "[Server]\nPort = {}\nHostname = localhost\n",
            crate::server_cfg::DEFAULT_PORT
        );
        assert_eq!(
            m.write_file(&alice, "server@explicit.ini", &explicit),
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
                "[Server]\nPort = 8083\nHostname = 10.0.0.1\n"
            ),
            Ok(()),
            "an unresolvable other instance is skipped"
        );
        std::fs::remove_file(sys.join("server@stale.ini")).expect("drop stale");
        std::fs::remove_file(sys.join("server@fresh.ini")).expect("drop fresh");
        // presets.ini has no address: never checked.
        assert_eq!(m.write_file(&alice, "presets.ini", presets), Ok(()));
        assert_eq!(m.list_instances(), ["bare", "default", "lan-2", "wild"]);
        assert!(
            m.service().calls.borrow().is_empty(),
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
        );
        let before = files_in(&sys);
        std::fs::set_permissions(&sys, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        // Root writes into a read-only directory anyway: then there is no
        // failure to provoke.
        let probe = sys.join("probe");
        let enforced = std::fs::write(&probe, "").is_err();
        let _ = std::fs::remove_file(&probe);
        let result = m.write_file(&alice, "presets.ini", "[Other]\nmodel = /m/o.gguf\n");
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

    // ── Boot: every instance started, past a failure, no authorization ───
    {
        let mut service = FakeService::new();
        service.failing = vec!["default".into()];
        let m = Manager::new(service, FakeAuthority::new(false), FakeResolver::lan());
        let failures = m.start_all_at_boot();
        assert_eq!(failures.len(), 1, "one failure: {failures:?}");
        assert_eq!(failures[0].0, "default");
        assert!(matches!(failures[0].1, ManagerError::Service(_)));
        assert_eq!(
            *m.service().calls.borrow(),
            vec![
                Call::Start("bare".into()),
                Call::Start("default".into()),
                Call::Start("lan-2".into()),
                Call::Start("wild".into()),
            ],
            "every instance, in list order, past the failing one"
        );
        assert!(
            m.authority().asked.borrow().is_empty(),
            "boot asks no authorization"
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
            "/etc/llama.cpp/presets.ini",
            "C:\\ProgramData\\llama.cpp\\config\\presets.ini",
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
        assert!(r.asked.borrow().is_empty(), "a literal IP is not resolved");
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
}
