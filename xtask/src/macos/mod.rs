//! The macOS phases (Apple Silicon): Metal plus Vulkan
//! through MoltenVK, a .pkg. scripts/00-install-prerequisites.sh installs the
//! Homebrew packages and Rust natively first, then runs `prereq` (the Vulkan
//! SDK and sccache).
//!
//! UNTESTED: written on Linux and only compile-checked for
//! aarch64-apple-darwin; the first run on a Mac is its test.

pub mod build_llama;
pub mod config;
pub mod configure;
pub mod dists;
pub mod install;
pub mod package;
pub mod prereq;

use crate::common::configurator;
use anyhow::Result;

/// The package's prefix on the target machine, and the payload path of it.
pub const OPT_DIR: &str = "/opt/llama.cpp";

/// `build config`: llama-cpp-config, copied next to llama-server.
pub fn build_config() -> Result<()> {
    let cfg = config::load()?;
    let target = configurator::build(&cfg.llama_cpp_dir, cfg.build_jobs)?;
    configurator::install_next_to_server(&target)
}
