//! Configure phase on Windows (cargo run -p xtask -- configure): detects paths and tools and
//! writes build\windows\config-build.psd1 plus the generated
//! .cargo\config.toml. Run it after scripts\00-install-prerequisites.ps1 and again
//! whenever the ROCm dist changes (the compiler path and the GPU targets are
//! derived from it). The CUDA Toolkit and the Vulkan SDK are not recorded:
//! the build resolves the newest of each on every run, so an upgrade of either
//! needs no reconfigure. The dist root in use IS recorded, so build, package
//! and the next configure use it without being told again.

use super::WinConfig;
use crate::common::report::Rows;
use crate::common::rocm::{gpu_targets, GPU_TARGETS_FALLBACK};
use crate::common::dists::Dists;
use crate::common::ui;
use crate::common::util::{self, cmd, output, which};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Launch-VsDevShell.ps1 of the newest Visual Studio. vswhere first: it knows
/// real product versions, so "newest" is correct across the year-dir to
/// major-version-dir naming switch (2022 vs 18). Without vswhere, the known
/// roots are scanned, years mapped to their major version (2022 to 17, 2019
/// to 16, 2017 to 15) so the comparison is numeric.
fn find_vs_dev_shell() -> Option<PathBuf> {
    let rel = Path::new("Common7").join("Tools").join("Launch-VsDevShell.ps1");
    let pf86 = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from);
    if let Some(pf86) = &pf86 {
        let vswhere = pf86.join("Microsoft Visual Studio").join("Installer").join("vswhere.exe");
        if vswhere.is_file() {
            if let Some(install) = output(cmd(&vswhere).args(["-latest", "-property", "installationPath"])) {
                let script = Path::new(install.lines().next().unwrap_or("").trim()).join(&rel);
                if script.is_file() {
                    return Some(script);
                }
            }
        }
    }
    let roots = [std::env::var_os("ProgramFiles").map(PathBuf::from), pf86];
    for root in roots.iter().flatten().map(|r| r.join("Microsoft Visual Studio")) {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        let mut versions: Vec<(u32, PathBuf)> = rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let major = match name.as_str() {
                    "2022" => 17,
                    "2019" => 16,
                    "2017" => 15,
                    n => n.parse().unwrap_or(0),
                };
                (major, e.path())
            })
            .collect();
        versions.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, ver) in versions {
            for edition in ["Enterprise", "Professional", "Community", "BuildTools"] {
                let script = ver.join(edition).join(&rel);
                if script.is_file() {
                    return Some(script);
                }
            }
        }
    }
    None
}

/// OpenSSL is a system library, not a dist of the layout: it is looked up
/// where winget's ShiningLight.OpenSSL.Dev installs it, then beside an
/// openssl.exe on PATH.
fn find_openssl() -> Option<PathBuf> {
    let has_header = |d: &Path| d.join("include").join("openssl").join("ssl.h").is_file();
    let mut candidates: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432"]
        .iter()
        .filter_map(|v| std::env::var_os(v).map(|p| PathBuf::from(p).join("OpenSSL-Win64")))
        .collect();
    candidates.push(PathBuf::from(r"C:\OpenSSL-Win64"));
    if let Some(found) = candidates.into_iter().find(|d| has_header(d)) {
        return Some(found);
    }
    let dir = which("openssl")?.parent()?.parent()?.to_path_buf();
    has_header(&dir).then_some(dir)
}

/// A tool on PATH, else in the ROCm dist, which ships its own toolchain
/// (bin\ in the legacy HIP SDK, lib\llvm\bin in the TheRock dist).
fn find_tool(name: &str, hip: Option<&Path>) -> Option<PathBuf> {
    which(name).or_else(|| {
        let hip = hip?;
        ["bin", r"lib\llvm\bin"]
            .iter()
            .map(|sub| hip.join(sub).join(format!("{name}.exe")))
            .find(|p| p.is_file())
    })
}

