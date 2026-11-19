# 08 H9: the model profile page's jump list as an index

Direction B's section nav, restyle part (brief `fe/briefs/04-visual-identity.md` 1.6 item 4 "Restyle
(H9)" and section 2 row H9). Today the jump list is ten outlined pill buttons wrapping over about six
rows at 320 px, with no visible label: it reads as ten actions (brief 04 1.6). H9 draws the same ten
buttons as an index, a column of underlined text entries behind a thin rule, and from 1280 px puts it
in a sticky rail beside the cards. The visible "On this page" label and the collapsing below 1280 px
are H10 (`fe/briefs/08-hearth-h10-h11.md`); the collapsed Assets card is H11. Two cycles:

| Cycle | What | DOM | Criteria |
|---|---|---|---|
| **H9a** | the list drawn as an index at every width | one class removed | 7 |
| **H9b** | the sticky right-hand rail from 1280 px | none | 6 |

Order: H9a, then H9b (same rules). H9a can run beside H6 and H7. Both come before H10, which changes
the `nav`'s children.

Labels: **measured**, **read** (with its URL), **assumed**, **preference**.

## 0. Decisions

- **They stay buttons.** Each entry moves the focus to its card's `h3` and writes no hash
  (`fe/briefs/02h-model-profile-save.md` 196, `ModelProfilePage.tsx` `jump`); a link would promise a
  navigation. They are drawn as text entries so the list reads as a table of contents, not a toolbar
  (a preference, brief 04 1.6).
- **One DOM change**: the jump buttons lose `className="secondary"`. Overriding every declaration of
  `.secondary` (and its forced-colours and disabled rules) from `.jump button` would be fragile; with
  no class, `.jump button` (0,1,1) owns the look.
- **No hover change** (H2a's "no hover on controls"; the entries are already underlined).
- **The rail uses `section:has(> .jump)`**: the model profile page's `section` holds the `h2`, the
  `nav.jump`, the notice, the Engine field, the cards and the save bar as direct children
  (`ModelProfilePage.tsx` `Content`), and has no class. `:has()` is already used by
  `html:has(.save-bar)` in `index.css`, so it adds no new engine requirement.
- **Not at 320 px**: a sticky list there, with the sticky save bar, would leave about half a 320 by
  568 viewport for content and risk [2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)
  (brief 04 1.6, "Not chosen").

## 1. H9a: the index look

**H9a.1 DOM.** The ten `button`s inside `nav.jump` have no `class` attribute. Everything else of the
`nav` (its `aria-label`, the `ul`, the `li`s, the button names in card order, `type="button"`, the
`jump` handler) is unchanged.

**H9a.2 `.jump ul`.** `display: block` (no `flex`, no `flex-wrap`), `margin: var(--space-2) 0
var(--space-4)`, `padding: 0`, `list-style: none`, `border-left: 2px solid var(--line)` (decorative:
the entries are told apart by their text, brief 04 criterion 4).

**H9a.3 `.jump button`.** `display: block`, `width: 100%`, `min-height: 32px`,
`padding: var(--space-1) var(--space-3)`, `border: 0`,
`border-radius: 0 var(--radius-pill) var(--radius-pill) 0`, `background: transparent`,
`color: var(--accent)`, `font-size: var(--fs-md)`, `text-align: left`,
`text-decoration: underline`, `text-decoration-thickness: 1px`, `text-underline-offset: 3px`. The
existing `.jump button { min-height: 32px }` rule is replaced by this one (one `.jump button` rule).

**H9a.4 Contrast and targets.** `--accent` on `--page` is 6.58 / 9.37 (brief 04 1.1, already pinned,
[1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum)); every entry is at least 32 px high and as
wide as the list ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)); the focus ring is the
global 3 px `--accent` ring, offset 2 px, and the rounded right end keeps it whole
([2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible)).

