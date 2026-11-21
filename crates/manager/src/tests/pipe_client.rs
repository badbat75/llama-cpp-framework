//! pai-core's stream client of the manager (`pai_core::client::pipe`)
//! against the real server (`manager::pipe::serve_stream`) over the core's
//! fakes, joined by a Unix socket pair: the named pipe's OS-neutral half.
//! Every client stream has a read deadline: a server that panics or hangs
//! fails the test, not hangs it.

use crate::manager::pipe::serve_stream;
use crate::manager::service::fakes::{Call, FakeAuthority, FakeResolver, FakeService};
use crate::manager::service::{
    plain, real_model_files, withheld, Action, Caller, Manager, UnitState,
};
use pai_core::checks::Refusal;
use pai_core::client::pipe::PipeClient;
use pai_core::client::{ClientError, InstanceState, ManagerClient};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

type Core = Manager<FakeService, FakeAuthority, FakeResolver>;

/// The longest a client waits for a reply.
const DEADLINE: Duration = Duration::from_secs(10);

/// `core` served as `caller` on one end of a socket pair, on its own
/// thread; pai-core's client on the other end. The server's thread ends
/// with the connection.
fn connect(
    core: &Arc<Core>,
    caller: &Caller,
) -> (JoinHandle<std::io::Result<()>>, PipeClient<UnixStream>) {
    let (s0, s1) = UnixStream::pair().expect("socket pair");
    s1.set_read_timeout(Some(DEADLINE)).expect("read timeout");
    let core = core.clone();
    let caller = caller.clone();
    let server = std::thread::spawn(move || serve_stream(&core, caller, s0));
    (server, PipeClient::new(s1))
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

/// A core whose units are seen `state`, except the ids in `running`, seen
/// active, with the ids in `failing` failing every call.
fn core_units(state: UnitState, allow: bool, failing: &[&str], running: &[&str]) -> Arc<Core> {
    let mut service = FakeService::new().running(running);
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

/// The client's connection closed: the server's thread ends cleanly.
fn hang_up(server: JoinHandle<std::io::Result<()>>, client: PipeClient<UnixStream>) {
    drop(client);
    let deadline = std::time::Instant::now() + DEADLINE;
    while !server.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "the server did not end with its connection"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let result = server.join().expect("the server thread panicked");
    assert!(
        result.is_ok(),
        "a client that hangs up is a clean end: {result:?}"
    );
}

/// E2E phase for the stream client against the server, for this crate's
/// e2e test to call: it sets the process-wide `PRIVATE_AI_DATA_ROOT`
/// redirect (`src/tests/mod.rs`).
pub(crate) fn manager_pipe_client_e2e() {
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
    // Opaque to the core; shaped like the Windows user SIDs the pipe's
    // client token will give.
    let alice = Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into());
    let bob = Caller("S-1-5-21-3623811015-3361044348-30300820-1014".into());

    // ── A connection with no identity: every change refused, nobody asked
    {
        let c = core(true, &[]);
        let (server, client) = connect(&c, &Caller(String::new()));
        assert_eq!(client.start("default"), Err(ClientError::NotAuthorized));
        assert_eq!(client.stop("default"), Err(ClientError::NotAuthorized));
        assert_eq!(client.restart("default"), Err(ClientError::NotAuthorized));
        assert_eq!(
            client.write_file("models.ini", "[x]\nmodel = /m/x.gguf\n", false),
            Err(ClientError::NotAuthorized)
        );
        // The compare and swap write is the same guarded call: no identity,
        // no write, nobody asked.
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
        // `Instances` is answered with no identity too, and its rows name the
        // server profile of each instance: the client reads the server file as
        // `server@<server_profile>.ini`, which is not `server@<id>.ini`.
        assert_eq!(
            client
                .instances()
                .expect("the rows")
                .iter()
                .map(|i| (i.id.as_str(), i.server_profile.as_deref(), i.port))
                .collect::<Vec<_>>(),
            vec![
                ("broken", Some("broken"), 8090),
                ("default", Some("default"), 8080),
                ("lan-2", Some("big"), 0),
            ],
            "each row names the profile its section names, the port read from that \
             profile's file: lan-2's profile has no file, so port 0"
        );
        assert_eq!(
            client.read_file("models.ini"),
            Ok(Some(plain(&model_profiles)))
        );
        assert_eq!(
            client.read_file("instances.ini"),
            Ok(Some(plain(record))),
            "the instance record, read with no identity, as assignments.ini"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("models.ini")).unwrap(),
            model_profiles,
            "a refused write leaves the file"
        );
        // Start at boot: the change needs an identity, the read none.
        assert_eq!(
            client.set_boot_enabled("default", true),
            Err(ClientError::NotAuthorized)
        );
        assert_eq!(client.boot_enabled("default"), Ok(false));
        assert_eq!(asked(&c), vec![], "nobody asked");
        hang_up(server, client);
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
            let (server, client) = connect(&c, &alice);
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
            hang_up(server, client);
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
            let (server, client) = connect(&c, &alice);
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
            hang_up(server, client);
        }
        std::fs::remove_file(sys.join("server@keyed.ini")).expect("remove keyed");
    }

    // ── A caller the authority refuses: asked as the connection's caller ─
    {
        let c = core(false, &[]);
        let (server, client) = connect(&c, &alice);
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
            model_profiles
        );
        // Start at boot: refused under configure, nothing changed.
        assert_eq!(
            client.set_boot_enabled("default", true),
            Err(ClientError::NotAuthorized)
        );
        assert_eq!(asked(&c)[3..], [(alice.clone(), Action::Configure)]);
        assert_eq!(calls(&c), vec![], "a refused caller enables nothing");
        assert_eq!(client.boot_enabled("default"), Ok(false));
        hang_up(server, client);
    }

    // ── Two connections on one core: each asks as its own caller ─────────
    {
        let c = core(true, &[]);
        let (server_a, client_a) = connect(&c, &alice);
        let (server_b, client_b) = connect(&c, &bob);
        assert_eq!(client_a.start("default"), Ok(()));
        assert_eq!(client_b.stop("default"), Ok(()));
        assert_eq!(client_a.restart("default"), Ok(()));
        assert_eq!(
            asked(&c),
            vec![
                (alice.clone(), Action::Control),
                (bob.clone(), Action::Control),
                (alice.clone(), Action::Control)
            ]
        );
        assert_eq!(
            calls(&c),
            vec![
                Call::Start("default".into()),
                Call::Stop("default".into()),
                Call::Restart("default".into())
            ]
        );
        hang_up(server_a, client_a);
        hang_up(server_b, client_b);
    }

    // ── An authorized caller: every method round-trips ───────────────────
    {
        let c = core(true, &["broken"]);
        let (server, client) = connect(&c, &alice);

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
        assert_eq!(asked(&c), vec![(alice.clone(), Action::Control); 3]);

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

        // Every refusal of the manager, as its own error, as over D-Bus.
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
        }
        // The address check judges a profile an instance runs, so the record
        // names lan-3 and its profile while these writes are made, and the
        // record goes back to what it was once they are done.
        std::fs::write(
            sys.join("instances.ini"),
            format!("{record}\n[lan-3]\nServerProfile = lan-3\n"),
        )
        .expect("write record with lan-3");
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
        // Values with spaces, a backslash and a non-ASCII letter come back whole.
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
        assert_eq!(
            client.write_file(
                "models.ini",
                &format!(
                    "{model_profiles}[m]\nmodel = /models/m.gguf\noverride-tensor = blk\\.0\\.ffn_up_exps\n"
                ), false
            ),
            Err(ClientError::Invalid(Refusal {
                key: "override-no-device",
                values: vec!["1".into(), r"blk\.0\.ffn_up_exps".into()],
            }))
        );
        assert!(
            !sys.join("server@lan-3.ini").exists(),
            "refused writes write nothing"
        );
        std::fs::write(sys.join("instances.ini"), record).expect("restore the record");
        assert_eq!(
            client.list_instances(),
            Ok(vec![
                "broken".to_string(),
                "default".to_string(),
                "lan-2".to_string()
            ]),
            "the record is back to its three sections"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).unwrap(),
            default_ini
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("models.ini")).unwrap(),
            model_profiles
        );

        // A read that fails for another reason than a missing file.
        std::fs::create_dir(sys.join("models.ini.disabled")).expect("dir in the way");
        match client.read_file("models.ini.disabled") {
            Err(ClientError::Io(kind)) => assert!(!kind.is_empty(), "the kind"),
            other => panic!("read of a directory: {other:?}"),
        }
        std::fs::remove_dir(sys.join("models.ini.disabled")).unwrap();

        // Start at boot, both ways, and its refusals, as over D-Bus.
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

        // A file of non-ASCII text with every JSON escape round-trips whole.
        let tricky = "[m]\nmodel = /models/é \"q\"\\\t\u{1}.gguf\n";
        std::fs::write(sys.join("models.ini.disabled"), tricky).expect("tricky file");
        assert_eq!(
            client.read_file("models.ini.disabled"),
            Ok(Some(plain(tricky)))
        );
        std::fs::remove_file(sys.join("models.ini.disabled")).unwrap();
        hang_up(server, client);
    }

    // ── Every state the manager reports ──────────────────────────────────
    for (state, expected) in [
        (UnitState::Inactive, InstanceState::Inactive),
        (UnitState::Failed, InstanceState::Failed),
        (UnitState::Activating, InstanceState::Activating),
        (UnitState::Deactivating, InstanceState::Deactivating),
    ] {
        let c = core_in(state, true, &[]);
        let (server, client) = connect(&c, &alice);
        assert_eq!(client.status("default"), Ok(expected), "{state:?}");
        hang_up(server, client);
    }

    // ── The compare and swap write, end to end ────────────────────────────
    // The same rule as over D-Bus, over the named pipe's frame: the content
    // on the wire is the file's own bytes, a caller holding a content the
    // file no longer holds gets `ClientError::Changed` with the file left
    // alone, and a file that is not there is `Changed` too.
    {
        let c = core(true, &[]);
        let (server, client) = connect(&c, &alice);
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
        hang_up(server, client);
        std::fs::write(sys.join("models.ini"), model_profiles).expect("restore presets");
    }

    // ── The assignment verb, end to end ────────────────────────────────
    // The same verb as over D-Bus, over the named pipe's frame: the
    // accepted call writes the record and renders the instance's model
    // profiles; the refusals are the core's own errors and write nothing.
    {
        let c = core(true, &[]);
        let (server, client) = connect(&c, &Caller(String::new()));
        let assignments =
            std::fs::read_to_string(sys.join("assignments.ini")).expect("assignments");
        assert_eq!(
            client.set_model_profile("default", "Qwen3.8-27B", "on"),
            Err(ClientError::NotAuthorized),
            "no identity, no assignment"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("assignments.ini")).expect("assignments"),
            assignments,
            "the refused call writes nothing"
        );
        assert_eq!(asked(&c), vec![], "a call with no identity asks nobody");
        hang_up(server, client);
    }
    {
        let c = core(true, &[]);
        let (server, client) = connect(&c, &alice);
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
            "the guarded call a write is: configure asked of the caller"
        );
        assert_eq!(calls(&c), vec![], "an assignment touches no unit");
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
        hang_up(server, client);
    }

    // ── The `As` form over a stream: the bff vouches for a member ────────
    // The frame names `SetModelProfileAs` and carries the principal beside
    // the verb's own arguments; the caller is the bff's service account and
    // the member's membership decides.
    {
        let passwords = crate::manager::service::fakes::FakePasswords::new();
        let asked_admin = passwords.asked_admin.clone();
        let (c, _) = auth_core(passwords);
        let (server, client) = connect(&c, &Caller(BFF_SID.into()));
        std::fs::write(sys.join("assignments.ini"), "[default]\n").expect("assignments");
        assert_eq!(
            client
                .acting_for("alice")
                .set_model_profile("default", "Qwen3.8-27B", "on"),
            Ok(()),
            "the member's assignment, vouched for by the bff"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("assignments.ini")).unwrap(),
            "[default]\nQwen3.8-27B = on\n",
            "the assignment landed"
        );
        assert_eq!(
            client
                .acting_for("bob")
                .set_model_profile("default", "Qwen3.8-27B", "off"),
            Err(ClientError::NotAuthorized),
            "bob is no member of the administrators"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("assignments.ini")).unwrap(),
            "[default]\nQwen3.8-27B = on\n",
            "the refused call wrote nothing"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["alice".to_string(), "bob".to_string()],
            "the membership of the vouched name, per call"
        );
        hang_up(server, client);
    }
    // A caller that is not the bff, vouching for a member: refused, and no
    // membership looked up for a caller that may not vouch.
    {
        let passwords = crate::manager::service::fakes::FakePasswords::new();
        let checked = passwords.checked.clone();
        let asked_admin = passwords.asked_admin.clone();
        let (c, _) = auth_core(passwords);
        let (server, client) = connect(
            &c,
            &Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into()),
        );
        assert_eq!(
            client
                .acting_for("alice")
                .set_model_profile("default", "Qwen3.8-27B", "off"),
            Err(ClientError::NotAuthorized),
            "only the bff vouches"
        );
        assert_eq!(
            *checked.lock().unwrap(),
            vec![],
            "no sign-in, so no password check"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            Vec::<String>::new(),
            "the caller gate refused before any membership question"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("assignments.ini")).unwrap(),
            "[default]\nQwen3.8-27B = on\n",
            "a caller that is not the bff writes nothing"
        );
        hang_up(server, client);
    }

    // ── The server profile verb, end to end ──────────────────────────────
    // The same verb as over D-Bus, over the named pipe's frame: the accepted
    // call writes the record, the refusals are the core's own errors and
    // write nothing.
    {
        let c = core(true, &[]);
        let (server, client) = connect(&c, &Caller(String::new()));
        assert_eq!(
            client.set_server_profile("default", "default"),
            Err(ClientError::NotAuthorized),
            "no identity, no server profile change"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            record,
            "the refused call writes no instance record"
        );
        assert_eq!(asked(&c), vec![], "a call with no identity asks nobody");
        hang_up(server, client);
    }
    {
        let c = core(true, &[]);
        let (server, client) = connect(&c, &alice);
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
        let landed = std::fs::read_to_string(sys.join("instances.ini")).unwrap();
        assert!(
            landed.contains("[lan-2]\nServerProfile = alt\n"),
            "the switch landed: {landed:?}"
        );
        assert!(
            landed.starts_with("[default]\nServerProfile = default\n\n[broken]\n"),
            "the sections the switch did not touch keep their bytes: {landed:?}"
        );
        assert_eq!(
            asked(&c),
            vec![(alice.clone(), Action::Configure)],
            "the guarded call a write is: configure asked of the caller"
        );
        assert_eq!(calls(&c), vec![], "a server profile change touches no unit");
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
        std::fs::write(sys.join("instances.ini"), record.as_bytes()).expect("restore instances");
        hang_up(server, client);
    }

    // ── The `As` form over a stream: the bff vouches for a member ────────
    // The frame names `SetServerProfileAs` and carries the principal beside
    // the verb's own arguments; the caller is the bff's service account and
    // the member's membership decides.
    {
        let passwords = crate::manager::service::fakes::FakePasswords::new();
        let asked_admin = passwords.asked_admin.clone();
        let (c, _) = auth_core(passwords);
        let (server, client) = connect(&c, &Caller(BFF_SID.into()));
        std::fs::write(
            sys.join("server@alt.ini"),
            "[Server]\nPort = 8081\nHostname = 127.0.0.1\n",
        )
        .expect("alt profile");
        assert_eq!(
            client
                .acting_for("alice")
                .set_server_profile("default", "alt"),
            Ok(()),
            "the member's switch, vouched for by the bff"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            "[default]\nServerProfile = alt\n\n[broken]\nServerProfile = broken\n\n[lan-2]\nServerProfile = big\n",
            "the switch landed, the other sections kept"
        );
        let kept = std::fs::read_to_string(sys.join("instances.ini")).unwrap();
        assert_eq!(
            client
                .acting_for("bob")
                .set_server_profile("default", "default"),
            Err(ClientError::NotAuthorized),
            "bob is no member of the administrators"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            kept,
            "the refused call wrote nothing"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec!["alice".to_string(), "bob".to_string()],
            "the membership of the vouched name, per call"
        );
        let _ = std::fs::remove_file(sys.join("server@alt.ini"));
        std::fs::write(sys.join("instances.ini"), record.as_bytes()).expect("restore instances");
        hang_up(server, client);
    }
    // A caller that is not the bff, vouching for a member: refused, and the
    // record kept.
    {
        let passwords = crate::manager::service::fakes::FakePasswords::new();
        let (c, _) = auth_core(passwords);
        let (server, client) = connect(&c, &alice);
        assert_eq!(
            client
                .acting_for("alice")
                .set_server_profile("default", "default"),
            Err(ClientError::NotAuthorized),
            "only the bff vouches"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).unwrap(),
            record,
            "a caller that is not the bff writes nothing"
        );
        hang_up(server, client);
    }

    // ── Creating, listing and deleting a server profile, end to end ──────
    // The three new verbs as the client methods of slice 14, over the frame:
    // the file a create writes, the answers of every refusal, and the rows of
    // the list, with `instance` read from the empty string.
    {
        let c = core(true, &[]);
        let (server, client) = connect(&c, &alice);
        let seed = std::fs::read_to_string(sys.join("instances.ini")).expect("seed");
        type Row = (String, String, u16, Vec<String>, Option<String>);
        let rows = |client: &PipeClient<UnixStream>| -> Vec<Row> {
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
        let bytes = |name: &str| std::fs::read_to_string(sys.join(name)).expect("profile");
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(
            client.create_server_profile("llama.cpp", "pipe-p", Some(8123)),
            Ok(()),
            "a name that is nothing yet"
        );
        assert!(
            bytes("server@pipe-p.ini").contains("Port = 8123"),
            "the port of the call, in the file: {:?}",
            bytes("server@pipe-p.ini")
        );
        assert_eq!(
            std::fs::metadata(sys.join("server@pipe-p.ini"))
                .expect("metadata")
                .permissions()
                .mode()
                & 0o7777,
            0o640,
            "the mode of a served profile, over the stream as over the bus"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).expect("record"),
            seed,
            "a create writes no instance record"
        );
        assert_eq!(
            client
                .server_profiles()
                .expect("server_profiles")
                .iter()
                .find(|r| r.id == "pipe-p")
                .map(|r| (r.engine.clone(), r.port, r.instance.clone())),
            Some(("llama.cpp".to_string(), 8123u16, None)),
            "the new file is a row of the list, and an unused one: the client reads \
             the empty instance of the frame as None"
        );
        assert_eq!(
            client.create_server_profile("llama.cpp", "pipe-p", Some(9001)),
            Err(ClientError::ServerProfileExists("pipe-p".into())),
            "a second create of the same name, the message naming it"
        );
        assert!(
            bytes("server@pipe-p.ini").contains("Port = 8123"),
            "the file kept its bytes"
        );
        assert_eq!(
            client.create_server_profile("llama.cpp", "pipe-zero", None),
            Ok(()),
            "port None is 0 on the wire"
        );
        assert!(
            !bytes("server@pipe-zero.ini")
                .lines()
                .any(|l| l.trim_start().starts_with("Port ")),
            "port 0 leaves no Port key: {:?}",
            bytes("server@pipe-zero.ini")
        );

        // The refusals, over the frame: the engine, the name, and the delete's
        // own answers.
        assert_eq!(
            client.create_server_profile("vllm", "pipe-q", Some(8123)),
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
            !sys.join("server@pipe-q.ini").exists() && !sys.join("x.ini").exists(),
            "a refused create created nothing, and no path outside the engine's directory"
        );
        assert_eq!(
            client.delete_server_profile("llama.cpp", "default"),
            Err(ClientError::ServerProfileInUse(
                "default".into(),
                "default".into()
            )),
            "the profile the section names, and the instance naming it"
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
            client.delete_server_profile("llama.cpp", "pipe-p"),
            Ok(()),
            "an unused profile goes"
        );
        assert!(
            !sys.join("server@pipe-p.ini").exists(),
            "its file is gone: the create and the delete round trip over the frame"
        );
        assert!(
            !rows(&client)
                .iter()
                .any(|r| r.0 == "pipe-p" || r.0 == "pipe-q" || r.0 == "as-p"),
            "the list is the files that are profiles, after the delete"
        );
        std::fs::write(sys.join("server@X.ini"), "[Server]\nPort = 1\n").expect("bad name");
        let ids = rows(&client)
            .iter()
            .map(|r| r.0.clone())
            .collect::<Vec<_>>();
        assert!(
            !ids.contains(&"X".to_string()),
            "a name the engine refuses is no row: {ids:?}"
        );
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "the rows are sorted by id");
        assert_eq!(
            rows(&client)
                .iter()
                .find(|r| r.0 == "broken")
                .map(|r| (r.2, r.4.clone())),
            Some((8090u16, Some("broken".to_string()))),
            "the used profile carries its port and its instance"
        );
        assert_eq!(
            rows(&client)
                .iter()
                .find(|r| r.0 == "pipe-zero")
                .map(|r| (r.2, r.4.clone())),
            Some((
                u16::try_from(pai_core::server_cfg::DEFAULT_PORT).expect("DEFAULT_PORT"),
                None
            )),
            "a file with no Port is the default port, and an unused profile no instance"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).expect("record"),
            seed,
            "a delete of an unused profile writes no record"
        );
        let _ = std::fs::remove_file(sys.join("server@pipe-zero.ini"));
        let _ = std::fs::remove_file(sys.join("server@X.ini"));
        hang_up(server, client);
    }

    // ── The `As` names of the new verbs: the bound client picks them ──────
    {
        let passwords = crate::manager::service::fakes::FakePasswords::new();
        let asked_admin = passwords.asked_admin.clone();
        let (c, _) = auth_core(passwords);
        let (server, client) = connect(&c, &Caller(BFF_SID.into()));
        assert_eq!(
            client
                .acting_for("alice")
                .create_server_profile("llama.cpp", "as-p", Some(8123)),
            Ok(()),
            "the member's create, vouched for by the bff"
        );
        assert!(
            sys.join("server@as-p.ini").is_file(),
            "the file the member's create wrote"
        );
        assert_eq!(
            client
                .acting_for("bob")
                .create_server_profile("llama.cpp", "as-q", Some(8123)),
            Err(ClientError::NotAuthorized),
            "bob is no member of the administrators"
        );
        assert!(
            !sys.join("server@as-q.ini").exists(),
            "the refused `As` create wrote nothing"
        );
        assert_eq!(
            client
                .acting_for("alice")
                .delete_server_profile("llama.cpp", "as-p"),
            Ok(()),
            "and the member's delete of it"
        );
        assert_eq!(
            client
                .acting_for("bob")
                .delete_server_profile("llama.cpp", "default"),
            Err(ClientError::NotAuthorized),
            "the same for a delete"
        );
        assert!(
            sys.join("server@default.ini").is_file(),
            "the refused `As` delete removed nothing"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec![
                "alice".to_string(),
                "bob".into(),
                "alice".into(),
                "bob".into()
            ],
            "the membership of the vouched name, per call"
        );
        hang_up(server, client);
    }
    // A caller that is not the bff, vouching for a member: the `As` name is
    // sent, and refused.
    {
        let passwords = crate::manager::service::fakes::FakePasswords::new();
        let asked_admin = passwords.asked_admin.clone();
        let (c, _) = auth_core(passwords);
        let (server, client) = connect(&c, &alice);
        assert_eq!(
            client
                .acting_for("alice")
                .create_server_profile("llama.cpp", "as-r", Some(8123)),
            Err(ClientError::NotAuthorized),
            "only the bff vouches"
        );
        assert_eq!(
            client
                .acting_for("alice")
                .delete_server_profile("llama.cpp", "default"),
            Err(ClientError::NotAuthorized),
            "the same for a delete"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            Vec::<String>::new(),
            "the caller gate refused before any membership question"
        );
        assert!(
            !sys.join("server@as-r.ini").exists() && sys.join("server@default.ini").is_file(),
            "a caller that is not the bff writes and removes nothing"
        );
        hang_up(server, client);
    }

    // ── Creating and removing an instance, end to end ────────────────────
    // The two verbs of slice 16 as the client methods, over the frame: the
    // section and the render a create writes, the answer of every refusal, and
    // what a remove takes away and leaves.
    {
        // The units are seen stopped, so a remove runs, and `lan-2` seen
        // active: one of the remove's refusals is a running instance.
        let c = core_units(UnitState::Inactive, true, &[], &["lan-2"]);
        let (server, client) = connect(&c, &alice);
        let seed = std::fs::read_to_string(sys.join("instances.ini")).expect("seed");
        let presets = std::fs::read_to_string(sys.join("models.ini")).expect("presets");
        let profile = sys.join("server@pipe-i.ini");
        std::fs::write(&profile, "[Server]\nPort = 8123\nHostname = 127.0.0.1\n")
            .expect("free profile");
        let render = sys.join("models@pipe-i.ini");
        let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);

        assert_eq!(
            client.create_instance("llama.cpp", "pipe-i", "pipe-i"),
            Ok(()),
            "a new id of a profile nothing else names"
        );
        assert_eq!(
            *calls(&c),
            Vec::<Call>::new(),
            "the create runs no service call: the template serves a new id and \
             its boot is off"
        );
        assert_eq!(
            &text_of("instances.ini")[seed.len()..],
            "[pipe-i]\nServerProfile = pipe-i\n",
            "the section is appended, naming the profile, every byte before it \
             the seed's"
        );
        assert_eq!(
            text_of("models@pipe-i.ini"),
            pai_core::assign::render(&presets, "", None),
            "the render is the pool with the new instance assigned nothing"
        );
        assert!(
            !pai_core::launch::has_model_profiles_in(&render),
            "the new instance has no model profile"
        );
        assert!(
            !sys.join("models@pipe-i.ini.tmp").exists(),
            "no temp file is left"
        );
        assert_eq!(
            client.boot_enabled("pipe-i"),
            Ok(false),
            "a new instance starts at boot off"
        );
        let rows = client.instances().expect("instances");
        let new = rows.iter().find(|r| r.id == "pipe-i").expect("listed");
        assert_eq!(
            (new.engine.as_str(), new.server_profile.as_deref(), new.port),
            ("llama.cpp", Some("pipe-i"), 8123),
            "the new instance is listed with the profile and the port it names"
        );

        // Every refusal, over the frame: the engine, the id, the name taken,
        // the profile that is not there and the one already in use. Each writes
        // no record and no render: the record stays as the create left it.
        let seeded = text_of("instances.ini");
        for (answer, want) in [
            (
                client.create_instance("vllm", "pipe-q", "pipe-i"),
                Err(ClientError::Invalid(Refusal {
                    key: "unknown-engine",
                    values: vec!["vllm".into()],
                })),
            ),
            (
                client.create_instance("llama.cpp", "pipe-i", "pipe-i"),
                Err(ClientError::Invalid(Refusal {
                    key: "instance-exists",
                    values: vec!["pipe-i".into()],
                })),
            ),
            (
                client.create_instance("llama.cpp", "pipe-z", "nosuch"),
                Err(ClientError::NoServerProfile("nosuch".into())),
            ),
            (
                client.create_instance("llama.cpp", "pipe-z", "broken"),
                Err(ClientError::ServerProfileInUse(
                    "broken".into(),
                    "broken".into(),
                )),
            ),
        ] {
            assert_eq!(answer, want, "{want:?}");
            assert_eq!(text_of("instances.ini"), seeded, "{want:?}");
            assert!(!sys.join("models@pipe-z.ini").exists(), "{want:?}");
        }
        for id in ["../x", "A", "", "a_b"] {
            assert_eq!(
                client.create_instance("llama.cpp", id, "pipe-i"),
                Err(ClientError::Invalid(Refusal {
                    key: "instance-bad-id",
                    values: vec![id.into()],
                })),
                "an id the rule refuses"
            );
            assert_eq!(text_of("instances.ini"), seeded, "{id}");
        }

        // The remove: the record's section and the render go, the profile, the
        // pool, and every other instance stay.
        let before = calls(&c);
        assert_eq!(client.remove_instance("pipe-i"), Ok(()));
        assert_eq!(
            calls(&c)[before.len()..],
            [Call::Status("pipe-i".into()), Call::Remove("pipe-i".into())],
            "the state is read before the unit is removed, once"
        );
        assert_eq!(
            text_of("instances.ini"),
            seed,
            "the section is gone and every other byte is the seed's"
        );
        assert!(!render.exists(), "the render is gone");
        assert!(profile.is_file(), "the server profile is the pool's");
        assert_eq!(
            text_of("models.ini"),
            presets,
            "the model profiles are nobody's to delete here"
        );
        assert_eq!(
            text_of("server@broken.ini"),
            broken_ini,
            "another instance's profile is its own"
        );
        assert_eq!(
            client.remove_instance("pipe-i"),
            Err(ClientError::NoInstance),
            "a second remove has nothing to remove"
        );

        // The remove's refusals: the protected id, a running one, an id the
        // rule refuses, and no such instance. The running one is refused
        // because its state is read: the status is asked for it, and it alone.
        for (answer, want) in [
            (
                client.remove_instance("default"),
                Err(ClientError::Invalid(Refusal {
                    key: "instance-protected",
                    values: vec!["default".into()],
                })),
            ),
            (
                client.remove_instance("lan-2"),
                Err(ClientError::Invalid(Refusal {
                    key: "instance-running",
                    values: vec!["lan-2".into()],
                })),
            ),
            (client.remove_instance("../x"), Err(ClientError::InvalidId)),
            (client.remove_instance("nope"), Err(ClientError::NoInstance)),
        ] {
            assert_eq!(answer, want, "{want:?}");
            assert_eq!(text_of("instances.ini"), seed, "{want:?}");
        }
        assert_eq!(
            calls(&c)[before.len()..],
            [
                Call::Status("pipe-i".into()),
                Call::Remove("pipe-i".into()),
                Call::Status("lan-2".into()),
            ],
            "the refusals remove no unit: the state read is the one of `lan-2`, \
             the refusal is that it is running"
        );
        let _ = std::fs::remove_file(&profile);
        hang_up(server, client);
    }

    // ── The `As` names of the instance verbs: the bff vouches for a member ─
    {
        let passwords = crate::manager::service::fakes::FakePasswords::new();
        let asked_admin = passwords.asked_admin.clone();
        let (c, _) = auth_core(passwords);
        let (server, client) = connect(&c, &Caller(BFF_SID.into()));
        let seed = std::fs::read_to_string(sys.join("instances.ini")).expect("seed");

        // The checks are the principal's own: a member's id the rule refuses
        // answers the check, a principal outside `pai` is refused.
        assert_eq!(
            client
                .acting_for("alice")
                .create_instance("llama.cpp", "../x", "default"),
            Err(ClientError::Invalid(Refusal {
                key: "instance-bad-id",
                values: vec!["../x".into()],
            })),
            "the member's create reaches the core's own check"
        );
        assert_eq!(
            client.acting_for("alice").remove_instance("../x"),
            Err(ClientError::InvalidId),
            "and the remove's own answer for an id the rule refuses"
        );
        assert_eq!(
            client
                .acting_for("bob")
                .create_instance("llama.cpp", "as-i", "default"),
            Err(ClientError::NotAuthorized),
            "bob is no member of the administrators"
        );
        assert_eq!(
            client.acting_for("bob").remove_instance("default"),
            Err(ClientError::NotAuthorized),
            "the same for a remove"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec![
                "alice".to_string(),
                "alice".into(),
                "bob".into(),
                "bob".into()
            ],
            "the membership of the vouched name, per call"
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("instances.ini")).expect("record"),
            seed,
            "nothing was written: no create of a bad id, nothing for a principal \
             outside `pai`"
        );
        assert!(!sys.join("models@as-i.ini").exists(), "no render either");
        hang_up(server, client);
    }

    // ── A create over a record whose last line has no final newline ──────
    // The create appends its section after every byte that is there, and the
    // design says a newline goes FIRST when the text does not end with one:
    // the section the administrator left as the file's last line keeps its
    // own keys, and the new instance's header starts a line of its own, so
    // the list reads it. The CRLF style is the file's own (every other
    // writer of the record, `pai_core::ini`, keeps the style it finds), so a
    // record edited on Windows arrives in it.
    for (style, record) in [
        (
            "LF",
            "[default]\nServerProfile = default\n\n[tail]\nServerProfile = tail",
        ),
        (
            "CRLF",
            "[default]\r\nServerProfile = default\r\n\r\n[tail]\r\nServerProfile = tail",
        ),
    ] {
        let c = core_units(UnitState::Inactive, true, &[], &[]);
        let (server, client) = connect(&c, &alice);
        std::fs::write(
            sys.join("server@pipe-n.ini"),
            "[Server]\nPort = 8127\nHostname = 127.0.0.1\n",
        )
        .expect("free profile");
        std::fs::write(sys.join("instances.ini"), record).expect("record with no final newline");

        assert_eq!(
            client.create_instance("llama.cpp", "pipe-n", "pipe-n"),
            Ok(()),
            "{style}: a create over a record whose last line has no final newline"
        );
        let text = std::fs::read_to_string(sys.join("instances.ini")).expect("record");
        assert!(
            text.starts_with(record),
            "{style}: every byte of the record is left as it was: {text:?}"
        );
        let sections = pai_core::ini::parse_sections(&text);
        let profile_of = |id: &str| {
            sections
                .iter()
                .find(|section| section.id == id)
                .and_then(|section| section.keys.get("ServerProfile"))
                .map(|name| name.trim().to_string())
        };
        assert_eq!(
            profile_of("tail"),
            Some("tail".to_string()),
            "{style}: the section that ended the file keeps the profile it names, \
             the appended header lands on a line of its own: {text:?}"
        );
        assert_eq!(
            profile_of("pipe-n"),
            Some("pipe-n".to_string()),
            "{style}: the record has a section of the new instance: {text:?}"
        );
        assert!(
            client
                .instances()
                .expect("instances")
                .iter()
                .any(|row| row.id == "pipe-n"),
            "{style}: the new instance is listed: {text:?}"
        );
        hang_up(server, client);
    }
}

