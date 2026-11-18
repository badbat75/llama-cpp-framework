# 07 H3: the Hearth mark and the favicon

The arched doorway with a keyhole (brief `fe/briefs/04-visual-identity.md` 1.1 "Mark", 1.2 "Banner",
section 2 row H3) beside the product name in the banner, and the same drawing as the browser tab's
icon. One test-first cycle, 9 criteria. The first Hearth slice that changes the DOM, and only by one
decorative element: the home link keeps its accessible name, every other name, role, id and text
stays.

Labels: **measured** (computed here, with how), **read** (a source, with its URL), **preference**.

## 0. Decisions

- **One shape, one colour, `currentColor`.** Round 1's mark (`build/ux-review/visual/src/gen.py`,
  `MARK['b']`) draws the doorway in `--accent` and the keyhole in `--on-accent` as three shapes. Here
  it becomes **one path** with `fill-rule="evenodd"`: the doorway with the keyhole cut out, so the
  keyhole shows whatever is behind it (the banner's `--surface` today, the page after H5). Its fill
  is `currentColor` and the colour comes from CSS (`.mark { color: var(--accent) }`), so in forced
  colours the mark takes the system colour the link's text takes and needs no rule of its own
  (brief 04 criterion 10; [CSS Color Adjustment 1, 3.1](https://www.w3.org/TR/css-color-adjust-1/#forced-colors-properties):
  `color` is forced, and `currentColor` follows it). **Corrected after the review (2026-11-18):**
  not for an `svg`. The UA sheet gives it `forced-color-adjust: preserve-parent-color`, which keeps
  an author `color` such as `var(--accent)` and only passes the parent's colour through an inherited
  one ([CSS Color Adjustment 1, 3.2](https://www.w3.org/TR/css-color-adjust-1/#forced-color-adjust-prop)).
  Measured in Chromium (forced colours emulated): the mark stayed `#7b3a8f` / `#dcaaf0` while the
  link was LinkText; with `.mark { color: inherit }` in a forced-colours block it took the link's
  colour (`rgb(0,0,159)` light, `rgb(255,255,0)` dark). Firefox's colour pref overrides it anyway.
  Hence H3.10. The keyhole is the circle `(16, 15) r 3.6` joined
  to the trapezoid `14.4,17 17.6,17 18.8,24 13.2,24` of round 1, as one outline: the two meet at
  `(14.21, 18.12)` and `(17.79, 18.12)` (measured: the circle crossed with the trapezoid's sides,
  Python, 2026-11-18). Two overlapping sub-paths would cut the overlap back in under `evenodd`.
- **Decorative, not named.** The link's text "Private AI Framework" (`app-name`) is its name; the
  SVG is `aria-hidden="true"` and `focusable="false"`, so a screen reader says the name once
  ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value), [1.1.1](https://www.w3.org/TR/WCAG22/#non-text-content):
  a decorative image is hidden from assistive technology).
- **Banner only.** The sign-in page shows the banner, mark included; it gets no second, larger mark:
  its picture is the illustration of H7 (brief 04 1.8). A preference: one mark per screen.
- **Size in rem.** `2rem` (32 px at the default text size, brief 04 1.1), so it grows with the text
  setting next to the name ([1.4.4](https://www.w3.org/TR/WCAG22/#resize-text)).
- **The favicon stays an inline data URI in `fe/index.html`**, as today (criterion 1b.8a of brief 01),
  not a file in `fe/public/`. Reasons: the bff serves only `/` and `/assets/*` (`fe/DESIGN.md` 2.3),
  so a `/favicon.svg` from `public/` would 404; and `crates/bff/build.rs` `content_type` maps only
  `html`, `js`, `css` and `woff2` (read, lines 114 to 122), so an `.svg`, `.png` or `.ico` file
  would go out as `application/octet-stream`. With the data URI **nothing is needed from the platform
  side**. If a file icon is ever wanted, the request would be: a route for it and `svg` to
  `image/svg+xml` ([IANA](https://www.iana.org/assignments/media-types/image/svg+xml)) in that table.
- **The favicon's dark variant follows the browser, not the page.** A `<style>` with
  `@media (prefers-color-scheme: dark)` inside the SVG switches the fill from the light `--accent`
  to the dark one; the media query inside a favicon is evaluated against the browser's scheme (read:
  [web.dev, "Building an adaptive favicon"](https://web.dev/articles/building/an-adaptive-favicon),
  Chromium and Firefox). It does not follow the H4 switch: the tab strip belongs to the browser, so
  it follows the browser's scheme (a preference, and the only thing a favicon can follow without
  script). The hex values are the `--accent` of each theme, written as data in the HTML, since a
  favicon cannot read CSS custom properties.
- **No PNG or ICO fallback.** Today's icon is SVG only too. Whether Safari shows an SVG favicon is
  **unverified** here; where it does not, the tab shows the browser's default icon, as today. A
  fallback would need the file route above.
- **No `theme-color` metas.** They follow the OS through their `media` attribute and would disagree
  with a theme chosen by H4's switch; a script to keep them in step is more than the slice needs (a
  preference).

## 1. The drawing

The component `fe/src/Mark.tsx` (a suggestion for the name) renders exactly:

```
<svg class="mark" viewBox="0 0 32 32" aria-hidden="true" focusable="false">
  <path fill="currentColor" fill-rule="evenodd"
        d="M5 30V15a11 11 0 0 1 22 0v15zM17.79 18.12 18.8 24h-5.6l1.01-5.88a3.6 3.6 0 1 1 3.58 0z"/>
</svg>
```

(in JSX `className`, `fillRule`; `focusable="false"` passes through as written). No `width` or
`height` attribute: CSS sizes it. No `<title>`.

The favicon, the `href` of the one `link rel="icon" type="image/svg+xml"`, exactly:

```
data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='2 3 28 28'%3E%3Cstyle%3Epath{fill:%237b3a8f}@media (prefers-color-scheme:dark){path{fill:%23dcaaf0}}%3C/style%3E%3Cpath fill-rule='evenodd' d='M5 30V15a11 11 0 0 1 22 0v15zM17.79 18.12 18.8 24h-5.6l1.01-5.88a3.6 3.6 0 1 1 3.58 0z'/%3E%3C/svg%3E
```

That is the same path with a tighter square `viewBox` (the doorway fills the 16 px tab icon), `<`,
`>` and `#` percent-encoded as today's icon is.

## 2. Criteria

**H3.1 The mark in the home link.** In the banner, the link named `app-name` ("Private AI
Framework", `href="#/"`) holds, before its text, one `svg` with class `mark`, `viewBox="0 0 32 32"`,
`aria-hidden="true"`, `focusable="false"`, and one `path` with `fill="currentColor"`,
`fill-rule="evenodd"` and the `d` of section 1, exactly.

**H3.2 The name is unchanged.** The link's accessible name is exactly `app-name`'s English text (the
role query by name keeps finding it, as 1a.1 does), its `textContent` is exactly that text, and the
SVG has no `title`, `aria-label` or `role`.

**H3.3 One mark.** The document holds exactly one `svg.mark`, on every route (Overview signed out,
`#/sign-in`, signed in): the banner's.

**H3.4 Layout of the home link.** `index.css`: `.app-name` declares `display: inline-flex`,
`align-items: center`, `gap: var(--space-2)`; its `color`, `font-weight`, `text-decoration` and
`padding` as today.

**H3.5 The mark's box and colour.** `index.css`: a top-level `.mark` rule with `width: 2rem`,
`height: 2rem`, `flex: none` and `color: var(--accent)`; no `fill` declaration in it (the path's
`currentColor` takes the colour). Contrast: `--accent` on `--surface` (the banner until H5) 6.46 /
8.04 and on `--page` (after H5) 6.58 / 9.37, pairs P13 and P11 of brief 06, already pinned; a
decorative image needs none ([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast) excepts it),
held anyway.

**H3.6 The favicon.** `fe/index.html` still declares exactly one `link rel="icon"`
`type="image/svg+xml"` in the head after the viewport meta, and its `href` is exactly the string of
section 1. Test 1b.8a in `fe/src/App.test.tsx` changes only its `ICON_HREF` constant (and the
comment above it) to that string.

**H3.7 The favicon's colours are the accent.** The two hex values in the favicon `href` (after
decoding `%23`) are the `--accent` of the light token block and of the dark token block of
`index.css`, read from the stylesheet (so a later accent change breaks this test, not the tab).

**H3.8 No colour literal in the CSS.** H1a.4 holds: the mark adds no hex or colour function to
`index.css` (the favicon's hex values are in `index.html`, which H1a.4 does not read).

**H3.9 Nothing else moves.** No strings key added or changed; no route, focus move, title or nav
change; every other test passes unchanged.

**H3.10 The mark in forced colours (added after the review).** `index.css` holds, inside an
`@media (forced-colors: active)` block, a `.mark` rule with `color: inherit` (no other declaration),
so the mark takes the home link's system colour (section 0, corrected). H3.5's "no fill" stands;
its forced-colours sentence in section 0 is superseded by this one rule.

## 3. What the tester pins

Component tests (Testing Library, in a new `fe/src/HearthMark.test.tsx` or beside 1a.1 in
`App.test.tsx`): H3.1 to H3.3 on the rendered `App` with the existing `fakeBff()`. Stylesheet-text
tests (as `fe/src/HearthH2a.test.ts` reads `index.css`: comments dropped, the rule regex, `prop()`):
H3.4, H3.5, H3.7 (the dark block located as `HearthTokens.test.ts` locates it; H4 rewrites that
locator, so whichever lands second updates this test's locator too). H3.6 by editing 1b.8a's
constant. Keyboard: nothing new to test (an `svg` with `focusable="false"` takes no Tab stop; the
skip link is still the first focusable element, 1a.1).

## 4. What only the designer's UX pass sees

`npm --prefix fe run build`, `npm --prefix fe/ux run check` (0 axe-core violations, 0 sideways
scroll). By eye, both themes, 1280 and 320 px: the doorway reads at 32 px, the keyhole open (the
banner fill shows through), mark and name on one line, centred; forced colours in Firefox
(`browser.display.document_color_use` 2): the mark in the link's system colour; the tab icon at 16
px in a light and a dark browser theme (plum, then lilac), and in Firefox with the forced-colours
pref (a favicon is not forced: it keeps its colours). The name still wraps at 320 px with a long
user name beside it (46b's case).

Measured in the review (2026-11-18, Playwright, all three engines): at 1280 px the banner stays 65
px; at 320 px it grows from 173.7 to 221.7 px, because the link (173 to 213 px wide) no longer
leaves room for Menu beside it, so Menu takes its own row. No sideways scroll. Accepted until H5a,
which groups the tools and hides the user name below 480 px (brief 04 1.2: three rows at 320 px).
The favicon's dark fill was seen in Chromium only: Playwright's Firefox and WebKit draw the light
fill under a dark `colorScheme`, so the tab icon in a dark Firefox, and Safari's SVG favicon at all,
stay unverified.

## 5. What must not change

DOM order, every id, class, role, accessible name, text and key except the added `svg`; focus order;
the 320 px rule ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)); no motion; the forced-colours
blocks; the title; `index.html`'s viewport meta and `lang`.
