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

/// The `src/` of the crates that hold the configurator's code: the GUI (this
/// crate), the command line (pai-cli) and the logic they share (pai-app).
fn src_dirs() -> [PathBuf; 3] {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    ["app", "cli", "config"].map(|c| crates.join(c).join("src"))
}

/// A source's path from `crates/`, as a violation names it.
fn rel(file: &Path) -> &Path {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    file.strip_prefix(crates).unwrap_or(file)
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
    let mut files = Vec::new();
    for src in src_dirs() {
        sources(&src, &mut files);
    }
    let mut violations = String::new();
    for file in files {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        if ALLOWED.contains(&name.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("read source");
        for (n, line) in text.lines().enumerate() {
            for hit in hits(line) {
                violations.push_str(&format!("\n  {}:{}: {hit}", rel(&file).display(), n + 1));
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

// ── The configuration files ─────────────────────────────────────────────
//
// server.ini (`server@default.ini` under the manager), presets.ini and
// presets.ini.disabled are read and written only through `server_control`
// (`Backend::read_config`, `write_config`, `config_path`): under the manager
// they live in the system directory, and a reader of the per-user path would
// see a stale file, a writer would write one nobody reads. `runstate.rs` is
// the `Direct` backend's launch, which hands llama-server the per-user
// presets.ini. Test code (a `#[cfg(test)]` item, `src/tests/`) may name them.

/// The files allowed to name the per-user paths.
const CONFIG_ALLOWED: [&str; 2] = ["runstate.rs", "server_control.rs"];

/// The `paths::` functions only the seam may call.
const CONFIG_PATHS: [&str; 3] = ["server_ini", "presets_ini", "presets_disabled_ini"];

/// The file names only the seam may join onto `config_dir()`.
const CONFIG_NAMES: [&str; 3] = ["server.ini", "presets.ini", "presets.ini.disabled"];

/// The per-user config paths named on one line of code, comments left out.
fn config_hits(line: &str) -> Vec<String> {
    let code = line.split("//").next().unwrap_or(line);
    let mut found = Vec::new();
    let mut rest = code;
    while let Some(i) = rest.find("paths::") {
        let after = &rest[i + "paths::".len()..];
        let end = after.find(|c: char| !is_ident(c)).unwrap_or(after.len());
        if CONFIG_PATHS.contains(&&after[..end]) {
            found.push(format!("paths::{}", &after[..end]));
        }
        rest = after;
    }
    let mut rest = code;
    while let Some(i) = rest.find("config_dir()") {
        let after = rest[i + "config_dir()".len()..].trim_start();
        if let Some(arg) = after.strip_prefix(".join(\"") {
            let name = arg.split('"').next().unwrap_or("");
            if CONFIG_NAMES.contains(&name) {
                found.push(format!("config_dir().join(\"{name}\")"));
            }
        }
        rest = &rest[i + "config_dir()".len()..];
    }
    found
}

/// The lines of `text` outside `#[cfg(test)]` items, numbered from 1. An
/// item runs from the attribute to its `;` or to the `}` that closes it
/// (braces counted per line, comments left out).
fn production_lines(text: &str) -> Vec<(usize, &str)> {
    enum State {
        Code,
        /// After a test attribute, before the item's body opens.
        Header,
        /// Inside a test item's body, opened at this depth.
        Body(i64),
    }
    let mut state = State::Code;
    let mut depth: i64 = 0;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let code = line.split("//").next().unwrap_or(line);
        let t = code.trim();
        let opens = code.matches('{').count() as i64;
        let closes = code.matches('}').count() as i64;
        match state {
            State::Code => {
                if t.starts_with("#[cfg(test)]") || t.starts_with("#[cfg(all(test") {
                    state = State::Header;
                } else {
                    out.push((n + 1, line));
                }
                depth += opens - closes;
            }
            State::Header => {
                if t.starts_with("#[") {
                    continue;
                }
                if opens > closes {
                    state = State::Body(depth);
                } else if opens > 0 || t.ends_with(';') {
                    state = State::Code;
                }
                depth += opens - closes;
            }
            State::Body(d) => {
                depth += opens - closes;
                if depth <= d {
                    state = State::Code;
                }
            }
        }
    }
    out
}

#[test]
fn only_the_seam_reads_or_writes_the_config_files() {
    let mut files = Vec::new();
    for src in src_dirs() {
        sources(&src, &mut files);
    }
    let mut violations = String::new();
    for file in files {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        if CONFIG_ALLOWED.contains(&name.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("read source");
        for (n, line) in production_lines(&text) {
            for hit in config_hits(line) {
                violations.push_str(&format!("\n  {}:{n}: {hit}", rel(&file).display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "read, write or show these through crate::server_control::backend() \
         (read_config, write_config, config_path), not the per-user path:{violations}"
    );
}

#[test]
fn config_scan_flags_the_per_user_paths_and_leaves_the_rest() {
    for (line, expected) in [
        (
            "    let path = paths::server_ini();",
            vec!["paths::server_ini"],
        ),
        (
            "    read_presets(&crate::paths::presets_ini())",
            vec!["paths::presets_ini"],
        ),
        (
            "    listed_from(&paths::presets_ini(), &paths::presets_disabled_ini())",
            vec!["paths::presets_ini", "paths::presets_disabled_ini"],
        ),
        (
            "    strings::fill(\"wrote-file\", &[&paths::server_ini().display()])",
            vec!["paths::server_ini"],
        ),
        (
            "    let p = paths::config_dir().join(\"presets.ini\");",
            vec!["config_dir().join(\"presets.ini\")"],
        ),
        (
            "    let p = config_dir().join(\"presets.ini.disabled\");",
            vec!["config_dir().join(\"presets.ini.disabled\")"],
        ),
        (
            "    let p = config_dir().join(\"server.ini\");",
            vec!["config_dir().join(\"server.ini\")"],
        ),
        // Not guarded: the user's own files, other directories, comments.
        ("    let s = paths::settings_ini();", vec![]),
        ("    let d = paths::config_dir();", vec![]),
        (
            "    let d = paths::config_dir().join(\"settings.ini\");",
            vec![],
        ),
        ("    let p = dir.path().join(\"presets.ini\");", vec![]),
        ("    let p = paths::server_ini_backup();", vec![]),
        (
            "    // paths::server_ini() would be the stale per-user file",
            vec![],
        ),
        ("/// `paths::presets_ini()` is Direct's", vec![]),
    ] {
        assert_eq!(config_hits(line), expected, "{line:?}");
    }
}

#[test]
fn config_scan_skips_test_items_and_keeps_the_code_after_them() {
    let text = "\
fn load() {
    a();
}
#[cfg(test)]
mod tests {
    fn t() {
        b();
    }
}
fn save() {
    c();
}
#[cfg(test)]
pub(crate) fn e2e(
    x: u8,
) {
    d();
}
#[cfg(all(test, target_os = \"linux\"))]
#[allow(dead_code)]
fn linux_e2e() { e(); }
#[cfg(test)]
use std::fs;
fn rename() {
    f();
}
#[cfg(any(test, feature = \"test-support\"))]
fn both() {
    g();
}
";
    let kept: Vec<&str> = production_lines(text)
        .into_iter()
        .map(|(_, l)| l.trim())
        .filter(|l| l.ends_with("();"))
        .collect();
    assert_eq!(kept, ["a();", "c();", "f();", "g();"]);
    let numbers: Vec<usize> = production_lines(text)
        .into_iter()
        .filter(|(_, l)| l.trim() == "f();")
        .map(|(n, _)| n)
        .collect();
    assert_eq!(numbers, [25], "numbered as in the file");
}

// ── The instance's log ──────────────────────────────────────────────────
//
// The per-user `paths::server_log()` is the log only `Direct` writes
// (`runstate.rs`); under the manager the instance writes the unit's journal
// (Linux) or the service's file (Windows). Every reader goes through
// `server_control` (`Backend::log_source`, `Backend::log_text`), which picks
// the right one. Test code may name it.

/// The files allowed to name the per-user log.
const LOG_ALLOWED: [&str; 2] = ["runstate.rs", "server_control.rs"];

/// `paths::server_log` named on one line of code, comments left out.
fn log_hits(line: &str) -> Vec<String> {
    let code = line.split("//").next().unwrap_or(line);
    let mut found = Vec::new();
    let mut rest = code;
    while let Some(i) = rest.find("paths::") {
        let after = &rest[i + "paths::".len()..];
        let end = after.find(|c: char| !is_ident(c)).unwrap_or(after.len());
        if &after[..end] == "server_log" {
            found.push("paths::server_log".to_string());
        }
        rest = after;
    }
    found
}

#[test]
fn only_the_seam_reads_the_instances_log() {
    let mut files = Vec::new();
    for src in src_dirs() {
        sources(&src, &mut files);
    }
    let mut violations = String::new();
    for file in files {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        if LOG_ALLOWED.contains(&name.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("read source");
        for (n, line) in production_lines(&text) {
            for hit in log_hits(line) {
                violations.push_str(&format!("\n  {}:{n}: {hit}", rel(&file).display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "read or name the instance's log through crate::server_control::backend() \
         (log_source, log_text), not the per-user file:{violations}"
    );
}

#[test]
fn log_scan_flags_the_per_user_log_and_leaves_the_rest() {
    for (line, expected) in [
        (
            "        let log = std::fs::read(paths::server_log()).unwrap_or_default();",
            vec!["paths::server_log"],
        ),
        (
            "    strings::fill(\"server-gone\", &[&crate::paths::server_log().display()]),",
            vec!["paths::server_log"],
        ),
        // Not guarded: other paths, the instance's own log, comments.
        ("    let p = paths::server_log_dir();", vec![]),
        ("    let p = paths::instance_log(INSTANCE);", vec![]),
        ("    // paths::server_log() is Direct's", vec![]),
        ("/// `paths::server_log` is the per-user file", vec![]),
    ] {
        assert_eq!(log_hits(line), expected, "{line:?}");
    }
}

/// Under the manager (Linux, Windows) the OS starts the instance at boot:
/// pai-config's launch starts llama-server only through
/// `server_control::start_at_launch`, which says no there, never on
/// settings.ini's `StartServerOnLaunch` alone.
#[test]
fn the_launch_starts_the_server_only_through_start_at_launch() {
    let gui = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gui.rs");
    let text = std::fs::read_to_string(&gui).expect("read gui.rs");
    let code: Vec<&str> = production_lines(&text)
        .into_iter()
        .map(|(_, l)| l)
        .collect();
    let code = code.join("\n");
    assert!(
        code.contains("server_control::start_at_launch("),
        "gui.rs does not decide the launch's start with start_at_launch"
    );
    assert!(
        !code.contains("if settings::load().start_server_on_launch"),
        "gui.rs still starts the server on StartServerOnLaunch alone"
    );
}
