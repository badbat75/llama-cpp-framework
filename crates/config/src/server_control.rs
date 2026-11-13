//! Starting, stopping and probing the `default` llama-server instance, behind
//! one seam with two backends: `Direct` (the configurator spawns and kills the
//! process itself, `runstate`) and `Manager` (the manager service runs it, asked
//! through `pai_core::client`). `backend()` picks the one of this OS.

use crate::server_cfg::ServerConfig;
use crate::strings;
use pai_core::client::{ClientError, InstanceState, ManagerClient};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The instance every caller of this seam acts on.
pub const INSTANCE: &str = "default";

/// A connected manager client, shared by the GUI's worker threads.
pub type Client = Box<dyn ManagerClient + Send + Sync>;

/// How the `Manager` backend connects, called on its first use.
pub type Connect = Box<dyn Fn() -> Result<Client, ClientError> + Send + Sync>;

/// Who runs llama-server.
pub enum Backend {
    /// The configurator itself: `runstate`. Built off Linux only (`backend`).
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    Direct,
    /// The manager service.
    Manager(ManagerBackend),
}

/// The `Manager` backend: a client connected lazily.
pub struct ManagerBackend {
    connect: Connect,
    /// The client once a connect has succeeded; shared out by `Arc` so a long
    /// call (a stop waiting for the exit) does not hold the lock other
    /// threads' probes need.
    client: Mutex<Option<Arc<Client>>>,
}

impl ManagerBackend {
    /// A backend that connects with `connect` on its first use, not now.
    pub fn new(connect: Connect) -> ManagerBackend {
        ManagerBackend {
            connect,
            client: Mutex::new(None),
        }
    }

    /// The connected client, connecting now if no connect has succeeded yet.
    fn client(&self) -> Result<Arc<Client>, ClientError> {
        let mut slot = self.client.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = slot.as_ref() {
            return Ok(c.clone());
        }
        let c = Arc::new((self.connect)()?);
        *slot = Some(c.clone());
        Ok(c)
    }

    fn is_running(&self) -> bool {
        self.client()
            .and_then(|c| c.status(INSTANCE))
            .is_ok_and(|s| {
                matches!(
                    s,
                    InstanceState::Active | InstanceState::Activating | InstanceState::Deactivating
                )
            })
    }

    fn start(&self) -> Result<Option<ServerConfig>, ClientError> {
        if self.is_running() {
            return Ok(None);
        }
        self.client()?.start(INSTANCE)?;
        Ok(Some(self.server_config()))
    }

    fn stop(&self) -> Result<bool, ClientError> {
        self.client()?.stop(INSTANCE)?;
        // The manager queues the stop: wait for the unit to be gone, as
        // `runstate::stop` waits for the process.
        Ok(crate::runstate::wait_gone(
            || self.is_running(),
            crate::runstate::STOP_GRACE,
            STOP_POLL,
        ))
    }

    fn restart(&self) -> Result<Option<ServerConfig>, ClientError> {
        self.client()?.restart(INSTANCE)?;
        Ok(Some(self.server_config()))
    }

    /// The instance's config as the manager holds it; defaults when it has
    /// none or cannot be read, as `server_cfg::load` reads a missing file.
    fn server_config(&self) -> ServerConfig {
        let text = self.read_config(ConfigFile::Server).ok().flatten();
        crate::server_cfg::from_text(&text.unwrap_or_default())
    }

    fn read_config(&self, file: ConfigFile) -> Result<Option<String>, ClientError> {
        self.client()?.read_file(&file.manager_name())
    }

    fn write_config(&self, file: ConfigFile, contents: &str) -> Result<(), ClientError> {
        self.client()?.write_file(&file.manager_name(), contents)
    }
}

/// How often a stop asks whether the instance is gone.
const STOP_POLL: Duration = Duration::from_millis(250);

/// A configuration file of the instance: read and written only through this
/// seam, from the manager on Linux and from the per-user files on `Direct`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFile {
    /// `server@default.ini` (`server.ini` per user).
    Server,
    /// `presets.ini`.
    Presets,
    /// `presets.ini.disabled`.
    PresetsDisabled,
}

impl ConfigFile {
    #[cfg(test)]
    pub const ALL: [ConfigFile; 3] = [
        ConfigFile::Server,
        ConfigFile::Presets,
        ConfigFile::PresetsDisabled,
    ];

    /// The file's name among the manager's files.
    pub fn manager_name(self) -> String {
        match self {
            ConfigFile::Server => format!("server@{INSTANCE}.ini"),
            ConfigFile::Presets => "presets.ini".into(),
            ConfigFile::PresetsDisabled => "presets.ini.disabled".into(),
        }
    }

    /// The per-user file `Direct` reads and writes.
    fn user_path(self) -> PathBuf {
        match self {
            ConfigFile::Server => crate::paths::server_ini(),
            ConfigFile::Presets => crate::paths::presets_ini(),
            ConfigFile::PresetsDisabled => crate::paths::presets_disabled_ini(),
        }
    }
}

impl Backend {
    /// Start the instance: `Ok(Some(cfg))` with the config it was started
    /// with, `Ok(None)` when it was already running, `Err` with the text to show.
    pub fn start(&self) -> Result<Option<ServerConfig>, String> {
        match self {
            Backend::Direct => crate::runstate::start().map_err(|e| e.to_string()),
            Backend::Manager(m) => m.start().map_err(|e| e.text()),
        }
    }

    /// Stop the instance: `Ok(true)` once it is gone, `Ok(false)` when it is
    /// still running after the grace, `Err` with the text to show.
    pub fn stop(&self) -> Result<bool, String> {
        match self {
            Backend::Direct => Ok(crate::runstate::stop()),
            Backend::Manager(m) => m.stop().map_err(|e| e.text()),
        }
    }

    /// Stop and start again; what `start` returns.
    pub fn restart(&self) -> Result<Option<ServerConfig>, String> {
        match self {
            Backend::Direct => {
                // `stop()` returns only once the old router and its children
                // are gone: starting earlier saw the old process and launched
                // nothing.
                if !crate::runstate::stop() {
                    return Err(strings::fill(
                        "old-server-still-running",
                        &[&crate::runstate::STOP_GRACE.as_secs()],
                    ));
                }
                std::thread::sleep(Duration::from_millis(1000));
                crate::runstate::start().map_err(|e| e.to_string())
            }
            Backend::Manager(m) => m.restart().map_err(|e| e.text()),
        }
    }

