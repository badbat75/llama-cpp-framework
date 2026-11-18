# 07 H4: the light/dark theme switch

One button in the banner that turns the dark theme on and off, starting from the system's preference
(brief `fe/briefs/04-visual-identity.md` 1.9 and criterion 9; the user's answer 3 of its section 6:
"two states, Light and Dark, starting from the OS and back to following the OS when switched to the
OS's theme"). One test-first cycle, 12 criteria. DOM and behaviour: a new button, a module that reads
and writes the choice, an attribute on `<html>`, and the stylesheet's dark block rewritten. Where this
brief differs from brief 04 1.9 (when the attribute is present, the pressed look), this brief wins.

Labels: **measured**, **read** (with its URL), **preference**.

## 0. Decisions

- **A toggle button, not a switch.** A `button` with `aria-pressed`, named "Dark theme" in every
  state: its name never changes with its state ([WAI-ARIA APG, Button](https://www.w3.org/WAI/ARIA/apg/patterns/button/):
  "the label on a toggle does not change when its state changes"). A `role="switch"` checkbox
  (`fe/DESIGN.md` 5.2) is for a setting written at once and named by a thing, shown with a visible
  On/Off word; in the banner there is no room for that word at 320 px, and a pressed button is the
  pattern the page already uses for the Show toggle (`.secondary[aria-pressed='true']`).
- **The stored choice is the only state; the attribute mirrors it.** `localStorage` key `pai-theme`,
  value `light` or `dark`, or no entry (follow the system). `<html>` carries `data-theme="light"` or
  `"dark"` **only while a choice is stored**, and no `data-theme` otherwise. With no attribute the
  stylesheet alone follows the OS, live, before any script runs: the common case (nothing stored)
  can never flash. The button's pressed state is `stored ?? system`, `true` for dark.
- **Pressing** flips the theme in effect. If the new theme equals the system's, the entry and the
  attribute are **removed** (the page follows the OS again); otherwise both are set to the new theme.
  So the way back to "follow my system" is one press to the system's own theme, and there is no third
  state to explain (answer 3). A choice stored earlier that happens to equal the system's stays until
  the next press (no rewrite at load: a preference, so a load never writes storage).
- **The OS preference changes while the page is open.** With nothing stored, the CSS follows at once
  and the button's pressed state follows (a `change` listener on
  `matchMedia('(prefers-color-scheme: dark)')`). With a choice stored, nothing changes. Another tab's
  press is seen at the next load (no `storage` listener: a preference).
- **Read before the first paint, in `main.tsx`, no inline script.** `main.tsx` applies the stored
  choice to `<html>` before `createRoot(...).render(...)`. The bff sends no CSP today, so an inline
  script in `index.html` would work, but it would duplicate the read logic outside the tested module
  and need a hash the day a CSP comes. The cost: only a user whose stored choice differs from the OS
  may see the page background in the OS's theme for the moment before the module script runs (the
  root is empty until then). Accepted, and checked by eye in the UX pass (section 4).
- **Storage may throw.** Every `getItem`, `setItem` and `removeItem` is in `try`/`catch`: a refused
  read counts as nothing stored; a refused write still flips the attribute for the page's life.
- **No `matchMedia` counts as light.** jsdom has none (read:
  [Jest, "Mocking methods which are not implemented in JSDOM"](https://jestjs.io/docs/manual-mocks#mocking-methods-which-are-not-implemented-in-jsdom));
  every browser the UI targets has it. So the existing tests render as today with no stub, and the
  theme tests stub it.
- **Forced colours: keep the button.** The theme changes nothing visible there (colours are forced),
  but hiding it would change the Tab order with a system setting, and the choice applies again when
  forced colours go off. Its pressed state stays visible: `Highlight` fill (criterion H4.10).
- **The pressed look** is the mockup's: `--accent-soft` fill, `--accent` text and border (brief 04 1.1
  "pressed theme switch"), calmer than the Show toggle's full accent fill for a button that stays
  pressed all day for a dark-theme user (a preference). Text pair P15 6.04 / 6.93; border `--accent`
  on the banner P13 6.46 / 8.04 (on the page after H5, P11).
- **One icon now.** The button shows Lucide's `moon` before its text, inline (`aria-hidden`,
  `focusable="false"`), so H5 can show it icon-only below 480 px. H8 later moves the path into
  `Icon.tsx`; Lucide's licence text is already in `installer/licenses/lucide.txt`.

## 1. The dark block (CSS)

CSS cannot give one declaration block both a media-conditioned and an unconditioned selector, so the
16 dark tokens are written **twice**, identical, and a test keeps them identical:

```
:root { ...the light tokens...; color-scheme: light dark; }           /* unchanged */

@media (prefers-color-scheme: dark) {
  :root:not([data-theme='light']) { ...the 16 dark tokens... }        /* block A: system dark */
}

:root[data-theme='dark'] { ...the same 16 dark tokens...; color-scheme: dark; }   /* block B */
:root[data-theme='light'] { color-scheme: light; }
```

