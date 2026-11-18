# 08 H7: the sign-in page's illustration

Direction B's sign-in (brief `fe/briefs/04-visual-identity.md` 1.8 and section 2 row H7): a
decorative drawing beside the form, the form in a lifted panel. One test-first cycle, 10 criteria.
Only the **page** at `#/sign-in` and in place of a protected page changes; the sign-in **dialog**
(`SignInDialog`) does not. Nothing about the form's behaviour, texts, errors or focus changes.

Order: after H2b (done). It edits `SignIn.tsx` and the sign-in rules of `index.css`; H8b also edits
`SignIn.tsx` (the notices' icons), so H7 lands before H8b. It can run beside H6 and H9a.

Labels: **measured**, **read** (with its URL), **assumed**, **preference**.

## 0. Decisions

- **The drawing is round 1's `ART_B`** (`build/ux-review/visual/src/gen.py`, git-ignored, so it is
  copied in full in H7.3): an arched doorway (the mark's shape, large), a small server panel inside
  it with three bars, a dotted cable to a laptop, a plant, a sun. **No text in it**, not even a
  glyph. The node's name over the `h1` waits for 05e (`GET /api/node` is planned, not served).
- **Decorative, hidden from assistive technology**: the wrapper `div` is `aria-hidden="true"` and the
  `svg` `focusable="false"`; no `title`, no `desc`, no `role="img"`
  ([1.1.1](https://www.w3.org/TR/WCAG22/#non-text-content): pure decoration is implemented so
  assistive technology can ignore it). As decoration it has no contrast requirement
  ([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast) covers graphics "required to understand
  the content").
- **Colours by class, not by `var()` in presentation attributes.** Each shape takes a class
  (`art-sun`, `art-ground`, `art-door`, ...) and the CSS gives its `fill` and `stroke` from tokens.
  Whether every engine resolves `fill="var(--x)"` written as an SVG attribute is **unverified** here;
  a CSS property certainly does. Geometry (`d`, `x`, `rx`, `stroke-width`) stays attributes.
- **Three new tokens**, round 1's (`build/ux-review/visual/src/b.css`), in all three theme blocks
  (the light `:root`, the dark `@media` block, the `:root[data-theme='dark']` block):

  | Token | Light | Dark |
  |---|---|---|
  | `--art-1` | `#ead9f0` | `#3a2a44` |
  | `--art-2` | `#f3e3cf` | `#3b2f24` |
  | `--art-3` | `#ffffff` | `#2b241f` |

- **Hidden at narrow widths and in forced colours** (`display: none`). Below 480 px it would push the
  form below the first screen of a 320 by 568 phone (intro, notices and two fields already fill it:
  assumed from the H5 measurements); in forced colours every fill becomes a system colour and the
  drawing turns into overlapping outlines (a preference: hide rather than redraw).
- **Two columns by room, not by a breakpoint**: `repeat(auto-fit, minmax(min(100%, 18rem), 1fr))`,
  so the columns form whenever `main` holds two 18rem columns and the gap (the nav takes 224 px from
  960 px up, so a viewport breakpoint would misjudge the room), and stack, art first, otherwise.

## 1. Criteria

**H7.1 DOM, page only.** When `SignIn` renders as a page (`inDialog` false), it renders one
`div.signin-page` holding, in order: `div.signin-art` (`aria-hidden="true"`), then
`div.signin-panel` holding everything it renders today, in today's order (the `h1`, the intro, the
notices, the form). In the dialog (`inDialog` true) it renders exactly what it renders today: no
`signin-page`, no `signin-art`, no `signin-panel`.

**H7.2 The `svg`.** `div.signin-art` holds exactly one element, an `svg` with
`viewBox="0 0 420 260"`, `focusable="false"`, `aria-hidden="true"`, and no `width` or `height`
attribute. It contains no `text`, `title`, `desc`, `foreignObject` or `a` element, and its
`textContent` is empty.

**H7.3 The drawing** (children of the `svg`, in this order; classes as given):

```
<circle class="art-sun" cx="330" cy="70" r="38"/>
<path class="art-ground" d="M0 214h420v46H0z"/>
<path class="art-door" d="M120 214V112a90 90 0 0 1 180 0v102z" stroke-width="3"/>
<rect class="art-panel" x="160" y="120" width="100" height="80" rx="12" stroke-width="3"/>
<rect class="art-bar-on" x="174" y="136" width="72" height="12" rx="6"/>
<rect class="art-bar" x="174" y="156" width="72" height="12" rx="6" stroke-width="2.5"/>
<rect class="art-bar" x="174" y="176" width="72" height="12" rx="6" stroke-width="2.5"/>
<circle class="art-knob" cx="236" cy="142" r="3"/>
<path class="art-cable" d="M260 196c40 0 40 18 80 18" stroke-width="3" stroke-linecap="round" stroke-dasharray="1 8"/>
<rect class="art-panel" x="320" y="180" width="64" height="34" rx="6" stroke-width="3"/>
<path class="art-line" d="M312 214h80" stroke-width="3" stroke-linecap="round"/>
<path class="art-stem" d="M60 214c0-30 10-46 26-58M86 214c0-22-6-38-20-50" stroke-width="3" stroke-linecap="round"/>
<ellipse class="art-leaf" cx="86" cy="152" rx="12" ry="6" transform="rotate(-35 86 152)"/>
<ellipse class="art-leaf" cx="64" cy="162" rx="11" ry="5" transform="rotate(30 64 162)"/>
```

(JSX spells the attributes `strokeWidth`, `strokeLinecap`, `strokeDasharray`, `className`.)

**H7.4 The colours** (`index.css`, rules scoped under `.signin-art`):
`.art-sun, .art-ground { fill: var(--art-1) }`; `.art-door { fill: var(--art-3); stroke: var(--text) }`;
`.art-panel { fill: var(--bg); stroke: var(--text) }`; `.art-bar-on { fill: var(--accent) }`;
`.art-bar { fill: none; stroke: var(--text) }`; `.art-knob { fill: var(--on-accent) }`;
`.art-cable, .art-stem { fill: none }`; `.art-cable, .art-line { stroke: var(--text) }`;
`.art-stem { stroke: var(--success) }`; `.art-leaf { fill: var(--success) }`.

**H7.5 Tokens.** `--art-1`, `--art-2`, `--art-3` with the values of section 0, each declared once in
each of the three theme blocks. They are not added to the contrast-pair tests (decoration).

**H7.6 Layout.** `.signin-page`: `display: grid`,
`grid-template-columns: repeat(auto-fit, minmax(min(100%, 18rem), 1fr))`, `gap: var(--space-6)`,
`align-items: center`. `.signin-art`: `background: var(--art-2)`, `border-radius: 24px`,
`padding: var(--space-5)`, `display: grid`, `place-items: center`. `.signin-art svg`: `width: 100%`,
`max-width: 420px`, `height: auto`. `.signin-panel`: `background: var(--bg)`,
`border: 1px solid transparent`, `border-radius: var(--radius-card)`, `box-shadow: var(--shadow)`,
`padding: var(--space-5)`, `min-width: 0`. The `.signin` form rule (`max-width: 24rem`) is unchanged.

**H7.7 Hidden when it does not help.** `@media (max-width: 479.98px)` holds `.signin-art { display: none }`;
an `@media (forced-colors: active)` block holds `.signin-art { display: none }` (either an existing
forced-colours block or a new one).

**H7.8 Reflow and zoom.** No sideways scroll at 320 px and at 400 % zoom of 1280 (the same reflow
case) ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)); the panel has no fixed height
([1.4.12](https://www.w3.org/TR/WCAG22/#text-spacing)).

**H7.9 Focus and reading unchanged.** The page's `h1` takes the focus on arrival as today; Tab order
is today's (skip link, banner, nav, user name, password, Sign in); a screen reader
meets nothing before the `h1` inside `main` (the art is hidden)
([1.3.2](https://www.w3.org/TR/WCAG22/#meaningful-sequence), [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)).

**H7.10 Nothing else moves.** No key added or changed; the insecure notice and its
`aria-describedby`, the revoked alert, the errors and their focus rules (`SignIn.test.tsx`,
`InsecureSignIn.test.tsx`, `SessionRevoked.test.tsx` hold as they are).

**What the tester pins**: H7.1 and H7.2 on the rendered `App` at `#/sign-in` and on a protected page
signed out (`#/profiles/model`), and in the dialog of a save bar's Sign in (no `.signin-art`); H7.3
as the list of child tag names and classes in order, and the `viewBox`; H7.9's reading check
(every element before the `h1` in `main` either has empty `textContent` or lies inside the
`aria-hidden` art); H7.4 to H7.7
as stylesheet text (rules by selector, media blocks by matched braces, as `HearthH2a.test.ts`; the
tokens in the three blocks, as `HearthTokens.test.ts`).

## 2. What only the designer's UX pass sees

The existing `sign-in` state, both themes: at 1280 the art and the panel side by side, the art no
taller than the panel; at 320 no art; in dark the drawing's lines `--text` on `--art-3` readable as a
picture; forced colours (Firefox pref): no art. No new UX check state is needed.

## 3. What must not change

`SignIn`'s props and behaviour; the dialog; every text and key; ids; focus; the banner's Sign in link
and its current look.
