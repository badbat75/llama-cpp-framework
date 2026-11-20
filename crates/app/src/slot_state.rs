//! Slot KV-cache snapshots: write the live conversation to disk when the server
//! stops, and hand it back to llama-server on the next start.
//!
//! WHY IT EXISTS. Prefill is quadratic in the prompt length, so a long agentic
//! session pays a cost that dwarfs everything else the framework tunes
//! (`crates/config/README.md`, "Conversation snapshots", has the measured
//! cost). The KV cache is process memory, so every restart of
//! llama-server pays it again from zero. Dumping the slot instead is sequential
//! IO: the file holds the KV cache of the tokens actually in
//! the conversation (so it tracks the conversation, not `ctx-size`; size per
//! token: `docs/qwen3.8_27b.md` section 1). Since v0.7.0
//! the file also carries the slot's context checkpoints (#26004, so a restored
//! hybrid / SWA slot can still roll back to one instead of re-reading the
//! prompt) and the draft context's state (#30574, so a restored slot does not
//! draft from an empty KV). Expect it larger by up to `ctx-checkpoints` copies
//! of the recurrent state plus the draft's KV.
//!
//! HOW IT REACHES THE CHILD, which is the part that nearly killed the feature.
//! The endpoints only exist on a server started with `--slot-save-path`, and the
//! framework always launches the ROUTER (`--models-preset`), whose per-model
//! children are separate processes with their own command lines. Two facts make
//! it work anyway:
//!
//!  * `--slot-save-path` had NO env name until v0.7.0 added
//!    `LLAMA_ARG_SLOT_SAVE_PATH` (#30272); the framework never needed one,
//!    because `get_map_key_opt` (`common/preset.cpp`) maps every option by its FLAG name
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
//! one conversation, restoring is a read of its whole KV cache plus a full model load, and the
//! LRU would evict whatever a second restore pushed out anyway. Restoring the
//! one model the user was last working on is the predictable choice; the others
//! stay on disk and are simply overwritten the next time they are live.
//!
//! WHAT LIVES HERE. The save leg is the core's (`pai_core::slot_save`), because
//! the instance's own stop reaches it without this crate: the client and the
//! service therefore write the same dumps. [`save_all`] is the seam the GUI and
//! the command line call, and the restore leg is entirely here.

use crate::strings;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::http::request;
use pai_core::slot_save::STATE_EXT;

/// Read timeout for the restore call. Generous on purpose: the server answers
/// only once the model is loaded and the whole state has been read back, which
/// for a long conversation can take minutes on a slow disk. The cost of guessing
/// too low is a restore reported as a failure while the server is still working.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(900);

/// Read timeout for the small control call (`GET /health`). Short: a router that
/// is not answering in five seconds is not going to restore anything either.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a restore waits for the router to open its port after a start.
///
/// The router listens only once it has measured the memory of every model profile it
/// does not have a figure for (`server_run_measure`, one short-lived child per
/// model profile, run before `ctx_http.start()` in `server.cpp`), and until
/// `ctx_http.is_ready` it answers 503 to everything. Observed: a few seconds on
/// a warm machine, long enough on 2026-10-29 that a fixed 3 s look found the
/// port closed and threw the snapshot away. Five minutes is an order of
/// magnitude above a slow measurement pass yet still bounded, and the wait
/// ends at once if the server process exits. The model load that follows is
/// NOT counted here: the restore request itself waits for it (see
/// [`slot_action`]), under [`TRANSFER_TIMEOUT`].
const LISTEN_TIMEOUT: Duration = Duration::from_secs(300);

/// Pause between two probes of a router that is not answering yet.
const LISTEN_POLL: Duration = Duration::from_millis(500);

/// The slot to snapshot. The framework runs `parallel = 1`, so slot 0 is the
/// only one a child has, and it is the one holding the conversation.
const SLOT_ID: u32 = 0;

// ── Where the dumps live: the core (`pai_core::slot_state`) ─────────────

pub use pai_core::slot_state::{ensure_dir, state_dir};

