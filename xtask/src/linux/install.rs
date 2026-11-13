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
//! Refuses to run while a user runs a program of the packages (by its argv[0]
//! in /usr/bin) or a process that holds a library of /usr/lib/private-ai:
//! rocBLAS and hipBLASLt load their kernel files lazily, so a server that
//! outlives the upgrade would read the NEW kernel files with the OLD
//! libraries. The services do not stop it (pai-manager, and the instances
//! the services' user runs): the maintainer scripts restart them.

use crate::common::components::{self, Component};
use crate::common::ui;
use crate::common::util::{self, cmd, output, run as run_cmd, which};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// The packages' private library directory.
const PRIVATE_LIB_DIR: &str = "/usr/lib/private-ai";

/// Whether a process, by its `/proc/<pid>/cmdline` (NUL-separated, readable
/// for every user's processes), runs a program of the packages: argv[0] is
/// one of their executables in /usr/bin (`pai-config`, `llama-*`).
/// pai-manager is not: it only runs as the manager's service.
fn runs_packaged_program(cmdline: &[u8]) -> bool {
    let argv0 = cmdline.split(|&b| b == 0).next().unwrap_or_default();
    argv0
        .strip_prefix(b"/usr/bin/")
        .is_some_and(|name| name == b"pai-config" || name.starts_with(b"llama-"))
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

/// The real uid of a process, from the `Uid:` line of its
/// `/proc/<pid>/status` (real, effective, saved, filesystem); None without one.
fn real_uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|v| v.split_whitespace().next())
        .and_then(|v| v.parse().ok())
}

/// The uid of the services' user (`private-ai`) in a passwd file's text, None
/// when the user does not exist (yet).
fn service_uid(passwd: &str) -> Option<u32> {
    passwd.lines().find_map(|l| {
        let mut fields = l.split(':');
        if fields.next() != Some("private-ai") {
            return None;
        }
        fields.nth(1).and_then(|uid| uid.parse().ok())
    })
}

/// Whether a process stops the install: a program of the packages
/// ([`runs_packaged_program`]) or a holder of their libraries
/// ([`holds_private_libraries`]) that a user runs, not the services (a
/// process of `service_uid`, which the maintainer scripts restart).
fn blocks_install(cmdline: &[u8], maps: &str, uid: Option<u32>, service_uid: Option<u32>) -> bool {
    let service = uid.is_some() && uid == service_uid;
    !service && (runs_packaged_program(cmdline) || holds_private_libraries(maps))
}

