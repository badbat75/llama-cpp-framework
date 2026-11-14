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
mod tests {
    use super::*;

    fn pkg(file: &str, new_version: &str, old_version: &str) -> Package {
        Package {
            file: file.into(),
            name: String::new(),
            new_version: new_version.into(),
            old_version: old_version.into(),
        }
    }

    /// The programs of the packages run from /usr/bin and
    /// /usr/libexec/private-ai, where a command line names no package; what
    /// they share is the libraries of /usr/lib/private-ai, so a process that
    /// maps one is a program the install would leave on the old libraries
    /// (and rocBLAS would read the new kernel files with them). A library of
    /// the system, or a directory that only starts with the same name, is not.
    #[test]
    fn a_process_mapping_the_private_libraries_counts_as_running() {
        let ours = "7f2a1c000000-7f2a1c200000 r-xp 00000000 103:02 1234567 \
                    /usr/lib/private-ai/libggml-base.so.0.9.0\n";
        let runtime = "7f2a1c000000-7f2a1c200000 r--p 00000000 103:02 7654321 \
                       /usr/lib/private-ai/lib/libamdhip64.so.7\n";
        let system = "55d0c0000000-55d0c0021000 r-xp 00000000 103:02 42 \
                      /usr/lib/x86_64-linux-gnu/libc.so.6\n\
                      7ffd00000000-7ffd00021000 rw-p 00000000 00:00 0 [stack]\n";
        let lookalike = "7f2a1c000000-7f2a1c200000 r-xp 00000000 103:02 9 \
                         /usr/lib/private-ai-other/libx.so\n";
        assert!(holds_private_libraries(ours));
        assert!(holds_private_libraries(runtime));
        assert!(holds_private_libraries(&format!("{system}{ours}")));
        assert!(!holds_private_libraries(system));
        assert!(!holds_private_libraries(lookalike));
        assert!(!holds_private_libraries(""));
    }

    /// A non-root install cannot read the maps of another user's processes,
    /// and llama-server runs as the service user, pai-manager as root; their
    /// command lines are readable by everyone. argv[0] alone decides: one of
    /// the packages' executables in /usr/bin. pai-manager in
    /// /usr/libexec/private-ai is not: it only ever runs as the manager's
    /// service, which the maintainer scripts restart on the upgrade. Another
    /// program in /usr/bin, the same name elsewhere, or one of ours named only
    /// as an argument is not either.
    #[test]
    fn a_process_running_a_packaged_program_counts_as_running() {
        for ours in [
            b"/usr/bin/llama-server\0--port\08080\0".as_slice(),
            b"/usr/bin/llama-server\0",
            b"/usr/bin/llama-bench\0-m\0/models/x.gguf\0",
            b"/usr/bin/llama-cli\0",
            b"/usr/bin/pai-config\0",
            b"/usr/bin/pai-config\0server\0start\0",
        ] {
            assert!(
                runs_packaged_program(ours),
                "{}",
                String::from_utf8_lossy(ours)
            );
        }
        for other in [
            b"".as_slice(),
            b"/usr/bin/bash\0",
            b"/usr/bin/python3\0/usr/bin/llama-server\0",
            b"/usr/bin/vim\0/usr/bin/pai-config\0",
            // Always the manager's service, which the maintainer scripts restart.
            b"/usr/libexec/private-ai/pai-manager\0serve\0",
            b"/usr/libexec/private-ai/pai-manager",
            b"/usr/local/bin/llama-server\0",
            b"/opt/other/bin/pai-config\0",
            b"/usr/libexec/private-ai-other/pai-manager\0",
            b"/usr/libexec/other/pai-manager\0",
            b"/usr/bin/\0",
        ] {
            assert!(
                !runs_packaged_program(other),
                "{}",
                String::from_utf8_lossy(other)
            );
        }
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
        let pkgs = [
            pkg("same.rpm", "1", "1"),
            pkg("new.rpm", "1", ""),
            pkg("up.rpm", "2", "1"),
            pkg("down.rpm", "1", "2"),
        ];
        let commands = install_commands(&pkgs, false, &|t| t == "dnf");
        assert_eq!(commands[0], ["dnf", "install", "-y", "new.rpm", "up.rpm"]);
        assert_eq!(commands[1], ["dnf", "downgrade", "-y", "down.rpm"]);
        assert_eq!(commands[2], ["dnf", "reinstall", "-y", "same.rpm"]);
    }

