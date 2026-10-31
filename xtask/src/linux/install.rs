//! Install phase (cargo run -p xtask -- install): installs the newest package the package
//! phase left in ./dist/ on this machine: a first install, an upgrade (a newer
//! framework or a newer bundled llama.cpp: the package version is
//! <framework>+llama.cpp.<tag>), a reinstall of the same version (a
//! repackage, which the package manager would otherwise call "already
//! installed") or a downgrade, all the same command (windows/install.rs and
//! macos/install.rs are the other systems').
//!
//! The format follows the distribution (/etc/os-release, then the package
//! managers on PATH): the .deb through apt-get (which also pulls the
//! Depends), the .rpm through dnf, zypper or bare rpm. A package file given on
//! the command line brings its own format. "Newest" is the last one written
//! of the files named for this architecture.
//!
//! Refuses to run while a program from /opt/llama.cpp is running: rocBLAS and
//! hipBLASLt load their kernel files lazily, so a server that outlives the
//! upgrade would read the NEW kernel files with the OLD libraries.

use crate::common::ui;
use crate::common::util::{self, cmd, output, run as run_cmd, which};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

const PKG_NAME: &str = "llama-cpp-framework";
const OPT_DIR: &str = "/opt/llama.cpp";

/// The distribution decides, from ID and ID_LIKE in /etc/os-release. The
/// tools on PATH come second and are not enough alone: a Debian build host
/// has rpm too (the package phase's rpmbuild), and rpm there has no database.
fn detect_format() -> Option<&'static str> {
    let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{key}=")))
            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
            .unwrap_or_default()
    };
    let ids = format!("{} {}", field("ID"), field("ID_LIKE"));
    for id in ids.split_whitespace() {
        match id {
            "debian" | "ubuntu" => return Some("deb"),
            "fedora" | "rhel" | "centos" | "rocky" | "almalinux" | "suse" | "sles" | "mageia" | "openmandriva" => {
                return Some("rpm")
            }
            _ if id.starts_with("opensuse") => return Some("rpm"),
            _ => {}
        }
    }
    let has = |t: &str| which(t).is_some();
    if has("apt-get") && has("dpkg") {
        Some("deb")
    } else if has("dnf") || has("zypper") || (has("rpm") && !has("dpkg")) {
        Some("rpm")
    } else {
        None
    }
}

