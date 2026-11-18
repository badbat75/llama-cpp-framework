//! Every start, stop and run probe of llama-server goes through
//! `server_control`, which picks who runs it (the manager on Linux): only its
//! `Direct` backend may call `runstate::start`, `runstate::stop` and
//! `runstate::is_running`. A caller reaching them directly would spawn or kill
//! a process behind the manager's back.

use std::path::{Path, PathBuf};

/// The scan shared with pai-cli's lint of `snapshot_before_stop`
/// (`crates/app/src/tests/gate_lint.rs`).
#[path = "../../../app/src/tests/gate_lint.rs"]
mod gate_lint;

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
// server.ini (`server@default.ini` under the manager), models.ini and
// models.ini.disabled are read and written only through `server_control`
// (`Backend::read_config`, `write_config`, `config_path`): under the manager
// they live in the system directory, and a reader of the per-user path would
// see a stale file, a writer would write one nobody reads. `runstate.rs` is
// the `Direct` backend's launch, which hands llama-server the per-user
// models.ini. Test code (a `#[cfg(test)]` item, `src/tests/`) may name them.

/// The files allowed to name the per-user paths.
const CONFIG_ALLOWED: [&str; 2] = ["runstate.rs", "server_control.rs"];

/// The `paths::` functions only the seam may call.
const CONFIG_PATHS: [&str; 3] = ["server_ini", "models_ini", "models_disabled_ini"];

