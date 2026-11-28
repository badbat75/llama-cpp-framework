//! IP3b: the entry-key save of an instance profile, the verbs that answer the
//! running instances, `CreateModelProfile` into `models.ini` with the stay keys
//! only, and the moved keys' checks over the entry keys of
//! `instance-profiles.ini` (`ARCHITECTURE.md` "Instance profiles", its checks
//! and slice IP3 part b).
//!
//! Every phase runs through `isolated` (the `#[test]`s of `mod.rs`), each in a
//! child of its own over a data root of its own, and writes only the engine's
//! config folder of that root.

use crate::manager::service::fakes::{
    FakeAuthority, FakeGpus, FakeGroup, FakeResolver, FakeService,
};
use crate::manager::service::{
    write_gguf_kv, Caller, Manager, ManagerError, UnitState, INSTANCES_FILE,
};
use std::path::Path;

const PROFILES: &str = "instance-profiles.ini";
const ASSIGNMENTS: &str = "assignments.ini";
const POOL: &str = "models.ini";
const OFF_POOL: &str = "models.ini.disabled";

/// The card the phases that do not measure memory hand the manager: big enough
/// that no save of these phases is a memory story.
const BIG_CARD: u64 = 400_000;

/// The record a launched instance leaves of the pair it started with: `applied`
/// in its own state folder, which its account writes and the manager reads
/// (IP3c). A phase marks it at the pair the fixture rendered, so the `running`
/// of a save's answer is the drift of a real pair: a save that leaves the pair
/// of a running instance asks it for no restart, one that moves it does.
fn mark_applied(id: &str) {
    let path = crate::manager::service::applied_file(id).expect("the applied record's path");
    std::fs::create_dir_all(path.parent().expect("the instance's state folder"))
        .expect("the instance's state folder");
    std::fs::write(
        &path,
        crate::manager::service::pair_digest(id).expect("the pair on disk"),
    )
    .expect("the applied record");
}

/// A `llama` GGUF whose KV layout `pai_core::gguf_shape` reads: the same keys
/// the memory fixtures of `service.rs` write, so the shape measured here is the
/// shape the memory check measures there.
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

/// The manager over the redirected tree: `running` are the units the service
/// manager answers active, `card` the MiB of the node's one `CUDA0` card.
fn manager(running: &[&str], card: u64) -> Manager<FakeService, FakeAuthority, FakeResolver> {
    let service = FakeService {
        running: running.iter().map(|s| (*s).to_string()).collect(),
        state: UnitState::Inactive,
        ..FakeService::new()
    };
    Manager::new(service, FakeAuthority::new(true), FakeResolver::lan())
        .with_group(FakeGroup::new())
        .with_gpus(FakeGpus::new(&[("CUDA0", card)]))
}

/// The install of the IP3b phases, seeded with the record already migrated: a
/// start merges the off pool's `beta` into `models.ini`, the one pool, and
/// retires `models.ini.disabled` and `assignments.ini` behind their `.2.99.0`
/// backups, so every phase reads the pool as the one file. The instances `lab` and `desk` (in the record's order, the
/// one the running list follows) both run the instance profile `gpu`, whose
/// server profile `desk` names `CUDA0` alone and whose other server profile
/// `wide` names `CUDA0, CUDA1`. The pool `models.ini` holds `alpha` (its model
/// file, one sampling key, and `ctx-size`, a key that moved to the entry) and
/// `beta` in the off pool; the entry `[gpu/alpha]` carries `entry_keys`.
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
        &format!("[*]\ntop-p = 0.9\n\n[alpha]\nmodel = {path}\ntemp = 0.7\n"),
    );
    write(OFF_POOL, &format!("[beta]\nmodel = {path}\ntemp = 0.3\n"));
    write(
        INSTANCES_FILE,
        "[lab]\nInstanceProfile = gpu\nPort = 8681\n\n[desk]\nInstanceProfile = gpu\nPort = 8581\n",
    );
    write(
        "server@desk.ini",
        "[Server]\nHostname = 127.0.0.1\nPort = 8581\nDevices = CUDA0\n",
    );
    write(
        "server@wide.ini",
        "[Server]\nHostname = 127.0.0.1\nPort = 8681\nDevices = CUDA0, CUDA1\n",
    );
    write(
        PROFILES,
        &format!("[gpu]\nServerProfile = desk\n\n[gpu/alpha]\n{entry_keys}"),
    );
    path
}

