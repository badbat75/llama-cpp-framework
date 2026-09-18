//! Probes / starts / stops the llama-server process, and reconstructs its launch
//! command from server.ini. `server_args()` is the single source of truth for the
//! arg list, shared by `start()` (spawns the process) and `command_line()` (the
//! human-readable, shell-pasteable rendering shown in the Server tab's Command
//! Line card). `start()` additionally sets cwd = `paths::data_root()`,
//! `LLAMA_CACHE` = ModelsDir and the ROCm PATH prepend
//! (`proc::prepend_rocm_path`: HIP devices vanish without it), and appends both
//! output streams to `logs\llama-server.log`; env/cwd/logging are NOT part of
//! `command_line()`'s pasteable rendering, so a pasted command reproduces the
//! args only.
//!
//! The log is closed when a run ends and it has grown past a threshold.
//! `stop()` closes it once the process is gone: the file is renamed to
//! `llama-server-<yyyymmdd-hhmmss>.log` (the closing instant, UTC like every
//! other stamp in this crate; see `bench::stamp`) and an empty
//! `llama-server.log` takes its place, so the Log window, which resets on a
//! file shorter than its offset, reads as blank right after the stop and the
//! next start writes into a clean file. A run that did not end through
//! `stop()` (a crash, a TDR, an external kill, a stop whose rename lost to a
//! handle still closing) leaves its log behind, and `start()` closes THAT one
//! with the file's mtime for a stamp before opening the new one, which is the
//! last write of the dead run and so the same closing instant, recovered
//! rather than recorded. Both go through `close_log_per_settings`, which reads
//! settings.ini (`LogRotate`, on by default, and `LogRotateKb`, 1024 by
//! default: a log at or under the threshold is left for the next run to append
//! to, since a file per short run is clutter, not history); the newest
//! `ROTATED_LOGS_KEPT` closed logs are kept, older ones deleted.

use std::io;

// ── Run detection (is_running + tasklist) ────────────────────────────────

/// `true` if an `llama-server` process is currently running.
pub fn is_running() -> bool {
    #[cfg(windows)]
    {
        let Some(output) = crate::proc::run_hidden(
            std::path::Path::new("tasklist"),
            ["/fi", "IMAGENAME eq llama-server.exe", "/fo", "csv", "/nh"],
        ) else {
            return false;
        };
        running_from_tasklist(
            &String::from_utf8_lossy(&output.stdout),
            &crate::proc::probe_pids(),
        )
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new("pgrep")
            .args(["-f", "llama-server"])
            .output()
            .is_ok_and(|o| o.status.success())
    }
}

/// The PID (2nd CSV field) of a `tasklist /fo csv /nh` row like
/// `"llama-server.exe","1234","Console","1","12,345 K"`. Splitting on `,` is
/// safe for field 2: the only comma-bearing field (Mem Usage) comes after it.
#[cfg(windows)]
fn parse_tasklist_pid(line: &str) -> Option<u32> {
    line.split(',')
        .nth(1)?
        .trim()
        .trim_matches('"')
        .parse()
        .ok()
}

/// `true` if the tasklist output names a live `llama-server.exe` whose PID is
/// NOT one of our own transient probes. tasklist prints "INFO: No tasks..." when
/// nothing matches (so no row names the image). A row whose PID won't parse is
/// counted as a real server (safer to over-report than to miss a live one).
#[cfg(windows)]
fn running_from_tasklist(stdout: &str, probe_pids: &[u32]) -> bool {
    stdout.lines().any(|line| {
        line.to_lowercase().contains("llama-server.exe")
            && parse_tasklist_pid(line).is_none_or(|pid| !probe_pids.contains(&pid))
    })
}

/// `true` if the presets file has at least one section (reusing the real INI
/// parser instead of a hand-rolled header scan).
fn has_presets() -> bool {
    !crate::ini::read_all(&crate::paths::presets_ini()).is_empty()
}

// ── Launch argument assembly (server_args) ───────────────────────────────

