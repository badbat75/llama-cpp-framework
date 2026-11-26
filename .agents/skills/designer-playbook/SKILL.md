---
name: designer-playbook
description: How the designer designs the web UI and its user experience in this project, and the pitfalls met so far. Use when writing a design brief, reviewing a UI slice or editing fe/DESIGN.md.
---

Rules are in `AGENTS.md` and the role in `.opencode/agents/designer.md`. Format of this file:
`AGENTS.md`, under "Skills".

## How to

### Writing a brief

- Name every text the page shows as a key in `fe/src/strings/en_US.json` with its English text, never
  inline (the project's "No user-visible text in the code"); numbers, units, acronyms and paths stay out
  of the strings. No em-dashes in briefs, `fe/DESIGN.md` and proposed texts.
- Split a big slice: the tester and coder run on a small local model and a slice of more than ~12
  criteria fails. The first sub-slice settles every shared rule, the rest are field tables (key, label
  key with toml line, hint, control, absent value, enable rule), each in its own brief file so the
  tester reads only that (`fe/briefs/02e-model-profile-cards.md`).
- A brief that says "join the read in flight" must name which reads may not join.
- A brief that sets a size amends the guard by name: `fe/src/HearthTokens.test.ts` H1a.12 freezes every
  min-height and min-width in the sheet, as it does the classes the tokens declare.
- Adding a nav entry ripples into every test that lists the nav's links in order (`App`,
  `ModelProfiles`, `ProfilesNav`, `HearthNav`, `NodeNav`, `HearthIcons`): name them in the brief.
