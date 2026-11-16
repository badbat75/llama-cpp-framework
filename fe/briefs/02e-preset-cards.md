# Brief 02e: the preset page's plain cards, read-only

Written by the designer on 2026-11-16 for `PLAN.md` step 10, web UI slice 2e (the slice table of
[02-models.md](02-models.md) section 0). Applies `fe/DESIGN.md` ("D5.2" points to its sections) and
builds on 2d ([02-models.md](02-models.md) section 2d): everything there stays true unless a line
here changes it. Criteria go on from 2d's numbers (2d ended at 82).

The reference is `pai-config`'s Models tab: the cards in `crates/config/ui/models_page.slint`
(cited "MP" with lines), their texts in `crates/core/strings/en_US.toml` (cited "toml" with lines),
how a preset is read in `Preset::from_keys` (`crates/app/src/presets.rs` 770-884) and shown in
`preset_to_form` (`crates/config/src/form.rs` 146-314).

**Split in five sub-slices**, each one tester run and one coder run, each shippable alone. 2e.1
settles every shared rule; 2e.2 to 2e.5 are mostly field tables applying it. A tester working on
2e.N reads section 0, section 1 (the shared rules, criteria 83 to 91) and section 2e.N only.

| Sub-slice | Criteria | Holds |
|---|---|---|
| **2e.1** | 83-95 | The shared rules (read-only Notice, every control `disabled`, how each kind of field shows a value, absent keys, unknown values, units, hints), the section fix from the 2d review, and the cards Resource / context and Embeddings |
| 2e.2 | 96-100 | KV cache, Sampling overrides, Advanced |
| 2e.2b | 118-121 | The full-value line: a long value stays readable under its disabled control (added 2026-11-16 after the 2e.1 review; applies to every field built so far and, through the shared renderer, to 2e.3 to 2e.5) |
| 2e.3 | 101-106 | Chat / reasoning, Vision |
| 2e.4 | 107-111 | Hardware |
| 2e.5 | 112-117 | Speculative decoding |

## 0. The main decision: real controls, disabled

**Decided: each field is the real control it will be when 2h saves (default box and input,
checkbox, select, text input), rendered with the native `disabled` attribute, under one Notice
saying the page is read-only.** Not a value list like the Assets card.

Why:

- **Built once.** 2h then enables the controls, adds the enable rules, dirty tracking and the save;
  the 2e tests that read a control's role, name and value stay valid. A value list now would be
  thrown away in 2h with its ~70 fields' tests.
- **The Server page did the same** (controls from 1a, saved by 1b) with the same components
  (`fe/src/Field.tsx`: `DefaultBoxField`, `CheckboxField`, `SelectField`, `TextField`): no new
  component, and the page looks the same before and after 2h.
- **Same widget as the GUI** (D1 principle 1): a tri-state reads as a choice among default / on /
  off, a checkbox as a checkbox. A value list would need a text form for every widget kind.
- **Accessibility holds.** A disabled control is not focusable and not in the tab sequence (HTML
  Standard, [disabled form controls](https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#enabling-and-disabling-form-controls:-the-disabled-attribute)),
  so ~70 inert controls add no tab stops; its disabled state and value are exposed natively
  ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)); screen readers read disabled controls
  with their value in browse mode and say "unavailable" or "dimmed" (general behaviour of NVDA,
  JAWS and VoiceOver, not checked per reader for this brief). WCAG exempts inactive components from
  [1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum) and
  [1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast); D5.2 "Disabled controls stay
  readable" still draws their values in `--muted` (6.1:1 light, 7.6:1 dark on `--bg`, D5.1). The
  Notice says why everything is disabled ([3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions)).
