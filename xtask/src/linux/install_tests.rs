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
        let d = std::env::temp_dir().join(format!("xtask-install-{name}-{}", std::process::id()));
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
