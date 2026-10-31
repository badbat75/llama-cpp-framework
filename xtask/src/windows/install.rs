//! Install phase on Windows (cargo run -p xtask -- install): installs the
//! newest MSI the package phase left in .\dist\ (or the one given) with
//! msiexec, basic UI: a progress bar and the UAC prompt the per-machine
//! package needs (/qn would show none and fail unelevated). A first install,
//! an upgrade and a same-version repackage are one command, the MSI's
//! MajorUpgrade (AllowSameVersionUpgrades). A downgrade, which the MSI
//! refuses, removes the installed version first (basic UI, so the user data
//! under %LOCALAPPDATA%\llama.cpp is kept: installer\AGENTS.md).
//!
//! Running programs need no check here: the package's StopProcesses custom
//! action stops the configurator and llama-server before files are replaced.
//! All features go to the recorded InstallDir (or the default directory);
//! for another choice run the .msi itself, with its full UI.

use crate::common::ui;
use crate::common::util::{self, cmd, output, vercmp};
use anyhow::{bail, Context, Result};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

const PKG_NAME: &str = "llama-cpp-framework";
/// The Apps entry the package writes (64-bit view).
const UNINSTALL_KEY: &str = r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\llama.cpp";

fn name(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

fn modified(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// One REG_SZ value of the Apps entry, None when the entry or value is absent.
fn reg_value(value: &str) -> Option<String> {
    let text = output(cmd("reg").args(["query", UNINSTALL_KEY, "/v", value, "/reg:64"]))?;
    // "    <name>    REG_SZ    <data>"
    text.lines().find_map(|l| {
        let rest = l.trim().strip_prefix(value)?.trim_start().strip_prefix("REG_SZ")?;
        Some(rest.trim().to_string())
    })
}

/// msiexec, waited for; 0 and 3010 (success, restart required) succeed.
fn msiexec(args: &[String], log: &Path) -> Result<()> {
    let mut c = cmd("msiexec");
    c.args(args).arg("/norestart").arg("/l*v").arg(log);
    let status = c.status().context("cannot run msiexec")?;
    match status.code() {
        Some(0) => Ok(()),
        Some(3010) => {
            ui::warn("Windows asks for a restart to finish replacing files in use");
            Ok(())
        }
        Some(1602) => bail!("cancelled"),
        code => bail!("msiexec failed (exit {}), see {}", code.map_or("none".into(), |c| c.to_string()), log.display()),
    }
}

pub fn run(dry_run: bool, file: Option<PathBuf>) -> Result<()> {
    let out_dir = util::repo_root().join("dist");
    let prefix = format!("{PKG_NAME}-v");
    let msi = match file {
        Some(f) => {
            if !f.is_file() || f.extension().map(|e| !e.eq_ignore_ascii_case("msi")).unwrap_or(true) {
                bail!("not an .msi: {}", f.display());
            }
            util::absolute(&f)
        }
        None => std::fs::read_dir(&out_dir)
            .map(|rd| rd.flatten().map(|e| e.path()).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter(|p| {
                let n = name(p);
                n.starts_with(&prefix) && n.ends_with("-x64.msi")
            })
            .max_by_key(|p| modified(p))
            .with_context(|| format!("no {PKG_NAME} .msi in {}. Run cargo run -p xtask -- package first.", out_dir.display()))?,
    };
    // llama-cpp-framework-v<fw>-<llama>-x64.msi -> <fw>
    let new_version = name(&msi)
        .strip_prefix(&prefix)
        .and_then(|r| r.split('-').next())
        .unwrap_or_default()
        .to_string();
    let old_version = reg_value("DisplayVersion").unwrap_or_default();

    ui::head(format!("Package:   {}", name(&msi)));
    ui::head(format!("Installed: {}", if old_version.is_empty() { "none" } else { &old_version }));

    let logs = util::build_root();
    std::fs::create_dir_all(&logs)?;

    // ── Downgrade: the MSI refuses it, so the installed version goes first ──
    let mut remove = None;
    if !old_version.is_empty() && vercmp(&new_version, &old_version) == Ordering::Less {
        // UninstallString is "MsiExec.exe /I{ProductCode} LLAMA_UNINSTALL=1".
        let code = reg_value("UninstallString")
            .and_then(|s| Some(s[s.find('{')?..=s.find('}')?].to_string()))
            .context("downgrade: the installed version's ProductCode is not in its Apps entry; uninstall it from Settings > Apps")?;
        ui::warn(format!("Downgrade: {old_version} is removed first (user data kept)"));
        remove = Some(vec!["/x".to_string(), code, "/qb".into()]);
    }
    let install = vec!["/i".to_string(), msi.display().to_string(), "/qb".into()];

    for args in remove.iter().chain([&install]) {
        ui::head(format!("Running: msiexec {} /norestart /l*v <log>", args.join(" ")));
    }
    if dry_run {
        ui::dim("(dry run: nothing installed)");
        return Ok(());
    }
    if let Some(args) = &remove {
        msiexec(args, &logs.join("msi-uninstall.log"))?;
    }
    msiexec(&install, &logs.join("msi-install.log"))?;

    let version = reg_value("DisplayVersion").unwrap_or_default();
    if version != new_version {
        bail!(
            "the Apps entry reports '{}', the package is {new_version}",
            if version.is_empty() { "nothing" } else { &version }
        );
    }
    ui::say("");
    ui::ok(format!("{PKG_NAME} {new_version} installed"));
    ui::say("");
    Ok(())
}
