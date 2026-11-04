//! Install phase (cargo run -p xtask -- install): installs the newest of each package the
//! package phase left in ./dist/ on this machine, or the files given to leave a
//! GPU runtime out: a first install, an upgrade (a newer
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
//! of the files named for this architecture, per package.
//!
//! Refuses to run while a program from /opt/llama.cpp is running: rocBLAS and
//! hipBLASLt load their kernel files lazily, so a server that outlives the
//! upgrade would read the NEW kernel files with the OLD libraries.

use crate::common::components::{self, Component};
use crate::common::ui;
use crate::common::util::{self, cmd, output, run as run_cmd, which};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

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

/// A package file with what the package manager says about it.
struct Package {
    file: PathBuf,
    name: String,
    new_version: String,
    old_version: String,
}

impl Package {
    fn read(file: PathBuf, deb: bool) -> Result<Package> {
        let (name, new_version) = if deb {
            (
                output(cmd("dpkg-deb").arg("-f").arg(&file).arg("Package")).unwrap_or_default(),
                output(cmd("dpkg-deb").arg("-f").arg(&file).arg("Version")).unwrap_or_default(),
            )
        } else {
            (
                output(cmd("rpm").args(["-qp", "--qf", "%{NAME}"]).arg(&file)).unwrap_or_default(),
                output(cmd("rpm").args(["-qp", "--qf", "%{VERSION}-%{RELEASE}"]).arg(&file)).unwrap_or_default(),
            )
        };
        if name.is_empty() || new_version.is_empty() {
            bail!("could not read the name and version of {}", file.display());
        }
        let old_version = if deb {
            output(cmd("dpkg-query").args(["-W", "-f=${db:Status-Status} ${Version}", &name]))
                .and_then(|s| s.strip_prefix("installed ").map(str::to_string))
        } else {
            output(cmd("rpm").args(["-q", "--qf", "%{VERSION}-%{RELEASE}", &name]))
                .filter(|s| !s.contains(' ') && !s.contains("not installed"))
        }
        .unwrap_or_default();
        Ok(Package { file, name, new_version, old_version })
    }

    fn action(&self) -> &'static str {
        if self.old_version.is_empty() {
            "install"
        } else if self.old_version == self.new_version {
            "reinstall"
        } else if util::version_gt(&self.new_version, &self.old_version) {
            "upgrade"
        } else {
            "downgrade"
        }
    }
}

/// The package manager's commands for `pkgs`, as argument lists. One transaction
/// wherever the manager takes one: the packages require each other's versions.
/// dnf wants a verb per action, and refuses to reinstall what is not installed,
/// so it gets one command per action, the versions going up first.
fn install_commands(pkgs: &[Package], deb: bool, has: &dyn Fn(&str) -> bool) -> Vec<Vec<String>> {
    let files = |subset: &[&Package]| subset.iter().map(|p| p.file.display().to_string()).collect::<Vec<_>>();
    let all: Vec<&Package> = pkgs.iter().collect();
    let with = |base: &[&str], subset: &[&Package]| {
        let mut argv: Vec<String> = base.iter().map(|s| s.to_string()).collect();
        argv.extend(files(subset));
        argv
    };
    if deb {
        // --reinstall makes the same version count; --allow-downgrades the older.
        return vec![with(&["apt-get", "install", "-y", "--reinstall", "--allow-downgrades"], &all)];
    }
    if has("dnf") {
        let mut commands = Vec::new();
        let up: Vec<&Package> = pkgs.iter().filter(|p| matches!(p.action(), "install" | "upgrade")).collect();
        if !up.is_empty() {
            commands.push(with(&["dnf", "install", "-y"], &up));
        }
        for action in ["downgrade", "reinstall"] {
            let subset: Vec<&Package> = pkgs.iter().filter(|p| p.action() == action).collect();
            if !subset.is_empty() {
                commands.push(with(&["dnf", action, "-y"], &subset));
            }
        }
        return commands;
    }
    if has("zypper") {
        // The packages are unsigned; --force covers reinstall and downgrade alike.
        return vec![with(&["zypper", "--non-interactive", "install", "--force", "--allow-unsigned-rpm"], &all)];
    }
    // Bare rpm checks the dependencies but cannot fetch them.
    vec![with(&["rpm", "-Uvh", "--replacepkgs", "--oldpackage"], &all)]
}

