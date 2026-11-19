//! pai-core's stream client of the manager (`pai_core::client::pipe`)
//! against the real server (`manager::pipe::serve_stream`) over the core's
//! fakes, joined by a Unix socket pair: the named pipe's OS-neutral half.
//! Every client stream has a read deadline: a server that panics or hangs
//! fails the test, not hangs it.

use crate::manager::pipe::serve_stream;
use crate::manager::service::fakes::{Call, FakeAuthority, FakeResolver, FakeService};
use crate::manager::service::{plain, withheld, Action, Caller, Manager, UnitState};
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
    let model_profiles = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
    let record = "[default]\nServerProfile = default\n\n[broken]\nServerProfile = broken\n\n[lan-2]\nServerProfile = big\n";
    std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
    std::fs::write(sys.join("server@broken.ini"), broken_ini).expect("write broken");
    std::fs::write(sys.join("models.ini"), model_profiles).expect("write presets");
    std::fs::write(sys.join("instances.ini"), record).expect("write instances");
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
        assert_eq!(
            client.read_file("models.ini"),
            Ok(Some(plain(model_profiles)))
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
                Ok(Some(plain(model_profiles))),
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
        assert_eq!(
            client.set_model_profile("default", "Qwen3.8-27B", "on"),
            Err(ClientError::NotAuthorized),
            "no identity, no assignment"
        );
        assert!(
            !sys.join("assignments.ini").exists(),
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
