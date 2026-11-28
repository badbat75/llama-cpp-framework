//! IP3c: convergence, the idempotent apply, the drift of a running instance and
//! its last good render (`ARCHITECTURE.md` "Convergence: idempotent apply, drift
//! and the last good render", slice IP3 part c).
//!
//! The checks split in two: the intrinsic ones judge the profile alone and
//! refuse the whole save, the fit ones judge one instance on this node and
//! decide, per instance, whether its rendered pair is written. The pair is
//! `models@<id>.ini` beside the new `launch@<id>.ini`: a pair that stands is the
//! last good state, so a fit failure leaves it byte for byte and reports the
//! instance not converged with that check's reason. Drift is the pair on disk
//! against the pair the running instance launched with (its `applied` record), so
//! a change reverted before a restart shows none.
//!
//! Every phase runs through `isolated` (the `#[test]`s of `mod.rs`), each in a
//! child of its own over a data root of its own, and writes only that root's
//! engine config and state folders. The three fields a row answers and a save's answer are
//! read off the stream face (`manager::pipe::serve_stream` on a Unix socket
//! pair), the files and the per-instance render off the core.

#[cfg(target_os = "linux")]
use crate::manager::dbus::{ManagerInterface, BUS_NAME, INTERFACE, OBJECT_PATH};
use crate::manager::pipe::serve_stream;
use crate::manager::service::fakes::{
    Call, FakeAuthority, FakeGpus, FakeGroup, FakeResolver, FakeService,
};
use crate::manager::service::{
    write_gguf_kv, Affected, Caller, Manager, ManagerError, UnitState, INSTANCES_FILE,
};
use pai_core::client::pipe::{read_frame, write_frame};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
#[cfg(target_os = "linux")]
use zbus::blocking::connection::Builder;

type Core = Manager<FakeService, FakeAuthority, FakeResolver>;

const PROFILES: &str = "instance-profiles.ini";
const POOL: &str = "models.ini";

/// The longest a wire client waits for a reply.
const DEADLINE: Duration = Duration::from_secs(10);

/// A `llama` GGUF whose KV layout `pai_core::gguf_shape` reads: the shape the
/// memory check measures is the shape the fixture wrote.
fn write_model(path: &Path, ctx_len: u32, tensors: &[(&str, u64)]) {
    write_gguf_kv(
        path,
        "llama",
        &[
            ("llama.context_length", ctx_len),
            ("llama.block_count", 2),
            ("llama.embedding_length", 2048),
            ("llama.attention.head_count", 32),
            ("llama.attention.head_count_kv", 4),
            ("llama.attention.key_length", 128),
            ("llama.attention.value_length", 128),
        ],
        tensors,
    );
}

/// The install of the IP3c phases: two instances, each running an instance
/// profile of its own, and each profile naming a server profile of its own, so
/// the two renders are judged on different cards. `desk` runs `desk-gpu`, whose
/// `ServerProfile` is `desk`, whose `Devices` is `CUDA0`; `lab` runs `lab-gpu`,
/// whose `ServerProfile` is `lab`, whose `Devices` is `CUDA1`. The record names
/// the instance profile and nothing else of the placement: the model data model
/// of `ARCHITECTURE.md` "Instance profiles" puts the server profile one step
/// behind it, so a record section that also carries `ServerProfile` is a stale
/// key the resolution never reads, and the fixture leaves it out. Beside them
/// `free` is a server profile no instance names, on `CUDA0` and a port of its
/// own, so a switch has a profile to take (the standing rule is one instance per
/// server profile) and the render it leaves fits. The pool has `alpha` and `beta`
/// on the same model file, and the entry `[<profile>/alpha]` of BOTH profiles
/// carries `entry_keys`, so an entry key is judged on the two card sets at once.
/// Answers the model file's path, under `root/models`.
fn seed(root: &Path, sys: &Path, entry_keys: &str) -> String {
    std::fs::create_dir_all(sys).expect("engine config dir");
    let models = root.join("models");
    std::fs::create_dir_all(&models).expect("models dir");
    let model = models.join("alpha.gguf");
    write_model(
        &model,
        262_144,
        &[
            ("token_embd.weight", 200 << 20),
            ("blk.0.attn_q.weight", 300 << 20),
            ("blk.1.attn_q.weight", 300 << 20),
            ("output.weight", 200 << 20),
        ],
    );
    let path = model.to_string_lossy().into_owned();
    let write = |name: &str, text: &str| {
        std::fs::write(sys.join(name), text).expect(name);
    };
    write(
        POOL,
        &format!(
            "[*]\ntop-p = 0.9\n\n[alpha]\nmodel = {path}\ntemp = 0.7\n\n[beta]\nmodel = {path}\ntemp = 0.3\n"
        ),
    );
    write(
        INSTANCES_FILE,
        "[desk]\nInstanceProfile = desk-gpu\nPort = 8581\n\n\
         [lab]\nInstanceProfile = lab-gpu\nPort = 8681\n",
    );
    write(
        "server@desk.ini",
        "[Server]\nHostname = 127.0.0.1\nPort = 8581\nDevices = CUDA0\n",
    );
    write(
        "server@lab.ini",
        "[Server]\nHostname = 127.0.0.1\nPort = 8681\nDevices = CUDA1\n",
    );
    write(
        "server@free.ini",
        "[Server]\nHostname = 127.0.0.1\nPort = 8781\nDevices = CUDA0\n",
    );
    write(
        PROFILES,
        &format!(
            "[desk-gpu]\nServerProfile = desk\n\n[desk-gpu/alpha]\n{entry_keys}\n\n\
             [lab-gpu]\nServerProfile = lab\n\n[lab-gpu/alpha]\n{entry_keys}"
        ),
    );
    path
}

/// The manager over the redirected tree: `running` are the units the service
/// manager answers active; `cuda0` and `cuda1` are the MiB of the node's two
/// cards, the two cards the two instances' server profiles each name one of.
fn manager(running: &[&str], cuda0: u64, cuda1: u64) -> Arc<Core> {
    let service = FakeService {
        running: running.iter().map(|s| (*s).to_string()).collect(),
        state: UnitState::Inactive,
        ..FakeService::new()
    };
    Arc::new(
        Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
            .with_group(FakeGroup::new())
            .with_gpus(FakeGpus::new(&[("CUDA0", cuda0), ("CUDA1", cuda1)])),
    )
}

/// A `Vec<(String, String)>` of key pairs, the shape every write verb takes.
fn vals(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}
/// The entry save of BOTH fixture profiles: the record names an instance profile
/// and the pool names the server profile behind it, so the entry of one profile
/// moves the render of the instances running that profile alone, and an entry key
/// meant for both renders is asked of `desk-gpu` (which `desk` runs) and of
/// `lab-gpu` (which `lab` runs). Each save carries the revision of the pool as
/// the read before it, so the page's own rule holds for each. The answer is the
/// last save's: the pass that follows the second judged every render, the pair
/// each instance stands on. A refusal stops at the profile it stopped, leaving
/// the second save unasked, so an intrinsic check of the entry writes nothing.
fn save_profiles(
    m: &Core,
    who: &Caller,
    model: &str,
    pairs: &[(String, String)],
) -> Result<Affected, ManagerError> {
    let mut last: Option<Result<Affected, ManagerError>> = None;
    for profile in ["desk-gpu", "lab-gpu"] {
        let revision = m
            .instance_profile(who, profile)
            .expect("IP3c: the fixture's record names an instance profile")
            .revision;
        last = Some(m.set_instance_profile_model(who, profile, model, pairs, &revision));
        if last.as_ref().is_some_and(|a| a.is_err()) {
            break;
        }
    }
    last.expect("IP3c: the fixture has two instance profiles")
}

/// A refusal of `key` carrying `facts` as its values, in any order: the order of
/// a keyed refusal's arguments is the strings file's.
fn refused_key<T: std::fmt::Debug>(
    answer: &Result<T, ManagerError>,
    key: &str,
    facts: &[&str],
) -> bool {
    match answer {
        Err(ManagerError::Invalid(r)) => {
            let mut got: Vec<&str> = r.values.iter().map(String::as_str).collect();
            let mut want: Vec<&str> = facts.to_vec();
            got.sort();
            want.sort();
            r.key == key && got == want
        }
        _ => false,
    }
}

/// The bytes of one file of the engine's config folder, None when it is not
/// there.
fn bytes(sys: &Path, name: &str) -> Option<Vec<u8>> {
    std::fs::read(sys.join(name)).ok()
}

/// The text of the section `[alpha]` of instance `id`'s render, empty when the
/// render or the section is not there.
fn render_alpha(sys: &Path, id: &str) -> String {
    let text = std::fs::read_to_string(sys.join(format!("models@{id}.ini"))).unwrap_or_default();
    pai_core::ini::text::section_text(&text, "alpha").unwrap_or_default()
}

/// The usable memory, in MiB, of one card of `total_mib`: `Gpu::usable` reads the
/// total alone, so the id and the name of this fixture card are the pair's.
fn usable(card: u64) -> u64 {
    pai_core::devices::pool_usable(&[pai_core::devices::Gpu {
        id: "CUDA1".to_string(),
        name: "NVIDIA GeForce CUDA1".to_string(),
        total_mib: card,
    }])
}

