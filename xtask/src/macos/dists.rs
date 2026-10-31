//! The macOS dists: the Vulkan SDK only (Metal is part of the system, and
//! there is no CUDA or ROCm for macOS). What the build and the package need
//! from it is also the completeness test of the prerequisites phase.

use crate::common::dists::{newest_dist, Dists};
use std::path::{Path, PathBuf};

impl Dists {
    /// The SDK proper (the macOS/ dir), which is what VULKAN_SDK names: the
    /// headers, the shader compiler ggml-vulkan invokes, the loader and the
    /// MoltenVK driver manifest the package bundles.
    pub fn find_vulkan(&self) -> Option<PathBuf> {
        let d = newest_dist(
            &self.vulkan_root(),
            &[
                &self.sources.vulkan.marker,
                "macOS/include/vulkan/vulkan.h",
                "macOS/bin/glslc",
                "macOS/lib/libvulkan.1.dylib",
                "macOS/share/vulkan/icd.d/MoltenVK_icd.json",
            ],
        )?;
        Some(d.join("macOS"))
    }
}

/// The SDK's MoltenVK driver manifest.
pub fn moltenvk_icd(sdk: &Path) -> PathBuf {
    sdk.join("share/vulkan/icd.d/MoltenVK_icd.json")
}
