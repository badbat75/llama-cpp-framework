//! pai-core's D-Bus client of the manager (`pai_core::client::dbus`) against
//! the real interface (`manager::dbus::ManagerInterface`) over the core's
//! fakes, served peer to peer on a Unix socket pair, never a bus. Every call
//! has a deadline: a stub that panics in the server fails the test, not hangs it.

use crate::manager::dbus::{ManagerInterface, OBJECT_PATH};
use crate::manager::service::fakes::{Call, FakeAuthority, FakeResolver, FakeService};
use crate::manager::service::{
    plain, real_model_files, withheld, Action, Caller, Manager, UnitState,
};
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
    let each: [(&str, ClientCall); 16] = [
        ("authenticate", |c| {
            c.authenticate("alice", "s3cret-Pw").err()
        }),
        ("list_instances", |c| c.list_instances().err()),
        ("instances", |c| c.instances().err()),
        ("set_instance_tags", |c| {
            c.set_instance_tags("default", &["edge".into()]).err()
        }),
        ("set_instance_label", |c| {
            c.set_instance_label("default", "Coding box").err()
        }),
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
        ("set_model_profile", |c| {
            c.set_model_profile("default", "Qwen3.8-27B", "on").err()
        }),
        ("set_server_profile", |c| {
            c.set_server_profile("default", "default").err()
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
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    );
    let record = "[default]\nServerProfile = default\n\n[broken]\nServerProfile = broken\n\n[lan-2]\nServerProfile = big\n";
    std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
    std::fs::write(sys.join("server@broken.ini"), broken_ini).expect("write broken");
    std::fs::write(sys.join("models.ini"), &model_profiles).expect("write presets");
    std::fs::write(sys.join("instances.ini"), record).expect("write instances");
    // A start loads the instance's render and refuses one with nothing to run.
    // The render is the manager's, from the pool and the assignment, so the
    // fixture assigns the profile and writes the render it renders.
    std::fs::write(
        sys.join("assignments.ini"),
        "[default]\nQwen3.8-27B = on\n\n[broken]\nQwen3.8-27B = on\n",
    )
    .expect("write assignments");
    for id in ["default", "broken"] {
        std::fs::write(
            sys.join(format!("models@{id}.ini")),
            "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
        )
        .expect("write render");
    }
    // Each of the two launched on the pair standing now (IP3c): its `applied`
    // record is what makes a later write's drift compare answer a real launch.
    // `lan-2` has no render of its own (its server profile has no file), so it
    // applied nothing and its state is asked for nothing.
    for id in ["default", "broken"] {
        crate::manager::service::write_applied(id).expect("the applied record");
    }
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
            Ok(Some(plain(&model_profiles)))
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
                Ok(Some(plain(&model_profiles))),
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
            Ok(Some(plain(&model_profiles)))
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
        // lan-3 joins the record, so its save is judged against the profiles
        // in use: a profile no instance runs binds nothing, and every address
        // would be free.
        std::fs::write(
            sys.join("instances.ini"),
            format!("{record}\n[lan-3]\nServerProfile = lan-3\n"),
        )
        .expect("write instances");
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
        // The record back to its sections: lan-3 is no instance below.
        std::fs::write(sys.join("instances.ini"), record).expect("write instances");

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
        assert_eq!(
            calls(&c),
            vec![
                Call::Status("default".into()),
                Call::Status("broken".into()),
            ],
            "IP3c: the accepted compare and swap answers the apply, whose drift compare asks \
             the state of the two instances whose pair moved off the pair their launch \
             recorded, in the record's order; `lan-2` renders no pair (its server profile has \
             no file), so it applied nothing and is asked for nothing; the refused ones ask \
             nothing, and no unit verb"
        );
        std::fs::write(sys.join("models.ini"), model_profiles).expect("restore presets");
    }

    // ── The assignment verb, end to end ────────────────────────────────
    // `set_model_profile` is the plain verb `(sss)`: the accepted call
    // writes the record and renders the instance's model profiles; the
    // refusals are the core's own errors and write nothing.
    {
        let c = core(true, &[]);
        let (_server, client) = connect(&c, None);
        let assignments =
            std::fs::read_to_string(sys.join("assignments.ini")).expect("assignments");
        assert_eq!(
            client.set_model_profile("default", "Qwen3.8-27B", "on"),
            Err(ClientError::NotAuthorized),
            "no sender, no assignment"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("assignments.ini")).expect("assignments"),
            assignments,
            "the refused call writes nothing"
        );
        // SSP-B-PRE-1 BEGIN (pre-existing red: the verb, not a pin)
        assert_eq!(
            client.set_server_profile("default", "default"),
            Err(ClientError::NotAuthorized),
            "no sender, no server profile change"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            record,
            "the refused call writes no instance record"
        );
        // SSP-B-PRE-1 END
        assert_eq!(asked(&c), vec![], "a call with no sender asks nobody");
    }
    {
        let c = core(true, &[]);
        let (_server, client) = connect(&c, Some(":1.42"));
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("assignments");
        assert_eq!(
            client.set_model_profile("default", "Qwen3.8-27B", "on"),
            Ok(()),
            "the pool's model profile, assigned to an instance"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("assignments.ini")).unwrap(),
            "[default]\nQwen3.8-27B = on\n",
            "the assignment landed"
        );
        assert!(
            std::fs::read_to_string(sys.join("models@default.ini"))
                .expect("the render")
                .contains("[Qwen3.8-27B]"),
            "the render carries the enabled model profile"
        );
        assert_eq!(
            asked(&c),
            vec![(alice.clone(), Action::Configure)],
            "the guarded call a write is: configure asked of the sender"
        );
        assert_eq!(
            calls(&c),
            vec![
                Call::Status("default".into()),
                Call::Status("broken".into()),
            ],
            "IP3c: the accepted assignment answers the apply, whose drift compare asks the \
             state of the two instances whose pair moved off the pair their launch recorded, \
             in the record's order; `lan-2` renders no pair, so it applied nothing and is asked \
             for nothing; it starts, stops and restarts nothing"
        );
        assert_eq!(
            client.set_model_profile("default", "Qwen3.8-27B", "off"),
            Ok(()),
            "the same profile switched off"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("assignments.ini")).unwrap(),
            "[default]\nQwen3.8-27B = off\n"
        );
        assert!(
            !std::fs::read_to_string(sys.join("models@default.ini"))
                .expect("the render")
                .contains("[Qwen3.8-27B]"),
            "the render drops the disabled model profile"
        );
        assert_eq!(
            client.set_model_profile("lan-3", "Qwen3.8-27B", "on"),
            Err(ClientError::NoInstance),
            "lan-3 is no instance"
        );
        for bad in ["", "../x", "Default", "a_b"] {
            assert_eq!(
                client.set_model_profile(bad, "Qwen3.8-27B", "on"),
                Err(ClientError::InvalidId),
                "id {bad:?}"
            );
        }
        assert_eq!(
            client.set_model_profile("default", "zzz", "on"),
            Err(ClientError::Invalid(Refusal {
                key: "no-model-profile-named",
                values: vec!["zzz".into()],
            })),
            "the pool has no model profile zzz"
        );
        for bad in ["maybe", "", "ON", "on,off"] {
            assert!(
                matches!(
                    client.set_model_profile("default", "Qwen3.8-27B", bad),
                    Err(ClientError::Invalid(_))
                ),
                "value {bad:?} is not one of the four"
            );
        }
        assert_eq!(
            std::fs::read_to_string(sys.join("assignments.ini")).unwrap(),
            "[default]\nQwen3.8-27B = off\n",
            "refused calls write nothing"
        );
    }

    // ── The server profile verb, end to end ──────────────────────────────
    // `set_server_profile` is the plain verb `(ss)`: the accepted call
    // writes the record, and every refusal is the core's own error and
    // writes nothing.
    {
        let c = core(true, &[]);
        let (_server, client) = connect(&c, Some(":1.42"));
        let before = std::fs::read_to_string(sys.join("instances.ini")).expect("record");
        std::fs::write(
            sys.join("server@alt.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("alt profile");
        assert_eq!(
            client.set_server_profile("lan-2", "alt"),
            Ok(()),
            "a server profile no instance uses"
        );
        let record = std::fs::read_to_string(sys.join("instances.ini")).unwrap();
        assert!(
            record.contains("[lan-2]\nServerProfile = alt\n"),
            "the switch landed: {record:?}"
        );
        assert!(
            record.starts_with("[default]\nServerProfile = default\n\n[broken]\n"),
            "the sections the switch did not touch keep their bytes: {record:?}"
        );
        assert_eq!(
            asked(&c),
            vec![(alice.clone(), Action::Configure)],
            "the guarded call a write is: configure asked of the sender"
        );
        assert_eq!(
            calls(&c),
            vec![
                Call::Status("default".into()),
                Call::Status("broken".into()),
            ],
            "IP3c: the accepted switch answers the apply, whose drift compare asks the state \
             of the two instances whose pair the renders of this phase moved off the pair \
             their launch recorded, in the record's order; `lan-2` renders no pair (its server \
             profile has no file), so it applied nothing and is asked for nothing; the \
             refusals ask nothing, and no unit verb"
        );
        assert_eq!(
            client.set_server_profile("lan-3", "alt"),
            Err(ClientError::NoInstance),
            "lan-3 is no instance"
        );
        for bad in ["", "../x", "Default", "a_b"] {
            assert_eq!(
                client.set_server_profile(bad, "alt"),
                Err(ClientError::InvalidId),
                "id {bad:?}"
            );
        }
        assert_eq!(
            client.set_server_profile("default", "nope"),
            Err(ClientError::NoServerProfile("nope".into())),
            "the pool has no server profile nope"
        );
        assert_eq!(
            client.set_server_profile("lan-2", "broken"),
            Err(ClientError::ServerProfileInUse(
                "broken".into(),
                "broken".into()
            )),
            "one server profile, one instance: the client splits profile and owner"
        );
        std::fs::write(
            sys.join("server@clash.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("clash profile");
        assert_eq!(
            client.set_server_profile("lan-2", "clash"),
            Err(ClientError::AddressInUse("default".into())),
            "the port default listens on, named by its instance"
        );
        std::fs::write(
            sys.join("server@web.ini"),
            "[Server]\nPort = 8590\nHostname = 127.0.0.1\n",
        )
        .expect("web profile");
        let kept = std::fs::read_to_string(sys.join("instances.ini")).unwrap();
        assert_eq!(
            client.set_server_profile("lan-2", "web"),
            Err(ClientError::AddressInUse("private-ai-bff".into())),
            "the port of the web service is named by its service"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            kept,
            "refused calls write nothing"
        );
        let _ = std::fs::remove_file(sys.join("server@clash.ini"));
        let _ = std::fs::remove_file(sys.join("server@web.ini"));
        let _ = std::fs::remove_file(sys.join("server@alt.ini"));
        std::fs::write(sys.join("instances.ini"), before.as_bytes()).expect("restore instances");
    }

    // ── The tags of an instance, over the wire ───────────────────────────
    // `Instances` answers the server profile, the port and the two lists, one
    // row per section of the record: the `ServerProfile` the section names,
    // and the port and the tags of that profile's file (the file the launch
    // reads), and the tags of the section itself. A profile whose file is not
    // there is no port and no profile tags. `SetInstanceTags` is the plain
    // verb `(sas)`: the accepted call writes the record, every refusal is the
    // core's own error and writes nothing.
    {
        let tagged = "[default]\nServerProfile = gpu\nTags = gpu , edge \n\n[lan-2]\nServerProfile = nowhere\n";
        std::fs::write(sys.join("instances.ini"), tagged).expect("record");
        std::fs::write(
            sys.join("server@gpu.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\nTags = edge,v2\n",
        )
        .expect("gpu profile");
        std::fs::write(
            sys.join("server@nowhere.ini"),
            "[Server]\nPort = 8099\nHostname = 127.0.0.1\n",
        )
        .expect("nowhere profile");
        let c = core(true, &[]);
        let (_server, client) = connect(&c, Some(":1.42"));
        let rows = client.instances().expect("instances");
        assert_eq!(
            rows.iter()
                .map(|i| (
                    i.id.as_str(),
                    i.engine.as_str(),
                    i.server_profile.as_deref(),
                    i.port,
                ))
                .collect::<Vec<_>>(),
            vec![
                ("default", "llama.cpp", Some("gpu"), 8080),
                ("lan-2", "llama.cpp", Some("nowhere"), 8099),
            ],
            "the record's order, each row with the server profile the section names and \
             the port of that profile: the name is the row's, never read off the id"
        );
        assert_eq!(
            rows.iter()
                .map(|i| (i.profile_tags.join(","), i.instance_tags.join(",")))
                .collect::<Vec<_>>(),
            vec![
                ("edge,v2".to_string(), "gpu,edge".to_string()),
                (String::new(), String::new()),
            ],
            "the profile's list and the section's own, the hand-spaced one trimmed"
        );
        // A section naming a profile whose file is not there is listed too:
        // no port and no tags, not a failure that drops the list.
        std::fs::write(
            sys.join("instances.ini"),
            format!("{tagged}\n[lan-9]\nServerProfile = absent\n"),
        )
        .expect("record");
        let rows = client.instances().expect("instances");
        assert_eq!(rows.len(), 3, "the section of a missing profile is listed");
        assert_eq!(
            (
                rows[2].id.as_str(),
                rows[2].port,
                rows[2].profile_tags.as_slice(),
                rows[2].instance_tags.as_slice()
            ),
            ("lan-9", 0, &[][..], &[][..]),
            "a profile with no file is no port and no tags"
        );
        std::fs::write(sys.join("instances.ini"), tagged).expect("record");
        assert_eq!(
            asked(&c),
            vec![],
            "the list is answered with no identity, as the list of ids is"
        );

        assert_eq!(
            client.set_instance_tags("default", &["edge".into(), "a-1".into()]),
            Ok(()),
            "a list of the rule's own shape"
        );
        let record = std::fs::read_to_string(sys.join("instances.ini")).unwrap();
        assert_eq!(
            record,
            "[default]\nServerProfile = gpu\nTags = edge,a-1\n\n[lan-2]\nServerProfile = nowhere\n",
            "the section's own Tags replaced in place, the other section as it was"
        );
        assert_eq!(
            asked(&c),
            vec![(alice.clone(), Action::Configure)],
            "the guarded call a write is: configure asked of the sender"
        );
        assert_eq!(calls(&c), vec![], "a tags change touches no unit");

        assert_eq!(
            client.set_instance_tags("default", &[]),
            Ok(()),
            "the empty list clears them"
        );
        assert!(
            std::fs::read_to_string(sys.join("instances.ini"))
                .unwrap()
                .contains("Tags = \n"),
            "the key stays, empty: an empty value is rendered as `Tags = `"
        );

        assert_eq!(
            client.set_instance_tags("lan-3", &["gpu".into()]),
            Err(ClientError::NoInstance),
            "lan-3 is no instance"
        );
        for bad in ["", "../x", "Default", "a_b"] {
            assert_eq!(
                client.set_instance_tags(bad, &["gpu".into()]),
                Err(ClientError::InvalidId),
                "id {bad:?}"
            );
        }
        for list in [
            vec!["".to_string()],
            vec!["t".repeat(33)],
            vec!["Edge".to_string()],
            vec!["a b".to_string()],
            vec!["ok".to_string(), "ok".to_string()],
            (0..9).map(|i| format!("t{i}")).collect(),
        ] {
            let got = client
                .set_instance_tags("default", &list)
                .expect_err("the rule");
            let want = pai_core::checks::tags_rule(&list).expect_err("the rule refuses it too");
            match got {
                ClientError::Invalid(refusal) => assert_eq!(
                    (refusal.key, refusal.values.as_slice()),
                    (want.key, want.values.as_slice()),
                    "{list:?}: the client decodes the rule's own refusal"
                ),
                other => panic!("{list:?}: {other:?}"),
            }
        }
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            "[default]\nServerProfile = gpu\nTags = \n\n[lan-2]\nServerProfile = nowhere\n",
            "refused calls write nothing"
        );

        // A client with no sender is refused, as every guarded call is.
        let (_server, anonymous) = connect(&c, None);
        assert_eq!(
            anonymous.set_instance_tags("default", &["gpu".into()]),
            Err(ClientError::NotAuthorized)
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            "[default]\nServerProfile = gpu\nTags = \n\n[lan-2]\nServerProfile = nowhere\n",
            "the refused write left the record"
        );
        let _ = std::fs::remove_file(sys.join("server@gpu.ini"));
        let _ = std::fs::remove_file(sys.join("server@nowhere.ini"));
        let _ = std::fs::remove_file(sys.join("server@lan-2.ini"));
    }

    // ── The label of an instance, over the wire ───────────────────────────
    // `Instances` answers the label beside the id, the engine and the port, so
    // the web UI shows one row out of one read: the label the section carries,
    // nothing when it carries none. `SetInstanceLabel` is the plain verb `(ss)`:
    // the text is written trimmed, the empty text REMOVES the key, and every
    // refusal is the core's own error with the record untouched. A label another
    // instance carries, ignoring case, is `instance-label-taken`. Nothing here
    // touches a unit: the label is metadata, and the instance is running.
    {
        let seeded = "[default]\nServerProfile = gpu\n\n[gpu-1]\nServerProfile = gpu\nLabel = GPU box\nnote = keep me\n";
        std::fs::write(sys.join("instances.ini"), seeded).expect("record");
        std::fs::write(
            sys.join("server@gpu.ini"),
            "[Server]\nPort = 8080\nHostname = 127.0.0.1\n",
        )
        .expect("gpu profile");
        let c = core(true, &[]);
        let (_server, client) = connect(&c, Some(":1.42"));
        let rows = client.instances().expect("instances");
        assert_eq!(
            rows.iter()
                .map(|i| (i.id.as_str(), i.engine.as_str(), i.port, i.label.as_deref(),))
                .collect::<Vec<_>>(),
            vec![
                ("default", "llama.cpp", 8080, None),
                ("gpu-1", "llama.cpp", 8080, Some("GPU box")),
            ],
            "the label of the section and no label for a section that names none, \
             the engine and the port of the same row for the fallback"
        );
        assert_eq!(
            asked(&c),
            vec![],
            "the list is answered with no identity, as the list of ids is"
        );
        assert_eq!(calls(&c), vec![], "reading the list touches no unit");

        assert_eq!(
            client.set_instance_label("default", "  Coding box "),
            Ok(()),
            "a text with hanging space"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            "[default]\nServerProfile = gpu\nLabel = Coding box\n\n[gpu-1]\nServerProfile = gpu\nLabel = GPU box\nnote = keep me\n",
            "the key written trimmed, every other byte of the record kept"
        );
        assert_eq!(
            asked(&c),
            vec![(alice.clone(), Action::Configure)],
            "the guarded call a write is: configure asked of the sender"
        );
        assert_eq!(
            calls(&c),
            vec![],
            "IP3c: a relabel starts, stops and restarts no unit, and asks the state of none: \
             the label is metadata, answered with no apply, so its own running row is read on \
             request and this verb reads nothing unasked"
        );
        assert_eq!(
            client.instances().expect("instances")[0].label.as_deref(),
            Some("Coding box"),
            "the write reads back over the same wire"
        );

        assert_eq!(
            client.set_instance_label("default", ""),
            Ok(()),
            "the empty text clears the label"
        );
        let cleared =
            "[default]\nServerProfile = gpu\n\n[gpu-1]\nServerProfile = gpu\nLabel = GPU box\nnote = keep me\n";
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            cleared,
            "the Label line is removed, never left empty"
        );
        assert_eq!(
            client.instances().expect("instances")[0].label,
            None,
            "the cleared label is no label, and the UI falls back to engine and port"
        );

        // An instance writing the label it already carries is not a clash with
        // itself.
        assert_eq!(
            client.set_instance_label("gpu-1", "GPU box"),
            Ok(()),
            "the label of the instance itself"
        );
        assert_eq!(
            asked(&c),
            vec![(alice.clone(), Action::Configure); 3],
            "one configure ask per accepted call"
        );

        assert_eq!(
            client.set_instance_label("lan-3", "Coding box"),
            Err(ClientError::NoInstance),
            "lan-3 is no instance"
        );
        for bad in ["", "../x", "Default", "a_b"] {
            assert_eq!(
                client.set_instance_label(bad, "Coding box"),
                Err(ClientError::InvalidId),
                "id {bad:?}"
            );
        }
        for text in [
            "z".repeat(pai_core::checks::LABEL_MAX + 1),
            "two\nlines".to_string(),
            "tab\there".to_string(),
            "a;b".to_string(),
            "Box #1".to_string(),
        ] {
            let got = client
                .set_instance_label("default", &text)
                .expect_err("the rule");
            let want =
                pai_core::checks::instance_label(&text).expect_err("the rule refuses it too");
            match got {
                ClientError::Invalid(refusal) => assert_eq!(
                    (refusal.key, refusal.values.as_slice()),
                    (want.key, want.values.as_slice()),
                    "{text:?}: the client decodes the rule's own refusal"
                ),
                other => panic!("{text:?}: {other:?}"),
            }
        }
        assert_eq!(
            client.set_instance_label("default", "GPU BOX"),
            Err(ClientError::Invalid(pai_core::checks::Refusal {
                key: "instance-label-taken",
                values: vec!["GPU BOX".to_string(), "gpu-1".to_string()],
            })),
            "the label of gpu-1, ignoring case, is taken, and the refusal names who holds it"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            cleared,
            "refused calls write nothing"
        );

        // A client with no sender is refused, as every guarded call is.
        let (_server, anonymous) = connect(&c, None);
        assert_eq!(
            anonymous.set_instance_label("default", "Coding box"),
            Err(ClientError::NotAuthorized)
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            cleared,
            "the refused write left the record"
        );
        std::fs::write(sys.join("instances.ini"), seeded).expect("restore the record");
        let _ = std::fs::remove_file(sys.join("server@gpu.ini"));
    }

    // ── Creating, listing and deleting a server profile, over the bus ─────
    // The three new client methods against the interface: the file a create
    // writes, the answers of every refusal decoded by the client, and the
    // rows of the list with `instance` read from the frame's empty string.
    {
        use std::os::unix::fs::PermissionsExt;

        let record = "[default]\nServerProfile = default\n\n[broken]\nServerProfile = broken\n";
        std::fs::write(sys.join("instances.ini"), record).expect("record");
        let c = core(true, &[]);
        let (_server, client) = connect(&c, Some(":1.42"));
        let text = |name: &str| std::fs::read_to_string(sys.join(name)).expect("profile");
        type Row = (String, String, u16, Vec<String>, Option<String>);
        let rows = |client: &DbusClient| -> Vec<Row> {
            client
                .server_profiles()
                .expect("server_profiles")
                .iter()
                .map(|r| {
                    (
                        r.id.clone(),
                        r.engine.clone(),
                        r.port,
                        r.tags.clone(),
                        r.instance.clone(),
                    )
                })
                .collect()
        };

        assert_eq!(
            client.create_server_profile("llama.cpp", "bus-p", Some(8123)),
            Ok(()),
            "a name that is nothing yet"
        );
        assert!(
            text("server@bus-p.ini").contains("Port = 8123"),
            "the port of the call, in the file: {:?}",
            text("server@bus-p.ini")
        );
        assert_eq!(
            std::fs::metadata(sys.join("server@bus-p.ini"))
                .expect("metadata")
                .permissions()
                .mode()
                & 0o7777,
            0o640,
            "the mode of a served profile"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).expect("record"),
            record,
            "a create writes no instance record"
        );
        assert_eq!(
            rows(&client)
                .iter()
                .find(|r| r.0 == "bus-p")
                .map(|r| (r.1.clone(), r.2, r.4.clone())),
            Some(("llama.cpp".to_string(), 8123u16, None)),
            "the new file is a row, and an unused one: the empty instance of the bus \
             answer is None to the client"
        );
        assert_eq!(
            client.create_server_profile("llama.cpp", "bus-p", Some(9001)),
            Err(ClientError::ServerProfileExists("bus-p".into())),
            "a second create of the same name"
        );
        assert!(
            text("server@bus-p.ini").contains("Port = 8123"),
            "the file kept its bytes"
        );
        assert_eq!(
            client.create_server_profile("llama.cpp", "bus-zero", None),
            Ok(()),
            "None is port 0 on the wire"
        );
        assert!(
            !text("server@bus-zero.ini")
                .lines()
                .any(|l| l.trim_start().starts_with("Port ")),
            "port 0 leaves no Port key: {:?}",
            text("server@bus-zero.ini")
        );
        assert_eq!(
            client.create_server_profile("vllm", "bus-q", Some(8123)),
            Err(ClientError::Invalid(Refusal {
                key: "unknown-engine",
                values: vec!["vllm".into()],
            })),
            "the engine the manager does not serve"
        );
        for name in ["../x", "X", "a_b", ""] {
            assert_eq!(
                client.create_server_profile("llama.cpp", name, Some(8123)),
                Err(ClientError::Invalid(Refusal {
                    key: "server-profile-bad-id",
                    values: vec![name.into()],
                })),
                "name {name:?}"
            );
        }
        assert!(
            !sys.join("server@bus-q.ini").exists() && !sys.join("x.ini").exists(),
            "a refused create created nothing, and no path outside the engine's directory"
        );
        assert_eq!(
            client.delete_server_profile("llama.cpp", "default"),
            Err(ClientError::ServerProfileInUse(
                "default".into(),
                "default".into()
            )),
            "in use: the client splits the profile and the instance the bus's message carries"
        );
        assert!(
            sys.join("server@default.ini").is_file(),
            "the file of an in-use profile is kept"
        );
        assert_eq!(
            client.delete_server_profile("llama.cpp", "nope"),
            Err(ClientError::NoServerProfile("nope".into())),
            "no file, no profile"
        );
        assert_eq!(
            client.delete_server_profile("vllm", "default"),
            Err(ClientError::Invalid(Refusal {
                key: "unknown-engine",
                values: vec!["vllm".into()],
            })),
            "the engine, before the file"
        );
        assert_eq!(
            client.delete_server_profile("llama.cpp", "bus-p"),
            Ok(()),
            "an unused profile goes"
        );
        assert!(
            !sys.join("server@bus-p.ini").exists(),
            "its file is gone: the create and the delete round trip over the bus"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).expect("record"),
            record,
            "neither the create nor the delete of an unused profile writes the record"
        );
        assert_eq!(
            asked(&c),
            vec![(alice.clone(), Action::Configure); 12],
            "every verb asked configure of the sender, once per call, the list never"
        );

        // The list needs no sender; the two verbs do.
        let (_server, anonymous) = connect(&c, None);
        assert_eq!(
            anonymous.create_server_profile("llama.cpp", "bus-anon", Some(8123)),
            Err(ClientError::NotAuthorized),
            "no sender, no create"
        );
        assert_eq!(
            anonymous.delete_server_profile("llama.cpp", "bus-zero"),
            Err(ClientError::NotAuthorized),
            "no sender, no delete"
        );
        assert!(
            !sys.join("server@bus-anon.ini").exists() && sys.join("server@bus-zero.ini").is_file(),
            "the refused calls wrote and removed nothing"
        );
        assert_eq!(
            rows(&anonymous)
                .iter()
                .filter(|r| r.0.starts_with("bus-"))
                .map(|r| (r.0.clone(), r.2, r.4.clone()))
                .collect::<Vec<_>>(),
            vec![("bus-zero".to_string(), u16::try_from(pai_core::server_cfg::DEFAULT_PORT).expect("DEFAULT_PORT"), None)],
            "the list is answered to a caller with no sender, the portless file at the default port"
        );
        assert_eq!(
            asked(&c),
            vec![(alice.clone(), Action::Configure); 12],
            "a call with no identity asks nobody, and the list asks nobody"
        );
        let _ = std::fs::remove_file(sys.join("server@bus-zero.ini"));
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
            protocol: 3,
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
    let model_profiles = real_model_files(
        dir.path(),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    );
    let more = format!("{model_profiles}[Qwen3.8-14B]\nmodel = /models/Qwen3.8-14B-Q4_K_M.gguf\n");
    std::fs::write(sys.join("server@default.ini"), ini).expect("write default");
    std::fs::write(sys.join("models.ini"), &model_profiles).expect("write presets");
    // The instance is the section of the record: the fixture writes it.
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = default\n",
    )
    .expect("write instances");
    // A start loads the instance's render, which the manager renders from the
    // pool and the assignment: these pins are about the caller, so `default`
    // has a model profile assigned and the render that names it.
    std::fs::write(sys.join("assignments.ini"), "[default]\nQwen3.8-27B = on\n")
        .expect("write assignments");
    std::fs::write(
        sys.join("models@default.ini"),
        "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n",
    )
    .expect("write render");
    // The launch of `default` recorded the pair standing now (IP3c): the pair
    // the render gives carries the pool's model path, which the fixture's own
    // text does not name, so the pair the answering writes below leave stands
    // off this record, and each of their applies asks this instance's state once.
    crate::manager::service::write_applied("default").expect("the applied record");
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
            "IP3c: every guarded call reached the service manager and the unguarded status is \
             its own probe; the accepted whole-file write answers no apply, so it asks the \
             state of nothing (the compare and swap write of the same file, right below, does)"
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
            "a refused principal reached no service manager: the count is the accepted calls \
             of the blocks above"
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
            "a caller that is not the bff started nothing: the count is the accepted calls \
             above plus this caller's own status probe"
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
            calls(&c)[calls_before..],
            [Call::Status("default".into())],
            "IP3c: the accepted write answers the apply, whose drift compare asks the state of \
             the record's one instance, and asks no unit verb; the refused write asks nothing"
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

    // ── The member's model profile assignment, through the client ────────
    // The bff's client bound to `alice`: `set_model_profile` goes as the
    // `As` form, the member's membership decides. The counts are relative:
    // this phase's logs already carry every call above.
    {
        let admins_before = asked_admin(&admins).len();
        let bff_before = bff_checks(&c).len();
        let calls_before = calls(&c).len();
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("assignments");
        assert_eq!(
            on("set_model_profile", &client, move |c| c
                .acting_for("alice")
                .set_model_profile("default", "Qwen3.8-27B", "on")),
            Ok(()),
            "a member assigns a model profile"
        );
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\nQwen3.8-27B = on\n",
            "the member's assignment landed"
        );
        assert_eq!(
            on("set_model_profile", &client, move |c| c
                .acting_for("bob")
                .set_model_profile("default", "Qwen3.8-27B", "off")),
            Err(ClientError::NotAuthorized),
            "a principal outside pai is refused"
        );
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\nQwen3.8-27B = on\n",
            "the refused call wrote nothing"
        );
        assert_eq!(
            calls(&c)[calls_before..],
            [Call::Status("default".into())],
            "IP3c: the accepted assignment answers the apply, whose drift compare asks the \
             state of the record's one instance, and asks no unit verb; the refused call asks \
             nothing"
        );
        assert_eq!(
            asked_admin(&admins).len(),
            admins_before + 2,
            "the membership of the vouched name, asked once per call"
        );
        assert_eq!(
            bff_checks(&c).len(),
            bff_before + 2,
            "the caller checked once per principal call"
        );
    }
    // A caller that is not the bff, vouching for a member: refused.
    {
        let (_server, other) = connect(&c, Some(OTHER));
        let other = Arc::new(other);
        assert_eq!(
            on("set_model_profile", &other, move |c| c
                .acting_for("alice")
                .set_model_profile("default", "Qwen3.8-27B", "off")),
            Err(ClientError::NotAuthorized),
            "a caller that is not the bff does not vouch"
        );
        assert_eq!(
            text_of("assignments.ini"),
            "[default]\nQwen3.8-27B = on\n",
            "a caller that is not the bff writes nothing"
        );
    }

    // ── The member's server profile of an instance, through the client ───
    // The bff's client bound to `alice`: `set_server_profile` goes as the
    // `As` form, the member's membership decides. The counts are relative:
    // this phase's logs already carry every call above.
    {
        let admins_before = asked_admin(&admins).len();
        let bff_before = bff_checks(&c).len();
        let calls_before = calls(&c).len();
        let before = std::fs::read_to_string(sys.join("instances.ini")).expect("record");
        std::fs::write(
            sys.join("server@alt.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("alt profile");
        assert_eq!(
            on("set_server_profile", &client, move |c| c
                .acting_for("alice")
                .set_server_profile("default", "alt")),
            Ok(()),
            "a member switches the server profile"
        );
        assert_eq!(
            text_of("instances.ini"),
            "[default]\nServerProfile = alt\n",
            "the member's switch landed"
        );
        let kept = text_of("instances.ini");
        assert_eq!(
            on("set_server_profile", &client, move |c| c
                .acting_for("bob")
                .set_server_profile("default", "default")),
            Err(ClientError::NotAuthorized),
            "a principal outside pai is refused"
        );
        assert_eq!(
            text_of("instances.ini"),
            kept,
            "the refused call wrote nothing"
        );
        assert_eq!(
            calls(&c)[calls_before..],
            [Call::Status("default".into())],
            "IP3c: the accepted switch answers the apply, whose drift compare asks the state of \
             the record's one instance, and asks no unit verb; the refused call asks nothing"
        );
        assert_eq!(
            asked_admin(&admins).len(),
            admins_before + 2,
            "the membership of the vouched name, asked once per call"
        );
        assert_eq!(
            bff_checks(&c).len(),
            bff_before + 2,
            "the caller checked once per principal call"
        );
        let kept = text_of("instances.ini");
        // A caller that is not the bff, vouching for a member: refused.
        let (_server, other) = connect(&c, Some(OTHER));
        let other = Arc::new(other);
        assert_eq!(
            on("set_server_profile", &other, move |c| c
                .acting_for("alice")
                .set_server_profile("default", "default")),
            Err(ClientError::NotAuthorized),
            "a caller that is not the bff does not vouch"
        );
        assert_eq!(
            text_of("instances.ini"),
            kept,
            "a caller that is not the bff writes nothing"
        );
        let _ = std::fs::remove_file(sys.join("server@alt.ini"));
        std::fs::write(sys.join("instances.ini"), before.as_bytes()).expect("restore instances");
    }

    // ── The member's create and delete of a server profile, through the client ──
    // The bound client picks the `As` name of the two new verbs: the member's
    // call is done, a principal outside `pai` and a caller that is not the bff
    // are refused with nothing written, and polkit is never asked for a
    // principal.
    {
        let admins_before = asked_admin(&admins).len();
        let bff_before = bff_checks(&c).len();
        let quiet_before = polkit_quiet(&c).len();
        assert_eq!(
            on("create_server_profile", &client, move |c| c
                .acting_for("alice")
                .create_server_profile("llama.cpp", "as-bus", Some(8123))),
            Ok(()),
            "a member creates a server profile"
        );
        assert!(
            sys.join("server@as-bus.ini").is_file(),
            "the member's create wrote the file"
        );
        assert_eq!(
            on("create_server_profile", &client, move |c| c
                .acting_for("bob")
                .create_server_profile("llama.cpp", "as-bob", Some(8123))),
            Err(ClientError::NotAuthorized),
            "a principal outside pai is refused"
        );
        assert!(
            !sys.join("server@as-bob.ini").exists(),
            "the refused create wrote nothing"
        );
        assert_eq!(
            on("delete_server_profile", &client, move |c| c
                .acting_for("alice")
                .delete_server_profile("llama.cpp", "as-bus")),
            Ok(()),
            "and the member deletes what it created"
        );
        assert!(!sys.join("server@as-bus.ini").exists(), "its file is gone");
        let before = text_of("server@default.ini");
        assert_eq!(
            on("delete_server_profile", &client, move |c| c
                .acting_for("bob")
                .delete_server_profile("llama.cpp", "default")),
            Err(ClientError::NotAuthorized),
            "the same for a delete by a principal outside pai"
        );
        assert_eq!(
            text_of("server@default.ini"),
            before,
            "the refused delete removed nothing"
        );
        assert_eq!(
            asked_admin(&admins).len(),
            admins_before + 4,
            "the membership of the vouched name, asked once per call"
        );
        assert_eq!(
            bff_checks(&c).len(),
            bff_before + 4,
            "the caller checked once per principal call"
        );
        assert_eq!(
            polkit_quiet(&c).len(),
            quiet_before,
            "a call for a member runs as the bff's own: polkit is not asked"
        );
        // A caller that is not the bff, vouching for a member: refused, the
        // membership never looked up.
        let admins_here = asked_admin(&admins).len();
        let (_server, other) = connect(&c, Some(OTHER));
        let other = Arc::new(other);
        assert_eq!(
            on("create_server_profile", &other, move |c| c
                .acting_for("alice")
                .create_server_profile("llama.cpp", "as-other", Some(8123))),
            Err(ClientError::NotAuthorized),
            "a caller that is not the bff does not vouch"
        );
        assert_eq!(
            on("delete_server_profile", &other, move |c| c
                .acting_for("alice")
                .delete_server_profile("llama.cpp", "default")),
            Err(ClientError::NotAuthorized),
            "the same for a delete"
        );
        assert_eq!(
            asked_admin(&admins).len(),
            admins_here,
            "the caller gate refused before any membership question"
        );
        assert!(
            !sys.join("server@as-other.ini").exists() && sys.join("server@default.ini").is_file(),
            "a caller that is not the bff writes and removes nothing"
        );
    }

    // ── The layer facts of a saved model profile (ModelInfo / ModelInfoAs) ──
    // L1 of "Layers, MoE and tensor placement on the web model profile page"
    // (ARCHITECTURE.md (1)). The bus carries the same answer the stream does
    // (`tests/pipe_client.rs` pins it there): `model` the resolved absolute
    // path, `info` the counts, `reason` the keyed why, the reply's D-Bus
    // signature the coder's to pick. `ModelInfo(s)` is the caller's own read,
    // gated on `control` asked WITHOUT interaction; `ModelInfoAs(s, s)` is the
    // bff's, for a current member of `pai`, and a refused read opens no file.
    // The read opens a file under this phase's redirect, so it lives here.
    {
        use crate::manager::service::write_gguf_kv;
        use pai_core::client::{ModelFacts, Reason, TensorRow};

        let row = |name: &str, bytes: u64| TensorRow {
            name: name.into(),
            bytes,
        };

        let models = dir.path().join("info-models");
        std::fs::create_dir_all(&models).expect("models dir");
        let gguf = models.join("bus.gguf");
        write_gguf_kv(
            &gguf,
            "qwen3moe",
            &[
                ("qwen3moe.block_count", 40),
                ("qwen3moe.expert_count", 128),
                ("qwen3moe.expert_used_count", 8),
            ],
            &[
                ("token_embd.weight", 1 << 20),
                ("blk.0.ffn_gate_exps.weight", 1 << 20),
                ("blk.1.ffn_gate_exps.weight", 1 << 20),
                ("output.weight", 1 << 20),
            ],
        );
        let gone = models.join("gone.gguf");
        std::fs::write(
            sys.join("models.ini"),
            format!(
                "[bus]\nmodel = {}\n\n[fetched]\nhf-repo = the-org/the-model\n\n[gone]\nmodel = {}\n",
                gguf.display(),
                gone.display()
            ),
        )
        .expect("models.ini");
        let facts = ModelFacts {
            block_count: 40,
            moe: true,
            expert_count: 128,
            expert_used: 8,
            // J1: the bus carries the tensor list beside the counts.
            tensors: vec![
                row("token_embd.weight", 1 << 20),
                row("blk.0.ffn_gate_exps.weight", 1 << 20),
                row("blk.1.ffn_gate_exps.weight", 1 << 20),
                row("output.weight", 1 << 20),
            ],
            tensors_truncated: false,
        };
        let same = |got: &str, want: &std::path::Path| {
            std::fs::canonicalize(got).ok() == std::fs::canonicalize(want).ok()
        };
        let bff = ":1.9";
        let other = ":1.42";

        // The bff's own read, and the member's read it vouches for.
        let (c, asked) = devices_core(bff, true);
        let (_server, client) = connect(&c, Some(bff));
        let client = Arc::new(client);
        let got = on("model_info", &client, |c| c.model_info("bus")).expect("the bff's own read");
        assert_eq!(
            got.info,
            Some(facts.clone()),
            "the facts, the fields of the design, over the bus"
        );
        assert!(
            got.model.as_deref().is_some_and(|p| same(p, &gguf)),
            "`model` is the resolved absolute path the read opened: {got:?}"
        );
        assert_eq!(got.reason, None, "facts: no reason");
        assert_eq!(
            *c.authority().asked_quietly.lock().unwrap(),
            vec![(Caller(bff.into()), Action::Control)],
            "control asked without interaction"
        );
        assert_eq!(
            *c.authority().asked.lock().unwrap(),
            Vec::<(Caller, Action)>::new(),
            "never asked with a prompt"
        );
        let member = Arc::new(client.acting_for("alice"));
        assert_eq!(
            on("model_info as alice", &member, |c| c.model_info("bus")).map(|got| got.info),
            Ok(Some(facts.clone())),
            "the member's read, vouched for by the bff: the same facts"
        );
        let outsider = Arc::new(client.acting_for("bob"));
        assert_eq!(
            on("model_info as bob", &outsider, |c| c.model_info("bus")),
            Err(ClientError::NotAuthorized),
            "bob is no member of `pai`: no facts, and no path of the profile's file"
        );
        assert_eq!(
            *asked.lock().unwrap(),
            Vec::<String>::new(),
            "the read asks the manager's own seam for nothing"
        );

        // The reasons are answers of a call that worked, and an id the pool has
        // no section for is the keyed refusal (the page's 404).
        assert_eq!(
            on("model_info fetched", &member, |c| c.model_info("fetched"))
                .map(|got| (got.info, got.reason)),
            Ok((
                None,
                Some(Reason {
                    key: "model-info-fetched".into(),
                    args: vec!["hf-repo".into()],
                })
            )),
            "a model to fetch has no facts, and the key the section names"
        );
        assert_eq!(
            on("model_info gone", &member, |c| c.model_info("gone")).map(|got| got.reason),
            Ok(Some(Reason {
                key: "model-info-missing".into(),
                args: vec![gone.to_string_lossy().into_owned()],
            })),
            "a file that is not there: the resolved path, as the page shows it"
        );
        assert_eq!(
            on("model_info nope", &member, |c| c.model_info("nope")),
            Err(ClientError::Invalid(Refusal {
                key: "no-model-profile-named",
                values: vec!["nope".into()],
            })),
            "no section named `nope`: the keyed refusal"
        );

        // A caller polkit refuses the quiet check: refused.
        let (c, asked) = devices_core(other, false);
        let (_server, client) = connect(&c, Some(other));
        let client = Arc::new(client);
        assert_eq!(
            on("model_info", &client, |c| c.model_info("bus")),
            Err(ClientError::NotAuthorized),
            "no control, no facts"
        );
        assert_eq!(
            *asked.lock().unwrap(),
            Vec::<String>::new(),
            "a refused read opens nothing"
        );

        // A caller that is not the bff, vouching for a member: refused.
        let (c, _) = devices_core(bff, true);
        let (_server, client) = connect(&c, Some(other));
        let client = Arc::new(client);
        assert_eq!(
            on("model_info as alice", &client, |c| c
                .acting_for("alice")
                .model_info("bus")),
            Err(ClientError::NotAuthorized),
            "only the bff's account vouches"
        );
    }
}

// ── The node's device list over the bus (G2, ARCHITECTURE.md 1) ──────────
//
// The same answer the stream gives (`tests/pipe_client.rs` pins it there):
// the list the probe answered, in the probe's order, with its moment.
// `Devices()` is the caller's own read, gated on `control` asked WITHOUT
// interaction; `DevicesAs(s)` is the bff's, for a current member of `pai`. A
// refused read runs no engine. Names the tests assume, the contract for the
// coder: `ManagerClient::devices()`, the interface's `Devices` and `DevicesAs`,
// and the `(t, a(ssstt))` reply of the design.

/// The moment the seam's probe answered.
const PROBED_AT: u64 = 1_766_271_243;

/// The host's inventory the seam answers beside the list (R1): figures no real
/// host has, so a reply carrying them came from the fake, and nothing read
/// `/proc/meminfo`, `GlobalMemoryStatusEx` or the CPU count.
const RAM_MIB: u64 = 3_932_160;
const CPU_THREADS: u32 = 8;

/// The device list the seam answers: three entries, in the probe's order.
fn cards() -> crate::manager::service::fakes::FakeGpus {
    crate::manager::service::fakes::FakeGpus::new(&[("CUDA0", 12281)])
        .with_devices(
            Some(PROBED_AT),
            &[
                (
                    "CUDA0",
                    "NVIDIA GeForce RTX 4070 SUPER",
                    "CUDA",
                    12281u64,
                    10844u64,
                ),
                (
                    "Vulkan0",
                    "AMD Radeon(TM) Graphics",
                    "Vulkan",
                    33593u64,
                    31913u64,
                ),
                (
                    "Vulkan1",
                    "NVIDIA GeForce RTX 4070 SUPER",
                    "Vulkan",
                    11997u64,
                    10844u64,
                ),
            ],
        )
        .with_inventory(RAM_MIB, CPU_THREADS)
}

/// The list the client must hand back for [`cards`].
fn listed() -> pai_core::client::NodeDevices {
    pai_core::client::NodeDevices {
        probed_at: Some(PROBED_AT),
        ram_mib: RAM_MIB,
        cpu_threads: CPU_THREADS,
        devices: [
            (
                "CUDA0",
                "NVIDIA GeForce RTX 4070 SUPER",
                "CUDA",
                12281u64,
                10844u64,
            ),
            (
                "Vulkan0",
                "AMD Radeon(TM) Graphics",
                "Vulkan",
                33593u64,
                31913u64,
            ),
            (
                "Vulkan1",
                "NVIDIA GeForce RTX 4070 SUPER",
                "Vulkan",
                11997u64,
                10844u64,
            ),
        ]
        .iter()
        .map(
            |(id, name, backend, total, free)| pai_core::client::DeviceInfo {
                id: (*id).to_string(),
                name: (*name).to_string(),
                backend: (*backend).to_string(),
                total_mib: *total,
                free_mib: *free,
            },
        )
        .collect(),
    }
}

/// A core answering the device list of [`cards`], with `bff` as the bff's
/// account over the fake accounts (alice a member of `pai`, bob not), and
/// polkit allowing the quiet check as `quiet`.
fn devices_core(bff: &str, quiet: bool) -> (Arc<Core>, Arc<std::sync::Mutex<Vec<String>>>) {
    use crate::manager::service::fakes::{FakeGroup, FakePasswords};
    let seam = cards();
    let asked = seam.asked.clone();
    let core = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true)
            .quietly(quiet)
            .bff(&Caller(bff.into())),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new())
    .with_passwords(FakePasswords::new())
    .with_gpus(seam);
    (Arc::new(core), asked)
}

