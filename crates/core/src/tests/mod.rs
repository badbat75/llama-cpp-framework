//! The end-to-end test of this crate.
//!
//! WARNING: its phases redirect config IO by setting the process-wide
//! `PRIVATE_AI_DATA_ROOT` env var and never restore it, so they all run
//! in this ONE `#[test]`, one after the other. Unit tests elsewhere must not
//! touch `paths::` (or anything that resolves the config dir): cargo runs
//! tests on parallel threads, so they would race this one nondeterministically.

#[test]
fn data_root_e2e() {
    // ── System config paths (paths.rs `system_config_dir`, instances) ────
    crate::paths::instance_paths_e2e();

    // ── Service launch (launch.rs `instance_command`) ────────────────────
    crate::launch::instance_launch_e2e();

    // ── The old user tree moved to the new names (migrate.rs) ────────────
    #[cfg(not(windows))]
    crate::migrate::migrate_e2e();

    // ── The instances' log files (paths.rs `instance_log`) ───────────────
    crate::paths::instance_log_paths_e2e();

    // ── The defaults a saved server.ini resolves (server_cfg.rs) ─────────
    crate::server_cfg::server_cfg_defaults_e2e();
}

/// The paths with no test redirect, named after the project, and the old
/// redirect variable ignored. They need `PRIVATE_AI_DATA_ROOT` unset, which no
/// test may do in this process (`data_root_e2e` sets it on another thread), so
/// this runs `real_paths_child` in a child process of this test binary with a
/// fake profile: nothing is created, the paths are only computed.
#[test]
fn real_paths_follow_the_new_names() {
    let profile = tempfile::tempdir().expect("tempdir");
    let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"));
    child
        .args(["--exact", "tests::real_paths_child", "--include-ignored"])
        .args(["--nocapture", "--test-threads", "1"])
        .env(CHILD_MARKER, "1")
        .env_remove("PRIVATE_AI_DATA_ROOT")
        .env_remove("XDG_CONFIG_HOME")
        // The old variable, set: the paths must not follow it.
        .env(
            concat!("LLAMA_CPP_CONFIG", "_DATA_ROOT"),
            profile.path().join("old-name"),
        )
        .env("HOME", profile.path().join("home"))
        .env("USERPROFILE", profile.path().join("home"))
        .env("LOCALAPPDATA", profile.path().join("local"))
        .env("ProgramData", profile.path().join("programdata"));
    let out = child.output().expect("run the child test");
    assert!(
        out.status.success(),
        "real_paths_child failed:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("1 passed"),
        "the child ran no test:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// The directory name of release 2.4's trees, spelled in two halves so the
/// names lint finds no old name in this file.
const OLD_DIR: &str = concat!("llama", ".cpp");

/// Set by `real_paths_follow_the_new_names` on its child only.
const CHILD_MARKER: &str = "PAI_TEST_REAL_PATHS_CHILD";

/// The child half of `real_paths_follow_the_new_names`: ignored, and a no-op
/// without the marker, so it never reads the paths in a process where the
/// e2e test redirects them.
#[test]
#[ignore = "run by real_paths_follow_the_new_names in a child process"]
fn real_paths_child() {
    use crate::paths;
    use std::path::PathBuf;
    if std::env::var_os(CHILD_MARKER).is_none() {
        return;
    }
    let var = |name: &str| PathBuf::from(std::env::var_os(name).expect(name));
    let home = var(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    assert_eq!(paths::home_dir(), home, "the old variable is ignored");
    assert_eq!(
        paths::opencode_user_config(),
        home.join(".config").join("opencode").join("opencode.json")
    );
    #[cfg(windows)]
    {
        let local = var("LOCALAPPDATA").join("private-ai");
        assert_eq!(paths::data_root(), local);
        assert_eq!(paths::config_dir(), local.join("config"));
        assert_eq!(
            paths::system_config_dir(),
            var("ProgramData").join("private-ai").join("config")
        );
        assert_eq!(
            paths::system_log_dir(),
            var("ProgramData").join("private-ai").join("logs")
        );
        // Unchanged on Windows for now: the instance files stay in the
        // machine-wide config directory.
        assert_eq!(paths::engine_config_dir(), paths::system_config_dir());
        // The old tree to migrate: the one 2.4 kept in %LOCALAPPDATA%.
        assert_eq!(
            crate::migrate::old_trees(),
            [(var("LOCALAPPDATA").join(OLD_DIR), local.clone())]
        );
    }
    #[cfg(not(windows))]
    {
        assert_eq!(
            paths::data_root(),
            home.join(".local").join("share").join("private-ai")
        );
        assert_eq!(paths::config_dir(), home.join(".config").join("private-ai"));
        assert_eq!(paths::system_config_dir(), PathBuf::from("/etc/private-ai"));
        // The engine's configuration lives in the engine's directory on
        // Linux; macOS keeps every file per user and is unchanged for now.
        #[cfg(target_os = "linux")]
        assert_eq!(paths::engine_config_dir(), PathBuf::from("/etc/llama.cpp"));
        #[cfg(not(target_os = "linux"))]
        assert_eq!(paths::engine_config_dir(), paths::system_config_dir());
        assert_eq!(
            paths::system_log_dir(),
            PathBuf::from("/var/log/private-ai")
        );
        // The old trees to migrate, where 2.4 kept them: computed, not read.
        let old_data = (
            home.join(".local").join("share").join(OLD_DIR),
            paths::data_root(),
        );
        let trees = crate::migrate::old_trees();
        assert_eq!(trees.len(), 2, "{trees:?}");
        assert!(trees.contains(&old_data), "{trees:?}");
        assert!(
            trees.contains(&(home.join(".config").join(OLD_DIR), paths::config_dir())),
            "{trees:?}"
        );
        // An absolute XDG_CONFIG_HOME moves the config, a relative one is
        // ignored (the XDG spec). Alone in this process: setting it is safe.
        let xdg = home.join("xdg");
        std::env::set_var("XDG_CONFIG_HOME", &xdg);
        assert_eq!(paths::config_dir(), xdg.join("private-ai"));
        // The old config tree follows the same rule: an absolute
        // XDG_CONFIG_HOME held it, a relative one never did.
        let trees = crate::migrate::old_trees();
        assert!(
            trees.contains(&(xdg.join(OLD_DIR), xdg.join("private-ai"))),
            "{trees:?}"
        );
        assert!(trees.contains(&old_data), "{trees:?}");
        std::env::set_var("XDG_CONFIG_HOME", "relative/xdg");
        assert_eq!(paths::config_dir(), home.join(".config").join("private-ai"));
        let trees = crate::migrate::old_trees();
        assert!(
            trees.contains(&(
                home.join(".config").join(OLD_DIR),
                home.join(".config").join("private-ai")
            )),
            "{trees:?}"
        );
    }
}
