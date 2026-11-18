//! pai-core's D-Bus client of the manager (`pai_core::client::dbus`) against
//! the real interface (`manager::dbus::ManagerInterface`) over the core's
//! fakes, served peer to peer on a Unix socket pair, never a bus. Every call
//! has a deadline: a stub that panics in the server fails the test, not hangs it.

use crate::manager::dbus::{ManagerInterface, OBJECT_PATH};
use crate::manager::service::fakes::{Call, FakeAuthority, FakeResolver, FakeService};
use crate::manager::service::{plain, withheld, Action, Caller, Manager, UnitState};
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
    let each: [(&str, ClientCall); 11] = [
        ("authenticate", |c| {
            c.authenticate("alice", "s3cret-Pw").err()
        }),
        ("list_instances", |c| c.list_instances().err()),
        ("status", |c| c.status("default").err()),
        ("start", |c| c.start("default").err()),
        ("stop", |c| c.stop("default").err()),
        ("restart", |c| c.restart("default").err()),
        ("read_file", |c| c.read_file("models.ini").err()),
        ("write_file", |c| {
            c.write_file("models.ini", "", false).err()
        }),
        ("write_file_if", |c| {
            c.write_file_if("models.ini", "", "").err()
        }),
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
    let model_profiles = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
    let record = "[default]\nServerProfile = default\n\n[broken]\nServerProfile = broken\n\n[lan-2]\nServerProfile = big\n";
    std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
    std::fs::write(sys.join("server@broken.ini"), broken_ini).expect("write broken");
    std::fs::write(sys.join("models.ini"), model_profiles).expect("write presets");
    std::fs::write(sys.join("instances.ini"), record).expect("write instances");
    let alice = Caller(":1.42".into());

    // ── No sender: every change refused, nobody asked ────────────────────
    {
        let c = core(true, &[]);
        let (_server, client) = connect(&c, None);
        assert_eq!(client.start("default"), Err(ClientError::NotAuthorized));
        assert_eq!(client.stop("default"), Err(ClientError::NotAuthorized));
        assert_eq!(client.restart("default"), Err(ClientError::NotAuthorized));
        assert_eq!(
            client.write_file("models.ini", "[x]\nmodel = /m/x.gguf\n", false),
            Err(ClientError::NotAuthorized)
        );
        // The compare and swap write is the same guarded call: no sender, no
        // write, nobody asked.
        assert_eq!(
            client.write_file_if("models.ini", "[x]\nmodel = /m/x.gguf\n", ""),
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
        // The model profile files, list and status need no caller.
        assert_eq!(
            client.list_instances(),
            Ok(vec![
                "broken".to_string(),
                "default".to_string(),
                "lan-2".to_string()
            ]),
            "the sections of instances.ini are the instances: lan-2 has no profile file of its own"
        );
        assert_eq!(client.status("default"), Ok(InstanceState::Active));
        assert_eq!(
            client.read_file("models.ini"),
            Ok(Some(plain(model_profiles)))
        );
        assert_eq!(
            client.read_file("instances.ini"),
            Ok(Some(plain(record))),
            "the instance record, read with no identity, as assignments.ini"
        );
        // Start at boot: the change needs a caller, the read none.
        assert_eq!(
            client.set_boot_enabled("default", true),
            Err(ClientError::NotAuthorized)
        );
        assert_eq!(client.boot_enabled("default"), Ok(false));
        assert_eq!(asked(&c), vec![], "nobody asked");
    }

    // ── The API key withheld from a caller without configure ────────────
    // The client hands back what the manager said: the key line blanked
    // and `withheld`, or the file as saved for an administrator.
    {
        let keyed = "[Server]\nPort = 8082\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
        std::fs::write(sys.join("server@keyed.ini"), keyed).expect("write keyed");
        for configure in [false, true] {
            let c = Arc::new(
                Manager::new(
                    FakeService::new(),
                    FakeAuthority::new(true).quietly(configure),
                    FakeResolver::lan(),
                )
                .with_group(crate::manager::service::fakes::FakeGroup::new()),
            );
            let (_server, client) = connect(&c, Some(":1.42"));
            let expected = if configure {
                plain(keyed)
            } else {
                withheld("[Server]\nPort = 8082\nHostname = 127.0.0.1\nOpencodeApiKey =\n")
            };
            assert_eq!(
                client.read_file("server@keyed.ini"),
                Ok(Some(expected)),
                "configure: {configure}"
            );
            assert_eq!(
                client.read_file("models.ini"),
                Ok(Some(plain(model_profiles))),
                "a preset file is never withheld"
            );
        }
        std::fs::remove_file(sys.join("server@keyed.ini")).expect("remove keyed");
    }

    // ── A write that keeps the API key the caller never saw ──────────────
    // The client passes `keep_api_key` through: the blanked line saved back
    // takes the manager's key; without it the key is dropped, as before.
    {
        let keyed = "[Server]\nPort = 8082\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
        let blanked = "[Server]\nPort = 8083\nHostname = 127.0.0.1\nOpencodeApiKey =\n";
        for keep in [true, false] {
            std::fs::write(sys.join("server@keyed.ini"), keyed).expect("write keyed");
            let c = core(true, &[]);
            let (_server, client) = connect(&c, Some(":1.42"));
            assert_eq!(
                client.write_file("server@keyed.ini", blanked, keep),
                Ok(()),
                "keep: {keep}"
            );
            let expected = if keep {
                blanked.replace("OpencodeApiKey =\n", "OpencodeApiKey = sk-local-3f9a\n")
            } else {
                blanked.to_string()
            };
            assert_eq!(
                std::fs::read_to_string(sys.join("server@keyed.ini")).expect("read keyed"),
                expected,
                "keep: {keep}"
            );
        }
        std::fs::remove_file(sys.join("server@keyed.ini")).expect("remove keyed");
    }

    // ── A caller polkit refuses ──────────────────────────────────────────
    {
        let c = core(false, &[]);
        let (_server, client) = connect(&c, Some(":1.42"));
        assert_eq!(client.start("default"), Err(ClientError::NotAuthorized));
        assert_eq!(
            client.write_file("models.ini", "", false),
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
            std::fs::read_to_string(sys.join("models.ini")).unwrap(),
            model_profiles,
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
            client.read_file("models.ini"),
            Ok(Some(plain(model_profiles)))
        );
        assert_eq!(
            client.read_file("models.ini.disabled"),
            Ok(None),
            "a missing file"
        );
        // A file whose text is empty is not a missing one.
        std::fs::write(sys.join("models.ini.disabled"), "").expect("empty file");
        assert_eq!(client.read_file("models.ini.disabled"), Ok(Some(plain(""))));
        std::fs::remove_file(sys.join("models.ini.disabled")).unwrap();

        let lan2 = "[Server]\nPort = 8081\nHostname = gpu-box.lan\n";
        assert_eq!(client.write_file("server@lan-2.ini", lan2, false), Ok(()));
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
        assert_eq!(client.read_file("server@lan-2.ini"), Ok(Some(plain(lan2))));

        // The instance record: a client reads it and never writes it.
        let swapped = "[default]\nServerProfile = big\n";
        assert_eq!(
            client.read_file("instances.ini"),
            Ok(Some(plain(record))),
            "an authorized client reads the record"
        );
        assert_eq!(
            client.write_file("instances.ini", swapped, false),
            Err(ClientError::BadFileName),
            "the record is not writable"
        );
        assert_eq!(
            client.write_file_if("instances.ini", swapped, ""),
            Err(ClientError::BadFileName),
            "not through the compare and swap write either"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).expect("read instances"),
            record,
            "a refused write leaves the record"
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
                client.write_file(bad, "x", false),
                Err(ClientError::BadFileName),
                "{bad:?}"
            );
            assert_eq!(
                client.write_file_if(bad, "x", ""),
                Err(ClientError::BadFileName),
                "{bad:?}"
            );
        }
        assert_eq!(
            client.write_file(
                "server@lan-3.ini",
                "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
                false
            ),
            Err(ClientError::AddressInUse("default".into()))
        );
        assert_eq!(
            client.write_file(
                "server@lan-3.ini",
                "[Server]\nPort = 8082\nHostname = nowhere.invalid\n",
                false
            ),
            Err(ClientError::UnresolvableHost("nowhere.invalid".into()))
        );
        assert_eq!(
            client.write_file(
                "server@default.ini",
                "[Server]\nPort = 70000\nHostname = 127.0.0.1\n",
                false
            ),
            Err(ClientError::Invalid(Refusal {
                key: "not-whole-number-in-range",
                values: vec!["Port".into(), "70000".into(), "1".into(), "65535".into()],
            }))
        );
        // A value with spaces and a non-ASCII letter comes back whole.
        assert_eq!(
            client.write_file(
                "models.ini",
                "[Qwen 3 (v2) é]\nmodel = /models/q.gguf\n",
                false
            ),
            Err(ClientError::Invalid(Refusal {
                key: "invalid-model-profile-id",
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
            std::fs::read_to_string(sys.join("models.ini")).unwrap(),
            model_profiles
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
        std::fs::create_dir(sys.join("models.ini.disabled")).expect("dir in the way");
        match client.read_file("models.ini.disabled") {
            Err(ClientError::Io(kind)) => assert!(!kind.is_empty(), "the kind"),
            other => panic!("read of a directory: {other:?}"),
        }
        std::fs::remove_dir(sys.join("models.ini.disabled")).unwrap();
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

    // ── The compare and swap write, end to end ────────────────────────────
    // `write_file_if` hands the content on the wire as the file's own bytes:
    // a caller holding them writes; a caller holding a content the file no
    // longer holds gets `ClientError::Changed` with the file left alone; a
    // file that is not there is `Changed` too.
    {
        let c = core(true, &[]);
        let (_server, client) = connect(&c, Some(":1.42"));
        let base = std::fs::read_to_string(sys.join("models.ini")).expect("read presets");
        let more = format!("{base}[Qwen3.8-14B]\nmodel = /models/Qwen3.8-14B-Q4_K_M.gguf\n");
        assert_eq!(
            client.write_file_if("models.ini", &more, &base),
            Ok(()),
            "a caller holding the file's content writes"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("models.ini")).unwrap(),
            more,
            "the fresh content landed"
        );
        assert_eq!(
            client.write_file_if("models.ini", &base, &base),
            Err(ClientError::Changed),
            "a caller holding a content the file no longer holds"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("models.ini")).unwrap(),
            more,
            "the refused write changed nothing on the way to the error"
        );
        assert_eq!(
            client.write_file_if(
                "server@nope.ini",
                "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
                ""
            ),
            Err(ClientError::Changed),
            "no file, no match"
        );
        assert_eq!(
            asked(&c),
            vec![(alice.clone(), Action::Configure); 3],
            "the guarded call the plain write is, at every call"
        );
        assert_eq!(calls(&c), vec![], "a file write touches no unit");
        std::fs::write(sys.join("models.ini"), model_profiles).expect("restore presets");
    }
}

/// A core taking `bff` for the bff's account over the fake accounts (alice
/// an administrator, bob not), and its checker's log.
fn auth_core(
    bff: &str,
    passwords: crate::manager::service::fakes::FakePasswords,
) -> (Arc<Core>, crate::manager::service::fakes::Checked) {
    let checked = passwords.checked.clone();
    let core = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true).bff(&Caller(bff.into())),
        FakeResolver::lan(),
    )
    .with_group(crate::manager::service::fakes::FakeGroup::new())
    .with_passwords(passwords);
    (Arc::new(core), checked)
}