/// The instances `m` reports not converged, in its order: the id and the key of
/// the check that stopped its render.
fn not_converged(m: &Core) -> Vec<(String, String)> {
    m.render_failures()
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
        .collect()
}

/// The keyed refusal `m` reports for instance `id`, None when it reports none.
fn refusal_of(m: &Core, id: &str) -> Option<pai_core::checks::Refusal> {
    m.render_failures()
        .into_iter()
        .find(|(row, _)| row == id)
        .and_then(|(_, err)| match err {
            ManagerError::Invalid(r) => Some(r),
            _ => None,
        })
}

/// `request` answered by `core` as `caller`, over a Unix socket pair: the
/// stream face of the named pipe, one connection per call.
fn call(core: &Arc<Core>, caller: &Caller, request: Value) -> Value {
    let (served, mut client) = UnixStream::pair().expect("socket pair");
    client
        .set_read_timeout(Some(DEADLINE))
        .expect("read deadline");
    let core = core.clone();
    let caller = caller.clone();
    let server = std::thread::spawn(move || serve_stream(&core, caller, served));
    write_frame(&mut client, request.to_string().as_bytes()).expect("write the request");
    let body = read_frame(&mut client)
        .unwrap_or_else(|e| {
            panic!(
                "read the reply of {request}: {e:?} (the wire's {DEADLINE:?} bound, the \
                 machine is loaded)"
            )
        })
        .expect("a reply");
    let reply: Value = serde_json::from_slice(&body).expect("a JSON reply");
    drop(client);
    let ended = server.join().expect("the server thread");
    assert!(
        ended.is_ok(),
        "the connection ends clean: {ended:?} of {request}"
    );
    reply
}

/// The `ListInstances` rows, as the stream answers them.
fn rows(core: &Arc<Core>, caller: &Caller) -> Vec<Value> {
    call(core, caller, json!({"method": "ListInstances"}))
        .get("ok")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The row of instance `id`: `Value::Null` when the list carries none.
fn row(rows: &[Value], id: &str) -> Value {
    rows.iter()
        .find(|r| r.get("id").and_then(Value::as_str) == Some(id))
        .cloned()
        .unwrap_or(Value::Null)
}

/// The interface of `core` served on one end of a Unix socket pair (peer to
/// peer, never a bus), and `core`'s own bus connection on the other, with
/// `DEADLINE` on every call: the face `InstanceStates` is answered on. The
/// server connection is returned to keep it alive, as in
/// `src/tests/dbus_client.rs`.
#[cfg(target_os = "linux")]
fn bus(core: &Arc<Core>) -> (zbus::blocking::Connection, zbus::blocking::Connection) {
    let (served, client_end) = UnixStream::pair().expect("socket pair");
    let iface = ManagerInterface::new(core.clone());
    let server = std::thread::spawn(move || {
        Builder::async_io_unix_stream(served)
            .server(zbus::Guid::generate())
            .expect("server guid")
            .p2p()
            .serve_at(OBJECT_PATH, iface)
            .expect("serve_at")
            .build()
            .expect("server connection")
    });
    let client = Builder::async_io_unix_stream(client_end)
        .p2p()
        .method_timeout(DEADLINE)
        .build()
        .expect("client connection");
    (server.join().expect("the server thread"), client)
}

/// The SHA-256 of the pair instance `id` is rendered with: the bytes of
/// `models@<id>.ini` followed by those of `launch@<id>.ini`, in lowercase hex.
fn pair_digest(sys: &Path, id: &str) -> String {
    let mut pair = bytes(sys, &format!("models@{id}.ini")).expect("the rendered model profiles");
    pair.extend(bytes(sys, &format!("launch@{id}.ini")).expect("the rendered launch file"));
    Sha256::digest(pair)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The record a launched instance leaves of the pair it started with: `applied`
/// in its own state folder, which its account writes and the manager reads.
fn mark_applied(id: &str, digest: &str) {
    let dir = crate::paths::instance_state_dir().join(id);
    std::fs::create_dir_all(&dir).expect("the instance's state folder");
    std::fs::write(dir.join("applied"), digest).expect("the applied record");
}

/// E2E phase: IP3c 1, 2 and 6. The fit checks split per instance: the entry save
/// that grows both renders lands where the render fits and is refused where it
/// does not, so the instance that fits is rewritten and the one that does not
/// keeps its last good pair byte for byte and is reported not converged with the
/// memory refusal. The next save that fits it converges it. The intrinsic check
/// of the entry itself is the opposite: it refuses the whole save and writes no
/// file at all.
pub(crate) fn ip3c_fit_split_e2e() {
    use pai_core::gguf_shape::{full_gpu_bytes, local_shape, GPU_RESERVE, MIB};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let shape = local_shape(Path::new(&path)).expect("IP3c 1: the fixture shape");
    let need = |ctx: u64| full_gpu_bytes(&shape, ctx, "f16", "f16");
    let mib_up = |bytes: u64| (bytes.div_ceil(MIB)).to_string();
    let mib_down = |bytes: u64| (bytes / MIB).to_string();
    // `CUDA0` finds the grown entry, `CUDA1` finds it at 1024 tokens and not at
    // 131072, so one save is a fit on `desk` and a refusal on `lab`.
    let big = need(131_072) / MIB + GPU_RESERVE / MIB + 64;
    let small = need(1024) / MIB + GPU_RESERVE / MIB + 32;
    assert!(
        need(131_072) <= usable(big) && need(131_072) > usable(small),
        "the two cards: one finds the grown render, the other refuses it"
    );

    let m = manager(&["desk"], big, small);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3c 1: the seeded install renders"
    );
    let alice = Caller(":1.42".into());
    let save = |pairs: &[(&str, &str)]| save_profiles(&m, &alice, "alpha", &vals(pairs));

    // ── 1a. The pair is rendered, for both instances ─────────────────────
    for id in ["desk", "lab"] {
        assert!(
            bytes(&sys, &format!("models@{id}.ini")).is_some(),
            "IP3c 1a: {id}'s rendered model profiles are there"
        );
        let launch = std::fs::read_to_string(sys.join(format!("launch@{id}.ini")))
            .expect("IP3c 1a: the rendered launch file");
        assert!(
            launch.contains(&format!("Port = 8{}", if id == "desk" { 581 } else { 681 })),
            "IP3c 1a: {id}'s launch file carries the address of its record section: \
             {launch}"
        );
        assert!(
            launch.contains(&format!(
                "Devices = CUDA{}",
                if id == "desk" { 0 } else { 1 }
            )),
            "IP3c 1a: {id}'s launch file carries the cards of its server profile: {launch}"
        );
    }
    let before = (
        bytes(&sys, "models@desk.ini"),
        bytes(&sys, "launch@desk.ini"),
        bytes(&sys, "models@lab.ini"),
        bytes(&sys, "launch@lab.ini"),
    );

    // ── 1b. The save that fits `desk` and not `lab` ──────────────────────
    let answer = save(&[("ctx-size", "131072")]);
    assert!(
        answer.is_ok(),
        "IP3c 1b: a save that fits one instance is no longer refused by the \
         instance it does not fit: {answer:?}"
    );
    assert!(
        render_alpha(&sys, "desk").contains("ctx-size = 131072"),
        "IP3c 1b: the render that fits carries the grown entry"
    );
    assert_ne!(
        bytes(&sys, "models@desk.ini"),
        before.0,
        "IP3c 1b: `desk`'s rendered model profiles are rewritten"
    );
    assert_eq!(
        bytes(&sys, "launch@desk.ini"),
        before.1,
        "IP3c 1b: the server profile did not change, so `desk`'s launch file is \
         left where it stands"
    );
    assert_eq!(
        (bytes(&sys, "models@lab.ini"), bytes(&sys, "launch@lab.ini")),
        (before.2, before.3),
        "IP3c 1b: the instance whose render does not fit keeps its pair byte for byte"
    );
    assert_eq!(
        not_converged(&m),
        vec![("lab".to_string(), "model-profile-over-memory".to_string())],
        "IP3c 1b: only the instance that kept its last good pair is reported not \
         converged"
    );
    let refusal = refusal_of(&m, "lab").expect("IP3c 1b: the refusal of `lab`");
    let need_mib = mib_up(need(131_072));
    let avail_mib = mib_down(usable(small));
    let mut got: Vec<&str> = refusal.values.iter().map(String::as_str).collect();
    let mut want: Vec<&str> = vec!["alpha", "lab", &need_mib, &avail_mib];
    got.sort();
    want.sort();
    assert_eq!(
        got, want,
        "IP3c 1b: the reason is the memory refusal, with the model, the instance, \
         the need and the memory of the cards `lab` runs on"
    );

    // ── 1c. The apply is idempotent ─────────────────────────────────────
    let pair = || {
        (
            bytes(&sys, "models@desk.ini"),
            bytes(&sys, "launch@desk.ini"),
            bytes(&sys, "models@lab.ini"),
            bytes(&sys, "launch@lab.ini"),
        )
    };
    let applied = pair();
    assert_eq!(
        not_converged(&m),
        vec![("lab".to_string(), "model-profile-over-memory".to_string())],
        "IP3c 1c: rendering again, with nothing changed, reports the same instance"
    );
    assert_eq!(
        pair(),
        applied,
        "IP3c 1c: the same profiles render the same pair: a render of a tree that \
         did not move changes no byte"
    );

    // ── 2. The save that fits `lab` converges it ─────────────────────────
    let answer = save(&[("ctx-size", "2048")]);
    assert!(
        answer.is_ok(),
        "IP3c 2: the save that fits both renders lands: {answer:?}"
    );
    assert!(
        render_alpha(&sys, "lab").contains("ctx-size = 2048"),
        "IP3c 2: `lab`'s render is written at last"
    );
    assert!(
        render_alpha(&sys, "desk").contains("ctx-size = 2048"),
        "IP3c 2: the entry of both fixture profiles rides both renders"
    );
    assert!(
        bytes(&sys, "launch@lab.ini").is_some(),
        "IP3c 2: the converged instance's pair is complete"
    );
    assert!(
        not_converged(&m).is_empty(),
        "IP3c 2: every instance converged, so nothing is reported: {:?}",
        not_converged(&m)
    );

    // ── 6. The intrinsic check of the entry refuses the whole save ───────
    let tree = || {
        (
            bytes(&sys, POOL),
            bytes(&sys, PROFILES),
            bytes(&sys, INSTANCES_FILE),
            bytes(&sys, "models@desk.ini"),
            bytes(&sys, "launch@desk.ini"),
            bytes(&sys, "models@lab.ini"),
            bytes(&sys, "launch@lab.ini"),
        )
    };
    let whole = tree();
    let answer = save(&[("ctx-size", "2048"), ("n-cpu-moe", "5000")]);
    assert!(
        refused_key(
            &answer,
            "not-whole-number-in-range",
            &["n-cpu-moe", "5000", "0", "4096"]
        ),
        "IP3c 6: a value out of range of the entry itself is an intrinsic refusal \
         of the whole save: {answer:?}"
    );
    assert_eq!(
        tree(),
        whole,
        "IP3c 6: the intrinsic refusal wrote no profile, no render and no launch file"
    );
}

/// The pair instance `id` is rendered with, as bytes on disk.
#[cfg(target_os = "linux")]
fn pair_bytes(sys: &Path, id: &str) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    (
        bytes(sys, &format!("models@{id}.ini")),
        bytes(sys, &format!("launch@{id}.ini")),
    )
}

