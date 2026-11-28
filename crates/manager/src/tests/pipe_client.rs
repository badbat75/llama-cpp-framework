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

/// E2E phase for the stream client against the server, for one of this crate's
/// isolated phase tests to call (`src/tests/mod.rs`): it runs in a child
/// process of its own (`pai_core::isolated::isolated`), where it sets the
/// process-wide `PRIVATE_AI_DATA_ROOT` redirect.
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
    // The instance profiles (IP4b: `CreateInstance` names one). `pipe-i` and
    // `pipe-n` are the names the create block gives, on server profiles of
    // their own; `default` and `broken` are the profiles of the record's own
    // instances, so the create's refusal of a profile in use has one.
    std::fs::write(
        sys.join("instance-profiles.ini"),
        "[pipe-i]\nServerProfile = pipe-i\n\n[pipe-n]\nServerProfile = pipe-n\n\n[default]\nServerProfile = default\n\n[broken]\nServerProfile = broken\n",
    )
    .expect("write instance profiles");
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
            client.write_file_if("models.ini", &more, &base).map(|_| ()),
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
            "IP3c: the accepted compare-and-swap answers the apply, whose drift compare asks \
             the state of the two instances whose pair moved off the pair their launch \
             recorded, in the record's order, and asks no unit verb; `lan-2` has no pair, so it \
             applied nothing and is asked for nothing; the two refused writes ask nothing"
        );
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
        assert_eq!(
            calls(&c),
            vec![
                Call::Status("default".into()),
                Call::Status("broken".into()),
            ],
            "IP3c: the accepted assignment answers the apply, whose drift compare asks the \
             state of the two instances whose pair moved off the pair their launch recorded, \
             in the record's order, and asks no unit verb: `lan-2` has no pair, so it applied \
             nothing and is asked for nothing"
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
        assert_eq!(
            calls(&c),
            vec![
                Call::Status("default".into()),
                Call::Status("broken".into()),
            ],
            "IP3c: the accepted switch answers the apply, whose drift compare asks the state \
             of the two instances whose pair stands off the pair their launch recorded, in the \
             record's order, and asks no unit verb; `lan-2`, whose pair the switch renders for \
             the first time, recorded no pair and so is asked for nothing; the refusals ask \
             nothing"
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
        std::fs::write(sys.join("instances.ini"), record.as_bytes()).expect("restore instances");
        hang_up(server, client);
    }

    // ── The label of an instance, end to end ─────────────────────────────
    // The same verb as over D-Bus, over the named pipe's frame: `Instances`
    // answers the label beside the id, the accepted text lands trimmed, the
    // empty text REMOVES the key, and every refusal is the core's own error
    // with the record untouched. The label is metadata: no unit is touched and
    // the instance runs the whole time.
    {
        let seeded =
            "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = big\nLabel = GPU box\n";
        std::fs::write(sys.join("instances.ini"), seeded).expect("record");
        let text_of = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);

        // No identity, no write, as every guarded verb.
        let c = core(true, &[]);
        let (server, client) = connect(&c, &Caller(String::new()));
        assert_eq!(
            client.set_instance_label("default", "Coding box"),
            Err(ClientError::NotAuthorized),
            "no identity, no label change"
        );
        assert_eq!(
            text_of("instances.ini"),
            seeded,
            "a refused call writes no record"
        );
        assert_eq!(asked(&c), vec![], "a call with no identity asks nobody");
        hang_up(server, client);

        let c = core(true, &[]);
        let (server, client) = connect(&c, &alice);
        assert_eq!(
            client
                .instances()
                .expect("instances")
                .iter()
                .map(|r| (r.id.as_str(), r.label.as_deref()))
                .collect::<Vec<_>>(),
            vec![("default", None), ("lan-2", Some("GPU box"))],
            "the label of the section, and no label for a section that names none"
        );
        assert_eq!(asked(&c), vec![], "the list is answered with no identity");

        assert_eq!(
            client.set_instance_label("default", "  Coding box "),
            Ok(()),
            "a text with hanging space"
        );
        assert_eq!(
            text_of("instances.ini"),
            "[default]\nServerProfile = default\nLabel = Coding box\n\n[lan-2]\nServerProfile = big\nLabel = GPU box\n",
            "the key written trimmed, every other byte of the record kept"
        );
        assert_eq!(
            asked(&c),
            vec![(alice.clone(), Action::Configure)],
            "the guarded call a write is: configure asked of the caller"
        );
        assert_eq!(calls(&c), vec![], "a relabel touches no unit");
        assert_eq!(
            client.instances().expect("instances")[0].label.as_deref(),
            Some("Coding box"),
            "the write reads back over the same frame"
        );

        assert_eq!(
            client.set_instance_label("default", ""),
            Ok(()),
            "the empty text clears the label"
        );
        assert_eq!(
            text_of("instances.ini"),
            seeded,
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
            client.set_instance_label("lan-2", "GPU box"),
            Ok(()),
            "the label of the instance itself"
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
            Err(ClientError::Invalid(Refusal {
                key: "instance-label-taken",
                values: vec!["GPU BOX".to_string(), "lan-2".to_string()],
            })),
            "the label lan-2 carries, ignoring case, is taken, and the refusal names who holds it"
        );
        assert_eq!(
            text_of("instances.ini"),
            seeded,
            "refused calls write nothing"
        );

        // (IL1 review D1) The label is unique among OTHER instances, over the
        // frame too: lan-2 changing only the letter case of its own label
        // relabels itself, and the new case stays taken for the neighbours.
        assert_eq!(
            client.set_instance_label("lan-2", "GPU BOX"),
            Ok(()),
            "lan-2 carries `GPU box`: the same text in another case is its own \
             label, not a label another instance holds"
        );
        assert_eq!(
            text_of("instances.ini"),
            "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = big\nLabel = GPU BOX\n",
            "the file holds the new case, every other byte kept"
        );
        assert_eq!(
            client.instances().expect("instances")[1].label.as_deref(),
            Some("GPU BOX"),
            "the new case reads back over the same frame"
        );
        assert_eq!(
            client.set_instance_label("default", "gpu box"),
            Err(ClientError::Invalid(Refusal {
                key: "instance-label-taken",
                values: vec!["gpu box".to_string(), "lan-2".to_string()],
            })),
            "the new case is still taken by lan-2 for another instance"
        );
        assert_eq!(
            text_of("instances.ini"),
            "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = big\nLabel = GPU BOX\n",
            "the refusal wrote nothing"
        );
        hang_up(server, client);
        std::fs::write(sys.join("instances.ini"), record.as_bytes()).expect("restore instances");
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
            Ok(8581),
            "a new id of an instance profile nothing else names, answering the port \
             the create proposed"
        );
        assert_eq!(
            *calls(&c),
            Vec::<Call>::new(),
            "the create runs no service call: the template serves a new id and \
             its boot is off"
        );
        assert_eq!(
            &text_of("instances.ini")[seed.len()..],
            "[pipe-i]\nInstanceProfile = pipe-i\nPort = 8581\n",
            "the section is appended, naming the instance profile and carrying the port the \
             create proposed (the first free one at or above 8581: no section of this \
             seed's record holds a `Port`), every byte before it the seed's"
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
            ("llama.cpp", Some("pipe-i"), 8581),
            "the new instance is listed with the profile and the record's port, the \
             one the create proposed, not the 8123 of the profile it names"
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
                Err(ClientError::NoInstanceProfile("nosuch".into())),
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
            Ok(8581),
            "{style}: a create over a record whose last line has no final newline"
        );
        let text = std::fs::read_to_string(sys.join("instances.ini")).expect("record");
        assert!(
            text.starts_with(record),
            "{style}: every byte of the record is left as it was: {text:?}"
        );
        let sections = pai_core::ini::parse_sections(&text);
        let key_of = |id: &str, key: &str| -> Option<String> {
            sections
                .iter()
                .find(|section| section.id == id)
                .and_then(|section| section.keys.get(key))
                .map(|name| name.trim().to_string())
        };
        assert_eq!(
            key_of("tail", "ServerProfile").as_deref(),
            Some("tail"),
            "{style}: the section that ended the file keeps the profile it names, \
             the appended header lands on a line of its own: {text:?}"
        );
        assert_eq!(
            key_of("pipe-n", "InstanceProfile").as_deref(),
            Some("pipe-n"),
            "{style}: the record has a section of the new instance, naming the instance \
             profile the create was given: {text:?}"
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

    // ── The layer facts of a saved model profile (ModelInfo / ModelInfoAs) ──
    // L1 of "Layers, MoE and tensor placement on the web model profile page"
    // (ARCHITECTURE.md (1)): the reply the bff reads (`model`, `info`,
    // `reason`), over the stream, gated as reading `models.ini` is. The read
    // opens a file under this phase's redirect, so it lives here.
    {
        use crate::manager::service::write_gguf_kv;
        use pai_core::client::{ModelFacts, Reason, TensorRow};

        let row = |name: &str, bytes: u64| TensorRow {
            name: name.into(),
            bytes,
        };

        let models = dir.path().join("info-models");
        std::fs::create_dir_all(&models).expect("models dir");
        // A MoE file: the header's counts are the reply, `moe` from the
        // routed-expert tensors.
        let gguf = models.join("stream.gguf");
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
        // 20 000 tensors of long names: the reply's tensor list is bounded by
        // bytes (J1), and the bound exists for THIS stream, whose frame is
        // `MAX_FRAME`.
        let long = models.join("long-names.gguf");
        let long_names: Vec<String> = (0..20_000)
            .map(|i| format!("t{i:05}{}", "w".repeat(94)))
            .collect();
        write_gguf_kv(
            &long,
            "llama",
            &[("llama.block_count", 1)],
            &long_names
                .iter()
                .map(|name| (name.as_str(), 4096u64))
                .collect::<Vec<(&str, u64)>>(),
        );
        std::fs::write(
            sys.join("models.ini"),
            format!(
                "[stream]\nmodel = {}\n\n[fetched]\nhf-repo = the-org/the-model\n\n[gone]\nmodel = {}\n\n[longnames]\nmodel = {}\n",
                gguf.display(),
                gone.display(),
                long.display()
            ),
        )
        .expect("models.ini");
        let facts = ModelFacts {
            block_count: 40,
            moe: true,
            expert_count: 128,
            expert_used: 8,
            // J1: the tensor list crosses the stream with the counts, so the
            // bff's page has the names to place.
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

        // The bff's own read, and the member's read it vouches for.
        let (c, asked) = devices_core(true);
        let (server, client) = connect(&c, &Caller(BFF_SID.into()));
        let got = client.model_info("stream").expect("the bff's own read");
        assert_eq!(
            got.info,
            Some(facts.clone()),
            "the facts, in the fields of the design, on the wire"
        );
        assert!(
            got.model.as_deref().is_some_and(|p| same(p, &gguf)),
            "`model` is the resolved absolute path the read opened: {got:?}"
        );
        assert_eq!(got.reason, None, "facts: no reason");
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
            client
                .acting_for("alice")
                .model_info("stream")
                .map(|got| (got.info, got.reason)),
            Ok((Some(facts.clone()), None)),
            "the member's read, vouched for by the bff: the same facts"
        );
        assert_eq!(
            client.acting_for("bob").model_info("stream"),
            Err(ClientError::NotAuthorized),
            "bob is no member of `pai`: no facts, and no path of the profile's file"
        );
        assert_eq!(
            *asked.lock().unwrap(),
            Vec::<String>::new(),
            "the read asks the manager's own seam for nothing"
        );
        hang_up(server, client);

        // A caller that is not the bff, vouching for a member: refused.
        let (c, _) = devices_core(true);
        let (server, client) = connect(&c, &alice);
        assert_eq!(
            client.acting_for("alice").model_info("stream"),
            Err(ClientError::NotAuthorized),
            "only the bff's account vouches"
        );
        hang_up(server, client);

        // A caller polkit refuses the quiet check: refused.
        let (c, _) = devices_core(false);
        let (server, client) = connect(&c, &alice);
        assert_eq!(
            client.model_info("stream"),
            Err(ClientError::NotAuthorized),
            "no control, no facts"
        );
        hang_up(server, client);

        // The reasons are answers of a call that worked, and an id the pool has
        // no section for is the keyed refusal (the page's 404).
        let (c, _) = devices_core(true);
        let (server, client) = connect(&c, &Caller(BFF_SID.into()));
        assert_eq!(
            client
                .acting_for("alice")
                .model_info("fetched")
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
            client
                .acting_for("alice")
                .model_info("gone")
                .map(|got| got.reason),
            Ok(Some(Reason {
                key: "model-info-missing".into(),
                args: vec![gone.to_string_lossy().into_owned()],
            })),
            "a file that is not there: the resolved path, as the page shows it"
        );
        assert_eq!(
            client.acting_for("alice").model_info("nope"),
            Err(ClientError::Invalid(Refusal {
                key: "no-model-profile-named",
                values: vec!["nope".into()],
            })),
            "no section named `nope`: the keyed refusal"
        );
        hang_up(server, client);

        // The byte bound of the tensor list, ON the stream (J1's follow-up,
        // `ARCHITECTURE.md` 5): 20 000 tensors of long names answer the rows
        // that fit 917 504 bytes of the rows' JSON, and the flag, and the reply
        // is a frame the client reads: the rows as the pipe spells them leave
        // 128 KiB of the 1 MiB frame for the reply's other fields.
        {
            use pai_core::client::pipe::MAX_FRAME;
            let (c, _) = devices_core(true);
            let (server, client) = connect(&c, &Caller(BFF_SID.into()));
            let got = client
                .model_info("longnames")
                .expect("the bounded tensor list crosses the stream in one frame");
            let facts = got
                .info
                .as_ref()
                .unwrap_or_else(|| panic!("no facts: {got:?}"));
            assert!(
                facts.tensors_truncated,
                "20 000 tensors of long names: the list on the wire is cut, and says so"
            );
            assert!(
                facts.tensors.len() < 20_000,
                "the byte bound cuts this list before the row bound: {} rows crossed",
                facts.tensors.len()
            );
            assert_eq!(
                facts
                    .tensors
                    .iter()
                    .map(|row| row.name.as_str())
                    .collect::<Vec<_>>(),
                long_names
                    .iter()
                    .take(facts.tensors.len())
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                "what crossed is the front of the file's order: {} rows",
                facts.tensors.len()
            );
            let rows = serde_json::to_string(
                &facts
                    .tensors
                    .iter()
                    .map(|row| serde_json::json!({ "name": &row.name, "bytes": row.bytes }))
                    .collect::<Vec<_>>(),
            )
            .expect("rows json");
            assert!(
                rows.len() + 128 * 1024 <= MAX_FRAME as usize,
                "the rows as the pipe spells them are {} bytes of the {MAX_FRAME} frame: the \
                 reply's other fields must keep 128 KiB",
                rows.len()
            );
            hang_up(server, client);
        }
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

// ── The node's own info over a stream (AN3, ARCHITECTURE.md "About and node
// info") ───────────────────────────────────────────────────────────────────
//
// The same answer the bus gives (`tests/dbus_client.rs`): the node's operating
// system, the word its Vulkan read answered, and the kernel's GPU rows with the
// Vulkan driver joined onto the row whose bus id the device names. `NodeInfo`
// is the caller's own read, gated on `control` asked WITHOUT interaction;
// `NodeInfoAs` is the bff's, for a current member of `pai`. A refused read
// spawns no Vulkan child: the seam's counter stays empty. Names the tests
// assume, the contract for the coder: `ManagerClient::node_info()` and the
// stream's `NodeInfo` / `NodeInfoAs`.

/// The node the seam answers, and the document the client must hand back: the
/// same fixture and the same joined rows the bus block pins.
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

/// A core answering the node facts of [`node_facts`], with the bff's account
/// known over the fake accounts (alice a member of `pai`, bob not), and polkit
/// allowing the quiet check as `quiet`. The seam's `asked` list comes back.
fn node_info_core(quiet: bool) -> (Arc<Core>, Arc<std::sync::Mutex<Vec<String>>>) {
    use crate::manager::service::fakes::{FakeGroup, FakePasswords};
    let seam = node_facts();
    let asked = seam.asked.clone();
    let core = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true)
            .quietly(quiet)
            .bff(&Caller(BFF_SID.into())),
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

/// pai-core's `node_info` against the stream server: the bff's own read, the
/// member's read vouched for, and both refusals. No file is read, so a plain
/// test.
#[test]
fn the_pipe_client_reads_the_node_info() {
    let alice = Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into());

    // The bff's own read, and the member's read it vouches for.
    let (c, asked) = node_info_core(true);
    let (server, client) = connect(&c, &Caller(BFF_SID.into()));
    assert_eq!(
        client.node_info(),
        Ok(node_document()),
        "the document of the fakes, joined and in bus id order"
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
        client.acting_for("alice").node_info(),
        Ok(node_document()),
        "the member's read, vouched for by the bff"
    );
    assert_eq!(
        client.acting_for("bob").node_info(),
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
    hang_up(server, client);

    // A caller polkit refuses the quiet check: refused, and no child.
    let (c, asked) = node_info_core(false);
    let (server, client) = connect(&c, &alice);
    assert_eq!(
        client.node_info(),
        Err(ClientError::NotAuthorized),
        "no control, no node info"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        Vec::<String>::new(),
        "a refused read spawns no Vulkan child"
    );
    hang_up(server, client);

    // A caller that is not the bff, vouching for a member: refused, no child.
    let (c, asked) = node_info_core(true);
    let (server, client) = connect(&c, &alice);
    assert_eq!(
        client.acting_for("alice").node_info(),
        Err(ClientError::NotAuthorized),
        "only the bff's account vouches"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        Vec::<String>::new(),
        "a forged pair spawns no Vulkan child"
    );
    hang_up(server, client);
}

/// Creating and deleting a model profile over the stream, with pai-core's
/// client (slice MP1, its rules of IP3b): the three sources of a create, the
/// clone keeping the source's lines, every rule's refusal decoded by the client,
/// the delete's section alone out of the pool, a caller the authority refuses
/// answered NotAuthorized with nothing done, and the `As` forms for a member of
/// `pai` and an outsider. A create writes the pool and joins no entry, so it
/// moves no render; a model an entry lists is not free.
pub(crate) fn manager_pipe_client_model_profiles_e2e() {
    use crate::manager::service::fakes::FakePasswords;
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
    let refused = |key: &'static str, values: &[&str]| -> ClientError {
        ClientError::Invalid(Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        })
    };

    let c = core(true, &[]);
    let alice = Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into());
    let (server, client) = connect(&c, &alice);

    // ── The three sources ──────────────────────────────────────────────────
    assert_eq!(
        client.create_model_profile(
            "pipe-model",
            &ModelProfileSource::Model("/models/pipe-model.gguf".into())
        ),
        Ok(()),
        "a profile of a model file of the node"
    );
    assert_eq!(
        section(&pool_text(), "pipe-model"),
        "[pipe-model]\nmodel = /models/pipe-model.gguf\n",
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
        "the mode of the pool file a create writes, over the stream as over the bus"
    );
    assert_eq!(
        client.create_model_profile(
            "pipe-hf",
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
            "pipe-repo",
            &ModelProfileSource::Hf {
                repo: "org/other".into(),
                file: None,
            }
        ),
        Ok(()),
        "a profile of a repository with no file named"
    );
    assert_eq!(
        section(&pool_text(), "pipe-hf"),
        "[pipe-hf]\nhf-file = Q4_K_M.gguf\nhf-repo = org/repo\n",
        "the pick's two keys, sorted: {}",
        pool_text()
    );
    assert_eq!(
        section(&pool_text(), "pipe-repo"),
        "[pipe-repo]\nhf-repo = org/other\n",
        "a repository alone is one key, and no `model`: {}",
        pool_text()
    );

    // ── The clone: the source's lines, kept ────────────────────────────────
    assert_eq!(
        client.create_model_profile("pipe-clone", &ModelProfileSource::From("off-one".into())),
        Ok(()),
        "a copy of a model no entry lists"
    );
    assert_eq!(
        section(&pool_text(), "pipe-clone"),
        // The source's lines with its trailing blank separator left out
        // (MP1, ARCHITECTURE.md done-when): the copy ends with one ending.
        format!(
            "[pipe-clone]\n{}\n",
            section(&pool, "off-one")["[off-one]\n".len()..].trim_end_matches('\n')
        ),
        "the source's comment, keys and order, under the new header, no blank at the \
         end: {}",
        pool_text()
    );
    assert_eq!(
        client.create_model_profile("pipe-on-clone", &ModelProfileSource::From("On-One".into())),
        Ok(()),
        "the source of a model an entry lists: the copy joins no entry, so it serves nobody"
    );
    assert_eq!(
        section(&pool_text(), "pipe-on-clone"),
        format!(
            "[pipe-on-clone]\n{}\n",
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
            Err(refused("model-profile-name-taken", &["OFF-ONE"])),
            "a name the pool holds in the other case",
        ),
        (
            "a b",
            ModelProfileSource::Model("/m/x.gguf".into()),
            Err(refused("invalid-model-profile-id", &["a b"])),
            "an id with a space",
        ),
        (
            "pipe-nosuch",
            ModelProfileSource::From("no-such-profile".into()),
            Err(refused("no-model-profile-named", &["no-such-profile"])),
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
        !pool_text().contains("[pipe-nosuch]"),
        "a refused create wrote nothing: {}",
        pool_text()
    );

    // ── The delete: the section alone, out of the pool ─────────────────────
    assert_eq!(
        client.delete_model_profile("free-me").map(|_| ()),
        Ok(()),
        "a model no entry lists"
    );
    assert_eq!(section(&pool_text(), "free-me"), "", "its section is gone");
    assert!(
        section(&pool_text(), "pipe-clone").contains("a note"),
        "the neighbour's section is kept: {}",
        pool_text()
    );
    for (id, want, why) in [
        (
            "On-One",
            Err(refused("model-profile-assigned", &["On-One", "gpu"])),
            "the entry `[gpu/On-One]` lists it",
        ),
        (
            "twin",
            Err(refused("model-profile-assigned", &["twin", "gpu"])),
            "the entry `[gpu/twin]`, naming the profile once though two instances run it",
        ),
        (
            "TWO-PLACES",
            Err(refused("model-profile-assigned", &["TWO-PLACES", "gpu"])),
            "the entry `[gpu/two-places]`, in the other letter case: the retired record's \
             keys are no rule",
        ),
        (
            "Retired",
            Err(refused("no-model-profile-named", &["Retired"])),
            "a section of the retired switched-off file is no proof it exists",
        ),
        (
            "gone",
            Err(refused("no-model-profile-named", &["gone"])),
            "a model the pool does not hold",
        ),
        (
            "*",
            Err(refused("no-model-profile-named", &["*"])),
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
        vec![(alice.clone(), Action::Configure); 15],
        "every verb asked configure of the connection's caller, once per call"
    );
    assert_eq!(
        calls(&c),
        vec![],
        "no verb of a model profile touches a unit"
    );
    hang_up(server, client);

    // ── A caller the authority refuses ─────────────────────────────────────
    let c = core(false, &[]);
    let (server, client) = connect(&c, &alice);
    assert_eq!(
        client.create_model_profile("pipe-no", &ModelProfileSource::Model("/m/a.gguf".into())),
        Err(ClientError::NotAuthorized),
        "a caller polkit refuses configure"
    );
    assert_eq!(
        client.delete_model_profile("pipe-clone"),
        Err(ClientError::NotAuthorized),
        "the same for a delete"
    );
    assert!(
        !pool_text().contains("[pipe-no]")
            && section(&pool_text(), "pipe-clone").contains("a note"),
        "the refused calls wrote and removed nothing: {}",
        pool_text()
    );
    hang_up(server, client);

    // ── The `As` names: a member's calls, vouched for by the bff ───────────
    let passwords = FakePasswords::new();
    let asked_admin = passwords.asked_admin.clone();
    let (c, _checked) = auth_core(passwords);
    let (server, client) = connect(&c, &Caller(BFF_SID.into()));
    assert_eq!(
        client.acting_for("alice").create_model_profile(
            "as-mp",
            &ModelProfileSource::Model("/models/as.gguf".into())
        ),
        Ok(()),
        "the member's create, vouched for by the bff"
    );
    assert_eq!(
        section(&pool_text(), "as-mp"),
        "[as-mp]\nmodel = /models/as.gguf\n",
        "the profile the member's create wrote, in the pool: {}",
        pool_text()
    );
    assert_eq!(
        client
            .acting_for("bob")
            .create_model_profile("as-bob", &ModelProfileSource::Model("/m/b.gguf".into())),
        Err(ClientError::NotAuthorized),
        "bob is no member of `pai`"
    );
    assert_eq!(
        client
            .acting_for("alice")
            .delete_model_profile("two-places"),
        Err(ClientError::Invalid(Refusal {
            key: "model-profile-assigned",
            values: vec!["two-places".into(), "gpu".into()],
        })),
        "the member's delete of a model an entry lists is the rule's refusal, naming the \
         instance profile that lists it"
    );
    assert_eq!(
        client
            .acting_for("alice")
            .delete_model_profile("as-mp")
            .map(|_| ()),
        Ok(()),
        "and the member's delete of its own profile"
    );
    assert_eq!(
        client.acting_for("bob").delete_model_profile("pipe-clone"),
        Err(ClientError::NotAuthorized),
        "the same for a delete"
    );
    assert!(
        !pool_text().contains("[as-bob]") && section(&pool_text(), "pipe-clone").contains("a note"),
        "a refusal of the `As` form wrote and removed nothing: {}",
        pool_text()
    );
    assert_eq!(
        *asked_admin.lock().unwrap(),
        vec![
            "alice".to_string(),
            "bob".into(),
            "alice".into(),
            "alice".into(),
            "bob".into()
        ],
        "the membership of the vouched name, asked once per call, never cached"
    );
    assert_eq!(
        calls(&c),
        vec![],
        "no verb of a model profile touches a unit, for a member either"
    );
    hang_up(server, client);
}

/// The instance profile verbs over the stream, with pai-core's client (IP3a,
/// `ARCHITECTURE.md` "Instance profiles", slice IP3 part a): what the client
/// hands back of each answer (the list rows, the single read with the pool's
/// text as its revision), every refusal decoded into its `ClientError`
/// (`NoInstanceProfile`, `InstanceProfileExists`, `InstanceProfileInUse` with
/// the profile and the instance, `NoServerProfile`, `NoInstance`, `Changed`,
/// `AddressInUse` with who holds the address, `Invalid` with the key and the
/// values), and the bytes each accepted verb leaves in `instance-profiles.ini`
/// and `instances.ini`. Every verb asks polkit `configure` of the connection's
/// caller once; the `As` forms, vouched for by the bff, ask polkit nothing and
/// ask the membership of the name once per call.
pub(crate) fn manager_pipe_client_instance_profiles_e2e() {
    use crate::manager::service::fakes::FakePasswords;

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
    // Each of the two launched on the pair standing now (IP3c), the render the
    // fixture wrote: every write that renders moves a pair off that record, so
    // the writes below answer a real drift and ask those instances' states.
    for id in ["default", "lab"] {
        crate::manager::service::write_applied(id).expect("the applied record");
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
    let refused = |key: &'static str, values: &[&str]| -> ClientError {
        ClientError::Invalid(Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        })
    };
    let bad_port = |port: &str| {
        ClientError::Invalid(Refusal {
            key: "not-whole-number-in-range",
            values: vec!["Port".into(), port.into()],
        })
    };

    let c = core(true, &[]);
    let alice = Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into());
    let (server, client) = connect(&c, &alice);

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
        "the list answers each profile with its server profile and its entries, in \
         the file's order, and no revision: the pool is one text"
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
        "the revision is the text of `instance-profiles.ini` the read returned"
    );
    assert_eq!(
        client.instance_profile("nope"),
        Err(ClientError::NoInstanceProfile("nope".into())),
        "a profile the pool has no section for, decoded by the client"
    );

    // ── The create: the refusals first, the pool untouched ────────────────
    assert_eq!(
        client.create_instance_profile("Bad Id", "lab"),
        Err(refused("instance-profile-bad-id", &["Bad Id"])),
        "an id the rule refuses"
    );
    assert_eq!(
        client.create_instance_profile("default", "lab"),
        Err(ClientError::InstanceProfileExists("default".into())),
        "a name the pool has"
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
        client.add_instance_profile_model("nope", "alpha"),
        Err(ClientError::NoInstanceProfile("nope".into())),
        "an entry of no profile"
    );
    assert_eq!(
        client.add_instance_profile_model("default", "nope"),
        Err(refused("no-model-profile-named", &["nope"])),
        "an entry of no model profile"
    );
    let before = pool();
    assert_eq!(
        client
            .add_instance_profile_model("default", "beta")
            .map(|_| ()),
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
        client
            .remove_instance_profile_model("default", "alpha")
            .map(|_| ()),
        Ok(()),
        "the entry removal lands"
    );
    assert_eq!(
        pool(),
        pai_core::ini::text::delete_section(&before, "default/alpha")
            .expect("the entry the pool held"),
        "the entry section is cut out, every other byte kept"
    );
    assert!(
        pool().contains("[default/beta]"),
        "the removal leaves the profile's other entries: {}",
        pool()
    );
    let kept = pool();
    assert_eq!(
        client.remove_instance_profile_model("default", "gone"),
        Err(refused("no-instance-profile-model", &["default", "gone"])),
        "removing an entry the profile does not list is its own conflict, naming the \
         profile and the model"
    );
    assert_eq!(pool(), kept, "and it wrote no byte");

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
        client
            .set_instance_profile("lab", "default", &rev)
            .map(|_| ()),
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
    assert_eq!(pool(), rev, "the switch never writes the pool");
    let render = std::fs::read_to_string(sys.join("models@lab.ini")).expect("render");
    assert!(
        sections(&render).contains(&"beta".to_string()),
        "the switch re-renders the instance as the profile it now runs: {:?}",
        sections(&render)
    );

    // ── `SetInstanceAddress`: the record's address, the record's text ─────
    let rec = record();
    assert_eq!(
        client.set_instance_address("ghost", "127.0.0.1", "8595", &rec),
        Err(ClientError::NoInstance),
        "an instance the record has no section for"
    );
    assert_eq!(
        client.set_instance_address("default", "127.0.0.1", "8595", "STALE TEXT"),
        Err(ClientError::Changed),
        "a revision the record no longer holds"
    );
    for port in ["not-a-port", "0", "70000"] {
        let answer = client.set_instance_address("default", "127.0.0.1", port, &rec);
        match answer {
            // The key, the key's name and the value: the bounds are the check's.
            Err(ClientError::Invalid(r)) => {
                let want = match bad_port(port) {
                    ClientError::Invalid(want) => want,
                    _ => unreachable!(),
                };
                assert_eq!(r.key, want.key, "the port `{port}`: {r:?}");
                assert_eq!(
                    (
                        r.values.first().map(String::as_str),
                        r.values.get(1).map(String::as_str)
                    ),
                    (
                        want.values.first().map(String::as_str),
                        want.values.get(1).map(String::as_str)
                    ),
                    "the refusal names the key and the value: {r:?}"
                );
            }
            other => panic!(
                "the port `{port}` is refused as {other:?}, not as a \
                            number out of range"
            ),
        }
    }
    assert_eq!(
        client.set_instance_address("default", "127.0.0.1", "8681", &rec),
        Err(ClientError::AddressInUse("lab".into())),
        "the address another instance holds, naming it (`lab` binds 127.0.0.1:8681)"
    );
    assert_eq!(
        client.set_instance_address("default", "127.0.0.1", "8590", &rec),
        Err(ClientError::AddressInUse("private-ai-bff".into())),
        "the web service's address, named as its own"
    );
    assert_eq!(record(), rec, "every refused address wrote no record");
    assert_eq!(
        client
            .set_instance_address("default", "127.0.0.1", "8595", &rec)
            .map(|_| ()),
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
    // The empty host removes the key, the port still lands.
    assert_eq!(
        client
            .set_instance_address("lab", "", "8692", &rec)
            .map(|_| ()),
        Ok(()),
        "the empty host is accepted"
    );
    let rec = record();
    assert!(
        !pai_core::ini::section_keys(&rec, "lab")
            .iter()
            .any(|(k, _)| k == "Hostname"),
        "the empty host removes `Hostname`: {:?}",
        pai_core::ini::section_keys(&rec, "lab")
    );
    assert_eq!(
        key_of(&rec, "lab", "Port").as_deref(),
        Some("8692"),
        "the clear of the host still sets the port"
    );

    // ── The delete: the profile an instance runs first ────────────────────
    assert_eq!(
        client.delete_instance_profile("nope"),
        Err(ClientError::NoInstanceProfile("nope".into())),
        "a profile the pool has no section for"
    );
    assert_eq!(
        client.delete_instance_profile("default"),
        Err(ClientError::InstanceProfileInUse(
            "default".into(),
            "default".into()
        )),
        "the profile an instance runs, naming the instance"
    );
    assert_eq!(
        client.delete_instance_profile("lab"),
        Err(ClientError::InstanceProfileInUse(
            "default".into(),
            "lab".into()
        )),
        "`lab` runs `default` since the switch above: the refusal names that instance"
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

    // ── `CreateInstance` proposes the port ────────────────────────────────
    // The node's instances sit on 8595 (`default`, moved above) and 8692
    // (`lab`, moved above) and the web service on 8590, so the first free port
    // at or above 8581 is 8581, and the next create takes the next free one.
    assert_eq!(
        client.create_instance("llama.cpp", "pipe-new", "edge"),
        Ok(8581),
        "the create lands, answering the first free port"
    );
    assert_eq!(
        key_of(&record(), "pipe-new", "Port").as_deref(),
        Some("8581"),
        "the record's section carries the proposed port"
    );
    assert_eq!(
        key_of(&record(), "pipe-new", "Hostname").as_deref(),
        None,
        "the create names no host: the profile's bind stands"
    );
    assert_eq!(
        client.create_instance("llama.cpp", "pipe-new2", "edge"),
        Ok(8582),
        "the second create lands, given the next free port"
    );
    assert_eq!(
        key_of(&record(), "pipe-new2", "Port").as_deref(),
        Some("8582"),
        "the second create is given the next free port, not 8581 again"
    );

    assert_eq!(
        asked(&c),
        vec![(alice.clone(), Action::Configure); 32],
        "every verb asked configure of the connection's caller, once per call, the \
         reads too: the pool's read is a `configure` read"
    );
    let states: Vec<String> = calls(&c)
        .into_iter()
        .map(|call| match call {
            Call::Status(id) => id,
            other => panic!("IP3c: a save of this phase asks a state, no unit verb: {other:?}"),
        })
        .collect();
    assert!(
        states.iter().all(|id| id == "default" || id == "lab")
            && states.iter().any(|id| id == "default")
            && states.iter().any(|id| id == "lab"),
        "IP3c: every ask is the state of one of the record's two instances, each of which the \
         fixture launched on the pair standing then: `pipe-new` and `pipe-new2` are created by \
         this phase, so they applied nothing and no save asks them. Which write answers which \
         pair, in record order, is pinned with every pair named in `tests::convergence` and \
         `tests::instance_profile_entries`: {states:?}"
    );
    hang_up(server, client);

    // ── A caller the authority refuses ────────────────────────────────────
    let c = core(false, &[]);
    let (server, client) = connect(&c, &alice);
    assert_eq!(
        client.create_instance_profile("pipe-no", "lab"),
        Err(ClientError::NotAuthorized),
        "a caller polkit refuses configure"
    );
    assert_eq!(
        client.delete_instance_profile("edge"),
        Err(ClientError::NotAuthorized),
        "the same for a delete"
    );
    assert_eq!(
        client.set_instance_profile("lab", "edge", pool_seed),
        Err(ClientError::NotAuthorized),
        "and for the switch"
    );
    assert_eq!(
        client.instance_profiles(),
        Err(ClientError::NotAuthorized),
        "and for the list"
    );
    assert!(
        !pool().contains("[pipe-no]") && pool().contains("[edge]"),
        "the refused calls wrote and removed nothing: {}",
        pool()
    );
    hang_up(server, client);

    // ── The `As` names: a member's calls, vouched for by the bff ──────────
    let passwords = FakePasswords::new();
    let asked_admin = passwords.asked_admin.clone();
    let (c, _checked) = auth_core(passwords);
    let (server, client) = connect(&c, &Caller(BFF_SID.into()));
    assert_eq!(
        client
            .acting_for("alice")
            .create_instance_profile("as-ip", "lab"),
        Ok(()),
        "the member's create, vouched for by the bff"
    );
    assert!(
        pool().contains("[as-ip]\nServerProfile = lab\n"),
        "the profile the member's create wrote: {}",
        pool()
    );
    assert_eq!(
        client.acting_for("alice").instance_profile("as-ip"),
        Ok(pai_core::client::InstanceProfileInfo {
            id: "as-ip".into(),
            server_profile: "lab".into(),
            entries: Vec::new(),
            revision: pool(),
        }),
        "the member's read answers the profile, with the pool's text as its revision"
    );
    assert_eq!(
        client
            .acting_for("alice")
            .add_instance_profile_model("as-ip", "gamma")
            .map(|_| ()),
        Ok(()),
        "the member's entry add"
    );
    assert_eq!(
        client
            .acting_for("alice")
            .set_instance_profile("lab", "as-ip", pool().as_str())
            .map(|_| ()),
        Ok(()),
        "the member's switch of `lab` onto the member's profile"
    );
    assert_eq!(
        client.delete_instance_profile("as-ip"),
        Err(ClientError::InstanceProfileInUse(
            "as-ip".into(),
            "lab".into()
        )),
        "the profile the instance now runs is refused, naming it: the plain verb on \
         the bff's own connection is authorized as the bff"
    );
    assert_eq!(
        client
            .acting_for("alice")
            .remove_instance_profile_model("as-ip", "gamma")
            .map(|_| ()),
        Ok(()),
        "the member's entry removal"
    );
    assert_eq!(
        client
            .acting_for("alice")
            .set_instance_address("lab", "127.0.0.1", "8699", record().as_str())
            .map(|_| ()),
        Ok(()),
        "the member's address write"
    );
    assert_eq!(
        (
            key_of(&record(), "lab", "Hostname").as_deref(),
            key_of(&record(), "lab", "Port").as_deref()
        ),
        (Some("127.0.0.1"), Some("8699")),
        "the record carries the address the member asked for"
    );
    assert_eq!(
        client
            .acting_for("bob")
            .create_instance_profile("as-bob", "lab"),
        Err(ClientError::NotAuthorized),
        "bob is no member of `pai`"
    );
    assert_eq!(
        client.acting_for("bob").delete_instance_profile("as-ip"),
        Err(ClientError::NotAuthorized),
        "the same for a delete"
    );
    assert_eq!(
        client.acting_for("bob").instance_profiles(),
        Err(ClientError::NotAuthorized),
        "and for a read"
    );
    assert!(
        !pool().contains("[as-bob]") && pool().contains("[as-ip]"),
        "a refusal of the `As` form wrote and removed nothing: {}",
        pool()
    );
    assert_eq!(
        *asked_admin.lock().unwrap(),
        vec![
            "alice".to_string(),
            "alice".into(),
            "alice".into(),
            "alice".into(),
            "alice".into(),
            "alice".into(),
            "bob".into(),
            "bob".into(),
            "bob".into()
        ],
        "the membership of the vouched name, asked once per call, never cached"
    );
    assert_eq!(
        asked(&c),
        vec![(Caller(BFF_SID.into()), Action::Configure)],
        "the `As` calls ask polkit nothing: only the plain verb on the bff's own \
         connection asked, of the bff"
    );
    let states: Vec<String> = calls(&c)
        .into_iter()
        .map(|call| match call {
            Call::Status(id) => id,
            other => panic!("IP3c: a save of this phase asks a state, no unit verb: {other:?}"),
        })
        .collect();
    assert!(
        states.iter().all(|id| id == "default" || id == "lab")
            && states.iter().any(|id| id == "default")
            && states.iter().any(|id| id == "lab"),
        "IP3c: every ask is the state of one of the two instances that recorded the pair their \
         launch started with: `pipe-new` and `pipe-new2` were created by this phase, so they \
         applied nothing and no save asks them. The exact set each write answers, in record \
         order, is pinned with every pair named in `tests::convergence` and \
         `tests::instance_profile_entries`: {states:?}"
    );
    hang_up(server, client);
}

/// M3 of "Choosing the model file on the web": `ModelFiles` / `ModelFilesAs`
/// over the stream, with pai-core's client. The frame the picker reads crosses:
/// the folder as an absolute path, every row with its path, size, shard count
/// and category (null for a file of the folder itself), the flag saying the
/// walk stopped at its bound, and the folder's keyed why INSIDE a success. The
/// client reads every key strictly, so this is the frame's pin; the listing's
/// rules are pinned by `manager_model_files_e2e`. The read opens a folder under
/// this phase's redirect, so it lives in the e2e.
pub(crate) fn manager_pipe_client_model_files_e2e() {
    use crate::manager::service::fakes::{FakeGroup, FakePasswords};
    use pai_core::client::{ModelFileRow, Reason};

    let alice = Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into());

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
                    .bff(&Caller(BFF_SID.into())),
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

    // ── The bff's own read, and the member's read it vouches for ───────────
    let c = core(true, 50_000);
    let (server, client) = connect(&c, &Caller(BFF_SID.into()));
    assert_eq!(
        client.model_files().map(frame),
        Ok((models.to_string_lossy().into_owned(), want(), false, None)),
        "the folder, the three rows with every field (the second shard, the txt \
         and the dot-folder left out), the flag false and no why"
    );
    assert_eq!(
        *c.authority().asked_quietly.lock().unwrap(),
        vec![(Caller(BFF_SID.into()), Action::Control)],
        "control asked without interaction"
    );
    assert_eq!(
        *c.authority().asked.lock().unwrap(),
        Vec::<(Caller, Action)>::new(),
        "never asked with a prompt: a file list is worth no challenge"
    );
    assert_eq!(
        client.acting_for("alice").model_files().map(frame),
        Ok((models.to_string_lossy().into_owned(), want(), false, None)),
        "the member's read, vouched for by the bff: the same rows"
    );
    assert_eq!(
        client.acting_for("bob").model_files(),
        Err(ClientError::NotAuthorized),
        "bob is no member of `pai`: no paths of the node's models folder"
    );
    assert_eq!(
        *c.authority().asked_quietly.lock().unwrap(),
        vec![(Caller(BFF_SID.into()), Action::Control)],
        "the principal gate decides the right: polkit is asked nothing more"
    );
    assert_eq!(
        *c.service().calls.lock().unwrap(),
        Vec::<Call>::new(),
        "the read reaches no service manager"
    );
    hang_up(server, client);

    // ── A caller that is not the bff, vouching for a member: refused ───────
    let c = core(true, 50_000);
    let (server, client) = connect(&c, &alice);
    assert_eq!(
        client.acting_for("alice").model_files(),
        Err(ClientError::NotAuthorized),
        "only the bff's account vouches"
    );
    hang_up(server, client);

    // ── A caller polkit refuses the quiet check: refused ───────────────────
    let c = core(false, 50_000);
    let (server, client) = connect(&c, &alice);
    assert_eq!(
        client.model_files(),
        Err(ClientError::NotAuthorized),
        "no control, no list"
    );
    hang_up(server, client);

    // ── The flag: a walk that stops at its bound says so across the stream ──
    let c = core(true, 2);
    let (server, client) = connect(&c, &Caller(BFF_SID.into()));
    let got = client.model_files().expect("served");
    assert!(
        got.truncated,
        "the walk stopped after 2 entries of the 5 the folder holds: {got:?}"
    );
    assert!(
        got.files.len() <= 2,
        "the list is cut with the flag: {}",
        got.files.len()
    );
    hang_up(server, client);

    // ── A folder that is not there: the keyed why inside a success ─────────
    let c = core(true, 50_000);
    let (server, client) = connect(&c, &Caller(BFF_SID.into()));
    std::fs::remove_dir_all(&models).expect("remove models dir");
    assert_eq!(
        client.model_files(),
        Ok(pai_core::client::ModelFilesReply {
            dir: models.to_string_lossy().into_owned(),
            files: Vec::new(),
            truncated: false,
            error: Some(Reason {
                key: "models-dir-unreadable".into(),
                args: vec![models.to_string_lossy().into_owned()],
            }),
        }),
        "the stream carries the why as the keyed pair, a null being the folder \
         having read: the picker shows an empty list with a text"
    );
    hang_up(server, client);
}

