# 06 H1: Hearth tokens (H1a) and fonts (H1b)

Restyle only: the colour, radius, spacing, shadow and type-scale tokens of direction B "Hearth" in
both themes, and the few rule changes that put each region on its Hearth surface. The page keeps the
system font stack (H1b adds the font files later), follows the OS theme as today (the switch is H4),
and keeps every control's shape (H2). No TSX, no strings key, no DOM change.

Sources: `fe/briefs/04-visual-identity.md` (round 2: 1.1 palette and type, 2 build order, 4 criteria,
6 the user's answers); the mockups `build/ux-review/visual/b2-overview.html` and
`b2-model-profile.html` (their `:root` blocks are the values below); today's `fe/src/index.css` (its
`:root` and the dark block); `fe/src/DisabledLook.test.ts`; `PLAN.md` step 10 (the platform's
"Before its font slice" line, the gate of H1b).

Labels: **measured** (computed here, with how), **read** (a source, with its URL), **preference**.

## 0. Decisions

- **One split rule.** H1a changes every **colour** (token values and which token a region sits on)
  and the **type scale and spacing**; H2 changes every **shape** (heights, border widths, radii of
  controls and cards, shadows, the hatch, the switch). So H1a declares all tokens H1a and H2 use, and
  H2 adds no token. Not here: `--art-1` to `--art-3` (H7, the illustration) and a tile radius (H6
  uses `--radius-card`; one card radius instead of brief 04's 16, 18 and 20 px, a preference).
- **The surface map.** Hearth draws the page on warm paper and lifts cards off it, the reverse of
  today (white page, grey cards). H1a moves: `body` to `--page`; `.card` and `.save-bar` to `--bg`;
  enabled fields and the unchecked checkbox to `--field`. The banner and the nav keep `--surface` and
  their `--control` rules until H5 restyles the shell. A disabled control stays on `--surface`, which
  is now slightly darker than the card it sits in (1.13:1 light): the cue stays the dashed border,
  never the fill (`fe/DESIGN.md` 5.2, [1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)).
- **About, open.** On a `--bg` card, today's `.about[aria-expanded='true'] { background: var(--bg) }`
  would show nothing; it becomes `--accent-soft` (accent text on it 6.04:1 light, 6.93:1 dark). The
  open state is still told by `aria-expanded` and the hint that appears; the fill is extra.
- **No colour literal outside the token blocks.** One exception: `.dialog::backdrop` keeps
  `rgb(0 0 0 / 0.5)`, because `::backdrop` "neither inherits from nor is inherited by any other
  elements", so a `:root` custom property does not reach it (read:
  [MDN, `::backdrop`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::backdrop)).
- **Font sizes become tokens.** Every `font-size` in a rule is a `var(--fs-*)`. Two existing tests pin
  `font-size: 0.875rem` as text (section 3, H1a.10); they move to `var(--fs-sm)`, whose value stays
  0.875rem. The body grows from 16 to 17 px (`--fs-body` 1.0625rem), h1 from 1.5 to 2 rem.
