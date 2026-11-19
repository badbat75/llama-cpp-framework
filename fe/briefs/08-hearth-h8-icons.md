# 08 H8: the Lucide icons

Direction B's icons (brief `fe/briefs/04-visual-identity.md` section 3 "Icons" and section 2 row H8)
on what the web UI has **today**. Placements of elements that do not exist yet (the node line, Server
profiles, Port, Tags, New, Delete: 05c, 05e, 05f) are not in H8: the slice that builds the element
adds its icon to the same component. Two test-first cycles:

| Cycle | What | Files | Criteria |
|---|---|---|---|
| **H8a** | the `Icon` component, the shell's icons, the moon moved into it | `Icon.tsx` (new), `App.tsx`, `ThemeSwitch.tsx`, `index.css` | 9 |
| **H8b** | the pages' icons, the field error's drawn icon replaced, the every-`svg` sweep | `InstancePage.tsx`, `ModelProfilePage.tsx`, `ModelProfiles.tsx`, `SignIn.tsx`, `Field.tsx`, `index.css` | 9 |

Order: H8a, then H8b. H8b after H7 (both edit `SignIn.tsx`). H10 and H11a use `Icon` for their
chevron, so they come after H8a. H8a edits the banner and nav of `App.tsx`; H6b edits only `Page`'s
call of `Overview` there: different regions, but run them one after the other.

Labels: **measured**, **read** (with its URL), **assumed**, **preference**.

## 0. Decisions

- **One component, `fe/src/Icon.tsx`** (brief 04 section 3), not an inline `svg` per use: 15 icons in
  16 placements would repeat the same eight attributes each time. `export function Icon({ name }: { name:
  IconName })` renders `<svg className="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"
  strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">`
  around the shapes of `name`, from a map `Record<IconName, ReactNode>`. No `width`/`height`
  attributes (CSS sizes it), no `title`, no prop for a label: **every icon is decorative**, beside
  visible text or with its text kept in the DOM.
- **Source**: the path data of `lucide-static@1.74.0` (ISC; the `@license lucide-static v1.74.0 - ISC`
  header of each file), copied as data. Its licence text is already in `installer/licenses/lucide.txt`
  (`installer/licenses/README.md`, row `lucide.txt`): **nothing new from the platform side**, same
  version.