pub fn run(llama_cpp_dir: Option<PathBuf>, dist_root: Option<PathBuf>) -> Result<()> {
    let recorded = WinConfig::load_optional().ok().flatten();
    let dists = Dists::resolve(dist_root.as_deref(), recorded.as_ref().map(|c| c.dist_root.as_str()))?;
    let mut rows = Rows { gaps: Vec::new() };

    ui::say("");
    ui::head("  llama.cpp-framework: Environment Check");
    ui::head("  ========================================");
    ui::say("");

    // ── Paths ──────────────────────────────────────────────────────────
    let vs = find_vs_dev_shell();
    match &vs {
        Some(p) => rows.ok("VsDevShell", p.display().to_string()),
        None => rows.miss("VsDevShell", "Install Visual Studio with the C++ workload"),
    }
    // Activated early, so the tool detection below (cmake, ninja) sees what
    // the build will see.
    if let Some(p) = &vs {
        super::activate_vs(&p.with_file_name("VsDevCmd.bat"))?;
    }

    rows.ok("DistRoot", format!("{} (<Name>\\<Version>\\{})", dists.root.display(), util::OS_DIR));

    let openssl = find_openssl();
    match &openssl {
        Some(p) => rows.ok("OpenSSLDir", p.display().to_string()),
        None => rows.miss("OpenSSLDir", "run scripts\\00-install-prerequisites.ps1 (winget ShiningLight.OpenSSL.Dev)"),
    }

    let hip = dists.find_rocm();
    match &hip {
        Some(p) => rows.ok("HipPath", p.display().to_string()),
        None => rows.opt(
            "HipPath",
            format!("not found under {} (optional, needed for HIP/ROCm)", dists.rocm_root().display()),
        ),
    }
    // Shown, not recorded (see the module header), so a missing one is seen
    // here rather than as a backend silently left to CMake's own search.
    match dists.find_cuda() {
        Some(d) => rows.ok("CUDA Toolkit", d.display().to_string()),
        None => rows.opt("CUDA Toolkit", format!("not found under {} (optional)", dists.cuda_root().display())),
    }
    match dists.find_vulkan() {
        Some(d) => rows.ok("Vulkan SDK", d.display().to_string()),
        None => rows.opt("Vulkan SDK", format!("not found under {} (optional)", dists.vulkan_root().display())),
    }

    let mut targets = hip.as_deref().map(gpu_targets).unwrap_or_default();
    if !targets.is_empty() {
        rows.ok("GpuTargets", format!("{} archs from the installed ROCm dist", targets.split(';').count()));
        ui::dim(format!("                         {targets}"));
    } else {
        targets = GPU_TARGETS_FALLBACK.to_string();
        rows.opt("GpuTargets", "no kernel list found in the dist, using the 7.14.0 snapshot");
    }

    let llama_dir = util::absolute(&llama_cpp_dir.unwrap_or_else(|| util::build_root().join("llama.cpp")));
    rows.ok("LlamaCppDir", llama_dir.display().to_string());

    // ── Tools (detected AFTER the VS environment) ──────────────────────
    ui::say("");
    ui::head("  Tools");
    ui::head("  -----");

    // The compiler: the ROCm dist's own clang BEFORE the PATH search. PATH
    // names whichever dist the machine env pointed at when the console
    // opened, possibly another one than the HipPath resolved here.
    let clang = hip
        .as_ref()
        .map(|h| h.join(r"lib\llvm\bin\clang.exe"))
        .filter(|p| p.is_file())
        .or_else(|| find_tool("clang", hip.as_deref()));
    for (tool, hint) in [
        ("cmake", "CMake, from https://cmake.org/download/"),
        ("ninja", "Ninja, via winget install Ninja-build.Ninja"),
        ("clang", "Clang, via the ROCm dist, Visual Studio or LLVM"),
        ("git", "Git, from https://git-scm.com/"),
    ] {
        let found = if tool == "clang" { clang.clone() } else { find_tool(tool, hip.as_deref()) };
        match found {
            Some(p) => rows.ok(tool, p.display().to_string()),
            None => rows.miss(tool, hint),
        }
    }

    // WiX (the package phase only: a gap for packaging, not for the build),
    // checked against installer\wix-toolset.psd1.
    let pin = super::wix_pin()?;
    let wix = super::find_wix();
    match &wix {
        Some(p) => match super::wix_version(p) {
            Some(v) if v == pin.version => rows.ok("wix", format!("{} ({v})", p.display())),
            v => rows.opt(
                "wix",
                format!(
                    "{} ({}, pinned {}: run scripts\\00-install-prerequisites.ps1)",
                    p.display(),
                    v.unwrap_or_else(|| "unknown".into()),
                    pin.version
                ),
            ),
        },
        None => rows.opt("wix", "not found (needed by cargo run -p xtask -- package only: run scripts\\00-install-prerequisites.ps1)"),
    }

    rows.summary();

    // Full path to the resolved clang (bare 'clang' as a last resort): with
    // the TheRock dist the compiler lives in HIP_PATH\lib\llvm\bin, which is
    // not on a stale console's PATH.
    let cc = clang.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "clang".into());
    // Linker: lld, but only the one shipping WITH the compiler above (looked
    // up next to it and nowhere else). CMake finds it unaided when clang comes
    // from a full LLVM tree, so this mostly PINS what happens; the point is
    // that the day it can't, the build does not silently fall back to
    // link.exe. Blank is a supported state (a bare-name clang, an MSVC one).
    let linker = clang
        .as_ref()
        .and_then(|c| c.parent())
        .map(|d| d.join("lld-link.exe"))
        .filter(|p| p.is_file())
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let jobs = util::default_build_jobs();

    WinConfig {
        dist_root: dists.root.display().to_string(),
        llama_cpp_dir: llama_dir.display().to_string(),
        openssl_dir: openssl.map(|p| p.display().to_string()).unwrap_or_default(),
        hip_path: hip.map(|p| p.display().to_string()).unwrap_or_default(),
        vs_dev_shell: vs.map(|p| p.display().to_string()).unwrap_or_default(),
        wix_exe: wix.map(|p| p.display().to_string()).unwrap_or_default(),
        gpu_targets: targets,
        build_type: "Release".into(),
        c_compiler: cc.clone(),
        cxx_compiler: cc,
        linker,
        build_jobs: jobs,
    }
    .save()?;
    ui::ok("  build\\windows\\config-build.psd1 written.");

    write_cargo_config(jobs)?;
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    ui::ok(format!(
        "  .cargo\\config.toml written (cargo jobs + test threads = {jobs} of {cores} cores; linker = rust-lld; target-dir = build\\windows\\cargo-target)."
    ));
    ui::dim("  Runtime / per-model settings are written on first launch (or by the installer).");
    ui::say("");

    // A failure when required tools are missing, so callers can script on
    // it; the config is still written above to ease fixing the gaps.
    if !rows.gaps.is_empty() {
        bail!("{} required tool(s) missing", rows.gaps.len());
    }
    Ok(())
}