/// E2E phase: IP4a, over the stream. The same install and the same pins as
/// `manager_dbus_client_ip4a_e2e`: every write that re-renders answers `running`
/// and `notConverged` (IP3c put the JSON object on the frame's `ok`), and here
/// pai-core's stream client turns it back into `pai_core::client::Affected`,
/// pinned against the rows the manager itself reports at the moment of the answer.
/// The two methods IP4a adds, the entry save and the server profile switch, are
/// asked of the frame: the entry save, whose arm the stream has never had, with
/// the entry's own refusals, and the switch, whose card taken away is no refusal
/// but a not-converged row of the instance that kept its last good pair.
#[cfg(unix)]
pub(crate) fn manager_pipe_client_ip4a_e2e() {
    use crate::manager::service::fakes::{FakeGpus, FakeGroup, FakePasswords};
    use crate::manager::service::ManagerError;
    use pai_core::client::{Affected, NotConvergedRow};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    // The install of the D-Bus phase: `desk` runs `desk-gpu` on `CUDA0`, `lab`
    // runs `lab-gpu` on `CUDA1`, both entries grown to 131 072 tokens, `free` on
    // the roomy card and `tight` on the card a switch takes away.
    let path = crate::tests::dbus_client::seed_ip4a(dir.path(), &sys);
    let (big, small) = crate::tests::dbus_client::cards_ip4a(&path);
    let service = FakeService {
        running: vec!["desk".to_string()],
        state: UnitState::Inactive,
        ..FakeService::new()
    };
    let c = Arc::new(
        Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
            .with_group(FakeGroup::new())
            .with_gpus(FakeGpus::new(&[("CUDA0", big), ("CUDA1", small)])),
    );
    c.start_up();
    assert_eq!(
        c.render_failures()
            .iter()
            .map(|(id, err)| {
                (
                    id.clone(),
                    match err {
                        ManagerError::Invalid(r) => r.key.to_string(),
                        other => format!("{other:?}"),
                    },
                )
            })
            .collect::<Vec<_>>(),
        vec![("lab".to_string(), "model-profile-over-memory".to_string())],
        "IP4a: the seeded install renders `desk` and leaves `lab` over its memory"
    );
    crate::manager::service::write_applied("desk").expect("the applied record");
    let alice = Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into());
    let (server, client) = connect(&c, &alice);

    let pool = || std::fs::read_to_string(sys.join("models.ini")).expect("models.ini");
    let profiles = || {
        std::fs::read_to_string(sys.join("instance-profiles.ini")).expect("instance-profiles.ini")
    };
    let record = || std::fs::read_to_string(sys.join("instances.ini")).expect("instances.ini");
    let entry = |section: &str| -> Vec<(String, String)> {
        pai_core::ini::section_keys(&profiles(), section)
    };
    let key_of = |text: &str, section: &str, key: &str| -> Option<String> {
        pai_core::ini::section_keys(text, section)
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    };
    let refused = |key: &'static str, values: &[&str]| -> Result<Affected, ClientError> {
        Err(ClientError::Invalid(Refusal {
            key,
            values: values.iter().map(|v| v.to_string()).collect(),
        }))
    };
    let vals = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    };
    // The rows the design says a write answers, read from the manager at the
    // moment of the answer: `running` is the running instances the write touched
    // (`scope`, its render set) that drift from the pair they launched with, in id
    // order; the not-converged rows are every instance that kept its last good
    // pair, with the key of the check that stopped it and that check's args.
    let running_ids = ["desk"];
    let rows = |m: &Arc<Core>, scope: &[&str]| -> Affected {
        Affected {
            running: m
                .instances()
                .iter()
                .filter(|i| {
                    i.pending_restart
                        && scope.contains(&i.id.as_str())
                        && running_ids.contains(&i.id.as_str())
                })
                .map(|i| i.id.clone())
                .collect(),
            not_converged: m
                .render_failures()
                .iter()
                .map(|(id, err)| match err {
                    ManagerError::Invalid(r) => NotConvergedRow {
                        instance: id.clone(),
                        key: r.key.to_string(),
                        args: r.values.clone(),
                    },
                    other => panic!("IP4a: the install's refusals are keyed: {other:?}"),
                })
                .collect(),
        }
    };
    let row_of = |got: &Affected, id: &str| -> NotConvergedRow {
        got.not_converged
            .iter()
            .find(|r| r.instance == id)
            .unwrap_or_else(|| panic!("IP4a: the answer carries no row for {id}: {got:?}"))
            .clone()
    };
    let pin = |what: &str, got: &Affected, m: &Arc<Core>, scope: &[&str]| {
        assert_eq!(
            got,
            &rows(m, scope),
            "IP4a {what}: the client decodes the rows the manager reports at the moment \
             of the answer"
        );
    };

    // ── The pool's compare and swap: the rows of a write that re-renders both ─
    let warm = pool().replace("temp = 0.7", "temp = 0.9");
    assert_ne!(warm, pool(), "IP4a: the pool write is a real change");
    let moved = client
        .write_file_if("models.ini", &warm, &pool())
        .expect("IP4a: the compare and swap lands and answers its rows");
    pin("the pool save", &moved, &c, &["desk", "lab"]);
    assert_eq!(
        moved.running,
        vec!["desk".to_string()],
        "IP4a: the save moved the pair of the running `desk` off the pair it launched \
         with: {moved:?}"
    );
    assert_eq!(
        row_of(&moved, "lab").key,
        "model-profile-over-memory",
        "IP4a: `lab` kept its last good pair, and the client's row carries the key that \
         stopped it: {moved:?}"
    );
    let lab = row_of(&moved, "lab");
    assert_eq!(
        (lab.args.first(), lab.args.get(1)),
        (Some(&"alpha".to_string()), Some(&"lab".to_string())),
        "IP4a: the row carries the check's own args, naming the model and the instance profile \
         that do not fit: {moved:?}"
    );

    // ── The entry add and remove: the same rows, the render set of one profile ─
    let added = client
        .add_instance_profile_model("desk-gpu", "beta")
        .expect("IP4a: the entry add lands and answers its rows");
    pin("the entry add", &added, &c, &["desk"]);
    assert!(
        profiles().contains("[desk-gpu/beta]"),
        "IP4a: the add wrote the entry of the profile `desk` runs: {}",
        profiles()
    );
    let removed = client
        .remove_instance_profile_model("desk-gpu", "beta")
        .expect("IP4a: the entry removal lands and answers its rows");
    pin("the entry remove", &removed, &c, &["desk"]);
    assert!(
        !profiles().contains("[desk-gpu/beta]"),
        "IP4a: the removal cut the entry: {}",
        profiles()
    );

    // ── The record's two writes answer the rows too ───────────────────────
    // The switch's revision is the pool's text (the profile's own), the address's
    // is the record's: each is the file the write compares against.
    let switched = client
        .set_instance_profile("desk", "lab-gpu", &profiles())
        .expect("IP4a: the switch lands and answers its rows");
    pin("the record's switch", &switched, &c, &["desk"]);
    assert_eq!(
        key_of(&record(), "desk", "InstanceProfile").as_deref(),
        Some("lab-gpu"),
        "IP4a: the record points `desk` at the profile the client asked for"
    );
    let back_on = client
        .set_instance_profile("desk", "desk-gpu", &profiles())
        .expect("IP4a: the switch back lands and answers its rows");
    pin("the record's switch back", &back_on, &c, &["desk"]);
    let rec = record();
    let addressed = client
        .set_instance_address("desk", "127.0.0.1", "8595", &rec)
        .expect("IP4a: the address lands and answers its rows");
    pin("the address", &addressed, &c, &["desk"]);

    // ── The server profile switch: a method IP4a adds ─────────────────────
    assert_eq!(
        client.set_instance_profile_server("ghost", "free"),
        Err(ClientError::NoInstanceProfile("ghost".into())),
        "IP4a: a profile the pool has no section for, decoded by the client"
    );
    assert_eq!(
        client.set_instance_profile_server("lab-gpu", "ghost"),
        Err(ClientError::NoServerProfile("ghost".into())),
        "IP4a: a server profile the node has no file for, decoded by the client"
    );
    let onto = client
        .set_instance_profile_server("lab-gpu", "free")
        .expect("IP4a: the switch lands and answers its rows");
    pin("the switch onto the roomy card", &onto, &c, &["lab"]);
    assert!(
        onto.not_converged.is_empty(),
        "IP4a: the switch moves the ground of every entry of the profile, and `free`'s \
         card finds the grown entry, so `lab` converges: {onto:?}"
    );
    assert_eq!(
        key_of(&profiles(), "lab-gpu", "ServerProfile").as_deref(),
        Some("free"),
        "IP4a: the switch wrote the `ServerProfile` of the instance profile"
    );
    let tight = client
        .set_instance_profile_server("desk-gpu", "tight")
        .expect("IP4a: a switch that takes a card away is no refusal");
    pin("the switch onto the tight card", &tight, &c, &["desk"]);
    assert_eq!(
        row_of(&tight, "desk").key,
        "model-profile-over-memory",
        "IP4a: the instance the switch left over the card is reported with the key that \
         stopped its render: {tight:?}"
    );
    let back = client
        .set_instance_profile_server("desk-gpu", "desk")
        .expect("IP4a: the switch back lands");
    pin("the switch back", &back, &c, &["desk"]);
    assert!(
        back.not_converged.is_empty(),
        "IP4a: the switch back onto `desk`'s card converges `desk` again: {back:?}"
    );

    // ── The entry save: a method IP4a adds, with the entry's own refusals ──
    let rev = client
        .instance_profile("desk-gpu")
        .expect("IP4a: the profile read")
        .revision;
    assert_eq!(
        rev,
        profiles(),
        "IP4a: the revision the read answers is the text of `instance-profiles.ini`"
    );
    let kept = profiles();
    assert_eq!(
        client.set_instance_profile_model("ghost", "alpha", &[], &rev),
        Err(ClientError::NoInstanceProfile("ghost".into())),
        "IP4a: an entry of a profile the pool has no section for, decoded by the client"
    );
    assert_eq!(
        client.set_instance_profile_model("desk-gpu", "zeta", &[], &rev),
        refused("no-instance-profile-model", &["desk-gpu", "zeta"]),
        "IP4a: the profile lists no such model, and the refusal names the profile and \
         the model"
    );
    assert_eq!(
        client.set_instance_profile_model("desk-gpu", "alpha", &[], "STALE TEXT"),
        Err(ClientError::Changed),
        "IP4a: a revision the pool no longer holds writes nothing"
    );
    match client.set_instance_profile_model(
        "desk-gpu",
        "alpha",
        &vals(&[("ctx-size", "abc")]),
        &rev,
    ) {
        Err(ClientError::Invalid(r)) => {
            assert_eq!(
                r.key, "not-whole-number-in-range",
                "IP4a: a value the entry's checks refuse is the keyed refusal, and it is \
                 not a row: {r:?}"
            );
            assert_eq!(
                r.values.first().map(String::as_str),
                Some("ctx-size"),
                "IP4a: the refusal names the key it stopped on: {r:?}"
            );
        }
        other => {
            panic!("IP4a: the entry save of a nonsense value answers a keyed refusal: {other:?}")
        }
    }
    assert_eq!(profiles(), kept, "IP4a: every refused save wrote no byte");
    let landed = client
        .set_instance_profile_model("desk-gpu", "alpha", &vals(&[("ctx-size", "1024")]), &rev)
        .expect("IP4a: the entry save lands over the frame");
    pin("the entry save", &landed, &c, &["desk"]);
    assert_eq!(
        landed.running,
        vec!["desk".to_string()],
        "IP4a: the entry the save shrank moved the pair of the running `desk`: {landed:?}"
    );
    assert!(
        entry("desk-gpu/alpha")
            .iter()
            .any(|(k, v)| k == "ctx-size" && v == "1024"),
        "IP4a: the save wrote the entry of [desk-gpu/alpha]: {}",
        profiles()
    );
    assert!(
        entry("lab-gpu/alpha")
            .iter()
            .any(|(k, v)| k == "ctx-size" && v == "131072"),
        "IP4a: the save of one profile left the other profile's entry alone: {}",
        profiles()
    );
    let render = std::fs::read_to_string(sys.join("models@desk.ini")).expect("the render");
    assert!(
        render.contains("ctx-size = 1024"),
        "IP4a: the entry the client saved rides the render of every instance running the \
         profile: {render}"
    );
    hang_up(server, client);

    // ── The authorization of the two methods IP4a adds ──────────────────
    // The two verbs are guarded as every write of a served file is, so over the
    // frame the principal gate and the `configure` right reach them too: a
    // `pai` member's call, vouched for by the bff's connection, lands; a name
    // outside `pai` is refused with no byte written; the plain name of the two
    // verbs, asked by a caller the authority refuses (outside `pai`), is refused.
    // A manager of its own over the same install.
    {
        let passwords = FakePasswords::new();
        let asked_admin = passwords.asked_admin.clone();
        let as_core = Arc::new(
            Manager::new(
                FakeService::new().running(&["desk"]),
                FakeAuthority::new(true).bff(&Caller(BFF_SID.into())),
                FakeResolver::lan(),
            )
            .with_group(FakeGroup::new())
            .with_passwords(passwords)
            .with_gpus(FakeGpus::new(&[("CUDA0", big), ("CUDA1", small)])),
        );
        let (server, client) = connect(&as_core, &Caller(BFF_SID.into()));

        // The switch: the member's lands, the outsider's writes nothing.
        client
            .acting_for("alice")
            .set_instance_profile_server("desk-gpu", "free")
            .map(|_| ())
            .expect("IP4a review: the member's switch lands over the frame, the bff vouching");
        assert_eq!(
            key_of(&profiles(), "desk-gpu", "ServerProfile").as_deref(),
            Some("free"),
            "IP4a review: the member's `As` switch wrote the profile's `ServerProfile`"
        );
        let kept = profiles();
        assert_eq!(
            client
                .acting_for("bob")
                .set_instance_profile_server("desk-gpu", "desk")
                .map(|_| ()),
            Err(ClientError::NotAuthorized),
            "IP4a review: the `As` switch of a name outside `pai` is refused"
        );
        assert_eq!(
            profiles(),
            kept,
            "IP4a review: the refused `As` switch wrote no byte: {kept}"
        );

        // The entry save: the member's lands, the outsider's writes nothing.
        let rev = client
            .acting_for("alice")
            .instance_profile("desk-gpu")
            .expect("IP4a review: the member reads the profile, the bff vouching")
            .revision;
        assert_eq!(
            rev,
            profiles(),
            "IP4a review: the read's revision is the text of `instance-profiles.ini`"
        );
        client
            .acting_for("alice")
            .set_instance_profile_model("desk-gpu", "alpha", &vals(&[("ctx-size", "4096")]), &rev)
            .map(|_| ())
            .expect("IP4a review: the member's entry save lands over the frame, the bff vouching");
        assert!(
            entry("desk-gpu/alpha")
                .iter()
                .any(|(k, v)| k == "ctx-size" && v == "4096"),
            "IP4a review: the member's `As` save wrote the entry: {}",
            profiles()
        );
        let kept = profiles();
        assert_eq!(
            client
                .acting_for("bob")
                .set_instance_profile_model(
                    "desk-gpu",
                    "alpha",
                    &vals(&[("ctx-size", "512")]),
                    &rev
                )
                .map(|_| ()),
            Err(ClientError::NotAuthorized),
            "IP4a review: the `As` entry save of a name outside `pai` is refused"
        );
        assert_eq!(
            profiles(),
            kept,
            "IP4a review: the refused `As` calls wrote no byte: {kept}"
        );
        assert_eq!(
            *asked_admin.lock().unwrap(),
            vec![
                "alice".to_string(),
                "bob".into(),
                "alice".into(),
                "alice".into(),
                "bob".into(),
            ],
            "IP4a review: the membership of the vouched name, asked once per `As` call, the \
             switch pair then the read and the entry-save pair"
        );
        assert_eq!(
            asked(&as_core),
            vec![],
            "IP4a review: the `As` calls ask polkit nothing: the right is the pair (the bff's \
             connection, a current member)"
        );
        hang_up(server, client);
    }

    // The plain names of the two verbs, asked by a caller outside `pai` (the
    // authority refuses it `configure`): refused, polkit asked of the caller,
    // no byte written.
    {
        let plain_core = Arc::new(
            Manager::new(
                FakeService::new(),
                FakeAuthority::new(false),
                FakeResolver::lan(),
            )
            .with_group(FakeGroup::new()),
        );
        let (server, client) = connect(&plain_core, &alice);
        let kept = profiles();
        assert_eq!(
            client
                .set_instance_profile_server("desk-gpu", "tight")
                .map(|_| ()),
            Err(ClientError::NotAuthorized),
            "IP4a review: the plain switch of a caller outside `pai` is refused"
        );
        assert_eq!(
            client
                .set_instance_profile_model(
                    "desk-gpu",
                    "alpha",
                    &vals(&[("ctx-size", "512")]),
                    &profiles()
                )
                .map(|_| ()),
            Err(ClientError::NotAuthorized),
            "IP4a review: the plain entry save of a caller outside `pai` is refused"
        );
        assert_eq!(
            profiles(),
            kept,
            "IP4a review: the plain refusals wrote no byte: {kept}"
        );
        assert_eq!(
            asked(&plain_core),
            [
                (alice.clone(), Action::Configure),
                (alice.clone(), Action::Configure),
            ],
            "IP4a review: each plain call asks polkit of the caller"
        );
        hang_up(server, client);
    }
}