- **Colour**: `currentColor`, no colour of its own, so it takes the text colour of its host in both
  themes and the system colour in forced colours (the UA's `preserve-parent-color` for `svg` passes
  an inherited colour through: [CSS Color Adjustment 1, 3.2](https://www.w3.org/TR/css-color-adjust-1/#forced-color-adjust-prop);
  the mark needed `color: inherit` only because it sets a colour of its own, brief 07 H3.10).
- **Size**: `1.25em` square, `flex: none`: it scales with the host's text and with the user's text
  size ([1.4.4](https://www.w3.org/TR/WCAG22/#resize-text)). The H4 rule `.theme-switch svg`
  (1.25rem) stays and wins on the theme button (specificity 0,1,1 over `.icon` 0,1,0).
- **No icon is a state cue.** Every icon sits beside text that says the same; nothing reads only
  from an icon ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color), [1.1.1](https://www.w3.org/TR/WCAG22/#non-text-content)).
  So no accessible name changes ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)), and
  [2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name) holds: names are the visible text.
- **No glyph characters are replaced**: the strings and components hold no arrow, check or warning
  character today (searched `fe/src` for U+2190, U+2192, U+2713, U+2714, U+00D7, U+26A0 and the
  triangles). The one drawn icon replaced is the field error's own `svg` (H8b.6), which also lacks
  `focusable="false"`.
- **Not in H8** (a preference: they are live regions whose text changes, and the colour and weight
  already set them apart): the save bar's alert and Saved status, the run status and alert, the boot
  and switch alerts. Brief 04's `circle-check` and the alert use of `circle-alert` are dropped; the
  start at boot field's `power` waits for a placement that needs it.

## 1. The list

| Icon (`IconName`) | Lucide id | Where | Host | Text |
|---|---|---|---|---|
| `house` | `house` | nav: Overview link | `.nav a` | visible `nav-overview` |
| `layers` | `layers` | nav: Model profiles link | `.nav a` | visible `nav-model-profiles` |
| `menu` | `menu` | banner: Menu button | `#menu-button` | visible `nav-menu` |
| `moon` | `moon` | banner: theme button (moved from `ThemeSwitch.tsx`'s inline `svg`) | `.theme-switch` | `theme-dark`, visually hidden below 480 px |
| `log-in` | `log-in` | banner: Sign in link | `a.session` | visible `session-sign-in` |
| `log-out` | `log-out` | banner: Sign out button | `.session button` | visible `session-sign-out` |
| `play` | `play` | instance: Start | `.run-actions button` | visible `run-start` |
| `square` | `square` | instance: Stop | `.run-actions button` | visible `run-stop` |
| `rotate-cw` | `rotate-cw` | instance: Restart | `.run-actions button` | visible `run-restart` |
| `chevron-left` | `chevron-left` | model profile: back link | `.model-profile-back a` | visible `model-profile-back` |
| `search` | `search` | model profiles: filter field | `.model-profiles-filter-row` | the field's label `model-profiles-filter-label` |
| `info` | `info` | sign-in: `signin-required` notice | `p.notice` | the notice |
| `triangle-alert` | `triangle-alert` | sign-in: `session-revoked`; model profile: `model-profile-engine-unavailable` | `p.notice` | the notice |
| `lock-open` | `lock-open` | sign-in: `signin-insecure` notice | `p.notice` | the notice |
| `circle-alert` | `circle-alert` | every field error (`FieldError`) | `p.field-error` | the error |

Shapes (children of the `svg`, exactly as in `lucide-static@1.74.0`):

```
house          <path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"/><path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>
layers         <path d="M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z"/><path d="M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12"/><path d="M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17"/>
menu           <path d="M4 5h16"/><path d="M4 12h16"/><path d="M4 19h16"/>
moon           <path d="M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401"/>
log-in         <path d="m10 17 5-5-5-5"/><path d="M15 12H3"/><path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/>
log-out        <path d="m16 17 5-5-5-5"/><path d="M21 12H9"/><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/>
play           <path d="M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z"/>
square         <rect width="18" height="18" x="3" y="3" rx="2"/>
rotate-cw      <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/>
chevron-left   <path d="m15 18-6-6 6-6"/>
chevron-down   <path d="m6 9 6 6 6-6"/>                (for H10 and H11a; in the map from H8a)
search         <path d="m21 21-4.34-4.34"/><circle cx="11" cy="11" r="8"/>
info           <circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>
triangle-alert <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/>
lock-open      <rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 9.9-1"/>
circle-alert   <circle cx="12" cy="12" r="10"/><line x1="12" x2="12" y1="8" y2="12"/><line x1="12" x2="12.01" y1="16" y2="16"/>
```

The icon is the host's **first** child in every placement except `search` (H8b.4).

## 2. H8a: the component and the shell

**H8a.1 `Icon.tsx`** as in section 0; `IconName` is the union of the 16 names of section 1's shapes
block (H8b's included, so H8b adds placements only).

**H8a.2 Nav.** The Overview link holds `Icon house` then its text; the Model profiles link `Icon
layers` then its text. Instance links get no icon (their ids are data). Names unchanged.

**H8a.3 Banner.** The Menu button holds `Icon menu` then `nav-menu`; the Sign in link `Icon log-in`
then `session-sign-in`; the Sign out button `Icon log-out` then `session-sign-out`. The theme button
renders `Icon moon` in place of its inline `svg`, then its `span.theme-switch-label` (the `MOON`
constant goes from `ThemeSwitch.tsx`). Names, ids, roles, `aria-pressed`, `aria-expanded`, Tab order
unchanged (H4.2, H5a.1).

**H8a.4 `.icon`** (top-level rule): `width: 1.25em`, `height: 1.25em`, `flex: none`.

**H8a.5 Hosts lay out icon and text.** A rule whose selector list is `.menu-button`,
`.session button` declares `display: inline-flex`, `align-items: center`, `gap: var(--space-2)`,
written before the `@media (min-width: 960px)` block whose `.menu-button { display: none }` hides
the Menu on a wide screen (H5a): same specificity, so the later rule wins and the Menu stays hidden
at 960 px and above. Never `#menu-button` here: an id (1,0,0) outweighs that hide rule (0,1,0) and
shows the Menu at every width (correction after the H8a review: the first text named `#menu-button`);
`a.session` (already `inline-flex`, H5a.4) and `.nav a` (already `flex`, H5b.2) add
`gap: var(--space-2)`. A nav entry that holds an icon breaks its label between words only:
`.nav a:has(> .icon) { flex-wrap: wrap; overflow-wrap: break-word }`, so when the label does not fit
beside the icon (200 % text in the 224 px column) it moves whole under the icon instead of breaking
mid-word; instance links (no icon) keep `overflow-wrap: anywhere`, which their long ids need (a flex
item's automatic minimum width is its min-content width, which `break-word` would make the whole
id: [CSS Text 3, overflow-wrap](https://www.w3.org/TR/css-text-3/#overflow-wrap-property)) (added
after the H8a review: measured "Overvi / ew" and "Mode / l profil / es" at 200 %). The H5a narrow block is unchanged (the Menu keeps its text at every width:
a preference, as H5 decided).

**H8a.6 Forced colours.** No `.icon` rule sets a colour, in or out of a forced-colours block (the
icon follows its host: section 0).

**H8a.7 Targets.** The Menu button and Sign out stay 40 px high (`--control-h`), the Sign in link and
nav pills at least 32 px ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).

**H8a.8 Every `svg` of the shell** (the banner and the nav, signed in and signed out) has
`aria-hidden="true"` and `focusable="false"`.

**H8a.9 Nothing else moves.** No key; the mark; the H4 and H5 pins.

**What the tester pins (H8a)**: H8a.1 by rendering `Icon` for each name (one `svg.icon`, the eight
attributes, the child tag names and `d` values of section 1); H8a.2, H8a.3 and H8a.8 on the rendered
`App` signed out and signed in (icon first child, the right shape by its first child's `d` or tag,
names by role unchanged); H8a.4 to H8a.6 as stylesheet text. `HearthTheme.test.tsx` checks on
the moon's `path` hold (same `d`).

## 3. H8b: the pages

**H8b.1 Run verbs.** Each `.run-actions` button holds `Icon play` / `square` / `rotate-cw` (Start,
Stop, Restart) then `label(a)`. `aria-disabled` buttons keep the icon. Names unchanged
(`RunControls.test.tsx` finds them by name).

**H8b.2 Back link.** `.model-profile-back a` holds `Icon chevron-left` then `model-profile-back`.

**H8b.3 Notices.** `p.notice` of `signin-required` holds `Icon info` first; of `session-revoked`
`Icon triangle-alert`; of `signin-insecure` `Icon lock-open`; of `model-profile-engine-unavailable`
`Icon triangle-alert`. Their `textContent` is unchanged (an `svg` with no text adds none), so the
tests that read it exactly hold; `role="alert"` on the revoked one stays.

**H8b.4 Filter.** `.model-profiles-filter-row` holds `Icon search` as its first child, before the
`input`; CSS places it inside the field: `.model-profiles-filter-row { position: relative }`,
`.model-profiles-filter-row > .icon { position: absolute; left: var(--space-3); pointer-events: none;
color: var(--muted) }` and the input `padding-left: calc(var(--space-3) + 1.25em + var(--space-2))`.
The `--muted` colour is set, so a forced-colours block gives it `color: inherit` (as `.mark`). The
field's label, hint, Escape and Clear unchanged.

**H8b.5 Layout.** `.run-actions button`, `.model-profile-back a`: `display: inline-flex`,
`align-items: center`, `gap: var(--space-2)`. `.notice`: `display: flex`, `align-items: flex-start`,
`gap: var(--space-2)`; `.notice > .icon`: `margin-top: calc((1lh - 1.25em) / 2)` (centred on the
first line).

**H8b.6 Field errors.** `FieldError` renders `Icon circle-alert` in place of its own drawn `svg`
(the `error-icon` circle and two rects), then the text; the `.error-icon` rule goes, and
`.field-error > .icon` takes its spacing (`margin-right: var(--space-1)`, `vertical-align: -0.25em`).
The error's text, id and `aria-describedby` link unchanged.

**H8b.7 The sweep.** On every page (Overview, sign-in, instance page, model profiles, a model
profile with a field error showing), every `svg` in the document has `aria-hidden="true"` and
`focusable="false"`.

**H8b.8 No icon alone.** Every element that holds an `.icon` also holds non-empty text (visible or
visually hidden), except the filter row, whose field is named by its label.

**H8b.9 Nothing else moves.** No key; focus moves; live regions.

**What the tester pins (H8b)**: H8b.1 to H8b.4 and H8b.6 on the rendered pages (icon first child by
its first `d`, names and `textContent` unchanged); H8b.7 and H8b.8 as one sweep test over the five
pages; H8b.4 and H8b.5 CSS as stylesheet text. `ModelProfileFlags.test.tsx` line 1243 looks for
`svg.error-icon`: it looks for `svg.icon` instead (still `aria-hidden="true"`, the field error still
in `--danger`).

## 4. What only the designer's UX pass sees

Both themes, 1280 and 320: icons aligned with their text (no baseline jump), the search icon inside
the field and not under typed text, the icon-only theme button still centred below 480 px, the
notices' icon on the first line; forced colours: icons take the text's system colour, the search
icon visible. No new UX check state.

## 5. What must not change

Every accessible name, role, id, key and text; focus order and focus moves; the mark; the favicon;
the H4 theme rules; the field error's announcement.

## 6. Review of H8a (2026-11-19)

Built from the working tree (`Icon.tsx`, `App.tsx`, `ThemeSwitch.tsx`, `index.css`), checked in
Chromium, Firefox and WebKit at 1280, 480 and 320 px, light and dark, signed out and signed in with a
32-character user name (`build/ux-review/h8a.js`, stub `stub8.py`). UX check: 0 failing, 48 states
per browser. **Rejected** for two defects, both corrected in H8a.5 above:

1. **Not met, H8a.5 / H8a.9: the Menu shows at 960 px and above.** `#menu-button` (1,0,0) in
   `index.css` (the rule after `a.session`) outweighs `@media (min-width: 960px) { .menu-button {
   display: none } }`: measured a 104 by 40 Menu in the banner at 1280 in all three engines, so the
   wide layout has a Menu that toggles a nav already shown. The fault is the brief's: its first text
   named `#menu-button`. Fix: the selector `.menu-button`; the stylesheet test that pins the selector
   list follows.
2. **Not met, H8a.5 (added): mid-word breaks at 200 % text.** In the 224 px column the icon (42.5 px
   plus the gap) leaves the label less than "Overview" needs, and `overflow-wrap: anywhere` breaks it:
   Overview 113 px high (61 at HEAD), Model profiles 219 (113 at HEAD); at 320 px Model profiles
   wraps at the space (113, fine). Fix: the `:has(> .icon)` rule of H8a.5.

Met: H8a.1 to H8a.4, H8a.6 to H8a.8. The five shell glyphs and the moon read as their meaning
(menu, house, layers, log-in, log-out, moon; zoomed shots at 4x). `.icon` is 1.25em of the 17 px body
text, 21.3 px (the moon 20, held by `.theme-switch svg`), 42.5 at 200 % text; centre 1.3 (Chromium)
to 1.5 px (Firefox, WebKit) above the text's x-height centre, gap 8 px; stroke equals the host's
colour at rest, hover, on the current entry and on the pressed theme button, in both themes (no
disabled control in the shell). Heights: Menu, theme, Sign out 40; Sign in and pills 34. Banner
heights equal those with the icons removed at every width (72 / 112 / 112 signed out, 72 / 194 / 112
signed in at 1280 / 480 / 320), no new wrap, no sideways scroll, nothing clipped at 200 % text and with
1.4.12 spacing. Accessible names: the banner and nav aria snapshots are identical with and without
the icons. Contrast: the icon needs none of its own where the same words are text beside it (1.4.11
covers graphics "required to understand the content", [WCAG 2.2, 1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast));
measured anyway on the current entry: 6.04 (light), 12.79 (dark). Forced colours (Chromium emulation,
Firefox pref): every icon visible in its host's system colour, `HighlightText` on the current entry and
the pressed theme button. A click on the Menu's `svg` lands on the button (`elementFromPoint` is its
`path`) and toggles it; Enter and Space toggle; the focus ring encloses the icon. Theme button: the
moon pixel-identical with and without `class="icon"`, box 148 by 40 (40 by 40 below 480). axe: 0 in
every state.

**Re-review after the fix (2026-11-19): Approved.** `index.css` only: the row rule is `.menu-button,
.session button`, before the wide hide; `.nav a:has(> .icon)` added (`build/ux-review/h8b.js`, stub
`stub8.py`, a second instance with a 79-character id). UX check: 0 failing, 48 states per browser.
Defect 1 met: in Chromium, Firefox and WebKit the Menu is not rendered at 1280 and 960 px and is at
959, 480 and 320 (icon and text in a row, gap 8, 40 high); click, Enter and Space toggle it. Defect 2
met: the rule applies in all three engines (computed `overflow-wrap: break-word`, `flex-wrap: wrap`
on Overview and Model profiles; `anywhere` on instance links). At 200 % text (Chromium, Firefox) in
the 224 px column Overview is 111 px, its label whole under the icon (61 at HEAD, 113 broken), Model
profiles 164, under the icon and broken at the space only (113 at HEAD, 219 broken); in the open menu
at 320 Overview stays beside its icon (61) and Model profiles moves under it on one line (111). No
break inside a word, nothing clipped, no sideways scroll; the long instance id still breaks anywhere
and stays inside its pill. Unchanged: banners 72 / 112 / 112 signed out and 72 / 194 / 112 signed in,
Sign out 124 by 40 (125 in Firefox, WebKit), axe 0 at 1280, 480, 320 in both themes and all three
engines.
