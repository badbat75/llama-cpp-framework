# Brief 21: tensor placement on the model profile page (slice 2j: TP1, TP2, TP3)

`PLAN.md` step 10, order of 2026-11-21, slice 6 ("2j web UI") of `ARCHITECTURE.md` "Layers, MoE and
tensor placement on the web model profile page" (accepted, user 2026-11-21), section (3) "2j:
`override-tensor` on the web". J1 (the key on the web, its two shape rules, the tensor list, `-ot`
devices under the device bound) is built (6570557). It builds on brief 14 (the GPU card) and brief 19
(the model facts read, LS1 and LS2, built at d2dc056). Criteria are numbered **21 T.n**; test names
cite them so (`21 T.4`).

**Three sub-slices, one tester and one coder run each, TP1 first** (designer playbook: more than about
12 criteria per run fails). TP2 and TP3 both need TP1 and not each other: either order, or one run for
both (8 criteria) if the coordinator prefers.

| Sub-slice | Criteria | Holds |
|---|---|---|
| **TP1** | 21 T.1 to 21 T.9 | The rule module (the GUI's `tensor_override.rs` mirrored), the Tensor placement card, its rule rows (Tensor kind, Layers, Pattern, Goes on, Remove), Add rule, saving `override-tensor` |
| **TP2** | 21 T.10 to 21 T.13 | The whole value as text (paste, reorder), the page's own checks at Save, the manager's refusals placed on the rule they name |
| **TP3** | 21 T.14 to 21 T.17 | The model's tensor list from the facts read and, under each rule, how many tensors it places and how much they weigh |

A tester on TP2 or TP3 reads sections 0, 1, 2, 21 T.1 to 21 T.4 (the shapes it builds on) and its own
criteria.

## 0. What exists

- **The save** (`crates/bff/README.md` "The save"): `override-tensor` is a web key since J1. The body
  carries the INI value: rules `<pattern>=<device>` joined by `,`, no spaces, `null` removes the key.
  Refusals, all 422 with `field` `override-tensor`, args the rule's position (1-based, counting the
  non-empty pieces between commas, `checks::override_tensor`) first: `override-no-pattern` `[n]`,
  `override-no-device` `[n, pattern]`, and only when the line changed `override-tensor-space` `[n]` and
  `override-bad-device` `[n, device]`; `comment-markers` (a `;` or `#`, `field` the first key);
  `model-profile-device-outside` `[profile, instance, key, device]` with `key` `override-tensor` when a
  rule's device is a GPU outside the `Devices` of the server profile of an instance the profile is in
  effect on (`CPU` passes; compared case-insensitively; "The device bound").
- **The model info** (`crates/bff/README.md` "The model info"): `info` now also holds `tensors`
  `[{name, bytes}]`, every tensor of every shard in data-offset order, and `tensorsTruncated` (true when
  the manager cut the list at 20 000 rows or 917 504 bytes of JSON). The page reads it once per arrival
  already (19 G.1).
- **The devices** (`GET /api/node/devices`, brief 14 GD1): the probed GPUs, the CPU left out by the
  manager. `CPU` is a legal `-ot` target the page adds itself (`tensor_override.rs` `device_options`).
- **llama.cpp's grammar** (`ARCHITECTURE.md` (3) facts 3 and 4, llama.cpp v1.0.0): the value is split
  on `,`, each rule at its first `=`, nothing trimmed, neither character escapable; the right side is
  looked up **exactly** among the devices' buffer type names (`ROCm0`, `CUDA0`, `Vulkan0`, `CPU`), an
  unknown name failing the model's load; each pattern is compiled with `std::regex` (ECMAScript) and
  searched anywhere in a tensor's name; the first matching rule wins.
