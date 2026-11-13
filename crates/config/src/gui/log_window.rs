//! View-logs window wiring: `LogWindow` (ui/log_window.slint) is an
//! independent third Slint root beside `AppWindow`/`AppTray`: non-modal, so
//! the configurator stays fully interactive while the log is open. Like the
//! tray it does NOT use `AppState`; Rust pushes state to it directly.
//!
//! Nothing here runs on the UI thread except the append itself. A worker
//! thread, alive only while the window is open (started by the View-logs
//! click, retired by the window's close button, so a closed window costs
//! nothing), polls `logs\llama-server.log` every 500 ms: `read_increment`
//! reads the bytes appended since the last poll and cuts them into rows, and
//! the result crosses to the event loop as a `Delta` that `apply` folds into
//! the window's row model. llama-server holds the file in append mode;
//! Windows' default share flags allow the concurrent read.
//! Truncation/rotation (len < offset) resets the rows; a missing file shows a
//! placeholder and keeps polling, so the first Start populates the window
//! live. Under the manager the log is the unit's journal instead
//! (`server_control::Backend::log_source`): `JournalTail` follows a
//! `journalctl -f` child and turns its lines into the same `Delta`s.
//!
//! The view is a list of rows, not one text widget, and that is the whole
//! reason the window stays responsive: a Slint text widget lays out ALL of
//! its text again on every change and every frame, far too slow on a real
//! log, while a list lays out the
//! visible rows only. It also fixes the view while Auto-scroll is off: an
//! append adds rows at the end and moves nothing, and the rows above are not
//! trimmed while the user is reading them (`HOLD_ROWS`).
//!
//! `live` holds the session id of the thread allowed to run, 0 while the
//! window is closed (as `hw_window.rs`): a new id per opening, so a close
//! followed by a quick reopen cannot leave two threads tailing.
//!
//! Helpers live in the parent `gui` module (`use super::*`).

use super::*;
use crate::strings;
use slint::Model;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// First-open (or post-rotation) backfill: start reading this far from EOF
/// instead of replaying a log that can be tens of MB at `-lv 4`.
const INIT_TAIL: u64 = 64 * 1024;
/// Rows retained while the view follows the tail. The full file stays on
/// disk, and the window shows its path.
const MAX_ROWS: usize = 5_000;
/// Rows retained while Auto-scroll is OFF. Trimming the top shifts everything
/// under the reader's eyes, so it waits: only past this (tens of MB of log
/// read with the box unticked) does the oldest part go.
const HOLD_ROWS: usize = 50_000;
/// Longest row, in bytes. A row is laid out whole whenever it is visible, so
/// one endless line (a prompt dump at `-lv 4`) is cut into several.
const ROW_MAX: usize = 2048;
/// An unterminated line longer than this is shown as if it had ended, so a
/// writer that never sends a newline cannot grow the pending tail forever.
const CARRY_MAX: usize = 64 * 1024;
/// Poll cadence. 500 ms reads as "live" for a log while staying invisible in
/// the profiler.
const POLL_EVERY: std::time::Duration = std::time::Duration::from_millis(500);

/// The tail-follower state between polls.
#[derive(Default)]
struct Tail {
    /// File offset already consumed; the next poll reads from here.
    offset: u64,
    /// The bytes after the last newline: a line still being written. Kept as
    /// bytes so a multi-byte char split across two polls is decoded whole.
    carry: Vec<u8>,
    /// How many rows at the end of the view show `carry`; the next delta
    /// replaces them.
    open_rows: usize,
    /// The view currently holds the missing-file text, not file content.
    missing: bool,
    /// False until the first poll, which must clear whatever a previous
    /// opening of the window left in the view.
    started: bool,
}

/// What one poll changes in the view.
#[derive(Debug, Default, PartialEq)]
struct Delta {
    /// Drop every row first (first poll, rotation, file appeared or vanished).
    reset: bool,
    /// Drop this many rows from the END first: the unterminated line shown by
    /// the previous delta, which `rows` now carries in its newer state.
    drop_last: usize,
    rows: Vec<String>,
}

impl Delta {
    fn is_empty(&self) -> bool {
        !self.reset && self.drop_last == 0 && self.rows.is_empty()
    }
}

/// Append `line` to `rows`, cut into `ROW_MAX` pieces on char boundaries. An
/// empty line is a row too: it is a blank line in the log.
fn push_rows(rows: &mut Vec<String>, line: &[u8]) {
    // Lossy: llama-server output is UTF-8, but a log is not a place to panic.
    let text = String::from_utf8_lossy(line);
    let mut rest = text.trim_end_matches('\r');
    while rest.len() > ROW_MAX {
        let mut cut = ROW_MAX;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        rows.push(rest[..cut].to_string());
        rest = &rest[cut..];
    }
    rows.push(rest.to_string());
}

