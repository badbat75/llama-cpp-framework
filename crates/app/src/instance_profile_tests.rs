use super::fake::{self, FakeClient};
use super::{ConfigFile, InstanceState};
use pai_core::client::ClientError;

/// The manager `default` runs the server profile `big`, whose file holds
/// `Port = 8099`.
fn manager_with_big() -> (fake::Log, FakeClient) {
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive)
        .with_instance("default", "big", 8080)
        .with_file("server@big.ini", "[Server]\nPort = 8099\n");
    (log, c)
}

/// The file names the fake was asked to read or write, in order: the
/// names the seam chose.
fn names_asked(log: &fake::Log) -> Vec<String> {
    log.lock()
        .unwrap()
        .iter()
        .filter_map(|e| {
            e.strip_prefix("read_file ")
                .or_else(|| e.strip_prefix("write_file "))
                .map(str::to_string)
        })
        .collect()
}

#[test]
fn the_server_read_is_the_profile_default_names() {
    let (log, c) = manager_with_big();
    let b = fake::backend(c);
    assert_eq!(
        b.read_config(ConfigFile::Server),
        Ok(Some("[Server]\nPort = 8099\n".to_string())),
        "the profile's file, not an empty read"
    );
    assert_eq!(
        names_asked(&log),
        ["server@big.ini"],
        "no call names server@default.ini"
    );
}

#[test]
fn the_server_save_is_the_profile_default_names() {
    let (log, c) = manager_with_big();
    let files = c.files.clone();
    let b = fake::backend(c);
    b.write_config(ConfigFile::Server, "[Server]\nPort = 8080\n")
        .expect("the write goes through");
    assert_eq!(
        fake::writes(&log),
        ["write_file server@big.ini"],
        "no call names server@default.ini"
    );
    assert_eq!(
        files
            .lock()
            .unwrap()
            .get("server@big.ini")
            .map(String::as_str),
        Some("[Server]\nPort = 8080\n")
    );
    assert!(
        !files.lock().unwrap().contains_key("server@default.ini"),
        "the file the instance does not use stays untouched"
    );
}

/// The path the user is shown is the file the seam reads.
#[test]
fn the_path_shown_is_the_profile_file() {
    let (_log, c) = manager_with_big();
    let b = fake::backend(c);
    let p = b.config_path(ConfigFile::Server);
    assert_eq!(
        p.file_name().and_then(std::ffi::OsStr::to_str),
        Some("server@big.ini"),
        "{p:?}"
    );
}

#[test]
fn an_instances_that_fails_reads_and_writes_nothing() {
    let (log, mut c) = manager_with_big();
    c.fail_instances = Some(ClientError::Unavailable);
    let c = c.with_file("server@default.ini", "[Server]\nPort = 8080\n");
    let files = c.files.clone();
    let b = fake::backend(c);
    assert_eq!(
        b.read_config(ConfigFile::Server),
        Err(ClientError::Unavailable.text()),
        "the manager's own error, no fall back"
    );
    assert_eq!(
        b.write_config(ConfigFile::Server, "[Server]\nPort = 8080\n"),
        Err(ClientError::Unavailable.text())
    );
    assert!(
        names_asked(&log).is_empty(),
        "the file is never asked: {:?}",
        names_asked(&log)
    );
    let files = files.lock().unwrap();
    assert_eq!(
        files.get("server@big.ini").map(String::as_str),
        Some("[Server]\nPort = 8099\n"),
        "nothing written"
    );
    assert_eq!(
        files.get("server@default.ini").map(String::as_str),
        Some("[Server]\nPort = 8080\n"),
        "never a fall back to the file of the id"
    );
}

/// `default` is not in the list: the same refusal, nothing read, nothing
/// written.
#[test]
fn an_absent_default_is_no_instance() {
    let (log, c) = manager_with_big();
    let c = c.with_instance("coder", "coder", 8081);
    let b = fake::backend(c);
    assert_eq!(
        b.read_config(ConfigFile::Server),
        Err(ClientError::NoInstance.text())
    );
    assert_eq!(
        b.write_config(ConfigFile::Server, "[Server]\nPort = 8080\n"),
        Err(ClientError::NoInstance.text())
    );
    assert!(
        names_asked(&log).is_empty(),
        "no file of any name is asked: {:?}",
        names_asked(&log)
    );
}

// ── IP7a: the download bug. Under the manager the server profile is read
// through the instance's record, and a failed read is an error, never the
// defaults (`server_cfg::load` swallows every refusal today).