    /// Whether a llama-server of the instance is there.
    pub fn is_running(&self) -> bool {
        match self {
            Backend::Direct => crate::runstate::is_running(),
            Backend::Manager(m) => m.is_running(),
        }
    }

    /// The file's text, `Ok(None)` when it does not exist yet, `Err` with the
    /// text to show.
    pub fn read_config(&self, file: ConfigFile) -> Result<Option<String>, String> {
        match self {
            Backend::Direct => match std::fs::read_to_string(file.user_path()) {
                Ok(text) => Ok(Some(text)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.to_string()),
            },
            Backend::Manager(m) => m.read_config(file).map_err(|e| e.text()),
        }
    }

    /// Replace the file with `contents`; `Err` with the text to show (a
    /// refusal of the manager in the user's language).
    pub fn write_config(&self, file: ConfigFile, contents: &str) -> Result<(), String> {
        match self {
            Backend::Direct => {
                let path = file.user_path();
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                crate::ini::atomic_write(&path, contents).map_err(|e| e.to_string())
            }
            Backend::Manager(m) => m.write_config(file, contents).map_err(|e| e.text()),
        }
    }

    /// Where the file is, to show the user.
    pub fn config_path(&self, file: ConfigFile) -> PathBuf {
        match self {
            Backend::Direct => file.user_path(),
            Backend::Manager(_) => crate::paths::system_config_dir().join(file.manager_name()),
        }
    }
}

/// The sections of the file as `backend()` reads it; none when it is missing
/// or cannot be read.
pub fn read_sections(file: ConfigFile) -> Vec<crate::ini::Section> {
    backend()
        .read_config(file)
        .ok()
        .flatten()
        .map(|t| crate::ini::parse_sections(&t))
        .unwrap_or_default()
}

/// The keys of one section of the file (named case-insensitively), as
/// `ini::read_section` reads a file: empty when it is not there.
pub fn read_section(file: ConfigFile, section: &str) -> std::collections::BTreeMap<String, String> {
    read_sections(file)
        .into_iter()
        .find(|s| s.id.eq_ignore_ascii_case(section))
        .map(|s| s.keys)
        .unwrap_or_default()
}

/// The connection the Linux backend makes: the system bus, except in a test
/// build or under the test redirect (`PRIVATE_AI_DATA_ROOT` set), where it is
/// `Unavailable` without dialing anything.
#[cfg(target_os = "linux")]
pub(crate) fn system_connect() -> Result<Client, ClientError> {
    if cfg!(test) || std::env::var_os("PRIVATE_AI_DATA_ROOT").is_some() {
        return Err(ClientError::Unavailable);
    }
    pai_core::client::dbus::DbusClient::connect_system().map(|c| Box::new(c) as Client)
}

/// A new backend of this OS: the manager on Linux, `Direct` elsewhere.
pub(crate) fn os_backend() -> Backend {
    #[cfg(target_os = "linux")]
    {
        Backend::Manager(ManagerBackend::new(Box::new(system_connect)))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Backend::Direct
    }
}

/// The backend of this OS: the manager on Linux, `Direct` elsewhere. In a
/// test binary, the one `fake::scoped` set on this thread, else
/// `fake::default_backend()`.
pub fn backend() -> &'static Backend {
    #[cfg(test)]
    return fake::current();
    #[cfg(not(test))]
    {
        static BACKEND: std::sync::OnceLock<Backend> = std::sync::OnceLock::new();
        BACKEND.get_or_init(os_backend)
    }
}

/// A recording [`ManagerClient`] for the tests of this seam and its callers.
#[cfg(test)]
pub(crate) mod fake {
    use pai_core::client::{ClientError, InstanceState, ManagerClient};
    use std::sync::{Arc, Mutex};

    /// One log shared by the fake and whatever else a test records (the
    /// llama-server stub), so the order of the two can be checked.
    pub(crate) type Log = Arc<Mutex<Vec<String>>>;

    pub(crate) fn log() -> Log {
        Arc::new(Mutex::new(Vec::new()))
    }

    /// What the fake was asked, `"<method> <id>"`, status probes and file
    /// reads left out.
    pub(crate) fn actions(log: &Log) -> Vec<String> {
        log.lock()
            .unwrap()
            .iter()
            .filter(|e| !e.starts_with("status ") && !e.starts_with("read_file "))
            .cloned()
            .collect()
    }

    /// The writes the fake was asked for, `"write_file <name>"`, in order.
    pub(crate) fn writes(log: &Log) -> Vec<String> {
        log.lock()
            .unwrap()
            .iter()
            .filter(|e| e.starts_with("write_file "))
            .cloned()
            .collect()
    }

    /// The manager's files as the fake holds them, by name; shared out so a
    /// test reads them after the fake moved into a backend.
    pub(crate) type Files = Arc<Mutex<std::collections::BTreeMap<String, String>>>;

    /// The manager as the tests need it: a state that start, stop and restart
    /// move (a stop goes through `Deactivating` once, as systemd's does), and
    /// optional failures.
    pub(crate) struct FakeClient {
        pub(crate) log: Log,
        /// The states `status` answers, the last one repeated.
        states: Mutex<Vec<InstanceState>>,
        /// What start, stop and restart fail with.
        pub(crate) fail_actions: Option<ClientError>,
        /// What status fails with.
        pub(crate) fail_status: Option<ClientError>,
        /// The files `read_file` answers and `write_file` replaces.
        pub(crate) files: Files,
        /// What read_file fails with.
        pub(crate) fail_read: Option<ClientError>,
        /// What write_file fails with (the file is then left as it was).
        pub(crate) fail_write: Option<ClientError>,
    }

    impl FakeClient {
        pub(crate) fn new(log: &Log, state: InstanceState) -> FakeClient {
            FakeClient {
                log: log.clone(),
                states: Mutex::new(vec![state]),
                fail_actions: None,
                fail_status: None,
                files: Files::default(),
                fail_read: None,
                fail_write: None,
            }
        }

