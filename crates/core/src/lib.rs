//! pai-core: the modules the configurator (`pai-config`) and the service
//! manager (`pai-manager`) share, with no GUI dependency: the paths, the INI
//! files and the server config, the llama-server launch, the strings.

pub mod checks;
pub mod client;
pub mod ini;
pub mod launch;
pub mod migrate;
pub mod net_ifaces;
pub mod paths;
pub mod proc;
pub mod server_cfg;
pub mod slot_state;
pub mod strings;
// The end-to-end test that runs every phase redirecting the data root
// (src/tests/mod.rs); per-module unit tests stay inline in their own files.
#[cfg(test)]
mod tests;
