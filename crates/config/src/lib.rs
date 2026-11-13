//! llama-cpp-config's library: every module, shared by the configurator
//! (`main.rs`) and the service launcher (`bin/private-ai-manager.rs`).

#[cfg(test)]
#[path = "../arch_tables.rs"]
mod arch_tables;
mod bench;
pub mod cli;
mod devices;
mod form;
mod gguf;
mod gpu_split;
pub mod gui;
mod hf;
mod hf_fit;
mod http;
#[cfg(any(target_os = "linux", windows))]
mod hwmon;
mod ini;
mod integrations;
mod kv_cache;
mod linked_files;
pub mod manager;
mod model_scan;
mod moe_cache;
mod net_ifaces;
mod paths;
mod presets;
mod proc;
mod runstate;
mod server_cfg;
mod server_form;
mod server_version;
mod settings;
mod sha256;
#[cfg(windows)]
mod single_instance;
mod slot_state;
mod startup;
pub mod strings;
mod tensor_override;
mod tensor_picker;
// Cross-cutting end-to-end tests (e.g. the Slint UI regression test) live under
// src/tests/; per-module unit tests stay inline in their own files.
#[cfg(test)]
mod tests;
