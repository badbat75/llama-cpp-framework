# Brief 04: visual identity (three directions to choose from)

Status: **proposal, nothing chosen**. The user found the web UI "very basic" (2026-11-17) and asked
for a more marked identity: a typeface of its own, a logo or illustrations, a cared-for sign-in page,
on top of a typographic scale, spacing, an accent colour, icons, visual states and the Overview as a
real status page; on the present hand-written CSS, no component library; built after the preset save
(02h) is finished. `fe/DESIGN.md` changes only once a direction is chosen.

The three directions are static pages, one file each, same five screens, same markup roles and names,
light and dark (a toggle at the top, the system setting first), usable at 320 px:

- `build/ux-review/visual/index.html` (links the three)
- `build/ux-review/visual/a-instrument.html`
- `build/ux-review/visual/b-hearth.html`
- `build/ux-review/visual/c-signal.html`
- their generator and checks: `build/ux-review/visual/src/` (`gen.py`, `base.css`, `a.css`, `b.css`,
  `c.css`, `body.html`, `check.js`); screenshots in `src/shots/`.

Fonts in the pages are system stand-ins for the typeface named below (nothing is loaded from the
network); icons are hand-drawn stand-ins drawn in the named set's manner.

Labels used below: **measured** (computed or run here, with how), **read** (in a source, with the
URL), **assumed** (a guess or a preference).

## 0. What every direction keeps

These are tested or decided and hold in all three (`fe/DESIGN.md` 4, 5):

- WCAG 2.2 AA: text 4.5:1 ([1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum)); control
  borders, focus ring and state shapes 3:1 ([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast));
  visible focus ring on `:focus-visible` ([2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible)).
- State never by colour alone: the state badge keeps its shape and its text; tiles and rows add a
  colour bar that is decorative on top of both ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)).
- Disabled stays "not editable" without opacity: the dashed border remains the cue in all three
  (B adds a faint hatch, C a 2 px dashed border), and it survives forced colours, which replace border
  colours but not `border-style`
  ([CSS Color Adjustment 1](https://www.w3.org/TR/css-color-adjust-1/#forced-colors-properties)).
- Forced colours: shapes, the drawn checkbox and the switch knob get system colours
  (`CanvasText`, `Highlight`, `GrayText`); icons are drawn in `currentColor`, so they take the text's
  system colour.
- The sticky save bar, its `scroll-padding-bottom` rule and the short-viewport fallback
  ([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)).
- 24 by 24 px targets at least ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)); reflow
  at 320 px with no sideways scroll ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)).
- DOM order, roles, accessible names and ids as today. Every icon or mark added is an inline SVG with
  `aria-hidden="true"` and `focusable="false"`, inside an element whose name comes from its text, so
  no accessible name changes ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)). Where a
  direction needs more than that, its section says so.
- Nothing loaded at run time: the fonts and icons are bundled into `fe/dist` by Vite and embedded in
  `pai-bff` like the rest of the UI.

**Measured on the three pages** (Playwright Chromium 1255 from `fe/ux/node_modules`, script
`src/check.js` and `src/over.js`, 2026-11-17): 0 sideways scroll at 1280 and 320 px, light and dark;
0 network requests; axe-core WCAG 2.0 to 2.2 A and AA: 0 violations on each page in each of the four
runs; no element wider than its screen frame at 320 px (both instance tabs). Forced colours were
emulated in Chromium for a screenshot only; Chromium's emulation kept most author colours, so forced
colours are **not** verified beyond the rules being present.

## 1. Direction A: Instrument

**Idea.** A calm technical console. IBM Plex Sans and Mono, an ink banner, hairline panels, small
uppercase card titles and tabular numbers. The identity comes from precision and a rack mark lit in
indigo; the Overview stays a table, read as a status board.

**Palette** (measured: WCAG relative luminance, script in this session, against each background).

