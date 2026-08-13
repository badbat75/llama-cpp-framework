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
//! ## The weights are RATIOS; the table edits them as BLOCKS
//! `--tensor-split` is normalized, so `85,15`, `17,3` and `29,5` are one launch.
//! What llama.cpp then does with the ratio is cut `n_layer_all + 1` positions in
//! two — every block plus the output layer — which means the knob has ~34 usable
//! settings on a 9B and a percent field offers 100 of them: 83 / 84 / 85% all cut
//! in the same place, so two of every three edits move a number and change
//! nothing. Hence `Layout` + `block_counts`: wherever a model is known the column
//! is shown and edited in blocks, which changes exactly when the launch does.
//!
//! Writing it back needs no inverse. A vector summing to the positions assigns
//! position `il` to the first device whose prefix sum passes it — so the weights
//! ARE the counts, and `set_blocks` just stores what it is given. What it cannot
//! do is store it alone: counts are constrained to the positions, so one going up
//! means another coming down (the back absorbs first). That constraint is also
//! what picks the widget — a `Slider` over `0..positions`, committing once on
//! `released`, because a share of a fixed budget is what a slider is for and
//! because applying it mid-drag would rebuild the row being dragged.
//!
//! The projection is one-way and re-derived on every rebuild — nothing persists a
//! block count. The INI keeps a ratio, which is the portable thing: re-point a
//! preset at a model with a different `block_count` and `29,5` still means 85%,
//! now cut somewhere else. The number in the cell is a VIEW of the ratio under the
//! current model, never the source of truth.
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

// ── Blocks: the split as llama.cpp will actually cut it ──────────────────

/// The positions one model's `--tensor-split` is resolved over.
///
/// llama.cpp does not split "layers" and it does not split by size: it splits
/// `n_layer_all + 1` POSITIONS — every block in the GGUF (the nextn/MTP ones
/// included, they are ordinary trailing blocks) plus one more for the output
/// layer, `dev_output = get_layer_buft_list(n_layer_all)` — and it NORMALIZES the
/// weight vector over them. So the weights are pure ratios at any scale, and the
/// thing the user actually chose is a cut point between two blocks.
///
/// That is why a percentage lies by ~3 points here: with 34 positions there are
/// only 34 places to cut, so 83 / 84 / 85% are the same launch and two of those
/// three edits change nothing. A block count cannot misreport that.
///
/// Partial offload takes positions off the FRONT — the first
/// `n_layer_all + 1 - n_gpu_layers` go to the CPU and the split runs over what is
/// left (`i_gpu_start` / `act_gpu_layers`, llama-model.cpp) — so `start` moves and
/// `count` shrinks as the GPU-layers slider comes down, and the counts shown to
/// the user move with it. That is the truth, not a glitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// First position handed to a GPU. Non-zero only under partial offload.
    pub start: i32,
    /// How many positions the split runs over. `start + count` is always
    /// `n_layer_all + 1`.
    pub count: i32,
}

impl Layout {
    /// `n_layer_all` is the GGUF's `block_count`; `n_gpu_layers` negative means
    /// "all", which is llama.cpp's own default (llama.h: *a negative value means
    /// all layers*). `None` whenever there is nothing to draw — no model read
    /// yet, or an offload of zero layers — and every caller then falls back to
    /// plain weights.
    pub fn new(n_layer_all: i32, n_gpu_layers: i32) -> Option<Self> {
        if n_layer_all <= 0 {
            return None;
        }
        let positions = n_layer_all + 1;
        let ngl = if n_gpu_layers < 0 {
            positions
        } else {
            n_gpu_layers.min(positions)
        };
        if ngl <= 0 {
            return None;
        }
        Some(Self {
            start: positions - ngl,
            count: ngl,
        })
    }

    /// The last position, which is the OUTPUT layer rather than a block — worth
    /// naming because it carries `output.weight`, routinely the single biggest
    /// tensor on its device (1.9 GiB on a 9B at BF16).
    fn output_position(self) -> i32 {
        self.start + self.count - 1
    }
}

/// How many positions each device ends up with, mirroring llama.cpp's assignment
/// exactly: position `il` goes to `upper_bound(splits, (il - i_gpu_start) /
/// act_gpu_layers)`, i.e. the first device whose cumulative share exceeds it.
///
/// Done in integers — `cum * count > pos * total` is that same comparison with
/// the two divisions cleared — which is not just faster but avoids inventing a
/// disagreement: llama.cpp's floats are exact here anyway (each side is one
/// correctly-rounded division of small integers, so equal ratios round equal and
/// the boundary case cannot drift between the two).
///
/// `None` for an all-zero vector: that is llama.cpp's auto mode, split by FREE
/// VRAM as measured at load, which nothing here can know ahead of time.
pub fn block_counts(weights: &[i32], layout: Layout) -> Option<Vec<i32>> {
    if weights.is_empty() || layout.count <= 0 {
        return None;
    }
    let mut cum: Vec<i64> = Vec::with_capacity(weights.len());
    let mut acc = 0i64;
    for &w in weights {
        acc += i64::from(w.max(0));
        cum.push(acc);
    }
    let total = acc;
    if total <= 0 {
        return None;
    }
    let count = i64::from(layout.count);
    let mut counts = vec![0i32; weights.len()];
    for pos in 0..count {
        let k = cum
            .iter()
            .position(|&c| c * count > pos * total)
            .unwrap_or(weights.len() - 1);
        counts[k] += 1;
    }
    Some(counts)
}