/// A `Vec<(String, String)>` of key pairs, the shape every write verb takes.
fn vals(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

/// A refusal of `key` carrying `facts` as its values, in any order: every fact
/// is pinned, the order of a keyed refusal's arguments is the strings file's.
fn refused_key(answer: &Result<Vec<String>, ManagerError>, key: &str, facts: &[&str]) -> bool {
    match answer {
        Err(ManagerError::Invalid(r)) if r.key == key => {
            let mut got: Vec<&str> = r.values.iter().map(String::as_str).collect();
            let mut want: Vec<&str> = facts.to_vec();
            got.sort();
            want.sort();
            got == want
        }
        _ => false,
    }
}

/// A keyed refusal naming `needle`, whatever its key: for a refusal of a rule
/// whose key the strings files do not carry yet.
fn refused_naming<T>(answer: &Result<T, ManagerError>, needle: &str) -> bool {
    matches!(
        answer,
        Err(ManagerError::Invalid(r)) if r.values.iter().any(|v| v.contains(needle))
    )
}

/// The keys of the section `[section]` of the text `text`, in the file's order.
fn keys_of(text: &str, section: &str) -> Vec<(String, String)> {
    pai_core::ini::section_keys(text, section)
}

/// The `ServerProfile` key of the section `[profile]` of `text`.
fn server_profile_of(text: &str, profile: &str) -> String {
    keys_of(text, profile)
        .into_iter()
        .find(|(k, _)| k == "ServerProfile")
        .map(|(_, v)| v)
        .unwrap_or_default()
}

/// The section `[alpha]` of the render of `id`, text included.
fn render_section(sys: &Path, id: &str) -> String {
    let text = std::fs::read_to_string(sys.join(format!("models@{id}.ini"))).unwrap_or_default();
    pai_core::ini::text::section_text(&text, "alpha").unwrap_or_default()
}

/// The usable memory, in MiB, of one `CUDA0` card of `total_mib`.
fn usable(card: u64) -> u64 {
    pai_core::devices::pool_usable(&[pai_core::devices::Gpu {
        id: "CUDA0".to_string(),
        name: "NVIDIA GeForce CUDA0".to_string(),
        total_mib: card,
    }])
}

/// E2E phase: IP3b 1. `CreateModelProfile` writes `models.ini`, the pool the
/// new shape has, and a copy of a profile carries that profile's stay keys
/// only: the placement and memory-sized keys belong to the entry, so a copy
/// that carried them would put a hardware rule back into the hardware-free
/// pool. The new profile joins no instance profile's entries, so nothing
/// renders, and the switch file the migration took away is never written.
pub(crate) fn ip3b_create_model_profile_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let m = manager(&["desk"], BIG_CARD);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3b 1: the seeded install renders"
    );

    let alice = Caller(":1.42".into());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);
    let bytes = |name: &str| std::fs::read(sys.join(name)).ok();
    let profiles_before = bytes(PROFILES);
    let renders_before = (bytes("models@desk.ini"), bytes("models@lab.ini"));

    // ── 1a. The copy: the source's stay keys, its placement keys left out ──
    assert_eq!(
        m.create_model_profile(&alice, "copy", "alpha", &[]),
        Ok(()),
        "IP3b 1a: a copy of a pool profile lands"
    );
    let pool = read(POOL);
    assert_eq!(
        keys_of(&pool, "copy"),
        vec![
            ("model".to_string(), path.clone()),
            ("temp".to_string(), "0.7".to_string())
        ],
        "IP3b 1a: the copy carries the source's model and its sampling key, and \
         none of the keys that moved to the entry (no `ctx-size`), nor the pool's \
         `[*]` keys"
    );
    assert_eq!(
        bytes(OFF_POOL),
        None,
        "IP3b 1a: the new profile is written to `models.ini`: the switch file is not \
         there to write (the start retired it), and a create brings it back not"
    );
    assert!(
        bytes(&format!("{OFF_POOL}.2.99.0")).is_some(),
        "IP3b 1a: the retired switch file stands as the migration's backup, the only \
         place its text is kept"
    );
    assert_eq!(
        bytes(ASSIGNMENTS),
        None,
        "IP3b 1a: the create writes no assignment record, retired as it is"
    );
    assert_eq!(
        bytes(PROFILES),
        profiles_before,
        "IP3b 1a: a new model profile joins no instance profile's entries"
    );
    assert_eq!(
        (bytes("models@desk.ini"), bytes("models@lab.ini")),
        renders_before,
        "IP3b 1a: a profile no entry lists renders nothing"
    );

    // ── 1b. The refusals, and the pool they leave alone ──────────────────
    // The pool as 1a left it, with the copy's section 1a added (1a pinned it):
    // these refusals write on top of a create that landed, so THIS is the bytes
    // they must not move.
    let pool_after_1a = bytes(POOL);
    assert_eq!(
        m.create_model_profile(&alice, "copy", "alpha", &[]),
        Err(ManagerError::Invalid(pai_core::checks::Refusal {
            key: "model-profile-name-taken",
            values: vec!["copy".to_string()],
        })),
        "IP3b 1b: a name the pool holds is refused, naming it"
    );
    assert_eq!(
        m.create_model_profile(&alice, "ghost", "nope", &[]),
        Err(ManagerError::Invalid(pai_core::checks::Refusal {
            key: "no-model-profile-named",
            values: vec!["nope".to_string()],
        })),
        "IP3b 1b: a source the pool has no section for is refused, naming it"
    );
    let moved = m.create_model_profile(
        &alice,
        "wider",
        "",
        &vals(&[("model", path.as_str()), ("n-cpu-moe", "4")]),
    );
    assert!(
        matches!(&moved, Err(ManagerError::Invalid(r))
            if r.key == "model-profile-placement-key" && r.values == ["n-cpu-moe"]),
        "IP3b 1b: a create of the pool carrying a key that moved to the entry is \
         refused, naming it: {moved:?}"
    );
    assert_eq!(
        bytes(POOL),
        pool_after_1a,
        "IP3b 1b: every refusal left the pool 1a wrote"
    );
    assert_eq!(
        bytes(PROFILES),
        profiles_before,
        "IP3b 1b: every refusal wrote the instance profiles"
    );
}

