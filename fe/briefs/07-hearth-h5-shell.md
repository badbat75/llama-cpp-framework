# 07 H5: the Hearth shell (banner, nav, three carried issues)

Direction B's banner and nav (brief `fe/briefs/04-visual-identity.md` 1.2 and section 2 row H5) on
today's shell, after H3 (the mark) and H4 (the theme button). Too big for one cycle, so three:

| Cycle | What | DOM | Criteria |
|---|---|---|---|
| **H5a** | the banner: transparent, the tools group pushed to the end, icon-only theme button and hidden user name below 480 px | one wrapper `div` | 7 |
| **H5b** | the nav: transparent, pills, the `--accent-soft` current entry replacing the inset bar, group labels and rules, the 320 px menu | none | 7 |
| **H5c** | the three carried issues: Discard's focus scroll, the About button under the save bar, the save bar with 200 % text | none (one CSS rule; the UX check's text-size method) | 5 |

Order: H5a, then H5b (both edit the shell's rules; H5b's current-entry rule also covers the banner's
Sign in link). H5c touches only the save bar and can land before, between or after them. No strings
key is added or changed in any of the three.

Labels: **measured**, **read** (with its URL), **assumed**, **preference**.

## 0. Decisions

- **Transparent shell.** Banner and nav lose their `--surface` fill and their `--control` rules: they
  sit on the paper (`--page`) and the content cards lift off it (brief 04 1.2, the b2 mockups). The
  rules were dividers, not boundaries a user needs (brief 04 criterion 4). Below 960 px, the open nav
  keeps one decorative `1px solid var(--line)` under it, so the menu's end reads (a preference).
- **One wrapper, no role.** Menu, the theme button and the session part go into a plain
  `div.banner-tools` pushed to the end with `margin-left: auto`, so they wrap as one group and stay
  right-aligned at every width. Without it, at about 400 px Menu stays on the name's row and the
  theme button and Sign out wrap to the left of a second row (measured on paper from the widths:
  name and mark about 220 px, Menu 90, a 44 px icon button, Sign out 100). A `div` adds nothing to
  the accessibility tree ([HTML-AAM, `div`](https://www.w3.org/TR/html-aam-1.0/#el-div): role
  `generic`), so names, order and focus stay.
- **Below 480 px**, the theme button shows its moon only and "Signed in as ..." is visually hidden,
  both kept in the DOM, so the button's name and the banner's text are unchanged (brief 04 1.2; the
  hidden user name is a preference recorded there: Sign out already shows a session is open). Same
  declarations as `.visually-hidden`, inside a media rule (a class toggled by width would need
  script). An icon-only button has no visible label, so
  [2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name) has nothing to match; it stays 40 by 40 px
  ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).
- **The current nav entry**: `--accent-soft` pill, `--accent` text, weight 600 (drawn as Atkinson's
  700, brief 06 H1b), no inset bar. The non-colour cue is the weight; the text pair `--accent` on
  `--accent-soft` is P15, 6.04 / 6.93, already pinned ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color),
  [1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum)). In forced colours the fill goes and the
  weight stays, as the bar's colour did before. Weight stays 600, not the mockup's 700, so the 600
  pin holds (rendered the same).
- **Hover** on nav links stays the underline (links, not controls; H2's "no hover on controls"
  stands). The mockup's `--surface` hover fill is not taken (a preference).
- **The Menu button keeps its text**, no icon until H8.

## 1. H5a: the banner

**H5a.1 DOM: the tools group.** The `header` holds, in order, the home link (`.app-name`, with H3's
mark) and one `div` with class `banner-tools` and no `role`; the `div` holds, in order, the Menu
button, the theme button (H4) and the session part (the Sign in link `a.session`, or
`div.session` with the user and Sign out; nothing while the session is read). Accessible names,
roles, ids and the Tab order (home, Menu, Dark theme, Sign in / Sign out) are those of H4.2;
1a.1, 1a.3 and 1b.5a hold unchanged.