fn name(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

fn modified(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

pub fn run(format: Option<&str>, force: bool, dry_run: bool, file: Option<PathBuf>) -> Result<()> {
    let out_dir = util::repo_root().join("dist");
    let mut format = format.map(str::to_string);

    // ── Format and file ─────────────────────────────────────────────────
    let mut pkg_file = None;
    if let Some(f) = file {
        if !f.is_file() {
            bail!("no such file: {}", f.display());
        }
        let f = util::absolute(&f);
        let ext = match f.extension().and_then(|e| e.to_str()) {
            Some(e @ ("deb" | "rpm")) => e.to_string(),
            _ => bail!("not a .deb or an .rpm: {}", f.display()),
        };
        let fmt = format.get_or_insert(ext.clone());
        if *fmt != ext {
            bail!("--{fmt} given for {}", name(&f));
        }
        pkg_file = Some(f);
    }
    let format = match format {
        Some(f) => f,
        None => {
            let f = detect_format().context("cannot tell whether this machine takes a .deb or an .rpm: pass --deb or --rpm")?;
            ui::dim(format!("Format: .{f} (detected)"));
            f.to_string()
        }
    };
    let deb = format == "deb";

    let pkg_file = match pkg_file {
        Some(f) => f,
        None => {
            // Each format names the architecture its own way, as the package
            // phase does.
            let arch = match (std::env::consts::ARCH, deb) {
                ("x86_64", true) => "amd64",
                ("x86_64", false) => "x86_64",
                ("aarch64", true) => "arm64",
                ("aarch64", false) => "aarch64",
                (a, _) => bail!("unsupported architecture: {a}"),
            };
            let suffix = format!("-{arch}.{format}");
            let prefix = format!("{PKG_NAME}-v");
            std::fs::read_dir(&out_dir)
                .map(|rd| rd.flatten().map(|e| e.path()).collect::<Vec<_>>())
                .unwrap_or_default()
                .into_iter()
                .filter(|p| {
                    let n = name(p);
                    n.starts_with(&prefix) && n.ends_with(&suffix)
                })
                .max_by_key(|p| modified(p))
                .with_context(|| {
                    format!("no {PKG_NAME} .{format} for {arch} in {}. Run cargo run -p xtask -- package first.", out_dir.display())
                })?
        }
    };

    // A package written before the last build is not what the tree holds now.
    let built = util::build_root().join("llama.cpp-cmake/bin/llama-server");
    if let (Some(b), Some(p)) = (modified(&built), modified(&pkg_file)) {
        if b > p {
            ui::warn(format!(
                "{} predates the last cargo run -p xtask -- build: run cargo run -p xtask -- package to package the new build",
                name(&pkg_file)
            ));
        }
    }

    // ── Versions ────────────────────────────────────────────────────────
    let (new_version, old_version) = if deb {
        let new = output(cmd("dpkg-deb").arg("-f").arg(&pkg_file).arg("Version")).unwrap_or_default();
        let old = output(cmd("dpkg-query").args(["-W", "-f=${db:Status-Status} ${Version}", PKG_NAME]))
            .and_then(|s| s.strip_prefix("installed ").map(str::to_string))
            .unwrap_or_default();
        (new, old)
    } else {
        let new = output(cmd("rpm").args(["-qp", "--qf", "%{VERSION}-%{RELEASE}"]).arg(&pkg_file)).unwrap_or_default();
        let old = output(cmd("rpm").args(["-q", "--qf", "%{VERSION}-%{RELEASE}", PKG_NAME]))
            .filter(|s| !s.contains(' ') && !s.contains("not installed"))
            .unwrap_or_default();
        (new, old)
    };
    if new_version.is_empty() {
        bail!("could not read the version of {}", pkg_file.display());
    }

    let action = if old_version.is_empty() {
        "install"
    } else if old_version == new_version {
        "reinstall"
    } else if util::version_gt(&new_version, &old_version) {
        "upgrade"
    } else {
        "downgrade"
    };

    ui::head(format!("Package:   {} ({} MiB)", name(&pkg_file), util::du("-m", &pkg_file)));
    ui::head(format!("Installed: {}", if old_version.is_empty() { "none" } else { &old_version }));
    ui::head(format!("Action:    {action} to {new_version}"));

    // ── Running programs ────────────────────────────────────────────────
    let running = output(cmd("pgrep").args(["-a", "-f", &format!("^{OPT_DIR}/bin/")])).unwrap_or_default();
    if !running.is_empty() {
        ui::warn(format!("Running from {OPT_DIR}:"));
        for line in running.lines() {
            ui::dim(format!("  {line}"));
        }
        if !force {
            bail!("stop them first (or pass --force): a server that outlives the {action} loads the new kernel files with its old libraries");
        }
        ui::warn("Continuing (--force): restart them once the install is done");
    }

    // ── Install ─────────────────────────────────────────────────────────
    let file = pkg_file.display().to_string();
    let mut argv: Vec<String> = if deb {
        // --reinstall makes the same version count; --allow-downgrades the older.
        vec!["apt-get", "install", "-y", "--reinstall", "--allow-downgrades"].into_iter().map(String::from).collect()
    } else if which("dnf").is_some() {
        match action {
            "reinstall" => vec!["dnf".into(), "reinstall".into(), "-y".into()],
            "downgrade" => vec!["dnf".into(), "downgrade".into(), "-y".into()],
            _ => vec!["dnf".into(), "install".into(), "-y".into()],
        }
    } else if which("zypper").is_some() {
        // The package is unsigned; --force covers reinstall and downgrade alike.
        ["zypper", "--non-interactive", "install", "--force", "--allow-unsigned-rpm"].map(String::from).to_vec()
    } else {
        // Bare rpm checks the dependencies but cannot fetch them.
        ["rpm", "-Uvh", "--replacepkgs", "--oldpackage"].map(String::from).to_vec()
    };
    argv.push(file);
    if output(cmd("id").arg("-u")).as_deref() != Some("0") {
        argv.insert(0, "sudo".into());
    }

    ui::head(format!("Running: {}", argv.join(" ")));
    if dry_run {
        ui::dim("(dry run: nothing installed)");
        return Ok(());
    }
    run_cmd(cmd(&argv[0]).args(&argv[1..]))?;

    // ── Check ───────────────────────────────────────────────────────────
    let version = output(cmd(format!("{OPT_DIR}/bin/llama-cpp-config")).arg("--version"))
        .and_then(|s| s.split_whitespace().last().map(str::to_string))
        .unwrap_or_default();
    let matches = !version.is_empty()
        && new_version
            .strip_prefix(&version)
            .map(|rest| rest.starts_with('+') || rest.starts_with('-'))
            .unwrap_or(false);
    if !matches {
        bail!(
            "{OPT_DIR}/bin/llama-cpp-config reports '{}', the package is {new_version}",
            if version.is_empty() { "nothing" } else { &version }
        );
    }
    ui::say("");
    ui::ok(format!("{PKG_NAME} {new_version} installed in {OPT_DIR} ({action})"));
    if !running.is_empty() {
        ui::warn("Restart the programs listed above to use it");
    }
    ui::say("");
    Ok(())
}