- **Not `readonly`**: HTML's `readonly` applies to text-like inputs only, not to checkboxes or
  selects ([HTML Standard, the readonly attribute](https://html.spec.whatwg.org/multipage/input.html#the-readonly-attribute)),
  so a page of mixed widgets would be half focusable and half not. **Not `aria-disabled`** (D5.2
  Inactive): it keeps every control in the tab sequence, ~70 stops where nothing can be done.

**Changed from the slice table: the enable rules move to 2h.** With every control disabled, a rule
that disables one more shows nothing, so a test of it would test nothing a user sees; and several
rules need data the page does not have yet (the model's MoE and MTP facts: 2g). Each table below
still lists its rules (column "Enable rule (2h)") so 2h has them in one place.

**The Assets card stays a value list** until 2i: its controls are pickers over a models-folder scan
the page does not have. The Engine select stays as 2d made it (enabled, one option, criterion 70).

## 1. Slice 2e.1: shared rules, Resource / context, Embeddings

### 1.1 Page structure

83. **The section fix (2d review).** The `<section>` that holds the `h2` `preset-settings-title`
    holds, in this order: the `h2`; the Notice of criterion 84; the engine Notice of criterion 72
    (when shown); the Engine field; the Assets card; then the 2e cards, in the GUI's order (MP
    472-1356): Hardware, Resource / context, Embeddings, KV cache, Chat / reasoning, Sampling
    overrides, Advanced, Vision, Speculative decoding. Each sub-slice adds its cards at their place
    (2e.1 adds only Resource / context and Embeddings, both after Assets); 2f and 2g will insert
    Model info, GPU distribution and Tensor placement between Assets and Hardware (MP 288-470). The
    Assets card moves inside the section, as the Server page nests its cards (`ServerSection.tsx`).
    The off line `preset-off` (criterion 69) stays above the `h2`. Replaces criterion 68's order. A
    test: each card's own `<section>` (D5.2 Card) lies inside the `<section>` whose `h2` is
    `preset-settings-title`. ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
    [1.3.2](https://www.w3.org/TR/WCAG22/#meaningful-sequence))
84. **The read-only Notice.** Right after the `h2`, a D5.2 Notice (bordered paragraph, not a live
    region: it is part of the page as read): `preset-read-only`. Shown for every loaded preset; 2h
    removes it. ([3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions))
85. **Every control of a 2e card is disabled**: the native `disabled` attribute (never
    `aria-disabled`, never `readonly`) on each input, checkbox and select, the default boxes
    included. None takes focus. Each field's **About** toggle stays enabled (criterion 88). Changing
    nothing is possible, so nothing is ever sent and there is no save bar (criterion 71 holds).

### 1.2 How a field shows its value

86. **The value read.** Each field reads `values[<key>]` of the 2d reply (criterion 81) for its key
    in the tables. **Absent** means the key is not in `values` **or** its value is `""`; the GUI
    reads both alike (`from_keys`: `parse_int("")` and `parse_bool("")` give none). Keys the page
    does not show are still ignored (criterion 81).
87. **The four kinds of field**, each the D5.2 Field component the Server page uses, named by its
    label (so `getByRole(<role>, { name: <label> })` finds the control):
    - **Number** (`DefaultBoxField`, the GUI's `DefaultLineEdit` and `AutoSlider`): a group named by
      the label holding the checkbox `field-default` ("default", the GUI's box text for both
      widgets, MP 389 and 308 of `components.slint`) and a text input named by the label.
      Absent: the box checked, the input's value `""`. Present: the box unchecked, the input's value
      the reply's string **exactly** (no parsing, no reformatting: `0172032` shows `0172032`, `abc`
      shows `abc`). Integer keys get the input's `numeric` form (`inputmode="numeric"`), float keys
      not (a plain text input). Differs from the GUI on purpose: the GUI parks a ticked box on a
      number of its own (`form.rs` 91-144, e.g. 32768 for Context size, an invented figure), and
      reads a value it cannot parse as unset; the web shows what the file holds (D1.3) and leaves
      the default to the hint, which names it.
    - **Select** (`SelectField`, the GUI's `SegmentedControl` and `EnumComboBox`): a native
      `<select>` with the table's options, in the table's order, each option's text its strings key
      or literal. The selected option follows the table's mapping column. **A value the mapping does
      not list** is shown as one extra, last option whose value and text are the reply's string as
      written, selected (as the "(not available)" engine of D2.2). Differs from the GUI on purpose
      for the tri-states, which the GUI reads as default when the file holds anything but `true` or
      `false` (`ini::parse_bool`, `crates/core/src/ini.rs` 523-529), and would then drop on save.
    - **Checkbox** (`CheckboxField`, the GUI's `CheckBox`): a group named by the label holding one
      checkbox whose text is the table's toggle text. Checked per the table's mapping; a value it
      does not list reads as the GUI reads it (the table says which).
    - **Text** (`TextField`, the GUI's `LineEdit`): a text input named by the label, monospace, its
      value the reply's string or `""` when absent. **No placeholder** in 2e: an example in a field
      nobody can type into reads as a value **(preference)**; the GUI's examples are 2h's to judge.
88. **Hints.** Every field has the D5.2 Hint: the **About** toggle after the label (accessible name
    "About " plus the label, `aria-expanded`, `aria-controls` the hint), the hint hidden until
    pressed, each `\n` of its text a paragraph. The text is the GUI's tooltip (the tables name the
    toml line), unchanged unless the table gives the web's English: a GUI text is changed only where
    it is never true on the web (it names a slider or a number the GUI parks on), not where a later
    slice makes it true (a hint naming Model info, which 2g adds, or Save, which 2h adds).
    ([3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions))
89. **Units.** A unit sits after the input as the Server page's Memory margin does (`unit` of
    `DefaultBoxField`: the literal `MiB`, not a string), never in the label, even where the GUI's
    label carries it ("Cache RAM (MiB)" becomes "Cache RAM" plus `MiB`). Numbers, units, flags and
    INI key names are literals, never strings (D7).
90. **Notes inside a card** (the GUI's muted `Text` between fields): a paragraph in `--muted`,
    0.875rem, at the GUI's place, as the card intro is drawn (D5.2 Card). **Groups inside a card**
    (the GUI's bold sub-headings): an `h4`, and the group a `<div role="group">` named by its `h4`
    (`aria-labelledby`), so fields that share a label in two groups ("Lookup n-gram", 2e.5) are told
    apart by their group ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
    [2.4.6](https://www.w3.org/TR/WCAG22/#headings-and-labels)).
91. **Keyboard and screen reader**, for every sub-slice. Tab order on the preset page: skip link,
    banner, nav, `preset-back`, the Engine field's About toggle and select, then each field's About
    toggle in card and field order (Assets has none). Nothing else takes focus. A screen reader on
    arrival hears the `h1` (focus); in browse mode, each card as a heading, each field's label, its
    control's role, its value and "unavailable" (or the reader's word), the default box as "default,
    checkbox, checked, unavailable". Nothing is announced when the page loads apart from the loading
    status going away. Width: the D4 field grid as on the Server page; at 320 px and at 400 % zoom
    no horizontal scroll: a long value never widens the page, it is cut off inside its control's
    box (an `<input>` or a `<select>` cannot wrap, and a disabled one cannot be scrolled). It is
    shown in full by the full-value line, which sub-slice 2e.2b adds (criteria 118 to 121): not
    part of 2e.1 or 2e.2. Checked in review, not in jsdom.
    ([2.1.1](https://www.w3.org/TR/WCAG22/#keyboard), [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order),
    [4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value), [1.4.10](https://www.w3.org/TR/WCAG22/#reflow))

### 1.3 Shared option texts

Used by every select below. Texts of the GUI (`option-*` keys exist: `option-default` "default",
`option-on` "on", `option-off` "off", `option-none` "none").

- **Tri-state** (the GUI's default / on / off `SegmentedControl`): options `default`
  (`option-default`), `on` (`option-on`), `off` (`option-off`). Mapping: absent shows `default`,
  `true` shows `on`, `false` shows `off` (`emit_bool` writes `true` / `false`, `presets.rs`
  2146-2150).
- **Off / on** (the GUI's two-pill `SegmentedControl` of a key written only when true): options
  `off` (`option-off`), `on` (`option-on`). Mapping: absent and `false` show `off`, `true` shows
  `on` (`from_keys`: `getb(..) == Some(true)`).

### 1.4 The cards of 2e.1

92. **Resource / context** (MP 674-741), a D5.2 Card, `h3` `preset-resource-context`:

    | Key | Label | Hint | Control | Absent shows | Enable rule (2h) |
    |---|---|---|---|---|---|
    | `ctx-size` | `preset-context-size` "Context size" (toml 287) | `preset-context-size-hint`, web text below | Number, integer | box checked, `""` | always |
    | `parallel` | `preset-parallel-slots` "Parallel slots" (toml 289) | `preset-parallel-slots-hint` = toml 290 | Number, integer | box checked, `""` | always |
    | `kv-unified` | `preset-unified-kv` "Unified KV" (toml 291) | `preset-unified-kv-hint` = toml 292 | Select, tri-state | `default` | enabled when `parallel` is present (MP 704) |
    | `kv-unified-per-slot` | `preset-per-slot-context` "Per-slot context" (toml 293) | `preset-per-slot-context-hint` = toml 294 | Number, integer | box checked, `""` | always |
    | `batch-size` | `preset-batch-size` "Batch size" (toml 295) | `preset-batch-size-hint` = toml 296 | Number, integer | box checked, `""` | always |
    | `ubatch-size` | `preset-u-batch-size` "U-Batch size" (toml 297) | `preset-u-batch-size-hint` = toml 298 | Number, integer | box checked, `""` | always |

    `preset-context-size-hint` (toml 288 without ", which can be far more than the number shown
    here", a number only the GUI shows): "How many tokens of conversation the model can hold.
    Bigger = longer history, more VRAM for the KV cache. It can be divided among requests: see
    Parallel slots.\nDefault: 0 (llama.cpp's own), meaning the context the model was trained
    with.\nFlag: --ctx-size"
93. **Embeddings** (MP 743-769), `h3` `preset-embeddings`:

    | Key | Label | Hint | Control | Absent shows | Enable rule (2h) |
    |---|---|---|---|---|---|
    | `embeddings` | `preset-embedding-mode` "Embedding mode" (toml 299) | `preset-embedding-mode-hint` = toml 300 | Select, off / on | `off` | always |
    | `pooling` | `preset-pooling` "Pooling" (toml 301) | `preset-pooling-hint` = toml 302 | Select: `default` (`option-default`), `none` (`option-none`), `mean` (`option-mean`), `cls` (literal), `last` (`option-last`), `rank` (`option-rank`) (MP 763-764) | `default` | always |

    Pooling's mapping: a value equal to an option's value (exactly, as the GUI's combo compares)
    selects it; anything else is the extra option of criterion 87.
94. **The tests stub** (2e.1): a 200 with every key of 92 and 93 set (`ctx-size` `172032`,
    `parallel` `2`, `kv-unified` `true`, `kv-unified-per-slot` `32768`, `batch-size` `4096`,
    `ubatch-size` `1024`, `embeddings` `true`, `pooling` `last`); a 200 with `values` `{}`; one
    with `ctx-size` `""` (box checked); `ctx-size` `abc` (shown as `abc`, box unchecked);
    `kv-unified` `false` (`off`) and `auto` (extra option `auto`, selected); `pooling` `weird`
    (extra option). Each test also checks that the control and its default box are disabled and the
    About toggle is not.
95. **Strings, 2e.1** (22 new; `fe/src/strings.test.ts` keeps each used):

    | Key | English | Source |
    |---|---|---|
    | `preset-read-only` | Read-only for now: this page cannot change a preset's settings yet. Change them in pai-config on the server machine. | new; removed by 2h |
    | `preset-resource-context` | Resource / context | toml 286 `resource-context` |
    | `preset-context-size` | Context size | toml 287 |
    | `preset-context-size-hint` | (criterion 92) | toml 288, changed |
    | `preset-parallel-slots`, `-hint` | toml 289, 290 | `parallel-seqs`, unchanged |
    | `preset-unified-kv`, `-hint` | toml 291, 292 | unchanged |
    | `preset-per-slot-context`, `-hint` | toml 293, 294 | unchanged |
    | `preset-batch-size`, `-hint` | toml 295, 296 | unchanged |
    | `preset-u-batch-size`, `-hint` | toml 297, 298 | unchanged |
    | `preset-embeddings` | Embeddings | toml 229 |
    | `preset-embedding-mode`, `-hint` | toml 299, 300 | unchanged |
    | `preset-pooling`, `-hint` | toml 301, 302 | unchanged |
    | `option-mean` | mean | toml 507 |
    | `option-last` | last | toml 508 |
    | `option-rank` | rank | toml 509 |

    Reused: `field-default`, `field-about`, `option-default`, `option-on`, `option-off`,
    `option-none`. Literal: `cls`.

**Out of 2e.1**: the other cards (2e.2 to 2e.5), the enable rules (2h), any warning strip (2k),
Model info, GPU distribution and Tensor placement (2f, 2g), saving (2h).

## 2e.2. KV cache, Sampling overrides, Advanced

Applies criteria 83 to 91. Cards at their place of criterion 83.

96. **KV cache** (MP 771-861), `h3` `preset-kv-cache`. Cache-type options (MP 95-96 of
    `components.slint`): `default` (`option-default`), then the literals `f32`, `f16`, `bf16`,
    `f8_e4m3`, `q8_0`, `q5_1`, `q5_0`, `q4_1`, `q4_0`, `iq4_nl`; absent shows `default`, a value equal
    to an option's value selects it, anything else is the extra option.

    | Key | Label | Hint | Control | Absent shows | Enable rule (2h) |
    |---|---|---|---|---|---|
    | `cache-type-k` | `preset-k-cache-type` "K-cache type" (toml 304) | = toml 305 | Select, cache types | `default` | always |
    | `cache-type-v` | `preset-v-cache-type` "V-cache type" (toml 306) | = toml 307 | Select, cache types | `default` | always |
    | `flash-attn` | `preset-flash-attention` "Flash attention" (toml 308) | = toml 309 | Select, tri-state | `default` | always |
    | `cache-ram` | `preset-cache-ram` "Cache RAM" (toml 310 without "(MiB)") | = toml 311 | Number, integer, unit `MiB` | box checked, `""` | always |
    | `cache-reuse` | `preset-cache-reuse` "Cache reuse" (toml 312) | = toml 313 | Number, integer | box checked, `""` | always |
    | `ctx-checkpoints` | `preset-context-checkpoints` "Context checkpoints" (toml 314) | = toml 315 | Number, integer | box checked, `""` | always |
    | `checkpoint-min-step` | `preset-checkpoint-step` "Checkpoint step" (toml 316) | = toml 317 | Number, integer | box checked, `""` | always |

97. **Sampling overrides** (MP 972-1049), `h3` `preset-sampling-overrides`:

    | Key | Label | Hint | Control | Absent shows | Enable rule (2h) |
    |---|---|---|---|---|---|
    | `temp` | `preset-temperature` "Temperature" (toml 26) | `preset-temperature-hint` = toml 341 | Number, float | box checked, `""` | always |
    | `top-k` | `Top-K` (literal, MP 987) | `preset-top-k-hint` = toml 342 | Number, integer | box checked, `""` | always |
    | `top-p` | `Top-P` (literal) | `preset-top-p-hint` = toml 343 | Number, float | box checked, `""` | always |
    | `min-p` | `Min-P` (literal) | `preset-min-p-hint` = toml 344 | Number, float | box checked, `""` | always |
    | `repeat-penalty` | `preset-repeat-penalty` "Repeat penalty" (toml 345) | = toml 346 | Number, float | box checked, `""` | always |
    | `presence-penalty` | `preset-presence-penalty` "Presence penalty" (toml 347) | = toml 348 | Number, float | box checked, `""` | always |
    | `backend-sampling` | `preset-backend-sampling` "Backend sampling" (toml 349) | = toml 350 | Select, tri-state | `default` | always |

98. **Advanced** (MP 1051-1073), `h3` `preset-advanced`:

    | Key | Label | Hint | Control | Absent shows | Enable rule (2h) |
    |---|---|---|---|---|---|
    | `load-on-startup` | `preset-load-on-startup` "Load on startup" (toml 352) | = toml 353 | Select, off / on | `off` | always |
    | `chat-template-kwargs` | `preset-chat-kwargs` "Chat kwargs" (toml 354) | = toml 355 | Text | `""` | always |

99. **The tests stub** (2e.2): every key set (`cache-type-k` `q8_0`, `cache-type-v` `q8_0`,
    `flash-attn` `true`, `cache-ram` `-1`, `cache-reuse` `256`, `ctx-checkpoints` `0`,
    `checkpoint-min-step` `8192`, `temp` `0.6`, `top-k` `20`, `top-p` `0.95`, `min-p` `0`,
    `repeat-penalty` `1.05`, `presence-penalty` `1,5` (shown as written, comma kept),
    `backend-sampling` `false`, `load-on-startup` `true`, `chat-template-kwargs`
    `{"enable_thinking":false}`); `values` `{}`; `cache-type-k` `q3_k` (extra option).
100. **Strings, 2e.2** (32 new): `preset-kv-cache` "KV cache" (toml 303); label and `-hint` pairs
    `preset-k-cache-type`, `preset-v-cache-type`, `preset-flash-attention`, `preset-cache-ram`,
    `preset-cache-reuse`, `preset-context-checkpoints`, `preset-checkpoint-step` (table 96);
    `preset-sampling-overrides` "Sampling overrides" (toml 340); `preset-temperature` and its hint,
    `preset-top-k-hint`, `preset-top-p-hint`, `preset-min-p-hint`, `preset-repeat-penalty`,
    `preset-presence-penalty`, `preset-backend-sampling` and their hints (table 97);
    `preset-advanced` "Advanced" (toml 351); `preset-load-on-startup`, `preset-chat-kwargs` and
    their hints (table 98). Every English is the toml line named, unchanged, except `preset-cache-ram`.

## 2e.2b. The full-value line

Added 2026-11-16 after the 2e.1 review, which rendered a 40-digit Context size cut to its first 8
digits and a long unlisted Pooling value cut at 320 px, with no way to see the rest. Applies
criteria 83 to 91. Built once in the shared field renderer (`fe/src/PresetCards.tsx`), so every
field of 2e.1 and 2e.2 gets it here and 2e.3 to 2e.5 get it without new work. Lands after 2e.2 and
before 2e.3.

**Decided: when a value is longer than its control shows, the field repeats it in full as text
under the control.** Weighed against:

- **`readonly` text inputs** (focusable, so the value can be scrolled with the arrow keys and
  selected): one more tab stop per number and text field, about 55 of the ~70 fields, so ~80 tab
  stops become ~135. A sighted user still sees the value cut off until they focus the input and
  scroll it, and the page then mixes focusable and unfocusable inert controls (section 0).
- **Full-width inputs**: no help at 320 px or 400 % zoom, where the column is about 240 px, and no
  help for a select.
- **Measuring overflow** (`scrollWidth > clientWidth`): exact, but it needs a resize observer and
  cannot be tested in jsdom, which lays nothing out.

The line costs **no tab stop**. Sighted users read the value without doing anything. Screen-reader
users already get the whole value from the control (4.1.2), so the line is hidden from them and
nothing is read twice.

118. **The full-value line.** Under a disabled field's control (after the unit, before the hint), a
    paragraph `preset-full-<key>` holds the shown value as written. A number or text field shows
    the input's value; a select shows the selected option's text. It is drawn in `--muted` and
    the monospace font, 0.875rem, and wraps anywhere (`overflow-wrap: anywhere`), so a value with
    no spaces breaks inside the column
    ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)). It shows only when the value is longer than
    the limit of criterion 119. An absent key never shows it: the input is `""`, and a select shows
    a short option text.
119. **The limits**, by character count, so the rule is the same at every width and testable
    without layout: **Number** more than **8** characters (the input is `10ch` wide,
    `fe/src/index.css` `input.number`; 8 digits were what the review saw fit); **Text** and
    **Select** more than **20** characters (about what a text input or a select holds in the
    ~240 px field column at 320 px, the narrowest case, after its padding and arrow). A value at
    or under the limit has no line. At 1280 px a value of 21 to about 60 characters therefore shows
    both in its box and on the line: accepted, since such values (JSON kwargs, an unknown option)
    are the ones worth reading in full **(preference)**.
120. **Accessibility.** The line has `aria-hidden="true"`: the control's value is already exposed
    in full ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)), and reading it twice in
    browse mode adds nothing. It takes no focus, so criterion 91's tab order is unchanged. It is
    not the control's description (no `aria-describedby`). Contrast is `--muted` on the card,
    which passes [1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum) as text (6.1:1 light,
    7.6:1 dark, D5.1). In review at 320 px: no sideways scroll with a 200-character value.
121. **Tests and strings.** No new string: the line is data. Stub values: `ctx-size` `172032000`
    (9 characters: line `172032000`) and `17203200` (8: no line); `values` `{}` (no line anywhere);
    `pooling` a 21-character unlisted value (line holds it) and a 20-character one (no line);
    `kv-unified` `auto` (no line); `chat-template-kwargs` `{"enable_thinking":false}` (25: line).
    Each line found by its id, with `aria-hidden="true"` and its text equal to the value.

## 2e.3. Chat / reasoning, Vision

Applies criteria 83 to 91, and 118 to 121 through the shared renderer: `reasoning-budget-message`
is the first Text field built after 2e.2b, so 2e.3's tests add one case to criterion 104's stub:
a 21-character budget message shows the full-value line, a 20-character one does not.

101. **Chat / reasoning** (MP 863-970), `h3` `preset-chat-reasoning`. Reasoning-effort options (MP
    917-918): `default` (`option-default`), `minimal` (`option-minimal`), `low` (`option-low`),
    `medium` (`option-medium`), `high` (`option-high`), `xhigh` (`option-extra-high`), `max`
    (`option-maximum`).

    | Key | Label | Hint | Control | Absent shows | Enable rule (2h) |
    |---|---|---|---|---|---|
    | `jinja` | `preset-chat-template` "Chat template" (toml 237) | `preset-chat-template-hint` = toml 319 | Checkbox, toggle `preset-chat-template-toggle` (toml 320) | checked (`false` unchecked; anything else checked: `Preset::default` `jinja: Some(true)`, `form.rs` 258) | always |
    | `prefill-assistant` | `preset-prefill-assistant` "Prefill assistant" (toml 321) | = toml 322 | Checkbox, toggle `preset-prefill-assistant-toggle` (toml 323) | checked (`false` unchecked; anything else checked: `form.rs` 296 `unwrap_or(true)`) | always |
    | `reasoning` | `preset-reasoning` "Reasoning" (toml 324) | = toml 325 | Select: `auto` (`option-auto`), `on` (`option-on`), `off` (`option-off`) | `auto` (`form.rs` 259, `Preset::default` `"auto"`) | always |
    | `reasoning-format` | `preset-reasoning-format` "Reasoning format" (toml 326) | = toml 327 | Select: `auto` (`option-auto`), `none` (`option-none`), `deepseek` (literal) | `auto` | always |
    | `reasoning-effort` | `preset-reasoning-effort` "Reasoning effort" (toml 328) | = toml 329 | Select, reasoning-effort options | `default` | always |
    | `reasoning-preserve` | `preset-keep-past-thinking` "Keep past thinking" (toml 330) | = toml 331 | Select, tri-state | `default` | always |
    | (note) | `preset-output-budget-note` = toml 332, before Thinking budget (MP 933-938) | | criterion 90 | | |
    | `reasoning-budget` | `preset-thinking-budget` "Thinking budget" (toml 333) | = toml 334 | Number, integer | box checked, `""` | always |
    | `reasoning-budget-message` | `preset-budget-message` "Budget message" (toml 335) | = toml 336 | Text | `""` | always |
    | `n-predict` | `preset-max-output` "Max output" (toml 338) | = toml 339 | Number, integer | box checked, `""` | always |

    Mapping for `reasoning`, `reasoning-format`, `reasoning-effort`: a value equal to an option's
    value selects it; anything else is the extra option.
102. **Vision** (MP 1075-1128), `h3` `preset-vision`:

    | Key | Label | Hint | Control | Absent shows | Enable rule (2h) |
    |---|---|---|---|---|---|
    | `mmproj-offload` | `preset-vision-on-gpu` "Vision on GPU" (toml 357) | = toml 358 | Checkbox, toggle `preset-vision-offload-toggle` (toml 359) | checked (`false` unchecked; anything else checked: `Preset::default` `Some(true)`, `form.rs` 160) | `mmproj` present (MP 1082) |
    | `mmproj-device` | `preset-vision-gpu` "Vision GPU" (toml 360) | = toml 361 | Select, device (criterion 103) | the empty option | `mmproj` present and Vision on GPU checked (MP 1092) |
    | (note) | `preset-image-tokens-note` = toml 362, before Min image tokens (MP 1099-1104) | | criterion 90 | | |
    | `image-min-tokens` | `preset-min-image-tokens` "Min image tokens" (toml 363) | = toml 364 | Number, integer | box checked, `""` | `mmproj` present |
    | `image-max-tokens` | `preset-max-image-tokens` "Max image tokens" (toml 365) | = toml 366 | Number, integer | box checked, `""` | `mmproj` present |

103. **A device select** (here Vision GPU; in 2e.4 Draft device): the GUI lists the server's probed
    devices (`devices::build_options`, `crates/app/src/devices.rs` 367-389), which the page does
    not have until 2f. In 2e it holds the empty option first (value `""`, text the table's key),
    then, when the key is present, one option whose value and text are the reply's string (data,
    like an engine name: not marked "(custom)", since without the device list the page cannot tell),
    selected. Absent: only the empty option, selected. 2f adds the devices.
104. **The tests stub** (2e.3): every key set (`jinja` `false`, `prefill-assistant` `false`,
    `reasoning` `off`, `reasoning-format` `deepseek`, `reasoning-effort` `xhigh`, `reasoning-preserve`
    `false`, `reasoning-budget` `-1`, `reasoning-budget-message` `Wrap up now.`, `n-predict` `8192`,
    `mmproj-offload` `false`, `mmproj-device` `ROCm1`, `image-min-tokens` `1024`,
    `image-max-tokens` `2048`); `values` `{}` (both checkboxes checked); `jinja` `yes` (checked,
    as the GUI); `reasoning` `maybe` (extra option).
105. **Strings, 2e.3** (41 new): `preset-chat-reasoning` "Chat / reasoning" (toml 318); the labels,
    hints and toggles of table 101 (`preset-chat-template`, `-hint`, `-toggle`;
    `preset-prefill-assistant`, `-hint`, `-toggle`; `preset-reasoning`, `preset-reasoning-format`,
    `preset-reasoning-effort`, `preset-keep-past-thinking`, `preset-thinking-budget`,
    `preset-budget-message`, `preset-max-output`, each with `-hint`; `preset-output-budget-note`);
    `option-auto` "auto" (toml 494), `option-minimal` "minimal" (511), `option-low` "low" (512),
    `option-medium` "medium" (513), `option-high` "high" (514), `option-extra-high` "xhigh" (515),
    `option-maximum` "max" (516); `preset-vision` "Vision" (toml 356); the labels, hints and toggle
    of table 102 (`preset-vision-on-gpu`, `-hint`; `preset-vision-offload-toggle`;
    `preset-vision-gpu`, `-hint`; `preset-image-tokens-note`; `preset-min-image-tokens`, `-hint`;
    `preset-max-image-tokens`, `-hint`); `preset-device-follows-model` "(default: follows device)"
    (toml 520, Vision GPU's empty option). Every English is the toml line named, unchanged. No
    placeholder string (criterion 87).
106. **Out of 2e.3**: the budget message's example placeholder (toml 337, 2h), the KV warning
    strip and the embedded-MTP note (2k, 2g).

## 2e.4. Hardware

Applies criteria 83 to 91 and 103.

107. **Hardware** (MP 472-672), `h3` `preset-hardware-config`. Lazy-mode options (MP 564-565):
    `default` (`option-default`), `on` (`option-on`), `auto` (`option-auto`), `off` (`option-off`).
    Load-mode options (MP 102-103 of `components.slint`): `default` (`option-default`), `auto`
    (`option-auto`), `none` (`option-none`), then the literals `mmap`, `mlock`, `mmap+mlock`, `dio`;
    a value matching one ignoring case and surrounding space selects it (`normalize_load_mode`,
    `crates/core/src/server_cfg.rs` 275-278), anything else is the extra option.

    | Key | Label | Hint | Control | Absent shows | Enable rule (2h) |
    |---|---|---|---|---|---|
    | `n-gpu-layers` | `preset-gpu-layers` "GPU layers" (toml 256) | web text below | Number, integer | box checked, `""` | always |
    | `n-cpu-moe` | `preset-moe-cpu-layers` "MoE CPU layers" (toml 258) | web text below | Number, integer | box checked, `""` | the model is MoE (MP 493; 2g) |
    | `moe-cache-mib` | `preset-moe-cache` "MoE cache" (toml 260 without "(MiB)") | web text below | Number, integer, unit `MiB` | box checked, `""` | the model is MoE (MP 503; 2g) |
    | `n-cpu-ffn` | `preset-dense-ffn-cpu-layers` "Dense FFN CPU layers" (toml 261) | web text below | Number, integer | box checked, `""` | always |
    | `op-offload` | `preset-op-offload` "Op offload" (toml 263) | = toml 264 | Select, tri-state | `default` | always |
    | `repack` | `preset-repack` "Repack" (toml 265) | = toml 266 | Select, tri-state | `default` | always |
    | `no-host` | `preset-no-host-buffer` "No host buffer" (toml 267) | = toml 268 | Select, off / on | `off` | always |
    | `lazy-mode` | `preset-lazy-tensors` "Lazy tensors" (toml 269) | = toml 270 | Select, lazy-mode options, exact match | `default` | always |
    | `load-mode` | `preset-load-mode` "Load mode" (toml 271) | = toml 272 | Select, load-mode options | `default` | always |
    | `fit` | `preset-fit-to-vram` "Fit to VRAM" (toml 273) | = toml 274 | Checkbox, criterion 108 | unchecked | Context size and GPU layers both absent (MP 585) |
    | `fit-ctx` | `preset-fit-min-context` "Fit min context" (toml 277) | = toml 278 | Number, integer | box checked, `""` | Fit to VRAM checked (MP 595) |
    | `fit-target` | `preset-fit-free-vram` "Fit free VRAM" (toml 279 without "(MiB)") | = toml 280 | Text, unit `MiB` after it | `""` | Fit to VRAM checked (MP 606) |
    | `device-draft` | `preset-draft-device` "Draft device" (toml 282) | = toml 283 | Select, device (criterion 103), empty option `preset-device-same-as-model` | the empty option | `model-draft` present (MP 632) |
    | `n-gpu-layers-draft` | `preset-draft-gpu-layers` "Draft GPU layers" (toml 284) | web text below | Number, integer | box checked, `""` | `model-draft` present (MP 650-665) |

    Web texts (each the toml hint without a line about a slider or a switch the web does not have):
    - `preset-gpu-layers-hint`: toml 257 without "Slider maximum = all layers.\n".
    - `preset-moe-cpu-layers-hint`: toml 259 without "Slider maximum = the model's layer count.\n".
    - `preset-dense-ffn-cpu-layers-hint`: toml 262 without "Slider maximum = the model's layer
      count.\n".
    - `preset-moe-cache-hint`: toml 491 with its `{0}` replaced by `.` (what the GUI fills in with no
      model read, MP 499), so no placeholder: "...and below the experts of one token.\nThe draft
      model...". 2g brings the floor back.
    - `preset-draft-gpu-layers-hint`: toml 285 with its second line ("A drafter with real layers
      (such as DFlash) gets a slider from 0 to its layer count. An MTP head file has no layers, so
      it gets a CPU/GPU switch.") replaced by "For an MTP head file, which has no layers, 99 puts
      the draft on the GPU and 0 on the CPU." (the values the GUI's switch writes, MP 669,
      `Options.all_layers` 99).
108. **Fit to VRAM's checkbox** shows what the GUI shows (`form.rs` 292, `fit_allowed`
    `presets.rs` 2165-2171): *allowed* when `ctx-size` and `n-gpu-layers` are both absent. Its
    toggle text is `preset-fit-allowed` (toml 275) when allowed, `preset-fit-not-allowed` (toml 276)
    when not. Checked only when allowed **and** the file's `fit` is `on` or `true`, ignoring case
    and surrounding space (`from_keys` 866-871). So `fit` `on` with `ctx-size` set shows unchecked,
    "needs Context size and GPU layers at default", as in the GUI; the web does not invent a third
    state **(preference: the same reading as the GUI, where the rest of 2e shows the file; here the
    GUI's own text explains the difference)**.
109. **The tests stub** (2e.4): every key set (`n-gpu-layers` `99`, `n-cpu-moe` `20`,
    `moe-cache-mib` `2048`, `n-cpu-ffn` `4`, `op-offload` `false`, `repack` `true`, `no-host`
    `true`, `lazy-mode` `auto`, `load-mode` `MLOCK` (selects `mlock`), `fit` `on`, `fit-ctx`
    `8192`, `fit-target` `2048,1024`, `device-draft` `ROCm1`, `n-gpu-layers-draft` `0`): Fit to
    VRAM unchecked with `preset-fit-not-allowed`; `values` `{fit: "on"}`: checked with
    `preset-fit-allowed`; `values` `{}`: unchecked with `preset-fit-allowed`; `fit` `TRUE` with
    nothing else: checked.
110. **Strings, 2e.4** (32 new): `preset-hardware-config` "Hardware config" (toml 255); the 14
    labels and 14 hints of table 107 (keys `preset-` plus the GUI key: `gpu-layers`,
    `moe-cpu-layers`, `moe-cache`, `dense-ffn-cpu-layers`, `op-offload`, `repack`,
    `no-host-buffer`, `lazy-tensors`, `load-mode`, `fit-to-vram`, `fit-min-context`,
    `fit-free-vram`, `draft-device`, `draft-gpu-layers`); `preset-fit-allowed`,
    `preset-fit-not-allowed`; `preset-device-same-as-model` "(auto / same as model)" (toml 521).
    English per the table and the web texts of 107; the rest unchanged.
111. **Out of 2e.4**: the embedded-MTP note (MP 620-625, needs the model read: 2g), the slider
    maxima and the CPU/GPU switch of a blockless drafter (2g), the probed device list (2f).

## 2e.5. Speculative decoding

Applies criteria 83 to 91. Cache-type options as 2e.2's criterion 96.

112. **Speculative decoding** (MP 1130-1356), `h3` `preset-speculative-decoding`, five groups of
    criterion 90, in this order, each an `h4` and a `role="group"` named by it:

    | Group `h4` | Key | Label | Hint | Control | Absent shows | Enable rule (2h) |
    |---|---|---|---|---|---|---|
    | `preset-draft-model-group` (toml 368) | `spec-draft-n-max` | `preset-draft-n-max` (toml 369) | = toml 370 | Number, integer | box checked, `""` | a draft: `model-draft` present or the model embeds MTP heads (MP 1151; the MTP half needs 2g) |
    | | `spec-draft-p-min` | `preset-draft-p-min` (toml 371) | = toml 372 | Number, float | box checked, `""` | a draft |
    | | `spec-draft-n-min` | `preset-draft-n-min` (toml 373) | = toml 374 | Number, integer | box checked, `""` | a draft |
    | | `spec-draft-n-ubatch` | `preset-draft-n-ubatch` (toml 375) | = toml 376 | Number, integer | box checked, `""` | a draft |
    | | (note) | `preset-draft-cache-note` = toml 377, before Draft K-cache type | | criterion 90 | | |
    | | `spec-draft-type-k` | `preset-draft-k-cache-type` (toml 378) | = toml 379 | Select, cache types | `default` | a draft |
    | | `spec-draft-type-v` | `preset-draft-v-cache-type` (toml 380) | = toml 381 | Select, cache types | `default` | a draft |
    | `ngram-mod` (literal) | `spec-ngram-mod-n-match` | `preset-match-length` (toml 382) | = toml 383 | Number, integer | box checked, `""` | `spec-type` lists `ngram-mod` (MP 1233) |
    | | `spec-ngram-mod-n-min` | `preset-min-draft` (toml 384) | = toml 385 | Number, integer | box checked, `""` | same |
    | | `spec-ngram-mod-n-max` | `preset-max-draft` (toml 386) | = toml 387 | Number, integer | box checked, `""` | same |
    | `ngram-simple` (literal) | `spec-ngram-simple-size-n` | `preset-lookup-n-gram` (toml 388) | `preset-ngram-simple-lookup-hint` = toml 389 | Number, integer | box checked, `""` | `spec-type` lists `ngram-simple` |
    | | `spec-ngram-simple-size-m` | `preset-draft-m-gram` (toml 390) | `preset-ngram-simple-draft-hint` = toml 391 | Number, integer | box checked, `""` | same |
    | `ngram-map-k` (literal) | `spec-ngram-map-k-size-n` | `preset-lookup-n-gram` | `preset-ngram-map-lookup-hint` = toml 392 | Number, integer | box checked, `""` | `spec-type` lists `ngram-map-k` |
    | | `spec-ngram-map-k-size-m` | `preset-draft-m-gram` | `preset-ngram-map-draft-hint` = toml 393 | Number, integer | box checked, `""` | same |
    | `ngram-map-k4v` (literal) | `spec-ngram-map-k4v-size-n` | `preset-lookup-n-gram` | `preset-ngram-map-values-lookup-hint` = toml 394 | Number, integer | box checked, `""` | `spec-type` lists `ngram-map-k4v` |
    | | `spec-ngram-map-k4v-size-m` | `preset-draft-m-gram` | `preset-ngram-map-values-draft-hint` = toml 395 | Number, integer | box checked, `""` | same |
    | | `spec-ngram-map-k4v-min-hits` | `preset-min-hits` (toml 396) | = toml 397 | Number, integer | box checked, `""` | same |

    "Lists" in 2h means an entry of `spec-type` split as criterion 75 does.
113. **Same labels, told apart by the group.** The three "Lookup n-gram" and three "Draft m-gram"
    fields carry the same visible label as in the GUI; each is found in its group:
    `within(getByRole('group', { name: 'ngram-simple' })).getByRole('group', { name: 'Lookup n-gram' })`.
    Their About toggles are "About Lookup n-gram" thrice, each next to its own hint
    (`aria-controls`) **(preference: the GUI's labels kept; the group heading is announced on entry
    and gives the context, [1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships))**.
114. **Headings**: the `h4` texts are the GUI's: `preset-draft-model-group` (a string) and the
    four speculator names (literals: INI values, D7).
115. **The tests stub** (2e.5): every key set (`spec-draft-n-max` `16`, `spec-draft-p-min` `0.75`,
    `spec-draft-n-min` `2`, `spec-draft-n-ubatch` `512`, `spec-draft-type-k` `q8_0`,
    `spec-draft-type-v` `q8_0`, each n-gram key a distinct number); `values` `{}`.
116. **Strings, 2e.5** (31 new): `preset-speculative-decoding` "Speculative decoding" (toml 367);
    `preset-draft-model-group` (toml 368); `preset-draft-n-max`, `preset-draft-p-min`,
    `preset-draft-n-min`, `preset-draft-n-ubatch`, `preset-draft-k-cache-type`,
    `preset-draft-v-cache-type`, `preset-match-length`, `preset-min-draft`, `preset-max-draft`,
    `preset-min-hits`, each with `-hint`; `preset-draft-cache-note`; `preset-lookup-n-gram`,
    `preset-draft-m-gram`; `preset-ngram-simple-lookup-hint`, `preset-ngram-simple-draft-hint`,
    `preset-ngram-map-lookup-hint`, `preset-ngram-map-draft-hint`,
    `preset-ngram-map-values-lookup-hint`, `preset-ngram-map-values-draft-hint`. Every English is
    the toml line named, unchanged.
117. **Out of 2e.5**: the drafter's own picks (Assets, 2i), the prunes of unused keys at save (2k).
    A list of links to the cards (the page will be ~5000 px long at 1280 px with ~80 About tab
    stops) is to be designed with 2h, not before (2e.1 review).

## 3. Notes for 2h (not tested in 2e)

- The full-value line (criteria 118 to 121) is for disabled controls: an enabled input can be
  scrolled. 2h decides whether it stays on the fields that are still disabled (dependent fields,
  ticked default boxes) and goes from the enabled ones.

- The enable rules are the tables' last column. Rules on "the model is MoE" and "the model embeds
  MTP heads" need 2g; before it, 2h can only use the file's half (`model-draft` present).
- The client checks of the inventory's (f) column use `PRESET_INT_KEYS`
  (`crates/core/src/checks.rs` 120-151): `ctx-size` is `0..` there (0 = the model's context, line
  127-128), not `1..` as [configurator-inventory.md](configurator-inventory.md) 1.2 says (stale).
- `cache-ram`, `reasoning-budget` and `n-predict` take `-1`; a phone keyboard for
  `inputmode="numeric"` may have no minus sign. 2h decides their input mode.
- Unticking a default box: the GUI fills the number it parks on (`form.rs` 103-144); the web has no
  API default for preset keys, so D5.2 "Default box" leaves the input empty unless 2h decides
  otherwise.
