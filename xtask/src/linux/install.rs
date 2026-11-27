//! Install phase (cargo run -p xtask -- install): installs the newest of each package the
//! package phase left in ./dist/ on this machine, or the files given to leave a
//! GPU runtime out: a first install, an upgrade (a newer
//! framework, or a newer llama.cpp: the engine's package carries its own
//! version, `1:<release>+pai<N>`), a reinstall of the same version (a
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
//! Refuses to run while a llama-server outside the framework's services runs
//! a program of the packages (by its argv[0] in /usr/bin) or holds a library
//! of /usr/lib/private-ai: rocBLAS and hipBLASLt load their kernel files
//! lazily, so a server that outlives the upgrade would read the NEW kernel
//! files with the OLD libraries. Any other such program (pai-config, pai) is
//! not reported. The services do not stop it (the manager, the bff and
//! the instances, told by their cgroup): their packages' maintainer scripts
//! stop them and start them again.

use crate::common::components::{self, Component};
use crate::common::ui;
use crate::common::util::{self, cmd, output, run as run_cmd, which};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// The packages' private library directory.
const PRIVATE_LIB_DIR: &str = "/usr/lib/private-ai";

/// Whether a process, by its `/proc/<pid>/cmdline` (NUL-separated, readable
/// for every user's processes), runs a program of the packages: argv[0] is
/// one of their executables in /usr/bin (`pai-config`, `pai`, `llama-*`).
/// pai-manager is not: it only runs as the manager's service.
fn runs_packaged_program(cmdline: &[u8]) -> bool {
    let argv0 = cmdline.split(|&b| b == 0).next().unwrap_or_default();
    argv0
        .strip_prefix(b"/usr/bin/")
        .is_some_and(|name| name == b"pai-config" || name == b"pai" || name.starts_with(b"llama-"))
}

/// Whether a process, by its `/proc/<pid>/maps`, has a file of
/// /usr/lib/private-ai mapped: a program the install would leave running on
/// the old libraries.
fn holds_private_libraries(maps: &str) -> bool {
    // A mapping's path is its last field and the only one with a slash.
    maps.lines().any(|line| {
        line.find('/')
            .is_some_and(|i| line[i..].starts_with(&format!("{PRIVATE_LIB_DIR}/")))
    })
}

/// What a process found on the packages' programs or libraries means for
/// the install.
#[derive(Debug, PartialEq, Eq)]
enum Holder {
    /// In a framework service's cgroup: its package's scripts stop and
    /// start it.
    Service,
    /// A llama-server outside the services, with its `<pid> <exe>` line:
    /// the install refuses.
    Refuses(String),
    /// Any other program (pai-config, pai, llama-bench...): not reported.
    Ignored,
}

/// Classifies a process by its pid, its executable (argv[0] of
/// `/proc/<pid>/cmdline`) and the text of `/proc/<pid>/cgroup` (both
/// readable for every user's processes).
fn classify_holder(pid: u32, exe: &str, cgroup: &str) -> Holder {
    // Each line is `<id>:<controllers>:<path>` (proc(5)); a framework
    // service's path is a system unit's, under /system.slice/.
    let service = cgroup.lines().any(|line| {
        let path = line.splitn(3, ':').nth(2).unwrap_or_default();
        path.starts_with("/system.slice/")
            && path.split('/').any(|unit| {
                unit.ends_with(".service")
                    && (unit.starts_with("llama.cpp@") || unit.starts_with("private-ai-"))
            })
    });
    if service {
        Holder::Service
    } else if exe.rsplit('/').next() == Some("llama-server") {
        Holder::Refuses(format!("{pid} {exe}"))
    } else {
        Holder::Ignored
    }
}