/// E2E phase: IP3b 2. The entry-key save: `SetInstanceProfileModel(p, m,
/// values, revision)` makes the entry's keys exactly what the page sent and
/// answers the running instances that use the profile, the ones the page offers
/// a restart to. The entry keys ride the render of every instance that runs the
/// profile, over the model profile's own keys.
pub(crate) fn ip3b_entry_save_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    // `desk` runs, `lab` does not.
    let m = manager(&["desk"], BIG_CARD);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3b 2: the seeded install renders"
    );
    // `desk` runs the pair the fixture rendered (IP3c): the record of it is what
    // makes this phase's `running` list the drift of a pair.
    mark_applied("desk");

    let alice = Caller(":1.42".into());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);
    let pool = || read(PROFILES);
    let revision = || {
        m.instance_profile(&alice, "gpu")
            .expect("IP3b 2: the read")
            .revision
    };
    assert_eq!(
        revision(),
        pool(),
        "IP3b 2: the revision the save takes is the text of the pool the read answers"
    );

    // ── 2a. The save, and the running instances it answers ───────────────
    let sent = vals(&[("ctx-size", "2048"), ("n-cpu-moe", "4")]);
    let answer = m
        .set_instance_profile_model(&alice, "gpu", "alpha", &sent, &revision())
        .map(|a| a.running);
    assert_eq!(
        answer,
        Ok(vec!["desk".to_string()]),
        "IP3b 2a: the save answers the running instances that use the profile, \
         and `lab` is not running"
    );
    assert_eq!(
        keys_of(&pool(), "gpu/alpha"),
        sent,
        "IP3b 2a: the entry holds exactly the keys the page sent, in the order it \
         sent them"
    );
    assert_eq!(
        pai_core::ini::text::section_text(&pool(), "gpu"),
        Some("[gpu]\nServerProfile = desk\n".to_string()),
        "IP3b 2a: the save writes the entry, not the profile's own section"
    );
    for id in ["desk", "lab"] {
        let section = render_section(&sys, id);
        assert!(
            section.contains("ctx-size = 2048") && section.contains("n-cpu-moe = 4"),
            "IP3b 2a: the render of {id} carries the entry's keys over the model \
             profile's own: {section}"
        );
        assert!(
            section.contains("temp = 0.7"),
            "IP3b 2a: the render keeps the model profile's stay keys: {section}"
        );
    }

    // ── 2b. The save of an entry the profile does not list ─────────────────
    let answer = m
        .set_instance_profile_model(
            &alice,
            "gpu",
            "beta",
            &vals(&[("ctx-size", "1")]),
            &revision(),
        )
        .map(|a| a.running);
    assert!(
        refused_key(&answer, "no-instance-profile-model", &["gpu", "beta"]),
        "IP3b 2b: an entry the profile does not list is refused, naming the \
         profile and the model: {answer:?}"
    );
    assert!(
        pai_core::ini::text::section_text(&pool(), "gpu/beta").is_none(),
        "IP3b 2b: the refused save wrote no entry"
    );

    // ── 2c. The save of a profile the pool has no section for ──────────────
    assert_eq!(
        m.set_instance_profile_model(
            &alice,
            "nope",
            "alpha",
            &vals(&[("ctx-size", "1")]),
            &revision()
        )
        .map(|a| a.running),
        Err(ManagerError::NoInstanceProfile("nope".to_string())),
        "IP3b 2c: a profile the pool has no section for is refused, naming it"
    );
    assert_eq!(
        keys_of(&pool(), "gpu/alpha"),
        sent,
        "IP3b 2: a refusal wrote no entry"
    );
}