/// The full llama-server argument list derived from server.ini.
/// Single source of truth for both `start()` and `command_line()`.
fn server_args(
    cfg: &crate::server_cfg::ServerConfig,
    presets_path: &std::path::Path,
) -> Vec<String> {
    let hostname = cfg.hostname_or_default();

    let mut args: Vec<String> = vec![
        "--models-preset".into(),
        presets_path.to_string_lossy().into_owned(),
    ];

    // --port / --models-max are omitted when unset (the UI's "default" checkbox
    // stores None). Forcing a value here is exactly what made "default" still
    // emit `--port 8080` / `--models-max 1`; omitting the flag lets llama.cpp
    // apply its own default (port 8080; 4 resident models). A 0 port is never
    // valid so it counts as unset, but models-max 0 (= unlimited) IS a real
    // value and is passed through.
    if let Some(p) = cfg.port.filter(|&n| n > 0) {
        args.push("--port".into());
        args.push(p.to_string());
    }
    if let Some(mm) = cfg.models_max {
        args.push("--models-max".into());
        args.push(mm.to_string());
    }

    args.push("--host".into());
    args.push(hostname);

    // --webui-mcp-proxy : serve the built-in web UI's MCP proxy endpoint. A
    //   presence flag, omitted when off (llama.cpp then defaults it disabled).
    // -fit on|off       : llama.cpp's auto-fit-to-VRAM. Always passed with an
    //   explicit value; defaults off because the GUI's "default" n-gpu-layers
    //   means "offload ALL layers", which -fit on would silently override.
    // Both were fixed framework policy; the Server tab's Advanced card now
    // exposes them (they still keep those framework defaults when untouched).
    if cfg.webui_mcp_proxy_or_default() {
        args.push("--webui-mcp-proxy".into());
    }
    args.push("-fit".into());
    args.push(if cfg.fit_or_default() {
        "on".into()
    } else {
        "off".into()
    });

    // --no-prefill-assistant : only when turned OFF. This is a NEGATIVE presence
    //   flag: llama.cpp prefills a trailing assistant message by default, and
    //   `--prefill-assistant` is merely the (redundant) affirmative, so passing
    //   nothing is what keeps the default behaviour.
    if !cfg.prefill_assistant_or_default() {
        args.push("--no-prefill-assistant".into());
    }

    // -lv N : log verbosity threshold into the captured llama-server.log.
    //   Framework default 4 (per-request logging) when unset; always passed
    //   (the Server tab's Advanced card exposes the level).
    args.push("-lv".into());
    args.push(cfg.log_verbosity_or_default().to_string());

    // CPU thread counts: when unset ("auto") omit the flag entirely so llama.cpp
    // applies its own default; only pass an explicit value when the user set one.
    // (We must NOT substitute a computed default here; that would defeat "auto".)
    if let Some(t) = cfg.threads.filter(|&n| n > 0) {
        args.push("-t".into());
        args.push(t.to_string());
    }
    if let Some(tb) = cfg.threads_batch.filter(|&n| n > 0) {
        args.push("--threads-batch".into());
        args.push(tb.to_string());
    }

    // -lm MODE : how the weights are brought in. Always passed with an explicit
    //   value (framework default "auto", which is llama.cpp's own), the same way
    //   -fit and -lv are; this is what the Command Line card has to show, and
    //   `load_mode_or_default` guarantees the value is one llama.cpp accepts.
    //
    //   NEVER emit the old --mlock / --no-mmap here. Deprecated in b10105, where
    //   they stopped composing (each overwrote the whole mode, so the pair this
    //   used to send, `--mlock --no-mmap`, was last-one-wins and dropped the
    //   mlock, while a lone `--mlock` also turned mmap OFF), and REMOVED in
    //   v0.4.1 (#28334), where any of them fails the parse and nothing starts;
    //   see `server_cfg::load_mode`.
    args.push("-lm".into());
    args.push(cfg.load_mode_or_default().into());
    if let Some(cr) = cfg.cache_reuse {
        if cr > 0 {
            args.push("--cache-reuse".into());
            args.push(cr.to_string());
        }
    }
    if let Some(dev) = cfg.device.as_deref().map(str::trim) {
        if !dev.is_empty() {
            args.push("--device".into());
            args.push(dev.to_string());
        }
    }
    if let Some(sm) = cfg.split_mode.as_deref().map(str::trim) {
        if !sm.is_empty() {
            args.push("--split-mode".into());
            args.push(sm.to_string());
        }
    }
    if let Some(ts) = cfg.tensor_split.as_deref().map(str::trim) {
        if !ts.is_empty() {
            args.push("--tensor-split".into());
            args.push(ts.to_string());
        }
    }
    // One flag carrying every rule, comma-joined: llama.cpp splits it itself. It
    // lands on the ROUTER's command line, which is exactly why it wins: the router
    // merges its own args into each preset as a key→value map, so this REPLACES a
    // preset's `override-tensor` rather than adding to it (see server_cfg).
    if let Some(ot) = cfg.override_tensor.as_deref().map(str::trim) {
        if !ot.is_empty() {
            args.push("--override-tensor".into());
            args.push(ot.to_string());
        }
    }
    // --slot-save-path DIR : turns on the /slots save|restore endpoints, which is
    //   what `slot_state` calls around a stop and a start. Emitted only when the
    //   feature is on, because the flag is not free: llama.cpp validates the
    //   directory while PARSING it and throws "not a directory" on a missing one,
    //   so an always-on flag would turn a deleted folder into a server that
    //   refuses to start. `start()` creates the directory first for the same
    //   reason.
    //
    //   It reaches the per-model CHILDREN even though it is the ROUTER's flag,
    //   and by an unusual route: the option has no env name, so it cannot ride a
    //   preset key, but `get_map_key_opt` (common/preset.cpp) also maps every
    //   option by its flag name with the dashes stripped. The router therefore
    //   parses its own argv into `base_preset` and merges that onto every model
    //   (`preset.merge(base_preset)`), and `unset_reserved_args` does not prune
    //   it. See the slot_state module header.
    if cfg.save_state_on_shutdown_or_default() {
        args.push("--slot-save-path".into());
        args.push(cfg.state_dir_or_default());
    }
    args
}

/// The environment llama-server is launched with, beyond the inherited one and
/// the ROCm PATH prepend (`proc::prepend_rocm_path`). LLAMA_CACHE is set
/// separately in `start()`: it comes from a path helper, not from a bare config
/// field. This is the sibling of `server_args` for the settings llama.cpp exposes
/// ONLY as env vars, so `start()` and `command_line()` render the same launch.
pub fn env_vars(cfg: &crate::server_cfg::ServerConfig) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();
    if let Some(dev) = cfg.mmproj_device.as_deref().map(str::trim) {
        if !dev.is_empty() {
            env.push(("MTMD_BACKEND_DEVICE".into(), dev.to_string()));
        }
    }
    // ROCBLAS_USE_HIPBLASLT: rocBLAS's switch between its two GEMM backends. Read
    // by rocBLAS itself (llama.cpp has no flag for it and never sees it) and
    // parsed as an INT, so the value is "1"/"0", never "true"/"false". `0` forces
    // Tensile, which is the cure for gfx1201's hipBLASLt failing the second
    // 16-bit GEMM of a process (see server_cfg::rocblas_use_hipblaslt). Unset
    // exports nothing at all, which is a third state and the default.
    if let Some(on) = cfg.rocblas_use_hipblaslt {
        env.push((
            "ROCBLAS_USE_HIPBLASLT".into(),
            if on { "1" } else { "0" }.into(),
        ));
    }
    env
}

/// How long `start()` watches a freshly spawned llama-server before declaring
/// success: long enough to catch a launch that dies on a bad preset, a taken
/// port, or a missing model (those exit within ~1 s), short enough that a
/// healthy server (still alive while it loads the model) isn't held up.
const LAUNCH_GRACE: std::time::Duration = std::time::Duration::from_millis(2500);

// ── Process control (start / stop) ───────────────────────────────────────

