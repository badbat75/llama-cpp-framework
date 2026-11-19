//! One compute device as `llama-server --list-devices` reports it, and the
//! parser of that block: the id `--device` takes (`CUDA0`), the description,
//! the bare product name and the device memory in MiB. Beside the devices,
//! the PHYSICAL pool behind a probe: `gpu_pool` reduces the entries to the
//! discrete cards, one per card, and `pool_usable` adds up what an estimate
//! may fill on them.
//!
//! `pai-app` builds its device dropdowns and its GPU table on this, and the
//! manager's memory check counts the cards with it, so the parse of the
//! engine's text lives here, not in the configurator's crate.

use crate::gguf_shape::{GPU_RESERVE, MIB};
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

// ── The physical pool ────────────────────────────────────────────────────

/// One physical GPU: the `--device` id it is written as, the name the probe
/// reports, its memory.
#[derive(Debug, Clone, PartialEq)]
pub struct Gpu {
    pub id: String,
    pub name: String,
    pub total_mib: u64,
}

impl Gpu {
    /// What the estimate may fill: the card minus `GPU_RESERVE`.
    pub fn usable(&self) -> u64 {
        (self.total_mib * MIB).saturating_sub(GPU_RESERVE)
    }

    /// The name without the vendor words: `RTX 4070 SUPER`, `AI PRO R9700`.
    pub fn short_name(&self) -> String {
        const VENDOR: [&str; 6] = ["nvidia", "geforce", "amd", "radeon", "intel", "ati"];
        let words: Vec<&str> = self
            .name
            .split_whitespace()
            .filter(|w| {
                let bare = w
                    .trim_end_matches("(TM)")
                    .trim_end_matches("(R)")
                    .to_ascii_lowercase();
                !VENDOR.contains(&bare.as_str())
            })
            .collect();
        if words.is_empty() {
            self.name.clone()
        } else {
            words.join(" ")
        }
    }
}

/// A GPU whose memory is system RAM: an APU's integrated graphics. By name,
/// since `--list-devices` does not say (AMD reports `AMD Radeon(TM)
/// Graphics`, or a `780M`/`890M`/`8060S` style model; Intel its UHD, Iris and
/// bare "Arc Graphics" iGPUs).
pub fn is_integrated(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.contains("radeon") {
        // `680M` / `780M` / `890M`, and Strix Halo's `8060S` / `8050S` /
        // `8040S`; a laptop dGPU's `RX 6600M` has four digits and an RX.
        let model = n.split(|c: char| !c.is_ascii_alphanumeric()).any(|t| {
            let all_digits = |d: &str| d.chars().all(|c| c.is_ascii_digit());
            match (t.strip_suffix('m'), t.strip_suffix('s')) {
                (Some(d), _) => d.len() == 3 && all_digits(d),
                (_, Some(d)) => d.len() == 4 && d.starts_with("80") && all_digits(d),
                _ => false,
            }
        });
        return model || n.ends_with("radeon(tm) graphics") || n.ends_with("radeon graphics");
    }
    n.contains("intel")
        && (n.contains("uhd")
            || n.contains("iris")
            || n.contains("hd graphics")
            || n.ends_with("arc(tm) graphics")
            || n.ends_with("arc graphics"))
}

/// The physical discrete GPUs of a device probe: CPU entries and unsized
/// devices dropped, one card counted once across backends, integrated GPUs
/// dropped when a discrete one exists. Each card keeps the id and size of the
/// backend that sees the most cards of its name, a non-Vulkan one on a tie
/// (CUDA0, not Vulkan1). Probe order.
pub fn gpu_pool(devs: &[DeviceOption]) -> Vec<Gpu> {
    // name -> backends in probe order, each with its devices of that name
    type Seen<'a> = Vec<(String, Vec<&'a DeviceOption>)>;
    let mut by_name: Vec<(String, Seen)> = Vec::new();
    for d in devs.iter().filter(|d| !d.is_cpu() && d.total_mib > 0) {
        let name = if d.name.is_empty() {
            d.id.clone()
        } else {
            d.name.trim().to_string()
        };
        let key = name.to_ascii_lowercase();
        let entry = match by_name.iter().position(|e| e.0.to_ascii_lowercase() == key) {
            Some(i) => &mut by_name[i],
            None => {
                by_name.push((name, Vec::new()));
                by_name.last_mut().unwrap()
            }
        };
        let b = backend_of(&d.id);
        match entry.1.iter_mut().find(|(k, _)| *k == b) {
            Some((_, list)) => list.push(d),
            None => entry.1.push((b, vec![d])),
        }
    }
    let mut gpus: Vec<Gpu> = Vec::new();
    for (name, backends) in by_name {
        let pick = backends
            .iter()
            .max_by_key(|(b, list)| (list.len(), b != "vulkan"));
        if let Some((_, list)) = pick {
            gpus.extend(list.iter().map(|d| Gpu {
                id: d.id.clone(),
                name: name.clone(),
                total_mib: d.total_mib,
            }));
        }
    }
    if gpus.iter().any(|g| !is_integrated(&g.name)) {
        gpus.retain(|g| !is_integrated(&g.name));
    }
    gpus
}