- Before numbering a criterion, grep `fe/src` for the prefix: ids collide across briefs (brief 16's test
  names cite `16 17a.n`, brief 20's `20 N.n`).
- Destructive actions use Confirm in place, not a dialog (coordinator, 2026-11-21); creates use the
  Create form in place; a write that needs a restart shows the Restart offer (`fe/DESIGN.md` 5.2).
  `pai-config` and `pai` have no server profile pool, instance create or assignment: the web is first.
- Long labels in a chip: one line with an ellipsis, wrapping only as the last resort (user,
  2026-11-21).
- A link whose visible words need a word of context to be heard right (`default · qwen-27b`) keeps the
  visible part `aria-hidden` and a visually hidden phrase in its place, every visible word still in the
  name in order (2.5.3).
- React 19 features have their jobs in `fe/DESIGN.md` 5.3 (optimistic writes, pending states, deferred
  filtering, portals only past a clipping ancestor).
- Who acts through the bff is decided (`PLAN.md` step 10): the user signs in with their OS account and
  the manager applies its rules to them. Errors follow the contract: `{key, args}` with a strings key
  (`fe/briefs/api-contract.md`).
- Model profiles are shared by every instance of an engine, the manager reads the model profile files
  without asking for a right, and the GUI lists switched-on profiles before switched-off ones, each in
  file order. A profile's engine is the file that holds it.
- `pai-config` (Slint, `crates/config/`) is the reference for what each task looks like today; its texts
  are in `crates/core/strings/en_US.toml`. Its hints sometimes name a slider or a number the GUI parks a
  ticked default box on (`crates/config/src/form.rs` `HINT_*`, e.g. 32768 for Context size, invented):
  the web shows no such number, so cut those lines from the web's hint. The GUI reads unparseable values
  as unset (`ini::parse_bool` takes only `true` / `false`); the web shows the file as written.
- Before proposing a bff reply, check what it links: `pai-bff` links `pai-core` only (`ReadFile`,
  `ini::parse_sections`; the `; url =` reader is `pai_core::ini::project_url_of`), while the model
  profile logic (`listed_in`, the spec-type helpers) is private to `pai-app`. Say which helper the bff
  must move or redo.

### Seeing a slice rendered

- To see a slice before the bff has its routes: `npm --prefix fe run build`, copy `fe/dist` to the
  scratchpad and add the API replies as static files (`api/session`, `api/version`,
  `api/instances/index.html` since `instances` is also a folder, `api/instances/<id>/server`), served on
  127.0.0.1 by a small Python server.
- Firefox here is a snap: its `--profile` and screenshot paths must be under `~/snap/firefox/common/`
  (it cannot see `/tmp`), and deleting that folder afterwards may be refused by the permission check:
  tell the user. `--headless --screenshot` shoots at the load event, before the fetches answer: add to
  the copied `index.html` an `<img>` the server answers after 2 s. Dark mode: `user.js` with
  `ui.systemUsesDarkTheme` 1.
- Forced colours: set `browser.display.document_color_use` to 2 in the profile, and remove it from
  `prefs.js` after.
- Text at 200 %: inject `html { font-size: 200% !important }` (every size is in `rem`; equals Firefox's
  real 32 px default, `font.size.variable.x-western`).
- The UX check runs 180 states per browser, its three browsers in parallel, in about 5 minutes (it took 12 one after another): near the 600 s foreground
  limit, so run it in the background with `timeout 1500` and wait for its notification (a `timeout 590`
  killed it during WebKit). Send it to a file (`> build/ux-review/<slice>/check.txt`) and read the lines
  without a leading space (the stub's BrokenPipe tracebacks are noise).
- Stop every stub or server you start before you report (`kill <pid>` of the one you launched, never by
  pattern), and start none in the background that you would wait on.

### Driving and measuring a page

- Clicks and focus: `/snap/bin/geckodriver` over plain WebDriver HTTP from a Python script against a
  scripted stub server (the tester playbook has the calls). A headless window cannot go below 500 CSS
  px, so check 375 px inside an iframe; check 2.4.11 (focus not obscured) by tabbing and comparing each
  focused element's rect with the sticky bar's.
- Reply shapes the stub lacks need no new stub: `page.route('**/api/node', (r) => r.fulfill({ json }))`
  (`build/ux-review/an5.js`: About, Node info, muted words, held read, Tab order, the 320 menu, 3
  engines x 1280/320 x themes x 200 %, about 9 minutes: run it in the foreground with a generous
  timeout, or it is moved to the background and must be waited on).
- Loading states: hold replies with `page.route('**/api/**')` and a delay, `/api/session` let through to
  see each page's own (`build/ux-review/ux17.js` shoots every page settled and loading). The stub serves
  one model profile document only, its id in `fe/ux/check.js`.
- Any 422 of a model profile, on the page's save and on the list's switch alike: `stub9.py`
  `/__set?pput=json&pcode=422&pbody=<json>` (it matches every `PUT /api/profiles/model/...`);
  `build/ux-review/m1.js` drives both surfaces in 3 engines at 1280 and 320 px and logs alerts, focus,
  `aria-invalid`, the re-read and sideways scroll. Do not run it beside the UX check: it doubles its time.
- `build/ux-review/r1.js` runs every page at 320 px in base, 200 %, 1.4.12 and both; `r1h.js` measures a
  table's scroll box (Tab ring to the box edge, arrows, computed lift, forced colours); `u17c-box.js`
  measures the Model profiles table's scroll box overflow at 320 px, with and without a CSS change;
  `gd1.js states|keys|save` with `gdstub.py` (devices states, profile keys, scriptable PUT, `/__reqs`)
  drives the GPU card; `t1.js`/`t1p.js` with `t1stub.py` (the UX stub plus a scriptable `PUT .../tags`,
  `/__reqs`) drive the tags field and measure the tiles; `c1stub.py` (`list=long`: 8 own tags of 32
  characters) and `c1.js` measure chips, the +, the pill and sideways scroll; `ls1shot.js` shoots the
  GPU card's field grid; `n1flex.js` compares nav flex bases by injection.
- Lucide shapes can be checked without `node_modules`:
  `curl -s https://unpkg.com/lucide-static@1.74.0/icons/<name>.svg` (the version `Icon.tsx` copies from).

- Apply `fe/DESIGN.md` principles 6 to 14 to every brief unasked: actions at the top and sticky; at
  most 2 clicks from the Overview for a common task (shortcut on the row or tile); list add and
  remove as `+` and `x`, delete as a `−` on each list row (disabled with a tip when it cannot act) and as text in the page's Save bar; `--space-3` between a list and its
  control; status beside its control; relations shown on lists; empty states with their next action;
  the instance's address; sizes as numbers, measured.

- A review also checks composition against a screenshot, not only that nothing moves and that axe
  passes. Take a full-page PNG at 1280 and 320 px (`page.screenshot`), look at it, and measure:
  - every control's centre against its row's or line's centre (within 1 px);
  - where each control sits in its column;
  - each column's width against its content;
  - the empty space at a row's end.
  `build/ux-review/lists-shots.js` is the model (user, 2026-11-25: nothing had pinned these).

## Pitfalls

- `display: flex` on a `td` makes it a flex box, not a table cell: `vertical-align` stops applying
  and its content sits at the top. Brief a wrapper inside the cell instead.

- `page.addStyleTag` puts a candidate CSS fix last in the cascade, so it wins ties the real
  sheet would lose: prepend it to `head` (or raise its specificity) before naming it as measured.

- A floating tip or popup inside a table's `.table-scroll` (overflow-x auto, so it clips both axes)
  is cut off if `absolute`; brief it `position: fixed` placed by script (or `popover`): in the flow
  it resizes the row and shifts the table (user, 2026-11-25). In a review, measure
  its right edge against the scroll box and move the pointer from the control onto it (1.4.13).
- The stub's default `key=withheld` makes the `default` server profile page show
  `server-load-failed`; set `key=set` to render it.

- jsdom applies no CSS, so check specificity by eye: `input[type='text']` (0,1,1) with `font: inherit`
  beats a class such as `.mono` (0,1,0).
- Playwright's WebKit does not scroll a control into view on a script `focus()` (checkbox and text input
  alike), Chromium does: a focus-after-refusal check at 320 px must read the focused element's rect per
  engine, and whether real Safari behaves the same is unverified.
- In Playwright, `goto` to the same hash URL does not reload: open a new page per scenario
  (`build/ux-review/c2.js`, `c2long.js` for a long option).
- Firefox makes an overflowing scroll box its own Tab stop.
- Hash routes keep `.` and `..` (the URL parser resolves dot segments in the path only), but the same id
  in an API path does not survive: `new URL('/api/instances/..', base)` is `/api/`.
- A closed native `select` fires `change` on each arrow key (Chromium, Firefox): a select whose change
  would write (a server profile switch) needs an explicit button.
- Before writing "wraps by the body's `break-word`" in a brief, check the parent: in a flex or grid item
  `break-word` leaves the min-content at the longest word (`fe/DESIGN.md`, the wrapping section), so a
  long unbroken value needs `anywhere` on that item (`.tile-facts dd` measured 18 px sideways at 320).
- In a `display: flex` label, an id span with `overflow-wrap: anywhere` shrinks to one character per
  line before the name beside it wraps ("CUD/A0" at 1280 px): put id and name in one inline span, or let
  the label wrap.
- In a wrapping flex row, a field's visible label and its input must be one flex item (a wrapper span),
  or the label ends one line and the input starts the next (`build/ux-review/gd1.js states` shoots it).
- `.field-control` is a wrapping flex row: anything a brief puts "under the row" inside it needs
  `flex-basis: 100%` and, if it holds user labels, `min-width: 0` plus `overflow-wrap: anywhere`; say so
  in the brief (a 32-character option once scrolled 267 px).
- In a wrapping flex pill (`.nav a:has(> .icon)`), a text item with no flex basis drops whole onto its
  own line under the icon as soon as its max-content is wider than the space left, and `min-width: 0`
  alone does not stop it: `flex: 1 1 4rem` keeps it beside the icon at 100 % and lets it move under at
  200 %, as H8a.5 does.
- The GPU card's blocks, Auto and malformed lines sit in a `.field` grid of two columns from 720 px: a
  new direct child of a fieldset that is a `.field` takes a grid cell and can push the list into the
  200 px label column, so measure the children's x at 1280 px.
- Every `button` has `min-height: var(--control-h)` (40 px), so an icon button inside a chip or pill
  needs its own min-height.
- An overlay over a chip must leave its x uncovered, or 1.4.13's "hoverable" fights the click; a cut
  chip already spans the row, so an overlay of the chip's width stays in the viewport.
- An `em` media query follows the browser's text size, not the page's: `@media (max-width: 15em)` catches
  large text on a narrow screen, and an injected `html { font-size: 200% }` does not trigger it.
- Before asking for a portal, grep `fe/src/index.css` for `overflow` on the overlay's ancestors.
- The manager's memory check (`model-profile-over-memory`) estimates from the model header, `ctx-size`
  and `cache-type-k`/`-v` only, all layers on GPU (`service.rs` `estimated_of`): advice that names GPU
  layers or MoE CPU layers would not clear it, so read the check before writing a fix.
- `-ot` devices match exactly (llama.cpp looks buffer types up by exact name), unlike `device`,
  `device-draft` and `mmproj-device` (case-insensitive); the GUI's per-row `,`/`=` stripping is a
  refusal at Save on the web.
- The browser's `RegExp` is a newer ECMAScript than `std::regex` (ES3: no lookbehind, no named groups,
  cppreference), so counts made in the page are advice.
