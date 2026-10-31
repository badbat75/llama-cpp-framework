//! The Windows dists: the same files dists.ps1 ($distFiles) requires, which
//! is also what scripts\00-install-prerequisites.ps1 calls a complete install.

use crate::common::dists::{newest_dist, Dists};
use std::path::PathBuf;

impl Dists {
    //   ROCm:    the compiler, the HIP CMake package, and rocBLAS's Tensile
    //            kernels (without them the backend loads and the first matmul
    //            dies with "Could not initialize Tensile host").
    //   CUDA:    nvcc, the static runtime ggml-cuda links, and cuBLAS.
    //   Vulkan:  the import lib and the shader compiler ggml-vulkan invokes.
    pub fn find_rocm(&self) -> Option<PathBuf> {
        newest_dist(
            &self.rocm_root(),
            &[
                &self.sources.rocm.marker,
                r"lib\llvm\bin\clang.exe",
                r"lib\cmake\hip\hip-config.cmake",
                r"bin\rocblas\library",
            ],
        )
    }
    pub fn find_cuda(&self) -> Option<PathBuf> {
        newest_dist(
            &self.cuda_root(),
            &[&self.sources.cuda.marker, r"bin\nvcc.exe", r"lib\x64\cudart_static.lib", r"lib\x64\cublas.lib"],
        )
    }
    /// The dist dir itself, which is what VULKAN_SDK names on Windows.
    pub fn find_vulkan(&self) -> Option<PathBuf> {
        newest_dist(&self.vulkan_root(), &[&self.sources.vulkan.marker, r"Lib\vulkan-1.lib", r"Bin\glslc.exe"])
    }
}