- **Bold stays 600.** Atkinson Hyperlegible Next ships 400 and 700 only (H1b); for a weight above 500
  the font matching picks the nearest heavier face, so 600 renders as 700 with no synthesis (read:
  [CSS Fonts 4, font matching, `font-weight`](https://www.w3.org/TR/css-fonts-4/#font-style-matching)).
  The nav's 600 pin (`DisabledLook.test.ts`, DESIGN.md 2.3) holds.
- **No motion.** Hearth adds no `transition` and no `animation` (H2's switch knob jumps), so
  `prefers-reduced-motion` needs no rule ([2.3.3](https://www.w3.org/TR/WCAG22/#animation-from-interactions)
  is AAA; a preference that matches `fe/DESIGN.md` 5.1 "Motion: none needed").

## 1. The tokens

All on the top-level `:root` (light); the 16 colour and shadow tokens again in the `:root` inside
`@media (prefers-color-scheme: dark)` (the dark block), as today. H4 later moves the dark block under
`:root:not([data-theme='light'])` and `:root[data-theme='dark']` (brief 04 1.9); the tests below find
it by "the `:root` rule nested in the `prefers-color-scheme: dark` media block", which H4 rewrites.
`color-scheme: light dark` stays on `:root`.

Today's 19 custom properties: all **kept** by name, none renamed, none removed. 21 are new. 40 in all.

| # | Token | Light | Dark | Today (light / dark) | Fate |
|---|---|---|---|---|---|
| 1 | `--page` | `#f7f1e8` | `#1b1714` | | new: behind everything (`body`) |
| 2 | `--bg` | `#fffdf9` | `#231d19` | `#ffffff` / `#0f1115` | kept, new value; now cards, save bar, dialog, secondary buttons (the table moves onto it in H2b, with its radius and shadow, as in the mockup's `.presets`; in H1a it stays on `--page`) |
| 3 | `--surface` | `#f5efe6` | `#2b241f` | `#f6f7f9` / `#181b22` | kept, new value; banner, nav (until H5), notices, disabled fill |
| 4 | `--field` | `#fffdf9` | `#1b1714` | | new: enabled control fill |
| 5 | `--line` | `#e6dccf` | `#3a3029` | | new: decorative dividers only (table rows; H2 card outline in dark) |
| 6 | `--text` | `#2a211c` | `#f1e9df` | `#1b1f24` / `#e6e8eb` | kept, new value |
| 7 | `--muted` | `#6b5d52` | `#b5a698` | `#59636e` / `#9ba5b0` | kept, new value |
| 8 | `--control` | `#8c7d70` | `#8a7c6f` | `#818b98` / `#7d8794` | kept, new value |
| 9 | `--accent` | `#7b3a8f` | `#dcaaf0` | `#0a5cc2` / `#6cb0ff` | kept, new value (plum) |
| 10 | `--on-accent` | `#ffffff` | `#1b1714` | `#ffffff` / `#0f1115` | kept, new dark value |
| 11 | `--accent-soft` | `#f1e4f6` | `#3a2a44` | | new: About open; H5 current nav; H6 Running badge |
| 12 | `--danger` | `#b3261e` | `#ff9a8a` | `#b42318` / `#ff8d80` | kept, new value |
| 13 | `--success` | `#2f6f3e` | `#7ccf8e` | `#1a7f37` / `#56c271` | kept, new value |
| 14 | `--warning` | `#8a5300` | `#e8b45a` | `#8a5a00` / `#d9a72e` | kept, new value |
| 15 | `--hatch` | `rgb(42 33 28 / 0.07)` | `rgb(241 233 223 / 0.06)` | | new: the disabled hatch (used by H2a) |
| 16 | `--shadow` | `0 1px 2px rgb(60 40 20 / 0.08), 0 6px 20px rgb(60 40 20 / 0.06)` | `0 0 0 1px var(--line)` | | new: card lift (used by H2b); in dark an outline, where a shadow does not show (assumed) |
| 17 to 21 | `--space-1` to `--space-5` | 4, 8, 12, 16, 24 px | (not redeclared) | the same | kept, same values |
| 22 | `--space-6` | `40px` | | `32px` | kept, new value |
| 23 | `--space-7` | `64px` | | | new (H5, H7) |
| 24 | `--radius` | `10px` | | `6px` | kept, new value: controls, notices, alerts |
| 25 | `--radius-check` | `5px` | | | new (H2a checkbox) |
| 26 | `--radius-card` | `16px` | | | new (H2b cards, tables, save bar, dialog; H6 tiles) |
| 27 | `--radius-pill` | `999px` | | | new (H2a buttons, About, switch; H5 nav entries) |
| 28 | `--font` | today's stack | | `system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif` | kept, unchanged until H1b |
| 29 | `--mono` | today's stack | | `ui-monospace, 'Cascadia Mono', Menlo, monospace` | kept, unchanged until H1b |
| 30 | `--fs-xs` | `0.8125rem` | | | new (H5 nav labels, H6 tags) |
| 31 | `--fs-sm` | `0.875rem` | | literal `0.875rem` in 8 rules | new: hints, notes, full-value line, About, field error, nav label |
| 32 | `--fs-md` | `0.9375rem` | | | new (H6 badges) |
| 33 | `--fs-body` | `1.0625rem` | | `1rem` on `body` | new: body (17 px) |
| 34 | `--fs-lg` | `1.125rem` | | `1rem` on `h3` | new: card titles (`h3`) |
| 35 | `--fs-h2` | `1.375rem` | | `1.25rem` | new |
| 36 | `--fs-h1` | `2rem` | | `1.5rem` | new |
| 37 | `--lh` | `1.55` | | `1.5` on `body` | new |
| 38 | `--control-h` | `40px` | | literal `32px` | new (H2a) |
| 39 | `--border-w` | `1.5px` | | literal `1px` | new (H2a) |
| 40 | `--ring-w` | `3px` | | literal `2px` | new (H2a) |

## 2. Contrast pairs (measured)

WCAG 2.2 contrast ratio from the relative luminance (read:
[WCAG 2.2, relative luminance](https://www.w3.org/TR/WCAG22/#dfn-relative-luminance) and
[contrast ratio](https://www.w3.org/TR/WCAG22/#dfn-contrast-ratio)), computed here with
`/tmp/claude-designer/cr2.py` from the hex values above (measured, 2026-11-18). The tester computes the
same in the test from the parsed token blocks, unrounded, and asserts `ratio >= floor`.

Floors: text 4.5:1 ([1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum); no text in this UI is
"large" except headings, which are `--text`, so 4.5 is used throughout); non-text 3:1 for control
borders, state shapes, the checked box and the focus ring
([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast), [2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible)).
A pair that is both text and non-text (accent: link text and ring) carries the higher floor.
[2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum) is about the ring being hidden, not
its colour (section 5); [2.4.13](https://www.w3.org/TR/WCAG22/#focus-appearance) is AAA, and the 3 px
ring (H2a) at 3:1 or more against the unfocused pixels meets it anyway (measured: the accent rows).

| # | Foreground | Background | Floor | Light | Dark | Where |
|---|---|---|---|---|---|---|
| P1 | `--text` | `--page` | 4.5 | 14.03 | 14.81 | page text, headings |
| P2 | `--text` | `--bg` | 4.5 | 15.51 | 13.84 | cards, save bar, dialog, tables, secondary buttons |
| P3 | `--text` | `--surface` | 4.5 | 13.78 | 12.70 | banner, nav, notices |
| P4 | `--text` | `--field` | 4.5 | 15.51 | 14.81 | values in enabled fields |
| P5 | `--text` | `--accent-soft` | 4.5 | 12.88 | 10.95 | (H5 nav, H6 badge) |
| P6 | `--muted` | `--page` | 4.5 | 5.65 | 7.52 | intro, filter hint and status, engine line |
| P7 | `--muted` | `--bg` | 4.5 | 6.24 | 7.03 | card intro, notes, field hints, units, inactive (aria-disabled) buttons, off rows |
| P8 | `--muted` | `--surface` | 4.5 | 5.55 | 6.45 | disabled values and labels, nav labels (4.5 a preference for disabled: 1.4.3 exempts them) |
| P9 | `--muted` | `--field` | 4.5 | 6.24 | 7.52 | placeholder |
| P10 | `--muted` | `--accent-soft` | 4.5 | 5.18 | 5.56 | (H5, H6) |
| P11 | `--accent` | `--page` | 4.5 | 6.58 | 9.37 | links, focus ring on the page |
| P12 | `--accent` | `--bg` | 4.5 | 7.27 | 8.76 | About, links and ring in cards, checked box |
| P13 | `--accent` | `--surface` | 4.5 | 6.46 | 8.04 | current nav entry and its bar, ring in banner and nav |
| P14 | `--accent` | `--field` | 4.5 | 7.27 | 9.37 | checked box and switch against the unchecked fill |
| P15 | `--accent` | `--accent-soft` | 4.5 | 6.04 | 6.93 | About open; ring on it |
| P16 | `--danger` | `--page` | 4.5 | 5.82 | 8.68 | run, boot and switch alerts; Failed shape |
| P17 | `--danger` | `--bg` | 4.5 | 6.43 | 8.12 | field errors, save alert, alert border |
| P18 | `--danger` | `--surface` | 4.5 | 5.72 | 7.45 | an error inside a notice |
| P19 | `--danger` | `--field` | 4.5 | 6.43 | 8.68 | invalid field border against its fill |
| P20 | `--danger` | `--accent-soft` | 4.5 | 5.34 | 6.42 | (H6 Failed badge) |
| P21 | `--success` | `--page` | 4.5 | 5.40 | 9.47 | Running shape, run status |
| P22 | `--success` | `--bg` | 4.5 | 5.97 | 8.86 | Saved |
| P23 | `--success` | `--surface` | 4.5 | 5.30 | 8.13 | |
| P24 | `--success` | `--field` | 4.5 | 5.97 | 9.47 | |
| P25 | `--success` | `--accent-soft` | 4.5 | **4.96** | 7.01 | (H6 Running badge) |
| P26 | `--warning` | `--page` | 4.5 | 5.64 | 9.42 | Starting / Stopping shape |
| P27 | `--warning` | `--bg` | 4.5 | 6.23 | 8.81 | "restart to apply" |
| P28 | `--warning` | `--surface` | 4.5 | 5.54 | 8.08 | |
| P29 | `--warning` | `--field` | 4.5 | 6.23 | 9.42 | |
| P30 | `--warning` | `--accent-soft` | 4.5 | 5.17 | 6.97 | |
| P31 | `--on-accent` | `--accent` | 4.5 | 7.38 | 9.37 | primary label, tick, pressed Show, skip link, switch knob on |
| N1 | `--control` | `--page` | 3 | 3.54 | 4.40 | filter input and run-control borders on the page |
| N2 | `--control` | `--bg` | 3 | 3.91 | 4.12 | field and button borders in cards, save bar top edge |
| N3 | `--control` | `--surface` | 3 | 3.48 | 3.78 | dashed disabled border, banner and nav rules, notice border |
| N4 | `--control` | `--field` | 3 | 3.91 | 4.40 | field border against its fill |
| N5 | `--control` | `--accent-soft` | 3 | **3.25** | **3.26** | any border drawn on accent-soft (none in H1a/H2) |

Lowest: light text **P25 4.96** (success on accent-soft), dark text **P10 5.56** (muted on
accent-soft); non-text **N5 3.25 / 3.26**; of the pairs used in H1a, N3 3.48 (light).

Decorative, **no floor, not tested** (they must carry no meaning: brief 04 criterion 4): `--line` on
`--bg` 1.33 / 1.30, on `--page` 1.21 / 1.39; `--bg` on `--page` 1.11 / 1.07; `--surface` on `--bg`
1.13 / 1.09; `--surface` on `--page` 1.02 / 1.17; `--accent-soft` on `--page` 1.09 / 1.35; `--field`
on `--page` 1.11 / 1.00 (dark: an enabled field on the page is told only by its border, N1 4.40).

Direction B as drawn meets AA in every pair: nothing had to change from brief 04's palette.

## 3. Criteria (H1a)

**H1a.1 Light tokens.** The top-level `:root` rule declares exactly the 40 custom properties of
section 1, each with the light value of the table, written as there (hex lower case;
`rgb(r g b / a)` space syntax). `color-scheme: light dark` stays.

**H1a.2 Dark tokens.** The `:root` rule inside `@media (prefers-color-scheme: dark)` declares exactly
tokens 1 to 16 (`--page`, `--bg`, `--surface`, `--field`, `--line`, `--text`, `--muted`, `--control`,
`--accent`, `--on-accent`, `--accent-soft`, `--danger`, `--success`, `--warning`, `--hatch`,
`--shadow`) with the dark values, and nothing else.

**H1a.3 Contrast.** For every row P1 to P31 and N1 to N5 of section 2, in each theme, the ratio
computed from the parsed hex values is at least the floor (test table in section 6).

**H1a.4 No colour literal outside the token blocks.** In every rule body of `index.css` except the two
token blocks and `.dialog::backdrop`, no hex colour (`#` then 3, 4, 6 or 8 hex digits) and no colour
function (`rgb(`, `rgba(`, `hsl(`, `hsla(`, `hwb(`, `lab(`, `lch(`, `oklab(`, `oklch(`, `color(`).
Selectors are not searched (an id selector is not a colour). Keywords stay allowed: `transparent`,
`currentColor`, `inherit` and the system colours of the forced-colours blocks (`GrayText`,
`CanvasText`, `Canvas`, `Highlight`, `HighlightText`).

**H1a.5 Body.** The `body` rule: `background: var(--page)`, `color: var(--text)`,
`font-family: var(--font)`, `font-size: var(--fs-body)`, `line-height: var(--lh)`.

**H1a.6 Headings.** `h1` `font-size: var(--fs-h1)` and `line-height: 1.2`; `h2` `var(--fs-h2)`; `h3`
`var(--fs-lg)`; `h4` `var(--fs-body)`. Margins and `overflow-wrap: anywhere` on `h1` unchanged.

**H1a.7 Every font size is a token.** Every `font-size` declaration outside `:root` has a value
`var(--fs-...)` naming one of the seven `--fs-` tokens; no `rem`, `px` or `em` literal remains (today
8 rules carry `0.875rem`: `.nav-label`, `.model-profiles-filter-hint, .model-profiles-filter-status`,
`.model-profiles-note`, `.card .note`, `.field-hint`, `.full-value`, `.about`, `.field-error`; all
become `var(--fs-sm)`). `font: inherit` stays where it is.

**H1a.8 Surfaces.** `.card` `background: var(--bg)`; `.save-bar` `background: var(--bg)`; the enabled
field rule (`input[type='text'], input[type='password'], select`) `background: var(--field)`;
`.model-profiles-filter-row input` `background: var(--field)`; `input[type='checkbox']`
`background: var(--field)`; `td` `border-bottom: 1px solid var(--line)`;
`.about[aria-expanded='true']` `background: var(--accent-soft)`. `.banner` and `.nav` keep
`var(--surface)` and their `1px solid var(--control)` rules (H5).

**H1a.9 The disabled and inactive looks hold** (section 4): every assertion of
`fe/src/DisabledLook.test.ts` stays green except the two it changes in H1a.10.

**H1a.10 The changed pins.** In `DisabledLook.test.ts`: 2h.163b's enabled background becomes
`var(--field)` (its border stays `1px solid var(--control)` until H2a), and 2h.163f's token list
becomes the 40 names of section 1 (its title "no new token" becomes "the Hearth token set"). In
`fe/src/ModelProfileFullValue.test.tsx` (2e.118, line 492) and `fe/src/ModelProfileFlags.test.tsx`
(2e.124, line 763), the `font-size: 0.875rem` regex becomes `font-size: var(--fs-sm)`; H1a.1 pins
`--fs-sm: 0.875rem`, so the size the brief set is still the size drawn.

**H1a.11 No motion.** No `transition`, `transition-*`, `animation` or `animation-*` declaration in
`index.css`.

**H1a.12 Nothing else moves.** No `.tsx`, `index.html` or `en_US.json` change; every other existing
test passes unchanged (section 5).

## 4. Disabled, inactive and "locked" looks

`DisabledLook.test.ts` pins names, not colours, so the look follows the tokens:

| Pin | H1a | H2a |
|---|---|---|
| 163a disabled field: `--surface`, dashed `--control`, `--muted` text and fill colour, not-allowed, opacity 1 | holds | border `var(--border-w) dashed var(--control)` (helper widened); a `background-image` hatch added after `background` |
| 163b enabled field `background` | `var(--bg)` becomes **`var(--field)`** | border becomes `var(--border-w) solid var(--control)` |
| 163b rule order (disabled after enabled) | holds | holds |
| 163f token list | becomes the 40 names | holds (H2 adds no token) |
| 163c disabled checkbox: `--surface`, dashed, `--muted` tick, after `:checked` | holds | holds (the shorthand width becomes `var(--border-w)`) |
| 163d `.check` label `--muted`, aria-disabled box cursor default | holds | holds |
| 163e disabled buttons `--surface`, dashed, `--muted`, not-allowed; aria-disabled on `--bg`, `--control` border, cursor default | holds | dashed width `var(--border-w)`; hatch added |
| DESIGN 5.2 scroll padding 7rem floor | holds | holds (re-measured by the UX pass: 40 px controls) |
| DESIGN 2.3 nav current: `--accent`, 600, inset 3px bar | holds | holds (H5 changes it) |
| 2h.196 `.card h3` scroll margin | holds | holds |

Measured for the disabled look in Hearth (exempt from 1.4.3 and 1.4.11 as inactive; held anyway, a
preference): `--muted` on `--surface` 5.55 / 6.45, dashed `--control` on `--surface` 3.48 / 3.78.

## 5. What must not change

- DOM order, every id, class, role, accessible name, text and key; focus order and every focus move.
- The 320 px rule: no sideways scroll on any page, both themes, nav open and closed
  ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)). The 17 px body makes text wrap more: the Model
  profiles table must keep its three columns at 320 px.
- The sticky save bar, `html:has(.save-bar)` scroll padding with its 7rem floor, and the
  short-viewport rule ([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)).
- `prefers-reduced-motion`: nothing moves (H1a.11).
- Forced colours: every existing `@media (forced-colors: active)` block stays as is. The token colours
  are replaced by system colours there; the cues that survive are border style (dashed), shapes and
  text ([CSS Color Adjustment 1, 3.1](https://www.w3.org/TR/css-color-adjust-1/#forced-colors-properties):
  colours forced, `box-shadow` and a gradient `background-image` compute to `none`).
- Target sizes: no `min-height`, `min-width` or size H1a touches (H2a does)
  ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).

## 6. For the tester

Read `index.css` as `DisabledLook.test.ts` does (comments dropped, the rule regex, `depth`). The light
block is the top-level rule whose selector is exactly `:root`; the dark block is the rule whose
selector is exactly `:root` inside the `@media (prefers-color-scheme: dark)` block, located by its
matched braces as the 7rem test locates `@media (max-height: 31.98rem)`. Parse `--name: value;` pairs.
Hex to sRGB; relative luminance with the 0.04045 threshold; ratio `(L1 + 0.05) / (L2 + 0.05)`.

The pairs, to copy (`[fg, bg, floor]`; each in both themes):

```
['--text','--page',4.5] ['--text','--bg',4.5] ['--text','--surface',4.5] ['--text','--field',4.5] ['--text','--accent-soft',4.5]
['--muted','--page',4.5] ['--muted','--bg',4.5] ['--muted','--surface',4.5] ['--muted','--field',4.5] ['--muted','--accent-soft',4.5]
['--accent','--page',4.5] ['--accent','--bg',4.5] ['--accent','--surface',4.5] ['--accent','--field',4.5] ['--accent','--accent-soft',4.5]
['--danger','--page',4.5] ['--danger','--bg',4.5] ['--danger','--surface',4.5] ['--danger','--field',4.5] ['--danger','--accent-soft',4.5]
['--success','--page',4.5] ['--success','--bg',4.5] ['--success','--surface',4.5] ['--success','--field',4.5] ['--success','--accent-soft',4.5]
['--warning','--page',4.5] ['--warning','--bg',4.5] ['--warning','--surface',4.5] ['--warning','--field',4.5] ['--warning','--accent-soft',4.5]
['--on-accent','--accent',4.5]
['--control','--page',3] ['--control','--bg',3] ['--control','--surface',3] ['--control','--field',3] ['--control','--accent-soft',3]
```

A failing message names the pair, the theme and the ratio to two decimals. Put the new tests in a new
file (`fe/src/HearthTokens.test.ts`, a suggestion) so `DisabledLook.test.ts` changes only where H1a.10
says.

## 7. The designer's UX pass (after the coder)

`npm --prefix fe run build`, then `npm --prefix fe/ux run check`: the seven states
(`overview-signed-out`, `sign-in`, `overview`, `instance-server`, `model-profiles`, `model-profile`,
`model-profile-missing`), Chromium, Firefox, WebKit, 1280 and 320 px, light and dark: 0 axe-core
violations, 0 sideways scroll. By eye, from screenshots: cards lift off the paper in light and read in
dark; the Server page with default boxes ticked (disabled fields distinct from enabled ones by the
dashed border); the About hint open; the Model profiles table at 320 px; a refused save (alert and
button in the bar) at 320 px; forced colours in Firefox (`browser.display.document_color_use` 2):
dashed borders, shapes and the drawn checkbox still read. Compared with the b2 mockups: colours match;
shapes still today's (H2).

## 8. Size

One cycle: 12 criteria, one CSS file, one new test file and three one-line test edits.

## 9. H1b: the font files (after the platform's "fonts backend ready")

Waits for `PLAN.md` step 10's line: `font/woff2` in `crates/bff/build.rs` and the licence texts in
`installer/licenses/`. Restyle only; no DOM.

- **Packages**: `@fontsource/atkinson-hyperlegible-next` and `@fontsource/atkinson-hyperlegible-mono`,
  both exactly `5.3.0`, as `devDependencies` of `fe/package.json` (the platform's decision); Vite
  copies the referenced files into `fe/dist/assets/` with a hash, each above the 4096-byte inline
  limit (read: [Vite, `build.assetsInlineLimit`](https://vite.dev/config/build-options#build-assetsinlinelimit)).
  No WOFF2 committed, nothing fetched at run time.
- **Files** (read: the jsDelivr listing of each package at 5.3.0, bytes): Next 400 and 700, Mono 400,
  each in the `latin` and `latin-ext` subsets (the user's answer 2, brief 04 section 6), six in all:
  `atkinson-hyperlegible-next-latin-400-normal.woff2` 12,096; `-latin-700-normal.woff2` 12,732;
  `-latin-ext-400-normal.woff2` 7,376; `-latin-ext-700-normal.woff2` 7,556;
  `atkinson-hyperlegible-mono-latin-400-normal.woff2` 10,056; `-latin-ext-400-normal.woff2` 6,528.
  No Mono 700 (mono text is never bold), no italic, no `.woff`.
- **`@font-face`**: written in `index.css`, not imported from the packages' `400.css`, whose `src`
  also names a `.woff` file that Vite would then copy and the bff embed for no browser that needs it.
  Six rules, one per file: `font-family` `'Atkinson Hyperlegible Next'` or
  `'Atkinson Hyperlegible Mono'`, `font-style: normal`, `font-weight` 400 or 700,
  `font-display: swap`, `src: url(<path>) format('woff2')` (one URL), and the package's own
  `unicode-range` for the subset (copied from its `400.css`: latin `U+0000-00FF, U+0131, ...`,
  latin-ext `U+0100-02BA, ...`), so the latin-ext file is fetched only when a page shows such a letter
  ([CSS Fonts 4, `unicode-range`](https://www.w3.org/TR/css-fonts-4/#unicode-range-desc)). The path
  reaches the package's `files/` folder in a way `vite build` resolves (the coder checks `fe/dist`
  holds the six files and no `.woff`); no `http` or `//` URL.
- **Metric fallbacks**, so the swap does not move the layout: three more `@font-face` rules with
  `src: local(...)` and overrides ([CSS Fonts 5, `size-adjust`](https://www.w3.org/TR/css-fonts-5/#size-adjust-desc);
  [CSS Fonts 4, metrics overrides](https://www.w3.org/TR/css-fonts-4/#font-metrics-override-desc)).
  Measured here from the upstream TTFs (`googlefonts/atkinson-hyperlegible-next` and `-next-mono`,
  `main`, `fonts/ttf/`: UPM 1000, ascender 984, descender -316, line gap 0) against the
  Arial-compatible Liberation Sans and the Courier-New-compatible Liberation Mono, advance widths
  summed over every string of `en_US.json` (assumed: the 5.3.0 WOFF2 files carry the same metrics):

  | Family | weight | `src` | `size-adjust` | `ascent-override` | `descent-override` | `line-gap-override` |
  |---|---|---|---|---|---|---|
  | `'Atkinson Hyperlegible Next Fallback'` | 400 | `local('Arial'), local('Liberation Sans')` | `98.8%` | `99.6%` | `32%` | `0%` |
  | `'Atkinson Hyperlegible Next Fallback'` | 700 | `local('Arial Bold'), local('Arial-BoldMT'), local('Liberation Sans Bold')` | `99.3%` | `99.1%` | `31.8%` | `0%` |
  | `'Atkinson Hyperlegible Mono Fallback'` | 400 | `local('Courier New'), local('Liberation Mono')` | `105.3%` | `93.4%` | `30%` | `0%` |

- **Stacks**: `--font: 'Atkinson Hyperlegible Next', 'Atkinson Hyperlegible Next Fallback', system-ui,
  -apple-system, 'Segoe UI', Roboto, sans-serif`; `--mono: 'Atkinson Hyperlegible Mono',
  'Atkinson Hyperlegible Mono Fallback', ui-monospace, 'Cascadia Mono', Menlo, monospace`. A machine
  with none of the local fonts goes on down the stack as today.
- **The tester pins** (stylesheet text): exactly six `@font-face` rules for the two families with the
  declarations above, each `src` a single `url(...)` ending in the exact file name followed by
  `format('woff2')`; no `.woff)` and no `http` anywhere in `index.css`; the three fallback rules with
  their values; the two stacks as the first values of `--font` and `--mono`; `fe/package.json` lists
  both packages at exactly `5.3.0` under `devDependencies`. H1a.1's token list is unchanged (the
  values of `--font` and `--mono` change).
- **UX pass**: the network panel of a load shows the four latin files only (no latin-ext on an English
  page); screenshots before and after the font loads (throttled) show no line moving more than a few
  pixels; the bundle in `fe/dist/assets/` holds six WOFF2 files totalling 56,344 bytes.
