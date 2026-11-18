# Brief 04: visual identity, direction B "Hearth" on the Profiles structure

Status: **direction chosen, round 2** (2026-11-18). The user chose (2026-11-17) direction B "Hearth"
(Atkinson Hyperlegible Next, warm paper, rounded cards, instances as tiles, an illustrated sign-in, the
arched-doorway mark), icons from a ready open set (**Lucide**) and a **light/dark switch**; building
starts after the preset save (02h, closed in commit 84cd76e). Round 2 re-specifies B on the information
architecture decided since round 1: `ARCHITECTURE.md`, "Nodes and instances", block "Revision: profiles"
and its "User decisions, second round"; `fe/briefs/api-contract.md`, "Planned: profiles, nodes and
instances"; `fe/briefs/05-profiles-rename.md` with its slices 05b to 05j. `fe/DESIGN.md` changes with the
first slice built (H1), not before.

Mockups (static, git-ignored, nothing loaded from the network; fonts are system stand-ins, icons are the
real Lucide paths):

- `build/ux-review/visual/b2-overview.html`: the shell with the Profiles nav and the Overview tiles.
- `build/ux-review/visual/b2-model-profile.html`: the model profile page top with the new section nav
  (add `#open` to the address for the open state).
- Generator and checks: `build/ux-review/visual/src/gen2.py`, `src/shots2.js`; screenshots
  `src/shots/b2-*.png` (light and dark; Overview at 1280 and 320, 320 with the menu open; model profile
  page at 320 closed and open, and at 1280).
- Round 1's three directions stay beside them (`a-instrument.html`, `b-hearth.html`, `c-signal.html`) as
  the record of what was compared.

Labels: **measured** (computed or run here, with how), **read** (in a source, with the URL), **assumed**
(a guess or a preference).

**Measured on the two mockups** (Playwright Chromium 1255 from `fe/ux/node_modules`, `src/shots2.js`,
2026-11-18): 0 sideways scroll, 0 network requests and 0 axe-core violations (WCAG 2.0 to 2.2 A and AA)
in each of the 12 runs (light, dark; the six views above). The first editable control of the model
profile page sits at **705 px** from the top at 320 px (today about 2900 px, the designer's 2h.10
review), 483 px at 1280.

## 0. What every slice keeps

- WCAG 2.2 AA is the floor; section 4 turns it into testable criteria for this palette.
- DOM order, roles, accessible names and ids stay as they are unless a slice below says "DOM". Every
  icon or the mark is an inline SVG with `aria-hidden="true"` and `focusable="false"`, inside an element
  whose name comes from its text, so no accessible name changes
  ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)).
- State never by colour alone: the badge keeps its shape and its text
  ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)).
