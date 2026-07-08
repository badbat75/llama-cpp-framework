//! View-logs window wiring: `LogWindow` (ui/log_window.slint) is an
//! independent third Slint root beside `AppWindow`/`AppTray` — non-modal, so
//! the configurator stays fully interactive while the log is open. Like the
//! tray it does NOT use `AppState`; Rust pushes state to it directly.
//!
//! The tail runs on a `slint::Timer` (UI thread) that exists only while the
//! window is open — armed by the View-logs click, stopped by the window's
//! close button, so a closed window costs nothing (no wakeups, no file IO).
//! Every 500 ms `read_increment` reads the bytes appended to
//! `logs\llama-server.log` since the last poll and folds them into a bounded
//! in-memory tail (`Tail`). Reading an append increment is a sub-millisecond
//! file operation, so no worker thread is needed (unlike the tasklist/version
//! probes). llama-server holds the file in append mode; Windows' default
//! share flags allow the concurrent read. Truncation/rotation (len < offset)
//! resets the tail; a missing file shows a placeholder and keeps polling, so
//! the first Start populates the window live.
//!
//! Helpers live in the parent `gui` module (`use super::*`).

use super::*;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// First-open (or post-rotation) backfill: start reading this far from EOF
/// instead of replaying a log that can be tens of MB at `-lv 4`.
const INIT_TAIL: u64 = 64 * 1024;
/// Cap on the retained text — the TextEdit gets sluggish on multi-MB content,
/// and "the last quarter-MB" is plenty for a live tail (the full file stays on
/// disk, and the window shows its path).
const MAX_RETAINED: usize = 256 * 1024;
/// Poll cadence. 500 ms reads as "live" for a log while staying invisible in
/// the profiler; the poll is skipped entirely while the window is hidden.
const POLL_EVERY: std::time::Duration = std::time::Duration::from_millis(500);

const MISSING_TEXT: &str =
    "(log file not found — it is created the first time llama-server starts)";

/// The tail-follower state between polls.
#[derive(Default)]
struct Tail {
    /// File offset already consumed; the next poll reads from here.
    offset: u64,
    /// Retained text, capped at `MAX_RETAINED` on a line boundary.
    buffer: String,
    /// `buffer` currently holds `MISSING_TEXT`, not file content.
    missing: bool,
}

/// Fold everything appended to `path` since the last call into the tail.
/// Split from `poll` so the whole file-side behavior (backfill, rotation
/// reset, retention cap, missing file) is unit-testable without a window.
fn read_increment(t: &mut Tail, path: &Path) {
    let Ok(mut file) = std::fs::File::open(path) else {
        t.offset = 0;
        t.buffer = MISSING_TEXT.to_string();
        t.missing = true;
        return;
    };
    if t.missing {
        t.buffer.clear();
        t.missing = false;
    }
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if len < t.offset {
        // Truncated or rotated under us — start over from the (new) top.
        t.offset = 0;
        t.buffer.clear();
    }
    let backfill = t.offset == 0 && len > INIT_TAIL;
    if backfill {
        t.offset = len - INIT_TAIL;
    }
    if len == t.offset || file.seek(SeekFrom::Start(t.offset)).is_err() {
        return;
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return;
    }
    t.offset += bytes.len() as u64;
    // Lossy: a multi-byte char split across two polls degrades to replacement
    // chars — acceptable for a log viewer (llama-server output is ASCII).
    let mut chunk = String::from_utf8_lossy(&bytes).into_owned();
    if backfill {
        // The backfill seek landed mid-line; drop the partial first line.
        if let Some(nl) = chunk.find('\n') {
            chunk.drain(..=nl);
        }
    }
    t.buffer.push_str(&chunk);
    if t.buffer.len() > MAX_RETAINED {
        let mut over = t.buffer.len() - MAX_RETAINED;
        // `over` is a byte count and can land inside a multi-byte char —
        // advance to a boundary BEFORE slicing, or the index panics inside
        // the timer callback (the real log is not pure ASCII: chat-template
        // dumps and model metadata carry Unicode).
        while !t.buffer.is_char_boundary(over) {
            over += 1;
        }
        // Cut on the next line boundary; if none is in range (one giant
        // line), cut at `over` itself.
        let cut = t.buffer[over..]
            .find('\n')
            .map(|i| over + i + 1)
            .unwrap_or(over);
        t.buffer.drain(..cut);
    }
}