| Token | Light | Dark | Use |
|---|---|---|---|
| `--page` | `#f3f5f8` | `#0d1117` | page behind the cards (new token) |
| `--bg` | `#ffffff` | `#11161e` | cards, save bar |
| `--surface` | `#f3f5f8` | `#161b24` | nav, disabled fill |
| `--field` | `#ffffff` | `#0d1117` | enabled control fill (new token) |
| `--line` | `#dde2e8` | `#262d38` | decorative hairlines only (new token) |
| `--text` | `#161b26` | `#e4e8ef` | 15.8 to 17.2 / 14.0 to 15.4 |
| `--muted` | `#556070` | `#9aa4b2` | 5.8 to 6.4 / 6.8 to 7.5 |
| `--control` | `#7c8797` | `#75808f` | 3.3 to 3.6 / 4.3 to 4.7 |
| `--accent` | `#3451c6` | `#94aaff` | **6.1 (page, surface), 6.7 (bg, field) / 7.8 (surface) to 8.5 (page)** |
| `--on-accent` | `#ffffff` | `#0d1117` | 6.7 / 8.5 on the accent |
| `--accent-soft` | `#e6ebf7` | `#1d2433` | current nav item; accent on it 5.6 / 7.0 |
| `--danger` | `#b42318` | `#ff8d80` | 6.0 to 6.6 / 7.7 to 8.4 |
| `--success` | `#1a7f37` | `#5cc87a` | 4.7 to 5.1 / 8.2 to 9.0 |
| `--warning` | `#8a5a00` | `#e0b040` | 5.4 to 5.9 / 8.6 to 9.4 |
| `--ink`, `--on-ink` | `#141a24`, `#f2f4f7` | `#05070a`, `#e4e8ef` | banner; 15.8 / 16.4 |
| `--ring-on-ink` | `#94aaff` | `#94aaff` | focus ring on the banner: 7.9 / 9.1 |

The light accent on the ink banner is 2.6:1, below 3:1: the banner's focus ring uses
`--ring-on-ink` instead (measured). The banner's button border `#7c8797` is 4.8 / 5.5 on ink.

- **Type**: IBM Plex Sans 400 and 600, IBM Plex Mono 400 (600 optional). Scale in rem: 0.75 (nav
  and table labels, uppercase), 0.8125 (card titles, uppercase), 0.875, 1 (body), 1.125 (h2), 1.5 (h1).
  `font-variant-numeric: tabular-nums` on the body.
- **Spacing**: 4, 8, 12, 16, 24, 32, 48 (adds `--space-7`). **Radius** 4 px controls and cards, 2 px
  checkbox. **Elevation**: none; hairline borders.