/// E2E phase: IP3b 3. The entry save's refusals: a revision the pool no longer
/// holds, a key that stays in the model profile, a number out of range, a
/// tensor rule of no shape. Every refusal writes nothing. A device outside the
/// profile's OWN server profile `Devices` is a fit check, not a refusal: the
/// save lands and the instance whose render stops keeps its last good pair
/// (IP3c review 2).
pub(crate) fn ip3b_entry_save_refusals_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let m = manager(&["desk"], BIG_CARD);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3b 3: the seeded install renders"
    );
    // `desk` runs the pair the fixture rendered (IP3c): the record of it is what
    // makes this phase's `running` list the drift of a pair.
    mark_applied("desk");

    let alice = Caller(":1.42".into());
    let bytes = |name: &str| std::fs::read(sys.join(name)).ok();
    let records_before = (bytes(INSTANCES_FILE), bytes(POOL));
    let revision = || {
        m.instance_profile(&alice, "gpu")
            .expect("IP3b 3: the read")
            .revision
    };
    let save = |pairs: &[(&str, &str)], revision: &str| {
        m.set_instance_profile_model(&alice, "gpu", "alpha", &vals(pairs), revision)
            .map(|a| a.running)
    };

    // ── 3a. A stale revision ─────────────────────────────────────────────
    assert_eq!(
        save(&[("ctx-size", "2048")], "STALE TEXT"),
        Err(ManagerError::Changed),
        "IP3b 3a: a revision the pool no longer holds is refused"
    );

    // ── 3b. A key that stays in the model profile ────────────────────────
    let answer = save(&[("ctx-size", "2048"), ("temp", "1.1")], &revision());
    assert!(
        refused_key(&answer, "model-profile-stay-key", &["temp"]),
        "IP3b 3b: a key of the model profile is refused on the entry, naming it: \
         {answer:?}"
    );

    // ── 3c. A number out of range ────────────────────────────────────────
    let answer = save(&[("n-cpu-moe", "5000")], &revision());
    assert!(
        refused_key(
            &answer,
            "not-whole-number-in-range",
            &["n-cpu-moe", "5000", "0", "4096"]
        ),
        "IP3b 3c: the range check rides the entry write: {answer:?}"
    );

    // ── 3d. A tensor rule of no shape ────────────────────────────────────
    let answer = save(&[("tensor-split", "abc,1")], &revision());
    assert!(
        refused_key(&answer, "tensor-split-format", &["abc,1"]),
        "IP3b 3d: the tensor rule check rides the entry write: {answer:?}"
    );
    let answer = save(&[("override-tensor", "blk.0")], &revision());
    assert!(
        refused_key(&answer, "override-no-device", &["1", "blk.0"]),
        "IP3b 3d: a tensor rule naming no device is refused, naming the rule: \
         {answer:?}"
    );

    // ── 3e. A device outside the profile's own server profile ────────────
    // `gpu` runs `desk`, whose `Devices` is `CUDA0` alone: the entry's card is
    // judged against the profile's own server profile, the same file. The fit
    // is the instance's own, so the save LANDS: the entry is written, the
    // instance whose render stops on the card keeps its last good pair byte for
    // byte, and is reported not converged with the check's key (IP3c review 2,
    // `ARCHITECTURE.md` "Convergence": the fit checks decide per instance).
    let seed_pair = (bytes("models@desk.ini"), bytes("launch@desk.ini"));
    let answer = save(
        &[("ctx-size", "1024"), ("device-draft", "CUDA1")],
        &revision(),
    );
    assert_eq!(
        answer,
        Ok(Vec::<String>::new()),
        "IP3c 3e: the entry of a card no instance's server profile names lands, and \
         moves no running instance to a restart: {answer:?}"
    );
    let text = std::fs::read_to_string(sys.join(PROFILES)).expect(PROFILES);
    assert!(
        pai_core::ini::text::section_text(&text, "gpu/alpha")
            .unwrap_or_default()
            .contains("device-draft = CUDA1"),
        "IP3c 3e: the save wrote the entry: {text}"
    );
    assert_eq!(
        (bytes("models@desk.ini"), bytes("launch@desk.ini")),
        seed_pair,
        "IP3c 3e: the refused render left the pair on disk byte for byte"
    );
    let failures: Vec<(String, String)> = m
        .render_failures()
        .iter()
        .map(|(id, e)| (id.clone(), format!("{e:?}")))
        .collect();
    assert!(
        failures.iter().any(|(id, e)| id == "desk"
            && e.contains("model-profile-device-outside")
            && e.contains("CUDA1")),
        "IP3c 3e: the instance whose render stopped is reported with the device check, \
         naming the card: {failures:?}"
    );
    let answer = save(
        &[("ctx-size", "1024"), ("override-tensor", "blk.0=CUDA1")],
        &revision(),
    );
    assert_eq!(
        answer,
        Ok(Vec::<String>::new()),
        "IP3c 3e: an `override-tensor` rule of a card the profile's server profile \
         does not name lands too, the render refused per instance: {answer:?}"
    );
    // A card the server profile names lands.
    assert_eq!(
        save(
            &[("ctx-size", "1024"), ("device-draft", "CUDA0")],
            &revision()
        ),
        Ok(vec!["desk".to_string()]),
        "IP3b 3e: a device the profile's server profile names is not refused"
    );

    // ── 3f. What the refusals left ───────────────────────────────────────
    let text = std::fs::read_to_string(sys.join(PROFILES)).expect(PROFILES);
    assert_eq!(
        keys_of(&text, "gpu/alpha"),
        vec![
            ("ctx-size".to_string(), "1024".to_string()),
            ("device-draft".to_string(), "CUDA0".to_string())
        ],
        "IP3b 3f: only the accepted save's keys stand in the entry"
    );
    assert_eq!(
        pai_core::ini::text::section_text(&text, "gpu"),
        Some("[gpu]\nServerProfile = desk\n".to_string()),
        "IP3b 3f: no refusal moved the profile's server profile"
    );
    for id in ["desk", "lab"] {
        let section = render_section(&sys, id);
        assert!(
            section.contains("ctx-size = 1024") && !section.contains("CUDA1"),
            "IP3b 3f: the render of {id} carries the accepted keys only: {section}"
        );
    }
    assert_eq!(
        bytes(INSTANCES_FILE),
        records_before.0,
        "IP3b 3: no entry write touched the instance record"
    );
    assert_eq!(
        bytes(POOL),
        records_before.1,
        "IP3b 3: no entry write touched the model profile pool"
    );
    assert_eq!(
        bytes(ASSIGNMENTS),
        None,
        "IP3b 3: no entry write created the retired assignment record"
    );
    assert!(
        m.render_failures().is_empty(),
        "IP3b 3: every render of the phase landed"
    );
}

