# Brief 25: ROCm host memory as GTT on the Server page (slice GT2)

User, 2026-11-24. The design is `ARCHITECTURE.md` "ROCm host memory as GTT". Slice GT1 built the
server profile key `RocmHostMemoryGtt` (bool). When it is unset or `true`, the launch exports
`HSA_USERPTR_FOR_PAGED_MEM=0` and `HSA_SVM_FOR_PAGED_MEM=0` to llama-server. When it is `false`, the
launch exports neither. The bff reads and saves the key, and refuses a value that is not a bool with
422 and `field`. This slice adds it as a field of the Server form's "SDK tuning" card, so the Server
page and every server profile page get it.

## The field

It follows the `Webui` pattern (`fe/src/ServerForm.tsx`), the form's other key that is on by default:
a checkbox, checked when the key is unset or `true`, saved as `true` or `false`. It is not the
hipBLASLt default/on/off select: that select exists because "default" there leaves the choice to
rocBLAS, while here the default is plainly on. The GUI (`crates/config/src/server_form.rs`) keeps a
tri-state with no widget yet. It differs on purpose: the web form writes `true` where the GUI would
leave the key unset, and both mean the same launch.

## Criteria (GT2.1 to GT2.5)

**GT2.1 Place.** The field is a `CheckboxField` with id `${fid}-RocmHostMemoryGtt`, in the ROCm
subgroup of the SDK tuning card. It comes after hipBLASLt and before Profiler attach, so the
order is hipBLASLt, Host memory as GTT, Profiler attach.

**GT2.2 Words** (new keys in `fe/src/strings/en_US.json`):
- `server-rocm-gtt`: `Host memory as GTT` (the label)
- `server-rocm-gtt-toggle`: `keep ROCm's host memory in GTT` (the checkbox's text)
- `server-rocm-gtt-hint`: `Puts the memory ROCm takes from system RAM in the graphics driver's GTT
  pool, so the kernel no longer moves it while llama-server runs. Without this, memory compaction
  and reclaim can stall the GPU in the middle of a request.\nTurn it off if loading a model fails
  after this was switched on: GTT memory cannot be swapped out, and GTT is capped near half the
  RAM, so a large allocation in system memory can fail where it used to succeed.\nChecked = set both
  variables to 0 (the default); unchecked = don't set them, so ROCm's own defaults apply.\nNo effect
  on CUDA, Vulkan or Metal builds.\nVariables: HSA_USERPTR_FOR_PAGED_MEM=0,
  HSA_SVM_FOR_PAGED_MEM=0`
  The `\n` breaks follow the other hints of the card (each starts a paragraph: `fe/DESIGN.md`, Hint).

**GT2.3 Default shown.** A document whose `values.RocmHostMemoryGtt` is `null` or missing shows the
checkbox checked, and so does one with `true`. One with `false` shows it unchecked.

**GT2.4 Save.** The save's `values` carry `RocmHostMemoryGtt`: `true` when checked and `false` when
unchecked, as `Webui` is saved. Unchecking marks the form dirty, and Discard puts it back. A 422
with `field: "RocmHostMemoryGtt"` shows its text as the field's error under the checkbox
(`aria-describedby`, the form's error pattern), and the first error takes the focus, as for every
field ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification)).

**GT2.5 Accessibility.** The checkbox has the label as its accessible name and the hint as its
description, as the card's other checkboxes do
([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
[4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)). Space toggles it, and it sits in the tab
order between hipBLASLt and Profiler attach ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)). It
adds no new component, token or style.

Out of scope: the GUI widget in `pai-config`, a check of the backend (the hint says which builds
ignore it), and the docs.

Tests to extend: `fe/src/ServerSaveKeys.test.tsx` (the key in the saved values, both states) and
`fe/src/ServerSave.test.tsx` (the place, the default shown, the 422 field). If the test fixtures
(`fe/src/test/bff.ts`) list the server document's keys, the key is added there as `null`.

## GT2 review (designer, 2026-11-25)

Driven in Chromium, Firefox and WebKit at 1280 and 320 px, light, dark, 200 % text and (Chromium)
forced colours (`build/ux-review/gt2.js`). The stub's document carries `true`; the unset case is
pinned by the tests.

- GT2.1 met: the field is in the ROCm subgroup, ordered hipBLASLt, Host memory as GTT, Profiler
  attach.
- GT2.2 met: the label, the toggle text and the hint as briefed. The About toggle opens the hint as
  5 paragraphs.
- GT2.3 met for `true`: the checkbox is checked.
- GT2.4 met: Space unchecks it and Save goes active. Space again re-checks it.
- GT2.5 met as the card's other checkboxes do it: the legend "Host memory as GTT" names the group,
  the checkbox is named by its toggle text, and the hint is behind its About toggle
  (`aria-controls`), not an `aria-describedby`. The brief's "the hint as its description" was
  inexact; this pattern is the one `fe/DESIGN.md` (Hint) sets. The tab order is About, the checkbox,
  then the next About. The box scales to 40 px at 200 %, there is no sideways scroll at 320 px or
  200 %, and under forced colours the box and its check are drawn.