**H9a.5 Forced colours.** An `@media (forced-colors: active)` block holds `.jump button { color:
LinkText }`: with no border the entries read as the links they look like; the underline stays
(only its colour is forced: [CSS Color Adjustment 1, 3.1](https://www.w3.org/TR/css-color-adjust-1/#forced-colors-properties)).

**H9a.6 Reflow.** A long card title wraps inside its entry (`overflow-wrap: anywhere` on
`.jump button`); no sideways scroll at 320 px ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)); no
fixed heights ([1.4.12](https://www.w3.org/TR/WCAG22/#text-spacing)).

**H9a.7 Nothing else moves.** Focus moves, Tab order, names (`ModelProfileSave16.test.tsx` 196, 197
hold), the card headings' `scroll-margin-top` and focus ring.

**What the tester pins (H9a)**: H9a.1 on the rendered model profile page (no `class` on the ten
buttons, names in card order); H9a.2, H9a.3, H9a.5, H9a.6 as declarations of the named rules
(stylesheet text; exactly one top-level `.jump button` rule).

## 2. H9b: the rail from 1280 px

**H9b.1 Grid.** Inside `@media (min-width: 1280px)`, `section:has(> .jump)` declares
`display: grid`, `grid-template-columns: minmax(0, 1fr) 15rem`, `column-gap: var(--space-6)`,
`align-items: start`, `max-width: calc(880px + 15rem + var(--space-6))` (it outgrows `main > *`'s
880 px by the rail).

**H9b.2 Placement.** In the same block, `section:has(> .jump) > *` declares `grid-column: 1`, and
`section:has(> .jump) > .jump` declares `grid-column: 2`, `grid-row: 2 / span 40` (from under the
`h2` to past the last card; empty implicit rows are 0 high), `position: sticky`,
`top: var(--space-4)`, `max-height: calc(100vh - 2 * var(--space-4))`, `overflow-y: auto`, `margin: 0`.

**H9b.3 Below 1280 px nothing of H9b applies**: the list stays in place, as H9a draws it.

**H9b.4 Reading and focus order unchanged.** The `nav` stays in the DOM between the `h2` and the
notice; only its box moves, so the Tab order is still back link, jump entries, Engine field, cards,
bar ([1.3.2](https://www.w3.org/TR/WCAG22/#meaningful-sequence),
[2.4.3](https://www.w3.org/TR/WCAG22/#focus-order): the rail is read where it sits, at the top of the
settings, as a table of contents is).

**H9b.5 Nothing covered.** The rail takes its own column, so it hides no focused control; the save
bar spans the first column only and keeps its scroll padding
([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)).

**H9b.6 Nothing else moves.** No DOM; the H9a look inside the rail.

**What the tester pins (H9b)**: H9b.1 and H9b.2 as declarations inside the `min-width: 1280px`
block (found by matched braces); no rule outside that block names `section:has(> .jump)`.

## 3. What only the designer's UX pass sees

The `model-profile` state at 1280 and 320 px, both themes (the check's widths): at 320 the index
column under the `h2`, no sideways scroll; at 1280 (the rail's first width) the rail beside the
cards, sticky while scrolling to the last card, its entries not under the save bar; a jump from the
rail lands on the card's `h3` with the ring whole and not under the bar; forced colours: entries in
LinkText, underlined. 2.4.11 measured by tabbing through the rail at 1280 (designer playbook).

## 4. What must not change

The ten buttons' names, order, handler and focus target; the nav's name; the cards; the save bar and
its scroll padding; the Engine field; every key.

## 5. H9a review (2026-11-19)

**Approved.** Measured on the build of the uncommitted change (`ModelProfilePage.tsx` 854 to 858,
`index.css` `.jump ul`, `.jump button` and its forced-colours rule), stub `build/ux-review/stub8.py`,
script `build/ux-review/h9a.js`, in Chromium, Firefox and WebKit at 1280, 960, 480 and 320 px, light
and dark; "HEAD" is the same page with HEAD's two rules and class put back through the CSSOM.

- **H9a.1 met**: ten `button type="button"`, no `class`, card order; the `nav`'s aria snapshot equals
  HEAD's in all three engines (`navigation "Model profile sections"`, the same ten names).
- **H9a.2, H9a.3 met**: `ul` block with a 2px `--line` rule (decorative, 1.21:1 light, 1.39:1 dark);
  entries 15px `--accent`, underline 1px at 3px offset, no border or fill, no hover change.
- **H9a.4 met**: text 6.58:1 light, 9.37:1 dark on the page; every entry 32 px high and as wide as
  the list (286 px at 320, 878 at 1280; the shortest label, "Assets", is 43 px of text), stacked with
  no gap, so each target is 32 px by the list's width
  ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)). Keyboard focus: 3px solid ring at 2px
  offset, `:focus-visible`, no clipping ancestor, inside the viewport at 320 (its left edge 13 px,
  over the rule); at 1280 the ring spans the 880 px row with the rounded right end (a preference: it
  reads long beside a short title; H9b's 15rem rail ends it at 1280 and above).
- **Not colour alone** ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)): the underline, kept
  in every state and under forced colours, plus the place in a named `nav` list. A button drawn as a
  link is acceptable here: it moves focus within the page and writes no hash, as an in-page link
  would, so what it looks like matches what it does (section 0).
- **Behaviour (H9a.7) met**: click, Enter and Space focus the card's `h3`; its top lands at y 12 and
  its bottom at 40, the save bar's top at 801 to 835 (clean and dirty form), no other sticky or fixed
  box; hash and `history.length` unchanged; Back leaves the page as before. The ring shows on the
  heading after Enter or Space, not after a mouse click (`:focus-visible`, as before).
- **Height**: the index is 320 px tall at every width (HEAD: 278 at 320, 77 at 1280); the first card
  moves from y 937 to 987 at 320 (+50) and from 533 to 785 at 1280 (+252). The cost at 1280 is H9b's
  to remove; at 320 it is small.
- **H9a.6 met**: no sideways scroll at any width; at 200 % text (Chromium, Firefox) and with
  [1.4.12](https://www.w3.org/TR/WCAG22/#text-spacing) spacing, entries wrap (up to 7 lines for an
  injected long title), none clipped, no sideways scroll.
- **H9a.5 met**: forced colours, Chromium emulation and Firefox's `document_color_use` 2: entries in
  `LinkText` (rgb(0,0,159) / rgb(255,255,0) / rgb(0,0,238)), underline kept in the same colour, the
  focus ring visible, the rule forced to `CanvasText` and still drawn (acceptable: decoration).
- **Motion**: `scroll-behavior` is `auto`; the jump is instant with and without reduced motion.
- **axe** (WCAG 2.0 to 2.2 A and AA): 0 violations at each width, both themes, forced colours too.
- **For H9b**: nothing missing from this build; `section` still has the `nav` as a direct child.
