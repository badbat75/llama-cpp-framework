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

use crate::strings;
use std::io;

// The launch itself (args, env, the process) is the core's, shared with
// `private-ai-manager run-instance`.
pub use pai_core::launch::env_vars;
use pai_core::launch::{has_presets_in, launch_command, server_args};

// ── Run detection (is_running + tasklist) ────────────────────────────────

/// `true` if an `llama-server` process is currently running.
pub fn is_running() -> bool {
    #[cfg(windows)]
    {
        // tasklist runs with the probe list HELD, not snapshotted afterwards:
        // see `proc::with_probe_pids`.
        crate::proc::with_probe_pids(|probes| {
            let Some(output) = crate::proc::run_hidden(
                std::path::Path::new("tasklist"),
                ["/fi", "IMAGENAME eq llama-server.exe", "/fo", "csv", "/nh"],
            ) else {
                return false;
            };
            running_from_tasklist(&String::from_utf8_lossy(&output.stdout), probes)
        })
    }

    #[cfg(not(windows))]
    {
        !live_server_pids().is_empty()
    }
}

/// The PID of a `/proc/<pid>/stat` line when it describes a LIVE llama-server:
/// the process name (the `comm` between the first `(` and the LAST `)`, since
/// a name may itself hold parentheses) is exactly `llama-server` and the state
/// after it is neither `Z` (zombie) nor `X` (dead).
///
/// The state is the point. A killed child stays in the process table as a
/// zombie until its parent waits for it, and the parent here is the
/// configurator: `pgrep -f llama-server`, which this replaces, matched that
/// `[llama-server] <defunct>` entry by name, so after the first Stop the server
/// read as running for as long as the GUI lived ("Stop timed out", then
/// "already running" on every Start). It also matched any command line merely
/// MENTIONING llama-server (a `tail -f llama-server.log`, an editor on the
/// log), which `pkill -f` then killed.
#[cfg(not(windows))]
fn live_server_pid(stat: &str) -> Option<u32> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    if stat.get(open + 1..close)? != "llama-server" {
        return None;
    }
    let state = stat.get(close + 1..)?.split_whitespace().next()?;
    if state == "Z" || state == "X" {
        return None;
    }
    stat.get(..open)?.trim().parse().ok()
}

/// Every live llama-server on the machine (the router and its per-model
/// children are the same executable), minus our own transient probes (as on
/// Windows), with the probe list held for the whole scan
/// (`proc::with_probe_pids` says why a snapshot is not enough). Read straight
/// from `/proc`: no `pgrep` to depend on, and the zombie filter of
/// `live_server_pid` has no portable spelling there.
#[cfg(not(windows))]
fn live_server_pids() -> Vec<u32> {
    crate::proc::with_probe_pids(|probes| {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|e| std::fs::read_to_string(e.path().join("stat")).ok())
            .filter_map(|stat| live_server_pid(&stat))
            .filter(|pid| !probes.contains(pid))
            .collect()
    })
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
    has_presets_in(&crate::paths::presets_ini())
}