/// One timer tick: advance the tail, and only when the text actually changed
/// push it to the window (an unconditional set would repaint — and clamp a
/// selection the user is dragging — every 500 ms even on an idle log). The
/// tail_status readout refreshes every tick regardless: its whole point is to
/// keep visibly aging while the file does NOT change.
fn poll(win: &LogWindow, tail: &RefCell<Tail>) {
    let mut t = tail.borrow_mut();
    let path = paths::server_log();
    read_increment(&mut t, &path);
    let status = SharedString::from(tail_status(&path));
    if win.get_tail_status() != status {
        win.set_tail_status(status);
    }
    if win.get_log_text() != t.buffer.as_str() {
        win.set_log_text(SharedString::from(t.buffer.as_str()));
        if !t.missing && win.get_auto_scroll() {
            // Byte length: set-selection-offsets takes UTF-8 offsets, and the
            // retention cap keeps it far below i32::MAX.
            win.invoke_scroll_to_end(t.buffer.len() as i32);
        }
    }
}

/// The header's liveness readout: "size · updated N ago". llama-server can
/// legitimately go quiet for minutes (router mode logs nothing between
/// requests) — a visibly aging "updated …" is what tells the user the tail is
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
        Some(e) if e.as_secs() < 2 => " · updated just now".into(),
        Some(e) if e.as_secs() < 120 => format!(" · updated {} s ago", e.as_secs()),
        Some(e) if e.as_secs() < 7200 => format!(" · updated {} min ago", e.as_secs() / 60),
        Some(e) => format!(" · updated {} h ago", e.as_secs() / 3600),
    };
    format!("{size}{age}")
}

/// Build the window and wire `AppState.view_logs`. The poll timer runs ONLY
/// while the window is open: started by the View-logs click, stopped by the
/// window's close button — closed, the tail costs literally nothing (no timer
/// wakeups, no file IO). The caller (gui::run) must keep the returned pair
/// alive for the whole event loop.
pub(super) fn wire(app: &AppWindow) -> Result<(LogWindow, Rc<slint::Timer>), slint::PlatformError> {
    let win = LogWindow::new()?;
    win.set_log_path(SharedString::from(
        paths::server_log().to_string_lossy().into_owned(),
    ));

    let tail = Rc::new(RefCell::new(Tail::default()));
    let timer = Rc::new(slint::Timer::default());

    // Closing hides (not destroys) and parks the timer. The tail state
    // survives, so reopening via View logs resumes instantly where it left off.
    {
        let timer = timer.clone();
        win.window().on_close_requested(move || {
            timer.stop();
            slint::CloseRequestResponse::HideWindow
        });
    }

    {
        let win_weak = win.as_weak();
        let tail = tail.clone();
        let timer = timer.clone();
        app.global::<AppState>().on_view_logs(move || {
            let Some(win) = win_weak.upgrade() else {
                return;
            };
            // Full activation, not a bare show(): a second click while the
            // window sits minimized/behind must surface it (same rationale as
            // gui::activate_window, which is AppWindow-typed).
            use slint::winit_030::WinitWindowAccessor;
            win.show().ok();
            win.window().with_winit_window(|w| {
                w.set_visible(true);
                w.set_minimized(false);
                w.focus_window();
            });
            // Immediate fill — don't sit blank until the first timer tick.
            poll(&win, &tail);
            // (Re)arm the poll. `start` on a running timer just restarts it,
            // so a second click while the window is already open is harmless.
            let tick_win = win_weak.clone();
            let tick_tail = tail.clone();
            timer.start(slint::TimerMode::Repeated, POLL_EVERY, move || {
                let Some(win) = tick_win.upgrade() else {
                    return;
                };
                // Belt and braces: close stops the timer, so a hidden window
                // here means some future code path hid it another way — skip
                // the file IO rather than tail into the void.
                if !win.window().is_visible() {
                    return;
                }
                poll(&win, &tick_tail);
            });
        });
    }

    Ok((win, timer))
}