/// The bff's account on Windows, `NT SERVICE\private-ai-bff`, by its
/// service SID.
const BFF_SID: &str = "S-1-5-80-4088784262-1422927641-1363593751-33154763-2531830098";

/// A core taking the bff's SID for the bff's account over the fake
/// accounts (alice an administrator, bob not), and its checker's log.
fn auth_core(
    passwords: crate::manager::service::fakes::FakePasswords,
) -> (Arc<Core>, crate::manager::service::fakes::Checked) {
    let checked = passwords.checked.clone();
    let core = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true).bff(&Caller(BFF_SID.into())),
        FakeResolver::lan(),
    )
    .with_group(crate::manager::service::fakes::FakeGroup::new())
    .with_passwords(passwords);
    (Arc::new(core), checked)
}

/// pai-core's `authenticate` against the stream server, as the bff, then as
/// another user and over a failing checker; no file is read, so a plain test.
#[test]
fn the_pipe_client_authenticates_as_the_bff() {
    use crate::manager::service::fakes::FakePasswords;
    let (c, checked) = auth_core(FakePasswords::new());
    let (server, client) = connect(&c, &Caller(BFF_SID.into()));
    assert_eq!(client.authenticate("alice", "s3cret-Pw"), Ok((true, true)));
    assert_eq!(client.authenticate("bob", "hunter2!"), Ok((true, false)));
    assert_eq!(client.authenticate("alice", "wrong"), Ok((false, false)));
    assert_eq!(
        client.authenticate("mallory", "s3cret-Pw"),
        Ok((false, false))
    );
    assert_eq!(client.authenticate("alice", ""), Ok((false, false)));
    assert_eq!(checked.lock().unwrap().len(), 4);
    hang_up(server, client);

    let (c, checked) = auth_core(FakePasswords::new());
    let alice = Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into());
    let (server, client) = connect(&c, &alice);
    assert_eq!(
        client.authenticate("alice", "s3cret-Pw"),
        Err(ClientError::NotAuthorized)
    );
    assert_eq!(*checked.lock().unwrap(), vec![]);
    hang_up(server, client);

    let failure = "LogonUserW: ERROR_NO_LOGON_SERVERS (1311)";
    let (c, _) = auth_core(FakePasswords::failing(failure));
    let (server, client) = connect(&c, &Caller(BFF_SID.into()));
    assert_eq!(
        client.authenticate("alice", "s3cret-Pw"),
        Err(ClientError::Service(failure.into()))
    );
    hang_up(server, client);
}