/// The same job count for CARGO, as a config file rather than a flag, so a
/// bare `cargo build` / `cargo test` typed by hand or fired by an IDE obeys
/// the policy too. Machine-specific, hence generated and gitignored.
fn write_cargo_config(jobs: u32) -> Result<()> {
    let dir = util::repo_root().join(".cargo");
    std::fs::create_dir_all(&dir)?;
    let text = format!(
        r#"# Generated by cargo run -p xtask -- configure; do not edit (re-run it to refresh).
# Caps both of cargo's thread pools at the same 3/4-of-cores policy as
# config-build.psd1's BuildJobs, which is where the number comes from.

[build]
jobs = {jobs}
# The target dir the build sets through CARGO_TARGET_DIR, so a bare
# `cargo build` / `cargo test` reuses the same artifacts instead of filling
# target\ at the workspace root. Relative to the repo root (the parent of this
# .cargo\); the environment variable still wins where it is set.
target-dir = "build/windows/cargo-target"

# Exported to test binaries: the libtest harness would otherwise open one
# thread per logical core, independently of [build] jobs above.
[env]
RUST_TEST_THREADS = "{jobs}"
# Each test thread's stack: the e2e tests build AppWindow on one, and the
# 2 MiB default leaves under 1 MiB of headroom (build.rs gives the exe 8).
RUST_MIN_STACK = "8388608"

# Link with lld, always; unlike the C++ side (config-build.psd1 Linker),
# which only uses it when the compiler is an LLVM one, this needs no
# condition: rust-lld ships INSIDE the toolchain
# (<sysroot>\lib\rustlib\<target>\bin) and rustc resolves the bare name from
# there, not from PATH, so it is always present and always version-matched
# to rustc. Naming an external lld-link.exe instead would borrow whatever
# LLVM happens to be installed (here AMD LLD 23, from TheRock) and break on
# a machine without it. The default for x86_64-pc-windows-msvc is still
# MSVC link.exe; rustc only made rust-lld the default on linux-gnu.
[target.x86_64-pc-windows-msvc]
linker = "rust-lld.exe"

# /force:multiple is what lld costs here, and it is not optional: Skia links
# ICU STATICALLY (skunicode_icu.lib) while the windows crate imports the
# SYSTEM one (icuuc.dll), so every uloc_* symbol is defined twice. MSVC
# link.exe has always accepted that silently (LNK4006, second definition
# ignored); lld refuses it, and this flag restores link.exe's behaviour for
# exactly that case rather than inventing a new one.
#
# It only bites the TEST binary: the second ICU arrives through
# i-slint-backend-testing, a dev-dependency (-> i-slint-common -> fontique ->
# windows), so `cargo build` links with no duplicates at all and the flag is
# inert there. The cost to accept knowingly: rustflags are not per-profile,
# so a genuine duplicate symbol in the SHIPPED binary would also be accepted
# instead of caught. Drop this the day the two ICUs stop colliding.
rustflags = ["-Clink-arg=/force:multiple"]
"#
    );
    let p = dir.join("config.toml");
    std::fs::write(&p, text).with_context(|| format!("writing {}", p.display()))
}
