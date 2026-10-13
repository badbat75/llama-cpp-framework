//! HW Monitor window wiring: `HwWindow` (ui/hw_window.slint) is an independent
//! Slint root beside `AppWindow`/`AppTray`/`LogWindow`: non-modal, and like
//! them it does NOT use `AppState`; Rust pushes its one property directly.
//!
//! The sampling (`hwmon::Sampler`) runs on a worker thread, not on a
//! `slint::Timer` as the log tail does: NVML measures PCIe throughput over
//! 20 ms per call, which would stall the event loop once a second per NVIDIA
//! card. The thread exists only while the window is open: the click that
//! opens the window starts it, and it leaves at its next wakeup once the
//! window closes, so a closed window costs nothing.
//!
//! `live` is how the thread knows: it holds the session id of the thread
//! allowed to run, 0 while the window is closed. A new id per opening, rather
//! than a bool, means a close followed by a quick reopen cannot leave the old
//! thread running beside the new one.
//!
//! Helpers live in the parent `gui` module (`use super::*`).

use super::*;
use crate::hwmon;
use slint::Model;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Gap between the baseline and the first sample: long enough for the rates to
/// mean something, short enough that the window does not sit empty.
const FIRST_SAMPLE: Duration = Duration::from_millis(400);
const SAMPLE_EVERY: Duration = Duration::from_secs(1);

fn bar_row(b: &hwmon::Bar) -> HwBar {
    HwBar {
        label: SharedString::from(b.label.as_str()),
        text: SharedString::from(b.text.as_str()),
        fraction: b.fraction,
    }
}

/// The devices the user switched off, by `hwmon::Group::id`. Shared with the
/// sampler thread, which reads nothing for them. Seeded from settings.ini
/// (`HwMonitorHidden`) and written back on every click (`persist`), so the
/// choice holds across launches.
type Off = Arc<Mutex<HashSet<String>>>;

fn group_row(g: &hwmon::Group, enabled: bool) -> HwGroup {
    let bars = if enabled { g.bars.as_slice() } else { &[] };
    HwGroup {
        id: SharedString::from(g.id.as_str()),
        title: SharedString::from(g.title.as_str()),
        detail: SharedString::from(g.detail.as_str()),
        enabled,
        bars: model(bars.iter().map(bar_row).collect()),
    }
}

/// Show one sample. While the set of devices and bars is the one already on
/// screen, only the bar rows are rewritten, in place: replacing the models
/// would rebuild every element each second and the fill animation would
/// never run.
///
/// `off` has the last word on a device's state, not the sample: one taken
/// just before a checkbox was unticked lands after it, and would otherwise
/// tick the box back for a second.
fn push(win: &HwWindow, groups: &[hwmon::Group], off: &HashSet<String>) {
    let enabled = |g: &hwmon::Group| g.enabled && !off.contains(&g.id);
    let bar_count = |g: &hwmon::Group| if enabled(g) { g.bars.len() } else { 0 };
    let shown = win.get_groups();
    let same_shape = shown.row_count() == groups.len()
        && shown.iter().zip(groups).all(|(row, g)| {
            row.id == g.id.as_str()
                && row.title == g.title.as_str()
                && row.detail == g.detail.as_str()
                && row.enabled == enabled(g)
                && row.bars.row_count() == bar_count(g)
        });
    if same_shape {
        for (row, g) in shown.iter().zip(groups) {
            for (i, b) in g.bars.iter().take(bar_count(g)).enumerate() {
                let new = bar_row(b);
                if row.bars.row_data(i).as_ref() != Some(&new) {
                    row.bars.set_row_data(i, new);
                }
            }
        }
        return;
    }
    win.set_groups(model(
        groups.iter().map(|g| group_row(g, enabled(g))).collect(),
    ));
}