/// Fold everything appended to `path` since the last call into a `Delta`.
/// Split from the thread loop so the whole file-side behavior (backfill,
/// rotation reset, the line still being written, missing file) is
/// unit-testable without a window.
fn read_increment(t: &mut Tail, path: &Path) -> Delta {
    let mut reset = !std::mem::replace(&mut t.started, true);
    let Ok(mut file) = std::fs::File::open(path) else {
        if t.missing && !reset {
            return Delta::default();
        }
        *t = Tail {
            missing: true,
            started: true,
            ..Tail::default()
        };
        return Delta {
            reset: true,
            drop_last: 0,
            rows: vec![strings::t("log-file-missing")],
        };
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    // The file appeared, or was truncated or rotated under us: start over
    // from the (new) top.
    if t.missing || len < t.offset {
        reset = true;
    }
    if reset {
        *t = Tail {
            started: true,
            ..Tail::default()
        };
    }
    let backfill = t.offset == 0 && len > INIT_TAIL;
    if backfill {
        t.offset = len - INIT_TAIL;
    }
    let mut bytes = Vec::new();
    if len == t.offset
        || file.seek(SeekFrom::Start(t.offset)).is_err()
        || file.read_to_end(&mut bytes).is_err()
        || bytes.is_empty()
    {
        return Delta {
            reset,
            ..Delta::default()
        };
    }
    t.offset += bytes.len() as u64;
    if backfill {
        // The backfill seek landed mid-line; drop the partial first line.
        match bytes.iter().position(|&b| b == b'\n') {
            Some(nl) => drop(bytes.drain(..=nl)),
            None => bytes.clear(),
        }
    }

    let mut data = std::mem::take(&mut t.carry);
    data.extend_from_slice(&bytes);
    let mut delta = Delta {
        reset,
        drop_last: std::mem::take(&mut t.open_rows),
        rows: Vec::new(),
    };
    let done = match data.iter().rposition(|&b| b == b'\n') {
        Some(nl) => nl + 1,
        None if data.len() > CARRY_MAX => data.len(),
        None => 0,
    };
    for line in data[..done].split_inclusive(|&b| b == b'\n') {
        push_rows(&mut delta.rows, line.strip_suffix(b"\n").unwrap_or(line));
    }
    if done < data.len() {
        let before = delta.rows.len();
        push_rows(&mut delta.rows, &data[done..]);
        t.open_rows = delta.rows.len() - before;
        t.carry = data[done..].to_vec();
    }
    delta
}

/// Fold one delta into the window's rows (UI thread). Appends add at the end
/// and never touch a row above, so the view does not move unless it is told
/// to: `scroll-to-end` runs only while Auto-scroll is ticked.
fn apply(win: &LogWindow, delta: Delta) {
    if delta.is_empty() {
        return;
    }
    let lines = win.get_lines();
    let Some(rows) = lines.as_any().downcast_ref::<VecModel<SharedString>>() else {
        return;
    };
    let new = delta.rows.into_iter().map(SharedString::from);
    if delta.reset {
        rows.set_vec(new.collect::<Vec<_>>());
    } else {
        for _ in 0..delta.drop_last.min(rows.row_count()) {
            rows.remove(rows.row_count() - 1);
        }
        rows.extend(new);
    }
    let follow = win.get_auto_scroll();
    // Trimmed in one step and with slack, so it happens once in a while
    // instead of on every append.
    let cap = if follow { MAX_ROWS } else { HOLD_ROWS };
    let count = rows.row_count();
    if count > cap + cap / 10 {
        rows.set_vec(rows.iter().skip(count - cap).collect::<Vec<_>>());
    }
    if follow {
        win.invoke_scroll_to_end();
    }
}

/// The header's liveness readout: "size · updated N ago". llama-server can
/// legitimately go quiet for minutes (router mode logs nothing between
/// requests); a visibly aging "updated …" is what tells the user the tail is
/// alive and the FILE is idle, not the window stuck.
fn tail_status(path: &std::path::Path) -> String {
    let Ok(meta) = std::fs::metadata(path) else {
        return String::new(); // the placeholder text already covers this case
    };
    let size = match meta.len() {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.0} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    };
    let age = match meta.modified().ok().and_then(|m| m.elapsed().ok()) {
        None => String::new(),
        Some(e) => format!(" · {}", age_text(e)),
    };
    format!("{size}{age}")
}

