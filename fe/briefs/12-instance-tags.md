# Brief 12: an instance's port and tags (slice T1)

`PLAN.md` step 10 item 13 (user, 2026-11-17): each instance shows "its engine, its port and its tags,
one or more labels the administrator assigns to it"; `ARCHITECTURE.md` (c) and line ~955: tags are
labels only, the router routes by model name, the Overview shows port and tags. This slice shows the
port and the tags on the Overview's tiles and on the instance page, and lets the administrator edit
the instance's own tags there. Criteria are numbered **12 T1.n**; test names cite them so
(`12 T1.4`). One slice, 14 criteria.

## 0. What exists (read on main at e85f188, with the bff's working tree of slices 11 and 12)

- **Two lists of tags.** A server profile's tags are `[Server] Tags = a, b` in its server file; an
  instance's own are `Tags` in its section of `instances.ini` (`ARCHITECTURE.md` (b) ~1459,
  `crates/core/src/client.rs` `InstanceInfo`: `profile_tags`, `instance_tags`). What an instance
  shows is the union: the profile's first, then its own, a tag in both kept once at its first place
  (`crates/bff/tests/instances.rs`,
  `the_rows_carry_the_tags_of_the_profile_then_their_own_none_twice`).
- **The rule** (`crates/core/src/checks.rs` `tags_rule`): 0 to 8 tags, each 1 to 32 characters of
  `a-z`, `0-9`, `-`, none twice; refused as `server-bad-tags` with args
  `[reason, list joined by ",", most tags, least chars, most chars]`, the reason an English phrase
  from the code (`too many tags`, `empty tag`, `too long`, `bad characters`, `the same tag twice`).
  No space can be in a tag, so a space can separate tags in an input.
