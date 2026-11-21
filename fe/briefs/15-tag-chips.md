# Brief 15: instance tags as chips (slices C1 and C2)

`PLAN.md` step 10 item 13, **Tags as chips** (user, 2026-11-21): the instance page's tags field
becomes a row of chips: a round "+" that, clicked, opens an oval input for one label; confirmed, it
becomes a chip and a new "+" follows. While typing, the labels already used (on any instance or
server profile) are suggested. No edit of a chip: remove it and add it again; how a chip is removed
is the designer's. Web UI only: `PUT /api/instances/{id}/tags` is unchanged. Two sub-slices, each
under 12 criteria: **C1** the chips, adding, removing and saving (criteria **15 C1.n**); **C2** the
suggestions (criteria **15 C2.n**), built after C1. Test names cite them (`15 C1.4`).

## 0. What exists (read on main at 67f4e24)

- **Brief 12** (`fe/briefs/12-instance-tags.md`, slice T1): the instance page's `TagsField`
  (`fe/src/InstancePage.tsx` ~716 to 899) is one text input **Instance tags**, split on commas and
  spaces, saved by **Save tags** or Enter, in a `form` of its own outside the Server form, with the
  Action feedback pattern (status, alert, Sign in); unsaved text makes leaving ask (U2). Under it,
  "From the server profile: edge" lists the profile's tags that are not the instance's own. The
  field is latched: filled from the first read, never rewritten by a poll. The Overview's tiles
  (T1.2 to T1.4) are not touched by this brief.
- **The rule** (`crates/core/src/checks.rs` `tags_rule`, ~714): 0 to 8 tags per list (`TAGS_MAX`),
  each 1 to 32 characters (`TAG_MIN`, `TAG_MAX`) of `a-z`, `0-9`, `-`, none twice. Refused as 422
  `{"key": "server-bad-tags", "args": [reason, list joined by ",", "8", "1", "32"], "field":
  "tags"}`; the fe shows `api-server-bad-tags`, the GUI's text verbatim.
- **`PUT /api/instances/{id}/tags`** `{"tags": [string]}` replaces the instance's own list: 204; 422
  as above; 404 `client-no-instance`; 400 `tags-malformed`; 403 `session-not-administrator` (the
  session ends); 403 `client-not-authorized`; 503 `client-unavailable`; 502 `client-other`; 401
  `session-required` (`fe/briefs/api-contract.md` "PUT /api/instances/{id}/tags").
- **Where used labels live.** `GET /api/instances` items carry `tags` (the union) and `instanceTags`
  (the instance's own); the page already holds that list (`store`, polled every 5 s).
  `GET /api/profiles/server` (served, `crates/bff/README.md` "The list") answers rows `{"id",
  "engine", "port", "tags", "instance"}`; errors 401, 403, 503, 502.
- **`pai-config` and `pai`** have no tags (no `Tags` in `crates/config/` or `crates/cli/`): the web UI
  is first, nothing to agree with but the refusal text.

## 1. Decisions

1. **Each add and each removal saves at once** (no Save tags button). Reasons: the page already
   writes its one-click settings at once (Start at boot, the model profile switch) and keeps a save
   button only for the Server form, a document of many fields; a chip that appears but is not yet
   saved would be a third state the user cannot see. Adding is an explicit act (Enter or the Add
   button), not a change of value, so [3.2.2 On Input](https://www.w3.org/TR/WCAG22/#on-input)
   holds. No confirmation and no undo on removal: tags are labels, the user re-adds one in two keys
   (user, 2026-11-21: "faster to delete it and make it again"). **(preference)**
2. **One write at a time.** While a PUT runs, every Remove button, the Add button and Enter do
   nothing (`aria-disabled="true"`), and the new-tag input is `readonly`; during a removal the +
   is `aria-disabled` too.
3. **The list sent** is the field's own list (as latched below) with the new tag appended, or with
   the removed tag taken out, order kept. **The field's list is latched as in brief 12 decision 7**:
   it starts as `instanceTags` of the read that opened the page and changes only by this field's
   own 204s; polls never move a chip (a chip disappearing under the keyboard would drop focus).
   After each 204 the page asks a fresh list read (`store.fresh()`) as today, so the profile chips,
   the tiles and the suggestions follow.
4. **Two chip rows.** The instance's own tags are removable chips; the profile's tags (those of the
   read's `tags` that are neither in its `instanceTags` nor in the field's list) are fixed chips in
   a second row under the text "From the server profile", shown only when there are any. A tag in
   both lists shows once, as the instance's own (brief 12 decision 4).
5. **Removal is a × button inside each own chip**, a native button named "Remove tag gpu". No
   Delete or Backspace shortcut on top: Enter and Space on the button already do it, one tab stop
   per chip, nothing to discover **(preference)**. After a removal's 204, focus goes to the next
   own chip's Remove button, else the previous one's, else the + button
   ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)); the status line says "Tag gpu removed."