- **The GUI** (`crates/app/src/tensor_override.rs`, `crates/config/src/tensor_override.rs`
  `build_rows`, `crates/config/ui/components.slint` `TensorOverrideTable` 1266-1480): a table of rules,
  each a Tensor combo (`KINDS`: 16 named tensor families plus "Custom regex…", in that order), a
  Layers field for a family inside the blocks (`10-12, 15`, written as `blk\.(10|11|12|15)<block>`), a
  Goes on combo (probed devices with CPU, an unknown value kept as "(custom) x"), Choose… (the tensor
  picker, `tensor_picker.rs`), and ×; the Custom row's regex field strips `,` and `=` as typed; under
  each rule a problem line or the coverage line ("12 tensors, 3.41 GiB; 2 more taken by an earlier
  rule"); "+ Add override" seeds the Embedding table on the first GPU (CPU before the probe); a summary
  line shows `--override-tensor <value>`. Texts: `crates/core/strings/en_US.toml` 124, 164-169,
  253-254, 818-849, 1016-1019.
- **The page today**: no Tensor placement card; a refusal naming `override-tensor` shows only in the
  bar's alert (02h 143). `fe/src/ModelProfileCards.tsx` `CARDS` puts GPU distribution first, then
  Hardware.

## 1. Decisions

1. **One card, after GPU distribution and before Hardware** (02e 83 reserved the place; the GUI's
   order). Its jump list entry follows from `CARDS` (02h 196).
2. **Rows of named tensor families are the editor; the raw value stays editable** (the request: place
   tensor groups without writing regexes, keep the raw rules). A rule is a Tensor kind (the GUI's
   `KINDS`, labels from the GUI), Layers for a kind inside the blocks, and Goes on. "Custom regex…"
   shows a Pattern field. Below the rows, **Rules as text** (TP2) holds the whole value: a guide's
   `-ot` string pasted there becomes rows, and it is the way to reorder rules. The tensor picker
   (Choose…) is not in this brief (question Q1).
3. **The fe keeps its own copy of `KINDS`** (architecture (3): constants that change with llama.cpp
   only; an unknown pattern reads as Custom, so a drift never rewrites a rule). Labels are strings
   keys; patterns and block suffixes are literals in the module.
4. **The device is matched exactly**, not case-insensitively as Draft device and Vision GPU are (19
   G.16): llama.cpp looks `-ot` buffer types up exactly (fact 3), so a file's `rocm0` beside a probed
   `ROCm0` is an unlisted value shown as "rocm0 (not detected)", which the user fixes by picking
   `ROCm0`. A deliberate difference from 19 G.16, for that reason.
5. **What the GUI does and the web does on purpose otherwise**: (a) Custom starts from the row's
   current pattern (the GUI empties it), so a family is a starting point for a regex, and the row
   remembers its Custom text and its Layers text for the visit, so moving through the Tensor select
   with the arrow keys (each arrow is a `change`) loses nothing until Discard; (b) a `,` or `=` typed in
   a pattern is refused at Save with a message, not stripped as typed (text vanishing under the caret
   says nothing, [3.3.1](https://www.w3.org/TR/WCAG22/#error-identification)); (c) a Layers text that
   does not read is refused at Save (the GUI keeps the last good value silently); (d) no × glyph: a
   Remove button with its words; (e) the summary line is the Rules as text field.
6. **The coverage lines count with the browser's `RegExp`** (TP3), which is ECMAScript like
   llama.cpp's `std::regex`, but a newer edition: `std::regex` follows ECMAScript 3 with no lookbehind
   and no named groups ([cppreference, "Modified ECMAScript regular expression grammar"](https://en.cppreference.com/w/cpp/regex/ecmascript)).
   So a pattern using those counts on the page and fails the model's load. The page does not judge
   the regex (architecture (3) "Not judged"); the counts are advice.
7. **No reorder buttons and no Confirm on Remove**: a rule's removal is an unsaved edit Discard puts
   back; order changes through Rules as text **(preference: the smallest design; reorder buttons come
   if the user asks, question Q2)**.

## 2. The texts

`…` is U+2026. Patterns, ids, `CPU`, units (`GiB`, `MiB`, `KiB`) and numbers are literals, never
strings (`fe/DESIGN.md` 7).

| Key | English | From | Part |
|---|---|---|---|
| `model-profile-tensor-placement` | `Tensor placement` | toml 253 | TP1 |
| `model-profile-tensor-placement-intro` | `Exceptions to the GPU distribution above: tensors that go on a device of their own, whatever their layer does. On a MoE model, sending the experts of some layers to the CPU frees the most VRAM, as MoE CPU layers does for the first ones. llama.cpp also keeps the embedding table in RAM even when it reports every layer on the GPU: put it on a GPU to reclaim that memory, if the card has room. The first rule that matches a tensor wins.\nKey: override-tensor.` | web, from toml 254 | TP1 |
| `model-profile-ot-rules` | `Rules` | web | TP1 |
| `model-profile-ot-none` | `No rules: every tensor goes where its layer goes.` | toml 838 | TP1 |
| `model-profile-ot-format` | `Layers: empty for every layer, or numbers and ranges such as 10-12, 15. Pattern: a regular expression searched in each tensor's name, such as blk\.1[0-9]\..*_exps.` | web, from toml 167, 168 | TP1 |
| `model-profile-ot-rule` | `Rule {0}` | web | TP1 |
| `model-profile-ot-tensor` | `Tensor` | toml 164 | TP1 |
| `model-profile-ot-layers` | `Layers` | toml 165 | TP1 |
| `model-profile-ot-pattern` | `Pattern` | web | TP1 |
| `model-profile-ot-goes-on` | `Goes on` | toml 166 | TP1 |
| `model-profile-ot-cpu` | `CPU: system memory` | web | TP1 |
| `model-profile-ot-no-device` | `(no device)` | web | TP1 |
| `model-profile-ot-add` | `Add rule` | toml 169 without the `+` (the `plus` icon) | TP1 |
| `model-profile-ot-remove` | `Remove` | web | TP1 |
| `model-profile-ot-kind-embd` | `Embedding table (token_embd)` | toml 818 | TP1 |
| `model-profile-ot-kind-ple` | `Per-layer embeddings (per_layer_token_embd)` | toml 819 | TP1 |
| `model-profile-ot-kind-output` | `Output head (output)` | toml 820 | TP1 |
| `model-profile-ot-kind-layer` | `Whole layer (blk.N.*)` | toml 821 | TP1 |
| `model-profile-ot-kind-attn` | `Attention (attn_*)` | toml 822 | TP1 |
| `model-profile-ot-kind-ssm` | `Linear attention / SSM (ssm_*)` | toml 823 | TP1 |
| `model-profile-ot-kind-ffn` | `Dense FFN (ffn_up/down/gate)` | toml 824 | TP1 |
| `model-profile-ot-kind-ffn_up` | `Dense FFN up (ffn_up)` | toml 825 | TP1 |
| `model-profile-ot-kind-ffn_gate` | `Dense FFN gate (ffn_gate)` | toml 826 | TP1 |
| `model-profile-ot-kind-ffn_down` | `Dense FFN down (ffn_down)` | toml 827 | TP1 |
| `model-profile-ot-kind-exps` | `MoE experts (ffn_*_exps)` | toml 828 | TP1 |
| `model-profile-ot-kind-exps_up` | `MoE experts up (ffn_up_exps)` | toml 829 | TP1 |
| `model-profile-ot-kind-exps_gate` | `MoE experts gate (ffn_gate_exps)` | toml 830 | TP1 |
| `model-profile-ot-kind-exps_down` | `MoE experts down (ffn_down_exps)` | toml 831 | TP1 |
| `model-profile-ot-kind-shexp` | `MoE shared expert (ffn_*_shexp)` | toml 832 | TP1 |
| `model-profile-ot-kind-router` | `MoE router (ffn_gate_inp)` | toml 833 | TP1 |
| `model-profile-ot-kind-custom` | `Custom regex…` | toml 834 | TP1 |
| `model-profile-ot-text` | `Rules as text` | web | TP2 |
| `model-profile-ot-text-hint` | `The whole value as llama.cpp reads it: rules <pattern>=<device> joined by commas, the first match winning. Paste a value from a guide here, or change the order of the rules. Applied when you press Enter or leave the field.\nFlag: --override-tensor` | web | TP2 |
| `model-profile-ot-layers-bad` | `Layers {0} is not a list of layers. Write numbers from 0 to 999 and ranges such as 10-12, 15, at most 256 layers, or leave it empty for every layer.` | web | TP2 |
| `model-profile-ot-pattern-empty` | `Write a pattern, or pick a tensor kind.` | web, from toml 835 | TP2 |
| `model-profile-ot-pattern-separator` | `A pattern cannot hold , or =: llama.cpp splits the rules on them, and neither can be escaped. Write the pattern without them (a range such as {1,2} as a class or an alternative).` | web | TP2 |
| `model-profile-ot-device-missing` | `Pick where this rule's tensors go.` | web, from toml 836 | TP2 |
| `api-override-tensor-space` | `tensor override {0} has a space at the ends of its pattern or its device: llama.cpp keeps it, so the pattern matches no tensor and the device names none. Remove the space` | toml 1018, verbatim | TP2 |
| `api-override-bad-device` | ``tensor override {0} names `{1}`: a device is a card name of letters, digits and `_` (CPU, ROCm0, CUDA1), and llama.cpp spells the CPU device CPU`` | toml 1019, verbatim | TP2 |
| `model-profile-ot-tensors-one` | `1 tensor, {0}` | toml 842 | TP3 |
| `model-profile-ot-tensors-many` | `{0} tensors, {1}` | toml 843 | TP3 |
| `model-profile-ot-matches-none` | `matches no tensor of this model` | toml 840 | TP3 |
| `model-profile-ot-all-taken` | `every tensor it matches is taken by an earlier rule` | toml 841 | TP3 |
| `model-profile-ot-more-taken` | `; {0} more taken by an earlier rule` | toml 844 | TP3 |
| `model-profile-ot-unreadable` | `this pattern cannot be read here, so its tensors are not counted` | web, from toml 839 | TP3 |
| `model-profile-ot-truncated` | `The model has more tensors than the page received: the counts cover the first {0}.` | web | TP3 |

Reused, unchanged: `model-profile-device-option` (`{0}: {1}`), `model-profile-device-option-missing`
(`{0} (not detected)`), `save-invalid`, `save-refused`, `api-override-no-pattern`,
`api-override-no-device`, `api-model-profile-device-outside`, `api-comment-markers`.

## 3. Criteria

### TP1: the rules and their rows

**21 T.1 The rule module.** A pure module of the fe (`fe/src/tensorRules.ts` suggested) mirroring
`crates/app/src/tensor_override.rs`: `KINDS` (id, pattern, block, label key; the 17 entries and their
order exactly as the Rust file, Custom last), `ALIASES` (`token_embd\.weight` read as
`^token_embd\.weight`), `parse(value)` (split on `,`, trim each piece, drop empty ones, split at the
first `=`, trim both sides, unalias the pattern, a piece with no `=` a rule with device `""`),
`render(rules)` (`pattern=device` joined by `,`), `splitPattern(pattern)` (kind index and layers or
none; only the canonical composed form reads back as a kind), `compose(kind, layers)`,
`layersText(layers)` (`10-12, 15`), `parseLayers(text)` (blank is every layer; commas or spaces; a
range out of order, past 999 or more than 256 layers does not read: a distinct "unreadable" result).
Its tests copy the vectors of the Rust tests `parse_and_render_round_trip_a_multi_rule_value`,
`parse_tolerates_whitespace_and_never_invents_a_rule`, `a_rule_splits_at_the_first_equals_like_llama_cpp_does`,
`a_piece_without_a_device_survives_as_a_rule_with_none`, `the_ffn_kinds_are_llama_cpps_own_regexes`,
`every_kind_is_splittable_and_its_block_agrees_with_its_pattern`, `half_typed_or_bad_layers_leave_the_value_alone`,
`a_new_kind_keeps_the_rows_layers`, `only_the_composed_layer_form_reads_back_as_a_kind`,
`the_old_unanchored_embedding_pattern_reads_as_the_kind`, `edits_never_reorder_the_rules`.

**21 T.2 The card.** A Card (`fe/DESIGN.md` 5.2) titled `model-profile-tensor-placement`, id
`tensor`, right after GPU distribution and before Hardware, its intro the two lines of
`model-profile-tensor-placement-intro` (one paragraph per `\n`, as the GPU card's). The jump list gains
its entry in that place. Then one `fieldset` with the visible legend `model-profile-ot-rules`, laid out
as a Field (label column from 720 px); everything after the legend sits in **one** wrapper `div` (the
second grid column at 720 px and wider, so no paragraph takes a grid cell of its own: the LS1 D1
lesson), holding in order: the `--muted` paragraph `model-profile-ot-format` (id kept for 21 T.4), the
empty line or the list (21 T.3), and the Add rule button (21 T.7). In TP2, Rules as text follows the
fieldset as its own Field.

**21 T.3 The list.** The rules of the form's `override-tensor` value (`parse`), in file order, as a
`ul role="list"` of `li`, one per rule. No rule (the key absent, empty, or only commas): instead of the
list, the plain paragraph `model-profile-ot-none`. Each `li` holds one `fieldset` whose visible legend
is `model-profile-ot-rule` with its 1-based position ("Rule 2"), renumbered after a removal.

**21 T.4 A rule's controls.** Inside the rule's fieldset, a wrapping flex row (gap `--space-2`), each
visible label and its control one flex item (playbook: never label on one line, control on the next):
1. **Tensor**: a native `select` labelled `model-profile-ot-tensor`, one option per `KINDS` entry,
   value its id, text its label key; selected by `splitPattern` (an unknown pattern selects Custom).
   Item `flex: 1 1 16rem; min-width: 0`.
2. **Layers**: only for a kind with a block suffix: a text input labelled `model-profile-ot-layers`,
   about 10 characters wide, `aria-describedby` the format paragraph, showing `layersText` of the
   rule's layers (empty for every layer).
3. **Goes on**: a native `select` labelled `model-profile-ot-goes-on` (options 21 T.5). Item
   `flex: 1 1 12rem; min-width: 0`.
4. **Remove**: a secondary button with the `x` icon (`aria-hidden`), visible text
   `model-profile-ot-remove` and a visually hidden " Rule 2" (the legend's text), so its name is
   "Remove Rule 2" ([2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name)).
5. **Pattern**: only when Tensor shows Custom: on its own line (`flex-basis: 100%; min-width: 0`), a
   monospace text input labelled `model-profile-ot-pattern`, full width, `aria-describedby` the format
   paragraph, `spellcheck="false"`, `autocapitalize="off"`.
Labels are plain: the rule's legend names the group ("Rule 2, group" then "Tensor, combo box").

**21 T.5 Goes on options.** In order: `CPU` (text `model-profile-ot-cpu`); once the device read
answered 200, each probed GPU in probe order, value its id, text `model-profile-device-option` (id,
name; the id alone when the name is null); then, for the rule's device when no option's value equals
it **exactly** (decision 4), that device as the last option, text `model-profile-device-option-missing`
when the probe answered with a time, else the device itself; a rule with device `""` shows a first
option value `""`, text `model-profile-ot-no-device`, selected. Pending or failed device read: `CPU`
and the rule's own device.

**21 T.6 Editing a rule.** Every edit rewrites that rule only and keeps the order of the rules:
- Tensor to a kind: the pattern is `compose(kind, layers)`, the layers being those the row's Layers
  text reads (kept while the field is hidden, so a kind without layers and back keeps them); Custom
  (from a kind): the Pattern field shows the row's remembered Custom text when it has one from this
  visit, else the current pattern; it becomes the rule's pattern.
- Layers: at each input, a text `parseLayers` reads sets the pattern to `compose(kind, layers)`; a
  text it cannot read leaves the pattern as it was (TP2 refuses it at Save).
- Pattern: at each input, the text as typed is the rule's pattern (TP2 checks it at Save); the Tensor
  select keeps showing Custom while the row is edited, even when the text equals a family's pattern.
- Goes on: the rule's device is the option's value.
Focus stays on the control used; no control the user is in is replaced (rows keyed by an id the page
gives them, never by index, so focus survives a renumbering).

**21 T.7 Add and Remove.** Add rule is a secondary button with the `plus` icon (`aria-hidden`) and
text `model-profile-ot-add`, always shown on an editable page. It appends `^token_embd\.weight=<the
first probed GPU>`, or `=CPU` when the device read has no GPU or has not answered (the GUI's `add`),
and moves focus to the new rule's Tensor select. Remove drops its rule; focus moves to the next rule's
Tensor select, else the previous rule's, else Add rule.

**21 T.8 Saving.** The key is changed when `render` of the rules on screen differs from `render(parse(
loaded value))`, so an untouched value (aliases, spaces, a hand-written order) is never sent, and an
edit undone is unchanged. Sent: `render(rules)`, or `null` when no rule is left. Discard restores the
loaded rules (and drops the rows' remembered texts). A 200 makes the saved rules the loaded ones. No
other key changes because of this card.

**21 T.9 Keyboard, screen reader, 320 px, nothing else.** Tab order in a rule: Tensor, Layers (when
shown), Goes on, Remove, Pattern (when shown); after the last rule, Add rule. A screen reader on Remove
hears "Remove Rule 2, button". No sideways scroll at 320 px and 200 % text with three rules, a
Custom pattern of 120 characters with no space and an unlisted 64-character device: the items wrap
inside the `li`, the Pattern input scrolls inside itself. Under the 720 px grid the wrapper `div` is in
the second column with the legend in the first. The strings file gains exactly TP1's keys.

### TP2: the text, the checks, the refusals

**21 T.10 Rules as text.** After the Rules fieldset, a Field labelled `model-profile-ot-text`
(`label` plus one monospace text input, `spellcheck="false"`, `autocapitalize="off"`), its hint
`model-profile-ot-text-hint` behind the About toggle. Not focused, it shows the loaded value as written
while the key is unchanged (21 T.8), else `render(rules)`. Typing changes only its text. On Enter or
when it loses focus: the rules become `parse(text)` (rows rebuilt with new ids, remembered texts
dropped) and the field shows `render` of them; a text that parses to the rules already on screen
changes nothing. Escape does nothing special. The rows never change while the field is typed in.

**21 T.11 The page's own checks at Save.** With the other checks of 02h 157, every rule is checked:
a shown Layers text `parseLayers` cannot read gives `model-profile-ot-layers-bad` (the text) on that
input; a Custom pattern that is empty gives `model-profile-ot-pattern-empty`, one holding `,` or `=`
gives `model-profile-ot-pattern-separator`, on the Pattern input; a device `""` gives
`model-profile-ot-device-missing` on Goes on. Each error is a Field Error under the rule's row
(`flex-basis: 100%`), the control `aria-invalid` with the error's id in its `aria-describedby`. Nothing
is sent; focus goes to the first failing control in DOM order; the bar's alert is `save-invalid`.
Editing the control clears its error.

**21 T.12 The manager's refusals.** Replacing 02h 143 for this key, a 422 with `field` (or the
fallback) `override-tensor`:
- `override-no-pattern`, `override-no-device`, `override-tensor-space`, `override-bad-device`:
  `api-<key>` filled with `args` goes on rule `args[0]` (1-based; the rules sent are the rows, so the
  numbers agree): on its Pattern input when shown, else its Tensor select, for the first two and the
  space; on Goes on for `override-bad-device`. A position with no row: on Rules as text.
- `model-profile-device-outside` with `args[2]` `override-tensor`: on Goes on of every rule whose
  device equals `args[3]` case-insensitively (the manager's comparison), focus on the first; none
  matches: on Rules as text.
- Any other key naming `override-tensor` (`comment-markers`): on Rules as text.
Each as 02h 142: `aria-invalid`, the error under the control, focus on it, the bar's alert
`save-refused` with the text. The `fieldOf` fallback (`ModelProfilePage.tsx` 540-543) gains the two
new keys.

**21 T.13 Keyboard, 320 px, nothing else.** Tab order: the last rule's controls, Add rule, the Rules
as text About toggle, its input. A 300-character value in Rules as text scrolls inside the input, no
sideways page scroll at 320 px and 200 %. The strings file gains exactly TP2's keys.

### TP3: what each rule places

**21 T.14 The tensor list in the read.** 19 G.1's good reply also reads `info.tensors`: an array whose
items each have a string `name` and a whole number `bytes` of 0 or more, and `info.tensorsTruncated` a
boolean (absent reads as false). A `tensors` absent or not of that shape is "no tensor list": the
layer facts still count (the read is not failed by it).

**21 T.15 The coverage line.** With a tensor list of one row or more, under each rule's row (after its
errors, `flex-basis: 100%`, `--muted`, 0.875rem, wrapping anywhere), one plain paragraph, the GUI's
`tensor_picker::coverage`: rules in order, a tensor counted for the first rule whose pattern finds it
(`new RegExp(pattern)` with no flags, `test` on the name, as `regex_search`); per rule `n` tensors newly
taken and their `bytes` summed, `s` found but taken earlier. Text: a pattern `RegExp` refuses,
`model-profile-ot-unreadable`; `n` 0 and `s` 0, `model-profile-ot-matches-none`; `n` 0, `s` above 0,
`model-profile-ot-all-taken`; `n` 1, `model-profile-ot-tensors-one`; more,
`model-profile-ot-tensors-many`; when `n` and `s` are both above 0, `model-profile-ot-more-taken` with
`s` appended. Bytes as the GUI's `fmt_bytes` (`1.21 GiB` two decimals from 1 GiB, `312.0 MiB` one from
1 MiB, else whole `KiB` rounded up; 1024-based). An empty pattern has no line. The line follows every
edit; a Custom pattern's line is computed from a deferred copy of the text (`useDeferredValue`,
`fe/DESIGN.md` 5.3) so typing never waits on 20 000 names. The pattern's input lists the line's id last
in its `aria-describedby` (the Tensor select when there is no Pattern input). Not a live region. Tests
copy the vectors of `coverage_counts_first_match_wins` and `an_unanchored_output_rule_shows_what_it_really_takes`.

**21 T.16 No list, a cut list.** Facts pending, failed, a reason, or no tensor list: no coverage line,
nothing else changes. `tensorsTruncated` true: in the wrapper `div`, right after the format paragraph,
the `--muted` paragraph `model-profile-ot-truncated` with the number of rows received.

**21 T.17 Nothing else.** No sideways scroll at 320 px and 200 % with three rules and their lines. The
strings file gains exactly TP3's keys. The GPU card's facts line is unchanged.

## 4. Out of scope

- The tensor picker (Choose…, a tick per tensor writing the regex: `tensor_picker.rs`; question Q1).
- Reorder buttons for rules (decision 7, question Q2).
- Warnings before Save: a rule's GPU outside the server profile's `Devices` (the page does not know
  them, as brief 19 Q1; the refusal of 21 T.12 names it); the embedding pin warning
  (`pins_embeddings_to_gpu`, it needs the embedding's type); Fit to VRAM with rules (2k's warnings).
- `moe-cache-mib` (architecture decision 4); the changed mark (U17e, slice 17); judging the regex.

## 5. Accessibility (WCAG 2.2 AA)

- Every control has a visible label; each rule is a group named by its visible legend "Rule N"; the
  Remove button's name starts with its visible text ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
  [2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name), [4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)).
- The Layers and Pattern formats are visible text, linked as descriptions, never a placeholder
  ([3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions)).
- Errors name the problem and the fix on the control, the page's checks and the manager's refusals
  alike ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification),
  [3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion)).
- A Tensor select's arrow key changes only the fields after it; focus never drops after Add, Remove or
  applying Rules as text ([3.2.2](https://www.w3.org/TR/WCAG22/#on-input),
  [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)).
- Buttons at least 40 px high (`--control-h`), inputs and selects at the shared height
  ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)); reflow at 320 px and 200 % text
  ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)).
