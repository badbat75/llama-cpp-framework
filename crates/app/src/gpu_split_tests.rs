use super::*;

#[test]
fn a_cuda_and_a_rocm_device_in_one_list_is_a_mixed_split() {
    assert!(mixes_cuda_and_rocm("ROCm0,CUDA0"));
    assert!(mixes_cuda_and_rocm("CUDA0, Vulkan1, ROCm1"));
    assert!(!mixes_cuda_and_rocm("ROCm0,ROCm1"));
    assert!(!mixes_cuda_and_rocm("CUDA0,Vulkan2"));
    assert!(!mixes_cuda_and_rocm(""));
}

fn sel(device: &str, tensor_split: &str) -> GpuSelection {
    GpuSelection {
        device: device.into(),
        tensor_split: tensor_split.into(),
    }
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
    assert_eq!(render(&[]), sel("", ""));
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
}

#[test]
fn two_devices_weighted_render_the_vector_in_device_order() {
    let s = set_weight(&sel("ROCm0,CUDA0", ""), "ROCm0", 3);
    // The untouched device is seeded to 1, not left at 0 (3:1, not 3:0).
    assert_eq!(s, sel("ROCm0,CUDA0", "3,1"));
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

// ── Blocks ────────────────────────────────────────────────────────────

// Ornith-1.0-9B: 33 blocks (32 trunk + 1 nextn/MTP) → 34 positions.
const ORNITH: i32 = 33;

fn full(n_layer_all: i32) -> Layout {
    Layout::new(n_layer_all, -1).expect("a model with blocks has a layout")
}

/// The first position a range label names, or `None` for the one label that
/// has no number in it (`output layer`).
fn first_position(label: &str) -> Option<i32> {
    label
        .trim_start_matches("blocks ")
        .trim_start_matches("block ")
        .split(['-', ' '])
        .next()?
        .parse()
        .ok()
}

// The whole reason the column can be edited in blocks: a vector that sums to
// the positions assigns EXACTLY itself, so there is no inverse to invent and
// nothing to round. Writing what the user typed reproduces what they typed.
#[test]
fn a_weight_vector_that_sums_to_the_positions_is_the_block_count_itself() {
    let l = full(ORNITH);
    assert_eq!(l.count, 34);
    assert_eq!(block_counts(&[29, 5], l), Some(vec![29, 5]));
    assert_eq!(block_counts(&[17, 17], l), Some(vec![17, 17]));
    assert_eq!(block_counts(&[34, 0], l), Some(vec![34, 0]));
    assert_eq!(block_counts(&[12, 11, 11], l), Some(vec![12, 11, 11]));
}

// …and the reason a percentage should NOT be the editable unit: three of them
// are the same launch, so two of every three nudges do nothing at all. The
// block column is the one that changes exactly when the launch does.
#[test]
fn neighbouring_percentages_collapse_onto_the_same_cut() {
    let l = full(ORNITH);
    for pct in [83, 84, 85] {
        assert_eq!(
            block_counts(&[pct, 100 - pct], l),
            Some(vec![29, 5]),
            "{pct}% cuts in the same place"
        );
    }
    assert_eq!(block_counts(&[82, 18], l), Some(vec![28, 6]));
    assert_eq!(block_counts(&[86, 14], l), Some(vec![30, 4]));
}

// Even is all-1s (see the module header), which is NOT one block each; the
// projection is what makes that legible instead of a column of 1s.
#[test]
fn even_projects_to_half_the_positions_not_to_a_weight_of_one() {
    assert_eq!(block_counts(&[1, 1], full(ORNITH)), Some(vec![17, 17]));
}

// Auto is decided by FREE VRAM at load, so there is no count to show and the
// table falls back to weights rather than printing a made-up one.
#[test]
fn auto_has_no_block_counts() {
    assert_eq!(block_counts(&[0, 0], full(ORNITH)), None);
    assert_eq!(block_counts(&[], full(ORNITH)), None);
}

// The positions are blocks PLUS the output layer, and partial offload eats
// them off the front (i_gpu_start), not the back.
#[test]
fn the_layout_counts_the_output_layer_and_partial_offload_shifts_the_start() {
    assert_eq!(
        Layout::new(ORNITH, -1),
        Some(Layout {
            start: 0,
            count: 34
        })
    );
    assert_eq!(
        Layout::new(ORNITH, 99),
        Some(Layout {
            start: 0,
            count: 34
        }),
        "an ngl past the end is still just all of them"
    );
    assert_eq!(
        Layout::new(ORNITH, 10),
        Some(Layout {
            start: 24,
            count: 10
        }),
        "10 offloaded = the LAST ten positions"
    );
    assert_eq!(Layout::new(0, -1), None, "no model read yet");
    assert_eq!(Layout::new(ORNITH, 0), None, "nothing on the GPU");
}

// The label has to name the output layer: it is a position but not a block,
// and it carries `output.weight`, routinely the
// biggest single tensor on its device.
#[test]
fn the_range_label_names_the_output_layer_separately() {
    let l = full(ORNITH);
    assert_eq!(
        block_ranges(&[29, 5], l),
        ["blocks 0-28", "blocks 29-32 + output"]
    );
    assert_eq!(
        block_ranges(&[33, 1], l),
        ["blocks 0-32", "output layer"],
        "a one-position tail is the output layer alone"
    );
    assert_eq!(
        block_ranges(&[32, 2], l),
        ["blocks 0-31", "block 32 + output"]
    );
    assert_eq!(
        block_ranges(&[34, 0], l),
        ["blocks 0-32 + output", "nothing"]
    );
    // Partial offload: the ranges start where the GPU does.
    let partial = Layout::new(ORNITH, 10).expect("layout");
    assert_eq!(
        block_ranges(&[5, 5], partial),
        ["blocks 24-28", "blocks 29-32 + output"]
    );
}

// Editing a count is not like editing a weight: the vector must keep summing
// to the positions, so the rows behind the edited one give the difference up.
#[test]
fn setting_a_block_count_takes_the_difference_from_the_back() {
    let l = full(ORNITH);
    let two = sel("ROCm0,CUDA0", "85,15");
    // Typing back the count already shown normalizes the vector and changes
    // nothing about the cut: 85,15 and 29,5 are the same launch.
    let same = set_blocks(&two, "ROCm0", 29, l);
    assert_eq!(same, sel("ROCm0,CUDA0", "29,5"));
    assert_eq!(block_counts(&[29, 5], l), block_counts(&[85, 15], l));
    // Giving the first device less hands the surplus to the other one.
    assert_eq!(
        set_blocks(&two, "ROCm0", 20, l),
        sel("ROCm0,CUDA0", "20,14")
    );
    // Three devices: the LAST row absorbs first, so an edit stays local.
    let three = sel("ROCm0,CUDA0,Vulkan0", "1,1,1");
    assert_eq!(
        set_blocks(&three, "ROCm0", 20, l),
        sel("ROCm0,CUDA0,Vulkan0", "20,11,3")
    );
    // …cascading forward only once the back is exhausted.
    assert_eq!(
        set_blocks(&three, "ROCm0", 34, l),
        sel("ROCm0,CUDA0,Vulkan0", "34,0,0")
    );
    // Out of range is clamped, never wrapped or written through.
    assert_eq!(
        set_blocks(&two, "ROCm0", 999, l),
        sel("ROCm0,CUDA0", "34,0")
    );
    assert_eq!(set_blocks(&two, "ROCm0", -5, l), sel("ROCm0,CUDA0", "0,34"));
    // An unknown id is a no-op, like every other edit here.
    assert_eq!(set_blocks(&two, "CUDA9", 4, l), two);
}

// Editing anything but the head: the tail still pays first, and a row IN FRONT
// of the edited one is touched only once everything behind it is at zero.
// (Three devices is where that direction becomes observable at all; with two
// there is only ever one other row to take from.)
#[test]
fn a_middle_or_last_row_edit_still_absorbs_from_the_tail_first() {
    let l = full(ORNITH);
    let three = sel("ROCm0,CUDA0,Vulkan0", "1,1,1"); // projects to 12/11/11
    assert_eq!(
        block_counts(&[1, 1, 1], l),
        Some(vec![12, 11, 11]),
        "the starting point the cases below move from"
    );
    // Middle row up: only the tail moves.
    assert_eq!(
        set_blocks(&three, "CUDA0", 20, l),
        sel("ROCm0,CUDA0,Vulkan0", "12,20,2")
    );
    // Middle row up past what the tail holds: the HEAD gives up the rest;
    // it has to, or the count asked for cannot be honoured.
    assert_eq!(
        set_blocks(&three, "CUDA0", 30, l),
        sel("ROCm0,CUDA0,Vulkan0", "4,30,0")
    );
    // Last row: the row before it is the tail, and absorbs both directions.
    assert_eq!(
        set_blocks(&three, "Vulkan0", 20, l),
        sel("ROCm0,CUDA0,Vulkan0", "12,2,20")
    );
    assert_eq!(
        set_blocks(&three, "Vulkan0", 0, l),
        sel("ROCm0,CUDA0,Vulkan0", "12,22,0")
    );
}

// Every count written back must survive its own projection, or the cell would
// renumber itself the moment the table rebuilt.
#[test]
fn a_block_edit_round_trips_through_the_projection() {
    let l = full(ORNITH);
    for n in 0..=l.count {
        let s = set_blocks(&sel("ROCm0,CUDA0", "1,1"), "ROCm0", n, l);
        let got = block_counts(&parse_weights(&s.tensor_split), l);
        // A 0/34 or 34/0 split renders a real vector, never a blank one:
        // blank would mean auto, which is a different launch entirely.
        assert_eq!(got, Some(vec![n, l.count - n]), "{n} blocks");
    }
}

// The same round-trip over THREE devices and every position of the edit, which
// is where the cascade has somewhere to go wrong: the invariant is that the
// vector still sums to the positions, the edited device got exactly what was
// typed, and the ranges still tile the whole span with no gap or overlap.
#[test]
fn every_three_device_edit_keeps_the_budget_and_the_ranges_contiguous() {
    let l = full(ORNITH);
    let three = sel("ROCm0,CUDA0,Vulkan0", "5,20,9");
    for (i, id) in ["ROCm0", "CUDA0", "Vulkan0"].iter().enumerate() {
        for n in 0..=l.count {
            let s = set_blocks(&three, id, n, l);
            let counts = block_counts(&parse_weights(&s.tensor_split), l)
                .unwrap_or_else(|| panic!("{id}={n} rendered an auto vector"));
            assert_eq!(counts.iter().sum::<i32>(), l.count, "{id}={n} sum");
            assert_eq!(counts[i], n, "{id}={n} is what the cell will show back");
            // Contiguity: walk the labels and check each picks up where the
            // previous one left off, ending on the output layer.
            let mut pos = l.start;
            for (c, label) in counts.iter().zip(block_ranges(&counts, l)) {
                if *c == 0 {
                    assert_eq!(label, "nothing", "{id}={n}");
                    continue;
                }
                // Every label but one opens with its first position. The
                // exception is a lone tail, which is NAMED (`output layer`)
                // rather than numbered, and can only be the last position.
                match first_position(&label) {
                    Some(first) => assert_eq!(first, pos, "{id}={n}: {label}"),
                    None => {
                        assert_eq!(label, "output layer", "{id}={n}");
                        assert_eq!(pos, l.start + l.count - 1, "{id}={n}: {label}");
                    }
                }
                pos += c;
            }
            assert_eq!(pos, l.start + l.count, "{id}={n} covers every position");
        }
    }
}

// Three rows projected end to end, as the table draws them.
#[test]
fn the_range_labels_tile_a_three_device_split() {
    assert_eq!(
        block_ranges(&[12, 11, 11], full(ORNITH)),
        ["blocks 0-11", "blocks 12-22", "blocks 23-32 + output"]
    );
}

// ── Split mode ────────────────────────────────────────────────────────

// The flag domain, plus the deliberate degradations: the form's "default"
// sentinel and an explicit "layer" are the same verdict, and a value the
// flag does not know must NOT earn a block cut.
#[test]
fn split_mode_parses_the_flag_domain_and_degrades_unknowns_to_row() {
    assert_eq!(SplitMode::parse(""), SplitMode::Layer);
    assert_eq!(SplitMode::parse("default"), SplitMode::Layer);
    assert_eq!(SplitMode::parse("layer"), SplitMode::Layer);
    assert_eq!(SplitMode::parse(" none "), SplitMode::Single);
    assert_eq!(SplitMode::parse("row"), SplitMode::Row);
    assert_eq!(SplitMode::parse("tensor"), SplitMode::Tensor);
    assert_eq!(SplitMode::parse("bogus"), SplitMode::Row);
    assert_eq!(SplitMode::Single.as_str(), "none");
    assert_eq!(SplitMode::Tensor.as_str(), "tensor");
}

// One combo entry for default+layer: for a model profile they are the same launch
// in every reachable configuration, so two entries were pure ambiguity. A
// hand-written value outside the list sits on that entry too (and keeps its
// INI spelling; nothing here rewrites it). `tensor` has its own entry now;
// `row`, deprecated upstream, keeps one, last.
#[test]
fn mode_index_collapses_default_and_layer_onto_one_entry() {
    assert_eq!(MODE_VALUES.len(), MODE_LABELS.len());
    assert_eq!(mode_index(""), 0);
    assert_eq!(mode_index("default"), 0);
    assert_eq!(mode_index("layer"), 0);
    assert_eq!(mode_index("none"), 1);
    assert_eq!(mode_index("tensor"), 2);
    assert_eq!(mode_index("row"), 3);
    assert_eq!(mode_index("bogus"), 0);
    // The values written back by a pick are the INI/form spellings.
    assert_eq!(MODE_VALUES, ["default", "none", "tensor", "row"]);
    assert!(
        strings::t(MODE_LABELS[3]).contains("deprecated"),
        "row must say it is deprecated"
    );
}
