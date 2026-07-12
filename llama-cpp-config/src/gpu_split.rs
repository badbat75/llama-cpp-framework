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
//! ## The row order IS the split order, and the user owns it
//! Checked devices come FIRST, in `--device` order, then the rest in probe order.
//! So the table read top to bottom is literally the `--device` list and its
//! `--tensor-split` vector — and the drag handle (`move_by`) is how you change it.
//!
//! Position 0 is not cosmetic: it is `devices[0]`, which is also `main_gpu`
//! (llama.cpp defaults `--main-gpu` to 0, and the framework never overrides it).
//! With `--split-mode none` that is the ONE GPU llama.cpp keeps; in `layer` mode
//! it takes the first slice of layers. Without a way to reorder there was no way
//! to put a chosen GPU at the head, which is why the handle exists.
//!
//! Checking a device APPENDS it (last in the split, weight 1 if the others are
//! weighted); unchecking removes it and its weight together. Weights are carried
//! in the same tuple as their device, so a reorder moves them as a unit — the
//! split proportions survive, only the positions change.

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

/// Render picks back into the INI string pair, IN THE GIVEN ORDER (that order is
/// the user's — see the module header) — the one place the four-state table above
/// is enforced. `tensor_split` collapses to empty for fewer than two devices
/// (nothing to split) and for an all-zero weight vector (auto).
fn render(picks: &[Pick]) -> GpuSelection {
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

// ── Edits (each returns the new selection; the caller writes it to the form) ──

/// Check / uncheck a device. A newly checked one is APPENDED — last in the split,
/// at weight 1 when the others are already weighted (adding it at 0 would silently
/// give the new GPU no layers, which is never what checking a box means). Use
/// `move_by` to promote it; nothing here reorders on its own.
pub fn toggle(sel: &GpuSelection, id: &str) -> GpuSelection {
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
    render(&picks)
}

/// Move a checked device `delta` places within the split (negative = towards the
/// head), clamped to the ends. This is the drag handle: it is the ONLY way to
/// choose which GPU sits at position 0 — `devices[0]` is llama.cpp's `main_gpu`
/// (the sole GPU under `--split-mode none`, and the first slice of layers under
/// `layer`). The weight rides along in the same tuple, so the proportions are
/// preserved and only the positions change.
pub fn move_by(sel: &GpuSelection, id: &str, delta: i32) -> GpuSelection {
    let mut picks = picks(sel);
    let Some(from) = picks.iter().position(|(d, _)| d.eq_ignore_ascii_case(id)) else {
        return sel.clone();
    };
    let last = picks.len().saturating_sub(1);
    let to = usize::try_from(i64::from(delta) + from as i64).unwrap_or(0).min(last);
    if to == from {
        return sel.clone();
    }
    let moved = picks.remove(from);
    picks.insert(to, moved);
    render(&picks)
}

/// Set one device's weight. Editing a weight while the selection is in auto mode
/// makes it explicit, so the untouched devices are seeded to 1 first: typing "3"
/// on a two-GPU auto split means 3:1, not 3:0 (which would strand the second GPU
/// with no layers).
pub fn set_weight(sel: &GpuSelection, id: &str, weight: i32) -> GpuSelection {
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
    render(&picks)
}

/// Auto: drop the explicit weights and let llama.cpp split by free VRAM.
pub fn set_auto(sel: &GpuSelection) -> GpuSelection {
    let picks: Vec<Pick> = picks(sel).into_iter().map(|(id, _)| (id, 0)).collect();
    render(&picks)
}

/// Even: give every selected device the same weight.
pub fn set_even(sel: &GpuSelection) -> GpuSelection {
    let picks: Vec<Pick> = picks(sel).into_iter().map(|(id, _)| (id, 1)).collect();
    render(&picks)
}

// ── Display ──────────────────────────────────────────────────────────────

/// The table rows: the CHECKED devices first, in split order — the row order IS
/// the `--device` order, which is what makes the drag handle mean something — then
/// every other probed GPU, in probe order.
///
/// A checked id the probe doesn't know — stale, hand-edited, or simply checked
/// before the async probe landed — still gets its row (`detected: false`), so the
/// next save can't silently drop it.
pub fn build_rows(devices: &[DeviceOption], sel: &GpuSelection) -> Vec<GpuRow> {
    let picks = picks(sel);
    let total: i32 = picks.iter().map(|&(_, w)| w).sum();
    let mut rows: Vec<GpuRow> = Vec::new();

    for (id, weight) in &picks {
        let dev = devices
            .iter()
            .find(|d| d.id.eq_ignore_ascii_case(id) && !d.is_cpu());
        rows.push(GpuRow {
            id: id.clone().into(),
            name: dev.map_or("(not detected)", |d| d.name.as_str()).into(),
            vram: dev
                .map(DeviceOption::vram_summary)
                .map_or_else(SharedString::new, Into::into),
            detected: dev.is_some(),
            enabled: true,
            weight: *weight,
            share: share(picks.len(), *weight, total).into(),
        });
    }

    for d in devices.iter().filter(|d| !d.is_cpu()) {
        if picks.iter().any(|(id, _)| id.eq_ignore_ascii_case(&d.id)) {
            continue;
        }
        rows.push(GpuRow {
            id: d.id.clone().into(),
            name: d.name.clone().into(),
            vram: d.vram_summary().into(),
            detected: true,
            enabled: false,
            weight: 0,
            share: "—".into(),
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

    // A real mixed box, trimmed. ROCm0 is the discrete R9700 and ROCm1 an iGPU
    // that cannot run inference — adjacent rows, one mis-click apart. (And that
    // pairing is not even stable: the same machine has enumerated them the other
    // way round, which is why the table shows the NAME next to every id.)
    const SAMPLE: &str = "Available devices:\n  \
        CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)\n  \
        ROCm0: AMD Radeon AI PRO R9700 (32624 MiB, 32462 MiB free)\n  \
        ROCm1: AMD Radeon(TM) Graphics (25706 MiB, 25555 MiB free)\n  \
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

    // ── The row order IS the split order ──────────────────────────────────

    #[test]
    fn checked_rows_come_first_in_split_order_then_the_rest_in_probe_order() {
        let rows = build_rows(&devs(), &sel("ROCm0,CUDA0", "3,1"));
        assert_eq!(ids(&rows), ["ROCm0", "CUDA0", "ROCm1", "Vulkan0"]);
        assert!(rows[0].enabled && rows[1].enabled && !rows[2].enabled);
    }

    // The drag handle. Position 0 is llama.cpp's main_gpu, and appending is the
    // only thing a checkbox can do — so without this there is no way to promote a
    // GPU to the head of the split.
    #[test]
    fn move_by_promotes_a_device_and_its_weight_together() {
        let s = move_by(&sel("CUDA0,ROCm0", "1,3"), "ROCm0", -1);
        assert_eq!(s, sel("ROCm0,CUDA0", "3,1"), "the weight rides along");
        assert_eq!(ids(&build_rows(&devs(), &s))[0], "ROCm0");
    }

    #[test]
    fn move_by_clamps_at_both_ends_and_ignores_a_no_op() {
        let head = sel("ROCm0,CUDA0", "3,1");
        assert_eq!(move_by(&head, "ROCm0", -5), head, "already at the head");
        assert_eq!(move_by(&head, "CUDA0", 9), head, "already at the tail");
        assert_eq!(move_by(&head, "ROCm0", 0), head);
        // An unchecked device has no position to move.
        assert_eq!(move_by(&head, "Vulkan0", -1), head);
    }

    #[test]
    fn move_by_reaches_across_a_three_device_split() {
        let s = move_by(&sel("CUDA0,ROCm0,Vulkan0", "1,3,2"), "Vulkan0", -2);
        assert_eq!(s, sel("Vulkan0,CUDA0,ROCm0", "2,1,3"));
    }

    // ── The four states of the table ──────────────────────────────────────

    #[test]
    fn nothing_selected_renders_both_strings_empty() {
        let s = render(&[]);
        assert_eq!(s, sel("", ""));
        assert!(summary(&s).starts_with("(all detected devices"));
    }

    #[test]
    fn one_device_never_gets_a_tensor_split() {
        // Even with a weight carried over from a 2-GPU selection: one device has
        // nothing to split against, and llama.cpp would ignore the vector anyway.
        let s = toggle(&sel("ROCm0,CUDA0", "3,1"), "CUDA0");
        assert_eq!(s, sel("ROCm0", ""));
    }

    #[test]
    fn two_devices_unweighted_stay_auto() {
        let s = toggle(&sel("ROCm0", ""), "CUDA0");
        assert_eq!(s, sel("ROCm0,CUDA0", ""));
        assert!(summary(&s).contains("auto split"));
    }

    #[test]
    fn two_devices_weighted_render_the_vector_in_device_order() {
        let s = set_weight(&sel("ROCm0,CUDA0", ""), "ROCm0", 3);
        // The untouched device is seeded to 1, not left at 0 — 3:1, not 3:0.
        assert_eq!(s, sel("ROCm0,CUDA0", "3,1"));
        assert_eq!(summary(&s), "--device ROCm0,CUDA0   --tensor-split 3,1");
    }

    // ── Edits ─────────────────────────────────────────────────────────────

    #[test]
    fn a_newly_checked_device_is_appended_at_weight_one() {
        let s = toggle(&sel("ROCm0,CUDA0", "3,1"), "Vulkan0");
        assert_eq!(s, sel("ROCm0,CUDA0,Vulkan0", "3,1,1"));
    }

    #[test]
    fn unchecking_drops_the_device_and_its_weight_together() {
        let s = toggle(&sel("ROCm0,CUDA0,Vulkan0", "3,1,1"), "CUDA0");
        assert_eq!(s, sel("ROCm0,Vulkan0", "3,1"));
    }

    #[test]
    fn toggle_matches_ids_case_insensitively() {
        assert_eq!(toggle(&sel("ROCm0", ""), "rocm0"), sel("", ""));
    }

    #[test]
    fn set_weight_on_an_unselected_device_is_a_no_op() {
        let before = sel("ROCm0,CUDA0", "3,1");
        assert_eq!(set_weight(&before, "Vulkan0", 5), before);
    }

    #[test]
    fn zeroing_every_weight_falls_back_to_auto() {
        let s = set_weight(&sel("ROCm0,CUDA0", "1,1"), "ROCm0", 0);
        assert_eq!(s, sel("ROCm0,CUDA0", "0,1"));
        let s = set_weight(&s, "CUDA0", 0);
        assert_eq!(s, sel("ROCm0,CUDA0", ""));
    }

    #[test]
    fn auto_and_even_are_different_launches() {
        let weighted = sel("ROCm0,CUDA0", "3,1");
        assert_eq!(set_auto(&weighted), sel("ROCm0,CUDA0", ""));
        assert_eq!(set_even(&weighted), sel("ROCm0,CUDA0", "1,1"));
    }

    // ── Rows ──────────────────────────────────────────────────────────────

    #[test]
    fn rows_carry_the_selection_its_weights_and_the_derived_share() {
        let rows = build_rows(&devs(), &sel("ROCm0,CUDA0", "3,1"));
        assert_eq!(rows.len(), 4); // CPU is not a --device participant
        assert_eq!(rows[0].share, "75%");
        assert_eq!(rows[1].share, "25%");
        assert_eq!(rows[2].share, "—");
        assert_eq!(rows[0].weight, 3);
        assert_eq!(rows[0].name, "AMD Radeon AI PRO R9700");
        assert_eq!(rows[0].vram, "31.9 GB (31.7 free)");
    }

    #[test]
    fn rows_report_auto_when_no_weights_are_set() {
        let rows = build_rows(&devs(), &sel("ROCm0,CUDA0", ""));
        assert_eq!(rows[0].share, "auto");
        assert_eq!(rows[1].share, "auto");
    }

    #[test]
    fn a_single_selected_device_holds_the_whole_model() {
        let rows = build_rows(&devs(), &sel("ROCm0", ""));
        assert_eq!(rows[0].share, "100%");
    }

    #[test]
    fn a_selected_id_the_probe_doesnt_know_survives_as_an_undetected_row() {
        // The probe is async: the GUI can render before it lands, and a config
        // may name a device from another machine. Dropping the row would let the
        // next save quietly rewrite `device`.
        let rows = build_rows(&devs(), &sel("SYCL3,ROCm0", "1,1"));
        assert_eq!(rows[0].id, "SYCL3");
        assert!(rows[0].enabled);
        assert!(!rows[0].detected);
        assert_eq!(rows[0].name, "(not detected)");
        assert!(rows[0].vram.is_empty());
        // …and it round-trips through an unrelated edit, weight intact.
        assert_eq!(
            toggle(&sel("SYCL3,ROCm0", "1,1"), "CUDA0"),
            sel("SYCL3,ROCm0,CUDA0", "1,1,1")
        );
    }

    #[test]
    fn rows_are_empty_of_gpus_before_the_probe_lands_but_keep_the_selection() {
        let rows = build_rows(&[], &sel("ROCm0", ""));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "ROCm0");
        assert!(!rows[0].detected);
    }

    // ── Hand-edited INI tolerance ─────────────────────────────────────────

    #[test]
    fn a_short_or_malformed_weight_vector_degrades_to_auto_not_garbage() {
        // Fewer weights than devices: the tail is unweighted.
        assert_eq!(
            picks(&sel("ROCm0,CUDA0", "3")),
            [("ROCm0".into(), 3), ("CUDA0".into(), 0)]
        );
        // Non-numeric / negative parts read as 0.
        assert_eq!(parse_weights("3, x, -2"), [3, 0, 0]);
        // Whitespace and a trailing comma never invent a device.
        assert_eq!(parse_device_list(" ROCm0 , CUDA0 , "), ["ROCm0", "CUDA0"]);
    }

    // A hand-written order is honoured as written — the table never re-sorts it
    // behind the user's back; only the drag handle moves a device.
    #[test]
    fn a_hand_written_order_survives_every_edit_that_is_not_a_move() {
        let hand = sel("Vulkan0,CUDA0", "3,1");
        assert_eq!(ids(&build_rows(&devs(), &hand))[0], "Vulkan0");
        assert_eq!(set_even(&hand), sel("Vulkan0,CUDA0", "1,1"));
        assert_eq!(
            set_weight(&hand, "CUDA0", 2),
            sel("Vulkan0,CUDA0", "3,2"),
            "order untouched"
        );
    }
}
