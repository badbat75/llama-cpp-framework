//! pai-core's D-Bus client of the manager (`pai_core::client::dbus`) against
//! the real interface (`manager::dbus::ManagerInterface`) over the core's
//! fakes, served peer to peer on a Unix socket pair, never a bus. Every call
//! has a deadline: a stub that panics in the server fails the test, not hangs it.

use crate::manager::dbus::{ManagerInterface, OBJECT_PATH};
use crate::manager::service::fakes::{Call, FakeAuthority, FakeResolver, FakeService};
use crate::manager::service::{Action, Caller, Manager, UnitState};
use pai_core::checks::Refusal;
use pai_core::client::dbus::DbusClient;
use pai_core::client::{ClientError, InstanceState, ManagerClient};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;
use zbus::blocking::connection::Builder;
use zbus::blocking::Connection;

type Core = Manager<FakeService, FakeAuthority, FakeResolver>;
/// One client method, its error when it fails.
type ClientCall = fn(&DbusClient) -> Option<ClientError>;

/// The longest a client call may take.
const DEADLINE: Duration = Duration::from_secs(10);

/// `core` served at `path` on one end of a socket pair (the server built on
/// its own thread: both ends handshake at once), pai-core's client on the
/// other, stamping `sender` as the bus would when given. The server
/// connection is returned to keep it alive.
fn serve_at(
    core: &Arc<Core>,
    path: &'static str,
    sender: Option<&str>,
) -> (Connection, DbusClient) {
    let (s0, s1) = UnixStream::pair().expect("socket pair");
    let iface = ManagerInterface::new(core.clone());
    let server = std::thread::spawn(move || {
        Builder::async_io_unix_stream(s0)
            .server(zbus::Guid::generate())
            .expect("server guid")
            .p2p()
            .serve_at(path, iface)
            .expect("serve_at")
            .build()
            .expect("server connection")
    });
    let client = Builder::async_io_unix_stream(s1)
        .p2p()
        .method_timeout(DEADLINE)
        .build()
        .expect("client connection");
    if let Some(name) = sender {
        client.inner().set_unique_name(name).expect("unique name");
    }
    (
        server.join().expect("server thread"),
        DbusClient::new(client),
    )
}

fn connect(core: &Arc<Core>, sender: Option<&str>) -> (Connection, DbusClient) {
    serve_at(core, OBJECT_PATH, sender)
}

fn core_in(state: UnitState, allow: bool, failing: &[&str]) -> Arc<Core> {
    let mut service = FakeService::new();
    service.state = state;
    service.failing = failing.iter().map(|s| s.to_string()).collect();
    Arc::new(
        Manager::new(service, FakeAuthority::new(allow), FakeResolver::lan())
            .with_group(crate::manager::service::fakes::FakeGroup::new()),
    )
}

fn core(allow: bool, failing: &[&str]) -> Arc<Core> {
    core_in(UnitState::Active, allow, failing)
}

fn calls(c: &Arc<Core>) -> Vec<Call> {
    c.service().calls.lock().unwrap().clone()
}

fn asked(c: &Arc<Core>) -> Vec<(Caller, Action)> {
    c.authority().asked.lock().unwrap().clone()
}

/// `f` on its own thread, its result within [`DEADLINE`] plus a margin: a
/// client that waits forever fails the test.
fn bounded<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let rx = crate::manager::service::on_thread(f);
    use std::sync::mpsc::RecvTimeoutError;
    rx.recv_timeout(DEADLINE + Duration::from_secs(5))
        .unwrap_or_else(|e| match e {
            RecvTimeoutError::Timeout => panic!("{what}: no answer in time"),
            RecvTimeoutError::Disconnected => panic!("{what}: the call panicked"),
        })
}

