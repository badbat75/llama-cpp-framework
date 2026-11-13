//! Where the slot KV-cache snapshots live (`StateDir`, its default), shared by
//! the launch (`launch.rs`: llama.cpp refuses a missing `--slot-save-path`
//! directory) and the configurator's save and restore legs (its
//! `slot_state.rs`, which holds the rest of the feature).

use std::path::PathBuf;

/// Default state directory when `StateDir` is unset: a sibling of `config\` and
/// `logs\` under the user's runtime root.
///
/// Overridable precisely BECAUSE of the default's one weakness: `%LOCALAPPDATA%`
/// sits on the system drive, and a dump is roughly one byte of KV cache per byte
/// the model would have recomputed, which for a large context is many GiB per
/// model. A user whose models live on another disk will want the snapshots there
/// too, and filling C:\ with no way out is not a default anyone should be stuck
/// with.
pub fn default_state_dir() -> PathBuf {
    crate::paths::data_root().join("state")
}

/// The configured state directory, or [`default_state_dir`] when unset/blank.
/// The single home for that fallback, shared by the launch path, the save/restore
/// legs and the form.
pub fn state_dir(cfg: &crate::server_cfg::ServerConfig) -> PathBuf {
    cfg.state_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(default_state_dir, PathBuf::from)
}

/// Create the state directory if it is missing.
///
/// Called from `runstate::start` BEFORE the spawn, never lazily: llama.cpp
/// validates `--slot-save-path` while parsing the argument and throws
/// "not a directory" on a path that does not exist yet, so a missing directory
/// does not degrade to "no snapshots", it stops the server from starting.
pub fn ensure_dir(cfg: &crate::server_cfg::ServerConfig) -> std::io::Result<PathBuf> {
    let dir = state_dir(cfg);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