/// `<pid> <command line>` of every process that stops the install
/// ([`blocks_install`]): one that runs a program of the packages
/// ([`runs_packaged_program`], which sees every user's processes) or holds
/// one of their libraries ([`holds_private_libraries`], for the maps this
/// user can read), not the services' own, a line each.
fn private_library_holders() -> String {
    let service = service_uid(&std::fs::read_to_string("/etc/passwd").unwrap_or_default());
    let mut out = Vec::new();
    for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let pid = entry.file_name().to_string_lossy().into_owned();
        if !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let cmdline = std::fs::read(entry.path().join("cmdline")).unwrap_or_default();
        let maps = std::fs::read_to_string(entry.path().join("maps")).unwrap_or_default();
        let status = std::fs::read_to_string(entry.path().join("status")).unwrap_or_default();
        if blocks_install(&cmdline, &maps, real_uid(&status), service) {
            let cmdline = String::from_utf8_lossy(&cmdline).replace('\0', " ");
            out.push(format!("{pid} {}", cmdline.trim_end()));
        }
    }
    out.join("\n")
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
                output(
                    cmd("rpm")
                        .args(["-qp", "--qf", "%{VERSION}-%{RELEASE}"])
                        .arg(&file),
                )
                .unwrap_or_default(),
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
    // A runtime comes out of an SDK, which the build does not change.
    let built = util::build_root().join("llama.cpp-cmake/bin/llama-server");
    for p in &picked {
        let runtime = Component::RUNTIMES
            .iter()
            .any(|c| name(p).starts_with(&format!("{}-v", c.package())));
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
    let running = private_library_holders();
    if !running.is_empty() {
        ui::warn(format!("Running on the libraries of {PRIVATE_LIB_DIR}:"));
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
        let version = output(cmd("/usr/bin/pai-config").arg("--version"))
            .and_then(|s| s.split_whitespace().last().map(str::to_string))
            .unwrap_or_default();
        let matches = !version.is_empty()
            && p.new_version
                .strip_prefix(&version)
                .map(|rest| rest.starts_with('+') || rest.starts_with('-'))
                .unwrap_or(false);
        if !matches {
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

    /// The `Uid:` line of `/proc/<pid>/status` holds the real, effective,
    /// saved and filesystem uids, tab-separated (proc_pid_status(5)); the
    /// real one is the first. A status without the line, or a nonsense one,
    /// gives none.
    #[test]
    fn the_real_uid_is_the_first_of_the_status_uid_line() {
        let status = "Name:\tllama-server\nUmask:\t0022\nState:\tS (sleeping)\n\
                      Tgid:\t4242\nPid:\t4242\nPPid:\t1\n\
                      Uid:\t988\t988\t988\t988\nGid:\t985\t985\t985\t985\n";
        assert_eq!(real_uid(status), Some(988));
        assert_eq!(real_uid("Uid:\t1000\t0\t0\t0\n"), Some(1000));
        assert_eq!(real_uid("Name:\tx\nGid:\t5\t5\t5\t5\n"), None);
        assert_eq!(real_uid(""), None);
        assert_eq!(real_uid("Uid:\tnobody\n"), None);
        assert_eq!(real_uid("Uid:\n"), None);
        // Only a line that starts with the key.
        assert_eq!(real_uid("XUid:\t7\t7\t7\t7\n"), None);
    }

    /// The services' user, as systemd-sysusers writes it into /etc/passwd
    /// (`name:x:uid:gid:gecos:home:shell`); a user whose name only starts
    /// the same, a missing user or a broken line gives none.
    #[test]
    fn the_service_uid_comes_from_the_private_ai_passwd_entry() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\n\
                      private-ai-other:x:977:977::/:/usr/sbin/nologin\n\
                      private-ai:x:988:985:Private AI Framework:/var/lib/private-ai:/usr/sbin/nologin\n\
                      emiliano:x:1000:1000:,,,:/home/emiliano:/bin/bash\n";
        assert_eq!(service_uid(passwd), Some(988));
        assert_eq!(
            service_uid(
                "root:x:0:0:root:/root:/bin/bash\nprivate-ai-other:x:977:977::/:/bin/false\n"
            ),
            None
        );
        assert_eq!(service_uid(""), None);
        assert_eq!(
            service_uid("private-ai:x:notanumber:985::/:/bin/false\n"),
            None
        );
        assert_eq!(service_uid("private-ai\n"), None);
    }

    /// The instances run as the services' user and the maintainer scripts
    /// restart them: a llama-server of that uid, its libraries mapped, does
    /// not stop the install. The same llama-server, pai-config, a llama-bench
    /// or a process holding the libraries run by a user (or by root) does;
    /// so does every one of them when the services' user does not exist yet
    /// or the uid could not be read. pai-manager never does.
    #[test]
    fn only_a_users_program_blocks_the_install_never_the_services() {
        const SERVICE: u32 = 988;
        let server = b"/usr/bin/llama-server\0--port\08080\0".as_slice();
        let maps = "7f2a1c000000-7f2a1c200000 r-xp 00000000 103:02 1234567 \
                    /usr/lib/private-ai/libggml-base.so.0.9.0\n";
        let manager = b"/usr/libexec/private-ai/pai-manager\0serve\0".as_slice();

        assert!(!blocks_install(server, maps, Some(SERVICE), Some(SERVICE)));
        assert!(!blocks_install(server, "", Some(SERVICE), Some(SERVICE)));
        assert!(!blocks_install(b"", maps, Some(SERVICE), Some(SERVICE)));
        assert!(!blocks_install(manager, "", Some(0), Some(SERVICE)));
        assert!(!blocks_install(manager, "", Some(0), None));

        for uid in [Some(1000), Some(0), None] {
            assert!(blocks_install(server, maps, uid, Some(SERVICE)), "{uid:?}");
            assert!(blocks_install(server, "", uid, Some(SERVICE)), "{uid:?}");
            assert!(blocks_install(
                b"/usr/bin/pai-config\0",
                "",
                uid,
                Some(SERVICE)
            ));
            assert!(blocks_install(
                b"/usr/bin/llama-bench\0",
                "",
                uid,
                Some(SERVICE)
            ));
            assert!(blocks_install(
                b"/usr/bin/python3\0",
                maps,
                uid,
                Some(SERVICE)
            ));
        }
        // No services' user yet: nothing is exempt but pai-manager.
        assert!(blocks_install(server, maps, Some(SERVICE), None));
        assert!(blocks_install(server, maps, None, None));
        // Neither a program of the packages nor a holder of their libraries.
        assert!(!blocks_install(
            b"/usr/bin/bash\0",
            "",
            Some(1000),
            Some(SERVICE)
        ));
    }
}
