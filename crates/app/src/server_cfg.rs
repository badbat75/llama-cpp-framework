//! server.ini's read and write through the seam (`server_control`): `load()`
//! and `save()`, beside the schema and the rest of the IO in the core
//! (`pai_core::server_cfg`), which it re-exports. `save` stays here because it
//! checks the presets (`presets::check_startup_count`).

use std::fs;
use std::io;

use crate::server_control::{self, ConfigFile, Os};
use pai_core::checks;
pub use pai_core::server_cfg::*;

/// The instance's server config, read through the seam: defaults when the
/// file is missing or cannot be read, as the core's `load` reads a file.
/// Shadows the core's re-exported `load`, which reads the per-user file.
pub fn load() -> ServerConfig {
    let text = server_control::backend()
        .read_config(ConfigFile::Server)
        .ok()
        .flatten();
    from_text(&text.unwrap_or_default())
}

/// The checks a server config must pass before it is written, the same for
/// every writer (the GUI, `pai`, the bff) and every backend (macOS has no
/// manager): the manager's own content checks (`pai_core::checks::server_file`)
/// on the values as set, and what needs this machine: `Threads` and
/// `ThreadsBatch` from 1 to `cpus`, the logical CPU count. Pure, no `paths::`.
pub fn check(cfg: &ServerConfig, cpus: usize) -> Result<(), checks::Refusal> {
    let ints = [
        ("Port", cfg.port),
        ("ModelsMax", cfg.models_max),
        ("ModelsMemoryMargin", cfg.models_memory_margin),
        ("LogVerbosity", cfg.log_verbosity),
    ];
    for (key, value) in ints {
        if let (Some(v), Some(range)) = (value, checks::server_int_range(key)) {
            checks::whole_number(key, &v.to_string(), &range)?;
        }
    }
    let cpus = 1..=i32::try_from(cpus).unwrap_or(i32::MAX);
    for (key, value) in [
        ("Threads", cfg.threads),
        ("ThreadsBatch", cfg.threads_batch),
    ] {
        if let Some(v) = value {
            checks::whole_number(key, &v.to_string(), &cpus)?;
        }
    }
    let texts = [
        ("ModelsDir", &cfg.models_dir),
        ("StateDir", &cfg.state_dir),
        ("OpencodeBaseUrl", &cfg.opencode_base_url),
        ("OpencodeApiKey", &cfg.opencode_api_key),
    ];
    for (key, value) in texts {
        if let Some(v) = value {
            checks::comment_markers(key, v)?;
        }
    }
    checks::bind_list(cfg.hostname.as_deref().unwrap_or_default())
}

pub fn save(cfg: &ServerConfig) -> io::Result<()> {
    let mut cfg = cfg.clone();
    cfg.opencode_base_url = cfg
        .opencode_base_url
        .as_deref()
        .and_then(normalize_base_url);
    let cfg = &cfg;
    let cpus = std::thread::available_parallelism().map_or(usize::MAX, |n| n.get());
    check(cfg, cpus).map_err(|r| io::Error::other(r.text()))?;
    validate_for_save(cfg)?;
    // A lower ModelsMax can strand the presets already set to load on startup:
    // the router would refuse to start (`presets::check_startup_count`).
    let startup = crate::presets::load_all()
        .iter()
        .filter(|p| p.load_on_startup)
        .count();
    crate::presets::check_startup_count(startup, cfg.models_max)?;
    let models_dir = cfg.models_dir_or_default();
    let backend = server_control::backend();
    let leave_dirs_unset = server_control::dirs_left_unset(backend.kind(), Os::current());
    backend
        .write_config(ConfigFile::Server, &render_for(cfg, leave_dirs_unset))
        .map_err(io::Error::other)?;
    // models_dir_or_default() never returns blank (it falls back to the
    // home-derived default), so this is unconditionally safe to create;
    // not where the dirs are left unset: the instances use the engine's
    // folders, and the user's default would be created for nothing.
    if !leave_dirs_unset {
        let _ = fs::create_dir_all(&models_dir);
    }
    Ok(())
}