/// Launch llama-server.exe with args from server.ini + presets.ini.
///
/// Returns `Some(cfg)` (the config the process was ACTUALLY launched with)
/// so the caller can snapshot the client URL from it (re-loading server.ini
/// after the fact would race a save landing between the two reads). `Ok(None)`
/// means the server was already running: nothing was launched, so there is no
/// launch config to snapshot; the live process may be on an older saved
/// config, or not GUI-launched at all.
///
/// After spawning, it watches the child for `LAUNCH_GRACE`: a process that
/// exits in that window (bad preset, port already bound, model load failure)
/// returns an `Err` pointing at the log, so the caller reports the failure NOW
/// instead of an optimistic "started" that the 5 s status tick later contradicts
/// with "no longer running". A process still alive after the window is reported
/// as started (it may still be loading the model; that's fine, it's up).
pub fn start() -> io::Result<Option<crate::server_cfg::ServerConfig>> {
    if is_running() {
        return Ok(None); // already running; we launched nothing
    }

    if !has_presets() {
        // Two different cures: the switched-off presets are still there, in the
        // file llama-server does not read.
        let msg = if crate::presets::load_listed().is_empty() {
            "No model presets configured: add one on the Models page first."
        } else {
            "Every model preset is switched off: switch one on in the Models page first."
        };
        return Err(io::Error::new(io::ErrorKind::NotFound, msg));
    }

    let exe = crate::paths::llama_server_exe().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "llama-server.exe not found. Build llama.cpp first.",
        )
    })?;

    let cfg = crate::server_cfg::load();
    let presets_path = crate::paths::presets_ini();

    let models_dir = cfg.models_dir_or_default();

    let data_root = crate::paths::data_root();
    let log_path = crate::paths::server_log();
    if let Some(log_dir) = log_path.parent() {
        std::fs::create_dir_all(log_dir)?;
    }

    // A run that ended without `stop()` (crash, external kill) left its log
    // here; close it under its own last-write stamp so this run starts clean.
    // Best effort: a rename that fails just leaves the old run above the new
    // one, which is what every start did before rotation existed.
    close_log_per_settings(LogStamp::LastWrite);

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    // The snapshot directory must exist BEFORE the spawn, not at the first save:
    // llama.cpp validates `--slot-save-path` inside the argument handler
    // (`fs_is_directory` or it throws "not a directory"), so a missing directory
    // does not degrade to "no snapshots this run", it makes llama-server exit
    // during arg parsing and the launch fails with nothing but a log line.
    if cfg.save_state_on_shutdown_or_default() {
        crate::slot_state::ensure_dir(&cfg)?;
    }

    let mut cmd = std::process::Command::new(&exe);
    cmd.args(server_args(&cfg, &presets_path));

    cmd.current_dir(&data_root);
    cmd.env("LLAMA_CACHE", &models_dir);
    // Put the image encoder (mmproj/CLIP) on a chosen GPU. MTMD_BACKEND_DEVICE
    // is the env name of `-mmdev` / `--mmproj-device` (v0.2.0; `clip_ctx` read
    // the variable itself before that), and unset it follows the first entry of
    // --device since v0.4.1, or, with no device pinned, takes the FIRST GPU
    // backend the registry offers, which up to v0.4.0 it did regardless. On a
    // mixed box that strands the encoder on the wrong card, where it holds VRAM
    // for the model's whole life and computes only when an image arrives.
    // Inherited by the router's per-model children, so it covers every preset.
    for dev in env_vars(&cfg) {
        cmd.env(dev.0, dev.1);
    }
    // Make ggml-hip.dll loadable: the HIP SDK's bin dir isn't on the system
    // PATH, and ggml silently skips a backend whose DLL deps don't resolve.
    crate::proc::prepend_rocm_path(&mut cmd);
    // llama-server writes most of its logging (model load, request logs,
    // GGML asserts, crash traces) to stderr; capture both streams in the
    // same log file so the log isn't empty and crashes leave a trail.
    let log_file_err = log_file.try_clone()?;
    cmd.stdout(log_file);
    cmd.stderr(log_file_err);

    // No console flash: same CREATE_NO_WINDOW as the fire-and-forget probes, but
    // applied to a Command we spawn with custom stdio/env (so not run_hidden).
    crate::proc::hide_console(&mut cmd);

    let mut child = cmd.spawn()?;

    // Confirm the server SURVIVES launch: spawning only proves the exe started,
    // not that it got past arg parsing / port bind / model load. Poll for a
    // short grace window; if it exits, surface that immediately with the log
    // path (llama-server writes its own error trail there). Runs on the caller's
    // worker thread (`start_server_async`), so the brief blocking wait keeps the
    // UI in "Starting…" rather than stalling it.
    let step = std::time::Duration::from_millis(150);
    let mut waited = std::time::Duration::ZERO;
    while waited < LAUNCH_GRACE {
        match child.try_wait() {
            // Exited during the grace window → a failed launch, not a running server.
            Ok(Some(status)) => {
                return Err(io::Error::other(format!(
                    "llama-server exited on launch ({status}). Check the log: {}",
                    log_path.display()
                )));
            }
            // Still alive: keep it running and stop watching.
            Ok(None) => {}
            // Can't poll the child; don't block the launch on a probe failure.
            Err(_) => break,
        }
        std::thread::sleep(step);
        waited += step;
    }

    // Dropping `child` here does NOT kill the process on Windows (std leaves it
    // detached): the server keeps running; we just stop watching it.
    Ok(Some(cfg))
}