/// pai-core's `devices` against the interface: the bff's own read, the
/// member's read vouched for, and both refusals. The row's signature is
/// pinned in `pai_core::client::dbus`.
#[test]
fn the_dbus_client_reads_the_node_devices() {
    let bff = ":1.9";
    let other = ":1.42";

    // The bff's own read, and the member's read it vouches for.
    let (c, asked) = devices_core(bff, true);
    let (_server, client) = connect(&c, Some(bff));
    let client = Arc::new(client);
    assert_eq!(
        on("devices", &client, |c| c.devices()),
        Ok(listed()),
        "the list the probe answered, in its order"
    );
    assert_eq!(
        *c.authority().asked_quietly.lock().unwrap(),
        vec![(Caller(bff.into()), Action::Control)],
        "control asked without interaction"
    );
    assert_eq!(
        *c.authority().asked.lock().unwrap(),
        Vec::<(Caller, Action)>::new(),
        "never asked with a prompt"
    );
    let member = Arc::new(client.acting_for("alice"));
    assert_eq!(
        on("devices as alice", &member, |c| c.devices()),
        Ok(listed()),
        "the member's read, vouched for by the bff"
    );
    let outsider = Arc::new(client.acting_for("bob"));
    assert_eq!(
        on("devices as bob", &outsider, |c| c.devices()),
        Err(ClientError::NotAuthorized),
        "bob is no member of `pai`"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        vec!["devices".to_string(); 2],
        "one probe per accepted call, none for the refusal"
    );
    assert_eq!(
        *c.service().calls.lock().unwrap(),
        Vec::<Call>::new(),
        "the read reaches no service manager"
    );

    // A caller polkit refuses the quiet check: refused, and no probe.
    let (c, asked) = devices_core(other, false);
    let (_server, client) = connect(&c, Some(other));
    let client = Arc::new(client);
    assert_eq!(
        on("devices", &client, |c| c.devices()),
        Err(ClientError::NotAuthorized),
        "no control, no list"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        Vec::<String>::new(),
        "a refused read runs no engine"
    );

    // A caller that is not the bff, vouching for a member: refused, no probe.
    let (c, asked) = devices_core(bff, true);
    let (_server, client) = connect(&c, Some(other));
    let client = Arc::new(client);
    assert_eq!(
        on("devices as alice", &client, |c| c
            .acting_for("alice")
            .devices()),
        Err(ClientError::NotAuthorized),
        "only the bff's account vouches"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        Vec::<String>::new(),
        "a forged pair runs no engine"
    );
}

