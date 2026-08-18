//! Slot KV-cache snapshots: write the live conversation to disk when the server
//! stops, and hand it back to llama-server on the next start.
//!
//! WHY IT EXISTS. Prefill is quadratic in the prompt length, so a long agentic
//! session pays a cost that dwarfs everything else the framework tunes.
//! Measured on Qwen3.8-27B (hybrid `qwen35`, 17 of 65 blocks carrying
//! attention) across a 234k-token prompt: the marginal cost per token fits
//! `0.95 ms + 0.0287 ms per 1k of context`, i.e. 984 t/s at position 4k against
//! 142 t/s at 218k, and integrating that line puts a cold 234k prefill at
//! roughly 17 MINUTES. The KV cache is process memory, so every restart of
//! llama-server pays it again from zero. Dumping the slot instead is sequential
//! IO, seconds on an NVMe: the file holds the KV cache of the tokens actually in
//! the conversation (so it tracks the conversation, not `ctx-size`), which for
//! this arch is 36 KiB per token, i.e. 4.38 GiB measured at 127,579 tokens and
//! ~8.3 GiB for a full 262k context.
//!
//! HOW IT REACHES THE CHILD, which is the part that nearly killed the feature.
//! The endpoints only exist on a server started with `--slot-save-path`, and the
//! framework always launches the ROUTER (`--models-preset`), whose per-model
//! children are separate processes with their own command lines. Two facts make
//! it work anyway:
//!
//!  * `--slot-save-path` has NO env name (`common/arg.cpp` gives it only
//!    `set_examples`), so it cannot ride a preset key like `LLAMA_ARG_*`. But
//!    `get_map_key_opt` (`common/preset.cpp`) maps every option by its FLAG name
//!    with the dashes stripped as well as by its env name, so the router parses
//!    its own `--slot-save-path` into `base_preset` (`load_from_args`) and
//!    `preset.merge(base_preset)` copies it onto every model. `unset_reserved_args`
//!    does not prune it. So one flag on the router's line reaches every child.
//!  * llama.cpp validates the directory while PARSING the flag
//!    (`fs_is_directory` or it throws "not a directory"), which is why
//!    [`ensure_dir`] runs before the launch and not lazily at save time. A
//!    missing directory does not disable the feature, it stops the server from
//!    starting at all.
//!
//! WHY THE RESTORE IS WORTH ANYTHING. `SERVER_TASK_TYPE_SLOT_RESTORE`
//! (`tools/server/server-context.cpp`) does not merely refill the KV buffer: it
//! deserializes the token list saved alongside it into `slot->prompt.tokens`.
//! That is what the next request's `get_common_prefix` compares against, so a
//! restored slot is indistinguishable from one the server prefilled itself and
//! the prompt is skipped. Without that half, the dump would be a file nobody
//! reads.
//!
//! SCOPE, deliberately narrow on the way back in. Saving covers every model the
//! router reports as `loaded`; restoring takes the NEWEST dump only. A dump is
//! one conversation, restoring is an 8 GiB read plus a full model load, and the
//! LRU would evict whatever a second restore pushed out anyway. Restoring the
//! one model the user was last working on is the predictable choice; the others
//! stay on disk and are simply overwritten the next time they are live.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Extension every dump carries. Not `.bin`: the directory is user-visible and
/// these are 8 GiB files, so it should be obvious what may be deleted.
const STATE_EXT: &str = "llamastate";

/// Read timeout for the save and restore calls. Generous on purpose: the server
/// answers only once the whole state has hit the disk, which at ~8 GiB is tens
/// of seconds on NVMe and minutes on a spinning disk. The cost of guessing too
/// low is a half-written dump reported as a failure.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(900);

/// Read timeout for the small control call (`GET /models`). Short: if the router
/// is not answering in five seconds it is not going to save anything either.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);

/// The slot to snapshot. The framework runs `parallel = 1`, so slot 0 is the
/// only one a child has, and it is the one holding the conversation.
const SLOT_ID: u32 = 0;

// ── Where the dumps live ─────────────────────────────────────────────────