- The empty line, the format line, the coverage lines and the cut line are plain paragraphs, not live
  regions ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages); brief 14's ruling of 2026-11-21).

## 6. Older tests that change

- `ModelProfileGpu4.test.tsx` 1611-1618 (GD2.10: no `model-profile-override-tensor-control`): the card
  is now there; the test keeps "the GPU card holds no Tensor placement field" by looking inside the GPU
  card only (TP1).
- `ModelProfileSave3.test.tsx` 440-470 (2h.143's table): its `override-no-pattern`,
  `override-no-device` and `comment-markers` on `override-tensor` rows move to 21 T.12's tests (TP2).
- Tests listing the cards or the jump list in order (`HearthJump`, `HearthJumpToggle`, `HearthH9a`,
  `HearthH9b`, `HearthH10`, `ModelProfileCards1`, `ModelProfilePage`, `ModelProfileContent`:
  `grep -l "jump\|model-profile-hardware-config" fe/src/*.test.*`): one more entry after GPU
  distribution (TP1).
- The bff fake (`fe/src/test/bff.ts`): its default model-info reply is a reason (19 section 6), so no
  coverage lines by default; TP3's tests answer with `tensors`.
- `HearthTokens.test.ts` H1a.12: no new `min-height` or `min-width` beyond `min-width: 0` on the row
  items, named by the class the coder gives them (the review checks).

## 7. `fe/ux` states

`fe/ux/stub.py`: `/__set?ot=1` serves the model profile with `override-tensor`
`blk\.(20|21|22|23)\.ffn_(up|down|gate|gate_up)_(ch|)exps=CPU,^token_embd\.weight=ROCm0,blk\.1[0-9]\..*_exps=CPU`
for a new state `model-profile-placement` (expect: the legend `Rule 3`), 1280 and 320, both themes,
200 % text (TP1). TP3 adds to the stub's model-info reply a `tensors` list of a small MoE (the
`blk.N.ffn_*_exps.weight` of 32 blocks plus `token_embd.weight` and `output.weight`) and
`tensorsTruncated` false, so the same state shows coverage lines.

## 8. Questions for the user (notes: nothing waits on them)

- **Q1.** Should the web get the GUI's tensor picker (tick the model's tensors, the page writes the
  regex)? Recommended: not now; the families with Layers cover experts to CPU and the embedding to a
  GPU, and TP3's counts show what a rule takes. Built as TP4 if asked.
- **Q2.** Should rules have Move up / Move down (the first match wins)? Recommended: not now; Rules as
  text reorders them, and the GUI has no reorder either.
