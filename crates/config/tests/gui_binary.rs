//! The `pai-config` executable given arguments that open no window. Every run
//! gets its own data root (`PRIVATE_AI_DATA_ROOT`, set on the child only) and a
//! system bus that does not exist, so nothing reaches the real tree or the
//! real manager.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// `pai-config <args>` with `root` as the whole data tree, killed after a
/// deadline (a GUI that opened by mistake would otherwise hang the test).
fn pai_config(root: &Path, args: &[&str]) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pai-config"))
        .args(args)
        .env("PRIVATE_AI_DATA_ROOT", root)
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}", root.join("no-bus").display()),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pai-config");
    let deadline = Instant::now() + Duration::from_secs(60);
    while child.try_wait().expect("try_wait").is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("pai-config {}: still running after 60 s", args.join(" "));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().expect("pai-config output");
    Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// `xtask package` and `xtask install` read `pai-config --version` back and
/// take its last word as the framework version.
#[test]
fn version_is_kept_for_xtask() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r = pai_config(dir.path(), &["--version"]);
    assert_eq!(r.code, Some(0), "stderr: {}", r.stderr);
    assert_eq!(
        r.stdout,
        format!("pai-config {}\n", env!("CARGO_PKG_VERSION"))
    );
}

/// A command is `pai`'s now: one line on stderr naming the same command
/// under `pai`, from the strings file (not its key), and clap's usage exit
/// code, 2. Nothing runs: `--help` here would print help were the command
/// still parsed.
#[test]
fn a_command_points_to_pai() {
    let dir = tempfile::tempdir().expect("tempdir");
    for args in [&["preset", "list", "--help"][..], &["--help"]] {
        let r = pai_config(dir.path(), args);
        assert_eq!(
            r.code,
            Some(2),
            "{args:?}: stdout {} stderr {}",
            r.stdout,
            r.stderr
        );
        assert_eq!(r.stdout, "", "{args:?}");
        let lines: Vec<&str> = r.stderr.lines().collect();
        assert_eq!(lines.len(), 1, "{args:?}: one line: {}", r.stderr);
        let wanted = format!("pai {}", args.join(" "));
        assert!(
            lines[0].contains(&wanted),
            "{args:?}: names `{wanted}`: {}",
            lines[0]
        );
        assert_ne!(
            lines[0].trim(),
            wanted,
            "{args:?}: a sentence from the strings file, not only the command"
        );
    }
}
