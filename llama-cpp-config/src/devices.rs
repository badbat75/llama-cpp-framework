// Enumerate GPU / compute devices by parsing `llama-server.exe --list-devices`,
// so the GUI can offer a dropdown of the *actually available* backends
// (CUDA0, Vulkan0, …) instead of a free-text field.

use std::sync::RwLock;

use crate::paths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceOption {
    /// The token llama.cpp expects after `--device` (e.g. `CUDA0`).
    pub id: String,
    /// Human-friendly description for the dropdown (e.g.
    /// `CUDA0 — NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)`).
    pub label: String,
}

/// The probe cache: `gui::spawn_device_probe` runs `list()` off the UI thread
/// (a few hundred ms — it spawns llama-server) and parks the result here; the
/// UI thread rebuilds its dropdowns from `probed()` without re-probing. A
/// plain Rust cache instead of Slint properties because the device list is
/// Rust-only data — no `.slint` file reads it.
static PROBED: RwLock<Vec<DeviceOption>> = RwLock::new(Vec::new());

/// Publish a probe's result. Replaces the previous list — the GUI re-probes
/// on Refresh/F5, e.g. after llama.cpp was rebuilt with a different backend.
pub fn set_probed(devs: Vec<DeviceOption>) {
    *PROBED.write().unwrap() = devs;
}

/// The cached probe result; empty until the first probe lands (the dropdowns
/// then show just the "(default)" entry plus any custom value).
pub fn probed() -> Vec<DeviceOption> {
    PROBED.read().unwrap().clone()
}

/// Spawn `llama-server --list-devices` and return the parsed device list.
/// Returns an empty vec when llama-server is missing or the call fails — the
/// GUI then falls back to just the "(default)" entry plus any custom value.
pub fn list() -> Vec<DeviceOption> {
    let Some(exe) = paths::llama_server_exe() else {
        return Vec::new();
    };
    match run(&exe) {
        Some(out) => parse(&out),
        None => Vec::new(),
    }
}

fn run(exe: &std::path::Path) -> Option<String> {
    let output = crate::proc::run_hidden(exe, ["--list-devices"])?;
    Some(crate::proc::combined_output(&output))
}

/// Parse the `--list-devices` block. Each device is an indented line shaped
/// `  CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)`.
/// The `Available devices:` header and any non-indented noise are ignored.
pub(crate) fn parse(s: &str) -> Vec<DeviceOption> {
    let mut out: Vec<DeviceOption> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in s.lines() {
        // Device entries are indented; the header line is not.
        if line.is_empty() || !line.starts_with([' ', '\t']) {
            continue;
        }
        let trimmed = line.trim();
        let Some((id, desc)) = trimmed.split_once(':') else {
            continue;
        };
        let id = id.trim();
        // A real device id is a single token like CUDA0 / Vulkan1 / SYCL0 / CPU.
        if id.is_empty() || id.contains(char::is_whitespace) {
            continue;
        }
        if !seen.insert(id.to_ascii_lowercase()) {
            continue;
        }
        let desc = desc.trim();
        let label = if desc.is_empty() {
            id.to_string()
        } else {
            format!("{id} — {desc}")
        };
        out.push(DeviceOption {
            id: id.to_string(),
            label,
        });
    }
    out
}

/// Build the `(labels, values, selected_index)` triple for a device combobox.
/// Index 0 is always an empty-valued "default" entry (`empty_label`); a current
/// value that isn't among the detected devices is preserved as a `(custom)` row
/// so a stale or hand-edited id never silently disappears.
pub fn build_options(
    devices: &[DeviceOption],
    current: &str,
    empty_label: &str,
) -> (Vec<String>, Vec<String>, i32) {
    let mut labels = vec![empty_label.to_string()];
    let mut values = vec![String::new()];
    for d in devices {
        labels.push(d.label.clone());
        values.push(d.id.clone());
    }

    let cur = current.trim();
    if cur.is_empty() {
        return (labels, values, 0);
    }
    if let Some(i) = values.iter().position(|v| v.eq_ignore_ascii_case(cur)) {
        return (labels, values, i as i32);
    }
    labels.insert(1, format!("(custom) {cur}"));
    values.insert(1, cur.to_string());
    (labels, values, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Available devices:\n  \
        CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)\n  \
        Vulkan0: AMD Radeon(TM) Graphics (33593 MiB, 31913 MiB free)\n  \
        Vulkan1: NVIDIA GeForce RTX 4070 SUPER (11997 MiB, 10844 MiB free)\n";

    #[test]
    fn parses_device_ids_and_labels() {
        let devs = parse(SAMPLE);
        assert_eq!(devs.len(), 3);
        assert_eq!(devs[0].id, "CUDA0");
        assert_eq!(devs[1].id, "Vulkan0");
        assert_eq!(devs[2].id, "Vulkan1");
        assert!(devs[0]
            .label
            .starts_with("CUDA0 — NVIDIA GeForce RTX 4070 SUPER"));
    }

    #[test]
    fn ignores_header_and_blank_lines() {
        assert!(parse("Available devices:\n\n").is_empty());
        assert!(parse("").is_empty());
    }

    #[test]
    fn skips_non_indented_and_multiword_keys() {
        // A non-indented "X: y" (e.g. a banner) must not be taken as a device.
        assert!(parse("ggml: using CUDA backend\n").is_empty());
        // Indented but the key has spaces → not a device token.
        assert!(parse("  load time: 5 ms\n").is_empty());
    }

    #[test]
    fn dedups_repeated_ids() {
        let devs = parse("  CUDA0: a\n  CUDA0: b\n");
        assert_eq!(devs.len(), 1);
    }

    #[test]
    fn build_options_default_when_empty() {
        let devs = parse(SAMPLE);
        let (labels, values, idx) = build_options(&devs, "", "(default)");
        assert_eq!(idx, 0);
        assert_eq!(values[0], "");
        assert_eq!(labels[0], "(default)");
        assert_eq!(values.len(), 4); // default + 3 devices
    }

    #[test]
    fn build_options_selects_matching_device() {
        let devs = parse(SAMPLE);
        let (_labels, values, idx) = build_options(&devs, "Vulkan0", "(default)");
        assert_eq!(values[idx as usize], "Vulkan0");
    }

    #[test]
    fn build_options_matches_case_insensitively() {
        let devs = parse(SAMPLE);
        let (_labels, values, idx) = build_options(&devs, "cuda0", "(default)");
        assert_eq!(values[idx as usize], "CUDA0");
    }

    #[test]
    fn build_options_preserves_stale_value_as_custom() {
        let devs = parse(SAMPLE);
        let (labels, values, idx) = build_options(&devs, "SYCL3", "(default)");
        assert_eq!(idx, 1);
        assert_eq!(values[1], "SYCL3");
        assert!(labels[1].starts_with("(custom)"));
    }
}