6. **The new-tag input** (the "oval"): the + is replaced in place by a pill holding a text input
   (visually hidden label "New tag", focused on opening, empty), an **Add** button (check icon) and
   a **Cancel** button (× icon). Enter or Add confirms; Escape (when no suggestion list is shown)
   or Cancel closes it with no request and puts focus back on the +. **Blur**: focus leaving the
   pill (and its suggestion list) with an empty input closes it; with text it stays open, text kept,
   nothing sent: a typed label is never thrown away nor saved by a stray click **(preference)**.
   The input is trimmed of spaces at both ends before anything else; an empty result sends nothing
   and Add is `aria-disabled`. No other client change: `Fast` is sent as `Fast` and the manager
   refuses it.
7. **One client check: a tag the instance already shows.** A trimmed text equal to one of the own
   or profile chips is refused under the input ("gpu is already a tag of this instance."), nothing
   sent: the manager's message for a repeat lists the whole rule and would not say which tag
   ([3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion)). Everything else (too long, bad
   characters, a ninth tag) is the manager's refusal under the input, unchanged text. The fe keeps
   no copy of the limits.
8. **After an add's 204** the new chip is appended after the last own chip, the pill closes, the +
   is back after it and takes focus, so a second tag is one more click or Enter away (the user's
   "another + is added"); status "Tag fast added."
9. **Unsaved changes (U2)** are now only a pill holding text (trimmed, non-empty): leaving asks, as
   for the Server form. Chips are always saved, so they never make leaving ask.
