---
name: designer-playbook
description: What the designer has learned about the web UI's design and user experience. Use when writing a design brief, reviewing a UI slice or editing fe/DESIGN.md.
---

Lessons only: the rules are in `AGENTS.md` and the role in `.opencode/agents/designer.md`.

- Every text the page shows is a key in `fe/src/strings/en_US.json`, never inline (the project's
  "No user-visible text in the code"). A brief names the key and its English text; numbers, units,
  acronyms and paths stay out of the strings.
- No em-dashes, in briefs, `fe/DESIGN.md` and proposed texts alike (`AGENTS.md`, "Key conventions").
- Who acts through the bff is decided (`PLAN.md` step 10): the user signs in with their OS account and
  the manager applies its rules to them. Until the bff's sign-in exists (step 10 slice 6), the real
  page is read-only; briefs for editing are built against `fe/briefs/api-contract.md` and its mocks.
- Errors follow the contract: `{key, args}` with a strings key (`fe/briefs/api-contract.md`); today's
  bff still returns `unavailable`, `not-authorized` and `failed` until slice 6 moves them.
- `pai-config` (Slint, `crates/config/`) is the reference for what each task looks like today; its
  texts are in `crates/core/strings/en_US.toml`.
- To see a slice rendered before the bff has its routes: `npm --prefix fe run build`, copy `fe/dist`
  to the scratchpad and add the API replies as static files (`api/session`, `api/version`,
  `api/instances/index.html` since `instances` is also a folder, `api/instances/<id>/server`), served
  on 127.0.0.1 by a small Python server. Firefox here is a snap: its `--profile` and screenshot paths
  must be under `~/snap/firefox/common/` (it cannot see `/tmp`), and deleting that folder afterwards
  may be refused by the permission check: tell the user. `--headless --screenshot` shoots at the load
  event, before the fetches answer: add to the copied `index.html` an `<img>` the server answers after
  2 s. Dark mode: `user.js` with `ui.systemUsesDarkTheme` 1. Clicks and focus cannot be driven this
  way; those stay read from the code.
- jsdom applies no CSS, so the review checks specificity by eye: `input[type='text']` (0,1,1) with
  `font: inherit` beats a class such as `.mono` (0,1,0).
- Clicks and focus can be driven after all: `/snap/bin/geckodriver` over plain WebDriver HTTP from a
  Python script against a scripted stub server (the tester playbook has the calls). A headless window
  cannot go below 500 CSS px, so check 375 px inside an iframe; check 2.4.11 (focus not obscured) by
  tabbing and comparing each focused element's rect with the sticky bar's.
- A brief that says "join the read in flight" must name which reads may not join. Forced colours:
  set `browser.display.document_color_use` to 2 in the profile, and remove it from `prefs.js` after.
- Presets are shared by every instance of an engine, the manager reads the preset files without
  asking for a right, and the GUI lists switched-on presets before switched-off ones, each in file
  order. A preset's engine is the file that holds it: llama-server refuses unknown preset keys.
- Before proposing a bff reply, check what it links: `pai-bff` links `pai-core` only (`ReadFile`,
  `ini::parse_sections`), while the preset logic (`; url =` comment, `listed_in`, the spec-type
  helpers) is private to `pai-app`. Say which helper the bff must move or redo.
- The tester and coder run on a small local model: a slice of more than ~12 criteria fails. Split a
  big slice into sub-slices, the first settling every shared rule, the rest field tables (key,
  label key with toml line, hint, control, absent value, enable rule); put them in their own brief
  file so the tester reads only that (`fe/briefs/02e-model-profile-cards.md`).
- GUI preset hints sometimes name a slider or a number the GUI parks a ticked default box on
  (`crates/config/src/form.rs` `HINT_*`, e.g. 32768 for Context size, invented): the web shows no
  such number, so those lines are cut from the web's hint. The GUI also reads unparseable values as
  unset (`ini::parse_bool` takes only `true` / `false`); the web shows the file as written.
- Hash routes keep `.` and `..` (the URL parser resolves dot segments in the path only), but the
  same id in an API path does not survive: `new URL('/api/presets/..', base)` is `/api/`.
- Text at 200 %: inject `html { font-size: 200% !important }` (every size is in `rem`; equals
  Firefox's real 32 px default, `font.size.variable.x-western`). `build/ux-review/r1.js` runs every
  page at 320 px in base, 200 %, 1.4.12 and both; `r1h.js` measures the table's scroll box (Tab ring
  to the box edge, arrows, computed lift, forced colours). Firefox makes an overflowing scroll box
  its own Tab stop.
- The manager's memory check (`model-profile-over-memory`) estimates from the model header,
  `ctx-size` and `cache-type-k`/`-v` only, all layers on GPU (`service.rs` `estimated_of`): advice
  that names GPU layers or MoE CPU layers would not clear it. Read the check before writing a fix.
