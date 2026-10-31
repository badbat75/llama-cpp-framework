//! Configure phase (cargo run -p xtask -- configure): detects paths and tools and writes
//! build/linux/config-build.toml. Run it after the prerequisites and again
//! whenever the ROCm dist changes (the compiler path and the GPU targets are
//! derived from it). CUDA, NCCL and Vulkan are not recorded: the build finds
//! the newest of each on every run. The dist root in use is RECORDED, so
//! build, package and the next configure use it without being told again.


use super::config::{self, BuildConfig};
use crate::common::dists::Dists;
use crate::common::report::Rows;
use crate::common::rocm::{gpu_targets, GPU_TARGETS_FALLBACK};
use crate::common::ui;
use crate::common::util::{self, which};
use anyhow::{bail, Result};
use std::path::PathBuf;

pub fn run(llama_cpp_dir: Option<PathBuf>, dist_root: Option<PathBuf>) -> Result<()> {
    let recorded = config::load_optional().ok().flatten();
    let dists = Dists::resolve(dist_root.as_deref(), recorded.as_ref().map(|c| c.dist_root.as_str()))?;
    let llama_dir = util::absolute(&llama_cpp_dir.unwrap_or_else(|| util::build_root().join("llama.cpp")));
    let mut rows = Rows { gaps: Vec::new() };

    ui::say("");
    ui::head("  llama.cpp-framework: Environment Check (Linux)");
    ui::head("  ==============================================");
    ui::say("");

    // An explicit HIP_PATH / ROCM_PATH naming a usable dist wins; otherwise
    // the newest complete one under the ROCm root.
    let mut hip: Option<PathBuf> = ["HIP_PATH", "ROCM_PATH"]
        .iter()
        .filter_map(|v| std::env::var_os(v).filter(|s| !s.is_empty()).map(PathBuf::from))
        .find(|c| util::is_executable(&c.join("lib/llvm/bin/clang++")))
        .map(|c| c.canonicalize().unwrap_or(c));
    if hip.is_none() {
        hip = dists.find_rocm();
    }
    match &hip {
        Some(h) => rows.ok("HipPath", h.display().to_string()),
        None => rows.opt("HipPath", "not found (needed for HIP/ROCm; run scripts/00-install-prerequisites.sh)"),
    }

    let mut targets = hip.as_deref().map(gpu_targets).unwrap_or_default();
    if !targets.is_empty() {
        rows.ok("GpuTargets", format!("{} archs from the installed ROCm dist", targets.split(';').count()));
        ui::dim(format!("                         {targets}"));
    } else {
        targets = GPU_TARGETS_FALLBACK.to_string();
        rows.opt("GpuTargets", "no kernel list found in the dist, using the 7.14.0 snapshot");
    }

    rows.ok("LlamaCppDir", llama_dir.display().to_string());
    rows.ok("DistRoot", format!("{} (<Name>/<Version>/{})", dists.root.display(), util::OS_DIR));

    match dists.find_cuda() {
        Some(d) => rows.ok("CUDA Toolkit", d.display().to_string()),
        None => rows.opt("CUDA Toolkit", format!("none under {} (needed for CUDA)", dists.cuda_root().display())),
    }
    match dists.find_nccl() {
        Some(d) => rows.ok("NCCL", d.display().to_string()),
        None => rows.opt(
            "NCCL",
            format!("none under {} (optional: split-mode tensor between CUDA cards)", dists.nccl_root().display()),
        ),
    }
    match dists.find_vulkan() {
        Some(d) => rows.ok("Vulkan SDK", d.display().to_string()),
        None => rows.opt("Vulkan SDK", format!("none under {} (needed for Vulkan)", dists.vulkan_root().display())),
    }

    ui::say("");
    ui::head("  Tools");
    ui::head("  -----");

    for tool in ["git", "cmake", "ninja", "g++"] {
        match which(tool) {
            Some(p) => rows.ok(tool, p.display().to_string()),
            None => rows.miss(tool, "run scripts/00-install-prerequisites.sh"),
        }
    }
    for tool in ["node", "npm"] {
        match which(tool) {
            Some(p) => rows.ok(tool, p.display().to_string()),
            None => rows.opt(tool, "not found (the chat UI falls back to the prebuilt download)"),
        }
    }
    // cargo builds llama-cpp-config; rustup puts it in ~/.cargo/bin, which
    // only a new login shell has on PATH.
    let home_cargo = util::home().join(".cargo/bin/cargo");
    match which("cargo").or_else(|| util::is_executable(&home_cargo).then_some(home_cargo)) {
        Some(p) => rows.ok("cargo", p.display().to_string()),
        None => rows.miss("cargo", "run scripts/00-install-prerequisites.sh"),
    }

    // Compiler: the clang shipping in the ROCm dist when there is one, so the
    // host half and the HIP device half of the build come from the same LLVM
    // (as on Windows), then a system clang, then GCC. Full paths: the dist's
    // clang is not on PATH and nothing here puts it there machine-wide.
    let (cc, cxx) = match &hip {
        Some(h) if util::is_executable(&h.join("lib/llvm/bin/clang")) => {
            (h.join("lib/llvm/bin/clang"), h.join("lib/llvm/bin/clang++"))
        }
        _ => match (which("clang"), which("clang++")) {
            (Some(c), Some(cx)) => (c, cx),
            _ => (
                which("gcc").unwrap_or_else(|| "gcc".into()),
                which("g++").unwrap_or_else(|| "g++".into()),
            ),
        },
    };
    rows.ok("C compiler", cc.display().to_string());
    rows.ok("C++ compiler", cxx.display().to_string());

    // Linker: lld, but only the one shipping WITH the compiler above (looked
    // up next to it and nowhere else). It is what makes GGML_LTO work with
    // clang: -flto=thin objects are LLVM bitcode, which GNU ld cannot link
    // without a plugin, and llama.cpp then turns LTO off silently. Blank is a
    // supported state (GCC, or a clang without lld): CMake keeps the default.
    let lld = cc.parent().map(|d| d.join("ld.lld")).unwrap_or_default();
    let is_clang = cc.file_name().map(|n| n.to_string_lossy().starts_with("clang")).unwrap_or(false);
    let linker_type = if is_clang && util::is_executable(&lld) {
        rows.ok("Linker", format!("lld ({})", lld.display()));
        "LLD"
    } else {
        rows.opt("Linker", "system default (no lld next to the compiler)");
        ""
    };

    // nvcc compiles the host half of every .cu with a HOST compiler, and
    // supports a narrow range of them: the distro's g++, not the ROCm clang.
    let cuda_host = which("g++").map(|p| p.display().to_string()).unwrap_or_default();

    rows.summary();

    BuildConfig {
        dist_root: dists.root.display().to_string(),
        llama_cpp_dir: llama_dir.display().to_string(),
        hip_path: hip.map(|h| h.display().to_string()).unwrap_or_default(),
        gpu_targets: targets,
        build_type: "Release".into(),
        c_compiler: cc.display().to_string(),
        cxx_compiler: cxx.display().to_string(),
        linker_type: linker_type.into(),
        cuda_host_compiler: cuda_host,
        build_jobs: util::default_build_jobs(),
    }
    .save()?;
    ui::ok(format!("  build/{}/config-build.toml written.", util::OS_DIR));
    ui::say("");

    // A failure when required tools are missing, so callers can script on it;
    // the config is still written above to ease fixing the gaps incrementally.
    if !rows.gaps.is_empty() {
        bail!("{} required tool(s) missing", rows.gaps.len());
    }
    Ok(())
}