/// E2E phase: IP3b 4. The entry add and remove carry no revision, refuse their
/// own conflict (the model already listed, the model not listed) and answer the
/// running instances that use the profile, in the record's order.
pub(crate) fn ip3b_add_remove_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    // Both instances run, `lab` first of the record.
    let m = manager(&["desk", "lab"], BIG_CARD);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3b 4: the seeded install renders"
    );
    // Both instances run the pair the fixture rendered (IP3c): the record of it
    // is what makes this phase's `running` list the drift of a pair.
    for id in ["desk", "lab"] {
        mark_applied(id);
    }

    let alice = Caller(":1.42".into());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);
    let bytes = |name: &str| std::fs::read(sys.join(name)).ok();
    let pool = || read(PROFILES);
    let records_before = (bytes(INSTANCES_FILE), bytes(POOL));
    let both = vec!["lab".to_string(), "desk".to_string()];

    // ── 4a. The add of a model the profile already lists ─────────────────
    let answer = m.add_instance_profile_model(&alice, "gpu", "alpha");
    assert!(
        refused_key(&answer, "instance-profile-model-listed", &["gpu", "alpha"]),
        "IP3b 4a: the add of a model the profile lists is refused on its own \
         conflict, naming the profile and the model: {answer:?}"
    );
    assert!(
        pai_core::ini::text::section_text(&pool(), "gpu/alpha").is_some(),
        "IP3b 4a: the refused add cut no entry"
    );

    // ── 4b. The add that lands, and the running list ─────────────────────
    assert_eq!(
        m.add_instance_profile_model(&alice, "gpu", "beta"),
        Ok(both.clone()),
        "IP3b 4b: the add answers the running instances that use the profile, in \
         the record's own order"
    );
    assert_eq!(
        pai_core::ini::text::section_text(&pool(), "gpu/beta"),
        Some("[gpu/beta]\n".to_string()),
        "IP3b 4b: the entry is the empty section the pool's shape gives it"
    );
    for id in ["desk", "lab"] {
        let text = read(&format!("models@{id}.ini"));
        assert!(
            pai_core::ini::parse_sections(&text)
                .iter()
                .any(|s| s.id == "beta"),
            "IP3b 4b: the add lands in the render of every instance running the \
             profile: {id}"
        );
    }

    // Both restart onto the pair the add left (IP3c): their `applied` record is
    // the pair on disk now, so the remove that follows moves each pair off it.
    for id in ["desk", "lab"] {
        mark_applied(id);
    }

    // ── 4c. The remove of a model the profile does not list ──────────────
    let answer = m.remove_instance_profile_model(&alice, "gpu", "gamma");
    assert!(
        refused_key(&answer, "no-instance-profile-model", &["gpu", "gamma"]),
        "IP3b 4c: the remove of a model the profile does not list is refused on \
         its own conflict, naming the profile and the model: {answer:?}"
    );
    assert!(
        pai_core::ini::text::section_text(&pool(), "gpu/beta").is_some(),
        "IP3b 4c: the refused remove cut no entry"
    );

    // ── 4d. The remove that lands, and the running list ──────────────────
    assert_eq!(
        m.remove_instance_profile_model(&alice, "gpu", "beta"),
        Ok(both),
        "IP3b 4d: the remove answers the running instances that use the profile"
    );
    assert_eq!(
        pai_core::ini::text::section_text(&pool(), "gpu/beta"),
        None,
        "IP3b 4d: the entry section is cut out"
    );
    for id in ["desk", "lab"] {
        let text = read(&format!("models@{id}.ini"));
        assert!(
            !pai_core::ini::parse_sections(&text)
                .iter()
                .any(|s| s.id == "beta"),
            "IP3b 4d: the model leaves the render of every instance running the \
             profile: {id}"
        );
    }
    assert_eq!(
        bytes(INSTANCES_FILE),
        records_before.0,
        "IP3b 4: no add and no remove wrote the instance record"
    );
    assert_eq!(
        bytes(POOL),
        records_before.1,
        "IP3b 4: no add and no remove wrote the model profile pool"
    );
    assert_eq!(
        bytes(ASSIGNMENTS),
        None,
        "IP3b 4: no add and no remove created the retired assignment record"
    );
}