// ── The node's device list over a stream (G2, ARCHITECTURE.md 1) ─────────
//
// The same answer the bus gives (`tests/dbus_client.rs`): the list the probe
// answered, in the probe's order, with its moment. `Devices` is the caller's
// own read, gated on `control` asked WITHOUT interaction; `DevicesAs` is the
// bff's, for a current member of `pai`. A refused read runs no engine: the
// seam's counter stays empty. Names the tests assume, the contract for the
// coder: `ManagerClient::devices()` and the pipe's `Devices` / `DevicesAs`.

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

/// A core answering the device list of [`cards`], with the bff's SID as the
/// bff's account over the fake accounts (alice a member of `pai`, bob not),
/// and polkit allowing the quiet check as `quiet`.
fn devices_core(quiet: bool) -> (Arc<Core>, Arc<std::sync::Mutex<Vec<String>>>) {
    use crate::manager::service::fakes::FakePasswords;
    let seam = cards();
    let asked = seam.asked.clone();
    let core = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true)
            .quietly(quiet)
            .bff(&Caller(BFF_SID.into())),
        FakeResolver::lan(),
    )
    .with_group(crate::manager::service::fakes::FakeGroup::new())
    .with_passwords(FakePasswords::new())
    .with_gpus(seam);
    (Arc::new(core), asked)
}