**H5a.2 `.banner`.** `background: transparent`; no `border-bottom` declaration (or `border-bottom: 0`);
`padding: var(--space-4) var(--space-5)`; `display: flex`, `flex-wrap: wrap`, `align-items: center`,
`gap: var(--space-2) var(--space-3)`.

**H5a.3 `.banner-tools`.** A top-level rule with `display: flex`, `flex-wrap: wrap`,
`align-items: center`, `gap: var(--space-2)`, `margin-left: auto`. The `.session` rule loses its
`margin-left: auto` (no `margin-left` declaration, or `0`); its flex, wrap and gap stay; 46b's
`.session > span { overflow-wrap: anywhere }` stays.

**H5a.4 The Sign in link.** `a.session`: `border-radius: var(--radius-pill)`, padding
`var(--space-1) var(--space-4)`, `min-height: 32px`, `display: inline-flex`, `align-items: center`.
Its current look is the nav's current rule (H5b.3).

**H5a.5 Narrow: icon-only and hidden name.** An `@media (max-width: 479.98px)` block holding a rule
whose selector list is exactly `.theme-switch-label` and `.session > span`, with the declarations of
`.visually-hidden` (`position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0;
overflow: hidden; clip-path: inset(50%); white-space: nowrap; border: 0`), and a `.theme-switch`
rule with `min-width: var(--control-h)` and `padding: var(--space-1) var(--space-2)`. The 46b rule
outside it is unchanged.