// ── IP4b: the row's instance profile and host, the port a create answers ────
// ── (ARCHITECTURE.md, the IP4 entry, "Slices", IP4b) ────────────────────────

/// One raw frame asked of the server, on a connection of its own: the reply's
/// `ok` value. The row's keys are the frame's own, so the two new fields are
/// pinned where they cross the wire, not only where the client decodes them.
fn ask_raw(core: &Arc<Core>, caller: &Caller, request: serde_json::Value) -> serde_json::Value {
    use pai_core::client::pipe::{read_frame, write_frame};

    let (s0, s1) = UnixStream::pair().expect("socket pair");
    let core = core.clone();
    let caller = caller.clone();
    let server = std::thread::spawn(move || serve_stream(&core, caller, s0));
    let mut stream = s1;
    stream
        .set_read_timeout(Some(DEADLINE))
        .expect("read timeout");
    write_frame(&mut stream, request.to_string().as_bytes()).expect("write frame");
    let body = read_frame(&mut stream)
        .expect("read frame")
        .expect("the server answers");
    let reply: serde_json::Value = serde_json::from_slice(&body).expect("the reply is JSON");
    drop(stream);
    server
        .join()
        .expect("the server thread")
        .expect("a clean end");
    reply
        .get("ok")
        .cloned()
        .unwrap_or_else(|| panic!("an ok reply: {reply}"))
}