/// pai-core's `authenticate` against the interface, as the bff: (ok,
/// can_configure) for each outcome; no file is read, so a plain test.
#[test]
fn the_dbus_client_authenticates_as_the_bff() {
    use crate::manager::service::fakes::FakePasswords;
    let (c, checked) = auth_core(":1.9", FakePasswords::new());
    let (_server, client) = connect(&c, Some(":1.9"));
    let client = Arc::new(client);
    for (user, password, expected) in [
        ("alice", "s3cret-Pw", (true, true)),
        ("bob", "hunter2!", (true, false)),
        ("alice", "wrong", (false, false)),
        ("mallory", "s3cret-Pw", (false, false)),
        ("", "", (false, false)),
    ] {
        let client = client.clone();
        let got = bounded("authenticate", move || client.authenticate(user, password));
        assert_eq!(got, Ok(expected), "{user:?} / {password:?}");
    }
    assert_eq!(checked.lock().unwrap().len(), 4);
}

/// Another sender is NotAuthorized, nothing checked; a failing checker is
/// Service with its message, never the password.
#[test]
fn the_dbus_client_reads_authenticate_refusals() {
    use crate::manager::service::fakes::FakePasswords;
    let (c, checked) = auth_core(":1.9", FakePasswords::new());
    let (_server, client) = connect(&c, Some(":1.42"));
    let got = bounded("authenticate", move || {
        client.authenticate("alice", "s3cret-Pw")
    });
    assert_eq!(got, Err(ClientError::NotAuthorized));
    assert_eq!(*checked.lock().unwrap(), vec![]);

    let failure = "pam_authenticate: Authentication service cannot retrieve authentication info";
    let (c, _) = auth_core(":1.9", FakePasswords::failing(failure));
    let (_server, client) = connect(&c, Some(":1.9"));
    let got = bounded("authenticate", move || {
        client.authenticate("alice", "s3cret-Pw")
    });
    assert_eq!(got, Err(ClientError::Service(failure.into())));
}

