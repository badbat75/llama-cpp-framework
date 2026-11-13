//! HW Monitor window wiring: `HwWindow` (ui/hw_window.slint) is an independent
//! Slint root beside `AppWindow`/`AppTray`/`LogWindow`: non-modal, and like
//! them it does NOT use `AppState`; Rust pushes its one property directly.
//!
//! The sampling (`hwmon::Sampler`) runs on a worker thread, not on a
//! `slint::Timer` as the log tail does: NVML measures PCIe throughput over
//! 20 ms per call, which would stall the event loop once a second per NVIDIA
//! card (and on Windows the first reading of the GPU counters takes its time
//! too). The thread exists only while the window is open: the click that
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

/// `ROCm1: 7.13.1`: a device id and the version of the runtime behind it.
fn sdk_line(id: &str, sdk: Option<&devices::Sdk>) -> String {
    match sdk.and_then(|s| s.version.as_deref()) {
        Some(version) => format!("{id}: {version}"),
        None => id.to_string(),
    }
}

/// A cache size in the unit that keeps it short.
fn cache_text(kib: u64) -> String {
    if kib >= 1024 {
        format!("{} MiB", kib / 1024)
    } else {
        format!("{kib} KiB")
    }
}

/// One socket's facts, one per line. The hint sits on a socket's row and the
/// sources count the machine, so the counts are divided by the sockets: the
/// processors of a multi-socket machine are of one model.
fn cpu_hint(cpu: &devices::Cpu) -> String {
    let sockets = cpu.sockets.unwrap_or(1).max(1);
    let each = |n: Option<u32>| n.map(|n| n / sockets);
    let mut lines = Vec::new();
    if let Some(socket) = &cpu.socket {
        lines.push(strings::fill("hw-cpu-socket", &[socket]));
    }
    let counts = [
        (strings::key("hw-cpu-cores"), each(cpu.cores)),
        (strings::key("hw-cpu-threads"), each(cpu.threads)),
        (strings::key("hw-cpu-base-clock"), cpu.base_mhz),
        (strings::key("hw-cpu-max-clock"), cpu.max_mhz),
    ];
    for (key, value) in counts {
        if let Some(value) = value {
            lines.push(strings::fill(key, &[&value]));
        }
    }
    for (name, kib) in [("L1", cpu.l1_kib), ("L2", cpu.l2_kib), ("L3", cpu.l3_kib)] {
        if let Some(kib) = kib {
            lines.push(format!("{name}: {}", cache_text(kib / sockets as u64)));
        }
    }
    if let Some(nodes) = each(cpu.numa_nodes) {
        lines.push(strings::fill("hw-cpu-numa-nodes", &[&nodes]));
    }
    if !cpu.instructions.is_empty() {
        lines.push(strings::fill(
            "hw-cpu-instructions",
            &[&cpu.instructions.join(", ")],
        ));
    }
    lines.join("\n")
}

