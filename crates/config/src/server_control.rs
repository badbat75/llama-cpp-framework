//! Starting, stopping and probing the `default` llama-server instance, behind
//! one seam with two backends: `Direct` (the configurator spawns and kills the
//! process itself, `runstate`) and `Manager` (the manager service runs it, asked
//! through `pai_core::client`). `backend()` picks the one of this OS.

use crate::server_cfg::ServerConfig;
use crate::strings;
use pai_core::client::{ClientError, InstanceState, ManagerClient};
use std::sync::{Arc, Mutex, OnceLock};
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
        Ok(Some(crate::server_cfg::load()))
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
        Ok(Some(crate::server_cfg::load()))
    }
}

/// How often a stop asks whether the instance is gone.
const STOP_POLL: Duration = Duration::from_millis(250);

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
}

/// The connection the Linux backend makes: the system bus, except under the
/// test redirect (`PRIVATE_AI_DATA_ROOT` set), where it is `Unavailable`
/// without dialing anything.
#[cfg(target_os = "linux")]
pub(crate) fn system_connect() -> Result<Client, ClientError> {
    if std::env::var_os("PRIVATE_AI_DATA_ROOT").is_some() {
        return Err(ClientError::Unavailable);
    }
    pai_core::client::dbus::DbusClient::connect_system().map(|c| Box::new(c) as Client)
}

/// The backend of this OS: the manager on Linux, `Direct` elsewhere.
pub fn backend() -> &'static Backend {
    static BACKEND: OnceLock<Backend> = OnceLock::new();
    BACKEND.get_or_init(|| {
        #[cfg(target_os = "linux")]
        {
            Backend::Manager(ManagerBackend::new(Box::new(system_connect)))
        }
        #[cfg(not(target_os = "linux"))]
        {
            Backend::Direct
        }
    })
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

    /// What the fake was asked, `"<method> <id>"`, status probes left out.
    pub(crate) fn actions(log: &Log) -> Vec<String> {
        log.lock()
            .unwrap()
            .iter()
            .filter(|e| !e.starts_with("status "))
            .cloned()
            .collect()
    }

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
    }

    impl FakeClient {
        pub(crate) fn new(log: &Log, state: InstanceState) -> FakeClient {
            FakeClient {
                log: log.clone(),
                states: Mutex::new(vec![state]),
                fail_actions: None,
                fail_status: None,
            }
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
            Ok(None)
        }
        fn write_file(&self, name: &str, _contents: &str) -> Result<(), ClientError> {
            self.record("write_file", name);
            Ok(())
        }
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
        assert!(matches!(backend(), Backend::Manager(_)));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn the_backend_off_linux_is_direct() {
        assert!(matches!(backend(), Backend::Direct));
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
            let running = backend().is_running();
            let _ = tx.send((connected, running));
        });
        match rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok((connected, running)) => {
                assert_eq!(connected, Err(ClientError::Unavailable), "system_connect");
                assert!(!running, "the global backend reads as not running");
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

    let server_ini = crate::paths::server_ini();
    std::fs::create_dir_all(server_ini.parent().unwrap()).expect("config dir");
    std::fs::write(&server_ini, "[Server]\nPort = 8099\n").expect("server.ini");
    // No presets.ini and no llama-server binary: those are the manager's to
    // check under this backend, not the configurator's.
    assert!(!crate::paths::presets_ini().exists());

    // Started from Inactive: one start of default, the config it read back.
    let log = fake::log();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
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

    // Restart: the manager's restart, the config read back.
    let log = fake::log();
    let b = fake::backend(FakeClient::new(&log, InstanceState::Active));
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