- **Icons**: Lucide, **ISC License**, with the Feather-derived icons under the **MIT License**
  (read: [LICENSE](https://github.com/lucide-icons/lucide/blob/main/LICENSE), "The following Lucide
  icons are derived from the Feather project"; npm `lucide-static` 1.74.0 says ISC). About 15 icons,
  copied as inline SVG paths (no runtime package needed); both licence texts ship.
- **Mark**: an indigo rounded square holding three rack units, the top one lit (original).
- **Sign-in**: centred panel with an accent top rule, the mark and name as a lockup above, the
  node's name and version under it in mono.
- **Overview**: the instances table kept, restyled as a status board: a 4 px state-coloured bar on
  the first cell (decorative), uppercase column labels, the tags under the instance's link, the port
  in mono with "Start at boot: on/off" under it. Three columns, so it fits 320 px.
- **`index.css`**: tokens (new: `--page`, `--field`, `--line`, `--accent-soft`, `--ink`,
  `--on-ink`, `--ring-on-ink`, `--space-7`, font tokens) and a few rules (banner band, card title
  rule, nav current item, status-table bar). The field grid, save bar and disabled rules stay.
- **Markup**: decorative `aria-hidden` additions only (mark, icons in the run buttons, notices, the
  save alert, nav entries, the back link; a `span` wrapper around the filter input for its search
  icon). The Overview's new columns are new data (below), not a restyle.
- **Cost**: 5 small slices: (1) tokens and bundled fonts with their licences; (2) shell: banner,
  mark, nav; (3) icon set and its placements; (4) cards, fields, switch, notices; (5) Overview status
  board (with the nodes data).
- **Risks**: the least marked of the three: it may still read as "basic". Plex has the Reserved Font
  Name "Plex" (read: [IBM/plex LICENSE.txt](https://github.com/IBM/plex/blob/master/LICENSE.txt)), so
  a subset made by us must not be called Plex: ship the unmodified files.

## 2. Direction B: Hearth

**Idea.** Warm and humane: the model stays at home. Atkinson Hyperlegible Next on warm paper, soft
rounded cards, pill controls and a plum accent; an arched-doorway mark, an illustrated sign-in and the
instances as friendly tiles.

**Palette** (measured, as in A).

| Token | Light | Dark | Use |
|---|---|---|---|
| `--page` | `#f7f1e8` | `#1b1714` | warm paper |
| `--bg` | `#fffdf9` | `#231d19` | cards, tiles, save bar |
| `--surface` | `#f5efe6` | `#2b241f` | disabled fill, badges, notices |
| `--field` | `#fffdf9` | `#1b1714` | enabled control fill |
| `--line` | `#e6dccf` | `#3a3029` | decorative dividers; the dark theme's card outline |
| `--text` | `#2a211c` | `#f1e9df` | 13.8 to 15.5 / 12.7 to 14.8 |
| `--muted` | `#6b5d52` | `#b5a698` | 5.5 to 6.2 / 6.5 to 7.5 |
| `--control` | `#8c7d70` | `#8a7c6f` | 3.5 to 3.9 / 3.8 to 4.4 |
| `--accent` | `#7b3a8f` | `#dcaaf0` | **6.5 (surface), 6.6 (page), 7.3 (bg, field) / 8.0 (surface) to 9.4 (page)** |
| `--on-accent` | `#ffffff` | `#1b1714` | 7.4 / 9.4 on the accent |
| `--accent-soft` | `#f1e4f6` | `#3a2a44` | current nav item, node header, Running badge; accent on it 6.0 / 6.9, text 12.9 / 11.0 |
| `--danger` | `#b3261e` | `#ff9a8a` | 5.7 to 6.4 / 7.4 to 8.7 |
| `--success` | `#2f6f3e` | `#7ccf8e` | 5.3 to 6.0 / 8.1 to 9.5 |
| `--warning` | `#8a5300` | `#e8b45a` | 5.5 to 6.2 / 8.1 to 9.4 |

- **Type**: Atkinson Hyperlegible Next 400 and 700, Atkinson Hyperlegible Mono 400. Body 1.0625 rem
  (17 px), line height 1.55; scale 0.8125, 0.875, 0.9375, 1.0625, 1.125 (card title), 1.375 (h2),
  2 (h1). The family was drawn by the Braille Institute for low-vision legibility (assumed from
  memory; not checked at the Braille Institute's page in this session).
- **Spacing**: 4, 8, 12, 16, 24, 40, 64. **Radius**: 10 px controls, 16 px cards, 18 px tiles, pills
  (999 px) for buttons, tabs, tags and the switch. **Elevation**: two soft shadows in light; in dark a
  1 px `--line` outline instead (a shadow does not show on a dark page; assumed).
- **Controls**: 40 px high (32 today), 1.5 px borders, 3 px focus ring; disabled adds a 135 degree
  hatch (`--hatch`, decorative) to the dashed border; the switch is a 44 by 24 px pill drawn on the
  same native checkbox.
- **Icons**: Phosphor, **MIT License** (read:
  [phosphor-icons/core LICENSE](https://github.com/phosphor-icons/core/blob/main/LICENSE); npm
  `@phosphor-icons/core` 2.1.1 says MIT), the duotone weight; about 15 icons as inline SVG.
- **Mark**: a plum arched doorway with a keyhole (original).
- **Sign-in**: two columns from 840 px: an illustration (an arched room holding a server, a cable to
  a laptop, a plant; inline SVG drawn in the theme's tokens, `aria-hidden`) beside the form in a
  rounded card; stacked below 840 px.
- **Overview**: a node header (accent-soft panel: "Node", host name, version, a one-line summary),
  then one tile per instance: name as the link, the state badge as a pill, and Port, Tags, Start at
  boot as a small description list.
- **`index.css`**: tokens (as A plus `--hatch`, `--shadow`, control height, border width, ring width)
  and structure: tiles, node header, sign-in two-column layout, pill shapes, switch drawing, larger
  control height (affects every form row and the save bar's height).
- **Markup**: the decorative additions of A, plus the sign-in illustration (`aria-hidden` SVG in a
  `div`), plus **one real change**: the Overview's instances become a list (`ul` of `li`, each with a
  link, the badge and a `dl`) instead of a `table`. `fe/src/Instances.test.tsx` (lines 44 to 48)
  reads rows and cells, so those tests are rewritten in that slice.
- **Cost**: 7 small slices: (1) tokens and fonts; (2) shell and mark; (3) icons; (4) controls (height,
  pills, switch, hatch) and cards; (5) save bar and notices; (6) sign-in with the illustration;
  (7) Overview tiles (with the nodes data).
- **Risks**: taller controls and 17 px body make the preset page longer and the save bar taller at
  320 px (the measured `scroll-padding-bottom` handles it; the 1b review measured 242 px at 375 px
  with today's sizes, this will be more); the illustration is one more thing to keep in two themes;
  the hatch may look busy on a page of disabled fields (the read-only cards).

## 3. Direction C: Signal

**Idea.** Bold and operational, made for the dark. Space Grotesk headings and UI, JetBrains Mono for
labels and values, square 2 px frames and a cyan accent. A bracket mark, a measured grid behind the
sign-in, and tiles with a state bar readable from across the room.

**Palette** (measured, as in A).

| Token | Light | Dark | Use |
|---|---|---|---|
| `--page`, `--bg`, `--field` | `#ffffff` | `#07090b` | page, cards, controls |
| `--surface` | `#eef1f3` | `#11161a` | nav, disabled fill, notices |
| `--line` | `#0a0d10` | `#3a454e` | 2 px frames (decorative in dark) |
| `--text` | `#0a0d10` | `#edf2f5` | 17.2 to 19.5 / 16.1 to 17.7 |
| `--muted` | `#4f5a63` | `#9aa8b2` | 6.2 to 7.1 / 7.5 to 8.2 |
| `--control` | `#6e7a84` | `#64727d` | 3.9 to 4.4 / 3.7 to 4.0 |
| `--accent` | `#006d79` | `#3dd6e8` | **5.3 (surface), 6.1 (bg) / 10.4 (surface), 11.4 (bg)** |
| `--on-accent` | `#ffffff` | `#07090b` | 6.1 / 11.4 on the accent |
| `--danger` | `#b0201a` | `#ff7a6e` | 6.0 to 6.8 / 7.2 to 7.8 |
| `--success` | `#18753a` | `#4fdc84` | 5.1 to 5.8 / 10.3 to 11.3 |
| `--warning` | `#855400` | `#ffc04d` | 5.7 to 6.4 / 11.2 to 12.3 |

- **Type**: Space Grotesk 400, 600, 700; JetBrains Mono 400 and 700. Scale in rem: 0.75 (mono
  uppercase labels), 0.8125, 0.875, 1 (body), 1.375 (h2), 2.25 (h1), 3 (the node's name on the
  Overview). Uppercase only on short labels (card titles, nav and table labels, badges, tabs).
- **Spacing**: 4, 8, 12, 16, 24, 40, 56. **Radius**: 2 px controls, 0 for the checkbox, switch and
  tags. **Elevation**: none, flat 2 px borders; one hard offset shadow (8 px, accent in dark, ink in
  light) on the sign-in card only.
- **Controls**: 36 px high, 2 px borders (dashed when disabled), 3 px focus ring; square switch.
- **Icons**: an original set drawn for the project (about 15 icons, 2.25 px square-capped strokes),
  so no third-party licence; or Tabler Icons, **MIT License** (read:
  [tabler-icons LICENSE](https://github.com/tabler/tabler-icons/blob/main/LICENSE); npm
  `@tabler/icons` 3.52.0 says MIT) if the user prefers a maintained set.
- **Mark**: two corner brackets holding a cyan square core (original).
- **Sign-in**: the mark large on a 24 px grid backdrop (decorative gradient), the form in a square
  card with the hard shadow.
- **Overview**: a big node header (host name at 3 rem, version and summary in mono), then tiles with
  an 8 px state-coloured top bar (dashed for the unknown state), the badge as an outlined uppercase
  label with its shape, the port large in mono.
- **`index.css`**: tokens and structure: 2 px borders everywhere change box sizes, uppercase mono
  label rules, tiles, node header, sign-in grid.
- **Markup**: as B (decorative additions plus the Overview list instead of the table, with the same
  test rewrite).
- **Cost**: 7 small slices, as B (the grid replaces the illustration).
- **Risks**: uppercase mono labels read slower and must stay short (assumed); 2 px borders on every
  control make each field taller and need a recheck of the save bar and focus-not-obscured checks;
  the light theme is stark (near-black frames on white); JetBrains Mono is the heaviest font payload
  of the three (below). Screen readers read the DOM text, not its CSS uppercase (assumed; not tested
  here).

## 4. Typefaces: licence and weight

All three families are under the **SIL Open Font License 1.1** (read: each upstream licence file:
[IBM Plex](https://github.com/IBM/plex/blob/master/LICENSE.txt),
[Atkinson Hyperlegible Next](https://github.com/googlefonts/atkinson-hyperlegible-next/blob/main/OFL.txt),
[Atkinson Hyperlegible Mono](https://github.com/googlefonts/atkinson-hyperlegible-next-mono/blob/main/OFL.txt),
[Space Grotesk](https://github.com/floriankarsten/space-grotesk/blob/main/OFL.txt),
[JetBrains Mono](https://github.com/JetBrains/JetBrainsMono/blob/master/OFL.txt); and the npm
metadata of `@fontsource/<family>` 5.3.0, "OFL-1.1"). The installers must ship each OFL text with its
copyright line.

Sizes: **measured** from the jsDelivr package listing of `@fontsource/<family>@5.3.0`
(`https://data.jsdelivr.com/v1/packages/npm/@fontsource/<family>@5.3.0?structure=flat`), the Latin
subset, WOFF2, upright only.

| Direction | Files (weight: kB) | Total |
|---|---|---|
| A | Plex Sans 400: 22.1, 600: 23.7; Plex Mono 400: 14.4 (600: 15.3 optional) | 60.2 kB (75.5 with Mono 600) |
| B | Atkinson Hyperlegible Next 400: 11.8, 700: 12.4; Atkinson Hyperlegible Mono 400: 9.8 | 34.0 kB |
| C | Space Grotesk 400: 13.1, 600: 13.0, 700: 12.5; JetBrains Mono 400: 20.7, 700: 21.4 | 80.7 kB |

Latin only covers English; a user name with other letters falls back to the system font in the same
stack (assumed acceptable). Latin Extended adds about 7 kB per file for Atkinson (measured: 7.2 to
7.4 kB); not measured for the others.

## 5. What the pages show that is not a restyle

So the user can tell the look from the features: these belong to their own slices, not to the
identity work.

- **The node's name** on top of the nav, on the Overview and on the sign-in page: step 10 item 13
  (`ARCHITECTURE.md`, "Nodes and instances"; the bff answers `{"name": <host name>, ...}`).
- **Port and tags** on the Overview: item 13 adds them to the instances call (`ARCHITECTURE.md`
  1054 to 1059, `Instances() -> [{id, engine, port, tags}]`, unauthenticated).
- **Start at boot** on the Overview: that call has no boot state; `BootEnabled` today is read for a
  signed-in administrator. Shown on the session-free Overview, it needs the architect (question 5).
- **The instance's Server and Presets tabs** and the Presets tab's per-instance override: item 13;
  the mock's select ("Follow preset (on)", On, Off) is illustrative only.
- **The Overview's summary line** and a state icon beside each instance in the nav: new; text keys
  below are proposals for that slice.
- A light/dark switch in the UI: the pages have one for the review only; the UI follows the system
  today (`fe/DESIGN.md` 5.1, a preference; question 4).

Proposed keys for that later slice (illustrative; its brief fixes them): `overview-node` "Node",
`nav-node-current` "This node", `overview-summary` "{0} instances: {1} running, {2} stopped, {3}
failed, {4} in an unknown state.", `instances-port` "Port", `instances-tags` "Tags",
`instances-boot` "Start at boot", `instances-boot-on` "On", `instances-boot-off` "Off"; an instance
with no tags reuses the "(none)" text (`preset-value-none`).

## 6. Recommendation

**B, Hearth** (a preference). It answers every point the user named: a typeface of its own with a
legibility rationale that fits the accessibility floor, a mark and an illustration, a cared-for
sign-in, tiles that already look like the collapsible instance cards item 13 asks for. It is also the
lightest font payload (34 kB). Its cost (7 slices) is two more than A; its risk (longer pages) is
handled by the existing save-bar mechanism and is checked by the UX run. If the user wants the
strongest look for a server console, C is the alternative; A is the choice only if the change should
stay small.

Parts can be mixed: C's state bar on B's tiles, or A's status table with B's type and palette.

## 7. Questions for the user

1. **Which direction?** (a) B Hearth (recommended); (b) C Signal; (c) A Instrument; (d) a mix: name
   the parts.
2. **Overview instances as tiles or table?** (a) tiles, a list instead of the table, with its tests
   rewritten (recommended, with B or C); (b) keep the table, restyled as a status board (as A).
3. **Icons?** (a) the direction's named set (Lucide ISC plus MIT, Phosphor MIT, or Tabler MIT),
   copied as inline SVG, its licence shipped (recommended); (b) an original set drawn for the project,
   no third-party licence.
4. **Light/dark switch in the UI?** (a) no, the system setting decides, as today (recommended);
   (b) yes, a Theme control in the header (auto, light, dark), remembered per browser.
5. **Start at boot on the session-free Overview?** (a) only when signed in (recommended: no new
   unauthenticated call); (b) for everyone (the architect adds the boot state to the unauthenticated
   instances call); (c) not on the Overview.
6. **Font coverage?** (a) Latin only (recommended, smallest); (b) Latin and Latin Extended (about
   +7 kB a file).