/// The per-device range label that goes with `block_counts`: `blocks 0-28`,
/// `blocks 29-32 + output`. The output layer is called out by name because it is
/// a position but not a block — "5 blocks" would otherwise quietly include the
/// model's biggest single tensor without saying so.
pub fn block_ranges(counts: &[i32], layout: Layout) -> Vec<String> {
    let mut out = Vec::with_capacity(counts.len());
    let mut pos = layout.start;
    for &c in counts {
        let first = pos;
        let last = pos + c - 1;
        pos += c;
        out.push(if c <= 0 {
            "nothing".to_string()
        } else if last >= layout.output_position() {
            match c {
                1 => "output layer".to_string(),
                2 => format!("block {first} + output"),
                _ => format!("blocks {first}-{} + output", last - 1),
            }
        } else if c == 1 {
            format!("block {first}")
        } else {
            format!("blocks {first}-{last}")
        });
    }
    out
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
    let to = usize::try_from(i64::from(delta) + from as i64)
        .unwrap_or(0)
        .min(last);
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

/// Set one device's BLOCK count — the same edit as `set_weight`, in the unit the
/// split is actually resolved in (see `Layout`).
///
/// The weights are rewritten as the counts themselves, which is EXACT rather than
/// approximate: a vector summing to `layout.count` gives position `il` to the
/// first device whose prefix sum exceeds it, so the weights *are* the counts.
/// `29,5` over 34 positions is blocks 0-28 / 29-33 — not an 85% that happens to
/// round there, and not a value that needs an inverse to be invented.
///
/// Unlike a weight, a count cannot be edited alone: the vector has to keep
/// summing to `layout.count`, so someone else has to give the difference up. The
/// rows absorb it from the BACK — the last row that isn't the edited one first,
/// then walking towards the head, taking from each only what it has. That keeps
/// the common edits local: with two devices the other row simply takes the
/// remainder, and with three, editing the head or the middle moves only the tail
/// until the tail runs out. A row before the edited one is touched only when
/// everything after it is already at zero, which is the point where SOMETHING in
/// front has to move or the request cannot be honoured at all.
pub fn set_blocks(sel: &GpuSelection, id: &str, blocks: i32, layout: Layout) -> GpuSelection {
    let mut picks = picks(sel);
    let Some(idx) = picks.iter().position(|(d, _)| d.eq_ignore_ascii_case(id)) else {
        return sel.clone();
    };
    let n = picks.len();
    // Auto has no counts to start from, so seed an even cut — the same "make the
    // implicit explicit before applying the edit" move `set_weight` makes.
    let mut counts = block_counts(&weights_of(&picks), layout)
        .unwrap_or_else(|| even_counts(n, layout.count));

    let want = blocks.clamp(0, layout.count);
    let mut delta = want - counts[idx];
    counts[idx] = want;
    for j in (0..n).rev() {
        if j == idx || delta == 0 {
            continue;
        }
        if delta > 0 {
            let take = delta.min(counts[j]);
            counts[j] -= take;
            delta -= take;
        } else {
            counts[j] -= delta;
            delta = 0;
        }
    }
    // Everyone else was already at zero: the edited row cannot have what it asked
    // for, so it keeps only what was actually free.
    counts[idx] -= delta;

    for (p, c) in picks.iter_mut().zip(counts) {
        p.1 = c;
    }
    render(&picks)
}

fn weights_of(picks: &[Pick]) -> Vec<i32> {
    picks.iter().map(|&(_, w)| w).collect()
}

/// `count` positions over `n` devices, remainder to the back — the same direction
/// `set_blocks` absorbs in.
fn even_counts(n: usize, count: i32) -> Vec<i32> {
    if n == 0 {
        return Vec::new();
    }
    let n_i32 = i32::try_from(n).unwrap_or(i32::MAX);
    let base = count / n_i32;
    let rem = count - base * n_i32;
    (0..n)
        .map(|i| base + i32::from(i >= n - usize::try_from(rem).unwrap_or(0)))
        .collect()
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
///
/// `layout` is the model to project the weights onto (`Layout`), and it is what
/// turns the ratio column into a block column. `None` — the server-wide table,
/// which applies to every preset and so has no single `n_layer_all`, or a model
/// whose header hasn't been read — leaves `blocks`/`blocks_label` empty and the
/// table falls back to editing raw weights.
pub fn build_rows(
    devices: &[DeviceOption],
    sel: &GpuSelection,
    layout: Option<Layout>,
) -> Vec<GpuRow> {
    let picks = picks(sel);
    let total: i32 = picks.iter().map(|&(_, w)| w).sum();
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
            name: dev.map_or("(not detected)", |d| d.name.as_str()).into(),
            vram: dev
                .map(DeviceOption::vram_summary)
                .map_or_else(SharedString::new, Into::into),
            detected: dev.is_some(),
            enabled: true,
            weight: *weight,
            share: share(picks.len(), *weight, total).into(),
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
        let rows = build_rows(&devs(), &sel("ROCm0,CUDA0", "3,1"), None);
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
        assert_eq!(ids(&build_rows(&devs(), &s, None))[0], "ROCm0");
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
        let rows = build_rows(&devs(), &sel("ROCm0,CUDA0", "3,1"), None);
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
        let rows = build_rows(&devs(), &sel("ROCm0,CUDA0", ""), None);
        assert_eq!(rows[0].share, "auto");
        assert_eq!(rows[1].share, "auto");
    }

    #[test]
    fn a_single_selected_device_holds_the_whole_model() {
        let rows = build_rows(&devs(), &sel("ROCm0", ""), None);
        assert_eq!(rows[0].share, "100%");
    }

    #[test]
    fn a_selected_id_the_probe_doesnt_know_survives_as_an_undetected_row() {
        // The probe is async: the GUI can render before it lands, and a config
        // may name a device from another machine. Dropping the row would let the
        // next save quietly rewrite `device`.
        let rows = build_rows(&devs(), &sel("SYCL3,ROCm0", "1,1"), None);
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
        let rows = build_rows(&[], &sel("ROCm0", ""), None);
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
        assert_eq!(ids(&build_rows(&devs(), &hand, None))[0], "Vulkan0");
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

    // Even is all-1s (see the module header), which is NOT one block each — the
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
        assert_eq!(Layout::new(ORNITH, -1), Some(Layout { start: 0, count: 34 }));
        assert_eq!(
            Layout::new(ORNITH, 99),
            Some(Layout { start: 0, count: 34 }),
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
    // and it carries `output.weight` — 1.9 GiB on a 9B at BF16, routinely the
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
        assert_eq!(block_ranges(&[32, 2], l), ["blocks 0-31", "block 32 + output"]);
        assert_eq!(block_ranges(&[34, 0], l), ["blocks 0-32 + output", "nothing"]);
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
        // nothing about the cut — 85,15 and 29,5 are the same launch.
        let same = set_blocks(&two, "ROCm0", 29, l);
        assert_eq!(same, sel("ROCm0,CUDA0", "29,5"));
        assert_eq!(block_counts(&[29, 5], l), block_counts(&[85, 15], l));
        // Giving the first device less hands the surplus to the other one.
        assert_eq!(set_blocks(&two, "ROCm0", 20, l), sel("ROCm0,CUDA0", "20,14"));
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
        assert_eq!(set_blocks(&two, "ROCm0", 999, l), sel("ROCm0,CUDA0", "34,0"));
        assert_eq!(set_blocks(&two, "ROCm0", -5, l), sel("ROCm0,CUDA0", "0,34"));
        // An unknown id is a no-op, like every other edit here.
        assert_eq!(set_blocks(&two, "CUDA9", 4, l), two);
    }

    // Editing anything but the head: the tail still pays first, and a row IN FRONT
    // of the edited one is touched only once everything behind it is at zero.
    // (Three devices is where that direction becomes observable at all — with two
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
        // Middle row up past what the tail holds: the HEAD gives up the rest —
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
            // A 0/34 or 34/0 split renders a real vector, never a blank one —
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
                    // rather than numbered — and can only be the last position.
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

    // The rows carry the projection only when a model is there to project onto —
    // the server-wide table passes None and stays a ratio table.
    #[test]
    fn rows_carry_blocks_only_when_a_layout_is_given() {
        let s = sel("ROCm0,CUDA0", "85,15");
        let with = build_rows(&devs(), &s, Some(full(ORNITH)));
        assert_eq!((with[0].blocks, with[1].blocks), (29, 5));
        assert_eq!(with[0].blocks_label, "blocks 0-28");
        assert_eq!(with[1].blocks_label, "blocks 29-32 + output");
        assert_eq!(with[0].weight, 85, "the stored ratio is untouched");
        // Unchecked rows have no position in the split.
        assert_eq!(with[2].blocks, 0);
        assert_eq!(with[2].blocks_label, "");

        let without = build_rows(&devs(), &s, None);
        assert!(without.iter().all(|r| r.blocks == 0 && r.blocks_label.is_empty()));

        // Three checked devices, projected the same way — the rows are the split
        // order, so the ranges run down the table in one unbroken sequence.
        let three = build_rows(
            &devs(),
            &sel("ROCm0,CUDA0,Vulkan0", "5,20,9"),
            Some(full(ORNITH)),
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
}
