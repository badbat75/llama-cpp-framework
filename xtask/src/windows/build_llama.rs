//! `build llama` on Windows (cargo run -p xtask -- build, first half): llama.cpp with CUDA +
//! Vulkan + HIP (ROCm), clang from the ROCm dist + lld-link + Ninja, in the
//! VS developer environment. The checkout, the patches, xtask/cmake/common.toml,
//! sccache and the web UI check are shared with Linux (common/llama.rs).

use super::WinConfig;
use crate::common::cmake_options::cmake_options;
use crate::common::llama::{cache_get, check_web_ui, checkout, reset_cuda_detection, SccacheDaemon};
use crate::common::dists::Dists;
use crate::common::ui;
use crate::common::util::{self, cmd, run as run_cmd};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// The config, the dists of its recorded root, and the process environment
/// a build needs: the ROCm vars first, then the VS environment on top (which
/// keeps the PATH it inherits).
pub fn load_build_env() -> Result<(WinConfig, Dists)> {
    let cfg = WinConfig::load()?;
    let dists = Dists::resolve(Some(Path::new(&cfg.dist_root)).filter(|p| !p.as_os_str().is_empty()), None)?;
    super::hip_env(&cfg);
    super::vs_env(&cfg)?;
    Ok((cfg, dists))
}

/// Paths as CMake writes them, compared the way PowerShell's -ne did:
/// separators unified, case ignored.
fn same_path(a: &str, b: &str) -> bool {
    let n = |s: &str| s.replace('/', "\\").trim_end_matches('\\').to_ascii_lowercase();
    n(a) == n(b)
}

