//! Writing KV-cache snapshots while a running llama-server is still alive.
//!
//! WHY IT EXISTS. Prefill is quadratic in the prompt length, so a long agentic
//! session pays a start cost that dwarfs everything the framework tunes. A
//! snapshot taken while the server runs turns the next start into a load: the
//! conversation is read back from disk instead of recomputed. This module is
//! that write: the conversation of every loaded model is saved to the state
//! directory through the engine's admin endpoint.
//!
//! HOW IT WORKS. One run asks the server, in order, for each model it has
//! loaded, posts the save action to `/slots/0` with the file name the engine
//! should write, and reports each model that lands. Every write goes to a name
//! ending `-partial` first and is renamed to `<model stem>.llamastate` only
//! once the engine has answered the whole save, so an older snapshot of that
//! model is replaced only by a complete newer one: an interrupted save never
//! destroys the snapshot the next start would restore.
//!
//! ONE DEADLINE FOR THE WHOLE RUN. The caller gives a `budget` and it covers
//! everything: the whole run ends at that instant, requests included. The
//! conversation of a model that is still being written when the deadline passes
//! is reported as an error, its dump from an earlier stop stays as it is, and
//! the models that are saved by then are reported as saved. A per-model timeout
//! would be wrong: with several models it lets the run last several budgets,
//! and a stop that waits that long is a stop that never happens.
//!
//! THE PRUNE. Snapshots of models that are no longer loaded are deleted at the
//! end of a run that saved something, so the state directory stays a picture of
//! the last shutdown rather than an accumulating pile. The dump of a model whose
//! save failed is left as it is: it is the snapshot the next start restores.

use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

use crate::http::request;
use crate::strings;

/// Extension every dump carries: a file in the state folder with any other
/// extension is not a snapshot, so it is never renamed onto, never pruned, and
/// never a restore candidate. The restore's leg (`pai-app`'s `slot_state.rs`)
/// looks for the same one.
pub const STATE_EXT: &str = "llamastate";

/// The one deadline every caller hands to [`save_all`]: the whole run, requests
/// included. It is the deadline of a save at a stop, so the instance's own stop
/// and the clients' wrapper cannot end up with two different waits. The stop's
/// own bound (the unit's `TimeoutStopSec=`, the Windows preshutdown timeout)
/// sits above it, leaving the server time to exit after the save.
pub const SAVE_BUDGET: Duration = Duration::from_secs(240);

/// The suffix a running save writes under. The engine answers only once the
/// whole state has hit the disk, so the name without the suffix is the promise
/// that the bytes behind it are complete.
const PARTIAL_SUFFIX: &str = "-partial";

/// The slot to snapshot. The framework runs `parallel = 1`, so slot 0 is the
/// only one a child has, and it is the one holding the conversation.
const SLOT_ID: u32 = 0;

/// One model's saved snapshot, as the engine reported it.
#[derive(Debug)]
pub struct Transfer {
    /// The model as the server listed it.
    pub model: String,
    /// Bytes the engine wrote, from its answer.
    pub bytes: u64,
    /// Tokens the engine saved.
    pub tokens: u64,
}

impl Transfer {
    /// The size and token count, in the framework's wording: the unit is
    /// spelled out, the MiB figure has no decimal. Numbers are not user text,
    /// so they stay in the code.
    pub fn describe(&self) -> String {
        format!("{} MiB, {} tokens", self.bytes / (1024 * 1024), self.tokens)
    }
}