/// Every process outside the services that runs a program of the packages
/// ([`runs_packaged_program`], which sees every user's processes) or holds
/// one of their libraries ([`holds_private_libraries`], for the maps this
/// user can read), classified by [`classify_holder`]: the `<pid> <exe>` line
/// of each one that refuses the install.
fn private_library_holders() -> Vec<String> {
    let mut refuses = Vec::new();
    for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        let cmdline = std::fs::read(entry.path().join("cmdline")).unwrap_or_default();
        let maps = std::fs::read_to_string(entry.path().join("maps")).unwrap_or_default();
        if !runs_packaged_program(&cmdline) && !holds_private_libraries(&maps) {
            continue;
        }
        let exe = cmdline.split(|&b| b == 0).next().unwrap_or_default();
        let cgroup = std::fs::read_to_string(entry.path().join("cgroup")).unwrap_or_default();
        match classify_holder(pid, &String::from_utf8_lossy(exe), &cgroup) {
            Holder::Refuses(line) => refuses.push(line),
            Holder::Service | Holder::Ignored => {}
        }
    }
    refuses
}

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
            "fedora" | "rhel" | "centos" | "rocky" | "almalinux" | "suse" | "sles" | "mageia"
            | "openmandriva" => return Some("rpm"),
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
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn modified(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Whether `package`, a package of component `c`, was written before the
/// last build under `build_root` of what it holds: a framework component
/// against its own executable, the engine against llama-server. A runtime
/// comes out of an SDK, which the build does not change, so it never is;
/// nor is a package with no build to compare against.
fn is_stale(c: Component, package: &Path, build_root: &Path) -> bool {
    let built = match c {
        Component::Config => "cargo-target/release/pai-config",
        Component::Cli => "cargo-target/release/pai",
        Component::Bff => "cargo-target/release/pai-bff",
        Component::Core => "cargo-target/release/pai-manager",
        Component::Llama => "llama.cpp-cmake/bin/llama-server",
        Component::Rocm | Component::Cuda | Component::Vulkan => return false,
    };
    matches!(
        (modified(&build_root.join(built)), modified(package)),
        (Some(b), Some(m)) if b > m
    )
}

/// Whether `/usr/bin/pai-config --version` (its last word, `reported`) is
/// the version of the configurator's package just installed
/// (`package_version`, as `Package::read` gives it).
fn reports_version(reported: &str, package_version: &str) -> bool {
    !reported.is_empty()
        && package_version
            .strip_prefix(reported)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('-'))
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
                output(cmd("rpm").args(["-qp", "--qf", "%{EVR}"]).arg(&file)).unwrap_or_default(),
            )
        };
        if name.is_empty() || new_version.is_empty() {
            bail!("could not read the name and version of {}", file.display());
        }
        let old_version = if deb {
            output(cmd("dpkg-query").args(["-W", "-f=${db:Status-Status} ${Version}", &name]))
                .and_then(|s| s.strip_prefix("installed ").map(str::to_string))
        } else {
            output(cmd("rpm").args(["-q", "--qf", "%{EVR}", &name]))
                .filter(|s| !s.contains(' ') && !s.contains("not installed"))
        }
        .unwrap_or_default();
        Ok(Package {
            file,
            name,
            new_version,
            old_version,
        })
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
    let files = |subset: &[&Package]| {
        subset
            .iter()
            .map(|p| p.file.display().to_string())
            .collect::<Vec<_>>()
    };
    let all: Vec<&Package> = pkgs.iter().collect();
    let with = |base: &[&str], subset: &[&Package]| {
        let mut argv: Vec<String> = base.iter().map(|s| s.to_string()).collect();
        argv.extend(files(subset));
        argv
    };
    if deb {
        // --reinstall makes the same version count; --allow-downgrades the older.
        return vec![with(
            &[
                "apt-get",
                "install",
                "-y",
                "--reinstall",
                "--allow-downgrades",
            ],
            &all,
        )];
    }
    if has("dnf") {
        let mut commands = Vec::new();
        let up: Vec<&Package> = pkgs
            .iter()
            .filter(|p| matches!(p.action(), "install" | "upgrade"))
            .collect();
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
        return vec![with(
            &[
                "zypper",
                "--non-interactive",
                "install",
                "--force",
                "--allow-unsigned-rpm",
            ],
            &all,
        )];
    }
    // Bare rpm checks the dependencies but cannot fetch them.
    vec![with(
        &["rpm", "-Uvh", "--replacepkgs", "--oldpackage"],
        &all,
    )]
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
            let f = detect_format().context(
                "cannot tell whether this machine takes a .deb or an .rpm: pass --deb or --rpm",
            )?;
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
        let found: Vec<PathBuf> = components::newest_in(&out_dir, &format!("-{arch}.{format}"))
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        if found.is_empty() {
            bail!(
                "no {} .{format} for {arch} in {}. Run cargo run -p xtask -- package first.",
                components::BASE_NAME,
                out_dir.display()
            );
        }
        found
    } else {
        given
    };

    // A package written before the last build is not what the tree holds now.
    let build_root = util::build_root();
    for p in &picked {
        let stale = Component::ALL
            .into_iter()
            .find(|c| components::is_file_of(*c, &name(p)))
            .is_some_and(|c| is_stale(c, p, &build_root));
        if stale {
            ui::warn(format!(
                "{} predates the last cargo run -p xtask -- build: run cargo run -p xtask -- package to package the new build",
                name(p)
            ));
        }
    }

    // ── Versions ────────────────────────────────────────────────────────
    let packages = picked
        .into_iter()
        .map(|f| Package::read(f, deb))
        .collect::<Result<Vec<_>>>()?;
    for p in &packages {
        ui::head(format!(
            "{}: {} ({} MiB)",
            p.name,
            name(&p.file),
            util::du("-m", &p.file)
        ));
        ui::head(format!(
            "  {} to {} (installed: {})",
            p.action(),
            p.new_version,
            if p.old_version.is_empty() {
                "none"
            } else {
                &p.old_version
            }
        ));
    }

    // ── Running programs ────────────────────────────────────────────────
    let refuses = private_library_holders();
    if !refuses.is_empty() {
        ui::warn(format!(
            "llama-server running outside the services on the libraries of {PRIVATE_LIB_DIR}:"
        ));
        for line in &refuses {
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
        let version = output(cmd("/usr/bin/pai-config").arg("--version"))
            .and_then(|s| s.split_whitespace().last().map(str::to_string))
            .unwrap_or_default();
        if !reports_version(&version, &p.new_version) {
            bail!(
                "/usr/bin/pai-config reports '{}', the package is {}",
                if version.is_empty() {
                    "nothing"
                } else {
                    &version
                },
                p.new_version
            );
        }
    }
    ui::say("");
    for p in &packages {
        ui::ok(format!(
            "{} {} installed ({})",
            p.name,
            p.new_version,
            p.action()
        ));
    }
    if !refuses.is_empty() {
        ui::warn("Restart the programs listed above to use it");
    }
    ui::say("");
    Ok(())
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