10. **Suggestions (C2) follow the APG combobox with a list popup, no automatic selection**
    ([APG Combobox](https://www.w3.org/WAI/ARIA/apg/patterns/combobox/),
    [ARIA 1.2 combobox](https://www.w3.org/TR/wai-aria-1.2/#combobox)): the input is the combobox,
    the list a `listbox` in the page's flow right under the chip row (not floating: nothing to clip
    at 320 px, nothing to cover a focused control, [2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)).
    The labels: every `tags` and `instanceTags` entry of the page's latest `GET /api/instances` and
    every `tags` entry of `GET /api/profiles/server`, read once at each opening of the pill (the
    profile list is not polled); minus the tags this instance shows; once each; sorted by code
    point; those containing the typed text, compared lower-cased; at most 8. A failed or pending
    profiles read is silent: the instances' labels are suggested meanwhile; suggestions are a help,
    never an error. Choosing a suggestion (Enter on the active one, or a click) adds it at once,
    exactly as typing it and pressing Enter.

## 2. The texts

| Key | English | Change |
|---|---|---|
| `instance-tags-own` | `Instance tags` | kept (now the group's legend) |
| `instance-tags-hint` | `Labels you give this instance to tell it apart; they change nothing in how it runs or which requests reach it. Add one with the + button, using a-z, 0-9 and -; each change is saved at once. The tags of its server profile are shown below and are not changed here.` | text changed |
| `instance-tags-from-profile` | `From the server profile` | new |
| `instance-tag-add` | `Add tag` | new (the +, visually hidden text) |
| `instance-tag-new` | `New tag` | new (the input's visually hidden label) |
| `instance-tag-confirm` | `Add` | new (check icon, visually hidden text) |
| `instance-tag-cancel` | `Cancel` | new (× icon, visually hidden text) |
| `instance-tag-remove` | `Remove tag {0}` | new (× icon, visually hidden text) |
| `instance-tag-shown` | `{0} is already a tag of this instance.` | new |
| `instance-tags-saving` | `Saving tags…` | kept |
| `instance-tag-added` | `Tag {0} added.` | new |
| `instance-tag-removed` | `Tag {0} removed.` | new |
| `instance-tag-suggestions` | `Used tags` | new (C2: the listbox's name) |

Removed: `instance-tags-save`, `instance-tags-saved`, `instance-tags-profile`. `{0}` is the tag as
typed or chosen. `api-server-bad-tags` and the failure keys of brief 12 T1.10 are reused unchanged.
`instance-tag-suggestions` comes with C2; C1 adds the other new keys.

## 3. Layout

In `InstancePage.tsx`, where `TagsField` is, after Start at boot (fe/DESIGN.md 5.2, **Chip**):

```
fieldset (legend "Instance tags" + About toggle)
  div.chips-row (flex, wrap, gap --space-2)
    ul.chips role="list"          (only when the own list is not empty)
      li.chip  "v2"  [button.chip-remove x "Remove tag v2"]
      li.chip  "gpu" [button.chip-remove x "Remove tag gpu"]
    button.tag-add (+, "Add tag")   OR   div.tag-new (pill: input "New tag", button Add, button Cancel)
  ul.tag-options role="listbox" "Used tags"   (C2; hidden while collapsed)
  p.field-error                    (the input's error, only with text)
  p.tags-profile "From the server profile"     (only with profile-only tags)
  ul.chips.chips-profile role="list" aria-labelledby that p
    li.chip "edge"                 (no button)
p role="status" .tags-status / p role="alert" .tags-alert / button Sign in .tags-sign-in
```

- **Chip**: a pill (`--radius-pill`), at least 32 px high (`min-height`, grows with text), padding
  inline `--space-3` (`--space-1` after the ×), `--field` fill, `--border-w` solid `--control`
  border, `--text` in the body font at 400. The tag text wraps anywhere (`overflow-wrap: anywhere`,
  `min-width: 0`, the chip `max-width: 100%`), so a 32-character tag stays inside 320 px.
- **Remove** (`.chip-remove`): a 24 by 24 px circle inside the chip at its end, no border or fill,
  icon `x` in the chip's text colour; the 24 px target meets
  [2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum) with no other target in its circle
  (the chips' gap is `--space-2`).
- **Fixed chip** (`.chips-profile .chip`): same pill, `--surface` fill, a transparent `--border-w`
  border (the edge forced colours show), no button. Told from an own chip by having no ×, its own
  labelled row and its fill, never by colour alone ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)).
- **+** (`.tag-add`): a 32 by 32 px circle, `--bg` fill, `--border-w` solid `--control` border, icon
  `plus` in `--text`; `aria-disabled` drawn as the Inactive button.
- **New-tag pill** (`.tag-new`): an inline flex pill 40 px high (`--control-h`), `--field` fill,
  `--border-w` solid `--control` border; inside, the input with no border of its own, about 16
  characters wide, `min-width: 0`, shrinking to fit 320 px; then Add (`check`) and Cancel (`x`) as
  32 px circles with no border. The pill wraps onto its own line in the chip row when it does not
  fit. The focus ring of 5.1 is on each focused control and nothing clips it.
- **Suggestions** (`.tag-options`, C2): a block list under the row, full width of the field, `--bg`
  fill, `--border-w` solid `--control` border, `--radius`; each option at least 32 px high, padding
  `--space-2 --space-3`, `--text`; the active option carries the focus ring drawn inset
  (`outline: var(--ring-w) solid var(--accent); outline-offset: calc(-1 * var(--ring-w))`): a shape,
  not a colour change. Hidden (`hidden`) while collapsed.
- **Icons**: `plus`, `x` and `check` join `Icon.tsx` (path data of `lucide-static` 1.74.0, as the
  others). These are the first icon-only buttons: each still holds its words, in a visually hidden
  span, the icon `aria-hidden`.
- **Classes** (amends `10 R1.8`'s freeze in `fe/src/Reflow.test.ts`): gains `chips-row`, `chips`,
  `chip`, `chip-remove`, `chips-profile`, `tag-add`, `tag-new` (C1) and `tag-options` (C2); loses
  `tags-save`. `tags-profile`, `tags-status`, `tags-alert`, `tags-sign-in` stay.

## 4. Slice C1: chips, adding, removing

Fixture as brief 12: `GET /api/instances` answers `[{"id": "default", "engine": "llama.cpp",
"state": "active", "port": 8080, "tags": ["edge", "v2", "gpu"], "instanceTags": ["v2", "gpu"]}]`;
boot `{"enabled": true}`; `GET /api/profiles/server` answers `[]` (C2 reads it; C1 tests ignore it).
Suggested file: `InstanceTagChips.test.tsx`, replacing the instance-page tests of
`InstanceTags.test.tsx` (section 6).

1. **15 C1.1** `fe/src/strings/en_US.json` has the C1 keys of section 2 with exactly their text and
   no longer has `instance-tags-save`, `instance-tags-saved`, `instance-tags-profile`.
2. **15 C1.2** The page shows a group named `Instance tags` holding a list with the items `v2` and
   `gpu` in that order, each with a button `Remove tag v2` / `Remove tag gpu`, then a button
   `Add tag`; no textbox and no `Save tags` button. After the group's chips, the text
   `From the server profile` and a list named by it with the one item `edge` and no button in it.
   With `tags` equal to `instanceTags` there is no such text or list; with `instanceTags` `[]` and
   `tags` `[]`, the group holds only `Add tag`. Tab order: Start at boot, About Instance tags,
   Remove tag v2, Remove tag gpu, Add tag, then the Server section.
3. **15 C1.3** Clicking `Add tag` replaces it with a textbox named `New tag`, empty and focused,
   followed by buttons `Add` (with `aria-disabled="true"` while the box is empty or only spaces)
   and `Cancel`. Escape in the box, or Cancel, closes it: `Add tag` is back and focused. Moving
   focus out of an empty box closes it too; out of a box holding `fast`, it stays open holding
   `fast`. None of these sends a request.
4. **15 C1.4** Typing ` fast ` and pressing Enter sends exactly one `PUT /api/instances/default/tags`
   with `Content-Type: application/json` and body `{"tags": ["v2", "gpu", "fast"]}`; pressing Add
   sends the same; Enter or Add with an empty or blank box sends nothing; `Fast` is sent as
   `Fast`. Typing `gpu`, or `edge`, and Enter sends nothing and shows under the box
   `gpu is already a tag of this instance.` (or with `edge`), the box `aria-invalid="true"` and
   described by that text, focus on the box.
5. **15 C1.5** While a PUT is pending: the status line reads `Saving tags…`; the box is `readonly`
   and keeps focus; `Add`, `Remove tag v2` and `Remove tag gpu` have `aria-disabled="true"`; a
   second Enter, Add or Remove click sends no request.
6. **15 C1.6** On 204 for `fast`: the list holds `v2`, `gpu`, `fast` in that order, the box is gone,
   `Add tag` is back and focused, the status line reads `Tag fast added.`, and a new
   `GET /api/instances` is sent after the PUT.
7. **15 C1.7** On 422 `{"key": "server-bad-tags", "args": ["bad characters", "v2,gpu,Fast", "8",
   "1", "32"], "field": "tags"}`: `t('api-server-bad-tags', ...args)` shows under the box, which has
   `aria-invalid="true"`, `aria-describedby` naming that text and focus, and still holds `Fast`; the
   status line is empty and no `role="alert"` renders; one keystroke in the box removes the error
   and `aria-invalid`.
8. **15 C1.8** Removing: clicking `Remove tag v2` sends one PUT with body `{"tags": ["gpu"]}`; while
   it is pending `v2` is still listed and focus stays on its button. On 204, `v2` is gone, focus is
   on `Remove tag gpu`, the status line reads `Tag v2 removed.` and a new `GET /api/instances` is
   sent. Removing `gpu` (the last) moves focus to `Remove tag v2`; removing the only chip moves it
   to `Add tag`.
9. **15 C1.9** Each other answer, to an add and to a removal, shows in the field's `role="alert"`
   exactly the text of brief 12 T1.10 (401 `session-ended-retry` with a `Sign in` button; 403
   `session-not-administrator` `session-revoked` with `Sign in`; 404 `instance-not-found` with
   `default`; 503 `api-client-unavailable`; 403 `client-not-authorized` `api-client-not-authorized`;
   422 unknown key `x-y` `api-other` with `x-y`; 400, 502, a body that is not JSON and a network
   failure `request-failed`); a 422 `server-bad-tags` to a removal (no box to put it under) shows
   `t('api-server-bad-tags', ...args)` there too. The status line is empty; after an add the box is
   still open with its text; after a removal the chip is still listed and focus is on its Remove
   button.
10. **15 C1.10** After a 401, `Sign in` opens the sign-in dialog; a sign-in that keeps the page
    closes it, removes the alert and puts focus on the `New tag` box when it is open (its text
    kept), else on `Add tag`; nothing is resent. Cancel puts focus back on `Sign in`.
11. **15 C1.11** The page's polls never change the chips or the box: after a poll answers
    `instanceTags` `["other"]` and `tags` `["edge", "other"]`, the list still holds `v2`, `gpu`,
    and an open box still holds what was typed.
12. **15 C1.12** Leaving: with the box open holding `new-tag`, following a nav link (and Back) opens
    the Unsaved changes dialog and a `beforeunload` is cancelled; with the box empty or closed, and
    after any add or removal's 204, leaving asks nothing. The chips never change the Server save
    bar's status or buttons.

## 5. Slice C2: suggestions

Fixture: `GET /api/instances` answers the C1 item plus `{"id": "b", "engine": "llama.cpp", "state":
"inactive", "port": 8081, "tags": ["fast", "gpu"], "instanceTags": ["fast"]}`;
`GET /api/profiles/server` answers `[{"id": "default", "engine": "llama.cpp", "port": 8080, "tags":
["edge"], "instance": "default"}, {"id": "big", "engine": "llama.cpp", "port": 8090, "tags": ["arm",
"zz-top"], "instance": null}]`. The instance `default` shows `v2`, `gpu`, `edge`, so the suggestions
are `arm`, `fast`, `zz-top`. Suggested file: `InstanceTagSuggest.test.tsx`.

1. **15 C2.1** `fe/src/strings/en_US.json` has `instance-tag-suggestions` = `Used tags`. Each click
   of `Add tag` sends exactly one `GET /api/profiles/server`; nothing else sends it.
2. **15 C2.2** The `New tag` box has role `combobox`, `aria-autocomplete="list"`, `aria-controls`
   naming a `listbox` named `Used tags`, and `aria-expanded="false"` on opening, the listbox hidden.
   Typing `a` shows it (`aria-expanded="true"`) with exactly the options `arm`, `fast` in that
   order; typing `A` instead shows the same; typing `q` (no match) hides it and sets
   `aria-expanded="false"`. In an empty box, ArrowDown shows all three, `arm`, `fast`, `zz-top`.
3. **15 C2.3** With the listbox shown and no option active, ArrowDown makes `arm` active: the box's
   `aria-activedescendant` is that option's id and the option has `aria-selected="true"`, focus
   staying in the box; ArrowDown moves to the next, from the last back to the first; ArrowUp moves
   back, from the first to the last. Typing a character leaves no option active (no
   `aria-activedescendant`).
4. **15 C2.4** Enter with `fast` active sends one PUT with body `{"tags": ["v2", "gpu", "fast"]}`,
   whatever the box holds; clicking the option `zz-top` sends `{"tags": ["v2", "gpu", "zz-top"]}`;
   each then behaves as 15 C1.5 and 15 C1.6. Enter with no option active adds the box's text, as
   15 C1.4.
5. **15 C2.5** Escape with the listbox shown hides it and keeps the box open with its text; a second
   Escape closes the box as 15 C1.3. Tab from the box moves focus to `Add` without adding anything
   and hides the listbox.
6. **15 C2.6** When `GET /api/profiles/server` answers 401, 503, a body that is not JSON or a network
   failure, or has not answered yet, typing `a` shows only `fast` (from the instances), and no alert
   or status text appears.
7. **15 C2.7** No more than 8 options show: with the profiles read answering one row of tags `t0`
   to `t7` plus `arm`, an empty box and ArrowDown show 8 options, `arm` then `t0` to `t6`.

## 6. Older tests that change

- `fe/src/InstanceTags.test.tsx`: the instance-page criteria of brief 12, **T1.1** (the nine keys)
  and **T1.5 to T1.14**, are replaced by C1 and are deleted with it (T1.1's port and tile keys stay
  pinned: `instance-port`, `instance-tags`, `instance-tags-own`, `instance-tags-saving`,
  `api-server-bad-tags`). The tile criteria **T1.2 to T1.4** and `InstanceTiles.test.tsx` stay.
- `fe/src/Reflow.test.ts` **10 R1.8**: the class list as section 3 amends it.
- `fe/src/strings.test.ts` keeps every key used; the three removed keys must leave the file.
- `fe/ux/stub.py`: the coder adds `GET /api/profiles/server` (C2) answering one row with tags, so the
  UX check can open the pill on the instance page.

## 7. Accessibility (WCAG 2.2 AA)

- **Names** ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value),
  [1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships)): the group by its legend
  "Instance tags"; each own chip a list item whose text is the tag, its button "Remove tag v2"; the
  fixed chips a list named "From the server profile"; the + "Add tag"; the input "New tag" (a label
  is present, visually hidden, [3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions)); Add,
  Cancel. Icon-only buttons have no visible text, so 2.5.3 asks nothing of them; their words are the
  visually hidden span. A screen reader hears "Instance tags, group, list, 2 items, v2, Remove tag
  v2, button, ...".
- **Keyboard** ([2.1.1](https://www.w3.org/TR/WCAG22/#keyboard),
  [2.1.2](https://www.w3.org/TR/WCAG22/#no-keyboard-trap)): every action is a native button or the
  input's Enter and Escape; the arrows move only inside an open listbox; Tab always leaves. No
  single-character shortcut ([2.1.4](https://www.w3.org/TR/WCAG22/#character-key-shortcuts)).
- **Focus never drops** ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)): buttons turning inactive
  use `aria-disabled`; after open, cancel, add and remove, focus lands where decisions 5, 6 and 8
  say.
- **Announcements** ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)): Saving, Tag added, Tag
  removed in the status line; failures in the alert; a refusal is the input's error, focused with it
  ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification)). The suggestions' count is not
  announced: the expanded state and the active option are (APG).
- **Targets** ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)): + 32 px, Add and Cancel
  32 px, Remove 24 px with no other target in its circle, options 32 px high.
