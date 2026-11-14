//! The D-Bus face of `pai-manager` on Linux: interface
//! `org.privateai.Manager1` at `/org/privateai/Manager1` on the system bus,
//! over the core [`Manager`]. The caller of a method is its sender (the
//! unique bus name the bus stamps), handed to the core as [`Caller`]; every
//! [`ManagerError`] replies as a D-Bus error `org.privateai.Manager1.Error.<Kind>`.

pub use super::service::state_name;
use super::service::{
    Action, Authority, Caller, Manager, ManagerError, Resolver, ServiceControl, ServiceError,
    SystemResolver, UnitState,
};
use std::sync::Arc;
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
    /// The message is `Refusal::encode`'s string: the client decodes it.
    Invalid(String),
}

/// The message is `service::error_reply`'s, the same on every transport.
impl From<ManagerError> for Error {
    fn from(e: ManagerError) -> Self {
        let (_, message) = super::service::error_reply(&e);
        match e {
            ManagerError::NotAuthorized => Error::NotAuthorized(message),
            ManagerError::InvalidId => Error::InvalidId(message),
            ManagerError::NoInstance => Error::NoInstance(message),
            ManagerError::BadFileName => Error::BadFileName(message),
            ManagerError::AddressInUse(_) => Error::AddressInUse(message),
            ManagerError::UnresolvableHost(_) => Error::UnresolvableHost(message),
            ManagerError::Service(_) => Error::Service(message),
            ManagerError::Io(_) => Error::Io(message),
            ManagerError::Invalid(_) => Error::Invalid(message),
        }
    }
}

/// The served object: the core, shared with whoever else holds it. zbus
/// runs every method of a connection on that connection's one executor
/// thread, so each method runs its core call on a thread of the `blocking`
/// pool: a caller polkit is still asking about holds up no other call.
pub struct ManagerInterface<S: ServiceControl, A: Authority, R: Resolver> {
    manager: Arc<Manager<S, A, R>>,
}

impl<S: ServiceControl, A: Authority, R: Resolver> ManagerInterface<S, A, R> {
    pub fn new(manager: Arc<Manager<S, A, R>>) -> Self {
        Self { manager }
    }
}

impl<S, A, R> ManagerInterface<S, A, R>
where
    S: ServiceControl + Send + Sync + 'static,
    A: Authority + Send + Sync + 'static,
    R: Resolver + Send + Sync + 'static,
{
    /// `f` over the core, off the connection's executor thread.
    async fn core<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Manager<S, A, R>) -> T + Send + 'static,
    ) -> T {
        let manager = self.manager.clone();
        blocking::unblock(move || f(&manager)).await
    }
}

/// The caller of a call from its header.
fn sender_of(header: &zbus::message::Header<'_>) -> Result<Caller, ManagerError> {
    caller(header.sender().map(|s| s.as_str()))
}

#[zbus::interface(name = "org.privateai.Manager1")]
impl<S, A, R> ManagerInterface<S, A, R>
where
    S: ServiceControl + Send + Sync + 'static,
    A: Authority + Send + Sync + 'static,
    R: Resolver + Send + Sync + 'static,
{
    async fn list_instances(&self) -> Vec<String> {
        self.core(|m| m.list_instances()).await
    }

    async fn status(&self, id: String) -> Result<String, Error> {
        let state = self.core(move |m| m.status(&id)).await?;
        Ok(state_name(state).to_string())
    }

    async fn start(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: String,
    ) -> Result<(), Error> {
        let caller = sender_of(&header)?;
        Ok(self.core(move |m| m.start(&caller, &id)).await?)
    }

    async fn stop(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: String,
    ) -> Result<(), Error> {
        let caller = sender_of(&header)?;
        Ok(self.core(move |m| m.stop(&caller, &id)).await?)
    }

    async fn restart(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: String,
    ) -> Result<(), Error> {
        let caller = sender_of(&header)?;
        Ok(self.core(move |m| m.restart(&caller, &id)).await?)
    }

    async fn boot_enabled(&self, id: String) -> Result<bool, Error> {
        Ok(self.core(move |m| m.boot_enabled(&id)).await?)
    }

    async fn set_boot_enabled(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: String,
        enabled: bool,
    ) -> Result<(), Error> {
        let caller = sender_of(&header)?;
        Ok(self
            .core(move |m| m.set_boot_enabled(&caller, &id, enabled))
            .await?)
    }

    async fn read_file(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        name: String,
    ) -> Result<(bool, String), Error> {
        let caller = sender_of(&header).ok();
        Ok(
            match self
                .core(move |m| m.read_file(caller.as_ref(), &name))
                .await?
            {
                Some(text) => (true, text),
                None => (false, String::new()),
            },
        )
    }

    async fn write_file(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        name: String,
        contents: String,
    ) -> Result<(), Error> {
        let caller = sender_of(&header)?;
        Ok(self
            .core(move |m| m.write_file(&caller, &name, &contents))
            .await?)
    }
}