/// pai-core's `devices` against the stream server: the bff's own read, the
/// member's read vouched for, and both refusals. No file is read, so a plain
/// test.
#[test]
fn the_pipe_client_reads_the_node_devices() {
    let alice = Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into());

    // The bff's own read, and the member's read it vouches for.
    let (c, asked) = devices_core(true);
    let (server, client) = connect(&c, &Caller(BFF_SID.into()));
    assert_eq!(
        client.devices(),
        Ok(listed()),
        "the list the probe answered, in its order"
    );
    assert_eq!(
        *c.authority().asked_quietly.lock().unwrap(),
        vec![(Caller(BFF_SID.into()), Action::Control)],
        "control asked without interaction"
    );
    assert_eq!(
        *c.authority().asked.lock().unwrap(),
        Vec::<(Caller, Action)>::new(),
        "never asked with a prompt"
    );
    assert_eq!(
        client.acting_for("alice").devices(),
        Ok(listed()),
        "the member's read, vouched for by the bff"
    );
    assert_eq!(
        client.acting_for("bob").devices(),
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
    hang_up(server, client);

    // A caller polkit refuses the quiet check: refused, and no probe.
    let (c, asked) = devices_core(false);
    let (server, client) = connect(&c, &alice);
    assert_eq!(
        client.devices(),
        Err(ClientError::NotAuthorized),
        "no control, no list"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        Vec::<String>::new(),
        "a refused read runs no engine"
    );
    hang_up(server, client);

    // A caller that is not the bff, vouching for a member: refused, no probe.
    let (c, asked) = devices_core(true);
    let (server, client) = connect(&c, &alice);
    assert_eq!(
        client.acting_for("alice").devices(),
        Err(ClientError::NotAuthorized),
        "only the bff's account vouches"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        Vec::<String>::new(),
        "a forged pair runs no engine"
    );
    hang_up(server, client);
}