- The GUI's block sliders (`pai-app` `gpu_split.rs` `set_blocks`) write the counts as the `tensor-split`
  weights and show blocks only under `layer`, a model read and an explicit vector (Auto keeps disabled
  fields; Even is the way in), and the `n-gpu-layers` slider is 0..block_count while the Layout's places
  block_count + 1 (the output layer): a web editor whose kind depends on its own typing (a weight field
  becoming a slider once the vector is explicit) loses the focus, so keep the kind tied to controls
  elsewhere and hide editors while the facts read is pending.
- The labels in use across the node are `GET /api/instances` items' `tags`/`instanceTags` plus
  `GET /api/profiles/server` rows' `tags`; an icon-only button keeps its words in a visually hidden span.
- A tile's chips take no Tab stop because its link leads to a page showing every tag whole: chips
  elsewhere need that page too, or text.
- Fixtures that leave a new field out keep the tests that find a link by its old name green.
- Before making a model profile key editable, check the save's accepted list (`api-contract.md`
  "Accepted keys", `WEB_MODEL_PROFILE_KEYS`): a key outside it is 400, so the brief names the bff
  change first. The profile document has no `assignments`; only the list rows do.
- The user had to ask for the + spacing, list icons, delete beside the status and actions on top: a
  brief that passes axe-core can still fail the user; check the brief against principles 6 to 14.
- A filtered copy of `fe/ux/check.js` made with line numbers breaks when the states move: find the end
  of `const STATES = [` by pattern (the first `^]$` after it) each time. Playwright's `click` waits
  forever on an `aria-disabled` button; use `click({ force: true })` to check that a switched-off
  control sends nothing.
