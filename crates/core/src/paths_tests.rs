use super::{
    installed_share_candidates, installed_share_dir, llama_bin_from, llama_bin_with_engine,
    macos_engine_install_dir, newest_version, valid_instance_id, windows_machine_dirs,
};
use std::path::{Path, PathBuf};

/// An empty file at `root/rel`, its directories made.
fn touch(root: &Path, rel: &str) -> PathBuf {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "").unwrap();
    p
}

fn canon(p: Option<PathBuf>) -> Option<PathBuf> {
    p.map(|p| p.canonicalize().unwrap())
}

// ── The machine-wide directories on Windows ──────────────────────────
// Pure over the `%ProgramData%` root, so the Windows layout is pinned
// on every OS: everything of the engine's instances under
// `%ProgramData%\llama.cpp\` (config, logs, the default models), only the
// framework's own config (`bff.ini`) under `%ProgramData%\private-ai\`.

/// The engine's directory name, as `/etc/llama.cpp` on Linux.
const ENGINE: &str = "llama.cpp";

#[test]
fn windows_instance_dirs_are_the_engine_s_under_program_data() {
    let pd = Path::new(r"C:\ProgramData");
    let d = windows_machine_dirs(pd);
    assert_eq!(d.engine_config, pd.join(ENGINE).join("config"));
    assert_eq!(d.instance_logs, pd.join(ENGINE).join("logs"));
    assert_eq!(d.instance_models, pd.join(ENGINE).join("models"));
    // The instances' slot snapshots: the engine's too, not the
    // LocalSystem profile the manager runs as.
    assert_eq!(d.instance_state, pd.join(ENGINE).join("state"));
}

#[test]
fn windows_framework_config_stays_in_private_ai() {
    let pd = Path::new(r"C:\ProgramData");
    let d = windows_machine_dirs(pd);
    assert_eq!(d.framework_config, pd.join("private-ai").join("config"));
    assert_ne!(d.framework_config, d.engine_config, "not the engine's");
}

/// The root is whatever `%ProgramData%` says, never a fixed `C:`.
#[test]
fn windows_machine_dirs_follow_the_program_data_root() {
    let pd = Path::new(r"D:\Data");
    let d = windows_machine_dirs(pd);
    for p in [
        &d.framework_config,
        &d.engine_config,
        &d.instance_logs,
        &d.instance_models,
        &d.instance_state,
    ] {
        assert!(p.starts_with(pd), "{}", p.display());
    }
    assert_eq!(d.engine_config, pd.join(ENGINE).join("config"));
}

// ── The build tree of each OS ────────────────────────────────────────
// Every OS builds into its own `build/<os>/` (xtask's `util::build_root`):
// a tree build on macOS finds its llama-server under `build/macos/`,
// never under Linux's directory.

#[test]
fn each_os_has_its_own_build_directory() {
    assert_eq!(super::build_os_dir("linux"), "linux");
    assert_eq!(super::build_os_dir("macos"), "macos");
    assert_eq!(super::build_os_dir("windows"), "windows");
}

/// `llama_bin_from` looks under the running OS's build directory,
/// through `build_os_dir`, with no per-OS constant of its own (the
/// macOS branch cannot run here).
#[test]
fn llama_bin_from_looks_under_the_running_os_build_directory() {
    let body = body_of("llama_bin_from");
    assert!(
        body.contains("build_os_dir(std::env::consts::OS)"),
        "llama_bin_from does not ask build_os_dir for this OS:\n{body}"
    );
    assert!(
        !body.contains("BUILD_OS_DIR"),
        "llama_bin_from still has its own per-OS constant:\n{body}"
    );
}

/// A tree build run from `<repo>/<dir>/<exe>` finds the llama-server
/// of this OS's build directory.
#[test]
fn a_tree_build_finds_llama_server_under_its_os_directory() {
    let root = tempfile::tempdir().unwrap();
    let os = super::build_os_dir(std::env::consts::OS);
    let exe = touch(
        root.path(),
        &format!("target/{}", super::binary_name("pai-config")),
    );
    let server = touch(
        root.path(),
        &format!(
            "build/{os}/llama.cpp-cmake/bin/{}",
            super::binary_name("llama-server")
        ),
    );
    assert_eq!(
        canon(llama_bin_from(&exe, "llama-server")),
        canon(Some(server))
    );
}