/// Force-kill all llama-server.exe processes (taskkill /f: llama-server has
/// no graceful shutdown channel when running detached without a console), then
/// close the run's log (module header) once the process is gone.
///
/// Infallible by design: a missing/failed kill is not surfaced here. The caller
/// (`stop_server_async`) re-polls `is_running()` and reports "still running" if
/// the kill didn't land; that re-check is the source of truth for the outcome.
/// The log is closed only after `is_running()` reads false, waited for up to
/// `STOP_GRACE`: renaming the file while the process still holds it as stdout
/// would carry its last lines into the closed log and, on a kill that never
/// lands, would file a live run under a closing stamp. A wait that runs out
/// skips the close; the next `start()` picks the file up by its mtime.
pub fn stop() {
    #[cfg(windows)]
    {
        // Spawn failure (taskkill missing) is effectively impossible on Windows;
        // if the kill doesn't land, the caller's re-check of the run state
        // surfaces it as "still running".
        crate::proc::run_hidden(
            std::path::Path::new("taskkill"),
            ["/f", "/im", "llama-server.exe"],
        );
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("pkill")
            .arg("-f")
            .arg("llama-server")
            .output();
    }

    let step = std::time::Duration::from_millis(250);
    let mut waited = std::time::Duration::ZERO;
    while is_running() {
        if waited >= STOP_GRACE {
            return;
        }
        std::thread::sleep(step);
        waited += step;
    }
    close_log_per_settings(LogStamp::Now);
}

/// How long `stop()` waits for the killed process to disappear before giving
/// up on closing the log. `taskkill /f` is synchronous in practice (the GUI's
/// own 15 s re-poll almost never loops), so this is a ceiling, not a delay.
const STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

// ── Log rotation ─────────────────────────────────────────────────────────

/// Closed logs kept beside `llama-server.log`; the oldest beyond this are
/// deleted by `close_log`. At `-lv 4` a long session writes tens of MB, and
/// before rotation the single file grew without bound, so a cap is the
/// conservative side of this change, not the aggressive one.
pub const ROTATED_LOGS_KEPT: usize = 10;

/// Which instant names a closed log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogStamp {
    /// The clock at the call: `stop()`, which knows the run just ended.
    Now,
    /// The file's own mtime: `start()`, closing a run that ended without a
    /// `stop()`, whose last write is the best record of when.
    LastWrite,
}

/// `close_log` on the live log under the user's settings: nothing when
/// `LogRotate` is off, otherwise with `LogRotateKb` as the threshold. Best
/// effort by design (its two callers are `stop()`, which cannot fail, and
/// `start()`, where a log left in place is exactly what every start did before
/// rotation existed): a rename that fails is retried by the next start, since
/// the file is still there and still over the threshold.
fn close_log_per_settings(stamp: LogStamp) {
    if let Some(min_bytes) = crate::settings::load().log_rotate_threshold() {
        let _ = close_log(&crate::paths::server_log(), stamp, min_bytes);
    }
}

/// Close the run held in `log_path` if it is LARGER than `min_bytes`: rename
/// it to `llama-server-<stamp>.log` beside itself, leave an EMPTY
/// `llama-server.log` in its place, and prune the closed logs down to
/// `ROTATED_LOGS_KEPT`. Returns the closed file's path, or `None` when there
/// was nothing to close: no file, one at or under the threshold (left for the
/// next run to append to), or an empty one whatever the threshold (the file
/// this function itself leaves behind, so a stop after a failed launch and a
/// start after a stop are both no-ops rather than a growing pile of empties).
///
/// A stamp collision (two closes within the same second, e.g. a `bench sweep`
/// leg that dies at once) gets a `-2`, `-3`... suffix rather than overwriting
/// the earlier run. The empty replacement is created here and not left to
/// `start()` so the Log window, which shows a "not found" placeholder for a
/// missing file, reads as blank after a stop instead of claiming the log has
/// never existed.
pub fn close_log(
    log_path: &std::path::Path,
    stamp: LogStamp,
    min_bytes: u64,
) -> io::Result<Option<std::path::PathBuf>> {
    let meta = match std::fs::metadata(log_path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if meta.len() == 0 || meta.len() <= min_bytes {
        return Ok(None);
    }
    let secs = match stamp {
        LogStamp::Now => crate::bench::now_secs(),
        LogStamp::LastWrite => meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or_else(crate::bench::now_secs),
    };
    let dir = log_path.parent().unwrap_or(std::path::Path::new("."));
    let stem = closed_log_stem(log_path);
    let base = format!("{stem}-{}", crate::bench::stamp(secs));
    let mut target = dir.join(format!("{base}.log"));
    let mut n = 1;
    while target.exists() {
        n += 1;
        target = dir.join(format!("{base}-{n}.log"));
    }
    std::fs::rename(log_path, &target)?;
    // The blank successor. Failing to create it is not a failed close: the
    // run IS filed, and `start()` creates the file anyway.
    let _ = std::fs::File::create(log_path);
    prune_closed_logs(dir, &stem, ROTATED_LOGS_KEPT);
    Ok(Some(target))
}

/// `llama-server` for `llama-server.log`: the prefix closed logs share.
fn closed_log_stem(log_path: &std::path::Path) -> String {
    log_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "llama-server".into())
}