/// The one save: every model the server has loaded, its conversation written
/// to `dir`, within one `budget` for the whole run.
///
/// Returns the models that landed and a per-model error list; a partial result
/// is still worth reporting, since one model failing says nothing about the
/// others. Dumps of models that are no longer loaded are pruned at the end of a
/// run that saved something, and the prune is reported in that same list.
pub fn save_all(port: u16, dir: &Path, budget: Duration) -> (Vec<Transfer>, Vec<String>) {
    let deadline = Instant::now() + budget;
    let mut errors = Vec::new();

    // The folder is the launch's to make, never this run's. llama.cpp validates
    // `--slot-save-path` while PARSING the flag, so a folder that is missing
    // now was never the one the engine was started with, and a dump written
    // where the launch did not point is a dump no start will look at.
    if let Err(e) = std::fs::metadata(dir) {
        errors.push(strings::fill("state-dir-failed", &[&e]));
        return (Vec::new(), errors);
    }

    // The model list is the run's first request, so it gets what is left of the
    // budget, which at this point is all of it.
    let Some(left) = remaining(deadline) else {
        errors.push(out_of_budget(port, budget));
        return (Vec::new(), errors);
    };
    let models = match loaded_models(port, left) {
        Ok(models) => models,
        Err(e) => {
            errors.push(e);
            return (Vec::new(), errors);
        }
    };
    if models.is_empty() {
        return (Vec::new(), errors);
    }

    // Every model the server reports as loaded, saved or not: a live model keeps
    // its dump, because it is the conversation of a model the next start can
    // load again.
    let keep: Vec<String> = models.iter().map(|m| dump_file_name(m)).collect();
    let mut done = Vec::new();
    for (model, name) in models.iter().zip(&keep) {
        // A model behind the deadline is never asked: asking it would spend the
        // budget a second time, and a stop that waits twice is a stop that
        // never happens.
        let Some(left) = remaining(deadline) else {
            break;
        };
        let partial = format!("{name}{PARTIAL_SUFFIX}");
        match save_one(port, model, &partial, left, budget, deadline) {
            Ok(transfer) => match std::fs::rename(dir.join(&partial), dir.join(name)) {
                Ok(()) => done.push(transfer),
                // The engine wrote, so a rename that failed is the framework's
                // own failure to report, and the file it left is not a snapshot.
                Err(e) => {
                    let _ = std::fs::remove_file(dir.join(&partial));
                    errors.push(strings::fill("snapshot-rename-failed", &[model, name, &e]));
                }
            },
            Err(e) => {
                // The dump of an earlier stop stays exactly as it is; a save
                // that did not complete leaves no half-written file for the
                // restore to pick as the newest one.
                let _ = std::fs::remove_file(dir.join(&partial));
                errors.push(e);
            }
        }
    }

    if !done.is_empty() {
        prune(dir, &keep);
    }
    (done, errors)
}

/// Where the snapshot machinery talks, read out of the launch arguments.
///
/// `None` when the arguments have no `--slot-save-path`: snapshots are off, the
/// state machinery must not run, and the folder named by the key in
/// `server.ini` is not the one the engine writes to.
///
/// The port is read from the arguments too, never from `server.ini`: the
/// arguments are the live truth of a running instance, a file the user has
/// edited since is not. No `--port` in them means the engine's own default.
///
/// Only the space form (`--flag value`) is read: that is the form `launch.rs`
/// writes, and a `--flag=value` in an argument list the framework did not build
/// is an argument the framework cannot trust to mean what the engine read.
pub fn slot_target(args: &[String]) -> Option<(u16, PathBuf)> {
    let dir = flag_value(args, "--slot-save-path")?;
    // The engine takes the next argument as the value, so a list that ENDS with
    // the flag names no folder, and a key set to nothing is snapshots off
    // rather than "the working directory". Guessing either would write dumps
    // where no start will look.
    if dir.is_empty() || dir.starts_with('-') {
        return None;
    }
    let port = match flag_value(args, "--port") {
        Some(text) => text.parse::<u16>().ok().filter(|p| *p != 0)?,
        None => crate::server_cfg::DEFAULT_PORT as u16,
    };
    Some((port, PathBuf::from(dir)))
}

/// The value of a launch flag in its space form, the LAST occurrence when the
/// flag appears twice: the engine keeps the last, so the snapshot machinery
/// looks at the same one. A flag with nothing after it has no value.
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let mut value = None;
    for pair in args.windows(2) {
        if pair[0] == flag {
            value = Some(pair[1].as_str());
        }
    }
    value
}

/// What is left of the run's one deadline, `None` once it is spent. Every
/// request's read timeout is this, so the deadline bounds the whole run and not
/// one request.
fn remaining(deadline: Instant) -> Option<Duration> {
    let now = Instant::now();
    (now < deadline).then_some(deadline - now)
}

/// The report for a model the run could not finish inside its budget. The
/// snapshot on disk is the one from the last stop that did finish, which is why
/// the text says the file stays.
fn out_of_budget(port: u16, budget: Duration) -> String {
    strings::fill("no-answer-within", &[&port, &budget.as_secs()])
}