/// One opening's journal follower (the `Manager` backend): a journal reader
/// started through `spawn` (`journalctl -u <unit> -f ...`,
/// `server_control::journalctl_args` with `MAX_ROWS`), whose stdout lines
/// become rows. The first poll resets the view; an access refusal
/// (`server_control::journal_access_denied` on its stderr) replaces the rows
/// with `log-journal-denied`; a reader that fails otherwise, or cannot be
/// spawned, adds `log-journal-failed`. The reader is killed and reaped by
/// `stop` and on drop, so a closed window leaves no child.
struct JournalTail {
    /// The running reader; `None` once stopped, or when it could not be spawned.
    child: Option<std::process::Child>,
    /// The reader's output, one message per line, from its two pipe threads.
    lines: std::sync::mpsc::Receiver<JournalLine>,
    /// The spawn error, shown by the first poll.
    spawn_error: Option<String>,
    /// False until the first poll, which clears the view.
    started: bool,
    /// The view shows the final message (denied or failed): nothing follows.
    done: bool,
    /// What the reader wrote on stderr so far.
    stderr: String,
    /// How many of the two pipes reached their end.
    closed: u8,
    /// When the last stdout line came.
    last_line: Option<std::time::Instant>,
}

/// One message from a reader's pipe thread.
enum JournalLine {
    Out(Vec<u8>),
    Err(String),
    /// A pipe reached its end.
    Closed,
}

/// Send each line read from `pipe` (without its newline) through `tx`, then
/// `Closed`.
fn pipe_lines(
    pipe: impl Read + Send + 'static,
    tx: std::sync::mpsc::Sender<JournalLine>,
    wrap: fn(Vec<u8>) -> JournalLine,
) {
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(pipe);
        loop {
            let mut line = Vec::new();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if line.last() == Some(&b'\n') {
                        line.pop();
                    }
                    if tx.send(wrap(line)).is_err() {
                        return;
                    }
                }
            }
        }
        let _ = tx.send(JournalLine::Closed);
    });
}

impl JournalTail {
    fn start(spawn: &crate::server_control::JournalSpawn, unit: &str) -> JournalTail {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut tail = JournalTail {
            child: None,
            lines: rx,
            spawn_error: None,
            started: false,
            done: false,
            stderr: String::new(),
            closed: 0,
            last_line: None,
        };
        match spawn(&crate::server_control::journalctl_args(unit, MAX_ROWS)) {
            Ok(mut child) => {
                if let Some(out) = child.stdout.take() {
                    pipe_lines(out, tx.clone(), JournalLine::Out);
                }
                if let Some(err) = child.stderr.take() {
                    pipe_lines(err, tx, |l| {
                        JournalLine::Err(String::from_utf8_lossy(&l).into_owned())
                    });
                }
                tail.child = Some(child);
            }
            Err(e) => tail.spawn_error = Some(e.to_string()),
        }
        tail
    }

    /// What arrived since the last poll; never blocks.
    fn poll(&mut self) -> Delta {
        let reset = !std::mem::replace(&mut self.started, true);
        let mut delta = Delta {
            reset,
            ..Delta::default()
        };
        if self.done {
            return delta;
        }
        if let Some(e) = self.spawn_error.take() {
            self.done = true;
            delta.rows = vec![strings::fill("log-journal-failed", &[&e])];
            return delta;
        }
        while let Ok(msg) = self.lines.try_recv() {
            match msg {
                JournalLine::Out(line) => {
                    push_rows(&mut delta.rows, &line);
                    self.last_line = Some(std::time::Instant::now());
                }
                JournalLine::Err(line) => {
                    self.stderr.push_str(&line);
                    self.stderr.push('\n');
                    if crate::server_control::journal_access_denied(&self.stderr) {
                        self.done = true;
                        return Delta {
                            reset: true,
                            drop_last: 0,
                            rows: vec![strings::t("log-journal-denied")],
                        };
                    }
                }
                JournalLine::Closed => self.closed += 1,
            }
        }
        // Both pipes drained: the reader has ended, or is about to.
        if self.closed >= 2 {
            let status = self
                .child
                .as_mut()
                .and_then(|c| c.try_wait().ok().flatten());
            if let Some(status) = status {
                self.child = None;
                self.done = true;
                if !status.success() {
                    let why = self
                        .stderr
                        .lines()
                        .map(str::trim)
                        .rfind(|l| !l.is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| status.to_string());
                    delta
                        .rows
                        .push(strings::fill("log-journal-failed", &[&why]));
                }
            }
        }
        delta
    }