/// Delete all but the newest `keep` closed logs (`<stem>-*.log`) in `dir`.
/// The stamp sorts lexically as it sorts chronologically (`bench::stamp`), so
/// the newest is simply the greatest name; the live `<stem>.log` carries no
/// dash after the stem and is never a candidate. Best effort throughout: a
/// file that will not delete stays, which costs disk, not correctness.
fn prune_closed_logs(dir: &std::path::Path, stem: &str, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let prefix = format!("{stem}-");
    let mut closed: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (name.starts_with(&prefix) && name.ends_with(".log")).then_some(name)
        })
        .collect();
    closed.sort();
    let excess = closed.len().saturating_sub(keep);
    for name in closed.into_iter().take(excess) {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

// ── Command-line rendering ───────────────────────────────────────────────

/// Shell line-continuation character: PowerShell uses a backtick, POSIX shells
/// use a backslash. Only used to pretty-print `command_line()` for the UI.
#[cfg(windows)]
const LINE_CONTINUATION: char = '`';
#[cfg(not(windows))]
const LINE_CONTINUATION: char = '\\';

/// Returns the command line `start()` would launch, reconstructed from
/// server.ini (the same deterministic args). Formatted for readability: the
/// executable and each `--flag [value]` group sit on their own line, joined
/// with the shell's line-continuation character (`` ` `` on Windows, `\` on
/// POSIX) so the whole block can be pasted straight into a terminal.
pub fn command_line() -> Option<String> {
    let cfg = crate::server_cfg::load();
    let exe = crate::paths::llama_server_exe()?;
    let presets_path = crate::paths::presets_ini();
    Some(render_command_line(
        &exe.to_string_lossy(),
        &server_args(&cfg, &presets_path),
        &env_vars(&cfg),
    ))
}

/// Group the flat arg list into `--flag [value...]` lines: a token that starts
/// with '-' opens a new line; following non-flag tokens (values) attach to the
/// current line. Pure (no IO): `command_line()` is the config-loading wrapper,
/// mirroring the `render`/`save` split in server_cfg.
///
/// `env` is prepended as standalone assignment lines (NOT continued into the
/// command): a setting llama.cpp only exposes as an env var is still part of the
/// launch, and a pasted block that silently dropped it would run a different
/// server than the GUI does.
pub fn render_command_line(exe: &str, args: &[String], env: &[(String, String)]) -> String {
    // PowerShell parses a quoted string at command position as an expression,
    // not a command, and the default install path ("C:\Program Files\…") gets
    // quoted by `quote_arg`. The call operator makes the paste work there and
    // is harmless for unquoted paths.
    #[cfg(windows)]
    let exe_line = format!("& {}", quote_arg(exe));
    #[cfg(not(windows))]
    let exe_line = quote_arg(exe);
    let mut out = String::new();
    for (k, v) in env {
        #[cfg(windows)]
        out.push_str(&format!("$env:{k} = \"{v}\"\n"));
        #[cfg(not(windows))]
        out.push_str(&format!("export {k}=\"{v}\"\n"));
    }
    let mut lines: Vec<String> = vec![exe_line];
    for arg in args {
        let q = quote_arg(arg);
        if arg.starts_with('-') {
            lines.push(q);
        } else if let Some(last) = lines.last_mut() {
            last.push(' ');
            last.push_str(&q);
        }
    }

    let joiner = format!(" {LINE_CONTINUATION}\n  ");
    out.push_str(&lines.join(&joiner));
    out
}

fn quote_arg(s: &str) -> String {
    if s.contains(char::is_whitespace) {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_cfg::ServerConfig;
    use std::path::Path;

    fn args_for(cfg: &ServerConfig) -> Vec<String> {
        server_args(cfg, Path::new("presets.ini"))
    }

    // ── Log rotation ─────────────────────────────────────────────────────

    fn closed_logs(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("llama-server-"))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn close_log_files_the_run_under_a_stamp_and_leaves_a_blank_successor() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        std::fs::write(&log, "run one\n").unwrap();

        let closed = close_log(&log, LogStamp::Now, 0).unwrap().expect("a run to close");
        let name = closed.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("llama-server-")
                && name.ends_with(".log")
                && name.len() == "llama-server-yyyymmdd-hhmmss.log".len(),
            "llama-server-yyyymmdd-hhmmss.log, got {name}"
        );
        assert_eq!(std::fs::read_to_string(&closed).unwrap(), "run one\n");
        // The live path is back, empty: the Log window resets on it and the
        // next start appends to a clean file.
        assert_eq!(std::fs::metadata(&log).unwrap().len(), 0);
    }

    /// The threshold is "larger than", in bytes: a log exactly at it stays.
    #[test]
    fn close_log_leaves_a_log_at_or_under_the_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        std::fs::write(&log, "x".repeat(1024)).unwrap();
        assert_eq!(close_log(&log, LogStamp::Now, 1024).unwrap(), None);
        assert_eq!(std::fs::metadata(&log).unwrap().len(), 1024, "left in place");
        assert!(closed_logs(dir.path()).is_empty());
        std::fs::write(&log, "x".repeat(1025)).unwrap();
        assert!(close_log(&log, LogStamp::Now, 1024).unwrap().is_some());
        assert_eq!(std::fs::metadata(&log).unwrap().len(), 0);
    }

    #[test]
    fn close_log_is_a_no_op_on_a_missing_or_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        assert_eq!(close_log(&log, LogStamp::Now, 0).unwrap(), None);
        assert!(!log.exists(), "nothing to close creates nothing");

        std::fs::write(&log, "").unwrap();
        assert_eq!(close_log(&log, LogStamp::LastWrite, 0).unwrap(), None);
        assert!(closed_logs(dir.path()).is_empty(), "an empty log is never filed");
    }

    #[test]
    fn close_log_keeps_both_runs_on_a_same_second_collision() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        std::fs::write(&log, "first\n").unwrap();
        let a = close_log(&log, LogStamp::Now, 0).unwrap().unwrap();
        std::fs::write(&log, "second\n").unwrap();
        let b = close_log(&log, LogStamp::Now, 0).unwrap().unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "first\n");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "second\n");
        // Same second, or the clock ticked over between the two closes; either
        // way both stamps sort after the live file and before nothing.
        assert_eq!(closed_logs(dir.path()).len(), 2);
    }

    #[test]
    fn close_log_last_write_stamps_from_the_file_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        std::fs::write(&log, "crashed run\n").unwrap();
        // Push the mtime a day into the past: the stamp must follow the FILE,
        // not the clock, or a log found at start would be filed under the
        // start time rather than the run's end.
        let past = std::time::SystemTime::now() - std::time::Duration::from_secs(86_400);
        let f = std::fs::OpenOptions::new().write(true).open(&log).unwrap();
        f.set_modified(past).unwrap();
        drop(f);
        let expected = crate::bench::stamp(
            past.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        );
        let closed = close_log(&log, LogStamp::LastWrite, 0).unwrap().unwrap();
        assert_eq!(
            closed.file_name().unwrap().to_string_lossy(),
            format!("llama-server-{expected}.log")
        );
    }

    #[test]
    fn prune_keeps_the_newest_closed_logs_and_never_the_live_one() {
        let dir = tempfile::tempdir().unwrap();
        for stamp in ["20260101-000000", "20260102-000000", "20260103-000000"] {
            std::fs::write(dir.path().join(format!("llama-server-{stamp}.log")), "x").unwrap();
        }
        std::fs::write(dir.path().join("llama-server.log"), "live").unwrap();
        std::fs::write(dir.path().join("other.log"), "not ours").unwrap();
        prune_closed_logs(dir.path(), "llama-server", 2);
        assert_eq!(
            closed_logs(dir.path()),
            vec![
                "llama-server-20260102-000000.log".to_string(),
                "llama-server-20260103-000000.log".to_string(),
            ]
        );
        assert!(dir.path().join("llama-server.log").exists());
        assert!(dir.path().join("other.log").exists());
    }

    #[test]
    fn close_log_prunes_past_the_retention_cap() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..ROTATED_LOGS_KEPT {
            std::fs::write(
                dir.path().join(format!("llama-server-20260101-{i:06}.log")),
                "x",
            )
            .unwrap();
        }
        let log = dir.path().join("llama-server.log");
        std::fs::write(&log, "one more\n").unwrap();
        close_log(&log, LogStamp::Now, 0).unwrap().unwrap();
        let kept = closed_logs(dir.path());
        assert_eq!(kept.len(), ROTATED_LOGS_KEPT);
        assert!(!kept.contains(&"llama-server-20260101-000000.log".to_string()), "oldest goes");
    }

    #[cfg(windows)]
    #[test]
    fn tasklist_pid_parses_the_second_csv_field() {
        assert_eq!(
            parse_tasklist_pid(r#""llama-server.exe","1234","Console","1","12,345 K""#),
            Some(1234)
        );
        assert_eq!(parse_tasklist_pid("INFO: No tasks are running ..."), None);
    }

    // The startup false-positive fix: a --version / --list-devices probe shares
    // the llama-server.exe image name, so is_running must exclude its PID or a
    // fresh GUI flips to "no longer running" when the probe exits.
    #[cfg(windows)]
    #[test]
    fn running_excludes_our_own_probe_pids() {
        let none = "INFO: No tasks are running which match the specified criteria.";
        assert!(!running_from_tasklist(none, &[]));

        let one = r#""llama-server.exe","1234","Console","1","12,345 K""#;
        assert!(
            running_from_tasklist(one, &[]),
            "a real server reads as running"
        );
        assert!(
            !running_from_tasklist(one, &[1234]),
            "the same PID, if it's our probe, must not"
        );

        let two = "\"llama-server.exe\",\"1234\",\"Console\",\"1\",\"10 K\"\n\
                   \"llama-server.exe\",\"5678\",\"Console\",\"1\",\"20 K\"";
        assert!(
            running_from_tasklist(two, &[1234]),
            "a real server alongside a live probe still counts"
        );
        assert!(
            !running_from_tasklist(two, &[1234, 5678]),
            "both are probes"
        );
    }

    // "auto" (unset / non-positive) MUST omit the flag entirely so llama.cpp
    // applies its own default; substituting a computed value would defeat it.
    #[test]
    fn auto_fields_omit_their_flags() {
        let a = args_for(&ServerConfig::default());
        // port / models-max join the unset-omits-the-flag group: the UI
        // "default" checkbox stores None, and a forced value here is what left
        // "default" still showing --port 8080 / --models-max 1.
        for flag in [
            "-t",
            "--threads-batch",
            "--cache-reuse",
            "--device",
            "--port",
            "--models-max",
        ] {
            assert!(!a.contains(&flag.to_string()), "{flag} must be omitted");
        }
        // The deprecated pair must never reappear: each overwrites the whole
        // load mode in llama.cpp, so emitting them alongside -lm would silently
        // decide it (last one wins).
        for flag in ["--mlock", "--no-mmap", "--mmap", "-dio"] {
            assert!(!a.contains(&flag.to_string()), "{flag} is deprecated");
        }
        // Framework policy flags and always-written fields remain present.
        assert!(
            a.iter()
                .position(|x| x == "-lm")
                .is_some_and(|i| a.get(i + 1).is_some_and(|v| v == "auto")),
            "load mode defaults to auto"
        );
        assert!(
            a.contains(&"localhost".to_string()),
            "host is always written"
        );
        assert!(a.contains(&"--webui-mcp-proxy".to_string()));
    }

    #[test]
    fn explicit_values_emit_flag_value_pairs() {
        let cfg = ServerConfig {
            threads: Some(12),
            threads_batch: Some(24),
            cache_reuse: Some(256),
            load_mode: Some("mmap+mlock".into()),
            ..Default::default()
        };
        let a = args_for(&cfg);
        let pair = |flag: &str, val: &str| {
            a.iter()
                .position(|x| x == flag)
                .is_some_and(|i| a.get(i + 1).is_some_and(|v| v == val))
        };
        assert!(pair("-t", "12"));
        assert!(pair("--threads-batch", "24"));
        assert!(pair("--cache-reuse", "256"));
        assert!(pair("-lm", "mmap+mlock"));
    }

    // Step-8 guard of the server-field recipe (top of server_cfg.rs): every
    // ServerConfig field must be consumed by the launch path: mapped to a
    // llama-server flag here, or explicitly waved through with a comment
    // saying where `start()` uses it. The exhaustive destructure breaks
    // compilation the moment a field is added, until this test decides.
    #[test]
    fn server_args_covers_every_config_field() {
        let cfg = ServerConfig {
            port: Some(9090),
            hostname: Some("0.0.0.0".into()),
            load_mode: Some("mlock".into()),
            threads: Some(6),
            cache_reuse: Some(64),
            threads_batch: Some(12),
            models_max: Some(3),
            models_dir: Some(r"D:\models".into()),
            device: Some("ROCm1,CUDA0".into()),
            split_mode: Some("row".into()),
            tensor_split: Some("3,1".into()),
            // Two rules: the `,` that joins them must reach llama-server as ONE
            // argument (it does its own splitting), not as two.
            override_tensor: Some(r"token_embd\.weight=ROCm1,^output\.weight=CPU".into()),
            mmproj_device: Some("ROCm1".into()),
            rocblas_use_hipblaslt: Some(false),
            webui_mcp_proxy: Some(false),
            fit: Some(true),
            // The NEGATIVE presence flag: Some(false) is the state that emits one.
            prefill_assistant: Some(false),
            log_verbosity: Some(2),
            // On, with an EXPLICIT dir: resolving the default would reach into
            // `paths::`, which unit tests must not touch (src/tests/mod.rs).
            save_state_on_shutdown: Some(true),
            state_dir: Some(r"E:\llama-state".into()),
            opencode_base_url: Some("https://llm.example.com".into()),
            opencode_api_key: Some("sk-test-key".into()),
        };
        let ServerConfig {
            port,
            hostname,
            load_mode,
            threads,
            cache_reuse,
            threads_batch,
            models_max,
            models_dir: _, // launch env only: start() exports it as LLAMA_CACHE
            device,
            split_mode,
            tensor_split,
            override_tensor,
            // launch env only: start() exports it as MTMD_BACKEND_DEVICE, the
            // env name of `-mmdev`; the env reaches every router child with
            // nothing to merge, where the flag would ride the router's argv.
            mmproj_device: _,
            // launch env only: `env_vars` exports it as ROCBLAS_USE_HIPBLASLT.
            // Also not a llama-server flag: rocBLAS reads it itself, below
            // llama.cpp. Its own assertions live in the env test further down.
            rocblas_use_hipblaslt: _,
            webui_mcp_proxy,
            fit,
            prefill_assistant,
            log_verbosity,
            save_state_on_shutdown,
            state_dir,
            // Integration-only: not a llama-server flag. Used by opencode.json
            // and Claude Code snippet. The GUI edits it on the Server tab
            // (Network section).
            opencode_base_url: _,
            // Integration-only: not a llama-server flag. Written as apiKey in
            // opencode.json. The GUI edits it on the Server tab (Network section).
            opencode_api_key: _,
        } = cfg.clone();
        let a = args_for(&cfg);
        let pair = |flag: &str, val: String| {
            a.iter()
                .position(|x| x == flag)
                .is_some_and(|i| a.get(i + 1).is_some_and(|v| *v == val))
        };
        assert!(pair("--port", port.unwrap().to_string()));
        assert!(pair("--host", hostname.unwrap()));
        assert!(pair("-lm", load_mode.unwrap()));
        assert!(pair("-t", threads.unwrap().to_string()));
        assert!(pair("--cache-reuse", cache_reuse.unwrap().to_string()));
        assert!(pair("--threads-batch", threads_batch.unwrap().to_string()));
        assert!(pair("--models-max", models_max.unwrap().to_string()));
        assert!(pair("--device", device.unwrap()));
        assert!(pair("--split-mode", split_mode.unwrap()));
        assert!(pair("--tensor-split", tensor_split.unwrap()));
        assert!(pair("--override-tensor", override_tensor.unwrap()));
        // webui-mcp-proxy is a presence flag: Some(false) here ⇒ omitted.
        assert_eq!(
            a.contains(&"--webui-mcp-proxy".to_string()),
            webui_mcp_proxy.unwrap()
        );
        // fit is always passed with an explicit on|off value.
        assert!(pair("-fit", if fit.unwrap() { "on" } else { "off" }.into()));
        // prefill-assistant is a NEGATIVE presence flag: it exists to turn
        // llama.cpp's default OFF, so Some(false) emits it and Some(true) emits
        // nothing. Asserting the emitted case AND the silent one, because "no flag"
        // is the state a `!` typo would produce for both.
        assert_eq!(
            a.contains(&"--no-prefill-assistant".to_string()),
            !prefill_assistant.unwrap()
        );
        assert!(!args_for(&ServerConfig {
            prefill_assistant: Some(true),
            ..Default::default()
        })
        .contains(&"--no-prefill-assistant".to_string()));
        // log verbosity is always passed (framework default 4 when unset).
        assert!(pair("-lv", log_verbosity.unwrap().to_string()));
        // The two snapshot fields ride ONE flag: the toggle decides whether it
        // appears, the directory is its value.
        assert_eq!(save_state_on_shutdown, Some(true));
        assert!(pair("--slot-save-path", state_dir.unwrap()));
    }

    /// `--slot-save-path` must be ABSENT when snapshots are off, and that is not
    /// cosmetic: llama.cpp validates the directory while parsing the flag and
    /// throws "not a directory" on a missing one, so emitting it unconditionally
    /// would turn a deleted folder into a server that refuses to start.
    #[test]
    fn slot_save_path_is_omitted_when_snapshots_are_off() {
        let off = args_for(&ServerConfig {
            state_dir: Some(r"E:\llama-state".into()),
            ..Default::default()
        });
        assert!(
            !off.contains(&"--slot-save-path".to_string()),
            "a StateDir alone must not enable the endpoints"
        );

        let on = args_for(&ServerConfig {
            save_state_on_shutdown: Some(true),
            state_dir: Some(r"E:\llama-state".into()),
            ..Default::default()
        });
        assert!(on
            .iter()
            .position(|x| x == "--slot-save-path")
            .is_some_and(|i| on.get(i + 1).is_some_and(|v| v == r"E:\llama-state")));
    }

    // Unlike the thread/port fields (where <= 0 means "unset"), models-max 0 is
    // a real value (= unlimited) and must be passed, not mistaken for "default".
    #[test]
    fn models_max_zero_is_passed_as_unlimited() {
        let a = args_for(&ServerConfig {
            models_max: Some(0),
            ..Default::default()
        });
        let at = a.iter().position(|x| x == "--models-max");
        assert_eq!(
            at.and_then(|i| a.get(i + 1)).map(String::as_str),
            Some("0"),
            "models-max 0 (unlimited) is passed explicitly"
        );
    }

    #[test]
    fn nonpositive_overrides_are_treated_as_auto() {
        let cfg = ServerConfig {
            threads: Some(0),
            threads_batch: Some(-1),
            cache_reuse: Some(0),
            ..Default::default()
        };
        let a = args_for(&cfg);
        for flag in ["-t", "--threads-batch", "--cache-reuse"] {
            assert!(!a.contains(&flag.to_string()), "{flag} must be omitted");
        }
    }

    #[test]
    fn blank_strings_are_omitted_and_padded_ones_trimmed() {
        let cfg = ServerConfig {
            device: Some("  ".into()),
            split_mode: Some(" row ".into()),
            tensor_split: Some("3,1".into()),
            ..Default::default()
        };
        let a = args_for(&cfg);
        assert!(!a.contains(&"--device".to_string()));
        let i = a.iter().position(|x| x == "--split-mode").unwrap();
        assert_eq!(a[i + 1], "row");
        let i = a.iter().position(|x| x == "--tensor-split").unwrap();
        assert_eq!(a[i + 1], "3,1");
    }

    // The Command Line card's grouping: values share their flag's line, each
    // line joins with the platform continuation char so the block pastes
    // straight into a terminal.
    #[test]
    fn render_command_line_groups_flags_with_their_values() {
        // --webui-mcp-proxy is the valueless flag of the three: it must get a
        // line of its own rather than swallow the next flag as its value.
        let args: Vec<String> = ["--port", "8080", "--webui-mcp-proxy", "-fit", "off"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let out = render_command_line(r"C:\bin\llama-server.exe", &args, &[]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 4, "exe + one line per flag group");
        #[cfg(windows)]
        assert!(lines[0].starts_with(r"& C:\bin\llama-server.exe"));
        #[cfg(not(windows))]
        assert!(lines[0].starts_with(r"C:\bin\llama-server.exe"));
        assert!(lines[1].contains("--port 8080"), "value attaches to flag");
        assert!(lines[2].contains("--webui-mcp-proxy"));
        assert!(lines[3].trim_start().starts_with("-fit off"));
        // Every line but the last ends with the continuation char.
        for line in &lines[..lines.len() - 1] {
            assert!(line.ends_with(LINE_CONTINUATION), "bad tail: {line:?}");
        }
        assert!(!lines[lines.len() - 1].ends_with(LINE_CONTINUATION));
    }

    // The default NSIS install dir has a space ("C:\Program Files\llama.cpp"),
    // so the pasted line must survive PowerShell's expression-vs-command
    // parsing: a bare quoted path at command position is an error there.
    #[cfg(windows)]
    #[test]
    fn render_command_line_calls_a_quoted_exe_with_the_call_operator() {
        let out = render_command_line(
            r"C:\Program Files\llama.cpp\bin\llama-server.exe",
            &["--port".to_string(), "8080".to_string()],
            &[],
        );
        assert!(
            out.starts_with("& \"C:\\Program Files\\llama.cpp\\bin\\llama-server.exe\""),
            "bad exe line: {out:?}"
        );
    }

    // MmprojDevice rides the env (MTMD_BACKEND_DEVICE, the env name of `-mmdev`),
    // never the args. Both launch surfaces must carry it: `start()` sets it on
    // the child, and the pasted command line has to as well, or the block a user
    // copies out of the GUI runs a differently-placed encoder.
    #[test]
    fn mmproj_device_rides_the_env_not_the_args() {
        let cfg = ServerConfig {
            mmproj_device: Some("ROCm1".into()),
            ..Default::default()
        };
        assert!(!args_for(&cfg).iter().any(|a| a.contains("ROCm1")));
        assert_eq!(
            env_vars(&cfg),
            [("MTMD_BACKEND_DEVICE".to_string(), "ROCm1".to_string())]
        );
        assert!(env_vars(&ServerConfig::default()).is_empty());

        let out = render_command_line(r"C:\bin\llama-server.exe", &[], &env_vars(&cfg));
        assert!(
            out.contains("MTMD_BACKEND_DEVICE"),
            "no env line in:\n{out}"
        );
        assert!(out.contains("ROCm1"));
        // The env assignment is its own statement: continuing it into the
        // command would make the paste a syntax error.
        assert!(!out.lines().next().unwrap().ends_with(LINE_CONTINUATION));
    }

    // The SDK-tuning tri-state, on the same env-only path as MmprojDevice above.
    // Three things it pins: the value is the INT rocBLAS parses ("0"/"1", never
    // "false"/"true"), `None` exports NOTHING (a default-on-every-machine
    // workaround is exactly what the tri-state exists to avoid), and it reaches
    // the pasteable command line; a block that dropped it would run the backend
    // this setting exists to steer away from.
    #[test]
    fn rocblas_hipblaslt_rides_the_env_as_zero_or_one() {
        let with = |v: Option<bool>| ServerConfig {
            rocblas_use_hipblaslt: v,
            ..Default::default()
        };
        assert_eq!(
            env_vars(&with(Some(false))),
            [("ROCBLAS_USE_HIPBLASLT".to_string(), "0".to_string())]
        );
        assert_eq!(
            env_vars(&with(Some(true))),
            [("ROCBLAS_USE_HIPBLASLT".to_string(), "1".to_string())]
        );
        assert!(
            env_vars(&with(None)).is_empty(),
            "unset must export nothing"
        );
        // Not an argument on either surface: llama-server would refuse it.
        assert!(!args_for(&with(Some(false)))
            .iter()
            .any(|a| a.contains("HIPBLASLT")));

        let out = render_command_line(
            r"C:\bin\llama-server.exe",
            &[],
            &env_vars(&with(Some(false))),
        );
        assert!(
            out.contains("ROCBLAS_USE_HIPBLASLT"),
            "no env line in:\n{out}"
        );
    }

    #[test]
    fn quote_arg_quotes_whitespace_only() {
        assert_eq!(
            quote_arg(r"C:\path with spaces\x.exe"),
            "\"C:\\path with spaces\\x.exe\""
        );
        assert_eq!(quote_arg("--port"), "--port");
        // Embedded quotes are NOT escaped: no config value carries them today;
        // revisit if one ever can.
        assert_eq!(quote_arg("a\"b"), "a\"b");
    }
}
