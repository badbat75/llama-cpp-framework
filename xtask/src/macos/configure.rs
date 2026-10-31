//! Configure phase on macOS (cargo run -p xtask -- configure): detects paths and tools and
//! writes build/macos/config-build.toml. The Vulkan SDK is not recorded: the
//! build finds the newest one on every run. The dist root in use is
//! RECORDED, so build, package and the next configure use it.

use super::config::{self, BuildConfig};
use crate::common::dists::Dists;
use crate::common::report::Rows;
use crate::common::ui;
use crate::common::util::{self, cmd, output, which};
use anyhow::{bail, Result};
use std::path::PathBuf;

pub fn run(llama_cpp_dir: Option<PathBuf>, dist_root: Option<PathBuf>) -> Result<()> {
    if std::env::consts::ARCH != "aarch64" {
        bail!("the macOS build is Apple Silicon only (this machine is {})", std::env::consts::ARCH);
    }
    let recorded = config::load_optional().ok().flatten();
    let dists = Dists::resolve(dist_root.as_deref(), recorded.as_ref().map(|c| c.dist_root.as_str()))?;
    let llama_dir = util::absolute(&llama_cpp_dir.unwrap_or_else(|| util::build_root().join("llama.cpp")));
    let mut rows = Rows { gaps: Vec::new() };

    ui::say("");
    ui::head("  llama.cpp-framework: Environment Check (macOS)");
    ui::head("  ==============================================");
    ui::say("");

    rows.ok("LlamaCppDir", llama_dir.display().to_string());
    rows.ok("DistRoot", format!("{} (<Name>/<Version>/{})", dists.root.display(), util::OS_DIR));
    match dists.find_vulkan() {
        Some(d) => rows.ok("Vulkan SDK", d.display().to_string()),
        None => rows.opt("Vulkan SDK", format!("none under {} (needed for Vulkan/MoltenVK)", dists.vulkan_root().display())),
    }

    ui::say("");
    ui::head("  Tools");
    ui::head("  -----");

    // Apple's clang, from the Command Line Tools (or Xcode): full paths, as
    // on the other systems, so the config names the compiler it was made for.
    let xcrun = |tool: &str| output(cmd("xcrun").args(["--find", tool])).map(PathBuf::from).filter(|p| p.is_file());
    let (cc, cxx) = (xcrun("clang"), xcrun("clang++"));
    match (&cc, &cxx) {
        (Some(c), Some(_)) => rows.ok("clang", c.display().to_string()),
        _ => rows.miss("clang", "install the Xcode Command Line Tools: xcode-select --install"),
    }
    for tool in ["git", "cmake", "ninja"] {
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
    let home_cargo = util::home().join(".cargo/bin/cargo");
    match which("cargo").or_else(|| util::is_executable(&home_cargo).then_some(home_cargo)) {
        Some(p) => rows.ok("cargo", p.display().to_string()),
        None => rows.miss("cargo", "run scripts/00-install-prerequisites.sh"),
    }
    // pkgbuild ships with macOS: only the package phase needs it.
    match which("pkgbuild") {
        Some(p) => rows.ok("pkgbuild", p.display().to_string()),
        None => rows.opt("pkgbuild", "not found (needed by cargo run -p xtask -- package only)"),
    }

    rows.summary();

    let path = |p: &Option<PathBuf>, bare: &str| p.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| bare.into());
    BuildConfig {
        dist_root: dists.root.display().to_string(),
        llama_cpp_dir: llama_dir.display().to_string(),
        build_type: "Release".into(),
        c_compiler: path(&cc, "clang"),
        cxx_compiler: path(&cxx, "clang++"),
        build_jobs: util::default_build_jobs(),
    }
    .save()?;
    ui::ok("  build/macos/config-build.toml written.");
    ui::say("");

    if !rows.gaps.is_empty() {
        bail!("{} required tool(s) missing", rows.gaps.len());
    }
    Ok(())
}