/// The code of `name`'s body in this file, before the tests.
fn body_of(name: &str) -> String {
    let code = include_str!("paths.rs")
        .split("#[cfg(test)]\nmod tests")
        .next()
        .unwrap();
    let start = code
        .find(&format!("pub fn {name}("))
        .unwrap_or_else(|| panic!("no fn {name}"));
    let rest = &code[start..];
    rest[..rest.find("\n}\n").expect("end of fn")].to_string()
}

/// The real Windows paths go through the pure function, so what the
/// tests above pin is what Windows gets (the Windows branches only
/// type-check on Linux).
#[test]
fn the_windows_branches_use_windows_machine_dirs() {
    for name in [
        "system_config_dir",
        "engine_config_dir",
        "system_log_dir",
        "instance_models_dir",
        "instance_state_dir",
    ] {
        let body = body_of(name);
        assert!(
            body.contains("windows_machine_dirs("),
            "{name} does not use windows_machine_dirs:\n{body}"
        );
    }
}

// ── llama-server from llama.cpp's own install (Windows) ──────────────
// The framework's MSI holds no llama.cpp file: pai-manager in
// `%ProgramFiles%\private-ai\bin` finds llama-server in the folder
// llama.cpp's MSI registers (`HKLM\Software\llama.cpp` `InstallDir`),
// under its `bin\`. Spelled with `/` here, the same components.

/// The framework's install finds the engine's llama-server (and
/// llama-bench) in `<engine InstallDir>\bin`.
#[test]
fn the_framework_finds_llama_server_in_the_engine_install() {
    let root = tempfile::tempdir().unwrap();
    let engine = root.path().join("Program Files/llama.cpp");
    let server = touch(
        &engine,
        &format!("bin/{}", super::binary_name("llama-server")),
    );
    let bench = touch(
        &engine,
        &format!("bin/{}", super::binary_name("llama-bench")),
    );
    let manager = touch(root.path(), "Program Files/private-ai/bin/pai-manager.exe");
    assert_eq!(
        canon(llama_bin_with_engine(
            &manager,
            "llama-server",
            Some(&engine)
        )),
        Some(server.canonicalize().unwrap())
    );
    assert_eq!(
        canon(llama_bin_with_engine(
            &manager,
            "llama-bench",
            Some(&engine)
        )),
        Some(bench.canonicalize().unwrap())
    );
}

/// No engine registered, or one registered whose llama-server is gone:
/// nothing, not a guess.
#[test]
fn no_engine_or_an_empty_engine_finds_nothing() {
    let root = tempfile::tempdir().unwrap();
    let manager = touch(root.path(), "Program Files/private-ai/bin/pai-manager.exe");
    assert_eq!(llama_bin_with_engine(&manager, "llama-server", None), None);
    let engine = root.path().join("Program Files/llama.cpp");
    std::fs::create_dir_all(engine.join("bin")).unwrap();
    assert_eq!(
        llama_bin_with_engine(&manager, "llama-server", Some(&engine)),
        None
    );
}

/// A tree build keeps running the llama-server it built, even with an
/// engine installed; without an engine it is `llama_bin_from`.
#[cfg(target_os = "linux")]
#[test]
fn a_tree_build_wins_over_the_installed_engine() {
    let root = tempfile::tempdir().unwrap();
    let tree = touch(root.path(), "build/linux/llama.cpp-cmake/bin/llama-server");
    let engine = root.path().join("Program Files/llama.cpp");
    touch(&engine, "bin/llama-server");
    let exe = touch(root.path(), "build/linux/cargo-target/release/pai-manager");
    assert_eq!(
        canon(llama_bin_with_engine(&exe, "llama-server", Some(&engine))),
        Some(tree.canonicalize().unwrap())
    );
    assert_eq!(
        llama_bin_with_engine(&exe, "llama-server", None),
        llama_bin_from(&exe, "llama-server")
    );
}

