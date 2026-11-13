//! The D-Bus face of `pai-manager` on Linux: interface
//! `org.privateai.Manager1` at `/org/privateai/Manager1` on the system bus,
//! over the core [`Manager`]. The caller of a method is its sender (the
//! unique bus name the bus stamps), handed to the core as [`Caller`]; every
//! [`ManagerError`] replies as a D-Bus error `org.privateai.Manager1.Error.<Kind>`.

use super::service::{
    Action, Authority, Caller, Manager, ManagerError, Resolver, ServiceControl, ServiceError,
    SystemResolver, UnitState,
};
use std::sync::{Arc, Mutex};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

/// The well-known name the service owns on the system bus.
pub const BUS_NAME: &str = "org.privateai.Manager1";
/// The object the interface is served at.
pub const OBJECT_PATH: &str = "/org/privateai/Manager1";
/// The interface name.
pub const INTERFACE: &str = "org.privateai.Manager1";

/// The D-Bus error name and message `e` replies with.
pub fn error_reply(e: &ManagerError) -> (String, String) {
    use zbus::DBusError;
    let error = Error::from(e.clone());
    let message = error.description().unwrap_or_default().to_string();
    (error.name().to_string(), message)
}

/// The caller of a method call from its sender; a call without one is
/// refused before anything is asked or done.
pub fn caller(sender: Option<&str>) -> Result<Caller, ManagerError> {
    match sender {
        Some(name) if !name.is_empty() => Ok(Caller(name.to_string())),
        _ => Err(ManagerError::NotAuthorized),
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

/// The error replies of the interface, `org.privateai.Manager1.Error.<Kind>`,
/// one per [`ManagerError`], the message carrying its detail.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.privateai.Manager1.Error")]
pub enum Error {
    NotAuthorized(String),
    InvalidId(String),
    NoInstance(String),
    BadFileName(String),
    AddressInUse(String),
    UnresolvableHost(String),
    Service(String),
    Io(String),
}

/// The message is protocol data, not user text: the detail when the error
/// has one (an instance id, a host, the service manager's own message, the
/// I/O error kind), else the kind's own name. The client shows its own text.
impl From<ManagerError> for Error {
    fn from(e: ManagerError) -> Self {
        match e {
            ManagerError::NotAuthorized => Error::NotAuthorized("NotAuthorized".into()),
            ManagerError::InvalidId => Error::InvalidId("InvalidId".into()),
            ManagerError::NoInstance => Error::NoInstance("NoInstance".into()),
            ManagerError::BadFileName => Error::BadFileName("BadFileName".into()),
            ManagerError::AddressInUse(id) => Error::AddressInUse(id),
            ManagerError::UnresolvableHost(host) => Error::UnresolvableHost(host),
            ManagerError::Service(e) => Error::Service(e.0),
            ManagerError::Io(kind) => Error::Io(kind.to_string()),
        }
    }
}

/// The served object: the core, shared with whoever else holds it.
pub struct ManagerInterface<S: ServiceControl, A: Authority, R: Resolver> {
    manager: Arc<Mutex<Manager<S, A, R>>>,
}

impl<S: ServiceControl, A: Authority, R: Resolver> ManagerInterface<S, A, R> {
    pub fn new(manager: Arc<Mutex<Manager<S, A, R>>>) -> Self {
        Self { manager }
    }

    /// The core, locked; a poisoned lock is taken over, since the core keeps
    /// no state a panic could leave half done.
    fn core(&self) -> std::sync::MutexGuard<'_, Manager<S, A, R>> {
        self.manager.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The caller of a call from its header.
fn sender_of(header: &zbus::message::Header<'_>) -> Result<Caller, ManagerError> {
    caller(header.sender().map(|s| s.as_str()))
}

#[zbus::interface(name = "org.privateai.Manager1")]
impl<S, A, R> ManagerInterface<S, A, R>
where
    S: ServiceControl + Send + 'static,
    A: Authority + Send + 'static,
    R: Resolver + Send + 'static,
{
    fn list_instances(&self) -> Vec<String> {
        self.core().list_instances()
    }

    fn status(&self, id: &str) -> Result<String, Error> {
        Ok(state_name(self.core().status(id)?).to_string())
    }

    fn start(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
    ) -> Result<(), Error> {
        let caller = sender_of(&header)?;
        Ok(self.core().start(&caller, id)?)
    }

    fn stop(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
    ) -> Result<(), Error> {
        let caller = sender_of(&header)?;
        Ok(self.core().stop(&caller, id)?)
    }

    fn restart(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
    ) -> Result<(), Error> {
        let caller = sender_of(&header)?;
        Ok(self.core().restart(&caller, id)?)
    }

    fn read_file(&self, name: &str) -> Result<(bool, String), Error> {
        Ok(match self.core().read_file(name)? {
            Some(text) => (true, text),
            None => (false, String::new()),
        })
    }

    fn write_file(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        name: &str,
        contents: &str,
    ) -> Result<(), Error> {
        let caller = sender_of(&header)?;
        Ok(self.core().write_file(&caller, name, contents)?)
    }
}

/// [`ServiceControl`] over systemd's D-Bus API (`org.freedesktop.systemd1.Manager`
/// `StartUnit` / `StopUnit` / `RestartUnit`, mode "replace", on
/// `llama.cpp@<id>.service`; the state is the unit's `ActiveState`, a unit
/// not loaded reading as inactive). Checked in a real run, not by tests.
/// Its connection is not the one the interface is served on: a blocking
/// call from a method handler would stall that connection's own executor.
pub struct SystemdService {
    conn: Connection,
}

impl SystemdService {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    fn unit_job(&self, method: &str, id: &str) -> Result<(), ServiceError> {
        let unit = format!("llama.cpp@{id}.service");
        self.manager()?
            .call::<_, _, OwnedObjectPath>(method, &(unit.as_str(), "replace"))
            .map(|_job| ())
            .map_err(service_error)
    }

    fn manager(&self) -> Result<Proxy<'_>, ServiceError> {
        Proxy::new(
            &self.conn,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )
        .map_err(service_error)
    }
}

fn service_error(e: zbus::Error) -> ServiceError {
    ServiceError(e.to_string())
}

impl ServiceControl for SystemdService {
    fn start(&self, id: &str) -> Result<(), ServiceError> {
        self.unit_job("StartUnit", id)
    }

    fn stop(&self, id: &str) -> Result<(), ServiceError> {
        self.unit_job("StopUnit", id)
    }

    fn restart(&self, id: &str) -> Result<(), ServiceError> {
        self.unit_job("RestartUnit", id)
    }

    fn status(&self, id: &str) -> Result<UnitState, ServiceError> {
        let unit = format!("llama.cpp@{id}.service");
        let path: OwnedObjectPath = self
            .manager()?
            .call("LoadUnit", &(unit.as_str(),))
            .map_err(service_error)?;
        let state: String = Proxy::new(
            &self.conn,
            "org.freedesktop.systemd1",
            path,
            "org.freedesktop.systemd1.Unit",
        )
        .and_then(|unit| unit.get_property("ActiveState"))
        .map_err(service_error)?;
        Ok(match state.as_str() {
            "active" => UnitState::Active,
            "failed" => UnitState::Failed,
            "activating" => UnitState::Activating,
            "deactivating" => UnitState::Deactivating,
            _ => UnitState::Inactive,
        })
    }
}

/// [`Authority`] over polkit (`org.freedesktop.PolicyKit1.Authority.CheckAuthorization`,
/// subject "system-bus-name" with the caller's name, AllowUserInteraction;
/// any failure answers not authorized). Checked in a real run, not by tests.
/// Like [`SystemdService`], on a connection other than the served one.
pub struct PolkitAuthority {
    conn: Connection,
}

impl PolkitAuthority {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }
}

impl Authority for PolkitAuthority {
    fn is_authorized(&self, caller: &Caller, action: Action) -> bool {
        use std::collections::HashMap;
        use zbus::zvariant::Value;
        /// `CheckAuthorizationFlags.AllowUserInteraction`.
        const ALLOW_USER_INTERACTION: u32 = 1;
        let subject = (
            "system-bus-name",
            HashMap::from([("name", Value::from(caller.0.as_str()))]),
        );
        let details: HashMap<&str, &str> = HashMap::new();
        let result = Proxy::new(
            &self.conn,
            "org.freedesktop.PolicyKit1",
            "/org/freedesktop/PolicyKit1/Authority",
            "org.freedesktop.PolicyKit1.Authority",
        )
        .and_then(|authority| {
            authority.call::<_, _, (bool, bool, HashMap<String, String>)>(
                "CheckAuthorization",
                &(subject, action.id(), details, ALLOW_USER_INTERACTION, ""),
            )
        });
        matches!(result, Ok((true, _, _)))
    }
}

/// The system-bus service: [`ManagerInterface`] over systemd and polkit
/// served at [`OBJECT_PATH`] under [`BUS_NAME`], then every instance started
/// once. The connection to keep alive while serving, and the boot failures.
pub fn serve() -> zbus::Result<(Connection, Vec<(String, ManagerError)>)> {
    let client = Connection::system()?;
    let core = Arc::new(Mutex::new(Manager::new(
        SystemdService::new(client.clone()),
        PolkitAuthority::new(client),
        SystemResolver,
    )));
    let served = zbus::blocking::connection::Builder::system()?
        .serve_at(OBJECT_PATH, ManagerInterface::new(core.clone()))?
        .name(BUS_NAME)?
        .build()?;
    let failures = core
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .start_all_at_boot();
    Ok((served, failures))
}

/// E2E phase for the D-Bus interface, for this crate's e2e test to call: it sets
/// the process-wide `PRIVATE_AI_DATA_ROOT` redirect (`src/tests/mod.rs`).
/// The interface is served over a peer-to-peer connection on a Unix socket
/// pair inside the test, never a bus; the seams are the core's fakes. A
/// client given a unique name stamps it as the sender of its calls, as the
/// bus daemon would; one without has none.
#[cfg(test)]
pub(crate) fn manager_dbus_e2e() {
    use super::service::fakes::{Call, FakeAuthority, FakeResolver, FakeService};
    use super::service::Action;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    use zbus::blocking::connection::Builder;
    use zbus::blocking::Connection;

    type Core = Manager<FakeService, FakeAuthority, FakeResolver>;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::system_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    let default_ini = "[Server]\nPort = 8080\nHostname = 0.0.0.0\n";
    let broken_ini = "[Server]\nPort = 8090\nHostname = 127.0.0.1\n";
    let presets = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
    std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
    std::fs::write(sys.join("server@broken.ini"), broken_ini).expect("write broken");
    std::fs::write(sys.join("presets.ini"), presets).expect("write presets");

    // Serve `core` on one end of a socket pair, the server built on its own
    // thread (both ends handshake at once); the client on the other end,
    // stamping `sender` when given. Every call has a deadline: a stub that
    // panics in the server must fail the test, not hang it.
    let connect = |core: &Arc<Mutex<Core>>, sender: Option<&str>| -> (Connection, Connection) {
        let (s0, s1) = UnixStream::pair().expect("socket pair");
        let iface = ManagerInterface::new(core.clone());
        let server = std::thread::spawn(move || {
            Builder::async_io_unix_stream(s0)
                .server(zbus::Guid::generate())
                .expect("server guid")
                .p2p()
                .serve_at(OBJECT_PATH, iface)
                .expect("serve_at")
                .build()
                .expect("server connection")
        });
        let client = Builder::async_io_unix_stream(s1)
            .p2p()
            .method_timeout(Duration::from_secs(10))
            .build()
            .expect("client connection");
        // The bus would stamp the sender; on a p2p client it is set by hand.
        if let Some(name) = sender {
            client.inner().set_unique_name(name).expect("unique name");
        }
        (server.join().expect("server thread"), client)
    };
    let core_in = |state: UnitState, authority: bool, failing: &[&str]| -> Arc<Mutex<Core>> {
        let mut service = FakeService::new();
        service.state = state;
        service.failing = failing.iter().map(|s| s.to_string()).collect();
        Arc::new(Mutex::new(Manager::new(
            service,
            FakeAuthority::new(authority),
            FakeResolver::lan(),
        )))
    };
    let core = |authority: bool, failing: &[&str]| core_in(UnitState::Active, authority, failing);
    // A method call: Ok(reply), or Err((error name, message)).
    fn call<B>(conn: &Connection, method: &str, body: &B) -> Result<zbus::Message, (String, String)>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        conn.call_method(None::<&str>, OBJECT_PATH, Some(INTERFACE), method, body)
            .map_err(|e| match e {
                zbus::Error::MethodError(name, detail, _) => {
                    (name.to_string(), detail.unwrap_or_default())
                }
                zbus::Error::FDO(e) => {
                    use zbus::DBusError;
                    (
                        e.name().to_string(),
                        e.description().unwrap_or_default().to_string(),
                    )
                }
                other => panic!("{method}: not a method error reply: {other:?}"),
            })
    }
    fn body<T: for<'de> zbus::zvariant::DynamicDeserialize<'de>>(m: &zbus::Message) -> T {
        m.body().deserialize().expect("reply body")
    }
    let err_name = |kind: &str| format!("org.privateai.Manager1.Error.{kind}");
    let calls = |c: &Arc<Mutex<Core>>| c.lock().unwrap().service().calls.borrow().clone();
    let asked = |c: &Arc<Mutex<Core>>| c.lock().unwrap().authority().asked.borrow().clone();
    let alice = Caller(":1.42".into());

    // ── No sender: every change refused before polkit is even asked ──────
    {
        let c = core(true, &[]);
        let (_server, client) = connect(&c, None);
        for (method, id) in [
            ("Start", "default"),
            ("Stop", "default"),
            ("Restart", "default"),
        ] {
            let e = call(&client, method, &(id,)).expect_err(method);
            assert_eq!(e.0, err_name("NotAuthorized"), "{method} without a sender");
        }
        let e = call(
            &client,
            "WriteFile",
            &("presets.ini", "[x]\nmodel = /m/x.gguf\n"),
        )
        .expect_err("WriteFile");
        assert_eq!(e.0, err_name("NotAuthorized"), "WriteFile without a sender");
        assert_eq!(
            std::fs::read_to_string(sys.join("presets.ini")).unwrap(),
            presets,
            "a refused write leaves the file"
        );
        assert_eq!(calls(&c), vec![], "nothing reached the service manager");
        assert_eq!(asked(&c), vec![], "a call with no sender asks nobody");

        // Reads need no caller.
        let r = call(&client, "ListInstances", &()).expect("ListInstances");
        assert_eq!(body::<Vec<String>>(&r), vec!["broken", "default"]);
        let r = call(&client, "Status", &("default",)).expect("Status");
        assert_eq!(body::<String>(&r), "active");
        let r = call(&client, "ReadFile", &("presets.ini",)).expect("ReadFile");
        assert_eq!(body::<(bool, String)>(&r), (true, presets.to_string()));
        assert_eq!(calls(&c), vec![Call::Status("default".into())]);
    }

    // ── A caller polkit refuses: asked with its bus name, nothing done ───
    {
        let c = core(false, &[]);
        let (_server, client) = connect(&c, Some(":1.42"));
        let e = call(&client, "Start", &("default",)).expect_err("Start");
        assert_eq!(e.0, err_name("NotAuthorized"));
        let e = call(&client, "WriteFile", &("presets.ini", "")).expect_err("WriteFile");
        assert_eq!(e.0, err_name("NotAuthorized"));
        assert_eq!(calls(&c), vec![], "a refused caller starts nothing");
        assert_eq!(
            asked(&c),
            vec![
                (alice.clone(), Action::Control),
                (alice.clone(), Action::Configure)
            ],
            "polkit is asked about the sender, per action"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("presets.ini")).unwrap(),
            presets
        );
    }

    // ── An authorized caller: each method reaches the core ───────────────
    {
        let c = core(true, &["broken"]);
        let (_server, client) = connect(&c, Some(":1.42"));

        for method in ["Start", "Stop", "Restart"] {
            call(&client, method, &("default",)).unwrap_or_else(|e| panic!("{method}: {e:?}"));
        }
        assert_eq!(
            calls(&c),
            vec![
                Call::Start("default".into()),
                Call::Stop("default".into()),
                Call::Restart("default".into())
            ]
        );
        assert_eq!(asked(&c), vec![(alice.clone(), Action::Control); 3]);

        // Refusals of the core, each with its own error name.
        for method in ["Status", "Start", "Stop", "Restart"] {
            for bad in ["", "../x", "Default", "a_b"] {
                let e = call(&client, method, &(bad,)).expect_err(method);
                assert_eq!(e.0, err_name("InvalidId"), "{method} {bad:?}");
            }
        }
        for method in ["Start", "Restart"] {
            let e = call(&client, method, &("lan-2",)).expect_err(method);
            assert_eq!(
                e.0,
                err_name("NoInstance"),
                "{method} of a missing instance"
            );
        }
        let e = call(&client, "Start", &("broken",)).expect_err("Start broken");
        assert_eq!(e.0, err_name("Service"));
        assert!(
            e.1.contains("llama.cpp@broken.service failed"),
            "the service manager's own message: {e:?}"
        );

        // Files.
        for bad in [
            "../x",
            "server.ini",
            "settings.ini",
            "/etc/passwd",
            "server@Bad.ini",
        ] {
            let e = call(&client, "ReadFile", &(bad,)).expect_err("ReadFile");
            assert_eq!(e.0, err_name("BadFileName"), "ReadFile {bad:?}");
            let e = call(&client, "WriteFile", &(bad, "x")).expect_err("WriteFile");
            assert_eq!(e.0, err_name("BadFileName"), "WriteFile {bad:?}");
        }
        assert!(!dir.path().join("x").exists() && !sys.join("server.ini").exists());
        let r = call(&client, "ReadFile", &("presets.ini.disabled",)).expect("ReadFile");
        assert_eq!(
            body::<(bool, String)>(&r),
            (false, String::new()),
            "a missing file"
        );

        let e = call(
            &client,
            "WriteFile",
            &(
                "server@lan-2.ini",
                "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
            ),
        )
        .expect_err("colliding WriteFile");
        assert_eq!(e.0, err_name("AddressInUse"));
        assert!(e.1.contains("default"), "names the other instance: {e:?}");
        let e = call(
            &client,
            "WriteFile",
            &(
                "server@lan-2.ini",
                "[Server]\nPort = 8081\nHostname = nowhere.invalid\n",
            ),
        )
        .expect_err("unresolvable WriteFile");
        assert_eq!(e.0, err_name("UnresolvableHost"));
        assert!(e.1.contains("nowhere.invalid"), "names the host: {e:?}");
        assert!(
            !sys.join("server@lan-2.ini").exists(),
            "refused writes write nothing"
        );

        let lan2 = "[Server]\nPort = 8081\nHostname = gpu-box.lan\n";
        call(&client, "WriteFile", &("server@lan-2.ini", lan2)).expect("WriteFile lan-2");
        assert_eq!(
            std::fs::read_to_string(sys.join("server@lan-2.ini")).unwrap(),
            lan2
        );
        let r = call(&client, "ListInstances", &()).expect("ListInstances");
        assert_eq!(body::<Vec<String>>(&r), vec!["broken", "default", "lan-2"]);
        let r = call(&client, "ReadFile", &("server@lan-2.ini",)).expect("ReadFile");
        assert_eq!(body::<(bool, String)>(&r), (true, lan2.to_string()));

        // A read that fails for another reason than a missing file.
        std::fs::create_dir(sys.join("presets.ini.disabled")).expect("dir in the way");
        let e = call(&client, "ReadFile", &("presets.ini.disabled",)).expect_err("ReadFile dir");
        assert_eq!(e.0, err_name("Io"));
        std::fs::remove_dir(sys.join("presets.ini.disabled")).unwrap();
    }

    // ── Status names every unit state as systemd does ────────────────────
    for (state, name) in [
        (UnitState::Inactive, "inactive"),
        (UnitState::Failed, "failed"),
        (UnitState::Activating, "activating"),
        (UnitState::Deactivating, "deactivating"),
    ] {
        let c = core_in(state, true, &[]);
        let (_server, client) = connect(&c, None);
        let r = call(&client, "Status", &("default",)).expect("Status");
        assert_eq!(body::<String>(&r), name, "{state:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;

    const PREFIX: &str = "org.privateai.Manager1.Error.";

    #[test]
    fn names_are_the_agreed_ones() {
        assert_eq!(BUS_NAME, "org.privateai.Manager1");
        assert_eq!(OBJECT_PATH, "/org/privateai/Manager1");
        assert_eq!(INTERFACE, "org.privateai.Manager1");
    }

    #[test]
    fn a_call_without_a_sender_is_refused() {
        assert_eq!(caller(None), Err(ManagerError::NotAuthorized));
        assert_eq!(caller(Some("")), Err(ManagerError::NotAuthorized));
    }

    #[test]
    fn the_sender_is_the_caller() {
        assert_eq!(caller(Some(":1.42")), Ok(Caller(":1.42".into())));
        assert_eq!(caller(Some(":1.7")), Ok(Caller(":1.7".into())));
    }

    #[test]
    fn every_error_has_its_own_name_under_the_prefix() {
        let cases = [
            (ManagerError::NotAuthorized, "NotAuthorized"),
            (ManagerError::InvalidId, "InvalidId"),
            (ManagerError::NoInstance, "NoInstance"),
            (ManagerError::BadFileName, "BadFileName"),
            (ManagerError::AddressInUse("default".into()), "AddressInUse"),
            (
                ManagerError::UnresolvableHost("gpu-box.lan".into()),
                "UnresolvableHost",
            ),
            (
                ManagerError::Service(super::super::service::ServiceError(
                    "Unit llama.cpp@default.service not found.".into(),
                )),
                "Service",
            ),
            (ManagerError::Io(ErrorKind::PermissionDenied), "Io"),
        ];
        for (error, kind) in cases {
            let (name, message) = error_reply(&error);
            assert_eq!(name, format!("{PREFIX}{kind}"), "name of {error:?}");
            assert!(!message.trim().is_empty(), "message of {error:?} is empty");
        }
    }

    #[test]
    fn error_messages_carry_the_detail() {
        let (_, m) = error_reply(&ManagerError::AddressInUse("lan-2".into()));
        assert!(m.contains("lan-2"), "the other instance's id: {m:?}");
        let (_, m) = error_reply(&ManagerError::UnresolvableHost("gpu-box.lan".into()));
        assert!(m.contains("gpu-box.lan"), "the host: {m:?}");
        let text = "Unit llama.cpp@default.service not found.";
        let (_, m) = error_reply(&ManagerError::Service(super::super::service::ServiceError(
            text.into(),
        )));
        assert!(m.contains(text), "systemd's own message: {m:?}");
        for kind in [
            ErrorKind::PermissionDenied,
            ErrorKind::IsADirectory,
            ErrorKind::StorageFull,
        ] {
            let (_, m) = error_reply(&ManagerError::Io(kind));
            assert!(m.contains(&kind.to_string()), "io kind {kind:?}: {m:?}");
        }
    }

    #[test]
    fn states_are_systemd_active_state_names() {
        assert_eq!(state_name(UnitState::Active), "active");
        assert_eq!(state_name(UnitState::Inactive), "inactive");
        assert_eq!(state_name(UnitState::Failed), "failed");
        assert_eq!(state_name(UnitState::Activating), "activating");
        assert_eq!(state_name(UnitState::Deactivating), "deactivating");
    }
}
