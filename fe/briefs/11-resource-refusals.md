# Brief 11: the resource refusals on the web (slice M1)

Renamed from `11-over-memory.md` when `model-profile-model-missing` was folded in (coordinator,
2026-11-20): it now covers both resource refusals of a model profile.

The manager now refuses a write that puts a model profile into effect when its estimate does not fit
the node's GPU memory (commit 16e6693, `PLAN.md` step 10 item 13). The bff answers it as
**422 `{"key": "model-profile-over-memory", "args": [profile, instance, needed MiB, available MiB]}`
with no `field`** (`fe/briefs/api-contract.md`, "The resource checks";
`crates/bff/src/model_profiles.rs` `field_of`). The web UI has no text for it today, so both surfaces
that can receive it show the fallback `api-other`: "The request was refused
(model-profile-over-memory)." This slice gives it its own text. Criteria are numbered **11 M1.n**;
test names cite them so (`11 M1.4`). One slice, 13 criteria: 1 to 9 the memory refusal, 10 to 13
the missing model file (section 7).

## 0. What exists (read on main at 84ab3ed)

- **Who answers it.** `PUT /api/profiles/model/{id}` (the profile page's save) and
  `PUT /api/profiles/model/{id}/enabled` (the Models list switch). The assignment routes can answer
  it too, but the web UI has no screen for them yet (out of scope).
- **When.** Only a write that makes a model profile's estimate larger can be refused
  (`crates/manager/src/manager/service.rs` `check_render_memory`): switching on (the estimate grows
  from 0), or a save that changes a key the estimate reads. A switch off is never refused.
- **What the estimate reads** (`estimated_of`, same file): the model file's header, `ctx-size`
  (0 or unset: the model's trained context) and `cache-type-k` / `cache-type-v` (unset: f16). It
  assumes every layer on the GPU, so GPU layers, MoE CPU layers and the like do not change it. The
  advice in the text therefore names only what the check reads and the page can change: Context
  size (card Resource / context), K-cache type and V-cache type (card KV cache). The model file is
  read-only on the web (`model-profile-assets-read-only`), so a smaller model is not suggested.
- **The figures.** `needed` is that one profile's estimate rounded up to a whole MiB; `available`
  is the sum of the node's usable GPU memory (`pai_core::devices::pool_usable`) rounded down. Both
  arrive as decimal strings.
- **The profile page today.** `ModelProfilePage.tsx` `REFUSALS.text` maps each known 422 key to its
  `api-<key>` text; an unknown 422 key falls to `api-other` (`save.ts` `saveAnswer`). A known key
  with no field (as `model-profile-name-taken`) is told in the save bar as `save-refused`
  ("Not saved: {0}"), no field is marked, the focus stays on Save changes and the edits are kept
  (`ModelProfilePage.tsx`, case `refused`; `fe/briefs/02h-model-profile-save.md` 142 to 144).
- **The list today.** `ModelProfiles.tsx` `failureText` maps known keys to their text in the
  `role="alert"` paragraph `.model-profiles-switch-alert`; an unknown 422 key gives `api-other`.
  After any failure other than a lost session the list reads the profiles again and the switch shows
  the row as read (02-models.md 2c.48).
- **The GUI and the CLI** say, in `crates/core/strings/en_US.toml:1011` and `cli.toml:195`:
  "`{0}` does not fit the GPU memory of instance `{1}`: it needs {2} MiB, the node has {3} MiB".

## 1. Decisions

1. **One text, both surfaces.** The key `api-model-profile-over-memory` is shown on the save bar
   (inside `save-refused`) and, bare, in the list's switch alert. The text starts with the profile
   name, so it reads well both after "Not saved: " and on its own.
2. **Wording: the GUI's sentence, plus a next step.** The first sentence is the GUI's, verbatim,
   backticks included, as `fe/DESIGN.md` section 7 asks (the web's other copied `api-*` texts keep
   their backticks too). The GUI text says what happened but not what to do; the web appends one
   sentence with the fix, which
   [3.3.3 Error Suggestion](https://www.w3.org/TR/WCAG22/#error-suggestion) asks for when it is
   known. **Differs from the GUI on purpose**: the GUI has no suggestion (question 1 below).
3. **MiB as sent, no GiB.** The fe shows `args[2]` and `args[3]` exactly as received: no conversion,
   no rounding, no thousands separator. Reasons: the GUI and the CLI print MiB; every memory field
   on the profile page is in MiB (`model-profile-cache-ram-hint`, `model-profile-fit-free-vram-hint`),
   so the figure can be compared with what the user types; a conversion would be code the fe has
   nowhere else, and rounding to GiB could print two equal figures for a profile 1 MiB over.
   (Preference, not a standard: GiB would read more easily for figures above 10 000.)
4. **Nothing new to see.** No new component, token, style or layout: the text lands in two places
   that already exist and already wrap long words (`body { overflow-wrap: break-word }`, inherited,
   `fe/src/index.css`), so a long profile name does not scroll sideways at 320 px
   ([1.4.10 Reflow](https://www.w3.org/TR/WCAG22/#reflow)).
5. **No link added in the list alert.** The row's name in the same table is already a link to the
   profile's page, where the fields named in the text are.

## 2. The text

| Key | English |
|---|---|
| `api-model-profile-over-memory` | ``"`{0}` does not fit the GPU memory of instance `{1}`: it needs {2} MiB, the node has {3} MiB. To make it fit, lower its Context size, or pick a smaller K-cache type or V-cache type (cards Resource / context and KV cache)."`` |

The value in `en_US.json` is the text between the outer double quotes: it starts with a backtick and
ends with `KV cache).` Placeholders by position: `{0}` profile, `{1}` instance, `{2}` needed MiB,
`{3}` available MiB. No other key is added or changed.

Example, with args `["qwen-27b", "default", "30721", "24560"]`:

- Profile page, save bar: "Not saved: \`qwen-27b\` does not fit the GPU memory of instance
  \`default\`: it needs 30721 MiB, the node has 24560 MiB. To make it fit, lower its Context size, or
  pick a smaller K-cache type or V-cache type (cards Resource / context and KV cache)."
- Models list, switch alert: the same without "Not saved: ".

## 3. Behaviour and accessibility

- **Profile page.** After Save changes answers this refusal: the save bar's `role="alert"` says the
  text above ([4.1.3 Status Messages](https://www.w3.org/TR/WCAG22/#status-messages),
  [3.3.1 Error Identification](https://www.w3.org/TR/WCAG22/#error-identification): the error is
  described in text, and here it is about the whole profile, so no field carries it). No field is
  marked and no control gets `aria-invalid`. The focus stays on Save changes
  ([2.4.3 Focus Order](https://www.w3.org/TR/WCAG22/#focus-order): nothing moves it to an unrelated
  field). The edits stay, the form stays changed, so the user can lower Context size and save
  again ([3.3.7 Redundant Entry](https://www.w3.org/TR/WCAG22/#redundant-entry)). The alert then
  follows the existing rules of the bar (02h 198a to 198g); nothing new.
- **Models list.** After the switch answers this refusal: `.model-profiles-switch-alert`
  (`role="alert"`) says the text; no sign-in button; the switch status line stays empty; the list is
  read again and the row shows what the read says (Off, since a refusal writes nothing); the focus
  stays on the switch the user pressed.
- **Keyboard and screen reader.** No new control, so the focus order is unchanged. A screen reader
  hears the alert once, when it appears; the switch then announces its own state when the user
  reaches it.

## 4. Acceptance criteria

Fixture: args `["qwen-27b", "default", "30721", "24560"]`, body with no `field`. Suggested files:
`ModelProfilePage.test.tsx` (or a new `ModelProfileOverMemory.test.tsx`) and
`ModelProfiles.test.tsx`; the string in `strings.test.ts` style.

1. **11 M1.1** `fe/src/strings/en_US.json` has `api-model-profile-over-memory` with exactly the text
   of section 2.
2. **11 M1.2** Profile page: a save answered 422 with this key shows in the save bar's
   `role="alert"` exactly `t('save-refused', t('api-model-profile-over-memory', ...args))`, the four
   args at their places as sent (`30721` and `24560`, no other number format).
3. **11 M1.3** Profile page: after that refusal no field shows an error text and no control has
   `aria-invalid="true"`.
4. **11 M1.4** Profile page: after that refusal the focus is on Save changes, and the edited
   control keeps the value the user gave it; Save changes and Discard stay enabled.
5. **11 M1.5** Profile page: the text of `api-other` ("The request was refused
   (model-profile-over-memory).") appears nowhere on the page.
6. **11 M1.6** Models list: a switch on answered 422 with this key shows in
   `.model-profiles-switch-alert` (`role="alert"`) exactly
   `t('api-model-profile-over-memory', ...args)`, with no "Not saved: " in front, and no
   `api-other` text anywhere.
7. **11 M1.7** Models list: after that refusal no sign-in button shows and the switch status line
   (`.model-profiles-switch-status`) is empty.
8. **11 M1.8** Models list: after that refusal the list is read again (a second `GET
   /api/profiles/model`), and when that read says the profile is off, its switch is unchecked and
   its status cell reads `model-profiles-status-off`.
9. **11 M1.9** Models list: after that refusal the focus is on the switch that was pressed.

M1.3, M1.4, M1.7 to M1.9 are the existing behaviour for a known field-less key; they are pinned here
for this key so a later change to the table cannot drop them. The UX check (`fe/ux/`) needs no new
state: the text uses existing elements.

## 5. Out of scope

- The assignment routes (`PUT /api/instances/{id}/profiles/model/...`) and the global switch: no
  web screen yet.
- Any GiB formatting, any estimate shown before saving, any link or button in the alert.
- The GUI's own text (question 1).

## 6. Questions only the user can answer

1. Should the GUI and the CLI get the same next-step sentence, so the three say the same thing? (The
   web adds it now, by 3.3.3; the GUI change is a separate slice in `crates/core/strings/`.)

## 7. The missing model file (`model-profile-model-missing`)

The same family of checks: 422 `{"key": "model-profile-model-missing", "args": [profile, instance,
path], "field": "model"}` (`fe/briefs/api-contract.md`, "The resource checks";
`crates/bff/src/model_profiles.rs` `field_of` maps it to `model`). The manager refuses only a
(profile, path) pair the write adds, the path being the one it looked at for that instance
(`crates/manager/src/manager/service.rs`, the doc comment above the refusal). Today it falls to
`api-other` on both surfaces too.

- **Where.** The same two places as the memory refusal. On the profile page the field `model` is the
  read-only Model file row of the Assets card, which has no control, so brief 02h 143 applies: only
  the bar's alert, no field marked, focus stays on Save changes. On the list the `field` is ignored,
  as for every switch refusal.
- **Wording.** The GUI's sentence verbatim (`crates/core/strings/en_US.toml:1010`), plus a next step.
  The web cannot change the model file (`model-profile-assets-read-only`), so the fix it names is on
  the server machine, in the words that text already uses. **Differs from the GUI on purpose**, as
  in decision 2.

| Key | English |
|---|---|
| `api-model-profile-model-missing` | ``"`{0}` names a model file that is not there: `{2}` is missing for instance `{1}`. Put the file at that path on the server machine, or change the Model file in pai-config there."`` |

Placeholders: `{0}` profile, `{1}` instance, `{2}` path, shown as sent. Fixture for the criteria:
args `["qwen-27b", "default", "/srv/models/qwen-27b.gguf"]`, `field` `model`.

10. **11 M1.10** `fe/src/strings/en_US.json` has `api-model-profile-model-missing` with exactly the
    text above.
11. **11 M1.11** Profile page: a save answered 422 with this key shows in the save bar's
    `role="alert"` exactly `t('save-refused', t('api-model-profile-model-missing', ...args))`, and
    no `api-other` text appears.
12. **11 M1.12** Profile page: after that refusal no field shows an error text, no control has
    `aria-invalid="true"`, the focus is on Save changes, and Save changes and Discard stay enabled.
13. **11 M1.13** Models list: a switch on answered 422 with this key shows in
    `.model-profiles-switch-alert` exactly `t('api-model-profile-model-missing', ...args)`, no
    `api-other` text anywhere, and the focus is on the switch that was pressed.
