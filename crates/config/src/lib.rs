//! pai-config's library: the configurator's GUI and CLI modules (`main.rs`),
//! over the logic they share (`pai-app`) and the shared core (`pai-core`).

// The core modules, under the names the sources use (`crate::paths`, ...).
pub use pai_core::strings;
use pai_core::{ini, net_ifaces, paths};

// The GUI-free modules, under the names the sources use (`crate::presets`, ...).
#[cfg(any(target_os = "linux", windows))]
use pai_app::hwmon;
pub use pai_app::migration;
#[cfg(test)]
use pai_app::sha256;
use pai_app::{
    bench, devices, gguf, hf, hf_fit, integrations, kv_cache, linked_files, model_scan, moe_cache,
    presets, runstate, server_cfg, server_control, server_version, settings, slot_state, startup,
    tensor_picker,
};

pub mod cli;
mod form;
mod gpu_split;
pub mod gui;
mod server_form;
#[cfg(windows)]
mod single_instance;
mod tensor_override;
// Cross-cutting end-to-end tests (e.g. the Slint UI regression test) live under
// src/tests/; per-module unit tests stay inline in their own files.
#[cfg(test)]
mod tests;
