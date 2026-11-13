//! server.ini's write: `save()`, beside the schema and the rest of the IO in
//! the core (`pai_core::server_cfg`), which it re-exports. It stays here
//! because it checks the presets (`presets::check_startup_count`).

use std::fs;
use std::io;

use crate::ini;
use crate::paths;
pub use pai_core::server_cfg::*;

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
    let path = paths::server_ini();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // models_dir_or_default() never returns blank (it falls back to the
    // home-derived default), so this is unconditionally safe to create.
    let _ = fs::create_dir_all(&models_dir);
    ini::atomic_write(&path, &render(cfg))
}