/// IP4b 3: `Version` answers, beside the framework version, the protocol number
/// of the methods this manager speaks. Nothing is read, so a plain test.
#[test]
fn the_pipe_client_reads_the_managers_protocol() {
    use pai_core::client::ManagerVersion;

    let c = core(true, &[]);
    let caller = Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into());
    let (server, client) = connect(&c, &caller);
    assert_eq!(
        client.version(),
        Ok(Some(ManagerVersion {
            version: env!("CARGO_PKG_VERSION").to_string(),
            // IP4b 3: the row grew `instanceProfile` and `hostname` and
            // `CreateInstance` answers the port, so the protocol is 4.
            protocol: 4,
        })),
        "the protocol the pipe client reads"
    );
    hang_up(server, client);
}

/// E2E phase: IP4b, over the stream. The same pins as
/// `manager_dbus_client_ip4b_e2e`, on the frame's own shape: the rows of
/// `Instances` and `ListInstances` carry the instance profile the section runs
/// and the host it sets, beside the states of the apply they already carried,
/// and pai-core's stream client decodes all five. `CreateInstance` takes the
/// instance profile and answers the port: 8581 on an empty record, 8582 with
/// 8581 taken, and a name the pool has no section for is the keyed refusal.
#[cfg(unix)]
pub(crate) fn manager_pipe_client_ip4b_e2e() {
    use crate::manager::service::fakes::{FakeGpus, FakeGroup};
    use serde_json::{json, Value};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    crate::tests::dbus_client::seed_ip4b(dir.path(), &sys);
    let c = Arc::new(
        Manager::new(
            FakeService::new(),
            FakeAuthority::new(true),
            FakeResolver::lan(),
        )
        .with_group(FakeGroup::new())
        .with_gpus(FakeGpus::new(&[("CUDA0", 400_000), ("CUDA1", 400_000)])),
    );
    c.start_up();
    // `desk` launched on a pair nobody launched with: converged, and drifting.
    let state = crate::paths::instance_state_dir().join("desk");
    std::fs::create_dir_all(&state).expect("the state folder");
    std::fs::write(state.join("applied"), "0".repeat(64)).expect("the applied record");
    let alice = Caller("S-1-5-21-3623811015-3361044348-30300820-1013".into());
    let (server, client) = connect(&c, &alice);

    // The row's key for the instance profile: the design names it
    // `instanceProfile` and the row's older keys are snake_case
    // (`server_profile`), so the pin accepts either spelling and pins the VALUE,
    // which is the rule.
    let profile_keys = ["instanceProfile", "instance_profile"];
    let row_of = |rows: &[Value], id: &str| -> Value {
        rows.iter()
            .find(|row| row.get("id").and_then(Value::as_str) == Some(id))
            .unwrap_or_else(|| panic!("IP4b: the rows carry {id}: {rows:?}"))
            .clone()
    };
    // The key is there, and its value: `null` is no host (the pipe has a null,
    // as the label's does), and the empty string says the same.
    let field = |row: &Value, names: &[&str]| -> Option<String> {
        let value = names
            .iter()
            .find_map(|name| row.get(*name))
            .unwrap_or_else(|| panic!("IP4b: the row carries a key of {names:?}: {row}"));
        match value {
            Value::Null => None,
            Value::String(text) => Some(text.clone()),
            other => panic!("IP4b: the field is text or null: {other}"),
        }
    };

    // ── IP4b 1: both frames' rows carry both fields, beside the states ─────
    for method in ["Instances", "ListInstances"] {
        let answer = ask_raw(&c, &alice, json!({"method": method}));
        let rows = answer
            .as_array()
            .expect("IP4b: the answer is an array of rows");
        let desk = row_of(rows, "desk");
        let lab = row_of(rows, "lab");
        assert_eq!(
            (
                field(&desk, &profile_keys).as_deref(),
                field(&desk, &["hostname"]).as_deref()
            ),
            (Some("desk"), Some("127.0.0.1")),
            "IP4b: {method} carries the instance profile `desk` runs and the host its \
             section sets: {rows:?}"
        );
        assert_eq!(
            field(&lab, &profile_keys).as_deref(),
            Some("one"),
            "IP4b: {method} carries the profile of the section that names no host: {rows:?}"
        );
        assert!(
            matches!(field(&lab, &["hostname"]).as_deref(), None | Some("")),
            "IP4b: {method} answers `lab` with no host, not with a host of the node's own: \
             {lab}"
        );
        assert_eq!(
            (
                desk.get("converged").and_then(Value::as_bool),
                desk.get("pendingRestart").and_then(Value::as_bool),
                field(&desk, &["reason"]),
            ),
            (Some(true), Some(true), None),
            "IP4b: the two new fields ride the row beside the states it already carried: \
             {desk}"
        );
        assert_eq!(
            (
                lab.get("converged").and_then(Value::as_bool),
                lab.get("pendingRestart").and_then(Value::as_bool),
                lab.get("reason")
                    .and_then(|r| r.get("key"))
                    .and_then(Value::as_str),
            ),
            (
                Some(false),
                Some(false),
                Some("model-profile-device-outside")
            ),
            "IP4b: the render the device check stopped rides its key on the same row: {lab}"
        );
        let args: Vec<&str> = lab
            .get("reason")
            .and_then(|r| r.get("args"))
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        assert!(
            args.contains(&"lab") && args.contains(&"CUDA1"),
            "IP4b: the reason's args name the instance and the card it stopped on: {args:?}"
        );
    }

    // ── IP4b 1: the client decodes both fields and the states, in one read ──
    let rows = client.instances().expect("IP4b: instances");
    let row = |id: &str| {
        rows.iter()
            .find(|i| i.id == id)
            .unwrap_or_else(|| panic!("IP4b: the list carries {id}: {rows:?}"))
            .clone()
    };
    let desk = row("desk");
    assert_eq!(
        (
            desk.instance_profile.as_deref(),
            desk.hostname.as_deref(),
            desk.server_profile.as_deref(),
            desk.port
        ),
        (Some("desk"), Some("127.0.0.1"), Some("desk"), 8581),
        "IP4b: the client's row carries the instance profile it runs and the host its \
         record names, beside the fields it carried"
    );
    assert_eq!(
        (desk.converged, desk.pending_restart, desk.reason.is_none()),
        (true, true, true),
        "IP4b: the new fields ride the row beside the states, in the one read"
    );
    let lab = row("lab");
    assert_eq!(
        (lab.instance_profile.as_deref(), lab.hostname.as_deref()),
        (Some("one"), None),
        "IP4b: a record that sets no host is answered as no host"
    );
    assert_eq!(
        (lab.converged, lab.pending_restart),
        (false, false),
        "IP4b: `lab` reads not converged, and not drifting: {lab:?}"
    );
    assert_eq!(
        lab.reason.as_ref().map(|r| r.key.as_str()),
        Some("model-profile-device-outside"),
        "IP4b: and its row carries the keyed reason"
    );

    // ── IP4b 2: the create answers the port, from the instance profile ──────
    std::fs::write(sys.join("instances.ini"), "").expect("the record emptied");
    let record = || std::fs::read_to_string(sys.join("instances.ini")).expect("instances.ini");
    let key_of = |text: &str, section: &str, key: &str| -> Option<String> {
        pai_core::ini::section_keys(text, section)
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    };
    assert_eq!(
        client.create_instance("llama.cpp", "first", "free"),
        Ok(8581),
        "IP4b 2: a create on an empty record answers 8581"
    );
    assert_eq!(
        (
            key_of(&record(), "first", "InstanceProfile").as_deref(),
            key_of(&record(), "first", "Port").as_deref(),
            key_of(&record(), "first", "ServerProfile"),
            key_of(&record(), "first", "Hostname"),
        ),
        (Some("free"), Some("8581"), None, None),
        "IP4b 2: the section names the instance profile the create was given and carries \
         the port it answered: no server profile, no host"
    );
    assert_eq!(
        client.create_instance("llama.cpp", "second", "free"),
        Ok(8582),
        "IP4b 2: with 8581 taken, the create answers the next free one, and two instances \
         of one instance profile is no refusal"
    );
    assert_eq!(
        client.create_instance("llama.cpp", "third", "nope"),
        Err(ClientError::NoInstanceProfile("nope".into())),
        "IP4b 2: the third name is judged as an instance profile, and the refusal names it"
    );
    assert_eq!(
        pai_core::ini::parse_sections(&record())
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec!["first".to_string(), "second".to_string()],
        "IP4b 2: the refused create wrote no section"
    );
    hang_up(server, client);
}
