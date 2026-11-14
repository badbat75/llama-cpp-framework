//! pai-cli's library: the command line (`pai`, `main.rs`), over the logic it
//! shares with the GUI (`pai-app`) and the shared core (`pai-core`).

// The core and pai-app modules, under the names the sources use
// (`crate::paths`, `crate::presets`, ...).
use pai_app::{
    bench, devices, hf, hf_fit, linked_files, presets, runstate, server_cfg, server_control,
    settings, slot_state,
};
pub use pai_app::{migration, strings};
use pai_core::paths;

pub mod cli;
// Cross-cutting tests (the data-root e2e, the source and lock lints) live
// under src/tests/; per-module unit tests stay inline in their own files.
#[cfg(test)]
mod tests;
