//! The temp-dir hygiene of every crate's tests: a folder made by `tempfile`
//! is owned by a guard, and every write under it happens while that guard
//! lives. The suite-wide redirect (`paths::redirect_for_tests`, which sets
//! `PRIVATE_AI_DATA_ROOT` for the whole process and never restores it) makes a
//! dropped guard expensive: the folder is deleted, the redirect still points
//! at it, and the next write under the data root recreates a tree that no
//! guard owns, in `$TMPDIR`, one per test run, forever. A plain text scan of
//! every `.rs` under `crates/`, comments left out.
//!
//! The shape the scan cannot see is a guard bound in a nested block that
//! closes before the writes: the braces in Rust strings make a text scan of
//! block ranges unreliable. That shape is checked by running a crate's suite
//! with `TMPDIR` set to a fresh empty directory and counting the entries left
//! in it (`PLAN.md`: a run must leave none).

use std::path::{Path, PathBuf};

/// The crates' root, from this crate's manifest.
fn crates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Every `.rs` under `dir`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The code lines of a source, `//` comments left out (the docs name these
/// APIs where they explain the rule).
fn code(source: &str) -> String {
    source
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A line that binds a temp-dir guard: a fresh `tempfile`, or a `TempDir`
/// arriving from a helper (a parameter, a returned tuple, a closure).
fn binds_guard(line: &str) -> bool {
    line.contains("tempdir()")
        || line.contains("tempdir_in(")
        || line.contains("TempDir::new")
        || line.contains("tempfile::Builder")
        || line.contains("TempDir")
}

/// The hygiene breaks of one source: `(1-based line, what breaks)`.
fn breaks(source: &str) -> Vec<(usize, &'static str)> {
    let stripped = code(source);
    let lines: Vec<&str> = stripped.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        // The redirect must point at a folder a guard owns, bound right there:
        // a guard bound further away may already be gone, and one named in a
        // static is gone as soon as the block that filled it closes.
        if line.contains("redirect_for_tests(") && !line.contains("fn redirect_for_tests") {
            let bound = lines[i.saturating_sub(4)..i]
                .iter()
                .any(|prev| binds_guard(prev));
            if !bound {
                out.push((
                    i + 1,
                    "redirect_for_tests with no temp-dir guard bound in the lines above: the \
                     redirect stays in force, and a later write recreates a deleted folder as \
                     a tree nothing owns",
                ));
            }
        }
        // The two calls that hand a folder to no one: they keep it, so nothing
        // removes it. The path they return outlives any guard.
        if line.contains(".into_path()") {
            out.push((
                i + 1,
                "into_path() on a temp dir: the folder is left behind on purpose, with no guard \
                 to remove it",
            ));
        }
        if line.contains(".keep()") {
            out.push((
                i + 1,
                "keep() on a temp dir: the guard gives up the folder, nothing removes it",
            ));
        }
        // A guard in a static, a `OnceLock` or a `LazyLock` is never dropped:
        // its folder outlives the process, and the tests that run after it
        // share a tree they did not create.
        if line.contains("TempDir")
            && (line.contains("static ") || line.contains("OnceLock") || line.contains("LazyLock"))
        {
            out.push((
                i + 1,
                "a temp-dir guard in a static: it is never dropped, so its folder outlives the \
                 run and later tests write under a tree they do not own",
            ));
        }
    }
    out
}

/// Every crate's tests: no hygiene break.
#[test]
fn a_temp_dir_guard_outlives_every_write_under_it() {
    let root = crates();
    let mut files = Vec::new();
    rust_files(&root.join("core"), &mut files);
    for name in ["app", "cli", "config", "manager", "bff"] {
        rust_files(&root.join(name), &mut files);
    }
    let mut bad = Vec::new();
    for path in &files {
        // This file names the breaks to check them; the scan is not for itself.
        if path.file_name().is_some_and(|n| n == "redirect_lint.rs") {
            continue;
        }
        let source = std::fs::read_to_string(path).expect("read source");
        for (line, what) in breaks(&source) {
            bad.push(format!(
                "{}:{}: {what}",
                path.strip_prefix(&root).unwrap_or(path).display(),
                line
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} hygiene break(s):\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// The scan sees the breaks it is for, and lets the safe shape pass: a
/// redirect bound to a guard next to it, a guard arriving from a helper.
#[test]
fn the_scan_flags_the_breaks_and_passes_the_safe_shape() {
    let leaks = "\
#[test]
fn a_static_holds_the_guard() {
    static DIR: OnceLock<TempDir> = OnceLock::new();
}
#[test]
fn the_folder_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.into_path();
    paths::redirect_for_tests(&path);
}
#[test]
fn the_guard_gives_up() {
    paths::redirect_for_tests(kept.path());
}
";
    let found = breaks(leaks);
    assert_eq!(
        found.iter().map(|(line, _)| *line).collect::<Vec<_>>(),
        vec![3, 8, 13],
        "the guard in a static, the folder kept on purpose, the redirect pointing \
         at a path no guard of this test owns"
    );

    let safe = "\
fn fresh() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    paths::redirect_for_tests(dir.path());
    (dir, crate::paths::engine_config_dir())
}
fn kept(dir: &TempDir) {
    paths::redirect_for_tests(dir.path());
}
";
    assert_eq!(breaks(safe), Vec::new(), "the guard travels with the path");
}