/// The memory modules, one per line. A module's name is on the card's title, so
/// it is repeated here only when the modules differ.
fn memory_hint(modules: &[devices::MemoryModule]) -> String {
    let same_model = modules.windows(2).all(|m| m[0].model() == m[1].model());
    modules
        .iter()
        .map(|m| {
            let size = format!("{:.1} GiB", m.bytes as f64 / (1u64 << 30) as f64);
            let mut parts = vec![size];
            parts.extend(m.kind.clone());
            if !same_model {
                parts.push(m.model());
            }
            parts.extend(
                m.configured_mts
                    .or(m.rated_mts)
                    .map(|mts| format!("{mts} MT/s")),
            );
            if let (Some(_), Some(rated)) = (m.configured_mts, m.rated_mts) {
                if m.configured_mts != m.rated_mts {
                    parts.push(strings::fill("hw-memory-rated", &[&rated]));
                }
            }
            parts.extend(
                m.configured_millivolts
                    .map(|mv| format!("{:.2} V", mv as f64 / 1000.0)),
            );
            let text = parts.join(" · ");
            if m.slot.is_empty() {
                text
            } else {
                format!("{}: {text}", m.slot)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A device's facts behind its info glyph, one per line; the ones nobody could
/// read are left out, and with none there is no glyph.
fn hint_text(g: &hwmon::Group) -> String {
    if hwmon::is_cpu_id(&g.id) {
        return devices::cpu().map_or(String::new(), |cpu| cpu_hint(&cpu));
    }
    if g.id == hwmon::MEMORY_ID {
        return memory_hint(&devices::memory());
    }
    let Some(gpu) = &g.gpu else {
        return String::new();
    };
    let gib = |mib: u64| format!("{:.1}", mib as f64 / 1024.0);
    let sdks = devices::sdks();
    let mut lines: Vec<String> = gpu
        .sdk_ids
        .iter()
        .map(|id| {
            let of = sdks.iter().find(|s| s.name == hwmon::sdk::backend(id));
            sdk_line(id, of)
        })
        .collect();
    if gpu.vram_mib > 0 {
        lines.push(format!("VRAM: {} GiB", gib(gpu.vram_mib)));
    }
    if gpu.shared_mib > 0 {
        lines.push(strings::fill("gpu-shared-memory", &[&gib(gpu.shared_mib)]));
    }
    if let Some(link) = gpu.link {
        lines.push(strings::fill("gpu-link", &[&link]));
    }
    if let Some(max) = gpu.max_link {
        lines.push(strings::fill("gpu-link-max", &[&max]));
    }
    if let Some(switches) = gpu.switches {
        lines.push(strings::fill("gpu-switches", &[&switches]));
    }
    lines.push(format!("PCI: {}", gpu.pci));
    if !gpu.pci_id.is_empty() {
        lines.push(format!("PCI ID: {}", gpu.pci_id));
    }
    if let Some(driver) = &gpu.driver {
        lines.push(strings::fill("gpu-driver", &[driver]));
    }
    lines.join("\n")
}

fn device_row(g: &hwmon::Group, enabled: bool) -> HwDevice {
    let bars = if enabled { g.bars.as_slice() } else { &[] };
    HwDevice {
        id: SharedString::from(g.id.as_str()),
        detail: SharedString::from(g.detail.as_str()),
        hint: SharedString::from(hint_text(g)),
        enabled,
        bars: model(bars.iter().map(bar_row).collect()),
    }
}

/// The window's boxes: the devices of one name together, each box where the
/// first of them came. Inside a box they run by their detail, which starts
/// with the SDK id where there is one, so ROCm1 sits above ROCm2 whatever the
/// PCI order.
fn boxed(groups: &[hwmon::Group]) -> Vec<Vec<&hwmon::Group>> {
    let mut boxes: Vec<Vec<&hwmon::Group>> = Vec::new();
    for g in groups {
        match boxes.iter_mut().find(|b| b[0].title == g.title) {
            Some(same_name) => same_name.push(g),
            None => boxes.push(vec![g]),
        }
    }
    for b in &mut boxes {
        b.sort_by(|x, y| x.detail.cmp(&y.detail));
    }
    boxes
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
    let boxes = boxed(groups);
    let shown = win.get_groups();
    let same_shape = shown.row_count() == boxes.len()
        && shown.iter().zip(&boxes).all(|(row, devices)| {
            row.title == devices[0].title.as_str()
                && row.devices.row_count() == devices.len()
                && row.devices.iter().zip(devices).all(|(device, g)| {
                    device.id == g.id.as_str()
                        && device.detail == g.detail.as_str()
                        && device.hint == hint_text(g).as_str()
                        && device.enabled == enabled(g)
                        && device.bars.row_count() == bar_count(g)
                })
        });
    if same_shape {
        for (row, devices) in shown.iter().zip(&boxes) {
            for (device, g) in row.devices.iter().zip(devices) {
                for (i, b) in g.bars.iter().take(bar_count(g)).enumerate() {
                    let new = bar_row(b);
                    if device.bars.row_data(i).as_ref() != Some(&new) {
                        device.bars.set_row_data(i, new);
                    }
                }
            }
        }
        return;
    }
    win.set_groups(model(
        boxes
            .iter()
            .map(|devices| HwGroup {
                title: SharedString::from(devices[0].title.as_str()),
                devices: model(devices.iter().map(|g| device_row(g, enabled(g))).collect()),
            })
            .collect(),
    ));
}

/// A device's checkbox was clicked. The set is what stops the sampler; the
/// row is rewritten here as well so the bars go (or the empty row comes back)
/// at once, not at the next sample.
fn toggle(win: &HwWindow, off: &Off, id: &str, on: bool) {
    {
        let mut off = off.lock().unwrap_or_else(|e| e.into_inner());
        if on {
            off.remove(id);
        } else {
            off.insert(id.to_string());
        }
    }
    for row in win.get_groups().iter() {
        if let Some(i) = row.devices.iter().position(|d| d.id == id) {
            if let Some(mut device) = row.devices.row_data(i) {
                device.enabled = on;
                device.bars = model(Vec::new());
                row.devices.set_row_data(i, device);
            }
            return;
        }
    }
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
    install_strings(win.global::<Strings>());
    let live = Arc::new(AtomicU64::new(0));
    restore_window_size(&win);
    let off: Off = Arc::new(Mutex::new(
        settings::load().hw_monitor_hidden.into_iter().collect(),
    ));

    // Closing hides (not destroys) and retires the sampler thread.
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
        let off = off.clone();
        win.on_device_toggled(move |id, on| {
            if let Some(win) = win_weak.upgrade() {
                toggle(&win, &off, &id, on);
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
            // window sits minimized/behind must surface it (gui::raise_window).
            raise_window(win.window());
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
                    let mut groups = sampler.sample(&now_off());
                    hwmon::name_sdk_ids(&mut groups, &devices::probed());
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
/// animation never runs), a changed device set rebuilds, two devices of one
/// name share a box, and a device's checkbox hides its bars and holds against
/// a sample already on its way.
#[cfg(test)]
pub(super) fn e2e() {
    use i_slint_backend_testing::ElementHandle;

    let group = |title: &str, bars: &[(&str, &str, f32)]| hwmon::Group {
        id: title.to_lowercase().replace(' ', "-"),
        title: title.to_string(),
        detail: format!("{title} spec"),
        gpu: None,
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
    install_strings(win.global::<Strings>());
    win.window().set_size(slint::PhysicalSize::new(800, 1200));
    win.show().expect("realize window");
    assert_eq!(shown(&win, "Sampling…"), 1, "the empty state");

    let a1 = [group(
        "Card A",
        &[("VRAM", "1.0 / 2.0 GiB", 0.5), ("GPU", "10 %", 0.1)],
    )];
    push(&win, &a1, &none);
    assert_eq!(shown(&win, "Sampling…"), 0);
    // (A specification is a CheckBox, which the accessibility tree lists
    // together with its own inner text: present, not "once".)
    assert!(shown(&win, "Card A spec") > 0);
    assert_eq!(shown(&win, "Card A"), 1, "the box's heading");
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
    let vram = before
        .row_data(0)
        .unwrap()
        .devices
        .row_data(0)
        .unwrap()
        .bars
        .row_data(0)
        .unwrap();
    assert_eq!(vram.fraction, 0.75);

    let two = [
        group("Card A", &[("VRAM", "1.5 / 2.0 GiB", 0.75)]),
        group("Card B", &[("GTT", "0.1 / 2.0 GiB", 0.05)]),
    ];
    push(&win, &two, &none);
    assert!(win.get_groups() != before, "another shape: rebuilt");
    assert!(shown(&win, "Card B spec") > 0);
    assert_eq!(shown(&win, "10 %"), 0);

    // Untick Card A through the window's own callback, wired as `wire` does
    // minus `persist` (settings.ini is settings.rs's to test).
    let off = Off::default();
    {
        let (weak, off) = (win.as_weak(), off.clone());
        win.on_device_toggled(move |id, on| toggle(&weak.unwrap(), &off, &id, on));
    }
    win.invoke_device_toggled("card-a".into(), false);
    assert!(off.lock().unwrap().contains("card-a"));
    assert!(shown(&win, "Card A spec") > 0, "the checkbox stays");
    assert_eq!(shown(&win, "Card A"), 1, "and so does the box");
    assert_eq!(shown(&win, "VRAM"), 0, "its bars are gone at once");
    assert_eq!(shown(&win, "GTT"), 1, "the other device is untouched");

    // A sample taken before the click still carries Card A's bars: the set
    // wins, and the bars stay hidden.
    push(&win, &two, &off.lock().unwrap());
    assert_eq!(shown(&win, "VRAM"), 0);
    let row = win.get_groups().row_data(0).unwrap();
    assert!(!row.devices.row_data(0).unwrap().enabled);

    // Ticked again: the bars are back with the next sample.
    win.invoke_device_toggled("card-a".into(), true);
    assert!(off.lock().unwrap().is_empty());
    push(&win, &two, &off.lock().unwrap());
    assert_eq!(shown(&win, "VRAM"), 1);

    // Two cards of one name: one box, one heading, a checkbox and bars each,
    // ordered by specification whatever order they came in; one can go alone.
    let card = |id: &str, detail: &str, text: &str| hwmon::Group {
        id: id.to_string(),
        title: "R9700".to_string(),
        detail: detail.to_string(),
        gpu: Some(devices::PhysicalGpu {
            pci: id.to_string(),
            vram_mib: 32624,
            link: Some(devices::PcieLink {
                generation: 5,
                width: 16,
            }),
            switches: Some(3),
            ..Default::default()
        }),
        enabled: true,
        bars: vec![hwmon::Bar {
            label: "VRAM".to_string(),
            text: text.to_string(),
            fraction: 0.5,
        }],
    };
    let twins = [
        card("gpu-03", "ROCm2 · AMD · PCI 03:00.0", "2 GiB"),
        card("gpu-14", "ROCm1 · AMD · PCI 14:00.0", "1 GiB"),
    ];
    push(&win, &twins, &off.lock().unwrap());
    let boxes = win.get_groups();
    assert_eq!(boxes.row_count(), 1);
    assert_eq!(shown(&win, "R9700"), 1);
    let ids: Vec<String> = boxes
        .row_data(0)
        .unwrap()
        .devices
        .iter()
        .map(|d| d.id.to_string())
        .collect();
    assert_eq!(ids, ["gpu-14", "gpu-03"]);
    assert_eq!(shown(&win, "VRAM"), 2);

    // The info glyph's text is the card's facts, and it never says a link is
    // the whole path when only the card's own end could be read.
    let hint = boxes
        .row_data(0)
        .unwrap()
        .devices
        .row_data(0)
        .unwrap()
        .hint
        .to_string();
    assert_eq!(
        hint.lines().collect::<Vec<_>>(),
        [
            "VRAM: 31.9 GiB",
            "PCIe link: 5.0 x16",
            "PCIe switches to the CPU: 3",
            "PCI: gpu-14",
        ]
    );
    assert!(hint_text(&group("CPU", &[])).is_empty(), "no GPU, no glyph");
    let sdk = devices::Sdk {
        name: "ROCm".to_string(),
        library: "amdhip64_7.dll".to_string(),
        version: Some("7.13.1".to_string()),
        built_with: None,
    };
    assert_eq!(sdk_line("ROCm1", Some(&sdk)), "ROCm1: 7.13.1");
    assert_eq!(sdk_line("ROCm1", None), "ROCm1");
    let cpu = devices::Cpu {
        socket: Some("AM5".to_string()),
        threads: Some(24),
        l2_kib: Some(12288),
        l1_kib: Some(960),
        instructions: vec!["AVX2", "AVX-512"],
        ..Default::default()
    };
    assert_eq!(
        cpu_hint(&cpu).lines().collect::<Vec<_>>(),
        [
            "Socket: AM5",
            "Threads: 24",
            "L1: 960 KiB",
            "L2: 12 MiB",
            "Instructions: AVX2, AVX-512"
        ]
    );
    let two_sockets = devices::Cpu {
        sockets: Some(2),
        threads: Some(48),
        l3_kib: Some(128 << 10),
        ..Default::default()
    };
    assert_eq!(cpu_hint(&two_sockets), "Threads: 24\nL3: 64 MiB");
    let module = |part: &str, rated: u32| devices::MemoryModule {
        maker: "CORSAIR".to_string(),
        part: part.to_string(),
        slot: "P0 CHANNEL A".to_string(),
        bytes: 32 << 30,
        kind: Some("DDR5".to_string()),
        configured_mts: Some(6000),
        rated_mts: Some(rated),
        configured_millivolts: Some(1100),
    };
    let twins = [module("A", 6000), module("A", 6000)];
    assert_eq!(
        memory_hint(&twins).lines().next(),
        Some("P0 CHANNEL A: 32.0 GiB · DDR5 · 6000 MT/s · 1.10 V")
    );
    assert_eq!(
        memory_hint(&[module("A", 6400), module("B", 6000)])
            .lines()
            .next(),
        Some("P0 CHANNEL A: 32.0 GiB · DDR5 · CORSAIR A · 6000 MT/s · rated 6400 MT/s · 1.10 V")
    );

    win.invoke_device_toggled("gpu-14".into(), false);
    assert_eq!((shown(&win, "1 GiB"), shown(&win, "2 GiB")), (0, 1));
    win.hide().ok();
}