/// The text of a row's field, `""` when the row has no such field.
#[cfg(target_os = "linux")]
fn text(row: &Value, field: &str) -> String {
    row.get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The not-converged rows of an answer's `ok`, as `(instance, key)`.
#[cfg(target_os = "linux")]
fn row_pairs(ok: &Value, verb: &str) -> Vec<(String, String)> {
    ok["notConverged"]
        .as_array()
        .unwrap_or_else(|| panic!("IP3c 7 {verb}: the answer carries the not-converged rows: {ok}"))
        .iter()
        .map(|r| (text(r, "instance"), text(r, "key")))
        .collect()
}

/// The rows of [`row_pairs`] an answer carries, pinned against what the service
/// reports.
#[cfg(target_os = "linux")]
fn rows_answered(m: &Arc<Core>, ok: &Value, verb: &str) {
    assert_eq!(
        row_pairs(ok, verb),
        not_converged(m),
        "IP3c 7 {verb}: the answer carries every instance that kept its last good pair, \
         with the key that stopped it: {ok}"
    );
}

/// `request` answered by `m` over the stream, pinned as a save that re-renders:
/// the pair of `desk` moves, and the answer is the design's object, carrying the
/// running instances the save touched and every instance that kept its last good
/// pair with the key that stopped it (the same rows the service reports).
#[cfg(target_os = "linux")]
fn converging(m: &Arc<Core>, sys: &Path, verb: &str, running: &[&str], request: Value) {
    let alice = Caller(":1.42".into());
    let desk = pair_bytes(sys, "desk");
    let answer = call(m, &alice, request);
    let ok = answer.get("ok").unwrap_or_else(|| {
        panic!(
            "IP3c 7 {verb}: the save lands and answers the affected instances, and the \
                stream answers {answer}"
        )
    });
    assert_ne!(
        pair_bytes(sys, "desk"),
        desk,
        "IP3c 7 {verb}: the save re-rendered `desk`, the running instance the pin runs \
         on: {answer}"
    );
    assert_eq!(
        ok["running"],
        json!(running),
        "IP3c 7 {verb}: the answer carries the running instances the save touched, in id \
         order: {answer}"
    );
    rows_answered(m, ok, verb);
}

/// `request` answered by `m` over the stream, pinned as a save that changes no
/// render of `desk`: the answer is the same object, the pair of `desk` keeps its
/// bytes, and the rows are the ones the files already give. `running` is the
/// running instances that DRIFT at the moment of the answer, the save having
/// moved their pair or an earlier one (IP3c review: `running` is built from the
/// instances whose pair drifts from their `applied` record).
#[cfg(target_os = "linux")]
fn unmoved(m: &Arc<Core>, sys: &Path, verb: &str, running: &[&str], request: Value) {
    let alice = Caller(":1.42".into());
    let desk = pair_bytes(sys, "desk");
    let answer = call(m, &alice, request);
    let ok = answer.get("ok").unwrap_or_else(|| {
        panic!(
            "IP3c 7 {verb}: the save lands and answers the affected instances, and the \
                stream answers {answer}"
        )
    });
    assert_eq!(
        pair_bytes(sys, "desk"),
        desk,
        "IP3c 7 {verb}: the save moved no render of `desk`: {answer}"
    );
    assert_eq!(
        ok["running"],
        json!(running),
        "IP3c 7 {verb}: the answer carries the running instances the save touched, in id \
         order: {answer}"
    );
    rows_answered(m, ok, verb);
}

/// E2E phase: IP3c 7 and 8. The saves that re-render answer the design's
/// object, the same as the entry add and remove: the model profile save (the pool
/// write), the server profile save (the server profile write), the address, the
/// entry save and the instance profile switch. Of the two switches the web UI's
/// pages use, the server profile switch re-renders: the record names an instance
/// profile, so the switch moves the `ServerProfile` key of the profile that
/// instance runs and its pair follows onto the new profile's cards. The model
/// profile switch moves no render: the entry sections of the instance profile are
/// the model list, so an assignment writes no render. `desk` runs and its card
/// finds every render of the
/// phase; `lab` does not run and its card refuses the entry grown to
/// 131 072 tokens, so each save answers `desk` as the running instance it touched
/// and `lab` with the memory refusal, the rows the service reports. The D-Bus face
/// carries the same two verbs with the same answer in its reply body, and the
/// entry save is asked of the frame as well: the frame has no arm for it, so the
/// Windows bff, which reaches the manager over the frame alone, cannot save an
/// entry key. A write that touches no size keeps `lab`'s pair byte for byte (the
/// fit is judged against the pair on disk, the last good one) and answers it with
/// the same key.
#[cfg(target_os = "linux")]
pub(crate) fn ip3c_saves_answer_convergence_e2e() {
    use crate::manager::dbus::{ManagerInterface, INTERFACE, OBJECT_PATH};
    use pai_core::gguf_shape::{full_gpu_bytes, local_shape, GPU_RESERVE, MIB};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let shape = local_shape(Path::new(&path)).expect("IP3c 7: the fixture shape");
    let need = |ctx: u64| full_gpu_bytes(&shape, ctx, "f16", "f16");
    // `desk` finds the grown entry, `lab` refuses it, so every save of the phase
    // answers both running instances and `lab` keeping its last good pair.
    let big = need(131_072) / MIB + GPU_RESERVE / MIB + 64;
    let small = need(1024) / MIB + GPU_RESERVE / MIB + 32;
    // Only `desk` runs, as in the split phase: the save of the grown entry lands
    // on it, and `lab` is reported not converged, so the phase's every answer
    // carries one running instance and one not-converged one.
    let m = manager(&["desk"], big, small);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3c 7: the seeded install renders"
    );
    let alice = Caller(":1.42".into());
    let file = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);
    let pool = || file(POOL);
    let profiles = || file(PROFILES);
    let record = || file(INSTANCES_FILE);
    let revision = || {
        m.instance_profile(&alice, "desk-gpu")
            .expect("IP3c 7: the pool read")
            .revision
    };
    // The grown entry, on the profile of each instance: `desk`'s card finds it,
    // `lab`'s does not.
    save_profiles(&m, &alice, "alpha", &vals(&[("ctx-size", "131072")]))
        .expect("IP3c 7: the entry grown on the instance that finds it");
    assert_eq!(
        not_converged(&m),
        vec![("lab".to_string(), "model-profile-over-memory".to_string())],
        "IP3c 7: `lab` keeps its last good pair from the start of the phase"
    );
    mark_applied("desk", &pair_digest(&sys, "desk"));

    // ── 7a. The model profile save: the pool write, re-rendering every render
    // whose instance profile has an entry for it ────────────────────────────
    let warm = pool().replace("temp = 0.7", "temp = 0.9");
    assert_ne!(warm, pool(), "IP3c 7: the pool write is a real change");
    converging(
        &m,
        &sys,
        "the model profile save",
        &["desk"],
        json!({"method": "WriteFileIf", "name": POOL, "contents": warm, "expected": pool(),
               "keep_api_key": false}),
    );

    // ── 7b. The server profile save: the cards of `desk` gain a ROCm card, so
    // every render that names the profile re-renders ────────────────────────
    let server_profile = file("server@desk.ini");
    let wider = server_profile.replace("Devices = CUDA0", "Devices = CUDA0,ROCm0");
    assert_ne!(
        wider, server_profile,
        "IP3c 7: the server profile write is a real change"
    );
    converging(
        &m,
        &sys,
        "the server profile save",
        &["desk"],
        json!({"method": "WriteFileIf", "name": "server@desk.ini", "contents": wider,
               "expected": server_profile, "keep_api_key": false}),
    );

    // ── 7c. The address of a running instance: its launch moves ────────────
    converging(
        &m,
        &sys,
        "the address",
        &["desk"],
        json!({"method": "SetInstanceAddress", "instance": "desk", "host": "127.0.0.1",
               "port": "8595", "revision": record()}),
    );

    // ── 7d. The two verbs the web UI's pages use for the same two saves. The
    // server profile switch lands and answers the object, and it re-renders: the
    // record names an instance profile and the pool names the server profile one
    // step behind it, so the switch of `desk` moves the `ServerProfile` key of
    // `desk-gpu`, the profile `desk` runs, and `desk`'s pair follows it onto
    // `free`'s card and port. `free` is the server profile no instance names
    // (`lab` names `lab`, and one instance per server profile refuses that), and
    // it sits on `CUDA0`, so the render the switch leaves fits. The model profile
    // switch writes `assignments.ini`, and the entry sections of the instance
    // profile the record names ARE the model list, so the assignment moves no
    // render. Both answer `desk` all the same: 7a to 7c moved its pair away from
    // the `applied` record marked before the phase, so it drifts, and a drifting
    // running instance is what `running` carries. ────────────────────────────
    converging(
        &m,
        &sys,
        "the server profile switch",
        &["desk"],
        json!({"method": "SetServerProfile", "instance": "desk", "profile": "free"}),
    );
    assert!(
        pai_core::ini::text::section_text(&profiles(), "desk-gpu")
            .unwrap_or_default()
            .contains("ServerProfile = free"),
        "IP3c 7d: the switch moved the ServerProfile key of the instance profile `desk` \
         runs: {}",
        profiles()
    );
    unmoved(
        &m,
        &sys,
        "the model profile switch",
        &["desk"],
        json!({"method": "SetModelProfile", "instance": "desk", "profile": "alpha", "value": "on"}),
    );

    // ── 7e. The entry save over the frame: the verb the bus has and the frame
    // has no arm for, so the Windows bff, which reaches the manager over the
    // frame alone, cannot save an entry key. It writes the entry, and answers
    // the same object the bus answers: the save's answer, not `null`. ────────
    converging(
        &m,
        &sys,
        "the entry save",
        &["desk"],
        json!({"method": "SetInstanceProfileModel", "profile": "desk-gpu", "model": "alpha",
               "values": [{"key": "ctx-size", "value": "32768"}], "revision": profiles()}),
    );
    assert!(
        pai_core::ini::text::section_text(&profiles(), "desk-gpu/alpha")
            .unwrap_or_default()
            .contains("ctx-size = 32768"),
        "IP3c 7e: the frame's entry save wrote the entry of [desk-gpu/alpha]: {}",
        profiles()
    );
    assert!(
        render_alpha(&sys, "desk").contains("ctx-size = 32768"),
        "IP3c 7e: the entry the frame saved rides the render of every instance running \
         the profile: {}",
        render_alpha(&sys, "desk")
    );

    // ── 8. The same answers over D-Bus: the reply body carries the object, the
    // entry save included (the stream has no arm for it). ───────────────────
    let (served, client) = UnixStream::pair().expect("socket pair");
    let core = m.clone();
    let served = std::thread::spawn(move || {
        zbus::blocking::connection::Builder::async_io_unix_stream(served)
            .server(zbus::Guid::generate())
            .expect("the server's guid")
            .p2p()
            .serve_at(OBJECT_PATH, ManagerInterface::new(core))
            .expect("serve the interface")
            .build()
            .expect("the server connection")
    });
    let bus = zbus::blocking::connection::Builder::async_io_unix_stream(client)
        .p2p()
        .method_timeout(DEADLINE)
        .build()
        .expect("the client connection");
    bus.inner()
        .set_unique_name(":1.42")
        .expect("the caller's name on the bus");
    // The server's connection lives to the end of the phase: dropping the
    // connection the thread returns closes the socket, and every call of the
    // phase would then fail with a broken pipe.
    let _server = served.join().expect("the server connection");
    /// `method` asked of the bus with `body`, its reply body read as what it is:
    /// the design's answer object `a{sv}` (`affected_answer` in
    /// `manager/dbus.rs`), carrying `running`, the `as` of ids the save moved to
    /// a restart, and `notConverged`, the `av` of `a{sv}` rows naming the
    /// `instance` and the `key`. The bus has no JSON, so the fields are decoded
    /// and compared as the texts they are.
    fn answer_on_bus<B>(
        bus: &zbus::blocking::Connection,
        method: &str,
        body: &B,
    ) -> (Vec<String>, Vec<(String, String)>)
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        let reply = bus
            .call_method(None::<&str>, OBJECT_PATH, Some(INTERFACE), method, body)
            .unwrap_or_else(|e| {
                panic!(
                    "IP3c 8: {method} over the bus answers no reply (the bus carries no \
                        such verb?): {e}"
                )
            });
        let answer: std::collections::BTreeMap<String, zbus::zvariant::OwnedValue> = reply
            .body()
            .deserialize()
            .expect("IP3c 8: the reply body of a re-rendering save is the answer object, `a{sv}`");
        let field = |key: &str| {
            zbus::zvariant::Value::from(
                answer
                    .get(key)
                    .unwrap_or_else(|| {
                        panic!("IP3c 8: {method} over the bus answers `{key}`: {answer:?}")
                    })
                    .try_clone()
                    .expect("IP3c 8: the answer's value clones"),
            )
        };
        let running = field("running")
            .downcast::<Vec<String>>()
            .expect("IP3c 8: the answer's `running` is the array of ids (`as`)");
        let not_converged = field("notConverged")
            .downcast::<zbus::zvariant::Array>()
            .expect("IP3c 8: the answer's `notConverged` is an array of rows (`av`)");
        let not_converged = not_converged
            .inner()
            .iter()
            .map(|row| {
                let fields = row
                    .try_clone()
                    .expect("IP3c 8: the row's value clones")
                    .downcast::<zbus::zvariant::Dict>()
                    .expect("IP3c 8: each not-converged row names its fields (`a{sv}`)");
                let text = |key: &str| {
                    let needle = zbus::zvariant::Value::new(key);
                    fields
                        .iter()
                        .find(|(k, _)| *k == &needle)
                        .and_then(|(_, v)| v.try_clone().ok())
                        // The field is a variant of `a{sv}`: unwrapped, it is the text.
                        .and_then(|v| v.downcast::<String>().ok())
                        .unwrap_or_default()
                };
                (text("instance"), text("key"))
            })
            .collect();
        (running, not_converged)
    }
    let pin_bus = |method: &str, answer: (Vec<String>, Vec<(String, String)>)| {
        let (running, not_converged) = answer;
        assert_eq!(
            running,
            vec!["desk".to_string()],
            "IP3c 8: {method} over the bus answers the running instances the save touched: \
             {running:?}"
        );
        assert_eq!(
            not_converged,
            vec![("lab".to_string(), "model-profile-over-memory".to_string())],
            "IP3c 8: {method} over the bus answers the instance that kept its last good \
             pair, with the key of the check that stopped it: {not_converged:?}"
        );
    };
    pin_bus(
        "SetInstanceProfileModel",
        answer_on_bus(
            &bus,
            "SetInstanceProfileModel",
            &(
                "desk-gpu".to_string(),
                "alpha".to_string(),
                vec![("ctx-size".to_string(), "65536".to_string())],
                revision(),
            ),
        ),
    );
    // The entry grown again, so `lab`'s render does not fit when the pool write
    // below runs: the pair it is judged against is the one on disk, the last good
    // one, which the entry save above just wrote.
    save_profiles(&m, &alice, "alpha", &vals(&[("ctx-size", "131072")]))
        .expect("IP3c 8b: the entry grown again on the card that refuses it");
    mark_applied("desk", &pair_digest(&sys, "desk"));
    let lab_pair = pair_bytes(&sys, "lab");
    assert_eq!(
        not_converged(&m),
        vec![("lab".to_string(), "model-profile-over-memory".to_string())],
        "IP3c 8b: the entry grown again leaves `lab` not converged"
    );
    let pool_answer = answer_on_bus(
        &bus,
        "WriteFileIf",
        &(
            POOL.to_string(),
            pool().replace("temp = 0.9", "temp = 0.8"),
            pool(),
        ),
    );
    pin_bus("WriteFileIf", pool_answer);
    // ── 8b. The pool write above touches nothing of the fit, and `lab` is still
    // the instance whose render does not fit: the pass judges the fit against the
    // pair on disk, the last good one, so `lab`'s pair keeps every byte it had
    // and the answer above carries it with the same key
    // (`ARCHITECTURE.md` "Convergence": a fit failure writes neither file). A
    // pass that judged the fit against the render of the sources as the write
    // found them grew nothing over that render, wrote `lab` the pair that never
    // fit, and answered nothing not converged. ──────────────────────────────
    assert_eq!(
        pair_bytes(&sys, "lab"),
        lab_pair,
        "IP3c 8b: the pool write that touches no size left `lab`'s pair byte for byte the pair \
         on disk, the last good one"
    );

    // ── 7f. The instance profile switch: a second profile, whose entry is a
    // different size, so the render of the instance the switch moves changes.
    // It comes last: it leaves `desk` running `fast`, and every pin above
    // measures `desk` on the render of `desk-gpu`. The revision is the text of
    // `instance-profiles.ini` the caller read, not the model pool's. The profile
    // is seeded through the verbs that own it: the manager writes
    // `instance-profiles.ini` itself, so the served-file write has no arm for it
    // and a hand-written second profile never arrives. ───────────────────────
    m.create_instance_profile(&alice, "fast", "desk")
        .expect("IP3c 7f: the second instance profile, running `desk`");
    m.add_instance_profile_model(&alice, "fast", "alpha")
        .expect("IP3c 7f: `alpha` added to the second instance profile");
    m.set_instance_profile_model(
        &alice,
        "fast",
        "alpha",
        &vals(&[("ctx-size", "65536")]),
        &profiles(),
    )
    .expect("IP3c 7f: the second profile's entry at 65 536 tokens");
    assert!(
        pai_core::ini::text::section_text(&profiles(), "fast/alpha")
            .unwrap_or_default()
            .contains("ctx-size = 65536"),
        "IP3c 7f: the verbs seeded [fast/alpha] with its own size: {}",
        profiles()
    );
    converging(
        &m,
        &sys,
        "the instance profile switch",
        &["desk"],
        json!({"method": "SetInstanceProfile", "instance": "desk", "profile": "fast",
               "revision": profiles()}),
    );
}