/// A peer that serves the interface, but not at the manager's path, has no
/// manager to ask: every method is Unavailable and the core is never reached.
#[test]
fn a_peer_without_the_manager_object_is_unavailable() {
    let c = core(true, &[]);
    let (_server, client) = serve_at(&c, "/org/privateai/Elsewhere", Some(":1.42"));
    let client = Arc::new(client);
    let each: [(&str, ClientCall); 9] = [
        ("list_instances", |c| c.list_instances().err()),
        ("status", |c| c.status("default").err()),
        ("start", |c| c.start("default").err()),
        ("stop", |c| c.stop("default").err()),
        ("restart", |c| c.restart("default").err()),
        ("read_file", |c| c.read_file("presets.ini").err()),
        ("write_file", |c| c.write_file("presets.ini", "").err()),
        ("boot_enabled", |c| c.boot_enabled("default").err()),
        ("set_boot_enabled", |c| {
            c.set_boot_enabled("default", true).err()
        }),
    ];
    for (method, f) in each {
        let client = client.clone();
        let e = bounded(method, move || f(&client));
        assert_eq!(e, Some(ClientError::Unavailable), "{method}");
    }
    assert_eq!(calls(&c), vec![], "nothing reached the service manager");
    assert_eq!(asked(&c), vec![], "nobody was asked");
}

/// A manager that went away (its end of the connection closed) is
/// Unavailable, never a hang.
#[test]
fn a_manager_that_went_away_is_unavailable() {
    let c = core(true, &[]);
    let (server, client) = connect(&c, Some(":1.42"));
    drop(server);
    let client = Arc::new(client);
    let each: [(&str, ClientCall); 3] = [
        ("list_instances", |c| c.list_instances().err()),
        ("status", |c| c.status("default").err()),
        ("start", |c| c.start("default").err()),
    ];
    for (method, f) in each {
        let client = client.clone();
        let e = bounded(method, move || f(&client));
        assert_eq!(e, Some(ClientError::Unavailable), "{method}");
    }
}

