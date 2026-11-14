//! server.ini's read and write through the seam (`server_control`): `load()`
//! and `save()`, beside the schema and the rest of the IO in the core
//! (`pai_core::server_cfg`), which it re-exports. `save` stays here because it
//! checks the presets (`presets::check_startup_count`).

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