- **`GET /api/instances`** (bff slices 11, 12, being built): each item gains `port` (number, the
  configured port of its server profile; 0 when the profile file is missing) and `tags` (the
  union). **This brief also needs `instanceTags`** (the instance's own list, for the editor), which
  `ARCHITECTURE.md` (f') ~1404 designs but the bff's tests do not yet answer: see question C1.
- **`PUT /api/instances/{id}/tags`** `{"tags": [string]}` (bff slice 12, being built; the tests in
  `crates/bff/tests/instances.rs`): 204 no body; 422 `{"key": "server-bad-tags", "args": [...],
  "field": "tags"}`; 404 `client-no-instance`; 400 `tags-malformed` (the fe never sends one); 403
  `session-not-administrator` with the session ended; 503 `client-unavailable`; 401
  `session-required`. It writes only the instance's own list; `[]` clears it.
- **The profile's tags** travel in the server document (`Tags` in `values` of `GET .../server`),
  and the bff keeps the file's tags when `Tags` is absent from a `PUT .../server` body
  (`ARCHITECTURE.md` (c)). The fe's Server form sends a fixed key set without `Tags`
  (`fe/src/ServerForm.tsx` `toValues`), so a server save keeps them. Editing them is out of scope.
- **`pai-config` and `pai`** have no tags yet (no `Tags` in `crates/config/` or `crates/cli/`), so
  the web UI is first; the only shared text is the refusal `server-bad-tags`
  (`crates/core/strings/en_US.toml:1023`), copied verbatim (`fe/DESIGN.md` section 7). The rule
  already states what is allowed, so no next-step sentence is appended.
- **Today's surfaces.** Tiles: `fe/src/Instances.tsx` `Tile`, a `dl.tile-facts` with the one fact
  Start at boot (`fe/DESIGN.md` 5.2 "Tile"). Instance page header: `fe/src/InstancePage.tsx`, engine
  line, state line, run actions, Start at boot (`Boot`, an immediate write outside the Server form,
  with the Action feedback pattern and its Sign in dialog).

## 1. Decisions

1. **Port and tags come with the list**, so they show at once, with no read of their own and no
   pending state. On the tile they are two more facts of `dl.tile-facts`, before Start at boot:
   **Port**, **Tags**, **Start at boot**. On the instance page the port is a line under the engine
   line, the tags a field after Start at boot.
2. **A fact with nothing to say has no row**, as Start at boot already does: no Port row when
   `port` is 0 or absent, no Tags row when the union is empty. A tile with no fact at all has no
   `dl`.
3. **Tags as text, joined by ", "**, the form of the file itself (`Tags = coding, fast`), not chips
   **(preference)**: no new component, it wraps at the spaces, and a long tag breaks by the body's
   `overflow-wrap: break-word` ([1.4.10 Reflow](https://www.w3.org/TR/WCAG22/#reflow)). The
   separator is the data's format, so it is not a strings key; the tags are data and never
   translated.
4. **Only the instance's own tags are edited, and the page says which are which.** The tile shows
   the union, unlabelled: a glance, and tags are labels only. The instance page shows a text input
   **Instance tags** holding only `instanceTags`, and under it, when the profile brings any, the line
   "From the server profile: edge, v2": the tags of `tags` that are not in `instanceTags`, in their
   order. A tag in both lists is therefore shown once, in the input (the user owns that copy; after
   removing it, the next read shows it on the profile line). The profile's tags are not editable
   on the web in this slice.
5. **One input and a Save tags button, in a form of its own** **(preference over chips with add and
   remove buttons)**: fewest controls; Enter in the input saves (a native form submit, not a change
   of value, so [3.2.2 On Input](https://www.w3.org/TR/WCAG22/#on-input) holds). The input is split
   on commas and white space, empty pieces dropped, order, case and repeats kept as typed: the
   manager is the one judge of the rule, and its refusal says what to fix
   ([3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion)). No client check.
6. **Like Start at boot, it is outside the Server form**: its own route and file, the Action feedback
   pattern (status line, alert, Sign in). It never touches the Server save bar, but **unsaved tags
   are the page's unsaved changes** (user, 2026-11-20, U2): leaving asks first, as for the Server
   form. The tags are unsaved while the input, split as in decision 5, differs from the list the
   page last read at opening or last saved with a 204 (so `v2,gpu` equals `v2, gpu`).
7. **The input is the user's.** It is filled from the first read of the page and never rewritten by
   a poll or by the answer of a save; only the profile line, the port and the tile follow the reads.

## 2. The texts

| Key | English |
|---|---|
| `instance-port` | `Port {0}` |
| `instance-tags` | `Tags` |
| `instance-tags-own` | `Instance tags` |
| `instance-tags-hint` | `Labels you give this instance to tell it apart; they change nothing in how it runs or which requests reach it. Separate them with commas or spaces, using a-z, 0-9 and -. The tags of its server profile are shown with them and are not changed here.` |
| `instance-tags-profile` | `From the server profile: {0}` |
| `instance-tags-save` | `Save tags` |
| `instance-tags-saving` | `Saving tags…` |
| `instance-tags-saved` | `Tags saved.` |
| `api-server-bad-tags` | `{0}: the tags {1} are not a list of at most {2} tags, each of {3} to {4} characters of a-z, 0-9 and -, none twice` |

The tile's Port row reuses `server-port` ("Port") as its `dt`, the Tags row `instance-tags`.
`instance-port` fills `{0}` with the number as sent (`8080`, no thousands separator).
`instance-tags-profile` fills `{0}` with the profile's tags joined by ", ". `api-server-bad-tags` is
the GUI's text verbatim, its args by position as the bff sends them. The failure texts reuse
existing keys (criterion 10). No other key is added or changed.

## 3. Layout

- **Tile** (`Instances.tsx` `Tile`): `dl.tile-facts`, one `div` per fact in the order Port, Tags,
  Start at boot. Port and Tags render with the tile; Start at boot keeps its rule (row only once its
  read answered). Example: Port `8080`; Tags `edge, v2, gpu`; Start at boot `On`.
- **Instance page header** (`InstancePage.tsx`): after the engine line, a paragraph
  `t('instance-port', port)` with the engine line's style (`.instance-header .engine`: `--muted`),
  only when the port is above 0. After Start at boot, a `form` (no `action`, submit handled by the
  page) holding: the text field **Instance tags** (the existing `TextField` look: label, About hint
  toggle, input, error), then the profile line as a paragraph in `--muted` when it has tags, then a
  secondary button **Save tags** (`type="submit"`), then the Action feedback of `fe/DESIGN.md` 5.2:
  status line (`role="status"`, always in the DOM), alert (`role="alert"`, only with text), and Sign
  in after a 401 or a 403 `session-not-administrator`.
- **320 px**: the input takes the control column's width, the button sits under it; nothing new
  needs a style beyond reusing these. Tags of 32 characters and 8 tags wrap inside the tile and the
  profile line.
- **Classes (T1 review, 2026-11-20).** The spacing does need rules: the stylesheet may gain exactly
  `port` (with `.instance-header .engine`), `tags-profile`, `tags-save`, `tags-status`, `tags-alert`
  and `tags-sign-in`, margins and the alert's `--danger` 600 only, `.tags-status` with no
  `min-height` (as `.boot-status`); `10 R1.8`'s class list is a freeze that a brief amends, so it
  gains these six. `.tile-facts dd` gains `overflow-wrap: anywhere`: a flex item keeps its longest
  word as its minimum width under `break-word`, and a 32-character tag scrolled the Overview 18 px
  sideways at 320 px (decision 3 was wrong on this).

## 4. States and behaviour

| State | What shows |
|---|---|
| Loading (list) | as today: the page's `loading`; the tile list renders nothing until it answers |
| No port / no tags | no Port row, no Tags row, no port line, no profile line; the input empty |
| Saving | status `instance-tags-saving`; Save tags `aria-disabled="true"`; a second Save or Enter sends nothing; the input stays editable |
| Saved (204) | status `instance-tags-saved`; the page asks a fresh read of the instances (`store.fresh()`), so the profile line, the nav and the Overview follow; the input keeps its text |
| Refused (422 `server-bad-tags`, field `tags`) | the field's error under the input, the input `aria-invalid="true"` and described by it, focus moved to the input after the error renders; status empty, no alert |
| Other failure | the alert, by key (criterion 10); the input keeps its text; after a 401 or a revoked session, Sign in |

- **Errors that clear.** An edit of the input removes its error and `aria-invalid` (the Field rule of
  `fe/DESIGN.md` 5.2). A new save clears the status, the alert and the error first. A sign-in that
  keeps the page removes a session alert, as for Start at boot.
- **The read's shape is forgiving.** An item whose `port` is absent or not a whole number from 0 to
  65535 is read as port 0; whose `tags` or `instanceTags` is absent or not an array of strings, as
  `[]`. Neither fails the list (the list's other rules are unchanged): the tags are extra facts, the
  bff's own rule too (`a_manager_that_answers_no_rows_shows_no_tags`).

## 5. Keyboard, focus and screen readers

- **Order** on the instance page: Start, Stop, Restart, (Sign in), Start at boot and its About,
  then About Instance tags, the input, Save tags, (Sign in), then the Server section. DOM order is
  the visual order ([2.4.3 Focus Order](https://www.w3.org/TR/WCAG22/#focus-order)).
- **Names**: the input is named by its visible label "Instance tags"
  ([2.5.3 Label in Name](https://www.w3.org/TR/WCAG22/#label-in-name),
  [4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)); the About toggle "About Instance tags";
  the button "Save tags". The profile line is plain text after the input, not part of its name;
  it is read in the flow ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships)).
- **Announcements**: the status line announces Saving and Saved; a failure is the alert
  ([4.1.3 Status Messages](https://www.w3.org/TR/WCAG22/#status-messages)); a refusal moves focus to
  the input, which announces its name, invalid state and the error that describes it
  ([3.3.1 Error Identification](https://www.w3.org/TR/WCAG22/#error-identification)).
- **Tiles**: nothing new takes focus; a screen reader hears, after the head, "Port, 8080", "Tags,
  edge, v2, gpu", then Start at boot, as term and definition. Rows that appear are not announced.
- **Focus after a sign-in** from this field's Sign in: back on Save tags, as Start at boot returns to
  its checkbox; Cancel returns it to Sign in.

## 6. Acceptance criteria

Fixture unless a criterion says otherwise: `GET /api/instances` answers
`[{"id": "default", "engine": "llama.cpp", "state": "active", "port": 8080, "tags": ["edge", "v2",
"gpu"], "instanceTags": ["v2", "gpu"]}]`; boot read `{"enabled": true}`. Suggested files:
`InstanceTags.test.tsx` (instance page) and `InstanceTiles.test.tsx` (Overview).

1. **12 T1.1** `fe/src/strings/en_US.json` has the nine keys of section 2 with exactly their text.
2. **12 T1.2** Overview: the tile's `dl.tile-facts` holds, in this order, the rows Port / `8080`,
   Tags / `edge, v2, gpu` and Start at boot / `On`; the Port and Tags rows are there while the boot
   read is still pending and after it fails.
3. **12 T1.3** Overview: with `port` 0, and with `port` absent, there is no Port row; with `tags`
   `[]`, and absent, no Tags row; with both missing and the boot read failed, the tile has no `dl`.
   An item with `"port": "8080"` or `"tags": "edge"` still shows the list (no `instances-failed`),
   without that row.
4. **12 T1.4** Instance page: the paragraph `Port 8080` follows the engine line; with `port` 0
   there is no such paragraph.
5. **12 T1.5** Instance page: a textbox named `Instance tags` holds `v2, gpu`; after it the text
   `From the server profile: edge`; with `tags` equal to `instanceTags`, no such text. The textbox
   comes after the Start at boot checkbox and before Save tags in the tab order, and outside the
   Server form (typing in it shows no Server unsaved-changes status).
6. **12 T1.6** Typing `v2,  gpu fast,,edge-1` and pressing Save tags sends exactly one
   `PUT /api/instances/default/tags` with `Content-Type: application/json` and body
   `{"tags": ["v2", "gpu", "fast", "edge-1"]}`; pressing Enter in the textbox sends the same; an
   emptied textbox sends `{"tags": []}`; `Fast` is sent as `Fast` (no client change or check).
7. **12 T1.7** While the PUT is pending: the status line reads `Saving tags…`, Save tags has
   `aria-disabled="true"`, and a second click or Enter sends no request.
8. **12 T1.8** On 204: the status line reads `Tags saved.`, a new `GET /api/instances` is sent
   after the PUT, the textbox keeps the text typed, and when that read answers `instanceTags`
   `["gpu"]` with `tags` `["edge", "v2", "gpu"]`, the profile line reads
   `From the server profile: edge, v2` while the textbox still shows what was typed.
9. **12 T1.9** On 422 `{"key": "server-bad-tags", "args": ["bad characters", "Fast", "8", "1",
   "32"], "field": "tags"}`: the text `t('api-server-bad-tags', ...args)` shows under the textbox,
   which has `aria-invalid="true"` and `aria-describedby` naming that text, focus is on the textbox,
   the status line is empty and no `role="alert"` is rendered for it; the textbox keeps its text;
   one keystroke in the textbox removes the error and `aria-invalid`.
10. **12 T1.10** Each other answer shows in the field's `role="alert"` exactly: 401 ->
    `session-ended-retry` and a `Sign in` button; 403 `session-not-administrator` ->
    `session-revoked` and a `Sign in` button; 404 `client-no-instance` -> `instance-not-found` with
    `default`; 503 `client-unavailable` -> `api-client-unavailable`; 403 `client-not-authorized` ->
    `api-client-not-authorized`; 422 with an unknown key `x-y` -> `api-other` with `x-y`; 400
    `tags-malformed`, 502 `client-other`, a body that is not JSON and a network failure ->
    `request-failed`. In each case the textbox keeps its text and the status line is empty.
11. **12 T1.11** After the 401 of criterion 10, Sign in opens the sign-in dialog; a sign-in that
    keeps the page closes it, removes the alert and puts focus on Save tags; Cancel puts focus back
    on Sign in.
12. **12 T1.12** The polls of the page (the 5 s read) never change the textbox: after typing
    `new-tag` and letting a poll answer `instanceTags` `["other"]`, the textbox still holds
    `new-tag`.

13. **12 T1.13** Unsaved tags guard the page: after typing `new-tag` in the textbox, following a nav
    link (and the browser's Back) opens the Unsaved changes dialog of the Server form
    (`fe/DESIGN.md` section 6), and a `beforeunload` event is cancelled; typing the text back to
    `v2,gpu` (same list as read) makes leaving ask nothing again.
14. **12 T1.14** After a 204 for `new-tag`, leaving asks nothing; after any failure of criteria 9
    and 10, it still asks. Unsaved tags never change the Server save bar's status or buttons.

**Checked by the designer, not the tester** (the UX check, `fe/ux/`, after the build): the Overview
and instance page at 320 px with a tile of 8 tags of 32 characters and port 65535 scroll nowhere
sideways, at 100 % and 200 % text, light and dark, axe clean. This needs the UX check's stub to
answer `port`, `tags` and `instanceTags` (the coder adds them to `fe/ux/`'s stub reply).

## 7. Out of scope

- Editing the server profile's tags: read-only for now, editable later on the server profile page
  of `#/profiles/server` (user, 2026-11-20, U1); the server save keeps them untouched (section 0).
- Collapsible instances, the host level in the nav, `serverProfile`, `removable`, creating instances.
- Filtering or routing by tags (labels only).
- The GUI and `pai` (no tags there yet).

## 8. Questions

- **C1** (answered, coordinator 2026-11-20): the bff sends `instanceTags` and `port` as
  `ARCHITECTURE.md` (f') designs.
- **C2** (pending, the architect): the key may become `instance-bad-tags` / `instance-malformed`;
  this brief keeps `server-bad-tags` / `tags-malformed` until told otherwise.
- **U1, U2** answered by the user on 2026-11-20 and folded in (section 7, decision 6, T1.13, T1.14).
