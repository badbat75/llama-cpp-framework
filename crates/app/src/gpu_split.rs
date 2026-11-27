//! The GPU distribution table: which devices a model runs on, and in what
//! proportion. Pure logic (no IO, no Slint state) so every rule below is unit
//! tested; `gui::refresh_gpu_rows` is the thin shell that pushes the rows into
//! `AppState` and writes the result back into the form.
//!
//! ## Why a table and not two text fields
//! `--tensor-split` is a positional vector indexed over the **filtered** device
//! list: the devices named by `--device`, in `--device` order (llama.cpp's
//! `llama-model.cpp` copies `tensor_split[0..n_devices()]` over `model->devices`,
//! and `docs/multi-gpu.md`: *"The values follow the order in --device"*). With
//! `--device` unset that list is every detected backend, which on a mixed box is
//! CUDA0 + two ROCm + three duplicate Vulkan views of the same three GPUs, so a
//! weight vector typed by hand is indexed against a list the user can't see and
//! didn't choose. The two fields only make sense together, which is what this
//! module models: one selection, rendered into the `device` + `tensor_split`
//! strings a model profile stores in `models.ini`.
//!
//! ## The four states (mirroring llama.cpp exactly)
//! | selected | `device`        | `tensor_split` | meaning                                       |
//! |----------|-----------------|----------------|-----------------------------------------------|
//! | 0        | `""`            | `""`           | llama.cpp uses all detected devices           |
//! | 1        | `"ROCm1"`       | `""`           | one GPU, nothing to split                     |
//! | ≥2 auto  | `"ROCm1,CUDA0"` | `""`           | llama.cpp splits by **free** VRAM at load     |
//! | ≥2 fixed | `"ROCm1,CUDA0"` | `"3,1"`        | explicit proportions                          |
//!
//! Note the third row: a blank `tensor_split` is *auto-by-free-VRAM*, NOT an even
//! split (llama.cpp's `all_zero` branch fills `splits[i]` with each device's free
//! memory). That's why the table offers **Auto** (clear) and **Even** (all 1s) as
//! two distinct buttons; they are two different launches.
//!
//! The selection is never held as separate state: it is derived from the form's
//! two strings on every rebuild and rendered straight back into them, so a
//! hand-edited INI stays authoritative and there is no third copy to desync.
//!
//! ## The weights are RATIOS; the table edits them as BLOCKS
//! `--tensor-split` is normalized, so `85,15`, `17,3` and `29,5` are one launch.
//! What llama.cpp then does with the ratio is cut `n_layer_all + 1` positions in
//! two (every block plus the output layer), which means the knob has ~34 usable
//! settings on a 9B and a percent field offers 100 of them: 83 / 84 / 85% all cut
//! in the same place, so two of every three edits move a number and change
//! nothing. Hence `Layout` + `block_counts`: wherever a model is known the column
//! is shown and edited in blocks, which changes exactly when the launch does.
//!
//! Writing it back needs no inverse. A vector summing to the positions assigns
//! position `il` to the first device whose prefix sum passes it, so the weights
//! ARE the counts, and `set_blocks` just stores what it is given. What it cannot
//! do is store it alone: counts are constrained to the positions, so one going up
//! means another coming down (the back absorbs first). That constraint is also
//! what picks the widget: a `Slider` over `0..positions`, committing once on
//! `released`, because a share of a fixed budget is what a slider is for and
//! because applying it mid-drag would rebuild the row being dragged.
//!
//! The projection is one-way and re-derived on every rebuild; nothing persists a
//! block count. The INI keeps a ratio, which is the portable thing: re-point a
//! model profile at a model with a different `block_count` and `29,5` still means 85%,
//! now cut somewhere else. The number in the cell is a VIEW of the ratio under the
//! current model, never the source of truth.
//!
//! ## The row order IS the split order, and the user owns it
//! Checked devices come FIRST, in `--device` order, then the rest in probe order.
//! So the table read top to bottom is literally the `--device` list and its
//! `--tensor-split` vector, and the drag handle (`move_by`) is how you change it.
//!
//! Position 0 is not cosmetic: it is `devices[0]`, which is also `main_gpu`
//! (llama.cpp defaults `--main-gpu` to 0, and the framework never overrides it).
//! With `--split-mode none` that is the ONE GPU llama.cpp keeps; in `layer` mode
//! it takes the first slice of layers. Without a way to reorder there was no way
//! to put a chosen GPU at the head, which is why the handle exists.
//!
//! Checking a device APPENDS it (last in the split, weight 1 if the others are
//! weighted); unchecking removes it and its weight together. Weights are carried
//! in the same tuple as their device, so a reorder moves them as a unit: the
//! split proportions survive, only the positions change.
//!
//! ## The split MODE decides what the columns mean
//! `--split-mode` is part of the same selection, which is why the combo sits
//! ABOVE the table: under `layer` (llama.cpp's default) the vector cuts blocks,
//! so the table edits blocks wherever a model is known; under `none` llama.cpp
//! keeps ONLY the head row (`devices[main_gpu]`, and the framework never moves
//! `--main-gpu` off 0) and the vector is dead, so the editors go away and the
//! head row reads 100%; under `row` the weight MATRICES are split row-wise by
//! the normalized vector (backend split buffer type; the layer cut still runs
//! over the same vector, so the KV cache and non-matrix tensors follow it, but
//! the bulk of the bytes follow the row fractions), so the honest editable unit
//! is the raw ratio and never blocks; under `tensor` (llama.cpp's tensor
//! parallelism, experimental upstream and what deprecated `row`) the checked
//! devices become ONE meta device and every weight AND the KV cache are cut
//! along a tensor axis by the same normalized vector
//! (`llama_meta_device_get_split_state`, `llama-model.cpp`), so it is a ratio
//! there too, with one difference the share column has to state: a BLANK
//! vector is an EVEN cut, not the free-VRAM weighting the other modes fall back
//! to. `SplitMode` is that verdict. The whole distribution (device,
//! tensor-split, split-mode) is per model profile only: server-wide it would ride the
//! router's own command line and shadow every model profile's.

