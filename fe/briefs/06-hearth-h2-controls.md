# 06 H2: Hearth controls (H2a) and containers (H2b)

Restyle only: the shapes of direction B "Hearth" on today's markup, with the tokens H1a declared
(`fe/briefs/06-hearth-h1-tokens.md`, section 1). H2 adds no token, no TSX, no strings key and no DOM.
Two cycles: **H2a** the controls (buttons, fields and selects, the boxed number, the checkbox, the
switch, the focus ring, the disabled hatch), **H2b** the containers (cards, tables, the save bar, the
dialog). H2b depends on H2a only for the order of the edits, not for any rule.

Sources: `fe/briefs/04-visual-identity.md` 1.1 (40 px controls, 1.5 px borders, 3 px ring, the
hatch, the 44 by 24 px switch, radii) and 4 (criteria); the b2 mockups' CSS
(`build/ux-review/visual/b2-model-profile.html`); `fe/DESIGN.md` 5.2 (Disabled looks, Switch,
Buttons, Save bar); `fe/src/DisabledLook.test.ts`.

## 0. Decisions

- **Selectors.** Generic or kept classes only: `button`, `.primary`, `.secondary`, `.about`, `.jump`,
  `input[...]`, `select`, `input[role='switch']`, `.card`, `table`, `th`, `td`, `.save-bar`,
  `.dialog`. No new class, and no `preset`/`presets` class (brief 04 section 2).
- **Not in H2, because it needs DOM**: the state badge as a pill (it has no wrapper element today:
  `StateBadge.tsx` renders the shape only; H6), a warning variant of the notice (needs a class), icons
  (H8). Not in H2, because it is the shell: the nav's entries, the banner, the Sign in link (H5). H2
  touches **no `.nav`, `.banner`, `.session` or `.nav-label` rule**; the Menu and Sign out buttons
  change shape only because they are `.secondary`.
- **The jump list keeps 32 px buttons** until H9 redraws it as a text list: ten 40 px pills would
  make the model profile page longer at 320 px for one slice (a preference).
- **The switch knob jumps, no transition** (H1a.11; brief 04 criterion 12 then holds trivially).
- **Notices and alerts keep their border.** They already use `--radius` (10 px since H1a) and
  `--surface`; the mockup drops the notice's border, but a notice is "a bordered paragraph"
  (`fe/DESIGN.md` 5.2) and a border survives forced colours where a fill does not (a preference). So
  `.notice` and `.alert` have no H2 change.
