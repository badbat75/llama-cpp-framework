# 05: preset becomes model profile (the web UI's rename)

The web UI's half of `PLAN.md` step 10 item 13a, as ONE mechanical slice with **no behaviour change**.
The authority for the names is `ARCHITECTURE.md`, "Nodes and instances", block "Revision: profiles"
(commit f3a519c), its table *Names* and its *(f')* routes and error keys. This brief is the fe's map;
`fe/briefs/api-contract.md`, "Planned: profiles, nodes and instances", is the wire side.

## When

- **After 2h is committed** (the preset save, today uncommitted in `PresetCards.tsx`, `PresetPage.tsx`,
  `en_US.json`, `test/bff.ts`, `test/presetCards.ts`, `PresetSave14.test.tsx`): the rename touches the
  same files. Every key or test 2h adds before then follows the rules below.
- **At the pause of the platform's slice 4 step 4b** (the wire names: routes, error keys), back to back
  with it: the fe's fetch paths and error keys in this slice match only the renamed bff. The bff embeds
  `fe/dist`, so one build carries both; the window to keep short is the repo's, between the two commits.
- The fe's mocked tests pass whatever the bff serves; the check that the two agree is the UX check's
  stub (`fe/ux/stub.py`), renamed here to the new routes.

## What changes (the map)

The rule, applied everywhere below: `preset` -> `model-profile` in kebab case (keys, ids, classes),
`presets` -> `model-profiles`; `preset` -> `modelProfile`, `Preset` -> `ModelProfile`, `PRESET` ->
`MODEL_PROFILE` in code names; "preset" -> "model profile" in displayed text, sentence case at the
start of a text or heading ("Model profile", "Model profiles"), lower case inside a sentence. The one
word that stays: llama.cpp's source path `common/preset.cpp` (cited in two comments).

### 1. String keys (`fe/src/strings/en_US.json`)

Every key that contains `preset` (at the time of writing 199: 173 `preset-*`, 21 `presets-*`, 5
`api-*`; 2h has added more since) is renamed by the rule; no key is added or removed. The 5 `api-*` ones follow the bff's renamed error keys:

| Old key | New key |
| --- | --- |
| `api-preset-changed-elsewhere` | `api-model-profile-changed-elsewhere` |
| `api-preset-no-model` | `api-model-profile-no-model` |
| `api-invalid-preset-id` | `api-invalid-model-profile-id` |
| `api-preset-name-taken` | `api-model-profile-name-taken` |
| `api-no-preset-named` | `api-no-model-profile-named` |

Examples of the rest: `presets-title` -> `model-profiles-title`, `presets-filter-count` ->
`model-profiles-filter-count`, `preset-back` -> `model-profile-back`, `preset-context-size` ->
`model-profile-context-size`, `preset-written-as` -> `model-profile-written-as`. A new key never
collides with a kept one: no key starts with `model-` today. `models-title` ("Models") and
`nav-models` ("Models") keep key and text in this slice (they move with the navigation, 05b).

### 2. Displayed texts: 32

Every text that says "preset", with its new English. Keys not listed keep their text (only the key
changes). The `api-*` and `load-on-startup` texts copy the GUI's `crates/core/strings/en_US.toml`
lines; if the platform's slice 4 words them differently, the toml's words win and the fe copies them.

| Key (new) | New English |
| --- | --- |
| `server-intro` | llama-server settings of this instance, shared by every model it serves ({0}). Per-model settings belong to the model profiles. |
| `api-load-on-startup-over-max` | {0} enabled model profiles would load on startup, but the server loads at most {1} models at once (ModelsMax) and would not start. Turn Load on startup off on another model profile, or raise ModelsMax |
| `api-load-on-startup-unlimited` | {0} enabled model profile(s) would load on startup with ModelsMax = 0, which llama-server rejects at startup (it reads 0 as a limit there, not as unlimited). Set ModelsMax to at least {0}, or turn Load on startup off |
| `api-model-profile-changed-elsewhere` | This model profile changed elsewhere (edited, or switched on or off), or the web service restarted, after you opened it. Reload to see it as it is now; your edits here will be lost. |
| `api-model-profile-no-model` | model profile \`{0}\` names no model: every model profile needs a \`model\` path |
| `api-invalid-model-profile-id` | invalid model profile id \`{0}\`: use letters, digits, '.', '-', '_' |
| `api-model-profile-name-taken` | a model profile named \`{0}\` already exists |
| `api-no-model-profile-named` | There is no model profile named {0} any more: it was renamed or deleted elsewhere. Open the Models page again to see the model profiles as they are. |
| `model-profiles-title` | Model profiles |
| `model-profiles-intro` | Each model profile is one model with its settings. Every instance of the profile's engine can serve it. |
| `model-profiles-column-name` | Model profile |
| `model-profiles-loading` | Loading the model profiles… |
| `model-profiles-empty` | No model profiles yet. |
| `model-profiles-load-failed` | The model profiles could not be read. |
| `model-profiles-off-note` | A model profile switched off is kept, but its engine does not serve it and OpenCode does not offer it. |
| `model-profiles-filter-label` | Filter model profiles |
| `model-profiles-filter-hint` | Matches part of the model profile name or of its model file path. Case does not matter. |
| `model-profiles-filter-count` | Model profiles shown: {0} of {1}. |
| `model-profiles-no-match` | No model profiles match the filter. |
| `model-profile-back` | All model profiles |
| `model-profile-loading` | Loading the model profile… |
| `model-profile-load-failed` | The model profile could not be read. |
| `model-profile-not-found` | There is no model profile named {0}. |
| `model-profile-off` | This model profile is switched off: it is kept, but its engine does not serve it. Switch it on in the model profiles list. |
| `model-profile-settings-title` | Model profile settings |
| `model-profile-engine-hint` | The engine whose model profile files hold this model profile. Every instance of that engine can serve it. |
| `model-profile-engine-unavailable` | {0} is not available on the server, so no instance can serve this model profile until it is installed again. |
| `model-profile-load-on-startup-hint` | today's text with "a preset added" -> "a model profile added", "more presets load" -> "more model profiles load", "(preset only, no command-line flag)" -> "(model profile only, no command-line flag)"; the rest byte for byte |
| `model-profile-vision-gpu-hint` | today's text with "this preset's device list" -> "this model profile's device list"; the rest byte for byte |
| `model-profile-vision-off` | Used only with an MMProj file, and this model profile has none (Assets). |
| `model-profile-draft-file-off` | Draft device and Draft GPU layers apply only to a Draft model file, and this model profile has none (Assets). |
| `model-profile-fit-invalid` | llama-server refuses this value, so this model profile does not start. Use on or off. |

The copies of these texts in `fe/src/test/bff.ts` change with them.

### 3. DOM ids: 5 fixed, 4 patterns

| Old | New | Where |
| --- | --- | --- |
| `presets-title` | `model-profiles-title` | the list's `h2`, the table's `aria-labelledby` |
| `presets-filter` | `model-profiles-filter` | the filter input, its `label for` |
| `presets-filter-hint` | `model-profiles-filter-hint` | the hint, the input's `aria-describedby` |
| `preset-engine` | `model-profile-engine` | the page's Engine select |
| `preset-save` | `model-profile-save` | the page's save bar (`saveId`) |
| `` `preset-${key}` `` | `` `model-profile-${key}` `` | `fieldId`: every field of the cards |
| `` `preset-full-${...}` `` | `` `model-profile-full-${...}` `` | the full-value notes |
| `` `preset-group-${...}` `` | `` `model-profile-group-${...}` `` | the subgroup headings |
| `` `preset-written-${...}` `` | `` `model-profile-written-${...}` `` | the "Written as" notes |

Every `aria-labelledby`, `aria-describedby` and `for` that points at one of them follows.

### 4. CSS classes (`index.css` and the TSX that uses them): 10

`presets` (the table) -> `model-profiles`; `presets-note`, `presets-filter`, `presets-filter-row`,
`presets-filter-hint`, `presets-filter-status`, `presets-switch`, `presets-switch-status`,
`presets-switch-alert` -> the same with `model-profiles-`; `preset-back` -> `model-profile-back`. The
rules' content does not change; the comments in `index.css` that say "preset" follow the rule.

### 5. Files, by `git mv`

The rule: **every file under `fe/src` with `Preset` or `preset` in its name** is renamed by it, keeping
the rest of the name (`PresetPage.tsx` -> `ModelProfilePage.tsx`, `PresetCards{1..5}.test.tsx` ->
`ModelProfileCards{1..5}.test.tsx`, `PresetSave{1..16}.test.tsx` -> `ModelProfileSave{1..16}.test.tsx`,
`test/presetCards.ts` -> `test/modelProfileCards.ts`). At the time of writing that was 26 files; 2h
has added more since (`PresetSave15`, `PresetSave16`), and the rule, not a count, is the list.

`Models.tsx`, `Models.test.tsx`, `ModelsFilter.test.tsx`, `ModelsSwitch.test.tsx` keep their names here
(the page is still "Models" at `#/models`; 05b renames it). Two brief files the code cites in comments
(36 citations) are renamed by the designer before the coder starts, and the comments follow:
`fe/briefs/02e-preset-cards.md` -> `02e-model-profile-cards.md`, `fe/briefs/02h-preset-save.md` ->
`02h-model-profile-save.md` (their text stays: a brief is a record of its slice).

### 6. Code names

By the rule, every identifier, type, local and comment in `fe/src`. The named ones: the components
`PresetPage`, `PresetCards`, `PresetField`, `PresetRow`; the types `Preset`, `PresetDoc`; the functions
`isPreset`, `isPresetDoc`, `presetKey`, `sendPresetSave`, `presetHref`, `presetIdUnreadable` (in
`route.ts`); the test constant `PRESET_CARDS`; the route value `{ page: 'preset' }` ->
`{ page: 'model-profile' }` (`route.ts`, `App.tsx`); the `presets` state of `Models.tsx` and every
helper of `test/bff.ts` that says preset.

### 7. Fetch paths: 3 templates, 4 call sites

| Old | New | Call sites |
| --- | --- | --- |
| `GET /api/presets` | `GET /api/profiles/model` | `Models.tsx` |
| `PUT /api/presets/{id}/enabled` | `PUT /api/profiles/model/{id}/enabled` | `Models.tsx` |
| `GET`, `PUT /api/presets/{id}` | `GET`, `PUT /api/profiles/model/{id}` | `PresetPage.tsx` (2) |

**Corrected after the platform's step 4b (commit 5d54590):** the server route did NOT move. `GET` and
`PUT /api/instances/{id}/server` stay as they are, and so do the run verbs and boot
(`/api/instances/{id}/start|stop|restart|boot`) and `GET /api/instances`. The server route's move to
`/api/profiles/server/{id}` (planned as this table's fourth row) belongs to 05c.

### 8. JSON fields: none

No body or reply field the fe reads or sends changes in step 4b (the model profile rows, the document,
the save and switch bodies, the server document and its save are byte for byte today's).

### 9. Error keys the fe maps: 5

| Old | New | Where the fe maps it |
| --- | --- | --- |
| `no-preset-named` | `no-model-profile-named` | `Models.tsx` switch failure; `PresetPage.tsx` read and save |
| `preset-changed-elsewhere` | `model-profile-changed-elsewhere` | `PresetPage.tsx` save |
| `preset-no-model` | `model-profile-no-model` | `PresetPage.tsx` refusal text and field (`model`) |
| `invalid-preset-id` | `invalid-model-profile-id` | `PresetPage.tsx` refusal text |
| `preset-name-taken` | `model-profile-name-taken` | `PresetPage.tsx` refusal text |

**Corrected after step 4b:** the server routes still answer `client-no-instance`, and the fe keeps
mapping it there (`ServerSection.tsx` read, `ServerForm.tsx` save) as everywhere else. The row
`client-no-instance` -> `client-no-server-profile` planned here belongs to 05c, with the route. Not
mapped today and still not: `presets-malformed` (-> `model-profiles-malformed` since step 4b, shown as
the generic failure).

### 10. Hash routes: none in this slice

`#/models`, `#/models/<id>` and `#/instances/<id>` stay. They move in 05b (`#/profiles/model`,
`#/profiles/model/<id>`) and 05c (`#/profiles/server/<id>`).

### 11. The UX check (`fe/ux/`)

`stub.py`: the routes of section 7 and the key of section 9 (`no-model-profile-named`); `check.js`: the
state names `preset` -> `model-profile`, `preset-missing` -> `model-profile-missing`, the selector
`#presets-filter` -> `#model-profiles-filter`, the string keys it reads (`model-profile-assets`,
`model-profile-not-found`); `README.md`: its words.

## What must NOT change

- Behaviour: every request (method, body, headers), every reply read, every state, focus move, live
  region and dialog is today's. Layout, styles, the order of everything on every page: today's.
- The Server form stays on the instance page (`#/instances/<id>`), with the run verbs and boot. It moves
  to a server profile page in 05c, not here.
- The Models page keeps its name, its route and its `h1` "Models"; its `h2` now reads "Model profiles".
- No test is added or removed; no assertion changes except names, texts and paths of this map.

## How to check it

1. Before: `npm --prefix fe test`, `typecheck`, `lint`, `format:check` green; note the number of tests.
2. After: the same four green, **the same number of tests**.
3. This grep, from the repo root, returns nothing:

       grep -rniE 'preset' fe/src fe/index.html fe/ux/check.js fe/ux/stub.py fe/ux/README.md | grep -v 'common/preset\.cpp'

4. (Moved to 05c after step 4b: the server route and `client-no-instance` stay in this slice, so the
   greps for `/api/instances/{id}/server` and for `client-no-instance` in `ServerSection.tsx` and
   `ServerForm.tsx` are 05c's check, not this one's.)
5. `npm --prefix fe run build`, then the designer's UX check `npm --prefix fe/ux run check`: the seven
   states pass in the three engines, 1280 and 320 px, light and dark, no axe violation.

## Order (the tree is never committed half-renamed)

1. Precondition: 2h committed; `fe/` clean; the bff's slice 4 ready at the same pause.
2. Designer: renames the two brief files (section 5) and updates `fe/DESIGN.md`'s words.
3. Tester: `git mv` the test files and `test/presetCards.ts` (the rule of section 5); renames, in them and in `test/bff.ts`,
   the keys, texts, ids, classes, paths and error keys of this map; shows them red (the imports of the
   renamed source files fail, the old paths and keys are not answered).
4. Coder, in this order: `en_US.json` (keys and texts in one pass); the source files by `git mv` and
   their imports; code names; DOM ids and classes with `index.css`; fetch paths; error keys;
   `fe/ux/`. Then the four commands and the grep of step 3.
5. Designer: build and UX check; review against this map.
6. One commit for the fe, beside the bff's slice 4 commit, in the same pause.

## The slices that follow, in build order (each small; the platform slice each needs)

- **05b Navigation, model side** (needs slice 4 only): the nav's "Profiles" group with its Model entry;
  `#/profiles/model` and `#/profiles/model/<id>` (old `#/models...` links: user question); `Models.tsx`
  becomes the model profiles page, its `h1` "Model profiles".
- **05c Server profile page** (needs the server route's move, not done in step 4b):
  `#/profiles/server/<id>` holds today's Server form and its save; the fetch path `GET`, `PUT
  /api/instances/{id}/server` -> `/api/profiles/server/{id}`, the document's `{id}` being the instance
  id of the page until 05e (a server profile is still the instance's own `server@<id>.ini`); the error
  key `client-no-instance` -> `client-no-server-profile` `[id]` on those routes only, shown with the
  same text as today (`instance-not-found` with the page's id), the run verbs and boot keeping
  `client-no-instance`; its check: `grep -rn "/api/instances/[^'\"\`]*/server" fe/src fe/ux/stub.py`
  and `grep -n "client-no-instance" fe/src/ServerSection.tsx fe/src/ServerForm.tsx` return nothing.
  The instance page keeps state, Start / Stop / Restart, start at boot and its associations (user,
  2026-11-18), and shows its server profile as a link; the Profiles group gains its Server entry.
- **05d Tags on the server profile page** (platform slice 12).
- **05d2 Tags on the instance page** (the instance's own tags, `PUT /api/instances/{id}/tags`; user,
  2026-11-18: tags on both the profile and the instance). Missing from the first list.
- **05e Instances with port, tags and node** (platform slice 13): Overview columns Port and Tags; the
  node's name over the nav; the server profile id from `GET /api/instances` `serverProfile`.
- **05f Server profiles list, create, delete** (platform slice 14): `#/profiles/server` with each
  profile's port, tags and the instance using it.
- **05g Assign a server profile** (platform slice 15): the instance page's Server profile select.
- **05h Model profiles of an instance** (platform slice 15): the assignment table (`all`, per row
  value, served here) and the resource refusals.
- **05i Where a change applies** (platform slice 15): the pool rows' `assignments`, the save's and the
  switch's `restart` list, the instance named in the refusals.
- **05j Create and remove an instance** (platform slice 16), `client-no-model-profiles` on Start.
- Later: the estimate as a read (platform slice 17).

## The user's answers of 2026-11-18 (they bind the slices above)

- **The instance page** shows the instance's state, the run verbs (Start / Stop / Restart), start at
  boot and its associations (its server profile, its model profiles); the Server form leaves it for the
  server profile page (05c).
- **The menu** names its two entries "Server profiles" and "Model profiles" (05b, 05c; the keys and
  texts are proposed in those briefs).
- **After a profile save**, the success message names the running instances the change reaches, each
  a link to its instance page; there is **no Restart button** in the message (05i).
- **On one node a server profile serves one instance**; across nodes the same server profile may be
  shared (05f, 05g: the list shows the one instance using it on this node).
- **Tags** go on both the server profile (05d) and the instance (05d2).