/// Default state directory when `StateDir` is unset: a sibling of `config\` and
/// `logs\` under the user's runtime root.
///
/// Overridable precisely BECAUSE of the default's one weakness: `%LOCALAPPDATA%`
/// sits on the system drive, and a dump is roughly one byte of KV cache per byte
/// the model would have recomputed, which for a 262k context is 8.3 GiB per
/// model. A user whose models live on another disk will want the snapshots there
/// too, and filling C:\ with no way out is not a default anyone should be stuck
/// with.
pub fn default_state_dir() -> PathBuf {
    crate::paths::data_root().join("state")
}

/// The configured state directory, or [`default_state_dir`] when unset/blank.
/// The single home for that fallback, shared by the launch path, the save/restore
/// legs and the form.
pub fn state_dir(cfg: &crate::server_cfg::ServerConfig) -> PathBuf {
    cfg.state_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(default_state_dir, PathBuf::from)
}

/// Create the state directory if it is missing.
///
/// Called from `runstate::start` BEFORE the spawn, never lazily: llama.cpp
/// validates `--slot-save-path` while parsing the argument and throws
/// "not a directory" on a path that does not exist yet, so a missing directory
/// does not degrade to "no snapshots", it stops the server from starting.
pub fn ensure_dir(cfg: &crate::server_cfg::ServerConfig) -> std::io::Result<PathBuf> {
    let dir = state_dir(cfg);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The dump file name for a model, as llama-server will see it.
///
/// Sanitized rather than escaped, because the server rejects the request
/// outright when `fs_validate_filename` dislikes the name and the error would
/// surface as a mysterious failed save. Anything outside `[A-Za-z0-9._-]`
/// becomes `_`, so two models whose names differ only in punctuation could
/// collide; that costs one wasted restore attempt (the token lists will not
/// match and the prompt is prefilled normally), never a wrong answer.
pub fn state_file_name(model: &str) -> String {
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

/// Every dump currently on disk, newest first. `mtime` is what orders them, so
/// the head is the model that was live when the server last went down.
fn dumps_newest_first(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == STATE_EXT))
        .filter_map(|p| {
            let t = p.metadata().ok()?.modified().ok()?;
            Some((t, p))
        })
        .collect();
    found.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    found.into_iter().map(|(_, p)| p).collect()
}

// ── Minimal HTTP client ──────────────────────────────────────────────────

/// One HTTP/1.1 response: the status code and the body.
struct Response {
    status: u16,
    body: String,
}

/// Send one request to llama-server on the loopback interface and read the whole
/// response.
///
/// Hand-rolled rather than pulled from a crate on purpose: the whole surface is
/// two calls to 127.0.0.1 with a JSON body of a few dozen bytes, and the
/// alternative (`reqwest`) drags an async runtime into a crate that has none.
/// `Connection: close` is what lets the body be read to EOF, so no
/// `Content-Length` bookkeeping is needed; chunked responses are decoded because
/// cpp-httplib may pick that framing regardless.
///
/// Always 127.0.0.1, never the configured `Hostname`: that value is the address
/// llama-server BINDS (`0.0.0.0` is a common one and not connectable as a
/// destination on every stack), while loopback reaches it under every binding
/// the framework can produce.
fn request(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&str>,
    timeout: Duration,
) -> Result<Response, String> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = std::net::TcpStream::connect_timeout(&addr, CONTROL_TIMEOUT)
        .map_err(|e| format!("cannot reach llama-server on port {port}: {e}"))?;
    stream.set_read_timeout(Some(timeout)).map_err(err_str)?;
    stream
        .set_write_timeout(Some(CONTROL_TIMEOUT))
        .map_err(err_str)?;

    let payload = body.unwrap_or("");
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Connection: close\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n",
        payload.len()
    );
    req.push_str(payload);
    stream.write_all(req.as_bytes()).map_err(err_str)?;
    stream.flush().map_err(err_str)?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(err_str)?;
    parse_response(&String::from_utf8_lossy(&raw))
}

fn err_str(e: std::io::Error) -> String {
    e.to_string()
}

