//! GT5a (`ARCHITECTURE.md` "ROCm instances load models with `load-mode = none`"):
//! on Windows the render adds nothing.
//!
//! Pinned as a text check of the manager's production code: every line that
//! names the `load-mode` key sits in an item the compiler builds only where
//! Windows never runs, that is an item whose own attribute block names `linux`
//! or `not(windows)`. The check reads the files whole, drops whole-line
//! comments, and stops at the first `#[cfg(test)]` of each file, so the e2e
//! phases' fixtures (which write the key as data) are out of its scope.
//!
//! What Linux proves: the shape of the gate. What it cannot: the Windows build
//! itself, which is the coder's
//! `cargo clippy -p pai-manager --all-targets --target x86_64-pc-windows-gnu`.
//! The rule is a day-one guard: today no production line names the key, so the
//! check passes, and it reddens the moment an ungated line writes it.

use std::path::{Path, PathBuf};

/// The words that open an item of a source file, after the visibility.
const ITEM_HEADS: [&str; 11] = [
    "fn ",
    "const ",
    "static ",
    "struct ",
    "enum ",
    "trait ",
    "mod ",
    "impl",
    "type ",
    "use ",
    "macro_rules",
];

/// `t` with the leading visibility dropped (`pub`, `pub(crate)`, `pub(super)`).
fn strip_vis(t: &str) -> &str {
    for v in ["pub(crate) ", "pub(super) ", "pub "] {
        if let Some(rest) = t.strip_prefix(v) {
            return strip_vis(rest);
        }
    }
    t
}

/// Whether `t` opens an item.
fn starts_item(t: &str) -> bool {
    let t = strip_vis(t.trim());
    ITEM_HEADS.iter().any(|h| t.starts_with(h))
}

/// The attribute block attached to the item that holds line `i`: the `#[...]`
/// lines directly above the nearest item-opening line at or before `i`. A line
/// of no letters (the tail of a multi-line attribute, a blank) is stepped over.
fn gate_of(lines: &[String], i: usize) -> String {
    let mut j = i;
    loop {
        if starts_item(&lines[j]) {
            let mut gate: Vec<String> = Vec::new();
            let mut k = j;
            while k > 0 {
                k -= 1;
                let a = lines[k].trim();
                if a.starts_with("#[") || a.starts_with("#![") {
                    gate.push(a.to_string());
                } else if a.chars().all(|c| !c.is_alphabetic()) {
                    continue;
                } else {
                    break;
                }
            }
            return gate.join(" ");
        }
        if j == 0 {
            return String::new();
        }
        j -= 1;
    }
}

/// The production lines of `path`: its lines up to the first `#[cfg(test)]`,
/// whole-line comments dropped.
fn production(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).expect("source readable");
    let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let end = lines
        .iter()
        .position(|l| l.trim() == "#[cfg(test)]")
        .unwrap_or(lines.len());
    lines[..end]
        .iter()
        .filter(|l| !l.trim_start().starts_with("//"))
        .cloned()
        .collect()
}

#[test]
fn gt5a_the_load_mode_key_is_written_only_where_windows_never_runs() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // The manager's own sources: `manager.rs` and the `.rs` files beside it.
    // `read_dir` is not recursive, so the crate's unit tests under `src/tests/`
    // are out of scope, which is the point: the rule is about production code.
    let mut files: Vec<PathBuf> = vec![src.join("manager.rs")];
    let mut from_dir: Vec<PathBuf> = std::fs::read_dir(src.join("manager"))
        .expect("the manager's sources")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    files.append(&mut from_dir);
    files.sort();
    for file in &files {
        let lines = production(file);
        for i in 0..lines.len() {
            let t = lines[i].trim();
            if !t.contains("load-mode") {
                continue;
            }
            let gate = gate_of(&lines, i);
            assert!(
                gate.contains("linux") || gate.contains("not(windows)"),
                "GT5a 9: {} line {}: `{t}` names the `load-mode` key inside an \
                 item with no Linux gate (its attributes: {gate:?}). On Windows \
                 the render adds nothing.",
                file.display(),
                i
            );
        }
    }
}
