# Brief 14: GPU distribution on the model profile page (slices GD1 and GD2)

`PLAN.md` step 10 item 13 (user, 2026-11-20: "Non vedo gli slider per la spartizione dei layer"):
the web UI edits a model profile's `device`, `split-mode` and `tensor-split` as `pai-config`'s GPU
table does. Row 2f of [02-models.md](02-models.md) (read) and the GPU half of 2j (write), built
together. The bff's part is `ARCHITECTURE.md` "GPU distribution on the model profile page" (bff slices
G1, the save keys and their two checks, and G2, the device read). Criteria are numbered **GD1.n** and
**GD2.n**; test names cite them so (`GD1.4`).

**Two sub-slices, one tester and one coder run each, GD1 first** (the tester and coder run on a small
local model: designer playbook, "more than ~12 criteria fails"):

| Sub-slice | Criteria | Holds |
|---|---|---|
| **GD1** | GD1.1 to GD1.12 | The card, the split mode select, the device read and its states, the GPU rows (check, order, memory, not detected), saving `device` and its refusal |
| **GD2** | GD2.1 to GD2.10 | The weights (`tensor-split`): the weight field per GPU, the share, Auto and Even, saving it, its check and its refusal |

A tester on GD2 reads sections 0, 1, 2 and the GD2 criteria.

## 0. What exists (read on main at 6350058, the bff part as designed)

- **`GET /api/node/devices`** (bff slice G2, designed, not yet served; `fe/briefs/api-contract.md`):
  behind a session, a page read. 200 `{"probedAt": "2026-11-20T09:14:03Z", "devices": [{"id":
  "ROCm0", "name": "AMD Radeon AI PRO R9700", "backend": "ROCm", "totalMib": 32624, "freeMib":
  32420}, ...]}`, in the engine's probe order, the CPU left out, duplicate views of one card (CUDA0
  and Vulkan1) and integrated GPUs included. `probedAt` is RFC 3339 UTC, or `null` when no probe
  answered (then `devices` is `[]`); a CPU-only host is `devices: []` with a time. `freeMib` is free
  memory **at the probe**: the manager caches the probe until it restarts, and there is no refresh
  (user, 2026-11-20). Errors: 401 `session-required`; 403 `session-not-administrator` (the session
  is ended); 503 `client-unavailable` (no manager, or no answer within 35 s: a cold probe can take
  30 s); 502 `client-other`.
- **`PUT /api/profiles/model/{id}`** (02h) accepts `device`, `split-mode` and `tensor-split` from G1
  on, as strings or `null`, changed keys only. New 422 keys: `device-bad-name` `[key, entry]`,
  `field` the key (`device`, `device-draft` or `mmproj-device`), and `tensor-split-format`
  `[value]`, `field` `tensor-split`. Existing: `not-one-of` on `split-mode` (the manager compares
  exactly: `Layer` is refused), and `moe-cache-with-tensor-split`, `field` `moe-cache-mib`. **No
  check against the probed list**: a well-formed id the probe lacks saves (a card can vanish for a
  while, and the GUI keeps unknown ids on purpose). The manager's memory check still counts every GPU
  of the host, whatever `device` says (user, 2026-11-20): nothing on this card changes it.
- **The page** (`fe/src/ModelProfilePage.tsx`, `fe/src/ModelProfileCards.tsx`): cards in the GUI's
  order after Assets, Hardware first; 02e criterion 83 reserved the place between Assets and Hardware
  for Model info, GPU distribution and Tensor placement. Draft device and Vision GPU are device
  selects holding only the file's value ("the device list arrives with 2f"): unchanged by this brief
  (section 4).
- **What llama.cpp does with the three keys** (`crates/app/src/gpu_split.rs` module header, with its
  sources): `tensor-split` is a vector of ratios indexed over the `device` list in its order; the
  first device is the main GPU; a blank `tensor-split` with two or more devices splits by **free**
  memory at load (not evenly), except under `split-mode = tensor`, where blank is an even cut; under
  `none` only the first device is used and the vector means nothing; no `device` means every GPU
  llama.cpp finds.