/// Split a raw HTTP response into its status code and decoded body.
/// Pure, so the chunked path is unit-testable without a socket.
fn parse_response(raw: &str) -> Result<Response, String> {
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| "truncated response from llama-server".to_string())?;

    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| "unreadable status line from llama-server".to_string())?;

    let chunked = head
        .lines()
        .any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.contains("chunked"));

    let body = if chunked {
        dechunk(body)
    } else {
        body.to_string()
    };
    Ok(Response { status, body })
}

/// Decode `Transfer-Encoding: chunked` framing. A malformed stream yields what
/// was decoded so far: the caller only ever reads a status message out of it, so
/// a partial body degrades the reporting rather than the outcome.
fn dechunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some((size_line, after)) = rest.split_once("\r\n") {
        let size = size_line.split(';').next().unwrap_or("").trim();
        let Ok(n) = usize::from_str_radix(size, 16) else {
            break;
        };
        if n == 0 || after.len() < n {
            out.push_str(&after[..n.min(after.len())]);
            break;
        }
        out.push_str(&after[..n]);
        rest = after[n..].strip_prefix("\r\n").unwrap_or(&after[n..]);
    }
    out
}

// ── Model discovery ──────────────────────────────────────────────────────

/// The models the router currently has resident, in the order it lists them.
///
/// Read from the router's `GET /models`, whose per-model `status.value` is one
/// of the strings in `server_model_status_to_string`. Only `loaded` qualifies:
/// `loading` has no slot to snapshot yet and `sleeping` has released its KV
/// cache, so asking either to save produces an error, not a dump.
pub fn loaded_models(port: u16) -> Result<Vec<String>, String> {
    let res = request(port, "GET", "/models", None, CONTROL_TIMEOUT)?;
    if res.status != 200 {
        return Err(format!("llama-server answered {} to /models", res.status));
    }
    let parsed: serde_json::Value =
        serde_json::from_str(&res.body).map_err(|e| format!("unreadable /models reply: {e}"))?;
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

// ── Save & restore ───────────────────────────────────────────────────────

/// What one save or restore did, for the status line.
pub struct Transfer {
    pub model: String,
    pub bytes: u64,
    pub tokens: u64,
}

impl Transfer {
    /// "Qwen3.8-27B-UD (234,102 tokens, 8.1 GiB)". Thousands separators are hand
    /// rolled: the crate has no formatting dependency and this is the only place
    /// that wants them.
    pub fn describe(&self) -> String {
        format!(
            "{} ({} tokens, {})",
            self.model,
            thousands(self.tokens),
            gib(self.bytes)
        )
    }
}

fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn gib(bytes: u64) -> String {
    let g = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if g >= 1.0 {
        format!("{g:.1} GiB")
    } else {
        format!("{:.0} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// POST one slot action through the ROUTER, which proxies it to the child that
/// owns the model.
///
/// `model` is what picks the child: the router's `proxy_post` reads it out of the
/// JSON body (`server-models.cpp`), while the child reads `filename` from the
/// same body and the action from the query string, which the proxy forwards
/// verbatim. One request therefore carries both routing and payload, and on
/// restore it also makes the router LOAD the model if it is not up, which is
/// exactly what a cold start needs.
fn slot_action(port: u16, action: &str, model: &str, filename: &str) -> Result<Transfer, String> {
    let body = serde_json::json!({ "filename": filename, "model": model }).to_string();
    let path = format!("/slots/{SLOT_ID}?action={action}");
    let res = request(port, "POST", &path, Some(&body), TRANSFER_TIMEOUT)?;

    let parsed: serde_json::Value =
        serde_json::from_str(&res.body).unwrap_or(serde_json::Value::Null);
    if res.status != 200 {
        // llama-server's error body is `{"error": {"message": "..."}}`; fall back
        // to the raw body so a proxy or gateway reply is not swallowed.
        let msg = parsed
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .map_or_else(|| res.body.trim().to_string(), str::to_string);
        return Err(format!("{model}: {msg}"));
    }
    Ok(transfer_from_json(model, &parsed))
}

/// Pull the counts out of a save/restore reply.
///
/// Split out to be testable without a socket, because the key mapping is the one
/// thing here that is easy to get backwards and impossible to notice: the two
/// directions report the SAME two numbers under four different names
/// (`server_task_result_slot_save_load::to_json`), and `n_saved` / `n_restored`
/// count TOKENS while `n_written` / `n_read` count bytes. Neither direction
/// emits an `n_tokens`, so reading one yields a silent zero rather than an error.
fn transfer_from_json(model: &str, parsed: &serde_json::Value) -> Transfer {
    let num = |k: &str| parsed.get(k).and_then(serde_json::Value::as_u64);
    Transfer {
        model: model.to_string(),
        bytes: num("n_written").or_else(|| num("n_read")).unwrap_or(0),
        tokens: num("n_saved").or_else(|| num("n_restored")).unwrap_or(0),
    }
}

/// Snapshot every loaded model, then prune the dumps of models that are no
/// longer live.
///
/// The prune is what keeps the directory meaningful: it makes the state dir a
/// picture of the LAST shutdown rather than an accumulating pile, which in turn
/// is what lets [`restore_newest`] treat "the newest file" as "the model you
/// were working on". It runs only over models that saved successfully, so a
/// failed save never deletes the older dump it failed to replace.
///
/// Returns the transfers plus a per-model error list; a partial result is still
/// worth reporting, since one model failing to snapshot says nothing about the
/// others.
pub fn save_all(cfg: &crate::server_cfg::ServerConfig) -> (Vec<Transfer>, Vec<String>) {
    let port = cfg.port_or_default() as u16;
    let dir = match ensure_dir(cfg) {
        Ok(d) => d,
        Err(e) => {
            return (
                Vec::new(),
                vec![format!("cannot create the state directory: {e}")],
            )
        }
    };

    let models = match loaded_models(port) {
        Ok(m) => m,
        Err(e) => return (Vec::new(), vec![e]),
    };
    if models.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut done = Vec::new();
    let mut errors = Vec::new();
    let mut keep = Vec::new();
    for model in &models {
        let filename = state_file_name(model);
        match slot_action(port, "save", model, &filename) {
            Ok(t) => {
                keep.push(filename);
                done.push(t);
            }
            Err(e) => errors.push(e),
        }
    }

    if !done.is_empty() {
        for stale in dumps_newest_first(&dir) {
            let is_kept = stale
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| keep.iter().any(|k| k == n));
            if !is_kept {
                let _ = std::fs::remove_file(stale);
            }
        }
    }
    (done, errors)
}

/// Hand the newest dump back to llama-server.
///
/// `Ok(None)` means there was nothing to restore, which is the ordinary state on
/// a first run and is not an error. The model name is recovered from the file
/// name, so it must round-trip through [`state_file_name`]: the candidate list
/// comes from the presets, and the one whose sanitized name matches wins.
///
/// The router loads the model as a side effect of the proxied request, so this
/// is slow (a full model load plus an 8 GiB read) and belongs on a worker
/// thread, never on the UI thread.
pub fn restore_newest(
    cfg: &crate::server_cfg::ServerConfig,
    known_models: &[String],
) -> Result<Option<Transfer>, String> {
    let port = cfg.port_or_default() as u16;
    let dir = state_dir(cfg);
    let Some(newest) = dumps_newest_first(&dir).into_iter().next() else {
        return Ok(None);
    };
    let filename = newest
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "unreadable dump file name".to_string())?
        .to_string();

    let model = known_models
        .iter()
        .find(|m| state_file_name(m) == filename)
        .ok_or_else(|| {
            format!("{filename} does not match any configured model; ignoring the snapshot")
        })?;

    slot_action(port, "restore", model, &filename).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_is_sanitized_and_stable() {
        assert_eq!(
            state_file_name("Qwen3.8-27B-UD"),
            "Qwen3.8-27B-UD.llamastate"
        );
        // Path separators are what `fs_validate_filename` rejects outright.
        assert_eq!(state_file_name("a/b\\c"), "a_b_c.llamastate");
        // Substitution, not deletion: a name of nothing but separators still
        // yields a usable (and still distinct) file rather than collapsing.
        assert_eq!(state_file_name("///"), "___.llamastate");
        // Leading/trailing dots would make a hidden or extension-less file.
        assert_eq!(state_file_name("..x.."), "x.llamastate");
        // The empty-stem fallback is reachable only once the dots are trimmed,
        // which is the one input that would otherwise produce a bare ".llamastate".
        assert_eq!(state_file_name("..."), "model.llamastate");
        assert_eq!(state_file_name(""), "model.llamastate");
    }

    #[test]
    fn parses_a_plain_response() {
        let raw = "HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n{\"a\": 1}\n";
        let res = parse_response(raw).unwrap();
        assert_eq!(res.status, 200);
        assert!(res.body.starts_with("{\"a\": 1}"));
    }

    #[test]
    fn parses_a_chunked_response() {
        let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                   4\r\n{\"a\"\r\n4\r\n: 1}\r\n0\r\n\r\n";
        let res = parse_response(raw).unwrap();
        assert_eq!(res.status, 200);
        assert_eq!(res.body, "{\"a\": 1}");
    }

    #[test]
    fn reports_a_non_200_status() {
        let raw = "HTTP/1.1 501 Not Implemented\r\nContent-Length: 2\r\n\r\n{}";
        assert_eq!(parse_response(raw).unwrap().status, 501);
    }

    #[test]
    fn truncated_response_is_an_error() {
        assert!(parse_response("HTTP/1.1 200 OK").is_err());
    }

    #[test]
    fn formats_sizes_and_counts() {
        let t = Transfer {
            model: "M".into(),
            bytes: 8_912_896_000,
            tokens: 234_102,
        };
        assert_eq!(t.describe(), "M (234,102 tokens, 8.3 GiB)");
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(gib(512 * 1024 * 1024), "512 MiB");
    }

    /// The save and restore replies name the same two quantities differently,
    /// and swapping them is invisible (a missing key reads as 0, not an error).
    /// Both shapes are copied from `server_task_result_slot_save_load::to_json`.
    #[test]
    fn counts_are_read_from_the_right_keys() {
        let save = serde_json::json!({
            "id_slot": 0, "filename": "m.llamastate",
            "n_saved": 234_102, "n_written": 8_912_896_000u64,
            "timings": { "save_ms": 41_000.0 }
        });
        let t = transfer_from_json("m", &save);
        assert_eq!(t.tokens, 234_102, "n_saved is a TOKEN count");
        assert_eq!(t.bytes, 8_912_896_000, "n_written is the byte count");

        let restore = serde_json::json!({
            "id_slot": 0, "filename": "m.llamastate",
            "n_restored": 234_102, "n_read": 8_912_896_000u64,
            "timings": { "restore_ms": 9_000.0 }
        });
        let t = transfer_from_json("m", &restore);
        assert_eq!(t.tokens, 234_102);
        assert_eq!(t.bytes, 8_912_896_000);

        // A reply with neither pair must degrade to zeroes, never panic.
        let t = transfer_from_json("m", &serde_json::json!({}));
        assert_eq!((t.tokens, t.bytes), (0, 0));
    }

    /// `loaded` is the only status with a slot worth dumping: `sleeping` has
    /// released its KV cache and `loading` has not built one yet.
    #[test]
    fn only_loaded_models_are_listed() {
        let body = serde_json::json!({
            "object": "list",
            "data": [
                {"id": "up",       "status": {"value": "loaded"}},
                {"id": "asleep",   "status": {"value": "sleeping"}},
                {"id": "starting", "status": {"value": "loading"}},
                {"id": "down",     "status": {"value": "unloaded"}},
            ]
        })
        .to_string();
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        let names: Vec<&str> = parsed["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["status"]["value"] == "loaded")
            .filter_map(|m| m["id"].as_str())
            .collect();
        assert_eq!(names, vec!["up"]);
    }
}
