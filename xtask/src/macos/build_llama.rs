//! `build llama` on macOS (cargo run -p xtask -- build, first half): llama.cpp with Metal and
//! Vulkan (MoltenVK), Apple's clang + Ninja. CUDA and HIP are switched off by
//! xtask/cmake/macos.toml. The checkout, the patches, the options
//! file, sccache and the web UI check are shared (common/llama.rs).

use super::config::{self, BuildConfig};
use crate::common::dists::Dists;
use crate::common::cmake_options::cmake_options;
use crate::common::llama::{cache_get, check_web_ui, checkout, print_version, SccacheDaemon};
use crate::common::ui;
use crate::common::util::{self, cmd, run as run_cmd};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The config and the dists of its recorded root.
pub fn load_build_env() -> Result<(BuildConfig, Dists)> {
    let cfg = config::load()?;
    let dists = Dists::resolve(Some(Path::new(&cfg.dist_root)).filter(|p| !p.as_os_str().is_empty()), None)?;
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

    // A new Xcode / Command Line Tools can move the compiler: start clean.
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
        format!("-DCMAKE_BUILD_TYPE={}", cfg.build_type),
        format!("-DCMAKE_C_COMPILER={}", cfg.c_compiler),
        format!("-DCMAKE_CXX_COMPILER={}", cfg.cxx_compiler),
    ]);

    // ── Vulkan SDK: the newest installed version, pinned explicitly ────
    // FindVulkan caches these paths and reuses them without checking the
    // files still exist, so they are passed on every configure. The build
    // tree's RPATH names the SDK's lib/, so the binaries run from here; the
    // package phase rewrites it.
    match dists.find_vulkan() {
        Some(vk) => {
            ui::head(format!("Vulkan SDK: {}", vk.display()));
            std::env::set_var("VULKAN_SDK", &vk);
            util::path_prepend(&vk.join("bin"), false);
            args.extend([
                format!("-DVulkan_INCLUDE_DIR:PATH={}", vk.join("include").display()),
                format!("-DVulkan_LIBRARY:FILEPATH={}", vk.join("lib/libvulkan.1.dylib").display()),
                format!("-DVulkan_GLSLC_EXECUTABLE:FILEPATH={}", vk.join("bin/glslc").display()),
            ]);
            let validator = vk.join("bin/glslangValidator");
            if util::is_executable(&validator) {
                args.push(format!("-DVulkan_GLSLANG_VALIDATOR_EXECUTABLE:FILEPATH={}", validator.display()));
            }
        }
        None => ui::warn(format!(
            "Vulkan SDK: none under {}, leaving detection to CMake (scripts/00-install-prerequisites.sh installs it)",
            dists.vulkan_root().display()
        )),
    }

    // ── sccache ────────────────────────────────────────────────────────
    // C and C++ only: the Metal backend's Objective-C sources are compiled
    // uncached (-U clears a launcher an experiment may have left).
    let home_sccache = util::home().join(".local/bin/sccache");
    let sccache = if util::is_executable(&home_sccache) { Some(home_sccache) } else { util::which("sccache") };
    let mut daemon = None;
    match &sccache {
        Some(sc) => {
            daemon = Some(SccacheDaemon::start(sc)?);
            args.extend([
                format!("-DCMAKE_C_COMPILER_LAUNCHER={}", sc.display()),
                format!("-DCMAKE_CXX_COMPILER_LAUNCHER={}", sc.display()),
                "-UCMAKE_OBJC_COMPILER_LAUNCHER".into(),
            ]);
        }
        None => {
            ui::dim("sccache not found, building without compiler cache");
            args.extend(
                ["-UCMAKE_C_COMPILER_LAUNCHER", "-UCMAKE_CXX_COMPILER_LAUNCHER", "-UCMAKE_OBJC_COMPILER_LAUNCHER"]
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
    print_version(&build_dir.join("bin/llama-server"), true).context("the freshly built llama-server does not start")?;
    ui::say("");
    Ok(())
}
