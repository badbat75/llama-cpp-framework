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

/// E2E phase, an isolated test of `src/tests/mod.rs`: it redirects the data
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
#[path = "slot_state_tests.rs"]
mod tests;