use crate::strings;

/// The two INI strings the table drives, always produced as a pair.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GpuSelection {
    /// `--device` value: comma-separated ids in split order, or empty.
    pub device: String,
    /// `--tensor-split` value: comma-separated weights parallel to `device`, or
    /// empty for "let llama.cpp decide".
    pub tensor_split: String,
}

/// A selected device and its weight. Weight 0 means "unweighted": when EVERY
/// selected device is 0 the selection is in auto mode and renders a blank
/// `tensor_split`.
pub type Pick = (String, i32);

// ── Split mode ───────────────────────────────────────────────────────────

/// What `--split-mode` makes of the selection (the DISPLAY verdict, not the
/// flag domain): a value this code does not know degrades to `Row` (ratio
/// editing, no block projection) because inventing a block cut for an unknown
/// mode would misreport exactly what the mode handling exists to make honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplitMode {
    /// `layer`, and the absent flag (llama.cpp defaults to layer): the vector
    /// cuts blocks, so the table may edit blocks (`Layout`).
    #[default]
    Layer,
    /// `none`: llama.cpp drops every device but `devices[main_gpu]`, the head
    /// row, before loading (`llama_prepare_model_devices`), so the weight
    /// vector means nothing and the drag handle is the only knob left.
    Single,
    /// `row` (and any unknown value): weight matrices are split row-wise by the
    /// normalized vector via the backend's split buffer type (CUDA/ROCm; a
    /// backend without one, Vulkan, quietly falls back to the layer cut).
    /// The layer assignment still runs over the same vector, so KV and the
    /// non-matrix tensors follow it, but the bulk of the bytes follow the row
    /// fractions, so the editable unit is the ratio, never blocks. DEPRECATED
    /// upstream since `tensor` landed (`docs/multi-gpu.md`: "superseded by
    /// `tensor` which should be universally superior if it can be used"), but
    /// still accepted by the flag, so it stays listed: an INI that carries it
    /// must show it, not read as layer.
    Row,
    /// `tensor`: llama.cpp's tensor parallelism (b8738, PR #19378, experimental
    /// upstream). The checked devices become ONE meta device, in `--device`
    /// order, and every weight AND the KV cache are cut along a tensor axis by
    /// the normalized vector (`llama_meta_device_get_split_state`), so the
    /// editable unit is the ratio (no block cut exists), and a BLANK vector is an
    /// EVEN cut (`tensor_split_scan.back() == 0` → `ne_s * (j+1) / n_devices`),
    /// not the free-VRAM weighting of the other modes. `main_gpu` means nothing
    /// here: there is one device. Three things the LAUNCH checks that no row can
    /// show, hence the second warning strip under the combo
    /// (`model_profile_split_mode_warning`): the arch must be one upstream implements
    /// (`gguf::arch_supports_sm_tensor`), flash attention must not be `off`
    /// (`auto` is promoted to on), and with no device pinned in either scope
    /// llama.cpp pools EVERY detected device into the split, duplicate Vulkan
    /// views included, since only the other modes dedup by device id. `--fit`
    /// is skipped with a log warning, so `ctx-size` is the user's to size.
    Tensor,
}

