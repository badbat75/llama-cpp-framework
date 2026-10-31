//! The Linux phases: the dists from their upstream
//! archives, CUDA + Vulkan + HIP (ROCm), a .deb and an .rpm.

pub mod build_llama;
pub mod config;
pub mod configure;
pub mod dists;
pub mod install;
pub mod package;
pub mod prereq;

use anyhow::Result;

/// `build config`: llama-cpp-config, copied next to llama-server.
pub fn build_config() -> Result<()> {
    let (cfg, _) = build_llama::load_build_env()?;
    let target = crate::common::configurator::build(&cfg.llama_cpp_dir, cfg.build_jobs)?;
    crate::common::configurator::install_next_to_server(&target)
}