/// IP7a 1 (day-one guard): the ModelsDir of the server profile the instance's
/// record names is what pai-app's resolver answers, the target the download
/// and the file delete follow (the CLI resolves both through it, pinned in
/// cli_tests' `the_cli_resolves_models_dir_for_the_backend_and_os`).
#[test]
fn ip7a1_the_models_dir_of_the_instance_server_file_is_the_target() {
    let (_log, c) = manager_with_big();
    let c = c.with_file("server@big.ini", "[Server]\nModelsDir = /x\n");
    let got = fake::scoped(fake::backend(c), crate::server_cfg::models_dir);
    assert_eq!(
        got, "/x",
        "the ModelsDir set in the profile the record names is the target"
    );
}

/// IP7a 3 (day-one guard): an unset ModelsDir under the manager answers the
/// instance default, unchanged from today. `models_dir()` and
/// `paths::instance_models_dir()` are read in the same process (no redirect:
/// the phase children redirect in their own processes), so the pair is
/// machine free.
#[test]
fn ip7a3_an_unset_models_dir_answers_the_instance_folder() {
    let (_log, c) = manager_with_big(); // server@big.ini sets no ModelsDir
    let want = crate::paths::instance_models_dir();
    let got = fake::scoped(fake::backend(c), crate::server_cfg::models_dir);
    assert_eq!(
        got,
        want.to_string_lossy(),
        "an unset ModelsDir is the instance's folder, not the user's"
    );
}

/// IP7a 2: a refused read of the instance's server file is an error that
/// names the refusal, never the defaults. The refusal's end-to-end
/// behaviour (exit non-zero, nothing fetched, nothing written) is pinned in
/// the CLI, where the exit code is decided.
#[test]
fn ip7a2_a_a_refused_server_profile_read_is_an_error_naming_the_refusal() {
    let (_log, c) = manager_with_big();
    let mut c = c.with_file("server@big.ini", "[Server]\nModelsDir = /x\n");
    c.fail_read = Some(ClientError::NotAuthorized);
    let got = fake::scoped(fake::backend(c), crate::server_cfg::load);
    let e = got.expect_err("a refused read is an error, not the defaults");
    assert!(
        e.contains(&ClientError::NotAuthorized.text()),
        "the error names the manager's refusal: {e}"
    );
}

// ── IP7c: the download writes the stay keys only. Under the manager the
// model profile goes in through CreateModelProfile's rules (no moved key),
// and `--instance-profile <p>` adds the entry [p/<id>] and saves the fit's
// placement into it. The seam is `hf::save_downloaded_profile`.

/// A download-derived profile as `model_profile_from_download` leaves it:
/// the model file (written into `dir`), one stay key, and the fit's
/// placement filled in.
fn ip7c_profile(dir: &std::path::Path) -> crate::model_profiles::ModelProfile {
    let model = dir.join("ip7c.gguf");
    std::fs::write(&model, b"gguf").expect("model file");
    crate::model_profiles::ModelProfile {
        id: "ip7c-m".into(),
        model: model.to_string_lossy().into_owned(),
        temp: Some(0.6),
        device: "CUDA0,CUDA1".into(),
        tensor_split: "26,23".into(),
        ctx_size: Some(4096),
        n_gpu_layers: Some(99),
        n_cpu_moe: Some(5),
        placement_note: "estimated at download for a 4k context".into(),
        ..Default::default()
    }
}

/// The fit's placement keys and values, as the entry must receive them.
const IP7C_FIT: [(&str, &str); 5] = [
    ("device", "CUDA0,CUDA1"),
    ("tensor-split", "26,23"),
    ("n-gpu-layers", "99"),
    ("n-cpu-moe", "5"),
    ("ctx-size", "4096"),
];