## GT5b: what an unset Load mode means (designer, 2026-11-25)

GT5a (f545865, `ARCHITECTURE.md` "ROCm instances load models with `load-mode = none`"): when a model
profile leaves `load-mode` unset, the manager's render gives a ROCm instance `load-mode = none` in
`[*]`, unless the pool's `[*]` sets its own. Any other instance keeps llama.cpp's `auto`. Both
pages still say "Default: auto". This slice corrects the words on the web page and in pai-config,
with the same text in both.

**Decision: static words, not the effective value (preference, argued).** The page cannot say
reliably which value applies:
- A model profile is served by every instance its assignments give it, and two of them can differ
  (one ROCm, one Vulkan).
- "ROCm" also depends on the engine's installed backends when `Devices` is unset, which no web
  route returns.
- Telling it would take the instance list, every server profile read and the device probe (one
  read per server profile, plus the slowest route), for a hint.

A new API field (say an `effectiveLoadMode` per instance) would move that judgement into the bff
for one select. I do not argue for one. The static words are true everywhere and match the
architecture record's "What the pages show".

**GT5b.1 The unset option says what it means.** In the Load mode select only, the first option (the
unset value, sent as no key) reads `default (none on ROCm, auto elsewhere)` instead of `default`.
- Web: new key `model-profile-load-mode-default` in `fe/src/strings/en_US.json`, used by
  `LOAD_MODES[0].text` (`ModelProfileCards.tsx` 366-374). `option-default` stays for every other
  select.
- GUI: new key `load-mode-default` in `crates/core/strings/en_US.toml`, used as the first entry of
  `Options.load_mode_labels` (`crates/config/ui/components.slint` 103) in place of
  `Strings.t("default")`.
- The value written stays `default` (web) and `""` (GUI): nothing changes in the file.
- At 320 px and at 200 % text the closed select shows the whole text or wraps, with no sideways
  scroll ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)); the GUI's combo is as wide as the form's
  other combos.

**GT5b.2 The About text.** `model-profile-load-mode-hint` (web) and `load-mode-hint` (GUI) get the
same new text. Only the "Default" line changes, and two lines are added after it:

```
How this model's weights get into memory. Pick one: the modes do not combine.
Default: none on an instance that runs on ROCm GPUs (Linux), auto everywhere else. A value in the models file's [*] section wins over both.
Why none on ROCm: memory-mapped weights are pinned by the ROCm runtime, and the driver evicts them now and then, which stalls the model; read into RAM up front, they are not.
The cost: layers kept on the CPU (n-cpu-moe, or override-tensor to CPU) then sit in RAM the system cannot drop or share, and the load reads the whole file. A profile that keeps many layers on the CPU can set mmap here.
auto: memory-maps the file (reads it on demand) unless a device can't; fastest to load.
none: reads the weights into RAM up front: slower, uses the model's size in RAM, but nothing can be dropped back to the file.
mmap: always memory-maps.
mlock: locks the weights in physical RAM WITHOUT memory-mapping: steady latency, needs room for the whole model.
mmap+mlock: memory-maps and locks what it maps.
dio: Direct I/O where the platform offers it.
Lazy tensors stay mapped whatever this says.
Flag: --load-mode
```

There are no figures in the text: the measurements live in `PLAN.md` GT4 and `ARCHITECTURE.md`.
The About toggle and its `aria-expanded` are unchanged (DESIGN.md Field). The text is help, so its
length is accepted behind About.

**GT5b.3 Nothing else changes.**
- A set value shows as it is.
- The select's options and their order stay.
- No new route or field.
- macOS and Windows show the same words: the "(Linux)" in the text covers them, and the GUI there
  never meets a ROCm instance.

Tests to amend or add:
- Web:
  - `fe/src/ModelProfileCards4.test.tsx` (256 and 344, the hint keys: unchanged keys, check
    nothing pins the old line);
  - `fe/src/ModelProfileSave13.test.tsx` 489 (2h.180, the Load mode options: the first option's
    text becomes the new key's);
  - new test: the first option of Load mode reads `model-profile-load-mode-default`, and every
    other select's unset option still reads `option-default`.
- GUI: `crates/config/src/tests/ui_bindings.rs` 410-418 (the `load_modes` drift check: values
  unchanged). Add a check that `load_mode_labels[0]` is `Strings.t("load-mode-default")`, and that
  the key exists in `en_US.toml`.
- Strings: the two hint texts are equal, line for line, in both files. A parity test, if the
  tester adds one, compares `model-profile-load-mode-hint` with `load-mode-hint`.