- **The GUI** (`crates/config/src/gpu_split.rs`, `components.slint`): the split mode combo above a
  table of every probed GPU, checked rows first in `device` order, then the rest in probe order; per
  row a checkbox, a drag handle (order), the name and memory, a slider and a share column; Auto and
  Even buttons; a checked id the probe does not list keeps its row, "(not detected)".

## 1. Decisions

1. **The control for a weight is a number field per GPU, with its share written beside it, not a
   slider** (preference, for keyboard and screen reader first). The GUI's slider edits **block
   counts**, a fixed budget shared by the rows, which needs the model's `block_count`; the web has no
   model facts yet (2g), so it can only edit the raw ratio, as the GUI itself does when the model is
   unknown. A ratio has no maximum, and the file's own values (`29`, `1.5`, `85`) must show exactly
   (02e criterion 87), which a range input with a fixed `max` and `step` cannot do. A text field
   types any value in one go, reads its value aloud as it is, and needs no dragging
   ([2.5.7 Dragging Movements](https://www.w3.org/TR/WCAG22/#dragging-movements)); the share ("Share:
   75%") is the feedback a slider's position gives, as text for everyone. When 2g brings the block
   count, a block slider can be weighed again (out of scope).
2. **The order is two buttons per checked row, Move up and Move down**, not a drag handle
   ([2.5.7](https://www.w3.org/TR/WCAG22/#dragging-movements): a single-pointer alternative, and
   keyboard operable by being buttons). The order matters: the first GPU is the main GPU and the one
   `none` keeps.
3. **The GPUs are a Checkbox list whose checked rows carry their controls** (`fe/DESIGN.md` 5.2,
   extended there as "GPU list"), not a table: at 320 px a five-column table would scroll inside its
   box at base text size, and each row's few controls wrap naturally in a list item (preference).
4. **Read once per arrival, never polled, never re-read after a save.** The probe's free memory is
   dated by `probedAt` ("Free memory as of ..."), never shown as now (user, 2026-11-20: no refresh
   button).
5. **A failed device read never signs the app out and never blocks the page.** The profile's own
   GPUs stay shown and editable; only adding another GPU needs the list. A 401 or 403 there shows the
   failure text; the next save meets the session's end through 2h.3 (as 13 N.3's shell read, so
   unsaved edits are never dropped by a background read).
6. **Split mode is never disabled** (deliberate difference: the GUI disables it with one GPU
   checked). With one GPU it does nothing, which its hint says; disabling it would need a reason
   line and gains nothing (smallest design).
7. **The GUI's split mode warnings** (tensor with no GPU checked, CUDA mixed with ROCm, flash
   attention off, architecture) **stay for 2k**, as the brief 02 split planned.

## 2. The texts

`…` is U+2026, as the other loading texts. Copied from `crates/core/strings/en_US.toml` unless the
column says "web": the web text drops what is never true here (sliders, blocks, drag) and says why.

| Key | English | From |
|---|---|---|
| `model-profile-gpu-distribution` | `GPU distribution` | toml 249 |
| `model-profile-gpu-distribution-intro` | `Which GPUs run THIS model and how much each holds, saved to models.ini. Check nothing to let llama.cpp choose. With two GPUs of different sizes, check both and give the bigger one more weight; left blank, the split follows free VRAM at load, not an even cut. The weights are ratios: 3 and 1 give 75% and 25%. The checked GPUs, top to bottom, are the split order: Move up and Move down change it; the first one is llama.cpp's main GPU.\nKeys: split-mode, device, tensor-split.` | web, from toml 250 |
| `model-profile-split-mode` | `GPU split mode` | toml 157 |
| `model-profile-split-mode-hint` | `How THIS model is divided between GPUs. Does nothing with one GPU checked. Same on CUDA and ROCm.\nlayer (default): each GPU takes a consecutive run of blocks, in the proportion of its weight. Fastest prefill; copes with a slow link between GPUs.\nnone: one GPU, the first checked row; other checked rows go unused.\ntensor (experimental): the GPUs act as one device, and all weights and the KV cache are divided by the ratio (blank = even). Fastest decode, needs a fast link. Flash attention must not be off, Fit to VRAM is skipped (set Context size yourself), some architectures refuse to load, and llama.cpp tunes it for CUDA with no guarantees on other backends.\nrow (deprecated, use tensor): weights are split by rows by the ratio (CUDA/ROCm; Vulkan falls back to layer). The KV cache follows the layer cut.\nFlag: --split-mode` | web, from toml 252 ("and the table edits the cut" is the GUI's block column) |
| `model-profile-split-layer` | `layer: split by blocks (default)` | toml 804 |
| `model-profile-split-none` | `none: single GPU` | toml 805 |
| `model-profile-split-tensor` | `tensor: tensor parallel across GPUs (experimental)` | toml 806 |
| `model-profile-split-row` | `row: split matrices by rows (deprecated)` | toml 807 |
| `model-profile-gpus` | `GPUs` | web (the GUI's column says "Device") |
| `model-profile-gpu-detecting` | `Detecting GPUs…` | toml 161 |
| `model-profile-gpu-failed` | `The host's GPU list could not be read. The GPUs this profile names are shown; to add another, open the page again.` | web |
| `model-profile-gpu-no-probe` | `No GPU list: the engine's device probe gave no answer on this host.` | web |
| `model-profile-gpu-none-found` | `No GPU found on this host.` | web |
| `model-profile-gpu-probed-at` | `Free memory as of {0}.` | web |
| `model-profile-gpu-memory` | `{0} GiB, {1} GiB free` | web (the GUI's `vram-summary` says GB for MiB / 1024) |
| `model-profile-gpu-not-detected` | `(not detected)` | toml 813 |
| `model-profile-gpu-move-up` | `Move up` | web |
| `model-profile-gpu-move-down` | `Move down` | web |
| `model-profile-gpu-moved` | `{0}: position {1} of {2}` | web |
| `model-profile-gpu-weight` | `Weight` | toml 159 |
| `model-profile-gpu-share` | `Share: {0}` | web (the GUI's column "Share", toml 160) |
| `model-profile-gpu-share-auto` | `auto` | the GUI's share word (`crates/config/src/gpu_split.rs`, a literal there) |
| `model-profile-gpu-share-even` | `even` | toml 493 |
| `model-profile-gpu-share-unused` | `unused` | the GUI's share word (a literal there) |
| `model-profile-gpu-auto` | `Auto` | toml 162 |
| `model-profile-gpu-even` | `Even` | toml 163 |
| `model-profile-gpu-weights-incomplete` | `Give every checked GPU a weight of 0 or more, or clear them all for Auto.` | web |
| `model-profile-gpu-split-without-device` | `This profile's tensor-split {0} applies to every GPU llama.cpp finds, in its order. Checking GPUs here replaces it.` | web |
| `api-device-bad-name` | ``{0}: `{1}` is not a device llama.cpp accepts: no spaces, none of ; # =, not CPU, and none only alone`` | the toml text G1 adds; proposed here |
| `api-tensor-split-format` | ``tensor-split = `{0}` is not a list of up to 15 numbers of 0 or more, separated by , or /`` | the toml text G1 adds; proposed here |

GD1 adds the keys from `model-profile-gpu-distribution` to `model-profile-gpu-moved` and
`api-device-bad-name` (20); GD2 adds the rest (10). The two `api-` texts are copied verbatim from
whatever G1 commits to `crates/core/strings/en_US.toml`, with no closing full stop (02h 150); the
wording above is the designer's proposal for that toml line. Numbers, `%`, `GiB` and the ids are
literals, never strings (`fe/DESIGN.md` 7).

## 3. Criteria

### GD1: the card, split mode, the GPU rows, saving `device`

**GD1.1 The card.** A D5.2 Card, `h3` `model-profile-gpu-distribution`, inside the settings section,
right after the Assets card and before Hardware (02e criterion 83's reserved place); its intro
`model-profile-gpu-distribution-intro`, each `\n` a paragraph. The jump list gains the entry "GPU
distribution" between Assets and Hardware, reaching the card's `h3` as every jump does (02h 196).
The card is editable as the others: no Notice, no `disabled`.

**GD1.2 Split mode.** The card's first field: a Select Field (`SelectField`), label
`model-profile-split-mode`, hint `model-profile-split-mode-hint` behind its About toggle, options in
this order and value: `""` `model-profile-split-layer`, `none` `model-profile-split-none`, `tensor`
`model-profile-split-tensor`, `row` `model-profile-split-row` (the GUI's order, `MODE_VALUES`). Shown:
the first option when `split-mode` is absent, `""` or `layer`; the matching option for `none`,
`tensor`, `row`; any other value (`Layer`, `foo`) as one extra, last option carrying the value as
written (D5.2 Select). Changed when the selected option differs from the one it loaded with; sent:
`null` for the first option, else its value. So an explicit `layer` in the file stays untouched until
the user picks another option, and picking the first option back removes the key. Never disabled
(decision 6).

**GD1.3 The device read.** On each arrival at a model profile's page, the page sends one `GET
/api/node/devices` beside its profile read; leaving and coming back sends another; a save, Discard,
Reload or any edit sends none, and nothing polls. Its time limit is **40 s** (the bff's 35 s plus a
margin; `fe/DESIGN.md` 7's 15 s does not apply), a time-out being its network failure. While it is
pending, the GPUs fieldset (GD1.4) shows `model-profile-gpu-detecting` in a plain paragraph
after its legend (no `role="status"`: it is there when the card first renders, nobody acted, and it
is replaced by a GD1.7 paragraph, as `fe/DESIGN.md` 5.2 Message; ruling of 2026-11-21), and the rows of the GPUs the profile names (GD1.6, no name or memory
line yet); the rest of the page is usable and saves.

**GD1.4 The GPUs fieldset.** After the split mode field: a `<fieldset>` whose `<legend>` is
`model-profile-gpus`, holding in this order: the read's paragraphs (GD1.3, GD1.7), then a
`ul role="list"` of one `li` per GPU row (GD1.5), then the move status region (GD1.9), then (GD2)
the Auto and Even buttons, then the error paragraphs. Each row: a native checkbox whose `<label>` holds the id (monospace, weight 600)
then the device's `name`; under it a `--muted` line with `model-profile-gpu-memory` filled with
`totalMib / 1024` and `freeMib / 1024` to one decimal (`32624`, `32420` give `31.9 GiB, 31.7 GiB
free`), its id the checkbox's `aria-describedby`; no memory line when `totalMib` is not a positive
number.

**GD1.5 Row order.** First the checked rows, in the order of the profile's `device` (the split order);
then every listed GPU not checked, in the reply's order; then any id the file named that is now
unchecked and that the reply does not list (GD1.6), in file order, so it can be checked again until
the page is left. Reading `device`: split on `,`, each entry trimmed of spaces and tabs, empty entries
dropped, an id that repeats one before it (case-insensitively) dropped. A file id matches a listed
GPU case-insensitively (llama.cpp's `striequals`); the row then shows the file's spelling and the
GPU's name and memory. With the reply of section 0 and `device = CUDA0,rocm0`: rows `CUDA0`
(checked), `rocm0` (checked, the R9700's name), `Vulkan0` (unchecked).

**GD1.6 A GPU the list does not have.** A file id that matches no listed GPU is a checked row with
the id and no name and no memory line. When the read answered 200 with a string `probedAt`, its
label adds `model-profile-gpu-not-detected` after the id; in every other case (pending, failed,
`probedAt` null) it adds nothing, since nothing says the GPU is missing. A well-formed unknown id
saves (section 0).

**GD1.7 The read's states.** 200 with GPUs: after the legend, `model-profile-gpu-probed-at` filled
with a `<time datetime="<probedAt as sent>">` whose text is `new Date(probedAt).toLocaleString(
undefined, { dateStyle: 'medium', timeStyle: 'short' })`; no such line when `probedAt` is not a
string that `Date` parses. 200 with `devices: []` and a time: `model-profile-gpu-none-found`, then the
as-of line. 200 with `probedAt: null`: `model-profile-gpu-no-probe`. A 401, 403, any other status, a
network failure, a time-out, or a malformed reply (not an object, or `devices` not an array):
`model-profile-gpu-failed`, and the session stays as it is (decision 5). Each is a plain paragraph
(nobody acted: not an alert, `fe/DESIGN.md` 5.2 Message). Inside a good reply, an item without a
non-empty string `id`, and an item whose `id` is `CPU` in any case, is left out; a `name` that is not
a string reads `""`.

**GD1.8 Checking and unchecking.** Checking an unchecked row makes it the last checked row (it moves
up to the end of the checked rows); unchecking a checked row moves it back among the unchecked rows
at its GD1.5 place. The row's checkbox keeps the focus through the move (rows keyed by id), and no
other control moves focus. A file id unchecked keeps its row (GD1.5) until the page is left or a save
re-reads the profile.

**GD1.9 Move up and Move down.** With two or more rows checked, each checked row holds two secondary
buttons after its other controls: `model-profile-gpu-move-up` and `model-profile-gpu-move-down`, each
with the accessible name of its visible text plus the id in a visually hidden span ("Move up rocm0",
[2.5.3 Label in Name](https://www.w3.org/TR/WCAG22/#label-in-name)). The first row's Move up and the
last checked row's Move down are `aria-disabled="true"` (D5.2 Inactive: a click does nothing, focus
stays). A press swaps the row with its neighbour among the checked rows; the pressed button keeps the
focus in the moved row, and the fieldset's one visually hidden `role="status"` region says
`model-profile-gpu-moved` filled with the id, its new position and the checked count ("rocm0: position
1 of 2"). The region is in the DOM, empty, from the card's first render, whatever the read's state
or the number of checked rows, so the first move is announced (`fe/DESIGN.md` 5.2 Action feedback:
a status line always in the DOM); it is the card's only `role="status"`. Page tests that count "the
one status line" mean the save bar's and pick it inside the save bar, not by counting the page's
regions (ruling of 2026-11-21). With fewer than two rows checked there are no move buttons; unchecked rows never have them.

**GD1.10 Saving `device`.** The device state is the list of the checked rows' ids, in row order.
Changed when that list differs (exact text, order included) from the list read in GD1.5; changing it
back makes it unchanged. Sent when changed: the ids joined with `,` and no spaces, or `null` when none
is checked. It counts for the save bar, the leave guard and Discard as any field (D5.2 Changed field):
Discard restores the loaded rows, checks and order. After a save the page's re-read rebuilds the rows
from the new values; the device list is not read again. An untouched `device`, however the file
spells it (`ROCm0, ROCm1`), is never sent.

**GD1.11 The `device-bad-name` refusal.** A 422 `device-bad-name` with `field` `device` puts
`api-device-bad-name` (args `[key, entry]`) as the error of the GPUs fieldset: a D5.2 Error paragraph
after the list, every checked checkbox `aria-invalid="true"` with the error's id added to its
`aria-describedby`, focus on the first checked checkbox (D5.2 Checkbox list), else on the first
checkbox, and the bar's alert `save-refused` with the same text (02h 142). Checking, unchecking or
moving a row clears the error. The bar's alert follows 02h 198b (`fe/DESIGN.md` 5.2 Save bar,
which wins over this brief): it stays while the form is still changed and goes, silently and without
moving focus, when the edit returns the form to its loaded state (ruling of 2026-11-21). Example, with
`device = CUDA9,CUDA0` and Vulkan0 checked then refused: checking rocm0, unchecking it, moving CUDA0
up each clear the error and keep the alert (Vulkan0 still checked); unchecking Vulkan0 when the rows
are back as loaded removes the alert. With `field` `device-draft` or `mmproj-device` the error
goes on that device select, as any field (02h 142, 144); a 422 without `field` falls back to `args[0]`.

**GD1.12 Keyboard, screen reader, 320 px, nothing else.** Tab order inside the card: the split mode
About toggle, its select, then per row in row order its checkbox, then (GD2) its weight field, then
its Move up and Move down; then (GD2) Auto and Even. Nothing in the card takes focus on arrival or
when the read lands. At 320 px and at 200 % text, no sideways scroll: a row's label, memory line and
controls wrap inside the `li` (the label `overflow-wrap: anywhere`, the controls a wrapping flex row),
a 64-character id or name included; buttons stay at least 40 px high (D5.2 Buttons), the checkbox's
target at least 24 px ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)). The strings file
gains exactly GD1's keys of section 2; nothing outside the card changes but the jump list's entry and
the 422 mapping of GD1.11.

### GD2: the weights (`tensor-split`)

**GD2.1 The weight fields.** With two or more rows checked and the split mode not `none`, each checked
row holds, after its label and memory line, a text input with visible label `model-profile-gpu-weight`
and accessible name that label plus the id in a visually hidden span ("Weight rocm0"),
`inputmode="decimal"` ([MDN, inputmode](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Global_attributes/inputmode)),
about 6 characters wide, then its share (GD2.2). Under `none`, or with fewer than two checked, no
weight field shows; the weights are kept as typed and come back when the condition holds again.
Loaded values: `tensor-split` split on `,` or `/` (llama.cpp's `[,/]+`), each entry trimmed, entry `i`
the weight of the `i`-th GPU of the file's `device` list (GD1.5's reading) **exactly as written**
(`1.5`, `029`); a missing entry is `""`, extra entries are not shown.

**GD2.2 The share.** Each checked row shows a plain text `model-profile-gpu-share`, its id the first in
the weight field's `aria-describedby` (an error's id after it), not a live region:
with one row checked, `100%`; under `none`, `100%` on the first checked row and
`model-profile-gpu-share-unused` on the others; with two or more checked and every weight empty or
`0`, `model-profile-gpu-share-even` under `tensor` and `model-profile-gpu-share-auto` otherwise; when
every weight of the checked rows is a number of 0 or more (GD2.6's pattern) and their sum is above 0,
`round(weight * 100 / sum)` followed by `%` (`3` and `1` give `75%` and `25%`); else (an empty or
malformed weight beside numbers) no share text on any row. With no row checked, no share. It updates
as the user types.

**GD2.3 Weights follow their GPU.** A newly checked row's weight is `1` when any other checked row has
a non-empty weight, else `""` (the GUI appends at weight 1, `gpu_split.rs` `toggle`); unchecking a row
drops its weight (checking it again follows the same rule); Move up and Move down carry the weight
with the row.

**GD2.4 Auto and Even.** While the weight fields show, two secondary buttons after the list:
`model-profile-gpu-auto` empties every weight field, `model-profile-gpu-even` sets each to `1`
(two different launches: the intro says why). Focus stays on the pressed button; both clear the
weights' error (GD2.6). Not shown when the weight fields are not.

**GD2.5 Saving `tensor-split`.** The composed value is `null` with fewer than two rows checked or with
every weight field empty, else the checked rows' weights in row order, trimmed, joined with `,`. The
loaded value is the file's `tensor-split` entries trimmed and joined with `,`, or `null` when absent.
`tensor-split` is changed only when the device state changed (GD1.10) or the weight fields differ from
the values they loaded with, **and** the composed value differs from the loaded one; sent when changed:
the composed value. So: `device = A,B`, `tensor-split = 3/1` untouched is not sent; Move up on `B`
sends `device` `B,A` and `tensor-split` `1,3`; unchecking `B` sends `device` `A` and `tensor-split`
`null`; a short vector the user does not touch (`device = A,B,C`, `tensor-split = 3,1`) is not sent.

**GD2.6 The fe's check.** At Save, when `tensor-split` is changed and composed non-null: each checked
row's weight must match `^(\d+(\.\d*)?|\.\d+)$` (a number of 0 or more, as llama.cpp's `stof` reads
it, no sign, no exponent). Otherwise nothing is sent and the GPUs fieldset shows
`model-profile-gpu-weights-incomplete` as a D5.2 Error after the list (after the device error when
both show), every weight field that fails `aria-invalid="true"` with the error's id in its
`aria-describedby`, focus on the first failing field, and the bar's status as every check
(`fe/DESIGN.md` 6, "Refused by a check"). Typing in a weight field, Auto, Even, or a change of the
checked rows clears it.

**GD2.7 The server's refusals.** A 422 `tensor-split-format` (`field` `tensor-split`) shows
`api-tensor-split-format` filled with the value as the weights' error of GD2.6's place, every visible
weight field `aria-invalid`, focus on the first weight field, or, with none showing (an untouched bad
value under `none` or with one GPU checked), on the first checked checkbox, else the split mode
select; the bar's alert `save-refused` with the text (02h 142). `moe-cache-with-tensor-split` is
unchanged (02h 150: on `moe-cache-mib`, 144's focus rule while it is disabled); `not-one-of` on
`split-mode` marks the split mode select and focuses it (02h 142).

**GD2.8 A `tensor-split` without a `device`.** When the file has `tensor-split` and no GPU in
`device`, a `--muted` paragraph after the as-of line says `model-profile-gpu-split-without-device`
filled with the value as written; it shows while no row is checked. Checking rows then sends the
composed value by GD2.5 (`null` while the new weights are empty), which replaces it.

**GD2.9 Keyboard and screen reader.** The weight field sits between its row's checkbox and Move up in
the Tab order; Auto and Even after the last row. A screen reader on a weight field hears "Weight
rocm0, edit text, 3, Share: 75%"; nothing is announced while typing (the share is not live). No
sideways scroll at 320 px and 200 % text with three checked rows, each with a 15-character weight
(the field scrolls its own text).

**GD2.10 Nothing else.** The strings file gains exactly GD2's keys of section 2. Draft device, Vision
GPU, Tensor placement and the split mode warnings are unchanged.

## 4. Out of scope

- The block projection and a block slider (needs the model's `block_count`, 2g); GPU layers stays a
  number field in Hardware.
- The split mode warning strips (tensor with no GPU checked, CUDA with ROCm, flash attention off,
  architecture): 2k.
- Filling Draft device and Vision GPU with the listed GPUs (the GUI's combos list them): a follow-up
  of one criterion, now that the list exists (question Q2).
- Tensor placement (`override-tensor`, 2j); a refresh of the probe (user, 2026-11-20: none);
  `device = none` gets no special row (it reads as an id the list lacks, as in the GUI).
- The memory check per GPU (user, 2026-11-20: it counts every GPU of the host).

## 5. Accessibility (WCAG 2.2 AA)

- The GPUs are a fieldset named by its legend, a list of rows, each checkbox named by its id and
  product name and described by its memory line; weight fields and move buttons carry the id in their
  names, so each is told apart out of context
  ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
  [2.4.6](https://www.w3.org/TR/WCAG22/#headings-and-labels),
  [4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)); the visible text starts each name
  ([2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name)).
- No dragging: order by buttons, weights by typing
  ([2.5.7](https://www.w3.org/TR/WCAG22/#dragging-movements)); everything by keyboard
  ([2.1.1](https://www.w3.org/TR/WCAG22/#keyboard)); focus stays on the control the user pressed when
  its row moves ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)).
- The share is text ("Share: 75%", "unused"), the not-detected state a word, never a colour
  ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)); the free memory is dated, so it is not read
  as now ([3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions)).
- The move's result is a status message, in a region present before the first move; "Detecting
  GPUs…" and the read's outcome are part of the card as it renders and appear without a user action,
  so they are plain paragraphs, neither status nor alert ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)).
- Errors name the problem and the fix, on the controls they concern
  ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification),
  [3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion)).
- Reflow at 320 px and 200 % text ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)); targets at least
  24 px ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).

## 6. Older tests that pin something that changes

- 02e/02h tests that list the cards or the jump list (criterion 83, 196): GPU distribution joins
  between Assets and Hardware.
- The bff fake (`test/bff.ts`): it needs `GET /api/node/devices`; a suggestion, the tester's call:
  answer it only when a test asks, and let the other page tests see it pending or failed, which
  leaves every other card as today (decision 5).
- The page tests' "one status line once the page is loaded" helpers (`fe/src/test/savealert.tsx`
  `status()`, `oneStatus`): they meant the save bar's status line; with the GPUs card's move region
  always present they pick the status inside the save bar instead of counting every `role="status"`
  after the h2 (GD1.9).
- The fake's `PUT` answer: the contract's `{ revision, running }` (02h, `fe/briefs/api-contract.md`);
  this brief changes nothing in the save's reply.
- Tests pinning the PUT's accepted keys or that `device`, `split-mode`, `tensor-split` are never sent.

## 7. Questions for the user

- **Q1.** Number fields with the share beside them, plus Move up / Move down, instead of the GUI's
  sliders and drag handle (decisions 1 and 2). Recommended: yes, until 2g brings the model's block
  count; then a block slider can be added beside the field.
- **Q2.** Should Draft device and Vision GPU list the probed GPUs in the same slice (one more
  criterion in GD1), or later? Recommended: later, a follow-up slice, to keep GD1 at 12 criteria.
