//! The llama-server launch, shared by the configurator's `runstate::start()` and
//! `pai-manager run-instance` (`instance_command`): `server_args()` is
//! the single source of truth for the arg list, `env_vars()` the settings
//! llama.cpp exposes only as env vars, `launch_command()` the process with its
//! cwd, `LLAMA_CACHE` and the ROCm PATH prepend (`proc::prepend_rocm_path`:
//! HIP devices vanish without it). Process control and the log stay in the
//! configurator (`runstate.rs`).

/// `true` if the presets file at `path` has at least one section.
pub fn has_presets_in(path: &std::path::Path) -> bool {
    !crate::ini::read_all(path).is_empty()
}

// ── Launch argument assembly (server_args) ───────────────────────────────

/// The full llama-server argument list derived from server.ini.
/// Single source of truth for both `start()` and `command_line()`.
pub fn server_args(
    cfg: &crate::server_cfg::ServerConfig,
    presets_path: &std::path::Path,
) -> Vec<String> {
    let hostname = cfg.hostname_or_default();

    let mut args: Vec<String> = vec![
        "--models-preset".into(),
        presets_path.to_string_lossy().into_owned(),
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

    // NO --device, --tensor-split, --split-mode, --override-tensor, -lm, -fit
    //   (nor its --fit-ctx / --fit-target), --prefill-assistant, --cache-reuse,
    //   --ctx-checkpoints, --checkpoint-min-step, --kv-unified,
    //   --kv-unified-per-slot, --moe-cache-mib, --repack, --no-host, --spec-draft-n-ubatch
    //   or --mmproj-device here: those are per-preset keys (the GPU
    //   distribution, the tensor placement, load-mode, fit, prefill-assistant,
    //   the prompt cache, the KV layout, the expert cache and repacking, the
    //   drafter, mmproj-device). The router copies its own args over every preset (`preset.merge(base_preset)`), so any of them on this
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
    //   preset key, but `get_map_key_opt` (common/preset.cpp) also maps every
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

/// The llama-server process of `cfg` and `presets_path`, before its stdio:
/// args, cwd and env, shared by `start()` and `instance_command()`.
pub fn launch_command(
    exe: &std::path::Path,
    cfg: &crate::server_cfg::ServerConfig,
    presets_path: &std::path::Path,
) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(server_args(cfg, presets_path));

    cmd.current_dir(crate::paths::data_root());
    cmd.env("LLAMA_CACHE", cfg.models_dir_or_default());
    // Settings llama.cpp exposes only as env vars; inherited by the router's
    // per-model children, so they cover every preset.
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
    /// No `server@<id>.ini` at this path.
    NoConfig(std::path::PathBuf),
    /// The system `presets.ini` has no section (`start()`'s `has_presets`).
    NoPresets,
    /// SaveStateOnShutdown is on and this slot-state directory could not be
    /// created, for this reason.
    StateDir(std::path::PathBuf, std::io::ErrorKind),
}

/// The llama-server launch of instance `id`: `exe` with `server_args()` of
/// `server@<id>.ini`, the cwd and env `start()` sets. No log, no rotation:
/// the service manager captures the output.
pub fn instance_command(
    id: &str,
    exe: &std::path::Path,
) -> Result<std::process::Command, LaunchError> {
    let ini = crate::paths::instance_ini(id).ok_or(LaunchError::InvalidId)?;
    if !ini.is_file() {
        return Err(LaunchError::NoConfig(ini));
    }
    let presets_path = crate::paths::engine_config_dir().join("presets.ini");
    if !has_presets_in(&presets_path) {
        return Err(LaunchError::NoPresets);
    }
    let cfg = crate::server_cfg::load_from(&ini);
    // Before the spawn, as in `start()`: llama.cpp refuses a missing
    // `--slot-save-path` directory during argument parsing.
    if cfg.save_state_on_shutdown_or_default() {
        crate::slot_state::ensure_dir(&cfg)
            .map_err(|e| LaunchError::StateDir(crate::slot_state::state_dir(&cfg), e.kind()))?;
    }
    Ok(launch_command(exe, &cfg, &presets_path))
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
    // A valid id with no file: no system directory at all yet.
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoConfig(sys.join("server@default.ini"))),
        "no system config directory"
    );
    assert!(
        !sys.exists(),
        "a refused launch must not create the directory"
    );
    // The directory exists and holds another instance, not this one.
    std::fs::create_dir_all(&sys).expect("system config dir");
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
    // Nor does one left in the framework's directory: nothing reads it there.
    std::fs::create_dir_all(&framework).expect("framework config dir");
    std::fs::write(
        framework.join("server@default.ini"),
        "[Server]\nPort = 8082\n",
    )
    .expect("stray server@default.ini");
    std::fs::write(
        framework.join("presets.ini"),
        "[stray]\nmodel = /m/stray.gguf\n",
    )
    .expect("stray presets.ini");
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoConfig(sys.join("server@default.ini"))),
        "the framework's directory holds no instance file"
    );

    // ── Refused for want of presets, as start() refuses (`has_presets`) ──
    // The instance file exists from here on; the presets are the system
    // directory's `presets.ini`, and llama-server reads only that file.
    std::fs::write(sys.join("server@default.ini"), "[Server]\nPort = 8081\n")
        .expect("server@default.ini");
    // The per-user presets.ini has a section: it must not count.
    let user_presets = crate::paths::presets_ini();
    std::fs::write(&user_presets, "[user-model]\nmodel = /m/user.gguf\n").expect("user presets");
    let sys_presets = sys.join("presets.ini");
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoPresets),
        "no presets.ini in the engine's directory (the framework's does not count)"
    );
    std::fs::write(&sys_presets, "; no model yet\n\n").expect("empty presets");
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoPresets),
        "a presets.ini with no section"
    );
    // Every preset switched off: the sections are in the file llama-server
    // does not read.
    std::fs::write(
        sys.join("presets.ini.disabled"),
        "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\n",
    )
    .expect("presets.ini.disabled");
    assert_eq!(
        instance_command("default", &exe).err(),
        Some(LaunchError::NoPresets),
        "only switched-off presets"
    );
    std::fs::write(
        &sys_presets,
        "[Qwen3.8-27B]\nmodel = /m/Qwen3.8-27B-Q8_0.gguf\nctx-size = 172032\n",
    )
    .expect("presets.ini");

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
    // Presets move to the system directory with the instance files (PLAN.md step 1).
    let expected_args = server_args(&cfg, &sys.join("presets.ini"));
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
    // And each id reads its own file.
    let cmd = instance_command("lan-2", &exe).expect("launch of server@lan-2.ini");
    assert_eq!(
        port_of(&cmd).as_deref(),
        Some("8090"),
        "port of server@lan-2.ini"
    );

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
    // On with no StateDir: the default one is created.
    std::fs::write(
        sys.join("server@default.ini"),
        "[Server]\nSaveStateOnShutdown = true\n",
    )
    .expect("server@default.ini");
    instance_command("default", &exe).expect("launch with default state dir");
    assert!(default_state.is_dir(), "default state directory created");
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
        server_args(cfg, Path::new("presets.ini"))
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
        // Per-preset keys never ride the router's line: the router would copy
        // them over every preset's own value.
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
            models_dir: Some(r"D:\models".into()),
            // Two rules: the `,` that joins them must reach llama-server as ONE
            // argument (it does its own splitting), not as two.
            rocblas_use_hipblaslt: Some(false),
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
        };
        let ServerConfig {
            port,
            hostname,
            threads,
            threads_batch,
            models_max,
            models_memory_margin,
            models_dir: _, // launch env only: start() exports it as LLAMA_CACHE
            // launch env only: `env_vars` exports it as ROCBLAS_USE_HIPBLASLT.
            // Also not a llama-server flag: rocBLAS reads it itself, below
            // llama.cpp. Its own assertions live in the env test further down.
            rocblas_use_hipblaslt: _,
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
        } = cfg.clone();
        let a = args_for(&cfg);
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
                env_vars(&with(Some(mode))),
                [("GGML_CUDA_ALLREDUCE".to_string(), mode.to_string())]
            );
        }
        assert!(env_vars(&with(None)).is_empty());
        assert!(env_vars(&with(Some("butterfly"))).is_empty());
    }

    // Off must export NOTHING: ggml checks GGML_CUDA_DISABLE_GRAPHS for
    // presence, so a `=0` would still switch the graphs off.
    #[test]
    fn diagnostic_toggles_export_one_or_nothing() {
        assert!(env_vars(&band_off()).is_empty());
        let cfg = ServerConfig {
            cuda_disable_graphs: true,
            attn_rot_disable: true,
            rocprof_attach: true,
            ..band_off()
        };
        assert_eq!(
            env_vars(&cfg),
            [
                ("GGML_CUDA_DISABLE_GRAPHS".to_string(), "1".to_string()),
                ("LLAMA_ATTN_ROT_DISABLE".to_string(), "1".to_string()),
                ("ROCP_TOOL_ATTACH".to_string(), "1".to_string()),
            ]
        );
        assert!(!args_for(&cfg).iter().any(|a| a.contains("GRAPHS")));
    }
}