        /// The fake with the manager's file `name` holding `text`.
        pub(crate) fn with_file(self, name: &str, text: &str) -> FakeClient {
            self.files
                .lock()
                .unwrap()
                .insert(name.to_string(), text.to_string());
            self
        }

        fn record(&self, method: &str, id: &str) {
            self.log.lock().unwrap().push(format!("{method} {id}"));
        }

        fn act(&self, method: &str, id: &str, then: Vec<InstanceState>) -> Result<(), ClientError> {
            self.record(method, id);
            if let Some(e) = &self.fail_actions {
                return Err(e.clone());
            }
            *self.states.lock().unwrap() = then;
            Ok(())
        }
    }

    impl ManagerClient for FakeClient {
        fn list_instances(&self) -> Result<Vec<String>, ClientError> {
            self.record("list_instances", "");
            Ok(vec!["default".into()])
        }
        fn status(&self, id: &str) -> Result<InstanceState, ClientError> {
            self.record("status", id);
            if let Some(e) = &self.fail_status {
                return Err(e.clone());
            }
            let mut states = self.states.lock().unwrap();
            Ok(if states.len() > 1 {
                states.remove(0)
            } else {
                states[0].clone()
            })
        }
        fn start(&self, id: &str) -> Result<(), ClientError> {
            self.act("start", id, vec![InstanceState::Active])
        }
        fn stop(&self, id: &str) -> Result<(), ClientError> {
            self.act(
                "stop",
                id,
                vec![InstanceState::Deactivating, InstanceState::Inactive],
            )
        }
        fn restart(&self, id: &str) -> Result<(), ClientError> {
            self.act("restart", id, vec![InstanceState::Active])
        }
        fn read_file(&self, name: &str) -> Result<Option<String>, ClientError> {
            self.record("read_file", name);
            if let Some(e) = &self.fail_read {
                return Err(e.clone());
            }
            Ok(self.files.lock().unwrap().get(name).cloned())
        }
        fn write_file(&self, name: &str, contents: &str) -> Result<(), ClientError> {
            self.record("write_file", name);
            if let Some(e) = &self.fail_write {
                return Err(e.clone());
            }
            self.files
                .lock()
                .unwrap()
                .insert(name.to_string(), contents.to_string());
            Ok(())
        }
    }

    /// The manager a test binary's `backend()` has by default: no instance to
    /// run (every action and probe is `Unavailable`, as with no manager, so no
    /// test spawns or kills a llama-server), and the configuration files kept
    /// where `Direct` keeps them, the per-user files under the test redirect,
    /// so the e2e phases that read them back keep working. Where the manager's
    /// bytes go is pinned with `FakeClient`, not with this.
    pub(crate) struct DiskClient;

    impl DiskClient {
        fn path(name: &str) -> Result<std::path::PathBuf, ClientError> {
            match name {
                "server@default.ini" => Ok(crate::paths::server_ini()),
                "presets.ini" => Ok(crate::paths::presets_ini()),
                "presets.ini.disabled" => Ok(crate::paths::presets_disabled_ini()),
                _ => Err(ClientError::BadFileName),
            }
        }
    }

    impl ManagerClient for DiskClient {
        fn list_instances(&self) -> Result<Vec<String>, ClientError> {
            Err(ClientError::Unavailable)
        }
        fn status(&self, _id: &str) -> Result<InstanceState, ClientError> {
            Err(ClientError::Unavailable)
        }
        fn start(&self, _id: &str) -> Result<(), ClientError> {
            Err(ClientError::Unavailable)
        }
        fn stop(&self, _id: &str) -> Result<(), ClientError> {
            Err(ClientError::Unavailable)
        }
        fn restart(&self, _id: &str) -> Result<(), ClientError> {
            Err(ClientError::Unavailable)
        }
        fn read_file(&self, name: &str) -> Result<Option<String>, ClientError> {
            match std::fs::read_to_string(Self::path(name)?) {
                Ok(text) => Ok(Some(text)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(ClientError::Io(format!("{:?}", e.kind()))),
            }
        }
        fn write_file(&self, name: &str, contents: &str) -> Result<(), ClientError> {
            let path = Self::path(name)?;
            let io = |e: std::io::Error| ClientError::Io(format!("{:?}", e.kind()));
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(io)?;
            }
            crate::ini::atomic_write(&path, contents).map_err(io)
        }
    }

