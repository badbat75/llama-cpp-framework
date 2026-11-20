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