// ── The node's own info over the bus (AN3, ARCHITECTURE.md "About and node
// info") ──────────────────────────────────────────────────────────────────
//
// One read answers the node's document: `os` (name, version, kernel), `vulkan`
// (the word the node's Vulkan read answered) and `gpus` (the kernel's rows with
// the Vulkan driver joined onto the row whose bus id the device names).
// `NodeInfo()` is the caller's own read, gated on `control` asked WITHOUT
// interaction, exactly as `Devices()` is; `NodeInfoAs(s)` is the bff's, for a
// current member of `pai`. A refused read runs no Vulkan child: the seam's
// counter stays empty. The bus carries no null, so every text the host did not
// answer is the empty string, and a row's Vulkan pair carries its own flag; the
// row's signature is pinned in `pai_core::client::dbus`. Names the tests
// assume, the contract for the coder: `ManagerClient::node_info()`, the
// interface's `NodeInfo` and `NodeInfoAs`, and the `(s, s, s, s, a(sssssbss))`
// reply of the design.

/// The node the seam answers: Ubuntu 26.04 on a kernel no real host runs, two
/// kernel rows (an amdgpu with its module's version, an nvidia with no version
/// and no name, as the Linux kernel names nothing) and the child answering `ok`
/// with a device on each row's bus id, one device with no bus id (its own row)
/// and a CPU device, which is no driver of a card.
fn node_facts() -> crate::manager::service::fakes::FakeNodeFacts {
    crate::manager::service::fakes::FakeNodeFacts::new("Ubuntu", "26.04", "6.14.0-24-generic")
        .with_kernel_rows(&[
            ("0000:03:00.0", "1002", "", "amdgpu", "2.5.0"),
            ("0000:08:00.0", "10de", "", "nvidia", ""),
        ])
        .with_vulkan(
            pai_core::vulkan::Status::Ok,
            &[
                (
                    "0000:03:00.0",
                    "AMD Radeon AI PRO R9700",
                    false,
                    "RADV",
                    "Mesa 25.2.0",
                ),
                (
                    "0000:08:00.0",
                    "NVIDIA GeForce RTX 4070 SUPER",
                    false,
                    "NVIDIA",
                    "560.35.03",
                ),
                ("", "Asus Vega", false, "RADV", "Mesa 25.1.0"),
                ("", "llvmpipe", true, "llvmpipe", "Mesa 25.2.0"),
            ],
        )
}

