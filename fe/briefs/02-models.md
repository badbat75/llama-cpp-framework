# Brief 02: the Models page

Written by the designer on 2026-11-15 for `PLAN.md` step 10, web UI slice 2. Applies `fe/DESIGN.md`;
"D2.2" points to its sections. Each numbered line of section 2 is an acceptance criterion the tester
turns into a Vitest + Testing Library test. The reference is `pai-config`'s Models tab
(`crates/config/ui/models_page.slint`, list built by `preset_summaries` in `crates/config/src/gui.rs`
over `presets::load_listed` in `crates/app/src/presets.rs`), its fields and checks inventoried in
[configurator-inventory.md](configurator-inventory.md) 1.2 and 1.3, and its texts in
`crates/core/strings/en_US.toml`. Engine decisions: D2.2.

## Administrators only (user, 2026-11-15)

The whole web UI is run by administrators: only an administrator can sign in (`fe/DESIGN.md`
section 3, brief 01's 14b), so the Models page, which needs a session, has no user's view. The
lines of 2c below are already fixed in place; 2a and 2b are built and change only in their wording.

**Removed**: 43 (the read-only Notice) and the user half of 42; the string `presets-read-only`
(2c.8); question U1 (answered: no user sees the page).

**Changed** (one line each):

- 13 (built): "Nothing in a row acts" holds until 2c; drop its note about a user's Notice.
- 26 (built): the field, button, hint and status are the same for every session (there is one role);
  no test change.
- 42: every row's switch is enabled for every session. No special case for a row whose engine is not
  available (2o revisits it).
- 49: a sign-in compares `user` only (brief 01's 43b as changed): the same `user` keeps the page,
  focus on the switch; a different `user` reads the list again, the filter emptied, focus on `h1`.
- 53: the tab order without "(a user's Notice holds no control)" and "(administrator; a user's
  disabled switches are not in the sequence)".
- 54: drop "for a user, the same with the reader's dimmed or unavailable".
- 55: 2c reads what 2b reads; not the session's `canConfigure`.
- 0, the split: "R" only reads, "W" adds writing; no slice has a view for a non-administrator.

No code on main to remove: 2a and 2b never read `canConfigure`.

## 0. The split

In build order. Each line is one tester run and one coder run; each ships alone. "R" only reads, "W"
adds writing (every signed-in session is an administrator's). Every W slice reuses the save bar, the refusal
pattern and the session-ended dialog of slice 1b.

| Slice | Holds |
|---|---|
| **2a** | R: nav entry, `#/models`, the one mixed preset list (name, engine, on or off), loading, empty, read errors. **This brief.** |
| 2b | R: the list's filter field (the GUI's "Filter presets…": id or model path, case-insensitive; adds `model` to the list reply). |
| 2c | W: the on/off switch of each row (moves the section between `presets.ini` and `presets.ini.disabled`). |
| 2d | R: the preset page `#/models/<id>`: heading, the row's name becomes a link, the **Engine** select (first field: the engine whose preset file holds the preset, shown with one entry, "(not available)" option and Notice per D2.2), and the Assets card's values as text (no file pickers yet). |
| 2e | R: the plain cards, each field as in the GUI: Hardware, Resource / context, KV cache, Chat / reasoning, Sampling, Embeddings, Advanced, Vision, Speculative (values and enable rules; no warnings). |
| 2f | R: GPU distribution and Tensor placement tables (needs the server machine's device list from the bff). |
| 2g | R: Model info card (needs a GGUF read on the server: the bff links pai-core only today, inventory 4.1). |
| 2h | W: save of the plain cards of 2e (client checks of the inventory's (f) column, server refusals beside their field, Revert). |
| 2i | W: Assets editing: model, mmproj and draft pickers over the server's models-folder scan, the draft pick rules (inventory 1.3, GUI code today). |
| 2j | W: GPU distribution and Tensor placement editing. |
| 2k | R+W: derived rules and warning strips: fit allowed, KV warning, split-mode warning, MoE floor, the prunes at save. |
| 2l | W: New preset (dialog: id and engine; the GUI's new-preset values). |
| 2m | W: Clone and Rename (id rules: `[A-Za-z0-9._-]+`, unique case-insensitively across both files). |
| 2n | W: Delete with its confirmation dialog (the linked-file deletion of the GUI is out until the bff may write the models folder: question U3). |
| 2o | R+W: a preset whose engine is not available (possible only once a second engine's preset file exists): unknown engine's fields as raw `key = value` lines, kept on save (D2.2). |

Out of slice 2: Downloads (Hugging Face), Benchmarks, bench sweeps, the Integrations snippets, any
second engine's own fields.

## 1. Data 2a reads (the tests stub it)

Only `GET /api/presets`, section 3. The session and the nav's `GET /api/instances` are as slice 1.
2a reads nothing else: not `GET /api/engines` (the list reply says whether each engine is
available) and not the instances (the presets are not per instance).

## 2. Slice 2a: the read-only preset list

### 2.1 Nav and route

1. **Nav entry.** Signed in, the nav holds, after the Instances group, a link **Models**
   (`nav-models`) to `#/models`. Signed out it is not in the nav (the nav holds only Overview, slice
   1 criterion 3). While the session read is pending, only Overview (slice 1 criterion 2). A failed
   instances read leaves out the Instances group, not Models.
   ([3.2.3](https://www.w3.org/TR/WCAG22/#consistent-navigation))
2. On `#/models` the link carries `aria-current="page"` with the current-page style of the other
   nav links. ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships))
3. **Title and focus.** On `#/models`: `h1` `models-title`, `document.title` `page-title` filled with
   `models-title` ("Models | Private AI Framework"), focus on the `h1` (`tabindex="-1"`) after the
   route change. ([2.4.2](https://www.w3.org/TR/WCAG22/#page-titled),
   [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order))
4. **Signed out.** `#/models` shows the sign-in form in its place with `signin-required`, the route
   unchanged; after sign-in, the Models page with its focus on the `h1` (slice 1 criteria 10, 13).
5. `#/models/<anything>` is still an unknown route in 2a: the `not-found` page (slice 1 criterion 8).
   2d makes it the preset page.

### 2.2 Page structure

6. Below the `h1`: `h2` `presets-title`, the intro `presets-intro`, then the content: the loading
   status, a Message, or the table. After the table (and after the empty Message), the note
   `presets-off-note` in `--muted`, 0.875rem. The `h2` and intro stay in every state.
   ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
   [2.4.6](https://www.w3.org/TR/WCAG22/#headings-and-labels))
7. **A table, not a list.** Each preset has the same attributes (name, engine, on or off), which a
   reader compares down a column: data with a logical relationship in a grid, what a data table is for
   ([W3C WAI Tables Tutorial](https://www.w3.org/WAI/tutorials/tables/)); a list would need each
   attribute repeated as a label in every row. It is the D5.2 Table: a native `<table>`, named by the
   `h2` (`aria-labelledby`), so `getByRole('table', { name: 'Presets' })` finds it. One header row of
   `<th scope="col">`: `presets-column-name`, `presets-column-engine`, `presets-column-status`, in
   that order (the GUI's row reads id first; the engine before the state, as the Overview orders
   instance, engine, state). ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
   technique [H51](https://www.w3.org/WAI/WCAG22/Techniques/html/H51))
8. **Engine column always shown**, even when every preset has the same engine (D2.2: each row has an
   engine; unlike the Overview's, which shows only with two engines).

### 2.3 Rows

9. One row per preset, in the reply's order; the fe does not sort (the bff gives the GUI's order:
   switched on first, then switched off, each in file order).
10. **Name cell**: the preset's `id`, plain text in 2a (no link: the preset page comes in 2d, and a
    link to nothing is empty chrome, D1.4).
11. **Engine cell**: the reply's `engine` as data (never a strings key, D2.2), e.g. `llama.cpp`.
    When `engineAvailable` is false: `presets-engine-not-available` filled with the engine
    ("Strata (not available)").
12. **Status cell**: `presets-status-on` ("On") when `enabled` is true, `presets-status-off` ("Off")
    when false. A switched-off row is drawn in `--muted` (the GUI mutes it), its text still saying
    "Off", so the state is never colour alone; `--muted` meets 4.5:1 on `--bg` and `--surface`
    (D5.1). ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color),
    [1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum))
13. **Nothing in a row acts**: no button, switch, checkbox or link (2c adds the switch).

### 2.4 States

14. **Loading**: `<p role="status">` `presets-loading` in place of the table.
    ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
15. **Empty** (200 with `[]`): Message `presets-empty`, a plain paragraph (not an alert), no table.
16. **401**: the whole app becomes signed out, as for the server read (slice 1, 2.6 "401 on the
    server read"): the sign-in form with `signin-required` in place of the page, the banner's
    **Sign in**, the nav without Instances and Models; after sign-in, the page reads again.
17. **Errors by key**, a plain paragraph in place of the table (no alert: nobody asked), the `h2`,
    intro and note kept:
    - 403 `client-not-authorized`: `api-client-not-authorized`;
    - 503 `client-unavailable`: `api-client-unavailable`;
    - any other status or key, a body that is not the error shape, a 200 that is not an array of
      rows with the four fields of their types, a network failure, or the 15 s `GET` limit running
      out: `presets-load-failed`. Never the reply's body.
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification))
18. **When it reads**: once each time the page opens (route change to `#/models`, and after a
    sign-in that returns to it). No polling, no refresh button in 2a (the GUI's list changes only by
    its own edits or a Refresh; 2c brings writes and their re-read).

### 2.5 Width

19. **At 375 px and down to 320 px** the table keeps its three columns and fits the main region with
    no horizontal scroll: the name cell wraps anywhere (`overflow-wrap: anywhere`, ids such as
    `Qwen3.6-35B-A3B-UD-Q4_K_XL` have no spaces); the engine cell wraps only at spaces, never inside
    a word (`white-space: normal; overflow-wrap: normal`: "Strata (not / available)", `llama.cpp`
    whole); the status cells and the three header cells do not wrap at all (`nowrap`, and not the
    global `overflow-wrap: anywhere` of `th`, which at 320 px breaks "Prese/t" and "Sta/tus"); cell
    padding `--space-2`. Measured in review at 320 px: this keeps the table at 288 px with the name
    column at 112 px, where a nowrap engine cell left the name 62 px and its id on 7 lines. At 960 px
    and up, the table spans the main column (max 880 px). Checked in review, not in jsdom. ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow),
    [1.4.4](https://www.w3.org/TR/WCAG22/#resize-text))

### 2.6 Keyboard and screen reader

20. Tab order on the page: skip link, banner, nav (Overview, the instance links, Models), then
    nothing inside the table (it holds no control). Screen-reader users move through it with table
    navigation. ([2.1.1](https://www.w3.org/TR/WCAG22/#keyboard),
    [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order))
21. A screen reader announces, on arrival: the `h1` "Models" (focus); in the table, "Presets, table,
    3 columns", and each cell with its column header ("Engine, llama.cpp", "Status, Off"), from the
    `<th scope="col">` cells. Nothing is announced when the list loads apart from the loading status
    going away (it is a page read, not an action).

### 2.7 Strings, slice 2a

Each key is used by the code (`fe/src/strings.test.ts`). Engine names and preset ids are data, not
strings (D7).

| Key | English | Source |
|---|---|---|
| `nav-models` | Models | GUI tab `models` |
| `models-title` | Models | GUI tab `models` |
| `presets-title` | Presets | GUI `presets` |
| `presets-intro` | Each preset is one model with its settings. Every instance of the preset's engine can serve it. | new (the GUI has no intro; one `presets.ini` serves every instance, inventory 5) |
| `presets-column-name` | Preset | new |
| `presets-column-engine` | Engine | new (same text as `instances-column-engine`, its own key per area) |
| `presets-column-status` | Status | new (the GUI's switch has no header) |
| `presets-status-on` | On | new (the GUI shows a switch) |
| `presets-status-off` | Off | new |
| `presets-engine-not-available` | {0} (not available) | D2.2 |
| `presets-loading` | Loading the presets… | new, as `server-loading` |
| `presets-empty` | No presets yet. | GUI `no-presets-yet`, without "click New… to create one" (no New in 2a; 2l adds it) |
| `presets-load-failed` | The presets could not be read. | new, as `server-load-failed` |
| `presets-off-note` | A preset switched off is kept, but its engine does not serve it and OpenCode does not offer it. | GUI `preset-off-note`, changed: "stays editable here" is false in 2a, and "llama-server" becomes "its engine" (a mixed list) |

Reused, unchanged: `page-title`, `signin-required`, `not-found`, `api-client-not-authorized`,
`api-client-unavailable`.

### 2.8 Out of 2a

The filter (2b), the on/off switch and any write (2c), links to the preset page and the page itself
(2d on), New, Clone, Rename, Delete (2l to 2n), the GUI's presets-file path line (`presets_path`: a
path on the server, of no use to a reader who cannot open it; the Server page names only its file),
any column the GUI's list does not show (model file, load on startup), sorting, polling.

## 2b. Slice 2b: the list's filter

Added on 2026-11-15. Criteria go on from 2a's numbers. Everything in 2a stays true unless a line
below changes it. The reference is the GUI's filter: the `LineEdit` with `input-type: search` above
the list (`crates/config/ui/models_page.slint` 76-81), the rule `preset_matches` in
`crates/config/src/gui.rs` 908-914, applied on each edit (`on_filter_presets`,
`crates/config/src/gui/models_tab.rs` 423), and the texts `filter-presets` and `no-presets-match`
(`crates/core/strings/en_US.toml`).

### 2b.1 Data

22. 2b reads only what 2a reads: `GET /api/presets`, whose rows now carry a fifth field, `model`
    (a string; section 2b.8). A 200 whose rows lack `model` or carry a non-string there is a reply of
    the wrong shape: `presets-load-failed`, as criterion 17. Filtering is in the fe, over the rows
    already read: typing reads nothing.

### 2b.2 The field

23. **Where.** Below `presets-intro`, before the status line (criterion 29) and the table: a label,
    the field, its hint, and the **Clear filter** button when the field holds text (criterion 31).
    The label `presets-filter-label` sits above the field as a visible `<label for>`; the hint
    `presets-filter-hint` sits below it in `--muted`, 0.875rem, and is tied to the field with
    `aria-describedby`. No placeholder: a placeholder is not a label and disappears as one types.
    ([3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions),
    [1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
    [2.4.6](https://www.w3.org/TR/WCAG22/#headings-and-labels))
24. **Type.** `<input type="search">` (the GUI's `input-type: search`; its role is `searchbox`, so
    `getByRole('searchbox', { name: 'Filter presets' })` finds it), with `autocomplete="off"` and
    `spellcheck="false"` (ids and paths are not words). Not inside a `<form>` and no `search`
    landmark: it filters one table and submits nothing **(preference)**. The browser's own clear
    glyph (`::-webkit-search-cancel-button`) is hidden, so Blink and WebKit do not show a second clear
    control beside the button of criterion 31 and Firefox looks the same.
25. **Width.** At 375 px and down to 320 px, the field and the Clear button sit on one line, the
    field taking the rest of the main column (flex 1, `min-width: 0`), the button not wrapping
    (`nowrap`); no horizontal scroll. From 640 px up the row is at most 24rem wide, left-aligned. The
    field's font is the body font, not monospace (ids are typed as words). Checked in review, not in
    jsdom. ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow))
26. **Same for both roles**: a user and an administrator see the same field, button, hint and
    status, with the same roles and names. Filtering changes nothing on the server.

### 2b.3 What matches

27. **The GUI's rule, exactly.** An empty field shows every row. Otherwise a row is shown when its
    `id` or its `model`, lowercased, contains the whole field's text, lowercased, as one substring:
    no trimming (a typed space counts, as in the GUI), no splitting into words, no wildcard or
    pattern, path separators matched as typed. Lowercasing is JavaScript's `toLowerCase()` (Unicode
    default case mapping, as Rust's `to_lowercase` in the GUI; not the locale-dependent
    `toLocaleLowerCase`). A row with `model` `""` matches only by its id. Examples a test can use:
    with rows `Qwen3-27B` (model `/m/qwen/Qwen3-27B-Q8_0.gguf`) and `Old-Coder` (model `""`):
    `qwen` and `Q8_0` show only `Qwen3-27B`; `coder` shows only `Old-Coder`; `-` shows both;
    `27b q8` shows none; ` qwen` (leading space) shows none.
28. **Order and look kept**: the shown rows keep the reply's order (criterion 9) and their 2a cells
    and muting; switched-off rows are filtered like the others. The filter hides rows, never sorts or
    highlights.

### 2b.4 When the list updates, and what is heard

29. **As typed.** Each change of the field's value (typing, pasting, deleting, Clear, Escape)
    updates the table at once; no Enter, no button, no delay. Enter does nothing. Nothing moves focus
    or changes the page's context. ([3.2.2](https://www.w3.org/TR/WCAG22/#on-input))
30. **The status line.** A `<p role="status">` (implicit `aria-live="polite"`, `aria-atomic` true),
    rendered whenever the field is, between the field row and the table, so it is in the page before
    its text changes (technique [ARIA22](https://www.w3.org/WAI/WCAG22/Techniques/aria/ARIA22): the
    container has `role="status"` before the message occurs). Its text:
    - field empty: empty (nothing to announce; the paragraph stays in the DOM);
    - one or more rows match: `presets-filter-count` filled with the shown count and the total
      ("Presets shown: 2 of 5.");
    - no row matches: `presets-no-match` ("No presets match the filter."), and the table is not
      rendered (no header row over nothing); `presets-off-note` stays below.
    A screen reader hears the count or the no-match text after the typed character's echo, without
    focus moving: a result count and "No results returned" are status messages in
    [Understanding 4.1.3](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html). Shown in
    `--muted`, 0.875rem. ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
    **Its line is kept while empty** (`min-height: 1lh`, one line of its own text), so the table does
    not move when the first character is typed or the field is cleared (preference: the rows a user
    is reading stay put). Measured in review at 1280 px: with the empty paragraph at height 0 its
    margins collapse and the table jumped 29 px down on the first character and back on Clear. The
    `lh` unit: [MDN, CSS values and units](https://developer.mozilla.org/en-US/docs/Web/CSS/length#lh).
    Checked in review, not in jsdom.

### 2b.5 Clearing

31. **Clear filter button.** A native `<button type="button">` `presets-filter-clear`, after the
    field on the same line, rendered only while the field holds text (a button that clears nothing is
    empty chrome, D1.4). Pressing it empties the field, shows every row, empties the status line, and
    puts focus in the field (the button it was on is gone).
    ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order),
    [2.4.6](https://www.w3.org/TR/WCAG22/#headings-and-labels))
32. **Escape** in the field, while it holds text, does the same as the button, focus staying in the
    field. With the field empty, Escape does nothing. (Blink also clears a search field on Escape by
    itself; the result must be the same, so the field is controlled and its value set to `""` either
    way.) ([2.1.1](https://www.w3.org/TR/WCAG22/#keyboard))

### 2b.6 States and life

33. **Not shown** while loading, on any error (criteria 14, 16, 17) and for an empty list (200 with
    `[]`, criterion 15: nothing to filter, D1.4). In those states neither the label, the field, the
    hint, the button nor the status paragraph is in the DOM. They appear with the table.
34. **Not kept.** Each time the page opens (criterion 18: route change to `#/models`, a sign-in
    that returns to it) the field is empty and every row shows. The text is not in the URL, in
    storage, or kept across a page change **(preference: keep it simple; the GUI keeps it only
    because its tab never closes)**.

### 2b.7 Keyboard and screen reader

35. **Tab order**: skip link, banner, nav, the filter field, the Clear filter button (when shown),
    then nothing in the table (criterion 20). Focus on arrival stays on the `h1` (criterion 3).
    ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order))
36. **On focus** a screen reader says the field's name, role and hint: "Filter presets, search
    field, Matches part of the preset name or of its model file path. Case does not matter." (the
    exact wording of the role is the reader's). The focus ring is DESIGN.md's, on both field and
    button. ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value),
    [2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible))

### 2b.8 The model path: searched, not shown

Decision: the `model` value is matched but not shown in 2b. Reasons: the GUI's list shows only the
id and the switch (2a's out-of-scope line already left the model column out); at 320 px the name
column is 112 px wide (criterion 19), and a fourth column of server paths would squeeze ids to a
character a line; the path is shown on the preset page (2d, Assets card). The hint (criterion 23)
says the filter also matches the model file, so a row matched by its path alone is not a surprise.

### 2b.9 Strings, slice 2b

| Key | English | Source |
|---|---|---|
| `presets-filter-label` | Filter presets | GUI `filter-presets`, without the ellipsis (a label, not a placeholder) |
| `presets-filter-hint` | Matches part of the preset name or of its model file path. Case does not matter. | new (the GUI says nothing; its list does not show the path either) |
| `presets-filter-clear` | Clear filter | new |
| `presets-filter-count` | Presets shown: {0} of {1}. | new; "label: n of m" so one text fits 1 and many |
| `presets-no-match` | No presets match the filter. | GUI `no-presets-match` |

### 2b.10 Out of 2b

Showing the model path, highlighting the matched text, matching by words or any other field (engine,
on or off), sorting, keeping the filter across pages or in the URL, a keyboard shortcut to the
field.

### 2b.11 Proposed contract change (for the bff's coordinator)

To `GET /api/presets` in [api-contract.md](api-contract.md), by its keeper:

> Each row gains `"model"`: the value of the section's `model` key as the preset file holds it, a
> string, unchanged by the bff (no path resolution against the models folder, no check that the file
> exists, the server machine's separators kept); `""` when the section has no `model` key or it is
> empty. It is what `presets::Preset::model` reads (`get("model")`, `crates/app/src/presets.rs`,
> empty by default) and what the GUI's filter matches. Success becomes
> `[{"id", "engine", "engineAvailable", "enabled", "model"}]`; the example rows gain
> `"model": "/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0.gguf"` and `"model": ""`. Same
> errors and right as today: any signed-in user, since the manager's `ReadFile` needs no right for
> the preset files. The fe treats a row without a string `model` as a malformed reply.

## 2c. Slice 2c: switching a preset on or off

Added on 2026-11-15. Criteria go on from 2b's numbers. Everything in 2a and 2b stays true unless a
line below changes it; 2c replaces criteria 13 (nothing in a row acts), 20 and 35 (tab order) and
21 (what is announced in a row), and adds to 12 and 19. The reference is the GUI's row switch
(`crates/config/ui/models_page.slint` 103-115), its handler `on_toggle_preset_enabled`
(`crates/config/src/gui/models_tab.rs` 384-416), `presets::set_enabled` and `move_between`
(`crates/app/src/presets.rs` 1329-1394), the CLI's `set_preset_enabled` (`crates/cli/src/cli.rs`
1547-1570) and the texts `preset-enabled`, `preset-disabled`, `preset-enabled-restart`,
`preset-disabled-restart`, `preset-switch-failed` (`crates/core/strings/en_US.toml` 603-607).

### 2c.1 What the GUI does (the behaviour 2c matches)

- **The move.** Switching off moves the section, as raw text and unchanged, from `presets.ini` to
  `presets.ini.disabled` (creating that file with a comment header when it is missing); switching
  on moves it back. The destination is written first, then the section is removed from the source,
  so an interruption leaves the preset in both files, listed once as on (`move_between`,
  `listed_in`). A section already in the asked-for file is success, not a refusal.
- **Refusals.** Switching on counts the presets that load on startup against `ModelsMax`
  (`check_startup_for`: `load-on-startup-over-max`, `load-on-startup-unlimited`); through the
  manager the moved section is a changed section of `presets.ini`, so every preset check runs on it
  (`checks::presets_file`, inventory 2.1), against the lowest `ModelsMax` of all instances (inventory
  3.4). Switching off is never refused by a check (`presets_disabled_file`, inventory 2.1). A preset
  in neither file: `no-preset-named`. There is no duplicate-id refusal (a twin in the destination is
  replaced, `replace_section`) and no "in use" refusal: a running llama-server keeps the set it was
  started with.
- **A running engine.** "The router reads presets.ini once, at launch" (`models_tab.rs` 397,
  `presets.rs` 1329-1331): when the server runs, the GUI says "restart llama-server to apply"; it
  restarts nothing.
- **No confirmation.** The switch is not behind the discard guard (inventory 1.10) and asks nothing.
- **After the switch** the GUI rebuilds the list from the files, so the row moves to the end of its
  new group (`refresh_preset_list`). 2c differs on purpose: 2c.5, criterion 47.

### 2c.2 The control

37. **A switch, as a native checkbox.** In each row's Status cell, an
    `<input type="checkbox" role="switch">`, checked when the preset is on. A switch, not a plain
    checkbox: the value is on or off, never mixed, and it takes effect at once, which is the APG
    switch pattern ("an input widget that allows users to choose one of two values: on or off",
    [APG Switch](https://www.w3.org/WAI/ARIA/apg/patterns/switch/)); the column already says On and
    Off (criterion 12) and the GUI draws a `Switch`. Built on the native checkbox, as APG's "Switch
    Example Using HTML Checkbox Input": its state is the `checked` property, never `aria-checked`
    (ARIA in HTML: authors must not use `aria-checked` on `input type=checkbox`, and `switch` is an
    allowed role there, [ARIA in HTML](https://www.w3.org/TR/html-aria/)). Space toggles it (native;
    APG: Space required, Enter optional, so Enter does nothing). `fe/DESIGN.md` 5.2 "Switch".
    ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value), [2.1.1](https://www.w3.org/TR/WCAG22/#keyboard))
38. **Its name is the preset's id**, from the row's name cell: `aria-labelledby` the id of the name
    cell's content (one unique id per row, made by the fe, not the preset id itself), so
    `getByRole('switch', { name: 'Qwen3-27B' })` finds it, and it stays so when 2d makes the name a
    link. The name never holds the state (the state is the switch's `checked`); the column header
    "Status" gives its purpose in table navigation. The visible label (the id in the same row) is the
    whole name, so voice control finds it by what is seen.
    ([2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name), [1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships))
39. **The cell**: the switch, then the word `presets-status-on` / `presets-status-off` of criterion 12
    (still shown, so a sighted reader does not have to read a tick under "Status"), the word
    `aria-hidden="true"` (the switch already says on or off; read once, not twice). Gap
    `--space-1` from the edge of the switch's 24 px hit area, the cell `nowrap`. The word follows the switch's state at every moment (criteria 44,
    46, 48). The switch's hit area is at least 24 by 24 CSS px (D5.1 Targets).
    ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum))
40. **Muting.** A switched-off row's text stays in `--muted` (criterion 12), but an enabled switch in
    it keeps its `--control` border and its tick in the text colour: an operable control's edge must
    meet 3:1 ([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast)); only a disabled switch is
    drawn muted (D5.2, "Disabled controls stay readable").
41. **Width.** Criterion 19 holds with the switch: at 320 px the table fits with no horizontal scroll,
    the Status column at most the width of the switch, the gap and "Off" (about the header's own
    width), the name column measured in review at no less than 100 px (2a measured 112 px). Checked
    in review, not in jsdom. ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow))

### 2c.3 Who can switch

42. Every row's switch is enabled, for every session (only an administrator can sign in, 2026-11-15).
    No special case for a row whose engine is not available (2o revisits it).
43. (Removed 2026-11-15: the read-only Notice for a user.)

### 2c.4 The write

44. **At once.** An administrator's change shows the new value at once (switch, word, muting) and
    sends `PUT /api/presets/<id>/enabled` with `{"enabled": <new value>}` (section 2c.10), the id
    percent-encoded as a path segment. No confirmation, no Save (as the GUI). At the change the
    page's switch status and alert (criterion 50) are emptied.
45. **One write at a time.** While a write runs (from the change until it has settled, the re-read
    of criterion 48 included), **every** row's switch is `aria-disabled="true"`: a click or Space
    changes nothing and sends nothing (the native toggle prevented), focus stays where it is, and
    the switches stay in the tab sequence. Drawn as a disabled checkbox, `GrayText` under
    `@media (forced-colors: active)`, never by `opacity` (D5.2 Inactive; 1c's 45c). One at a time
    because a switch is two file writes and the page has one message region **(preference)**. No
    text while it runs (as Start at boot, 45c). The filter stays usable.
    ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order), [2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible))
46. **Success**: a 2xx whose body is `{"enabled": <boolean>, "running": <boolean>}`. The switch takes
    the reply's `enabled`; the status (criterion 50) reads, filled with the row's id and its engine:
    - `enabled` true, `running` false: `presets-switched-on`;
    - `enabled` false, `running` false: `presets-switched-off`;
    - `enabled` true, `running` true: `presets-switched-on-restart`;
    - `enabled` false, `running` true: `presets-switched-off-restart`.
    A 2xx of another shape is a failure (criterion 48, `request-failed`).
    ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
47. **The row stays where it is.** After a success the row keeps its place in the table; the other
    rows do not move, nothing is read again. The bff's order (on first, then off) comes back the next
    time the page opens (criterion 18). The filter is unchanged: it matches id and model only
    (criterion 27), so the row stays shown and the count (criterion 30) does not change. Differs from
    the GUI on purpose: a row that jumps to the end of the table while focused would carry focus
    away from where the administrator was working down the list, and Tab would then leave the table
    **(preference)**; an "Off" row among "On" rows states nothing false, its word and muting say
    which it is.
48. **Failure**, by key, in the page's alert (criterion 50); the status stays empty:
    - 401, whatever the body: `session-ended-retry` and a **Sign in** button (criterion 49); the
      switch shows the value before the change, nothing is read again;
    - `client-not-authorized`: `api-client-not-authorized`;
    - `client-unavailable`: `api-client-unavailable`;
    - `no-preset-named`: `api-no-preset-named` with the id;
    - `load-on-startup-over-max`, `load-on-startup-unlimited`: `api-load-on-startup-over-max`,
      `api-load-on-startup-unlimited` with the reply's `args`;
    - a 422 with any other key: `api-other` with the key (the contract's rule);
    - anything else (another key, no key, a body that is not the error shape, a 2xx of another shape,
      a network failure, the 120 s limit of a non-`GET` call running out, 1c's 47): `request-failed`.
      Never the reply's body.
    After every failure but the 401 the fe reads `GET /api/presets` once (15 s limit) and the switch
    takes **that row's** `enabled` from it (matched by id), as Start at boot reads its state again
    (1c's 45e): the write may have happened. Only that row's value is taken: the order, the other
    rows and the filter stay as shown. When that read fails in any way (401 included: it changes
    nothing else) or holds no row with that id, the switch shows the value it had before the change.
    The switches stay `aria-disabled` until that read has answered or failed.
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification), [3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion))
49. **401 and Sign in.** The Sign in button opens the sign-in dialog of 1c's 43a (`h2`
    `signin-title`, Cancel; Escape or Cancel closes it, focus returns to this button, the alert
    stays, nothing is sent). A sign-in follows 1c's 43b: the same `user`: the
    dialog closes, the alert and the button go, focus goes to the switch whose write got the 401
    (showing its value before the change); nothing is resent, the administrator switches again.
    Any other session: the banner shows it, the alert goes, the page reads `GET /api/presets` again
    for the new session (criterion 18; the filter emptied, criterion 34),
    focus on the page's `h1`. ([3.3.7](https://www.w3.org/TR/WCAG22/#redundant-entry),
    [2.1.2](https://www.w3.org/TR/WCAG22/#no-keyboard-trap))

### 2c.5 Feedback: one region for the page

50. **Where.** One D5.2 Action feedback region for the whole table, not one per row: after the
    filter's status line (criterion 30) and before the table: the status `<p role="status">`
    (always in the DOM while the table or the no-match text is), the alert `<p role="alert">`
    (rendered only while it has text), then the Sign in button after a 401. Its messages name the
    preset, so one region serves every row. Per row would put a paragraph inside a 3-column table
    cell squeezed to about 60 px at 320 px (criterion 41), or add rows to the table that are not
    presets **(preference)**. Both texts go at the next switch change or when the page is left; the
    alert also at a sign-in (criterion 49). Typing in the filter does not clear them. Shown at the
    body size (not the filter status's 0.875rem), the alert in `--danger` at weight 600 with no icon,
    as the run controls' and Start at boot's alerts (D5.2 Action feedback; the error icon is a
    field error's). The status paragraph keeps one line's height while empty (`min-height: 1lh`, as
    the filter status, criterion 30), so the table does not move down and up by a line at every
    switch (review of 2026-11-15 measured a 24 px jump at each change and success) **(preference)**.
    ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages), [1.3.2](https://www.w3.org/TR/WCAG22/#meaningful-sequence))
51. **Not lost below a long list.** On a failure the switch itself shows the value read again (or the
    one before), so the row a sighted administrator is looking at shows that the switch did not
    hold, and the alert names the preset. Nothing scrolls the page and focus does not move to the
    alert (focus stays on the switch, criterion 52). The review looks at 20 rows at 375 px and reports
    whether the alert is out of sight; today's lists hold a handful of presets (assumption).

### 2c.6 Keyboard and screen reader

52. **Focus never moves** on a write: it stays on the switch through the change, the run, the
    success, the failure and the re-read (the row does not move, criterion 47, so no element holding
    focus is removed or reinserted). The one exception is criterion 49's sign-in.
    ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order), [3.2.2](https://www.w3.org/TR/WCAG22/#on-input))
53. **Tab order**, replacing criteria 20 and 35: skip link, banner, nav, the filter field, Clear
    filter (when shown), the switch region's Sign in (when shown), then each row's switch in row
    order. Focus on arrival stays on the `h1` (criterion 3). The focus ring is D5's on each
    switch, never under a sticky element ([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)).
    ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order), [2.1.1](https://www.w3.org/TR/WCAG22/#keyboard))
54. **What a screen reader says**, replacing criterion 21 for the Status cell: on focus, "Qwen3-27B,
    switch, on" (the role and state words are the reader's); in table navigation, "Status,
    Qwen3-27B, switch, on". After a
    switch, the status politely ("Switched Qwen3-27B off."), or the alert assertively. The hidden
    On/Off word is not read. ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value),
    [4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))

### 2c.7 Data and the tests

55. 2c reads what 2b reads. It writes
    only `PUT /api/presets/<id>/enabled`. The tests stub it with: a 200 of each `enabled` x
    `running` pair; 401; 403 `client-not-authorized`; 503 `client-unavailable`; 404
    `no-preset-named`; 422 `load-on-startup-over-max` with args `["3", "2"]`; 422 `preset-no-model`
    (unknown to the fe: `api-other`); a 502 `client-other`; a 2xx `{}`; a network failure; a
    timeout; each failure followed by a re-read that says on, says off, lacks the row, and fails.

### 2c.8 Strings, slice 2c

| Key | English | Source |
|---|---|---|
| `presets-switched-on` | Switched {0} on. | GUI `preset-enabled` ("Enabled [{0}].") and CLI `preset-switched-on`, the web's "switched" wording of `presets-off-note` |
| `presets-switched-off` | Switched {0} off. | GUI `preset-disabled`, without "llama-server will not offer it" (the off-note below the table says so; and a mixed list has more than one engine) |
| `presets-switched-on-restart` | Switched {0} on: restart the running instances of {1} to apply. | GUI `preset-enabled-restart`; {1} the row's engine (data), since every instance of that engine serves the preset |
| `presets-switched-off-restart` | Switched {0} off: restart the running instances of {1} to apply. | GUI `preset-disabled-restart`, likewise |
| `api-no-preset-named` | There is no preset named {0} any more: it was renamed or deleted elsewhere. Open the Models page again to see the presets as they are. | core `no-preset-named`, with what to do ([3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion)) |

Reused, unchanged: `presets-status-on`, `presets-status-off`, `session-ended-retry`,
`request-failed`, `api-client-not-authorized`, `api-client-unavailable`,
`api-load-on-startup-over-max`, `api-load-on-startup-unlimited`, `api-other`, `signin-title` and
the sign-in dialog's texts. The preset id and the engine are data, never strings (D7).

### 2c.9 Out of 2c

Confirmation before switching (the GUI has none), restarting an instance from the Models page,
naming which instances run, moving the row or sorting after a switch, a bulk switch, switching from
the preset page (2d on), any special case for an engine not available (2o), polling the list.

### 2c.10 Proposed contract (for the bff's coordinator)

To add to [api-contract.md](api-contract.md) by its keeper; the rules at its top apply.

| Route | Who | Slice | Success |
| --- | --- | --- | --- |
| `PUT /api/presets/{id}/enabled` | administrator (`configure`) | 2c, proposed | `{"enabled", "running"}` |

> ### `PUT /api/presets/{id}/enabled`
>
> Switches one preset on or off, as `presets::set_enabled` does for the GUI and `pai preset
> enable|disable` (`crates/app/src/presets.rs`): it moves the section, as raw text and unchanged,
> between its engine's `presets.ini` (on) and `presets.ini.disabled` (off), through the manager's
> `ReadFile` and `WriteFile` (so `configure`), the destination written first, then the source; a
> `presets.ini.disabled` that does not exist is created with the GUI's comment header
> (`DISABLED_FILE_HEADER`). `{id}` is the section name, matched case-insensitively as the GUI does;
> today every preset is llama.cpp's.
>
> - Body `{"enabled": <bool>}`.
> - 200 `{"enabled": <bool>, "running": <bool>}`: `enabled` read back after the write (true when the
>   section is in `presets.ini`); `running` true when an instance of the preset's engine is `active`
>   or `activating` (manager `Status`), since llama-server reads `presets.ini` only at launch: the fe
>   then says to restart. The bff restarts nothing.
> - Idempotent: a preset already where it is asked to go answers 200, nothing written (as
>   `move_between`).
> - Switching on counts the presets that load on startup against the lowest `ModelsMax` of all
>   instances (the manager's rule), and the manager runs every preset check on the moved section;
>   switching off is never refused by a check.
> - The bff runs one preset-file write at a time: a move is two `WriteFile`s, and two moves at once
>   would lose one.
> - Errors: 401 not signed in; 403 `client-not-authorized`; 404 `no-preset-named` with
>   `args: [id]` when neither file holds the id; 422 `{"key", "args"}` for a refusal of the manager's
>   checks: `load-on-startup-over-max` (`args: [count, max]`), `load-on-startup-unlimited`
>   (`args: [count]`), or a preset check on a hand-edited section (`preset-no-model`,
>   `not-whole-number-in-range`, `comment-markers`, `invalid-preset-id`, `fit-target-format`,
>   `override-no-pattern`, `override-no-device`, `moe-cache-with-tensor-split`,
>   `preset-name-taken`); 400 `presets-malformed` for a body that is not `{"enabled": <bool>}`
>   (the fe never sends one: it shows it as a generic failure); 503 `client-unavailable`; 502
>   `client-other` for anything else, empty `args` (the detail stays in the bff's log).
> - **Ids made only of dots.** The id rule `[A-Za-z0-9._-]+` (`checks::preset_id`) allows `.` and
>   `..`, which no URL path can carry: the URL parser drops `.` and `%2e` segments and resolves `..`
>   and `%2e%2e` against the parent, whatever the encoding
>   ([URL Standard, single-dot and double-dot path segments](https://url.spec.whatwg.org/#single-dot-path-segment)).
>   Such a preset cannot be switched from the web until that is settled (question U4).

## 3. Proposed contract: `GET /api/presets` (for the bff's coordinator)

To add to [api-contract.md](api-contract.md) by its keeper; the rules at its top apply.

| Route | Who | Slice | Success |
| --- | --- | --- | --- |
| `GET /api/presets` | signed in | 2a, proposed | `[{"id", "engine", "engineAvailable", "enabled"}]` |

### `GET /api/presets`

Every preset of every engine, one mixed list (D2.2). Not under an instance: one `presets.ini`
serves every instance of its engine (`launch::instance_command`). Today the bff reads llama.cpp's
`presets.ini` and `presets.ini.disabled` through the manager's `ReadFile`, which needs no right for
the preset files; the route still needs a session (the Models page is for signed-in users, D2.1).

```json
[
  {"id": "Qwen3-27B", "engine": "llama.cpp", "engineAvailable": true, "enabled": true},
  {"id": "Old-Coder", "engine": "llama.cpp", "engineAvailable": true, "enabled": false}
]
```

- `id`: the section name.
- `engine`: the id of the engine whose preset file holds the section; no key in the file names it
  (llama-server refuses a `--models-preset` file with a key it does not know: llama.cpp v1.0.0
  `common/preset.cpp`, "option not recognized in preset"), so each engine keeps its own preset files
  (llama.cpp's `presets.ini` and `presets.ini.disabled`; D2.2). `llama.cpp` for every preset today.
- `engineAvailable`: whether `engine` is in `GET /api/engines`; true for every preset while llama.cpp
  is the only engine.
- `enabled`: `true` for a section in `presets.ini`, `false` for one in `presets.ini.disabled`.
- **Order**: the enabled presets first, then the disabled ones, each group in file order. An id in
  both files (an interrupted on/off switch) is listed once, as enabled. Both are what
  `presets::load_listed` does for the GUI (`crates/app/src/presets.rs`), so the two agree.
- A missing file is an empty group, not an error (`presets.ini.disabled` is often absent).
- **Errors**: 401 not signed in; 403 `client-not-authorized`; 503 `client-unavailable`; 502
  `client-other` for any other failure, each with empty `args`.
- Only the four fields 2a shows; 2b adds `model` (section 2b.11).

## 4. Questions only the user can answer

- **U1.** (Answered 2026-11-15: only administrators sign in, so no user sees the page.)
- **U2. Wording "Status, On / Off".** The GUI has a switch and no word for it. "Status" and "On" /
  "Off" are proposed; "Served" / "Not served" would say more but differ from the GUI's "switched off".
- **U3. Delete's linked files.** The GUI offers to delete the model files a preset links. On the web
  that means the bff writing the models folder (`/var/lib/private-ai/.llama.cpp`), which it cannot
  today. Keep Delete to the preset only on the web, or plan that write access?
- **U4. Preset ids `.` and `..`.** The id rule allows them, but no URL path can carry them (2c.10).
  Refuse ids made only of dots in `checks::preset_id` (a change to pai-core, which the GUI and CLI
  would then also enforce), or address presets in the request body instead of the path?
- **U5. The row stays in place after a switch** (criterion 47), where the GUI moves it to the end of
  its group at once. Acceptable, or should the web match the GUI's order immediately (re-read and
  move the row, focus kept on its switch)?
- **U6. Preset ids across engines.** `PUT /api/presets/{id}/enabled` assumes an id names one preset
  across every engine's files (as 2m's rename rule says for both files of one engine). Once a second
  engine exists, may two engines hold the same id? If yes, the route needs the engine too.
