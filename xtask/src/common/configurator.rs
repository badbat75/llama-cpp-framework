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

/// The npm commands `build` runs in `fe/` before cargo: a clean install from
/// the committed lock, then Vite's build into `fe/dist`, which pai-bff embeds.
pub const FE_STEPS: &[&[&str]] = &[&["ci"], &["run", "build"]];

/// Builds the web UI, then pai-config, pai, pai-manager and pai-bff in release
/// mode and returns cargo's target dir.
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

    // The web UI pai-bff embeds (its build script reads fe/dist), built
    // first. npm is npm.cmd on Windows, hence the PATH lookup.
    ui::head("Building the web UI (fe/)...");
    let fe = repo.join("fe");
    let npm = util::which("npm").context("npm not found: node + npm build the web UI in fe/")?;
    for step in FE_STEPS {
        run_cmd(cmd(&npm).current_dir(&fe).args(*step))?;
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
        "-p",
        "pai-bff",
    ]);
    // Its build script generates the architecture tables from this clone.
    c.env("PRIVATE_AI_LLAMA_CPP_DIR", llama_cpp_dir);
    // pai-bff embeds the web UI just built, never its test fixture.
    c.env("PRIVATE_AI_FE_DIST", fe.join("dist"));
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
    use super::{FE_STEPS, NEXT_TO_SERVER};

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

    /// The web UI is built before the bff that embeds it: a clean install
    /// from the committed lock (`npm ci`, which refuses a lock out of step
    /// with package.json), then Vite's build into `fe/dist`.
    #[test]
    fn the_web_ui_is_built_with_npm_ci_then_the_build_script() {
        assert_eq!(FE_STEPS, [&["ci"][..], &["run", "build"][..]]);
    }

    /// The steps run in `fe/`, through the PATH's npm (npm.cmd on Windows),
    /// and before cargo builds pai-bff, which embeds `fe/dist`.
    #[test]
    fn the_build_runs_the_web_ui_steps_in_fe_before_cargo() {
        let code = production();
        let steps = code
            .find("in FE_STEPS")
            .expect("build does not run FE_STEPS");
        let fe_dir = code.find("join(\"fe\")").expect("no fe/ directory");
        let cargo = code
            .find("\"build\",\n        \"--release\"")
            .expect("the cargo build");
        assert!(steps < cargo, "FE_STEPS run after the cargo build");
        assert!(fe_dir < cargo, "fe/ is named only after the cargo build");
    }

    /// `xtask build` compiles the bff with the other workspace programs.
    #[test]
    fn the_build_compiles_the_bff() {
        let code = production();
        let cargo = code
            .find("\"build\",\n        \"--release\"")
            .expect("the cargo build");
        assert!(
            code[cargo..].contains("\"pai-bff\""),
            "common/configurator.rs does not build -p pai-bff"
        );
    }

    /// The cargo build is told where the web UI is, absolute, so pai-bff
    /// embeds `fe/dist` and never its test fixture by accident: the value is
    /// `fe.join("dist")`, set after FE_STEPS and before cargo runs.
    #[test]
    fn the_bff_is_given_the_web_ui_it_embeds() {
        let code = production();
        let at = code
            .find("\"PRIVATE_AI_FE_DIST\"")
            .expect("build does not set PRIVATE_AI_FE_DIST");
        let statement = &code[at..at + code[at..].find(';').expect("end of statement")];
        assert!(
            statement.contains("fe.join(\"dist\")")
                || statement.contains("repo.join(\"fe\").join(\"dist\")"),
            "PRIVATE_AI_FE_DIST is not <repo>/fe/dist: {statement}"
        );
        let steps = code.find("in FE_STEPS").expect("FE_STEPS");
        let run = code.find("run_cmd(&mut c)").expect("the cargo run");
        assert!(steps < at && at < run, "set outside FE_STEPS..cargo");
        let let_fe = code.find("let fe = repo.join(\"fe\")").expect("let fe");
        assert!(let_fe < at, "fe is not <repo>/fe");
    }

    /// The first report call, `rows.opt(` or `rows.miss(`, after the first
    /// `"<tool>"` in a configure phase's source: how a missing tool is
    /// reported (`miss` is a gap that fails the phase, `opt` a warning).
    fn report_of(source: &str, tool: &str) -> Option<&'static str> {
        let from = source.find(&format!("\"{tool}\""))?;
        let rest = &source[from..];
        let opt = rest.find("rows.opt(");
        let miss = rest.find("rows.miss(");
        match (opt, miss) {
            (Some(o), Some(m)) if o < m => Some("opt"),
            (_, Some(_)) => Some("miss"),
            (Some(_), None) => Some("opt"),
            (None, None) => None,
        }
    }

    /// `xtask build` builds the web UI with npm on every OS (FE_STEPS) and
    /// fails without it, so `xtask configure` reports node and npm as
    /// required on every OS, a gap that fails the phase like cmake, not an
    /// optional warning; the old "chat UI falls back" hint is gone (it was
    /// about llama.cpp's own UI). Windows checked neither before.
    #[test]
    fn configure_requires_node_and_npm_on_every_os() {
        for (name, source) in [
            ("linux/configure.rs", include_str!("../linux/configure.rs")),
            ("macos/configure.rs", include_str!("../macos/configure.rs")),
            (
                "windows/configure.rs",
                include_str!("../windows/configure.rs"),
            ),
        ] {
            for tool in ["node", "npm"] {
                assert_eq!(
                    report_of(source, tool),
                    Some("miss"),
                    "{name}: a missing {tool} is not a required gap"
                );
            }
            assert!(
                !source.contains("falls back to the prebuilt download"),
                "{name} still calls node and npm optional"
            );
        }
    }

    #[test]
    fn report_of_reads_the_first_report_after_the_tool() {
        let opt = r#"for tool in ["node"] { match which(tool) { Some(p) => rows.ok(tool, p), None => rows.opt(tool, "x") } } rows.miss("cargo", "y")"#;
        assert_eq!(report_of(opt, "node"), Some("opt"));
        let miss = r#"for (tool, hint) in [("node", "a"), ("npm", "b")] { match find(tool) { Some(p) => rows.ok(tool, p), None => rows.miss(tool, hint) } }"#;
        assert_eq!(report_of(miss, "npm"), Some("miss"));
        assert_eq!(report_of(miss, "git"), None);
    }

    /// The Windows bootstrap installs what `xtask build` needs for the web
    /// UI: Node.js LTS (which brings npm) through winget, in the same list
    /// as its other winget packages, so it is installed when missing and
    /// upgraded with them. The id is the winget-pkgs manifest's
    /// PackageIdentifier (manifests/o/OpenJS/NodeJS/LTS: `OpenJS.NodeJS.LTS`,
    /// "Node.js (LTS)").
    #[test]
    fn the_windows_bootstrap_installs_node_lts_with_winget() {
        let script = include_str!("../../../scripts/00-install-prerequisites.ps1");
        let list = script
            .split("$wingetPackages = @(")
            .nth(1)
            .and_then(|rest| rest.split("\n)").next())
            .expect("the $wingetPackages list");
        assert!(
            list.lines()
                .any(|l| l.contains("Id = 'OpenJS.NodeJS.LTS'") && l.contains("Name = ")),
            "Node.js LTS is not in $wingetPackages:\n{list}"
        );
    }
}
