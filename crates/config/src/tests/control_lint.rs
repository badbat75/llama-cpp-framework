//! Every start, stop and run probe of llama-server goes through
//! `server_control`, which picks who runs it (the manager on Linux): only its
//! `Direct` backend may call `runstate::start`, `runstate::stop` and
//! `runstate::is_running`. A caller reaching them directly would spawn or kill
//! a process behind the manager's back.

use std::path::{Path, PathBuf};

/// The files allowed to call them: `runstate` itself and the seam.
const ALLOWED: [&str; 2] = ["runstate.rs", "server_control.rs"];

/// The functions only the seam may call.
const GUARDED: [&str; 3] = ["start", "stop", "is_running"];

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The guarded calls (and imports) on one line of code, comments left out.
fn hits(line: &str) -> Vec<String> {
    let code = line.split("//").next().unwrap_or(line);
    let mut found = Vec::new();
    let mut rest = code;
    while let Some(i) = rest.find("runstate::") {
        let after = &rest[i + "runstate::".len()..];
        // `use crate::runstate::{self, start}`: every name in the braces.
        let names: Vec<&str> = if let Some(inner) = after.strip_prefix('{') {
            let inner = inner.split('}').next().unwrap_or(inner);
            inner.split(',').map(str::trim).collect()
        } else {
            let end = after.find(|c: char| !is_ident(c)).unwrap_or(after.len());
            vec![&after[..end]]
        };
        for name in names {
            let name = name.split(" as ").next().unwrap_or(name).trim();
            if GUARDED.contains(&name) {
                found.push(format!("runstate::{name}"));
            }
        }
        rest = after;
    }
    found
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "tests") {
                sources(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn only_the_seam_starts_stops_or_probes_llama_server() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    let mut violations = String::new();
    for file in files {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        if ALLOWED.contains(&name.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("read source");
        for (n, line) in text.lines().enumerate() {
            for hit in hits(line) {
                violations.push_str(&format!(
                    "\n  {}:{}: {hit}",
                    file.strip_prefix(&src).unwrap().display(),
                    n + 1
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "call these through crate::server_control::backend(), not runstate:{violations}"
    );
}

#[test]
fn control_scan_flags_calls_imports_and_fn_pointers_and_leaves_the_rest() {
    for (line, expected) in [
        ("    let r = runstate::start();", vec!["runstate::start"]),
        ("    if !runstate::stop() {", vec!["runstate::stop"]),
        (
            "    let running = crate::runstate::is_running();",
            vec!["runstate::is_running"],
        ),
        (
            "        crate::runstate::is_running,",
            vec!["runstate::is_running"],
        ),
        (
            "use crate::runstate::{self, start, is_running as up};",
            vec!["runstate::start", "runstate::is_running"],
        ),
        ("use crate::runstate::stop;", vec!["runstate::stop"]),
        // Not guarded: constants, other functions, comments.
        ("    &[&runstate::STOP_GRACE.as_secs()]", vec![]),
        ("    let line = runstate::command_line();", vec![]),
        ("    runstate::start_something_else();", vec![]),
        ("    // `runstate::stop` would skip the snapshot", vec![]),
        ("/// `runstate::start` opens with a probe", vec![]),
        ("use crate::runstate;", vec![]),
    ] {
        assert_eq!(hits(line), expected, "{line:?}");
    }
}