/// E2E phase: IP3b 5. The server profile switch of an instance profile:
/// `SetInstanceProfileServer(p, server)` carries no revision, answers the
/// running instances whose pair it moves, and re-checks EVERY entry against
/// the new server profile's `Devices`. The check is a fit check, so it never
/// refuses the switch: a switch that takes a card an entry names away lands,
/// the instances running the profile keep their last good pair, and each is
/// reported not converged (IP3c review 2).
pub(crate) fn ip3b_server_switch_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let _path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let m = manager(&["desk", "lab"], BIG_CARD);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3b 5: the seeded install renders"
    );
    // Both instances run the pair the fixture rendered (IP3c): the record of it
    // is what makes this phase's `running` list the drift of a pair.
    for id in ["desk", "lab"] {
        mark_applied(id);
    }

    let alice = Caller(":1.42".into());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);
    let bytes = |name: &str| std::fs::read(sys.join(name)).ok();
    let pool = || read(PROFILES);
    let revision = || {
        m.instance_profile(&alice, "gpu")
            .expect("IP3b 5: the read")
            .revision
    };
    let records_before = (bytes(INSTANCES_FILE), bytes(POOL));
    let both = vec!["lab".to_string(), "desk".to_string()];

    // ── 5a. The refusals of the switch itself ────────────────────────────
    assert_eq!(
        m.set_instance_profile_server(&alice, "nope", "wide"),
        Err(ManagerError::NoInstanceProfile("nope".to_string())),
        "IP3b 5a: a profile the pool has no section for is refused, naming it"
    );
    assert_eq!(
        m.set_instance_profile_server(&alice, "gpu", "nope"),
        Err(ManagerError::NoServerProfile("nope".to_string())),
        "IP3b 5a: a server profile the node has no file for is refused, naming it"
    );

    // ── 5b. The switch, and the entry that fits the new cards ────────────
    // `gpu` runs `desk` and `lab`, both on `desk` (`CUDA0`): an entry naming
    // `CUDA1` fails the fit of both. The fit is each instance's own, so the
    // save LANDS (IP3c review 2): the entry is written, every instance keeps
    // its last good pair byte for byte, and the save moves no running
    // instance to a restart.
    let seed_pairs: Vec<(Vec<u8>, Vec<u8>)> = ["desk", "lab"]
        .iter()
        .map(|id| {
            (
                bytes(&format!("models@{id}.ini")).expect("rendered profiles"),
                bytes(&format!("launch@{id}.ini")).expect("rendered launch"),
            )
        })
        .collect();
    let outside = m
        .set_instance_profile_model(
            &alice,
            "gpu",
            "alpha",
            &vals(&[("ctx-size", "1024"), ("device-draft", "CUDA1")]),
            &revision(),
        )
        .map(|a| a.running);
    assert_eq!(
        outside,
        Ok(Vec::<String>::new()),
        "IP3c 5b: on `desk` the entry's `CUDA1` fails the fit of every instance \
         running the profile, and the save lands moving nobody: {outside:?}"
    );
    for (id, pair) in ["desk", "lab"].iter().zip(&seed_pairs) {
        assert_eq!(
            (
                bytes(&format!("models@{id}.ini")),
                bytes(&format!("launch@{id}.ini"))
            ),
            (Some(pair.0.clone()), Some(pair.1.clone())),
            "IP3c 5b: {id} kept its last good pair byte for byte"
        );
    }
    let failures: Vec<(String, String)> = m
        .render_failures()
        .iter()
        .map(|(id, e)| (id.clone(), format!("{e:?}")))
        .collect();
    for id in ["desk", "lab"] {
        assert!(
            failures
                .iter()
                .any(|(who, e)| who == id && e.contains("model-profile-device-outside")),
            "IP3c 5b: {id} runs the profile and its render stops on the card: \
             {failures:?}"
        );
    }
    assert_eq!(
        m.set_instance_profile_server(&alice, "gpu", "wide")
            .map(|a| a.running),
        Ok(both.clone()),
        "IP3b 5b: the switch answers the running instances that use the profile"
    );
    assert_eq!(
        server_profile_of(&pool(), "gpu"),
        "wide",
        "IP3b 5b: the profile's section names the new server profile"
    );
    assert_eq!(
        m.set_instance_profile_model(
            &alice,
            "gpu",
            "alpha",
            &vals(&[("ctx-size", "1024"), ("device-draft", "CUDA1")]),
            &revision()
        )
        .map(|a| a.running),
        Ok(both.clone()),
        "IP3b 5b: on `wide` the same entry lands"
    );

    // ── 5c. The switch back: the entry's card is outside the new cards ───
    // The switch is a save like any other: re-checking the entry against
    // `desk` fails the fit of every instance running the profile, so the
    // switch LANDS with the profile moved, every instance keeping the pair
    // the `wide` render left, and both reported not converged (IP3c
    // review 2: the fit checks decide per instance, a fit refusal never
    // refuses the save).
    let wide_pairs: Vec<(Vec<u8>, Vec<u8>)> = ["desk", "lab"]
        .iter()
        .map(|id| {
            (
                bytes(&format!("models@{id}.ini")).expect("rendered profiles"),
                bytes(&format!("launch@{id}.ini")).expect("rendered launch"),
            )
        })
        .collect();
    // Both restart onto the pair the `wide` render left (IP3c): their `applied`
    // record is the pair on disk now, so the switch back, which leaves that pair
    // alone, asks neither of them for a restart.
    for id in ["desk", "lab"] {
        mark_applied(id);
    }
    let answer = m
        .set_instance_profile_server(&alice, "gpu", "desk")
        .map(|a| a.running);
    assert_eq!(
        answer,
        Ok(Vec::<String>::new()),
        "IP3c 5c: a switch that takes a card an entry names away lands, moving \
         no running instance to a restart: {answer:?}"
    );
    assert_eq!(
        server_profile_of(&pool(), "gpu"),
        "desk",
        "IP3c 5c: the switch landed, the profile's section names the server \
         profile it switched to"
    );
    for (id, pair) in ["desk", "lab"].iter().zip(&wide_pairs) {
        assert_eq!(
            (
                bytes(&format!("models@{id}.ini")),
                bytes(&format!("launch@{id}.ini"))
            ),
            (Some(pair.0.clone()), Some(pair.1.clone())),
            "IP3c 5c: {id} kept the pair the `wide` render left, byte for byte"
        );
    }
    let failures: Vec<(String, String)> = m
        .render_failures()
        .iter()
        .map(|(id, e)| (id.clone(), format!("{e:?}")))
        .collect();
    for id in ["desk", "lab"] {
        assert!(
            failures
                .iter()
                .any(|(who, e)| who == id && e.contains("model-profile-device-outside")),
            "IP3c 5c: {id} is reported not converged with the device check: \
             {failures:?}"
        );
    }
    for id in ["desk", "lab"] {
        let section = render_section(&sys, id);
        assert!(
            section.contains("device-draft = CUDA1"),
            "IP3c 5c: the render of {id} still carries the entry that stands: \
             {section}"
        );
    }

    // ── 5d. The fix that moves the pair, and the no-op switch ────────────
    assert_eq!(
        m.set_instance_profile_model(
            &alice,
            "gpu",
            "alpha",
            &vals(&[("ctx-size", "1024")]),
            &revision()
        )
        .map(|a| a.running),
        Ok(both),
        "IP3b 5d: the entry without the card lands, and the pair it moves takes \
         the running instances to a restart"
    );
    // Both restart onto the pair that fix left (IP3c): the switch that follows
    // moves no pair, so it asks neither of them for a restart.
    for id in ["desk", "lab"] {
        mark_applied(id);
    }
    assert_eq!(
        m.set_instance_profile_server(&alice, "gpu", "desk")
            .map(|a| a.running),
        Ok(Vec::<String>::new()),
        "IP3c 5d: the profile already runs `desk`: a switch that moves no pair \
         answers nobody"
    );
    assert_eq!(
        server_profile_of(&pool(), "gpu"),
        "desk",
        "IP3b 5d: the profile's section names the server profile it switched to"
    );
    for id in ["desk", "lab"] {
        let section = render_section(&sys, id);
        assert!(
            !section.contains("device-draft"),
            "IP3b 5d: the render of {id} carries the entry as saved: {section}"
        );
    }
    assert_eq!(
        bytes(INSTANCES_FILE),
        records_before.0,
        "IP3b 5: no switch wrote the instance record"
    );
    assert_eq!(
        bytes(POOL),
        records_before.1,
        "IP3b 5: no switch wrote the model profile pool"
    );
    assert_eq!(
        bytes(ASSIGNMENTS),
        None,
        "IP3b 5: no switch created the retired assignment record"
    );
    assert!(
        m.render_failures().is_empty(),
        "IP3b 5: every render of the phase landed"
    );
}

