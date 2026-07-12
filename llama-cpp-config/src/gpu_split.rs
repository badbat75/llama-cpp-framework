//! The GPU distribution table: which devices a model runs on, and in what
//! proportion. Pure logic (no IO, no Slint state) so every rule below is unit
//! tested; `gui::refresh_gpu_rows` is the thin shell that pushes the rows into
//! `AppState` and writes the result back into the form.
//!
//! ## Why a table and not two text fields
//! `--tensor-split` is a positional vector indexed over the **filtered** device
//! list — the devices named by `--device`, in `--device` order (llama.cpp's
//! `llama-model.cpp` copies `tensor_split[0..n_devices()]` over `model->devices`,
//! and `docs/multi-gpu.md`: *"The values follow the order in --device"*). With
//! `--device` unset that list is every detected backend, which on a mixed box is
//! CUDA0 + two ROCm + three duplicate Vulkan views of the same three GPUs — so a
//! weight vector typed by hand is indexed against a list the user can't see and
//! didn't choose. The two fields only make sense together, which is what this
//! module models: one selection, rendered into the `device` + `tensor_split`
//! strings that both `server.ini` and `presets.ini` already store.
//!
//! ## The four states (mirroring llama.cpp exactly)
//! | selected | `device`        | `tensor_split` | meaning                                       |
//! |----------|-----------------|----------------|-----------------------------------------------|
//! | 0        | `""`            | `""`           | llama.cpp uses all detected devices           |
//! | 1        | `"ROCm1"`       | `""`           | one GPU — nothing to split                    |
//! | ≥2 auto  | `"ROCm1,CUDA0"` | `""`           | llama.cpp splits by **free** VRAM at load     |
//! | ≥2 fixed | `"ROCm1,CUDA0"` | `"3,1"`        | explicit proportions                          |
//!
//! Note the third row: a blank `tensor_split` is *auto-by-free-VRAM*, NOT an even
//! split (llama.cpp's `all_zero` branch fills `splits[i]` with each device's free
//! memory). That's why the table offers **Auto** (clear) and **Even** (all 1s) as
//! two distinct buttons — they are two different launches.
//!
//! The selection is never held as separate state: it is derived from the form's
//! two strings on every rebuild and rendered straight back into them, so a
//! hand-edited INI stays authoritative and there is no third copy to desync.
//!
//! ## One order, everywhere: the probe's
//! Rows are ALWAYS listed in `--list-devices` order, checked or not, and every
//! edit re-sorts the `device` list into that same order (`canonical`). Two reasons,
//! and the first is not cosmetic:
//!
//! 1. **Rows must not move under the cursor.** An earlier cut listed the checked
//!    devices first, so each click re-sorted the table — and the next click landed
//!    on whatever row had slid into that spot. That is how a model got pinned to
//!    `ROCm0` (an iGPU that cannot run inference: the Windows ROCm build ships no
//!    rocBLAS kernels for those archs, so llama-server dies with an access
//!    violation on the warmup matmul) when the user was aiming at `ROCm1`.
//! 2. It makes the display order the split order: the checked rows, read top to
//!    bottom, ARE the `--device` list and the `--tensor-split` vector.
//!
//! The cost is that a hand-written `device = CUDA0,ROCm1` is normalized back to
//! probe order the first time the table is touched (weights travel with their
//! device, so the split is preserved — only which GPU is "first" can change).

use slint::SharedString;

use crate::devices::DeviceOption;
use crate::gui::GpuRow;

/// The two INI strings the table drives, always produced as a pair.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GpuSelection {
    /// `--device` value: comma-separated ids in split order, or empty.
    pub device: String,
    /// `--tensor-split` value: comma-separated weights parallel to `device`, or
    /// empty for "let llama.cpp decide".
    pub tensor_split: String,
}

/// A selected device and its weight. Weight 0 means "unweighted" — when EVERY
/// selected device is 0 the selection is in auto mode and renders a blank
/// `tensor_split`.
type Pick = (String, i32);

// ── String ↔ selection ───────────────────────────────────────────────────