**H5a.6 Padding at 320 px.** Inside the `@media (max-width: 959.98px)` block, `.banner` takes
`padding: var(--space-3) var(--space-4)` (the 16 px gutter of the page, so mark, name and tools fit
at 320 px: [1.4.10](https://www.w3.org/TR/WCAG22/#reflow)).

**H5a.7 Nothing else moves.** No key; the nav rules untouched (H5b); every other test green.

**What the tester pins (H5a)**: H5a.1 on the rendered `App` (the `div.banner-tools` is the
`header`'s second element child; its element children in order; the names by role inside it; the
`header` has exactly two element children); H5a.2 to H5a.6 as declarations of the named rules
(`prop()`, media blocks by matched braces, as `HearthH2a.test.ts`).

## 2. H5b: the nav

**H5b.1 `.nav`.** `background: transparent`; `padding: var(--space-3) var(--space-4) var(--space-5)`;
its `border-bottom` becomes `1px solid var(--line)` (seen only below 960 px, when the menu is open).
In the `@media (min-width: 960px)` block, `.nav` keeps `border-bottom: 0` and declares no
`border-right` (or `border-right: 0`).

**H5b.2 Entries as pills.** `.nav a`: `display: flex`, `align-items: center`, `min-height: 32px`,
`padding: var(--space-1) var(--space-4)`, `border-radius: var(--radius-pill)`; `color: var(--text)`,
`text-decoration: none`, `overflow-wrap: anywhere` kept. `.nav a:hover` keeps the underline.

**H5b.3 The current entry.** The one rule whose selector list is `.nav a[aria-current='page']`,
`.nav a[aria-current='true']`, `a.session[aria-current='page']` (unchanged list, still the only rule
naming `aria-current='true'`) declares `background: var(--accent-soft)`, `color: var(--accent)`,
`font-weight: 600`, and **no** `box-shadow`. The DESIGN.md 2.3 test of `fe/src/DisabledLook.test.ts`
(its last assertion, `box-shadow: inset 3px 0 0 var(--accent)`) changes to: `background` is
`var(--accent-soft)` and the rule has no `box-shadow`; its other assertions hold.

**H5b.4 Group labels.** `.nav-label`: `font-size: var(--fs-xs)`, `color: var(--muted)`,
`font-weight: 600`, `margin: var(--space-4) 0 var(--space-1)`, `padding: 0 var(--space-4)` (aligned
with the pills' text). `--muted` on `--page` P6 5.65 / 7.52.

**H5b.5 Group rule.** A rule `.nav ul[aria-labelledby]` with `margin-left: var(--space-3)`,
`padding-left: var(--space-2)`, `border-left: 2px solid var(--line)`: decorative (the group is told
by its label and by the list's name, `aria-labelledby`), so `--line` needs no contrast
(brief 04 criterion 4). The Overview list (no `aria-labelledby`) has no rule.

**H5b.6 Targets and reflow.** Every nav link at least 32 px high (2.5.8 asks 24); at 320 px a long
instance id still breaks inside its pill (`overflow-wrap: anywhere`), no sideways scroll
([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)); text spacing does not clip a pill (only
`min-height`, [1.4.12](https://www.w3.org/TR/WCAG22/#text-spacing)).

**H5b.0 (carried from the H5a review) The tools stay right-aligned when they wrap.** `.banner-tools`
also declares `justify-content: flex-end`. Measured 2026-11-18 (H5a review, all three engines): when
the group is wider than what is left on the name's row, it takes a whole row and its items wrap to
the left inside it (480 px signed in with the 32-character stub name: Menu at x 16, Sign out on a
third row at x 16; 320 px with 200 % text). Section 0's "right-aligned at every width" needs this one
declaration; DOM, Tab order (row by row, still the visual order, 1.3.2 / 2.4.3) unchanged.
**Correction (H5b review, 2026-11-18):** that declaration is not enough. At 480 px signed in, Menu
and the theme button end at x 464 (right-aligned), but `.session` (the name and Sign out) is itself a
wrapping flex row wider than the line, so its items wrap to the left inside it: the name at x 16 on
the third row, Sign out at x 16 on the fourth (Chromium, Firefox, WebKit alike). `.session` also
declares `justify-content: flex-end`; the tester pins it beside `.banner-tools`'.

**H5b.7 Nothing else moves.** No DOM, no key; `ProfilesNav.test.tsx` holds as is (it pins the
`nav-label` class, the ids and the lists, not the look); the Menu's behaviour (Escape, close on a
link, `aria-expanded`) unchanged.

**What the tester pins (H5b)**: H5b.0's declaration on `.banner-tools`; H5b.1 to H5b.5 as declarations of the named rules; the edited
DisabledLook test; H5b.6's `min-height: 32px` and `overflow-wrap: anywhere` on `.nav a`.

## 3. H5c: the three carried issues

**(a) Giving Discard focus scrolls the page** (Chromium 422 to 451 px, WebKit 100 to 147 px; from the
commit that left it open, cause not verified). Likely cause (assumed): `html:has(.save-bar)` sets
`scroll-padding-bottom` to the bar's height plus `--space-3`, which the browser uses to decide
whether a focused element is in view (read: [CSS Scroll Snap 1, `scroll-padding`](https://www.w3.org/TR/css-scroll-snap-1/#scroll-padding):
it "should affect whether an element ... is considered scrolled into view (e.g. for targeting or
focusing operations)"). A control inside the sticky bar always lies in that padded strip, so the
browser scrolls toward it, as far as the page allows (hence a small, end-of-page amount). Fix: the
bar's own controls cancel the padding with an equal negative `scroll-margin-bottom`, so their scroll
area sits inside the region. The spec ties `scroll-margin` to scroll-into-view operations
([5.1](https://www.w3.org/TR/css-scroll-snap-1/#scroll-margin)) but names focus only for
`scroll-padding`, so whether every engine's focus scroll reads it is **assumed**, and the UX pass
measures it in all three.

**(b) At 320 px with the menu open, an About button sits under the save bar** on a model profile
page. A sticky bar covering content scrolled under it is the design (`fe/DESIGN.md` 4, "Sticky save
bar"); what WCAG asks is that a **focused** control is not hidden
([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)). So no change unless the UX
pass shows a focused About button under the bar; if it does, H5c reopens with that measurement.

**(c) The Server save bar with text at 200 %.** The review emulated 200 % text by setting
`font-size: 200%` on `<html>` (`build/ux-review/h1a.js` line 143). That does not move the
short-viewport rule `@media (max-height: 31.98rem)`: a `rem` in a media query is the **initial**
font size, the browser's text-size setting, never a declaration (read:
[Media Queries 4, 1.3 Units](https://www.w3.org/TR/mediaqueries-4/#units)). A user who sets text to
200 % in the browser gets a 32 px initial size, so the rule applies below 1023 px of viewport
height: on a 320 by 568 phone and a 1280 by 720 window the bar is **not sticky** and sits at the end
of the form. So no CSS change: the check must set the browser's default font size instead.

**H5c.1 The bar's controls cancel the padding.** `index.css`: a top-level rule `.save-bar button`
with `scroll-margin-bottom: calc(-1 * (max(var(--save-bar-height, 7rem), 7rem) + var(--space-3)))`
(the same expression as the `html:has(.save-bar)` padding, negated; `/* prettier-ignore */` as that
one), after the `html:has(.save-bar)` rule.

**H5c.2 Not in the short viewport.** Inside `@media (max-height: 31.98rem)`, a rule
`.save-bar button { scroll-margin-bottom: 0; }`: there the bar is static and the padding is 0, and a
negative margin would let a focused button end below the viewport. The existing 2h.135 and 7rem
tests hold.

**H5c.3 Measured: no scroll on Discard's focus** (UX pass, not a unit test). On the Server page and
the model profile page, dirty, at 1280 and 320 px, in Chromium, Firefox and WebKit: `scrollY` before
and after `focus()` on Discard, and after Tab from the last field to Discard, differ by at most 1 px
when Discard was already fully in view. If an engine still moves, the fix is wrong for it: report
the figures, do not widen the fix in this slice.

**H5c.4 Measured: About focus clears the bar** (UX pass). 320 px, menu open, model profile page,
dirty: Tab from the nav through every About button and control down to the bar; each focused
element's bounding box ends above the bar's top (the 2.4.11 check of the designer playbook).

**H5c.5 Measured: 200 % text** (UX pass). The browser's own default font size at 32 px (Firefox:
the pref `font.size.variable.x-western` 32 through Playwright's `firefoxUserPrefs`; Chromium: the
DevTools Protocol's `Page.setFontSizes` with `standard: 32`; WebKit: page zoom 200 %, since its text
setting is not reachable from Playwright, assumed): at 320 by 568 and 1280 by 720 the Server page's
bar computes `position: static`, no sideways scroll, and every field, tabbed to, is not under it.
`build/ux-review/` drops the inline `font-size: 200%` method.

**What the tester pins (H5c)**: H5c.1 and H5c.2 as stylesheet text (the declaration, its order after
`html:has(.save-bar)`, the media rule by matched braces). H5c.3 to H5c.5 are the designer's.

## 4. What only the designer's UX pass sees (after each cycle)

`npm --prefix fe run build`, `npm --prefix fe/ux run check` (seven states, three engines, 1280 and
320 px, light and dark: 0 axe-core violations, 0 sideways scroll). By eye, against the b2 mockups:
H5a, the banner on the paper, the tools right-aligned on one row at 1280 and on a second row at 320
px (two rows in all, signed in), icon-only theme button 40 by 40, a 32-character user name (46b) still
breaks inside at 480 px and above; H5b, the nav pills, the current entry (and `aria-current="true"`
on a model profile page) filled, the group rules, the open menu at 320 px with its `--line` end, the
focus ring whole on a current pill; forced colours: the current entry bold, the banner's buttons
bordered. H5c as its criteria.

## 5. What must not change (all three)

DOM order, ids, classes, roles, names, texts and keys except H5a's wrapper; focus order and every
focus move; the Menu's Escape and close rules; the 320 px rule; the sticky save bar, its scroll
padding and its short-viewport rule; no motion; the existing forced-colours blocks; targets at least
24 by 24 px. `fe/DESIGN.md` 2.3 ("current-page style") and 4 ("App shell") are updated by the designer
at the review of H5b, not before.