    // ── The services do not stop an install ─────────────────────────────

    /// `pai`, the command line, is a program of the packages like the GUI:
    /// a `pai bench run` in a user's shell stops the install. Only by its
    /// exact name in /usr/bin.
    #[test]
    fn a_running_pai_counts_as_running() {
        for ours in [
            b"/usr/bin/pai\0".as_slice(),
            b"/usr/bin/pai\0bench\0run\0--spec\0standard\0",
        ] {
            assert!(
                runs_packaged_program(ours),
                "{}",
                String::from_utf8_lossy(ours)
            );
        }
        for other in [
            b"/usr/bin/paint\0".as_slice(),
            b"/usr/local/bin/pai\0",
            b"/usr/bin/vim\0/usr/bin/pai\0",
        ] {
            assert!(
                !runs_packaged_program(other),
                "{}",
                String::from_utf8_lossy(other)
            );
        }
    }

    /// The cgroups of real processes on Ubuntu 26.04 (systemd 259, cgroup
    /// v2: one `0::<path>` line, proc(5) /proc/pid/cgroup): an instance's
    /// llama-server runs in its unit's cgroup (the unit's children stay in
    /// it), the services in theirs, a user's programs in their session.
    const INSTANCE_CG: &str = "0::/system.slice/system-llama.cpp.slice/llama.cpp@default.service\n";
    const MANAGER_CG: &str = "0::/system.slice/private-ai-manager.service\n";
    const BFF_CG: &str = "0::/system.slice/private-ai-bff.service\n";
    const USER_CG: &str =
        "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-com.microsoft.VSCode-10983.scope\n";

    /// The framework's own services never stop the install: their
    /// packages' scripts stop them before the files change and start them
    /// again (an instance's llama-server, the manager, the bff), whatever
    /// user they run as. Read from the cgroup, not the uid: the bff runs as
    /// its own user, and a user may run a llama-server as `private-ai`.
    #[test]
    fn the_framework_services_never_stop_the_install() {
        for (exe, cg) in [
            ("/usr/bin/llama-server", INSTANCE_CG),
            ("/usr/libexec/private-ai/pai-manager", INSTANCE_CG),
            ("/usr/libexec/private-ai/pai-manager", MANAGER_CG),
            ("/usr/libexec/private-ai/pai-bff", BFF_CG),
            // A hybrid cgroup v1 host: the systemd hierarchy names the unit.
            (
                "/usr/bin/llama-server",
                "12:pids:/system.slice/system-llama.cpp.slice/llama.cpp@gpu1.service\n\
                 1:name=systemd:/system.slice/system-llama.cpp.slice/llama.cpp@gpu1.service\n\
                 0::/system.slice/system-llama.cpp.slice/llama.cpp@gpu1.service\n",
            ),
        ] {
            assert_eq!(
                classify_holder(4242, exe, cg),
                Holder::Service,
                "{exe} {cg}"
            );
        }
    }

    /// Only a llama-server outside the services stops the install (started
    /// by hand: a user's, root's in a login session, or under a unit of
    /// someone else's), since it would load the new kernel files with its
    /// old libraries; so does one whose cgroup could not be read. A user
    /// unit that borrows a service's name does not count: only the system
    /// services under /system.slice are the framework's.
    #[test]
    fn a_llama_server_outside_the_services_refuses_the_install() {
        for cg in [
            USER_CG,
            "0::/user.slice/user-0.slice/session-3.scope\n",
            "0::/system.slice/run-u42.service\n",
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/llama.cpp@x.service\n",
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/private-ai-manager.service\n",
            "",
        ] {
            assert_eq!(
                classify_holder(1177879, "/usr/bin/llama-server", cg),
                Holder::Refuses("1177879 /usr/bin/llama-server".into()),
                "{cg}"
            );
        }
    }