    /// `backend()` in a test binary when no `scoped` is active on the thread.
    pub(crate) fn default_backend() -> &'static super::Backend {
        static DEFAULT: std::sync::OnceLock<super::Backend> = std::sync::OnceLock::new();
        DEFAULT.get_or_init(|| {
            super::Backend::Manager(super::ManagerBackend::new(Box::new(|| {
                Ok(Box::new(DiskClient) as super::Client)
            })))
        })
    }

    thread_local! {
        static SCOPED: std::cell::Cell<Option<&'static super::Backend>> =
            const { std::cell::Cell::new(None) };
    }

    /// What `backend()` returns on this thread.
    pub(crate) fn current() -> &'static super::Backend {
        SCOPED.with(|s| s.get()).unwrap_or_else(default_backend)
    }

    /// Run `f` with `backend()` returning `b` on this thread (not on threads
    /// `f` spawns), then put back what it returned before, even on a panic.
    pub(crate) fn scoped<R>(b: super::Backend, f: impl FnOnce() -> R) -> R {
        struct Restore(Option<&'static super::Backend>);
        impl Drop for Restore {
            fn drop(&mut self) {
                SCOPED.with(|s| s.set(self.0));
            }
        }
        let b: &'static super::Backend = Box::leak(Box::new(b));
        let _restore = Restore(SCOPED.with(|s| s.replace(Some(b))));
        f()
    }

    /// A llama-server stand-in on a free loopback port: records each request
    /// as `"<METHOD> <path>"` in `log` and answers the snapshot calls
    /// (`/models` with no model loaded, `/health`, a slot restore). Its thread
    /// lives as long as the test binary.
    pub(crate) fn http_stub(log: &Log) -> u16 {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let log = log.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let mut raw = Vec::new();
                let mut buf = [0u8; 4096];
                let head_end = loop {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => break None,
                        Ok(n) => raw.extend_from_slice(&buf[..n]),
                    }
                    if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break Some(i + 4);
                    }
                };
                let Some(head_end) = head_end else { continue };
                let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
                let length: usize = head
                    .lines()
                    .find_map(|l| l.strip_prefix("Content-Length: "))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
                while raw.len() < head_end + length {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => raw.extend_from_slice(&buf[..n]),
                    }
                }
                let line = head.lines().next().unwrap_or("");
                let mut parts = line.split(' ');
                let request = format!(
                    "{} {}",
                    parts.next().unwrap_or(""),
                    parts.next().unwrap_or("")
                );
                log.lock().unwrap().push(request.clone());
                let body = if request.starts_with("GET /models") {
                    r#"{"data":[]}"#
                } else if request.starts_with("GET /health") {
                    r#"{"status":"ok"}"#
                } else {
                    r#"{"id_slot":0,"n_restored":1,"n_read":1}"#
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        port
    }

    /// A `Manager` backend over `client`.
    pub(crate) fn backend(client: FakeClient) -> super::Backend {
        let client = std::sync::Mutex::new(Some(client));
        super::Backend::Manager(super::ManagerBackend::new(Box::new(move || {
            let c = client.lock().unwrap().take().expect("connected twice");
            Ok(Box::new(c) as super::Client)
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{self, FakeClient};
    use super::*;
    use pai_core::client::InstanceState;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // These touch no path: start and restart (which load server.ini) are
    // pinned in `server_control_e2e`.

    #[test]
    fn manager_stop_stops_default_and_waits_until_it_is_gone() {
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
        assert_eq!(b.stop(), Ok(true));
        assert_eq!(fake::actions(&log), ["stop default"]);
        // The stop is queued, not done, when the manager answers: the seam
        // asks again until the unit is past Deactivating.
        let entries = log.lock().unwrap().clone();
        let after_stop = entries
            .iter()
            .skip_while(|e| *e != "stop default")
            .filter(|e| *e == "status default")
            .count();
        assert!(
            after_stop >= 2,
            "status asked until Inactive, once per state: {entries:?}"
        );
    }

    #[test]
    fn manager_stop_refused_is_the_client_text() {
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Active);
        c.fail_actions = Some(ClientError::NotAuthorized);
        let b = fake::backend(c);
        assert_eq!(b.stop(), Err(ClientError::NotAuthorized.text()));
        assert_eq!(fake::actions(&log), ["stop default"]);
    }

    /// A process is there from Activating to Deactivating: it holds the GPU
    /// and the port, which is what every caller of `is_running` asks (the
    /// benchmarks wait for it to be gone before they touch the GPU).
    #[test]
    fn manager_is_running_reads_each_state() {
        for (state, running) in [
            (InstanceState::Active, true),
            (InstanceState::Activating, true),
            (InstanceState::Deactivating, true),
            (InstanceState::Inactive, false),
            (InstanceState::Failed, false),
            (InstanceState::Other("maintenance".into()), false),
        ] {
            let log = fake::log();
            let b = fake::backend(FakeClient::new(&log, state.clone()));
            assert_eq!(b.is_running(), running, "{state:?}");
            assert_eq!(
                log.lock().unwrap().clone(),
                ["status default"],
                "{state:?}: one status probe of default, nothing else"
            );
        }
    }

    #[test]
    fn manager_status_error_reads_as_not_running() {
        for e in [ClientError::Unavailable, ClientError::NotAuthorized] {
            let log = fake::log();
            let mut c = FakeClient::new(&log, InstanceState::Active);
            c.fail_status = Some(e.clone());
            assert!(!fake::backend(c).is_running(), "{e:?}");
        }
    }

    /// No manager: the user reads that the manager is not running, from the
    /// strings file, and nothing panics.
    #[test]
    fn no_manager_says_so_and_reads_as_not_running() {
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive);
        c.fail_actions = Some(ClientError::Unavailable);
        c.fail_status = Some(ClientError::Unavailable);
        let b = fake::backend(c);
        assert!(!b.is_running());
        assert_eq!(b.stop(), Err(ClientError::Unavailable.text()));
    }

    #[test]
    fn a_connection_that_fails_is_unavailable() {
        let b = Backend::Manager(ManagerBackend::new(Box::new(|| {
            Err(ClientError::Unavailable)
        })));
        assert!(!b.is_running());
        assert_eq!(b.stop(), Err(ClientError::Unavailable.text()));
    }

    #[test]
    fn the_manager_is_connected_on_first_use_and_once() {
        let connects = Arc::new(AtomicUsize::new(0));
        let log = fake::log();
        let client = std::sync::Mutex::new(Some(FakeClient::new(&log, InstanceState::Active)));
        let counted = connects.clone();
        let b = Backend::Manager(ManagerBackend::new(Box::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            let c = client.lock().unwrap().take().expect("connected twice");
            Ok(Box::new(c) as Client)
        })));
        assert_eq!(connects.load(Ordering::SeqCst), 0, "not at construction");
        assert!(b.is_running());
        assert_eq!(b.stop(), Ok(true));
        assert!(!b.is_running());
        assert_eq!(connects.load(Ordering::SeqCst), 1, "once, then reused");
    }

    /// The manager may start after the configurator: a connect that failed is
    /// tried again on the next call, and the client it then makes is used.
    #[test]
    fn a_failed_connect_is_retried_on_the_next_call() {
        let connects = Arc::new(AtomicUsize::new(0));
        let log = fake::log();
        let client = std::sync::Mutex::new(Some(FakeClient::new(&log, InstanceState::Active)));
        let counted = connects.clone();
        let b = Backend::Manager(ManagerBackend::new(Box::new(move || {
            if counted.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(ClientError::Unavailable);
            }
            let c = client
                .lock()
                .unwrap()
                .take()
                .expect("connected after success");
            Ok(Box::new(c) as Client)
        })));
        assert!(!b.is_running(), "no manager yet");
        assert_eq!(connects.load(Ordering::SeqCst), 1);
        assert!(
            b.is_running(),
            "the manager is up now: the new client answers"
        );
        assert_eq!(connects.load(Ordering::SeqCst), 2, "tried again");
        assert_eq!(b.stop(), Ok(true));
        assert_eq!(connects.load(Ordering::SeqCst), 2, "kept once connected");
        assert_eq!(fake::actions(&log), ["stop default"]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_backend_on_linux_is_the_manager() {
        assert!(matches!(os_backend(), Backend::Manager(_)));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn the_backend_off_linux_is_direct() {
        assert!(matches!(os_backend(), Backend::Direct));
    }

    // ── The configuration files ─────────────────────────────────────────

    #[test]
    fn config_files_have_the_manager_names() {
        assert_eq!(ConfigFile::Server.manager_name(), "server@default.ini");
        assert_eq!(ConfigFile::Presets.manager_name(), "presets.ini");
        assert_eq!(
            ConfigFile::PresetsDisabled.manager_name(),
            "presets.ini.disabled"
        );
    }

    #[test]
    fn manager_read_config_reads_each_file_from_the_manager() {
        let log = fake::log();
        let c = FakeClient::new(&log, InstanceState::Inactive)
            .with_file("server@default.ini", "[Server]\nPort = 8099\n")
            .with_file("presets.ini", "[Qwen3.8-27B]\nctx-size = 8192\n");
        let b = fake::backend(c);
        assert_eq!(
            b.read_config(ConfigFile::Server),
            Ok(Some("[Server]\nPort = 8099\n".to_string()))
        );
        assert_eq!(
            b.read_config(ConfigFile::Presets),
            Ok(Some("[Qwen3.8-27B]\nctx-size = 8192\n".to_string()))
        );
        assert_eq!(
            b.read_config(ConfigFile::PresetsDisabled),
            Ok(None),
            "a file the manager does not have is none, not an error"
        );
        assert_eq!(
            log.lock().unwrap().clone(),
            [
                "read_file server@default.ini",
                "read_file presets.ini",
                "read_file presets.ini.disabled"
            ]
        );
    }

    #[test]
    fn manager_write_config_hands_the_contents_to_the_manager() {
        for file in ConfigFile::ALL {
            let log = fake::log();
            let c = FakeClient::new(&log, InstanceState::Inactive);
            let files = c.files.clone();
            let b = fake::backend(c);
            let text = format!("; {file:?}\r\n[m]\r\nctx-size = 4096\r\n");
            assert_eq!(b.write_config(file, &text), Ok(()), "{file:?}");
            let name = file.manager_name();
            assert_eq!(fake::writes(&log), [format!("write_file {name}")]);
            assert_eq!(files.lock().unwrap().get(&name), Some(&text), "{file:?}");
        }
    }

    /// A write the manager refuses reaches the user as the refusal's own text
    /// (in the language of the strings file this process reads), and the
    /// manager's other errors as the client's text.
    #[test]
    fn a_refused_config_write_is_the_refusals_text() {
        let refusal = pai_core::checks::Refusal {
            key: "not-whole-number-in-range",
            values: ["Port", "99999", "1", "65535"].map(String::from).to_vec(),
        };
        for e in [
            ClientError::Invalid(refusal.clone()),
            ClientError::NotAuthorized,
            ClientError::Unavailable,
        ] {
            let log = fake::log();
            let mut c = FakeClient::new(&log, InstanceState::Inactive)
                .with_file("server@default.ini", "[Server]\nPort = 8080\n");
            c.fail_write = Some(e.clone());
            let files = c.files.clone();
            let b = fake::backend(c);
            assert_eq!(
                b.write_config(ConfigFile::Server, "[Server]\nPort = 99999\n"),
                Err(e.text()),
                "{e:?}"
            );
            assert_eq!(
                files.lock().unwrap().get("server@default.ini").cloned(),
                Some("[Server]\nPort = 8080\n".to_string()),
                "{e:?}: the file as it was"
            );
        }
        assert_eq!(
            ClientError::Invalid(refusal.clone()).text(),
            refusal.text(),
            "Invalid is the refusal's text"
        );
    }

    #[test]
    fn a_failed_config_read_is_the_client_text() {
        for e in [ClientError::NotAuthorized, ClientError::Unavailable] {
            let log = fake::log();
            let mut c =
                FakeClient::new(&log, InstanceState::Inactive).with_file("presets.ini", "[m]\n");
            c.fail_read = Some(e.clone());
            let b = fake::backend(c);
            assert_eq!(b.read_config(ConfigFile::Presets), Err(e.text()), "{e:?}");
        }
        let b = Backend::Manager(ManagerBackend::new(Box::new(|| {
            Err(ClientError::Unavailable)
        })));
        assert_eq!(
            b.read_config(ConfigFile::Server),
            Err(ClientError::Unavailable.text()),
            "no connection"
        );
    }

    /// No test binary of pai-config dials the system bus, whatever runs
    /// first: `system_connect` is `Unavailable` in a test build even without
    /// the redirect. The redirect cannot be unset in this process (the e2e
    /// test sets it), so the check runs in a child of this binary with no
    /// redirect and `DBUS_SYSTEM_BUS_ADDRESS` on a socket of ours, which
    /// nothing may connect to.
    #[cfg(target_os = "linux")]
    #[test]
    fn system_connect_never_dials_from_a_test_binary() {
        use std::time::Instant;
        let dir = tempfile::tempdir().expect("tempdir");
        let socket = dir.path().join("system-bus");
        let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind bus socket");
        listener.set_nonblocking(true).expect("nonblocking");
        let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "server_control::tests::system_connect_child",
                "--include-ignored",
            ])
            .args(["--nocapture", "--test-threads", "1"])
            .env(CHILD_MARKER, "1")
            .env_remove("PRIVATE_AI_DATA_ROOT")
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                format!("unix:path={}", socket.display()),
            )
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("run the child test");
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.try_wait().expect("wait") {
                break status;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the child did not finish within 30 s");
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let out = child.wait_with_output().expect("child output");
        let stdout = String::from_utf8_lossy(&out.stdout);
        match listener.accept() {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Ok(_) => panic!("system_connect dialed the system bus from a test binary:\n{stdout}"),
            Err(e) => panic!("accept: {e}"),
        }
        assert!(
            status.success(),
            "system_connect_child failed:\n{stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            stdout.contains("1 passed"),
            "the child ran no test:\n{stdout}"
        );
    }

    /// Set by `system_connect_never_dials_from_a_test_binary` on its child.
    #[cfg(target_os = "linux")]
    const CHILD_MARKER: &str = "PAI_TEST_SYSTEM_CONNECT_CHILD";

    /// The child half of `system_connect_never_dials_from_a_test_binary`:
    /// ignored, and a no-op without the marker.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "run by system_connect_never_dials_from_a_test_binary in a child process"]
    fn system_connect_child() {
        if std::env::var_os(CHILD_MARKER).is_none() {
            return;
        }
        assert!(std::env::var_os("PRIVATE_AI_DATA_ROOT").is_none());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(system_connect().map(|_| ()));
        });
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(r) => assert_eq!(r, Err(ClientError::Unavailable)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("system_connect did not return within 10 s")
            }
            Err(e) => panic!("the probe thread panicked: {e:?}"),
        }
    }
}

