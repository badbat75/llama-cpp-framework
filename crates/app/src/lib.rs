//! pai-app: the configurator's modules that need no GUI (presets, benchmarks,
//! HF, GGUF, the configuration, the server control seam), shared by the GUI
//! and the CLI, over the shared core (`pai-core`). It links no Slint.

// The core modules, under the names the sources use (`crate::paths`, ...).
pub use pai_core::{ini, net_ifaces, paths, proc, strings};

#[cfg(test)]
#[path = "../arch_tables.rs"]
mod arch_tables;
pub mod bench;
pub mod devices;
pub mod gguf;
pub mod gpu_split;
#[cfg(windows)]
pub mod gui_close;
pub mod hf;
pub mod hf_fit;
pub mod http;
#[cfg(any(target_os = "linux", windows))]
pub mod hwmon;
pub mod integrations;
pub mod kv_cache;
pub mod linked_files;
pub mod migration;
pub mod model_scan;
pub mod moe_cache;
pub mod presets;
pub mod runstate;
pub mod server_cfg;
pub mod server_control;
pub mod server_version;
pub mod settings;
pub mod sha256;
pub mod slot_state;
pub mod startup;
pub mod tensor_override;
pub mod tensor_picker;

#[cfg(test)]
mod tests;