- "Not editable" stays the dashed border (Hearth adds a faint hatch on top); forced colours keep
  `border-style` ([CSS Color Adjustment 1](https://www.w3.org/TR/css-color-adjust-1/#forced-colors-properties)).
- The sticky save bar, its `scroll-padding-bottom` rule and the short-viewport fallback stay
  ([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)); Hearth's 40 px controls make
  the bar taller, so the UX check re-measures it.
- Nothing loaded at run time: fonts are files in `fe/dist/assets/`, icons are paths in the JS bundle,
  both embedded in `pai-bff` with the rest of the UI (section 3).
- Every text is a key in `fe/src/strings/en_US.json`. Keys below that start with `model-profile` assume
  the rename (05) has landed; a slice that lands before it does not add such keys.

## 1. Direction B on the Profiles structure

### 1.1 Tokens, type, controls

**Palette** (measured: WCAG relative luminance, `/tmp/claude-designer/cr.py` in this session; ratio of
the token as foreground against each background).

| Token | Light | Dark | Use | Light: page, bg, surface, field, accent-soft | Dark: same order |
|---|---|---|---|---|---|
| `--page` | `#f7f1e8` | `#1b1714` | warm paper behind everything (new) | | |
| `--bg` | `#fffdf9` | `#231d19` | cards, tiles, save bar | | |
| `--surface` | `#f5efe6` | `#2b241f` | disabled fill, badges, tags, notices | | |
| `--field` | `#fffdf9` | `#1b1714` | enabled control fill (new) | | |
| `--line` | `#e6dccf` | `#3a3029` | decorative dividers only; the dark card outline (new) | 1.33 on bg | 1.30 on bg |
| `--text` | `#2a211c` | `#f1e9df` | body text | 14.0, 15.5, 13.8, 15.5, 12.9 | 14.8, 13.8, 12.7, 14.8, 11.0 |
| `--muted` | `#6b5d52` | `#b5a698` | hints, disabled values, secondary text | 5.65, 6.24, 5.55, 6.24, 5.18 | 7.52, 7.03, 6.45, 7.52, 5.56 |
| `--control` | `#8c7d70` | `#8a7c6f` | control borders, dashed disabled border, switch knob off | 3.54, 3.91, 3.48, 3.91, 3.25 | 4.40, 4.12, 3.78, 4.40, 3.26 |
| `--accent` | `#7b3a8f` | `#dcaaf0` | links, primary, focus ring, current nav, checked | 6.58, 7.27, 6.46, 7.27, 6.04 | 9.37, 8.76, 8.04, 9.37, 6.93 |
| `--on-accent` | `#ffffff` | `#1b1714` | text and knob on the accent | 7.38 on accent | 9.37 on accent |
| `--accent-soft` | `#f1e4f6` | `#3a2a44` | current nav item, node panel, Running badge, pressed theme switch (new) | | |
| `--danger` | `#b3261e` | `#ff9a8a` | errors, Failed | 5.82, 6.43, 5.72, 6.43, 5.34 | 8.68, 8.12, 7.45, 8.68, 6.42 |
| `--success` | `#2f6f3e` | `#7ccf8e` | Running, Saved | 5.40, 5.97, 5.30, 5.97, 4.96 | 9.47, 8.86, 8.13, 9.47, 7.01 |
| `--warning` | `#8a5300` | `#e8b45a` | Starting, Stopping, restart to apply | 5.64, 6.23, 5.54, 6.23, 5.17 | 9.42, 8.81, 8.08, 9.42, 6.97 |

Also `--hatch` (`rgb(42 33 28 / 0.07)` light, `rgb(241 233 223 / 0.06)` dark, decorative), `--shadow`
(two soft shadows in light; `0 0 0 1px var(--line)` in dark, where a shadow does not show: assumed), and
the illustration's `--art-1`, `--art-2`, `--art-3` (round 1's `src/b.css`).

- **Type**: Atkinson Hyperlegible Next 400 and 700; Atkinson Hyperlegible Mono 400 for paths, flags and
  values (never bold: no Mono 700 file ships, so a bold mono would be synthesized). Stack:
  `'Atkinson Hyperlegible Next', system-ui, sans-serif` and `'Atkinson Hyperlegible Mono', ui-monospace,
  'Cascadia Mono', Menlo, monospace`. Body 1.0625 rem (17 px), line height 1.55; scale 0.8125, 0.875,
  0.9375, 1.0625, 1.125 (card title), 1.375 (h2), 2 (h1). `font-display: swap`. The family is the
  Braille Institute's, "crafted for low-vision readers" (read:
  [brailleinstitute.org/freefont](https://www.brailleinstitute.org/freefont/)).
- **Spacing**: 4, 8, 12, 16, 24, 40, 64 px (`--space-1` to `--space-7`; 5 and 6 change from 24/32 to
  24/40). **Radius**: 10 px controls, 5 px checkbox, 16 px cards, 18 px tiles, 999 px pills (buttons,
  tags, badges, the switch, nav items).
- **Controls**: 40 px high (32 today), 1.5 px borders, 3 px focus ring offset 2 px; disabled = dashed
  border plus the 135 degree hatch; the switch is a 44 by 24 px pill drawn on the same native checkbox.
- **Mark**: the plum arched doorway with a keyhole (round 1's `src/gen.py` `MARK['b']`), 32 px in the
  banner; the favicon becomes the same drawing (a data URI in `fe/index.html`, today a blue "A").

### 1.2 Shell: banner and nav

**Banner** (one `header`, in this order): the mark and "Private AI Framework" as the home link (name
unchanged: the SVG is `aria-hidden`); then, pushed to the end, Menu (below 960 px, as today), the theme
switch (1.9), and the session part ("Signed in as alex" and Sign out, or Sign in). Below 480 px the
theme switch shows its icon only (its text stays in the DOM, visually hidden, so the name is unchanged)
and "Signed in as ..." is visually hidden the same way (restyle; a preference: Sign out already shows
that a session is open; measured in the mockup, the banner still takes three rows at 320 px).

**Nav** (signed in; signed out it holds Overview only, as today):

```
Overview                      (house icon)
garnet                        (server icon; the node's name, from GET /api/node: 05e)
  Instances                   (sub-label)
    ● default                 (state shape, as today)
    ○ coder
  Profiles                    (sub-label)
    Server profiles           (sliders-horizontal icon; 05c adds it, 05f gives it a list)
    Model profiles            (layers icon; 05b)
```

- Each sub-list is a `ul` labelled by its sub-label; once the node's name is shown, by both:
  `aria-labelledby="nav-node nav-instances"` and `"nav-node nav-profiles"`, so a screen reader hears
  "garnet Instances, list" and the structure holds when a hub lists several nodes. Until 05e there is no
  node line and the lists keep today's labels.
- Look: items are pills; the current one `--accent-soft` fill, `--accent` text, bold; a 2 px `--line`
  rule left of each sub-list (decorative). `aria-current="page"` on the page's own entry,
  `aria-current="true"` on the list entry of a page below it (a profile's page), as `fe/DESIGN.md` 2.3.
- Keys (fixed by 05b, 05c; proposed here so they agree): `nav-profiles` "Profiles",
  `nav-server-profiles` "Server profiles", `nav-model-profiles` "Model profiles". The node line shows
  data, no key.

### 1.3 Overview: node panel and tiles

1. `h1` "Overview" (unchanged).
2. **Node panel** (05e; `--accent-soft`, radius 20 px): eyebrow "Node" (server icon), the node's name
   (1.5 rem bold, a `p`, not a heading: the `h1` stays the page title), the version line (today's
   `Version`, moved inside). Before 05e the version line stays where it is.
3. `h2` "Instances", then a **list of tiles** (`ul.tiles`, `li.tile`), grid
   `repeat(auto-fill, minmax(min(100%, 15rem), 1fr))`. Each tile, in DOM order:
   - the instance id as the link to its page (1.25 rem bold), and the state badge (pill with its
     shape and text) on the same row, wrapping below at narrow widths;
   - for Failed, today's hint line;
   - when more than one engine exists, today's `instance-engine` line;
   - a `dl` of facts, each row an icon, the term and the value: **Port** (mono; `(none)` when the
     reply's port is 0), **Tags** (pills: the reply's `tags`, the profile's first, then the
     instance's, as the bff sends them; `(none)` when empty), **Start at boot** (On / Off: only if the
     user answers question 1 with "everyone"; with the recommended answer the row shows only when
     signed in, read per tile from `GET /api/instances/{id}/boot`).
   - Port and Tags arrive with 05e; before it the tile has no `dl`.
4. Keys (new): `instances-port` "Port", `instances-tags` "Tags", `instances-none` "(none)",
   `instances-boot` "Start at boot", `instances-boot-on` "On", `instances-boot-off` "Off",
   `overview-node` "Node". Round 1's summary line ("4 instances: 1 running ...") is dropped: the tiles
   say it, and its plurals need more than one string (a preference).

### 1.4 Instance page (`#/instances/<id>`, after 05c)

The instance page only shows state and makes associations (`ARCHITECTURE.md`, second round (c)):

1. Header (no card): `h1` id; today's engine line; the state badge as a pill; Start, Stop, Restart as
   pill buttons with icons (`play`, `square`, `rotate-cw`); the run status line; the start at boot
   switch (`power` icon in its label) as today.
2. Card **Server profile** (05c: the profile id as a link to `#/profiles/server/<id>` and its port in
   mono; 05g adds the select described in `api-contract.md`).
3. Card **Model profiles** (05h: the assignment table).
4. Card **Tags** (the union shown as pills; editing the instance's own tags has no 05 slice yet:
   `PUT /api/instances/{id}/tags` is platform slice 12, 05d covers only the server profile's tags. A
   gap for the coordinator).

### 1.5 Server profile page (`#/profiles/server/<id>`, 05c)

1. Back link "All server profiles" (`chevron-left`) once the list exists (05f).
2. `h1` the profile id; a summary line under it: "Engine llama.cpp, used by default" with the instance
   as a link and its state shape, or "not used by any instance" (`server-profile-used-by` "Engine {0},
   used by {1}", `server-profile-unused` "Engine {0}, not used by any instance"; 05c's brief fixes them).
3. Today's Server form cards (the long form) restyled by H2, the sticky save bar, the post-save message
   naming the running instance linked (second round (e)). Delete (`trash-2`, secondary, danger text)
   only while unused (05f). No section nav in this round (a preference: the form has 5 cards).

### 1.6 Model profile page (`#/profiles/model/<id>`): the fix at 320 px

Today at 320 px: back link, `h1`, `h2`, the jump list (ten outlined secondary buttons, about six
rows, no visible label: it reads as ten actions), the engine Notice, the Engine select (read-only, one
option), the read-only Assets card (long paths), then the nine editable cards: the first editable
control is about 2900 px down.

Hearth layout (mockup `b2-model-profile.html`), in DOM order:

1. Back link "All model profiles" (`chevron-left`).
2. `h1` the profile id; under it a **summary line** (new, DOM): "Engine llama.cpp, model
   Qwen3.8-27B-Q8_0.gguf", the file name (last path segment) in mono. Keys
   `model-profile-summary` "Engine {0}, model {1}" and `model-profile-summary-no-model` "Engine {0}, no
   model file".
3. `h2` "Model profile settings".
4. **Section nav** (`nav` labelled by a visible lead-in "On this page"):
   - **Restyle (H9)**: the ten entries stay buttons (they move the focus, `fe/briefs/02h` 196) but drawn
     as text links: no border, `--accent` text, underlined, 32 px rows, a 2 px `--line` rule on the
     left; from **1280 px** the nav is a **sticky right-hand rail** beside the cards (CSS grid, `position:
     sticky; top: 16px`), always open. Below 1280 px, H9 alone shows the same text list in place (lighter,
     still about ten rows at 320 px).
   - **DOM (H10)**: the visible lead-in `p#model-profile-jump-label` "On this page" (the key
     `model-profile-jump-label`, today `preset-jump-label` "Preset sections.", changes its text and
     becomes visible; the `nav` takes `aria-labelledby` instead of `aria-label`). Below 1280 px the
     list is **collapsed** behind a disclosure button "On this page" (`chevron-down`, `aria-expanded`,
     `aria-controls` the list; the APG disclosure pattern,
     [WAI-ARIA APG](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/)), closed on each page load;
     pressing a section button moves the focus as today and closes the list. From 1280 px the button is
     not shown (CSS) and the list always is. The list is hidden by a class whose `display: none` applies
     only below 1280 px, **not** by the `hidden` attribute (the base rule `[hidden] { display: none
     !important }` would win in the rail; measured in the mockup).
   - Not chosen: a sticky section nav at 320 px (with the sticky save bar it would leave about half a
     320 by 568 viewport for content and risks [2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum));
     a `select` that jumps on change ([3.2.2](https://www.w3.org/TR/WCAG22/#on-input) asks for no
     change of context on a choice).
5. The engine Notice (`model-profile-engine-unavailable`) when the engine is missing: unchanged.
6. **Assets, collapsed (H11, DOM)**: the read-only card keeps its place and its dashed look; its `h3`
   holds a disclosure button "Assets" (`aria-expanded="false"` on load); `model-profile-assets-read-only`
   stays visible under the heading; the `dl` (files, Project URL, drafters) shows when expanded. The
   **Engine** becomes the first row of that `dl` (`model-profile-engine` "Engine", value the engine id)
   instead of a disabled one-option select; `model-profile-engine-hint` and the id
   `model-profile-engine` go. A section button "Assets" expands the card, then moves the focus to its
   heading as the others do. Question 4 asks whether the user wants this.
7. The nine editable cards, then the save bar, unchanged in DOM.

Result measured in the mockup: first editable control at 705 px at 320 px (H9 alone: about 1500 px,
assumed from ten 32 px rows replacing six 40 px rows plus the Assets card still open).

### 1.7 The two profile lists

- **Model profiles** (`#/profiles/model`, 05b): today's filter (search icon inside the field, H8),
  status line, table inside a card (radius 16, `--line` row dividers, muted column heads), the on/off
  switch as the Hearth pill. No DOM change beyond 05b's.
- **Server profiles** (`#/profiles/server`, 05f): the same table look; columns Server profile (link),
  Port (mono), Tags (pills), Used by (instance link with its state shape, or "(none)"); a primary
  "New server profile" (`plus`) above the table. 05f's brief fixes the texts.

### 1.8 Sign-in

Two columns from 840 px: the illustration (round 1's `ART_B`, inline SVG in theme tokens, in a
`div` with `aria-hidden="true"`) beside the form in a rounded card; stacked, illustration first and
smaller, below 840 px. Above the `h1` "Sign in", the node's name with the server icon once 05e serves
it. Texts, fields, errors and focus unchanged.

### 1.9 The light/dark switch

As specified for the recommended answer to question 3:

- One `button` in the banner, `type="button"`, text "Dark theme" (`theme-dark`), moon icon,
  `aria-pressed="true"` while the dark theme is in effect, `false` otherwise. The name never changes
  with the state ([WAI-ARIA APG, button](https://www.w3.org/WAI/ARIA/apg/patterns/button/): a toggle
  button's label does not change when its state does).
- On load: a stored choice wins; with none, the OS preference (`prefers-color-scheme`) decides, and the
  page follows it live while nothing is stored.
- Pressing it flips the theme. If the new theme equals the OS's, the stored choice is **removed**
  (the page follows the OS again); otherwise `light` or `dark` is stored. So there is no third
  "System" state to explain, and getting back to "follow my system" is one press.
- Storage: `localStorage` key `pai-theme`, values `light` | `dark`, per browser and per origin (the
  bff's address); every read and write in `try`/`catch`: when storage is refused the switch still works
  for the page's life. No server state, no cookie.
- Applied as `data-theme="light" | "dark"` on `<html>`, set in `main.tsx` before the first render (no
  inline script; `fe/index.html` has none today). CSS: light tokens on `:root`; dark tokens under
  `@media (prefers-color-scheme: dark) { :root:not([data-theme='light']) }` and under
  `:root[data-theme='dark']`; `color-scheme` follows, so native controls and scrollbars match.
- Out of scope: per-account themes, a high-contrast theme, following the OS's later change once a
  choice is stored.

## 2. Build order

Slice briefs: H1 is split into **H1a** (tokens, both themes, system fonts) and **H1b** (font files,
after the platform's "fonts backend ready"), `fe/briefs/06-hearth-h1-tokens.md`; H2 into **H2a**
(controls) and **H2b** (containers), `fe/briefs/06-hearth-h2-controls.md`; H3 is
`fe/briefs/07-hearth-h3-mark.md`, H4 `fe/briefs/07-hearth-h4-theme.md`, H5 is split into **H5a**
(banner), **H5b** (nav) and **H5c** (three carried save-bar issues), `fe/briefs/07-hearth-h5-shell.md`;
H6 into **H6a** (tiles) and **H6b** (start at boot, signed in), `fe/briefs/08-hearth-h6-overview.md`;
H7 is `fe/briefs/08-hearth-h7-signin.md`; H8 into **H8a** (component, shell) and **H8b** (pages),
`fe/briefs/08-hearth-h8-icons.md`; H9 into **H9a** (index look) and **H9b** (rail),
`fe/briefs/08-hearth-h9-jump.md`; H10, **H11a** (summary, collapsed Assets) and **H11b** (Engine as a
row), `fe/briefs/08-hearth-h10-h11.md`. Those briefs win where they differ from this table.

Each row is one test-first cycle (12 criteria at most, the playbook's limit). "Restyle" = CSS and tokens
only: checked by the designer's UX pass (`fe/ux`) and the stylesheet-text tests (as
`fe/src/DisabledLook.test.ts` reads `index.css`). "DOM" = markup or behaviour: the tester writes failing
tests first. **Before 05** = can land before the rename without being redone.

| # | Slice | What users see | Kind | Depends on | Before 05 |
|---|---|---|---|---|---|
| H1 | Tokens, type, fonts | warm paper, plum accent, Atkinson type, new scale and spacing, both themes | Restyle + the font files (section 3); stylesheet-text tests: every token in both theme blocks, the section 4 contrast pairs computed from the CSS text, `@font-face` URLs relative, no `http` URL in any CSS | 02h done | **yes** |
| H2 | Controls and cards | pill buttons, 40 px fields, 1.5 px borders, 3 px ring, hatch on disabled, the pill switch, rounded cards, notices, badges, tables in a card | Restyle; selectors must not name `preset`/`presets` classes (use `input[role='switch']`, `table`), or wait for 05 | H1 | **yes** |
| H3 | Mark and favicon | the doorway mark beside the name; the favicon | DOM (small): the home link's name stays "Private AI Framework", the SVG `aria-hidden`, `focusable="false"` | H1 | **yes** |
| H4 | Light/dark switch | the switch in the banner (1.9) | DOM + behaviour: initial state from the OS mock, press flips `data-theme` and `aria-pressed`, store and remove rules, storage that throws, the constant name | H1 | **yes** |
| H5 | Shell restyle | banner layout, nav pills, current item, sub-label rules, icon-only switch below 480 px | Restyle (generic selectors: `.nav a[aria-current]`, `.nav-label`), works on today's nav and on 05b/05c's | H2, H4 | **yes** |
| H6 | Overview tiles | instances as tiles (state, failed hint, engine line) | DOM: `table` becomes `ul` of `li` with link, badge, `dl`; `fe/src/Instances.test.tsx` row and cell queries rewritten | H2 | **yes**; Port, Tags, node panel join with 05e; boot per question 1 |
| H7 | Sign-in illustration | two columns with the illustration | DOM (decorative `div` + SVG `aria-hidden`) + layout; texts and focus unchanged | H2 | **yes**; node name with 05e |
| H9 | Section nav restyle | the jump list as a light text list; sticky rail from 1280 px | Restyle (`.jump` only, a class 05 keeps) | H2 | **yes** |
| H8 | Icons | the Lucide icons of section 3 in every placement | DOM (decorative); one test that every `svg` in the app is `aria-hidden` and `focusable="false"` and that names are unchanged | 05 (touches files 05 renames), 05b, 05c (nav entries) | no |
| H10 | Section nav, DOM | "On this page" visible; collapsed below 1280 px | DOM + behaviour (1.6 item 4) | H9, 05 | no |
| H11 | Summary line and collapsed Assets | the model file under the title; Assets closed; Engine as a row | DOM + behaviour (1.6 items 2 and 6) | 05; question 4 | no |

Not separate visual slices: the nav's Profiles group (05b, 05c), the node line (05e), the instance page
(05c, 05g, 05h), the server profile page and list (05c, 05f) are built **directly in Hearth** from
section 1 by their own slices, so nothing built there is restyled twice. If H1 to H5 have not landed when
those slices start, they use today's look and inherit Hearth from H1 to H5 later without change.

## 3. Assets for the platform side

Nothing is fetched at run time or at build time beyond what `npm ci` already does; the files are
committed in `fe/` and land in `fe/dist/`, which `pai-bff` embeds (`crates/bff/build.rs`).

**Fonts** (SIL Open Font License 1.1, no Reserved Font Name in either licence; read:
[OFL.txt, Next](https://github.com/googlefonts/atkinson-hyperlegible-next/blob/main/OFL.txt) "Copyright
2020-2024 The Atkinson Hyperlegible Next Project Authors";
[OFL.txt, Mono](https://github.com/googlefonts/atkinson-hyperlegible-next-mono/blob/main/OFL.txt) "Copyright
2020-2024 The Atkinson Hyperlegible Mono Project Authors"). Source: the unmodified WOFF2 files of
`@fontsource/atkinson-hyperlegible-next@5.3.0` and `@fontsource/atkinson-hyperlegible-mono@5.3.0`
(npm licence field "OFL-1.1"), copied into `fe/src/assets/fonts/` with a README naming package, version
and licence (a preference over an npm dependency: three files, no package to update). Upright only, no
italic (the UI uses none). Sizes measured from the jsDelivr listing
(`https://data.jsdelivr.com/v1/packages/npm/@fontsource/<family>@5.3.0?structure=flat`), in bytes:

| File | Latin | Latin Extended (question 2) |
|---|---|---|
| `atkinson-hyperlegible-next-<subset>-400-normal.woff2` | 12,096 | 7,376 |
| `atkinson-hyperlegible-next-<subset>-700-normal.woff2` | 12,732 | 7,556 |
| `atkinson-hyperlegible-mono-<subset>-400-normal.woff2` | 10,056 | 6,528 |
| Total | **34,884 (34.1 KiB)** | **+21,460 (+21.0 KiB), 56,344 in all** |

The Latin subset's `unicode-range` is U+0000-00FF plus a few marks and U+2000-206F (so it covers
English, the Western European accented letters and the ellipsis the strings use); Latin Extended is
U+0100-02BA and more (Polish, Czech, Turkish, Romanian and more) (read: `@fontsource/...@5.3.0/400.css`).
With one `@font-face` per subset and its `unicode-range`, a browser downloads the extended file only
when the page shows such a character ([CSS Fonts 4, `unicode-range`](https://www.w3.org/TR/css-fonts-4/#unicode-range-desc)),
so Latin Extended costs the binary and the packages 21 KiB, the page nothing until it is needed.

Vite emits each WOFF2 as its own hashed file in `fe/dist/assets/`: they are above
`build.assetsInlineLimit`, default 4096 bytes (read:
[Vite build options](https://vite.dev/config/build-options#build-assetsinlinelimit)).

**The bff's content type**: `crates/bff/build.rs` `content_type` answers `application/octet-stream`
for every extension but html, js and css. Fonts still load under `X-Content-Type-Options: nosniff`,
which blocks only script and style destinations (read:
[Fetch, 3.6.1](https://fetch.spec.whatwg.org/#should-response-to-request-be-blocked-due-to-nosniff?)),
but the right type is `font/woff2` (read: [IANA](https://www.iana.org/assignments/media-types/font/woff2),
RFC 8081). Proposed for the platform side: map `woff2` to `font/woff2` in that function, with H1.

**Icons**: Lucide, from `lucide-static@1.74.0` (npm licence "ISC"), copied as path data into one
component (`fe/src/Icon.tsx`, a map from name to the SVG children), drawn at 24 by 24 with
`stroke="currentColor"`, `stroke-width="2"`; no runtime package, no SVG files. The 24 icons, by Lucide
name, and where:

| Icon | Where |
|---|---|
| `house` | nav: Overview |
| `server` | nav: the node line; Overview node panel; sign-in node line |
| `sliders-horizontal` | nav: Server profiles |
| `layers` | nav: Model profiles |
| `menu` | banner: Menu |
| `moon` | banner: the theme switch |
| `log-in`, `log-out` | banner: Sign in, Sign out |
| `play`, `square`, `rotate-cw` | instance: Start, Stop, Restart |
| `power` | Start at boot (instance switch, tile row) |
| `plug`, `tag` | tile rows Port, Tags |
| `info`, `triangle-alert`, `lock-open` | notices: info, warning, the unencrypted sign-in notice |
| `circle-alert`, `circle-check` | save and run alerts; Saved and run success status |
| `search` | the model profiles filter |
| `chevron-left` | back links |
| `chevron-down` | the section nav and Assets disclosures |
| `plus`, `trash-2` | New server profile, Delete (05f) |

Licence: Lucide's `LICENSE` is ISC, with the icons derived from Feather under MIT (Cole Bemis); of the
24, `log-in`, `log-out`, `server`, `square`, `power`, `info`, `search`, `chevron-left`, `chevron-down`,
`plus`, `trash-2`, `moon` and the old names of `circle-alert` and `triangle-alert` are on its Feather
list (read: [lucide LICENSE](https://github.com/lucide-icons/lucide/blob/main/LICENSE)), so both texts
ship.

**Licence texts**: in the framework package that carries `pai-bff`, beside its other licences
(`share\licenses\` in the MSI, `/usr/share/private-ai/licenses/<package>/` on Linux; staged by
`xtask/src/common/licenses.rs`, `docs/build-packaging.md` "Every package carries its licences"):
`OFL-atkinson-hyperlegible-next.txt`, `OFL-atkinson-hyperlegible-mono.txt` (each the upstream file with
its copyright line), `lucide.txt` (the whole upstream `LICENSE`: ISC and MIT), and a line each in that
package's `README.txt`. The texts come from `installer/licenses/` with their source in its README, as for
the other copied texts. macOS ships no `pai-bff`, so nothing there.

## 4. Contrast and accessibility criteria (testable)

Each holds in **both themes**. The first group is checkable from `index.css` text by a unit test
(parse the two token blocks, compute the WCAG ratio); the rest by the UX check or a DOM test.

1. Text ([1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum), 4.5:1): `--text`, `--muted`,
   `--accent`, `--danger`, `--success`, `--warning` each on `--page`, `--bg`, `--surface`, `--field` and
   `--accent-soft`; `--on-accent` on `--accent`. Lowest today: success on accent-soft, 4.96 (light).
2. Non-text ([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast), 3:1): `--control` (borders,
   dashed disabled border, the switch knob off) on `--page`, `--bg`, `--surface`, `--field`,
   `--accent-soft` (lowest 3.25, dark); the state shapes (`--success`, `--warning`, `--danger`,
   `--muted`) on `--bg` and `--accent-soft`; the checked checkbox and the switch on: `--accent` against
   its surroundings and `--on-accent` mark on it.
3. Focus ([2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible); the ring meets 1.4.11 too): a 3 px
   `--accent` outline, offset 2 px, on `:focus-visible`, on every focusable element, including the
   pressed theme switch and the current nav item (`--accent` on `--accent-soft`: 6.04 / 6.93). The
   headings a section button lands on show the ring (today's rule).
4. `--line` (1.3:1) carries no meaning: no control, state or boundary that the user needs is drawn only
   with it (the tiles and cards are understood without their outline).
5. Targets ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)): every button, link in the nav,
   section button, disclosure, switch and checkbox at least 24 by 24 px; Hearth draws 32 to 40 px.
6. Reflow ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)): no sideways scroll at 320 px in any page,
   both themes, nav open and closed, section nav open and closed.
7. Text spacing ([1.4.12](https://www.w3.org/TR/WCAG22/#text-spacing)): pills and tiles have no fixed
   heights that clip text (min-height only).
8. State not by colour ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)): each badge keeps its shape
   and text in both themes and in forced colours.
9. The theme switch: a `button` with `aria-pressed` ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value));
   its accessible name is "Dark theme" in every state and at every width; pressing it moves no focus
   and announces nothing beyond the pressed state (no live region); the choice survives a reload in the
   same browser and does not exist in another; with storage throwing, the page renders and the switch
   works.
10. Forced colours: shapes, the drawn checkbox and the switch knob use system colours as today; icons
    and the mark are `currentColor` or theme tokens with `forced-color-adjust` left on, so they take
    system colours.
11. Disclosures (H10, H11): `aria-expanded` matches what is shown; Enter and Space toggle (native
    `button`); the controlled region follows the button in DOM order
    ([1.3.2](https://www.w3.org/TR/WCAG22/#meaningful-sequence)).
12. Motion: the chevron turn and the switch knob slide stop under `prefers-reduced-motion: reduce`.

## 5. Not chosen (round 1 record)

- **A, Instrument**: IBM Plex, an ink banner, hairline panels, the Overview kept as a status table.
  The least marked; the user wanted a stronger identity.
- **C, Signal**: Space Grotesk and JetBrains Mono, square 2 px frames, a cyan accent, uppercase mono
  labels. The boldest, but the heaviest fonts (80.7 KiB) and the starkest light theme.
- Phosphor icons (round 1's B set) were replaced by Lucide at the user's choice. Both directions'
  pages and measurements stay in `build/ux-review/visual/` (round 1 text: this file at commit 16530de).

## 6. Questions for the user

**Answered by the user, 2026-11-18** (the questions are kept below as the record):

1. Start at boot on the Overview: **(a)**, shown only after sign-in, read per tile; the signed-out
   Overview shows no boot state and `GET /api/instances` stays as it is.
2. Font coverage: **(b)**, Latin plus Latin Extended, the extra file downloaded only when a page shows
   such a letter.
3. Theme switch: **(a)**, two states, Light and Dark, starting from the OS and back to following the OS
   when switched to the OS's theme.
4. Assets on the model profile page: **(a)**, collapsed by default, with the model file name under the
   title and the Engine as a row of Assets.

1. **Start at boot on the signed-out Overview?** (a) only when signed in, read per tile (recommended:
   no new unauthenticated data); (b) for everyone (the architect adds the boot state to the
   unauthenticated `GET /api/instances`); (c) not on the Overview, only on the instance page.
2. **Font coverage?** (a) Latin only, 34.1 KiB (recommended: the UI is English and Latin covers
   Western European names; other letters fall back to the system font); (b) Latin and Latin Extended,
   56.3 KiB in the packages, the extra file downloaded only when a page shows such a letter.
3. **The theme switch's states?** (a) two states, Light and Dark, starting from the OS and back to
   following the OS when switched to the OS's theme (recommended: matches "a light/dark switch", one
   button); (b) three choices, System, Light, Dark, in a small menu in the banner.
4. **Assets on the model profile page?** (a) collapsed by default, with the model file name under the
   title and the Engine as a row of Assets (recommended: the first setting moves from about 2900 px to
   about 700 px at 320 px); (b) open as today, moved after the editable cards; (c) as today.

## 7. Notes for the coordinator

- Instance tags: `PUT /api/instances/{id}/tags` (platform slice 12) now has its fe slice, 05d2 in
  `fe/briefs/05-profiles-rename.md` (user, 2026-11-18: tags on profile and instance); 1.4 item 4 is it.
- `crates/bff/build.rs`: `woff2` -> `font/woff2` (section 3), a platform change to schedule with H1.
- `fe/DESIGN.md` 5.1 says "No theme switch (preference)"; H4 changes it, with the user's choice.
