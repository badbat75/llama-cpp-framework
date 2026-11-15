# Brief 02: the Models page

Written by the designer on 2026-11-15 for `PLAN.md` step 10, web UI slice 2. Applies `fe/DESIGN.md`;
"D2.2" points to its sections. Each numbered line of section 2 is an acceptance criterion the tester
turns into a Vitest + Testing Library test. The reference is `pai-config`'s Models tab
(`crates/config/ui/models_page.slint`, list built by `preset_summaries` in `crates/config/src/gui.rs`
over `presets::load_listed` in `crates/app/src/presets.rs`), its fields and checks inventoried in
[configurator-inventory.md](configurator-inventory.md) 1.2 and 1.3, and its texts in
`crates/core/strings/en_US.toml`. Engine decisions: D2.2.

## 0. The split

In build order. Each line is one tester run and one coder run; each ships alone. "R" is read-only for
every role, "W" adds writing for administrators. Every W slice reuses the save bar, the refusal
pattern and the session-ended dialog of slice 1b.

| Slice | Holds |
|---|---|
| **2a** | R: nav entry, `#/models`, the one mixed preset list (name, engine, on or off), loading, empty, read errors. **This brief.** |
| 2b | R: the list's filter field (the GUI's "Filter presets…": id or model path, case-insensitive; adds `model` to the list reply). |
| 2c | W: the on/off switch of each row (moves the section between `presets.ini` and `presets.ini.disabled`); a user sees it disabled with a Notice. |
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
13. **Nothing in a row acts**: no button, switch, checkbox or link, for either role. No Notice for a
    user either: in 2a nobody can edit, so there is nothing to tell a user apart (the Notice
    `presets-read-only` comes with 2c).

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

- **U1. On/off visible to users.** In 2c, a user (not administrator) sees the switch disabled. Should
  a user see switched-off presets at all, or only what the engine serves? The brief assumes they see
  all, as in the GUI.
- **U2. Wording "Status, On / Off".** The GUI has a switch and no word for it. "Status" and "On" /
  "Off" are proposed; "Served" / "Not served" would say more but differ from the GUI's "switched off".
- **U3. Delete's linked files.** The GUI offers to delete the model files a preset links. On the web
  that means the bff writing the models folder (`/var/lib/private-ai/.llama.cpp`), which it cannot
  today. Keep Delete to the preset only on the web, or plan that write access?