impl SplitMode {
    /// The INI/form spelling → the verdict. "" and the form's "default"
    /// sentinel are Layer (llama.cpp's own default), and so is an explicit
    /// `layer`.
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "none" => Self::Single,
            "" | "default" | "layer" => Self::Layer,
            "tensor" => Self::Tensor,
            _ => Self::Row,
        }
    }

    /// The spelling the tables' gating property carries
    /// ("layer"/"none"/"tensor"/"row").
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Layer => "layer",
            Self::Single => "none",
            Self::Row => "row",
            Self::Tensor => "tensor",
        }
    }
}

/// The split-mode combo as label/value/index rows rather than an enum list: the
/// old list spelled "default" and "layer" as two entries, which for a model profile are
/// the SAME launch in every reachable configuration (an absent key falls back to
/// layer), so the duplicate entry was pure ambiguity. One entry now carries both;
/// picking it stores "default", which the form conversions drop from the INI.
/// `row` sits LAST and says so: deprecated upstream (superseded by `tensor`),
/// it stays listed because the flag still accepts it and an INI that carries it
/// has to show it rather than silently reading as layer.
pub const MODE_VALUES: [&str; 4] = ["default", "none", "tensor", "row"];
pub const MODE_LABELS: [&str; 4] = [
    strings::key("split-layer"),
    strings::key("split-none"),
    strings::key("split-tensor"),
    strings::key("split-row"),
];

/// The combo row for a form value, aligned with `MODE_VALUES`. Everything that
/// is not `none`/`tensor`/`row` sits on the merged layer entry, including a
/// hand-written value this build doesn't list, which keeps its INI spelling
/// untouched until the user actually picks a row.
pub fn mode_index(form_value: &str) -> i32 {
    match form_value.trim() {
        "none" => 1,
        "tensor" => 2,
        "row" => 3,
        _ => 0,
    }
}

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

/// Whether a `device` list names both a CUDA and a ROCm device. Under split
/// mode `tensor` that pair does not load unless `GGML_CUDA_ALLREDUCE=none`:
/// ggml-cuda and ggml-hip share one backend guid, so each brings its direct
/// AllReduce up over the other's device and aborts on the first reduction
/// (`server_cfg::cuda_allreduce` has the measurement). Vulkan devices are not
/// part of it: they never pass either backend's check.
pub fn mixes_cuda_and_rocm(device_list: &str) -> bool {
    let devs = parse_device_list(device_list);
    let has = |prefix: &str| devs.iter().any(|d| d.starts_with(prefix));
    has("CUDA") && has("ROCm")
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
pub fn picks(sel: &GpuSelection) -> Vec<Pick> {
    let weights = parse_weights(&sel.tensor_split);
    parse_device_list(&sel.device)
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id, weights.get(i).copied().unwrap_or(0)))
        .collect()
}