/// E2E phase: IP3b 6. The memory check measures the ENTRY keys: an entry save
/// that makes the render of an instance that runs the profile over the node's
/// usable memory is a fit failure, not a refusal: the save lands, the instance
/// whose render stops keeps its last good pair and is reported with the
/// existing `model-profile-over-memory` key (IP3c review 2). The same save
/// that stays inside the cards lands as before. The card is sized from the
/// estimate read off the fixture's file, so no figure here restates the
/// arithmetic.
pub(crate) fn ip3b_entry_memory_e2e() {
    use pai_core::gguf_shape::{full_gpu_bytes, local_shape, GPU_RESERVE, MIB};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let shape = local_shape(Path::new(&path)).expect("IP3b 6: the fixture shape");
    let need = |ctx: u64| full_gpu_bytes(&shape, ctx, "f16", "f16");
    let mib_up = |bytes: u64| (bytes.div_ceil(MIB)).to_string();
    // A card that finds the entry at 1024 tokens and not at 131072.
    let card = need(1024) / MIB + GPU_RESERVE / MIB + 32;
    assert!(
        need(1024) <= usable(card) && need(131_072) > usable(card),
        "the card finds the entry at 1024 tokens and not at 131072"
    );

    let m = manager(&["desk"], card);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3b 6: the seeded install renders"
    );
    // `desk` runs the pair the fixture rendered (IP3c): the record of it is what
    // makes this phase's `running` list the drift of a pair.
    mark_applied("desk");
    let alice = Caller(":1.42".into());
    let bytes = |name: &str| std::fs::read(sys.join(name)).ok();
    let render_before = bytes("models@desk.ini");
    let revision = || {
        m.instance_profile(&alice, "gpu")
            .expect("IP3b 6: the read")
            .revision
    };

    // ── 6a. The entry that grows a render over the card ──────────────────
    // The fit is the instance's own, so the save LANDS: the entry is
    // written, `desk` keeps its last good pair byte for byte, and the
    // memory check is reported with its row.
    let answer = m
        .set_instance_profile_model(
            &alice,
            "gpu",
            "alpha",
            &vals(&[("ctx-size", "131072")]),
            &revision(),
        )
        .map(|a| a.running);
    assert_eq!(
        answer,
        Ok(Vec::<String>::new()),
        "IP3c 6a: the entry whose render stands over the card lands, moving no \
         running instance to a restart: {answer:?}"
    );
    let text = std::fs::read_to_string(sys.join(PROFILES)).expect(PROFILES);
    assert!(
        pai_core::ini::text::section_text(&text, "gpu/alpha")
            .unwrap_or_default()
            .contains("ctx-size = 131072"),
        "IP3c 6a: the save wrote the entry: {text}"
    );
    assert_eq!(
        bytes("models@desk.ini"),
        render_before,
        "IP3c 6a: the refused render left the pair on disk byte for byte"
    );
    let failures: Vec<(String, String)> = m
        .render_failures()
        .iter()
        .map(|(id, e)| (id.clone(), format!("{e:?}")))
        .collect();
    assert!(
        failures.iter().any(|(id, e)| id == "desk"
            && e.contains("model-profile-over-memory")
            && e.contains(&mib_up(need(131_072)))),
        "IP3c 6a: the instance over its card is reported with the memory check, \
         naming the bytes its render needs: {failures:?}"
    );

    // ── 6b. The entry that fits ──────────────────────────────────────────
    assert_eq!(
        m.set_instance_profile_model(
            &alice,
            "gpu",
            "alpha",
            &vals(&[("ctx-size", "2048")]),
            &revision()
        )
        .map(|a| a.running),
        Ok(vec!["desk".to_string()]),
        "IP3b 6b: a context the card finds lands"
    );
    let section = render_section(&sys, "desk");
    assert!(
        section.contains("ctx-size = 2048"),
        "IP3b 6b: the render carries the entry's context: {section}"
    );
}

