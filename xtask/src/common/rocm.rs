//! The GPU targets of a ROCm build, derived from the installed dist: one rule
//! for the Linux and the Windows build.

use std::collections::BTreeSet;
use std::path::Path;

// GPU targets are DERIVED from the installed ROCm dist, not hand-maintained,
// by ONE rule on both operating systems, so the Windows and the Linux build
// cover the same hardware; a dist bump only needs a re-run of the configure
// phase.
//  - the dist's .kpack/blas_lib_gfx*.kpack is the authoritative per-arch
//    kernel list (the 7.1-era TensileLibrary_lazy_*.dat files survive as a
//    legacy subset, only used as fallback; bin/rocblas on Windows,
//    lib/rocblas on Linux, both looked at);
//  - the data-center families are dropped: CDNA/Vega (gfx9xx), for which
//    Windows has no driver (and gfx906's kernels are gone from TheRock), and
//    gfx125x (AMD ships it as the `gfx125X-dcgpu` dist, the Windows
//    multiarch dist has no kernels for it, and llama.cpp v0.6.0 does not
//    compile for it: mma.cuh uses the gfx12 WMMA builtins, which gfx1250
//    lacks, "needs target feature wmma-128b-insts"). Linux does drive the MI
//    cards: add them by hand in the config if this machine has one (~56 MiB
//    of ggml-hip per arch);
//  - the Ryzen iGPU archs are always kept, even if a future dist drops their
//    kernels: the driver-bundled HIP runtime on Windows fails kernel-module
//    load on EVERY visible device when one visible device has no matching
//    code object, so a visible iGPU would otherwise break the dGPU with
//    "device kernel image is invalid".
const GPU_TARGETS_IGPU_SAFETY: [&str; 5] = ["gfx1035", "gfx1036", "gfx1103", "gfx1152", "gfx1153"];
pub const GPU_TARGETS_FALLBACK: &str = "gfx1010;gfx1011;gfx1012;gfx1030;gfx1031;gfx1032;gfx1033;gfx1034;gfx1035;gfx1036;gfx1100;gfx1101;gfx1102;gfx1103;gfx1150;gfx1151;gfx1152;gfx1153;gfx1200;gfx1201";

/// The archs named by `<prefix>gfx...<suffix>` files in `dir`.
fn archs_in(dir: &Path, prefix: &str, suffix: &str) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let arch = name.strip_prefix(prefix)?.strip_suffix(suffix)?;
            let ok = arch.strip_prefix("gfx").map(|r| !r.is_empty() && r.bytes().all(|c| c.is_ascii_alphanumeric()));
            ok.unwrap_or(false).then(|| arch.to_string())
        })
        .collect()
}

/// The semicolon-separated target list for a ROCm dist, empty when the dist
/// carries no kernel list (the caller then uses GPU_TARGETS_FALLBACK).
pub fn gpu_targets(hip: &Path) -> String {
    let mut archs = archs_in(&hip.join(".kpack"), "blas_lib_", ".kpack");
    for legacy in ["bin", "lib"] {
        if archs.is_empty() {
            archs = archs_in(&hip.join(legacy).join("rocblas").join("library"), "TensileLibrary_lazy_", ".dat");
        }
    }
    if archs.is_empty() {
        return String::new();
    }
    let mut set: BTreeSet<String> = archs
        .into_iter()
        .filter(|a| !(a.starts_with("gfx9") || a.starts_with("gfx125")))
        .collect();
    set.extend(GPU_TARGETS_IGPU_SAFETY.iter().map(|s| s.to_string()));
    set.into_iter().collect::<Vec<_>>().join(";")
}