/// A device's checkbox was clicked. The set is what stops the sampler; the
/// row is rewritten here as well so the card goes (or the empty card comes
/// back) at once, not at the next sample.
fn toggle(win: &HwWindow, off: &Off, index: usize, on: bool) {
    let shown = win.get_groups();
    let Some(mut row) = shown.row_data(index) else {
        return;
    };
    let key = row.id.to_string();
    {
        let mut off = off.lock().unwrap_or_else(|e| e.into_inner());
        if on {
            off.remove(&key);
        } else {
            off.insert(key);
        }
    }
    row.enabled = on;
    row.bars = model(Vec::new());
    shown.set_row_data(index, row);
}

/// Write the switched-off set to settings.ini, read-modify-write so no other
/// key is touched. A failed write costs only the memory of the choice (the
/// window already obeys it), and this window has nowhere to say so.
fn persist(off: &Off) {
    let mut hidden: Vec<String> = off
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .cloned()
        .collect();
    hidden.sort();
    let mut cfg = settings::load();
    if cfg.hw_monitor_hidden != hidden {
        cfg.hw_monitor_hidden = hidden;
        let _ = settings::save(&cfg);
    }
}

/// Build the window and wire `AppState.open_hw_monitor`. The caller
/// (gui::run) must keep the returned window alive for the whole event loop.
pub(super) fn wire(app: &AppWindow) -> Result<HwWindow, slint::PlatformError> {
    let win = HwWindow::new()?;
    let live = Arc::new(AtomicU64::new(0));
    let off: Off = Arc::new(Mutex::new(
        settings::load().hw_monitor_hidden.into_iter().collect(),
    ));

    // Closing hides (not destroys) and retires the sampler thread.
    {
        let live = live.clone();
        win.window().on_close_requested(move || {
            live.store(0, Ordering::Relaxed);
            slint::CloseRequestResponse::HideWindow
        });
    }

    {
        let win_weak = win.as_weak();
        let off = off.clone();
        win.on_group_toggled(move |index, on| {
            if let Some(win) = win_weak.upgrade() {
                toggle(&win, &off, index as usize, on);
                persist(&off);
            }
        });
    }

    {
        let win_weak = win.as_weak();
        let sessions = std::cell::Cell::new(0u64);
        app.global::<AppState>().on_open_hw_monitor(move || {
            let Some(win) = win_weak.upgrade() else {
                return;
            };
            // Full activation, not a bare show(): a second click while the
            // window sits minimized/behind must surface it (as log_window.rs).
            use slint::winit_030::WinitWindowAccessor;
            win.show().ok();
            win.window().with_winit_window(|w| {
                w.set_visible(true);
                w.set_minimized(false);
                w.focus_window();
            });
            if live.load(Ordering::Relaxed) != 0 {
                return; // already open and sampling
            }
            sessions.set(sessions.get() + 1);
            let id = sessions.get();
            live.store(id, Ordering::Relaxed);

            // Read on every opening, so the disk follows a ModelsDir edit.
            let models_dir = PathBuf::from(server_cfg::load().models_dir_or_default());
            let live = live.clone();
            let off = off.clone();
            let win_weak = win_weak.clone();
            std::thread::spawn(move || {
                // A snapshot per reading: the lock is never held across one.
                let now_off = || off.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let mut sampler = hwmon::Sampler::new(&models_dir, &now_off());
                let mut wait = FIRST_SAMPLE;
                loop {
                    std::thread::sleep(wait);
                    wait = SAMPLE_EVERY;
                    if live.load(Ordering::Relaxed) != id {
                        return;
                    }
                    let groups = sampler.sample(&now_off());
                    let win_weak = win_weak.clone();
                    let off = off.clone();
                    let posted = slint::invoke_from_event_loop(move || {
                        if let Some(win) = win_weak.upgrade() {
                            let off = off.lock().unwrap_or_else(|e| e.into_inner());
                            push(&win, &groups, &off);
                        }
                    });
                    if posted.is_err() {
                        return; // the event loop is gone: the app is quitting
                    }
                }
            });
        });
    }

    Ok(win)
}

