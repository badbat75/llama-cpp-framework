# Brief 10: reflow and small look items (slice R1)

Five open items from the designer's earlier reviews (H11b, S1a, S1b), measured again on main at
b7e6518 (one `npm --prefix fe run build`) before deciding. Criteria are numbered **10 R1.n**; test
names cite them so (`10 R1.3`). One slice, 8 criteria: no split. Suggested test file:
`Reflow.test.ts` (stylesheet text) plus `ModelProfiles.test.tsx` for the markup of R1.3.

## 0. What was measured

Playwright 1.x from `fe/ux/node_modules`, Chromium, Firefox and WebKit, the stub
`build/ux-review/stub9.py`; scripts `build/ux-review/r1.js` (all pages and items), `r1b.js` to
`r1g.js` (causes and the candidate CSS); output `r1-r.txt`, `r1-s.txt`, `r1-x.txt`, `r1-n.txt`,
`r1-ff.txt`, `r1e-r.txt`.

How the text was enlarged. "200 %" is `html { font-size: 200% !important }` added to the page: every
size in `index.css` is in `rem` or a token in `rem`, so this equals a browser default text size of
32 px. Firefox was also run with its real setting (`font.size.variable.x-western` 32): the same
numbers on every page (`r1-ff.txt`). WebKit has no such setting reachable from Playwright, so in
WebKit only the injected rule was measured; that is the first WebKit measurement at 200 % text.
"1.4.12" is the spacing of [1.4.12 Text Spacing](https://www.w3.org/TR/WCAG22/#text-spacing)
(line height 1.5, letter 0.12em, word 0.16em, paragraph 2em) as `!important` rules.

Why 200 % text at 320 px is the test: the Understanding page of Reflow says "it should be possible
to increase the size of all text to at least 200% while simultaneously meeting Reflow"
([Understanding 1.4.10](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html)). Text spacing at
320 px is not required by either criterion (Understanding 1.4.12 does not mention reflow); it is
measured because 1.4.12 fails when "Text ... is cut off" or overlaps
([Understanding 1.4.12](https://www.w3.org/WAI/WCAG22/Understanding/text-spacing.html)).

### 0.1 Item 1: sideways scroll at 320 px

`documentElement.scrollWidth` minus `clientWidth` (320), per mode. Pages: sign-in, sign-in on plain
HTTP (the notice), Overview with a tile, instance page, Model profiles list, a model profile closed
and with Assets open, not-found. Every page and mode not listed: 0.

| Page | Mode | Chromium | Firefox | WebKit | What overflows |
|---|---|---|---|---|---|
| Model profiles | 200 % | +207 | +205 | +205 | the table, 511 px wide: the headers do not wrap (`white-space: nowrap`) and the status cell does not wrap |
| Model profiles | 1.4.12 | +24 | +27 | +27 | the same table, 328 to 331 px |
| Model profiles | 200 % + 1.4.12 | +308 | +305 | +304 | the same table, 612 px |
| Instance page | 200 % + 1.4.12 | +75 | +74 | +74 | the intro's word `(server@default.ini).`, 378 px, no break point |

Inside a box, with no page scroll:

- Instance page, 200 %: the same word 5 px into the section's padding (all three).
- Sign-in, plain-HTTP notice: at 200 % the word "Framework," pushes the text past the notice's
  content box by 1 (Chromium), 11 (Firefox), 0 (WebKit) px; at 200 % + 1.4.12 by 41, 52, 40 px:
  the letters are drawn across the notice's border (screenshot `r1-firefox-signin-http-200+1412.png`),
  a 1.4.12 failure by the Understanding text above. Cause: the notice is a flex row, its text an
  anonymous flex item whose minimum width is its longest word.
- Model profile, 200 % + 1.4.12: the card titles "Embeddings" and "Speculative decoding" 5 to 14 px
  past the `h3` box, into the card's padding; "Temperature" 8 px past its legend. Nothing cut.
- At 300 % (beyond any criterion, for the record): +62 on every page, the banner's app name. Out of
  scope.

### 0.2 Items 2 to 5

- **Item 2, sign-in at 1280 px**: art x 40 to 460, panel x 500 to 920, at 960, 1280 and 1600 px in all
  three engines (`.signin-page` is `main > *`, `max-width: 880px`, no inline margin): at 1280 the
  right 360 px are empty, at 1600 the right 680 px.
- **Item 3, forced colours dark, Chromium**: reproduced. axe `color-contrast` on `.primary`: "1.17
  (foreground #1b1714, background #000000)". The computed colours are `rgb(255, 255, 255)` on
  `rgb(0, 0, 0)` and the screenshot shows white text on black (`r1-fc-dark.png`): axe reports the
  author colour (`--on-accent` in the dark theme, `#1b1714`) instead of the forced one. Light forced
  colours: axe 0 (the author colour happens to pass). Firefox forced colours dark: axe 0. An
  artefact of the tool, not a failure: in forced colours the browser paints system colours
  ([CSS Color Adjustment 1, 3.1](https://www.w3.org/TR/css-color-adjust-1/#forced-colors-properties)).
- **Item 4, nav at 110 %**: reproduced in all three engines. "Model profiles" (the sub-entry) wraps
  under its icon at 110 % when it is the current entry (600 weight; entry 68 px high), at 125 % in
  every state; "Overview" at 200 %. This is the designed reflow of an entry with an icon (H8a.5:
  the label moves whole under the icon), all text visible.
- **Item 5, Firefox notice at 200 %**: now 11 px in Firefox (the "1 px" of the S1b review was the
  notice box; this is the text past the content box), see 0.1.

## 1. Decisions

| Item | Decision | Why |
|---|---|---|
| 1, table | The table scrolls inside its own box (R1.3 to R1.6); the page never scrolls sideways | Reflow excepts data tables: "Data tables and grids have a two-dimensional relationship between column and row headers and their data cells", and the Pass of its Figure 13 is "The table is excepted from Reflow, and is rendered within a scrollable container" ([Understanding 1.4.10](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html)). Stacked rows were rejected: `display: block` on a table needs a label per cell (strings and markup) and is more code than one wrapper; letting the headers and names break anywhere was measured (`r1d.js`, candidate B): it fits, but in 60 px columns with "Model profile" broken into letters, unreadable |
| 1, instance intro, card titles | Long words break when they do not fit, everywhere (R1.1) | One inherited declaration; the break happens only where a line has no other break point ([CSS Text 3, overflow-wrap](https://www.w3.org/TR/css-text-3/#overflow-wrap-property)) |
| 1, notice; 5 | The notice's text breaks a long word (R1.2) | `break-word` does not change a flex item's minimum width, `anywhere` does (same source) |
| 1, the "Temperature" legend | Accepted, no change | 8 px into the card's padding, nothing cut or overlapped, only at 200 % + 1.4.12 at 320 px (beyond the floor) |
| 2 | Centre the sign-in pair in `main` (R1.7) | **Preference**: the banner spans the window with the theme button at the far right; a pair stuck at the left leaves a third of a 1280 px window empty. Signed in, pages stay left-aligned beside the nav |
| 3 | No change | Artefact (0.2); the UX check runs no forced-colours state, so it does not meet it |
| 4 | Not in this slice | Designed behaviour, nothing lost, no criterion fails; a lighter current weight would weaken a cue that is not colour (1.4.1) |

## 2. Criteria

**10 R1.1 Long words break in all text.** The top-level `body` rule (today: margin, background,
color, font-family, font-size, line-height) gains `overflow-wrap: break-word`. It is inherited; every
rule that sets `overflow-wrap` itself keeps it (`h1`, `.session > span`, the nav entries, `th, td`,
`.model-profiles th`, `.model-profiles td:nth-child(2)`, `.tile-id`, `.model-profile-summary`,
`.jump ul button`, `.full-value`, `.assets dd`): an element's own declaration always beats an
inherited value, whatever the specificity. Pin: the `body` rule declares
`overflow-wrap: break-word`; the rules just listed keep their values.

**10 R1.2 The notice breaks a long word.** The top-level `.notice` rule gains
`overflow-wrap: anywhere` (its padding, border, radius, background, display, align-items and gap
unchanged: `HearthH8b.test.ts` pins three of them). `.notice > .icon` (0,2,0) is unaffected (an svg
has no text).

**10 R1.3 The table sits in a scroll box.** On the Model profiles page, the `table.model-profiles`
is the only child of a `div.table-scroll`, which sits where the table is today (after the filter
and its status lines, before `model-profiles-off-note`: the order `framed()` checks is unchanged).
The div has no `tabindex`, no `role`, no `aria-*` attribute: the table keeps its name
(`aria-labelledby="page-heading"`) and every row holds a focusable control (the switch, and the link
when the id is readable), so Tab reaches every column and the browser scrolls the box to show it
([2.1.1 Keyboard](https://www.w3.org/TR/WCAG22/#keyboard)). A region with a name would be one more
landmark repeating the `h1` and one more Tab stop on every visit, in the common case where nothing
scrolls. Firefox makes an overflowing scroll box a Tab stop of its own (measured); fine. The empty,
error, loading and no-match states render no `div.table-scroll`.

**10 R1.4 The box rule.** A new top-level rule, selector exactly `.table-scroll` (0,1,0):

```css
.table-scroll {
  overflow-x: auto;
  scroll-padding-inline: var(--space-2);
  background: var(--bg);
  border-radius: var(--radius-card);
  box-shadow: var(--shadow);
  border: 1px solid transparent;
}
```

The lift moves from the table to the box: a scroll box clips what its child paints outside it, so
the table's own shadow (in dark a 1 px `--line` ring outside the table) would vanish. The rounded
box clips the table's corners. `scroll-padding-inline` keeps the 3 px focus ring and its 2 px offset
whole when Tab scrolls a switch or link to the box's edge
([2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible)); measured with the candidate: 3 to 53 px
between ring and edge in all three engines. No `overflow-y` (stays visible), no height, no width.

**10 R1.5 The table rule loses the lift.** The top-level `table` rule keeps `border-collapse:
separate`, `border-spacing: 0`, `background: var(--bg)` and `width: 100%`, and no longer declares
`border-radius`, `box-shadow` or `border`. The `.model-profiles th`, `td`, `td:nth-child(2)`,
`td:last-child` rules and `tr:last-child td { border-bottom: 0 }` are unchanged (the headers stay
on one line, so at 320 px and 100 % text the table is 288 px as today and nothing scrolls: measured
286 of 286 px in all three).

**10 R1.6 Forced colours.** No forced-colours block is added or changed. The box's transparent 1 px
border is the table's edge there, where a shadow computes to none
([CSS Color Adjustment 1](https://www.w3.org/TR/css-color-adjust-1/#forced-colors-properties)); the
shadow-token boxes are now `.card`, `.dialog`, `.save-bar`, `.signin-panel`, `.tile`,
`.table-scroll` (no `table`).

**10 R1.7 The sign-in pair is centred.** The top-level `.signin-page` rule gains
`margin-inline: auto` after its four declarations (display, grid-template-columns, gap,
align-items). `main > *` (0,0,1) sets only `max-width: 880px` and no margin, so nothing competes.
Below 960 px `main`'s content box is at most 880 px wide, so the margin is 0 there and the narrow
layouts do not move; signed in there is no `.signin-page`. In forced colours the drawing is hidden
and the panel alone takes the 880 px, centred the same way.

**10 R1.8 Nothing else.** No string is added, changed or removed (`en_US.json` untouched). No new
class but `table-scroll`; no CSS selects an element by `#id`. The nav (item 4) and the forced-colours
blocks (item 3) are unchanged.

Keyboard and screen reader: the Tab order of the Model profiles page is unchanged in Chromium and
WebKit (filter, Clear when shown, then per row link and switch); Firefox adds the box as one stop
when it overflows. A screen reader reads the same table, same name, same headers: the div is
generic and has no role.

## 3. Older tests that pin something that changes

- `HearthH2b.test.ts`: "H2b.2: the table is a lifted box…" (radius, shadow and border move to
  `.table-scroll`; collapse, spacing, background and width stay on `table`); "section 0: the boxes
  lifted by a shadow alone carry the 1px transparent border…" (`['.card', 'table']` becomes
  `['.card', '.table-scroll']`); the test whose message says "the boxes drawn with the shadow token
  are exactly the containers of H2b" (`table` becomes `.table-scroll` in the expected list); its
  `CONTAINERS` list (no state look) gains `.table-scroll`.
- `HearthH7.test.ts`: "H7.6: the page is a two-column grid by room…" (the exact declaration list of
  `.signin-page` gains `margin-inline`; its message "no margin the brief did not ask for" now allows
  this one).
- Any other test that asserts the exact declaration list of `body` or `.notice` (none found by the
  designer; `HearthH8b.test.ts` checks three `.notice` properties by name, unaffected).

## 4. UX check (`fe/ux/check.js`)

Gain one cheap text pass: at 320 px, light theme only, every existing state (10) loaded once more
with `html { font-size: 200% !important }` added after the page settles, and once with that plus the
1.4.12 rules of section 0; only the scrollWidth check runs (no axe: the colours do not change).
Cost: 20 more loads per browser without axe, about 20 to 30 s on the current ~2 minutes (an
estimate). It would have caught the table and the intro. Its lines name the mode
(`200%`, `200%+1.4.12`). The stub needs nothing new.

## 5. What the designer measures after the coder

- `r1.js` again: every page, 320 px, all four modes, three engines: scrollWidth 320 everywhere; the
  table's box 286 px wide, its content wider at 200 %.
- Tab through the table at 200 %: the box scrolls to each switch and back to each link, ring whole;
  arrow keys scroll the box once it or a link in it has focus (measured with the candidate: 40 to
  110 px per press).
- Light and dark at 1280 and 320: the box looks as the table did (same corner, shadow, outline in
  dark), forced colours (Chromium, Firefox): the box's edge drawn.
- The notice at 200 % + 1.4.12: no letter past its border. The instance intro: the file name broken
  inside the column.
- Sign-in at 960, 1280, 1600 px: the pair centred in `main` (at 1280 x 200 to 1080); at 320 and
  480 unchanged.
- `npm --prefix fe/ux run check`: 0 failing, including the new text pass.