- No attribute: the OS decides (block A in dark, `:root` in light); `color-scheme: light dark`
  resolves native controls and scrollbars to the OS's scheme.
- `data-theme="dark"`: block B, whatever the OS; `color-scheme: dark` for native controls.
- `data-theme="light"` on a dark OS: block A is excluded by its `:not()`, `:root` gives the light
  tokens, `color-scheme: light` keeps native controls light
  ([CSS Color Adjustment 1, `color-scheme`](https://www.w3.org/TR/css-color-adjust-1/#color-scheme-prop)).
- Specificity: A and B are (0,2,0), the light `:root` (0,1,0); A and B never disagree.
- Not chosen: `light-dark()` per token, which would declare each once, takes two `<color>` values
  only (read: [CSS Color 5, `light-dark()`](https://www.w3.org/TR/css-color-5/#light-dark)) and
  `--shadow` is a shadow list, and it would rewrite every H1a token test; an attribute always set by
  script (one dark block) would flash for every dark-OS user before the script runs.

## 2. Strings

| Key | English |
|---|---|
| `theme-dark` | Dark theme |

No other key: the pressed state is announced by the screen reader from `aria-pressed` ("Dark theme,
toggle button, pressed"), no live region.

## 3. Criteria

**H4.1 The button.** The banner holds one `button` `type="button"`, class `secondary theme-switch`,
accessible name exactly `theme-dark`'s text, children in this order: an `svg` (`aria-hidden="true"`,
`focusable="false"`, `viewBox="0 0 24 24"`, `fill="none"`, `stroke="currentColor"`,
`stroke-width="2"`, `stroke-linecap="round"`, `stroke-linejoin="round"`, one `path` with Lucide's
`moon` `d`: `M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401`),
then a `span` with class `theme-switch-label` holding the text. It renders signed out, signed in and
while the session is read.

**H4.2 Place and focus order.** In DOM order: the home link, the Menu button, the theme button, then
the session part (Sign in link, or the user and Sign out). Tab reaches them in that order
([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)); the skip link stays the first focusable
element (1a.1).

**H4.3 Initial state.** Nothing stored: `aria-pressed` is `"true"` when the stubbed
`matchMedia('(prefers-color-scheme: dark)').matches` is true, `"false"` otherwise and when
`matchMedia` is absent; `<html>` has no `data-theme`. Stored `dark` or `light`: `aria-pressed`
`"true"` / `"false"` whatever the system, and `<html data-theme>` equals the stored value. A stored
value other than `light` or `dark` counts as nothing stored (attribute absent).

**H4.4 Before the first render.** The module's start function, called with storage holding `dark`,
sets `data-theme="dark"` on `document.documentElement` synchronously; `fe/src/main.tsx` imports it
and calls it before `createRoot(` (pinned on the file's text).

**H4.5 Press, away from the system.** System light, nothing stored: one click makes `aria-pressed`
`"true"`, `data-theme="dark"`, `localStorage['pai-theme'] === 'dark'`. Mirror case: system dark, one
click gives `"false"`, `data-theme="light"`, stored `light`.

**H4.6 Press, back to the system.** System light, stored `dark`: one click makes `aria-pressed`
`"false"`, removes `data-theme` and removes the `pai-theme` entry. Mirror: system dark, stored
`light`: `"true"`, attribute and entry removed. Two clicks from nothing stored leave nothing stored.

**H4.7 Focus and announcement.** After a click (or Enter, or Space: native `button`) the focus is
still on the button; the name is unchanged; no element with `role="status"`, `role="alert"` or
`aria-live` appears or changes because of the press; no route, title or focus move.

**H4.8 The system changes while open.** Nothing stored: firing the stubbed media query's `change`
with `matches: true` makes `aria-pressed` `"true"` and leaves `data-theme` absent; with a choice
stored, a `change` alters neither. The listener is removed when the app unmounts.

**H4.9 Storage refused.** With `Storage.prototype.getItem`, `setItem` and `removeItem` throwing: the
app renders, the button starts from the system, a click still flips `aria-pressed` and
`data-theme`, and no error escapes (nothing thrown, no error boundary).

**H4.10 CSS: the dark block.** `index.css`: inside `@media (prefers-color-scheme: dark)` exactly one
rule, selector exactly `:root:not([data-theme='light'])`, declaring exactly the 16 dark tokens of
brief 06 H1a.2 and nothing else; a top-level rule `:root[data-theme='dark']` declaring the same 16
custom properties with the same values in the same order plus `color-scheme: dark`, nothing else; a
top-level rule `:root[data-theme='light']` declaring only `color-scheme: light`; the top-level
`:root` keeps `color-scheme: light dark`.

**H4.11 CSS: the pressed look.** A top-level rule `.theme-switch[aria-pressed='true']` after
`.secondary[aria-pressed='true']` with `background: var(--accent-soft)`, `color: var(--accent)`,
`border-color: var(--accent)`. Inside an `@media (forced-colors: active)` block a rule
`.theme-switch[aria-pressed='true']` with `forced-color-adjust: none`, `background: Highlight`,
`color: HighlightText`, `border-color: Highlight`. A `.theme-switch svg` rule sizes the icon
`width: 1.25rem; height: 1.25rem; flex: none`, and `.theme-switch` lays out icon and text with
`display: inline-flex; align-items: center; gap: var(--space-2)`.

**H4.12 Nothing else moves.** Existing tests pass with only the edits of section 5; no other key;
the H1a contrast pairs unchanged.

## 4. What only the designer's UX pass sees

`npm --prefix fe run build`, `npm --prefix fe/ux run check` (it emulates the OS scheme; nothing
stored, so it checks block A and the light `:root`), then by hand in the three engines at 1280 and 320
px: press the button on a light OS and on a dark OS (`page.emulateMedia({ colorScheme })`) and read
the computed `--page` and `color-scheme` of `<html>` (block B and the light override); axe-core on
both forced states; reload: the choice holds, and a stored choice against the OS shows no visible
flash (throttled CPU, a screenshot at first paint); the native `select`, checkbox and scrollbars
match the theme in effect; forced colours: the pressed button in `Highlight`; the ring on the pressed
button (`--accent` around `--accent-soft`) whole; at 320 px the banner wraps with no sideways scroll
(the icon-only look is H5).

Measured in the review (2026-11-18, stub, local): with a stored choice against the OS, Chromium
paints one frame of the empty page in the OS's theme before `main.tsx` runs (first paint 12 ms,
attribute at 18 ms); Firefox and WebKit show at most one animation frame before the attribute; with
the script delayed 300 ms, Chromium and Firefox paint the OS's background for those 300 ms, WebKit
waits for the script. Accepted as section 0 says; if it is ever not, the fix is a small external
classic script in `<head>` (render-blocking, no CSP hash), not an inline one.

### 4a. What `fe/ux/check.js` gains (a later change to the check, not a criterion of this cycle)

The check today emulates the OS scheme with nothing stored, so it never reads block B or the light
override. Add a stored-choice axis:

- `CHOICES = [null, 'opposite']`. For `'opposite'`, before `ctx.newPage()`, call
  `ctx.addInitScript((v) => { try { localStorage.setItem('pai-theme', v) } catch {} }, other)`,
  `other` being `'dark'` when the context's `colorScheme` is `'light'` and the reverse. Run it only
  for the states `overview`, `instance-server` and `model-profile` (3 states x 2 widths x 2 schemes =
  12 more per browser); the summary line counts them.
- For each such state, besides axe and overflow: `document.documentElement.dataset.theme === other`,
  `.theme-switch` has `aria-pressed` `String(other === 'dark')`, and
  `getComputedStyle(document.body).backgroundColor` equals the body background of the opposite OS
  scheme with nothing stored (`bodyBackground(browser, other)`, already computed). A mismatch is a
  failing line `<browser> <width>px <scheme>+<other> <state>: ...`.
- Nothing else changes: the stub, the widths, the tags.

## 5. For the tester

- The theme logic in its own module (`fe/src/theme.ts`, a suggestion) with the button in
  `fe/src/ThemeSwitch.tsx`; tests in a new `fe/src/ThemeSwitch.test.tsx` (component) and
  `fe/src/HearthH4.test.ts` (stylesheet and `main.tsx` text).
- Stub `window.matchMedia` per test with an object whose `matches` is settable and whose
  `addEventListener('change', fn)` keeps `fn` so the test can fire it; restore it after. In
  `afterEach`: `localStorage.clear()` and `document.documentElement.removeAttribute('data-theme')`
  (jsdom keeps both across tests in a file).
- `fe/src/HearthTokens.test.ts`: its `darkBody()` locator and the `blocks` set of H1a.4 look for
  `:root:not([data-theme='light'])` (compact form, no spaces) inside the media block instead of
  `:root`; H1a.4's exemption set adds the top-level `:root[data-theme='dark']` (it holds hex values);
  its comment "H4 later rewrites the locator" goes. Every other assertion of that file holds. The
  existing test that `color-scheme: light dark` stays on `:root` holds.
- Brief 07 H3.7 reads the dark `--accent`: if H3 lands first, its locator moves the same way.

## 6. What must not change

Every other id, class, role, name, text and key; the Menu button's and the session part's behaviour;
focus moves on route changes; the token values (H1a); the 320 px rule
([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)); targets at least 24 by 24 px (the button is 40 px
high, [2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)); no motion (no transition on the
theme change); no server state, cookie or request for the theme.
