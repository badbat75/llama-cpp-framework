//! One compute device as `llama-server --list-devices` reports it, and the
//! parser of that block: the id `--device` takes (`CUDA0`), the description,
//! the bare product name and the device memory in MiB.
//!
//! `pai-app` builds its device dropdowns and its GPU table on this, and the
//! manager's memory check counts the cards with it, so the parse of the
//! engine's text lives here, not in the configurator's crate.

use crate::strings;

/// One compute device, as the engine's `--list-devices` block reports it. The
/// GPU table needs the parts separately (the name in one column, the VRAM in
/// another, the free memory to explain llama.cpp's automatic split), so the
/// trailing `(12281 MiB, 10844 MiB free)` is broken out into `total_mib` and
/// `free_mib` rather than left baked into the label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceOption {
    /// The token llama.cpp expects after `--device` (e.g. `CUDA0`).
    pub id: String,
    /// Human-friendly description for the dropdown (e.g.
    /// `CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)`).
    pub label: String,
    /// The bare product name, with the VRAM parenthetical stripped (e.g.
    /// `NVIDIA GeForce RTX 4070 SUPER`). Empty when the line carried none.
    pub name: String,
    /// Total device memory in MiB; 0 when `--list-devices` didn't report it.
    pub total_mib: u64,
    /// Free device memory in MiB at probe time; 0 when unreported. llama.cpp's
    /// default (blank `--tensor-split`) splits a model in proportion to THIS.
    pub free_mib: u64,
}

impl DeviceOption {
    /// True for the CPU pseudo-device, which `--device` does not accept and the
    /// GPU table must skip.
    pub fn is_cpu(&self) -> bool {
        self.id.eq_ignore_ascii_case("CPU")
    }

    /// The VRAM column: `32.6 GB (32.4 free)`, or empty when unreported.
    pub fn vram_summary(&self) -> String {
        if self.total_mib == 0 {
            return String::new();
        }
        strings::fill(
            "vram-summary",
            &[
                &format!("{:.1}", self.total_mib as f64 / 1024.0),
                &format!("{:.1}", self.free_mib as f64 / 1024.0),
            ],
        )
    }
}

/// Parse the `--list-devices` block. Each device is an indented line shaped
/// `  CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)`. The
/// `Available devices:` header and any non-indented noise are ignored.
pub fn parse(s: &str) -> Vec<DeviceOption> {
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
            format!("{id}: {desc}")
        };
        let (name, total_mib, free_mib) = split_desc(desc);
        out.push(DeviceOption {
            id: id.to_string(),
            label,
            name,
            total_mib,
            free_mib,
        });
    }
    out
}

/// Split a device description into `(name, total_mib, free_mib)`.
/// `NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)` →
/// `("NVIDIA GeForce RTX 4070 SUPER", 12281, 10844)`. A description without the
/// trailing parenthetical (or with an unparseable one) keeps its whole text as
/// the name and reports 0/0; the table then shows a blank VRAM cell rather than
/// a wrong one.
fn split_desc(desc: &str) -> (String, u64, u64) {
    let Some(open) = desc.rfind('(') else {
        return (desc.to_string(), 0, 0);
    };
    let Some(inner) = desc[open + 1..].strip_suffix(')') else {
        return (desc.to_string(), 0, 0);
    };
    let name = desc[..open].trim().to_string();
    let mut parts = inner.split(',').map(|p| leading_u64(p.trim()));
    let total = parts.next().flatten();
    let free = parts.next().flatten();
    match total {
        Some(t) => (name, t, free.unwrap_or(0)),
        None => (desc.to_string(), 0, 0),
    }
}

/// The leading integer of `12281 MiB` / `10844 MiB free`. `None` when the part
/// doesn't start with digits.
fn leading_u64(s: &str) -> Option<u64> {
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
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
            .starts_with("CUDA0: NVIDIA GeForce RTX 4070 SUPER"));
    }

    #[test]
    fn parses_name_and_vram_out_of_the_description() {
        let devs = parse(SAMPLE);
        assert_eq!(devs[0].name, "NVIDIA GeForce RTX 4070 SUPER");
        assert_eq!(devs[0].total_mib, 12281);
        assert_eq!(devs[0].free_mib, 10844);
        // A name that itself contains parens: only the LAST one is the VRAM.
        assert_eq!(devs[1].name, "AMD Radeon(TM) Graphics");
        assert_eq!(devs[1].total_mib, 33593);
    }

    #[test]
    fn a_description_without_vram_keeps_its_whole_text_as_the_name() {
        let devs = parse("  SYCL0: Some Accelerator\n");
        assert_eq!(devs[0].name, "Some Accelerator");
        assert_eq!(devs[0].total_mib, 0);
        assert_eq!(devs[0].free_mib, 0);
        assert!(devs[0].vram_summary().is_empty());
    }

    #[test]
    fn vram_summary_renders_gib() {
        let devs = parse(SAMPLE);
        assert_eq!(devs[0].vram_summary(), "12.0 GB (10.6 free)");
    }

    #[test]
    fn cpu_is_flagged_so_the_gpu_table_can_skip_it() {
        let devs = parse("  CPU: AMD Ryzen 9 9900X (63090 MiB, 48233 MiB free)\n");
        assert!(devs[0].is_cpu());
        assert!(!parse(SAMPLE)[0].is_cpu());
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
}
