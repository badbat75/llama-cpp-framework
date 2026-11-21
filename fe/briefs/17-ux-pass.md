# Brief 17: the UX pass (slices U17a to U17g)

User direction (2026-11-21, translated): "In general use all the [React] features across the whole
GUI. At the moment it is very basic." This brief is the audit and the slices that follow from it. Build
them after C1 (brief 15) and before slice 17 of the plan, in the order of section 2. Each sub-slice has
at most 12 criteria, and test names cite them (`17a.3`). React features are a means, not a goal
(`fe/DESIGN.md` 5.3): a slice names the one it uses, or says none fits. Every accessibility rule of
`fe/DESIGN.md` stands.

## 0. How the audit was done

`npm --prefix fe run build`, then the UX stub (`fe/ux/stub.py`) driven by
`build/ux-review/ux17.js` in Chromium at 1280 px light and 320 px dark. Each page was shot settled and
while loading: every `/api/*` reply held 3 s, then every reply except `/api/session` held 3 s. The
shots are in `build/ux-review/ux17/`. The stub serves one model profile document, so the model
profile page was read in the code and in earlier shots (brief 14). Pages: sign-in, shell and nav,
Overview, instance page, Model profiles, model profile page, About, not found.

## 1. Findings, ranked by value to the user

1. **Loading shows nothing, or one line of text.** While the session read is pending, the whole page
   is the word "Loading…" under the header (`loading`). The Overview shows its heading and the
   "Instances" heading and nothing else: `Instances.tsx` returns `null` before its read answers, so
   that page has no Loading state at all, a gap against `fe/DESIGN.md` 6. The instance page, the list,
   the profile page and About each show one line ("Loading the model profiles…"), then jump to the
   full layout. On loopback this is a flash; on a slow manager (the server read waits on the manager)
   it is a blank page. **U17a.**