/// The ModelsDir the configurator scans, downloads into and browses from
/// (pure): the configured one; when unset or blank, the instance's default
/// folder `instance_models` where a save leaves the folders unset
/// (`server_control::dirs_left_unset`, the Windows manager), else the
/// user's `models_dir_or_default()`.
pub fn effective_models_dir(
    cfg: &ServerConfig,
    kind: server_control::Kind,
    os: Os,
    instance_models: &std::path::Path,
) -> String {
    if server_control::dirs_left_unset(kind, os) && configured_models_dir(cfg).is_none() {
        return instance_models.to_string_lossy().into_owned();
    }
    cfg.models_dir_or_default()
}

/// The StateDir of the running instance (pure): the configured one; when
/// unset or blank, the instance's default folder `instance_state` where a
/// save leaves the folders unset (`server_control::dirs_left_unset`, the
/// manager on Linux and Windows), else the user's
/// `slot_state::state_dir(cfg)`.
pub fn effective_state_dir(
    cfg: &ServerConfig,
    kind: server_control::Kind,
    os: Os,
    instance_state: &std::path::Path,
) -> std::path::PathBuf {
    if server_control::dirs_left_unset(kind, os)
        && cfg.state_dir.as_deref().is_none_or(|s| s.trim().is_empty())
    {
        return instance_state.to_path_buf();
    }
    crate::slot_state::state_dir(cfg)
}

/// Whether a preset save or a download may seed an unset ModelsDir into the
/// server file (pure): only when it is unset or blank, and never where a
/// save leaves the folders unset (the instance default is in force there).
pub fn seeds_models_dir(cfg: &ServerConfig, kind: server_control::Kind, os: Os) -> bool {
    configured_models_dir(cfg).is_none() && !server_control::dirs_left_unset(kind, os)
}