/// One GPU row of the answer, `None` texts written out.
fn gpu_row(
    pci: Option<&str>,
    vendor: Option<&str>,
    name: &str,
    driver: Option<&str>,
    version: Option<&str>,
    vulkan: Option<(&str, &str)>,
) -> pai_core::vulkan::NodeGpu {
    let text = |one: Option<&str>| one.map(str::to_string);
    pai_core::vulkan::NodeGpu {
        pci: text(pci),
        vendor: text(vendor),
        name: Some(name.to_string()),
        driver: text(driver),
        version: text(version),
        vulkan: vulkan.map(|(driver, info)| pai_core::vulkan::Driver {
            driver: Some(driver.to_string()),
            info: Some(info.to_string()),
        }),
    }
}

/// The document the client must hand back for [`node_facts`]: the kernel's rows
/// with the Vulkan driver joined onto the row whose bus id the device names,
/// their names filled from `deviceName`, and the device with no bus id appended
/// as its own row. The CPU device is not a row.
fn node_document() -> pai_core::client::NodeInfoReply {
    pai_core::client::NodeInfoReply {
        os: pai_core::host::OsInfo {
            name: Some("Ubuntu".into()),
            version: Some("26.04".into()),
            kernel: Some("6.14.0-24-generic".into()),
        },
        vulkan: pai_core::vulkan::Status::Ok,
        gpus: vec![
            gpu_row(
                Some("0000:03:00.0"),
                Some("1002"),
                "AMD Radeon AI PRO R9700",
                Some("amdgpu"),
                Some("2.5.0"),
                Some(("RADV", "Mesa 25.2.0")),
            ),
            gpu_row(
                Some("0000:08:00.0"),
                Some("10de"),
                "NVIDIA GeForce RTX 4070 SUPER",
                Some("nvidia"),
                None,
                Some(("NVIDIA", "560.35.03")),
            ),
            gpu_row(
                None,
                None,
                "Asus Vega",
                None,
                None,
                Some(("RADV", "Mesa 25.1.0")),
            ),
        ],
    }
}

