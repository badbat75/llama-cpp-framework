# Brief 19: layer sliders on the model profile page (slices LS1 and LS2)

`PLAN.md` step 10, "GPU distribution on the model profile page", order of 2026-11-21, slice 2g of
`ARCHITECTURE.md` "Layers, MoE and tensor placement on the web model profile page" (accepted, user
2026-11-21: the sliders read the SAVED model; an `hf-repo` not downloaded gives no layer facts; the
manager refuses a `device-draft` or `mmproj-device` naming a GPU outside the instance's server profile
`Devices`; `moe-cache-mib` stays off the web). It builds on brief 14 (GD1, GD2: the GPU card, its
weight fields, shares and Move buttons, all built). Criteria are numbered **19 G.n**; test names cite
them so (`19 G.4`).

**Two sub-slices, one tester and one coder run each, LS1 first** (designer playbook: more than about
12 criteria per run fails):

| Sub-slice | Criteria | Holds |
|---|---|---|
| **LS1** | 19 G.1 to 19 G.12 | The model facts read and its states, the block arithmetic, the per-GPU block slider and number in the GPU card, Auto and Even there, saving `tensor-split` as block counts |
| **LS2** | 19 G.13 to 19 G.19 | The range beside the GPU layers and MoE CPU layers fields, `n-cpu-moe` editable with its states, the Draft device and Vision GPU lists from the probe, the device bound refusal |

A tester on LS2 reads sections 0, 1, 2 and the LS2 criteria; 19 G.1 and 19 G.2 (the read and its
states) are LS1's and LS2 uses them.

## 0. What exists

- **`GET /api/profiles/model/{id}/model-info`** (bff slice L2, designed, not yet served;
  `ARCHITECTURE.md` (1) "The bff route"): behind a session, the manager asked as the signed-in user.
  200 `{"model": "/abs/x.gguf" | null, "info": {"blockCount": 64, "moe": true, "expertCount": 128,
  "expertUsed": 8} | null, "reason": {"key": "...", "args": [...]} | null}`. `info` is null exactly when
  `reason` is set, with one of: `model-info-fetched` `[key]` (an `hf-repo`, `model-url` or
  `docker-repo` in the section: nothing read), `model-profile-no-model` `[id]`, `model-info-missing`
  `[path]`, `model-info-unreadable` `[path]`. `blockCount` is the GGUF's `<arch>.block_count` (every
  block, MTP ones included); `expertCount`, `expertUsed` are 0 when the header has none. Failures: 401
  `session-*`, 403 (the session ended), 404 `no-model-profile-named`, 503 `session-unavailable`, 504 on
  the manager's 5 s deadline. It reads the **saved** `model` (decision 1); this page does not edit
  `model`.
