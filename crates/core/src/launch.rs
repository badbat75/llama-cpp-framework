//! The llama-server launch, shared by the configurator's `runstate::start()` and
//! `pai-manager run-instance` (`instance_command`): `server_args()` is
//! the single source of truth for the arg list, `env_vars()` the settings
//! llama.cpp exposes only as env vars, `launch_command()` the process with its
//! cwd, `LLAMA_CACHE` and the ROCm PATH prepend (`proc::prepend_rocm_path`:
//! HIP devices vanish without it). Process control and the log stay in the
//! configurator (`runstate.rs`).

/// `true` if the model profiles file at `path` has at least one model profile section. The
/// render's global `[*]` holds defaults for every model profile, not a model profile of its
/// own, so a file with only that section loads nothing.
pub fn has_model_profiles_in(path: &std::path::Path) -> bool {
    crate::ini::read_all(path).iter().any(|s| s.id != "*")
}

// ── Launch argument assembly (server_args) ───────────────────────────────

/// The full llama-server argument list derived from server.ini.
/// Single source of truth for both `start()` and `command_line()`.
pub fn server_args(
    cfg: &crate::server_cfg::ServerConfig,
    models_path: &std::path::Path,
) -> Vec<String> {
    let hostname = cfg.hostname_or_default();

    let mut args: Vec<String> = vec![
        "--models-preset".into(),
        models_path.to_string_lossy().into_owned(),
    ];

    // --port / --models-max are omitted when unset (the UI's "default" checkbox
    // stores None). Forcing a value here is exactly what made "default" still
    // emit `--port` / `--models-max 1`; omitting the flag lets llama.cpp apply
    // its own default (port `server_cfg::DEFAULT_PORT`; 4 resident models), which
    // is why that constant must track the bundled release. A 0 port is never
    // valid so it counts as unset, but models-max 0 (= unlimited) IS a real
    // value and is passed through.
    if let Some(p) = cfg.port.filter(|&n| n > 0) {
        args.push("--port".into());
        args.push(p.to_string());
    }
    if let Some(mm) = cfg.models_max {
        args.push("--models-max".into());
        args.push(mm.to_string());
    }
    // Same omit-when-unset rule; an explicit 0 (off, llama.cpp's default) is
    // passed as written.
    if let Some(mm) = cfg.models_memory_margin {
        args.push("--models-memory-margin".into());
        args.push(mm.to_string());
    }

    args.push("--host".into());
    args.push(hostname);

    // --no-ui : no built-in chat UI. Only the OFF state is spelled out: on is
    //   llama.cpp's default, so on and unset both pass nothing. It belongs on
    //   THIS line because the router serves the page itself
    //   (`server_http_context::init` on the router's params, server.cpp main);
    //   the children inherit it through `base_preset`, harmlessly. The API
    //   routes are registered regardless, so every client but the browser page
    //   is unaffected.
    let ui = cfg.webui_or_default();
    if !ui {
        args.push("--no-ui".into());
    }

    // --webui-mcp-proxy : serve the built-in web UI's MCP proxy endpoint. A
    //   presence flag, omitted when off (llama.cpp then defaults it disabled),
    //   and omitted with the UI off: upstream registers `/cors-proxy` whatever
    //   `--no-ui` says, and nothing but the UI calls it, so the stored choice
    //   would only open an experimental endpoint for nobody.
    if ui && cfg.webui_mcp_proxy_or_default() {
        args.push("--webui-mcp-proxy".into());
    }

    // -lv N : log verbosity threshold into the captured llama-server.log.
    //   Framework default 4 (per-request logging) when unset; always passed
    //   (the Server tab's Advanced card exposes the level).
    args.push("-lv".into());
    args.push(cfg.log_verbosity_or_default().to_string());

    // CPU thread counts: when unset ("auto") omit the flag entirely so llama.cpp
    // applies its own default; only pass an explicit value when the user set one.
    // (We must NOT substitute a computed default here; that would defeat "auto".)
    if let Some(t) = cfg.threads.filter(|&n| n > 0) {
        args.push("-t".into());
        args.push(t.to_string());
    }
    if let Some(tb) = cfg.threads_batch.filter(|&n| n > 0) {
        args.push("--threads-batch".into());
        args.push(tb.to_string());
    }

    // --device LIST : the GPUs of this server profile, one argument with the
    //   commas kept (the router splits the list itself). Omitted when unset,
    //   which is every GPU of the node, as before the key existed; `none` is
    //   passed as the value `none` (CPU only). It is the ROUTER's flag on
    //   purpose: the router overlays its own argv on every model profile
    //   (`preset.merge(base_preset)`), so this wins over each profile's
    //   `device`, and the placement (llama.cpp's `--fit`) is derived over
    //   exactly these devices. An id the node has no GPU for throws
    //   `invalid device` at the router's start, so the instance fails closed.
    if let Some(d) = crate::server_cfg::opt_nonblank(cfg.devices.clone()) {
        args.push("--device".into());
        args.push(d);
    }

    // NO --tensor-split, --split-mode, --override-tensor, -lm, -fit
    //   (nor its --fit-ctx / --fit-target), --prefill-assistant, --cache-reuse,
    //   --ctx-checkpoints, --checkpoint-min-step, --kv-unified,
    //   --kv-unified-per-slot, --moe-cache-mib, --repack, --no-host, --spec-draft-n-ubatch
    //   or --mmproj-device here: those are per-model-profile keys (the GPU
    //   distribution, the tensor placement, load-mode, fit, prefill-assistant,
    //   the prompt cache, the KV layout, the expert cache and repacking, the
    //   drafter, mmproj-device). The router copies its own args over every model profile (`preset.merge(base_preset)`), so any of them on this
    //   line would override each model's own value.
    // --slot-save-path DIR : turns on the /slots save|restore endpoints, which is
    //   what `slot_state` calls around a stop and a start. Emitted only when the
    //   feature is on, because the flag is not free: llama.cpp validates the
    //   directory while PARSING it and throws "not a directory" on a missing one,
    //   so an always-on flag would turn a deleted folder into a server that
    //   refuses to start. `start()` creates the directory first for the same
    //   reason.
    //
    //   It reaches the per-model CHILDREN even though it is the ROUTER's flag,
    //   and by an unusual route: the option has no env name, so it cannot ride a
    //   model profile key, but `get_map_key_opt` (common/preset.cpp) also maps every
    //   option by its flag name with the dashes stripped. The router therefore
    //   parses its own argv into `base_preset` and merges that onto every model
    //   (`preset.merge(base_preset)`), and `unset_reserved_args` does not prune
    //   it. See the slot_state module header.
    if cfg.save_state_on_shutdown_or_default() {
        args.push("--slot-save-path".into());
        args.push(cfg.state_dir_or_default());
    }
    args
}