/// A core answering the node facts of [`node_facts`], with `bff` as the bff's
/// account over the fake accounts (alice a member of `pai`, bob not), and
/// polkit allowing the quiet check as `quiet`.
fn node_info_core(bff: &str, quiet: bool) -> (Arc<Core>, Arc<std::sync::Mutex<Vec<String>>>) {
    use crate::manager::service::fakes::{FakeGroup, FakePasswords};
    let seam = node_facts();
    let asked = seam.asked.clone();
    let core = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true)
            .quietly(quiet)
            .bff(&Caller(bff.into())),
        FakeResolver::lan(),
    )
    .with_group(FakeGroup::new())
    .with_passwords(FakePasswords::new())
    .with_node_facts(seam);
    (Arc::new(core), asked)
}

/// The seam's `asked` list after `calls` accepted reads: each one reads the OS,
/// the kernel's rows and the Vulkan answer, in that order.
fn read_parts(calls: usize) -> Vec<String> {
    (0..calls)
        .flat_map(|_| ["os", "gpus", "vulkan"])
        .map(str::to_string)
        .collect()
}

/// pai-core's `node_info` against the interface: the bff's own read, the
/// member's read vouched for, and the refusals. The reply crosses as the
/// document of the design, the empty string read back as no value.
#[test]
fn the_dbus_client_reads_the_node_info() {
    let bff = ":1.9";
    let other = ":1.42";

    // The bff's own read, and the member's read it vouches for.
    let (c, asked) = node_info_core(bff, true);
    let (_server, client) = connect(&c, Some(bff));
    let client = Arc::new(client);
    assert_eq!(
        on("node info", &client, |c| c.node_info()),
        Ok(node_document()),
        "the document of the fakes, joined and in bus id order"
    );
    assert_eq!(
        *c.authority().asked_quietly.lock().unwrap(),
        vec![(Caller(bff.into()), Action::Control)],
        "control asked without interaction, as `Devices` asks it"
    );
    assert_eq!(
        *c.authority().asked.lock().unwrap(),
        Vec::<(Caller, Action)>::new(),
        "never asked with a prompt"
    );
    let member = Arc::new(client.acting_for("alice"));
    assert_eq!(
        on("node info as alice", &member, |c| c.node_info()),
        Ok(node_document()),
        "the member's read, vouched for by the bff"
    );
    let outsider = Arc::new(client.acting_for("bob"));
    assert_eq!(
        on("node info as bob", &outsider, |c| c.node_info()),
        Err(ClientError::NotAuthorized),
        "bob is no member of `pai`"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        read_parts(2),
        "one read of each part per accepted call, none for the refusal"
    );
    assert_eq!(
        *c.service().calls.lock().unwrap(),
        Vec::<Call>::new(),
        "the read reaches no service manager"
    );

    // A caller polkit refuses the quiet check: refused, and no child.
    let (c, asked) = node_info_core(other, false);
    let (_server, client) = connect(&c, Some(other));
    let client = Arc::new(client);
    assert_eq!(
        on("node info", &client, |c| c.node_info()),
        Err(ClientError::NotAuthorized),
        "no control, no node info"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        Vec::<String>::new(),
        "a refused read spawns no Vulkan child"
    );

    // A caller that is not the bff, vouching for a member: refused, no child.
    let (c, asked) = node_info_core(bff, true);
    let (_server, client) = connect(&c, Some(other));
    let client = Arc::new(client);
    assert_eq!(
        on("node info as alice", &client, |c| c
            .acting_for("alice")
            .node_info()),
        Err(ClientError::NotAuthorized),
        "only the bff's account vouches"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        Vec::<String>::new(),
        "a forged pair spawns no Vulkan child"
    );
}

/// Creating and deleting a model profile over D-Bus, with pai-core's client
/// (slice MP1, its rules of IP3b): the three sources of a create, the clone
/// keeping the source's lines, every rule's refusal decoded by the client
/// (`ClientError::Invalid` with the key and its values), the delete's section
/// alone out of the pool, a caller with no sender refused, and the `As` forms
/// for a member of `pai` and an outsider. A create writes the pool and joins no
/// entry, so it moves no render; a model an entry lists is not free, and the
/// retired `assignments.ini` and `models.ini.disabled` are read by nothing.
pub(crate) fn manager_dbus_client_model_profiles_e2e() {
    use pai_core::client::ModelProfileSource;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    // The record: two instances, both running the instance profile `gpu`.
    const RECORD: &str = "[default]\nInstanceProfile = gpu\nPort = 8080\n\n[lan-2]\nInstanceProfile = gpu\nPort = 8081\n";
    // The entries ARE the model list: `gpu` lists three of the pool's models,
    // each in its own letter case, and `plain` lists none.
    const PROFILES: &str = "[gpu]\nServerProfile = gpu\n\n[gpu/On-One]\n\n[gpu/twin]\n\n[gpu/two-places]\n\n[plain]\nServerProfile = plain\n";
    std::fs::write(sys.join("instances.ini"), RECORD).expect("record");
    std::fs::write(sys.join("instance-profiles.ini"), PROFILES).expect("instance profiles");
    for name in ["gpu", "plain"] {
        std::fs::write(
            sys.join(format!("server@{name}.ini")),
            "[Server]\nDevices = CUDA0\n",
        )
        .expect("server profile");
    }
    // The pool: `On-One`, `twin` and `two-places` are listed by an entry,
    // `off-one` and `free-me` by none. The retired `models.ini.disabled` holds
    // `Retired` and the retired `assignments.ini` names `two-places`: neither is
    // read, so neither blocks a delete, and the test pins neither is written.
    let pool = real_model_files(
        dir.path(),
        "[On-One]\nmodel = /models/on1.gguf\n\n[Twin]\nmodel = /models/twin.gguf\n\n[off-one]\n; a note\ntemp = 0.3\nmodel = /models/off1.gguf\n\n[two-places]\nmodel = /models/two.gguf\n\n[free-me]\nmodel = /models/free.gguf\n",
    );
    let retired = real_model_files(dir.path(), "[Retired]\nmodel = /models/retired.gguf\n");
    std::fs::write(sys.join("models.ini"), &pool).expect("models.ini");
    std::fs::write(sys.join("models.ini.disabled"), &retired).expect("models.ini.disabled");
    let keys = "[default]\ntwo-places = inherit\n\n[lan-2]\ntwo-places = off\n";
    std::fs::write(sys.join("assignments.ini"), keys).expect("assignments.ini");
    let section = |text: &str, id: &str| -> String {
        pai_core::ini::text::section_text(text, id).unwrap_or_default()
    };
    for id in ["default", "lan-2"] {
        std::fs::write(
            sys.join(format!("models@{id}.ini")),
            pai_core::assign::render_profile(&pool, PROFILES, "gpu"),
        )
        .expect("render");
    }
    let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect("read");
    let pool_text = || text_of("models.ini");
    // Every render, by instance with its bytes: a create joins no entry, and the
    // delete below takes a model no entry listed.
    let renders = || -> Vec<(String, Vec<u8>)> {
        ["default", "lan-2"]
            .iter()
            .map(|id| {
                (
                    id.to_string(),
                    std::fs::read(sys.join(format!("models@{id}.ini"))).expect("render"),
                )
            })
            .collect()
    };
    let renders_before = renders();
    let refused = |key: &'static str, values: &[&str]| -> Result<(), ClientError> {
        Err(ClientError::Invalid(Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        }))
    };

    let c = core(true, &[]);
    let (_server, client) = connect(&c, Some(":1.42"));
    let alice = Caller(":1.42".into());

    // ── The three sources ──────────────────────────────────────────────────
    assert_eq!(
        client.create_model_profile(
            "bus-model",
            &ModelProfileSource::Model("/models/bus-model.gguf".into())
        ),
        Ok(()),
        "a profile of a model file of the node"
    );
    assert_eq!(
        section(&pool_text(), "bus-model"),
        "[bus-model]\nmodel = /models/bus-model.gguf\n",
        "the new section, at the end of the pool: {}",
        pool_text()
    );
    assert_eq!(
        std::fs::metadata(sys.join("models.ini"))
            .expect("metadata")
            .permissions()
            .mode()
            & 0o7777,
        0o644,
        "the mode of the pool file a create writes: readable by the instance"
    );
    assert_eq!(
        client.create_model_profile(
            "bus-hf",
            &ModelProfileSource::Hf {
                repo: "org/repo".into(),
                file: Some("Q4_K_M.gguf".into()),
            }
        ),
        Ok(()),
        "a profile of a Hugging Face pick with a file"
    );
    assert_eq!(
        client.create_model_profile(
            "bus-repo",
            &ModelProfileSource::Hf {
                repo: "org/other".into(),
                file: None,
            }
        ),
        Ok(()),
        "a profile of a repository with no file named"
    );
    assert_eq!(
        section(&pool_text(), "bus-hf"),
        "[bus-hf]\nhf-file = Q4_K_M.gguf\nhf-repo = org/repo\n",
        "the pick's two keys, sorted: {}",
        pool_text()
    );
    assert_eq!(
        section(&pool_text(), "bus-repo"),
        "[bus-repo]\nhf-repo = org/other\n",
        "a repository alone is one key, and no `model`: {}",
        pool_text()
    );

    // ── The clone: the source's lines, kept ────────────────────────────────
    assert_eq!(
        client.create_model_profile("bus-clone", &ModelProfileSource::From("off-one".into())),
        Ok(()),
        "a copy of a model no entry lists"
    );
    assert_eq!(
        section(&pool_text(), "bus-clone"),
        // The source's lines with its trailing blank separator left out
        // (MP1, ARCHITECTURE.md done-when): the copy ends with one ending.
        format!(
            "[bus-clone]\n{}\n",
            section(&pool, "off-one")["[off-one]\n".len()..].trim_end_matches('\n')
        ),
        "the source's comment, keys and order, under the new header, no blank at the \
         end: {}",
        pool_text()
    );
    assert_eq!(
        client.create_model_profile("bus-on-clone", &ModelProfileSource::From("On-One".into())),
        Ok(()),
        "the source of a model an entry lists: the copy joins no entry, so it serves nobody"
    );
    assert_eq!(
        section(&pool_text(), "bus-on-clone"),
        format!(
            "[bus-on-clone]\n{}\n",
            section(&pool, "On-One")["[On-One]\n".len()..].trim_end_matches('\n')
        ),
        "the source's lines, in the pool: {}",
        pool_text()
    );
    assert_eq!(
        text_of("models.ini.disabled"),
        retired,
        "the retired switched-off file keeps every byte: a create writes the pool"
    );
    assert_eq!(
        text_of("assignments.ini"),
        keys,
        "a create writes no key of the retired record"
    );
    assert_eq!(
        text_of("instance-profiles.ini"),
        PROFILES,
        "and joins no entry: a new model is in no instance's list until a page writes it"
    );
    assert_eq!(
        text_of("instances.ini"),
        RECORD,
        "and writes no instance record"
    );
    assert_eq!(
        renders(),
        renders_before,
        "no create joined an entry, so no instance's render moved"
    );

    // ── Every rule's refusal, decoded by the client ────────────────────────
    for (id, source, want, why) in [
        (
            "OFF-ONE",
            ModelProfileSource::Model("/m/x.gguf".into()),
            refused("model-profile-name-taken", &["OFF-ONE"]),
            "a name the pool holds in the other case",
        ),
        (
            "*",
            ModelProfileSource::Model("/m/x.gguf".into()),
            refused("invalid-model-profile-id", &["*"]),
            "an id of no id",
        ),
        (
            "bus-hf",
            ModelProfileSource::From("off-one".into()),
            refused("model-profile-name-taken", &["bus-hf"]),
            "an id taken by an earlier call of this phase",
        ),
        (
            "bus-nosuch",
            ModelProfileSource::From("no-such-profile".into()),
            refused("no-model-profile-named", &["no-such-profile"]),
            "a source the pool does not hold",
        ),
    ] {
        assert_eq!(
            client.create_model_profile(id, &source),
            want,
            "{why}: the refusal the client decodes"
        );
    }
    assert!(
        !pool_text().contains("[bus-nosuch]"),
        "a refused create wrote nothing: {}",
        pool_text()
    );

    // ── The delete: the section alone, out of the pool ─────────────────────
    assert_eq!(
        client.delete_model_profile("free-me"),
        Ok(()),
        "a model no entry lists"
    );
    assert_eq!(section(&pool_text(), "free-me"), "", "its section is gone");
    assert!(
        section(&pool_text(), "bus-clone").contains("a note"),
        "the neighbour's section is kept: {}",
        pool_text()
    );
    for (id, want, why) in [
        (
            "On-One",
            refused("model-profile-assigned", &["On-One", "gpu"]),
            "the entry `[gpu/On-One]` lists it",
        ),
        (
            "twin",
            refused("model-profile-assigned", &["twin", "gpu"]),
            "the entry `[gpu/twin]`, naming the profile once though two instances run it",
        ),
        (
            "TWO-PLACES",
            refused("model-profile-assigned", &["TWO-PLACES", "gpu"]),
            "the entry `[gpu/two-places]`, in the other letter case: the retired record's \
             keys are no rule",
        ),
        (
            "Retired",
            refused("no-model-profile-named", &["Retired"]),
            "a section of the retired switched-off file is no proof it exists",
        ),
        (
            "gone",
            refused("no-model-profile-named", &["gone"]),
            "a model the pool does not hold",
        ),
        (
            "*",
            refused("no-model-profile-named", &["*"]),
            "no id: the pool's refusal, not the id rule",
        ),
    ] {
        assert_eq!(client.delete_model_profile(id), want, "{why}");
    }
    assert_eq!(
        text_of("assignments.ini"),
        keys,
        "neither the create nor the delete of a free model writes a key of the retired record"
    );
    assert_eq!(
        renders(),
        renders_before,
        "and no render moved: the deleted model was in no entry, so in no render"
    );
    assert_eq!(
        asked(&c),
        vec![(alice.clone(), Action::Configure); 16],
        "every verb asked configure of the sender, once per call"
    );
    assert_eq!(
        calls(&c),
        vec![],
        "no verb of a model profile touches a unit"
    );

    // ── A caller with no sender ────────────────────────────────────────────
    let (_server, anonymous) = connect(&c, None);
    assert_eq!(
        anonymous.create_model_profile("bus-anon", &ModelProfileSource::Model("/m/a.gguf".into())),
        Err(ClientError::NotAuthorized),
        "no sender, no create"
    );
    assert_eq!(
        anonymous.delete_model_profile("bus-clone"),
        Err(ClientError::NotAuthorized),
        "no sender, no delete"
    );
    assert!(
        !pool_text().contains("[bus-anon]")
            && section(&pool_text(), "bus-clone").contains("a note"),
        "the refused calls wrote and removed nothing: {}",
        pool_text()
    );
}