fn fwd(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

pub fn run() -> Result<()> {
    let (cfg, dists) = load_build_env()?;
    let llama = PathBuf::from(&cfg.llama_cpp_dir);

    ui::say("");
    ui::warn("  === llama.cpp (CMake) ===");

    checkout(&llama)?;

    let build_dir = util::build_root().join("llama.cpp-cmake");
    let cache = build_dir.join("CMakeCache.txt");
    std::fs::create_dir_all(&build_dir)?;

    // A CMakeCache.txt written by a previous toolchain pins the old compiler
    // path (and the HIP paths derived from it): when the ROCm dist moves, the
    // cached entries poison the reconfigure. Only comparable when the config
    // carries a full path.
    let cached_cc = cache_get(&cache, "CMAKE_C_COMPILER");
    if !cached_cc.is_empty() && Path::new(&cfg.c_compiler).is_absolute() && !same_path(&cached_cc, &cfg.c_compiler) {
        ui::warn("Cached compiler changed:");
        ui::warn(format!("  {cached_cc} -> {}", cfg.c_compiler));
        ui::warn(format!("  wiping {} for a clean configure", build_dir.display()));
        util::remove_all(&build_dir)?;
        std::fs::create_dir_all(&build_dir)?;
    }
    // A cache written somewhere else (a moved build dir or repo) still names
    // its old location, and cmake refuses to configure it. Nothing in it is
    // reusable from the new path.
    let cached_dir = cache_get(&cache, "CMAKE_CACHEFILE_DIR");
    if !cached_dir.is_empty() && !same_path(&cached_dir, &build_dir.display().to_string()) {
        ui::warn("Build dir was relocated:");
        ui::warn(format!("  {cached_dir} -> {}", build_dir.display()));
        ui::warn(format!("  wiping {} for a clean configure", build_dir.display()));
        util::remove_all(&build_dir)?;
        std::fs::create_dir_all(&build_dir)?;
    }

    // FindOpenSSL caches what it found and reuses those paths without
    // checking they still sit under OPENSSL_ROOT_DIR: after OpenSSLDir moves
    // the configure "finds" the old one and fails on a missing OpenSSL::SSL
    // target (found 2026-10-13). Those entries are dropped then; no wipe.
    let openssl = cfg.openssl_dir.replace('\\', "/");
    let mut openssl_reset: Vec<String> = Vec::new();
    let cached_ssl = cache_get(&cache, "OPENSSL_INCLUDE_DIR");
    if !cached_ssl.is_empty() && !cached_ssl.to_ascii_lowercase().starts_with(&format!("{openssl}/").to_ascii_lowercase()) {
        ui::warn(format!("OpenSSL moved to {} (cache had {cached_ssl}): searching again", cfg.openssl_dir));
        openssl_reset = ["-UOPENSSL_INCLUDE_DIR", "-ULIB_EAY*", "-USSL_EAY*", "-UOPENSSL_APPLINK_SOURCE"].map(String::from).to_vec();
    }

    // ── Vulkan SDK: the newest installed version, pinned explicitly ────
    // VULKAN_SDK in a console opened before an upgrade, and the four paths
    // FindVulkan caches (reused WITHOUT checking the files still exist), both
    // keep pointing at an older SDK; so the newest of the layout is pinned
    // here on every configure. Not a wipe: only the Vulkan backend recompiles.
    let mut vulkan_args: Vec<String> = Vec::new();
    match dists.find_vulkan() {
        Some(vk) => {
            ui::head(format!("Vulkan SDK: {}", vk.display()));
            if let Some(old) = std::env::var_os("VULKAN_SDK").filter(|o| Path::new(o) != vk) {
                ui::dim(format!("  (VULKAN_SDK was '{}', overridden for this build)", old.to_string_lossy()));
            }
            std::env::set_var("VULKAN_SDK", &vk);
            let v = fwd(&vk);
            vulkan_args.extend([
                format!("-DVulkan_INCLUDE_DIR:PATH={v}/Include"),
                format!("-DVulkan_LIBRARY:FILEPATH={v}/Lib/vulkan-1.lib"),
                format!("-DVulkan_GLSLC_EXECUTABLE:FILEPATH={v}/Bin/glslc.exe"),
            ]);
            if vk.join(r"Bin\glslangValidator.exe").is_file() {
                vulkan_args.push(format!("-DVulkan_GLSLANG_VALIDATOR_EXECUTABLE:FILEPATH={v}/Bin/glslangValidator.exe"));
            }
        }
        None => ui::warn(format!(
            "Vulkan SDK: none under {}, leaving detection to CMake (scripts\\00-install-prerequisites.ps1 installs it)",
            dists.vulkan_root().display()
        )),
    }

    // ── CUDA toolkit: the newest installed version ─────────────────────
    // CMake picks nvcc once and caches ~40 toolkit paths next to it, so the
    // newest toolkit is pinned on every configure, by VERSION and never by
    // CUDA_PATH. A changed CMAKE_CUDA_COMPILER alone makes CMake delete the
    // WHOLE cache, so on a change only the CUDA half is reset (its
    // identification files and the toolkit's cache entries): only the CUDA
    // objects recompile.
    let mut cuda_args: Vec<String> = Vec::new();
    match dists.find_cuda() {
        Some(cuda) => {
            ui::head(format!("CUDA toolkit: {}", cuda.display()));
            if let Some(old) = std::env::var_os("CUDA_PATH").filter(|o| !same_path(&o.to_string_lossy(), &cuda.display().to_string())) {
                ui::dim(format!("  (CUDA_PATH was '{}', overridden for this build)", old.to_string_lossy()));
            }
            std::env::set_var("CUDA_PATH", &cuda);
            let nvcc = fwd(&cuda.join(r"bin\nvcc.exe"));
            let cached_nvcc = cache_get(&cache, "CMAKE_CUDA_COMPILER");
            if !cached_nvcc.is_empty() && !same_path(&cached_nvcc, &nvcc) {
                ui::warn("Cached CUDA toolkit changed:");
                ui::warn(format!("  {cached_nvcc} -> {nvcc}"));
                ui::warn("  resetting the CUDA detection only (the CUDA objects recompile, nothing else)");
                reset_cuda_detection(&build_dir)?;
                cuda_args.extend(
                    ["-UCUDA_*", "-UCUDAToolkit_*", "-U_cmake_CUDAToolkit_*", "-UFIND_PACKAGE_MESSAGE_DETAILS_CUDAToolkit"]
                        .map(String::from),
                );
            }
            cuda_args.push(format!("-DCMAKE_CUDA_COMPILER:FILEPATH={nvcc}"));
        }
        None => ui::warn(format!(
            "CUDA toolkit: none under {}, leaving detection to CMake (scripts\\00-install-prerequisites.ps1 installs it)",
            dists.cuda_root().display()
        )),
    }

    // ── sccache ────────────────────────────────────────────────────────
    // The copy scripts\00-install-prerequisites.ps1 keeps current comes first, from
    // its fixed directory: this path is part of every compile command via the
    // launcher, so it must not move between builds. PATH second.
    let fixed = std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join(r"Programs\sccache\sccache.exe"))
        .filter(|p| p.is_file());
    let sccache = fixed.or_else(|| util::which("sccache"));
    let daemon = match &sccache {
        Some(sc) => Some(SccacheDaemon::start(sc)?),
        None => {
            ui::dim("sccache not found, building without compiler cache");
            None
        }
    };

    // ── The configure line: checked-in options + this machine's paths ──
    let (generator, defines) = cmake_options()?;
    ui::dim(format!(
        "CMake options (xtask/cmake/): {}",
        defines.iter().map(|d| &d[2..]).collect::<Vec<_>>().join(" ")
    ));
    let mut args: Vec<String> = vec![
        "-S".into(),
        cfg.llama_cpp_dir.clone(),
        "-B".into(),
        build_dir.display().to_string(),
        "-G".into(),
        generator,
    ];
    args.extend(defines);
    args.extend([
        format!("-DGPU_TARGETS={}", cfg.gpu_targets),
        // hip-config-amd.cmake seeds GPU_BUILD_TARGETS from GPU_TARGETS with
        // a set(... CACHE ...), a no-op once cached: cleared so an edited
        // target list is re-derived on every configure.
        "-UGPU_BUILD_TARGETS".into(),
        format!("-DCMAKE_BUILD_TYPE={}", cfg.build_type),
        format!("-DCMAKE_C_COMPILER={}", cfg.c_compiler),
        format!("-DCMAKE_CXX_COMPILER={}", cfg.cxx_compiler),
        format!("-DOPENSSL_ROOT_DIR:PATH={openssl}"),
    ]);
    args.extend(vulkan_args);
    args.extend(openssl_reset);
    args.extend(cuda_args);

    // ── HIP and MSVC 14.51's <cmath> ───────────────────────────────────
    // Up to AMD clang 23 the stock __clang_hip_runtime_wrapper.h included
    // <cmath> before the HIP device math headers, and MSVC 14.51's constexpr
    // overloads clashed with the __device__ isgreater/isless declarations.
    // Clang 24 (TheRock ROCm 10.1.0) fixed it (llvm/llvm-project#201563), so
    // clang 24 is the floor: an older dist fails here, not at the first HIP
    // TU. CMAKE_CXX_FLAGS and CMAKE_HIP_FLAGS, both in the shared options
    // (xtask/cmake/common.toml), still replace the -include a cache built
    // with the old patched wrapper holds: CMake on Windows compiles the HIP
    // sources as CXX when enable_language(HIP) is not called.
    let hip = Path::new(&cfg.hip_path);
    let clang_major = [r"lib\llvm\lib\clang", r"lib\clang"]
        .iter()
        .filter_map(|res| std::fs::read_dir(hip.join(res)).ok())
        .find_map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_string_lossy().parse::<u32>().ok())
                .max()
        });
    let Some(clang_major) = clang_major else {
        bail!(
            "could not detect the ROCm clang resource version under {} (probed lib\\llvm\\lib\\clang and lib\\clang)",
            cfg.hip_path
        );
    };
    if clang_major < 24 {
        bail!(
            "ROCm clang {clang_major} under {} is below 24: its HIP runtime wrapper clashes with MSVC's <cmath>. Install a newer dist (scripts\\00-install-prerequisites.ps1 takes the newest stable) and re-run cargo run -p xtask -- configure.",
            cfg.hip_path
        );
    }

    // lld-link, when the compiler is an LLVM one that ships it (configure
    // leaves Linker empty otherwise, and an MSVC toolchain keeps link.exe).
    if !cfg.linker.is_empty() {
        args.push(format!("-DCMAKE_LINKER={}", cfg.linker));
    }
    // No linker flags of our own, and -U so none survive from an experiment:
    // the three variables are language-agnostic and also reach the CUDA
    // target, linked through `cmake -E vs_link_dll` with lld-link called
    // directly, where a `-Wl,` flag is an unknown argument. lld-link's ThinLTO
    // backend already runs one thread per physical core.
    args.extend(
        ["-UCMAKE_EXE_LINKER_FLAGS", "-UCMAKE_SHARED_LINKER_FLAGS", "-UCMAKE_MODULE_LINKER_FLAGS"].map(String::from),
    );

    match &sccache {
        Some(sc) => {
            // HIP is wrapped by the CXX launcher (compiled as CXX here, clang
            // with `-x hip`). nvcc is intentionally NOT wrapped: sccache
            // treats nvcc's output-less `ptxas -arch=compute_90` syntax check
            // as a cacheable step and every .cu dies with `Missing "cubin"
            // file output` (sccache PR #2809, open). -U clears a stale CUDA
            // launcher an earlier run left in the cache.
            args.extend([
                format!("-DCMAKE_C_COMPILER_LAUNCHER={}", sc.display()),
                format!("-DCMAKE_CXX_COMPILER_LAUNCHER={}", sc.display()),
                "-UCMAKE_CUDA_COMPILER_LAUNCHER".into(),
            ]);
        }
        None => args.extend(
            [
                "-UCMAKE_C_COMPILER_LAUNCHER",
                "-UCMAKE_CXX_COMPILER_LAUNCHER",
                "-UCMAKE_HIP_COMPILER_LAUNCHER",
                "-UCMAKE_CUDA_COMPILER_LAUNCHER",
            ]
            .map(String::from),
        ),
    }

    ui::head("Configuring...");
    run_cmd(cmd("cmake").args(&args))?;

    ui::head("Building...");
    let mut b = cmd("cmake");
    b.arg("--build").arg(&build_dir).arg("-j");
    if cfg.build_jobs > 0 {
        b.arg(cfg.build_jobs.to_string());
    }
    run_cmd(&mut b)?;
    drop(daemon);

    check_web_ui(&build_dir, &llama)?;

    ui::ok(format!("llama.cpp build complete: {}\\", build_dir.join("bin").display()));
    Ok(())
}