/// E2E phase: IP3c 3, 4 and 5, over the stream. The save's answer carries the
/// affected instances that are up and now drift, and the ones that kept their
/// last good pair. A row answers `converged`, its `reason` and `pendingRestart`,
/// derived from the pair on disk against the `applied` record of the launched
/// instance: drift shows when the pair moves, and shows nothing once the change
/// is reverted. A manager restart reports the same rows.
pub(crate) fn ip3c_drift_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    // Both cards find every render of this phase: drift is the subject, not fit.
    let roomy = 400_000;
    let m = manager(&["desk"], roomy, roomy);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3c 3: the seeded install renders"
    );
    let alice = Caller(":1.42".into());
    // The entry verbs of `desk-gpu`, the profile `desk` runs: the render that
    // moves is the one the phase's pins read, `desk` being the running instance.
    let verb = |method: &str, model: &str| {
        call(
            &m,
            &alice,
            json!({"method": method, "profile": "desk-gpu", "model": model}),
        )
    };

    // ── 3a. The instance launched with the pair it has shows no drift ────
    mark_applied("desk", &pair_digest(&sys, "desk"));
    // `lab` is stopped, and its launch recorded the pair it has (IP3c review 4):
    // the drift is the compare of the pair on disk against that record, answered
    // of the files with no unit asked, so a pair standing on its record shows no
    // drift whatever its unit does.
    mark_applied("lab", &pair_digest(&sys, "lab"));
    let list = rows(&m, &alice);
    assert_eq!(
        row(&list, "desk")["converged"],
        json!(true),
        "IP3c 3a: the pair landed, so `desk` converged"
    );
    assert!(
        row(&list, "desk")["reason"].is_null(),
        "IP3c 3a: a converged instance answers no reason"
    );
    assert_eq!(
        row(&list, "desk")["pendingRestart"],
        json!(false),
        "IP3c 3a: the running instance whose pair is the one it launched with shows \
         no drift"
    );
    assert_eq!(
        row(&list, "lab")["pendingRestart"],
        json!(false),
        "IP3c 3a: the stopped instance whose pair is the one its launch recorded shows \
         no drift"
    );

    // ── 3b. A save that re-renders it: the answer and the drift ─────────
    let answer = verb("AddInstanceProfileModel", "beta");
    assert_eq!(
        answer.get("ok"),
        Some(&json!({"running": ["desk"], "notConverged": []})),
        "IP3c 3b: a re-rendering save answers the running instances that now drift, \
         and the ones that kept their last good pair: {answer}"
    );
    let list = rows(&m, &alice);
    assert_eq!(
        row(&list, "desk")["pendingRestart"],
        json!(true),
        "IP3c 3b: the pair on disk is no longer the pair the instance launched with"
    );
    assert_eq!(
        row(&list, "desk")["converged"],
        json!(true),
        "IP3c 3b: a drifted instance is not a not-converged one"
    );

    // ── 3c. The change reverted before a restart shows no drift ─────────
    let answer = verb("RemoveInstanceProfileModel", "beta");
    assert_eq!(
        answer.get("ok"),
        Some(&json!({"running": [], "notConverged": []})),
        "IP3c 3c: a save that puts the pair back moves nobody to a restart: {answer}"
    );
    let list = rows(&m, &alice);
    assert_eq!(
        row(&list, "desk")["pendingRestart"],
        json!(false),
        "IP3c 3c: the pair on disk is the pair it launched with again"
    );
    let settled = list;

    // ── 4. A manager restart reports the same states ────────────────────
    // The same tree, a manager that never saw a write: its start renders every
    // instance and derives the rows from the files, so the rows are the same.
    let again = manager(&["desk"], roomy, roomy);
    again.start_up();
    assert_eq!(
        rows(&again, &alice),
        settled,
        "IP3c 4: not converged and drift are derived, not kept: a manager that \
         starts again over the same tree reports the same rows"
    );
}

