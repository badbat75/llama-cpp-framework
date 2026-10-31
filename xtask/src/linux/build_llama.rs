//! `build llama` on Linux (cargo run -p xtask -- build, first half): llama.cpp with CUDA +
//! Vulkan + HIP (ROCm), the ROCm dist's clang + lld + Ninja. The checkout, the
//! patches, xtask/cmake/common.toml, sccache and the web UI check are shared
//! (common/llama.rs).

use super::config::{self, BuildConfig};
use super::dists::vulkan_loader;
use crate::common::dists::Dists;
use crate::common::cmake_options::cmake_options;
use crate::common::llama::{cache_get, check_web_ui, checkout, print_version, reset_cuda_detection, SccacheDaemon};
use crate::common::ui;
use crate::common::util::{self, cmd, run as run_cmd};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Loads the config and sets the per-process ROCm environment. ROCM_PATH is
/// what llama.cpp's ggml-hip CMakeLists reads to find the hip / hipblas /
/// rocblas packages (it falls back to /opt/rocm, then /usr).
/// HIP_DEVICE_LIB_PATH points clang at the device bitcode, which TheRock
/// keeps under lib/llvm/amdgcn/bitcode rather than the <rocm>/amdgcn/bitcode
/// layout clang derives from --rocm-path. Process scope only, as on Windows.
pub fn load_build_env() -> Result<(BuildConfig, Dists)> {
    let cfg = config::load()?;
    // The root the configure phase recorded, so the CUDA Toolkit and the
    // Vulkan SDK resolved at build time come from the same root as hip_path.
    let dists = Dists::resolve(Some(Path::new(&cfg.dist_root)).filter(|p| !p.as_os_str().is_empty()), None)?;
    let hip = Path::new(&cfg.hip_path);
    if !cfg.hip_path.is_empty() && hip.is_dir() {
        std::env::set_var("HIP_PATH", hip);
        std::env::set_var("ROCM_PATH", hip);
        let llvm_bin = hip.join("lib/llvm/bin");
        if llvm_bin.is_dir() {
            util::path_prepend(&llvm_bin, true);
        }
        util::path_prepend(&hip.join("bin"), true);
        let bitcode = hip.join("lib/llvm/amdgcn/bitcode");
        if bitcode.is_dir() {
            std::env::set_var("HIP_DEVICE_LIB_PATH", bitcode);
            std::env::set_var("HIP_PLATFORM", "amd");
        }
    }
    Ok((cfg, dists))
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

    // A cache written by a previous toolchain pins the old compiler path and
    // the HIP paths derived from it: when the ROCm dist moves, start clean.
    let cached_cc = cache_get(&cache, "CMAKE_C_COMPILER");
    if !cached_cc.is_empty() && cached_cc != cfg.c_compiler {
        ui::warn("Cached compiler changed:");
        ui::warn(format!("  {cached_cc} -> {}", cfg.c_compiler));
        ui::warn(format!("  wiping {} for a clean configure", build_dir.display()));
        util::remove_all(&build_dir)?;
        std::fs::create_dir_all(&build_dir)?;
    }

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
    ]);

    // lld by FLAG (-fuse-ld=lld in the three linker-flag variables), not
    // CMAKE_LINKER as on Windows: the GNU-style drivers pick their linker by
    // flag. Not CMAKE_LINKER_TYPE=LLD either: CMake 4.2 defines no linker
    // types for the HIP language, and the configure dies in the HIP compiler
    // check with "LINKER_TYPE 'LLD' is unknown or not supported by this
    // toolchain". The flag reaches all four link drivers (clang for C/C++/
    // HIP, g++ for CUDA, which finds ld.lld through the PATH set above).
    // These are the only linker flags the build passes, and they are always
    // passed, blank included, so none survives in the cache from an
    // experiment or from a config that has since lost its lld; -U retires the
    // cached linker type of a build dir configured before this was changed.
    let ld_flags = if cfg.linker_type == "LLD" { "-fuse-ld=lld" } else { "" };
    args.extend([
        format!("-DCMAKE_EXE_LINKER_FLAGS={ld_flags}"),
        format!("-DCMAKE_SHARED_LINKER_FLAGS={ld_flags}"),
        format!("-DCMAKE_MODULE_LINKER_FLAGS={ld_flags}"),
        "-UCMAKE_LINKER_TYPE".into(),
    ]);

    // ── HIP ────────────────────────────────────────────────────────────
    // On Linux llama.cpp uses CMake's own HIP language (on Windows the HIP
    // sources are compiled as CXX), so the device compiler is named
    // separately: the dist's clang++, with the dist as its ROCm root (CMake
    // would otherwise look for one relative to the compiler and in
    // /opt/rocm). ROCM_PATH, set above, is what the ggml-hip CMakeLists
    // itself reads. None of the MSVC <cmath> header workaround of
    // the Windows build (windows/build_llama.rs) applies here.
    if cfg.hip_path.is_empty() {
        bail!("no ROCm dist configured (hip_path is empty) but xtask/cmake/common.toml builds GGML_HIP. Run scripts/00-install-prerequisites.sh, then cargo run -p xtask -- configure.");
    }
    let hip = Path::new(&cfg.hip_path);
    args.extend([
        format!("-DCMAKE_HIP_COMPILER={}", hip.join("lib/llvm/bin/clang++").display()),
        format!("-DCMAKE_HIP_COMPILER_ROCM_ROOT={}", cfg.hip_path),
    ]);
    // RCCL, AMD's NCCL: the collective library `--split-mode tensor` reduces
    // through between two or more ROCm devices. Linux only (the Windows dist
    // ships none, so there the reduction is ggml's internal AllReduce, staged
    // through pinned host memory), hence passed here and not in the shared
    // xtask/cmake/common.toml. Upstream leaves it OFF because it "was not
    // universally beneficial during testing"; it is a default, not a lock:
    // GGML_CUDA_ALLREDUCE=internal in llama-server's environment selects the
    // internal AllReduce again at run time, with no rebuild. It costs nothing
    // outside the tensor mode: the communicators are created only when a
    // model loads with every device of the split on ROCm. Passed either way,
    // since a cached ON would outlive a dist that lost the library and fail
    // the configure in find_package(rccl REQUIRED). The package phase needs
    // no list for it: librccl.so and what it loads are in the ldd closure.
    if hip.join("lib/cmake/rccl/rccl-config.cmake").is_file() {
        args.push("-DGGML_HIP_RCCL=ON".into());
        ui::dim("RCCL: found in the ROCm dist, split-mode tensor reduces through it");
    } else {
        args.push("-DGGML_HIP_RCCL=OFF".into());
        ui::warn(format!(
            "RCCL: not in {}, split-mode tensor falls back to the internal AllReduce",
            cfg.hip_path
        ));
    }

    // ── Vulkan SDK: the newest installed version, pinned explicitly ────
    // FindVulkan caches these four paths and reuses them without checking
    // the files still exist, so they are passed on every configure and
    // follow the newest SDK. VULKAN_SDK is also what ggml-vulkan adds to
    // CMAKE_PREFIX_PATH.
    match dists.find_vulkan() {
        Some(vk) => {
            ui::head(format!("Vulkan SDK: {}", vk.display()));
            std::env::set_var("VULKAN_SDK", &vk);
            util::path_prepend(&vk.join("bin"), false);
            let loader = vulkan_loader(&vk).unwrap_or_default();
            args.extend([
                format!("-DVulkan_INCLUDE_DIR:PATH={}", vk.join("include").display()),
                format!("-DVulkan_LIBRARY:FILEPATH={}", loader.display()),
                format!("-DVulkan_GLSLC_EXECUTABLE:FILEPATH={}", vk.join("bin/glslc").display()),
            ]);
            let validator = vk.join("bin/glslangValidator");
            if util::is_executable(&validator) {
                args.push(format!("-DVulkan_GLSLANG_VALIDATOR_EXECUTABLE:FILEPATH={}", validator.display()));
            }
        }
        None => ui::warn(format!(
            "Vulkan SDK: none under {}, leaving detection to CMake",
            dists.vulkan_root().display()
        )),
    }

    // ── CUDA toolkit: the newest installed version ─────────────────────
    // CMake picks nvcc once and caches ~40 toolkit paths next to it, so the
    // newest toolkit is pinned on every configure. A changed
    // CMAKE_CUDA_COMPILER alone makes CMake delete the WHOLE cache, so on a
    // change only the CUDA half is reset: that language's identification
    // files and the toolkit's cache entries (the GGML_CUDA_* knobs survive).
    // Only the CUDA objects recompile.
    match dists.find_cuda() {
        Some(cuda) => {
            ui::head(format!("CUDA toolkit: {}", cuda.display()));
            std::env::set_var("CUDA_PATH", &cuda);
            let nvcc = cuda.join("bin/nvcc").display().to_string();
            let cached_nvcc = cache_get(&cache, "CMAKE_CUDA_COMPILER");
            if !cached_nvcc.is_empty() && cached_nvcc != nvcc {
                ui::warn("Cached CUDA toolkit changed:");
                ui::warn(format!("  {cached_nvcc} -> {nvcc}"));
                ui::warn("  resetting the CUDA detection only (the CUDA objects recompile, nothing else)");
                reset_cuda_detection(&build_dir)?;
                args.extend(
                    ["-UCUDA_*", "-UCUDAToolkit_*", "-U_cmake_CUDAToolkit_*", "-UFIND_PACKAGE_MESSAGE_DETAILS_CUDAToolkit"]
                        .map(String::from),
                );
            }
            args.extend([
                format!("-DCMAKE_CUDA_COMPILER:FILEPATH={nvcc}"),
                format!("-DCUDAToolkit_ROOT:PATH={}", cuda.display()),
                // The toolkit's library dir must be named as a build RPATH by
                // hand: it is one of nvcc's IMPLICIT link directories, and
                // CMake leaves those out of the RPATH it computes (a system
                // toolkit is found through ldconfig, a dist under /mnt/data is
                // not), so libggml-cuda.so would fail to load libcudart/
                // libcublas and the CUDA backend would silently be absent.
                format!("-DCMAKE_BUILD_RPATH={}", cuda.join("lib").display()),
            ]);
            // nvcc's host compiler: the distro g++ (see configure), not the
            // ROCm clang that compiles everything else.
            if !cfg.cuda_host_compiler.is_empty() {
                args.push(format!("-DCMAKE_CUDA_HOST_COMPILER:FILEPATH={}", cfg.cuda_host_compiler));
            }
        }
        None => {
            ui::warn(format!(
                "CUDA toolkit: none under {}, leaving detection to CMake",
                dists.cuda_root().display()
            ));
            args.push("-UCMAKE_BUILD_RPATH".into());
        }
    }

    // ── NCCL: the newest installed dist, pinned explicitly ─────────────
    // The CUDA counterpart of RCCL above: what `--split-mode tensor` reduces
    // through between two or more CUDA devices. GGML_CUDA_NCCL is ON by
    // upstream default and only needs the library to be FOUND, which ggml's
    // FindNCCL does from two cache paths; both are passed on every configure
    // so they follow the newest dist, and are cleared when there is none, so
    // a dist that was removed cannot leave the build pointing at it (the
    // backend then says "NCCL not found" and uses its internal AllReduce).
    // Like RCCL it is a default: GGML_CUDA_ALLREDUCE picks at run time.
    match dists.find_nccl() {
        Some(nccl) => {
            ui::head(format!("NCCL: {}", nccl.display()));
            args.extend([
                format!("-DNCCL_INCLUDE_DIR:PATH={}", nccl.join("include").display()),
                format!("-DNCCL_LIBRARY:FILEPATH={}", nccl.join("lib/libnccl.so").display()),
            ]);
        }
        None => {
            ui::dim(format!(
                "NCCL: none under {}, split-mode tensor between CUDA cards uses the internal AllReduce",
                dists.nccl_root().display()
            ));
            args.extend(["-UNCCL_INCLUDE_DIR", "-UNCCL_LIBRARY"].map(String::from));
        }
    }

    // ── sccache ────────────────────────────────────────────────────────
    // The fixed path the prerequisites install to comes first (it is part of
    // every compile command, so it must not move), PATH second. HIP is
    // wrapped too: it is clang with `-x hip`, which sccache caches as the HIP
    // language, --offload-arch list included (checked on 0.18.0: a repeat
    // compile of one ggml-cuda object was a cache hit). nvcc is intentionally
    // NOT wrapped, for the reason given in windows/build_llama.rs (the PTX-only
    // compute_90 step, pending sccache PR #2809); -U clears a stale launcher
    // left in the cache. Adding or removing a launcher changes every HIP
    // compile command, so the first build after the switch rebuilds all of
    // ggml-hip once.
    // The build gets its own daemon on SCCACHE_SERVER_PORT, started here with
    // the settings below: a daemon already running on the default port would
    // otherwise serve every compile with ITS cache dir and settings, which win
    // over the client's. A leftover on that port (a killed run) is stopped
    // first. Ours prints its stats and stops right after the build, or when
    // the configure or the build fails.
    let home_sccache = util::home().join(".local/bin/sccache");
    let sccache = if util::is_executable(&home_sccache) { Some(home_sccache) } else { util::which("sccache") };
    let mut daemon = None;
    match &sccache {
        Some(sc) => {
            daemon = Some(SccacheDaemon::start(sc)?);
            let s = sc.display();
            args.extend([
                format!("-DCMAKE_C_COMPILER_LAUNCHER={s}"),
                format!("-DCMAKE_CXX_COMPILER_LAUNCHER={s}"),
                format!("-DCMAKE_HIP_COMPILER_LAUNCHER={s}"),
                "-UCMAKE_CUDA_COMPILER_LAUNCHER".into(),
            ]);
        }
        None => {
            ui::dim("sccache not found, building without compiler cache");
            args.extend(
                [
                    "-UCMAKE_C_COMPILER_LAUNCHER",
                    "-UCMAKE_CXX_COMPILER_LAUNCHER",
                    "-UCMAKE_HIP_COMPILER_LAUNCHER",
                    "-UCMAKE_CUDA_COMPILER_LAUNCHER",
                ]
                .map(String::from),
            );
        }
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

    ui::say("");
    ui::ok(format!("llama.cpp build complete: {}/", build_dir.join("bin").display()));
    // The binaries carry a build-tree RPATH to the ROCm, CUDA and Vulkan
    // dists they were linked against, so they run from here with no
    // LD_LIBRARY_PATH.
    print_version(&build_dir.join("bin/llama-server"), true)
        .context("the freshly built llama-server does not start")?;
    ui::say("");
    Ok(())
}