    /// Any other program on the packages' programs or libraries outside
    /// the services (an open pai-config, a `pai bench run`, a llama-bench,
    /// a script that loaded the libraries) is ignored: no refusal, no
    /// warning (user's decision).
    #[test]
    fn other_programs_outside_the_services_are_ignored() {
        for exe in [
            "/usr/bin/pai-config",
            "/usr/bin/pai",
            "/usr/bin/python3",
            "/usr/bin/llama-bench",
        ] {
            for cg in [USER_CG, ""] {
                assert_eq!(
                    classify_holder(31337, exe, cg),
                    Holder::Ignored,
                    "{exe} {cg}"
                );
            }
        }
    }

    /// The install no longer refuses on uid: `run` classifies with
    /// [`classify_holder`], refuses only on [`Holder::Refuses`] and reports
    /// nothing else: no list of other programs to warn about.
    #[test]
    fn the_install_check_uses_classify_holder() {
        let code = include_str!("install.rs");
        let code = code.split("#[cfg(test)]").next().unwrap();
        assert!(
            code.matches("classify_holder(").count() >= 2,
            "classify_holder is not called"
        );
        assert!(
            code.contains("Holder::Refuses"),
            "run does not refuse on Holder::Refuses"
        );
        assert!(
            !code.contains("Holder::Warns") && !code.contains("warns"),
            "run still collects or warns about the ignored programs"
        );
        assert!(
            !code.contains("blocks_install("),
            "the uid rule is still used"
        );
    }

    // ── The framework and its engine ────────────────────────────────────