/// The file names only the seam may join onto `config_dir()`.
const CONFIG_NAMES: [&str; 3] = ["server.ini", "models.ini", "models.ini.disabled"];

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
            "    read_presets(&crate::paths::models_ini())",
            vec!["paths::models_ini"],
        ),
        (
            "    listed_from(&paths::models_ini(), &paths::models_disabled_ini())",
            vec!["paths::models_ini", "paths::models_disabled_ini"],
        ),
        (
            "    strings::fill(\"wrote-file\", &[&paths::server_ini().display()])",
            vec!["paths::server_ini"],
        ),
        (
            "    let p = paths::config_dir().join(\"models.ini\");",
            vec!["config_dir().join(\"models.ini\")"],
        ),
        (
            "    let p = config_dir().join(\"models.ini.disabled\");",
            vec!["config_dir().join(\"models.ini.disabled\")"],
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
        ("    let p = dir.path().join(\"models.ini\");", vec![]),
        ("    let p = paths::server_ini_backup();", vec![]),
        (
            "    // paths::server_ini() would be the stale per-user file",
            vec![],
        ),
        ("/// `paths::models_ini()` is Direct's", vec![]),
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

// ── The stop worker saves snapshots only for the server it stops locally ──
//
// Under the manager the instance service saves the slot snapshots at its own
// stop, so a stop from the GUI saves none of its own; the local backend
// (macOS) keeps the client-side save. The decision is
// `server_control::client_saves_snapshots(backend)`, and the save (and the
// progress line printed with it) sits behind it. Checked on the text: the
// worker runs on a spawned thread (a Direct stop there would SIGTERM a
// llama-server that is really running, and `fake::scoped` does not reach a
// spawned thread), and no e2e phase drives the stop path.

/// The body of the GUI's stop worker, comments and test items left out.
fn stop_worker() -> String {
    let gui = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gui.rs");
    let text = std::fs::read_to_string(&gui).expect("read gui.rs");
    let code = gate_lint::strip_comments(
        &production_lines(&text)
            .into_iter()
            .map(|(_, l)| l)
            .collect::<Vec<_>>()
            .join("\n"),
    );
    gate_lint::fn_body(&code, "stop_server_async_with")
}

/// Whether the gate `gate(..)` asks the backend the worker stops: the call
/// names `backend()` in its own argument, or an argument bound by a
/// `let <name> = ...backend();` before the gate. A gate asked of nothing in
/// particular (a config, a stale local) decides for no backend.
fn gate_asks_the_backend(body: &str, gate: &str) -> bool {
    let Some(at) = body.find(gate) else {
        return false;
    };
    // The gate text includes its opening parenthesis: the argument list runs
    // from there to the closing one that brings the depth back to 0.
    let rest = &body[at + gate.len()..];
    let mut depth = 1_i64;
    let mut end = rest.len();
    for (i, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    let arg = &rest[..end];
    if arg.contains("backend()") {
        return true;
    }
    let head: String = arg.chars().take_while(|c| is_ident(*c)).collect();
    !head.is_empty()
        && body[..at]
            .rfind(&format!("let {head}"))
            .is_some_and(|k| body[k..at].contains("backend()"))
}

#[test]
fn the_stop_worker_saves_snapshots_only_on_the_local_backend() {
    let body = stop_worker();
    const GATE: &str = "client_saves_snapshots(";
    assert!(
        body.contains(GATE),
        "the stop worker never asks who saves the snapshots"
    );
    assert!(
        gate_asks_the_backend(&body, GATE),
        "the stop worker's decision does not ask the backend it stops: {body}"
    );
    for call in ["slot_state::save_all(", "snapshot-saving"] {
        assert!(
            gate_lint::call_behind_gate(&body, GATE, call),
            "the stop worker's {call:?} is not behind the decision: {body}"
        );
    }
    assert!(
        body.contains("slot_state::save_all("),
        "the local backend's stop must still save the snapshots"
    );
    assert!(
        body.contains(".stop()"),
        "the stop worker must still stop the server"
    );
}

/// The three sanctioned shapes, each spelled as the two production call sites
/// spell it and as a plain one, and the shapes that must not read as guarded:
/// an answer nothing uses, a guard of some other call, a gate after the save,
/// and both inverted polarities (a guard that returns when the decision says
/// yes, a block that saves when it says no). The needle is the gate as the
/// lint passes it, path-qualified in the two real shapes.
#[test]
fn the_gate_scan_accepts_the_three_guarded_shapes_and_refuses_the_rest() {
    for (gate, body, expected) in [
        // The gate inside the condition of the `if` whose block holds the call.
        (
            "saves(",
            "if saves(b) {\n    slot_state::save_all(&cfg);\n}",
            true,
        ),
        // A guard clause: the call sits below the block that returns.
        (
            "saves(",
            "if !saves(b) {\n    return;\n}\nslot_state::save_all(&cfg);",
            true,
        ),
        // The answer is bound to a name, and an `if` tests it before the call.
        (
            "saves(",
            "let on = saves(b);\nif on {\n    slot_state::save_all(&cfg);\n}",
            true,
        ),
        // The name tested first in a conjunction (the GUI's stop worker).
        (
            "client_saves_snapshots(",
            "let server = backend();\nlet saves = client_saves_snapshots(server);\n\
             if saves && cfg.save_state_on_shutdown() {\n    \
             slot_state::save_all(&cfg);\n}",
            true,
        ),
        // The name tested last in a conjunction.
        (
            "saves(",
            "let on = saves(b);\nif cfg.save_state() && on {\n    \
             slot_state::save_all(&cfg);\n}",
            true,
        ),
        // The bound name in a guard clause.
        (
            "saves(",
            "let on = saves(b);\nif !on {\n    return;\n}\n\
             slot_state::save_all(&cfg);",
            true,
        ),
        // The guard clause as the CLI writes it.
        (
            "client_saves_snapshots(",
            "if !client_saves_snapshots(server) {\n    return;\n}\n\
             slot_state::save_all(&cfg);",
            true,
        ),
        // The answer is asked and nothing uses it.
        (
            "saves(",
            "let _ = saves(b);\nslot_state::save_all(&cfg);",
            false,
        ),
        // The gate asks after the save.
        (
            "saves(",
            "slot_state::save_all(&cfg);\nif saves(b) {\n    post();\n}",
            false,
        ),
        // The gate guards only some other call.
        (
            "saves(",
            "if saves(b) {\n    post();\n}\nslot_state::save_all(&cfg);",
            false,
        ),
        // The bound name is asked and never tested: an `if` whose condition
        // names nothing of the gate holds the call (the GUI's worker with its
        // `saves &&` deleted).
        (
            "client_saves_snapshots(",
            "let server = backend();\nlet saves = client_saves_snapshots(server);\n\
             if cfg.save_state_on_shutdown() {\n    \
             slot_state::save_all(&cfg);\n}",
            false,
        ),
        // The bound name is tested only in an argument, in a condition whose
        // block holds no call.
        (
            "saves(",
            "let on = saves(b);\nif verify(on) {\n    post();\n}\n\
             slot_state::save_all(&cfg);",
            false,
        ),
        // No `if` at all: a word carrying the letters `if` is not a condition
        // (the scan that took any `if` between the gate and the call passed
        // these two).
        (
            "saves(",
            "let on = saves(b);\nlet ok = verify(on);\n\
             slot_state::save_all(&cfg);",
            false,
        ),
        (
            "saves(",
            "let on = saves(b);\nmodify(cfg);\nslot_state::save_all(&cfg);",
            false,
        ),
        // The name matched only inside a longer name is not the name.
        (
            "saves(",
            "let on = saves(b);\nif on_the_fly {\n    \
             slot_state::save_all(&cfg);\n}",
            false,
        ),
        // A guard clause with the wrong polarity: it returns when the decision
        // says the client saves.
        (
            "client_saves_snapshots(",
            "if client_saves_snapshots(server) {\n    return;\n}\n\
             slot_state::save_all(&cfg);",
            false,
        ),
        // A block that saves when the decision says it must not.
        (
            "client_saves_snapshots(",
            "if !client_saves_snapshots(server) {\n    \
             slot_state::save_all(&cfg);\n}",
            false,
        ),
        // The bound name negated while its block holds the call.
        (
            "saves(",
            "let on = saves(b);\nif !on {\n    \
             slot_state::save_all(&cfg);\n}",
            false,
        ),
    ] {
        assert_eq!(
            gate_lint::call_behind_gate(body, gate, "slot_state::save_all("),
            expected,
            "{body:?}"
        );
    }
}

#[test]
fn the_gate_ask_scan_needs_the_backend_the_gate_asks() {
    for (body, expected) in [
        // The argument is the backend call itself.
        (
            "if client_saves_snapshots(crate::server_control::backend()) {\n    a();\n}",
            true,
        ),
        // The argument is a name bound to the backend before the gate.
        (
            "let server = crate::server_control::backend();\n\
             if client_saves_snapshots(server) {\n    a();\n}",
            true,
        ),
        // The argument is a local config: no backend is asked.
        (
            "let cfg = server_cfg::load();\nif client_saves_snapshots(cfg) {\n    a();\n}",
            false,
        ),
        // No gate at all.
        ("a();", false),
    ] {
        assert_eq!(
            gate_asks_the_backend(body, "client_saves_snapshots("),
            expected,
            "{body:?}"
        );
    }
}

// ── The instance files are the engine's ────────────────────────────────
// `server@<id>.ini`, `models.ini` and `models.ini.disabled` live in the
// engine's directory (`paths::engine_config_dir`, `/etc/llama.cpp` on Linux);
// the framework's (`paths::system_config_dir`, `/etc/private-ai`) keeps only
// its own files (`bff.ini`). Production code of every crate that touches
// them names the framework's directory only where it is meant: `paths.rs`,
// which defines it, and the bff's `listen.rs`, which reads `bff.ini`. Test
// code may name it.

/// The files allowed to name `system_config_dir`, relative to `crates/`.
const SYSTEM_DIR_ALLOWED: [&str; 3] = [
    "core/src/paths.rs",
    "bff/src/listen.rs",
    // The manager's own log sits in `logs` beside the framework's config
    // directory, so `manager_log_path` takes that directory's parent.
    "manager/src/manager/action_log.rs",
];

/// `system_config_dir` named on one line of code, comments left out.
fn system_dir_hits(line: &str) -> Vec<String> {
    let code = line.split("//").next().unwrap_or(line);
    let mut found = Vec::new();
    let mut rest = code;
    while let Some(i) = rest.find("system_config_dir") {
        let before = rest[..i].chars().next_back();
        let after = rest[i + "system_config_dir".len()..].chars().next();
        if !before.is_some_and(is_ident) && !after.is_some_and(is_ident) {
            found.push("system_config_dir".to_string());
        }
        rest = &rest[i + "system_config_dir".len()..];
    }
    found
}

#[test]
fn only_the_framework_s_own_files_are_in_its_system_directory() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for c in ["app", "bff", "cli", "config", "core", "manager"] {
        sources(&crates.join(c).join("src"), &mut files);
    }
    let mut violations = String::new();
    for file in files {
        let r = rel(&file).to_string_lossy().replace('\\', "/");
        if SYSTEM_DIR_ALLOWED.contains(&r.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("read source");
        for (n, line) in production_lines(&text) {
            for hit in system_dir_hits(line) {
                violations.push_str(&format!("\n  {r}:{n}: {hit}"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "the instance files are the engine's: name paths::engine_config_dir() \
         (or instance_ini, instance_ids), not the framework's directory:{violations}"
    );
}

#[test]
fn system_dir_scan_flags_the_framework_s_directory_and_leaves_the_rest() {
    for (line, expected) in [
        (
            "        let dir = crate::paths::system_config_dir();",
            vec!["system_config_dir"],
        ),
        (
            "    Ok(crate::paths::system_config_dir().join(name))",
            vec!["system_config_dir"],
        ),
        (
            "use pai_core::paths::{system_config_dir, data_root};",
            vec!["system_config_dir"],
        ),
        // Not flagged: the engine's directory, other names, comments.
        (
            "        let dir = crate::paths::engine_config_dir();",
            vec![],
        ),
        ("    let d = my_system_config_dir_backup();", vec![]),
        ("    // paths::system_config_dir() holds bff.ini", vec![]),
        (
            "//! their config files in `paths::system_config_dir()`",
            vec![],
        ),
    ] {
        assert_eq!(system_dir_hits(line), expected, "{line:?}");
    }
}