/// E2E phase (called from `editable_widgets_track_model_after_edit`): the
/// `Manager` backend's start and restart, which load server.ini, and that it
/// leaves the per-user tree to the manager (no presets or binary check, no log
/// rotation).
#[cfg(test)]
pub(crate) fn server_control_e2e() {
    use fake::FakeClient;
    use pai_core::client::InstanceState;
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());

    // Under the redirect the Linux backend never dials the system bus: a
    // socket of ours stands in for it, and nothing may connect to it. The only
    // place in this binary that sets `DBUS_SYSTEM_BUS_ADDRESS`.
    #[cfg(target_os = "linux")]
    {
        let socket = dir.path().join("system-bus");
        let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind bus socket");
        listener.set_nonblocking(true).expect("nonblocking");
        std::env::set_var(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}", socket.display()),
        );
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let connected = system_connect().map(|_| ());
            // The backend this OS builds, over the real connect.
            let running =
                Backend::Manager(ManagerBackend::new(Box::new(system_connect))).is_running();
            let _ = tx.send((connected, running));
        });
        match rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok((connected, running)) => {
                assert_eq!(connected, Err(ClientError::Unavailable), "system_connect");
                assert!(!running, "the manager backend reads as not running");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("system_connect or backend() did not return within 10 s")
            }
            Err(e) => panic!("the probe thread panicked: {e:?}"),
        }
        match listener.accept() {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Ok(_) => panic!("something dialed the system bus under the test redirect"),
            Err(e) => panic!("accept: {e}"),
        }
    }

    // The per-user server.ini is not the instance's config under this
    // backend: the manager's server@default.ini is (8099 below).
    let server_ini = crate::paths::server_ini();
    std::fs::create_dir_all(server_ini.parent().unwrap()).expect("config dir");
    std::fs::write(&server_ini, "[Server]\nPort = 9999\n").expect("server.ini");
    const MANAGED: &str = "[Server]\nPort = 8099\n";
    // No presets.ini and no llama-server binary: those are the manager's to
    // check under this backend, not the configurator's.
    assert!(!crate::paths::presets_ini().exists());

    // Started from Inactive: one start of default, the config read back from
    // the manager.
    let log = fake::log();
    let b = fake::backend(
        FakeClient::new(&log, InstanceState::Inactive).with_file("server@default.ini", MANAGED),
    );
    match b.start() {
        Ok(Some(cfg)) => assert_eq!(cfg.port_or_default(), 8099),
        other => panic!("start from Inactive: {other:?}"),
    }
    assert_eq!(fake::actions(&log), ["start default"]);

    // Already there (Activating is a start under way): nothing asked.
    for state in [InstanceState::Active, InstanceState::Activating] {
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, state.clone()));
        assert!(matches!(b.start(), Ok(None)), "{state:?}");
        assert!(fake::actions(&log).is_empty(), "{state:?}: {log:?}");
    }

    // A refused start and no manager: the client's text.
    for e in [ClientError::NotAuthorized, ClientError::Unavailable] {
        let log = fake::log();
        let mut c = FakeClient::new(&log, InstanceState::Inactive);
        c.fail_actions = Some(e.clone());
        if e == ClientError::Unavailable {
            c.fail_status = Some(e.clone());
        }
        let b = fake::backend(c);
        match b.start() {
            Err(text) => assert_eq!(text, e.text(), "{e:?}"),
            Ok(x) => panic!("{e:?}: started {x:?}"),
        }
        match b.restart() {
            Err(text) => assert_eq!(text, e.text(), "{e:?}"),
            Ok(x) => panic!("{e:?}: restarted {x:?}"),
        }
    }

    // Restart: the manager's restart, the config read back from the manager.
    let log = fake::log();
    let b = fake::backend(
        FakeClient::new(&log, InstanceState::Active).with_file("server@default.ini", MANAGED),
    );
    match b.restart() {
        Ok(Some(cfg)) => assert_eq!(cfg.port_or_default(), 8099),
        other => panic!("restart: {other:?}"),
    }
    assert_eq!(fake::actions(&log), ["restart default"]);

    // The per-user log is not this backend's: a log well past the rotation
    // threshold (settings default) is neither closed on start nor on stop.
    let server_log = crate::paths::server_log();
    let logs = server_log.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&logs).expect("logs dir");
    let big = "x".repeat(2 * 1024 * 1024);
    std::fs::write(&server_log, &big).expect("big log");
    let log = fake::log();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
    assert!(matches!(b.start(), Ok(Some(_))));
    assert_eq!(b.stop(), Ok(true));
    assert_eq!(fake::actions(&log), ["start default", "stop default"]);
    assert_eq!(
        std::fs::read_to_string(&server_log).expect("log still there"),
        big,
        "log untouched"
    );
    let listed: Vec<_> = std::fs::read_dir(&logs)
        .expect("logs dir")
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(listed, [std::ffi::OsString::from("llama-server.log")]);
}