/// The environment llama-server is launched with, beyond the inherited one and
/// the ROCm PATH prepend (`proc::prepend_rocm_path`). LLAMA_CACHE is set
/// separately in `start()`: it comes from a path helper, not from a bare config
/// field. This is the sibling of `server_args` for the settings llama.cpp exposes
/// ONLY as env vars, so `start()` and `command_line()` render the same launch.
pub fn env_vars(cfg: &crate::server_cfg::ServerConfig) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();
    // ROCBLAS_USE_HIPBLASLT: rocBLAS's switch between its two GEMM backends. Read
    // by rocBLAS itself (llama.cpp has no flag for it and never sees it) and
    // parsed as an INT, so the value is "1"/"0", never "true"/"false". `0` forces
    // Tensile, which is the cure for gfx1201's hipBLASLt failing the second
    // 16-bit GEMM of a process (see server_cfg::rocblas_use_hipblaslt). Unset
    // exports nothing at all, which is a third state and the default.
    if let Some(on) = cfg.rocblas_use_hipblaslt {
        env.push((
            "ROCBLAS_USE_HIPBLASLT".into(),
            if on { "1" } else { "0" }.into(),
        ));
    }
    // HSA_USERPTR_FOR_PAGED_MEM / HSA_SVM_FOR_PAGED_MEM: with both at `0` the
    // ROCm runtime (libhsakmt) maps paged host memory through the driver's GTT,
    // so compaction and reclaim stop evicting this process's GPU queues (see
    // server_cfg::rocm_host_memory_gtt and `ARCHITECTURE.md` "ROCm host memory
    // as GTT"). Unset is the on state, so the pair goes out unless the profile
    // says `false`, and to every backend: only ROCm reads these two, the others
    // ignore an env they never ask for.
    if cfg.rocm_host_memory_gtt != Some(false) {
        env.push(("HSA_USERPTR_FOR_PAGED_MEM".into(), "0".into()));
        env.push(("HSA_SVM_FOR_PAGED_MEM".into(), "0".into()));
    }
    // GGML_CUDA_ALLREDUCE: the AllReduce of `--split-mode tensor`, read by
    // ggml-cuda and ggml-hip alike (one source, one name). Normalized here as
    // well as on read: the backend runs an unknown word as `none` behind a
    // warning (see server_cfg::cuda_allreduce). Unset exports nothing.
    if let Some(mode) = cfg
        .cuda_allreduce
        .as_deref()
        .and_then(crate::server_cfg::normalize_allreduce)
    {
        env.push(("GGML_CUDA_ALLREDUCE".into(), mode.into()));
    }
    // GGML_CUDA_DISABLE_GRAPHS: ggml tests PRESENCE, not value, so off must
    // export nothing (an `=0` would disable graphs all the same).
    if cfg.cuda_disable_graphs {
        env.push(("GGML_CUDA_DISABLE_GRAPHS".into(), "1".into()));
    }
    // LLAMA_ATTN_ROT_DISABLE: read by llama.cpp with `atoi`.
    if cfg.attn_rot_disable {
        env.push(("LLAMA_ATTN_ROT_DISABLE".into(), "1".into()));
    }
    // ROCP_TOOL_ATTACH: what rocprofiler-sdk requires of a process before
    // `rocprofv3 --attach` may trace it. Off exports nothing.
    if cfg.rocprof_attach {
        env.push(("ROCP_TOOL_ATTACH".into(), "1".into()));
    }
    env
}

// ── Launch command ──────────────────────────────────────────────────────

/// The llama-server process of `cfg` and `models_path`, before its stdio:
/// args, cwd and env, shared by `start()` and `instance_command()`.
pub fn launch_command(
    exe: &std::path::Path,
    cfg: &crate::server_cfg::ServerConfig,
    models_path: &std::path::Path,
) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(server_args(cfg, models_path));

    cmd.current_dir(crate::paths::data_root());
    cmd.env("LLAMA_CACHE", cfg.models_dir_or_default());
    // Settings llama.cpp exposes only as env vars; inherited by the router's
    // per-model children, so they cover every model profile.
    for dev in env_vars(cfg) {
        cmd.env(dev.0, dev.1);
    }
    // Make ggml-hip.dll loadable: the HIP SDK's bin dir isn't on the system
    // PATH, and ggml silently skips a backend whose DLL deps don't resolve.
    crate::proc::prepend_rocm_path(&mut cmd);
    cmd
}

// ── Service launch (pai-manager run-instance) ─────────────────────

/// Why `instance_command` built no launch.
#[derive(Debug, PartialEq, Eq)]
pub enum LaunchError {
    /// The id is not a valid instance id (`paths::valid_instance_id`).
    InvalidId,
    /// The record of the instances (`instances.ini`) has no section for the
    /// instance, or its section names no server profile; the path is the
    /// record.
    NoConfig(std::path::PathBuf),
    /// The instance's `models@<id>.ini` has no model profile section
    /// (`start()`'s `has_model_profiles`).
    NoModelProfiles,
    /// SaveStateOnShutdown is on and this slot-state directory could not be
    /// created, for this reason.
    StateDir(std::path::PathBuf, std::io::ErrorKind),
}

/// The llama-server launch of instance `instance`: the section `instance` of
/// `paths::instances_ini()` names the server profile in its `ServerProfile`
/// key, and `exe` is launched with `server_args()` of that profile's file
/// `server@<profile>.ini`; the model profiles stay the render of the instance
/// id, `models@<id>.ini`, with the cwd and env `start()` sets. Its slot-state
/// folder, when its server profile leaves `StateDir` unset, is the folder of
/// the instance below the engine's state folder, so one instance's snapshots
/// never sit in another's. No log, no rotation: the service manager captures
/// the output.
pub fn instance_command(
    instance: &str,
    exe: &std::path::Path,
) -> Result<std::process::Command, LaunchError> {
    if !crate::paths::valid_instance_id(instance) {
        return Err(LaunchError::InvalidId);
    }
    // The record of the instances, and the server profile its section names:
    // a record with no section, a section with no key, a key with an empty
    // value, a name that builds no file: no profile to read, and the refusal
    // names the record.
    let record = crate::paths::instances_ini();
    let profile = crate::paths::instance_server_profile(instance)
        .ok_or_else(|| LaunchError::NoConfig(record.clone()))?;
    // The profile's file.
    let ini = crate::paths::instance_ini(&profile)
        .ok_or_else(|| LaunchError::NoConfig(record.clone()))?;
    if !ini.is_file() {
        return Err(LaunchError::NoConfig(ini));
    }
    let models_path = crate::paths::instance_models(instance).ok_or(LaunchError::InvalidId)?;
    if !has_model_profiles_in(&models_path) {
        return Err(LaunchError::NoModelProfiles);
    }
    // The instance's own snapshot folder, below the engine's state root: it is
    // the default `with_instance_defaults` fills in when the profile leaves
    // `StateDir` unset, and `ensure_dir` below creates it before the spawn.
    let cfg = with_instance_defaults(
        crate::server_cfg::load_from(&ini),
        cfg!(any(windows, target_os = "linux")),
        &crate::paths::instance_models_dir(),
        &crate::paths::instance_state_dir().join(instance),
    );
    // Before the spawn, as in `start()`: llama.cpp refuses a missing
    // `--slot-save-path` directory during argument parsing.
    if cfg.save_state_on_shutdown_or_default() {
        crate::slot_state::ensure_dir(&cfg)
            .map_err(|e| LaunchError::StateDir(crate::slot_state::state_dir(&cfg), e.kind()))?;
    }
    let mut cmd = launch_command(exe, &cfg, &models_path);
    cmd.current_dir(instance_current_dir(
        cfg!(windows),
        &crate::paths::data_root(),
        &crate::paths::instance_state_dir(),
    ));
    Ok(cmd)
}

/// The defaults of an instance the manager launches, filled into `cfg`
/// where `server@<id>.ini` leaves ModelsDir or StateDir unset or blank:
/// under the manager (`under_manager`: Windows and Linux) the engine's
/// folders `models` and `state` (`paths::instance_models_dir`,
/// `paths::instance_state_dir`: under `%ProgramData%\llama.cpp` on Windows,
/// the service user's on Linux), the ones the configurator names as the
/// instance's defaults, not the launching account's own; otherwise `cfg`
/// unchanged.
pub fn with_instance_defaults(
    cfg: crate::server_cfg::ServerConfig,
    under_manager: bool,
    models: &std::path::Path,
    state: &std::path::Path,
) -> crate::server_cfg::ServerConfig {
    if !under_manager {
        return cfg;
    }
    let unset = |v: &Option<String>| v.as_deref().is_none_or(|s| s.trim().is_empty());
    let mut cfg = cfg;
    if unset(&cfg.models_dir) {
        cfg.models_dir = Some(models.to_string_lossy().into_owned());
    }
    if unset(&cfg.state_dir) {
        cfg.state_dir = Some(state.to_string_lossy().into_owned());
    }
    cfg
}

/// The working directory of an instance the manager launches: on Windows
/// (`windows`) the engine's `state` directory (`paths::instance_state_dir`,
/// which llama.cpp's MSI creates writable by the instances' accounts), not
/// `paths::data_root()` of the instance's virtual account, a system profile;
/// elsewhere `data_root`, unchanged.
pub fn instance_current_dir(
    windows: bool,
    data_root: &std::path::Path,
    state: &std::path::Path,
) -> std::path::PathBuf {
    if windows { state } else { data_root }.to_path_buf()
}