/// The configured ModelsDir, `None` when unset or blank.
fn configured_models_dir(cfg: &ServerConfig) -> Option<&str> {
    cfg.models_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// `effective_models_dir` for the saved config, the real backend and OS.
pub fn models_dir() -> String {
    effective_models_dir(
        &load(),
        server_control::backend().kind(),
        Os::current(),
        &crate::paths::instance_models_dir(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_control::Kind;
    use std::path::Path;

    /// The instance default as `paths::instance_models_dir` gives it on
    /// Windows; passed in, so no unit test reads `paths::`. Split, so the
    /// names lint does not read the engine's folder as the old user tree.
    fn instance_models() -> &'static Path {
        Path::new(concat!(r"C:\ProgramData\llama", r".cpp\models"))
    }

    /// The same on Linux: the shared folder the engine's shipped
    /// `server@default.ini` names.
    fn linux_instance_models() -> &'static Path {
        Path::new("/var/lib/llama.cpp/models")
    }

    fn unset_configs() -> [ServerConfig; 3] {
        [
            ServerConfig::default(),
            ServerConfig {
                models_dir: Some(String::new()),
                ..ServerConfig::default()
            },
            ServerConfig {
                models_dir: Some("   ".into()),
                ..ServerConfig::default()
            },
        ]
    }

    fn set_config() -> ServerConfig {
        ServerConfig {
            models_dir: Some(r"E:\models".into()),
            ..ServerConfig::default()
        }
    }

    /// Under the manager (Windows and Linux) an unset (or blank) ModelsDir
    /// is the instance's folder, never the user's `~/.llama.cpp`, which the
    /// instance's account cannot read: the scans, the downloads and the
    /// Browse start all follow it.
    #[test]
    fn under_the_manager_an_unset_models_dir_is_the_instance_folder() {
        for (os, models) in [
            (Os::Windows, instance_models()),
            (Os::Linux, linux_instance_models()),
        ] {
            for cfg in unset_configs() {
                assert_eq!(
                    effective_models_dir(&cfg, Kind::Manager, os, models),
                    models.to_string_lossy(),
                    "{os:?} {cfg:?}"
                );
                assert_ne!(
                    effective_models_dir(&cfg, Kind::Manager, os, models),
                    cfg.models_dir_or_default(),
                    "{os:?} {cfg:?}"
                );
            }
        }
    }

    /// A configured ModelsDir wins everywhere, the Windows manager included.
    #[test]
    fn a_configured_models_dir_wins_on_every_os_and_backend() {
        for kind in [Kind::Manager, Kind::Direct] {
            for os in [Os::Windows, Os::Linux, Os::MacOs] {
                assert_eq!(
                    effective_models_dir(&set_config(), kind, os, instance_models()),
                    r"E:\models",
                    "{kind:?} {os:?}"
                );
            }
        }
    }

    /// Elsewhere (macOS, any direct launch) an unset one is the user's
    /// default, as before.
    #[test]
    fn elsewhere_an_unset_models_dir_is_the_user_default() {
        let cases = [
            (Kind::Manager, Os::MacOs),
            (Kind::Direct, Os::Linux),
            (Kind::Direct, Os::MacOs),
            (Kind::Direct, Os::Windows),
        ];
        for (kind, os) in cases {
            for cfg in unset_configs() {
                assert_eq!(
                    effective_models_dir(&cfg, kind, os, instance_models()),
                    cfg.models_dir_or_default(),
                    "{kind:?} {os:?} {cfg:?}"
                );
            }
        }
    }

    /// Nothing seeds ModelsDir into the manager's file (Windows and Linux):
    /// the instance default is already in force, and a folder inferred from
    /// a model path or a download would pin one the instance may not read.
    /// Elsewhere an unset one is seeded as before; a set one never is.
    #[test]
    fn models_dir_is_seeded_only_when_unset_and_not_under_the_manager() {
        for cfg in unset_configs() {
            for os in [Os::Windows, Os::Linux] {
                assert!(!seeds_models_dir(&cfg, Kind::Manager, os), "{os:?} {cfg:?}");
            }
            for (kind, os) in [
                (Kind::Manager, Os::MacOs),
                (Kind::Direct, Os::MacOs),
                (Kind::Direct, Os::Linux),
            ] {
                assert!(seeds_models_dir(&cfg, kind, os), "{kind:?} {os:?} {cfg:?}");
            }
        }
        for kind in [Kind::Manager, Kind::Direct] {
            for os in [Os::Windows, Os::Linux, Os::MacOs] {
                assert!(
                    !seeds_models_dir(&set_config(), kind, os),
                    "{kind:?} {os:?}"
                );
            }
        }
    }

    /// The instances' default StateDir as `paths::instance_state_dir` gives
    /// it on Windows and Linux; passed in, so no unit test reads `paths::`.
    fn instance_state(os: Os) -> &'static Path {
        match os {
            Os::Windows => Path::new(concat!(r"C:\ProgramData\llama", r".cpp\state")),
            _ => Path::new("/var/lib/private-ai/.local/share/private-ai/state"),
        }
    }

    fn unset_state_configs() -> [ServerConfig; 3] {
        [
            ServerConfig::default(),
            ServerConfig {
                state_dir: Some(String::new()),
                ..ServerConfig::default()
            },
            ServerConfig {
                state_dir: Some("   ".into()),
                ..ServerConfig::default()
            },
        ]
    }

    /// Under the manager (Windows and Linux) an unset (or blank) StateDir
    /// is the instance's folder, where the instance writes its snapshots,
    /// never the user's data root: `pai server show` names it and the
    /// restore after a start reads it.
    #[test]
    fn under_the_manager_an_unset_state_dir_is_the_instance_folder() {
        for os in [Os::Windows, Os::Linux] {
            for cfg in unset_state_configs() {
                assert_eq!(
                    effective_state_dir(&cfg, Kind::Manager, os, instance_state(os)),
                    instance_state(os),
                    "{os:?} {cfg:?}"
                );
            }
        }
    }

    /// A configured StateDir wins on every OS and backend; elsewhere
    /// (macOS, any direct launch) an unset one is the user's default
    /// (`slot_state::state_dir`, `<data root>/state`).
    #[test]
    fn a_configured_state_dir_wins_and_elsewhere_the_user_default_stays() {
        let set = ServerConfig {
            state_dir: Some(r"E:\llama-state".into()),
            ..ServerConfig::default()
        };
        for kind in [Kind::Manager, Kind::Direct] {
            for os in [Os::Windows, Os::Linux, Os::MacOs] {
                assert_eq!(
                    effective_state_dir(&set, kind, os, instance_state(os)),
                    Path::new(r"E:\llama-state"),
                    "{kind:?} {os:?}"
                );
            }
        }
        for (kind, os) in [
            (Kind::Manager, Os::MacOs),
            (Kind::Direct, Os::Linux),
            (Kind::Direct, Os::MacOs),
            (Kind::Direct, Os::Windows),
        ] {
            // The user's default is `<data root>/state`; the data root is
            // not compared, since `data_root_e2e` moves it on another thread.
            for cfg in unset_state_configs() {
                let got = effective_state_dir(&cfg, kind, os, instance_state(os));
                assert_ne!(got, instance_state(os), "{kind:?} {os:?} {cfg:?}");
                assert!(
                    got.ends_with("state") && got.is_absolute(),
                    "{kind:?} {os:?} {cfg:?}: {}",
                    got.display()
                );
            }
        }
    }

    /// The body of the top-level `pub fn <name>(` in `code` (test functions
    /// are indented, so a line-start match is the production one).
    fn fn_body(code: &str, name: &str) -> String {
        let start = code
            .find(&format!("\npub fn {name}("))
            .unwrap_or_else(|| panic!("fn {name}"));
        let rest = &code[start..];
        rest[..rest.find("\n}\n").expect("end of fn")].to_string()
    }

    /// `models_dir()` resolves for the real backend and OS through the pure
    /// rule above. The Windows branch only type-checks on Linux, hence a
    /// check on the text.
    #[test]
    fn models_dir_resolves_for_the_backend_and_os() {
        let body = fn_body(include_str!("server_cfg.rs"), "models_dir");
        for want in [
            "effective_models_dir(",
            "Os::current()",
            "instance_models_dir()",
            ".kind()",
        ] {
            assert!(body.contains(want), "models_dir has no {want}:\n{body}");
        }
    }

    /// The two places that seed ModelsDir (a Hugging Face download's preset
    /// and the first preset save) ask `seeds_models_dir` for the real
    /// backend and OS, instead of seeding whenever it is unset.
    #[test]
    fn preset_saves_seed_models_dir_through_the_rule() {
        for (file, code) in [
            (
                "hf.rs save_preset",
                fn_body(include_str!("hf.rs"), "save_preset"),
            ),
            (
                "presets.rs save",
                fn_body(include_str!("presets.rs"), "save"),
            ),
        ] {
            for want in ["seeds_models_dir(", "Os::current()"] {
                assert!(code.contains(want), "{file} has no {want}:\n{code}");
            }
            assert!(
                !code.contains(concat!("models_dir", ".is_none()")),
                "{file} still seeds whenever ModelsDir is unset:\n{code}"
            );
        }
    }

    // ── The checks before a write (`check`) ─────────────────────────────

    /// A config with both folders explicit: their defaults resolve through
    /// `paths::`, which unit tests must not touch.
    fn checked_base() -> ServerConfig {
        ServerConfig {
            models_dir: Some("/models".into()),
            state_dir: Some("/var/lib/private-ai/state".into()),
            ..ServerConfig::default()
        }
    }

    fn not_int(key: &'static str, text: &str, lo: i32, hi: i32) -> pai_core::checks::Refusal {
        pai_core::checks::Refusal {
            key: "not-whole-number-in-range",
            values: vec![key.into(), text.into(), lo.to_string(), hi.to_string()],
        }
    }

    /// A config as the configurator writes it passes, all keys set or none.
    #[test]
    fn the_check_passes_a_config_as_the_configurator_writes_it() {
        let rich = ServerConfig {
            port: Some(9090),
            hostname: Some("127.0.0.1,192.168.1.10".into()),
            threads: Some(12),
            threads_batch: Some(16),
            models_max: Some(0),
            models_memory_margin: Some(0),
            log_verbosity: Some(5),
            opencode_base_url: Some("https://gw.example.com/llm".into()),
            opencode_api_key: Some("sk-abc123".into()),
            ..checked_base()
        };
        assert_eq!(check(&rich, 16), Ok(()));
        assert_eq!(check(&checked_base(), 16), Ok(()));
        let all = ServerConfig {
            hostname: Some("0.0.0.0".into()),
            log_verbosity: Some(0),
            port: Some(65535),
            ..checked_base()
        };
        assert_eq!(check(&all, 1), Ok(()));
    }

    /// What the manager refuses in `server@<id>.ini`, refused here too, on the
    /// value as set: a port of 0 or a negative ModelsMax would otherwise
    /// render as the commented hint, i.e. silently unset.
    #[test]
    #[allow(clippy::type_complexity)]
    fn the_check_refuses_what_the_manager_refuses() {
        use pai_core::checks::Refusal;
        let max = i32::MAX;
        let cases: Vec<(&str, fn(&mut ServerConfig), Refusal)> = vec![
            (
                "port 0",
                |c| c.port = Some(0),
                not_int("Port", "0", 1, 65535),
            ),
            (
                "port 65536",
                |c| c.port = Some(65536),
                not_int("Port", "65536", 1, 65535),
            ),
            (
                "ModelsMax -1",
                |c| c.models_max = Some(-1),
                not_int("ModelsMax", "-1", 0, max),
            ),
            (
                "ModelsMemoryMargin -1",
                |c| c.models_memory_margin = Some(-1),
                not_int("ModelsMemoryMargin", "-1", 0, max),
            ),
            (
                "LogVerbosity 7",
                |c| c.log_verbosity = Some(7),
                not_int("LogVerbosity", "7", 0, 5),
            ),
            (
                "LogVerbosity -1",
                |c| c.log_verbosity = Some(-1),
                not_int("LogVerbosity", "-1", 0, 5),
            ),
            (
                "0.0.0.0 with another address",
                |c| c.hostname = Some("0.0.0.0,127.0.0.1".into()),
                Refusal {
                    key: "bind-all-with-others",
                    values: vec![],
                },
            ),
            (
                "a comment marker",
                |c| c.models_dir = Some("/models #1".into()),
                Refusal {
                    key: "comment-markers",
                    values: vec!["ModelsDir".into()],
                },
            ),
        ];
        for (label, set, want) in cases {
            let mut cfg = checked_base();
            set(&mut cfg);
            assert_eq!(check(&cfg, 16), Err(want), "{label}");
        }
    }

    /// The thread counts run from 1 to this machine's logical CPU count, as
    /// the Server tab's sliders do.
    #[test]
    fn the_check_holds_the_thread_counts_to_the_cpu_count() {
        for (key, set) in [
            (
                "Threads",
                (|c, n| c.threads = Some(n)) as fn(&mut ServerConfig, i32),
            ),
            ("ThreadsBatch", |c, n| c.threads_batch = Some(n)),
        ] {
            for bad in [17, 0, -1] {
                let mut cfg = checked_base();
                set(&mut cfg, bad);
                assert_eq!(
                    check(&cfg, 16),
                    Err(not_int(key, &bad.to_string(), 1, 16)),
                    "{key} = {bad}"
                );
            }
            for good in [1, 16] {
                let mut cfg = checked_base();
                set(&mut cfg, good);
                assert_eq!(check(&cfg, 16), Ok(()), "{key} = {good}");
            }
        }
    }
}
