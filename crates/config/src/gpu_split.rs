//! The GPU distribution table's rows: the selection logic lives in
//! `pai_app::gpu_split` (its module header says why the table is shaped this
//! way); this is the conversion into the Slint `GpuRow`s the table shows.

pub use pai_app::gpu_split::*;

use crate::strings;
use slint::SharedString;

use crate::devices::DeviceOption;
use crate::gui::GpuRow;

// ── Display ──────────────────────────────────────────────────────────────

/// The table rows: the CHECKED devices first, in split order (the row order IS
/// the `--device` order, which is what makes the drag handle mean something),
/// then every other probed GPU, in probe order.
///
/// A checked id the probe doesn't know (stale, hand-edited, or simply checked
/// before the async probe landed) still gets its row (`detected: false`), so the
/// next save can't silently drop it.
///
/// `layout` is the model to project the weights onto (`Layout`), and it is what
/// turns the ratio column into a block column. `None` (a model whose header
/// hasn't been read) leaves `blocks`/`blocks_label` empty and the
/// table falls back to editing raw weights.
///
/// `mode` is the preset's split mode (`SplitMode::parse`). It bends the rows the
/// way llama.cpp will bend the launch: `Row` and `Tensor` discard the layout
/// (the bytes follow row fractions, or a tensor-axis cut, so a block cut would
/// misreport the placement), `Tensor` additionally names a blank vector "even"
/// (the one mode where blank is not the free-VRAM weighting "auto" stands for),
/// and `Single` overrides the share column: the head row holds the whole model
/// and every other checked row is dead weight, which "unused" says where a "—"
/// would read as merely unchecked.
pub fn build_rows(
    devices: &[DeviceOption],
    sel: &GpuSelection,
    layout: Option<Layout>,
    mode: SplitMode,
) -> Vec<GpuRow> {
    let picks = picks(sel);
    let total: i32 = picks.iter().map(|&(_, w)| w).sum();
    let layout = if mode == SplitMode::Layer {
        layout
    } else {
        None
    };
    let counts = layout.and_then(|l| block_counts(&weights_of(&picks), l));
    let ranges = counts
        .as_ref()
        .zip(layout)
        .map(|(c, l)| block_ranges(c, l))
        .unwrap_or_default();
    let mut rows: Vec<GpuRow> = Vec::new();

    for (i, (id, weight)) in picks.iter().enumerate() {
        let dev = devices
            .iter()
            .find(|d| d.id.eq_ignore_ascii_case(id) && !d.is_cpu());
        rows.push(GpuRow {
            id: id.clone().into(),
            name: dev
                .map_or_else(|| strings::t("not-detected"), |d| d.name.clone())
                .into(),
            vram: dev
                .map(DeviceOption::vram_summary)
                .map_or_else(SharedString::new, Into::into),
            detected: dev.is_some(),
            enabled: true,
            weight: *weight,
            share: match mode {
                SplitMode::Single if i == 0 => "100%".into(),
                SplitMode::Single => "unused".into(),
                // A blank vector under tensor parallelism is an EVEN cut
                // (`llama_meta_device_get_split_state`: an all-zero scan cuts
                // `ne_s * (j+1) / n_devices`), not the free-VRAM weighting
                // "auto" names in the other modes.
                SplitMode::Tensor if picks.len() >= 2 && total <= 0 => "even".into(),
                _ => share(picks.len(), *weight, total).into(),
            },
            blocks: counts.as_ref().and_then(|c| c.get(i)).copied().unwrap_or(0),
            blocks_label: ranges.get(i).cloned().unwrap_or_default().into(),
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
            blocks: 0,
            blocks_label: SharedString::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices;

    // A real mixed box, trimmed. ROCm0 is the discrete R9700 and ROCm1 an iGPU
    // that cannot run inference: adjacent rows, one mis-click apart. (And that
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
        let rows = build_rows(&devs(), &sel("ROCm0,CUDA0", "3,1"), None, SplitMode::Layer);
        assert_eq!(ids(&rows), ["ROCm0", "CUDA0", "ROCm1", "Vulkan0"]);
        assert!(rows[0].enabled && rows[1].enabled && !rows[2].enabled);
    }

    // The drag handle. Position 0 is llama.cpp's main_gpu, and appending is the
    // only thing a checkbox can do, so without this there is no way to promote a
    // GPU to the head of the split.
    #[test]
    fn move_by_promotes_a_device_and_its_weight_together() {
        let s = move_by(&sel("CUDA0,ROCm0", "1,3"), "ROCm0", -1);
        assert_eq!(s, sel("ROCm0,CUDA0", "3,1"), "the weight rides along");
        assert_eq!(
            ids(&build_rows(&devs(), &s, None, SplitMode::Layer))[0],
            "ROCm0"
        );
    }

    // ── Rows ──────────────────────────────────────────────────────────────

    #[test]
    fn rows_carry_the_selection_its_weights_and_the_derived_share() {
        let rows = build_rows(&devs(), &sel("ROCm0,CUDA0", "3,1"), None, SplitMode::Layer);
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
        let rows = build_rows(&devs(), &sel("ROCm0,CUDA0", ""), None, SplitMode::Layer);
        assert_eq!(rows[0].share, "auto");
        assert_eq!(rows[1].share, "auto");
    }

    #[test]
    fn a_single_selected_device_holds_the_whole_model() {
        let rows = build_rows(&devs(), &sel("ROCm0", ""), None, SplitMode::Layer);
        assert_eq!(rows[0].share, "100%");
    }

    #[test]
    fn a_selected_id_the_probe_doesnt_know_survives_as_an_undetected_row() {
        // The probe is async: the GUI can render before it lands, and a config
        // may name a device from another machine. Dropping the row would let the
        // next save quietly rewrite `device`.
        let rows = build_rows(&devs(), &sel("SYCL3,ROCm0", "1,1"), None, SplitMode::Layer);
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
        let rows = build_rows(&[], &sel("ROCm0", ""), None, SplitMode::Layer);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "ROCm0");
        assert!(!rows[0].detected);
    }

    // A hand-written order is honoured as written: the table never re-sorts it
    // behind the user's back; only the drag handle moves a device.
    #[test]
    fn a_hand_written_order_survives_every_edit_that_is_not_a_move() {
        let hand = sel("Vulkan0,CUDA0", "3,1");
        assert_eq!(
            ids(&build_rows(&devs(), &hand, None, SplitMode::Layer))[0],
            "Vulkan0"
        );
        assert_eq!(set_even(&hand), sel("Vulkan0,CUDA0", "1,1"));
        assert_eq!(
            set_weight(&hand, "CUDA0", 2),
            sel("Vulkan0,CUDA0", "3,2"),
            "order untouched"
        );
    }

    // ── Blocks ────────────────────────────────────────────────────────────

    // Ornith-1.0-9B: 33 blocks (32 trunk + 1 nextn/MTP) → 34 positions.
    const ORNITH: i32 = 33;

    fn full(n_layer_all: i32) -> Layout {
        Layout::new(n_layer_all, -1).expect("a model with blocks has a layout")
    }

    // The rows carry the projection only when a model is there to project onto:
    // without a header read, None keeps it a ratio table.
    #[test]
    fn rows_carry_blocks_only_when_a_layout_is_given() {
        let s = sel("ROCm0,CUDA0", "85,15");
        let with = build_rows(&devs(), &s, Some(full(ORNITH)), SplitMode::Layer);
        assert_eq!((with[0].blocks, with[1].blocks), (29, 5));
        assert_eq!(with[0].blocks_label, "blocks 0-28");
        assert_eq!(with[1].blocks_label, "blocks 29-32 + output");
        assert_eq!(with[0].weight, 85, "the stored ratio is untouched");
        // Unchecked rows have no position in the split.
        assert_eq!(with[2].blocks, 0);
        assert_eq!(with[2].blocks_label, "");

        let without = build_rows(&devs(), &s, None, SplitMode::Layer);
        assert!(without
            .iter()
            .all(|r| r.blocks == 0 && r.blocks_label.is_empty()));

        // Three checked devices, projected the same way: the rows are the split
        // order, so the ranges run down the table in one unbroken sequence.
        let three = build_rows(
            &devs(),
            &sel("ROCm0,CUDA0,Vulkan0", "5,20,9"),
            Some(full(ORNITH)),
            SplitMode::Layer,
        );
        assert_eq!(
            three
                .iter()
                .take(3)
                .map(|r| (r.blocks, r.blocks_label.to_string()))
                .collect::<Vec<_>>(),
            [
                (5, "blocks 0-4".into()),
                (20, "blocks 5-24".into()),
                (9, "blocks 25-32 + output".to_string()),
            ]
        );
    }

    // Under `none` llama.cpp drops every device but the head, so the rows must
    // say so: no block projection even when a layout is known, the head at
    // 100%, and the other CHECKED rows explicitly "unused" (a "—" would read as
    // merely unchecked).
    #[test]
    fn single_mode_uses_only_the_head_row() {
        let rows = build_rows(
            &devs(),
            &sel("ROCm0,CUDA0", "3,1"),
            Some(full(ORNITH)),
            SplitMode::Single,
        );
        assert_eq!(rows[0].share, "100%");
        assert_eq!(rows[1].share, "unused");
        assert_eq!(rows[2].share, "—", "unchecked rows keep the dash");
        assert!(rows
            .iter()
            .all(|r| r.blocks == 0 && r.blocks_label.is_empty()));
        assert_eq!(rows[0].weight, 3, "the stored ratio survives the mode");
    }

    // Under `row` the bytes follow row fractions, not block cuts, so the block
    // column must stay out even when the model is known, while the ratio/share
    // pair keeps working exactly as in the no-layout fallback.
    #[test]
    fn row_mode_edits_ratios_never_blocks() {
        let rows = build_rows(
            &devs(),
            &sel("ROCm0,CUDA0", "3,1"),
            Some(full(ORNITH)),
            SplitMode::Row,
        );
        assert!(rows
            .iter()
            .all(|r| r.blocks == 0 && r.blocks_label.is_empty()));
        assert_eq!(rows[0].share, "75%");
        assert_eq!(rows[1].share, "25%");
    }

    // Under `tensor` the vector cuts every weight and the KV cache along a
    // tensor axis, so it is a ratio too (no block cut, layout or not), and a
    // BLANK vector is the one place llama.cpp splits EVENLY rather than by free
    // VRAM, which the share column must say instead of "auto".
    #[test]
    fn tensor_mode_edits_ratios_and_reads_a_blank_vector_as_even() {
        let rows = build_rows(
            &devs(),
            &sel("ROCm0,CUDA0", "3,1"),
            Some(full(ORNITH)),
            SplitMode::Tensor,
        );
        assert!(rows
            .iter()
            .all(|r| r.blocks == 0 && r.blocks_label.is_empty()));
        assert_eq!(rows[0].share, "75%");
        assert_eq!(rows[1].share, "25%");

        let rows = build_rows(&devs(), &sel("ROCm0,CUDA0", ""), None, SplitMode::Tensor);
        assert_eq!(rows[0].share, "even");
        assert_eq!(rows[1].share, "even");
        assert_eq!(rows[2].share, "—", "unchecked rows keep the dash");
        // The same blank vector is free-VRAM "auto" everywhere else.
        let rows = build_rows(&devs(), &sel("ROCm0,CUDA0", ""), None, SplitMode::Row);
        assert_eq!(rows[0].share, "auto");
        // One device has nothing to cut, evenly or otherwise.
        let rows = build_rows(&devs(), &sel("ROCm0", ""), None, SplitMode::Tensor);
        assert_eq!(rows[0].share, "100%");
    }
}
