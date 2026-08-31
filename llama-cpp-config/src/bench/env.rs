//! The environment stamp a benchmark report carries.
//!
//! WHY IT EXISTS, and it is not bookkeeping: on 2026-08-24 a full day of GPU
//! measurements on this machine turned out to be worthless because Windows
//! Update had installed display drivers three times underneath them. Nothing in
//! the numbers said so. Two runs that disagree are only interpretable if you can
//! tell whether the machine was the same machine, so every report records what
//! it ran on and a comparison starts by diffing those two blocks.
//!
//! WHAT IS RECORDED, and why these three. The **boot time** answers "did the
//! machine restart between these runs", which matters because a driver install
//! only takes effect after a reboot and because the WDDM state this box's VRAM
//! eviction depends on is reset by one. The **display adapters' driver version
//! and date** answer "is it still the same driver", which is the variable that
//! silently invalidated that day. The **llama-server / llama-bench build** is
//! recorded by the caller alongside these, so the software half is covered too.
//!
//! WHAT IS DELIBERATELY NOT RECORDED: the Windows Update event log. The old
//! shell harness scanned it for driver activity in the last 24 hours, which
//! needs an event-log API this crate has no binding for, and it answered a
//! question the pair (boot time, driver version) already answers when two
//! reports are compared: a driver that changed shows up as a different version,
//! and one installed but not yet in effect shows up as a boot older than the
//! install. Recording the facts beats recording an inference about them.
//!
//! Everything here is best effort: a value that cannot be read is left out
//! rather than guessed, because a wrong environment stamp is worse than none.

/// The display-adapter class key. Windows numbers the adapters `0000`, `0001`,
/// … under it, each with `DriverDesc` / `DriverVersion` / `DriverDate`.
#[cfg(windows)]
const DISPLAY_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

/// How many adapter subkeys to probe. Enumerating them properly would need
/// `RegEnumKeyExW`, a second FFI binding for a list that is never long: a
/// machine with more than eight display adapters is not one this framework
/// runs on.
#[cfg(windows)]
const MAX_ADAPTERS: u32 = 8;

/// Seconds since the machine booted, or `None` when the call is unavailable.
#[cfg(windows)]
fn uptime_secs() -> Option<u64> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetTickCount64() -> u64;
    }
    // Milliseconds since boot, monotonic and unaffected by clock changes, which
    // is exactly what makes it a better boot marker than a wall-clock reading.
    Some(unsafe { GetTickCount64() } / 1000)
}

#[cfg(not(windows))]
fn uptime_secs() -> Option<u64> {
    None
}

/// The display adapters as `(description, version, date)`, in registry order.
#[cfg(windows)]
fn adapters() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for i in 0..MAX_ADAPTERS {
        let sub = format!("{DISPLAY_CLASS}\\{i:04}");
        // No DriverDesc means the slot is empty (or not an adapter): skip it
        // rather than emitting a row of blanks.
        let Some(desc) = crate::startup::machine_reg_sz(&sub, "DriverDesc") else {
            continue;
        };
        // Windows registers a Remote Display Adapter per RDP session slot, six
        // of them on this box, all with the same OS-version "driver". They are
        // not hardware and would bury the two rows that matter.
        if desc.contains("Remote Display") {
            continue;
        }
        let ver = crate::startup::machine_reg_sz(&sub, "DriverVersion").unwrap_or_default();
        let date = crate::startup::machine_reg_sz(&sub, "DriverDate").unwrap_or_default();
        out.push((desc, ver, date));
    }
    out
}

#[cfg(not(windows))]
fn adapters() -> Vec<(String, String, String)> {
    Vec::new()
}

/// The stamp as `(label, value)` rows for the report's Environment table.
/// `now_secs` is passed in rather than read here so the boot-time arithmetic is
/// testable and so the whole report shares one clock reading.
pub fn stamp(now_secs: u64) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if let Some(up) = uptime_secs() {
        // Saturating: a clock that has been moved backwards must not wrap into
        // a boot time in the far future.
        rows.push((
            "booted".to_string(),
            format!(
                "{} ({})",
                super::stamp_human(now_secs.saturating_sub(up)),
                human_uptime(up)
            ),
        ));
    }
    for (desc, ver, date) in adapters() {
        let mut v = ver;
        if !date.is_empty() {
            v = format!("{v}, dated {date}");
        }
        rows.push((format!("display driver ({desc})"), v));
    }
    rows
}

/// `3 d 4 h`, `4 h 12 m`, `12 m`: enough to see at a glance whether two runs
/// sit on the same session without reading two timestamps.
fn human_uptime(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, (secs % 86_400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("up {d} d {h} h")
    } else if h > 0 {
        format!("up {h} h {m} m")
    } else {
        format!("up {m} m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptime_reads_as_days_hours_minutes() {
        assert_eq!(human_uptime(0), "up 0 m");
        assert_eq!(human_uptime(59), "up 0 m");
        assert_eq!(human_uptime(3 * 60), "up 3 m");
        assert_eq!(human_uptime(4 * 3600 + 12 * 60), "up 4 h 12 m");
        assert_eq!(human_uptime(3 * 86_400 + 4 * 3600), "up 3 d 4 h");
    }

    // A clock moved backwards behind a monotonic uptime must not produce a boot
    // time in the future; the report would then read as nonsense rather than as
    // an unknown.
    #[test]
    fn a_backwards_clock_cannot_produce_a_future_boot_time() {
        let rows = stamp(0);
        for (label, value) in &rows {
            if label == "booted" {
                assert!(
                    value.starts_with("1970-01-01"),
                    "saturated to the epoch, not wrapped: {value}"
                );
            }
        }
    }
}