/// E2E phase: IP3b 6b. The shrink rule of the same check: an entry whose render
/// stands over the cards, saved to a context the cards find, lands: the check
/// refuses a growth, never a fix. The render is made to stand by a manager on
/// big cards, the cards are taken away, and the save is judged on the small
/// ones.
pub(crate) fn ip3b_entry_memory_shrink_e2e() {
    use pai_core::gguf_shape::{full_gpu_bytes, local_shape, GPU_RESERVE, MIB};

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    let shape = local_shape(Path::new(&path)).expect("IP3b 6b: the fixture shape");
    let need = |ctx: u64| full_gpu_bytes(&shape, ctx, "f16", "f16");
    // The card finds 1024 tokens and not the trained context.
    let card = need(1024) / MIB + GPU_RESERVE / MIB + 32;
    assert!(
        need(262_144) > usable(card),
        "the context the render is grown to stands over the small card"
    );
    let alice = Caller(":1.42".into());

    // The render is grown on a node with room for it.
    let roomy = manager(&["desk"], BIG_CARD);
    roomy.start_up();
    // `desk` runs the pair the fixture rendered (IP3c): the record of it is what
    // makes this phase's `running` list the drift of a pair.
    mark_applied("desk");
    assert_eq!(
        roomy
            .set_instance_profile_model(
                &alice,
                "gpu",
                "alpha",
                &vals(&[("ctx-size", "262144")]),
                &roomy
                    .instance_profile(&alice, "gpu")
                    .expect("IP3b 6b: the read")
                    .revision
            )
            .map(|a| a.running),
        Ok(vec!["desk".to_string()]),
        "IP3b 6b: the grown entry lands on the node that has room"
    );
    let grown = std::fs::read_to_string(sys.join("models@desk.ini")).expect("render");
    assert!(
        grown.contains("ctx-size = 262144"),
        "IP3b 6b: the render that stands is over the small card"
    );
    // `desk` restarts on the grown pair (IP3c): its record names it, so the save
    // that shrinks it moves a real running instance to a restart.
    mark_applied("desk");

    // The card is taken away, and the save shrinks the render that stands.
    let tight = manager(&["desk"], card);
    tight.start_up();
    assert!(
        tight.render_failures().is_empty(),
        "IP3b 6b: a render that stands over the cards, and that no write touches, \
         is no failure of the start"
    );
    let answer = tight
        .set_instance_profile_model(
            &alice,
            "gpu",
            "alpha",
            &vals(&[("ctx-size", "1024")]),
            &tight
                .instance_profile(&alice, "gpu")
                .expect("IP3b 6b: the read")
                .revision,
        )
        .map(|a| a.running);
    assert_eq!(
        answer,
        Ok(vec!["desk".to_string()]),
        "IP3b 6b: a save that shrinks a render already over the card lands"
    );
    let section = render_section(&sys, "desk");
    assert!(
        section.contains("ctx-size = 1024"),
        "IP3b 6b: the render carries the shrunk context: {section}"
    );
}

/// E2E phase: IP3b 7. The retirement of the two pool files and of the
/// assignment record (`ARCHITECTURE.md` "Instance profiles", "What goes", and
/// `PLAN.md` step 17d and 17e): `models.ini` is the one pool a model profile
/// lives in and dies from, a profile an instance profile lists is not
/// deletable. The `assignments.ini` an install carries is retired by the start:
/// no verb reads it, and the phases pin none writes it back.
pub(crate) fn ip3b_pool_is_one_file_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    let path = seed(dir.path(), &sys, "ctx-size = 1024\n");
    // The one pool: `beta`, which the seed keeps switched off, joins
    // `models.ini`, and the off file is gone as the migration leaves it.
    std::fs::write(
        sys.join(POOL),
        format!(
            "[*]\ntop-p = 0.9\n\n[alpha]\nmodel = {path}\ntemp = 0.7\n\n[beta]\nmodel = {path}\ntemp = 0.3\n"
        ),
    )
    .expect(POOL);
    std::fs::remove_file(sys.join(OFF_POOL)).expect("the off pool is gone");
    // The record of an install that reads it and will never write it again.
    let legacy = "[desk]\nbeta = off\n[lab]\nbeta = off\n";
    std::fs::write(sys.join(ASSIGNMENTS), legacy).expect(ASSIGNMENTS);
    let m = manager(&["desk"], BIG_CARD);
    m.start_up();
    assert!(
        m.render_failures().is_empty(),
        "IP3b 7: the seeded install renders"
    );

    let alice = Caller(":1.42".into());
    let read = |name: &str| std::fs::read_to_string(sys.join(name)).expect(name);
    let bytes = |name: &str| std::fs::read(sys.join(name)).ok();
    let pool_before = bytes(POOL);
    let renders_before = (bytes("models@desk.ini"), bytes("models@lab.ini"));

    // ── 7a. A profile an instance profile lists is not deletable ─────────
    let answer = m.delete_model_profile(&alice, "alpha");
    assert!(
        refused_naming(&answer, "alpha"),
        "IP3b 7a: a profile the instance profile `gpu` lists in its entry \
         `[gpu/alpha]` is refused, naming it: {answer:?}"
    );
    assert_eq!(
        bytes(POOL),
        pool_before,
        "IP3b 7a: the refusal wrote the pool"
    );

    // ── 7b. A profile no instance profile lists dies from the one pool ───
    assert_eq!(
        m.delete_model_profile(&alice, "beta").map(|_| ()),
        Ok(()),
        "IP3b 7b: a profile no entry lists is deleted from `models.ini`"
    );
    let pool = read(POOL);
    assert!(
        pai_core::ini::text::section_text(&pool, "beta").is_none(),
        "IP3b 7b: its section is out of the pool"
    );
    assert!(
        pai_core::ini::text::section_text(&pool, "alpha").is_some(),
        "IP3b 7b: the pool keeps every other section"
    );
    assert_eq!(
        bytes(OFF_POOL),
        None,
        "IP3b 7b: the off pool the migration removed is never written again"
    );
    assert_eq!(
        bytes(ASSIGNMENTS),
        None,
        "IP3b 7b: the assignment record the install carries is retired: no verb reads it, \
         and the delete wrote it back not"
    );
    assert_eq!(
        bytes(&format!("{ASSIGNMENTS}.2.99.0")),
        Some(legacy.as_bytes().to_vec()),
        "IP3b 7b: the retired record's bytes stand in the migration's backup, exactly as \
         the install wrote them"
    );
    assert_eq!(
        (bytes("models@desk.ini"), bytes("models@lab.ini")),
        renders_before,
        "IP3b 7b: a profile no entry lists is in no render, so its delete \
         renders nothing"
    );
}