/// The `As` forms of the two verbs, over D-Bus, for a client bound to a
/// member (slice MP1, its rules of IP3b): the member's create lands in the pool
/// and its delete takes the section out, an outsider's are refused with nothing
/// done, and the pair (the bff's account, a current member) decides the right:
/// polkit is asked nothing. A model an entry lists is not free, and the retired
/// record's key for it is read by nothing.
pub(crate) fn manager_dbus_client_model_profiles_as_e2e() {
    use crate::manager::service::fakes::{FakeGroup, FakePasswords};
    use pai_core::client::ModelProfileSource;

    const BFF: &str = ":1.9";
    const RECORD: &str = "[default]\nInstanceProfile = gpu\nPort = 8080\n";
    const PROFILES: &str = "[gpu]\nServerProfile = gpu\n\n[gpu/two-places]\n";

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    std::fs::write(sys.join("instances.ini"), RECORD).expect("record");
    std::fs::write(sys.join("instance-profiles.ini"), PROFILES).expect("instance profiles");
    std::fs::write(sys.join("server@gpu.ini"), "[Server]\nDevices = CUDA0\n")
        .expect("server profile");
    // The pool holds `keep`, which no entry lists, and `two-places`, which
    // `[gpu/two-places]` lists. The retired record and the retired switched-off
    // file name the same model: neither is read.
    let pool = real_model_files(
        dir.path(),
        "[keep]\nmodel = /models/keep.gguf\n\n[two-places]\nmodel = /models/two.gguf\n",
    );
    std::fs::write(sys.join("models.ini"), &pool).expect("models.ini");
    std::fs::write(
        sys.join("models.ini.disabled"),
        "[Retired]\nmodel = /models/retired.gguf\n",
    )
    .expect("the retired switched-off file");
    std::fs::write(sys.join("assignments.ini"), "[default]\ntwo-places = on\n")
        .expect("the retired record");
    std::fs::write(
        sys.join("models@default.ini"),
        pai_core::assign::render_profile(&pool, PROFILES, "gpu"),
    )
    .expect("render");
    let section = |text: &str, id: &str| -> String {
        pai_core::ini::text::section_text(text, id).unwrap_or_default()
    };
    let pool_text = || std::fs::read_to_string(sys.join("models.ini")).expect("pool");

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
    let polkit = |c: &Arc<Core>| c.authority().asked.lock().unwrap().clone();
    let bff_checks = |c: &Arc<Core>| c.authority().asked_bff.lock().unwrap().clone();
    let asked_admin = |a: &Arc<std::sync::Mutex<Vec<String>>>| a.lock().unwrap().clone();

    let (_server, client) = connect(&c, Some(BFF));
    let member = Arc::new(client.acting_for("alice"));

    assert_eq!(
        on("create_model_profile", &member, |c| c.create_model_profile(
            "alice-mp",
            &ModelProfileSource::Model("/models/alice.gguf".into())
        )),
        Ok(()),
        "a member's create, vouched for by the bff: the right is the pair, not polkit"
    );
    assert_eq!(
        section(&pool_text(), "alice-mp"),
        "[alice-mp]\nmodel = /models/alice.gguf\n",
        "the member's profile, in the pool: {}",
        pool_text()
    );
    assert_eq!(
        on("delete_model_profile", &member, |c| c
            .delete_model_profile("alice-mp")),
        Ok(()),
        "and the member's delete of it: the new model was in no entry, so it was free"
    );
    assert_eq!(section(&pool_text(), "alice-mp"), "", "it is gone");
    assert_eq!(
        on("delete_model_profile", &member, |c| c
            .delete_model_profile("two-places")),
        Err(ClientError::Invalid(Refusal {
            key: "model-profile-assigned",
            values: vec!["two-places".into(), "gpu".into()],
        })),
        "the member's delete of a model an entry lists is the rule's refusal, naming the \
         instance profile that lists it, not a permission of its own: the manager answers \
         the web's rules as the caller's own"
    );
    assert_eq!(
        on("create_model_profile", &member, |c| c.create_model_profile(
            "keep",
            &ModelProfileSource::Model("/models/again.gguf".into())
        )),
        Err(ClientError::Invalid(Refusal {
            key: "model-profile-name-taken",
            values: vec!["keep".into()],
        })),
        "a name the pool holds, for a member as for anybody"
    );

    let outsider = Arc::new(client.acting_for("bob"));
    assert_eq!(
        on("create_model_profile", &outsider, |c| c
            .create_model_profile(
                "bob-mp",
                &ModelProfileSource::Model("/models/bob.gguf".into())
            )),
        Err(ClientError::NotAuthorized),
        "bob is no member of `pai`"
    );
    assert_eq!(
        on("delete_model_profile", &outsider, |c| c
            .delete_model_profile("keep")),
        Err(ClientError::NotAuthorized),
        "the same for a delete"
    );
    assert!(
        !pool_text().contains("[bob-mp]") && section(&pool_text(), "keep").contains("keep.gguf"),
        "a refusal of the `As` form wrote and removed nothing: {}",
        pool_text()
    );
    assert_eq!(
        polkit(&c),
        vec![],
        "the principal gate decides the right: polkit is asked nothing"
    );
    assert_eq!(
        bff_checks(&c),
        vec![Caller(BFF.into()); 6],
        "the caller checked once per `As` call"
    );
    assert_eq!(
        asked_admin(&admins),
        [
            "alice".to_string(),
            "alice".into(),
            "alice".into(),
            "alice".into(),
            "bob".into(),
            "bob".into(),
        ],
        "the membership of the vouched name, asked once per call, never cached"
    );
}

/// The instance profile verbs over D-Bus, with pai-core's client (IP3a,
/// `ARCHITECTURE.md` "Instance profiles", slice IP3 part a): the frame each
/// verb sends and the answer it reads on this transport (the list's rows, the
/// single read's `(sass)`), every error name decoded into its `ClientError`
/// (`NoInstanceProfile`, `InstanceProfileExists`, `InstanceProfileInUse` with
/// the profile and the instance, `NoServerProfile`, `NoInstance`, `Changed`,
/// `AddressInUse` with who holds the address, `Invalid` with the key and the
/// values), and the bytes the accepted verbs leave. Every verb asks polkit
/// `configure` of the sender, once per call; a sender-less call is refused and
/// writes nothing.
pub(crate) fn manager_dbus_client_instance_profiles_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    let pool_seed = "[default]\nServerProfile = default\n\n[default/alpha]\ndevice = cuda0\n\n[lab]\nServerProfile = lab\n\n[lab/alpha]\n\n[unassigned]\nServerProfile = default\n";
    let record_seed = "[default]\nInstanceProfile = default\nHostname = 127.0.0.1\nPort = 8581\n\n[lab]\nInstanceProfile = lab\nHostname = 0.0.0.0\nPort = 8681\n";
    std::fs::write(sys.join("instance-profiles.ini"), pool_seed).expect("pool");
    std::fs::write(sys.join("instances.ini"), record_seed).expect("record");
    for (name, port) in [("default", 8581), ("lab", 8681), ("edge", 8700)] {
        std::fs::write(
            sys.join(format!("server@{name}.ini")),
            format!("[Server]\nHostname = 127.0.0.1\nPort = {port}\n"),
        )
        .expect("server profile");
    }
    std::fs::write(
        sys.join("models.ini"),
        real_model_files(
            dir.path(),
            "[alpha]\nmodel = /models/alpha.gguf\n\n[beta]\nmodel = /models/beta.gguf\n\n[gamma]\nmodel = /models/gamma.gguf\n",
        )
    )
    .expect("models.ini");
    std::fs::write(sys.join("models.ini.disabled"), "").expect("models.ini.disabled");
    for id in ["default", "lab"] {
        std::fs::write(sys.join(format!("models@{id}.ini")), "").expect("render");
    }

    let pool = || std::fs::read_to_string(sys.join("instance-profiles.ini")).expect("pool");
    let record = || std::fs::read_to_string(sys.join("instances.ini")).expect("record");
    let key_of = |text: &str, section: &str, key: &str| -> Option<String> {
        pai_core::ini::section_keys(text, section)
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    };
    let sections = |text: &str| -> Vec<String> {
        pai_core::ini::parse_sections(text)
            .into_iter()
            .map(|s| s.id)
            .collect()
    };
    let refused = |key: &'static str, values: &[&str]| -> Result<(), ClientError> {
        Err(ClientError::Invalid(Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        }))
    };

    let c = core(true, &[]);
    let (_server, client) = connect(&c, Some(":1.42"));
    let alice = Caller(":1.42".into());

    // ── The reads: the list, and one profile with the pool's text ─────────
    assert_eq!(
        client
            .instance_profiles()
            .expect("instance_profiles")
            .into_iter()
            .map(|p| (p.id, p.server_profile, p.entries, p.revision))
            .collect::<Vec<_>>(),
        vec![
            (
                "default".to_string(),
                "default".to_string(),
                vec!["alpha".to_string()],
                String::new()
            ),
            (
                "lab".to_string(),
                "lab".to_string(),
                vec!["alpha".to_string()],
                String::new()
            ),
            (
                "unassigned".to_string(),
                "default".to_string(),
                Vec::new(),
                String::new()
            ),
        ],
        "the list's rows cross as the id, the server profile and the entries, in \
         the file's order, with no revision: the pool is one text"
    );
    let one = client
        .instance_profile("default")
        .expect("instance_profile");
    assert_eq!(
        (
            one.id.as_str(),
            one.server_profile.as_str(),
            one.entries.as_slice()
        ),
        ("default", "default", &["alpha".to_string()][..]),
        "the read answers the profile it was asked for"
    );
    assert_eq!(
        one.revision, pool_seed,
        "the revision crosses as the third member of the tuple: the text of \
         `instance-profiles.ini` the read returned"
    );
    assert_eq!(
        client.instance_profile("nope"),
        Err(ClientError::NoInstanceProfile("nope".into())),
        "the name's refusal crosses and is decoded with the profile"
    );

    // ── The create: the refusals first, the pool untouched ────────────────
    assert_eq!(
        client.create_instance_profile("Bad Id", "lab"),
        refused("instance-profile-bad-id", &["Bad Id"]),
        "an id the rule refuses, decoded as the keyed refusal"
    );
    assert_eq!(
        client.create_instance_profile("default", "lab"),
        Err(ClientError::InstanceProfileExists("default".into())),
        "a name the pool has, decoded with the name"
    );
    assert_eq!(
        client.create_instance_profile("edge", "nope"),
        Err(ClientError::NoServerProfile("nope".into())),
        "no server profile of that name"
    );
    assert_eq!(pool(), pool_seed, "every refused create wrote the pool");
    assert_eq!(record(), record_seed, "and no record");
    assert_eq!(
        client.create_instance_profile("edge", "lab"),
        Ok(()),
        "the create of a new name lands"
    );
    assert_eq!(
        pool(),
        pai_core::instance_profiles::add_profile(pool_seed, "edge", "lab"),
        "the section is appended, every byte before it the file's own"
    );

    // ── The entry add and remove ──────────────────────────────────────────
    assert_eq!(
        client.add_instance_profile_model("default", "nope"),
        refused("no-model-profile-named", &["nope"]),
        "an entry of no model profile, decoded as the keyed refusal"
    );
    let before = pool();
    assert_eq!(
        client.add_instance_profile_model("default", "beta"),
        Ok(()),
        "the entry add lands"
    );
    assert_eq!(
        pool(),
        pai_core::instance_profiles::add_entry(&before, "default", "beta"),
        "the empty entry section is appended"
    );
    let render = std::fs::read_to_string(sys.join("models@default.ini")).expect("render");
    assert!(
        sections(&render).contains(&"beta".to_string()),
        "the entry add re-renders the instance that runs the profile: {:?}",
        sections(&render)
    );
    let before = pool();
    assert_eq!(
        client.remove_instance_profile_model("default", "alpha"),
        Ok(()),
        "the entry removal lands"
    );
    assert_eq!(
        pool(),
        pai_core::ini::text::delete_section(&before, "default/alpha")
            .expect("the entry the pool held"),
        "the entry section is cut out, every other byte kept"
    );

    // ── `SetInstanceProfile`: the record's pointer, the pool's text ───────
    let rev = pool();
    assert_eq!(
        client.set_instance_profile("ghost", "default", &rev),
        Err(ClientError::NoInstance),
        "an instance the record has no section for"
    );
    assert_eq!(
        client.set_instance_profile("lab", "nope", &rev),
        Err(ClientError::NoInstanceProfile("nope".into())),
        "a profile the pool has no section for"
    );
    assert_eq!(
        client.set_instance_profile("lab", "default", "STALE TEXT"),
        Err(ClientError::Changed),
        "a revision the pool no longer holds"
    );
    assert_eq!(
        record(),
        record_seed,
        "every refused switch wrote no record"
    );
    assert_eq!(
        client.set_instance_profile("lab", "default", &rev),
        Ok(()),
        "the switch of `lab` to `default` lands"
    );
    let rec = record();
    assert_eq!(
        (
            key_of(&rec, "lab", "InstanceProfile").as_deref(),
            key_of(&rec, "lab", "Port").as_deref(),
            key_of(&rec, "default", "InstanceProfile").as_deref(),
        ),
        (Some("default"), Some("8681"), Some("default")),
        "the section names the new profile, keeps its address and leaves every \
         other section alone"
    );

    // ── `SetInstanceAddress`: the record's address, the record's text ─────
    assert_eq!(
        client.set_instance_address("default", "127.0.0.1", "8681", &rec),
        Err(ClientError::AddressInUse("lab".into())),
        "the address another instance holds, naming it"
    );
    assert_eq!(
        client.set_instance_address("default", "127.0.0.1", "8590", &rec),
        Err(ClientError::AddressInUse("private-ai-bff".into())),
        "the web service's address, named as its own"
    );
    // The port travels as text: the check answers its key, with the key's name
    // and the value (the bounds are the check's, not pinned here).
    match client.set_instance_address("default", "127.0.0.1", "not-a-port", &rec) {
        Err(ClientError::Invalid(r)) => {
            assert_eq!(r.key, "not-whole-number-in-range", "{r:?}");
            assert_eq!(
                (
                    r.values.first().map(String::as_str),
                    r.values.get(1).map(String::as_str)
                ),
                (Some("Port"), Some("not-a-port")),
                "the refusal names the key and the value: {r:?}"
            );
        }
        other => panic!("a port that is no number is refused as {other:?}"),
    }
    assert_eq!(record(), rec, "every refused address wrote no record");
    assert_eq!(
        client.set_instance_address("default", "127.0.0.1", "8595", &rec),
        Ok(()),
        "a free address lands"
    );
    let rec = record();
    assert_eq!(
        (
            key_of(&rec, "default", "Hostname").as_deref(),
            key_of(&rec, "default", "Port").as_deref(),
            key_of(&rec, "lab", "Port").as_deref(),
        ),
        (Some("127.0.0.1"), Some("8595"), Some("8681")),
        "the section carries the address, every other section kept"
    );

    // ── The delete: the profile an instance runs first ────────────────────
    assert_eq!(
        client.delete_instance_profile("default"),
        Err(ClientError::InstanceProfileInUse(
            "default".into(),
            "default".into()
        )),
        "the profile an instance runs: the two names cross in one string and the \
         client splits them back"
    );
    let before = pool();
    let rec_before = record();
    assert_eq!(
        client.delete_instance_profile("unassigned"),
        Ok(()),
        "a profile nobody runs is removed"
    );
    assert_eq!(
        pool(),
        pai_core::ini::text::delete_section(&before, "unassigned")
            .expect("the profile the pool held"),
        "the section is cut out, every other byte kept"
    );
    assert_eq!(record(), rec_before, "the delete writes no record");
    assert!(
        client
            .instance_profiles()
            .expect("instance_profiles")
            .iter()
            .all(|p| p.id != "unassigned"),
        "the list no longer answers the removed profile"
    );

    assert_eq!(
        asked(&c),
        vec![(alice.clone(), Action::Configure); 21],
        "every verb asked configure of the sender, once per call, the reads too"
    );
    // IP3c: every verb of this phase answers the apply, whose drift compare asks the
    // state of the instances whose pair the write moved. The number of probes is the
    // pass's (which instances drift is the render's business, pinned in
    // `src/tests/convergence.rs`); what this phase pins is that the record's verbs,
    // the entry writes, the reads and every refused write reach the service manager
    // ONLY to read a state: no unit verb, whatever the drift.
    let unit_verbs = calls(&c)
        .into_iter()
        .filter(|call| !matches!(call, Call::Status(_)))
        .collect::<Vec<_>>();
    assert_eq!(
        unit_verbs,
        vec![],
        "the instance profile verbs and the reads touch no unit: only state probes, no \
         Start, Stop, Restart, boot read or boot write"
    );

    // ── A caller with no sender ───────────────────────────────────────────
    let (_server, anonymous) = connect(&c, None);
    for (what, answer) in [
        (
            "instance_profiles",
            anonymous.instance_profiles().map(|_| ()),
        ),
        (
            "instance_profile",
            anonymous.instance_profile("default").map(|_| ()),
        ),
        (
            "create_instance_profile",
            anonymous.create_instance_profile("bus-no", "lab"),
        ),
        (
            "delete_instance_profile",
            anonymous.delete_instance_profile("edge"),
        ),
        (
            "add_instance_profile_model",
            anonymous.add_instance_profile_model("default", "gamma"),
        ),
        (
            "remove_instance_profile_model",
            anonymous.remove_instance_profile_model("default", "beta"),
        ),
        (
            "set_instance_profile",
            anonymous.set_instance_profile("lab", "edge", pool_seed),
        ),
        (
            "set_instance_address",
            anonymous.set_instance_address("lab", "127.0.0.1", "8699", record_seed),
        ),
        (
            "create_instance",
            anonymous.create_instance("llama.cpp", "bus-no", "edge"),
        ),
    ] {
        assert_eq!(answer, Err(ClientError::NotAuthorized), "{what}");
    }
    assert!(
        !pool().contains("[bus-no]") && pool().contains("[edge]"),
        "the refused calls wrote and removed nothing: {}",
        pool()
    );
    assert!(
        !record().contains("[bus-no]")
            && key_of(&record(), "lab", "Port").as_deref() == Some("8681"),
        "and wrote no record: {}",
        record()
    );
}