/// How long `start()` watches a freshly spawned llama-server before declaring
/// success: long enough to catch a launch that dies on a bad preset, a taken
/// port, or a missing model (those exit at once), short enough that a
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
            strings::t("presets-none-configured")
        } else {
            strings::t("all-presets-off")
        };
        return Err(io::Error::new(io::ErrorKind::NotFound, msg));
    }

    let exe = crate::paths::llama_server_exe().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            strings::fill(
                "binary-not-found",
                &[&crate::paths::binary_name("llama-server")],
            ),
        )
    })?;

    let cfg = crate::server_cfg::load();
    let presets_path = crate::paths::presets_ini();

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

    let mut cmd = launch_command(&exe, &cfg, &presets_path);
    // llama-server writes most of its logging (model load, request logs,
    // GGML asserts, crash traces) to stderr; capture both streams in the
    // same log file so the log isn't empty and crashes leave a trail.
    let log_file_err = log_file.try_clone()?;
    cmd.stdout(log_file);
    cmd.stderr(log_file_err);
    // The router reads nothing from stdin (only its per-model children watch
    // theirs, a pipe the router makes), so it gets none of ours.
    cmd.stdin(std::process::Stdio::null());

    // No console flash: same CREATE_NO_WINDOW as the fire-and-forget probes, but
    // applied to a Command we spawn with custom stdio/env (so not run_hidden).
    crate::proc::hide_console(&mut cmd);
    // Nor any of our other standard handles, which Windows would hand it
    // anyway: a server holding the CLI's stdout hung whoever read it.
    crate::proc::keep_std_handles_private();

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
                return Err(io::Error::other(strings::fill(
                    "exited-on-launch",
                    &[&status, &log_path.display()],
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
    //
    // On Linux it must still be WAITED for: an exited child nobody waits for
    // stays a zombie for the life of its parent, which for the GUI is hours. A
    // parked thread reaps it whenever it ends (a stop, a crash); under the CLI
    // the process exits first and init inherits the server, which is fine.
    #[cfg(not(windows))]
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(Some(cfg))
}

/// Force-kill all llama-server.exe processes (taskkill /f: llama-server has
/// no graceful shutdown channel when running detached without a console; on
/// Linux a SIGTERM to every `live_server_pids` entry), then close the run's log
/// (module header) once the process is gone.
///
/// Returns `true` once no llama-server is left, `false` when one is still
/// alive after `STOP_GRACE`; the caller reports that as "still running", and a
/// restart must not go on to `start()`, which would see the old process and
/// launch nothing. The wait matters on Linux, where SIGTERM is a GRACEFUL stop:
/// the router waits for its children, and a child that holds a large KV cache
/// takes a long while to exit after logging "cleaning up before exit".
/// The log is closed only after `is_running()` reads false: renaming the file
/// while the process still holds it as stdout would carry its last lines into
/// the closed log and, on a kill that never lands, would file a live run under
/// a closing stamp. A wait that runs out skips the close; the next `start()`
/// picks the file up by its mtime.
pub fn stop() -> bool {
    #[cfg(windows)]
    {
        // Spawn failure (taskkill missing) is effectively impossible on Windows;
        // if the kill doesn't land, the wait below returns false and the
        // caller surfaces it as "still running".
        crate::proc::run_hidden(
            std::path::Path::new("taskkill"),
            ["/f", "/im", "llama-server.exe"],
        );
    }
    #[cfg(not(windows))]
    {
        // SIGTERM to exactly the processes `is_running` counts, so the two
        // cannot disagree about what a llama-server is.
        let pids = live_server_pids();
        if !pids.is_empty() {
            let _ = std::process::Command::new("kill")
                .arg("-TERM")
                .args(pids.iter().map(u32::to_string))
                .output();
        }
    }

    let gone = wait_gone(
        is_running,
        STOP_GRACE,
        std::time::Duration::from_millis(250),
    );
    if gone {
        close_log_per_settings(LogStamp::Now);
    }
    gone
}

/// How long `stop()` waits for the stopped process to disappear before it
/// reports failure. `taskkill /f` is synchronous in practice, so on Windows
/// this is a ceiling, not a delay; on Linux the graceful exit of a router whose
/// child holds a large KV cache is slow, so the ceiling is twice the longest
/// exit seen.
pub const STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(120);

/// Poll `running` every `step` until it reads false (`true`), or until `grace`
/// has passed with it still true (`false`). Split out of `stop()` so a process
/// that takes long to exit can be simulated in a test.
fn wait_gone(
    mut running: impl FnMut() -> bool,
    grace: std::time::Duration,
    step: std::time::Duration,
) -> bool {
    let deadline = std::time::Instant::now() + grace;
    while running() {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(step);
    }
    true
}

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
/// A stamp collision (two closes within the same second, e.g. a `bench run`
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

    // ── Stop wait ────────────────────────────────────────────────────────

    /// A process that outlives several polls (the router waiting for a child
    /// to write out its KV cache) is waited for, not reported gone at once.
    #[test]
    fn wait_gone_waits_for_a_slow_exit() {
        use std::time::{Duration, Instant};
        let exits_at = Instant::now() + Duration::from_millis(400);
        let mut polls = 0;
        let gone = wait_gone(
            || {
                polls += 1;
                Instant::now() < exits_at
            },
            Duration::from_secs(5),
            Duration::from_millis(20),
        );
        assert!(gone);
        assert!(Instant::now() >= exits_at);
        assert!(polls > 1);
    }

    /// One that never exits is reported still running once the grace is up.
    #[test]
    fn wait_gone_gives_up_after_the_grace() {
        use std::time::{Duration, Instant};
        let began = Instant::now();
        let gone = wait_gone(
            || true,
            Duration::from_millis(200),
            Duration::from_millis(20),
        );
        assert!(!gone);
        assert!(began.elapsed() >= Duration::from_millis(200));
        assert!(began.elapsed() < Duration::from_secs(5));
    }

    /// Nothing left to wait for: no sleep at all.
    #[test]
    fn wait_gone_returns_at_once_when_nothing_runs() {
        use std::time::Duration;
        assert!(wait_gone(|| false, Duration::ZERO, Duration::from_secs(60)));
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

        let closed = close_log(&log, LogStamp::Now, 0)
            .unwrap()
            .expect("a run to close");
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
        assert_eq!(
            std::fs::metadata(&log).unwrap().len(),
            1024,
            "left in place"
        );
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
        assert!(
            closed_logs(dir.path()).is_empty(),
            "an empty log is never filed"
        );
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
            past.duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
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
        assert!(
            !kept.contains(&"llama-server-20260101-000000.log".to_string()),
            "oldest goes"
        );
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

    // The Linux probe reads /proc/<pid>/stat. A zombie is the regression: the
    // killed server stays listed under its name until the configurator reaps
    // it, and counting it made every Stop time out.
    #[cfg(not(windows))]
    #[test]
    fn live_server_pid_skips_zombies_and_other_names() {
        use super::live_server_pid;
        assert_eq!(
            live_server_pid("29023 (llama-server) S 28855 29023 5495 0 -1"),
            Some(29023)
        );
        assert_eq!(live_server_pid("29023 (llama-server) R 1 2 3"), Some(29023));
        assert_eq!(
            live_server_pid("29023 (llama-server) Z 28855 29023 5495 0 -1"),
            None,
            "a zombie is not a running server"
        );
        assert_eq!(live_server_pid("29023 (llama-server) X 1 2 3"), None);
        assert_eq!(
            live_server_pid("28855 (llama-cpp-confi) S 5495 1 2"),
            None,
            "the configurator itself"
        );
        assert_eq!(
            live_server_pid("31 (tail) S 1 2 3"),
            None,
            "a command line naming llama-server.log is not a server"
        );
        // The name is everything up to the LAST ')': it may hold parentheses.
        assert_eq!(live_server_pid("7 (a) (llama-server) S 1 2 3"), None);
        assert_eq!(live_server_pid("garbage"), None);
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

    // The default install dir has a space ("C:\Program Files\llama.cpp"),
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

    /// A default config with the FlashAttention band OFF: the one SDK knob
    /// whose default exports something, switched off so the env tests of the
    /// others can pin "unset exports nothing".
    fn band_off() -> ServerConfig {
        ServerConfig {
            ..Default::default()
        }
    }

    // The SDK-tuning tri-state, on the env-only path.
    // Three things it pins: the value is the INT rocBLAS parses ("0"/"1", never
    // "false"/"true"), `None` exports NOTHING (a default-on-every-machine
    // workaround is exactly what the tri-state exists to avoid), and it reaches
    // the pasteable command line; a block that dropped it would run the backend
    // this setting exists to steer away from.

    #[test]
    fn rocblas_hipblaslt_rides_the_env_as_zero_or_one() {
        let with = |v: Option<bool>| ServerConfig {
            rocblas_use_hipblaslt: v,
            ..band_off()
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
