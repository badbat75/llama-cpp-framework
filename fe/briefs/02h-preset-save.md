# Brief 02h: editing and saving a preset

Written by the designer on 2026-11-16 for `PLAN.md` step 10, web UI slice 2h (the slice table of
[02-models.md](02-models.md) section 0). Applies `fe/DESIGN.md` ("D5.2" points to its sections),
builds on 2e ([02e-preset-cards.md](02e-preset-cards.md), criteria 83 to 129a, its field tables)
and reuses the Server page's save ([01-server.md](01-server.md) section 3, S1, S3; "01's 30a"
names its criteria). Criteria go on from 130. The bff route is accepted, not served yet, in
[api-contract.md](api-contract.md), `PUT /api/presets/{id}`; the read it builds on,
`GET /api/presets/{id}`, is served (36a892d).

References: the configurator's save `presets::save` and `render_section`
(`crates/app/src/presets.rs` 1411-1428, 1754-2167), the editor's enable rules
(`crates/config/ui/models_page.slint`, "MP", the `enabled:` lines; `crates/config/ui/state.slint`
128-130, 346-356), its form conversions (`crates/config/src/form.rs` 146-314, 316-560), the checks
(`crates/core/src/checks.rs` 105-576), texts `crates/core/strings/en_US.toml` ("toml").

**Split in ten sub-slices**, each one tester run and one coder run. 2h.1 settles every shared rule
of the save; 2h.2 and 2h.3 the answers; 2h.4 to 2h.9 enable cards and add their rules; 2h.10
finishes. A tester working on 2h.N reads section 0, section 1 (2h.1) and section 2h.N, plus the
2e table of the cards it names.

| Sub-slice | Criteria | Holds | New strings |
|---|---|---|---|
| **2h.1** | 130-141 | Smallest end-to-end save: Embeddings editable; the changed-field rule; what is sent; save bar, Discard, PUT, 200, generic failure; leave guard | 1 |
| 2h.2 | 142-150 | The PUT's keyed answers: 422 on a field, 409 and Reload, 404, 403, 503, 504 and time-out | 12 |
| 2h.3 | 151-154 | Session ended (401) and revoked (403 `session-not-administrator`) during a save | 0 |
| 2h.4 | 155-163 | Number fields (default box, integer checks, units, input mode) and Resource / context | 1 |
| 2h.5 | 164-169 | KV cache, Sampling overrides (decimal check), Advanced (JSON check) | 2 |
| 2h.6 | 170-174 | Chat / reasoning | 0 |
| 2h.7 | 175-182 | Vision, and Hardware but the Fit trio | 3 |
| 2h.8 | 183-188 | Fit to VRAM, Fit min context, Fit free VRAM | 4 |
| 2h.9 | 189-193 | Speculative decoding: the draft group and the n-gram groups | 2 |
| 2h.10 | 194-198 | The read-only Notice goes, the Assets card says why it is read-only, the section jump list | 2 |

## 0. The main decisions

**D-a. A save sends only what the user changed, and the bff edits those lines in place.** The
configurator re-renders the whole section from its `Preset` struct (`render_section`): it drops the
section's comments and every key it does not know, rewrites spellings (`0` becomes `false`, an
invalid `fit` becomes `off`) and puts its own header and key order. The web does not, on purpose:
since 2e.3b the page shows the file's spelling ("Written as 0"), so a save that rewrote an
untouched control would change a value the user never touched; and a key llama-server accepts but
the configurator does not model (any llama-server option) would be lost. So the body carries the
changed keys only, a string to set, `null` to remove; every other line of the file stays byte for
byte (an edited line is rewritten whole, so an inline comment on that line is lost). Differs from
the Server PUT (whole document, re-rendered as the GUI does): said in the contract. **Decided by
the user (2026-11-16, U1).** The bff accepts only the 65 keys 2h.1 to 2h.9 enable, in exact case,
and refuses an empty string (400): clearing a key is always `null` (table 133 never sends `""`).
The ten keys it refuses (400) are never sent by any 2h sub-slice: `model`, `mmproj`, `model-draft`,
`spec-type` are the Assets card's text (2i), `n-cpu-moe` and `moe-cache-mib` stay disabled with a
reason line (178), and `device`, `split-mode`, `tensor-split`, `override-tensor` are on no 2e card
(GPU distribution 2f, Tensor placement 2j), so 2h shows no control for them (checked 2026-11-17).