- **Cards and tables in forced colours.** `box-shadow` computes to `none` there (read:
  [CSS Color Adjustment 1, 3.1](https://www.w3.org/TR/css-color-adjust-1/#forced-colors-properties)),
  so the lifted boxes would lose their edge; they get `border: 1px solid transparent`: only
  `background-color` keeps its alpha when forced, other colours are forced to a system colour (same
  section), so the border shows there and is invisible otherwise. Cards carry no meaning (brief 04
  criterion 4), so this is a preference.
- **Tables use `border-collapse: separate; border-spacing: 0`**, because `border-radius` does not apply
  to a table whose borders collapse (read:
  [CSS Backgrounds 3, 4.6 Effect on Tables](https://www.w3.org/TR/css-backgrounds-3/#border-radius-tables):
  "When border-collapse is collapse, they have no effect"). The last
  row drops its divider so no straight line crosses the rounded corners.
- **No hover look on controls** in H2 (as built and as the mockup: its only `:hover` rules are the
  nav link and the tile id). Hover is no WCAG requirement; the pointer cursor and the 3 px focus ring
  stay the feedback. A hover tint for buttons is a later, separate decision (a preference).
- **`--border-w` 1.5px renders as 1 px at a device pixel ratio of 1** in Chromium, Firefox and
  WebKit (measured, H2a review): a border width over one device pixel is rounded down to whole
  device pixels ([CSS Values 4, snap a length as a border width](https://www.w3.org/TR/css-values-4/#lengths)).
  The contrast figures of H2a.9 do not depend on the width.
- **The hatch** goes on the disabled field and the disabled button, not on the checkbox or the switch
  (20 px boxes show no pattern; a preference). In forced colours a gradient `background-image`
  computes to `none` (same section): the dashed border stays the cue.

## 1. H2a: controls

**H2a.1 Buttons.** The `button` rule: `min-height: var(--control-h)` (40 px), `min-width: 24px`
(kept), `border-radius: var(--radius)`. `.primary` `border: var(--border-w) solid var(--accent)`,
`.secondary` `border: var(--border-w) solid var(--control)`, both `border-radius:
var(--radius-pill)` and `padding: var(--space-1) var(--space-4)`. `.about` `border-radius:
var(--radius-pill)`, its `min-height: 24px` kept. `.jump button` (a new rule on the kept class)
`min-height: 32px`. Every target stays at least 24 by 24 px
([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).

**H2a.2 Fields.** The enabled field rule (`input[type='text'], input[type='password'], select`) and
`.model-profiles-filter-row input`: `min-height: var(--control-h)`, `border: var(--border-w) solid
var(--control)`, `border-radius: var(--radius)`, background `var(--field)` (H1a). `input.number`
keeps `10ch` (the full-value rule counts 8 characters, `fe/DESIGN.md` 5.2). The `.field-label`
`min-height` in the 720 px media rule becomes `var(--control-h)`, so a label lines up with its
control.

**H2a.3 Checkbox.** `input[type='checkbox']`: `width` and `height: 1.25rem`, `border: var(--border-w)
solid var(--control)`, `border-radius: var(--radius-check)`; the tick (`::before`) scaled to
`0.6rem` by `0.32rem`, still drawn by its left and bottom borders in `--on-accent`. Checked, disabled
and aria-disabled looks keep their rules and their order (2h.163c).

**H2a.4 Switch track.** A rule `input[role='switch']`, after the checkbox rules: `width: 2.75rem`,
`height: 1.5rem` (44 by 24 px), `border-radius: var(--radius-pill)`, `place-content: center start`,
`padding: 0 2px`. The track takes the checkbox's fills: `--field` off, `--accent` checked, `--surface`
with the dashed border disabled or aria-disabled (the existing rules apply, as `fe/DESIGN.md` 5.2
says "drawn and made inactive exactly as a checkbox").

**H2a.5 Switch knob.** `input[role='switch']::before`: `visibility: visible`, `width` and `height:
1.05rem`, `margin: 0`, `border: 0`, `border-radius: 50%`, `transform: none`, `background:
var(--control)`. `input[role='switch']:checked::before`: `background: var(--on-accent)`,
`translate: 1.2rem 0`. `input[role='switch']:disabled::before,
input[role='switch'][aria-disabled='true']::before`: `background: var(--muted)`, written **after**
the checked knob rule, so a disabled switch that is on keeps a `--muted` knob (as 2h.163c for the
box). The state is also the word beside the switch, so position and colour are not the only cue
([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)).

**H2a.6 Switch in forced colours.** Inside `@media (forced-colors: active)`:
`input[role='switch']::before, input[role='switch']:checked::before { forced-color-adjust: none;
background: CanvasText; }` and, after it, for the disabled and aria-disabled knob, `background:
GrayText`. Without it the knob's background would be forced to the track's colour and vanish. The
checked selector is needed for specificity: `input[role='switch']:checked::before` (H2a.5, outside
the media block) outweighs `input[role='switch']::before` and would keep the `--on-accent` knob on
the forced `Canvas` track (correction after the H2a review, 2026-11-18: the knob of a switch that is
on measured 1:1 against its track in Chromium's forced-colours emulation and in Firefox with
`browser.display.document_color_use` 2). The disabled rule has the checked rule's specificity and
comes after it, so it still wins. The existing GrayText border rules stay.

**H2a.7 Focus ring.** `:focus-visible` and `.card h3:focus-visible`: `outline: var(--ring-w) solid
var(--accent)`, `outline-offset: 2px`. `[tabindex='-1']:focus { outline: none }` stays. Ring
contrast: the accent rows P11 to P15 of the H1 brief, at least 6.04:1
([2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible), [1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast)).

**H2a.8 Disabled hatch.** The disabled field rule (`input:disabled, select:disabled,
textarea:disabled`) and the disabled button rule (`.primary:disabled, .secondary:disabled`): keep
`background: var(--surface)`, then, after it in the same block, `background-image:
repeating-linear-gradient(135deg, transparent 0 6px, var(--hatch) 6px 8px)`; the dashed border
becomes `var(--border-w) dashed var(--control)`. Everything else in 2h.163a and 2h.163e holds.

**H2a.9 Hatched contrast.** Computed in the test with `--hatch` composited over `--surface` (each
channel `a * hatch + (1 - a) * surface`, unrounded), in both themes:
`--muted` on it at least 4.5 (a preference: disabled text is exempt from
[1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum)), measured **4.86** light, 5.49 dark;
`--control` on it at least 3 (exempt from 1.4.11 likewise), measured **3.05** light, 3.21 dark
(measured: `/tmp/claude-designer/cr2.py`, 2026-11-18). This is the tightest pair of Hearth; a darker
`--hatch` or a lighter `--surface` breaks it first.

**H2a.10 The pins that change** in `DisabledLook.test.ts`: `dashedControl` accepts
`var(--border-w) dashed var(--control)` (shorthand) or `border-style: dashed` plus `border-color:
var(--control)` (163a, 163c, 163e); 163b's enabled border becomes `var(--border-w) solid
var(--control)`. 163a and 163e add: the rule's `background-image` is the hatch of H2a.8. The rest of
the file holds, the token list included (H2 adds no token: pinned by H1a's test, unchanged).

**H2a.11 Nothing else moves.** No `.tsx` change; every other test green.

**What the tester pins (H2a)**: H2a.1 to H2a.8 as declarations of the named top-level rules (with
`prop()` from `DisabledLook.test.ts`, the last value of a property in a block); the two orders of
H2a.4 (switch rule after `input[type='checkbox']`) and H2a.5 (disabled knob after checked knob) by
`at`; H2a.6 inside the forced-colours block by matched braces; H2a.9 as computed ratios; the
`min-height` values `var(--control-h)` (buttons, fields), `24px` (`.about`, `.check`,
`.model-profiles-switch`), `32px` (`.jump button`), and H1a's `--control-h: 40px`.

**Size**: 11 criteria, one CSS file, edits to one test file plus one new test file.

## 2. H2b: containers

**H2b.1 Card.** `.card`: `background: var(--bg)` (H1a), `border-radius: var(--radius-card)`,
`box-shadow: var(--shadow)`, `border: 1px solid transparent`. Padding unchanged
(`var(--space-4) var(--space-5)`): the 320 px layout stays as measured.

**H2b.2 Table box.** `table`: `border-collapse: separate`, `border-spacing: 0`, `background:
var(--bg)`, `border-radius: var(--radius-card)`, `box-shadow: var(--shadow)`, `border: 1px solid
transparent`, `width: 100%` (kept). Applies to the Model profiles table and, until H6 turns it into
tiles, the Overview's instances table.

**H2b.3 Table cells.** `th`: `color: var(--muted)`, `font-size: var(--fs-sm)`, border
`1px solid var(--control)` kept (the header line, N2 3.91 / 4.12). `td`: `1px solid var(--line)`
(H1a). A rule `tr:last-child td { border-bottom: 0; }`. Cell paddings unchanged (the Model profiles
table keeps its three columns at 320 px). `.model-profiles tr.off { color: var(--muted) }` holds.

**H2b.4 Save bar.** `.save-bar`: `background: var(--bg)` (H1a), `border-top: 1px solid
var(--control)` kept (the bar's edge over a card of the same fill: N2), `border-radius:
var(--radius-card) var(--radius-card) 0 0`, `box-shadow: var(--shadow)`. `position: sticky`,
`bottom: 0`, `z-index`, paddings, the `html:has(.save-bar)` scroll padding and the short-viewport
rule all unchanged ([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum); 2h.135 and the
7rem test hold). Differs from mockup B on purpose: no extra upward
shadow and the top line stays `--control`, not `--line`; the line is the bar's edge over a card of the
same fill (a preference; H2b review, 2026-11-18).

**H2b.5 Dialog.** `.dialog`: `border-radius: var(--radius-card)`, `box-shadow: var(--shadow)`;
`background: var(--bg)` and `border: 1px solid var(--control)` kept; `::backdrop` unchanged.

**H2b.6 Nothing else moves.** No `.tsx` change; `.notice`, `.alert`, `.nav`, `.banner` rules
unchanged; every other test green.

**What the tester pins (H2b)**: the declarations of H2b.1 to H2b.5 on the named top-level rules; that
the `table` rule's `border-collapse` is `separate`; the `tr:last-child td` rule; that the save bar's
`position: sticky` and the scroll padding are as before (the existing 2h.135 and 7rem tests).

**Size**: 6 criteria, one CSS file, one new test file.

## 3. What only the designer's UX pass can see (after each of H2a and H2b)

`npm --prefix fe run build`, `npm --prefix fe/ux run check` (seven states, three engines, 1280 and
320 px, light and dark; 0 axe-core violations, 0 sideways scroll). By eye, with screenshots in both
themes:

- H2a: the Server page with default boxes ticked and unticked (hatch plus dashed vs solid), a field
  marked invalid, the Show toggle pressed; the instance page's run controls with one aria-disabled;
  the Model profiles switches off, on, and aria-disabled during a write (stub delay); focus on a pill
  button, a field, a checkbox, a switch, a jump target (the 3 px ring whole, not clipped by a card's
  edge, [2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible)); forced colours in Firefox
  (`browser.display.document_color_use` 2): the knob visible in both positions, disabled controls
  dashed.
- H2b: cards and tables lifted in light, outlined in dark; the save bar at 320 px clean and with a
  refused-save alert and its button: re-measure its height against the 7rem floor (brief 04 section 0:
  40 px controls make it taller) and tab to the last field of the Server page and of the model profile
  page to check the field is not under the bar; a short viewport (height 500 px): the bar not sticky;
  the Model profiles table at 320 px (three columns, rounded corners, no line across them); the
  sign-in Dialog after a 401; forced colours: card and table edges visible.

## 4. What must not change (both cycles)

DOM order, ids, classes, roles, names, texts and keys; focus order and focus moves; the 320 px rule
([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)); the sticky save bar and its scroll padding; no
motion (H1a.11); the existing forced-colours blocks (H2a.6 only adds); text spacing: every height H2
sets is a `min-height`, never a fixed `height` on a box holding text
([1.4.12](https://www.w3.org/TR/WCAG22/#text-spacing)); the switch's and the checkbox's `width` and
`height` are boxes without text.
