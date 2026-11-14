//! The `pai` executable as a user runs it. Every run gets its own data root
//! (`PRIVATE_AI_DATA_ROOT`, set on the child only, so no other test races it)
//! and a system bus that does not exist: nothing here reaches the real tree
//! or the real manager. Only commands that read neither are run.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// `pai <args>` with `root` as the whole data tree, killed after a deadline.
fn pai(root: &Path, args: &[&str]) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pai"))
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
        .expect("spawn pai");
    let deadline = Instant::now() + Duration::from_secs(60);
    while child.try_wait().expect("try_wait").is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("pai {}: still running after 60 s", args.join(" "));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().expect("pai output");
    Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// `--version` names the executable: `pai <framework version>`.
#[test]
fn version_names_pai() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r = pai(dir.path(), &["--version"]);
    assert_eq!(r.code, Some(0), "stderr: {}", r.stderr);
    assert_eq!(r.stdout, format!("pai {}\n", env!("CARGO_PKG_VERSION")));
}

/// `--help` is `pai`'s: the configurator's commands, no `gui` command, and
/// no other name.
#[test]
fn help_lists_the_commands_and_no_gui() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r = pai(dir.path(), &["--help"]);
    assert_eq!(r.code, Some(0), "stderr: {}", r.stderr);
    assert!(r.stdout.contains("Usage: pai "), "{}", r.stdout);
    let commands: Vec<&str> = r
        .stdout
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(
        commands,
        ["server", "preset", "control", "bench", "config", "help"],
        "{}",
        r.stdout
    );
    assert!(!r.stdout.contains("pai-config"), "{}", r.stdout);
    assert!(
        !r.stdout.contains("no args"),
        "no arguments is not the GUI any more: {}",
        r.stdout
    );
}

/// No arguments is not the GUI any more: clap's usage error, exit 2.
#[test]
fn no_arguments_is_a_usage_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r = pai(dir.path(), &[]);
    assert_eq!(r.code, Some(2), "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(r.stderr.contains("Usage: pai "), "{}", r.stderr);
}

/// The GUI's own arguments are pai-config's alone: `gui` and the HW
/// Monitor's hidden `sdk-devices` are unknown to `pai`.
#[test]
fn the_gui_arguments_are_not_pai_commands() {
    let dir = tempfile::tempdir().expect("tempdir");
    for args in [&["gui"][..], &["gui", "--minimized"], &["sdk-devices"]] {
        let r = pai(dir.path(), args);
        assert_eq!(r.code, Some(2), "pai {args:?}: {}", r.stderr);
        assert!(
            r.stderr
                .contains(&format!("unrecognized subcommand '{}'", args[0])),
            "pai {args:?}: {}",
            r.stderr
        );
    }
}

/// `pai` moves a 2.4 config tree at start, before it parses (so even
/// `--version` does): under the redirect the old tree sits in the redirected
/// home, the new one is `<root>/private-ai/config`.
#[test]
fn pai_moves_the_old_tree_at_start() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = dir.path().join(".config").join(concat!("llama", ".cpp"));
    std::fs::create_dir_all(&old).expect("old tree");
    std::fs::write(old.join("settings.ini"), "[Settings]\nLogRotate = false\n").expect("w");
    let r = pai(dir.path(), &["--version"]);
    assert_eq!(r.code, Some(0), "stderr: {}", r.stderr);
    let new = dir
        .path()
        .join("private-ai")
        .join("config")
        .join("settings.ini");
    assert_eq!(
        std::fs::read_to_string(&new).ok().as_deref(),
        Some("[Settings]\nLogRotate = false\n"),
        "{} moved to {}",
        old.display(),
        new.display()
    );
    assert!(
        !old.exists(),
        "the old tree is removed once the new one is complete"
    );
}