**D-e. A value is the read's, cut as llama-server cuts it.** The read gives each value after the
INI parser (trimmed, cut at the first `;` or `#`, the last of a repeated key), which is what
llama-server reads (api-contract.md, `GET /api/presets/{id}`). No 2h rule needs the untruncated
text: the written-as line (134) describes the spelling llama-server gets; the invalid-`fit` state
(185) judges the value llama-server reads (`fit = on # x` is `on`, valid); the full-value line (134)
shows the whole value llama-server uses (a comment is not part of it); the changed rule (D-b)
compares control states, and an unchanged field is never sent, so its line, comment included,
stays byte for byte. A value the fe would send can hold no `;` or `#` (422 `comment-markers`).

**D-b. Changed = the control's state differs from the state it loaded in.** Not "was operated":
changing a value back makes the field unchanged again (01's 29). An unchanged field is never sent,
whatever its spelling. A changed field sends the configurator's canonical spelling (table 133).

**D-c. The configurator is the source of the enable rules**, with three deliberate differences,
each in its sub-slice: the fields that need the model's facts (MoE, embedded MTP) wait for 2g
(2h.7, 2h.9: MoE read-only, user U3); Fit to VRAM stays operable when Context size or GPU layers
is set and says what llama-server will fit (2h.8, user U2); a disabled group or card says why in one line
(2h.7, 2h.9), where the GUI only greys it out.

**D-d. Discard and Reload.** Discard puts the controls back to the loaded values, with no request
(01's 30): it stands in for the GUI's **Revert**, minus the re-read. Reload re-reads the preset and
is offered only after a 409 (2h.2), as on the Server page (01's 35). No third button.

**Not in 2h**: the Assets card (value list until 2i, so `model`, `mmproj`, `model-draft`,
`spec-type` and the project URL are never sent), the Engine select (one option: nothing to
change), GPU distribution and Tensor placement (2f, 2j), Model info (2g), the warning strips and the
prunes at save (2k), New, Clone, Rename, Delete (2l to 2n), a raw-key view (2o).

## 1. Sub-slice 2h.1: the smallest end-to-end save

130. **What becomes editable.** The Embeddings card's two controls (2e's criterion 93: Embedding
    mode, Pooling) lose `disabled`. Every other 2e card stays as 2e made it (criterion 85). The
    read-only Notice `preset-read-only` is replaced, at the same place (criterion 84), by the
    Notice `preset-partly-editable`. ([3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions))
131. **The revision.** The read (2d's criterion 81) now needs `revision`, a string: a 200 without
    one is `preset-load-failed` (2d's 66), since such a page could not save. The fe never parses
    or compares it.
132. **Changed and dirty** (D-b). Each field has a loaded state (what 2e shows for the read's value)
    and a current state. A select's state is its selected option (the extra option of criterion 87
    included); a checkbox's its checked state; a number field's the pair (default box ticked, input
    text); a text field's its text, exactly. A field is **changed** when the two differ. The form
    is **dirty** while any field is changed: the status reads `save-unsaved` and both bar buttons
    are enabled (01's 29). A disabled control keeps its state and still counts (D5.2 Dependent
    field). Hint toggles change nothing.
133. **What a changed field sends** (`values[key]` of the body; an unchanged field is absent from
    the body). By the field's kind and reading (2e's criteria 87, 122):

    | Field | Sends |
    |---|---|
    | Select, off / on (`embeddings`, `no-host`, `load-on-startup`) | `on`: `"true"`; `off`: `null` (the GUI writes these only when true, `render_section`) |
    | Select, tri-state flag (`kv-unified`, `reasoning-preserve`, `backend-sampling`, `op-offload`, `repack`) and `flash-attn` | `default`: `null`; `on`: `"true"`; `off`: `"false"` (`emit_bool`) |
    | Select of words (`pooling`, cache types, `reasoning-effort`, `lazy-mode`, `load-mode`) | the option `default`: `null`; any other option: its value |
    | Select `reasoning`, `reasoning-format` (no `default` option) | the option's value (`auto` included, as the GUI writes it) |
    | Device select (`mmproj-device`, `device-draft`) | the empty option: `null` |
    | Checkbox `jinja`, `mmproj-offload` | checked `"true"`, unchecked `"false"` (`form.rs` 324, 502) |
    | Checkbox `prefill-assistant` | checked `null`, unchecked `"false"` (`form.rs` 559) |
    | Checkbox `fit` | checked `"on"`, unchecked `"off"` (`render_section`) |
    | Number | default ticked: `null`; else criterion 157 |
    | Text | the text trimmed of spaces and tabs; empty after trimming: `null` |

    A select's extra option (a value the select does not list) can only be the loaded state, so
    picking it again makes the field unchanged: it is never sent.
134. **The two lines under a control while editing.** The written-as line (2e's 124) shows only
    while its field is **unchanged** (it describes the file, which the save leaves as it is); a
    changed field hides it and its control's `aria-describedby` drops its id. The full-value line
    (2e's 118 to 121) shows only under a **disabled** control: an enabled input can be scrolled and
    an enabled select opened, which shows the whole value. So in 2h.1 Embeddings shows no
    full-value line; the disabled cards keep theirs.
135. **The save bar** (D5.2 Save bar; same DOM, texts and behaviour as the Server page's, 01's 28,
    30a, `ServerForm.tsx` 901-960, which the coder may lift into a shared component; the Server
    page's tests stay green): the last child of the `<section>` holding `preset-settings-title`,
    after the last card; status `role="status"`, the alert slot, **Discard** (`save-discard`),
    **Save changes** (`save-button`), both `aria-disabled="true"` while clean, sticky with D4's
    scroll padding. ([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum),
    [4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
136. **Discard**: every control back to its loaded state, field errors and the bar's alert cleared,
    status empty, focus stays on Discard, no request (01's 30, and its 3.1 ruling).
137. **Save** on a dirty form: client checks first (none in 2h.1; 2h.4 adds them, 01's 31), then
    `PUT /api/presets/<id>` (the id as the read gave it, encoded as 2d's read encodes it) with
    `{"revision": <loaded revision>, "values": <the changed keys, table 133>}`. While it runs: Save
    reads `save-saving` and both buttons are `aria-disabled`; a second click sends nothing; the
    controls stay editable (01's 32).
138. **200** with a body whose `revision` is a string and `running` a boolean: the loaded values
    become the read's `values` with the sent keys applied (`null` removes the key), the loaded
    revision the reply's; status `save-saved`, or `save-saved-restart` when `running` is true;
    focus stays on Save changes. When nothing was edited during the PUT, the controls show what was
    sent (a number as criterion 157 wrote it, a text trimmed), so the form is clean; edits made
    during the PUT stay on screen and the form is dirty again against the new loaded values (01's
    33, 33a, 33b). The written-as lines follow the new loaded values (a key sent as `true` shows
    none). A save never restarts or reloads llama-server.
139. **Any other answer** in 2h.1 (any non-2xx, a 2xx not shaped as 138, a body that is not JSON, a
    network failure, the fe's time limit): the bar's `role="alert"` shows `save-failed`; status back
    to `save-unsaved`; the edits stay, still dirty (01's 30a, 37). 2h.2 and 2h.3 give the keyed
    answers their own text; this stays the rule for everything they do not name.
140. **Leaving while dirty**: the preset page reports its dirty state to the app (`onDirty`, as the
    instance page does), so a nav link, the product link, `preset-back`, Sign out, the browser's Back
    and Forward and a typed hash all ask with the discard dialog, and closing the tab arms
    `beforeunload` (01's 38, 38a to 38h, unchanged). Clean, nothing asks. Leaving clears the dirty
    state.
141. **Keyboard, screen reader, tests, strings.** Each enabled control joins the tab order at its
    place (criterion 91's order: About toggle, then the control), then the bar's Discard and Save
    changes at the end of the page. Status changes are announced politely, failures by the alert.
    Stub: `embeddings` `1`, `pooling` `mean`, revision `r1`. Tests: the line "Written as 1" shows;
    picking `off` makes the form dirty, hides the line and Save sends exactly
    `{"revision":"r1","values":{"embeddings":null}}`; picking `on` back makes it clean and shows the
    line again; Pooling `rank` sends `{"pooling":"rank"}`, `default` sends `{"pooling":null}`; a 200
    `{revision:"r2",running:true}` shows `save-saved-restart` and the next save sends `r2`;
    `running:false` shows `save-saved`; a 500 shows `save-failed`, still dirty; Discard restores;
    Back while dirty opens the dialog. Every other card's controls are still disabled.
    **String (1 new)**: `preset-partly-editable` | Some settings cannot be changed here yet: their
    controls are greyed out. Change those in pai-config on the server machine. (Removed by 2h.10.)
    Reused: `save-*`, `discard-*`.

## 2h.2. The PUT's keyed answers

Applies 01's rule "the key decides, never the status, except 401", in this order: 401 (2h.3);
`session-not-administrator` (2h.3); `preset-changed-elsewhere`; `client-not-authorized`,
`client-unavailable`, `no-preset-named`, `client-timeout`; a check key (422). A 422 with a key the
fe does not know is `api-other` with the key. Anything else is 139.

142. **A refusal on a field.** A 422 `{key, args, field}` whose key is in table 150: `api-<key>`
    filled with `args` becomes the error of the field whose INI key is `field` (`aria-invalid`,
    `aria-describedby`, D5.2 Error), focus moves to its control once the error is rendered, and the
    bar's alert shows `save-refused` with the same text (01's 34). Editing that field clears its
    error, not the alert. Without `field`, the fallback of table 150 names it.
143. **The field is not a control of the page** (`model`, `override-tensor`, a key 2h shows nowhere),
    or the refusal has no field (`invalid-preset-id`, `preset-name-taken`): only the bar's alert,
    no field marked, focus stays on Save changes.
144. **The field's control is disabled, or on another card** (an untouched bad value: the manager
    checks the section whole). Disabled means a dependent field, a ticked default box, a group off
    by its rule, the MoE fields (178), or a card not yet enabled in this sub-slice's build: its
    error and `aria-invalid` go on it as on any field, the bar's alert reads the text, and focus
    goes to the field's first enabled control (its default box), else stays on Save changes (01's
    34b; the switch that enables a preset field is in Assets, not operable in 2h). An enabled
    control on another card takes focus like any other (142), scrolled into view by the browser.
145. **409 `preset-changed-elsewhere`**: the bar's alert `api-preset-changed-elsewhere` and the button
    **Reload** (`save-reload`) after it (01's 35, 3.1). Reload opens the discard dialog when dirty
    (Keep editing: nothing happens, focus back on Reload); Discard changes, or no dialog when clean,
    reads the preset again: `preset-loading` in place of the settings, then the form with the read's
    values, clean, no alert, focus on the page's `h1`. A read that fails shows 2d's messages.
146. **404 `no-preset-named`**: the bar's alert `api-no-preset-named` filled with the id; the edits
    stay, still dirty; no Reload (the page is not replaced, 01's 37a).
147. **403 `client-not-authorized`**: `api-client-not-authorized`; **503 `client-unavailable`**:
    `api-client-unavailable`; edits kept, still dirty (01's 37).
148. **504 `client-timeout`, and the fe's own time limit** (120 s for a `PUT`, D7): `save-timeout`
    with no button; the revision held is unchanged, so a later save meets 409 if the file was
    written (01's 47d, S3.6).
149. **The guard's keys and `presets-malformed`** (400, the fe never sends a malformed body):
    `save-failed` (01's S1.1).
150. **Check keys, their field and their text** (the bff's 422 keys for a preset, `checks.rs`
    `KEYS`; English verbatim from the toml line named, no closing full stop like every copied check
    text, 01's 5.6):

    | Key | Fallback field | `api-<key>` English (toml) |
    |---|---|---|
    | `not-whole-number-in-range` | `args[0]` | exists |
    | `comment-markers` | `args[0]` | exists |
    | `not-a-number` | `args[0]` | {0} = `{1}` is not a number (1015) |
    | `not-one-of` | `args[0]` | {0} = `{1}` is not one of: {2} (1016) |
    | `not-json-object` | `args[0]` | {0} is not a JSON object (a list of "key": value pairs in braces) (1017) |
    | `min-above-max` | `args[0]` | {0} = {1} is above {2} = {3}: the minimum cannot exceed the maximum (1018) |
    | `fit-target-format` | `fit-target` | fit-target `{0}`: {1} as whole numbers, one value or a comma-separated list per device (1002) |
    | `moe-cache-with-tensor-split` | `moe-cache-mib` | (1001, verbatim) |
    | `load-on-startup-over-max`, `-unlimited` | `load-on-startup` | exist |
    | `preset-no-model` | none (Assets) | preset `{0}` names no model: every preset needs a `model` path (1007) |
    | `override-no-pattern` | none (2j) | tensor override {0} has an empty pattern (1008) |
    | `override-no-device` | none (2j) | (1009, verbatim) |
    | `invalid-preset-id` | none | invalid preset id `{0}`: use letters, digits, '.', '-', '_' (996) |
    | `preset-name-taken` | none | a preset named `{0}` already exists (1006) |

    The manager checks the whole section, so a hand-edited value the user did not touch (Pooling
    `weird`) is refused too, on its field: the user corrects it or picks another option. Tests: one
    per row of 142 to 149; `not-one-of` on `pooling` marks Pooling and focuses it; `preset-no-model`
    marks nothing; `not-one-of` on `cache-type-k` while the KV cache card is still disabled (an
    untouched bad value) marks that field and leaves focus on Save changes (144). **Strings (12
    new)**: the nine `api-` rows above marked with a toml line, plus `api-preset-changed-elsewhere`
    | This preset changed elsewhere (edited, or switched on or off), or the web service restarted,
    after you opened it. Reload to see it as it is now; your edits here will be lost. (The revision
    covers the section and its file's name: a save of another preset gives no 409.)

## 2h.3. The session ends during a save

The Server page's rules, unchanged, on the preset page. Nothing new to render: the sign-in Dialog,
`SignIn` in the dialog and the alert all exist.

151. **401** (any body): the bar's alert `session-ended`, a **Sign in** button (`session-sign-in`)
    after it; the edits stay, status `save-unsaved`; the banner and the nav stay as they were (01's
    36, 36a). The button opens the sign-in Dialog; Escape or Cancel closes it, focus back on that
    button.
152. **The same user signs in**: the dialog closes, the alert goes, focus returns to Save changes,
    the edits are intact, nothing is resent (01's 36). **Another user**: the edits are dropped, the
    preset is read again for the new session (clean), a `role="alert"` paragraph
    `server-edits-dropped` (reused: its text names no page) sits right after the `h2`
    `preset-settings-title` until the page is left, focus on the `h1` (01's 36b).
153. **403 `session-not-administrator`**: as 151 with `session-revoked` in place of
    `session-ended` (01's S1.6); signing in again as the same account is 14b inside the dialog (S1.8).
154. **Tests**: 401 then same user; 401 then another user (one `GET`, the dropped alert, focus);
    revoked. No new string.

## 2h.4. Number fields, and Resource / context

155. **Resource / context editable** (2e's table 92). Enable rule: **Unified KV** is enabled while
    Parallel slots' default box is unticked (MP 704), live as the user edits; the other five always.
    No reason line: Parallel slots sits just above and Unified KV's hint says why
    (`preset-unified-kv-hint`, "so this is disabled").
156. **The default box** (D5.2 Default box; the Server page's 28a): ticked, the input is disabled
    and empty (a preset has no default value from the API; the hint names the default);
    unticked, the input shows again the text it held before it was ticked: the file's value for a
    key that was present, empty for one that was absent, or what the user typed. When it comes
    back empty, focus moves to the input. Discard and a 200 reset that memory.
157. **Integer check and what is sent.** At Save, every enabled number field whose box is unticked
    is checked, changed or not (the manager checks the whole section anyway; 01's 5.4): the text
    trimmed must be an optional `+` or `-` then digits, with a value in the key's range (table
    below, `checks.rs` `PRESET_INT_KEYS`). Failure: the field's error, focus on the first failing
    control in DOM order, the bar's alert `save-invalid` (01's 31). Sent: the number in decimal with
    no sign for a positive value and no leading zeros (`+0172032` sends `172032`, as the GUI's
    `emit_i32`).

    | Range | Keys | Error text |
    |---|---|---|
    | any whole number | `cache-ram`, `reasoning-budget`, `n-predict`, `top-k` | `field-error-whole-number` |
    | 0 or more | `ctx-size`, `spec-draft-n-min`, `spec-draft-n-ubatch`, `moe-cache-mib`, `ctx-checkpoints`, `checkpoint-min-step` | `field-error-number-min` with 0 |
    | 1 or more | `parallel`, `kv-unified-per-slot`, `batch-size`, `ubatch-size`, `cache-reuse`, `fit-ctx`, `image-min-tokens`, `image-max-tokens`, `spec-draft-n-max` | `field-error-number-min` with 1 |
    | 1 to 1024 | `spec-ngram-mod-n-match` and the six `spec-ngram-{simple,map-k,map-k4v}-size-{n,m}` | `field-error-number` with 1, 1024 |
    | 0 to 1024 | `spec-ngram-mod-n-min`, `spec-ngram-mod-n-max` | `field-error-number` with 0, 1024 |
    | 1 to 65535 | `spec-ngram-map-k4v-min-hits` | `field-error-number` with 1, 65535 |
    | (not in the table, "whole number" only up to 2147483647) | `n-gpu-layers`, `n-cpu-moe`, `n-cpu-ffn`, `n-gpu-layers-draft` | `field-error-whole-number` |

    The maximum of every row without one is 2147483647 (`i32`); a value past it fails with the
    row's text. The last row's keys are not in `PRESET_INT_KEYS` (the GUI's sliders bound them);
    the fe checks only that they are whole numbers. The pair checks (`min-above-max`) are left to
    the server (2h.2).
158. **Input mode** (2e's notes for 2h): integer fields whose range starts at 0 or above keep
    `inputmode="numeric"`; the any-whole-number keys and the last row get none, since a phone's
    numeric keyboard may have no minus key
    ([MDN, `inputmode`](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Global_attributes/inputmode):
    "Devices may or may not show a minus key"). Float fields keep none (2e).
159. **A unit is the input's description.** The unit after an input (D5.2 Units) gets the id
    `<field id>-unit`, and the input's `aria-describedby` lists it first, then the error's id when
    there is one, so "Cache RAM, edit text, MiB" is read. Built once in the shared fields
    (`Field.tsx`), so it applies to the Server page's Memory margin too; the unit stays visible
    text, not `aria-hidden`. ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
    [3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions))
160. **Full-value line**: 134 holds. In this card only Unified KV can be disabled with a value
    (Parallel slots at default); a ticked box's input is empty, so it never has one.
161. **Focus after an error on a ticked box's field** (a 422 on `ctx-size` while default is
    ticked): 144, focus on the default box.
162. **Tests**: tick and untick memory; `ctx-size` `abc` (loaded) blocks Save with
    `field-error-number-min` 0 and focus on Context size; `parallel` `0` refused with min 1; `+08`
    sends `8`; ticking Parallel slots' box disables Unified KV and keeps its state; unticking an
    absent field's box empties and focuses its input; the Server page's Memory margin input has
    `aria-describedby` naming its unit.
163. **String (1 new)**: `field-error-whole-number` | Enter a whole number, without a decimal point.

## 2h.5. KV cache, Sampling overrides, Advanced

164. **Editable**: 2e's tables 96, 97, 98, every field always enabled (MP).
165. **Decimal check** (the float keys `temp`, `top-p`, `min-p`, `repeat-penalty`,
    `presence-penalty`, and in 2h.9 `spec-draft-p-min`; `checks.rs` `PRESET_FLOAT_KEYS`, read
    with `,` as a decimal separator as `ini::parse_float` does): the text trimmed, with `,`
    replaced by `.`, must be a finite decimal number (optional sign, digits with at most one `.`,
    an optional exponent such as `e-3`; not `inf` or `nan`). Error `field-error-decimal`. Sent: the
    trimmed text with `,` replaced by `.` (`1,5` sends `1.5`). The loaded `presence-penalty` `1,5`
    passes the check and, unchanged, is not sent.
166. **Chat kwargs** (`chat-template-kwargs`, Text): checked when not empty: no `;` or `#`
    (`field-error-comment-markers`, exists), then one JSON object (`JSON.parse` gives an object
    that is not an array or `null`): `field-error-json-object`. The comment check comes first.
167. **Cache RAM's input mode**: none (158). **Load on startup** sends per 133 (`on` `true`, `off`
    `null`); its truthy reading (2e's 122) still decides what the select shows.
168. **Tests**: `temp` `0,7` sends `0.7`; `temp` `hot` blocks with `field-error-decimal`; kwargs
    `[1]` blocks with `field-error-json-object`; kwargs `{"a":"#"}` blocks with the comment message;
    `flash-attn` `auto` loaded (extra option) is not sent until another option is picked;
    `cache-type-k` `default` sends `null`.
169. **Strings (2 new)**: `field-error-decimal` | Enter a number, such as 0.7 (a decimal comma also
    works). `field-error-json-object` | Enter one JSON object in braces, such as
    {"enable_thinking": false}.

## 2h.6. Chat / reasoning

170. **Editable**: 2e's table 101, every field always enabled (MP 863-970).
171. **Sends** per 133: `jinja` checked `true` / unchecked `false`; `prefill-assistant` checked
    `null` / unchecked `false`; `reasoning`, `reasoning-format` their option's value;
    `reasoning-effort` `default` `null`; `reasoning-preserve` the tri-state.
172. **Budget message** (Text): no `;` or `#` (`field-error-comment-markers`); no placeholder
    (2e's criterion 87 kept **(preference)**: the GUI's example, toml 337, reads as a value in an
    empty box, and the hint already says what the line does).
173. **Numbers**: `reasoning-budget` and `n-predict` any whole number, no input mode (157, 158).
174. **Tests**: `jinja` `0` loaded shows "Written as 0", unchecked; checking it sends `true`;
    `prefill-assistant` absent (checked), unchecking sends `false`, checking again makes it clean;
    `reasoning-budget` `-1` passes. No new string.

## 2h.7. Vision, and Hardware but the Fit trio

175. **Editable**: 2e's table 102 (Vision) and table 107 without `fit`, `fit-ctx`, `fit-target`
    (2h.8).
176. **Vision's rule** (MP 1082-1122): every control of the card is enabled only when the read's
    `mmproj` is present and not empty; Vision GPU also needs Vision on GPU checked (live). While
    `mmproj` is absent, a paragraph `preset-vision-off` (D5.2 Card note, `--muted`) sits right
    after the card's `h3`, before the first field. Vision GPU needs no line of its own (its switch
    is just above).
177. **Draft placement** (MP 632, 660-665): Draft device and Draft GPU layers are enabled only when
    the read's `model-draft` is present and not empty. While it is absent a note row
    `preset-draft-file-off` sits before Draft device (where the GUI shows its embedded-MTP note,
    MP 620-625; 2g adds that note).
178. **MoE CPU layers and MoE cache stay disabled** until 2g (the GUI enables them only for a model
    with experts, MP 493-503, a fact read from the GGUF; a `moe-cache-mib` on a dense model does not
    load, `presets.rs` 1548-1552). A note row `preset-moe-later` sits before MoE CPU layers.
    Decided by the user (U3). Their values are kept and never sent (the bff refuses both keys).
179. **A device select** (Vision GPU, Draft device: 2e's criterion 103) offers the empty option and
    the file's value only, until 2f brings the devices: picking the empty option sends `null`.
180. **Load mode** sends the option's value, lowercase (`MLOCK` loaded selects `mlock`; unchanged it
    is not sent; picking `mlock` from another option sends `mlock`).
181. **The reason lines** of 176 to 178 are plain paragraphs in reading order (not live regions, not
    in any `aria-describedby`): a disabled control is not in the tab order, so they are met in
    browse mode with the controls they explain.
    ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
    [3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions))
182. **Tests and strings.** `values` without `mmproj`: the four Vision controls disabled and the line
    shown; with `mmproj` and `mmproj-offload` `false`: Vision GPU disabled, the others enabled, no
    line; without `model-draft`: Draft device and Draft GPU layers disabled with their note; MoE
    fields always disabled with their note. **Strings (3 new)**: `preset-vision-off` | Used only
    with an MMProj file, and this preset has none (Assets). `preset-draft-file-off` | Draft device
    and Draft GPU layers apply only to a Draft model file, and this preset has none (Assets).
    `preset-moe-later` | MoE CPU layers and MoE cache stay as the file has them: this page cannot
    yet read whether the model has experts. Change them in pai-config on the server machine.
    (Removed by 2g.)

## 2h.8. Fit to VRAM, Fit min context, Fit free VRAM

Rechecked in the llama.cpp v1.0.0 clone (`build/linux/llama.cpp`, `common/fit.cpp`): the fit first
cuts the context when it is not set by the user (lines 479-545), then throws "n_gpu_layers already
set by user ... abort" when GPU layers is set (550-551); the throw is caught and logged as a
warning (995-997) and does not undo the context already cut. With the context set it keeps it and
sizes the layers. So the three cases differ.

183. **Fit to VRAM is always enabled** (D-c, user U2; the GUI disables it unless Context size
     and GPU layers are both at default, MP 585, and saves `off` then, `form.rs` 550). Its checked
     state follows 2e.3c (criterion 127) at load, then the user's clicks.
184. **Its toggle text** (the checkbox's accessible name, as in 2e), from the live state of the two
     default boxes:

     | Context size | GPU layers | Box | Text |
     |---|---|---|---|
     | default | default | any | `preset-fit-allowed` (toml 275) |
     | either or both set | | unchecked (or invalid) | `preset-fit-not-allowed` (toml 276) |
     | set | default | checked | `preset-fit-layers-only` |
     | default | set | checked | `preset-fit-context-only` |
     | set | set | checked | `preset-fit-nothing` |

     "Set" = the field's default box is unticked.

     The text changes silently as the user edits those fields (not a live region); the box keeps
     its state.
185. **An invalid `fit`** (2e's 128: `TRUE`, `""`, `auto`) loads in a state of its own: unchecked,
     with the written-as line, the error `preset-fit-invalid` and `aria-invalid`. **It blocks the
     save**: at Save it is a client check failure like 157's (the error stays, focus on the box,
     `save-invalid`), because nothing else catches it (the manager's checks do not read `fit`,
     `checks.rs` `PRESET_ENUM_KEYS`) and llama-server would not start the preset. The first click
     checks the box (sends `on`); further clicks toggle between checked (`on`) and unchecked
     (`off`), both changed: no click returns to the invalid state, only Discard does. Once changed,
     the error, `aria-invalid` and the written-as line go.
186. **Fit min context and Fit free VRAM** are enabled while the box is checked (MP 595, 606), live.
     Fit min context: 1 or more (157). Fit free VRAM (Text, unit `MiB`, 159): checked when not
     empty: no `;` or `#` (`field-error-comment-markers`), then whole numbers separated by commas,
     spaces around each allowed (`checks::fit_target`): `field-error-fit-target`. Sent trimmed.
187. **Tests**: the rows of 184, and the text following an untick of Context size's box; `fit`
     `TRUE` blocks Save, one click sends `on`, two clicks send `off`, Discard brings the error back;
     unchecking the box disables both dependents; `fit-target` `2048,x` blocks with
     `field-error-fit-target`, `2048, 1024` passes.
188. **Strings (4 new)**: `preset-fit-layers-only` | auto-fit the GPU layers only: Context size is
     set (-fit on). `preset-fit-context-only` | auto-fit the context only: GPU layers is set, so
     llama.cpp keeps your layers (-fit on). `preset-fit-nothing` | nothing to fit: Context size and
     GPU layers are both set (-fit on). `field-error-fit-target` | Enter MiB as whole numbers: one
     number, or one per device separated by commas, such as 2048,1024.

## 2h.9. Speculative decoding

189. **Editable**: 2e's table 112, with the rules below (MP 1151-1350).
190. **The draft group** (`preset-draft-model-group`) is enabled while the preset has a draft: the
     read's `model-draft` is not empty, **or** `spec-type` holds a `draft-` entry (split as 2e's
     criterion 75). `model-draft` and `spec-type` are read, never sent (the bff refuses them). The
     GUI's second half is "the model embeds MTP heads" (`state.slint` 346), a GGUF
     fact the page reads only from 2g; until then a drafter named in `spec-type` stands in for it
     (the GUI's save removes a `draft-mtp` the model cannot use, `prune_inactive_draft_keys`, so a
     file it saved names one only for a model that has the heads). Deliberate difference: an MTP
     model whose `spec-type` names no drafter has the group disabled here, enabled in the GUI.
191. **Each n-gram group** is enabled while `spec-type` lists its name (`ngram-mod`, `ngram-simple`,
     `ngram-map-k`, `ngram-map-k4v`; MP 1233-1350), all its fields together.
192. **One reason line per group**, right after its `h4`, inside the `role="group"` div, before the
     first field (D5.2 Card note style), shown only while the group is disabled: the draft group
     `preset-draft-group-off`; an n-gram group `preset-ngram-off` filled with its name (literal).
     Read in browse order (181). `spec-type` cannot change in 2h (Assets, 2i), so the rule is fixed
     for the page's life.
193. **Tests and strings.** `spec-type` `ngram-mod,draft-mtp` with no `model-draft`: the draft group
     and `ngram-mod` enabled, the three others disabled each with its line; `values` `{}`: all five
     disabled with lines; `spec-draft-p-min` uses 165's decimal check; `spec-ngram-mod-n-min` `2000`
     blocks with `field-error-number` 0, 1024. **Strings (2 new)**: `preset-draft-group-off` | Used
     only with a draft: a Draft model file or a Drafter, and this preset has neither (Assets).
     `preset-ngram-off` | Used only when N-gram drafters (Assets) includes {0}.

## 2h.10. Finishing: the Notice, Assets, the jump list

194. **The Notice goes**: `preset-partly-editable` is removed from the page and the strings file
     (and `preset-read-only`, replaced in 2h.1). Every 2e card is now editable under its rules; the
     only greyed-out controls are those of 176 to 178, 190 to 192 and the dependents, each with a
     visible reason or an adjacent switch. ([3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions))
195. **The Assets card says why it is text**: an intro paragraph `preset-assets-read-only` right
     after its `h3` (D5.2 Card intro). Removed by 2i.
196. **The section jump list.** The page is about 5000 px tall at 1280 px with about 180 tab stops,
     so between the `h2` `preset-settings-title` and the engine Notice (or the Engine field), a
     `<nav>` named `preset-jump-label` holds a list (`<ul>`) of buttons, one per card in page order
     (Assets, Hardware, Resource / context, Embeddings, KV cache, Chat / reasoning, Sampling
     overrides, Advanced, Vision, Speculative decoding), each named by its card's title (the same
     string). Activating one moves focus to that card's `h3` (`tabindex="-1"`) and scrolls it into
     view; the hash does not change. Buttons, not links: an in-page `#fragment` would be read by the
     hash router as a route and, while dirty, ask the discard dialog. Laid out as a wrapping row of
     secondary text buttons, 24 px targets at least (D5.1). **(preference: a list rather than
     nothing; headings alone serve screen-reader users, not sighted keyboard users)**
     ([2.4.1](https://www.w3.org/TR/WCAG22/#bypass-blocks), [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order),
     [2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum))
197. **Tab order**, final: skip link, banner, nav, `preset-back`, the jump buttons, the Engine
     field, then every card's About toggles and enabled controls in DOM order, then Discard and
     Save changes. Known limit: from a field of the first cards, Save is many stops away; the jump
     list's last entries shorten it **(accepted)**.
198. **Tests and strings.** No `preset-read-only` or `preset-partly-editable` on the page; the jump
     nav has ten buttons in card order; a click on "KV cache" focuses its `h3` and leaves
     `location.hash` unchanged. **Strings (2 new)**: `preset-jump-label` | Preset sections.
     `preset-assets-read-only` | The files of a preset cannot be picked here yet. Change them in
     pai-config on the server machine. Removed: `preset-read-only`, `preset-partly-editable`.