/// [`ServiceControl`] over systemd's D-Bus API (`org.freedesktop.systemd1.Manager`
/// `StartUnit` / `StopUnit` / `RestartUnit`, mode "replace", on
/// `llama.cpp@<id>.service`; the state is the unit's `ActiveState`, a unit
/// not loaded reading as inactive; the start at boot is its unit file state
/// "enabled", changed by `EnableUnitFiles` / `DisableUnitFiles` then
/// `Reload`, as `systemctl enable/disable` does). The start, stop and state
/// are checked in a real run; the start at boot against a stand-in too.
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

    fn boot_enabled(&self, id: &str) -> Result<bool, ServiceError> {
        let unit = format!("llama.cpp@{id}.service");
        let state: String = self
            .manager()?
            .call("GetUnitFileState", &(unit.as_str(),))
            .map_err(service_error)?;
        Ok(state == "enabled")
    }

    fn set_boot_enabled(&self, id: &str, enabled: bool) -> Result<(), ServiceError> {
        let unit = format!("llama.cpp@{id}.service");
        let units: &[&str] = &[unit.as_str()];
        let manager = self.manager()?;
        if enabled {
            manager
                .call::<_, _, (bool, Vec<(String, String, String)>)>(
                    "EnableUnitFiles",
                    &(units, false, false),
                )
                .map(|_changes| ())
        } else {
            manager
                .call::<_, _, Vec<(String, String, String)>>("DisableUnitFiles", &(units, false))
                .map(|_changes| ())
        }
        .map_err(service_error)?;
        manager
            .call::<_, _, ()>("Reload", &())
            .map_err(service_error)
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
/// served at [`OBJECT_PATH`] under [`BUS_NAME`]; it starts no instance:
/// systemd starts the enabled ones at boot. The connection to keep alive
/// while serving.
pub fn serve() -> zbus::Result<Connection> {
    let client = Connection::system()?;
    let core = Arc::new(Manager::new(
        SystemdService::new(client.clone()),
        PolkitAuthority::new(client),
        SystemResolver,
    ));
    zbus::blocking::connection::Builder::system()?
        .serve_at(OBJECT_PATH, ManagerInterface::new(core))?
        .name(BUS_NAME)?
        .build()
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
    let connect = |core: &Arc<Core>, sender: Option<&str>| -> (Connection, Connection) {
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
    let core_in = |state: UnitState, authority: bool, failing: &[&str]| -> Arc<Core> {
        let mut service = FakeService::new();
        service.state = state;
        service.failing = failing.iter().map(|s| s.to_string()).collect();
        Arc::new(
            Manager::new(service, FakeAuthority::new(authority), FakeResolver::lan())
                .with_group(crate::manager::service::fakes::FakeGroup::new()),
        )
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
    let calls = |c: &Arc<Core>| c.service().calls.lock().unwrap().clone();
    let asked = |c: &Arc<Core>| c.authority().asked.lock().unwrap().clone();
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

        // Start at boot: the read needs no caller, the change one.
        let e = call(&client, "SetBootEnabled", &("default", true)).expect_err("SetBootEnabled");
        assert_eq!(
            e.0,
            err_name("NotAuthorized"),
            "SetBootEnabled without a sender"
        );
        let r = call(&client, "BootEnabled", &("default",)).expect("BootEnabled");
        assert!(!body::<bool>(&r), "a new instance starts disabled");
        assert_eq!(
            calls(&c),
            vec![
                Call::Status("default".into()),
                Call::BootEnabled("default".into())
            ]
        );
        assert_eq!(asked(&c), vec![], "a call with no sender asks nobody");
    }

    // ── A caller polkit refuses: asked with its bus name, nothing done ───
    {
        let c = core(false, &[]);
        let (_server, client) = connect(&c, Some(":1.42"));
        let e = call(&client, "Start", &("default",)).expect_err("Start");
        assert_eq!(e.0, err_name("NotAuthorized"));
        let e = call(&client, "WriteFile", &("presets.ini", "")).expect_err("WriteFile");
        assert_eq!(e.0, err_name("NotAuthorized"));
        let e = call(&client, "SetBootEnabled", &("default", true)).expect_err("SetBootEnabled");
        assert_eq!(e.0, err_name("NotAuthorized"));
        assert_eq!(calls(&c), vec![], "a refused caller starts nothing");
        assert_eq!(
            asked(&c),
            vec![
                (alice.clone(), Action::Control),
                (alice.clone(), Action::Configure),
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

        // Start at boot, changed and read back.
        call(&client, "SetBootEnabled", &("default", true)).expect("SetBootEnabled on");
        let r = call(&client, "BootEnabled", &("default",)).expect("BootEnabled");
        assert!(body::<bool>(&r), "enabled");
        call(&client, "SetBootEnabled", &("default", false)).expect("SetBootEnabled off");
        let r = call(&client, "BootEnabled", &("default",)).expect("BootEnabled");
        assert!(!body::<bool>(&r), "disabled again");
        assert_eq!(
            calls(&c)[3..],
            [
                Call::SetBootEnabled("default".into(), true),
                Call::BootEnabled("default".into()),
                Call::SetBootEnabled("default".into(), false),
                Call::BootEnabled("default".into()),
            ]
        );
        assert_eq!(
            asked(&c)[3..],
            [
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure)
            ],
            "only the changes ask, for configure"
        );
        for method in ["BootEnabled", "SetBootEnabled"] {
            let e = if method == "BootEnabled" {
                call(&client, method, &("lan-2",))
            } else {
                call(&client, method, &("lan-2", true))
            }
            .expect_err(method);
            assert_eq!(
                e.0,
                err_name("NoInstance"),
                "{method} of a missing instance"
            );
            let e = if method == "BootEnabled" {
                call(&client, method, &("../x",))
            } else {
                call(&client, method, &("../x", true))
            }
            .expect_err(method);
            assert_eq!(e.0, err_name("InvalidId"), "{method} of an invalid id");
            let e = if method == "BootEnabled" {
                call(&client, method, &("broken",))
            } else {
                call(&client, method, &("broken", true))
            }
            .expect_err(method);
            assert_eq!(e.0, err_name("Service"), "{method} of a failing unit");
            assert!(e.1.contains("llama.cpp@broken.service failed"), "{e:?}");
        }

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

/// E2E phase for the D-Bus interface while polkit waits for a user to
/// authenticate, for this crate's e2e test to call (it sets the process-wide
/// `PRIVATE_AI_DATA_ROOT` redirect, `src/tests/mod.rs`). On the system bus
/// every caller reaches the service over its one connection; here one p2p
/// connection carries both calls: a `Status` must answer while a `Start` on
/// the same connection waits for polkit, and the `Start` then gets its own
/// reply.
#[cfg(test)]
pub(crate) fn manager_dbus_waits_e2e() {
    use super::service::fakes::{FakeResolver, FakeService, Gate, GateAuthority, GATE_WAIT};
    use super::service::on_thread;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    use zbus::blocking::connection::Builder;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::system_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    let presets = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
    )
    .expect("write default");
    std::fs::write(sys.join("presets.ini"), presets).expect("write presets");

    let alice = Caller(":1.42".into());
    let gate = Gate::new();
    let core = Manager::new(
        FakeService::new(),
        GateAuthority::new(&alice, &gate, &[(&alice, true)]),
        FakeResolver::lan(),
    )
    .with_group(crate::manager::service::fakes::FakeGroup::new());
    let iface = ManagerInterface::new(Arc::new(core));

    let (s0, s1) = UnixStream::pair().expect("socket pair");
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
    // The client's deadline is past the gate's, so a held Start fails on the
    // gate's own timeout, never hangs.
    let client = Builder::async_io_unix_stream(s1)
        .p2p()
        .method_timeout(GATE_WAIT + Duration::from_secs(5))
        .build()
        .expect("client connection");
    client
        .inner()
        .set_unique_name(alice.0.as_str())
        .expect("unique name");
    let _server = server.join().expect("server thread");

    // A call on its own thread, its reply decoded as `T`, or the error.
    fn call<T, B>(
        client: &zbus::blocking::Connection,
        method: &'static str,
        body: B,
    ) -> std::sync::mpsc::Receiver<Result<T, String>>
    where
        T: for<'de> zbus::zvariant::DynamicDeserialize<'de> + Send + 'static,
        B: serde::ser::Serialize + zbus::zvariant::DynamicType + Send + 'static,
    {
        let client = client.clone();
        on_thread(move || {
            client
                .call_method(None::<&str>, OBJECT_PATH, Some(INTERFACE), method, &body)
                .map_err(|e| e.to_string())
                .and_then(|r| r.body().deserialize::<T>().map_err(|e| e.to_string()))
        })
    }
    // The result on `rx`, which must come while the Start is held at the
    // gate. On a timeout the gate is opened first, so nothing is left
    // waiting, then the test fails.
    fn answer<T>(gate: &Gate, what: &str, rx: std::sync::mpsc::Receiver<T>) -> T {
        let deadline = Duration::from_secs(2);
        rx.recv_timeout(deadline).unwrap_or_else(|_| {
            gate.open();
            panic!("{what}: no reply within {deadline:?} while a Start waited for polkit")
        })
    }

    let start = call::<(), _>(&client, "Start", ("default",));
    assert!(gate.arrived(1), "the Start reached polkit");

    let status = answer(
        &gate,
        "Status",
        call::<String, _>(&client, "Status", ("default",)),
    );
    assert_eq!(status, Ok("active".to_string()), "Status replies");
    let read = answer(
        &gate,
        "ReadFile",
        call::<(bool, String), _>(&client, "ReadFile", ("presets.ini",)),
    );
    assert_eq!(read, Ok((true, presets.to_string())), "ReadFile replies");

    assert!(
        start.try_recv().is_err(),
        "the Start is still waiting for polkit"
    );
    gate.open();
    let started = start
        .recv_timeout(GATE_WAIT + Duration::from_secs(5))
        .expect("the Start replies once polkit answers");
    assert!(started.is_ok(), "the Start's own reply: {started:?}");
}

/// E2E phase for a refused write's reply over D-Bus, for this crate's e2e
/// test to call (it sets the process-wide `PRIVATE_AI_DATA_ROOT` redirect,
/// `src/tests/mod.rs`): the client gets `org.privateai.Manager1.Error.Invalid`
/// and reads the refusal back, key and values, with `Refusal::decode`.
#[cfg(test)]
pub(crate) fn manager_dbus_checks_e2e() {
    use super::service::fakes::{FakeAuthority, FakeResolver, FakeService};
    use pai_core::checks::Refusal;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    use zbus::blocking::connection::Builder;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::system_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    let default_ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
    let presets = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
    std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
    std::fs::write(sys.join("presets.ini"), presets).expect("write presets");

    let core = Arc::new(
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(crate::manager::service::fakes::FakeGroup::new()),
    );
    let (s0, s1) = UnixStream::pair().expect("socket pair");
    let iface = ManagerInterface::new(core);
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
    client
        .inner()
        .set_unique_name(":1.42")
        .expect("unique name");
    let _server = server.join().expect("server thread");

    // A WriteFile: Ok, or Err((error name, message)).
    let write = |name: &str, contents: &str| -> Result<(), (String, String)> {
        client
            .call_method(
                None::<&str>,
                OBJECT_PATH,
                Some(INTERFACE),
                "WriteFile",
                &(name, contents),
            )
            .map(|_| ())
            .map_err(|e| match e {
                zbus::Error::MethodError(name, detail, _) => {
                    (name.to_string(), detail.unwrap_or_default())
                }
                other => panic!("WriteFile: not a method error reply: {other:?}"),
            })
    };
    let refusal = |key: &'static str, values: &[&str]| Refusal {
        key,
        values: values.iter().map(|v| v.to_string()).collect(),
    };
    let invalid = "org.privateai.Manager1.Error.Invalid";

    let (name, message) = write(
        "server@default.ini",
        "[Server]\nPort = 70000\nHostname = 127.0.0.1\n",
    )
    .expect_err("an invalid port");
    assert_eq!(name, invalid);
    assert_eq!(
        Refusal::decode(&message),
        Some(refusal(
            "not-whole-number-in-range",
            &["Port", "70000", "1", "65535"]
        )),
        "{message:?}"
    );
    assert_eq!(
        std::fs::read_to_string(sys.join("server@default.ini")).unwrap(),
        default_ini,
        "refused: the file kept"
    );

    // A value with spaces, a backslash and a non-ASCII letter comes back whole.
    let hostile = "[Qwen 3 (v2) é]\nmodel = /models/q.gguf\n";
    let (name, message) = write("presets.ini", hostile).expect_err("an invalid id");
    assert_eq!(name, invalid);
    assert_eq!(
        Refusal::decode(&message),
        Some(refusal("invalid-preset-id", &["Qwen 3 (v2) é"])),
        "{message:?}"
    );
    let (name, message) = write(
        "presets.ini",
        &format!("{presets}[m]\nmodel = /models/m.gguf\noverride-tensor = blk\\.0\\.ffn_up_exps\n"),
    )
    .expect_err("an override with no device");
    assert_eq!(name, invalid);
    assert_eq!(
        Refusal::decode(&message),
        Some(refusal(
            "override-no-device",
            &["1", r"blk\.0\.ffn_up_exps"]
        )),
        "{message:?}"
    );
    assert_eq!(
        std::fs::read_to_string(sys.join("presets.ini")).unwrap(),
        presets,
        "refused: the file kept"
    );

    // A valid write still goes through.
    let more = format!("{presets}[m]\nmodel = /models/m.gguf\nctx-size = 8192\n");
    assert_eq!(write("presets.ini", &more), Ok(()));
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
            (
                ManagerError::Invalid(pai_core::checks::Refusal {
                    key: "comment-markers",
                    values: vec!["ModelsDir".into()],
                }),
                "Invalid",
            ),
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

    /// The client reads the refusal back from the message, key and values.
    #[test]
    fn an_invalid_reply_carries_the_refusal() {
        use pai_core::checks::Refusal;
        for refusal in [
            Refusal {
                key: "load-on-startup-over-max",
                values: vec!["3".into(), "2".into()],
            },
            Refusal {
                key: "moe-cache-with-tensor-split",
                values: vec![],
            },
            Refusal {
                key: "comment-markers",
                values: vec!["OpencodeApiKey".into()],
            },
        ] {
            let (name, message) = error_reply(&ManagerError::Invalid(refusal.clone()));
            assert_eq!(name, format!("{PREFIX}Invalid"));
            assert_eq!(Refusal::decode(&message), Some(refusal), "{message:?}");
        }
    }

    // ── SystemdService against a stand-in for systemd ───────────────────

    /// What the stand-in for systemd was asked, one line per call.
    type Log = Arc<std::sync::Mutex<Vec<String>>>;

    /// systemd's list of changes: (type, symlink, destination).
    type Changes = Vec<(String, String, String)>;

    /// `org.freedesktop.systemd1.Manager` as far as the start at boot needs
    /// it (org.freedesktop.systemd1(5)): `GetUnitFileState` answers `state`,
    /// `EnableUnitFiles` and `DisableUnitFiles` record their arguments (or
    /// fail with `refuse`, as systemd refuses a caller without the right),
    /// `Reload` is recorded.
    struct FakeSystemd {
        log: Log,
        state: String,
        refuse: Option<String>,
    }

    #[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
    impl FakeSystemd {
        fn get_unit_file_state(&self, file: String) -> String {
            self.log
                .lock()
                .unwrap()
                .push(format!("GetUnitFileState {file}"));
            self.state.clone()
        }

        fn enable_unit_files(
            &self,
            files: Vec<String>,
            runtime: bool,
            force: bool,
        ) -> zbus::fdo::Result<(bool, Changes)> {
            self.log.lock().unwrap().push(format!(
                "EnableUnitFiles {files:?} runtime={runtime} force={force}"
            ));
            if let Some(m) = &self.refuse {
                return Err(zbus::fdo::Error::AccessDenied(m.clone()));
            }
            let link = format!("/etc/systemd/system/multi-user.target.wants/{}", files[0]);
            let target = "/usr/lib/systemd/system/llama.cpp@.service".to_string();
            Ok((true, vec![("symlink".into(), link, target)]))
        }

        fn disable_unit_files(
            &self,
            files: Vec<String>,
            runtime: bool,
        ) -> zbus::fdo::Result<Changes> {
            self.log
                .lock()
                .unwrap()
                .push(format!("DisableUnitFiles {files:?} runtime={runtime}"));
            if let Some(m) = &self.refuse {
                return Err(zbus::fdo::Error::AccessDenied(m.clone()));
            }
            let link = format!("/etc/systemd/system/multi-user.target.wants/{}", files[0]);
            Ok(vec![("unlink".into(), link, String::new())])
        }

        fn reload(&self) {
            self.log.lock().unwrap().push("Reload".into());
        }
    }

    /// A [`SystemdService`] over a peer-to-peer connection to a
    /// [`FakeSystemd`] (never the system bus), the server connection to keep
    /// alive, and the log. Every call has a deadline.
    fn systemd(state: &str, refuse: Option<&str>) -> (Connection, SystemdService, Log) {
        use std::os::unix::net::UnixStream;
        use zbus::blocking::connection::Builder;
        let log = Log::default();
        let fake = FakeSystemd {
            log: log.clone(),
            state: state.to_string(),
            refuse: refuse.map(str::to_string),
        };
        let (s0, s1) = UnixStream::pair().expect("socket pair");
        let server = std::thread::spawn(move || {
            Builder::async_io_unix_stream(s0)
                .server(zbus::Guid::generate())
                .expect("server guid")
                .p2p()
                .serve_at("/org/freedesktop/systemd1", fake)
                .expect("serve_at")
                .build()
                .expect("server connection")
        });
        let client = Builder::async_io_unix_stream(s1)
            .p2p()
            .method_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("client connection");
        (
            server.join().expect("server thread"),
            SystemdService::new(client),
            log,
        )
    }

    fn logged(log: &Log) -> Vec<String> {
        log.lock().unwrap().clone()
    }

    /// The instance starts at boot when its unit file is enabled, as
    /// `systemctl is-enabled llama.cpp@<id>` reads it (systemctl(1) Table 3):
    /// "enabled" is the `WantedBy=` symlink in /etc. Every other state reads
    /// as not started at boot: "enabled-runtime" is a symlink in /run, gone
    /// before the next boot; "disabled", "masked", "linked" and the rest
    /// start nothing.
    #[test]
    fn the_start_at_boot_is_the_unit_file_state_enabled() {
        for (state, enabled) in [
            ("enabled", true),
            ("disabled", false),
            ("enabled-runtime", false),
            ("masked", false),
            ("masked-runtime", false),
            ("linked", false),
            ("linked-runtime", false),
            ("static", false),
            ("indirect", false),
            ("", false),
        ] {
            let (_server, service, log) = systemd(state, None);
            assert_eq!(service.boot_enabled("lan-2"), Ok(enabled), "{state:?}");
            assert_eq!(
                logged(&log),
                ["GetUnitFileState llama.cpp@lan-2.service"],
                "{state:?}: one question, about the instance's unit"
            );
        }
    }

    /// `systemctl enable llama.cpp@<id>`: `EnableUnitFiles` on the
    /// instance's unit, persistent (runtime false: /etc, not /run) and
    /// without replacing other symlinks (force false), then `Reload`, as
    /// systemctl reloads after an enable unless told `--no-reload`
    /// (systemctl(1)).
    #[test]
    fn enabling_at_boot_enables_the_instances_unit_persistently_and_reloads() {
        let (_server, service, log) = systemd("disabled", None);
        assert_eq!(service.set_boot_enabled("default", true), Ok(()));
        assert_eq!(
            logged(&log),
            [
                r#"EnableUnitFiles ["llama.cpp@default.service"] runtime=false force=false"#,
                "Reload",
            ]
        );
    }

    /// `systemctl disable llama.cpp@<id>`: `DisableUnitFiles` (runtime
    /// false), then `Reload`.
    #[test]
    fn disabling_at_boot_disables_the_instances_unit_and_reloads() {
        let (_server, service, log) = systemd("enabled", None);
        assert_eq!(service.set_boot_enabled("default", false), Ok(()));
        assert_eq!(
            logged(&log),
            [
                r#"DisableUnitFiles ["llama.cpp@default.service"] runtime=false"#,
                "Reload",
            ]
        );
    }

    /// A refusal of systemd comes back as the service manager's failure,
    /// its own message kept, and nothing is reloaded.
    #[test]
    fn a_refused_enable_or_disable_is_systemds_failure_and_reloads_nothing() {
        let denied = "Access denied";
        for on in [true, false] {
            let (_server, service, log) = systemd("disabled", Some(denied));
            match service.set_boot_enabled("default", on) {
                Err(ServiceError(m)) => assert!(m.contains(denied), "{on}: {m:?}"),
                other => panic!("{on}: {other:?}"),
            }
            assert!(
                !logged(&log).iter().any(|l| l == "Reload"),
                "{on}: {:?}",
                logged(&log)
            );
        }
    }

    /// No systemd on the other end: a failure, never a hang or a panic.
    #[test]
    fn no_systemd_is_a_failure() {
        let (server, service, _log) = systemd("enabled", None);
        drop(server);
        assert!(service.boot_enabled("default").is_err());
        assert!(service.set_boot_enabled("default", true).is_err());
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