/// E2E phase: IP3c 4, the row of an instance that kept its last good pair, read
/// again after a manager restart. `desk` fits the grown entry and drifts (it runs,
/// and its pair is not the one it launched with); `lab` does not fit, keeps its
/// pair and is reported not converged with the memory refusal as its reason.
pub(crate) fn ip3c_not_converged_survives_restart_e2e() {
    use pai_core::gguf_shape::{full_gpu_bytes, local_shape, GPU_RESERVE, MIB};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let shape = local_shape(Path::new(&path)).expect("IP3c 4: the fixture shape");
    let need = |ctx: u64| full_gpu_bytes(&shape, ctx, "f16", "f16");
    let big = need(131_072) / MIB + GPU_RESERVE / MIB + 64;
    let small = need(1024) / MIB + GPU_RESERVE / MIB + 32;

    let m = manager(&["desk"], big, small);
    m.start_up();
    let alice = Caller(":1.42".into());
    assert!(
        save_profiles(&m, &alice, "alpha", &vals(&[("ctx-size", "131072")])).is_ok(),
        "IP3c 4: the save that fits `desk` lands"
    );
    // `lab` kept its last good pair, and its launch recorded that pair (IP3c
    // review 4): the row's drift is the compare of the pair on disk against the
    // record, so the instance that stands on its record shows no drift.
    mark_applied("lab", &pair_digest(&sys, "lab"));
    let list = rows(&m, &alice);
    assert_eq!(
        row(&list, "lab")["converged"],
        json!(false),
        "IP3c 4: the instance that kept its last good pair is reported not converged"
    );
    assert_eq!(
        row(&list, "lab")["reason"]["key"],
        json!("model-profile-over-memory"),
        "IP3c 4: its reason is the check that stopped its render, keyed"
    );
    let args = row(&list, "lab")["reason"]["args"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let named: Vec<&str> = args.iter().filter_map(Value::as_str).collect();
    assert!(
        named.contains(&"lab") && named.contains(&"alpha"),
        "IP3c 4: the reason names the instance and the model profile: {named:?}"
    );
    assert_eq!(
        row(&list, "lab")["pendingRestart"],
        json!(false),
        "IP3c 4: the pair `lab` stands on is the pair its launch recorded, so the \
         instance that is not running shows no drift"
    );
    assert_eq!(
        row(&list, "desk")["converged"],
        json!(true),
        "IP3c 4: the instance that took the new render converged"
    );
    assert_eq!(
        row(&list, "desk")["pendingRestart"],
        json!(true),
        "IP3c 4: `desk` runs a pair that is not on disk any more"
    );
    let list_before = list;

    // The same tree, a manager that never saw the write: its start renders every
    // instance, and the rows it derives are the rows the write derived.
    let again = manager(&["desk"], big, small);
    again.start_up();
    assert_eq!(
        rows(&again, &alice),
        list_before,
        "IP3c 4: and they are the same after its start rendered every instance"
    );
}

/// The keyed check names `m` reports for its instances, `(id, key)`, sorted: the
/// same rows `not_converged` reads, for the files a phase pins.
fn failure_keys(m: &Core) -> Vec<(String, String)> {
    not_converged(m).into_iter().collect::<Vec<_>>()
}

/// E2E phase: IP3c review 1. The pair is `models@<id>.ini` beside
/// `launch@<id>.ini`, so the removal of an instance takes the whole pair: its
/// launch file and the `applied` record of its state folder go with the model
/// profiles file, and the pair of every other instance is left byte for byte.
/// A recreation of the same id on another server profile is judged with no file
/// of the old install standing, and starts with no `applied` record.
pub(crate) fn ip3c_removal_takes_the_pair_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let roomy = 400_000;
    let m = manager(&["desk"], roomy, roomy);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3c review 1: the seeded install renders"
    );
    let alice = Caller(":1.42".into());
    assert!(
        bytes(&sys, "launch@lab.ini").is_some(),
        "IP3c review 1: the seeded install rendered the launch pair file"
    );
    mark_applied("lab", &pair_digest(&sys, "lab"));
    mark_applied("desk", &pair_digest(&sys, "desk"));
    let desk_pair = bytes(&sys, "models@desk.ini").expect("desk's pair");
    let desk_launch = bytes(&sys, "launch@desk.ini").expect("desk's launch");

    // ── 1a. A running instance is left alone (the standing rule), and the ──
    // ── stopped one loses the whole pair: models, launch, and the record ───
    let answer = m.remove_instance(&alice, "desk");
    assert!(
        matches!(
            answer,
            Err(ManagerError::Invalid(ref r)) if r.key == "instance-running"
        ),
        "IP3c review 1: a running instance is not removable: {answer:?}"
    );
    m.remove_instance(&alice, "lab")
        .expect("IP3c review 1: the removal of a stopped instance lands");
    assert!(
        !pai_core::ini::parse_sections(&std::fs::read_to_string(sys.join(INSTANCES_FILE)).unwrap())
            .iter()
            .any(|s| s.id == "lab"),
        "IP3c review 1: the record names the instance no more"
    );
    assert!(
        bytes(&sys, "models@lab.ini").is_none(),
        "IP3c review 1: the removal deleted the rendered model profiles"
    );
    assert!(
        bytes(&sys, "launch@lab.ini").is_none(),
        "IP3c review 1: the pair is models beside launch, so the removal deletes \
         launch@lab.ini too"
    );
    assert!(
        !crate::paths::instance_state_dir()
            .join("lab")
            .join("applied")
            .exists(),
        "IP3c review 1: the removal deleted the applied record of the state folder"
    );
    assert!(
        bytes(&sys, "models@desk.ini").as_deref() == Some(desk_pair.as_slice())
            && bytes(&sys, "launch@desk.ini").as_deref() == Some(desk_launch.as_slice()),
        "IP3c review 1: the removal of another instance leaves this one's pair byte for byte \
         (models {} B, launch {} B)",
        desk_pair.len(),
        desk_launch.len()
    );

    // ── 1b. The same id, on the server profile the first install left free ─
    // The recreation is judged with the node holding `desk` alone: the address
    // the removed instance bound (127.0.0.1:8681) binds nothing now, so the
    // create's address check passes. The record's port is the first port of the
    // node's range no instance holds, which is the standing rule of `create`
    // (desk holds 8581, so the create proposes 8582), and the launch the
    // recreation renders carries the record's port. With no `applied` record of
    // the old install left, the recreation starts clean.
    // The pool gains the instance profile the recreation runs (IP4b: the
    // create's third name is an instance profile, and its section names the
    // server profile `free`, the one binding an address nothing holds now).
    std::fs::write(
        sys.join(PROFILES),
        format!(
            "{}\n[free]\nServerProfile = free\n",
            std::fs::read_to_string(sys.join(PROFILES)).expect(PROFILES)
        ),
    )
    .expect(PROFILES);
    m.create_instance(&alice, "llama.cpp", "lab", "free")
        .expect("IP3c review 1: the recreation of the id on `free` lands");
    let record = std::fs::read_to_string(sys.join(INSTANCES_FILE)).unwrap();
    assert!(
        record.contains("[lab]\nInstanceProfile = free\nPort = 8582"),
        "IP3c review 1: the record names the recreated `lab` on the instance profile it \
         was created with, on the first port of the range no instance holds: {record}"
    );
    let launch = std::fs::read_to_string(sys.join("launch@lab.ini"))
        .expect("IP3c review 1: the recreation rendered a launch pair file");
    assert!(
        launch.contains("8582"),
        "IP3c review 1: the new launch carries the record's port: {launch}"
    );
    assert!(
        bytes(&sys, "models@lab.ini").is_some(),
        "IP3c review 1: the recreation rendered the model profiles pair file too"
    );
    assert!(
        !crate::paths::instance_state_dir()
            .join("lab")
            .join("applied")
            .exists(),
        "IP3c review 1: the recreation starts with no applied record of the old install"
    );
}

