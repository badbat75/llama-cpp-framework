//! pai-manager's library: the modules of `pai-manager`
//! (`bin/pai-manager.rs`), over the shared core (`pai-core`).

// The core modules this crate uses, under the names its sources use.
use pai_core::{http, launch, net_ifaces, paths, server_cfg, strings};
// The save leg of the instance unit's `ExecStop=`: the Linux command only.
#[cfg(target_os = "linux")]
use pai_core::slot_save;

pub mod manager;
// The end-to-end test that runs every phase redirecting the data root
// (src/tests/mod.rs); each module's unit tests are in the file beside it.
#[cfg(test)]
mod tests;