/// The running program's search (`llama_bin`, behind
/// `llama_server_exe`) goes through the engine's registration, read from
/// `HKLM\Software\llama.cpp` `InstallDir` (the Windows branch only
/// type-checks on Linux).
#[test]
fn llama_bin_asks_the_engine_registration() {
    let body = body_of("llama_bin");
    assert!(
        body.contains("llama_bin_with_engine(") && body.contains("engine_install_dir()"),
        "llama_bin does not look in the engine's install:\n{body}"
    );
    let reg = body_of("engine_install_dir");
    assert!(reg.contains(r#"r"Software\llama.cpp""#), "{reg}");
    assert!(reg.contains(r#""InstallDir""#), "{reg}");
}

// ── llama-server from llama.cpp's own .pkg (macOS) ───────────────────
// The engine's `.pkg` installs under `/opt/llama.cpp`, apart from the
// framework's `/opt/private-ai`: `pai-config` there finds llama-server
// in the engine's `bin/` when no tree build is around. Pure over the
// filesystem root, so it is pinned on Linux.

/// The engine's tree when it holds `bin/llama-server`.
#[test]
fn the_macos_engine_install_is_found_under_the_root() {
    let root = tempfile::tempdir().unwrap();
    touch(root.path(), "opt/llama.cpp/bin/llama-server");
    assert_eq!(
        macos_engine_install_dir(root.path()),
        Some(root.path().join("opt").join(ENGINE))
    );
}

/// No engine, an empty one, or a `llama-server` that is a directory:
/// nothing, not a guess. 2.4's framework tree (`/opt/private-ai` never
/// held llama-server since the split) is not the engine.
#[test]
fn no_macos_engine_install_finds_nothing() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(macos_engine_install_dir(root.path()), None);
    std::fs::create_dir_all(root.path().join("opt").join(ENGINE).join("bin")).unwrap();
    assert_eq!(macos_engine_install_dir(root.path()), None);
    std::fs::create_dir_all(
        root.path()
            .join("opt")
            .join(ENGINE)
            .join("bin")
            .join("llama-server"),
    )
    .unwrap();
    assert_eq!(macos_engine_install_dir(root.path()), None);
    let other = tempfile::tempdir().unwrap();
    touch(other.path(), "opt/private-ai/bin/llama-server");
    assert_eq!(macos_engine_install_dir(other.path()), None);
}

/// `pai-config` in `/opt/private-ai/bin` finds the engine's
/// llama-server and llama-bench through it.
#[cfg(unix)]
#[test]
fn the_macos_framework_finds_llama_server_in_the_engine_pkg() {
    let root = tempfile::tempdir().unwrap();
    let server = touch(root.path(), "opt/llama.cpp/bin/llama-server");
    let bench = touch(root.path(), "opt/llama.cpp/bin/llama-bench");
    let config = touch(root.path(), "opt/private-ai/bin/pai-config");
    let engine = macos_engine_install_dir(root.path());
    assert_eq!(
        canon(llama_bin_with_engine(
            &config,
            "llama-server",
            engine.as_deref()
        )),
        Some(server.canonicalize().unwrap())
    );
    assert_eq!(
        canon(llama_bin_with_engine(
            &config,
            "llama-bench",
            engine.as_deref()
        )),
        Some(bench.canonicalize().unwrap())
    );
}

/// On macOS the running program's engine is the `.pkg` install under
/// `/`: `engine_install_dir` asks `macos_engine_install_dir` (the macOS
/// branch only type-checks there).
#[test]
fn engine_install_dir_asks_the_macos_pkg_install() {
    let body = body_of("engine_install_dir");
    assert!(body.contains(r#"target_os = "macos""#), "{body}");
    assert!(
        body.contains(r#"macos_engine_install_dir(Path::new("/"))"#),
        "engine_install_dir does not look under / on macOS:\n{body}"
    );
}

/// No llama-server anywhere it is looked for: nothing, not a guess.
#[test]
fn llama_server_missing_is_none() {
    let root = tempfile::tempdir().unwrap();
    let manager = touch(root.path(), "usr/libexec/private-ai/pai-manager");
    let config = touch(root.path(), "usr/bin/pai-config");
    assert_eq!(llama_bin_from(&manager, "llama-server"), None);
    assert_eq!(llama_bin_from(&config, "llama-server"), None);
}

/// The Linux packages: `pai-config` beside llama-server in /usr/bin, and
/// `pai-manager` in /usr/libexec/private-ai, which finds it there too.
#[cfg(target_os = "linux")]
#[test]
fn the_usr_layout_finds_llama_server_in_usr_bin() {
    let root = tempfile::tempdir().unwrap();
    let server = touch(root.path(), "usr/bin/llama-server");
    let bench = touch(root.path(), "usr/bin/llama-bench");
    let config = touch(root.path(), "usr/bin/pai-config");
    let manager = touch(root.path(), "usr/libexec/private-ai/pai-manager");
    let want = Some(server.canonicalize().unwrap());
    assert_eq!(canon(llama_bin_from(&config, "llama-server")), want);
    assert_eq!(canon(llama_bin_from(&manager, "llama-server")), want);
    assert_eq!(
        canon(llama_bin_from(&manager, "llama-bench")),
        Some(bench.canonicalize().unwrap())
    );
}

/// A tree build keeps finding the llama-server it built: from the build
/// scripts' cargo target dir and from a bare `cargo build` at the root.
#[cfg(target_os = "linux")]
#[test]
fn a_tree_build_finds_its_own_llama_server() {
    let root = tempfile::tempdir().unwrap();
    let server = touch(root.path(), "build/linux/llama.cpp-cmake/bin/llama-server");
    let want = Some(server.canonicalize().unwrap());
    for exe in [
        "build/linux/cargo-target/debug/pai-config",
        "build/linux/cargo-target/release/pai-manager",
        "target/release/pai-config",
    ] {
        let exe = touch(root.path(), exe);
        assert_eq!(
            canon(llama_bin_from(&exe, "llama-server")),
            want,
            "{}",
            exe.display()
        );
    }
}

/// Nothing installed beside the program: `None`, so the caller falls back
/// to the repo's own files (a tree build).
#[test]
fn no_installed_share_is_none() {
    let root = tempfile::tempdir().unwrap();
    let exe = touch(root.path(), "build/linux/cargo-target/debug/pai-config");
    assert_eq!(installed_share_dir(&exe, "benchmarks"), None);
    assert_eq!(installed_share_dir(&exe, "strings"), None);
    let exe = touch(root.path(), "usr/bin/pai-config");
    assert_eq!(installed_share_dir(&exe, "benchmarks"), None);
}

/// The Linux packages install the benchmark method and the language files
/// in /usr/share/private-ai.
#[test]
fn the_usr_layout_finds_its_data_in_usr_share_private_ai() {
    let root = tempfile::tempdir().unwrap();
    let exe = touch(root.path(), "usr/bin/pai-config");
    for sub in ["benchmarks", "strings"] {
        let dir = root.path().join("usr/share/private-ai").join(sub);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            canon(installed_share_dir(&exe, sub)),
            Some(dir.canonicalize().unwrap()),
            "{sub}"
        );
    }
}

/// /usr/share/<sub> is whatever else a system installed there: the
/// package's own directory wins.
#[test]
fn the_package_s_own_data_wins_over_a_stray_one() {
    let root = tempfile::tempdir().unwrap();
    let exe = touch(root.path(), "usr/bin/pai-config");
    std::fs::create_dir_all(root.path().join("usr/share/strings")).unwrap();
    let ours = root.path().join("usr/share/private-ai/strings");
    std::fs::create_dir_all(&ours).unwrap();
    assert_eq!(
        canon(installed_share_dir(&exe, "strings")),
        Some(ours.canonicalize().unwrap())
    );
}

/// Where each install puts the read-only data (the benchmark method,
/// the strings files), looked for first: for a program in /usr/bin AND
/// in /usr/libexec/private-ai (pai-manager, pai-bff) the Linux packages'
/// /usr/share/private-ai, never /usr/libexec/share; for macOS
/// /opt/private-ai/bin, /opt/private-ai/share; for Windows
/// <InstallDir>\bin, <InstallDir>\share (spelled with `/` here, the
/// same components).
#[test]
fn the_share_dir_is_looked_for_where_each_install_puts_it() {
    let first = |exe: &str| {
        installed_share_candidates(Path::new(exe), "strings")
            .into_iter()
            .next()
    };
    assert_eq!(
        first("/usr/bin/pai"),
        Some(PathBuf::from("/usr/share/private-ai/strings"))
    );
    for exe in [
        "/usr/libexec/private-ai/pai-bff",
        "/usr/libexec/private-ai/pai-manager",
    ] {
        let all = installed_share_candidates(Path::new(exe), "strings");
        assert_eq!(
            all.first(),
            Some(&PathBuf::from("/usr/share/private-ai/strings")),
            "{exe}: {all:?}"
        );
        assert!(
            !all.iter().any(|d| d.starts_with("/usr/libexec")),
            "{exe}: {all:?}"
        );
    }
    for (exe, want) in [
        ("/opt/private-ai/bin/pai", "/opt/private-ai/share/strings"),
        (
            "/C/Program Files/private-ai/bin/pai-bff.exe",
            "/C/Program Files/private-ai/share/strings",
        ),
    ] {
        let all = installed_share_candidates(Path::new(exe), "strings");
        assert!(all.contains(&PathBuf::from(want)), "{exe}: {all:?}");
    }
    let all = installed_share_candidates(Path::new("/usr/bin/pai"), "benchmarks");
    assert_eq!(
        all.first(),
        Some(&PathBuf::from("/usr/share/private-ai/benchmarks"))
    );
}

/// The Linux packages' services in /usr/libexec/private-ai find the
/// installed strings, so their error lines are texts, not bare keys.
#[test]
fn a_program_in_libexec_finds_the_usr_share_data() {
    let root = tempfile::tempdir().unwrap();
    let ours = root.path().join("usr/share/private-ai/strings");
    std::fs::create_dir_all(&ours).unwrap();
    for exe in [
        "usr/libexec/private-ai/pai-bff",
        "usr/libexec/private-ai/pai-manager",
    ] {
        let exe = touch(root.path(), exe);
        assert_eq!(
            canon(installed_share_dir(&exe, "strings")),
            Some(ours.canonicalize().unwrap()),
            "{}",
            exe.display()
        );
    }
}

/// A tree under its own prefix (macOS, /opt/private-ai) keeps its
/// `share/` beside `bin/`.
#[test]
fn a_prefix_tree_finds_its_data_beside_bin() {
    let root = tempfile::tempdir().unwrap();
    let exe = touch(root.path(), "opt/private-ai/bin/pai-config");
    let dir = root.path().join("opt/private-ai/share/benchmarks");
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(
        canon(installed_share_dir(&exe, "benchmarks")),
        Some(dir.canonicalize().unwrap())
    );
}

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn newest_version_compares_numerically_not_lexically() {
    let n = names(&["6.2", "10.0", "7.1"]);
    assert_eq!(newest_version(&n).map(String::as_str), Some("10.0"));
}

#[test]
fn newest_version_handles_mixed_depth_and_ignores_non_numeric() {
    let n = names(&["6.4.2", "6.4", "temp", "7.1-beta"]);
    assert_eq!(newest_version(&n).map(String::as_str), Some("6.4.2"));
}

/// A crafted id never yields a log path; refused before any path is
/// read, so this needs no redirect.
#[test]
fn invalid_ids_have_no_log() {
    for bad in [
        "", "../x", "..", "a/b", "a\\b", "Default", "a@b", "a b", "x.log",
    ] {
        assert_eq!(super::instance_log(bad), None, "id {bad:?}");
    }
}

#[test]
fn valid_instance_id_accepts_short_lowercase_names() {
    let max = "a".repeat(32);
    for id in [
        "default",
        "lan-2",
        "8080",
        "a",
        "0",
        "gpu-0-r9700",
        max.as_str(),
    ] {
        assert!(valid_instance_id(id), "id {id:?} should be valid");
    }
}

#[test]
fn valid_instance_id_refuses_empty_long_and_unsafe_names() {
    let too_long = "a".repeat(33);
    for id in [
        "",
        too_long.as_str(),
        "Default",
        "LAN",
        "a_b",
        "a.b",
        "..",
        "a/b",
        "../x",
        "a\\b",
        "a@b",
        "a b",
        " a",
        "-a",
        "-",
        "caf\u{e9}",
        "a\n",
    ] {
        assert!(!valid_instance_id(id), "id {id:?} should be refused");
    }
}

#[test]
fn newest_version_none_when_nothing_version_shaped() {
    assert_eq!(newest_version(&names(&["bin", "docs"])), None);
    assert_eq!(newest_version(&[]), None);
}