/// E2E phase: IP3c review 3. The same entry stands in the two instance profiles
/// the two instances run, `desk-gpu` (whose `ServerProfile` is `desk`, on
/// `CUDA0`) and `lab-gpu` (whose is `lab`, on `CUDA1`), so ONE entry key is
/// judged on two card sets at once, one save per profile. An entry key of a card
/// one instance names: both saves land, the save of the profile the fitting
/// instance runs rewrites it and answers it in `running`, and the save of the
/// profile the other runs leaves it on its last good pair byte for byte and
/// reports it with the device check's key. The other way round the pair that
/// stands is the pair the first round just landed: the fit is judged against the
/// pair on disk, never against a kept-over render.
pub(crate) fn ip3c_two_profiles_one_entry_two_card_sets_e2e() {
    use pai_core::gguf_shape::{full_gpu_bytes, local_shape, GPU_RESERVE, MIB};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let shape = local_shape(Path::new(&path)).expect("IP3c review 3: the fixture shape");
    let need = |ctx: u64| full_gpu_bytes(&shape, ctx, "f16", "f16");
    // Both cards find every render of this phase: the story is the device list
    // of each instance's own server profile, not memory.
    let big = need(131_072) / MIB + GPU_RESERVE / MIB + 64;
    let small = need(1024) / MIB + GPU_RESERVE / MIB + 32;
    let m = manager(&["desk"], big, small);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3c review 3: the seeded install renders"
    );
    // `desk` runs the pair the fixture rendered, and records it. `lab` renders
    // too, but its unit is inactive, so its pair never reaches a `running` list.
    mark_applied("desk", &pair_digest(&sys, "desk"));
    let alice = Caller(":1.42".into());
    let launch = |id: &str| {
        std::fs::read_to_string(sys.join(format!("launch@{id}.ini")))
            .unwrap_or_else(|e| panic!("IP3c review 3: the launch of {id}: {e}"))
    };
    // The entry save of ONE profile: the entry of `desk-gpu` moves only `desk`'s
    // render and the entry of `lab-gpu` only `lab`'s, so each save answers the
    // instance that runs the profile it wrote.
    let save_one = |profile: &str, device: &str| {
        let revision = m
            .instance_profile(&alice, profile)
            .expect("IP3c review 3: the read")
            .revision;
        m.set_instance_profile_model(
            &alice,
            profile,
            "alpha",
            &vals(&[("ctx-size", "1024"), ("device-draft", device)]),
            &revision,
        )
        .expect("IP3c review 3: the entry save lands")
    };
    let rows = |a: &Affected| -> Vec<(String, String)> {
        a.not_converged
            .iter()
            .map(|r| (r.instance.clone(), r.key.clone()))
            .collect()
    };

    // ── 3a. The entry names `desk`'s card, on both profiles: `desk` lands,
    // ── `lab` keeps ────────────────────────────────────────────────────────
    let lab = pair_bytes(&sys, "lab");
    let desk_answer = save_one("desk-gpu", "CUDA0");
    assert_eq!(
        desk_answer.running,
        vec!["desk".to_string()],
        "IP3c review 3: `running` is the running instance the save of its own \
         profile moved to a restart: {desk_answer:?}"
    );
    assert!(
        desk_answer.not_converged.is_empty(),
        "IP3c review 3: the save of the fitting profile's entry leaves nobody not \
         converged: {desk_answer:?}"
    );
    let lab_answer = save_one("lab-gpu", "CUDA0");
    assert_eq!(
        rows(&lab_answer),
        vec![(
            "lab".to_string(),
            "model-profile-device-outside".to_string()
        )],
        "IP3c review 3: the same entry on `lab`'s own profile is what stops `lab`, \
         with the device check's key: {lab_answer:?}"
    );
    assert!(
        launch("desk").contains("CUDA0"),
        "IP3c review 3: the render of the instance that fits carries the entry: \
         {}",
        launch("desk")
    );
    assert_eq!(
        pair_bytes(&sys, "lab"),
        lab,
        "IP3c review 3: the instance whose card set the entry names away kept its \
         last good pair byte for byte"
    );
    assert_eq!(
        failure_keys(&m),
        vec![(
            "lab".to_string(),
            "model-profile-device-outside".to_string()
        )],
        "IP3c review 3: the refused instance is the one the row names, with the device \
         check's key"
    );

    // ── 3b. The same save names `lab`'s card, on both profiles: the roles
    // ── swap ──────────────────────────────────────────────────────────────
    let desk = pair_bytes(&sys, "desk");
    let desk_answer = save_one("desk-gpu", "CUDA1");
    assert_eq!(
        desk_answer.running,
        vec!["desk".to_string()],
        "IP3c review 3: `desk` is the running instance and refused, so the save of \
         its own profile answers the drift that stands: `desk` runs the seeded pair, \
         and the pair 3a landed is the one it restarts onto (IP3c review 5: `running` \
         is the drift, not this save's delta): {desk_answer:?}"
    );
    assert_eq!(
        rows(&desk_answer),
        vec![
            (
                "desk".to_string(),
                "model-profile-device-outside".to_string()
            ),
            (
                "lab".to_string(),
                "model-profile-device-outside".to_string()
            ),
        ],
        "IP3c review 3: the save's pass judges every render: the entry of `lab`'s card \
         on `desk`'s profile stops `desk`, and `lab`'s entry from 3a still stands on \
         `desk`'s card, so the pass reports both: {desk_answer:?}"
    );
    let lab_answer = save_one("lab-gpu", "CUDA1");
    assert!(
        lab_answer.running.is_empty(),
        "IP3c review 3: the save walks the profile `lab` runs, `lab` recorded no \
         applied pair, and `desk` runs the other profile, so the answer carries \
         nobody: {lab_answer:?}"
    );
    assert_eq!(
        rows(&lab_answer),
        vec![(
            "desk".to_string(),
            "model-profile-device-outside".to_string()
        )],
        "IP3c review 3: `lab`'s save converges `lab` and leaves only `desk` not \
         converged: {lab_answer:?}"
    );
    assert!(
        launch("lab").contains("CUDA1"),
        "IP3c review 3: the render of the instance that fits carries the entry: \
         {}",
        launch("lab")
    );
    assert_eq!(
        pair_bytes(&sys, "desk"),
        desk,
        "IP3c review 3: `desk` keeps the pair 3a landed: the fit is judged against the \
         pair on disk, the last good one"
    );
    assert_eq!(
        failure_keys(&m),
        vec![(
            "desk".to_string(),
            "model-profile-device-outside".to_string()
        )],
        "IP3c review 3: the row moves to the instance the card set now refuses"
    );
}