    /// The newest of each package in dist/ by its new name: the framework's
    /// four at the bare framework version, the engine `llama.cpp` at its own
    /// version (the epoch is not in a file name, as dpkg-name leaves it out)
    /// and its runtimes `llama.cpp-<sdk>`, told apart from the engine. The
    /// never-published 2.99.x names (`private-ai-framework-llama`, `-rocm`)
    /// are never picked.
    #[test]
    fn install_finds_the_newest_of_each_package_by_the_new_names() {
        let dir = std::env::temp_dir().join(format!("xtask-install-split-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for file in [
            "private-ai-framework-config-v2.99.1-amd64.deb",
            "private-ai-framework-cli-v2.99.1-amd64.deb",
            "private-ai-framework-bff-v2.99.1-amd64.deb",
            "private-ai-framework-core-v2.99.1-amd64.deb",
            "llama.cpp-v1.0.0+pai1-amd64.deb",
            "llama.cpp-rocm-v10.1.0-amd64.deb",
            "llama.cpp-vulkan-v1.4.363.1-amd64.deb",
            "private-ai-framework-llama-v2.99.0-v1.0.0-amd64.deb",
            "private-ai-framework-rocm-v10.1.0-amd64.deb",
        ] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        let mut found: Vec<String> = components::newest_in(&dir, "-amd64.deb")
            .iter()
            .map(|(_, p)| name(p))
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        found.sort();
        assert_eq!(
            found,
            [
                "llama.cpp-rocm-v10.1.0-amd64.deb",
                "llama.cpp-v1.0.0+pai1-amd64.deb",
                "llama.cpp-vulkan-v1.4.363.1-amd64.deb",
                "private-ai-framework-bff-v2.99.1-amd64.deb",
                "private-ai-framework-cli-v2.99.1-amd64.deb",
                "private-ai-framework-config-v2.99.1-amd64.deb",
                "private-ai-framework-core-v2.99.1-amd64.deb",
            ]
        );
    }

    /// The engine's version carries the epoch `1:` (Ubuntu's own
    /// `llama.cpp` is `8681+dfsg-1`): replacing the distribution's package
    /// with ours is an upgrade, the way back a downgrade, and a new build
    /// revision of the same release an upgrade, in the `.deb` form and the
    /// rpm EVR form alike (rpm-version(7): an omitted epoch is zero and the
    /// epoch outranks the rest).
    #[test]
    fn the_engine_versions_are_compared_with_their_epoch() {
        for (new, old, want) in [
            ("1:1.0.0+pai1", "8681+dfsg-1", "upgrade"),
            ("8681+dfsg-1", "1:1.0.0+pai1", "downgrade"),
            ("1:1.0.0+pai2", "1:1.0.0+pai1", "upgrade"),
            ("1:1.0.0+pai10", "1:1.0.0+pai9", "upgrade"),
            ("1:1.0.1+pai1", "1:1.0.0+pai3", "upgrade"),
            ("1:1.0.0+pai1", "1:1.0.0+pai1", "reinstall"),
            ("1:1.0.0-pai1", "b11232-1", "upgrade"),
            ("1:1.0.0-pai2", "1:1.0.0-pai1", "upgrade"),
            ("1:1.0.0-pai1", "1:1.0.1-pai1", "downgrade"),
            ("1:1.0.0+pai1", "", "install"),
            // The framework packages carry no epoch: as before.
            ("2.99.1", "2.99.0", "upgrade"),
            ("2.99.1-1", "2.99.2-1", "downgrade"),
        ] {
            assert_eq!(
                pkg("llama.cpp-v1.0.0+pai1-amd64.deb", new, old).action(),
                want,
                "{new} over {old}"
            );
        }
    }

    /// An `.rpm`'s version is read with its epoch, as rpm prints `%{EVR}`
    /// (`1:1.0.0-pai1`; `2.99.1-1` without one, checked with rpm 6.0.1),
    /// for the file and the installed package alike: `%{VERSION}-%{RELEASE}`
    /// drops the epoch, and the engine would then compare as `1.0.0` against
    /// another build's. Read from the code before the tests.
    #[test]
    fn rpm_versions_are_read_with_their_epoch() {
        let src = include_str!("install.rs");
        let code = src.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        assert!(
            !code.contains("%{VERSION}-%{RELEASE}"),
            "an rpm version is read without its epoch"
        );
        assert_eq!(
            code.matches("%{EVR}").count(),
            2,
            "the file's and the installed package's version, each with %{{EVR}}"
        );
    }

    /// After the install, `/usr/bin/pai-config --version` must name the
    /// configurator's package version: now the bare framework version
    /// (`2.99.1` in a `.deb`, `2.99.1-1` as rpm's EVR), no `+llama.cpp...`
    /// any more. Another version, a longer one that only starts the same, or
    /// nothing, is not it.
    #[test]
    fn the_installed_configurator_reports_its_package_version() {
        for (reported, package) in [("2.99.1", "2.99.1"), ("2.99.1", "2.99.1-1")] {
            assert!(reports_version(reported, package), "{reported} {package}");
        }
        for (reported, package) in [
            ("2.99.0", "2.99.1"),
            ("2.99.1", "2.99.10"),
            ("2.99.1", "2.99.10-1"),
            ("", "2.99.1"),
            ("2.99.1", ""),
        ] {
            assert!(
                !reports_version(reported, package),
                "{reported:?} {package:?}"
            );
        }
    }

    /// The check after the install goes through [`reports_version`].
    #[test]
    fn the_install_check_uses_reports_version() {
        let src = include_str!("install.rs");
        let code = src.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        let run = code.split("pub fn run(").nth(1).expect("run()");
        assert!(
            run.contains("reports_version("),
            "run() checks pai-config's version inline"
        );
    }

    // ── A package older than its build ──────────────────────────────────

    /// A build tree as `xtask build` leaves it: llama-server of the engine,
    /// the framework's four executables in cargo's release directory, each
    /// written at `secs` after the epoch.
    fn build_tree(root: &Path, engine_secs: u64, framework_secs: u64) {
        let at = |p: PathBuf, secs: u64| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            let f = std::fs::File::create(&p).unwrap();
            f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
                .unwrap();
        };
        at(root.join("llama.cpp-cmake/bin/llama-server"), engine_secs);
        for exe in ["pai-config", "pai", "pai-bff", "pai-manager"] {
            at(root.join("cargo-target/release").join(exe), framework_secs);
        }
    }

    /// A package file in `dir` written at `secs` after the epoch.
    fn package_at(dir: &Path, file: &str, secs: u64) -> PathBuf {
        let p = dir.join(file);
        let f = std::fs::File::create(&p).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
        p
    }

