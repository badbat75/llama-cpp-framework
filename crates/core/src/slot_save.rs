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
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    /// How long a `Hang` step keeps the server from answering. Far beyond the
    /// budgets the tests give, so a run that respects its deadline cannot be a
    /// run that got an answer.
    const HANG: Duration = Duration::from_secs(5);

    /// How long a `Trickle` step waits between two bytes of its reply. Short
    /// enough that every single read is answered well inside any read timeout,
    /// long enough that a full reply takes far beyond the budgets the tests
    /// give.
    const TRICKLE_GAP: Duration = Duration::from_millis(300);

    /// What the fake server does with the next save request.
    enum Step {
        /// Answers `status` with `body`, writing no dump.
        Reply { status: u16, body: String },
        /// Writes the dump under the file name the request asked for, then
        /// answers with the engine's save reply.
        Saves,
        /// Accepts the request and answers nothing, until its client gives up.
        Hang,
        /// Answers with the whole save reply, one byte at a time, until the
        /// client closes the connection. The engine wrote nothing: the reply
        /// only ever reaches a client that reads it under one deadline.
        Trickle,
    }

    /// One request the client sent.
    #[derive(Clone, Debug)]
    struct Ask {
        method: String,
        path: String,
        query: String,
        body: String,
    }

    /// A stand-in for llama-server's admin endpoint on a loopback port.
    ///
    /// `GET /models` answers with `models`, each one `loaded`. A
    /// `POST /slots/:id` request is driven by the next `Step` of `script`; with
    /// no step left it saves. `loaded_models`, this module's, is the shape this
    /// mirrors, read from the engine.
    struct FakeServer {
        port: u16,
        asks: Arc<Mutex<Vec<Ask>>>,
        shutdown: Arc<Mutex<bool>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl FakeServer {
        /// Every model in `models` is loaded.
        fn start(dir: &Path, models: &[&str], script: Vec<Step>) -> FakeServer {
            let with_status: Vec<(&str, &str)> = models.iter().map(|m| (*m, "loaded")).collect();
            FakeServer::start_with_status(dir, &with_status, script)
        }

        /// `models` is `(id, status)` pairs: only a `loaded` one is resident.
        fn start_with_status(dir: &Path, models: &[(&str, &str)], script: Vec<Step>) -> FakeServer {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
            let port = listener.local_addr().expect("the port").port();
            let asks = Arc::new(Mutex::new(Vec::new()));
            let shutdown = Arc::new(Mutex::new(false));
            let script = Arc::new(Mutex::new(VecDeque::from(script)));
            let dir = dir.to_path_buf();
            let models: Vec<(String, String)> = models
                .iter()
                .map(|(id, s)| (id.to_string(), s.to_string()))
                .collect();
            let asks_t = Arc::clone(&asks);
            let shutdown_t = Arc::clone(&shutdown);
            let thread = std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if *shutdown_t.lock().expect("the shutdown lock") {
                        break;
                    }
                    let Ok(mut stream) = stream else { continue };
                    let Some(ask) = read_request(&mut stream) else {
                        continue;
                    };
                    // Record what arrived before answering: a step that waits
                    // (the `Hang`) must not hide the request from a test that
                    // reads the asks while it is still waiting.
                    asks_t.lock().expect("the asks lock").push(ask.clone());
                    let save = ask.path.starts_with("/slots/") && ask.method == "POST";
                    if save {
                        let step = script
                            .lock()
                            .expect("the script lock")
                            .pop_front()
                            .unwrap_or(Step::Saves);
                        match step {
                            Step::Reply { status, body } => {
                                reply(&mut stream, status, &body);
                            }
                            Step::Hang => {
                                // The reply never comes: the deadline is the
                                // only way out for the client.
                                let _ = stream.write_all(b"waiting");
                                std::thread::sleep(HANG);
                            }
                            Step::Trickle => {
                                let body = r#"{"saved_slot":0,"n_saved":166000,"filename":"trickle","n_written":2097152}"#.to_string();
                                let text = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                                    body.len()
                                );
                                for byte in text.as_bytes() {
                                    // The client giving up is the only end of
                                    // this reply: a broken pipe is the signal.
                                    if stream.write_all(&[*byte]).is_err()
                                        || stream.flush().is_err()
                                    {
                                        break;
                                    }
                                    std::thread::sleep(TRICKLE_GAP);
                                }
                            }
                            Step::Saves => {
                                // The engine names the file the request asked
                                // for; the fake writes it the same way, and
                                // answers with the counts of a real save.
                                let filename = json_field(&ask.body, "filename");
                                let dump = dir.join(&filename);
                                let _ = std::fs::write(&dump, "state");
                                reply(
                                    &mut stream,
                                    200,
                                    &format!(
                                        r#"{{"saved_slot":0,"n_saved":166000,"filename":"{filename}","n_written":2097152}}"#
                                    ),
                                );
                            }
                        }
                    } else {
                        reply(&mut stream, 200, &model_list(&models));
                    }
                }
            });
            FakeServer {
                port,
                asks,
                shutdown,
                thread: Some(thread),
            }
        }

        fn port(&self) -> u16 {
            self.port
        }

        /// The save requests, in the order the server saw them.
        fn asks(&self) -> Vec<Ask> {
            self.asks.lock().expect("the asks lock").clone()
        }

        /// The save requests only, in order.
        fn saves(&self) -> Vec<Ask> {
            self.asks()
                .into_iter()
                .filter(|a| a.path.starts_with("/slots/"))
                .collect()
        }

        /// The `filename` of each save request, in order.
        fn filenames(&self) -> Vec<String> {
            self.saves()
                .iter()
                .map(|a| json_field(&a.body, "filename"))
                .collect()
        }

        fn stop(self) {
            *self.shutdown.lock().expect("the shutdown lock") = true;
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            if let Some(thread) = self.thread {
                let _ = thread.join();
            }
        }
    }

    fn model_list(models: &[(String, String)]) -> String {
        let entries: Vec<String> = models
            .iter()
            .map(|(id, status)| format!(r#"{{"id":"{id}","status":{{"value":"{status}"}}}}"#))
            .collect();
        format!(r#"{{"data":[{}]}}"#, entries.join(","))
    }

    fn reply(stream: &mut TcpStream, status: u16, body: &str) {
        // One write, head and body together: a client that reads the stream in
        // one pass must never see the head of a JSON reply apart from its body.
        let text = format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(text.as_bytes());
    }

    fn json_field(body: &str, key: &str) -> String {
        serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get(key).and_then(|f| f.as_str()).map(|f| f.to_string()))
            .unwrap_or_default()
    }

    fn query_value(query: &str, key: &str) -> String {
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            if parts.next() == Some(key) {
                return parts.next().unwrap_or("").to_string();
            }
        }
        String::new()
    }

    fn read_request(stream: &mut TcpStream) -> Option<Ask> {
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        let head = loop {
            if stream.read(&mut byte).ok()? == 0 {
                return None;
            }
            buf.push(byte[0]);
            if buf.ends_with(b"\r\n\r\n") {
                break String::from_utf8_lossy(&buf).into_owned();
            }
            if buf.len() > 8192 {
                return None;
            }
        };
        let mut lines = head.lines();
        let first = lines.next().unwrap_or("").trim_end().to_string();
        let mut words = first.split_whitespace();
        let method = words.next().unwrap_or("").to_string();
        let target = words.next().unwrap_or("").to_string();
        let (path, query) = match target.split_once('?') {
            Some((p, q)) => (p.to_string(), q.to_string()),
            None => (target.clone(), String::new()),
        };
        let length: usize = head
            .lines()
            .find_map(|l| {
                let (name, value) = l.split_once(':')?;
                if name.trim().eq_ignore_ascii_case("content-length") {
                    value.trim().parse().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let mut body = vec![0u8; length];
        if length > 0 {
            stream.read_exact(&mut body).ok()?;
        }
        Some(Ask {
            method,
            path,
            query,
            body: String::from_utf8_lossy(&body).into_owned(),
        })
    }

    /// The lines of a launch argument list, in the shape `launch.rs` writes:
    /// flag, value, flag, value.
    fn args(pairs: &[(&str, &str)]) -> Vec<String> {
        let mut out = vec!["--model".to_string(), "GGUF".to_string()];
        for (flag, value) in pairs {
            out.push(flag.to_string());
            out.push(value.to_string());
        }
        out
    }

    /// The files in `dir`, sorted by name. A dump left at its `-partial` name
    /// is the point of the tests that read this.
    fn dumps(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("read the state dir")
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .collect();
        names.sort();
        names
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn arguments_without_a_slot_save_path_have_no_target() {
        // The contract in one line: snapshots off means no port, no dir. The
        // state folder of server.ini must never be a fallback from here,
        // because the engine is not writing to it.
        let argv = args(&[("--port", "8080")]);
        assert_eq!(slot_target(&argv), None);
    }

    #[test]
    fn a_slot_save_path_with_a_port_gives_both() {
        let argv = args(&[("--port", "8080"), ("--slot-save-path", "/state/dir")]);
        assert_eq!(
            slot_target(&argv),
            Some((8080, PathBuf::from("/state/dir")))
        );
    }

    #[test]
    fn the_last_slot_save_path_wins() {
        // A launch argument list can carry the flag twice (a model profile, then a
        // later section): the engine keeps the last, so the snapshot machinery
        // must look at the same one.
        let argv = args(&[
            ("--slot-save-path", "/state/first"),
            ("--slot-save-path", "/state/second"),
            ("--port", "9000"),
        ]);
        assert_eq!(
            slot_target(&argv),
            Some((9000, PathBuf::from("/state/second")))
        );
    }

    #[test]
    fn an_empty_slot_save_path_has_no_target() {
        // A key set to nothing is snapshots off, not "the working directory".
        let argv = args(&[("--port", "8080"), ("--slot-save-path", "")]);
        assert_eq!(slot_target(&argv), None);
    }

    #[test]
    fn a_slot_save_path_that_eats_the_next_flag_has_no_target() {
        // The engine reads the next argument as the value: a list that ends
        // with the flag names no folder, and guessing one would write dumps
        // where no start will look.
        let argv = args(&[("--slot-save-path", "--port")]);
        assert_eq!(
            slot_target(&argv),
            None,
            "a path that is the next flag must not be a target"
        );
    }

    #[test]
    fn no_port_argument_gives_the_default_port() {
        // The engine's own default, the same constant the launch builder uses.
        let argv = args(&[("--slot-save-path", "/state/dir")]);
        assert_eq!(
            slot_target(&argv),
            Some((
                crate::server_cfg::DEFAULT_PORT as u16,
                PathBuf::from("/state/dir")
            ))
        );
    }

    #[test]
    fn a_port_that_is_not_a_number_has_no_target() {
        for garbage in ["abc", "9931x", "-1", "0", "70000"] {
            let argv = args(&[("--port", garbage), ("--slot-save-path", "/state/dir")]);
            assert_eq!(
                slot_target(&argv),
                None,
                "--port {garbage} cannot be a port to talk to"
            );
        }
    }

    /// The one budget every caller hands to `save_all`: the instance's own stop
    /// and the clients' wrapper must give the same deadline, so a stop cannot
    /// wait longer than a save anyone else was ready to wait for. The stop's
    /// own bound (the unit's `TimeoutStopSec=`, the Windows preshutdown
    /// timeout) sits above it and is pinned with the files that ship it.
    #[test]
    fn the_save_budget_is_the_designs_one_deadline() {
        assert_eq!(SAVE_BUDGET, Duration::from_secs(240));
    }

    #[test]
    fn the_save_asks_for_the_partial_name_and_lands_it_under_the_final_one() {
        let dir = tempdir();
        let server = FakeServer::start(dir.path(), &["Qwen3.8-27B"], vec![Step::Saves]);
        let (done, errors) = save_all(server.port(), dir.path(), Duration::from_secs(30));
        let saves = server.saves();
        server.stop();

        assert_eq!(errors, Vec::<String>::new());
        assert_eq!(
            done.len(),
            1,
            "one model loaded, one transfer, got: {done:?}"
        );
        assert_eq!(done[0].model, "Qwen3.8-27B");

        // The endpoint is the engine's own slot action: slot 0, `action=save`,
        // the file name in the body (llama.cpp's `handle_slots_save` reads it
        // from there, not from the query).
        let save = &saves[0];
        assert_eq!(saves.len(), 1, "one loaded model, one save request");
        assert_eq!(save.path, "/slots/0", "the save goes to slot 0");
        assert_eq!(
            query_value(&save.query, "action"),
            "save",
            "the save is the action the query asks for, query: {}",
            save.query
        );
        assert_eq!(
            json_field(&save.body, "filename"),
            "Qwen3.8-27B.llamastate-partial",
            "the save must be asked at the partial name"
        );
        assert_eq!(json_field(&save.body, "model"), "Qwen3.8-27B");

        // What is on disk after the run is the final name, never the partial.
        assert_eq!(
            dumps(dir.path()),
            vec!["Qwen3.8-27B.llamastate".to_string()],
            "the dump lands under the final name only"
        );
    }

    #[test]
    fn the_saved_models_are_reported_with_what_the_engine_answered() {
        let dir = tempdir();
        let server = FakeServer::start_with_status(
            dir.path(),
            &[("Qwen3.8-27B", "loaded"), ("Qwen3.8-Flash-Next", "loaded")],
            vec![Step::Saves, Step::Saves],
        );
        let (done, errors) = save_all(server.port(), dir.path(), Duration::from_secs(30));
        let names = server.filenames();
        server.stop();

        assert_eq!(errors, Vec::<String>::new());
        assert_eq!(done.len(), 2, "both models save, got: {done:?}");
        assert_eq!(done[0].model, "Qwen3.8-27B");
        assert_eq!(done[1].model, "Qwen3.8-Flash-Next");
        assert_eq!(
            names,
            vec![
                "Qwen3.8-27B.llamastate-partial".to_string(),
                "Qwen3.8-Flash-Next.llamastate-partial".to_string()
            ],
            "one save request per loaded model, in the server's order"
        );
        // The engine answers a save with n_saved TOKENS and n_written BYTES.
        assert_eq!(done[0].tokens, 166000, "n_saved counts tokens");
        assert_eq!(done[0].bytes, 2097152, "n_written counts bytes");
        assert_eq!(
            dumps(dir.path()),
            vec![
                "Qwen3.8-27B.llamastate".to_string(),
                "Qwen3.8-Flash-Next.llamastate".to_string()
            ]
        );
    }

    #[test]
    fn a_model_that_is_not_loaded_is_never_asked_to_save() {
        // A `loading` model has no slot yet and a `sleeping` one has released
        // its KV cache: asking either to save answers an error, not a dump.
        let dir = tempdir();
        let server = FakeServer::start_with_status(
            dir.path(),
            &[
                ("Qwen3.8-27B", "loaded"),
                ("Qwen3.6-27B", "loading"),
                ("Qwen3.5-9B", "sleeping"),
            ],
            vec![Step::Saves],
        );
        let (done, errors) = save_all(server.port(), dir.path(), Duration::from_secs(30));
        let names = server.filenames();
        server.stop();

        assert_eq!(
            errors,
            Vec::<String>::new(),
            "a model that is not loaded is not an error, got: {errors:?}"
        );
        assert_eq!(done.len(), 1, "only the loaded model saves, got: {done:?}");
        assert_eq!(names, vec!["Qwen3.8-27B.llamastate-partial".to_string()]);
    }

    #[test]
    fn a_transfer_description_is_mebibytes_and_tokens() {
        let transfer = Transfer {
            model: "Qwen3.8-27B".to_string(),
            bytes: 2 * 1024 * 1024,
            tokens: 166000,
        };
        assert_eq!(transfer.describe(), "2 MiB, 166000 tokens");

        let zero = Transfer {
            model: "Qwen3.8-27B".to_string(),
            bytes: 0,
            tokens: 0,
        };
        assert_eq!(zero.describe(), "0 MiB, 0 tokens");
    }

    #[test]
    fn the_state_folder_is_never_created_by_the_save() {
        // The folder is there before the save, put there by whoever started the
        // instance. A save that mkdirs a folder the engine's own launch did not
        // have would leave the framework writing dumps the next start cannot
        // find. The run is one error, not a crash, and it writes nothing.
        let dir = tempdir();
        let missing = dir.path().join("nope");
        let server = FakeServer::start(&missing, &["Qwen3.8-27B"], vec![Step::Saves]);
        let (done, errors) = save_all(server.port(), &missing, Duration::from_secs(30));
        let saves = server.saves();
        server.stop();

        assert!(
            done.is_empty(),
            "nothing saves into a folder that is not there"
        );
        assert_eq!(errors.len(), 1, "one clear error, got: {errors:?}");
        assert!(
            saves.is_empty(),
            "the save must not even be asked when the folder is missing"
        );
        assert!(
            !missing.exists(),
            "the save must not create the state folder"
        );
    }

    #[test]
    fn a_listener_that_never_answers_ends_the_run_in_error_with_the_old_dump_intact() {
        let dir = tempdir();
        std::fs::write(dir.path().join("Qwen3.8-27B.llamastate"), "old")
            .expect("write the older dump");
        let server = FakeServer::start(dir.path(), &["Qwen3.8-27B"], vec![Step::Hang]);

        let started = std::time::Instant::now();
        let (done, errors) = save_all(server.port(), dir.path(), Duration::from_secs(1));
        let took = started.elapsed();
        server.stop();

        assert!(
            took < Duration::from_secs(4),
            "the run outlived its budget: {took:?}"
        );
        assert!(
            done.is_empty(),
            "a server that never answered saved nothing"
        );
        assert_eq!(
            errors.len(),
            1,
            "the model that timed out is reported, got: {errors:?}"
        );
        assert_eq!(
            dumps(dir.path()),
            vec!["Qwen3.8-27B.llamastate".to_string()],
            "the dump of the earlier stop is intact, and no partial is left"
        );
    }

    #[test]
    fn a_model_whose_save_reports_an_error_keeps_its_dump_and_the_others_still_save() {
        let dir = tempdir();
        std::fs::write(dir.path().join("Qwen3.8-27B.llamastate"), "old")
            .expect("write the older dump");
        let server = FakeServer::start_with_status(
            dir.path(),
            &[("Qwen3.8-27B", "loaded"), ("Qwen3.8-Flash-Next", "loaded")],
            vec![
                Step::Reply {
                    status: 400,
                    body: r#"{"error":{"type":"invalid_request","message":"no slots to save"}}"#
                        .to_string(),
                },
                Step::Saves,
            ],
        );
        let (done, errors) = save_all(server.port(), dir.path(), Duration::from_secs(30));
        let names = server.filenames();
        server.stop();

        assert_eq!(
            done.len(),
            1,
            "the model the engine answered saves, got: {done:?}"
        );
        assert_eq!(done[0].model, "Qwen3.8-Flash-Next");
        assert_eq!(
            errors.len(),
            1,
            "the refused model is reported, got: {errors:?}"
        );
        assert_eq!(
            names,
            vec![
                "Qwen3.8-27B.llamastate-partial".to_string(),
                "Qwen3.8-Flash-Next.llamastate-partial".to_string()
            ],
            "both loaded models are asked; one of them is refused"
        );
        assert_eq!(
            dumps(dir.path()),
            vec![
                "Qwen3.8-27B.llamastate".to_string(),
                "Qwen3.8-Flash-Next.llamastate".to_string()
            ],
            "the refusal keeps the older dump of that model, and leaves no partial behind"
        );
    }

    #[test]
    fn a_reply_that_is_an_error_never_lands_a_snapshot() {
        // The engine answers a failed save with an `error` object and writes
        // nothing. A run that treated that as a saved model would report a
        // snapshot the next start cannot restore.
        let dir = tempdir();
        for (status, body) in [
            (
                404,
                r#"{"error":{"type":"not_found","message":"no such model"}}"#,
            ),
            (
                501,
                r#"{"error":{"type":"unsupported","message":"admin is off"}}"#,
            ),
            (500, r#"{}"#),
        ] {
            let server = FakeServer::start(
                dir.path(),
                &["Qwen3.8-27B"],
                vec![Step::Reply {
                    status,
                    body: body.to_string(),
                }],
            );
            let (done, errors) = save_all(server.port(), dir.path(), Duration::from_secs(30));
            server.stop();

            assert!(done.is_empty(), "HTTP {status} is not a saved model");
            assert_eq!(
                errors.len(),
                1,
                "HTTP {status} is reported, got: {errors:?}"
            );
            assert_eq!(
                dumps(dir.path()),
                Vec::<String>::new(),
                "HTTP {status} leaves no dump and no partial, not even an empty file"
            );
        }
    }

    #[test]
    fn a_refused_save_never_replaces_the_dump_of_that_model() {
        let dir = tempdir();
        std::fs::write(dir.path().join("Qwen3.8-27B.llamastate"), "old")
            .expect("write the older dump");
        let server = FakeServer::start(
            dir.path(),
            &["Qwen3.8-27B"],
            vec![Step::Reply {
                status: 400,
                body: r#"{"error":{"type":"invalid_request","message":"no slots to save"}}"#
                    .to_string(),
            }],
        );
        let (done, errors) = save_all(server.port(), dir.path(), Duration::from_secs(30));
        server.stop();

        assert!(done.is_empty(), "the refused model saved nothing");
        assert_eq!(errors.len(), 1, "the refusal is reported, got: {errors:?}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("Qwen3.8-27B.llamastate"))
                .expect("read the dump"),
            "old",
            "the dump the engine never replaced keeps its bytes"
        );
    }

    #[test]
    fn nothing_is_asked_after_the_budget_is_gone() {
        let dir = tempdir();
        // The first save hangs, so the deadline passes while the server is
        // still working on it. The models behind it must not be asked at all:
        // each of them would spend the budget again.
        let server = FakeServer::start_with_status(
            dir.path(),
            &[("Qwen3.8-27B", "loaded"), ("Qwen3.8-Flash-Next", "loaded")],
            vec![Step::Hang, Step::Saves],
        );
        let (done, errors) = save_all(server.port(), dir.path(), Duration::from_secs(2));
        let names = server.filenames();
        server.stop();

        assert!(
            done.is_empty(),
            "the run ended at the deadline, got: {done:?}"
        );
        assert_eq!(
            names.len(),
            1,
            "the model behind the deadline is never asked, got: {names:?}"
        );
        assert!(
            !errors.is_empty(),
            "the model that ran out of budget is reported"
        );
        assert_eq!(
            dumps(dir.path()),
            Vec::<String>::new(),
            "a run that never completed leaves no dump and no partial"
        );
    }

    #[test]
    fn prune_touches_only_the_models_that_are_no_longer_live() {
        let dir = tempdir();
        for name in [
            "Qwen3.8-27B.llamastate",
            "Qwen3.6-27B.llamastate",
            "Qwen3.8-Flash-Next.llamastate",
        ] {
            std::fs::write(dir.path().join(name), "old").expect("write an older dump");
        }
        // Qwen3.8-27B saves now, so its dump is the fresh one. Qwen3.8-Flash-Next
        // is not loaded at all, so its dump is the one to drop. Qwen3.6-27B is
        // loaded and refused, so it is live and its dump stays: the save never
        // destroys the snapshot the next start restores. A foreign file in the
        // folder is not the save's to touch.
        std::fs::write(dir.path().join("notes.txt"), "keep").expect("write a foreign file");
        let server = FakeServer::start_with_status(
            dir.path(),
            &[("Qwen3.8-27B", "loaded"), ("Qwen3.6-27B", "loaded")],
            vec![
                Step::Saves,
                Step::Reply {
                    status: 400,
                    body: r#"{"error":{"type":"invalid_request","message":"no slots to save"}}"#
                        .to_string(),
                },
            ],
        );
        let (done, _errors) = save_all(server.port(), dir.path(), Duration::from_secs(30));
        server.stop();

        assert_eq!(done.len(), 1, "one model saved, got: {done:?}");
        assert_eq!(
            dumps(dir.path()),
            vec![
                "Qwen3.6-27B.llamastate".to_string(),
                "Qwen3.8-27B.llamastate".to_string(),
                "notes.txt".to_string()
            ],
            "the prune drops the dump of a model that is no longer loaded, and nothing else"
        );
    }

    #[test]
    fn an_empty_model_list_reports_nothing_and_prunes_nothing() {
        // A server with no model loaded is a normal stop, not an error, and the
        // dumps of the models that used to be loaded stay for the next start.
        let dir = tempdir();
        std::fs::write(dir.path().join("Qwen3.8-27B.llamastate"), "old")
            .expect("write an older dump");
        let server = FakeServer::start_with_status(dir.path(), &[] as &[(&str, &str)], vec![]);
        let (done, errors) = save_all(server.port(), dir.path(), Duration::from_secs(30));
        let saves = server.saves();
        server.stop();

        assert!(done.is_empty(), "nothing is loaded, nothing saves");
        assert_eq!(
            errors,
            Vec::<String>::new(),
            "an empty server is not an error, got: {errors:?}"
        );
        assert!(saves.is_empty(), "no model, no save request");
        assert_eq!(
            dumps(dir.path()),
            vec!["Qwen3.8-27B.llamastate".to_string()],
            "the prune follows a save: with nothing saved, the dumps stay"
        );
    }

    #[test]
    fn an_unreachable_port_is_one_error_and_no_hang() {
        // A bound listener, then closed: the port answers nothing. The run
        // reports it and returns, well inside the budget.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("the port").port();
        drop(listener);

        let started = std::time::Instant::now();
        let (done, errors) = save_all(
            port,
            Path::new("/nonexistent/state"),
            Duration::from_secs(30),
        );
        let took = started.elapsed();

        assert!(
            took < Duration::from_secs(29),
            "the run hung on a dead port: {took:?}"
        );
        assert!(done.is_empty(), "an unreachable server saved nothing");
        assert_eq!(
            errors.len(),
            1,
            "one clear error for an unreachable server, got: {errors:?}"
        );
    }

    #[test]
    fn a_reply_that_is_not_json_is_reported_and_not_panicked() {
        // The port answers, but not with a model list. A run that trusted the
        // reply would panic inside the framework in the middle of a stop.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("the port").port();
        let handle = std::thread::spawn(move || {
            let mut streams = Vec::new();
            for _ in 0..8 {
                let Ok((mut stream, _addr)) = listener.accept() else {
                    break;
                };
                let _ = stream.write_all(b"not json at all\n");
                streams.push(stream);
            }
        });

        let (done, errors) = save_all(
            port,
            Path::new("/nonexistent/state"),
            Duration::from_secs(30),
        );
        let _ = handle;

        assert!(
            done.is_empty(),
            "a reply that is not a model list saves nothing"
        );
        assert!(!errors.is_empty(), "the junk reply is reported as an error");
    }

    #[test]
    fn a_save_of_one_model_is_the_partial_name_then_the_rename() {
        // One model, one file, the exact pair of names. A dump that stayed at
        // the partial name is a snapshot no restore ever finds: the restore
        // looks for `<stem>.llamastate`.
        let dir = tempdir();
        let server = FakeServer::start(dir.path(), &["Qwen3.8-27B"], vec![Step::Saves]);
        let (done, errors) = save_all(server.port(), dir.path(), Duration::from_secs(30));
        let asked = server.filenames();
        server.stop();

        assert_eq!(errors, Vec::<String>::new());
        assert_eq!(done.len(), 1);
        assert_eq!(asked, vec!["Qwen3.8-27B.llamastate-partial".to_string()]);
        assert_eq!(
            dumps(dir.path()),
            vec!["Qwen3.8-27B.llamastate".to_string()],
            "the partial becomes the final dump, and the partial is gone"
        );
    }

    #[test]
    fn a_model_id_with_path_separators_is_dumped_with_every_one_substituted() {
        // llama.cpp's own comment for the save endpoint: the name must not
        // contain a separator, because the save endpoint validates it with
        // `fs_validate_filename`. The name is the WHOLE id sanitized: the same
        // rule the restore of the `pai-app` leg applies today, so the file the
        // save writes is the file the restore looks for.
        let dir = tempdir();
        let server = FakeServer::start(
            dir.path(),
            &["/models/Qwen3.8-27B/Qwen3.8-27B-Q4_K_M"],
            vec![Step::Saves],
        );
        let (done, errors) = save_all(server.port(), dir.path(), Duration::from_secs(30));
        let asked = server.filenames();
        server.stop();

        assert_eq!(errors, Vec::<String>::new());
        assert_eq!(done.len(), 1);
        assert_eq!(
            asked,
            vec!["_models_Qwen3.8-27B_Qwen3.8-27B-Q4_K_M.llamastate-partial".to_string()],
            "the file name is the whole id sanitized, never a path the engine would have to create"
        );
        assert_eq!(
            dumps(dir.path()),
            vec!["_models_Qwen3.8-27B_Qwen3.8-27B-Q4_K_M.llamastate".to_string()]
        );
    }

    #[test]
    fn the_dump_name_sanitizes_the_whole_model_id() {
        // The rule the restore follows too: every character outside
        // `[A-Za-z0-9._-]` becomes `_`, leading and trailing dots are trimmed,
        // and an id that leaves nothing is `model`. Substituting the whole id,
        // not just its last component, is what keeps two ids that differ only
        // in their folder from landing on one file.
        assert_eq!(dump_file_name("org/model:Q4"), "org_model_Q4.llamastate");
        assert_eq!(
            dump_file_name("/models/Qwen3.8-27B/Qwen3.8-27B-Q4_K_M"),
            "_models_Qwen3.8-27B_Qwen3.8-27B-Q4_K_M.llamastate"
        );
        assert_eq!(
            dump_file_name(r"C:\models\Qwen3.8-27B.gguf"),
            r"C__models_Qwen3.8-27B.gguf.llamastate"
        );
        // A leading dot would make a hidden file, a trailing one an
        // extension-less one.
        assert_eq!(dump_file_name("..hidden.."), "hidden.llamastate");
        // The empty-stem fallback: a name of nothing usable is still a file.
        assert_eq!(dump_file_name(""), "model.llamastate");
        assert_eq!(dump_file_name("///"), "___.llamastate");
    }

    #[test]
    fn a_trickling_answer_is_cut_by_the_budget() {
        // The budget bounds the WHOLE request, connect, write and every read
        // together. A server that answers a byte every 300 ms is answered at
        // every read, so a per-read timeout never fires and the stop waits
        // forever: the run must end at its deadline and leave the snapshot of
        // the last stop that finished exactly as it is.
        let dir = tempdir();
        let name = "Qwen3.8-27B.llamastate";
        std::fs::write(dir.path().join(name), "the last good snapshot").expect("the earlier dump");
        let server = FakeServer::start(dir.path(), &["Qwen3.8-27B"], vec![Step::Trickle]);
        let port = server.port();
        let path = dir.path().to_path_buf();

        let started = std::time::Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (done, errors) = save_all(port, &path, Duration::from_secs(1));
            let _ = tx.send((done.len(), errors));
        });
        let got = match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(got) => got,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!(
                "save_all did not return within 10 s of a server trickling a byte every 300 ms"
            ),
            Err(e) => panic!("the save thread panicked: {e:?}"),
        };
        let took = started.elapsed();
        server.stop();

        assert!(
            took < Duration::from_secs(3),
            "the run outlived its 1 s budget: {took:?}"
        );
        assert_eq!(got.0, 0, "a save the reply never completed saved nothing");
        assert_eq!(
            got.1.len(),
            1,
            "the trickling save is reported as one error, got: {:?}",
            got.1
        );
        assert_eq!(
            dumps(dir.path()),
            vec![name.to_string()],
            "the earlier dump stays and no -partial file is left"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join(name)).expect("the earlier dump"),
            "the last good snapshot",
            "a save that did not complete does not touch the dump the next start restores"
        );
    }

    #[test]
    fn a_failed_rename_reports_the_cause_from_the_strings_file() {
        // The engine wrote the dump, the framework could not put it where the
        // restore looks for it. That is the framework's own failure, so it is
        // the framework's text, and the io::Error behind it must survive: a
        // bare "model: name" tells the user nothing about why.
        //
        // The block: a non-empty DIRECTORY at the final name. A file cannot be
        // renamed onto one, so the rename fails for a real cause.
        let dir = tempdir();
        let model = "Qwen3.8-27B";
        let name = dump_file_name(model);
        let blocked = dir.path().join(&name);
        std::fs::create_dir(&blocked).expect("the blocked final name");
        std::fs::write(blocked.join("not-a-snapshot"), b"occupied").expect("the occupant");
        let server = FakeServer::start(dir.path(), &[model], vec![Step::Saves]);
        let (done, errors) = save_all(server.port(), dir.path(), Duration::from_secs(30));
        server.stop();

        assert!(
            done.is_empty(),
            "a dump that never reached its final name is not a saved snapshot: {done:?}"
        );
        assert_eq!(
            errors.len(),
            1,
            "the failed rename is reported once, got: {errors:?}"
        );
        let msg = &errors[0];
        assert_ne!(
            msg,
            &format!("{model}: {name}"),
            "the report is the strings file's text, not one built inline"
        );
        // The template's own text before the model name, read through the
        // strings API. A `strings::t` of this key is refused by the binding
        // lint (a 3-placeholder key asked with no values, test code
        // included), so the template is filled with a sentinel at {0}: the
        // text before the sentinel is the head, and it is a real prefix of
        // the filled message because the message fills {0} with the model.
        let head = strings::fill("snapshot-rename-failed", &[&"\u{1}", &"", &""])
            .split('\u{1}')
            .next()
            .unwrap_or("")
            .trim_end()
            .to_string();
        assert!(
            msg.starts_with(head.as_str()),
            "the message is `snapshot-rename-failed` filled in, got: {msg}"
        );
        assert!(
            !msg.starts_with("snapshot-rename-failed"),
            "the key is in the strings file, got: {msg}"
        );
        assert!(
            msg.contains(model),
            "the message names the model, got: {msg}"
        );
        assert!(
            msg.contains(name.as_str()),
            "the message names the file, got: {msg}"
        );
        assert!(
            !msg.contains("llamastate-partial"),
            "the message is not a listing of the half-written name, got: {msg}"
        );
        assert_eq!(
            dumps(dir.path()),
            vec![name],
            "the failed save leaves no -partial file behind"
        );
    }
}