/// E2E phase: IP3c review 4. The instance list answers the derived states from
/// the files: the GPU probe is not on its path, and the state of a unit is
/// asked only of the instances that drifted, never of every running unit. An
/// instance running from before its `applied` record existed reads its drift.
pub(crate) fn ip3c_states_ask_nothing_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let roomy = 400_000;
    let mut service = FakeService::new();
    service.running = vec!["desk".to_string()];
    service.state = UnitState::Inactive;
    let gpus = FakeGpus::new(&[("CUDA0", roomy), ("CUDA1", roomy)]);
    let asked = gpus.asked.clone();
    let m = Arc::new(
        Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
            .with_group(FakeGroup::new())
            .with_gpus(gpus),
    );
    m.start_up();
    let alice = Caller(":1.42".into());

    // ── 4a. `desk` runs from before any `applied` record of this install was ─
    // ── written: that is drift by the design, so its row says restart pending ─
    assert_eq!(
        row(&rows(&m, &alice), "desk")["pendingRestart"],
        json!(true),
        "IP3c review 4: an instance started before its `applied` record existed reads \
         its drift, until its next start"
    );
    mark_applied("desk", &pair_digest(&sys, "desk"));
    let probes = asked.lock().unwrap().len();
    let states = m
        .service()
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| matches!(c, Call::Status(_)))
        .count();

    // ── 4b. The core's list asks the probe no card, and the service no unit ──
    let list = m.instances();
    assert!(
        list.iter().any(|i| i.id == "desk"),
        "IP3c review 4: the list carries the seeded instances"
    );
    assert_eq!(
        asked.lock().unwrap().len(),
        probes,
        "IP3c review 4: the list of instances asks the GPU probe nothing: the states \
         are derived from the files"
    );
    assert_eq!(
        m.service()
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| matches!(c, Call::Status(_)))
            .count(),
        states,
        "IP3c review 4: the list of instances asks the state of no unit"
    );

    // ── 4c. The stream's `ListInstances`: the same, over the wire ───────────
    let rows = rows(&m, &alice);
    assert_eq!(
        row(&rows, "desk")["pendingRestart"],
        json!(false),
        "IP3c review 4: the rows arrive with the list read"
    );
    assert_eq!(
        asked.lock().unwrap().len(),
        probes,
        "IP3c review 4: `ListInstances` over the stream asks the probe nothing"
    );
    assert_eq!(
        m.service()
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| matches!(c, Call::Status(_)))
            .count(),
        states,
        "IP3c review 4: `ListInstances` over the stream asks the state of no unit"
    );

    // ── 4d. A drifted instance: the state of THAT one is asked, of no other ──
    // The save of `desk-gpu` moves `desk`'s pair, and `desk` runs: the save's
    // `running` is where the state of `desk` is asked. `lab` runs `lab-gpu`, the
    // profile the save does not touch, so its pair stands and its state is asked
    // of no unit. The list that follows asks the state of no unit at all, so the
    // asks counted below are the save's, and the row for `desk` comes from the
    // files.
    m.add_instance_profile_model(&alice, "desk-gpu", "beta")
        .expect("IP3c review 4: the entry add lands");
    // The save's own pass measures every render against the cards, so the probe's
    // count is taken HERE: the pin below is about the list, not about the save.
    let probes_now = asked.lock().unwrap().len();
    let list = m.instances();
    assert_eq!(
        list.iter()
            .find(|i| i.id == "desk")
            .map(|i| i.pending_restart),
        Some(true),
        "IP3c review 4: the drifted running instance shows its drift"
    );
    let asked_now: Vec<Call> = m
        .service()
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| matches!(c, Call::Status(_)))
        .cloned()
        .collect();
    assert_eq!(
        asked_now,
        vec![Call::Status("desk".to_string())],
        "IP3c review 4: the state is asked only of the instances that drifted"
    );
    assert_eq!(
        asked.lock().unwrap().len(),
        probes_now,
        "IP3c review 4: the list of instances never asks the GPU probe"
    );
}

/// E2E phase: IP3c review 8. The reads of an instance that is NOT converged ask
/// the GPU probe nothing. The memory check runs a probe, so a not-converged
/// instance is the one a read would measure again: the render the files give is
/// larger than the pair on disk, that is growth, and the seam would be asked.
/// The rows of `instances`, of the bus's `InstanceStates` and of the stream's
/// `ListInstances` carry the keyed reason the last render pass left, with its
/// arguments, and ask no card (`ARCHITECTURE.md` "Convergence": not converged is
/// derived, kept in memory, recomputed at start and at every write, never
/// measured at a read). A manager that starts again over the same tree answers
/// the same row from its start-up pass, and its reads ask nothing either.
pub(crate) fn ip3c_not_converged_reads_ask_nothing_e2e() {
    use pai_core::gguf_shape::{full_gpu_bytes, local_shape, GPU_RESERVE, MIB};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let shape = local_shape(Path::new(&path)).expect("IP3c review 8: the fixture shape");
    let need = |ctx: u64| full_gpu_bytes(&shape, ctx, "f16", "f16");
    // `CUDA0` finds the grown entry and `CUDA1` does not, so ONE save is a fit
    // on `desk` and the memory refusal on `lab` (`ip3c_fit_split_e2e`).
    let big = need(131_072) / MIB + GPU_RESERVE / MIB + 64;
    let small = need(1024) / MIB + GPU_RESERVE / MIB + 32;
    // `desk` runs, `lab` does not: the row of an instance that runs is the one a
    // read must not earn by measuring.
    let install = |gpus: FakeGpus| -> Arc<Core> {
        let mut service = FakeService::new();
        service.running = vec!["desk".to_string()];
        service.state = UnitState::Inactive;
        Arc::new(
            Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
                .with_group(FakeGroup::new())
                .with_gpus(gpus),
        )
    };
    let gpus = FakeGpus::new(&[("CUDA0", big), ("CUDA1", small)]);
    let asked = gpus.asked.clone();
    let m = install(gpus);
    m.start_up();
    mark_applied("desk", &pair_digest(&sys, "desk"));
    let alice = Caller(":1.42".into());

    // ── 8a. The save grows the context: `desk` lands, `lab` keeps its pair ───
    let answer = save_profiles(&m, &alice, "alpha", &vals(&[("ctx-size", "131072")]))
        .expect("IP3c review 8: the save that fits one instance lands");
    let kept = match answer.not_converged.as_slice() {
        [one] => one.clone(),
        rows => panic!(
            "IP3c review 8: the save answers one not-converged row, the memory refusal: {rows:?}"
        ),
    };
    assert_eq!(
        (kept.instance.as_str(), kept.key.as_str()),
        ("lab", "model-profile-over-memory"),
        "IP3c review 8: the memory check is what stops `lab`: {answer:?}"
    );
    // The save's own pass measures the renders on the cards: its asks are the
    // base, every pin below is about the READS that follow it.
    let probes = asked.lock().unwrap().len();
    let reason = json!({ "key": kept.key, "args": kept.args });
    let want = (false, Some((kept.key.as_str(), kept.args.as_slice())));

    // ── 8b. The core's list: the row the pass left, no card asked ───────────
    let list = m.instances();
    let lab = list
        .iter()
        .find(|i| i.id == "lab")
        .expect("IP3c review 8: the list carries `lab`");
    assert_eq!(
        (
            lab.converged,
            lab.reason
                .as_ref()
                .map(|r| (r.key.as_str(), r.args.as_slice()))
        ),
        want,
        "IP3c review 8: the list's row for the not-converged instance carries the reason \
         the save answered: {lab:?}"
    );
    assert_eq!(
        asked.lock().unwrap().len(),
        probes,
        "IP3c review 8: the list of instances asks the GPU probe no card, not even for an \
         instance the memory check refused"
    );

    // ── 8c. The bus's sibling method `InstanceStates`, beside `Instances`: the ─
    // ── same rows, the same rule (`manager::dbus::instance_states` maps it) ──
    #[cfg(target_os = "linux")]
    {
        let (_server, client) = bus(&m);
        let states: Vec<(String, bool, String, Vec<String>, bool)> = client
            .call_method(
                Some(BUS_NAME),
                OBJECT_PATH,
                Some(INTERFACE),
                "InstanceStates",
                &(),
            )
            .expect("IP3c review 8: the bus answers `InstanceStates`")
            .body()
            .deserialize()
            .expect("IP3c review 8: `InstanceStates` is `a(sbssasb)`");
        let state = states
            .iter()
            .find(|(id, ..)| id == "lab")
            .unwrap_or_else(|| panic!("IP3c review 8: the bus's rows carry `lab`: {states:?}"));
        assert_eq!(
            (state.1, state.2.as_str(), state.3.as_slice()),
            (false, kept.key.as_str(), kept.args.as_slice()),
            "IP3c review 8: the bus's row for the not-converged instance carries the reason \
             the save answered: {states:?}"
        );
        assert_eq!(
            asked.lock().unwrap().len(),
            probes,
            "IP3c review 8: `InstanceStates` over the bus asks the GPU probe no card"
        );
    }

    // ── 8d. The stream's `ListInstances`: the same row, over the wire ───────
    let listed = rows(&m, &alice);
    assert_eq!(
        row(&listed, "lab")["converged"],
        json!(false),
        "IP3c review 8: the stream's row for `lab` says not converged: {listed:?}"
    );
    assert_eq!(
        row(&listed, "lab")["reason"],
        reason,
        "IP3c review 8: the stream's row for `lab` carries the reason the save answered: \
         {listed:?}"
    );
    assert_eq!(
        asked.lock().unwrap().len(),
        probes,
        "IP3c review 8: `ListInstances` over the stream asks the GPU probe no card"
    );

    // ── 8e. A manager that starts again over the same tree: its start-up pass ─
    // ── measured the renders, and the reads of the same row ask nothing ───────
    let gpus2 = FakeGpus::new(&[("CUDA0", big), ("CUDA1", small)]);
    let asked2 = gpus2.asked.clone();
    let m2 = install(gpus2);
    m2.start_up();
    let probes2 = asked2.lock().unwrap().len();
    let lab2 = m2
        .instances()
        .into_iter()
        .find(|i| i.id == "lab")
        .expect("IP3c review 8: the new manager's list carries `lab`");
    assert_eq!(
        (
            lab2.converged,
            lab2.reason
                .as_ref()
                .map(|r| (r.key.as_str(), r.args.as_slice()))
        ),
        want,
        "IP3c review 8: a manager that starts again over the same tree answers the same \
         reason, from its start-up pass: {lab2:?}"
    );
    assert_eq!(
        asked2.lock().unwrap().len(),
        probes2,
        "IP3c review 8: the new manager's list asks the GPU probe no card: the pass at \
         start is what measured the renders"
    );
}

