//! server.ini's read and write through the seam (`server_control`): `load()`
//! and `save()`, beside the schema and the rest of the IO in the core
//! (`pai_core::server_cfg`), which it re-exports. `save` stays here because it
//! checks the presets (`presets::check_startup_count`).

use std::fs;
use std::io;

use crate::server_control::{self, ConfigFile};
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
    server_control::backend()
        .write_config(ConfigFile::Server, &render(cfg))
        .map_err(io::Error::other)?;
    // models_dir_or_default() never returns blank (it falls back to the
    // home-derived default), so this is unconditionally safe to create.
    let _ = fs::create_dir_all(&models_dir);
    Ok(())
}