pub fn run(format: Option<&str>, force: bool, dry_run: bool, files: Vec<PathBuf>) -> Result<()> {
    let out_dir = util::repo_root().join("dist");
    let mut format = format.map(str::to_string);

    // ── Format and files ────────────────────────────────────────────────
    let mut given = Vec::new();
    for f in files {
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
        given.push(f);
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

    let picked = if given.is_empty() {
        // Each format names the architecture its own way, as the package
        // phase does.
        let arch = match (std::env::consts::ARCH, deb) {
            ("x86_64", true) => "amd64",
            ("x86_64", false) => "x86_64",
            ("aarch64", true) => "arm64",
            ("aarch64", false) => "aarch64",
            (a, _) => bail!("unsupported architecture: {a}"),
        };
        let found: Vec<PathBuf> =
            components::newest_in(&out_dir, &format!("-{arch}.{format}")).into_iter().map(|(_, p)| p).collect();
        if found.is_empty() {
            bail!("no {} .{format} for {arch} in {}. Run cargo run -p xtask -- package first.", components::BASE_NAME, out_dir.display());
        }
        found
    } else {
        given
    };

    // A package written before the last build is not what the tree holds now.
    // A runtime comes out of an SDK, which the build does not change.
    let built = util::build_root().join("llama.cpp-cmake/bin/llama-server");
    for p in &picked {
        let runtime = Component::RUNTIMES.iter().any(|c| name(p).starts_with(&format!("{}-v", c.package())));
        if let (false, Some(b), Some(m)) = (runtime, modified(&built), modified(p)) {
            if b > m {
                ui::warn(format!(
                    "{} predates the last cargo run -p xtask -- build: run cargo run -p xtask -- package to package the new build",
                    name(p)
                ));
            }
        }
    }

    // ── Versions ────────────────────────────────────────────────────────
    let packages = picked.into_iter().map(|f| Package::read(f, deb)).collect::<Result<Vec<_>>>()?;
    for p in &packages {
        ui::head(format!("{}: {} ({} MiB)", p.name, name(&p.file), util::du("-m", &p.file)));
        ui::head(format!(
            "  {} to {} (installed: {})",
            p.action(),
            p.new_version,
            if p.old_version.is_empty() { "none" } else { &p.old_version }
        ));
    }

    // ── Running programs ────────────────────────────────────────────────
    let running = output(cmd("pgrep").args(["-a", "-f", &format!("^{OPT_DIR}/bin/")])).unwrap_or_default();
    if !running.is_empty() {
        ui::warn(format!("Running from {OPT_DIR}:"));
        for line in running.lines() {
            ui::dim(format!("  {line}"));
        }
        if !force {
            bail!("stop them first (or pass --force): a server that outlives the install loads the new kernel files with its old libraries");
        }
        ui::warn("Continuing (--force): restart them once the install is done");
    }

    // ── Install ─────────────────────────────────────────────────────────
    let root = output(cmd("id").arg("-u")).as_deref() == Some("0");
    let commands = install_commands(&packages, deb, &|t| which(t).is_some());
    for mut argv in commands {
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

    // ── Check ───────────────────────────────────────────────────────────
    let config = Component::Config.package();
    if let Some(p) = packages.iter().find(|p| p.name == config) {
        let version = output(cmd(format!("{OPT_DIR}/bin/llama-cpp-config")).arg("--version"))
            .and_then(|s| s.split_whitespace().last().map(str::to_string))
            .unwrap_or_default();
        let matches = !version.is_empty()
            && p.new_version
                .strip_prefix(&version)
                .map(|rest| rest.starts_with('+') || rest.starts_with('-'))
                .unwrap_or(false);
        if !matches {
            bail!(
                "{OPT_DIR}/bin/llama-cpp-config reports '{}', the package is {}",
                if version.is_empty() { "nothing" } else { &version },
                p.new_version
            );
        }
    }
    ui::say("");
    for p in &packages {
        ui::ok(format!("{} {} installed in {OPT_DIR} ({})", p.name, p.new_version, p.action()));
    }
    if !running.is_empty() {
        ui::warn("Restart the programs listed above to use it");
    }
    ui::say("");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(file: &str, new_version: &str, old_version: &str) -> Package {
        Package { file: file.into(), name: String::new(), new_version: new_version.into(), old_version: old_version.into() }
    }

    #[test]
    fn apt_takes_every_package_in_one_transaction() {
        let pkgs = [pkg("a.deb", "2", "1"), pkg("b.deb", "1", "1")];
        let commands = install_commands(&pkgs, true, &|_| false);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0][0], "apt-get");
        assert_eq!(commands[0][commands[0].len() - 2..], ["a.deb", "b.deb"]);
    }

    #[test]
    fn dnf_gets_one_verb_per_action_with_the_rising_versions_first() {
        let pkgs = [pkg("same.rpm", "1", "1"), pkg("new.rpm", "1", ""), pkg("up.rpm", "2", "1"), pkg("down.rpm", "1", "2")];
        let commands = install_commands(&pkgs, false, &|t| t == "dnf");
        assert_eq!(commands[0], ["dnf", "install", "-y", "new.rpm", "up.rpm"]);
        assert_eq!(commands[1], ["dnf", "downgrade", "-y", "down.rpm"]);
        assert_eq!(commands[2], ["dnf", "reinstall", "-y", "same.rpm"]);
    }
}