/// E2E phase: IP3c review 5. A save's `running` is the running instances that
/// drift from their `applied` record at the moment it answers, over the verbs of
/// the pool and the entries: the pool add moves `desk`'s pair, so it answers
/// `desk`; the pool remove puts that pair back on the record, so it answers
/// nobody; and from there a verb that moves no render answers nobody either,
/// `desk` running all the while.
pub(crate) fn ip3c_running_is_drifted_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let roomy = 400_000;
    let m = manager(&["desk"], roomy, roomy);
    m.start_up();
    mark_applied("desk", &pair_digest(&sys, "desk"));
    let alice = Caller(":1.42".into());
    let seeded = pair_bytes(&sys, "desk");

    // ── 5a. The pool add over the stream: `desk`'s pair moves, and it runs ─
    converging(
        &m,
        &sys,
        "the pool add",
        &["desk"],
        json!({"method": "AddInstanceProfileModel", "profile": "desk-gpu", "model": "beta"}),
    );

    // ── 5b. The pool remove puts the pair back on the `applied` record: the ─
    // ── answer carries nobody, `desk` running and all (it drifts no more) ────
    let answer = call(
        &m,
        &alice,
        json!({"method": "RemoveInstanceProfileModel", "profile": "desk-gpu", "model": "beta"}),
    );
    let ok = answer
        .get("ok")
        .unwrap_or_else(|| panic!("IP3c review 5: the pool remove lands: {answer}"));
    assert_eq!(
        pair_bytes(&sys, "desk"),
        seeded,
        "IP3c review 5: the remove puts the pair back byte for byte, models beside launch"
    );
    assert_eq!(
        ok["running"],
        json!([]),
        "IP3c review 5: `desk` runs, its pair stands on its `applied` record, so it drifts \
         no more and the answer carries nobody: {answer}"
    );
    rows_answered(&m, ok, "the pool remove");

    // ── 5c. From there, a verb that moves no render answers nobody: nothing ─
    // ── drifted when it was asked, the running instance included ─────────────
    unmoved(
        &m,
        &sys,
        "the model profile switch",
        &[],
        json!({"method": "SetModelProfile", "instance": "desk", "profile": "alpha", "value": "on"}),
    );
}

/// E2E phase: IP3c review 7. The `load-on-startup` count is a FIT check of the
/// instance: it belongs to the instance (its `ModelsMax`, its model list), so a
/// save that grows one instance's count over that instance's cap lands, leaves
/// that instance's pair byte for byte, and reports it with the check's key. The
/// other instance runs its own profile with the same model: its list and its cap
/// judge it on its own.
pub(crate) fn ip3c_startup_is_a_fit_check_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    // `alpha` loads at startup. `load-on-startup` is a placement key, so the
    // entry carries it and the merged pool carries none: which of an instance's
    // models sit in memory from the start is the instance's own. `desk`'s
    // render starts at the one profile its instance profile lists, under the
    // cap of one set on its server profile; the entry that makes `beta` load,
    // once the add has given it to the profile, grows that render over the cap.
    // A preset counts toward the startup load only when its entry names
    // `load-on-startup` truthily (llama.cpp `tools/server/server-models.cpp`,
    // the rule `pai_core::checks::startup_loads` reads).
    let _path = seed(
        dir.path(),
        &sys,
        "ctx-size = 1024\nload-on-startup = true\n",
    );
    std::fs::write(
        sys.join("server@desk.ini"),
        "[Server]\nHostname = 127.0.0.1\nPort = 8581\nDevices = CUDA0\nModelsMax = 1\n",
    )
    .expect("server@desk.ini");
    let roomy = 400_000;
    let m = manager(&["desk"], roomy, roomy);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3c review 7: the seeded install renders (one profile loads, under the cap)"
    );
    let alice = Caller(":1.42".into());
    mark_applied("desk", &pair_digest(&sys, "desk"));

    // ── 7a. The entry that makes the added model load grows over the cap ───
    // The add of `beta` starts nothing, so it grows no count and lands; the
    // save of its entry puts a second profile that loads at startup on `desk`'s
    // cards, over its `ModelsMax = 1`. `lab` runs `lab-gpu`, which lists `beta`
    // as well, on a server profile with no cap of its own and with an entry that
    // loads one profile, so the same count is judged on its own and it fits.
    let added = call(
        &m,
        &alice,
        json!({"method": "AddInstanceProfileModel", "profile": "desk-gpu", "model": "beta"}),
    );
    assert!(
        added.get("ok").is_some(),
        "IP3c review 7: the add of a model that loads nothing lands: {added}"
    );
    m.add_instance_profile_model(&alice, "lab-gpu", "beta")
        .expect("IP3c review 7: the same model added to the profile `lab` runs");
    // The pair the add left is `desk`'s last good pair: the save that follows is
    // judged against it (IP3c), so it is the bytes the pin below reads back.
    let desk = pair_bytes(&sys, "desk");
    let answer = call(
        &m,
        &alice,
        json!({"method": "SetInstanceProfileModel", "profile": "desk-gpu", "model": "beta",
               "values": [{"key": "load-on-startup", "value": "true"}],
               "revision": m
                   .instance_profile(&alice, "desk-gpu")
                   .expect("IP3c review 7: the read")
                   .revision}),
    );
    let ok = answer.get("ok").unwrap_or_else(|| {
        panic!("IP3c review 7: the save lands, and the stream answers {answer}")
    });
    assert_eq!(
        ok["notConverged"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|r| (text(r, "instance"), text(r, "key")))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        vec![("desk".to_string(), "load-on-startup-over-max".to_string())],
        "IP3c review 7: the instance over its own cap kept its last good pair and is \
         reported with the check's key: {answer}"
    );
    assert_eq!(
        pair_bytes(&sys, "desk"),
        desk,
        "IP3c review 7: the over-cap instance kept its pair byte for byte"
    );
    assert_eq!(
        failure_keys(&m),
        vec![("desk".to_string(), "load-on-startup-over-max".to_string())],
        "IP3c review 7: the row is the one instance whose list grows over its cap \
         (the count is per instance: `lab` has no cap of its own to break)"
    );
    let reason = refusal_of(&m, "desk").expect("IP3c review 7: the row carries the refusal");
    let named: Vec<&str> = reason.values.iter().map(String::as_str).collect();
    assert!(
        named.contains(&"desk"),
        "IP3c review 7: the keyed check names the instance it stopped: {named:?}"
    );
}
