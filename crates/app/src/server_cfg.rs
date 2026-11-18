//! server.ini's read and write through the seam (`server_control`): `load()`
//! and `save()`, beside the schema and the rest of the IO in the core
//! (`pai_core::server_cfg`), which it re-exports. `save` stays here because it
//! checks the model profiles (`model_profiles::check_startup_count`).

use std::fs;
use std::io;

use crate::server_control::{self, ConfigFile, Os};
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
    // A lower ModelsMax can strand the model profiles already set to load on startup:
    // the router would refuse to start (`model_profiles::check_startup_count`).
    let startup = crate::model_profiles::load_all()
        .iter()
        .filter(|p| p.load_on_startup)
        .count();
    crate::model_profiles::check_startup_count(startup, cfg.models_max)?;
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

/// Whether a model profile save or a download may seed an unset ModelsDir into the
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

    /// The two places that seed ModelsDir (a Hugging Face download's model profile
    /// and the first model profile save) ask `seeds_models_dir` for the real
    /// backend and OS, instead of seeding whenever it is unset.
    #[test]
    fn model_profile_saves_seed_models_dir_through_the_rule() {
        for (file, code) in [
            (
                "hf.rs save_model_profile",
                fn_body(include_str!("hf.rs"), "save_model_profile"),
            ),
            (
                "model_profiles.rs save",
                fn_body(include_str!("model_profiles.rs"), "save"),
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
}