// ── Acting for a signed-in user: `acting_for` and `version` ──────────────

/// A client method call, on its own thread, within the deadline: the stub
/// that panics fails the test, it does not hang.
fn on<T: Send + 'static>(
    what: &str,
    client: &Arc<DbusClient>,
    f: impl FnOnce(&DbusClient) -> T + Send + 'static,
) -> T {
    let client = client.clone();
    bounded(what, move || f(&client))
}

/// The manager's interface as a manager of an older protocol has it: the
/// methods it knows, no `Version`. Served at the manager's path and name.
struct NoVersion;

#[zbus::interface(name = "org.privateai.Manager1")]
impl NoVersion {
    async fn list_instances(&self) -> Vec<String> {
        vec!["default".to_string()]
    }
}

/// [`NoVersion`] served on a socket pair (never a bus), pai-core's client on
/// the other end, stamping `sender` as the bus would.
fn serve_no_version(sender: Option<&str>) -> (Connection, DbusClient) {
    let (s0, s1) = UnixStream::pair().expect("socket pair");
    let server = std::thread::spawn(move || {
        Builder::async_io_unix_stream(s0)
            .server(zbus::Guid::generate())
            .expect("server guid")
            .p2p()
            .serve_at(OBJECT_PATH, NoVersion)
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

/// `version` reads the `(s version, u protocol)` reply: the framework version
/// the manager answers with and the protocol beside it. No file is read, so a
/// plain test.
#[test]
fn the_dbus_client_reads_the_managers_version() {
    use pai_core::client::ManagerVersion;
    let c = core(true, &[]);
    let (_server, client) = connect(&c, Some(":1.42"));
    let client = Arc::new(client);
    let got = bounded("version", move || client.version());
    assert_eq!(
        got,
        Ok(Some(ManagerVersion {
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol: 2,
        })),
        "the version this manager answers"
    );
}

/// A manager with no `Version` method: that is no version (None), not a
/// failure. The peer is checked to answer its own method first, so the None
/// cannot be read as no manager at all.
#[test]
fn the_dbus_client_has_no_version_against_an_object_without_the_method() {
    let (_server, client) = serve_no_version(Some(":1.42"));
    let client = Arc::new(client);
    let peer = client.clone();
    assert_eq!(
        bounded("list_instances", move || peer.list_instances()),
        Ok(vec!["default".to_string()]),
        "the peer answers the methods it has"
    );
    let got = bounded("version", move || client.version());
    assert_eq!(got, Ok(None), "a manager older than the method");
}

/// E2E phase for pai-core's D-Bus client acting for a signed-in user:
/// [`DbusClient::acting_for`], the bff's account vouching for a `pai` member,
/// against the real interface over a socket pair (never a bus), over the files
/// a guarded call touches: it sets the process-wide `PRIVATE_AI_DATA_ROOT`
/// redirect (`src/tests/mod.rs`).
pub(crate) fn manager_dbus_client_principal_e2e() {
    use crate::manager::service::fakes::{FakeGroup, FakePasswords};
    use std::sync::Mutex;

    /// The bff's bus name in this phase.
    const BFF: &str = ":1.9";
    /// Another account's bus name: a caller that is not the bff's.
    const OTHER: &str = ":1.42";

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\nOpencodeApiKey = sk-local-3f9a\n";
    let model_profiles = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
    let more = format!("{model_profiles}[Qwen3.8-14B]\nmodel = /models/Qwen3.8-14B-Q4_K_M.gguf\n");
    std::fs::write(sys.join("server@default.ini"), ini).expect("write default");
    std::fs::write(sys.join("models.ini"), model_profiles).expect("write presets");
    // The instance is the section of the record: the fixture writes it.
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = default\n",
    )
    .expect("write instances");
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");

    // polkit refuses this caller everything, and would challenge it for
    // `configure`: the right of a call for a member is the pair (the bff's
    // account, a current member), so a call that ran as the bff's own is
    // refused here.
    let passwords = FakePasswords::new();
    let admins = passwords.asked_admin.clone();
    let c = Arc::new(
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(false)
                .quietly(false)
                .bff(&Caller(BFF.into())),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_passwords(passwords),
    );
    let polkit_quiet = |c: &Arc<Core>| c.authority().asked_quietly.lock().unwrap().clone();
    let bff_checks = |c: &Arc<Core>| c.authority().asked_bff.lock().unwrap().clone();
    let asked_admin = |a: &Arc<Mutex<Vec<String>>>| a.lock().unwrap().clone();
    let bff_asks = |n: usize| vec![Caller(BFF.into()); n];
    let other_asks = |n: usize| vec![Caller(OTHER.into()); n];
    let member_asks = |n: usize| vec!["alice".to_string(); n];

    let (_server, client) = connect(&c, Some(BFF));
    let client = Arc::new(client);

    // ── A client bound to a member: the guarded calls carry the principal ──
    {
        let bound = Arc::new(client.acting_for("alice"));
        assert_eq!(
            on("start", &bound, |c| c.start("default")),
            Ok(()),
            "a member starts the instance"
        );
        assert_eq!(
            on("stop", &bound, |c| c.stop("default")),
            Ok(()),
            "a member stops it"
        );
        assert_eq!(
            on("restart", &bound, |c| c.restart("default")),
            Ok(()),
            "a member restarts it"
        );
        assert_eq!(
            on("set_boot_enabled", &bound, |c| c
                .set_boot_enabled("default", true)),
            Ok(()),
            "a member switches the start at boot on"
        );
        let text = more.clone();
        assert_eq!(
            on("write_file", &bound, move |c| c.write_file(
                "models.ini",
                &text,
                false
            )),
            Ok(()),
            "a member writes the presets"
        );
        assert_eq!(text_of("models.ini"), more, "the member's write landed");
        assert_eq!(
            on("read_file", &bound, |c| c.read_file("server@default.ini")),
            Ok(Some(plain(ini))),
            "a member's read of an instance file keeps the key"
        );
        // The unguarded methods are the caller's own: `BootEnabled`, `Status`
        // and `ListInstances` have no `As` form.
        assert_eq!(
            on("boot_enabled", &bound, |c| c.boot_enabled("default")),
            Ok(true),
            "the start at boot reads back on"
        );
        assert_eq!(
            on("status", &bound, |c| c.status("default")),
            Ok(InstanceState::Active),
            "the state of a bound client"
        );
        assert_eq!(
            on("list_instances", &bound, |c| c.list_instances()),
            Ok(vec!["default".to_string()]),
            "the instance list of a bound client"
        );
        assert_eq!(
            calls(&c),
            vec![
                Call::Start("default".into()),
                Call::Stop("default".into()),
                Call::Restart("default".into()),
                Call::SetBootEnabled("default".into(), true),
                Call::BootEnabled("default".into()),
                Call::Status("default".into()),
            ],
            "every guarded call reached the service manager, the unguarded status its own probe"
        );
        assert_eq!(
            asked(&c),
            vec![],
            "polkit asked, so a guarded call ran as the bff's own, not for the member"
        );
        assert_eq!(
            polkit_quiet(&c),
            vec![],
            "configure asked to decide the read: the key is kept on the member's right, not the bff's"
        );
        assert_eq!(
            bff_checks(&c),
            bff_asks(6),
            "the caller is checked first, once per guarded call"
        );
        assert_eq!(
            asked_admin(&admins),
            member_asks(6),
            "the membership of the member, at every guarded call"
        );
    }

    // ── The client `acting_for` was made from is unchanged ───────────────
    // Its guarded calls are the caller's own, so polkit decides: refused here,
    // with polkit asked for the caller.
    assert_eq!(
        on("start", &client, |c| c.start("default")),
        Err(ClientError::NotAuthorized),
        "the unbound client's own call, refused by polkit"
    );
    assert_eq!(
        asked(&c),
        vec![(Caller(BFF.into()), Action::Control)],
        "the unbound client sends the plain method: polkit is asked for the caller"
    );

    // ── A bound client whose principal is no member ──────────────────────
    // The membership questions expected, kept across the blocks: 6 for alice
    // (above), 3 for bob, none for a caller that is not the bff.
    let mut expect_admin = member_asks(6);
    {
        let bob = Arc::new(client.acting_for("bob"));
        assert_eq!(
            on("start", &bob, |c| c.start("default")),
            Err(ClientError::NotAuthorized),
            "a non-member starts nothing"
        );
        assert_eq!(
            on("write_file", &bob, |c| c.write_file(
                "models.ini",
                "junk\n",
                false
            )),
            Err(ClientError::NotAuthorized),
            "a non-member writes nothing"
        );
        assert_eq!(
            text_of("models.ini"),
            more,
            "a refused write leaves the file"
        );
        assert_eq!(
            on("read_file", &bob, |c| c.read_file("server@default.ini")),
            Err(ClientError::NotAuthorized),
            "a non-member reads no key"
        );
        assert_eq!(
            calls(&c).len(),
            6,
            "a refused principal reached no service manager"
        );
        expect_admin.extend(["bob", "bob", "bob"].map(String::from));
        assert_eq!(
            asked_admin(&admins),
            expect_admin,
            "the membership of the name each call vouches for"
        );
        assert_eq!(
            bff_checks(&c),
            bff_asks(9),
            "one check of the caller per guarded call"
        );
    }

    // ── A caller that is not the bff, bound to a member ──────────────────
    {
        let (_server, other) = connect(&c, Some(OTHER));
        let bound = Arc::new(other.acting_for("alice"));
        assert_eq!(
            on("start", &bound, |c| c.start("default")),
            Err(ClientError::NotAuthorized),
            "start from a caller that is not the bff"
        );
        assert_eq!(
            on("stop", &bound, |c| c.stop("default")),
            Err(ClientError::NotAuthorized),
            "stop from a caller that is not the bff"
        );
        assert_eq!(
            on("restart", &bound, |c| c.restart("default")),
            Err(ClientError::NotAuthorized),
            "restart from a caller that is not the bff"
        );
        assert_eq!(
            on("set_boot_enabled", &bound, |c| {
                c.set_boot_enabled("default", false)
            }),
            Err(ClientError::NotAuthorized),
            "the start at boot from a caller that is not the bff"
        );
        assert_eq!(
            on("write_file", &bound, |c| {
                c.write_file("models.ini", "junk\n", false)
            }),
            Err(ClientError::NotAuthorized),
            "a write from a caller that is not the bff"
        );
        assert_eq!(
            on("read_file", &bound, |c| c.read_file("server@default.ini")),
            Err(ClientError::NotAuthorized),
            "a read from a caller that is not the bff"
        );
        assert_eq!(
            on("status", &bound, |c| c.status("default")),
            Ok(InstanceState::Active),
            "an unguarded method of a bound client of another caller still answers"
        );
        assert_eq!(
            calls(&c).len(),
            7,
            "a caller that is not the bff started nothing (only the status probe)"
        );
        assert_eq!(
            on("boot_enabled", &client, |c| c.boot_enabled("default")),
            Ok(true),
            "the refused change left the start at boot as the member set it"
        );
        assert_eq!(text_of("models.ini"), more, "the file kept");
        assert_eq!(
            asked(&c),
            vec![(Caller(BFF.into()), Action::Control)],
            "the refusals are the principal gate's: no polkit question for the other caller"
        );
        assert_eq!(
            polkit_quiet(&c),
            vec![],
            "a refused read asks nothing to decide it"
        );
        assert_eq!(
            asked_admin(&admins),
            expect_admin,
            "no membership looked up for a caller that is not the bff"
        );
        // The principal gate asks `is_bff` of the caller of the call, so the
        // log carries the bff for the principal calls above and this caller
        // for its own: a caller that is not the bff is checked as itself.
        let mut expect_bff = bff_asks(9);
        expect_bff.extend(other_asks(6));
        assert_eq!(
            bff_checks(&c),
            expect_bff,
            "the caller of every guarded call is checked, and the other caller as itself"
        );
    }

    // ── The member's compare and swap write, through the client ──────────
    // The bff's client bound to `alice`: the member's `write_file_if` writes
    // while the file holds the content the caller sends, and is
    // `ClientError::Changed` when it no longer does, leaving the file. The
    // counts are relative: this phase's logs already carry every call above.
    {
        let admins_before = asked_admin(&admins).len();
        let bff_before = bff_checks(&c).len();
        let calls_before = calls(&c).len();
        let base = text_of("models.ini");
        let next = format!("{base}[Qwen3.5-9B]\nmodel = /models/Qwen3.5-9B-Q4_K_M.gguf\n");
        let (land, land_of) = (next.clone(), base.clone());
        assert_eq!(
            on("write_file_if", &client, move |c| c
                .acting_for("alice")
                .write_file_if("models.ini", &land, &land_of)),
            Ok(()),
            "a member's compare and swap write, holding the file's content"
        );
        assert_eq!(
            text_of("models.ini"),
            next,
            "the member's fresh content landed"
        );
        let (back, back_of) = (base.clone(), base.clone());
        assert_eq!(
            on("write_file_if", &client, move |c| c
                .acting_for("alice")
                .write_file_if("models.ini", &back, &back_of)),
            Err(ClientError::Changed),
            "the same call with a content the file no longer holds"
        );
        assert_eq!(
            text_of("models.ini"),
            next,
            "the refused write changed nothing on the way to the error"
        );
        assert_eq!(
            calls(&c).len(),
            calls_before,
            "a file write touches no unit"
        );
        assert_eq!(
            asked_admin(&admins).len(),
            admins_before + 2,
            "the membership of the vouched name, asked once per write"
        );
        assert_eq!(
            bff_checks(&c).len(),
            bff_before + 2,
            "the caller checked once per principal call"
        );
    }
}