    struct Tmp(PathBuf);
    impl Tmp {
        fn new(name: &str) -> Tmp {
            let d =
                std::env::temp_dir().join(format!("xtask-install-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(d.join("dist")).unwrap();
            Tmp(d)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const FRAMEWORK: [(Component, &str); 4] = [
        (
            Component::Config,
            "private-ai-framework-config-v2.99.1-amd64.deb",
        ),
        (Component::Cli, "private-ai-framework-cli-v2.99.1-amd64.deb"),
        (Component::Bff, "private-ai-framework-bff-v2.99.1-amd64.deb"),
        (
            Component::Core,
            "private-ai-framework-core-v2.99.1-amd64.deb",
        ),
    ];
    const ENGINE: &str = "llama.cpp-v1.0.0+pai1-amd64.deb";

    /// After `xtask build framework` the framework's packages predate its
    /// executables, while llama-server is older than all of them: each
    /// framework package is stale, the engine's is not.
    #[test]
    fn a_framework_package_older_than_the_framework_build_is_stale() {
        let t = Tmp::new("stale-framework");
        let dist = t.0.join("dist");
        build_tree(&t.0, 1_000, 3_000);
        for (c, file) in FRAMEWORK {
            let p = package_at(&dist, file, 2_000);
            assert!(is_stale(c, &p, &t.0), "{file} is not stale");
        }
        let engine = package_at(&dist, ENGINE, 2_000);
        assert!(
            !is_stale(Component::Llama, &engine, &t.0),
            "{ENGINE} is stale"
        );
    }

    /// After `xtask build llama` the engine's package predates llama-server,
    /// while the framework's packages are newer than its executables.
    #[test]
    fn an_engine_package_older_than_llama_server_is_stale() {
        let t = Tmp::new("stale-engine");
        let dist = t.0.join("dist");
        build_tree(&t.0, 3_000, 1_000);
        let engine = package_at(&dist, ENGINE, 2_000);
        assert!(
            is_stale(Component::Llama, &engine, &t.0),
            "{ENGINE} is not stale"
        );
        for (c, file) in FRAMEWORK {
            let p = package_at(&dist, file, 2_000);
            assert!(!is_stale(c, &p, &t.0), "{file} is stale");
        }
    }

    /// Packages written after the whole build are not stale; a runtime comes
    /// out of an SDK, which the build does not change, so it never is.
    #[test]
    fn packages_newer_than_the_build_and_runtimes_are_not_stale() {
        let t = Tmp::new("fresh");
        let dist = t.0.join("dist");
        build_tree(&t.0, 2_000, 2_000);
        for (c, file) in FRAMEWORK.into_iter().chain([(Component::Llama, ENGINE)]) {
            let p = package_at(&dist, file, 3_000);
            assert!(!is_stale(c, &p, &t.0), "{file} is stale");
        }
        build_tree(&t.0, 9_000, 9_000);
        for (c, file) in [
            (Component::Rocm, "llama.cpp-rocm-v10.1.0-amd64.deb"),
            (Component::Cuda, "llama.cpp-cuda-v13.0.2-amd64.deb"),
            (Component::Vulkan, "llama.cpp-vulkan-v1.4.363.1-amd64.deb"),
        ] {
            let p = package_at(&dist, file, 1_000);
            assert!(!is_stale(c, &p, &t.0), "{file} is stale");
        }
    }

    /// No build at all (packages copied in from elsewhere): nothing to
    /// compare against, so no package is stale.
    #[test]
    fn without_a_build_no_package_is_stale() {
        let t = Tmp::new("no-build");
        let dist = t.0.join("dist");
        for (c, file) in FRAMEWORK.into_iter().chain([(Component::Llama, ENGINE)]) {
            let p = package_at(&dist, file, 1_000);
            assert!(!is_stale(c, &p, &t.0), "{file} is stale");
        }
    }

    /// `run()` warns through `is_stale` for each picked package, with its
    /// component, against the build root, not llama-server's date alone.
    #[test]
    fn the_install_warning_uses_is_stale() {
        let src = include_str!("install.rs");
        let code = src.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        let run = code.split("pub fn run(").nth(1).expect("run()");
        assert!(run.contains("is_stale("), "run() does not call is_stale");
        assert!(
            !run.contains("llama.cpp-cmake/bin/llama-server"),
            "run() still compares every package with llama-server"
        );
    }
}