/// The `As` forms of the instance profile verbs, over D-Bus, for a client
/// bound to a member (IP3a): the member's pool write lands, its switch of an
/// instance lands, an outsider's are refused with nothing done, and the pair
/// (the bff's account, a current member) decides the right: polkit is asked
/// nothing. The two reads have no `As` form, as `ServerProfiles` has none: the
/// bff reads the node as itself, so those two ask polkit.
pub(crate) fn manager_dbus_client_instance_profiles_as_e2e() {
    use crate::manager::service::fakes::FakePasswords;

    const BFF: &str = ":1.9";

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    let pool_seed = "[lab]\nServerProfile = lab\n\n[unassigned]\nServerProfile = lab\n";
    let record_seed = "[default]\nInstanceProfile = lab\nHostname = 127.0.0.1\nPort = 8581\n";
    std::fs::write(sys.join("instance-profiles.ini"), pool_seed).expect("pool");
    std::fs::write(sys.join("instances.ini"), record_seed).expect("record");
    std::fs::write(
        sys.join("server@lab.ini"),
        "[Server]\nHostname = 127.0.0.1\nPort = 8581\n",
    )
    .expect("server profile");
    std::fs::write(
        sys.join("models.ini"),
        real_model_files(dir.path(), "[gamma]\nmodel = /models/gamma.gguf\n"),
    )
    .expect("models.ini");
    std::fs::write(sys.join("models.ini.disabled"), "").expect("models.ini.disabled");
    std::fs::write(sys.join("models@default.ini"), "").expect("render");
    // The launch of `default` recorded the pair standing now (IP3c), the render
    // the fixture wrote: every write below moves that pair, so each accepted one
    // answers an apply with one state probe of this instance.
    crate::manager::service::write_applied("default").expect("the applied record");

    let pool_file = sys.join("instance-profiles.ini");
    let record_file = sys.join("instances.ini");
    let pool = move || std::fs::read_to_string(&pool_file).expect("pool");
    let record = move || std::fs::read_to_string(&record_file).expect("record");
    let key_of = |text: &str, section: &str, key: &str| -> Option<String> {
        pai_core::ini::section_keys(text, section)
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    };

    let passwords = FakePasswords::new();
    let admins = passwords.asked_admin.clone();
    let (c, _checked) = auth_core(BFF, passwords);
    let polkit = |c: &Arc<Core>| c.authority().asked.lock().unwrap().clone();
    let bff_checks = |c: &Arc<Core>| c.authority().asked_bff.lock().unwrap().clone();
    let asked_admin = |a: &Arc<std::sync::Mutex<Vec<String>>>| a.lock().unwrap().clone();

    let (_server, client) = connect(&c, Some(BFF));
    let member = Arc::new(client.acting_for("alice"));

    assert_eq!(
        on("create_instance_profile", &member, |c| c
            .create_instance_profile("alice-ip", "lab")),
        Ok(()),
        "a member's create, vouched for by the bff: the right is the pair, not polkit"
    );
    assert!(
        pool().contains("[alice-ip]\nServerProfile = lab\n"),
        "the profile the member's create wrote: {}",
        pool()
    );
    assert_eq!(
        on("add_instance_profile_model", &member, |c| c
            .add_instance_profile_model("alice-ip", "gamma")),
        Ok(()),
        "the member's entry add"
    );
    assert!(
        pool().contains("[alice-ip/gamma]"),
        "the entry the member added: {}",
        pool()
    );
    let rev = pool();
    assert_eq!(
        on("set_instance_profile", &member, move |c| c
            .set_instance_profile("default", "alice-ip", &rev)),
        Ok(()),
        "the member's switch of the instance onto its profile"
    );
    assert_eq!(
        key_of(&record(), "default", "InstanceProfile").as_deref(),
        Some("alice-ip"),
        "the record points at the member's profile"
    );
    assert_eq!(
        on("delete_instance_profile", &member, |c| c
            .delete_instance_profile("alice-ip")),
        Err(ClientError::InstanceProfileInUse(
            "alice-ip".into(),
            "default".into()
        )),
        "the member's delete of the profile the instance now runs is the rule's \
         refusal, not a permission of its own"
    );
    let rec = record();
    assert_eq!(
        on("set_instance_address", &member, move |c| c
            .set_instance_address("default", "127.0.0.1", "8595", &rec)),
        Ok(()),
        "the member's address write"
    );
    assert_eq!(
        (
            key_of(&record(), "default", "Hostname").as_deref(),
            key_of(&record(), "default", "Port").as_deref()
        ),
        (Some("127.0.0.1"), Some("8595")),
        "the record carries the address the member asked for"
    );
    assert_eq!(
        on("remove_instance_profile_model", &member, |c| c
            .remove_instance_profile_model("alice-ip", "gamma")),
        Ok(()),
        "the member's entry removal"
    );
    assert!(
        !pool().contains("[alice-ip/gamma]"),
        "the entry is gone: {}",
        pool()
    );

    let outsider = Arc::new(client.acting_for("bob"));
    assert_eq!(
        on("create_instance_profile", &outsider, |c| c
            .create_instance_profile("bob-ip", "lab")),
        Err(ClientError::NotAuthorized),
        "bob is no member of `pai`"
    );
    let rev = pool();
    assert_eq!(
        on("set_instance_profile", &outsider, move |c| c
            .set_instance_profile("default", "lab", &rev)),
        Err(ClientError::NotAuthorized),
        "the same for a switch"
    );
    assert!(
        !pool().contains("[bob-ip]")
            && key_of(&record(), "default", "InstanceProfile").as_deref() == Some("alice-ip"),
        "a refusal of the `As` form wrote and removed nothing: {}",
        pool()
    );

    // The reads, answered to the bff's own account.
    let one = client
        .instance_profile("alice-ip")
        .expect("instance_profile");
    assert_eq!(
        (
            one.id.as_str(),
            one.server_profile.as_str(),
            one.entries.as_slice()
        ),
        ("alice-ip", "lab", &[] as &[String]),
        "the bff's read answers the profile"
    );
    assert_eq!(one.revision, pool(), "with the pool's text as its revision");
    assert_eq!(
        client
            .instance_profiles()
            .expect("instance_profiles")
            .into_iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        vec![
            "lab".to_string(),
            "unassigned".to_string(),
            "alice-ip".to_string()
        ],
        "the list answers the profiles the pool holds, in the file's order"
    );

    assert_eq!(
        polkit(&c),
        vec![(Caller(BFF.into()), Action::Configure); 2],
        "only the two reads ask polkit, of the bff: an `As` call asks nothing"
    );
    assert_eq!(
        bff_checks(&c),
        vec![Caller(BFF.into()); 8],
        "the caller checked once per `As` call"
    );
    assert_eq!(
        asked_admin(&admins),
        [
            "alice".to_string(),
            "alice".into(),
            "alice".into(),
            "alice".into(),
            "alice".into(),
            "alice".into(),
            "bob".into(),
            "bob".into(),
        ],
        "the membership of the vouched name, asked once per call, never cached"
    );
    // IP3c: every accepted write of this phase answers the apply, whose drift compare asks
    // the state of `default`, the one instance whose pair these writes move (the switch of
    // the record included). The count is the accepted writes', one probe each; the shape
    // that stays pinned is that nothing reaches a unit verb.
    assert_eq!(
        calls(&c),
        vec![Call::Status("default".into()); 5],
        "IP3c: each accepted write answers the apply with a state probe of the instance \
         whose pair it moves; the refused ones ask nothing, and nothing reaches a unit verb"
    );
}