/// The models the server has resident, in the order it lists them, read from
/// its `GET /models`, whose per-model `status.value` is one of the strings in
/// `server_model_status_to_string`. Only `loaded` qualifies: `loading` has no
/// slot to snapshot yet and `sleeping` has released its KV cache, so asking
/// either to save answers an error, not a dump.
fn loaded_models(port: u16, timeout: Duration) -> Result<Vec<String>, String> {
    let res = request(port, "GET", "/models", None, timeout)?;
    if res.status != 200 {
        return Err(strings::fill("models-status", &[&res.status]));
    }
    let parsed: serde_json::Value =
        serde_json::from_str(&res.body).map_err(|e| strings::fill("models-unreadable", &[&e]))?;
    let Some(items) = parsed.get("data").and_then(|d| d.as_array()) else {
        return Ok(Vec::new());
    };
    Ok(items
        .iter()
        .filter(|m| {
            m.get("status")
                .and_then(|s| s.get("value"))
                .and_then(|v| v.as_str())
                == Some("loaded")
        })
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
        .map(str::to_string)
        .collect())
}

/// Ask one model to write `partial`: `POST /slots/{SLOT_ID}?action=save`,
/// through the ROUTER, which proxies it to the child that owns the model.
///
/// The body carries both the routing and the payload: the router's `proxy_post`
/// reads `model` out of it (`server-models.cpp`), while the child reads
/// `filename` from the same body and the action from the query string, which the
/// proxy forwards verbatim.
fn save_one(
    port: u16,
    model: &str,
    partial: &str,
    left: Duration,
    budget: Duration,
    deadline: Instant,
) -> Result<Transfer, String> {
    let body = serde_json::json!({ "filename": partial, "model": model }).to_string();
    let path = format!("/slots/{SLOT_ID}?action=save");
    let res = request(port, "POST", &path, Some(&body), left).map_err(|e| {
        // A read that ran past the deadline is the budget spent, not a broken
        // pipe: the engine may well still be writing.
        if Instant::now() >= deadline {
            out_of_budget(port, budget)
        } else {
            e
        }
    })?;

    let parsed: serde_json::Value =
        serde_json::from_str(&res.body).unwrap_or(serde_json::Value::Null);
    if res.status != 200 {
        // llama-server's error body is `{"error": {"message": "..."}}`; fall
        // back to the raw body so a proxy or gateway reply is not swallowed.
        let msg = parsed
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .map_or_else(|| res.body.trim().to_string(), str::to_string);
        return Err(format!("{model}: {msg}"));
    }
    Ok(transfer_from_json(model, &parsed))
}

/// Pull the counts out of a save reply: `n_saved` counts TOKENS and
/// `n_written` bytes (`server_task_result_slot_save_load::to_json`). The two are
/// easy to get backwards and there is no `n_tokens` to ask for, so a wrong key
/// reads as a silent zero.
fn transfer_from_json(model: &str, parsed: &serde_json::Value) -> Transfer {
    let num = |k: &str| parsed.get(k).and_then(serde_json::Value::as_u64);
    Transfer {
        model: model.to_string(),
        bytes: num("n_written").unwrap_or(0),
        tokens: num("n_saved").unwrap_or(0),
    }
}

/// The file name the engine writes for a model: the WHOLE model id sanitized,
/// because a model id is usually a path and the engine's save endpoint validates
/// the name with `fs_validate_filename`, which refuses a separator. The id's last
/// component would do that, but two ids that differ only in their folder would
/// then land on one file. Sanitized to the characters that call takes
/// (`[A-Za-z0-9._-]`, every other one becomes `_`, leading and trailing dots
/// trimmed, an id that leaves nothing is `model`) so an odd id never surfaces as
/// a mysterious failed save; the same name is what the prune compares and what
/// the restore of the `pai-app` leg looks for.
pub fn dump_file_name(model: &str) -> String {
    let stem: String = model
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = stem.trim_matches('.');
    let stem = if stem.is_empty() { "model" } else { stem };
    format!("{stem}.{STATE_EXT}")
}

/// Drop the dumps of models that are no longer loaded, so the state folder
/// stays a picture of the last shutdown. A model that was asked and refused is
/// loaded, so its dump stays: it is the snapshot the next start restores. A file
/// that is not a dump is nobody's to touch, and a half-written `-partial` name
/// is not a dump.
fn prune(dir: &Path, keep: &[String]) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some(STATE_EXT) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !keep.iter().any(|k| k == name) {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
#[path = "slot_save_tests.rs"]
mod tests;
