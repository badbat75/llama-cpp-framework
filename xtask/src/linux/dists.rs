//! The Linux dists: what each build leg needs from its dist, which is also
//! the completeness test the prerequisites phase applies before calling a
//! version installed.

use crate::common::dists::{newest_dist, Dists};
use std::path::{Path, PathBuf};

impl Dists {
    pub fn find_rocm(&self) -> Option<PathBuf> {
        newest_dist(
            &self.rocm_root(),
            &[&self.sources.rocm.marker, "lib/llvm/bin/clang++", "lib/cmake/hip/hip-config.cmake"],
        )
    }
    pub fn find_cuda(&self) -> Option<PathBuf> {
        newest_dist(
            &self.cuda_root(),
            &[&self.sources.cuda.marker, "bin/nvcc", "lib64/libcudart.so", "lib64/libcublas.so"],
        )
    }
    pub fn find_nccl(&self) -> Option<PathBuf> {
        newest_dist(&self.nccl_root(), &[&self.sources.nccl.marker, "lib/libnccl.so", "include/nccl.h"])
    }
    /// The SDK proper (the x86_64 dir), which is what VULKAN_SDK names.
    pub fn find_vulkan(&self) -> Option<PathBuf> {
        let d = newest_dist(
            &self.vulkan_root(),
            &[&self.sources.vulkan.marker, "x86_64/include/vulkan/vulkan.h", "x86_64/bin/glslc"],
        )?;
        let sdk = d.join("x86_64");
        vulkan_loader(&sdk).map(|_| sdk)
    }

}

/// The loader library of a Vulkan SDK. Since 1.4.3xx LunarG keeps it out of
/// lib/, in lib/VulkanLoader/lib, so that putting the SDK on a library path
/// does not replace the system loader by accident; older SDKs have it in lib/.
pub fn vulkan_loader(sdk: &Path) -> Option<PathBuf> {
    ["lib/libvulkan.so", "lib/VulkanLoader/lib/libvulkan.so"]
        .iter()
        .map(|rel| sdk.join(rel))
        .find(|p| p.exists())
}