    /// The reader's pid while it runs.
    #[cfg(test)]
    fn child_id(&self) -> Option<u32> {
        self.child.as_ref().map(std::process::Child::id)
    }

    /// Kill the reader and wait for it.
    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for JournalTail {
    fn drop(&mut self) {
        self.stop();
    }
}

/// "updated N ago" for an age, as the header shows it.
fn age_text(e: std::time::Duration) -> String {
    match e.as_secs() {
        s if s < 2 => strings::t("updated-just-now"),
        s if s < 120 => strings::fill("updated-seconds-ago", &[&s]),
        s if s < 7200 => strings::fill("updated-minutes-ago", &[&(s / 60)]),
        s => strings::fill("updated-hours-ago", &[&(s / 3600)]),
    }
}

/// The header's readout for the journal: how long ago the last line came
/// ("updated 5 s ago"), empty before the first one. No size: the journal is
/// not one file.
fn journal_status(since_last_line: Option<std::time::Duration>) -> String {
    since_last_line.map(age_text).unwrap_or_default()
}

/// What the header names: the file's path, or the unit whose journal is read.
fn header_path(source: &crate::server_control::LogSource) -> String {
    match source {
        crate::server_control::LogSource::File(p) => p.to_string_lossy().into_owned(),
        crate::server_control::LogSource::Journal { unit } => unit.clone(),
    }
}

/// The tail thread of one opening of the window: poll, post, sleep, until
/// `live` no longer holds `id`. The first poll runs at once, so the window
/// does not sit blank for a tick.
fn tail_thread(
    win: slint::Weak<LogWindow>,
    live: Arc<AtomicU64>,
    id: u64,
    source: crate::server_control::LogSource,
) {
    use crate::server_control::LogSource;
    enum Follow {
        File(std::path::PathBuf, Tail),
        Journal(JournalTail),
    }
    // A journal reader is killed and reaped when this drops, at the end.
    let mut follow = match source {
        LogSource::File(path) => Follow::File(path, Tail::default()),
        LogSource::Journal { unit } => Follow::Journal(JournalTail::start(
            crate::server_control::journal_spawner(),
            &unit,
        )),
    };
    let mut last_status = None;
    while live.load(Ordering::Relaxed) == id {
        // The readout ages even while the log does not change; that is its
        // point. Posted only when the text differs, so an idle log wakes the
        // event loop once a second at most.
        let (delta, status) = match &mut follow {
            Follow::File(path, tail) => (read_increment(tail, path), tail_status(path)),
            Follow::Journal(j) => {
                let delta = j.poll();
                (delta, journal_status(j.last_line.map(|t| t.elapsed())))
            }
        };
        let status_changed = last_status.as_ref() != Some(&status);
        if !delta.is_empty() || status_changed {
            last_status = Some(status.clone());
            let win = win.clone();
            let posted = slint::invoke_from_event_loop(move || {
                if let Some(win) = win.upgrade() {
                    win.set_tail_status(SharedString::from(status));
                    apply(&win, delta);
                }
            });
            if posted.is_err() {
                return; // the event loop is gone: the app is quitting
            }
        }
        std::thread::sleep(POLL_EVERY);
    }
}

/// Build the window and wire `AppState.view_logs`. The caller (gui::run) must
/// keep the returned window alive for the whole event loop.
pub(super) fn wire(app: &AppWindow) -> Result<LogWindow, slint::PlatformError> {
    let win = LogWindow::new()?;
    install_strings(win.global::<Strings>());
    win.set_log_path(SharedString::from(header_path(
        &crate::server_control::backend().log_source(),
    )));
    win.set_lines(model(Vec::<SharedString>::new()));
    let live = Arc::new(AtomicU64::new(0));
    restore_window_size(&win);

    // Closing hides (not destroys) and retires the tail thread. The rows
    // stay, so a reopen shows the old tail until its first poll replaces it.
    {
        let live = live.clone();
        let win_weak = win.as_weak();
        win.window().on_close_requested(move || {
            live.store(0, Ordering::Relaxed);
            if let Some(win) = win_weak.upgrade() {
                remember_window_size(&win);
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    {
        let win_weak = win.as_weak();
        win.on_copy_all(move || {
            let Some(win) = win_weak.upgrade() else {
                return;
            };
            let rows: Vec<SharedString> = win.get_lines().iter().collect();
            win.invoke_copy_text(SharedString::from(rows.join("\n")));
        });
    }

    {
        let win_weak = win.as_weak();
        let sessions = std::cell::Cell::new(0u64);
        app.global::<AppState>().on_view_logs(move || {
            let Some(win) = win_weak.upgrade() else {
                return;
            };
            // Full activation, not a bare show(): a second click while the
            // window sits minimized/behind must surface it (gui::raise_window).
            raise_window(win.window());
            if live.load(Ordering::Relaxed) != 0 {
                return; // already open and tailing
            }
            sessions.set(sessions.get() + 1);
            let id = sessions.get();
            live.store(id, Ordering::Relaxed);
            let (win_weak, live) = (win_weak.clone(), live.clone());
            let source = crate::server_control::backend().log_source();
            std::thread::spawn(move || tail_thread(win_weak, live, id, source));
        });
    }

    Ok(win)
}

/// E2E phase, run from `tests/ui_bindings.rs`'s single `#[test]` (it needs the
/// testing backend that test owns): what `apply` does to the rows the window
/// shows.
#[cfg(test)]
pub(super) fn e2e() {
    let win = LogWindow::new().expect("build LogWindow");
    win.set_lines(model(Vec::<SharedString>::new()));
    let rows = |win: &LogWindow| -> Vec<String> {
        win.get_lines().iter().map(|s| s.to_string()).collect()
    };
    let delta = |reset: bool, drop_last: usize, new: &[&str]| Delta {
        reset,
        drop_last,
        rows: new.iter().map(|s| s.to_string()).collect(),
    };

    apply(&win, delta(true, 0, &["a", "b", "partial"]));
    assert_eq!(rows(&win), ["a", "b", "partial"]);
    // The line still being written is replaced by its newer state, in place
    // at the end; nothing above it is touched (the same model object).
    let before = win.get_lines();
    apply(&win, delta(false, 1, &["partial line", "c"]));
    assert_eq!(rows(&win), ["a", "b", "partial line", "c"]);
    assert!(win.get_lines() == before);
    // An empty delta changes nothing; a reset replaces everything.
    apply(&win, Delta::default());
    assert_eq!(rows(&win).len(), 4);
    apply(&win, delta(true, 0, &["fresh"]));
    assert_eq!(rows(&win), ["fresh"]);

    // Following the tail: past the cap (plus its slack) the oldest rows go.
    let many: Vec<String> = (0..MAX_ROWS + MAX_ROWS / 10 + 1)
        .map(|i| i.to_string())
        .collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    apply(&win, delta(true, 0, &many));
    assert_eq!(rows(&win).len(), MAX_ROWS);
    assert_eq!(rows(&win).last().unwrap(), many.last().unwrap());

    // Auto-scroll off: the reader's rows are NOT trimmed from under them.
    win.set_auto_scroll(false);
    apply(&win, delta(false, 0, &many));
    assert_eq!(rows(&win).len(), MAX_ROWS + many.len());
    assert_eq!(rows(&win)[0], (many.len() - MAX_ROWS).to_string());
}

// File-side tail behavior (no Slint backend needed: `read_increment` is pure
// file IO on a caller-supplied path, so this never touches `paths::`).
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn append(path: &Path, s: &str) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }

    /// What a view shows after folding `d` in: `apply`'s row arithmetic,
    /// without a window.
    fn fold(view: &mut Vec<String>, d: Delta) {
        if d.reset {
            view.clear();
        }
        view.truncate(view.len() - d.drop_last);
        view.extend(d.rows);
    }

    #[test]
    fn follows_appends_and_reports_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        let (mut t, mut view) = (
            Tail::default(),
            vec!["left by a previous opening".to_string()],
        );

        // Missing file → the placeholder replaces whatever was shown, once.
        fold(&mut view, read_increment(&mut t, &log));
        assert_eq!(view, [strings::t("log-file-missing")]);
        assert!(read_increment(&mut t, &log).is_empty());

        // File appears → placeholder replaced by content from offset 0.
        append(&log, "line 1\n");
        fold(&mut view, read_increment(&mut t, &log));
        assert_eq!(view, ["line 1"]);

        // Appends accumulate; an unchanged file is an empty delta.
        append(&log, "line 2\r\n\nline 4\n");
        fold(&mut view, read_increment(&mut t, &log));
        assert!(read_increment(&mut t, &log).is_empty());
        assert_eq!(view, ["line 1", "line 2", "", "line 4"]);
    }

    #[test]
    fn a_line_still_being_written_is_shown_and_then_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        let (mut t, mut view) = (Tail::default(), Vec::new());

        append(&log, "done\nhalf");
        fold(&mut view, read_increment(&mut t, &log));
        assert_eq!(view, ["done", "half"]);

        // The rest of the line arrives, split inside a multi-byte char.
        let rest = " way é\nnext\n".as_bytes();
        let cut = rest.iter().position(|&b| b == 0xC3).unwrap() + 1;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&log)
            .unwrap()
            .write_all(&rest[..cut])
            .unwrap();
        fold(&mut view, read_increment(&mut t, &log));
        assert_eq!(view.len(), 2, "still one open row, shown lossily for now");
        std::fs::OpenOptions::new()
            .append(true)
            .open(&log)
            .unwrap()
            .write_all(&rest[cut..])
            .unwrap();
        fold(&mut view, read_increment(&mut t, &log));
        assert_eq!(view, ["done", "half way é", "next"]);
    }