/// E2E phase (called from `editable_widgets_track_model_after_edit`): every
/// reader and writer of the instance's configuration files goes through the
/// seam. `Direct` keeps the per-user files under their names; the `Manager`
/// backend reads and writes the manager's files and nothing under the
/// per-user config directory; a write the manager refuses reaches the caller
/// (the CLI, the GUI's save status) as the refusal's own text.
#[cfg(test)]
pub(crate) fn config_seam_e2e() {
    use crate::paths;
    use crate::presets::{self, Preset};
    use crate::server_cfg::{self, ServerConfig};
    use fake::FakeClient;
    use pai_core::client::InstanceState;
    let dir = tempfile::tempdir().expect("tempdir");
    paths::redirect_for_tests(dir.path());
    let user = paths::config_dir();

    // ── Direct: the per-user files, under the names they always had ──────
    let d = Backend::Direct;
    for (file, path) in [
        (ConfigFile::Server, paths::server_ini()),
        (ConfigFile::Presets, paths::presets_ini()),
        (ConfigFile::PresetsDisabled, paths::presets_disabled_ini()),
    ] {
        assert_eq!(d.config_path(file), path, "{file:?}");
        assert_eq!(d.read_config(file), Ok(None), "{file:?}: no file yet");
        let text = format!("; {file:?}\r\n[m]\r\nctx-size = 4096\r\n");
        assert_eq!(d.write_config(file, &text), Ok(()), "{file:?}");
        assert_eq!(std::fs::read_to_string(&path).expect("written"), text);
        assert_eq!(d.read_config(file), Ok(Some(text)), "{file:?}");
    }

    // A write that fails: an error, the old file kept, no temp file left.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let listing = || {
            let mut v: Vec<_> = std::fs::read_dir(&user)
                .expect("config dir")
                .map(|e| e.unwrap().file_name())
                .collect();
            v.sort();
            v
        };
        let before = listing();
        let old = std::fs::read_to_string(paths::server_ini()).expect("server.ini");
        std::fs::set_permissions(&user, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        // Root ignores the mode: only check what a failing write does when
        // a write really fails.
        let probe = std::fs::write(user.join("probe"), "x");
        let result = probe
            .is_err()
            .then(|| d.write_config(ConfigFile::Server, "[Server]\nPort = 1\n"));
        std::fs::set_permissions(&user, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        if let Some(result) = result {
            assert!(result.is_err(), "a failed write is an error: {result:?}");
            assert_eq!(
                std::fs::read_to_string(paths::server_ini()).expect("server.ini"),
                old
            );
            assert_eq!(listing(), before, "no temp file left");
        } else {
            let _ = std::fs::remove_file(user.join("probe"));
        }
    }

    // A file that cannot be read is an error, not "no file": a directory
    // in its place.
    std::fs::remove_file(paths::presets_disabled_ini()).expect("remove");
    std::fs::create_dir(paths::presets_disabled_ini()).expect("dir in its place");
    assert!(
        d.read_config(ConfigFile::PresetsDisabled).is_err(),
        "an unreadable file is not none"
    );
    std::fs::remove_dir(paths::presets_disabled_ini()).expect("remove dir");

    // ── Manager: the manager's files, shown where the manager keeps them ──
    let log = fake::log();
    let m = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
    for file in ConfigFile::ALL {
        assert_eq!(
            m.config_path(file),
            paths::system_config_dir().join(file.manager_name()),
            "{file:?}"
        );
    }

    // Reads: from the manager, never from the per-user files, which say
    // something else here.
    std::fs::write(paths::server_ini(), "[Server]\nPort = 9999\n").expect("user server.ini");
    std::fs::write(
        paths::presets_ini(),
        "[user-only]\nmodel = /models/user-only.gguf\n",
    )
    .expect("user presets.ini");
    let seeded = || {
        FakeClient::new(&log, InstanceState::Inactive)
            .with_file("server@default.ini", "[Server]\nPort = 8099\n")
            .with_file(
                "presets.ini",
                "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
            )
            .with_file(
                "presets.ini.disabled",
                "[Qwen3.8-Flash-Next]\nmodel = /models/Qwen3.8-Flash-Next-IQ3_S.gguf\n",
            )
    };
    fake::scoped(fake::backend(seeded()), || {
        assert_eq!(server_cfg::load().port_or_default(), 8099, "server.ini");
        let enabled: Vec<String> = presets::load_all().into_iter().map(|p| p.id).collect();
        assert_eq!(enabled, ["Qwen3.8-27B"], "presets.ini");
        let listed: Vec<(String, bool)> = presets::load_listed()
            .into_iter()
            .map(|l| (l.preset.id, l.enabled))
            .collect();
        assert_eq!(
            listed,
            [
                ("Qwen3.8-27B".to_string(), true),
                ("Qwen3.8-Flash-Next".to_string(), false)
            ]
        );
    });

    // Writes: the manager's files, and nothing under the per-user directory.
    std::fs::remove_dir_all(&user).expect("clear the per-user config");
    let nothing_per_user = |when: &str| {
        for name in ["server.ini", "presets.ini", "presets.ini.disabled"] {
            assert!(
                !user.join(name).exists(),
                "{when}: {name} written under the per-user config directory"
            );
        }
    };
    let c = seeded();
    let files = c.files.clone();
    let file = |name: &str| files.lock().unwrap().get(name).cloned().unwrap_or_default();
    let ids = |text: String| -> Vec<String> {
        crate::ini::parse_sections(&text)
            .into_iter()
            .map(|s| s.id)
            .collect()
    };
    let cfg = ServerConfig {
        port: Some(8100),
        models_dir: Some(dir.path().join("models").display().to_string()),
        state_dir: Some(dir.path().join("state").display().to_string()),
        ..Default::default()
    };
    let q8 = Preset::new_default(
        "Qwen3.8-27B-Q8".into(),
        "/models/Qwen3.8-27B-Q8_0.gguf".into(),
    );
    fake::scoped(fake::backend(c), || {
        log.lock().unwrap().clear();
        server_cfg::save(&cfg).expect("server save");
        assert_eq!(fake::writes(&log), ["write_file server@default.ini"]);
        assert_eq!(file("server@default.ini"), server_cfg::render(&cfg));
        nothing_per_user("server_cfg::save");

        log.lock().unwrap().clear();
        presets::save(&q8).expect("preset save");
        assert_eq!(fake::writes(&log), ["write_file presets.ini"]);
        assert_eq!(ids(file("presets.ini")), ["Qwen3.8-27B", "Qwen3.8-27B-Q8"]);
        let section = crate::ini::parse_sections(&file("presets.ini"))
            .into_iter()
            .find(|s| s.id == "Qwen3.8-27B-Q8")
            .expect("saved section");
        assert_eq!(
            section.keys.get("model").map(String::as_str),
            Some("/models/Qwen3.8-27B-Q8_0.gguf")
        );
        nothing_per_user("presets::save");

        // Switched off: the destination written first, as with the files.
        log.lock().unwrap().clear();
        presets::set_enabled("Qwen3.8-27B", false).expect("switch off");
        assert_eq!(
            fake::writes(&log),
            ["write_file presets.ini.disabled", "write_file presets.ini"]
        );
        assert_eq!(ids(file("presets.ini")), ["Qwen3.8-27B-Q8"]);
        assert_eq!(
            ids(file("presets.ini.disabled")),
            ["Qwen3.8-Flash-Next", "Qwen3.8-27B"]
        );
        log.lock().unwrap().clear();
        presets::set_enabled("Qwen3.8-27B", true).expect("switch on");
        assert_eq!(
            fake::writes(&log),
            ["write_file presets.ini", "write_file presets.ini.disabled"]
        );
        assert_eq!(ids(file("presets.ini.disabled")), ["Qwen3.8-Flash-Next"]);
        assert_eq!(ids(file("presets.ini")), ["Qwen3.8-27B-Q8", "Qwen3.8-27B"]);

        presets::rename("Qwen3.8-27B-Q8", "Qwen3.8-27B-Q8_0").expect("rename");
        assert_eq!(
            ids(file("presets.ini")),
            ["Qwen3.8-27B-Q8_0", "Qwen3.8-27B"]
        );
        presets::delete("Qwen3.8-Flash-Next").expect("delete");
        assert!(ids(file("presets.ini.disabled")).is_empty());
        nothing_per_user("set_enabled, rename, delete");

        // The first preset saved with no ModelsDir seeds it in the manager's
        // server file.
        files.lock().unwrap().insert(
            "server@default.ini".into(),
            "[Server]\nPort = 8099\n".into(),
        );
        let llm = dir.path().join("llm");
        let model = llm.join("models").join("Qwen3.8-4B-Q4_K_M.gguf");
        presets::save(&Preset::new_default(
            "Qwen3.8-4B".into(),
            model.display().to_string(),
        ))
        .expect("save seeding ModelsDir");
        assert_eq!(
            server_cfg::from_text(&file("server@default.ini")).models_dir,
            Some(llm.display().to_string())
        );

        // A download's preset pins ModelsDir to where its files went.
        files.lock().unwrap().insert(
            "server@default.ini".into(),
            "[Server]\nPort = 8099\n".into(),
        );
        let hf_root = dir.path().join("hf-models");
        let downloaded = Preset::new_default(
            "Qwen3.8-8B".into(),
            hf_root
                .join("models")
                .join("example")
                .join("Qwen3.8-8B-GGUF")
                .join("Qwen3.8-8B-Q4_K_M.gguf")
                .display()
                .to_string(),
        );
        crate::hf::save_preset(&downloaded, &hf_root).expect("download's preset");
        assert_eq!(
            server_cfg::from_text(&file("server@default.ini")).models_dir,
            Some(hf_root.display().to_string())
        );
        assert!(ids(file("presets.ini")).contains(&"Qwen3.8-8B".to_string()));
        nothing_per_user("ModelsDir seeding");
    });

    // ── Refused writes: the refusal's own text, and nothing changed ──────
    let refusal = pai_core::checks::Refusal {
        key: "not-whole-number-in-range",
        values: ["Port", "99999", "1", "65535"].map(String::from).to_vec(),
    };
    for e in [
        ClientError::Invalid(refusal),
        ClientError::NotAuthorized,
        ClientError::Unavailable,
    ] {
        let mut c = seeded();
        c.fail_write = Some(e.clone());
        let files = c.files.clone();
        let before = files.lock().unwrap().clone();
        fake::scoped(fake::backend(c), || {
            let err = server_cfg::save(&cfg).expect_err("server save refused");
            assert_eq!(err.to_string(), e.text(), "{e:?}: server_cfg::save");
            let err = presets::save(&q8).expect_err("preset save refused");
            assert_eq!(err.to_string(), e.text(), "{e:?}: presets::save");
            let err = presets::set_enabled("Qwen3.8-27B", false).expect_err("switch refused");
            assert_eq!(err.to_string(), e.text(), "{e:?}: set_enabled");
        });
        assert_eq!(*files.lock().unwrap(), before, "{e:?}: nothing changed");
        nothing_per_user("a refused write");
    }

    // ── A read that fails stops the write ────────────────────────────────
    // Writing what an empty read leaves would replace the manager's file
    // with one section, or none: every other preset lost.
    let e = ClientError::Io("PermissionDenied".into());
    let mut c = seeded();
    c.fail_read = Some(e.clone());
    let files = c.files.clone();
    let before = files.lock().unwrap().clone();
    log.lock().unwrap().clear();
    fake::scoped(fake::backend(c), || {
        for (what, result) in [
            ("presets::save", presets::save(&q8)),
            (
                "set_enabled off",
                presets::set_enabled("Qwen3.8-27B", false),
            ),
            (
                "set_enabled on",
                presets::set_enabled("Qwen3.8-Flash-Next", true),
            ),
            ("rename", presets::rename("Qwen3.8-27B", "Qwen3.8-27B-Q8_0")),
            ("delete", presets::delete("Qwen3.8-27B")),
            (
                "hf::save_preset",
                crate::hf::save_preset(&q8, &dir.path().join("hf-models")),
            ),
        ] {
            let err = result.expect_err(what);
            assert_eq!(err.to_string(), e.text(), "{what}");
        }
    });
    assert_eq!(fake::writes(&log), Vec::<String>::new(), "nothing written");
    assert_eq!(*files.lock().unwrap(), before, "the files as they were");
    nothing_per_user("a failed read");
}