- **Reflow and text size** ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow),
  [1.4.4](https://www.w3.org/TR/WCAG22/#resize-text), [1.4.12](https://www.w3.org/TR/WCAG22/#text-spacing)):
  chips wrap; a 32-character tag wraps inside its chip; the pill shrinks to the row; heights are
  minimums. Checked by the designer with the UX check at 320 px, 200 % text, light and dark, with
  8 tags of 32 characters and the pill open.
- **Contrast**: every pair is already in the table of 5.1 (`--text` on `--field`, `--bg`,
  `--surface`; `--control` borders; the `--accent` ring). Forced colours: the chips' borders and
  the ring stay; the fixed chip shows its transparent border as an edge.

## 8. Out of scope

- Editing the server profile's tags (later, on its own page: brief 12 U1).
- Tags on the Overview's tiles (unchanged, text joined by ", ").
- Reordering chips, editing a chip, an undo, a removal confirmation.
- Creating a tag from a paste of several (a comma or space is just a character the manager refuses).
- The GUI and `pai`.

## 9. Choices taken while the user was away (recommended answers)

- **Q1** Save at once on each add and removal, no Save tags button (decision 1).
- **Q2** Removal by a × in each chip, no Delete/Backspace shortcut, no confirmation (decision 5).
- **Q3** Blur keeps a pill that holds text, closes an empty one; Escape and Cancel close (decision 6).
- **Q4** The only client check is "already a tag of this instance"; the limits stay the manager's
  (decision 7).
- **Q5** Suggestions: contains the typed text, case-insensitive, sorted, at most 8, the server
  profiles read at each opening, its failure silent; choosing one adds it at once (decision 10).
- **Q6** Profile tags as fixed chips in their own labelled row, not mixed into the own row
  (decision 4).

## 10. C1 review rulings (designer, 2026-11-21)

- **R1, `.chip-remove`**: accepted as `.chip button` (only an own chip has a button). The class list
  of section 3 loses `chip-remove`; C1 adds six classes. `fe/src/Reflow.test.ts` R1.8's comment
  still says seven with `chip-remove`: it says six.
- **R2, sizes**: not accepted at the page's control height. The global `button { min-height:
  var(--control-h) }` makes each x and the + 40 px high (chip 50 px, + a 24 by 40 oval). `.chip`
  and `.tag-new` keep no min-height of their own (they grow from their content); `HearthTokens.test.ts`
  H1a.12's min-size list gains exactly these, as section 3 amended R1.8: `.chip button: min-height:
  24px`, `.tag-add: min-height: 32px`, `.tag-add: min-width: 32px`, `.tag-new button: min-height:
  32px`, `.tag-new button: min-width: 32px`, and the flex children that must shrink (R3)
  `min-width: 0`.
- **R3, reflow** ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)), amended for the user's rule
  (2026-11-21: "very long tags are shortened with an ellipsis; avoid wrapping if possible; if really
  not possible, wrap"):
  - Every chip's text (own and profile) is in a `span`: `.chip > span { min-width: 0; overflow:
    hidden; white-space: nowrap; text-overflow: ellipsis }` (an ellipsis needs a block box of its
    own; the profile chip's bare text gains the span). No `overflow-wrap` on `.chip` itself.
  - The cap is the row: `.chip { max-width: 100% }` (kept), no `ch` cap. A 32-character tag is
    about 320 px and fits whole wherever the row is wider; it is cut only where it cannot fit
    (320 px: 2 or 3 characters; 320 px at 200 %: about 12 still shown).
  - No smaller font step **(preference)**: `--fs-md` would save about 2 characters of 32, and a
    size that changes with the width needs a query of its own; the body size stays.
  - Last resort, wrapping: `@media (max-width: 15em) { .chip > span { white-space: normal;
    overflow-wrap: anywhere } }`. A media query's `em` is the browser's own text size, not the
    page's ([Media Queries 4, units](https://www.w3.org/TR/mediaqueries-4/#units)), so it fires
    only when the user's text setting makes the viewport under 15 text widths (320 px at 200 %
    text is 10em), where an ellipsis would leave about a dozen characters. The UX check's injected
    `html { font-size: 200% }` does not trigger it: there the ellipsis shows, also with no
    sideways scroll.
  - The full tag stays available: the cut is visual only, so the list item's text and the x's name
    "Remove tag `<full>`" carry it to screen readers; sighted users get the expanded pill (R5). No
    `title` (it would duplicate R5 and shows to the mouse only).
  - The pill (defect 2) as before: `.tag-new` and its input `min-width: 0`, the pill
    `max-width: 100%`, the input `flex: 0 1 16ch` or equal. Seen before the fix: 15 px sideways at
    320 px with 32-character tags, 152 px with the pill open at 200 %.
  - Pins for the tester: R1.1's overflow-wrap list gains `.chip > span` inside the `15em` block
    only (not `.chip`); H1a.12's min-size list gains `.chip > span: min-width: 0`,
    `.tag-new: min-width: 0`, `.tag-new input: min-width: 0` beside R2's five; the `.chip > span`
    top-level rule declares exactly the four above; one new `@media (max-width: 15em)` block
    holding only that rule; no `title` on any chip.
- **R4, rows**: the group is `.field-control`, a wrapping flex row, so "From the server profile",
  its chips and the input's error sit beside the chips at 1280 px. Each of the three takes a row of
  its own under the chips row (`flex-basis: 100%`, as `.boot-alert` does), as section 3 draws.
- **R5, the expanded pill** (user, 2026-11-21: "when the cursor passes over it, a temporary big
  pill opens to show the whole text"), meeting [1.4.13 Content on Hover or Focus](https://www.w3.org/TR/WCAG22/#content-on-hover-or-focus):
  - **When**: only for a chip whose text is cut, measured at the moment of hover or focus
    (`span.scrollWidth > span.clientWidth`); a whole tag opens nothing. Opened by the pointer
    entering the `li.chip` and by focus entering it (the x of an own chip; a profile chip is
    focusable, `tabindex="0"`, so the keyboard reaches its text: profile chips follow `Add tag` in
    the tab order, amending C1.2).
  - **What**: a `span.chip-full` inside the `li`, `aria-hidden="true"` (the list item already reads
    the full tag), holding the full tag. Over the chip, not in the flow: `.chip { position:
    relative }`; `.chip-full { position: absolute; top: 0; inset-inline-start: 0; z-index: 1;
    min-height: 100%; white-space: normal; overflow-wrap: anywhere }`, the chip's padding, border
    and fill (`--field`; `--surface` with `--control` border in the profile row so it reads as
    raised), `--radius` (it can be several lines), the same font. Its width is the chip's: on an
    own chip `inset-inline-end` leaves the x uncovered (`calc(24px + var(--space-1))`), on a profile
    chip `0`. A cut chip already spans the row, so the pill stays inside the viewport and wraps
    downward over what follows; nothing moves.
  - **Hoverable and persistent**: being inside the `li`, the pointer can move onto it; it closes when
    both the pointer and focus have left the `li`. **Dismissible**: Escape (listened on the document
    while it shows) hides it with focus unchanged; it opens again only on a new entry of pointer or
    focus.
  - **Criteria (jsdom, the tester stubs `scrollWidth`/`clientWidth` on the span)**:
    **15 C1.13** with the span cut, `mouseenter` on `li` shows `.chip-full` (`aria-hidden="true"`,
    text the full tag); `mouseleave` hides it; with the span whole, nothing shows.
    **15 C1.14** focus on `Remove tag <tag>` (cut) shows it, and so does focus on a cut profile
    chip; focus leaving the `li` hides it; Escape hides it, `document.activeElement` unchanged,
    and it stays hidden until a new focus or `mouseenter`.
    **15 C1.15** profile chips have `tabindex="0"` and follow `Add tag` in Tab order; no chip has a
    `title`; the class list (R1.8) gains `chip-full`; R1.1's overflow-wrap list gains `.chip-full`;
    the `.chip-full` rule declares the properties above.
  - **UX check only** (CSS layout jsdom cannot see): at 320 px and 200 % text the pill stays inside
    the viewport, adds no sideways scroll, covers without moving content, leaves the x clickable,
    and its text meets the contrast table of 5.1.
- **R6, React patterns for C1** (fe/DESIGN.md 5.3, user 2026-11-21):
  - **Optimistic chips** (`useOptimistic` over the latched list): on an add, the new chip is listed
    at once, while the pill stays open, read-only, with its text until the answer; on a removal,
    the chip leaves at once and focus moves at once as decision 5 says. On 204 the latched list
    takes the change (no visible step); on any refusal the chip comes back or leaves again,
    restored from the latched list: an add keeps the pill and its text with the error or alert as
    C1.7 and C1.9; a removal puts focus back on the restored chip's x, with the alert. Still one
    write at a time (C1.5), pending drawn as today; `isPending` of `useTransition` may replace the
    `saving` flag (the coder's call; the tests see only the DOM).
  - This amends **C1.5** (while an add is pending the list already holds the new chip), **C1.8**
    (while a removal is pending the chip is gone and focus is on the next x, else the previous,
    else `Add tag`) and **C1.9** (after a refused removal the chip is back in its place, focus on
    its x). No new criterion; C1 stays at 15 (C1.1 to C1.12 plus R5's three), tested in two rounds
    if the tester needs: C1.1 to C1.12, then C1.13 to C1.15.
  - **No portal for the hover pill** (R5 stands): no ancestor of the tags field clips or stacks
    (`index.css` sets `overflow: hidden` only on `.visually-hidden` and `.session > span`, and
    scrolls only `.table-scroll` and the jump list). A portal would need positioning by script from
    the chip's rectangle, kept in step with scroll and resize, for nothing.
