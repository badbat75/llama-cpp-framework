//! Install phase on macOS (cargo run -p xtask -- install): installs the newest .pkg the
//! package phase left in ./dist/ (or the one given) with `installer`, as root.
//! A first install, an upgrade, a reinstall and a downgrade are the same
//! command: a flat component package is laid down whatever is there.
//!
//! Refuses to run while a program from /opt/llama.cpp is running (--force
//! goes ahead): a server that outlives the install keeps the old libraries.

use super::package::PKG_NAME;
use super::OPT_DIR;
use crate::common::ui;
use crate::common::util::{self, cmd, output, run as run_cmd};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

fn name(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

fn modified(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

pub fn run(force: bool, dry_run: bool, file: Option<PathBuf>) -> Result<()> {
    let out_dir = util::repo_root().join("dist");
    let prefix = format!("{PKG_NAME}-v");
    let pkg_file = match file {
        Some(f) => {
            if !f.is_file() || f.extension().map(|e| e != "pkg").unwrap_or(true) {
                bail!("not a .pkg: {}", f.display());
            }
            util::absolute(&f)
        }
        None => std::fs::read_dir(&out_dir)
            .map(|rd| rd.flatten().map(|e| e.path()).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter(|p| {
                let n = name(p);
                n.starts_with(&prefix) && n.ends_with("-arm64.pkg")
            })
            .max_by_key(|p| modified(p))
            .with_context(|| format!("no {PKG_NAME} .pkg in {}. Run cargo run -p xtask -- package first.", out_dir.display()))?,
    };
    // llama-cpp-framework-v<fw>-<llama>-arm64.pkg -> <fw>
    let new_version = name(&pkg_file)
        .strip_prefix(&prefix)
        .and_then(|r| r.split('-').next())
        .unwrap_or_default()
        .to_string();
    let old_version = output(cmd(format!("{OPT_DIR}/bin/llama-cpp-config")).arg("--version"))
        .and_then(|s| s.split_whitespace().last().map(str::to_string))
        .unwrap_or_default();

    ui::head(format!("Package:   {}", name(&pkg_file)));
    ui::head(format!("Installed: {}", if old_version.is_empty() { "none" } else { &old_version }));

    // ── Running programs ────────────────────────────────────────────────
    // BSD pgrep: -l with -f prints the full command line.
    let running = output(cmd("pgrep").args(["-l", "-f", &format!("^{OPT_DIR}/bin/")])).unwrap_or_default();
    if !running.is_empty() {
        ui::warn(format!("Running from {OPT_DIR}:"));
        for line in running.lines() {
            ui::dim(format!("  {line}"));
        }
        if !force {
            bail!("stop them first (or pass --force)");
        }
        ui::warn("Continuing (--force): restart them once the install is done");
    }

    let mut argv = vec!["installer".to_string(), "-pkg".into(), pkg_file.display().to_string(), "-target".into(), "/".into()];
    if output(cmd("id").arg("-u")).as_deref() != Some("0") {
        argv.insert(0, "sudo".into());
    }
    ui::head(format!("Running: {}", argv.join(" ")));
    if dry_run {
        ui::dim("(dry run: nothing installed)");
        return Ok(());
    }
    run_cmd(cmd(&argv[0]).args(&argv[1..]))?;

    let version = output(cmd(format!("{OPT_DIR}/bin/llama-cpp-config")).arg("--version"))
        .and_then(|s| s.split_whitespace().last().map(str::to_string))
        .unwrap_or_default();
    if version != new_version {
        bail!(
            "{OPT_DIR}/bin/llama-cpp-config reports '{}', the package is {new_version}",
            if version.is_empty() { "nothing" } else { &version }
        );
    }
    ui::say("");
    ui::ok(format!("{PKG_NAME} {new_version} installed in {OPT_DIR}"));
    if !running.is_empty() {
        ui::warn("Restart the programs listed above to use it");
    }
    ui::say("");
    Ok(())
}