// File-side tail behavior (no Slint backend needed — `read_increment` is pure
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

    #[test]
    fn follows_appends_and_reports_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        let mut t = Tail::default();

        // Missing file → placeholder, and the tail keeps polling.
        read_increment(&mut t, &log);
        assert!(t.missing);
        assert_eq!(t.buffer, MISSING_TEXT);

        // File appears → placeholder replaced by content from offset 0.
        append(&log, "line 1\n");
        read_increment(&mut t, &log);
        assert!(!t.missing);
        assert_eq!(t.buffer, "line 1\n");

        // Appends accumulate; unchanged file is a no-op.
        append(&log, "line 2\n");
        read_increment(&mut t, &log);
        read_increment(&mut t, &log);
        assert_eq!(t.buffer, "line 1\nline 2\n");
    }

    #[test]
    fn truncation_resets_and_rereads_from_the_top() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        let mut t = Tail::default();

        append(&log, "old old old\n");
        read_increment(&mut t, &log);
        std::fs::write(&log, "fresh\n").unwrap(); // shorter than consumed → rotation
        read_increment(&mut t, &log);
        assert_eq!(t.buffer, "fresh\n");
    }

    #[test]
    fn first_read_backfills_only_the_tail_and_drops_the_partial_line() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        // One INIT_TAIL-dwarfing file of known lines.
        let line = format!("{}\n", "x".repeat(99)); // 100 bytes/line
        append(&log, &line.repeat(2000)); // 200 KB > INIT_TAIL

        let mut t = Tail::default();
        read_increment(&mut t, &log);
        assert!(t.buffer.len() <= INIT_TAIL as usize);
        // The seek lands mid-line; the partial first line must be gone, so the
        // retained text starts on a boundary and is made of whole lines.
        assert!(t.buffer.starts_with(&line));
        assert!(t.buffer.ends_with('\n'));
        assert_eq!(t.buffer.len() % line.len(), 0);
    }

    // The real log is not pure ASCII (chat-template dumps, model metadata):
    // when the retention cut lands INSIDE a multi-byte char, the trim must
    // step to the next boundary instead of panicking mid-slice.
    #[test]
    fn retention_cap_survives_a_cut_inside_a_multibyte_char() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        let mut t = Tail::default();

        // Small first read (no backfill), then one over-cap append whose
        // byte layout puts `len - MAX_RETAINED` at an odd offset inside a
        // run of 2-byte chars — i.e. mid-'é'.
        append(&log, "x\n");
        read_increment(&mut t, &log);
        append(&log, &format!("{}\n", "é".repeat(131_500)));
        append(&log, "tail line\n");
        read_increment(&mut t, &log);

        assert!(t.buffer.len() <= MAX_RETAINED);
        // The é-monster line had no newline in range, so the cut lands right
        // after it — only the following full line survives.
        assert_eq!(t.buffer, "tail line\n");
    }

    #[test]
    fn retention_cap_trims_oldest_lines_on_a_line_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("llama-server.log");
        let mut t = Tail::default();

        let line = format!("{}\n", "y".repeat(99));
        append(&log, &line.repeat(1000)); // ~100 KB seed (backfilled to 64 KB)
        read_increment(&mut t, &log);
        // Keep appending past the retention cap in follow mode.
        for _ in 0..4 {
            append(&log, &line.repeat(1000));
            read_increment(&mut t, &log);
        }
        assert!(t.buffer.len() <= MAX_RETAINED);
        assert!(t.buffer.starts_with(&line), "must trim on a line boundary");
    }
}