/// `"ROCm1, CUDA0"` → `["ROCm1", "CUDA0"]`. Blank entries are dropped, so a
/// trailing comma or a hand-typed `"ROCm1,"` doesn't produce a phantom device.
pub fn parse_device_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// `"3,1"` → `[3, 1]`. A part that isn't a non-negative integer reads as 0 (the
/// same thing llama.cpp's own `std::stof` fallback would end up with), so a
/// malformed hand-edit degrades to "auto" rather than being rejected.
pub fn parse_weights(s: &str) -> Vec<i32> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<i32>().unwrap_or(0).max(0))
        .collect()
}

/// The selected devices with their weights, in split order. A device without a
/// matching weight (shorter `tensor_split`, or a blank one) gets 0 = unweighted.
fn picks(sel: &GpuSelection) -> Vec<Pick> {
    let weights = parse_weights(&sel.tensor_split);
    parse_device_list(&sel.device)
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id, weights.get(i).copied().unwrap_or(0)))
        .collect()
}

/// Sort picks into probe order (see the module header) and render them back into
/// the INI string pair — the one place the four-state table above is enforced.
/// A stable sort, so ids the probe doesn't know (all keyed `usize::MAX`) keep
/// their relative order at the end instead of shuffling. `tensor_split` collapses
/// to empty for fewer than two devices (nothing to split) and for an all-zero
/// weight vector (auto).
fn canonical(devices: &[DeviceOption], mut picks: Vec<Pick>) -> GpuSelection {
    picks.sort_by_key(|(id, _)| probe_index(devices, id));
    let device = picks
        .iter()
        .map(|(id, _)| id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let auto = picks.len() < 2 || picks.iter().all(|&(_, w)| w <= 0);
    let tensor_split = if auto {
        String::new()
    } else {
        picks
            .iter()
            .map(|&(_, w)| w.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    GpuSelection {
        device,
        tensor_split,
    }
}

/// A device's position in the probe, or `usize::MAX` when it isn't in it.
fn probe_index(devices: &[DeviceOption], id: &str) -> usize {
    devices
        .iter()
        .position(|d| d.id.eq_ignore_ascii_case(id) && !d.is_cpu())
        .unwrap_or(usize::MAX)
}

// ── Edits (each returns the new selection; the caller writes it to the form) ──

/// Check / uncheck a device. A newly checked one starts at weight 1 when the
/// others are already weighted — adding it at 0 would silently give the new GPU
/// no layers at all, which is never what checking a box means.
pub fn toggle(devices: &[DeviceOption], sel: &GpuSelection, id: &str) -> GpuSelection {
    let mut picks = picks(sel);
    match picks.iter().position(|(d, _)| d.eq_ignore_ascii_case(id)) {
        Some(i) => {
            picks.remove(i);
        }
        None => {
            let weighted = picks.iter().any(|&(_, w)| w > 0);
            picks.push((id.to_string(), if weighted { 1 } else { 0 }));
        }
    }
    canonical(devices, picks)
}

/// Set one device's weight. Editing a weight while the selection is in auto mode
/// makes it explicit, so the untouched devices are seeded to 1 first: typing "3"
/// on a two-GPU auto split means 3:1, not 3:0 (which would strand the second GPU
/// with no layers).
pub fn set_weight(
    devices: &[DeviceOption],
    sel: &GpuSelection,
    id: &str,
    weight: i32,
) -> GpuSelection {
    let mut picks = picks(sel);
    if !picks.iter().any(|(d, _)| d.eq_ignore_ascii_case(id)) {
        return sel.clone();
    }
    if picks.iter().all(|&(_, w)| w <= 0) {
        for p in &mut picks {
            p.1 = 1;
        }
    }
    for p in &mut picks {
        if p.0.eq_ignore_ascii_case(id) {
            p.1 = weight.max(0);
        }
    }
    canonical(devices, picks)
}

/// Auto: drop the explicit weights and let llama.cpp split by free VRAM.
pub fn set_auto(devices: &[DeviceOption], sel: &GpuSelection) -> GpuSelection {
    let picks: Vec<Pick> = picks(sel).into_iter().map(|(id, _)| (id, 0)).collect();
    canonical(devices, picks)
}

/// Even: give every selected device the same weight.
pub fn set_even(devices: &[DeviceOption], sel: &GpuSelection) -> GpuSelection {
    let picks: Vec<Pick> = picks(sel).into_iter().map(|(id, _)| (id, 1)).collect();
    canonical(devices, picks)
}

// ── Display ──────────────────────────────────────────────────────────────

/// The table rows: EVERY probed GPU, in probe order, checked or not — the order
/// never depends on the selection, so a click cannot slide the next row under the
/// cursor (see the module header; that bug pinned a model to an iGPU). Read top to
/// bottom, the checked rows are exactly the `--device` list and its
/// `--tensor-split` vector, because every edit re-sorts into this same order.
///
/// A selected id the probe doesn't know — stale, hand-edited, or simply selected
/// before the async probe landed — is appended as a `detected: false` row so the
/// next save can't silently drop it.
pub fn build_rows(devices: &[DeviceOption], sel: &GpuSelection) -> Vec<GpuRow> {
    let picks = picks(sel);
    let total: i32 = picks.iter().map(|&(_, w)| w).sum();
    let weight_of = |id: &str| {
        picks
            .iter()
            .find(|(d, _)| d.eq_ignore_ascii_case(id))
            .map(|&(_, w)| w)
    };
    let mut rows: Vec<GpuRow> = Vec::new();

    for d in devices.iter().filter(|d| !d.is_cpu()) {
        let picked = weight_of(&d.id);
        rows.push(GpuRow {
            id: d.id.clone().into(),
            name: d.name.clone().into(),
            vram: d.vram_summary().into(),
            detected: true,
            enabled: picked.is_some(),
            weight: picked.unwrap_or(0),
            share: match picked {
                Some(w) => share(picks.len(), w, total),
                None => "—".into(),
            }
            .into(),
        });
    }

    for (id, weight) in &picks {
        if probe_index(devices, id) != usize::MAX {
            continue;
        }
        rows.push(GpuRow {
            id: id.clone().into(),
            name: "(not detected)".into(),
            vram: SharedString::new(),
            detected: false,
            enabled: true,
            weight: *weight,
            share: share(picks.len(), *weight, total).into(),
        });
    }
    rows
}

/// The share column for a SELECTED device: its slice of the model, or the word
/// llama.cpp's own default earns when no weights are set.
fn share(count: usize, weight: i32, total: i32) -> String {
    if count < 2 {
        return "100%".into();
    }
    if total <= 0 {
        return "auto".into();
    }
    format!("{:.0}%", f64::from(weight) * 100.0 / f64::from(total))
}

/// The line under the table: the llama-server flags this selection produces.
pub fn summary(sel: &GpuSelection) -> String {
    let picks = picks(sel);
    if picks.is_empty() {
        return "(all detected devices — llama.cpp chooses and splits automatically)".into();
    }
    let mut s = format!("--device {}", sel.device);
    if sel.tensor_split.is_empty() {
        if picks.len() > 1 {
            s.push_str("   (auto split, by free VRAM at load)");
        }
    } else {
        s.push_str(&format!("   --tensor-split {}", sel.tensor_split));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices;

    // Emiliano's box, trimmed: the probe order is CUDA0, ROCm0, ROCm1, Vulkan0 —
    // note ROCm0 is the iGPU and ROCm1 the discrete R9700, adjacent rows one
    // mis-click apart. That adjacency is why the row order must never move.
    const SAMPLE: &str = "Available devices:\n  \
        CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)\n  \
        ROCm0: AMD Radeon(TM) Graphics (25706 MiB, 25555 MiB free)\n  \
        ROCm1: AMD Radeon AI PRO R9700 (32624 MiB, 32462 MiB free)\n  \
        Vulkan0: AMD Radeon(TM) Graphics (33593 MiB, 31913 MiB free)\n  \
        CPU: AMD Ryzen 9 9900X (63090 MiB, 48233 MiB free)\n";

    fn devs() -> Vec<DeviceOption> {
        devices::parse(SAMPLE)
    }

    fn sel(device: &str, tensor_split: &str) -> GpuSelection {
        GpuSelection {
            device: device.into(),
            tensor_split: tensor_split.into(),
        }
    }

    fn ids(rows: &[GpuRow]) -> Vec<String> {
        rows.iter().map(|r| r.id.to_string()).collect()
    }

    // ── The row order NEVER depends on the selection ──────────────────────

    // The regression that pinned a model to the iGPU: rows used to be re-sorted
    // selection-first, so every click slid the next row under the cursor. Now the
    // table is the probe's list, always — only the checkmarks move.
    #[test]
    fn rows_keep_probe_order_whatever_is_selected() {
        let order = ["CUDA0", "ROCm0", "ROCm1", "Vulkan0"]; // CPU is not a --device
        for s in [
            sel("", ""),
            sel("ROCm1", ""),
            sel("ROCm1,CUDA0", "3,1"),
            sel("Vulkan0", ""),
        ] {
            assert_eq!(ids(&build_rows(&devs(), &s)), order, "reordered by {s:?}");
        }
    }

    // …and an edit re-sorts the device list into that same order, so the checked
    // rows read top-to-bottom ARE the --device list and its --tensor-split vector.
    #[test]
    fn an_edit_canonicalizes_the_device_list_into_probe_order() {
        // Checking CUDA0 (probe index 0) after ROCm1 (index 2) puts it FIRST.
        let s = toggle(&devs(), &sel("ROCm1", ""), "CUDA0");
        assert_eq!(s, sel("CUDA0,ROCm1", ""));
        // Weights travel with their device across the re-sort — the split is kept.
        let s = toggle(&devs(), &sel("ROCm1,Vulkan0", "3,1"), "CUDA0");
        assert_eq!(s, sel("CUDA0,ROCm1,Vulkan0", "1,3,1"));
    }

    // ── The four states of the table ──────────────────────────────────────

    #[test]
    fn nothing_selected_renders_both_strings_empty() {
        let s = canonical(&devs(), vec![]);
        assert_eq!(s, sel("", ""));
        assert!(summary(&s).starts_with("(all detected devices"));
    }

    #[test]
    fn one_device_never_gets_a_tensor_split() {
        // Even with a weight carried over from a 2-GPU selection: one device has
        // nothing to split against, and llama.cpp would ignore the vector anyway.
        let s = toggle(&devs(), &sel("CUDA0,ROCm1", "1,3"), "CUDA0");
        assert_eq!(s, sel("ROCm1", ""));
    }

    #[test]
    fn two_devices_unweighted_stay_auto() {
        let s = toggle(&devs(), &sel("ROCm1", ""), "CUDA0");
        assert_eq!(s, sel("CUDA0,ROCm1", ""));
        assert!(summary(&s).contains("auto split"));
    }

    #[test]
    fn two_devices_weighted_render_the_vector_in_device_order() {
        let s = set_weight(&devs(), &sel("CUDA0,ROCm1", ""), "ROCm1", 3);
        // The untouched device is seeded to 1, not left at 0 — 1:3, not 0:3.
        assert_eq!(s, sel("CUDA0,ROCm1", "1,3"));
        assert_eq!(summary(&s), "--device CUDA0,ROCm1   --tensor-split 1,3");
    }

    // ── Edits ─────────────────────────────────────────────────────────────

    #[test]
    fn a_newly_checked_device_joins_a_weighted_selection_at_weight_one() {
        let s = toggle(&devs(), &sel("CUDA0,ROCm1", "1,3"), "Vulkan0");
        assert_eq!(s, sel("CUDA0,ROCm1,Vulkan0", "1,3,1"));
    }

    #[test]
    fn unchecking_drops_the_device_and_its_weight_together() {
        let s = toggle(&devs(), &sel("CUDA0,ROCm1,Vulkan0", "1,3,1"), "CUDA0");
        assert_eq!(s, sel("ROCm1,Vulkan0", "3,1"));
    }

    #[test]
    fn toggle_matches_ids_case_insensitively() {
        assert_eq!(toggle(&devs(), &sel("ROCm1", ""), "rocm1"), sel("", ""));
    }

    #[test]
    fn set_weight_on_an_unselected_device_is_a_no_op() {
        let before = sel("CUDA0,ROCm1", "1,3");
        assert_eq!(set_weight(&devs(), &before, "Vulkan0", 5), before);
    }

    #[test]
    fn zeroing_every_weight_falls_back_to_auto() {
        let s = set_weight(&devs(), &sel("CUDA0,ROCm1", "1,1"), "ROCm1", 0);
        assert_eq!(s, sel("CUDA0,ROCm1", "1,0"));
        let s = set_weight(&devs(), &s, "CUDA0", 0);
        assert_eq!(s, sel("CUDA0,ROCm1", ""));
    }

    #[test]
    fn auto_and_even_are_different_launches() {
        let weighted = sel("CUDA0,ROCm1", "1,3");
        assert_eq!(set_auto(&devs(), &weighted), sel("CUDA0,ROCm1", ""));
        assert_eq!(set_even(&devs(), &weighted), sel("CUDA0,ROCm1", "1,1"));
    }

    // ── Rows ──────────────────────────────────────────────────────────────

    #[test]
    fn rows_carry_the_selection_its_weights_and_the_derived_share() {
        let rows = build_rows(&devs(), &sel("CUDA0,ROCm1", "1,3"));
        assert_eq!(rows.len(), 4); // CPU is not a --device participant
        assert!(rows[0].enabled && !rows[1].enabled && rows[2].enabled);
        assert_eq!(rows[0].share, "25%");
        assert_eq!(rows[1].share, "—");
        assert_eq!(rows[2].share, "75%");
        assert_eq!(rows[2].weight, 3);
        assert_eq!(rows[2].name, "AMD Radeon AI PRO R9700");
        assert_eq!(rows[2].vram, "31.9 GB (31.7 free)");
    }

    #[test]
    fn rows_report_auto_when_no_weights_are_set() {
        let rows = build_rows(&devs(), &sel("CUDA0,ROCm1", ""));
        assert_eq!(rows[0].share, "auto");
        assert_eq!(rows[2].share, "auto");
    }

    #[test]
    fn a_single_selected_device_holds_the_whole_model() {
        let rows = build_rows(&devs(), &sel("ROCm1", ""));
        assert_eq!(rows[2].share, "100%");
    }

    #[test]
    fn a_selected_id_the_probe_doesnt_know_survives_as_an_undetected_row() {
        // The probe is async: the GUI can render before it lands, and a config
        // may name a device from another machine. Dropping the row would let the
        // next save quietly rewrite `device`. Unknown ids sort last (probe index
        // usize::MAX), so they land at the bottom of both the table and --device.
        let rows = build_rows(&devs(), &sel("SYCL3,ROCm1", "1,1"));
        let last = rows.last().expect("a row");
        assert_eq!(last.id, "SYCL3");
        assert!(last.enabled);
        assert!(!last.detected);
        assert_eq!(last.name, "(not detected)");
        assert!(last.vram.is_empty());
        // …and it round-trips through an unrelated edit, weight intact.
        assert_eq!(
            toggle(&devs(), &sel("SYCL3,ROCm1", "1,1"), "CUDA0"),
            sel("CUDA0,ROCm1,SYCL3", "1,1,1")
        );
    }

    #[test]
    fn rows_are_empty_of_gpus_before_the_probe_lands_but_keep_the_selection() {
        let rows = build_rows(&[], &sel("ROCm1", ""));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "ROCm1");
        assert!(!rows[0].detected);
    }

    // ── Hand-edited INI tolerance ─────────────────────────────────────────

    #[test]
    fn a_short_or_malformed_weight_vector_degrades_to_auto_not_garbage() {
        // Fewer weights than devices: the tail is unweighted.
        assert_eq!(
            picks(&sel("ROCm1,CUDA0", "3")),
            [("ROCm1".into(), 3), ("CUDA0".into(), 0)]
        );
        // Non-numeric / negative parts read as 0.
        assert_eq!(parse_weights("3, x, -2"), [3, 0, 0]);
        // Whitespace and a trailing comma never invent a device.
        assert_eq!(parse_device_list(" ROCm1 , CUDA0 , "), ["ROCm1", "CUDA0"]);
    }

    // A hand-written list in a non-probe order is READ as written (the rows and
    // the launch honour it) and only normalized once the table is touched — with
    // each weight still attached to its own device.
    #[test]
    fn a_hand_written_order_is_read_as_is_and_normalized_only_on_edit() {
        let hand = sel("ROCm1,CUDA0", "3,1");
        assert_eq!(build_rows(&devs(), &hand)[0].weight, 1, "CUDA0 keeps its 1");
        assert_eq!(build_rows(&devs(), &hand)[2].weight, 3, "ROCm1 keeps its 3");
        assert_eq!(
            set_even(&devs(), &hand),
            sel("CUDA0,ROCm1", "1,1"),
            "the first edit canonicalizes the order"
        );
    }
}
