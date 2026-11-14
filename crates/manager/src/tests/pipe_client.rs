//! pai-core's stream client of the manager (`pai_core::client::pipe`)
//! against the real server (`manager::pipe::serve_stream`) over the core's
//! fakes, joined by a Unix socket pair: the named pipe's OS-neutral half.
//! Every client stream has a read deadline: a server that panics or hangs
//! fails the test, not hangs it.

use crate::manager::pipe::serve_stream;
use crate::manager::service::fakes::{Call, FakeAuthority, FakeResolver, FakeService};
use crate::manager::service::{Action, Caller, Manager, UnitState};
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
    let sys = crate::paths::system_config_dir();
    std::fs::create_dir_all(&sys).expect("system config dir");
    let default_ini = "[Server]\nPort = 8080\nHostname = 0.0.0.0\n";
    let broken_ini = "[Server]\nPort = 8090\nHostname = 127.0.0.1\n";
    let presets = "[Qwen3.8-27B]\nmodel = /models/Qwen3.8-27B-Q8_0.gguf\n";
    std::fs::write(sys.join("server@default.ini"), default_ini).expect("write default");
    std::fs::write(sys.join("server@broken.ini"), broken_ini).expect("write broken");
    std::fs::write(sys.join("presets.ini"), presets).expect("write presets");
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
        assert_eq!(
            std::fs::read_to_string(sys.join("presets.ini")).unwrap(),
            presets,
            "a refused write leaves the file"
        );
        hang_up(server, client);
    }

    // ── A caller the authority refuses: asked as the connection's caller ─
    {
        let c = core(false, &[]);
        let (server, client) = connect(&c, &alice);
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
            presets
        );
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
        // Values with spaces, a backslash and a non-ASCII letter come back whole.
        assert_eq!(
            client.write_file("presets.ini", "[Qwen 3 (v2) é]\nmodel = /models/q.gguf\n"),
            Err(ClientError::Invalid(Refusal {
                key: "invalid-preset-id",
                values: vec!["Qwen 3 (v2) é".into()],
            }))
        );
        assert_eq!(
            client.write_file(
                "presets.ini",
                &format!(
                    "{presets}[m]\nmodel = /models/m.gguf\noverride-tensor = blk\\.0\\.ffn_up_exps\n"
                )
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
        assert_eq!(
            std::fs::read_to_string(sys.join("server@default.ini")).unwrap(),
            default_ini
        );
        assert_eq!(
            std::fs::read_to_string(sys.join("presets.ini")).unwrap(),
            presets
        );

        // A read that fails for another reason than a missing file.
        std::fs::create_dir(sys.join("presets.ini.disabled")).expect("dir in the way");
        match client.read_file("presets.ini.disabled") {
            Err(ClientError::Io(kind)) => assert!(!kind.is_empty(), "the kind"),
            other => panic!("read of a directory: {other:?}"),
        }
        std::fs::remove_dir(sys.join("presets.ini.disabled")).unwrap();

        // A file of non-ASCII text with every JSON escape round-trips whole.
        let tricky = "[m]\nmodel = /models/é \"q\"\\\t\u{1}.gguf\n";
        std::fs::write(sys.join("presets.ini.disabled"), tricky).expect("tricky file");
        assert_eq!(
            client.read_file("presets.ini.disabled"),
            Ok(Some(tricky.to_string()))
        );
        std::fs::remove_file(sys.join("presets.ini.disabled")).unwrap();
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
}