/// Render picks back into the INI string pair, IN THE GIVEN ORDER (that order is
/// the user's; see the module header): the one place the four-state table above
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
/// `n_layer_all + 1` POSITIONS, i.e. every block in the GGUF (the nextn/MTP ones
/// included, they are ordinary trailing blocks) plus one more for the output
/// layer, `dev_output = get_layer_buft_list(n_layer_all)`; and it NORMALIZES the
/// weight vector over them. So the weights are pure ratios at any scale, and the
/// thing the user actually chose is a cut point between two blocks.
///
/// That is why a percentage lies by ~3 points here: with 34 positions there are
/// only 34 places to cut, so 83 / 84 / 85% are the same launch and two of those
/// three edits change nothing. A block count cannot misreport that.
///
/// Partial offload takes positions off the FRONT: the first
/// `n_layer_all + 1 - n_gpu_layers` go to the CPU and the split runs over what is
/// left (`i_gpu_start` / `act_gpu_layers`, llama-model.cpp), so `start` moves and
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
    /// all layers*). `None` whenever there is nothing to draw (no model read
    /// yet, or an offload of zero layers), and every caller then falls back to
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

    /// The last position, which is the OUTPUT layer rather than a block, worth
    /// naming because it carries `output.weight`, routinely the single biggest
    /// tensor on its device.
    fn output_position(self) -> i32 {
        self.start + self.count - 1
    }
}

/// How many positions each device ends up with, mirroring llama.cpp's assignment
/// exactly: position `il` goes to `upper_bound(splits, (il - i_gpu_start) /
/// act_gpu_layers)`, i.e. the first device whose cumulative share exceeds it.
///
/// Done in integers (`cum * count > pos * total` is that same comparison with
/// the two divisions cleared), which is not just faster but avoids inventing a
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
/// a position but not a block; "5 blocks" would otherwise quietly include the
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
                1 => strings::t("output-layer"),
                2 => strings::fill("block-plus-output", &[&first]),
                _ => strings::fill("blocks-plus-output", &[&first, &(last - 1)]),
            }
        } else if c == 1 {
            strings::fill("block-one", &[&first])
        } else {
            strings::fill("blocks-range", &[&first, &last])
        });
    }
    out
}

// ── Edits (each returns the new selection; the caller writes it to the form) ──

/// Check / uncheck a device. A newly checked one is APPENDED: last in the split,
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
/// choose which GPU sits at position 0: `devices[0]` is llama.cpp's `main_gpu`
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

/// Set one device's BLOCK count: the same edit as `set_weight`, in the unit the
/// split is actually resolved in (see `Layout`).
///
/// The weights are rewritten as the counts themselves, which is EXACT rather than
/// approximate: a vector summing to `layout.count` gives position `il` to the
/// first device whose prefix sum exceeds it, so the weights *are* the counts.
/// `29,5` over 34 positions is blocks 0-28 / 29-33, not an 85% that happens to
/// round there, and not a value that needs an inverse to be invented.
///
/// Unlike a weight, a count cannot be edited alone: the vector has to keep
/// summing to `layout.count`, so someone else has to give the difference up. The
/// rows absorb it from the BACK: the last row that isn't the edited one first,
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
    // Auto has no counts to start from, so seed an even cut, the same "make the
    // implicit explicit before applying the edit" move `set_weight` makes.
    let mut counts =
        block_counts(&weights_of(&picks), layout).unwrap_or_else(|| even_counts(n, layout.count));

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

pub fn weights_of(picks: &[Pick]) -> Vec<i32> {
    picks.iter().map(|&(_, w)| w).collect()
}

/// `count` positions over `n` devices, remainder to the back: the same direction
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

#[cfg(test)]
#[path = "gpu_split_tests.rs"]
mod tests;