    #[test]
    fn truncation_resets_and_rereads_from_the_top() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        let (mut t, mut view) = (Tail::default(), Vec::new());

        append(&log, "old old old\n");
        fold(&mut view, read_increment(&mut t, &log));
        std::fs::write(&log, "fresh\n").unwrap(); // shorter than consumed → rotation
        fold(&mut view, read_increment(&mut t, &log));
        assert_eq!(view, ["fresh"]);
    }

    #[test]
    fn first_read_backfills_only_the_tail_and_drops_the_partial_line() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        // One INIT_TAIL-dwarfing file of known lines.
        let line = format!("{}\n", "x".repeat(99)); // 100 bytes/line
        append(&log, &line.repeat(2000)); // 200 KB > INIT_TAIL

        let mut t = Tail::default();
        let d = read_increment(&mut t, &log);
        // The seek lands mid-line; the partial first line must be gone, so
        // every row is a whole line and together they fit the backfill.
        assert!(d.reset);
        assert!(d.rows.len() <= INIT_TAIL as usize / line.len());
        assert!(d.rows.len() >= INIT_TAIL as usize / line.len() - 1);
        assert!(d.rows.iter().all(|r| r.len() == 99));
    }

    // The real log is not pure ASCII (chat-template dumps, model metadata),
    // and a prompt dump is one endless line: it is cut into rows, on char
    // boundaries, losing nothing.
    #[test]
    fn an_overlong_line_is_cut_into_rows_on_char_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        // 2-byte chars behind one ASCII byte: ROW_MAX lands mid-char.
        let long = format!("x{}", "é".repeat(ROW_MAX));
        append(&log, &format!("{long}\ntail line\n"));

        let d = read_increment(&mut Tail::default(), &log);
        assert_eq!(d.rows.last().unwrap(), "tail line");
        let pieces = &d.rows[..d.rows.len() - 1];
        assert!(pieces.len() > 1 && pieces.iter().all(|p| p.len() <= ROW_MAX));
        assert_eq!(pieces.concat(), long);
    }

    #[test]
    fn a_writer_that_never_ends_its_line_cannot_grow_the_pending_tail() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        let mut t = Tail::default();
        append(&log, &"y".repeat(INIT_TAIL as usize / 2));
        assert!(!read_increment(&mut t, &log).rows.is_empty());
        assert!(!t.carry.is_empty(), "short enough to wait for its newline");
        append(&log, &"y".repeat(CARRY_MAX));
        read_increment(&mut t, &log);
        assert!(t.carry.is_empty() && t.open_rows == 0);
    }

    // ── The journal (the `Manager` backend) ─────────────────────────────
    // The reader is a `sh -c` script standing in for journalctl, through the
    // injected spawner: no test runs the real journalctl.

    use crate::server_control::{journalctl_args, LogSource};
    use std::process::{Child, Command, Stdio};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    const UNIT: &str = "llama.cpp@default.service";

    /// A spawner running `script` in `sh`, piped as journalctl would be, and
    /// recording the argv it was asked for.
    fn sh_spawner(
        script: &'static str,
        argv: Arc<Mutex<Vec<Vec<String>>>>,
    ) -> impl Fn(&[String]) -> std::io::Result<Child> + Send + Sync {
        move |args: &[String]| {
            argv.lock().unwrap().push(args.to_vec());
            Command::new("sh")
                .arg("-c")
                .arg(script)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        }
    }

    /// Poll `tail` into `view` until `done` holds, failing after 5 s.
    fn poll_until(
        tail: &mut JournalTail,
        view: &mut Vec<String>,
        done: impl Fn(&[String]) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fold(view, tail.poll());
            if done(view) {
                return;
            }
            if Instant::now() > deadline {
                tail.stop();
                panic!("the view never got there within 5 s: {view:?}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// `/proc/<pid>` is gone: the child was killed AND reaped (a zombie keeps it).
    fn gone(pid: u32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if !Path::new(&format!("/proc/{pid}")).exists() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn the_journal_tail_asks_for_the_unit_and_the_retained_rows() {
        let argv = Arc::new(Mutex::new(Vec::new()));
        let spawn = sh_spawner("exec sleep 30", argv.clone());
        let mut tail = JournalTail::start(&spawn, UNIT);
        tail.stop();
        assert_eq!(
            *argv.lock().unwrap(),
            [journalctl_args(UNIT, MAX_ROWS)],
            "one reader, for the unit, backfilling as many rows as the view keeps"
        );
    }

    #[test]
    fn journal_lines_become_rows_and_the_first_poll_clears_the_view() {
        let argv = Arc::new(Mutex::new(Vec::new()));
        let spawn = sh_spawner(
            "printf 'main: server is listening\\r\\n\\nsrv  update_slots: all slots are idle\\n'; exec sleep 30",
            argv,
        );
        let mut tail = JournalTail::start(&spawn, UNIT);
        let mut view = vec!["left by a previous opening".to_string()];
        let first = tail.poll();
        assert!(
            first.reset,
            "the first poll clears what the last opening left"
        );
        fold(&mut view, first);
        poll_until(&mut tail, &mut view, |v| v.len() >= 3);
        assert_eq!(
            view,
            [
                "main: server is listening",
                "",
                "srv  update_slots: all slots are idle"
            ]
        );
        // Nothing new: an empty delta.
        assert!(tail.poll().is_empty());
        tail.stop();
    }

    /// The window's row bound and cutting apply to the journal as to the file.
    #[test]
    fn an_overlong_journal_line_is_cut_into_rows() {
        let argv = Arc::new(Mutex::new(Vec::new()));
        // ROW_MAX * 2 + 1 bytes, then a short line.
        let spawn = sh_spawner(
            "head -c 4097 /dev/zero | tr '\\0' x; printf '\\nend\\n'; exec sleep 30",
            argv,
        );
        assert_eq!(ROW_MAX, 2048, "the script's length assumes ROW_MAX");
        let mut tail = JournalTail::start(&spawn, UNIT);
        let mut view = Vec::new();
        poll_until(&mut tail, &mut view, |v| {
            v.last().is_some_and(|l| l == "end")
        });
        tail.stop();
        assert_eq!(
            view.len(),
            4,
            "{:?}",
            view.iter().map(String::len).collect::<Vec<_>>()
        );
        assert!(view[..3].iter().all(|r| r.len() <= ROW_MAX));
        assert_eq!(view[..3].concat(), "x".repeat(4097));
    }

    /// Not root and in none of the journal's groups: journalctl opens the
    /// user's own journal, prints systemd's hint on stderr and keeps running
    /// with nothing to show. The window says why, naming the groups.
    #[test]
    fn a_hint_that_the_system_journal_is_hidden_shows_the_groups_message() {
        let argv = Arc::new(Mutex::new(Vec::new()));
        let spawn = sh_spawner(
            "printf 'Hint: You are currently not seeing messages from other users and the system.\\n      \
             Users in groups '\"'\"'adm'\"'\"', '\"'\"'systemd-journal'\"'\"' can see all messages.\\n      \
             Pass -q to turn off this notice.\\n' >&2; exec sleep 30",
            argv,
        );
        let mut tail = JournalTail::start(&spawn, UNIT);
        let mut view = vec!["left by a previous opening".to_string()];
        let denied = strings::t("log-journal-denied");
        poll_until(&mut tail, &mut view, |v| v.contains(&denied));
        assert_eq!(view, std::slice::from_ref(&denied), "the message alone");
        // Once: the next polls change nothing.
        std::thread::sleep(Duration::from_millis(100));
        assert!(tail.poll().is_empty());
        tail.stop();
    }

    /// No journal file opened at all: systemd's error and a failure exit.
    #[test]
    fn insufficient_permissions_and_a_failure_exit_show_the_groups_message() {
        let argv = Arc::new(Mutex::new(Vec::new()));
        let spawn = sh_spawner(
            "echo 'No journal files were opened due to insufficient permissions.' >&2; exit 1",
            argv,
        );
        let mut tail = JournalTail::start(&spawn, UNIT);
        let mut view = Vec::new();
        let denied = strings::t("log-journal-denied");
        poll_until(&mut tail, &mut view, |v| v.contains(&denied));
        assert_eq!(view, [denied]);
        tail.stop();
    }

    /// Another failure keeps the rows already shown and adds why, from the
    /// last line journalctl wrote on stderr.
    #[test]
    fn a_reader_that_fails_otherwise_says_why_after_its_rows() {
        let argv = Arc::new(Mutex::new(Vec::new()));
        let spawn = sh_spawner(
            "echo 'srv  load_model: loading'; echo 'Failed to iterate through journal: Bad message' >&2; exit 3",
            argv,
        );
        let mut tail = JournalTail::start(&spawn, UNIT);
        let mut view = Vec::new();
        let failed = strings::fill(
            "log-journal-failed",
            &[&"Failed to iterate through journal: Bad message"],
        );
        poll_until(&mut tail, &mut view, |v| v.contains(&failed));
        assert_eq!(view, ["srv  load_model: loading".to_string(), failed]);
        tail.stop();
    }

    #[test]
    fn a_reader_that_cannot_be_spawned_says_why() {
        let spawn = |_: &[String]| -> std::io::Result<Child> {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "journalctl not found",
            ))
        };
        let mut tail = JournalTail::start(&spawn, UNIT);
        assert_eq!(tail.child_id(), None);
        let mut view = vec!["left by a previous opening".to_string()];
        fold(&mut view, tail.poll());
        assert_eq!(
            view,
            [strings::fill(
                "log-journal-failed",
                &[&"journalctl not found"]
            )]
        );
        assert!(tail.poll().is_empty());
        tail.stop();
    }

    /// Closing the window stops the tail: the reader is killed and reaped.
    #[test]
    fn stop_kills_and_reaps_the_journal_reader() {
        let argv = Arc::new(Mutex::new(Vec::new()));
        let spawn = sh_spawner("exec sleep 30", argv);
        let mut tail = JournalTail::start(&spawn, UNIT);
        let pid = tail.child_id().expect("the reader runs");
        assert!(Path::new(&format!("/proc/{pid}")).exists());
        tail.stop();
        assert!(gone(pid), "the reader {pid} outlived the stop");
        assert_eq!(tail.child_id(), None);
        tail.stop(); // a second stop is harmless
    }

    /// No orphan even when the tail is dropped without a stop (the tail
    /// thread unwinding).
    #[test]
    fn dropping_the_journal_tail_kills_the_reader() {
        let argv = Arc::new(Mutex::new(Vec::new()));
        let spawn = sh_spawner("exec sleep 30", argv);
        let tail = JournalTail::start(&spawn, UNIT);
        let pid = tail.child_id().expect("the reader runs");
        drop(tail);
        assert!(gone(pid), "the reader {pid} outlived the drop");
    }

    /// journalctl(1): root and the members of systemd-journal, adm and wheel
    /// read the system journal.
    #[test]
    fn the_denied_message_names_the_groups() {
        let text = strings::t("log-journal-denied");
        assert_ne!(text, "log-journal-denied", "the key is in the strings file");
        for group in ["systemd-journal", "adm", "wheel"] {
            assert!(text.contains(group), "{group} missing from {text:?}");
        }
        let failed = strings::fill("log-journal-failed", &[&"WHY"]);
        assert!(failed.contains("WHY") && !failed.contains("log-journal-failed"));
    }

    #[test]
    fn the_journal_readout_ages_from_the_last_line() {
        assert_eq!(journal_status(None), "", "nothing yet: no readout");
        assert_eq!(
            journal_status(Some(Duration::from_millis(500))),
            strings::t("updated-just-now")
        );
        assert_eq!(
            journal_status(Some(Duration::from_secs(5))),
            strings::fill("updated-seconds-ago", &[&5])
        );
        assert_eq!(
            journal_status(Some(Duration::from_secs(600))),
            strings::fill("updated-minutes-ago", &[&10])
        );
        assert_eq!(
            journal_status(Some(Duration::from_secs(3 * 3600))),
            strings::fill("updated-hours-ago", &[&3])
        );
    }

    #[test]
    fn the_header_names_the_file_or_the_unit() {
        assert_eq!(header_path(&LogSource::Journal { unit: UNIT.into() }), UNIT);
        let file = std::path::PathBuf::from("/tmp/x/logs/llama-server.log");
        assert_eq!(
            header_path(&LogSource::File(file.clone())),
            file.to_string_lossy()
        );
    }
}
