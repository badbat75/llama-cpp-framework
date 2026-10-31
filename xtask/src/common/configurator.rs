//! `build config` (02-build, second half): llama-cpp-config, the Rust GUI +
//! CLI configurator, built with cargo. A module of its own: it needs nothing
//! from the llama.cpp build but the clone (for the icon). What follows the
//! cargo build is the OS's: Linux and macOS copy the binary next to
//! llama-server (`install_next_to_server`), Windows stages it straight from
//! cargo's release dir in the package phase.

use crate::common::llama::prereq_script;
use crate::common::ui;
use crate::common::util::{self, cmd, run as run_cmd};
use anyhow::{Context, Result};
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

/// Builds llama-cpp-config in release mode and returns cargo's target dir.
/// `llama_cpp_dir` is the clone the icon is drawn from; `build_jobs` caps
/// cargo (0 = cargo's default, all cores).
pub fn build(llama_cpp_dir: &str, build_jobs: u32) -> Result<PathBuf> {
    let repo = util::repo_root();

    ui::say("");
    ui::warn("  === llama-cpp-config (Rust) ===");

    // The cargo that runs xtask (cargo run sets CARGO), else PATH, else where
    // rustup installs it: only a NEW login shell has ~/.cargo/bin on PATH.
    let home_cargo = util::home().join(".cargo").join("bin").join(if cfg!(windows) { "cargo.exe" } else { "cargo" });
    let cargo = std::env::var_os("CARGO")
        .map(PathBuf::from)
        .or_else(|| util::which("cargo"))
        .or_else(|| util::is_executable(&home_cargo).then_some(home_cargo))
        .with_context(|| {
            format!("cargo not found. Run {} (it installs Rust through rustup).", prereq_script())
        })?;
    if let Some(d) = cargo.parent() {
        util::path_prepend(d, false);
    }
    ui::dim(format!("cargo: {}", cargo.display()));

    // The icon is generated, not checked in (build.rs bootstraps it the same
    // way, but from the default clone location, build/<os>/llama.cpp): done
    // here first, from the configured clone. sharp ships one native package
    // per platform, so the npm install also completes a node_modules that the
    // other OS created. npm is npm.cmd on Windows, hence the PATH lookup.
    let ico = repo.join("resources").join("llama.ico");
    if !ico.is_file() {
        ui::head("Generating resources/llama.ico from the llama.cpp webui logo...");
        let resources = repo.join("resources");
        let npm = util::which("npm").context("npm not found: node + npm generate resources/llama.ico")?;
        run_cmd(
            cmd(npm)
                .current_dir(&resources)
                .args(["install", "--no-save", "--silent", "sharp", "sharp-ico"]),
        )?;
        run_cmd(
            cmd("node")
                .current_dir(&resources)
                .arg("generate-llama-ico.mjs")
                .arg(&llama_cpp_dir)
                .arg(&ico),
        )?;
    }

    // Its own target dir under build/<os>, never the other OS's. Cap cargo
    // at the C++ build's job count (0 = cargo's default, all cores).
    let target = util::build_root().join("cargo-target");
    std::env::set_var("CARGO_TARGET_DIR", &target);
    let mut c = cmd(&cargo);
    c.current_dir(&repo).args(["build", "--release", "-p", "llama-cpp-config"]);
    if build_jobs > 0 {
        c.arg("--jobs").arg(build_jobs.to_string());
    }
    run_cmd(&mut c)?;

    Ok(target)
}

/// Copies the build next to llama-server (Linux, macOS).
#[cfg(unix)]
pub fn install_next_to_server(target: &Path) -> Result<()> {
    let bin_dir = util::build_root().join("llama.cpp-cmake/bin");

    // Copied next to llama-server: the configurator looks for llama.cpp's
    // binaries in its OWN directory first (the installer layout on Windows),
    // ahead of its dev-layout fallbacks, so this copy is the one to launch.
    // Staged beside the target and renamed over it, as install(1) does, so a
    // configurator that is running keeps its file.
    std::fs::create_dir_all(&bin_dir)?;
    let dest = bin_dir.join("llama-cpp-config");
    let staged = bin_dir.join(".llama-cpp-config.new");
    std::fs::copy(target.join("release/llama-cpp-config"), &staged)?;
    set_mode(&staged, 0o755)?;
    std::fs::rename(&staged, &dest)?;
    ui::ok(format!("llama-cpp-config build complete: {}", dest.display()));
    crate::common::llama::print_version(&dest, false)?;
    ui::say("");
    Ok(())
}

#[cfg(unix)]
pub fn set_mode(p: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("chmod {mode:o} {}", p.display()))
}