- Playwright's WebKit does not scroll a control into view on a script `focus()` (checkbox and text
  input alike, 2026-11-16); Chromium does. A focus-after-refusal check at 320 px must read the focused
  element's rect per engine; whether real Safari behaves the same is unverified.
- Any 422 of a model profile, on the page's save and on the list's switch alike: `stub9.py`
  `/__set?pput=json&pcode=422&pbody=<json>` (it matches every `PUT /api/profiles/model/...`);
  `build/ux-review/m1.js` drives both surfaces in 3 engines at 1280 and 320 px and logs alerts, focus,
  `aria-invalid`, the re-read and sideways scroll. Do not run it beside the UX check: it doubles its time.
- Before writing "wraps by the body's `break-word`" in a brief, check the parent: in a flex or grid
  item `break-word` leaves the min-content at the longest word (`fe/DESIGN.md` section on wrapping),
  so a long unbroken value needs `anywhere` on that item (T1: `.tile-facts dd`, 18 px sideways at
  320). `build/ux-review/t1stub.py` (the UX stub plus a scriptable `PUT .../tags`, `/__reqs`) and
  `t1.js`/`t1p.js` drive the tags field and measure the tiles.
- The web has no model facts (no `block_count`) until 2g, so the GUI's block sliders for
  `tensor-split` cannot be rebuilt; the GUI itself falls back to raw ratios when no model is read
  (`crates/config/src/gpu_split.rs` `build_rows`). Brief 14 uses ratio fields with a share text.
- In a `display: flex` label, an id span with `overflow-wrap: anywhere` shrinks to one character per
  line before the name beside it wraps ("CUD/A0" at 1280 px, brief 14): put id and name in one inline
  span, or let the label wrap. `build/ux-review/gdstub.py` (devices states, profile keys, scriptable
  PUT, `/__reqs`) and `gd1.js states|keys|save` drive the GPU card in 3 engines.
- The labels in use across the node are `GET /api/instances` items' `tags`/`instanceTags` plus
  `GET /api/profiles/server` rows' `tags` (brief 15). The fe's icons were all decorative beside
  text until brief 15's chips: an icon-only button keeps its words in a visually hidden span.
- In a wrapping flex row, a field's visible label and its input must be one flex item (a wrapper
  span), or the label can end one line and the input start the next (brief 14 at 1280 px: "Weight"
  beside the memory figures, its box on the line below). `build/ux-review/gd1.js states` shoots it.
- A closed native `select` fires `change` on each arrow key (Chromium, Firefox): a select whose
  change would write (a server profile switch) needs an explicit button (brief 16 decision 6).
- Destructive actions use Confirm in place, not a dialog (coordinator, 2026-11-21); creates use the
  Create form in place; a write that needs a restart shows the Restart offer (`fe/DESIGN.md` 5.2).
  `pai-config` and `pai` have no server profile pool, instance create or assignment: the web is first.
- Every `button` has `min-height: var(--control-h)` (40 px), so an icon button inside a chip or
  pill needs its own min-height, and `HearthTokens.test.ts` H1a.12 freezes every min-height and
  min-width: a brief that sets a size amends that list by name, as it amends R1.8's classes (brief
  15 R2). `build/ux-review/c1stub.py` (`list=long`: 8 own tags of 32 characters) and `c1.js`
  measure chips, the +, the pill and sideways scroll in 3 engines, 1280/320, both themes, 200 %.
- Long labels in a chip: one line with an ellipsis, wrapping only as the last resort (user,
  2026-11-21). An `em` media query follows the browser's text size, not the page's, so
  `@media (max-width: 15em)` catches large text on a narrow screen; an injected
  `html { font-size: 200% }` does not trigger it (brief 15 R3).
- An overlay over a chip must leave its x uncovered, or 1.4.13's "hoverable" fights the click; a
  cut chip already spans the row, so an overlay of the chip's width stays in the viewport (R5).
- React 19 features have their jobs in `fe/DESIGN.md` 5.3 (optimistic writes, pending states,
  deferred filtering, portals only past a clipping ancestor). Before asking for a portal, grep
  `index.css` for `overflow` on the overlay's ancestors.
- Loading states are seen by holding replies in Playwright (`page.route('**/api/**')` with a delay,
  `/api/session` let through to see each page's own): `build/ux-review/ux17.js` shoots every page
  settled and loading. The stub serves one model profile document only (its id in `check.js`).
- Stop every stub or server you start before you report (`kill <pid>` of the one you launched, never
  by pattern), and start none in the background that you would wait on: on 2026-11-21 three stubs
  (`build/ux-review/n1stub.py`, two `fe/ux/stub.py`) were left running for up to a day, and the
  review task stayed "running" for hours after its last report.
- The UX check now runs 102 states per browser and takes about 10 minutes (2026-11-21): a
  `timeout 400` killed it with no output. Send it to a file (`> build/ux-review/<slice>/check.txt`),
  read the lines without a leading space (the stub's BrokenPipe tracebacks are noise). `u17c-box.js`
  measures the Model profiles table's scroll box overflow at 320 px, with and without a CSS change.
