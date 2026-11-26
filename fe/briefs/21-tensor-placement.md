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
  text it cannot read leaves the pattern as the last readable text composed it in this visit (the
  GUI's "last good value"), not the loaded one: loaded `blk\.(0|1|2)...`, typed `10-12, 15` then
  `10-`, the rule holds `blk\.(10|11|12|15)...` and, in TP1, Save sends it (ruling of 2026-11-22,
  reading (a)). TP2's 21 T.11 then refuses Save while the shown Layers text is unreadable, so with
  TP2 built nothing is sent in that case.
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

## TP1 review (designer, 2026-11-23)

I read `tensorRules.ts`, `TensorPlacement.tsx`, the TP1 diff of `ModelProfilePage.tsx`,
`ModelProfileCards.tsx`, `en_US.json` and `fe/ux`, and the test names. I drove the page in Chromium,
Firefox and WebKit with the `fe/ux` stub (`ot=1`), at 1280, 760 and 320 px, light and dark, 100 % and
200 % text (`build/ux-review/tp1.js layout|act`, `tp1shot.js`; shots in `build/ux-review/tp1/`).
**Verdict: not accepted. D1 must be fixed; D2 and D3 are small.**

- 21 T.1 met: the module and its copied vectors (`tensorRules.test.ts`).
- 21 T.2 met except D1: the card sits between GPU distribution and Hardware config, its jump entry
  in place, the two intro lines, the Rules legend and one wrapper `div` in the right order.
- 21 T.3 met except D2: one `li` per rule in file order, legends "Rule N", renumbered after a removal;
  an absent key shows `model-profile-ot-none` and no Save.
- 21 T.4 met in names and roles, not in layout (D1). The group is "Rule 1"; the controls are
  combobox "Tensor", textbox "Layers", combobox "Goes on", button "Remove Rule 1" (an `aria-label`
  where the brief asked for a hidden span: the same name, accepted). Pattern is shown only for Custom.
- 21 T.5 met except D3: CPU first, then the probed GPUs as "id: name", an unlisted value last.
- 21 T.6 met: Custom from the embedding kind shows `^token_embd\.weight`, and the text typed there
  comes back after a round trip through Attention; Layers `10-12, 15` then `10-` keeps blocks 10, 11,
  12 and 15 (the ruling); focus stays on the control.
- 21 T.7 met: Add focuses the new Tensor select and puts the rule on the first GPU; Remove moves focus
  to the next rule, and after the last rule to Add rule.
- 21 T.8 met: an untouched value sends nothing; the PUT carries only `override-tensor` as rendered.
- 21 T.9 met for the Tab order (Tensor, Layers, Goes on, Remove, Pattern, then Add rule) in all three
  engines, and no sideways scroll anywhere. Not met for layout (D1). My route for five rules, a
  120-character pattern and a 64-character device did not apply, so the long values are left to the
  re-check.
- `npm --prefix fe/ux run check` (144 states per browser): only the 8 known `instance-server`
  target-size findings. `model-profile-placement` is clean in all three engines, which axe can pass
  even with D1.

**D1 (must fix): the rows are unusable at 720 px and wider and badly wrapped at 320 px.**
`index.css` gained no rule for the card.
- At 720 px and wider, the outer wrapper `div` (`TensorPlacement.tsx` 237) falls into the 200 px label
  column. Each rule's `fieldset className="field"` (302) becomes a grid of `200px 0px`, so
  `.field-control` is 0 px wide. The selects are then 18 px wide and "Remove" stands one letter per line
  (`build/ux-review/tp1/card-1280.png`).
- At 320 px each label and its control are separate flex items (305-365): the "Pattern" label ends
  up beside Remove, and its input lands on the next line (`card-320.png`).
- Fix:
  - Place `#…ot fieldset > div` in `grid-column: 2` in the 720 px rule, as LS1's D1 did.
  - Give each rule a plain `fieldset` with its legend above, not `.field`.
  - Wrap each label and its control in one span item: Tensor `flex: 1 1 16rem; min-width: 0`, Goes on
    `flex: 1 1 12rem; min-width: 0`, Pattern `flex-basis: 100%; min-width: 0` with its input full width
    (21 T.4).
- Pin it with a test that reads the CSS, as `Reflow.test.ts` does.

**D2 (small).** After the last rule is removed, an empty `ul role="list"` stays beside the
`model-profile-ot-none` paragraph (`TensorPlacement.tsx` 244, `|| dirty`). 21 T.3 says the paragraph
comes instead of the list, and an empty list is heard as "list, 0 items". Drop `|| dirty`: focus goes
to Add rule either way.

**D3 (small).** A rule with device `""` gets its `""` option only once the probe has answered
(`TensorPlacement.tsx` 183-185). While the probe is pending, the select's value matches no option,
so it shows "CPU: system memory" while the rule has no device. 21 T.5: the `""` option comes first
whatever the read.

Comment notes, no defect: `TensorPlacement.tsx` 14 says the manager refuses an unreadable Layers text
at Save, but it is the page's own check (21 T.11). Lines 15-16 say the Pattern text becomes the rule on
blur or Enter, but the code takes it at each input, as 21 T.6 says.

**Re-check (2026-11-23): D2 and D3 are fixed; D1 is fixed in placement but its layout fix brings
D4. Not accepted.** I rebuilt `fe/dist` and drove the page in all three engines at 1280, 760 and
320 px, light and dark, 100 % and 200 % (`build/ux-review/tp1b.js`, log `tp1/run2.txt`). The route now
applies: five rules, a 120-character pattern, a 64-character unlisted device and a wrong-case `rocm0`.
- D1 placement met: from 720 px the Rules legend is in column 1 and the wrapper in column 2 (505 to
  935 px at 1280); each label and its control is one item; Pattern takes its own line, full width.
- D2 met: removing the last rule leaves no `ul`, the empty paragraph shows, focus is on Add rule.
- D3 met: the code puts the `""` option first whatever the probe; the tests pin it.
- Tab order, focus after Add and Remove, the ruling, the PUT and the names are as in the first pass.

**D4 (must fix): the selects keep the width of their longest option, so the page scrolls sideways.**
`npm --prefix fe/ux run check` fails `model-profile-placement` at 320 px in all three engines
(scrollWidth 430 of 320 in Chromium, 427 in Firefox, 436 in WebKit; 778 to 979 at 200 %). Only the 8
known `instance-server` findings are otherwise left. With my five rules the 64-character device
select spills at 1280 px as well (59 px in Chromium), and at 760 px three Goes on selects spill.
- The cause: the select now sits inside the `span.ot-tensor` or `span.ot-goes-on` item
  (`index.css` 1806-1823), not directly in the flex row. The item has `min-width: 0`, but the select
  inside it still asks for its own content width; the shared `select` rule sets no width.
- Fix: make `.ot-tensor` and `.ot-goes-on` flex rows themselves (`display: flex; flex-wrap: wrap;
  align-items: center; gap: var(--space-2)`), give their select `flex: 1 1 12rem; min-width: 0`, and
  give `.ot-layers input.number` `max-width: 100%`. At 200 % text on a 1280 px window the second
  column is 190 px, so the Layers input spills too. Pin it with the CSS test, and with `fe/ux`'s state
  at 320 px.

**Second re-check (2026-11-23): D4 is fixed. One small defect is left, D5.** I rebuilt `fe/dist`,
reran `npm --prefix fe/ux run check` and my long-values route (`build/ux-review/tp1b.js layout`, log
`tp1/run3.txt`) in all three engines at 1280, 760 and 320 px, light and dark, 100 % and 200 %.
- D4 met: no select leaves its rule anywhere, the 64-character device included, and there is no
  sideways scroll in any run.
- The UX check reports only the 8 known `instance-server` target-size findings, and
  `model-profile-placement` passes in all three engines.

**D5 (small, the last point of 21 T.9).** At 1280 px with 200 % text, both Layers inputs reach x 725
while the rule's column ends at 695, 30 px past it, in all three engines. The page does not scroll.
The cause: `.ot-layers` (`index.css` 1850) is a flex item with no `min-width: 0`, so its
min-content is the input's `10ch`, and the input's `max-width: 100%` is measured against that same
span. Fix: give `.ot-layers` the same flex row as `.ot-tensor` (`display: flex; flex-wrap: wrap;
align-items: center; gap: var(--space-2); min-width: 0`) and its input `min-width: 0`. With D5 fixed,
TP1 is accepted; every other point of 21 T.1 to T.9 is met.

## TP2 review (designer, 2026-11-23)

I read the TP2 diff (`TensorPlacement.tsx`, `ModelProfilePage.tsx`, `Field.tsx`, `index.css`,
`en_US.json`) and the test names of `TensorPlacement2.test.tsx`. I drove the page in Chromium,
Firefox and WebKit at 1280 and 320 px, light and dark, 100 % and 200 % text
(`build/ux-review/tp2.js`, log `tp1/run4.txt`, shots `tp1/tp2-*.png`).
`npm --prefix fe/ux run check` passes: 168 states per browser, 0 failing.
**Verdict: not accepted. D1 must be fixed; D2 and D3 are small.**

**The ruling asked for: the brief wins.** The ruling of 2026-11-22 in 21 T.6 already says that once
TP2 is built, Save is refused while the Layers text on screen does not read, whatever was typed
before it. `otLayersBad`'s narrowing (`TensorPlacement.tsx` 148, the `layersGood` field) contradicts
that ruling; that is D1.

- 21 T.10 met. The field shows the file's value while the key is unchanged. Typing leaves the rows as
  they are (3 rows while typing). Enter applies the text: two rows, the text re-rendered without the
  space, focus kept on the field. Its names are textbox "Rules as text" and button "About Rules as
  text".
- 21 T.11 met except D1. With an unreadable Layers text, an empty Pattern, an `a{1,2}` Pattern and a
  rule on no device, each control is marked with its text, focus goes to the first one, the alert reads
  `save-invalid` and nothing is sent.
- 21 T.12 met:
  - `override-tensor-space` at position 2 lands on rule 2's Tensor (its kind is not Custom), and
    `override-bad-device` on Goes on.
  - Position 9, with no such row, lands on Rules as text, and so does `comment-markers`.
  - `model-profile-device-outside` with `ROCM0` marks only the Goes on of the rule on `ROCm0` (case
    ignored).
  - Each time focus goes to the marked control and the alert reads `save-refused` with the text.
- 21 T.13 met. Tab goes from the last rule to Add rule, the About toggle, then the input. A 309- to
  365-character value scrolls inside the input (505 to 935 px at 1280, 41 to 279 at 320), with no
  sideways scroll in any engine, width, theme or text size.

**D1 (must fix): Save is not refused while a Layers text does not read** (`TensorPlacement.tsx` 148,
`layersGood` at 65 and 138). Loaded `blk\.(0|1|2)…`, typed `10-12, 15` then `10-`: the PUT goes out in
all three engines. Fix:
- make `otLayersBad(row)` simply `'unreadable' in parseLayers(row.layersText)` for a shown Layers
  field, and drop `layersGood`;
- change `TensorPlacement1.test.tsx` 1140-1164 so it no longer saves. It reads the Rules as text
  input, which shows `render` of the rows (`blk\.(10|11|12|15)…=CPU` while Layers shows `10-`), and
  that still proves the pattern kept its last readable form (21 T.6). It then expects Save to mark
  `model-profile-ot-layers-bad` and send nothing (21 T.11).

**D2 (small): Add rule changed its device rule without a brief or a test** (`TensorPlacement.tsx`
254-268, `otNewDevice`). While the probe is pending it copies the last rule's device. 21 T.7 says CPU
while the read has not answered (the GUI's `add`), and copying can bring along a device the bound
refuses. Put back `(probe?.devices ?? [])[0]?.id ?? 'CPU'`.

**D3 (small): an edit clears the errors of the row's other controls** (`ModelProfilePage.tsx` otEdit,
the loop over `tensor`, `layers`, `goes-on`, `pattern`). 21 T.11 and 02h 142 say editing a control
clears its own error. Example: a device-outside mark on Goes on vanishes when the user changes the
Tensor kind. Clear only the edited control's key.

**Re-check (2026-11-23): D1 to D3 are fixed. TP2 is accepted.** I rebuilt `fe/dist` and ran
`build/ux-review/tp2b.js` in Chromium, Firefox and WebKit.
- D1: loaded `blk\.(0|1|2)…`, typed `10-12, 15` then `10-`. Rules as text shows
  `blk\.(10|11|12|15)…=CPU`, Save marks Layers, focus goes to it, the alert reads `save-invalid`, and
  nothing is sent.
- D2: with the device read held, Add rule puts the new rule on CPU.
- D3: after a device-outside refusal on rule 2's Goes on, changing rule 2's Tensor keeps the mark,
  and picking a device clears it.

No `fe/ux` state changed, so I did not rerun the UX check (it passed with 0 failing at the TP2 review).

## TP1b: the rule row's layout and its icon buttons (user feedback, 2026-11-23)

**What the user saw, on the deployed TP2 page at desktop width (and why):**
- "Tensor" sits beside its select while "Goes on" sits above its own, so the two selects stand at
  different heights. Each item is a wrapping flex row (the D4 fix), so a label that fits beside its
  control stays there and one that does not wraps above it.
- Remove is a 40 px pill standing on a third line.
- The selects cut their text ("Embedding table (token_", "ROCm0: AMD Radeon A").
- "Rules" is drawn as plain text while "Rules as text" is a bold field label.

User's words: "the rows are all misaligned; a pill button labelled Remove is not needed when it sits
on the same row: an x is enough (for every list); Add too: a + in the row below is enough (ask the
expert)". `fe/DESIGN.md` 5.2 "Rule list" and "Icon buttons of a list" now hold the rules.

**Decisions.**
1. **Labels above controls, in every item of a rule's row**, and the row aligned at the bottom, so
   all the controls of a line share one top edge. This agrees with the user. The label column stays:
   it is the form's layout at 720 px and wider, and "Rules" becomes a field label like "Rules as text"
   (`legend.field-label`), which is what made the indent look wrong.
2. **Remove is an `x` icon button at the end of the row.** Agreed. It is 40 px square, so it lines up
   with the selects, borderless, named "Remove Rule N".
3. **Add is a `plus` icon button under the list.** Agreed, as the chips' + already is. It keeps an
   outline, so after the empty-list line it still reads as a control. It is named "Add rule". The one
   difference from the user is a preference of mine: it is a 40 px outlined square, not a bare glyph,
   to keep a clear target beside text.
4. **The selects get more of the line**: Tensor `flex: 2 1 18rem`, Goes on `flex: 2 1 16rem`, Layers
   a fixed `8rem`. With the 40 px x, at 1280 px both selects show their whole text for the GUI's
   kinds and the stub's GPU names. A closed select still cuts a longer text at 320 px; its open list
   shows the whole text.
5. **Lists touched**: only `fe/src/TensorPlacement.tsx` and its CSS. The instance tags' chips
   (`InstanceTagChips`) already follow the pattern (24 px x, 32 px +). The profile pages' Delete
   stays a text button with Confirm in place.

**Strings**: `model-profile-ot-remove` changes from `Remove` to `Remove {0}`, filled with the rule's
legend, in the visually hidden span. No other text changes.

**Criteria (one tester and coder run, about 6 criteria).**
- **21 T.18** "Rules" is a `legend.field-label`, drawn as the "Rules as text" label (a CSS-reading
  test: the same declarations apply to both).
- **21 T.19** In a rule's row each item (Tensor, Layers, Goes on, Pattern) holds its label then its
  control as block boxes, label above (`display: flex; flex-direction: column`). The row has
  `align-items: flex-end`, so in a browser the tops of Tensor, Layers and Goes on on one line are
  equal (fe/ux and the review measure it; jsdom pins the CSS).
- **21 T.20** Remove: a button with only the `x` icon (`aria-hidden`) and a `visually-hidden` span
  `model-profile-ot-remove` filled with "Rule N", so its name is "Remove Rule N"; no visible text;
  `width` and `min-height` 40 px, no border, no fill; the last item of the row. Focus after a
  removal is unchanged (21 T.7).
- **21 T.21** Add: a button with only the `plus` icon (`aria-hidden`) and a `visually-hidden` span
  `model-profile-ot-add`, so its name is "Add rule"; 40 by 40 px, `--control` outline; after the
  list (or the empty line), before the end of the wrapper `div`; its id is unchanged. Focus after Add
  is unchanged (21 T.7).
- **21 T.22** Sizes: Tensor `flex: 2 1 18rem`, Goes on `flex: 2 1 16rem`, Layers `flex: 0 0 8rem`, all
  `min-width: 0`, their controls `width: 100%`. Remove is `align-self: flex-end`. `HearthTokens.test.ts`
  H1a.12's list gains the two 40 px buttons under their classes (the coder names them).
- **21 T.23** No sideways scroll and no item past its `li` at 1280, 760 and 320 px, 100 % and 200 %
  text, light and dark, with the five long rules of the TP1 review. At 320 px each item takes its own
  line. Under forced colours both icons stay visible (`ButtonText`).

**Older tests that change**: the TP1 tests that find Remove by its name keep "Remove Rule N", and
those that read its visible text "Remove" change; the Add button's name is unchanged.
`fe/ux` `model-profile-placement` stays the state, and the review measures it.

## TP1b review (designer, 2026-11-23)

I rebuilt `fe/dist` and drove the card in Chromium, Firefox and WebKit at 1600, 1280, 760 and 320 px,
light and dark, 100 % and 200 % text, plus forced colours in Chromium
(`build/ux-review/tp1c.js`, `tp1d.js`, log `tp1/run5.txt`, shots `tp1/tp1b-*.png`). I measured each
control's top, size and whether its text is cut. `npm --prefix fe/ux run check`: 168 states per
browser, 0 failing; it measures neither control heights nor text cuts.
**Verdict: not accepted. D1 must be fixed; D2 and D3 are small.**

- 21 T.18 met: "Rules" is weight 600 at 17 px, as the "Rules as text" label (34 px at 200 %).
- 21 T.20 and T.21 met:
  - The x is 40 by 40 px, with no border or fill, named "Remove Rule N", with no visible text, on the
    line of the Goes on select.
  - The + is 40 by 40 px, outlined, named "Add rule", under the list.
  - Under forced colours both draw in the system text colour.
- 21 T.19 met where a line has only selects: at 1600 px, rule 2's Tensor, Goes on and x share one
  top. Not met with a Layers or Pattern field (D1).
- 21 T.22 met except D2.
- 21 T.23: no page scroll anywhere, but D1 and D2 break the rows.
- The selects' text: "Embedding table (token_embd)" is whole wherever Tensor has a line or a share of
  one. "ROCm0: AMD Radeon AI PRO R9700 / RDNA4" is cut in a shared line (263 px at 1600, 382 at 1280,
  where the rail leaves the control column 430 px). That cut is not avoidable at those widths without
  dropping the label column; the open list shows the whole name.

**D1 (must fix): the Layers and Pattern inputs are far too tall.** The shared
`input[type='text'] { flex: 1 1 16rem }` (`index.css` 1572-1574) also applies inside the new column
items (`.ot-layers` 1865, `.ot-pattern` 1896), where a flex basis is a height:
- The Layers input is 110 px tall.
- The Pattern input is 256 px tall (512 at 200 %).
- With the row aligned at the bottom, rule 1's Tensor select stands 70 px below the Layers label:
  the misalignment the user reported, now on another field
  (`build/ux-review/tp1/tp1b-chromium-1600-light.png`).

Fix: `.ot-rule-row input, .ot-rule-row select { flex: none; }`, the width coming from `width: 100%`.
Pin it with the CSS test, and with a browser measure that every control of a line is 40 px tall (42
in WebKit) and shares one top.

**D2 (small).** `.ot-layers { flex: 0 0 8rem }` (`index.css` 1869) cannot shrink. At 200 % text it is
256 px wide in a 190 px column (1280 px window) or a 238 px one (320), spilling its rule by 18 to 66 px
in all three engines; the page does not scroll. Fix: `flex: 0 1 8rem; min-width: 0`.

**D3 (small, preference).** The rules sit with no space between them, so "Rule 2" reads as a line of
rule 1 just under its x. Give `#model-profile-ot-rules li + li` a `margin-top: var(--space-4)`.

Screenshot for the user: `build/ux-review/tp1/tp1b-chromium-1600-light.png` shows the state now,
D1 included. `tp1b-chromium-1280-dark.png` is the user's case (one embedding rule, dark).

**Re-check (2026-11-23): D1 to D3 fixed.** Measured in Chromium, Firefox and WebKit with
`build/ux-review/tp1e.js` and `tp1f.js`, on one rule, three rules (a block kind with Layers, a table,
a Custom pattern) and the five-rule set with the 128-character pattern and the 64-character device,
at 1280 and 320 px, in light and dark, at 100 % and 200 % text:
- In Chromium and Firefox every control of a rule is 40 px tall. At 1280 px the card's column fits
  Tensor and Layers on one line and Goes on and the x on the next (Pattern on a third for Custom),
  and the controls of each line share one top.
- 20 px between rules; nothing reaches past its rule; no sideways scroll anywhere.
- At 200 % text each control has a line of its own, so their heights (54 to 64 px) align with
  nothing beside them.
- **D4 (small, every page, WebKit only).** WebKit draws a native `select` 42 px tall (`line-height:
  normal` with 4 px padding beats `min-height: var(--control-h)`), so on a rule line the selects
  stand 2 px above the 40 px Layers input and x (bottoms aligned). Fix in the global `select` rule
  (`index.css` ~1561): `height: var(--control-h)` beside the min-height, or `line-height` set as the
  inputs'. It is not TP1b's change and does not block it.

`npm --prefix fe/ux run check`: 168 states per browser, 0 failing (`build/ux-review/tp1/check-tp1b2.txt`).

TP1b accepted. Screenshots for the user: `build/ux-review/tp1/tp1b2-chromium-1280-dark-one.png`
(one embedding rule) and `tp1b2-chromium-1280-dark-several.png` (a MoE experts rule with layers
20-23, the embedding table, a Custom pattern).

## TP1c: room above the + (user feedback, 2026-11-23)

The user, on the deployed build: "il bottone + è attaccato alla riga sopra" (the + touches the row
above it). The cause: `ul.check-list` has `margin: 0` (`index.css` ~1743), and `.ot-add` (~1951) has no
margin of its own, so the + sits on the last rule's bottom edge.

- **21 T.24** The Add rule `+` (`.ot-add`) has `margin-top: var(--space-3)` (12 px), so its top edge
  is 12 px below the last rule's `li`, and below the `model-profile-ot-none` line when there is no
  rule. That is less than the `--space-4` between two rules, so the + reads as the list's own control
  and not as another rule **(preference)**. Nothing else moves. The tester pins the sheet rule as the
  TP1b sheet tests do; the designer measures it in the three browsers at the review.

## TP1c review (designer, 2026-11-24)

I measured it in Chromium, Firefox and WebKit at 1280 and 320 px, light and dark, at 200 % text and
under forced colours (`build/ux-review/17b2.js`, shots in `build/ux-review/17b2/`).
- **21 T.24 met after rules:** the + is 12 px below the last rule's `li` everywhere.
- **D1, not met after the empty line:** with no rule, the + is 24 px below
  `model-profile-ot-none`, not 12. That paragraph keeps the global `p` bottom margin of 12 px
  (`index.css` 314 to 315), and the + is an `inline-flex` box, whose margin never collapses with it
  (`index.css` ~2001 to 2007). Fix: `p + .ot-add { margin-top: 0 }`, so the paragraph's 12 px is the
  gap. The same fix covers 17b2's empty pool.
- The UX check is green (180 states per browser, 0 failing). **Verdict: not accepted until D1 is fixed;** a re-measure of the empty list is enough.
- At 200 % text the + is 40 by 45 px: it grows with the text and is still above 24 px. It has no
  sideways scroll, and under forced colours its icon and outline are `ButtonText`.

**Re-check (2026-11-24): D1 fixed, TP1c accepted.** With no rule, the + is 12 px under the
`model-profile-ot-none` line in Chromium, Firefox and WebKit, at 1280 and 320 px and at 200 % text,
with no sideways scroll (`build/ux-review/tp1g.js`). The fix also covers 17b2's empty pool.

## TP3 check (2026-11-26)

Checked 21 T.14 to 21 T.17 against main at 8f239cf: the rule row since TP1b and TP1c
(`TensorPlacement.tsx` 403-479: a wrapping `.ot-rule-row`, labels above controls, the borderless x,
the Field Errors last with `flex-basis: 100%`, `index.css` 2179), the model-info read
(`api.ts` 67-139, read once on the page's arrival, `ModelProfilePage.tsx` 331-334), the Save bar and
22D4 (no contact: the coverage line is not a check and blocks no Save), and the bff's
`info.tensors` / `info.tensorsTruncated` (`crates/bff/README.md` 1040-1054: `{name, bytes}` rows,
cut by the manager at 20 000 rows or 917 504 bytes of JSON, relayed as they come). Where this
section differs from TP3, it wins.

Still true: 21 T.14's shape rule and 21 T.16 (the format paragraph is still `#model-profile-ot-format`
inside the Rules fieldset, `TensorPlacement.tsx` 311).

**21 T.14a Where the list is read.** `ModelFacts` (`api.ts` 71) gains `tensors: {name, bytes}[] |
null` and `tensorsTruncated: boolean`, read in `modelFacts`; a `tensors` not of the shape is
`null` and the layer facts still stand. The Tensor placement card receives the facts the page
already reads for the GPU card; no second request.

**21 T.15a The line in today's row.** 21 T.15 stands, with:
- The line is the last child of `.ot-rule-row`, after the Field Errors, as they are:
  `flex-basis: 100%`, `--muted`, `font-size: var(--fs-sm)` (the token for 0.875rem, `index.css` 56),
  `overflow-wrap: anywhere`.
- The pattern counted is the rule as written into `override-tensor`: the kind's composed pattern
  (`tensorRules.ts`) for a named kind, the Pattern text for Custom; an edit in "Rules as text" redraws
  the rows and their lines.
- The GUI's texts are now under the keys `regex-unreadable`, `matches-no-tensor`,
  `all-taken-by-earlier`, `tensors-one`, `tensors-many`, `more-taken-by-earlier`
  (`crates/core/strings/en_US.toml` 843-848); the line numbers in section 2 are stale, the web
  texts stand as written. `model-profile-ot-unreadable` differs from the GUI's on purpose
  ("pattern", "counted"): the web shows counts, not a tensor list.
- The test vectors are `crates/app/src/tensor_picker.rs` 661-683.

**21 T.17a The UX state (section 7 amended).** No new state: `model-profile-placement`
(`ot=1`) shows the lines once the stub's `MODEL_INFO` carries `tensors`. The list matches the facts
the stub already serves: **64** blocks (not 32), each with `blk.N.ffn_{up,down,gate}_exps.weight`
and `blk.N.attn_q.weight`, plus `token_embd.weight` and `output.weight`, and `tensorsTruncated`
false. On the state's three rules that gives a counted line on rule 1 (blocks 20 to 23), a line on
rule 2 (`token_embd.weight`, 1 tensor) and a counted line on rule 3 (blocks 10 to 19), so the one
and many texts render at 320 px and 200 %. The other texts and the cut list are pinned by unit tests.

Choices taken (the user asleep; recommended options):
1. No new `fe/ux` state; the existing placement state carries the lines.
2. No coverage under the "Rules as text" field: the rows already show it, and the text field is
   for pasting.
3. The facts are not read again after an Assets change of the model; the line then counts the
   tensors of the model read on arrival until the page is opened again. Out of scope, as the GPU
   card's facts line already behaves.

Tests to amend or add: `api.ts`'s parse tests (the two fields), a new `TensorPlacementCoverage*.test.tsx`
for 21 T.15a and 21 T.16, `fe/ux/stub.py` `MODEL_INFO` (the tester; the designer runs the state).

## TP3 review (designer, 2026-11-26)

Built code read (`api.ts`, `tensorRules.ts` `coverage` and `fmtBytes`, `TensorPlacement.tsx`
320-364 and 436-545, `index.css` `.ot-rule-row .ot-coverage`) and measured with
`build/ux-review/tp3.js` in Chromium, Firefox and WebKit at 1280, 320 and 320 px at 200 % text, on
`ot=1` with the stub's 64-block list, and with the list cut to 50 rows by a routed reply.

- 21 T.14a met: the facts carry the list; no second request.
- 21 T.15a met: under each rule, after its controls and inside its row, `--muted`, 14 px
  (`--fs-sm`): "12 tensors, 12.0 MiB" (rule 1, blocks 20 to 23), "1 tensor, 1.21 GiB" (rule 2),
  "30 tensors, 30.0 MiB" (rule 3); each control's `aria-describedby` names the line last. Layers
  of rule 1 typed as `10-23` redraws all three: "42 tensors", rule 3 "every tensor it matches is
  taken by an earlier rule" (first match wins). At 200 % text two lines wrap to 2 lines, none cut.
- 21 T.16 met: with `tensorsTruncated` the paragraph "The model has more tensors than the page
  received: the counts cover the first 50." follows the format paragraph, and the counts cover
  those rows.
- 21 T.17 met: no sideways scroll in any case.

Adviser notes (not defects): at 320 px the rule's x sits alone on its own line before the coverage
line (TP1b's wrap, unchanged); and the line reads as a fact about the rule, not about the device,
which is right. Nothing to change in this slice.

Screenshots: `build/ux-review/shots/tp3-1280-light.png`, `tp3-320-light.png`.

UX check (222 states per browser, one at a time): 0 failing in Chromium and Firefox; WebKit 1 load timeout on `overview-tiles` (its first state), 0 failing on its rerun.