2. **The Model profiles list cannot be sorted.** It shows file order (switched-on profiles first,
   the GUI's order), filterable. With many profiles, finding one by engine or name means reading the
   whole table. **U17c.**
3. **The Overview tiles show tags as one comma-joined string.** Eight long tags make a tile a column
   of broken words (shot `overview-1280-light.png`), and these labels look nothing like the instance
   page's chips (brief 15). **U17b.**
4. **The Server form and the model profile page do not mark what changed.** The save bar says "You
   have unsaved changes." but not where. On the Server form, 25 fields over five cards, the user has
   to remember. **U17e.**
5. **Back loses the scroll position.** Every page change focuses the page heading
   (`App.tsx` ~202, ~385), which scrolls to the top. Back from a profile to a long list lands at the
   top, not on the row that was opened. **U17d.**
6. **About has no way to copy the versions** a bug report needs. They are monospace text to select
   by hand. **U17f.**
7. **No motion at all.** Content, chips and badges appear in one frame. That is calm, and also abrupt
   when a skeleton gives way to content. A short opacity fade, only under
   `prefers-reduced-motion: no-preference`, softens it. **U17g.**

Already good, so not changed: the sign-in page (art, intro, inline errors, pending button), the
filter (kept for the tab's life per user), the switches and Start at boot (already optimistic by
hand: `ModelProfiles.tsx` ~298, `InstancePage.tsx` ~656; moving them to `useOptimistic` would change
nothing the user sees, so it is a refactor for the coder's own time, not a slice), the run controls'
pending and timeout texts, the Unsaved changes dialog, the not-found page.

Asked and answered (section 4): Start/Stop on the Overview tiles, yes (U17h); code-splitting the
bundle with `React.lazy`, no.

## 2. Order

U17a, U17c, U17b, U17h, U17e, U17d, U17f, U17g. Each stands alone. U17h follows U17b because both
change the tile and its tests; U17g comes last because it fades what U17a and U17b add.

## U17a. Skeletons while loading

React: none fits. The reads are not Suspense resources, so a 300 ms timer in state does the job.
**Rule** (`fe/DESIGN.md` 5.2 **Skeleton**, 6 Loading): while a page's first read is pending, the
page's loading text is its `role="status"`, rendered at once, now inside a `.visually-hidden` span.
After 300 ms pending, a skeleton in the page's shape shows (`aria-hidden="true"`). A fast reply
therefore shows no flash, and a slow one shows the layout to come. Blocks are `--surface` with a
transparent `--border-w` border (the edge forced colours draw) and `--radius`. They do not animate.

| Page | Status text (key) | Skeleton |
|---|---|---|
| Boot (session pending) | `loading` (kept) | in `main`: one `.skeleton` line 40 % wide (the heading), two `.skeleton-box` |
| Overview | `instances-loading` **new**: `Loading the instances…` | two `.tile` each holding three `.skeleton` lines |
| Instance page | `loading` (kept, the header's) and `server-loading` (kept) | three `.skeleton` lines (header), one `.card` holding a `.skeleton-box` |
| Model profiles | `model-profiles-loading` (kept) | three `.skeleton` lines in the table's place |
| Model profile page | `model-profile-loading` (kept) | two `.card` each holding a `.skeleton-box` |
| About | `about-loading` (kept) | four `.skeleton` lines |

CSS: `.skeleton { display: block; height: 1lh; max-width: 100%; background: var(--surface);
border: var(--border-w) solid transparent; border-radius: var(--radius) }`, plus
`.skeleton + .skeleton { margin-top: var(--space-2) }` and `.skeleton-box` the same with
`height: 8rem`. The 40 % width is `.skeleton:first-child` in the boot `main` only, or the coder's
equal. Heights in `lh` and `rem` follow text size (1.4.4).

1. **17a.1** `en_US.json` has `instances-loading` = `Loading the instances…`.
2. **17a.2** Overview with `GET /api/instances` pending: a `role="status"` holds
   `Loading the instances…` (inside a `.visually-hidden` element), and no tile shows.
3. **17a.3** Fake timers: before 300 ms no `.skeleton` or `.skeleton-box` is in the document; at
   300 ms the Overview's instances section holds two `.tile` elements each with three `.skeleton`,
   every skeleton element inside an `aria-hidden="true"` ancestor.
4. **17a.4** The same rule (status text at once, visually hidden; skeleton of section U17a's table
   after 300 ms) for the boot, the instance page, Model profiles, the model profile page and About,
   with each kept key's text unchanged.
5. **17a.5** When the read answers, whatever the answer (data, empty, each error key), the status
   text and the skeleton are gone in the same render, and `document.activeElement` is what it was.
6. **17a.6** No skeleton has a tab stop or text: `querySelectorAll('[aria-hidden="true"] *')` under
   the skeletons holds no focusable element and an empty `textContent`.
7. **17a.7** CSS: `.skeleton` and `.skeleton-box` declare the properties above; neither has an
   `animation` or `transition`; R1.8's class list gains `skeleton` and `skeleton-box`.

Tests that change: every test that finds a loading text by `getByText` keeps passing (the text is
still in the DOM). Tests that assert the Overview renders nothing while pending
(`Instances*.test.tsx`, if any) now expect the status. `fe/ux/check.js` gains a "loading" state
(replies held) for the UX check; the designer runs it.

### U17a review (designer, 2026-11-21)

- **Defect, the fill**: `--surface` (#f5efe6) on the page's `--page` (#f7f1e8) is all but invisible
  in light mode, so the boot skeleton, the instance header's lines and the Model profiles rows read
  as a blank page (dark mode shows them). The brief's token was wrong. Ruling: `.skeleton` and
  `.skeleton-box` take `--bg` (the card colour on the page); inside a `.card` or `.tile` (whose fill
  is `--bg`) they take `--surface`: `.card .skeleton, .card .skeleton-box, .tile .skeleton
  { background: var(--surface) }`.
- **Defect, the tiles**: the Overview's two skeleton tiles are not in a `.tiles` grid
  (`Instances.tsx` ~258), so they span the column, touching, and the answer replaces them with the
  grid of 15rem tiles: a jump in width and count. Wrap them in an element with class `tiles`.
- **Accepted**: the 40 % first line (`.skeleton:first-child`) everywhere `Lines` starts a shape. It is
  a heading in the boot, the instance header, a tile (the id) and About (the host), and reads as the
  table's head row in Model profiles. The heights (`1lh`, `8rem`): the content is always taller, so
  it grows downward from the same top and nothing above it moves. A jump downward is the
  content arriving, not a layout shift of what was read.
- Minor, coder's call: the instance header's last line touches the card box under it; a
  `margin-top: var(--space-2)` on a box after a line would match the lines' gap.

## U17c. Sort the Model profiles list

React: `useDeferredValue` for the filter text feeding the filtered and sorted rows (`memo` on the
table), so typing stays smooth on a long list ([react.dev](https://react.dev/reference/react/useDeferredValue)).
No criterion pins it; jsdom cannot see it.

**Rule** (APG sortable table, [APG](https://www.w3.org/WAI/ARIA/apg/patterns/table/examples/sortable-table/)):
the **Model profile** and **Engine** headers hold a native button with the header's text and an icon
(`arrow-up`, `arrow-down`, or `arrow-up-down` while unsorted, `aria-hidden`). The sorted column's `th`
has `aria-sort="ascending"` or `"descending"`; the others have none. Clicks cycle ascending,
descending, file order. Status is not sortable: switched-on first is already the file order's
grouping. Comparison: `localeCompare` with `{ sensitivity: 'base', numeric: true }`, ties in file
order. The sort is kept for the tab's life with the filter, and cleared at sign-out as the filter is
(`App.tsx` ~85, ~353). The filter's status line (`role="status"`, `.model-profiles-filter-status`)
announces the change; a later filter change replaces it with the count as today.

| Key | English |
|---|---|
| `model-profiles-sorted-asc` | `Sorted by {0}, A to Z.` |
| `model-profiles-sorted-desc` | `Sorted by {0}, Z to A.` |
| `model-profiles-sorted-file` | `In file order.` |

1. **17c.1** The three keys exist with that text; `Icon.tsx` gains `arrow-up`, `arrow-down`,
   `arrow-up-down` (lucide-static 1.74.0 path data, as the others).
2. **17c.2** The Model profile and Engine headers each hold one button named exactly
   `Model profile` / `Engine`; Status has none; no `th` has `aria-sort` at first; rows in file order.
3. **17c.3** One click on `Model profile`: rows by name A to Z (`Old-Coder`, `Qwen3-27B`,
   `Qwen3.6-35B-A3B-UD-Q4_K_XL`, `Swift-Flash-Next-Draft` for the stub's fixture, numeric
   collation); its `th` `aria-sort="ascending"`; status `Sorted by Model profile, A to Z.`; focus
   stays on the button.
4. **17c.4** Second click: Z to A, `aria-sort="descending"`, `Sorted by Model profile, Z to A.`.
   Third: file order, no `aria-sort`, `In file order.`.
5. **17c.5** Clicking `Engine` while sorted by name moves `aria-sort` to Engine's `th` (ascending)
   and removes it from the name's; ties keep file order.
6. **17c.6** Sort and filter combine (filter `qwen`, sorted descending: `Qwen3.6-…` before
   `Qwen3-27B`); typing in the filter shows the count text as today.
7. **17c.7** Switching a profile while sorted keeps the row where it is and focus on its switch.
8. **17c.8** Going to a profile and back keeps the sort; sign-out and a new sign-in start in file
   order.
9. **17c.9** CSS: `.model-profiles th button` is borderless, inherits the header's font and colour,
   at least 24 px high (H1a.12's list gains `.model-profiles th button: min-height: 24px`); no new
   class.

Tests that change: the list tests pinning the header cells' exact markup (`ModelProfiles.test.tsx`
column checks) now find the text inside a button.

### U17c review (designer, 2026-11-21)

In the browser (`build/ux-review/u17c.js`, `u17c-box.js`: Chromium, Firefox, WebKit, 1280 and 320
px, light and dark, 200 % text). Met: the two buttons and their names, the arrow icons
(`aria-hidden`), `aria-sort` and the cycle by click, Enter and Space, the numeric order on the stub's
fixture, Engine taking `aria-sort`, focus kept on the button with its ring, the status line (sort
sentence ahead of the count under a filter, the count alone after typing), 24 px buttons (43 px at
200 %), no page scroll sideways.

- **Defect, the width at 320 px**: the icon and its gap widen the nowrap header (`.model-profiles th`
  keeps `white-space: nowrap`), so at 100 % text the table, which fitted before, now overflows its
  scroll box by 12 to 15 px in all three engines: the Status switches are cut. Ruling:
  `.model-profiles th button { white-space: normal; text-align: start }`, so "Model profile" wraps
  above its arrow when the column is narrow. Checked: hiding the icon brings the overflow back to 0.
  **Fixed and re-measured**: overflow 0 at 320 px in all three engines, both themes, every switch
  whole; at 200 % the box scrolls 169 px, less than the 221 px of the table before U17c (a data
  table may scroll in its box, 1.4.10). Accepted.

## U17b. Tags as chips on the Overview tiles

React: none. **Rule**: the Tags fact of a tile (`.tile-facts dd`) holds a `ul.chips` (`role="list"`)
of fixed chips, the instance's own and the profile's alike (the tile does not tell them apart, as
today), in the order of `tags`. On a tile (`--surface`) a fixed chip is drawn with `--bg` fill and a
`--control` `--border-w` border (`.tile .chip`). Long tags follow brief 15 R3 (ellipsis, 15em wrap)
and R5 (the expanded pill on hover; a tile chip is not focusable: the tile's id link leads to the
page that shows them whole). Accessible text is unchanged in content: a list of the tags.

1. **17b.1** A tile with tags `edge, v2, gpu` shows in its Tags `dd` a list of three items `edge`,
   `v2`, `gpu`, no button inside; a tile with no tags shows no Tags row (as today).
2. **17b.2** No text `edge, v2, gpu` (comma-joined) remains on the tile.
3. **17b.3** CSS: `.tile .chip` declares `background: var(--bg)` and
   `border: var(--border-w) solid var(--control)`; the `.chip > span` rules of brief 15 R3 apply.
4. **17b.4** A 32-character tag on a tile at 320 px stays inside its tile (UX check state
   `overview-tiles`, no sideways scroll).
5. **17b.5** The tile's `dl` keeps its `dt`/`dd` pairs; the `dd` holding the list keeps
   `overflow-wrap: anywhere` (12 T1 layout).

Tests that change: `InstanceTags.test.tsx` T1.2 to T1.4 and `InstanceTiles.test.tsx`, which read the
joined text, now read the list items.

## U17e. Mark the changed fields

React: none (the form already knows each field's saved and current value).
**Rule**: on the Server form and the model profile page, a field whose current value differs from the
last read or save gets the class `changed` on its `.field`. It is drawn as a `--accent` bar
`--ring-w` wide at the inline start of the label row (a shape beside the label, never colour on the
text alone, [1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color); `--accent` on `--surface` is in
5.1's table). The control's `aria-describedby` gains, last, a visually hidden span `Changed, not
saved.` (key `field-changed`), so a screen reader hears it on focus. Discard and a successful save
clear every mark. A field set back by hand to its saved value loses its mark.

1. **17e.1** `en_US.json` has `field-changed` = `Changed, not saved.`.
2. **17e.2** Typing in Server > Port: its `.field` gets `changed`; the input's `aria-describedby`
   ends with an id whose element's text is `Changed, not saved.`; other fields are unmarked.
3. **17e.3** Typing the saved value back removes `changed` and that id.
4. **17e.4** Discard removes every mark; a 204 save removes every mark; a refused save keeps them.
5. **17e.5** A checkbox (Server > Web UI) and a select (Log level) are marked the same way.
6. **17e.6** The model profile page: the same rules on a card field (Context size).
7. **17e.7** CSS: `.field.changed .field-label` draws the bar (`box-shadow: inset var(--ring-w) 0 0
   var(--accent)` with `padding-inline-start: var(--space-2)`, or the coder's equal that is a shape);
   a forced-colours rule draws it in `Highlight` (H1a.12's forced block count rises to 6); R1.8 gains
   `changed`.

Tests that change: tests asserting a field's whole `aria-describedby` after an edit
(`ServerSave.test.tsx`, the model profile save tests) gain the id at the end.

## U17d. Scroll kept on Back and Forward

React: none (history and scroll are the browser's). **Rule**: before a hash change, the page's
`scrollY` is stored in the current entry's `history.state` (`replaceState`, no new entry). On a
Back or Forward that lands on an entry with a stored position, the page renders, then
`scrollTo(0, y)`, and the heading takes focus with `{ preventScroll: true }`. A new navigation (a
link) goes to the top as today. The Unsaved changes dialog's cancel restores nothing (the page did
not change).

1. **17d.1** Scroll the Overview to 600, open an instance, press Back: `window.scrollTo` is called
   with `(0, 600)` after the Overview renders; focus is on the page heading; `focus` was called with
   `{ preventScroll: true }`.
2. **17d.2** Following a link to a page never visited scrolls to `(0, 0)` (as today).
3. **17d.3** Forward to the instance page restores its stored position the same way.
4. **17d.4** Storing adds no history entry (`history.length` unchanged by it).
5. **17d.5** With Unsaved changes, Back opens the dialog; Stay keeps the scroll and focus where they
   were.

## U17f. Copy the versions (About)

React: none. **Rule**: under the versions, a secondary button **Copy versions** (icon `copy`) writes
to the clipboard (`navigator.clipboard.writeText`) the page as plain text, in page order: each
section heading on its own line (the host first), then one `<label>: <value>` line per row. Its status line (`role="status"`) says `Versions copied.`; a refusal or a
missing clipboard says `The versions could not be copied: select them instead.` The text stays
selectable.

| Key | English |
|---|---|
| `about-copy` | `Copy versions` |
| `about-copied` | `Versions copied.` |
| `about-copy-failed` | `The versions could not be copied: select them instead.` |

1. **17f.1** The keys exist; `Icon.tsx` gains `copy` (lucide-static 1.74.0).
2. **17f.2** After the read, a button `Copy versions` follows the versions; none while loading or
   on `about-failed`.
3. **17f.3** Clicking it calls `writeText` once with exactly the lines of the stub's fixture
   (`gpu-box`, `Private AI Framework: 2.99.1`, `llama.cpp`, `Version: 1:1.0.0+pai2`, `CUDA: …`, in
   order, joined by `\n`);
   status `Versions copied.`; focus stays on the button.
4. **17f.4** `writeText` rejecting, or `navigator.clipboard` undefined: status
   `The versions could not be copied: select them instead.`.

## U17g. Short fades, only when motion is welcome

React: none. **Rule** (`fe/DESIGN.md` 5.1 **Motion**, amended): opacity only, 150 ms, `ease-out`,
declared only inside `@media (prefers-reduced-motion: no-preference)`
([2.3.3](https://www.w3.org/TR/WCAG22/#animation-from-interactions), level AAA, followed anyway as
the identity's calm). It applies to: the content that replaces a skeleton (U17a), a chip entering
(brief 15), the expanded pill (brief 15 R5), the state badge's change. Nothing moves or scales.

1. **17g.1** CSS: one `@media (prefers-reduced-motion: no-preference)` block holds every
   `transition` and `animation` of the sheet; outside it none exists.
2. **17g.2** Inside it only `opacity` is animated, at `150ms`, on exactly the four targets above
   (a `@keyframes` from `opacity: 0` for elements that mount).
3. **17g.3** The UX check passes with `reducedMotion: 'reduce'` and `'no-preference'` (the designer
   runs both).

## U17h. Start and Stop on the Overview tiles

User (2026-11-21, Q1): yes. React: none new. The tiles read the shared `InstancesStore`, and each tile
keeps its own run state. **Rule**: the instance page's run controls (`InstancePage.tsx` ~340 to 460,
brief 01 40a and on) brought to each tile, with the same states, cadence and refusals. The
differences: only **Start** and **Stop** (Restart stays on the instance page), and texts that name
the instance, because several tiles speak in one page. Under the tile's facts sits a
`div.run-actions` (the existing class) with two secondary buttons. Their visible text is `Start` /
`Stop` (`run-start`, `run-stop`); their accessible name, set with `aria-label`, is `Start <id>` /
`Stop <id>`, which begins with the visible text ([2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name)).
After the buttons, the tile's own `p role="status"` (`.run-status`) and, on a failure,
`p role="alert"` (`.run-alert`) with a Sign in button when the session ended. Buttons that cannot act
are `aria-disabled` and keep focus ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)). **One write
at a time per tile**: while a tile's action runs, both its buttons are `aria-disabled`; other tiles
stay usable. After a 2xx the list is re-read every `ACTION_MS` (1 s), as on the instance page, until
the tile's state settles or the deadline passes (a minute for Start, six minutes for Stop).

| Key | English |
|---|---|
| `tile-start` | `Start {0}` |
| `tile-stop` | `Stop {0}` |
| `tile-starting` | `Starting {0}…` |
| `tile-stopping` | `Stopping {0}…` |
| `tile-started` | `{0} started.` |
| `tile-stopped` | `{0} stopped.` |
| `tile-ended-failed` | `{0} is in the Failed state. Its log on the server says why.` |
| `tile-timeout` | `{0} did not reach the expected state within a minute. Its state keeps updating.` |
| `tile-timeout-stop` | `{0} did not reach the expected state within six minutes. Its state keeps updating.` |

`{0}` is the instance id. Failure texts are the instance page's, unchanged (its `failureText`).

1. **17h.1** `en_US.json` has the nine keys with exactly that text.
2. **17h.2** Each tile shows, after its facts, buttons with visible text `Start` and `Stop` and names
   `Start <id>` / `Stop <id>`; no Restart. Tab order in a tile: the id link, Start, Stop.
3. **17h.3** Inactive by state, as `inactiveIn`: `active` Start; `inactive` and `failed` Stop;
   `activating` and `deactivating` both; an unknown state neither. A click on an `aria-disabled`
   button sends nothing.
4. **17h.4** Clicking `Start coder` sends one `POST /api/instances/coder/start` with no body; that
   tile's status reads `Starting coder…` and both its buttons are `aria-disabled`; `Stop default`
   on another tile stays active and sends its own POST when clicked.
5. **17h.5** After 204, `GET /api/instances` is sent every 1 s; when `coder` reads `active` the
   status reads `coder started.`, the buttons follow the state, and focus has stayed on
   `Start coder` throughout. Stop works the same way with `coder stopped.` on `inactive`.
6. **17h.6** Reaching `failed` shows `coder is in the Failed state. Its log on the server says
   why.`; no settled state by the deadline shows `tile-timeout` (Start) or `tile-timeout-stop`
   (Stop) with `coder`, the buttons follow the state again, and the 1 s re-reads stop (the 5 s poll
   goes on).
7. **17h.7** Each refusal shows in that tile's `role="alert"` the instance page's text for the same
   reply (404 `instance-not-found` with the id; 503 `api-client-unavailable`; 403
   `client-not-authorized`; 422 unknown key `api-other`; 400, 502, a body that is not JSON and a
   network failure `request-failed`); the status is empty; the buttons follow the state.
8. **17h.8** 401: the alert `session-ended-retry` and a `Sign in` button; a sign-in that keeps the
   page removes the alert and puts focus back on the button that was clicked; nothing is resent.
   403 `session-not-administrator`: `session-revoked` with `Sign in`, as on the instance page.
9. **17h.9** Opening the instance page from a tile during a run shows the state the store read; the
   Overview unmounting stops that tile's re-reads (no request after the unmount).
10. **17h.10** Layout: no new class; the tile's buttons are `.run-actions` buttons; the UX check's
    `overview-tiles` state at 320 px and 200 % text has no sideways scroll.

Tests that change: tests of the tile's tab stops and content (`InstanceTiles.test.tsx`, the H6 tile
tests in `Overview*.test.tsx`), which expected the id link as the tile's only control; tests that
count `role="status"` regions on the Overview. `fe/ux/check.js` `overview-tiles` now has buttons in
each tile.

## 3. Accessibility (WCAG 2.2 AA), across the slices

- Skeletons are `aria-hidden` and the status text carries the state
  ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)); focus never moves for them
  ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)).
- Sort state is `aria-sort` on the header and a status text
  ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships), 4.1.3).
- The changed mark is a shape plus a description, not colour alone
  ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)), with a forced-colours rule
  ([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast)).
- A restored position may leave the heading above the viewport while it holds focus (taken with
  `preventScroll`). That is allowed: 2.4.11 is about author content covering focus, and the heading is
  not a control the user then operates. The page is still announced through the heading.
- Motion only under `no-preference`.
- New targets: the sort buttons, at least 24 px high; Copy versions at `--control-h`
  ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).

## 4. Questions answered by the user (2026-11-21)

- **Q1** Start and Stop on each Overview tile: **yes**, U17h.
- **Q2** Split the bundle with `React.lazy` + `Suspense`: **no**, not for now.

## 5. Out of scope

New data or routes; the server profile pages (brief 16); dark mode or token changes; the GUI.