- **`n-cpu-moe` on the web** (L3): the PUT accepts it, a whole number from 0 to 4096; outside, 422
  `not-whole-number-in-range` `[key, value, 0, 4096]` with `field` `n-cpu-moe` (already mapped by 02h:
  the field's error, `api-not-whole-number-in-range` exists in `en_US.json`).
- **The device bound** (L4): a save or an assignment whose `device-draft` or `mmproj-device` names a GPU
  outside the server profile `Devices` of an instance the profile is in effect on is 422
  `model-profile-device-outside` `[profile, instance, key, device]`, nothing written.
- **The GUI** (`crates/app/src/gpu_split.rs` `Layout`, `block_counts`, `block_ranges`, `set_blocks`;
  `crates/config/ui/components.slint` `GpuSplitTable`, about 770 to 1240): under split mode `layer`
  with a model read and an **explicit** vector (sum of weights above 0), each checked row's editor is a
  slider `0..positions` with its live count beside it and a range label under the name ("blocks 0-28",
  "blocks 29-32 + output"); the column header says "Blocks/34"; the share is `count * 100 /
  positions`. `positions` is `Layout.count`: `blockCount + 1` (every block plus the output layer),
  less the positions partial offload leaves on the CPU (`n-gpu-layers`). In Auto (blank vector) there
  are no counts (llama.cpp splits by free VRAM at load): the weight fields stay, disabled; Even (all
  `1`) is the way into blocks. A slider commits on release; `set_blocks` writes the counts as the
  weights and takes the difference from the back rows first. GPU layers and MoE CPU layers are
  `AutoSlider`s (a "default" box plus a slider `0..block_count`), MoE CPU layers usable only on a MoE
  model (`state.slint` 346, `models_page.slint` 483, 494). Draft device and Vision GPU are combos of
  the probed devices, "`id`: `description`", an unlisted current value kept as "(custom) `id`"
  (`crates/app/src/devices.rs` `build_options`).
- **The page today** (`fe/src/GpuSplit.tsx`, `fe/src/ModelProfileCards.tsx`): the GPU card of brief 14;
  in Hardware, GPU layers a number field with its default box, MoE CPU layers and MoE cache disabled
  under the note `model-profile-moe-later`; Draft device and Vision GPU device selects holding only the
  file's value.

## 1. Decisions

1. **One read of the model facts per arrival**, beside the profile and device reads, never polled and
   never re-read after a save (this page cannot change `model`). A failed read never signs the app out
   and never blocks the page, as brief 14 decision 5: the next save meets an ended session.
2. **Blocks only where the GUI shows them**: split mode `layer` (the key absent, `""` or `layer`), the
   facts read with a positive `blockCount`, a layout (`n-gpu-layers` not 0), two or more GPUs checked.
   Everywhere else the rows keep brief 14's weight fields (`row`, `tensor`, no facts) or nothing
   (`none`, one GPU). `tensor-split` stays the stored key, holding the counts as weights exactly as the
   GUI does; a ratio already in the file shows as its block counts and is never rewritten until the
   user moves a block control.
3. **In blocks mode the weight field never shows, and the editor never changes kind under the focus.**
   The kind of editor depends on things the user changes elsewhere (the split mode select, a checkbox,
   Auto, Even, GPU layers in another card) or on the facts read, never on typing in the editor itself.
   So: while the facts read is pending, the rows show no weight editor at all (their shares stay);
   in Auto (every weight empty or 0) the rows show no editor and a line says Even is the way in, as the
   GUI's disabled fields do; a malformed vector shows a line instead of fields. A weight field whose
   typing could make the vector explicit, and so swap itself for a slider, never exists in blocks mode
   ([3.2.2 On Input](https://www.w3.org/TR/WCAG22/#on-input), [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)).
4. **A slider and a number field per GPU, both editing the same count** (user, 2026-11-20: "slider
   per la spartizione dei layer"; brief 14 decision 1 weighed again now that the budget is known). The
   slider gives the cut at a glance and by arrow keys; the number types an exact count. A native
   `<input type="range">` (role `slider`, arrows, Home, End, a click on the track: a single pointer
   operation, [2.5.7](https://www.w3.org/TR/WCAG22/#dragging-movements)), not a custom widget
   ([ARIA in HTML](https://www.w3.org/TR/html-aria/), `input type=range`).
5. **The slider applies as it moves; the number applies on Enter or when it loses focus.** Moving one
   count moves the back rows, so a number typed digit by digit ("2" then "29") would move the other rows
   twice through a value nobody meant. A pointer drag is one edit from where it started, as the GUI's
   commit on release, so dragging up and back leaves the other rows as they were.
6. **The range beside a number field** (GPU layers, MoE CPU layers): the existing field (default box
   plus text) gains a slider `0..blockCount` after its text input, both in sync, only while the facts
   are read. The text stays the value (it may hold `auto`, `all` or a number above the count); the slider
   is a view of it and a way to set it.
7. **MoE CPU layers** is editable unless the facts say the model is dense; while the facts are pending
   it is disabled, so it never turns disabled under the focus. With no facts (fetched, missing,
   unreadable, failed) it is editable as a plain number field, the server checking 0 to 4096.
8. **Draft device and Vision GPU list the probed GPUs** as the GUI's combos do, the file's value kept;
   Vision GPU also offers `none` (llama.cpp: "none = don't offload", `common/arg.cpp` `--mmproj-device`,
   v1.0.0). Draft device does not (the GUI offers neither; Draft GPU layers 0 does the same).
9. **Deliberate differences from the GUI**: the number beside each block slider is editable (the GUI's
   is a readout); the budget is a sentence, not a "Blocks/34" column header (there are no columns); the
   facts line sits in the GPU card, not a Model info card (02e 83's reserved card stays unbuilt); an
   unlisted device value is an extra last option "(not detected)", not "(custom)" (fe/DESIGN.md 5.2
   Select, brief 14 GD1.6).

## 2. The texts

`…` is U+2026. Numbers, `%`, ids and key names are literals, never strings (`fe/DESIGN.md` 7).

| Key | English | From |
|---|---|---|
| `model-profile-model-facts-reading` | `Reading the model file…` | web |
| `model-profile-model-facts` | `Model file: {0} blocks.` | web |
| `model-profile-model-facts-moe` | `Model file: {0} blocks, with experts: {1} per layer, {2} used per token.` | web |
| `model-profile-model-facts-moe-plain` | `Model file: {0} blocks, with experts.` | web |
| `model-profile-model-facts-none` | `No layer counts: {0}. The GPU weights stay ratios, and the layer fields have no slider.` | web |
| `model-profile-model-facts-failed` | `The model file's layer counts could not be read. The GPU weights stay ratios, and the layer fields have no slider; open the page again to retry.` | web |
| `api-model-info-fetched` | ``its model is fetched at startup (`{0}`), so its layer facts are unknown until it is downloaded`` | toml 997 |
| `api-model-info-missing` | ``the model file `{0}` is not there`` | toml 998 |
| `api-model-info-unreadable` | `` `{0}` is not a readable model file `` | toml 999 |
| `model-profile-gpu-blocks` | `Blocks` | web (the GUI's `blocks-of-positions` header, toml 492, without the count) |
| `model-profile-gpu-blocks-budget` | `The checked GPUs share {0} places: {1} blocks and the output layer.` | web |
| `model-profile-gpu-blocks-auto` | `Auto: llama.cpp shares the {0} places by free memory at load. Choose Even to set the blocks yourself.` | web |
| `model-profile-gpu-blocks-malformed` | `tensor-split {0} is not a list of numbers of 0 or more. Choose Even to set the blocks yourself, or Auto.` | web |
| `model-profile-gpu-blocks-value` | `{0} of {1}: {2}` | web |
| `model-profile-gpu-blocks-nothing` | `nothing` | the GUI's literal (`gpu_split.rs` `block_ranges`) |
| `model-profile-gpu-block-one` | `block {0}` | toml 811 |
| `model-profile-gpu-blocks-range` | `blocks {0}-{1}` | toml 812 |
| `model-profile-gpu-block-plus-output` | `block {0} + output` | toml 809 |
| `model-profile-gpu-blocks-plus-output` | `blocks {0}-{1} + output` | toml 810 |
| `model-profile-gpu-output-layer` | `output layer` | toml 808 |
| `model-profile-layers-value` | `{0} of {1}` | web |
| `model-profile-moe-reading` | `Reading the model file to see whether it has experts…` | web |
| `model-profile-moe-dense` | `This model has no experts (a dense model), so MoE CPU layers does nothing for it. Use Dense FFN CPU layers below.` | web, from toml 259 |
| `model-profile-device-option` | `{0}: {1}` | the GUI's label shape (`crates/core/src/devices.rs` 86) |
| `model-profile-device-option-missing` | `{0} (not detected)` | web, from toml 813 |
| `model-profile-device-none` | `none: not on a GPU` | web, from llama.cpp's help ("none = don't offload") |
| `api-model-profile-device-outside` | `{2} = {3} names a GPU outside the Devices of instance {1}'s server profile, so model profile {0} may not use it. Pick a GPU of those Devices, or widen the server profile's Devices first` | proposed for the toml line L4 adds; copied verbatim from what L4 commits, no closing full stop (02h 150) |

Changed texts (same keys; tests that read them through `t()` keep passing):

| Key | New English |
|---|---|
| `model-profile-gpu-distribution-intro` | `Which GPUs run THIS model and how much each holds, saved to models.ini. Check nothing to let llama.cpp choose. With two GPUs of different sizes, check both and give the bigger one more; left on Auto, the split follows free VRAM at load, not an even cut. Under split mode layer, once the model file is read, each GPU takes a number of blocks: the model's blocks and its output layer, shared out top to bottom. Otherwise the weights are ratios: 3 and 1 give 75% and 25%. The checked GPUs, top to bottom, are the split order: Move up and Move down change it; the first one is llama.cpp's main GPU.\nKeys: split-mode, device, tensor-split.` |
| `model-profile-gpu-layers-hint` | `How many of the model's layers run on the GPU. The rest stay on the CPU: less VRAM, slower.\nSlider maximum = all layers, once the model file is read.\nDefault: all layers on the GPU. A model too big for it fails to load, unless Fit to VRAM is on.\nFlag: --n-gpu-layers` (the GUI's line 257 back, with the condition) |
| `model-profile-moe-cpu-layers-hint` | `Frees VRAM by keeping the experts of this many layers on the CPU; generation gets slower. Only for MoE models: on a dense model use 'Dense FFN CPU layers' below.\nSlider maximum = the model's layer count, once the model file is read.\nDefault: 0, every expert stays on the GPU with its layer.\nFlag: --n-cpu-moe` |
| `model-profile-moe-later` | `MoE cache stays as the file has it: its floor needs the model's tensors, which this page does not read yet. Change it in pai-config on the server machine.` |

LS1 adds the keys from `model-profile-model-facts-reading` to `model-profile-gpu-output-layer` and
changes the intro; LS2 adds the rest and changes the other three.

## 3. Criteria

### LS1: the facts and the block sliders

**19 G.1 The facts read.** On each arrival at a model profile's page, the page sends one `GET
/api/profiles/model/<id>/model-info`, the id encoded as the page's profile read encodes it, beside the
profile and device reads (the same once-per-mount guard against the development StrictMode as the
device read). Leaving and coming back sends another; a save, Discard, Reload, any edit sends none;
nothing polls. Time limit 15 s (`fe/DESIGN.md` 7), a time-out being its network failure. A good reply
is a 200 object where either `info` is an object with `blockCount` a whole number above 0, `moe` a
boolean, `expertCount` and `expertUsed` whole numbers of 0 or more (any other value of these two reads
as 0), or `info` is null and `reason` is an object with a string `key` and an array `args`. Anything
else (another status, a network failure, a time-out, a body not of that shape, a `reason` key not one
of the four of section 0) is a failed read. The read never changes the session (decision 1).

**19 G.2 The facts line.** In the GPU card, after its intro and before the split mode field, one plain
`--muted` paragraph (not a live region, `fe/DESIGN.md` 5.2 Message): pending,
`model-profile-model-facts-reading`; read and `moe` false, `model-profile-model-facts` with
`blockCount`; `moe` true and `expertCount` above 0, `model-profile-model-facts-moe` with `blockCount`,
`expertCount`, `expertUsed`; `moe` true and `expertCount` 0, `model-profile-model-facts-moe-plain`; a
reason, `model-profile-model-facts-none` filled with `api-<reason key>` filled with its `args` (so
`{"key": "model-info-missing", "args": ["/m/x.gguf"]}` reads ``No layer counts: the model file
`/m/x.gguf` is not there. The GPU weights stay ratios, and the layer fields have no slider.``); failed,
`model-profile-model-facts-failed`. It wraps anywhere (a long path).

**19 G.3 The block arithmetic.** A pure module of the fe mirroring `crates/app/src/gpu_split.rs`:
`layout(blockCount, ngl)` (`Layout::new`: `null` for `blockCount <= 0` or no position on the GPU;
`ngl` negative is all), `blockCounts(weights, layout)` (`block_counts`: `null` for an empty or all-zero
vector), `blockRanges(counts, layout)` (`block_ranges`, the texts of section 2), `setBlocks(weights,
index, blocks, layout)` (`set_blocks`, with `even_counts` as its seed for an all-zero vector). Its
tests copy the vectors of `gpu_split.rs`'s unit tests: `a_weight_vector_that_sums_to_the_positions_is_the_block_count_itself`,
`neighbouring_percentages_collapse_onto_the_same_cut`, `even_projects_to_half_the_positions_not_to_a_weight_of_one`,
`auto_has_no_block_counts`, `the_layout_counts_the_output_layer_and_partial_offload_shifts_the_start`,
`the_range_label_names_the_output_layer_separately`, `the_range_labels_tile_a_three_device_split`,
`setting_a_block_count_takes_the_difference_from_the_back`, `a_middle_or_last_row_edit_still_absorbs_from_the_tail_first`,
`a_block_edit_round_trips_through_the_projection`, `every_three_device_edit_keeps_the_budget_and_the_ranges_contiguous`
(Ornith: `blockCount` 33, 34 positions). Weights are the GD2.6 decimal texts; an empty one is 0
(llama.cpp pads a short vector with zeros). A decimal vector is projected exactly: every weight scaled
by `10^d`, `d` the most digits after a `.` among them, and the comparison `cum * count > pos * total`
done in integers without rounding (BigInt), so `0.6,0.4` and `3,2` give the same counts.

**19 G.4 When the rows show blocks.** "Blocks mode" holds when: the split mode select shows its first
option (`split-mode` absent, `""` or `layer`; an extra option such as `Layer` is not), the facts are
read, the layout exists, and two or more rows are checked. The layout's `ngl` is GPU layers' live
state: its default box ticked, or its text `auto`, `all`, a negative whole number, or anything that is
no whole number, is all (-1); a whole number of 0 or more is that number (0 gives no layout: weight
fields, as the GUI). In blocks mode, by the vector of the checked rows' weights:
- every weight empty or a GD2.6 number, sum above 0: the block controls (19 G.5) in place of the weight
  field;
- every weight empty or 0: no editor; after the as-of line and before the list, a `--muted`
  paragraph `model-profile-gpu-blocks-auto` with the places (`layout.count`); the shares read `auto` (GD2.2);
- any weight not a GD2.6 number: no editor; at the same place, the paragraph
  `model-profile-gpu-blocks-malformed` filled
  with the checked rows' weights joined with `,`; no share.
While the facts read is pending, a checked row in a two-or-more split (not `none`) shows no weight
editor and keeps its GD2.2 share (decision 3). Not in blocks mode and the read settled: brief 14 as
built.

**19 G.5 The block controls.** Inside the checked row's controls (the wrapping flex row of GD1.12), in
this order: one item holding a visible label `model-profile-gpu-blocks` and a text input
(`inputmode="numeric"`, about 4 characters wide) showing the row's count; then the slider, an `<input
type="range">` with `min` 0, `max` the places, `step` 1, value the count, taking the free width
(`flex: 1 1 12rem; min-width: 0`); then one plain text item with the range text (`blockRanges`) and the
share `model-profile-gpu-share` filled with `round(count * 100 / places)` and `%`. Both inputs have
the accessible name of the visible label plus the id in a visually hidden span ("Blocks rocm0",
[2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name)); the text input is described by the range
text and the share (`aria-describedby`, the share's id first as GD2.2); the slider carries
`aria-valuetext` `model-profile-gpu-blocks-value` (count, places, range text: "29 of 34: blocks 0-28")
and no `aria-valuemin`, `-max` or `-now` (native). Before the list, after the as-of line, the
`--muted` paragraph `model-profile-gpu-blocks-budget` with the places and the places minus one, shown
in blocks mode with the controls.

**19 G.6 Moving a slider.** Each `input` of a row's slider sets the checked rows' weights to
`setBlocks(base, row, value, layout)` written as whole numbers; the slider keeps the focus and every row
updates at once (count, range text, share, the other sliders). `base` is the weights as they stood
when the pointer went down on that slider, kept until the pointer goes up or is cancelled; with no
pointer down (keyboard, assistive technology, a test's change event) `base` is the current weights. So
with `3,1,1` checked on Ornith (12, 11, 11), a drag of the first row to 30 and back to 12 leaves
12, 11, 11; the arrow keys step one at a time from the current counts. The sum of the counts always
equals the places.

**19 G.7 Typing a count.** The row's text input shows the count; typing changes only its text. On Enter
or when it loses focus: a whole number is clamped to `0..places` and applied as 19 G.6 with the current
weights as `base`; anything else (empty, `x`, `2.5`, `-1`) puts back the count, with no error. Escape
does nothing special. The slider and the other rows change only then.

**19 G.8 Auto and Even in blocks mode.** Auto and Even show whenever two or more rows are checked and
the split mode is not `none` (GD2.4's rule, with or without an editor: Auto mode needs Even to leave).
Auto empties every weight (blocks mode's Auto paragraph shows, the block controls go); Even sets each
to `1` (so the counts are the projection of all ones: 17, 17 on Ornith with two GPUs, 12, 11, 11 with
three, as the GUI). Focus stays on the pressed button.

**19 G.9 Checking, unchecking, moving in blocks mode.** Brief 14's rules hold unchanged: a newly
checked row's weight is `1` (the others are weighted, GD2.3), so its count is the projection; unchecking
drops the row's weight (one row left: no blocks mode); Move up and Move down carry the weight, so the
counts move with their GPU and the range texts are recomputed in the new order. The focus rules of GD1.8
and GD1.9 hold.

**19 G.10 Saving.** No new save rule: the weights a block control writes are the weights of GD2.5, so
`tensor-split` is sent only when changed by GD2.5's rule, as the counts joined with `,` (Ornith, file
`85,15`, first slider set to 20: `20,14`). An untouched ratio in the file is never sent, however blocks
mode shows it. GD2.6 passes on counts. A 422 `tensor-split-format` in blocks mode goes to GD2.7's
place, every visible block text input `aria-invalid` with the error's id in its `aria-describedby`,
focus on the first one. Discard restores the loaded weights, so the counts come back as projected.

**19 G.11 Partial offload.** Changing GPU layers re-projects the rows at once from the unchanged
weights (`layout.start` moves, `places` shrinks; GUI `the_layout_counts_the_output_layer...`): with
`29,5` on Ornith and GPU layers 10, the budget says 10 places and the counts are the projection over 10.
Nothing is written by that change alone; GPU layers 0 leaves blocks mode (weight fields return).

**19 G.12 Keyboard, screen reader, 320 px, nothing else.** Tab order in a blocks row: checkbox, the
Blocks text input, the slider, Move up, Move down; then Auto and Even. The slider takes the native keys
(arrows by 1, Home 0, End the places; [APG Slider](https://www.w3.org/WAI/ARIA/apg/patterns/slider/)).
A screen reader on the slider hears "Blocks rocm0, slider, 29 of 34: blocks 0-28"; nothing is announced
while it moves beyond its own value. No sideways scroll at 320 px and 200 % text with three checked
rows and a 64-character id: the label item, the slider and the text item wrap inside the `li`. The
slider is at least 24 px high ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)) with
`accent-color: var(--accent)` and no custom track (forced colours keep the native drawing). The
strings file gains exactly LS1's keys; nothing outside the GPU card changes.

### LS2: the layer fields and the device lists

**19 G.13 The range beside a number field.** For GPU layers, and for MoE CPU layers when 19 G.14 says
so: while the facts are read, an `<input type="range">` after the field's text input, inside its
control row (`flex: 1 1 12rem; min-width: 0`), `min` 0, `max` `blockCount`, `step` 1, named by the
field's legend (`aria-labelledby`, so "GPU layers", role slider beside the textbox of the same name),
`aria-valuetext` `model-profile-layers-value` (value, `blockCount`). Its value is the text read: a
whole number clamped to `0..blockCount`; for GPU layers `auto`, `all` and a negative number are
`blockCount`; the default box ticked is `blockCount` for GPU layers and 0 for MoE CPU layers; any other
text (empty, `x`) leaves the slider where it was (at first `blockCount` for GPU layers, 0 for MoE CPU
layers). Typing updates the slider at each keystroke; moving the slider writes its value as the text, an
edit like typing (changed, sent, Discard by 02h). Disabled (native) while the default box is ticked or
the field is disabled. Facts pending, failed or a reason: no slider.

**19 G.14 MoE CPU layers.** The field becomes editable (`n-cpu-moe` joins the form's keys and the
fe's integer check as 0 to 4096, `field-error-number`). Its state by the facts: pending, disabled with
the reason paragraph `model-profile-moe-reading` right before it; `moe` true, enabled with the slider of
19 G.13; `moe` false, disabled with `model-profile-moe-dense` right before it, the file's value shown and
kept (never sent: unchanged); a reason or failed, enabled with no slider. The note `model-profile-moe-later`
moves to right before MoE cache, which stays disabled.

**19 G.15 Saving MoE CPU layers.** As every number field of 02h: changed when its state differs, sent
as the number, `null` with the box ticked. A 422 `not-whole-number-in-range` with `field` `n-cpu-moe`
puts `api-not-whole-number-in-range` on the field (02h 142).

**19 G.16 Draft device.** Once the device read answered 200 with a list, the select's options are: the
empty option `model-profile-device-same-as-model`, then each listed GPU in probe order, value its id,
text `model-profile-device-option` (id, name; the id alone when the name is null), then, when the file's
value matches no listed id (case-insensitively), that value as the last option, text
`model-profile-device-option-missing` when the probe answered with a time, else the value. A listed GPU
that matches the file's value case-insensitively takes the file's spelling as its value, so an untouched
select is never sent (02h). Pending or failed read: today's options (empty and the file's value).
Enable rule unchanged (needs a Draft model file).

**19 G.17 Vision GPU.** As 19 G.16, with `model-profile-device-follows-model` as the empty option and
`none` after it, text `model-profile-device-none` (a file value `none` selects it, never an extra
option). Enable rules unchanged.

**19 G.18 The device bound refusal.** A 422 `model-profile-device-outside` puts
`api-model-profile-device-outside` (args as sent) on the select of the key it names: `field` when
present, else `args[2]` (`device-draft` or `mmproj-device`); the select `aria-invalid`, focus on it, or
on its field's first enabled control by `fe/DESIGN.md` 5.2 Error when it is disabled; the bar's alert
`save-refused` with the text (02h 142).

**19 G.19 Keyboard, screen reader, 320 px, nothing else.** Tab order in a field with a slider: the
About toggle, the default box, the text input, the slider. No sideways scroll at 320 px and 200 % text,
a 64-character device name in an option included (a native select's closed width follows the field).
The strings file gains exactly LS2's keys; nothing else in Hardware or Vision changes (Draft GPU
layers keeps no slider: the draft model's facts are not read).

## 4. Out of scope

- Override-tensor (2j, its own brief); `moe-cache-mib` (decision 4 of the architecture).
- Flagging, before the save, a Draft device or Vision GPU outside the instance's server profile
  `Devices` (`ARCHITECTURE.md` (3) ends "the page still flags such a GPU"): the page does not know which
  instances the profile is in effect on, nor their server profiles' `Devices`; the refusal of 19 G.18
  covers it (question Q1).
- A slider for Draft GPU layers; the layer facts of an `hf-repo` not yet downloaded (decision 2 of the
  architecture); a Model info card; a re-read of the facts.
- The split mode warnings (2k).

## 5. Accessibility (WCAG 2.2 AA)

- Every slider and block text input is named by its visible label plus the GPU id, the visible text
  first ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
  [2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name), [4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value));
  a slider's value is spoken as text with its meaning ("29 of 34: blocks 0-28"), not a bare number
  ([ARIA 1.2 `aria-valuetext`](https://www.w3.org/TR/wai-aria-1.2/#aria-valuetext)).
- Native range inputs: keyboard by arrows, Home, End ([2.1.1](https://www.w3.org/TR/WCAG22/#keyboard));
  a click on the track and the number field as single-pointer alternatives to dragging
  ([2.5.7](https://www.w3.org/TR/WCAG22/#dragging-movements)); at least 24 px high
  ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).
- No control changes kind or disappears under the focus while the user types or drags (decision 3;
  [3.2.2](https://www.w3.org/TR/WCAG22/#on-input), [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)).
- The share and the range text are text, never only the slider's position or colour
  ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)); the slider's track and thumb in `--accent`
  keep 3:1 against the card ([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast); checked in the
  review, the native track's unfilled part drawn by each engine).
- The facts line, the budget, the Auto and malformed lines and the MoE reasons are part of the card as
  it renders, appear without a user action and are plain paragraphs, not live regions
  ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages); brief 14's ruling of 2026-11-21).
- Errors name the problem and the fix on the control ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification),
  [3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion)); reflow at 320 px and 200 % text
  ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)).

## 6. Older tests that change

- **The bff fake** (`fe/src/test/bff.ts`): it needs `GET /api/profiles/model/<id>/model-info`.
  Suggestion (the tester's call): answer it by default with `{"model": null, "info": null, "reason":
  {"key": "model-info-missing", "args": ["/m/x.gguf"]}}`, so the GD2 tests keep their weight fields
  (an unanswered read leaves them pending, with no editor, 19 G.4) and MoE CPU layers is enabled with
  no slider.
- Tests that count or list the requests a page arrival sends (GD1.3's): one more GET.
- Tests pinning MoE CPU layers disabled, the note `model-profile-moe-later` before it, or `n-cpu-moe`
  never sent (`ModelProfileCards2`, `3`, `4`, `ModelProfileSave3`, `7`, `13`, `16`, `SaveAlertClean`:
  `grep -l "n-cpu-moe\|moe-later\|moe-cpu-layers" fe/src/*.test.*`).
- Tests pinning the Draft device and Vision GPU options as the empty option plus the file's value
  (`ModelProfileGpu3`, `4`, `ModelProfileSave1`, `12`, `13`, `16`, `ModelProfileCards3`, `4`): those
  hold only while the device read is pending or failed.
- `HearthTokens.test.ts` H1a.12's list of every `min-height` and `min-width`: it gains the slider's
  `min-height: 24px` and `min-width: 0` under the class the coder gives it (named in the review).

## 7. `fe/ux` states

`fe/ux/stub.py` answers `GET /api/profiles/model/<id>/model-info` with `{"model":
"/m/qwen/Qwen3-27B-Q8_0.gguf", "info": {"blockCount": 64, "moe": true, "expertCount": 128,
"expertUsed": 8}, "reason": null}` (so `model-profile` shows the GPU layers and MoE CPU layers sliders),
and `/__set?split=layer` serves the profile with `split-mode` removed and `tensor-split` `40,25`, for a
new state `model-profile-blocks` (expect: a `p` with the budget's text up to `{0}`) at 1280 and 320,
both themes, 200 % text. Both added by the tester with LS1 (the stub) and LS2 (no new state).

## 8. Questions for the user

- **Q1.** Should the page warn, before Save, when Draft device or Vision GPU names a GPU outside the
  server profile `Devices` of an instance the profile is assigned to? Recommended: not now; the save's
  refusal names the GPU and the instance (19 G.18), and the warning comes cheaply with R4/R5, when the
  page reads the server profile's `Devices` for the GPU rows anyway.

## LS1 review (designer, 2026-11-22)

Read `fe/src/gpuBlocks.ts`, the LS1 diff of `GpuSplit.tsx`, `ModelProfilePage.tsx`, `ModelProfileCards.tsx`,
`api.ts`, `index.css`, `en_US.json`; driven in Chromium, Firefox and WebKit with the `fe/ux` stub
(`split=layer`) and Playwright routes (`build/ux-review/ls1.js facts|act|layout`, `ls1shot.js`; shots in
`build/ux-review/ls1/`). **Verdict: accepted after re-check (D1, D2 below fixed and re-checked
2026-11-22); everything else met.**

- 19 G.1 met: one read per arrival (`ModelProfilePage.tsx` 300), shape check and failed reads
  (`api.ts` 67-157): a 500, a body of no shape and an unknown reason key all give the failed line.
- 19 G.2 met in all three engines: pending, dense, MoE with and without expert counts, the four reasons,
  failed; plain `p.muted` after the intro, before split mode; a 190-character path wraps (no sideways
  scroll at 320).
- 19 G.3 met (`gpuBlocks.ts`, BigInt projection).
- 19 G.4 met except D1 and D2: pending shows no editor, a reason shows the weight fields, GPU layers 0
  leaves blocks mode.
- 19 G.5 met: label item, slider, range plus share; names "Blocks CUDA0" on textbox and slider;
  `aria-describedby` share first; `aria-valuetext` "40 of 65: blocks 0-39"; no `aria-value*` attributes.
- 19 G.6 met: a drag to 64 and back leaves 40, 25; a track click sets the count; arrows, Home, End from
  the current counts; a pointer released off the slider ends the drag (the next arrow on another row
  works from the current counts); the sum is always the places.
- 19 G.7 met: typing moves nothing; Enter and blur apply; `x` puts the count back; 99 clamps to 65;
  Escape leaves the typed text, applied on blur (as the brief says); the count already shown writes
  nothing (`GpuSplit.tsx` 812).
- 19 G.8 met: Even gives 33, 32 (22, 22, 21 with three GPUs), Auto shows the Auto line, the focus stays
  on the button. 19 G.9 met: the new row takes its share, Move down carries the count, one GPU leaves
  blocks mode with the focus kept on the checkbox. 19 G.10 met: no save button while untouched; typed 20
  saves only `{"tensor-split": "20,45"}`. 19 G.11 met: GPU layers 10 gives "10 places", counts 7, 3.
- 19 G.12 met except D1: Tab order checkbox, Blocks, slider, Move up, Move down; sliders 24 px high,
  native track with `accent-color`; no sideways scroll at 320 px and 200 % with three rows and a
  64-character id, both themes. The unfilled track is drawn by the engine (1.44:1 in WebKit, 2.09:1 in
  Chromium light): UA-drawn appearance is outside 1.4.11, the thumb and the value text carry the state.

**D1 (must fix).** At 720 px and wider, `.field` is a two-column grid (`index.css` 1388-1404) and the
GPU fieldset's children place themselves in it. The new budget, Auto or malformed paragraph
(`GpuSplit.tsx` 614-638) takes the second cell of the first row, so the list (639) drops into the 200 px
label column and Auto and Even sit in the second column, stretched to the whole list's height
(`build/ux-review/ls1/firefox-1280-dark.png`; `ls1shot.js` logs the x positions). Fix: give the three
blocks paragraphs one class (e.g. `gpu-blocks-line`), and in the 720 px rule place
`#model-profile-gpu-devices > .gpu-blocks-line` and `#model-profile-gpu-devices > .check-list` in
`grid-column: 2`, so row 1 is the as-of line beside the budget, the list goes under the budget, and Auto
and Even land on the row after it as today. Pin it with a test that reads the CSS (as `Reflow.test.ts`
does).

**D2 (small).** `GpuSplit.tsx` 583: `Number('0x0')` and `Number('0e1')` are 0, so a weight like that is
Auto and malformed at once, and both paragraphs show. Fix: `const autoSplit = !malformed && ...`
(move 586 above 583), with a test.

**Re-check (2026-11-22): D1 and D2 fixed.** `build/ux-review/ls1fix.js`, all three engines at 1280 and
760 px, light and dark: in blocks, Auto and malformed states the as-of line sits in column 1 beside the
blocks line in column 2, the list is under it in column 2 (430 or 462 px wide), and Auto and Even are
on the row after it, 40 px high. The tensor state is as before. `0x0,0e1` shows only the malformed line.
No sideways scroll. `npm --prefix fe/ux run check` (132 states per browser): only the 8 known
`instance-server` target-size findings (Chromium, Firefox); `model-profile-blocks` clean in all three.

## LS2a review (designer, 2026-11-22)

19 G.13 to G.15 only (G.16 to G.19 are LS2b). Read the LS2a diff (`Field.tsx`, `ModelProfileCards.tsx`,
`ModelProfilePage.tsx`, `GpuSplit.tsx`, `index.css`, `en_US.json`); driven in Chromium, Firefox and
WebKit with the `fe/ux` stub and Playwright routes (`build/ux-review/ls2a.js states|act|layout`, shots
`build/ux-review/ls1/ls2a-*.png`).

**The key gap both agents flagged.** No such gap: section 2 of this brief never listed
`field-slider-label` (`grep` finds it nowhere in the brief or the strings file). The LS2 keys are the
ones the coder used: the slider is named by the field's legend (`aria-labelledby`, 19 G.13) and speaks
`model-profile-layers-value` (`{0} of {1}`). Nothing to add or drop.

- 19 G.13 met: a range after the text input in the control row (`Field.tsx` 320-337), min 0, max
  `blockCount`, step 1, named "GPU layers" / "MoE CPU layers" beside the textbox of the same name,
  value text "15 of 64". Typed `1`, `10` move it at each keystroke; `auto`, `all`, `-1` read as 64; `100`
  clamps to 64; `x` and empty leave it where it was; the ticked box reads 64 (GPU layers) and 0 (MoE),
  slider disabled. Arrows, Home, End and a track click write the number as the text; GPU layers 0 from
  the slider leaves blocks mode and 64 brings it back. No slider while the read is pending, failed or a
  reason.
- 19 G.14 met: pending, disabled with `model-profile-moe-reading` right before it; MoE (with or without
  expert counts) enabled with the slider; dense, disabled with `model-profile-moe-dense`, the file's `12`
  shown and kept; reason or failed, enabled with no slider; `model-profile-moe-later` now right before
  MoE cache (`ModelProfileCards.tsx` 422-450, 1906).
- 19 G.15 met: `5000` is refused by the fe with "Enter a whole number from 0 to 4096.", focus on the
  field, nothing sent; `7` sends only `{"n-cpu-moe": "7"}` (`ModelProfileCards.tsx` 1551).
- Keyboard: Tab goes About, default box, text input, slider (the ticked box skips the disabled pair).
  Sliders 24 px high, accent colour, native track (`index.css` 1761). No sideways scroll at 1280 and
  320 px, 100 % and 200 % text, light and dark, in all three engines.
- Note, no defect (preference): at 1280 px GPU layers' text input is the wide `mono` one (it takes
  words), so its slider wraps under it, while MoE CPU layers' slider sits beside its narrow input. Both
  are in the control row as 19 G.13 says.
- `npm --prefix fe/ux run check` (132 states per browser): only the 8 known `instance-server`
  target-size findings; `model-profile` and `model-profile-blocks` (both now with the two layer sliders)
  clean in all three engines.

**Verdict: LS2a accepted.**
