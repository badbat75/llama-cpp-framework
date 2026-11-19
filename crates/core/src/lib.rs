//! pai-core: the modules the configurator (`pai-config`) and the service
//! manager (`pai-manager`) share, with no GUI dependency: the paths, the INI
//! files and the server config, the llama-server launch, the strings.

// One instance's model profiles file, rendered from the pool and its assignment.
pub mod assign;
pub mod bff_cfg;
pub mod checks;
pub mod client;
// A compute device as `llama-server --list-devices` reports it, and its parser.
pub mod devices;
// The GGUF header reader, a model's shape and the KV formula (the estimate's inputs).
pub mod gguf_shape;
pub mod http;
pub mod ini;
pub mod launch;
pub mod migrate;
pub mod net_ifaces;
pub mod paths;
pub mod proc;
pub mod server_cfg;
// The save leg of the slot snapshots, over the router's HTTP endpoints.
pub mod slot_save;
pub mod slot_state;
pub mod strings;
// The end-to-end test that runs every phase redirecting the data root
// (src/tests/mod.rs); per-module unit tests stay inline in their own files.
#[cfg(test)]
mod tests;