/// `cuda` for `CUDA0`: a device id's backend, lowercase.
pub fn backend_of(id: &str) -> String {
    id.trim_end_matches(|c: char| c.is_ascii_digit())
        .to_ascii_lowercase()
}

/// What an estimate of the whole node is measured against: every card's usable
/// memory added up, a ceiling and not a capacity (a block never straddles two
/// cards).
pub fn pool_usable(gpus: &[Gpu]) -> u64 {
    gpus.iter().fold(0, |sum, g| sum.saturating_add(g.usable()))
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

    // ── The physical pool: `Gpu`, `gpu_pool` and `is_integrated` move here
    // from `pai-app`'s `hf_fit` (slice 10b-1, `pai-app` re-exports them), so
    // the manager's memory check counts the cards exactly as the estimate
    // does. The pool test moved with them, unchanged. ──

    fn dev(id: &str, name: &str, total: u64) -> DeviceOption {
        DeviceOption {
            id: id.into(),
            label: String::new(),
            name: name.into(),
            total_mib: total,
            free_mib: total,
        }
    }

    /// This repo's test machine, as `llama-server --list-devices` reports it.
    fn probe() -> Vec<DeviceOption> {
        vec![
            dev("CUDA0", "NVIDIA GeForce RTX 4070 SUPER", 12281),
            dev("ROCm0", "AMD Radeon(TM) Graphics", 37078),
            dev("ROCm1", "AMD Radeon AI PRO R9700", 32624),
            dev("Vulkan0", "AMD Radeon(TM) Graphics", 48754),
            dev("Vulkan1", "NVIDIA GeForce RTX 4070 SUPER", 11997),
            dev("Vulkan2", "AMD Radeon AI PRO R9700", 32624),
        ]
    }

    #[test]
    fn the_pool_counts_each_card_once_without_the_igpu() {
        let g = gpu_pool(&probe());
        assert_eq!(
            g,
            [
                Gpu {
                    id: "CUDA0".into(),
                    name: "NVIDIA GeForce RTX 4070 SUPER".into(),
                    total_mib: 12281
                },
                // ROCm1: the iGPU took ROCm0, and ROCm, not Vulkan2, names it.
                Gpu {
                    id: "ROCm1".into(),
                    name: "AMD Radeon AI PRO R9700".into(),
                    total_mib: 32624
                },
            ]
        );
        assert_eq!(g[0].short_name(), "RTX 4070 SUPER");
        assert_eq!(g[1].short_name(), "AI PRO R9700");
        // Two identical cards stay two.
        let twin = [
            dev("CUDA0", "RTX 3090", 24576),
            dev("CUDA1", "RTX 3090", 24576),
            dev("Vulkan0", "RTX 3090", 24000),
            dev("Vulkan1", "RTX 3090", 24000),
        ];
        let ids: Vec<String> = gpu_pool(&twin).into_iter().map(|g| g.id).collect();
        assert_eq!(ids, ["CUDA0", "CUDA1"]);
        // A card only Vulkan sees keeps its Vulkan id.
        assert_eq!(
            gpu_pool(&[dev("Vulkan0", "Intel(R) Arc(TM) A770 Graphics", 16000)])[0].id,
            "Vulkan0"
        );
        // An APU alone keeps its iGPU; the CPU never counts.
        let apu = [
            dev("Vulkan0", "AMD Radeon 8060S Graphics", 98304),
            dev("CPU", "CPU", 65536),
        ];
        assert_eq!(gpu_pool(&apu).len(), 1);
        for n in [
            "AMD Radeon(TM) Graphics",
            "AMD Radeon 780M Graphics",
            "AMD Radeon(TM) 890M",
            "Intel(R) UHD Graphics 770",
            "Intel(R) Arc(TM) Graphics",
        ] {
            assert!(is_integrated(n), "{n}");
        }
        for n in [
            "AMD Radeon AI PRO R9700",
            "AMD Radeon RX 7900 XTX",
            "Intel(R) Arc(TM) A770 Graphics",
            "NVIDIA GeForce RTX 4070 SUPER",
        ] {
            assert!(!is_integrated(n), "{n}");
        }
    }

    #[test]
    fn the_pool_leaves_the_reserve_on_every_card() {
        use crate::gguf_shape::{GPU_RESERVE, MIB};
        assert_eq!(pool_usable(&[]), 0, "no card, nothing usable");
        // The RTX seen by CUDA and Vulkan counts once (gpu_pool), the iGPU is
        // dropped: two cards, each with its own reserve.
        let gpus = gpu_pool(&probe());
        assert_eq!(
            pool_usable(&gpus),
            (12281 + 32624) * MIB - 2 * GPU_RESERVE,
            "the CUDA + Vulkan card is not counted twice"
        );
        // A card smaller than the reserve gives 0, never a negative: the pool
        // is its big card alone.
        let tiny = [
            Gpu {
                id: "CUDA0".into(),
                name: "NVIDIA GeForce RTX 4060".into(),
                total_mib: 1024,
            },
            Gpu {
                id: "ROCm0".into(),
                name: "AMD Radeon AI PRO R9700".into(),
                total_mib: 32624,
            },
        ];
        assert_eq!(pool_usable(&tiny), 32624 * MIB - GPU_RESERVE);
    }
}
