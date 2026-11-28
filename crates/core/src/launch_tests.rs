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
    let cfg = with_instance_defaults(ServerConfig::default(), true, &models, &root.join("gpu-1"));
    assert_eq!(
        cfg.state_dir_or_default(),
        root.join("gpu-1").to_string_lossy()
    );
    let other = with_instance_defaults(ServerConfig::default(), true, &models, &root.join("gpu-2"));
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