/// The dump file name for a model, as llama-server will see it: the core's rule
/// (`pai_core::slot_save::dump_file_name`), because the save leg there writes
/// that name and the restore here is the one that reads it.
pub fn state_file_name(model: &str) -> String {
    pai_core::slot_save::dump_file_name(model)
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
        strings::fill(
            "snapshot-describe",
            &[&self.model, &thousands(self.tokens), &gib(self.bytes)],
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

/// Wait until the router answers `GET /health` with 200, i.e. it is listening
/// and past `is_ready` (before that its middleware answers 503 "Loading model").
///
/// `alive` is asked between probes so a server that died during startup ends
/// the wait at once instead of after `timeout`; it is a parameter so a test can
/// run without a llama-server process.
fn wait_listening(
    port: u16,
    timeout: Duration,
    step: Duration,
    mut alive: impl FnMut() -> bool,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Ok(res) = request(port, "GET", "/health", None, CONTROL_TIMEOUT) {
            if res.status == 200 {
                return Ok(());
            }
        }
        if !alive() {
            return Err(strings::fill("exited-before-answer", &[&port]));
        }
        if std::time::Instant::now() >= deadline {
            return Err(strings::fill(
                "no-answer-within",
                &[&port, &timeout.as_secs()],
            ));
        }
        std::thread::sleep(step);
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
/// exactly what a cold start needs: the router's `proxy_post` runs
/// `ensure_model_ready` (`server-models.cpp`), which loads the model (through
/// the scheduler's queue) and holds the request until the child reports
/// `loaded`, so the restore never reaches a child that is still loading. That
/// wait counts against [`TRANSFER_TIMEOUT`], which a model load leaves ample.
/// What it cannot cover is a router that is not listening yet: the caller of a
/// restore after a start waits for that first ([`wait_listening`]).
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
/// The saving is the core's (`pai_core::slot_save`), the same code the
/// instance's own stop runs, so the client and the service write the same dumps.
/// This is the seam the GUI and the command line call: it reads the port and the
/// state folder out of `cfg` and hands over the core's `SAVE_BUDGET`, the one
/// deadline for the whole run. The core writes each dump under a `-partial`
/// name and renames
/// it only once the server has answered the whole save.
///
/// The prune is what keeps the directory meaningful: it makes the state dir a
/// picture of the LAST shutdown rather than an accumulating pile, which in turn
/// is what lets [`restore_newest`] treat "the newest file" as "the model you
/// were working on". Every model the server reported as loaded keeps its dump,
/// saved or refused: the dump of a save that did not complete is the snapshot
/// the next start restores.
///
/// Returns the transfers plus a per-model error list; a partial result is still
/// worth reporting, since one model failing to snapshot says nothing about the
/// others.
pub fn save_all(cfg: &crate::server_cfg::ServerConfig) -> (Vec<Transfer>, Vec<String>) {
    let port = cfg.port_or_default() as u16;
    let dir = match ensure_dir(cfg) {
        Ok(d) => d,
        Err(e) => return (Vec::new(), vec![strings::fill("state-dir-failed", &[&e])]),
    };
    let (done, errors) =
        pai_core::slot_save::save_all(port, &dir, pai_core::slot_save::SAVE_BUDGET);
    (
        done.into_iter()
            .map(|t| Transfer {
                model: t.model,
                bytes: t.bytes,
                tokens: t.tokens,
            })
            .collect(),
        errors,
    )
}

/// Hand the newest dump back to llama-server.
///
/// `Ok(None)` means there was nothing to restore, which is the ordinary state on
/// a first run and is not an error. The model name is recovered from the file
/// name, so it must round-trip through [`state_file_name`]: the candidate list
/// comes from the model profiles, and the one whose sanitized name matches wins.
///
/// Called right after a start, so it first waits (up to [`LISTEN_TIMEOUT`]) for
/// the router to open its port: sent at once, the request met a router still
/// measuring its model profiles and failed with "connection refused".
/// The router then loads the model as a side effect of the proxied request, so
/// this is slow (a full model load plus a read of the whole KV cache) and belongs on a worker
/// thread, never on the UI thread.
pub fn restore_newest(
    cfg: &crate::server_cfg::ServerConfig,
    known_models: &[String],
) -> Result<Option<Transfer>, String> {
    restore_newest_with(cfg, known_models, || {
        crate::server_control::backend().is_running()
    })
}

/// `restore_newest` with `alive` as the probe of whether the server is still
/// there while it waits for the port (the caller's backend, not the global one).
pub fn restore_newest_with(
    cfg: &crate::server_cfg::ServerConfig,
    known_models: &[String],
    alive: impl FnMut() -> bool,
) -> Result<Option<Transfer>, String> {
    let port = cfg.port_or_default() as u16;
    let dir = crate::server_cfg::effective_state_dir(
        cfg,
        crate::server_control::backend().kind(),
        crate::server_control::Os::current(),
        // The instance the configurator drives: an unset StateDir is that
        // instance's folder, the one the launch passes, never the root of
        // every instance's folders.
        &crate::paths::instance_state_dir().join("default"),
    );
    let Some(newest) = dumps_newest_first(&dir).into_iter().next() else {
        return Ok(None);
    };
    let filename = newest
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| strings::t("dump-name-unreadable"))?
        .to_string();

    let model = known_models
        .iter()
        .find(|m| state_file_name(m) == filename)
        .ok_or_else(|| strings::fill("dump-no-model", &[&filename]))?;

    wait_listening(port, LISTEN_TIMEOUT, LISTEN_POLL, alive)?;
    slot_action(port, "restore", model, &filename).map(Some)
}

/// E2E phase for `data_root_e2e` (`src/tests/mod.rs`): it redirects the data
/// tree. Under the manager (Linux and Windows; the test build's default
/// backend is a manager) an instance whose StateDir is unset writes its
/// snapshots to the folder of ITS OWN: the engine's state root
/// (`paths::instance_state_dir()`) with the instance's id joined, the folder
/// the launch passes as `--slot-save-path`. The restore after a start reads
/// that folder: neither the engine's root (where a dump of another instance
/// sits), nor the user's data root. On macOS it is the user's default, as
/// before. A dump no model profile matches is refused before any request, so
/// the error names the file the restore picked.
#[cfg(test)]
pub(crate) fn restore_reads_the_instance_state_dir_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    // The instance the configurator drives.
    let root = crate::paths::instance_state_dir();
    let instance = root.join("default");
    let user = pai_core::slot_state::default_state_dir();
    assert_ne!(instance, user, "fixture sanity: the two defaults differ");
    assert_ne!(instance, root, "fixture sanity: a folder of its own");
    std::fs::create_dir_all(&instance).expect("instance state dir");
    std::fs::create_dir_all(&user).expect("user state dir");
    let instance_dump = state_file_name("Qwen3.8-27B");
    let user_dump = state_file_name("Qwen3.8-Flash-Next");
    let root_dump = state_file_name("Qwen2.5-7B");
    std::fs::write(instance.join(&instance_dump), b"dump").expect("instance dump");
    std::fs::write(user.join(&user_dump), b"dump").expect("user dump");
    // A dump sitting in the engine's root, beside the folders of the
    // instances: it belongs to no instance, so the restore must not see it.
    std::fs::write(root.join(&root_dump), b"dump").expect("root dump");

    let expected = if cfg!(any(windows, target_os = "linux")) {
        &instance_dump
    } else {
        &user_dump
    };
    for cfg in [
        crate::server_cfg::ServerConfig::default(),
        crate::server_cfg::ServerConfig {
            state_dir: Some("  ".into()),
            ..Default::default()
        },
    ] {
        let got = restore_newest_with(&cfg, &[], || false).map(|t| t.map(|t| t.model));
        assert_eq!(
            got,
            Err(strings::fill("dump-no-model", &[expected])),
            "the restore reads the state folder of the instance for {cfg:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The restore asks the probe it is given (the caller's backend, here a
    /// fake manager that reports the instance gone) while the port does not
    /// answer, and gives up at once with the probe's answer.
    #[test]
    fn restore_asks_the_probe_it_is_given() {
        use crate::server_control::fake::{self, FakeClient};
        use pai_core::client::InstanceState;
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(state_file_name("Qwen3.8-27B")), b"dump").expect("dump");
        let port = free_port();
        let cfg = crate::server_cfg::ServerConfig {
            port: Some(i32::from(port)),
            state_dir: Some(dir.path().display().to_string()),
            ..Default::default()
        };
        let log = fake::log();
        let b = fake::backend(FakeClient::new(&log, InstanceState::Inactive));
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = restore_newest_with(&cfg, &["Qwen3.8-27B".to_string()], || b.is_running());
            let _ = tx.send(r.map(|t| t.map(|t| t.model)));
        });
        let result = match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(r) => r,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("restore did not give up within 20 s")
            }
            Err(e) => panic!("the restore thread panicked: {e:?}"),
        };
        assert_eq!(result, Err(strings::fill("exited-before-answer", &[&port])));
        assert!(
            log.lock().unwrap().iter().any(|e| e == "status default"),
            "the given backend was asked: {log:?}"
        );
    }

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

    /// The name is the core's: the save leg in `pai_core::slot_save` writes it,
    /// the restore here reads it. One rule, one owner, or a dump lands at a
    /// name no start ever looks for (an id with a separator is where they
    /// differed: the save took the last component, the restore the whole id).
    #[test]
    fn the_dump_name_is_the_cores() {
        for id in [
            "Qwen3.8-27B-UD",
            "org/model:Q4",
            r"C:\models\Qwen3.8-27B.gguf",
            "/models/Qwen3.8-27B/Qwen3.8-27B-Q4_K_M",
            ".hidden.",
            "",
        ] {
            assert_eq!(
                state_file_name(id),
                pai_core::slot_save::dump_file_name(id),
                "the name the save writes and the name the restore reads must be one name for {id:?}"
            );
        }
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

    // ── Waiting for the router after a start ─────────────────────────────

    /// A port nothing listens on yet: bound to learn a free number, then
    /// released, so a connect is refused as it was on 2026-10-29.
    fn free_port() -> u16 {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        l.local_addr().unwrap().port()
    }

    /// A stand-in router that starts listening on `port` after `delay` and
    /// answers its connections in order with `replies` (status, body), then
    /// stops. Returns the request lines it saw.
    fn fake_router(
        port: u16,
        delay: Duration,
        replies: Vec<(u16, &'static str)>,
    ) -> std::thread::JoinHandle<Vec<String>> {
        use std::io::{BufRead, BufReader, Read, Write};
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let l = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
            let mut seen = Vec::new();
            for (status, body) in replies {
                let (mut s, _) = l.accept().unwrap();
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                seen.push(line.trim().to_string());
                let mut len = 0;
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                }
                let mut req_body = vec![0; len];
                r.read_exact(&mut req_body).unwrap();
                write!(
                    s,
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
            seen
        })
    }

    /// The 2026-10-29 failure: the router opens its port seconds after the
    /// launch, then answers 503 until it is ready. The restore must wait it
    /// out and only then send the slot request.
    #[test]
    fn restore_waits_for_a_router_that_listens_late() {
        let port = free_port();
        let began = std::time::Instant::now();
        let router = fake_router(
            port,
            Duration::from_millis(800),
            vec![
                (503, r#"{"error":{"message":"Loading model"}}"#),
                (200, r#"{"status":"ok"}"#),
                (200, r#"{"n_restored":166000,"n_read":5000000000}"#),
            ],
        );
        wait_listening(
            port,
            Duration::from_secs(10),
            Duration::from_millis(50),
            || true,
        )
        .unwrap();
        assert!(began.elapsed() >= Duration::from_millis(800));
        let t = slot_action(port, "restore", "M", "M.llamastate").unwrap();
        assert_eq!(t.tokens, 166_000);
        let seen = router.join().unwrap();
        assert_eq!(
            seen,
            vec![
                "GET /health HTTP/1.1",
                "GET /health HTTP/1.1",
                "POST /slots/0?action=restore HTTP/1.1",
            ]
        );
    }

    /// Nobody ever listens: the wait gives up once the timeout is past, with
    /// a message that names the port.
    #[test]
    fn wait_gives_up_after_the_timeout() {
        let port = free_port();
        let began = std::time::Instant::now();
        let err = wait_listening(
            port,
            Duration::from_millis(300),
            Duration::from_millis(50),
            || true,
        )
        .unwrap_err();
        assert!(began.elapsed() >= Duration::from_millis(300));
        assert!(err.contains(&format!("port {port} within")), "{err}");
    }

    /// A server that died during startup ends the wait at once instead of
    /// after the full timeout.
    #[test]
    fn wait_stops_when_the_server_exits() {
        let port = free_port();
        let began = std::time::Instant::now();
        let err = wait_listening(
            port,
            Duration::from_secs(60),
            Duration::from_secs(60),
            || false,
        )
        .unwrap_err();
        assert!(began.elapsed() < Duration::from_secs(10));
        assert!(err.contains("exited"), "{err}");
    }

    // ── The save leg: a wrapper over the core's ──────────────────────────

    /// The `filename` a save request asks the server to write.
    fn requested_filename(body: &str) -> Option<String> {
        serde_json::from_str::<serde_json::Value>(body)
            .ok()?
            .get("filename")?
            .as_str()
            .map(str::to_string)
    }

    /// The names in `dir`, sorted (a `read_dir` listing is unordered).
    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("read the state dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// A stand-in router that writes what it is asked to write, as llama-server
    /// does: it answers `GET /models` with `models_body` and then, for each of
    /// the `saves` saves that follow, writes the `filename` of the request into
    /// `dir` and answers with the counts. The listener is bound before the
    /// helper returns, so the caller never races it. Returns the port and the
    /// request lines and bodies, in the order the router saw them.
    ///
    /// Every accept is bounded: a caller that stops asking ends the thread with
    /// what it saw, so a red is an assertion, never a hang.
    fn saving_router(
        dir: &Path,
        models_body: &str,
        saves: usize,
    ) -> (u16, std::thread::JoinHandle<Vec<(String, String)>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpStream;
        fn read_request(sock: &mut TcpStream) -> (String, String) {
            let mut r = BufReader::new(sock.try_clone().expect("clone socket"));
            let mut line = String::new();
            r.read_line(&mut line).expect("request line");
            let mut len = 0usize;
            loop {
                let mut head = String::new();
                if r.read_line(&mut head).expect("request header") == 0 {
                    break;
                }
                if head.trim().is_empty() {
                    break;
                }
                if let Some(v) = head.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().expect("content-length");
                }
            }
            let mut body = vec![0u8; len];
            if len > 0 {
                r.read_exact(&mut body).expect("request body");
            }
            (
                line.trim().to_string(),
                String::from_utf8_lossy(&body).to_string(),
            )
        }
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback");
        let port = l.local_addr().expect("local addr").port();
        l.set_nonblocking(true).expect("nonblocking listener");
        let dir = dir.to_path_buf();
        let list = models_body.to_string();
        (
            port,
            std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(20);
                let mut seen = Vec::new();
                for i in 0..=saves {
                    let mut sock = loop {
                        match l.accept() {
                            Ok((sock, _)) => {
                                sock.set_nonblocking(false).expect("blocking socket");
                                sock.set_read_timeout(Some(Duration::from_secs(10)))
                                    .expect("read timeout");
                                break sock;
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                if std::time::Instant::now() > deadline {
                                    return seen;
                                }
                                std::thread::sleep(Duration::from_millis(20));
                            }
                            Err(e) => panic!("accept: {e}"),
                        }
                    };
                    let (line, body) = read_request(&mut sock);
                    seen.push((line, body.clone()));
                    let reply = if i == 0 {
                        (200, list.clone())
                    } else {
                        if let Some(name) = requested_filename(&body) {
                            std::fs::write(dir.join(name), "the bytes llama-server wrote")
                                .expect("write the dump");
                        }
                        (
                            200,
                            r#"{"id_slot":0,"n_saved":166000,"n_written":1024}"#.to_string(),
                        )
                    };
                    write!(
                        sock,
                        "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        reply.0,
                        reply.1.len(),
                        reply.1
                    )
                    .expect("reply");
                }
                seen
            }),
        )
    }

    /// The save the GUI and `pai` call is a thin wrapper: it hands the core's save
    /// the port and the folder the config names, and the dump lands under the name
    /// the restore picks.
    #[test]
    fn the_wrapper_saves_through_the_core_and_lands_the_dump() {
        let dir = tempfile::tempdir().expect("tempdir");
        let list = serde_json::json!({
            "object": "list",
            "data": [
                {"id": "Qwen3.8-27B", "status": {"value": "loaded"}},
                {"id": "Qwen3.8-Flash-Next", "status": {"value": "loaded"}}
            ]
        })
        .to_string();
        let (port, router) = saving_router(dir.path(), &list, 2);
        let cfg = crate::server_cfg::ServerConfig {
            port: Some(i32::from(port)),
            state_dir: Some(dir.path().display().to_string()),
            ..Default::default()
        };

        let (done, errors) = save_all(&cfg);
        assert_eq!(
            errors,
            Vec::<String>::new(),
            "the save reported: {errors:?}"
        );
        let models_saved: Vec<String> = done.iter().map(|t| t.model.clone()).collect();
        assert_eq!(
            models_saved,
            vec!["Qwen3.8-27B", "Qwen3.8-Flash-Next"],
            "one transfer per loaded model, with its counts"
        );
        assert_eq!(done[0].tokens, 166_000, "n_saved counts tokens");
        assert_eq!(done[0].bytes, 1_024, "n_written counts bytes");

        let seen = router.join().expect("the router ended");
        let (lines, bodies): (Vec<String>, Vec<String>) = seen.into_iter().unzip();
        assert_eq!(
            lines,
            vec![
                "GET /models HTTP/1.1",
                "POST /slots/0?action=save HTTP/1.1",
                "POST /slots/0?action=save HTTP/1.1",
            ],
            "the loaded models of the config's port, slot 0, the save action"
        );
        let asked: Vec<String> = bodies
            .iter()
            .map(|b| requested_filename(b).unwrap_or_default())
            .collect();
        assert_eq!(
            asked,
            vec![
                "",
                "Qwen3.8-27B.llamastate-partial",
                "Qwen3.8-Flash-Next.llamastate-partial",
            ],
            "the save asks for the partial name, so a save cut short leaves the previous dump"
        );

        assert_eq!(
            names_in(dir.path()),
            vec![
                state_file_name("Qwen3.8-27B"),
                state_file_name("Qwen3.8-Flash-Next")
            ],
            "the partial is renamed to the dump's name, nothing is left behind"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join(state_file_name("Qwen3.8-27B")))
                .expect("the dump the save wrote"),
            "the bytes llama-server wrote",
            "the file that lands is the one the server wrote"
        );
    }

    /// The save leg lives in the core, so the instance's own stop (the Linux
    /// unit's `ExecStop=`, the Windows service's stop) reaches the same code as
    /// the clients, and the wrapper is what carries the design's one budget to
    /// it: the core owns that constant, so the stop and the client cannot end
    /// up with two different deadlines.
    #[test]
    fn the_save_leg_delegates_to_the_core() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/slot_state.rs"),
        )
        .expect("read slot_state.rs");
        // Only the production half: the e2e phase and this module come after it.
        let code = src.split("\n#[cfg(test)]\n").next().expect("the code half");
        assert!(
            code.contains("slot_save::save_all"),
            "the wrapper must call the core's save: this file is where the GUI and the CLI ask for a snapshot"
        );
        assert!(
            code.contains("slot_save::SAVE_BUDGET"),
            "the wrapper hands over the core's budget, one deadline for every request of the run"
        );
        assert!(
            !code.contains(concat!("from_secs(", "240)")),
            "the budget is the core's constant, not seconds copied here"
        );
        let posts_a_save = regex::Regex::new(r#"slot_action\(\s*port,\s*"save""#).expect("regex");
        assert!(
            !posts_a_save.is_match(code),
            "the save request is the core's now: only the restore leg posts a slot action here"
        );
    }

    /// A save cut short leaves `<stem>.llamastate-partial` on disk. The restore
    /// takes the newest dump, so the partial must never be a candidate.
    #[test]
    fn a_partial_is_never_a_restore_candidate() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(state_file_name("Qwen3.8-27B")), b"dump").expect("dump");
        std::fs::write(
            dir.path().join("Qwen3.8-Flash-Next.llamastate-partial"),
            b"half written",
        )
        .expect("partial");

        assert_eq!(
            dumps_newest_first(dir.path()),
            vec![dir.path().join(state_file_name("Qwen3.8-27B"))],
            "the restore reads the newest complete dump, never a half-written one"
        );
    }
}
