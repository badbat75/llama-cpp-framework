//! Install phase on Windows (cargo run -p xtask -- install): installs the
//! newest MSI of llama.cpp the package phase left in .\dist\, then the newest
//! of the framework (only what is there; or the files given, at most one of
//! each, the engine first), each with
//! msiexec, basic UI: a progress bar and the UAC prompt the per-machine
//! package needs (/qn would show none and fail unelevated). A first install,
//! an upgrade and a same-version repackage are one command, the MSI's
//! MajorUpgrade (AllowSameVersionUpgrades). A downgrade, which the MSI
//! refuses, removes the installed version of that product first (basic UI,
//! so the framework's user data under %LOCALAPPDATA%\private-ai and the
//! engine's %ProgramData%\llama.cpp are kept: installer\AGENTS.md; the
//! engine's with KEEPINSTANCES=1, so its instance services stay).
//!
//! Running programs need no check here: the package's StopProcesses custom
//! action stops the configurator and llama-server before files are replaced.
//! All features go to the recorded InstallDir (or the default directory);
//! for another choice run the .msi itself, with its full UI.

use crate::common::msi;
use crate::common::ui;
use crate::common::util::{self, cmd, output, vercmp};
use crate::Product;
use anyhow::{bail, Context, Result};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

fn name(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// One REG_SZ value of a product's Apps entry, None when the entry or value
/// is absent.
fn reg_value(product: Product, value: &str) -> Option<String> {
    let key = msi::uninstall_key(product);
    let text = output(cmd("reg").args(["query", key, "/v", value, "/reg:64"]))?;
    // "    <name>    REG_SZ    <data>"
    text.lines().find_map(|l| {
        let rest = l
            .trim()
            .strip_prefix(value)?
            .trim_start()
            .strip_prefix("REG_SZ")?;
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
        code => bail!(
            "msiexec failed (exit {}), see {}",
            code.map_or("none".into(), |c| c.to_string()),
            log.display()
        ),
    }
}

pub fn run(dry_run: bool, files: Vec<PathBuf>) -> Result<()> {
    let out_dir = util::repo_root().join("dist");
    let plan = if files.is_empty() {
        msi::install_plan(&out_dir)
    } else {
        for f in &files {
            if !f.is_file() {
                bail!("not an .msi: {}", f.display());
            }
        }
        msi::files_plan(&files)?
            .iter()
            .map(|f| util::absolute(f))
            .collect()
    };
    if plan.is_empty() {
        bail!(
            "nothing to install: no llama.cpp or private-ai-framework .msi in {}. Run cargo run -p xtask -- package first.",
            out_dir.display()
        );
    }
    let logs = util::build_root();
    std::fs::create_dir_all(&logs)?;
    for msi in &plan {
        install(msi, dry_run, &logs)?;
    }
    if dry_run {
        ui::dim("(dry run: nothing installed)");
    }
    Ok(())
}

/// One MSI: a downgrade removes that product's installed version first.
fn install(msi: &Path, dry_run: bool, logs: &Path) -> Result<()> {
    let (product, new_version) = msi::msi_product(&name(msi)).with_context(|| {
        format!(
            "{} is not named as xtask package names an MSI (llama.cpp-v<release>-x64.msi, private-ai-framework-v<version>-x64.msi)",
            name(msi)
        )
    })?;
    let tag = match product {
        Product::Llama => "llama.cpp",
        Product::Framework => "private-ai-framework",
    };
    let old_version = reg_value(product, "DisplayVersion").unwrap_or_default();

    ui::head(format!("Package:   {}", name(msi)));
    ui::head(format!(
        "Installed: {}",
        if old_version.is_empty() {
            "none"
        } else {
            &old_version
        }
    ));

    // ── Downgrade: the MSI refuses it, so the installed version goes first ──
    let mut remove = None;
    if !old_version.is_empty() && vercmp(&new_version, &old_version) == Ordering::Less {
        // UninstallString is "MsiExec.exe /I{ProductCode} <PROPERTY>=1".
        let code = reg_value(product, "UninstallString")
            .and_then(|s| Some(s[s.find('{')?..=s.find('}')?].to_string()))
            .context("downgrade: the installed version's ProductCode is not in its Apps entry; uninstall it from Settings > Apps")?;
        ui::warn(format!(
            "Downgrade: {tag} {old_version} is removed first (its data kept)"
        ));
        remove = Some(msi::removal_args(product, &code));
    }
    let install = vec!["/i".to_string(), msi.display().to_string(), "/qb".into()];

    for args in remove.iter().chain([&install]) {
        ui::head(format!(
            "Running: msiexec {} /norestart /l*v <log>",
            args.join(" ")
        ));
    }
    if dry_run {
        return Ok(());
    }
    if let Some(args) = &remove {
        msiexec(args, &logs.join(format!("msi-uninstall-{tag}.log")))?;
    }
    msiexec(&install, &logs.join(format!("msi-install-{tag}.log")))?;

    let version = reg_value(product, "DisplayVersion").unwrap_or_default();
    if version != new_version {
        bail!(
            "the Apps entry of {tag} reports '{}', the package is {new_version}",
            if version.is_empty() {
                "nothing"
            } else {
                &version
            }
        );
    }
    ui::say("");
    ui::ok(format!("{tag} {new_version} installed"));
    ui::say("");
    Ok(())
}