/// E2E phase for `instance_command`, for this crate's e2e test to call: it sets
/// the process-wide `PRIVATE_AI_DATA_ROOT` redirect (`src/tests/mod.rs`).
/// The expected launch is always computed by the existing path
/// (`server_cfg::load()` of `paths::server_ini()` fed to `server_args`), never
/// hard-coded.
#[cfg(test)]
pub(crate) fn instance_launch_e2e() {
    use std::collections::BTreeMap;
    use std::ffi::OsString;

    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    // The instance files are the engine's (`engine_config_dir`), not the
    // framework's `system_config_dir`, which holds only its own (bff.ini).
    let sys = crate::paths::engine_config_dir();
    let framework = crate::paths::system_config_dir();
    let exe = dir
        .path()
        .join("bin")
        .join(crate::paths::binary_name("llama-server"));
    let envs_of = |cmd: &std::process::Command| -> BTreeMap<OsString, Option<OsString>> {
        cmd.get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(|v| v.to_owned())))
            .collect()
    };
    let port_of = |cmd: &std::process::Command| -> Option<String> {
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let i = args.iter().position(|a| a == "--port")?;
        args.get(i + 1).cloned()
    };

    // ── Refusals ─────────────────────────────────────────────────────────
    // A crafted or malformed id is refused before any path is built.
    for bad in ["", "../x", "..", "a/b", "Default", "a_b", "-a", "x.ini"] {
        assert_eq!(
            instance_command(bad, &exe).err(),
            Some(LaunchError::InvalidId),
            "id {bad:?}"
        );
    }
    // A valid id with no file anywhere: no system directory at all, so no
    // `instances.ini` to name the instance's server profile. A launch that
    // cannot name its profile is refused, and the refusal names the record.
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoConfig(sys.join("instances.ini"))),
        "no instances.ini at all: the refusal names the record"
    );
    assert!(
        !sys.exists(),
        "a refused launch must not create the directory"
    );
    // The directory exists and holds another instance, not this one.
    std::fs::create_dir_all(&sys).expect("system config dir");
    // The record exists and has a section for `lan-2`, none for `default`: no
    // section names no profile, so the refusal is the record, not a
    // `server@default.ini` built from the id.
    std::fs::write(
        sys.join("instances.ini"),
        "[lan-2]\nServerProfile = lan-2\n",
    )
    .expect("instances.ini");
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoConfig(sys.join("instances.ini"))),
        "the record has no section for the instance"
    );
    // A section of this instance that names no profile: no `ServerProfile` key,
    // a key beside other keys, a key with an empty value, a key with a value of
    // spaces only. The launch reads `server@<ServerProfile>.ini` and has no
    // profile to read, so every one of them is the record.
    for record in [
        "[default]\n",
        "[default]\nPort = 8080\n",
        "[default]\nServerProfile = \n",
        "[default]\nServerProfile =   \n",
    ] {
        std::fs::write(sys.join("instances.ini"), record).expect("instances.ini");
        assert_eq!(
            instance_command("default", &exe).err(),
            Some(LaunchError::NoConfig(sys.join("instances.ini"))),
            "section {record:?} names no profile"
        );
    }
    // An instance is the section of `instances.ini`; its `ServerProfile` is the
    // file the launch reads.
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = lan-2\n",
    )
    .expect("instances.ini");
    std::fs::write(sys.join("server@lan-2.ini"), "[Server]\nPort = 8090\n").expect("lan-2");
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoConfig(sys.join("server@default.ini"))),
        "another instance's file is not this one's"
    );
    // The per-user server.ini does not stand in for a missing instance file.
    let user_ini = crate::paths::server_ini();
    std::fs::create_dir_all(user_ini.parent().expect("config dir")).expect("user config dir");
    std::fs::write(&user_ini, "[Server]\nPort = 9999\n").expect("server.ini");
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoConfig(sys.join("server@default.ini"))),
        "server.ini is not server@default.ini"
    );
    // Nor does one left in the framework's directory: nothing reads it there,
    // and the framework's `instances.ini` is not the file the launch reads.
    std::fs::create_dir_all(&framework).expect("framework config dir");
    std::fs::write(
        framework.join("server@default.ini"),
        "[Server]\nPort = 8082\n",
    )
    .expect("stray server@default.ini");
    std::fs::write(
        framework.join("models.ini"),
        "[stray]\nmodel = /m/stray.gguf\n",
    )
    .expect("stray models.ini");
    std::fs::write(
        framework.join("instances.ini"),
        "[default]\nServerProfile = stray\n",
    )
    .expect("stray instances.ini");
    std::fs::write(
        framework.join("server@stray.ini"),
        "[Server]\nPort = 8084\n",
    )
    .expect("stray server@stray.ini");
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoConfig(sys.join("server@default.ini"))),
        "the framework's directory holds no instance file and no record"
    );

    // ── Refused for want of model profiles, as start() refuses (`has_model_profiles`) ──
    // The instance file exists from here on; the instance reads the render
    // of its id (models@default.ini), never the pool.
    std::fs::write(sys.join("server@default.ini"), "[Server]\nPort = 8081\n")
        .expect("server@default.ini");
    // The per-user models.ini has a section: it must not count.
    let user_model_profiles = crate::paths::models_ini();
    std::fs::write(&user_model_profiles, "[user-model]\nmodel = /m/user.gguf\n")
        .expect("user presets");
    let sys_model_profiles = sys.join("models.ini");
    let render = sys.join("models@default.ini");
    std::fs::write(
        &sys_model_profiles,
        "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n",
    )
    .expect("pool");
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoModelProfiles),
        "the pool has presets, the render of the instance does not exist (the framework's directory does not count)"
    );
    std::fs::write(&render, "[*]\ntemp = 0.7\n").expect("render");
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoModelProfiles),
        "a render with only the global section"
    );
    // Every model profile switched off: the sections are in the file llama-server
    // does not read.
    std::fs::write(
        sys.join("models.ini.disabled"),
        "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n",
    )
    .expect("models.ini.disabled");
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoModelProfiles),
        "only switched-off presets"
    );
    std::fs::write(
        &render,
        "[*]\ntemp = 0.7\n\n[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\nctx-size = 172032\n",
    )
    .expect("render with a preset");

    // ── Same launch as start() for the same config ───────────────────────
    // Real keys; `Threads = lots` is nonsense, which load() reads as unset:
    // the launcher must read it the same way.
    let models = dir.path().join("models");
    let content = format!(
        "[Server]\nPort = 8081\nHostname = 127.0.0.1\nModelsDir = {}\nModelsMax = 2\n\
         LogVerbosity = 3\nThreads = lots\nWebui = false\nAttnRotDisable = true\n\
         RocblasUseHipblaslt = false\n",
        models.display()
    );
    std::fs::write(sys.join("server@default.ini"), &content).expect("server@default.ini");
    std::fs::write(&user_ini, &content).expect("server.ini");
    let cfg = crate::server_cfg::load();
    // The instance reads the render of its id, not the pool.
    let expected_args = server_args(&cfg, &sys.join("models@default.ini"));
    let expected_env = env_vars(&cfg);
    assert!(
        expected_args.windows(2).any(|w| w == ["--port", "8081"]),
        "fixture sanity: {expected_args:?}"
    );
    assert!(
        !expected_env.is_empty(),
        "fixture sanity: env-only settings set"
    );

    let cmd = instance_command("default", &exe).expect("launch of server@default.ini");
    assert_eq!(cmd.get_program(), exe.as_os_str(), "program");
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args, expected_args,
        "args are server_args() of the instance file"
    );
    assert_eq!(
        cmd.get_current_dir(),
        Some(crate::paths::data_root().as_path()),
        "cwd as start() sets it"
    );
    let envs = envs_of(&cmd);
    assert_eq!(
        envs.get(&OsString::from("LLAMA_CACHE")),
        Some(&Some(models.clone().into_os_string())),
        "LLAMA_CACHE is the file's ModelsDir"
    );
    for (k, v) in &expected_env {
        assert_eq!(
            envs.get(&OsString::from(k)),
            Some(&Some(OsString::from(v))),
            "env-only setting {k}"
        );
    }
    // The ROCm PATH prepend of start() (`proc::prepend_rocm_path`).
    match crate::paths::rocm_bin_dir() {
        None => assert!(
            !envs.contains_key(&OsString::from("PATH")),
            "no ROCm runtime found: PATH left inherited"
        ),
        Some(bin) => {
            let path = envs
                .get(&OsString::from("PATH"))
                .cloned()
                .flatten()
                .expect("PATH set with a ROCm runtime");
            assert_eq!(
                std::env::split_paths(&path).next(),
                Some(bin),
                "ROCm bin dir first on PATH"
            );
        }
    }
    // systemd captures the output: the launcher writes no log of its own.
    assert!(
        !crate::paths::server_log().exists(),
        "the launcher must not create llama-server.log"
    );

    // ── GT1: the GTT pair reaches the launch, `false` takes it away ───────
    // Paged host memory as driver GTT: every instance exports both variables
    // as `0`, so compaction and reclaim stop evicting its GPU queues, and the
    // profile that sets `RocmHostMemoryGtt = false` exports neither (the
    // runtime's defaults return). Pinned on the two paths that build a launch:
    // `instance_command` (the manager's `run-instance`) and `launch_command`
    // (what `start()` spawns), the one `instance_command` itself calls.
    let gtt_of = |cmd: &std::process::Command| -> BTreeMap<String, String> {
        envs_of(cmd)
            .into_iter()
            .filter(|(k, _)| k.to_string_lossy().starts_with("HSA_"))
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map_or(String::new(), |v| v.to_string_lossy().into_owned()),
                )
            })
            .collect()
    };
    let both = || {
        BTreeMap::from([
            ("HSA_USERPTR_FOR_PAGED_MEM".to_string(), "0".to_string()),
            ("HSA_SVM_FOR_PAGED_MEM".to_string(), "0".to_string()),
        ])
    };
    // The fixture of the block above names no `RocmHostMemoryGtt`: the unset
    // state is the first case, and it is the state of every instance today.
    for (key, want) in [("", both()), ("true", both()), ("false", BTreeMap::new())] {
        let gtt = if key.is_empty() {
            String::new()
        } else {
            format!("RocmHostMemoryGtt = {key}\n")
        };
        let text = format!(
            "[Server]\nPort = 8081\nModelsDir = {}\nAttnRotDisable = true\n{gtt}",
            models.display()
        );
        std::fs::write(sys.join("server@default.ini"), &text).expect("server@default.ini with GTT");
        std::fs::write(&user_ini, &text).expect("server.ini with GTT");
        let cmd = instance_command("default", &exe).expect("launch of server@default.ini");
        assert_eq!(
            gtt_of(&cmd),
            want,
            "instance_command with RocmHostMemoryGtt {key:?}"
        );
        // The same env on `launch_command`, the path `start()` takes.
        let cmd = launch_command(&exe, &crate::server_cfg::load(), &render);
        assert_eq!(
            gtt_of(&cmd),
            want,
            "launch_command with RocmHostMemoryGtt {key:?}"
        );
    }

    // ── The instance file is what is read, not server.ini ────────────────
    let other_models = dir.path().join("other-models");
    std::fs::write(
        &user_ini,
        format!(
            "[Server]\nPort = 9999\nModelsDir = {}\n",
            other_models.display()
        ),
    )
    .expect("server.ini");
    std::fs::write(
        sys.join("server@default.ini"),
        format!("[Server]\nPort = 8082\nModelsDir = {}\n", models.display()),
    )
    .expect("server@default.ini");
    let cmd = instance_command("default", &exe).expect("launch of server@default.ini");
    assert_eq!(
        port_of(&cmd).as_deref(),
        Some("8082"),
        "port of server@default.ini"
    );
    assert_eq!(
        envs_of(&cmd).get(&OsString::from("LLAMA_CACHE")),
        Some(&Some(models.into_os_string())),
        "ModelsDir of server@default.ini"
    );
    // And each id reads its own file, and its own render.
    std::fs::write(
        sys.join("models@lan-2.ini"),
        "[*]\ntemp = 0.7\n\n[lan-model]\nmodel = /m/lan.gguf\n",
    )
    .expect("models@lan-2.ini");
    let cmd = instance_command("lan-2", &exe).expect("launch of server@lan-2.ini");
    assert_eq!(
        port_of(&cmd).as_deref(),
        Some("8090"),
        "port of server@lan-2.ini"
    );
    let lan_model_profile = sys.join("models@lan-2.ini").to_string_lossy().into_owned();
    let lan_args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(
        lan_args
            .windows(2)
            .any(|w| w == ["--models-preset", lan_model_profile.as_str()]),
        "lan-2 reads its own render: {lan_args:?}"
    );

    // ── The section names the server profile the launch reads ─────────────
    // default's section names `big`: the settings come from `server@big.ini`,
    // while the model profiles stay the render of the instance id.
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = big\n\n[lan-2]\nServerProfile = lan-2\n",
    )
    .expect("instances.ini");
    std::fs::write(
        sys.join("server@big.ini"),
        format!(
            "[Server]\nPort = 8100\nHostname = 127.0.0.1\nModelsDir = {}\n",
            dir.path().join("big-models").display()
        ),
    )
    .expect("server@big.ini");
    std::fs::write(
        sys.join("models@big.ini"),
        "[*]\ntemp = 0.7\n\n[big-model]\nmodel = /m/big.gguf\n",
    )
    .expect("models@big.ini");
    let cmd =
        instance_command("default", &exe).expect("launch through the profile the section names");
    assert_eq!(
        port_of(&cmd).as_deref(),
        Some("8100"),
        "the port of the profile the section names, not the one named after the id"
    );
    let big_args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let own_render = sys
        .join("models@default.ini")
        .to_string_lossy()
        .into_owned();
    let profile_render = sys.join("models@big.ini").to_string_lossy().into_owned();
    assert!(
        big_args
            .windows(2)
            .any(|w| w == ["--models-preset", own_render.as_str()]),
        "the render of the instance id, whatever the profile: {big_args:?}"
    );
    assert!(
        !big_args
            .windows(2)
            .any(|w| w == ["--models-preset", profile_render.as_str()]),
        "never a render named after the profile: {big_args:?}"
    );
    // A section naming a profile whose file is missing: the refusal names that
    // profile's file, and `server@default.ini` (which is there) stands in for
    // nothing.
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = huge\n",
    )
    .expect("instances.ini");
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoConfig(sys.join("server@huge.ini"))),
        "NoConfig names the profile the section names"
    );
    // The same record with a blank profile, while the file named after the id
    // IS there: the id never stands in for a profile the section does not name,
    // so the refusal is the record, not that file.
    std::fs::write(sys.join("instances.ini"), "[default]\nServerProfile = \n")
        .expect("instances.ini");
    assert!(
        sys.join("server@default.ini").is_file(),
        "fixture sanity: the file named after the id is there"
    );
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoConfig(sys.join("instances.ini"))),
        "a blank ServerProfile names no profile"
    );
    // A profile name that builds no path (not a valid instance id: a parent
    // directory, an uppercase letter, a slash) names no file either, so the
    // refusal is the record as well, never a path built out of the name and
    // never the file named after the id (which is there).
    for profile in ["../x", "Big", "a/b"] {
        std::fs::write(
            sys.join("instances.ini"),
            format!("[default]\nServerProfile = {profile}\n"),
        )
        .expect("instances.ini");
        assert_eq!(
            instance_command("default", &exe).err(),
            Some(LaunchError::NoConfig(sys.join("instances.ini"))),
            "ServerProfile = {profile:?} names no profile file"
        );
    }
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = lan-2\n",
    )
    .expect("instances.ini");

    // The section header names the instance id exactly: a record whose only
    // section is `[Default]` has no section for `default`, so the launch has
    // no profile, and the refusal is the record even with a valid
    // ServerProfile and that profile's file on disk. An instance is the
    // section the caller named, not a header that differs in case.
    std::fs::write(
        sys.join("instances.ini"),
        "[Default]\nServerProfile = default\n",
    )
    .expect("instances.ini");
    assert!(
        sys.join("server@default.ini").is_file()
            && has_model_profiles_in(&sys.join("models@default.ini")),
        "fixture sanity: the named profile's file and the instance's render are both there"
    );
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoConfig(sys.join("instances.ini"))),
        "a section header of `[Default]` is not a section for `default`"
    );
    // The exact-name record back, for the blocks below.
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = lan-2\n",
    )
    .expect("instances.ini");
    // Under the manager (Linux and Windows) a ModelsDir left unset or blank
    // in server@<id>.ini is the engine's `paths::instance_models_dir()`: the
    // folder the configurator scans and names in its hint when a save leaves
    // it unset, not the launching account's home.
    let user_models = crate::server_cfg::default_models_dir();
    let instance_models = crate::paths::instance_models_dir();
    assert_ne!(
        instance_models.to_string_lossy(),
        user_models,
        "fixture sanity: the two defaults differ"
    );
    let expected_models = if cfg!(any(windows, target_os = "linux")) {
        instance_models.into_os_string()
    } else {
        OsString::from(user_models)
    };
    for content in [
        "[Server]\nPort = 8083\n",
        "[Server]\nPort = 8083\nModelsDir =   \n",
    ] {
        std::fs::write(sys.join("server@default.ini"), content).expect("server@default.ini");
        let cmd = instance_command("default", &exe).expect("launch with ModelsDir unset");
        assert_eq!(
            envs_of(&cmd).get(&OsString::from("LLAMA_CACHE")),
            Some(&Some(expected_models.clone())),
            "LLAMA_CACHE for {content:?}"
        );
    }

    // ── The slot-state directory, as start() creates it ──────────────────
    // Off (the fixtures above): no directory, default or configured.
    let default_state = crate::slot_state::default_state_dir();
    assert!(
        default_state.starts_with(dir.path()),
        "fixture sanity: the default state dir is under the redirect"
    );
    assert!(
        !default_state.exists(),
        "SaveStateOnShutdown unset: no state directory"
    );
    let state = dir.path().join("states").join("default");
    std::fs::write(
        sys.join("server@default.ini"),
        format!(
            "[Server]\nSaveStateOnShutdown = false\nStateDir = {}\n",
            state.display()
        ),
    )
    .expect("server@default.ini");
    instance_command("default", &exe).expect("launch with snapshots off");
    assert!(
        !state.exists(),
        "SaveStateOnShutdown off: StateDir not created"
    );
    // On with StateDir: created before the launch is returned, with its parents.
    std::fs::write(
        sys.join("server@default.ini"),
        format!(
            "[Server]\nSaveStateOnShutdown = true\nStateDir = {}\n",
            state.display()
        ),
    )
    .expect("server@default.ini");
    let cmd = instance_command("default", &exe).expect("launch with snapshots on");
    assert!(state.is_dir(), "SaveStateOnShutdown on: StateDir created");
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--slot-save-path" && w[1] == state.to_string_lossy()),
        "the created directory is the one passed: {args:?}"
    );
    // On with no StateDir: the instance gets a snapshot folder of its own,
    // named by its id. Under the manager (Linux and Windows) it sits under
    // the engine's `paths::instance_state_dir()`, the folder the
    // configurator's Server tab names in its hint, not the launching
    // account's data root.
    let state_root = if cfg!(any(windows, target_os = "linux")) {
        crate::paths::instance_state_dir()
    } else {
        default_state.clone()
    };
    let instance_state = state_root.join("default");
    assert!(
        instance_state.starts_with(dir.path()) && !instance_state.exists(),
        "fixture sanity: {}",
        instance_state.display()
    );
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nSaveStateOnShutdown = true\n",
    )
    .expect("server@default.ini");
    let cmd = instance_command("default", &exe).expect("launch with default state dir");
    assert!(
        instance_state.is_dir(),
        "default state directory created: {}",
        instance_state.display()
    );
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--slot-save-path" && w[1] == instance_state.to_string_lossy()),
        "the instance's default state directory is the one passed: {args:?}"
    );
    // A second instance with no StateDir: its own folder, the same root, and
    // the first instance's folder untouched: two instances never share the
    // snapshots.
    // An instance IS the section of `instances.ini` (pinned above), so the
    // record gains the `[gpu-1]` section, naming the profile whose file the
    // block writes. Its render is keyed by the id too (`models@<id>.ini`), so
    // the fixture writes that as well, with a model profile section.
    std::fs::write(
        sys.join("instances.ini"),
        "[default]\nServerProfile = default\n\n[lan-2]\nServerProfile = lan-2\n\n[gpu-1]\nServerProfile = gpu-1\n",
    )
    .expect("instances.ini");
    std::fs::write(
        sys.join("models@gpu-1.ini"),
        "[*]\ntemp = 0.7\n\n[gpu-model]\nmodel = /m/gpu.gguf\n",
    )
    .expect("models@gpu-1.ini");
    std::fs::write(
        sys.join("server@gpu-1.ini"),
        "[Server]\nSaveStateOnShutdown = true\n",
    )
    .expect("server@gpu-1.ini");
    let other = state_root.join("gpu-1");
    let cmd = instance_command("gpu-1", &exe).expect("launch of a second instance");
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--slot-save-path" && w[1] == other.to_string_lossy()),
        "the second instance has its own folder: {args:?}"
    );
    assert!(
        other.is_dir(),
        "the second folder is created: {}",
        other.display()
    );
    assert_eq!(
        std::fs::read_dir(&instance_state)
            .expect("first folder readable")
            .count(),
        0,
        "the first instance's folder carries nothing of the second"
    );
    // A directory that cannot be created (a file is in the way) refuses the
    // launch, as start() returns the error, rather than a server that dies
    // parsing --slot-save-path.
    let blocker = dir.path().join("not-a-dir");
    std::fs::write(&blocker, "x").expect("blocker file");
    let blocked = blocker.join("state");
    std::fs::write(
        sys.join("server@default.ini"),
        format!(
            "[Server]\nSaveStateOnShutdown = true\nStateDir = {}\n",
            blocked.display()
        ),
    )
    .expect("server@default.ini");
    // The error keeps the reason, as the io::ErrorKind std gives for this very
    // operation (NotADirectory on Linux, checked with a real run).
    let kind = std::fs::create_dir_all(&blocked)
        .expect_err("fixture: a directory under a regular file")
        .kind();
    #[cfg(unix)]
    assert_eq!(kind, std::io::ErrorKind::NotADirectory, "fixture sanity");
    let err = instance_command("default", &exe).err();
    assert!(
        matches!(&err, Some(LaunchError::StateDir(p, ..)) if *p == blocked),
        "state directory that cannot be created: {err:?}"
    );
    assert_eq!(
        err,
        Some(LaunchError::StateDir(blocked, kind)),
        "the refusal carries the io error kind"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_cfg::ServerConfig;
    use std::path::Path;

    fn args_for(cfg: &ServerConfig) -> Vec<String> {
        server_args(cfg, Path::new("models.ini"))
    }

    // The render's global section `[*]` holds defaults for every model profile, not
    // a model profile of its own: a render with only that section loads nothing.
    #[test]
    fn has_model_profiles_in_ignores_the_global_section() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("models@default.ini");
        assert!(!has_model_profiles_in(&path), "the file does not exist");
        std::fs::write(&path, "[*]\ntemp = 0.7\n").expect("write render");
        assert!(
            !has_model_profiles_in(&path),
            "a render with only the global section loads no preset"
        );
        std::fs::write(&path, "[*]\ntemp = 0.7\n\n[a]\nmodel = a.gguf\n").expect("write render");
        assert!(
            has_model_profiles_in(&path),
            "a preset section beside the global one counts"
        );
    }

    // "auto" (unset / non-positive) MUST omit the flag entirely so llama.cpp
    // applies its own default; substituting a computed value would defeat it.
    #[test]
    fn auto_fields_omit_their_flags() {
        let a = args_for(&ServerConfig::default());
        // port / models-max join the unset-omits-the-flag group: the UI
        // "default" checkbox stores None, and a forced value here is what left
        // "default" still showing --port / --models-max 1.
        for flag in [
            "-t",
            "--threads-batch",
            "--port",
            "--models-max",
            "--models-memory-margin",
        ] {
            assert!(!a.contains(&flag.to_string()), "{flag} must be omitted");
        }
        // The deprecated pair must never reappear: each overwrites the whole
        // load mode in llama.cpp, so emitting them alongside -lm would silently
        // decide it (last one wins).
        for flag in ["--mlock", "--no-mmap", "--mmap", "-dio"] {
            assert!(!a.contains(&flag.to_string()), "{flag} is deprecated");
        }
        // Per-model profile keys never ride the router's line: the router would copy
        // them over every model profile's own value.
        for flag in [
            "--device",
            "--tensor-split",
            "-ts",
            "--override-tensor",
            "-ot",
            "-fit",
            "--fit",
            "--prefill-assistant",
            "--no-prefill-assistant",
            "--cache-reuse",
            "--ctx-checkpoints",
            "-ctxcp",
            "--checkpoint-min-step",
            "-cms",
            "--kv-unified",
            "-kvu",
            "--no-kv-unified",
            "--kv-unified-per-slot",
            "--moe-cache-mib",
            "--repack",
            "--no-repack",
            "-nr",
            "--no-host",
            "--spec-draft-n-ubatch",
            "--fit-ctx",
            "-fitc",
            "--fit-target",
            "-fitt",
            "-lm",
            "--load-mode",
            "--split-mode",
            "-mmdev",
            "--mmproj-device",
            "--embeddings",
            "--pooling",
        ] {
            assert!(!a.contains(&flag.to_string()), "{flag} is per preset");
        }
        // Framework policy flags and always-written fields remain present.
        assert!(
            a.contains(&"localhost".to_string()),
            "host is always written"
        );
        assert!(
            !a.contains(&"--webui-mcp-proxy".to_string()),
            "off unless asked for"
        );
    }

    #[test]
    fn explicit_values_emit_flag_value_pairs() {
        let cfg = ServerConfig {
            threads: Some(12),
            threads_batch: Some(24),
            ..Default::default()
        };
        let a = args_for(&cfg);
        let pair = |flag: &str, val: &str| {
            a.iter()
                .position(|x| x == flag)
                .is_some_and(|i| a.get(i + 1).is_some_and(|v| v == val))
        };
        assert!(pair("-t", "12"));
        assert!(pair("--threads-batch", "24"));
    }

    // R2 (ARCHITECTURE.md "Resources belong to the server profile"): the
    // server profile's `Devices` rides the router's argv as `--device <value>`.
    // A list is ONE argument, commas kept (the router splits it itself);
    // `none` is the flag with the value `none` (CPU only); absent is no flag
    // at all, every GPU as today (the same absence `auto_fields_omit_their_flags`
    // pins for the default config).
    #[test]
    fn devices_rides_the_argv_as_one_device_flag() {
        for value in ["ROCm0,ROCm1", "none"] {
            let cfg = ServerConfig {
                devices: Some(value.into()),
                ..Default::default()
            };
            let a = args_for(&cfg);
            let at = a
                .iter()
                .position(|x| x == "--device")
                .unwrap_or_else(|| panic!("Devices = {value:?} emits no --device: {a:?}"));
            assert_eq!(
                a[at + 1],
                value,
                "Devices = {value:?}: one value, one argument"
            );
            assert_eq!(
                a.iter().filter(|x| **x == "--device").count(),
                1,
                "Devices = {value:?} emits exactly one --device"
            );
        }
        // Absent: no flag anywhere on the line.
        let a = args_for(&ServerConfig::default());
        assert!(
            !a.contains(&"--device".to_string()),
            "no Devices: no --device flag: {a:?}"
        );
    }

    // Step-8 guard of the server-field recipe (top of server_cfg.rs): every
    // ServerConfig field must be consumed by the launch path: mapped to a
    // llama-server flag here, or explicitly waved through with a comment
    // saying where `start()` uses it. The exhaustive destructure breaks
    // compilation the moment a field is added, until this test decides.
    #[test]
    fn server_args_covers_every_config_field() {
        let cfg = ServerConfig {
            port: Some(9090),
            hostname: Some("0.0.0.0".into()),
            threads: Some(6),
            threads_batch: Some(12),
            models_max: Some(3),
            models_memory_margin: Some(1024),
            // R2: the GPUs of the server profile, in order, in ONE value.
            devices: Some("ROCm0,CUDA1".into()),
            models_dir: Some(r"D:\models".into()),
            // Two rules: the `,` that joins them must reach llama-server as ONE
            // argument (it does its own splitting), not as two.
            rocblas_use_hipblaslt: Some(false),
            // GT1: on explicitly (unset is the same for the launch): launch env
            // only, asserted in the env test further down.
            rocm_host_memory_gtt: Some(true),
            cuda_allreduce: Some("none".into()),
            cuda_disable_graphs: true,
            attn_rot_disable: true,
            rocprof_attach: true,
            webui: Some(false),
            webui_mcp_proxy: Some(false),
            log_verbosity: Some(2),
            // On, with an EXPLICIT dir: resolving the default would reach into
            // `paths::`, which unit tests must not touch (src/tests/mod.rs).
            save_state_on_shutdown: Some(true),
            state_dir: Some(r"E:\llama-state".into()),
            opencode_base_url: Some("https://llm.example.com".into()),
            opencode_api_key: Some("sk-test-key".into()),
            // Metadata for the web UI, set here so the assertions below can
            // prove none of it reaches the launch.
            tags: vec!["edge".into(), "v2".into()],
        };
        let ServerConfig {
            port,
            hostname,
            threads,
            threads_batch,
            models_max,
            models_memory_margin,
            // R2: the server profile's GPUs, mapped to `--device` (asserted below).
            devices,
            models_dir: _, // launch env only: start() exports it as LLAMA_CACHE
            // launch env only: `env_vars` exports it as ROCBLAS_USE_HIPBLASLT.
            // Also not a llama-server flag: rocBLAS reads it itself, below
            // llama.cpp. Its own assertions live in the env test further down.
            rocblas_use_hipblaslt: _,
            // launch env only (GT1): `env_vars` exports the two HSA_* variables
            // of the ROCm runtime, which reads them below llama.cpp. Asserted
            // in the env test further down.
            rocm_host_memory_gtt: _,
            // launch env only: `env_vars` exports it as GGML_CUDA_ALLREDUCE,
            // which ggml-cuda / ggml-hip read themselves. Asserted in the env
            // test further down.
            cuda_allreduce: _,
            // launch env only (GGML_CUDA_DISABLE_GRAPHS / LLAMA_ATTN_ROT_DISABLE),
            // asserted in the env test further down.
            cuda_disable_graphs: _,
            attn_rot_disable: _,
            // launch env only (ROCP_TOOL_ATTACH), read below llama.cpp. Asserted
            // in the env tests further down.
            rocprof_attach: _,
            webui,
            webui_mcp_proxy,
            log_verbosity,
            save_state_on_shutdown,
            state_dir,
            // Integration-only: not a llama-server flag. Used by opencode.json
            // and Claude Code snippet. The GUI edits it on the Server tab
            // (Network section).
            opencode_base_url: _,
            // Integration-only: not a llama-server flag. Written as apiKey in
            // opencode.json. The GUI edits it on the Server tab (Network section).
            opencode_api_key: _,
            // Metadata only: the tags of the profile, shown by the web UI.
            // Never a flag, never in the environment: the assertion below
            // proves the launch carries none of them.
            tags,
        } = cfg.clone();
        let a = args_for(&cfg);
        assert!(
            a.iter()
                .all(|x| tags.iter().all(|t| !x.contains(t.as_str()))),
            "a tag of the profile reached the launch: {a:?}"
        );
        let pair = |flag: &str, val: String| {
            a.iter()
                .position(|x| x == flag)
                .is_some_and(|i| a.get(i + 1).is_some_and(|v| *v == val))
        };
        assert!(pair("--port", port.unwrap().to_string()));
        assert!(pair("--host", hostname.unwrap()));
        assert!(pair("-t", threads.unwrap().to_string()));
        assert!(pair("--threads-batch", threads_batch.unwrap().to_string()));
        assert!(pair("--models-max", models_max.unwrap().to_string()));
        assert!(pair(
            "--models-memory-margin",
            models_memory_margin.unwrap().to_string()
        ));
        assert!(pair("--device", devices.unwrap()), "Devices on the argv");
        // The UI is off here, so --no-ui is passed.
        assert_eq!(a.contains(&"--no-ui".to_string()), !webui.unwrap());
        // webui-mcp-proxy is a presence flag: Some(false) here ⇒ omitted.
        assert_eq!(
            a.contains(&"--webui-mcp-proxy".to_string()),
            webui_mcp_proxy.unwrap()
        );
        // log verbosity is always passed (framework default 4 when unset).
        assert!(pair("-lv", log_verbosity.unwrap().to_string()));
        // The two snapshot fields ride ONE flag: the toggle decides whether it
        // appears, the directory is its value.
        assert_eq!(save_state_on_shutdown, Some(true));
        assert!(pair("--slot-save-path", state_dir.unwrap()));
    }

    /// `--slot-save-path` must be ABSENT when snapshots are off, and that is not
    /// cosmetic: llama.cpp validates the directory while parsing the flag and
    /// throws "not a directory" on a missing one, so emitting it unconditionally
    /// would turn a deleted folder into a server that refuses to start.
    #[test]
    fn slot_save_path_is_omitted_when_snapshots_are_off() {
        let off = args_for(&ServerConfig {
            state_dir: Some(r"E:\llama-state".into()),
            ..Default::default()
        });
        assert!(
            !off.contains(&"--slot-save-path".to_string()),
            "a StateDir alone must not enable the endpoints"
        );

        let on = args_for(&ServerConfig {
            save_state_on_shutdown: Some(true),
            state_dir: Some(r"E:\llama-state".into()),
            ..Default::default()
        });
        assert!(on
            .iter()
            .position(|x| x == "--slot-save-path")
            .is_some_and(|i| on.get(i + 1).is_some_and(|v| v == r"E:\llama-state")));
    }

    /// The web UI switch: only OFF reaches the line (`--no-ui`), since on is
    /// llama.cpp's default; neither `--ui` nor the deprecated `--webui` spelling
    /// is ever passed. With the UI off the MCP proxy flag goes too, while its
    /// stored choice stays (the next enable brings it back).
    #[test]
    fn no_ui_only_when_the_web_ui_is_off() {
        for (webui, no_ui) in [(None, false), (Some(true), false), (Some(false), true)] {
            let a = args_for(&ServerConfig {
                webui,
                webui_mcp_proxy: Some(true),
                ..Default::default()
            });
            assert_eq!(a.contains(&"--no-ui".to_string()), no_ui, "{webui:?}");
            for never in ["--ui", "--webui", "--no-webui"] {
                assert!(!a.contains(&never.to_string()), "{webui:?}: {never}");
            }
            assert_eq!(
                a.contains(&"--webui-mcp-proxy".to_string()),
                !no_ui,
                "{webui:?}: the proxy follows the UI"
            );
        }
    }

    // Unlike the thread/port fields (where <= 0 means "unset"), models-max 0 is
    // a real value (= unlimited) and must be passed, not mistaken for "default".
    #[test]
    fn models_max_zero_is_passed_as_unlimited() {
        let a = args_for(&ServerConfig {
            models_max: Some(0),
            ..Default::default()
        });
        let at = a.iter().position(|x| x == "--models-max");
        assert_eq!(
            at.and_then(|i| a.get(i + 1)).map(String::as_str),
            Some("0"),
            "models-max 0 (unlimited) is passed explicitly"
        );
    }

    #[test]
    fn nonpositive_overrides_are_treated_as_auto() {
        let cfg = ServerConfig {
            threads: Some(0),
            threads_batch: Some(-1),
            ..Default::default()
        };
        let a = args_for(&cfg);
        for flag in ["-t", "--threads-batch"] {
            assert!(!a.contains(&flag.to_string()), "{flag} must be omitted");
        }
    }

    /// A default config with the FlashAttention band OFF: the one SDK knob
    /// whose default exports something, switched off so the env tests of the
    /// others can pin "unset exports nothing".
    fn band_off() -> ServerConfig {
        ServerConfig {
            ..Default::default()
        }
    }

    /// The launch env without the GTT pair (GT1): `env_vars` exports
    /// `HSA_USERPTR_FOR_PAGED_MEM` and `HSA_SVM_FOR_PAGED_MEM` for every profile
    /// that does not switch them off, so the env tests of the other knobs read
    /// the env with that pair filtered out. The pair itself is pinned by its own
    /// test below, and on the real launch by `instance_launch_e2e`.
    fn env_vars_no_gtt(cfg: &ServerConfig) -> Vec<(String, String)> {
        env_vars(cfg)
            .into_iter()
            .filter(|(k, _)| !k.starts_with("HSA_"))
            .collect()
    }

    // The AllReduce choice rides the env verbatim, an unset one exports
    // nothing, and a word the backend does not know is not exported either (it
    // would be run as `none`).
    #[test]
    fn cuda_allreduce_rides_the_env_and_unset_exports_nothing() {
        let with = |v: Option<&str>| ServerConfig {
            cuda_allreduce: v.map(str::to_string),
            ..band_off()
        };
        for mode in crate::server_cfg::ALLREDUCE_MODES {
            assert_eq!(
                env_vars_no_gtt(&with(Some(mode))),
                [("GGML_CUDA_ALLREDUCE".to_string(), mode.to_string())]
            );
        }
        assert!(env_vars_no_gtt(&with(None)).is_empty());
        assert!(env_vars_no_gtt(&with(Some("butterfly"))).is_empty());
    }

    /// GT1: the GTT pair is the default. A profile that never names the key,
    /// and one that names it `true`, export BOTH variables as `0` (paged host
    /// memory as driver GTT, so compaction and reclaim stop evicting the GPU
    /// queues); `false` exports neither, which puts the runtime's own defaults
    /// back. Every backend: the pair goes out for a CUDA and a Vulkan profile
    /// too, only ROCm reading them (the decision is one rule, no detection).
    #[test]
    fn rocm_host_memory_gtt_exports_both_variables_unless_it_is_false() {
        let gtt = |cfg: &ServerConfig| -> std::collections::BTreeMap<String, String> {
            env_vars(cfg)
                .into_iter()
                .filter(|(k, _)| k.starts_with("HSA_"))
                .collect()
        };
        let both = || {
            std::collections::BTreeMap::from([
                ("HSA_USERPTR_FOR_PAGED_MEM".to_string(), "0".to_string()),
                ("HSA_SVM_FOR_PAGED_MEM".to_string(), "0".to_string()),
            ])
        };
        for state in [None, Some(true)] {
            assert_eq!(
                gtt(&ServerConfig {
                    rocm_host_memory_gtt: state,
                    ..band_off()
                }),
                both(),
                "{state:?} exports the pair, both as 0"
            );
        }
        assert_eq!(
            gtt(&ServerConfig {
                rocm_host_memory_gtt: Some(false),
                ..band_off()
            }),
            std::collections::BTreeMap::new(),
            "false exports neither variable"
        );
        for devices in [None, Some("CUDA0,CUDA1"), Some("Vulkan0")] {
            assert_eq!(
                gtt(&ServerConfig {
                    rocm_host_memory_gtt: None,
                    devices: devices.map(str::to_string),
                    ..band_off()
                }),
                both(),
                "the pair rides every backend, devices {devices:?}"
            );
        }
    }

    // Off must export NOTHING: ggml checks GGML_CUDA_DISABLE_GRAPHS for
    // presence, so a `=0` would still switch the graphs off.
    #[test]
    fn diagnostic_toggles_export_one_or_nothing() {
        assert!(env_vars_no_gtt(&band_off()).is_empty());
        let cfg = ServerConfig {
            cuda_disable_graphs: true,
            attn_rot_disable: true,
            rocprof_attach: true,
            ..band_off()
        };
        assert_eq!(
            env_vars_no_gtt(&cfg),
            [
                ("GGML_CUDA_DISABLE_GRAPHS".to_string(), "1".to_string()),
                ("LLAMA_ATTN_ROT_DISABLE".to_string(), "1".to_string()),
                ("ROCP_TOOL_ATTACH".to_string(), "1".to_string()),
            ]
        );
        assert!(!args_for(&cfg).iter().any(|a| a.contains("GRAPHS")));
    }

    // ── The defaults of an instance the manager launches ─────────────────
    // On Windows the manager runs as LocalSystem: a ModelsDir or StateDir
    // left unset in server@<id>.ini must not resolve against its profile
    // (`server_cfg::default_models_dir`, `slot_state::default_state_dir`),
    // but to the engine's folders under %ProgramData%\llama.cpp, which
    // llama.cpp's MSI creates with the instances' ACL. Pure over the OS and
    // the two directories, so the Windows choice is pinned on every OS.

    fn engine_dirs() -> (std::path::PathBuf, std::path::PathBuf) {
        // Spelled through a const: the names lint refuses the bare join.
        const ENGINE: &str = "llama.cpp";
        let engine = Path::new(r"C:\ProgramData").join(ENGINE);
        (engine.join("models"), engine.join("state"))
    }

    #[test]
    fn windows_instances_default_to_the_engine_s_models_and_state() {
        let (models, state) = engine_dirs();
        let cfg = with_instance_defaults(ServerConfig::default(), true, &models, &state);
        assert_eq!(cfg.models_dir_or_default(), models.to_string_lossy());
        assert_eq!(cfg.state_dir_or_default(), state.to_string_lossy());
    }

    /// Blank is unset, as everywhere else in server.ini.
    #[test]
    fn windows_instances_treat_a_blank_dir_as_unset() {
        let (models, state) = engine_dirs();
        let blank = ServerConfig {
            models_dir: Some("   ".into()),
            state_dir: Some(String::new()),
            ..ServerConfig::default()
        };
        let cfg = with_instance_defaults(blank, true, &models, &state);
        assert_eq!(cfg.models_dir_or_default(), models.to_string_lossy());
        assert_eq!(cfg.state_dir_or_default(), state.to_string_lossy());
    }

    /// What the file names wins: the defaults fill only what is missing, and
    /// nothing else of the file changes.
    #[test]
    fn windows_instances_keep_a_configured_dir() {
        let (models, state) = engine_dirs();
        let mine = ServerConfig {
            models_dir: Some(r"E:\models".into()),
            state_dir: Some(r"E:\llama-state".into()),
            port: Some(8082),
            ..ServerConfig::default()
        };
        assert_eq!(
            with_instance_defaults(mine.clone(), true, &models, &state),
            mine
        );
        let only_models = ServerConfig {
            models_dir: Some(r"E:\models".into()),
            ..ServerConfig::default()
        };
        let cfg = with_instance_defaults(only_models, true, &models, &state);
        assert_eq!(cfg.models_dir.as_deref(), Some(r"E:\models"));
        assert_eq!(cfg.state_dir_or_default(), state.to_string_lossy());
    }

    /// The state directory the caller hands in is the instance's own: the
    /// engine's state root with the id under it. Two instances over the same
    /// root never resolve to one snapshot folder.
    #[test]
    fn an_instance_with_no_state_dir_gets_the_folder_named_by_its_id() {
        let (models, root) = engine_dirs();
        let cfg =
            with_instance_defaults(ServerConfig::default(), true, &models, &root.join("gpu-1"));
        assert_eq!(
            cfg.state_dir_or_default(),
            root.join("gpu-1").to_string_lossy()
        );
        let other =
            with_instance_defaults(ServerConfig::default(), true, &models, &root.join("gpu-2"));
        assert_ne!(
            cfg.state_dir_or_default(),
            other.state_dir_or_default(),
            "two instances share a snapshot folder"
        );
    }

    /// What the profile names stays as it is: the id is never appended to a
    /// folder the user chose.
    #[test]
    fn a_state_dir_the_profile_names_never_gains_the_id() {
        let (models, root) = engine_dirs();
        let named = ServerConfig {
            models_dir: Some(r"E:\models".into()),
            state_dir: Some(r"E:\llama-state".into()),
            ..ServerConfig::default()
        };
        let resolved = with_instance_defaults(named.clone(), true, &models, &root.join("gpu-1"));
        assert_eq!(resolved, named);
        assert_eq!(resolved.state_dir_or_default(), r"E:\llama-state");
    }

    /// Linux and macOS are unchanged: the service user's home on Linux (the
    /// shipped server@default.ini names it), the user's own tree on macOS.
    #[test]
    fn other_oses_keep_their_own_defaults() {
        let (models, state) = engine_dirs();
        for cfg in [
            ServerConfig::default(),
            ServerConfig {
                models_dir: Some(" ".into()),
                ..ServerConfig::default()
            },
        ] {
            assert_eq!(
                with_instance_defaults(cfg.clone(), false, &models, &state),
                cfg
            );
        }
    }

    /// On Windows an instance runs as its virtual account, whose
    /// `paths::data_root()` is a system profile it has no business in: its
    /// working directory is the engine's `state` folder, which llama.cpp's
    /// MSI creates writable by the instances' services.
    #[test]
    fn windows_instances_work_in_the_engine_s_state_folder() {
        let (_, state) = engine_dirs();
        // Any system profile: where it is does not matter, only that it is
        // not used.
        let data_root = Path::new(r"C:\Windows\system-profile\AppData\Local\private-ai");
        assert_eq!(instance_current_dir(true, data_root, &state), state);
    }

    /// Linux and macOS are unchanged: the data root of whoever runs it.
    #[test]
    fn other_oses_work_in_the_data_root() {
        let (_, state) = engine_dirs();
        let data_root = Path::new("/var/lib/private-ai/.local/share/private-ai");
        assert_eq!(instance_current_dir(false, data_root, &state), data_root);
    }

    /// The manager's launch takes its working directory from the pure
    /// function with the real OS and the engine's state folder. The Windows
    /// branch only type-checks on Linux, hence a check on the text.
    #[test]
    fn the_service_launch_sets_the_instance_working_directory() {
        let body = instance_command_body();
        for want in [
            "instance_current_dir(",
            "cfg!(windows)",
            "paths::data_root()",
            "paths::instance_state_dir()",
        ] {
            assert!(
                body.contains(want),
                "instance_command has no {want}:\n{body}"
            );
        }
    }

    /// The code of `instance_command` in this file, before the tests.
    fn instance_command_body() -> String {
        let code = include_str!("launch.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap();
        let start = code
            .find("pub fn instance_command(")
            .expect("fn instance_command");
        let rest = &code[start..];
        rest[..rest.find("\n}\n").expect("end of fn")].to_string()
    }

    /// The manager's launch goes through the pure function with the
    /// engine's directories, before the state directory is created (so a
    /// snapshot-on instance with no StateDir creates the engine's `state`,
    /// not one in the LocalSystem profile). The Windows branch only
    /// type-checks on Linux, hence a check on the text; which OSes fill the
    /// defaults is pinned by `instance_launch_e2e` (Linux and Windows).
    #[test]
    fn the_service_launch_applies_the_instance_defaults() {
        let body = instance_command_body();
        for want in [
            "with_instance_defaults(",
            "paths::instance_models_dir()",
            "paths::instance_state_dir()",
        ] {
            assert!(
                body.contains(want),
                "instance_command has no {want}:\n{body}"
            );
        }
        let fill = body.find("with_instance_defaults(").unwrap();
        let ensure = body.find("ensure_dir(").expect("ensure_dir");
        let launch = body.find("launch_command(").expect("launch_command");
        assert!(
            fill < ensure && fill < launch,
            "the defaults are filled after they are used:\n{body}"
        );
    }
}
