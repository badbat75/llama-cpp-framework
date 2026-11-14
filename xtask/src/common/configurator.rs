//! `build config` (02-build, second half): pai-config (the GUI), pai (the
//! command line) and pai-manager, built with cargo. A module of its own: it
//! needs nothing from the llama.cpp build but the clone (for the icon). What
//! follows the cargo build is the OS's: Linux and macOS copy the front ends
//! next to llama-server (`install_next_to_server`), Windows stages them
//! straight from cargo's release dir in the package phase.

use crate::common::llama::prereq_script;
use crate::common::ui;
use crate::common::util::{self, cmd, run as run_cmd};
use anyhow::{Context, Result};
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

/// The programs of the cargo workspace copied next to llama-server (Linux,
/// macOS), where the package phases take them from.
#[cfg_attr(not(unix), allow(dead_code))]
pub const NEXT_TO_SERVER: &[&str] = &["pai-config", "pai"];

/// Builds pai-config, pai and pai-manager in release mode and returns cargo's
/// target dir.
/// `llama_cpp_dir` is the clone the architecture tables come from; `build_jobs` caps
/// cargo (0 = cargo's default, all cores).
pub fn build(llama_cpp_dir: &str, build_jobs: u32) -> Result<PathBuf> {
    let repo = util::repo_root();

    ui::say("");
    ui::warn("  === pai-config (Rust) ===");

    // The cargo that runs xtask (cargo run sets CARGO), else PATH, else where
    // rustup installs it: only a NEW login shell has ~/.cargo/bin on PATH.
    let home_cargo = util::home()
        .join(".cargo")
        .join("bin")
        .join(if cfg!(windows) { "cargo.exe" } else { "cargo" });
    let cargo = std::env::var_os("CARGO")
        .map(PathBuf::from)
        .or_else(|| util::which("cargo"))
        .or_else(|| util::is_executable(&home_cargo).then_some(home_cargo))
        .with_context(|| {
            format!(
                "cargo not found. Run {} (it installs Rust through rustup).",
                prereq_script()
            )
        })?;
    if let Some(d) = cargo.parent() {
        util::path_prepend(d, false);
    }
    ui::dim(format!("cargo: {}", cargo.display()));

    // The icon is generated, not checked in (build.rs bootstraps it the same
    // way): done here first, so the packaged binaries ship it. sharp ships
    // one native package per platform, so the npm install also completes a
    // node_modules that the other OS created. npm is npm.cmd on Windows,
    // hence the PATH lookup.
    let ico = repo.join("resources").join("llama.ico");
    if !ico.is_file() {
        ui::head("Generating resources/llama.ico from the llama-brand icon...");
        let resources = repo.join("resources");
        let npm =
            util::which("npm").context("npm not found: node + npm generate resources/llama.ico")?;
        run_cmd(cmd(npm).current_dir(&resources).args([
            "install",
            "--no-save",
            "--silent",
            "sharp",
            "sharp-ico",
        ]))?;
        run_cmd(
            cmd("node")
                .current_dir(&resources)
                .arg("generate-llama-ico.mjs")
                .arg(&ico),
        )?;
    }

    // Its own target dir under build/<os>, never the other OS's. Cap cargo
    // at the C++ build's job count (0 = cargo's default, all cores).
    let target = util::build_root().join("cargo-target");
    std::env::set_var("CARGO_TARGET_DIR", &target);
    let mut c = cmd(&cargo);
    c.current_dir(&repo).args([
        "build",
        "--release",
        "-p",
        "pai-config",
        "-p",
        "pai-cli",
        "-p",
        "pai-manager",
    ]);
    // Its build script generates the architecture tables from this clone.
    c.env("PRIVATE_AI_LLAMA_CPP_DIR", llama_cpp_dir);
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
    for name in NEXT_TO_SERVER {
        let dest = bin_dir.join(name);
        let staged = bin_dir.join(format!(".{name}.new"));
        std::fs::copy(target.join("release").join(name), &staged)?;
        set_mode(&staged, 0o755)?;
        std::fs::rename(&staged, &dest)?;
        ui::ok(format!("{name} build complete: {}", dest.display()));
        crate::common::llama::print_version(&dest, false)?;
    }
    ui::say("");
    Ok(())
}

#[cfg(unix)]
pub fn set_mode(p: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("chmod {mode:o} {}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::NEXT_TO_SERVER;

    /// This file's code without its tests, which spell the names they look for.
    fn production() -> String {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/common/configurator.rs"),
        )
        .unwrap();
        text.split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap()
            .to_string()
    }

    /// `xtask build` compiles `pai` (package pai-cli) with the GUI and the
    /// manager, on every OS: the package phases find it in cargo's release
    /// dir.
    #[test]
    fn the_build_compiles_the_command_line_on_every_os() {
        let code = production();
        assert!(
            code.contains("\"pai-cli\""),
            "common/configurator.rs does not build -p pai-cli"
        );
        for kept in ["\"pai-config\"", "\"pai-manager\""] {
            assert!(code.contains(kept), "{kept} no longer built");
        }
    }

    /// `pai` is copied next to llama-server with the GUI, where the Linux and
    /// macOS package phases stage the build's `bin/` from; the copy goes
    /// through the list, not one hard-coded configurator.
    #[test]
    fn the_command_line_is_copied_next_to_llama_server() {
        assert_eq!(NEXT_TO_SERVER, ["pai-config", "pai"]);
        let code = production();
        assert!(
            code.matches("NEXT_TO_SERVER").count() >= 2,
            "install_next_to_server does not copy NEXT_TO_SERVER"
        );
    }
}