/// IP7c 1 (red): no `models.ini` write carries a moved key. Everything that
/// reached the pool is scanned: the text the fake was asked to write into
/// `models.ini`, and the keys of every `CreateModelProfile` call. Today the
/// download's save writes the fit's placement into the pool.
#[test]
fn ip7c1_no_models_ini_write_carries_a_moved_key() {
    let (_log, c) = manager_with_big();
    let files = c.files.clone();
    let created = c.created.clone();
    let b = fake::backend(c);
    let dir = tempfile::tempdir().expect("tempdir");
    let p = ip7c_profile(dir.path());
    fake::scoped(b, || {
        crate::hf::save_downloaded_profile(&p, dir.path(), None)
    })
    .expect("the model profile is created");
    let mut moved: Vec<String> = Vec::new();
    if let Some(text) = files.lock().unwrap().get("models.ini") {
        for line in text.lines() {
            let key = line.split('=').next().unwrap_or("").trim().to_string();
            if pai_core::checks::PLACEMENT_KEYS.contains(&key.as_str()) {
                moved.push(key);
            }
        }
    }
    for (_id, _from, values) in created.lock().unwrap().iter() {
        for (key, _value) in values {
            if pai_core::checks::PLACEMENT_KEYS.contains(&key.as_str()) {
                moved.push(key.clone());
            }
        }
    }
    assert!(
        moved.is_empty(),
        "the pool carries the moved keys {moved:?}"
    );
}

/// IP7c 2 (red): with the option, `add_instance_profile_model` gives the
/// profile the model and `set_instance_profile_model` saves the fit's keys
/// into the entry.
#[test]
fn ip7c2_the_entry_holds_the_fits_keys() {
    let (log, c) = manager_with_big();
    let c = c.with_profile("desk", "big", "rev-1");
    let entries_set = c.entries_set.clone();
    let b = fake::backend(c);
    let dir = tempfile::tempdir().expect("tempdir");
    let p = ip7c_profile(dir.path());
    let r = fake::scoped(b, || {
        crate::hf::save_downloaded_profile(&p, dir.path(), Some("desk"))
    });
    assert!(matches!(r, Ok(None)), "the entry save goes through: {r:?}");
    let acts = fake::actions(&log);
    assert!(
        acts.contains(&"add_instance_profile_model desk ip7c-m".to_string()),
        "the add is asked: {acts:?}"
    );
    let sets = entries_set.lock().unwrap();
    let values = sets
        .iter()
        .find(|(profile, model, _)| profile == "desk" && model == "ip7c-m")
        .map(|(_, _, values)| values.clone())
        .expect("the entry save is asked");
    for (key, value) in IP7C_FIT {
        assert!(
            values.iter().any(|(k, v)| k == key && v == value),
            "the entry must hold {key} = {value}: {values:?}"
        );
    }
}

/// IP7c 3 (red): a refusal of the entry leaves the model profile created
/// (created first, never taken back) and travels out so the caller says so.
#[test]
fn ip7c3_a_refused_entry_leaves_the_profile_created_and_says_so() {
    let (log, c) = manager_with_big();
    let mut c = c.with_profile("desk", "big", "rev-1");
    c.fail_add_model = Some(ClientError::NoInstanceProfile("desk".into()));
    let created = c.created.clone();
    let b = fake::backend(c);
    let dir = tempfile::tempdir().expect("tempdir");
    let p = ip7c_profile(dir.path());
    let r = fake::scoped(b, || {
        crate::hf::save_downloaded_profile(&p, dir.path(), Some("desk"))
    });
    let refusal = r
        .expect("the model profile is created: a refused entry ends no command")
        .expect("the entry refusal travels out for the caller to say");
    assert!(
        refusal
            .text()
            .contains(&ClientError::NoInstanceProfile("desk".into()).text()),
        "the refusal names the manager's refusal: {refusal:?}"
    );
    assert!(
        created
            .lock()
            .unwrap()
            .iter()
            .any(|(id, _from, _values)| id == "ip7c-m"),
        "the model profile stays created"
    );
    let acts = fake::actions(&log);
    let create = acts.iter().position(|e| e == "create_model_profile ip7c-m");
    let add = acts
        .iter()
        .position(|e| e == "add_instance_profile_model desk ip7c-m");
    assert!(
        matches!((&create, &add), (Some(c), Some(a)) if c < a),
        "the create precedes the add: {acts:?}"
    );
}

/// The model profiles files keep their names: the profile of an instance
/// says where the server file is, nothing else.
#[test]
fn the_model_profiles_files_keep_their_names() {
    let (log, c) = manager_with_big();
    let c = c.with_file("models.ini", "[Qwen3.8-27B]\nmodel = /m/q.gguf\n");
    let b = fake::backend(c);
    assert_eq!(
        b.read_config(ConfigFile::Models),
        Ok(Some("[Qwen3.8-27B]\nmodel = /m/q.gguf\n".to_string()))
    );
    assert_eq!(b.read_config(ConfigFile::ModelsDisabled), Ok(None));
    assert_eq!(names_asked(&log), ["models.ini", "models.ini.disabled"]);
}