/// M3 of "Choosing the model file on the web": `ModelFiles` / `ModelFilesAs`
/// over the bus, with pai-core's client. The frame the picker reads crosses:
/// the folder as an absolute path, every row with its path, size, shard count
/// and category, the flag saying the walk stopped at its bound, and the
/// folder's keyed why INSIDE a success. The bus has no null, so a row's
/// category travels with its flag and the why with its key: those two crossings
/// are what this phase pins, the listing's rules are pinned by
/// `manager_model_files_e2e`. The read opens a folder under this phase's
/// redirect, so it lives in the e2e.
pub(crate) fn manager_dbus_client_model_files_e2e() {
    use crate::manager::service::fakes::{FakeGroup, FakePasswords};
    use pai_core::client::{ModelFileRow, Reason};

    const BFF: &str = ":1.9";
    const OTHER: &str = ":1.42";

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let models = crate::paths::instance_models_dir();
    std::fs::create_dir_all(&models).expect("models dir");
    let file = |rel: &str, bytes: usize| {
        let p = models.join(rel);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("parent dir");
        std::fs::write(&p, vec![b'g'; bytes]).expect("file");
    };
    file("loose.gguf", 5);
    file("models/big-00001-of-00003.gguf", 1000);
    file("models/big-00002-of-00003.gguf", 1000);
    file("mmprojs/mm.GGUF", 30);
    file("notes.txt", 9);
    file(".hf-staging/staged.gguf", 99);

    let core = |quiet: bool, entries: usize| {
        Arc::new(
            Manager::new(
                FakeService::new(),
                FakeAuthority::new(true)
                    .quietly(quiet)
                    .bff(&Caller(BFF.into())),
                FakeResolver::lan(),
            )
            .with_group(FakeGroup::new())
            .with_passwords(FakePasswords::new())
            .with_walk_max_entries(entries),
        )
    };
    let row = |rel: &str, size: u64, shards: u64, category: Option<&str>| ModelFileRow {
        path: models.join(rel).to_string_lossy().into_owned(),
        size,
        shards,
        category: category.map(str::to_string),
    };
    let sorted = |rows: Vec<ModelFileRow>| -> Vec<ModelFileRow> {
        let mut rows = rows;
        rows.sort_by(|a, b| a.path.cmp(&b.path));
        rows
    };
    let want = || {
        sorted(vec![
            row("loose.gguf", 5, 1, None),
            row("mmprojs/mm.GGUF", 30, 1, Some("mmprojs")),
            row("models/big-00001-of-00003.gguf", 1000, 3, Some("models")),
        ])
    };
    let frame = |got: pai_core::client::ModelFilesReply| {
        (got.dir, sorted(got.files), got.truncated, got.error)
    };

    // ── The bff's own read: the frame crosses whole ────────────────────────
    let c = core(true, 50_000);
    let (_server, client) = connect(&c, Some(BFF));
    let client = Arc::new(client);
    let got = on("model_files", &client, |c| c.model_files()).expect("the bff's own read");
    assert_eq!(
        frame(got),
        (models.to_string_lossy().into_owned(), want(), false, None),
        "the folder, the three rows with every field (the second shard, the txt \
         and the dot-folder left out), the flag false and no why"
    );
    assert_eq!(
        *c.authority().asked_quietly.lock().unwrap(),
        vec![(Caller(BFF.into()), Action::Control)],
        "control asked without interaction, the way `NodeInfo` asks it"
    );
    assert_eq!(
        *c.authority().asked.lock().unwrap(),
        Vec::<(Caller, Action)>::new(),
        "never asked with a prompt: a file list is worth no challenge"
    );
    assert_eq!(
        *c.service().calls.lock().unwrap(),
        Vec::<Call>::new(),
        "the read reaches no service manager"
    );

    // ── The member's read the bff vouches for: the same frame ──────────────
    let member = Arc::new(client.acting_for("alice"));
    assert_eq!(
        on("model_files as alice", &member, |c| c.model_files()).map(frame),
        Ok((models.to_string_lossy().into_owned(), want(), false, None)),
        "the member's read, vouched for by the bff: the same rows"
    );

    // ── A non-pai principal: refused, no frame ─────────────────────────────
    let outsider = Arc::new(client.acting_for("bob"));
    assert_eq!(
        on("model_files as bob", &outsider, |c| c.model_files()),
        Err(ClientError::NotAuthorized),
        "bob is no member of `pai`: no paths of the node's models folder"
    );
    assert_eq!(
        *c.authority().asked_quietly.lock().unwrap(),
        vec![(Caller(BFF.into()), Action::Control)],
        "the principal gate decides the right: polkit is asked nothing more"
    );

    // ── A caller that is not the bff, vouching for a member: refused ───────
    let c = core(true, 50_000);
    let (_server, client) = connect(&c, Some(OTHER));
    let client = Arc::new(client);
    assert_eq!(
        on("model_files as alice", &client, |c| c
            .acting_for("alice")
            .model_files()),
        Err(ClientError::NotAuthorized),
        "only the bff's account vouches"
    );

    // ── A caller polkit refuses the quiet check: refused ───────────────────
    let c = core(false, 50_000);
    let (_server, client) = connect(&c, Some(OTHER));
    let client = Arc::new(client);
    assert_eq!(
        on("model_files", &client, |c| c.model_files()),
        Err(ClientError::NotAuthorized),
        "no control, no list"
    );

    // ── The flag: a walk that stops at its bound says so across the bus ────
    let c = core(true, 2);
    let (_server, client) = connect(&c, Some(BFF));
    let client = Arc::new(client);
    let got = on("model_files bounded", &client, |c| c.model_files()).expect("served");
    assert!(
        got.truncated,
        "the walk stopped after 2 entries of the 5 the folder holds: {got:?}"
    );
    assert!(
        got.files.len() <= 2,
        "the list is cut with the flag: {}",
        got.files.len()
    );

    // ── A folder that is not there: the keyed why inside a success ─────────
    std::fs::remove_dir_all(&models).expect("remove models dir");
    assert_eq!(
        on("model_files gone", &client, |c| c.model_files()),
        Ok(pai_core::client::ModelFilesReply {
            dir: models.to_string_lossy().into_owned(),
            files: Vec::new(),
            truncated: false,
            error: Some(Reason {
                key: "models-dir-unreadable".into(),
                args: vec![models.to_string_lossy().into_owned()],
            }),
        }),
        "the bus carries the why as the (key, args) pair, an empty key being the \
         folder having read: the picker shows an empty list with a text"
    );
}

/// E2E phase: IP3c review 6. The states of the apply reach every transport:
/// `Instances` keeps its row signature (here `(sssqasas)` with the label)
/// beside the sibling method `InstanceStates`, one row
/// `(s id, b converged, s reason key, as reason args, b pendingRestart)` per
/// instance, the reason `{key, args}` the pipe's rows carry, and pai-core's
/// client merges the rows into `instances()`. The fixture: `desk` launched
/// with a pair that is not the one on disk (drift), and `lab` whose render a
/// device outside its server profile stopped (not converged, with the keyed
/// reason and its args).
#[cfg(target_os = "linux")]
pub(crate) fn manager_dbus_client_convergence_e2e() {
    use crate::manager::dbus::INTERFACE;
    use crate::manager::service::fakes::FakeGpus;
    use crate::manager::service::write_gguf_kv;
    use pai_core::client::dbus::BUS_NAME;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");
    let model = dir.path().join("alpha.gguf");
    write_gguf_kv(&model, "llama", &[("llama.context_length", 1024)], &[]);
    let write = |name: &str, text: &str| {
        std::fs::write(sys.join(name), text).expect(name);
    };
    // The pool carries one model, `alpha`, whose file is there. `desk` runs its
    // own profile, whose entry names `alpha` alone: it renders and fits. `lab`
    // runs instance profile `one`, whose entry names `CUDA1` while `lab`'s
    // server profile names `CUDA0` alone: `lab`'s render fails the device fit
    // and keeps nothing. The record is the migration's own shape (every section
    // names its instance profile), so the start's migration leaves it, and with
    // it `lab` on the profile the fixture gave it.
    write(
        "models.ini",
        &format!("[alpha]\nmodel = {}\n", model.to_string_lossy()),
    );
    write(
        "instances.ini",
        "[desk]\nInstanceProfile = desk\nPort = 8581\n\n[lab]\nInstanceProfile = one\nPort = 8681\n",
    );
    write(
        "instance-profiles.ini",
        "[desk]\nServerProfile = desk\n\n[desk/alpha]\n\n[one]\nServerProfile = lab\n\n\
         [one/alpha]\ndevice-draft = CUDA1\n",
    );
    write(
        "server@desk.ini",
        "[Server]\nHostname = 127.0.0.1\nPort = 8581\nDevices = CUDA0\n",
    );
    write(
        "server@lab.ini",
        "[Server]\nHostname = 127.0.0.1\nPort = 8681\nDevices = CUDA0\n",
    );
    // `desk` runs: its drift is read against the state of its unit. The cards
    // are roomy: the story is the device list, not memory.
    let mut service = FakeService::new();
    service.running = vec!["desk".to_string()];
    let c = Arc::new(
        Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
            .with_group(crate::manager::service::fakes::FakeGroup::new())
            .with_gpus(FakeGpus::new(&[("CUDA0", 400_000), ("CUDA1", 400_000)])),
    );
    c.start_up();

    // The pair on disk is not the pair `desk` launched with: its record names
    // the digest of a pair nobody launched with (IP3c review 4). `lab` is
    // stopped and recorded no pair at all (its render never landed): the drift
    // is a compare of files, and a pair no launch ever recorded moves for
    // nobody, so its row shows none.
    let state = crate::paths::instance_state_dir().join("desk");
    std::fs::create_dir_all(&state).expect("the state folder");
    std::fs::write(
        state.join("applied"),
        "0000000000000000000000000000000000000000000000000000000000000000",
    )
    .expect("the applied record");

    let (_server, bus) = connect(&c, Some(":1.42"));
    let client = Arc::new(bus);

    // ── The sibling method, called by name ───────────────────────────────
    let reply = client
        .connection()
        .call_method(
            Some(BUS_NAME),
            OBJECT_PATH,
            Some(INTERFACE),
            "InstanceStates",
            &(),
        )
        .expect("IP3c review 6: the bus answers `InstanceStates`");
    let states: Vec<(String, bool, String, Vec<String>, bool)> = reply
        .body()
        .deserialize()
        .expect("IP3c review 6: `InstanceStates` is one `a(sbssasb)` of `(s id, b \\\n         converged, s reason key, as reason args, b pendingRestart)`");
    let find = |id: &str| {
        states
            .iter()
            .find(|(row, ..)| row == id)
            .unwrap_or_else(|| panic!("IP3c review 6: the rows carry {id}: {states:?}"))
    };
    assert_eq!(
        (find("desk").1, find("desk").2.as_str(), find("desk").4),
        (true, "", true),
        "IP3c review 6: `desk` converged, and the running instance whose pair on disk is \
         not the pair it launched with awaits a restart: {states:?}"
    );
    assert!(
        find("desk").3.is_empty(),
        "IP3c review 6: a converged row carries no reason: {states:?}"
    );
    let lab = find("lab");
    assert_eq!(
        (lab.1, lab.2.as_str(), lab.4),
        (false, "model-profile-device-outside", false),
        "IP3c review 6: the stopped render answers not converged with the device \
         check's key, and shows no drift: its pair is the one its launch recorded: \
         {states:?}"
    );
    let wire: Vec<&str> = lab.3.iter().map(String::as_str).collect();
    assert!(
        wire.contains(&"lab") && wire.contains(&"CUDA1"),
        "IP3c review 6: the row carries the reason's args, naming the instance and \
         the card: {states:?}"
    );

    // ── The client merges the rows into `instances()` ────────────────────
    let rows = on("instances", &client, |c| c.instances()).expect("IP3c review 6: served");
    let row = |id: &str| {
        rows.iter()
            .find(|i| i.id == id)
            .unwrap_or_else(|| panic!("IP3c review 6: the list carries {id}: {rows:?}"))
            .clone()
    };
    assert_eq!(
        (row("desk").converged, row("desk").pending_restart),
        (true, true),
        "IP3c review 6: the client's rows carry the drift the sibling method answered"
    );
    assert!(
        row("desk").reason.is_none(),
        "IP3c review 6: the converged row carries no reason: {:?}",
        row("desk")
    );
    let lab = row("lab");
    assert!(
        !lab.converged && !lab.pending_restart,
        "IP3c review 6: the client's `lab` reads not converged, and not drifting: {lab:?}"
    );
    assert_eq!(
        lab.reason.as_ref().map(|r| r.key.as_str()),
        Some("model-profile-device-outside"),
        "IP3c review 6: the client's row carries the keyed reason"
    );
    let named: Vec<&str> = lab
        .reason
        .as_ref()
        .map(|r| r.args.iter().map(String::as_str).collect())
        .unwrap_or_default();
    assert!(
        named.contains(&"lab") && named.contains(&"CUDA1"),
        "IP3c review 6: the reason names the instance and the card the check stopped \
         on: {named:?}"
    );
    assert_eq!(
        (row("desk").port, row("desk").server_profile.as_deref()),
        (8581, Some("desk")),
        "IP3c review 6: the sibling method leaves the `Instances` rows as they are: the \
         record's own fields ride the old signature"
    );
}
