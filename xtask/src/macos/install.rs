//! Install phase on macOS (cargo run -p xtask -- install): installs the newest .pkg of each
//! component the package phase left in ./dist/ (or the ones given) with
//! `installer`, as root.
//! A first install, an upgrade, a reinstall and a downgrade are the same
//! command: a flat component package is laid down whatever is there.
//!
//! The engine's packages go down first, then the framework's. Refuses to
//! run while a program from either installed tree is running (--force goes
//! ahead): a server that outlives the install keeps the old libraries.

use crate::common::components::{self, Component};
use crate::common::macos_pkg::{self, ENGINE_DIR, FRAMEWORK_DIR};
use crate::common::ui;
use crate::common::util::{self, cmd, output, run as run_cmd};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

fn name(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

pub fn run(force: bool, dry_run: bool, files: Vec<PathBuf>) -> Result<()> {
    let out_dir = util::repo_root().join("dist");
    let pkgs: Vec<PathBuf> = if files.is_empty() {
        let found: Vec<PathBuf> = components::newest_in(&out_dir, "-arm64.pkg")
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        if found.is_empty() {
            bail!(
                "no .pkg in {}. Run cargo run -p xtask -- package first.",
                out_dir.display()
            );
        }
        found
    } else {
        let mut given = Vec::new();
        for f in files {
            if !f.is_file() || f.extension().map(|e| e != "pkg").unwrap_or(true) {
                bail!("not a .pkg: {}", f.display());
            }
            given.push(util::absolute(&f));
        }
        given
    };
    let pkgs = macos_pkg::install_order(pkgs);
    // private-ai-framework-config-v<fw>-arm64.pkg -> <fw>
    let config_prefix = format!("{}-v", Component::Config.package());
    let new_version = pkgs.iter().find_map(|p| {
        let file = name(p);
        file.strip_prefix(&config_prefix)
            .and_then(|r| r.split('-').next())
            .map(str::to_string)
    });
    let old_version = output(cmd(format!("{FRAMEWORK_DIR}/bin/pai-config")).arg("--version"))
        .and_then(|s| s.split_whitespace().last().map(str::to_string))
        .unwrap_or_default();

    for p in &pkgs {
        ui::head(format!("Package:   {}", name(p)));
    }
    ui::head(format!(
        "Installed: {}",
        if old_version.is_empty() {
            "none"
        } else {
            &old_version
        }
    ));

    // ── Running programs ────────────────────────────────────────────────
    // BSD pgrep: -l with -f prints the full command line.
    let running =
        output(cmd("pgrep").args(["-l", "-f", &macos_pkg::running_pattern()])).unwrap_or_default();
    if !running.is_empty() {
        ui::warn(format!("Running from {ENGINE_DIR} or {FRAMEWORK_DIR}:"));
        for line in running.lines() {
            ui::dim(format!("  {line}"));
        }
        if !force {
            bail!("stop them first (or pass --force)");
        }
        ui::warn("Continuing (--force): restart them once the install is done");
    }

    let root = output(cmd("id").arg("-u")).as_deref() == Some("0");
    for p in &pkgs {
        let mut argv = vec![
            "installer".to_string(),
            "-pkg".into(),
            p.display().to_string(),
            "-target".into(),
            "/".into(),
        ];
        if !root {
            argv.insert(0, "sudo".into());
        }
        ui::head(format!("Running: {}", argv.join(" ")));
        if !dry_run {
            run_cmd(cmd(&argv[0]).args(&argv[1..]))?;
        }
    }
    if dry_run {
        ui::dim("(dry run: nothing installed)");
        return Ok(());
    }

    if let Some(new_version) = &new_version {
        let version = output(cmd(format!("{FRAMEWORK_DIR}/bin/pai-config")).arg("--version"))
            .and_then(|s| s.split_whitespace().last().map(str::to_string))
            .unwrap_or_default();
        if version != *new_version {
            bail!(
                "{FRAMEWORK_DIR}/bin/pai-config reports '{}', the package is {new_version}",
                if version.is_empty() {
                    "nothing"
                } else {
                    &version
                }
            );
        }
    }
    ui::say("");
    ui::ok(format!("{} packages installed", pkgs.len()));
    if !running.is_empty() {
        ui::warn("Restart the programs listed above to use it");
    }
    ui::say("");
    Ok(())
}