/// E2E phase for the client against the interface, for this crate's e2e
/// test to call: it sets the process-wide `PRIVATE_AI_DATA_ROOT` redirect
/// (`src/tests/mod.rs`).
pub(crate) fn manager_dbus_client_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    let default_ini = "[Server]\nPort = 8080\nHostname = 0.0.0.0\n";
    let broken_ini = "[Server]\nPort = 8090\nHostname = 127.0.0.1\n";
    let presets = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
    std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
    std::fs::write(sys.join("server@broken.ini"), broken_ini).expect("write broken");
    std::fs::write(sys.join("presets.ini"), presets).expect("write presets");
    let alice = Caller(":1.42".into());

    // ── No sender: every change refused, nobody asked ────────────────────
    {
        let c = core(true, &[]);
        let (_server, client) = connect(&c, None);
        assert_eq!(client.start("default"), Err(ClientError::NotAuthorized));
        assert_eq!(client.stop("default"), Err(ClientError::NotAuthorized));
        assert_eq!(client.restart("default"), Err(ClientError::NotAuthorized));
        assert_eq!(
            client.write_file("presets.ini", "[x]\nmodel = /m/x.gguf\n"),
            Err(ClientError::NotAuthorized)
        );
        // An instance file can hold a secret (OpencodeApiKey): its read
        // needs the control authorization, refused here unasked.
        assert_eq!(
            client.read_file("server@default.ini"),
            Err(ClientError::NotAuthorized),
            "an instance file read with no identity"
        );
        assert_eq!(
            client.read_file("server@nope.ini"),
            Err(ClientError::NotAuthorized),
            "refused before it is known whether the instance exists"
        );
        assert_eq!(calls(&c), vec![]);
        assert_eq!(asked(&c), vec![]);
        // The preset files, list and status need no caller.
        assert_eq!(
            client.list_instances(),
            Ok(vec!["broken".to_string(), "default".to_string()])
        );
        assert_eq!(client.status("default"), Ok(InstanceState::Active));
        assert_eq!(
            client.read_file("presets.ini"),
            Ok(Some(presets.to_string()))
        );
        // Start at boot: the change needs a caller, the read none.
        assert_eq!(
            client.set_boot_enabled("default", true),
            Err(ClientError::NotAuthorized)
        );
        assert_eq!(client.boot_enabled("default"), Ok(false));
        assert_eq!(asked(&c), vec![], "nobody asked");
    }

    // ── A caller polkit refuses ──────────────────────────────────────────
    {
        let c = core(false, &[]);
        let (_server, client) = connect(&c, Some(":1.42"));
        assert_eq!(client.start("default"), Err(ClientError::NotAuthorized));
        assert_eq!(
            client.write_file("presets.ini", ""),
            Err(ClientError::NotAuthorized)
        );
        assert_eq!(
            client.read_file("server@default.ini"),
            Err(ClientError::NotAuthorized),
            "a refused caller reads no instance file"
        );
        assert_eq!(calls(&c), vec![], "a refused caller starts nothing");
        assert_eq!(
            asked(&c),
            vec![
                (alice.clone(), Action::Control),
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Control)
            ]
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("presets.ini")).unwrap(),
            presets,
            "a refused write leaves the file"
        );
        // Start at boot: refused under configure, nothing changed.
        assert_eq!(
            client.set_boot_enabled("default", true),
            Err(ClientError::NotAuthorized)
        );
        assert_eq!(asked(&c)[3..], [(alice.clone(), Action::Configure)]);
        assert_eq!(calls(&c), vec![], "a refused caller enables nothing");
        assert_eq!(client.boot_enabled("default"), Ok(false));
    }

    // ── An authorized caller: every method round-trips ───────────────────
    {
        let c = core(true, &["broken"]);
        let (_server, client) = connect(&c, Some(":1.42"));

        assert_eq!(client.start("default"), Ok(()));
        assert_eq!(client.stop("default"), Ok(()));
        assert_eq!(client.restart("default"), Ok(()));
        assert_eq!(
            calls(&c),
            vec![
                Call::Start("default".into()),
                Call::Stop("default".into()),
                Call::Restart("default".into())
            ]
        );

        assert_eq!(
            client.read_file("presets.ini"),
            Ok(Some(presets.to_string()))
        );
        assert_eq!(
            client.read_file("presets.ini.disabled"),
            Ok(None),
            "a missing file"
        );
        // A file whose text is empty is not a missing one.
        std::fs::write(sys.join("presets.ini.disabled"), "").expect("empty file");
        assert_eq!(
            client.read_file("presets.ini.disabled"),
            Ok(Some(String::new()))
        );
        std::fs::remove_file(sys.join("presets.ini.disabled")).unwrap();

        let lan2 = "[Server]\nPort = 8081\nHostname = gpu-box.lan\n";
        assert_eq!(client.write_file("server@lan-2.ini", lan2), Ok(()));
        assert_eq!(
            std::fs::read_to_string(sys.join("server@lan-2.ini")).unwrap(),
            lan2
        );
        assert_eq!(
            client.list_instances(),
            Ok(vec![
                "broken".to_string(),
                "default".to_string(),
                "lan-2".to_string()
            ])
        );
        assert_eq!(
            client.read_file("server@lan-2.ini"),
            Ok(Some(lan2.to_string()))
        );

        // Every refusal of the manager, as its own error.
        for bad in ["", "../x", "Default", "a_b"] {
            assert_eq!(
                client.status(bad),
                Err(ClientError::InvalidId),
                "status {bad:?}"
            );
            assert_eq!(
                client.start(bad),
                Err(ClientError::InvalidId),
                "start {bad:?}"
            );
        }
        assert_eq!(client.start("lan-3"), Err(ClientError::NoInstance));
        assert_eq!(client.restart("lan-3"), Err(ClientError::NoInstance));
        match client.start("broken") {
            Err(ClientError::Service(m)) => assert!(
                m.contains("llama.cpp@broken.service failed"),
                "the service manager's own message: {m:?}"
            ),
            other => panic!("start broken: {other:?}"),
        }
        for bad in ["../x", "server.ini", "settings.ini", "/etc/passwd"] {
            assert_eq!(
                client.read_file(bad),
                Err(ClientError::BadFileName),
                "{bad:?}"
            );
            assert_eq!(
                client.write_file(bad, "x"),
                Err(ClientError::BadFileName),
                "{bad:?}"
            );
        }
        assert_eq!(
            client.write_file(
                "server@lan-3.ini",
                "[Server]\nPort = 8080\nHostname = 127.0.0.1\n"
            ),
            Err(ClientError::AddressInUse("default".into()))
        );
        assert_eq!(
            client.write_file(
                "server@lan-3.ini",
                "[Server]\nPort = 8082\nHostname = nowhere.invalid\n"
            ),
            Err(ClientError::UnresolvableHost("nowhere.invalid".into()))
        );
        assert_eq!(
            client.write_file(
                "server@default.ini",
                "[Server]\nPort = 70000\nHostname = 127.0.0.1\n"
            ),
            Err(ClientError::Invalid(Refusal {
                key: "not-whole-number-in-range",
                values: vec!["Port".into(), "70000".into(), "1".into(), "65535".into()],
            }))
        );
        // A value with spaces and a non-ASCII letter comes back whole.
        assert_eq!(
            client.write_file("presets.ini", "[Qwen 3 (v2) é]\nmodel = /models/q.gguf\n"),
            Err(ClientError::Invalid(Refusal {
                key: "invalid-preset-id",
                values: vec!["Qwen 3 (v2) é".into()],
            }))
        );
        assert!(
            !sys.join("server@lan-3.ini").exists(),
            "refused writes write nothing"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).unwrap(),
            default_ini
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("presets.ini")).unwrap(),
            presets
        );

        // Start at boot, both ways, and its refusals.
        assert_eq!(client.boot_enabled("default"), Ok(false));
        assert_eq!(client.set_boot_enabled("default", true), Ok(()));
        assert_eq!(client.boot_enabled("default"), Ok(true));
        assert_eq!(client.set_boot_enabled("default", false), Ok(()));
        assert_eq!(client.boot_enabled("default"), Ok(false));
        assert_eq!(client.boot_enabled("lan-3"), Err(ClientError::NoInstance));
        assert_eq!(
            client.set_boot_enabled("lan-3", true),
            Err(ClientError::NoInstance)
        );
        assert_eq!(client.boot_enabled("../x"), Err(ClientError::InvalidId));
        assert_eq!(
            client.set_boot_enabled("Default", true),
            Err(ClientError::InvalidId)
        );
        match client.set_boot_enabled("broken", true) {
            Err(ClientError::Service(m)) => assert!(
                m.contains("llama.cpp@broken.service failed"),
                "the service manager's own message: {m:?}"
            ),
            other => panic!("set_boot_enabled broken: {other:?}"),
        }

        // A read that fails for another reason than a missing file.
        std::fs::create_dir(sys.join("presets.ini.disabled")).expect("dir in the way");
        match client.read_file("presets.ini.disabled") {
            Err(ClientError::Io(kind)) => assert!(!kind.is_empty(), "the kind"),
            other => panic!("read of a directory: {other:?}"),
        }
        std::fs::remove_dir(sys.join("presets.ini.disabled")).unwrap();
    }

    // ── Every state the manager reports ──────────────────────────────────
    for (state, expected) in [
        (UnitState::Inactive, InstanceState::Inactive),
        (UnitState::Failed, InstanceState::Failed),
        (UnitState::Activating, InstanceState::Activating),
        (UnitState::Deactivating, InstanceState::Deactivating),
    ] {
        let c = core_in(state, true, &[]);
        let (_server, client) = connect(&c, None);
        assert_eq!(client.status("default"), Ok(expected), "{state:?}");
    }
}