/// E2E phase, run from `tests/ui_bindings.rs`'s single `#[test]` (it needs the
/// testing backend that test owns): the bars reach the screen, a sample of the
/// same shape rewrites them IN PLACE (the same model object, or the fill
/// animation never runs), a changed device set rebuilds, and a device's
/// checkbox hides its card and holds against a sample already on its way.
#[cfg(test)]
pub(super) fn e2e() {
    use i_slint_backend_testing::ElementHandle;

    let group = |title: &str, bars: &[(&str, &str, f32)]| hwmon::Group {
        id: title.to_lowercase().replace(' ', "-"),
        title: title.to_string(),
        detail: String::new(),
        enabled: true,
        bars: bars
            .iter()
            .map(|(label, text, fraction)| hwmon::Bar {
                label: label.to_string(),
                text: text.to_string(),
                fraction: *fraction,
            })
            .collect(),
    };
    let shown =
        |win: &HwWindow, text: &str| ElementHandle::find_by_accessible_label(win, text).count();
    let none = HashSet::new();

    let win = HwWindow::new().expect("build HwWindow");
    win.window().set_size(slint::PhysicalSize::new(800, 1200));
    win.show().expect("realize window");
    assert_eq!(shown(&win, "Sampling…"), 1, "the empty state");

    let a1 = [group(
        "Card A",
        &[("VRAM", "1.0 / 2.0 GiB", 0.5), ("GPU", "10 %", 0.1)],
    )];
    push(&win, &a1, &none);
    assert_eq!(shown(&win, "Sampling…"), 0);
    // (A title is a CheckBox, which the accessibility tree lists together
    // with its own inner text: present, not "once".)
    assert!(shown(&win, "Card A") > 0);
    for text in ["VRAM", "1.0 / 2.0 GiB", "GPU", "10 %"] {
        assert_eq!(shown(&win, text), 1, "{text:?} must be drawn once");
    }

    let before = win.get_groups();
    let a2 = [group(
        "Card A",
        &[("VRAM", "1.5 / 2.0 GiB", 0.75), ("GPU", "10 %", 0.1)],
    )];
    push(&win, &a2, &none);
    assert!(win.get_groups() == before, "same shape: updated in place");
    assert_eq!(shown(&win, "1.5 / 2.0 GiB"), 1);
    assert_eq!(shown(&win, "1.0 / 2.0 GiB"), 0);
    let vram = before.row_data(0).unwrap().bars.row_data(0).unwrap();
    assert_eq!(vram.fraction, 0.75);

    let two = [
        group("Card A", &[("VRAM", "1.5 / 2.0 GiB", 0.75)]),
        group("Card B", &[("GTT", "0.1 / 2.0 GiB", 0.05)]),
    ];
    push(&win, &two, &none);
    assert!(win.get_groups() != before, "another shape: rebuilt");
    assert!(shown(&win, "Card B") > 0);
    assert_eq!(shown(&win, "10 %"), 0);

    // Untick Card A through the window's own callback, wired as `wire` does
    // minus `persist` (settings.ini is settings.rs's to test).
    let off = Off::default();
    {
        let (weak, off) = (win.as_weak(), off.clone());
        win.on_group_toggled(move |i, on| toggle(&weak.unwrap(), &off, i as usize, on));
    }
    win.invoke_group_toggled(0, false);
    assert!(off.lock().unwrap().contains("card-a"));
    assert!(shown(&win, "Card A") > 0, "the checkbox stays");
    assert_eq!(shown(&win, "VRAM"), 0, "its card is gone at once");
    assert_eq!(shown(&win, "GTT"), 1, "the other device is untouched");

    // A sample taken before the click still carries Card A's bars: the set
    // wins, and the card stays hidden.
    push(&win, &two, &off.lock().unwrap());
    assert_eq!(shown(&win, "VRAM"), 0);
    assert!(!win.get_groups().row_data(0).unwrap().enabled);

    // Ticked again: the bars are back with the next sample.
    win.invoke_group_toggled(0, true);
    assert!(off.lock().unwrap().is_empty());
    push(&win, &two, &off.lock().unwrap());
    assert_eq!(shown(&win, "VRAM"), 1);
    win.hide().ok();
}
