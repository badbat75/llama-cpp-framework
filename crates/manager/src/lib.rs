//! pai-manager's library: the modules of `pai-manager`
//! (`bin/pai-manager.rs`), over the shared core (`pai-core`).

// The core modules this crate uses, under the names its sources use.
use pai_core::{launch, net_ifaces, paths, server_cfg, strings};

pub mod manager;
// The end-to-end test that runs every phase redirecting the data root
// (src/tests/mod.rs); per-module unit tests stay inline in their own files.
#[cfg(test)]
mod tests;
