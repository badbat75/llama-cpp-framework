# 08 H10 and H11: "On this page", the summary line and the collapsed Assets card

Direction B's model profile page, DOM part (brief `fe/briefs/04-visual-identity.md` 1.6 items 2, 4
"DOM (H10)" and 6, section 2 rows H10 and H11), with the user's answer to question 4 (2026-11-18):
**Assets collapsed by default, the model file name under the title, the Engine as a row of Assets**.
Goal: the first editable control near the top at 320 px (measured in the b2 mockup: about 705 px,
against about 2900 px before H9).

| Cycle | What | Criteria |
|---|---|---|
| **H10** | the visible "On this page" label; below 1280 px the jump list collapsed behind a disclosure | 10 |
| **H11a** | the summary line under the `h1`; the Assets card collapsed behind a disclosure in its heading | 11 |
| **H11b** | the Engine field becomes the first row of Assets; its select, About toggle and hint go | 7 |

Order: H10, H11a, H11b, one after the other: all three edit `ModelProfilePage.tsx` `Content`, and
all three rewrite the model profile page's Tab-order tests (the "ten jump buttons" tests in
`ModelProfileContent`, `ModelProfilePage`, `ModelProfileCards1` to `5` and `ModelProfileSave16`).
Prerequisites: H8a (the `Icon` component and its `chevron-down`), H9a (the index look H10 collapses).
H11 does not depend on H10's behaviour, but running them in the other order only moves the test
rewrites; keep H10 first.

Labels: **measured**, **read** (with its URL), **assumed**, **preference**.

## 0. Decisions

- **Disclosures are a native `button` with `aria-expanded` and `aria-controls`**, the APG disclosure
  pattern ([WAI-ARIA APG, Disclosure](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/)): Enter
  and Space come with the `button`. **Not `details`/`summary`** for Assets: the card's `h3` must stay
  a heading (the heading list, and the jump target with its `tabindex="-1"`), and a heading inside a
  `summary` loses its role, since a `summary` is exposed as a button and a button's children are
  presentational ([WAI-ARIA 1.2, `button`](https://www.w3.org/TR/wai-aria-1.2/#button): "Children
  Presentational: True"). For the jump list, `details` could not stay always open in the 1280 px
  rail without script; one pattern for both (a preference).
- **The jump list is hidden by a class, not by `hidden`**, so the rail (H9b) shows it from 1280 px
  whatever the state: the base rule `[hidden] { display: none !important }` would win there (brief
  04 1.6, measured in the mockup). The Assets body uses `hidden`: it has no rail.
- **One key, two places.** The label `p` and the toggle carry the same text, `model-profile-jump-label`,
  whose English changes from "Model profile sections" to **"On this page"**. The `nav` takes
  `aria-labelledby` the `p`, which is `display: none` below 1280 px: a hidden element still names
  what references it by `aria-labelledby`
  ([Accessible Name 1.2, step 2A](https://www.w3.org/TR/accname-1.2/#comp_hidden_not_referenced)).
- **Assets holds no editable field**, so a refused save never needs to open it: `focusFailure` and
  the `refused` case of `save` are unchanged. A 422 naming an Assets key (`model`, `mmproj`, ...) is
  already "only told" in the bar (`fieldControl` finds no control, 2h 143); the card stays as it is.
  The nine editable cards are not collapsible in this round. **If a later slice makes an editable
  card collapsible, `focusField` must open it before focusing** (recorded here for that slice).
- **Jumping to Assets opens it**, then moves the focus to its `h3` as every other jump does (brief 04
  1.6 item 6): the user asked to go there, so the content is shown.
- **Closed on each mount** of the form (a page load, the bar's Reload, a sign-in as another user);
  a save keeps the state. No memory across pages (a preference: no storage for a view state).
- **The summary line is plain text**, the file name not in mono: `t()` returns a string, and a
  styled placeholder would need a rich-text helper the app does not have (a preference; brief 04
  asked mono).
- **Chevron after the text**, unlike H8's icons (first child): the disclosure convention, and the
  turn shows the state. No transition, so nothing to stop under `prefers-reduced-motion` (brief 04
  criterion 12 holds trivially).

## 1. H10: "On this page"

**H10.1 DOM.** `nav.jump` has `aria-labelledby="model-profile-jump-label"` and no `aria-label`. Its
children, in order: `p#model-profile-jump-label.jump-label` (text `model-profile-jump-label`), then
`button#model-profile-jump-toggle.jump-toggle.secondary` (`type="button"`,
`aria-controls="model-profile-jump-list"`, `aria-expanded` `"false"` or `"true"`, text
`model-profile-jump-label` then `Icon chevron-down`), then `ul#model-profile-jump-list` with class
`collapsed` while closed and no `class` while open, holding the ten buttons as H9a.

**H10.2 Initial state.** Closed (`aria-expanded="false"`, `ul.collapsed`) every time the form mounts.

**H10.3 Toggle.** Activating the toggle (click, Enter, Space) flips `aria-expanded` and the class;
the focus stays on the toggle; nothing is announced beyond the expanded state (no live region).

**H10.4 A jump closes it.** Activating one of the ten buttons moves the focus to the card's `h3`
and scrolls it into view as today (196), then the list is closed (`aria-expanded="false"`,
`ul.collapsed`). The dirty form asks nothing (as today).

**H10.5 Strings.** `model-profile-jump-label` becomes "On this page" in `en_US.json` and in
`src/test/bff.ts` `EN`; no key added. Tests that find the `nav` by `en('model-profile-jump-label')`
hold; the one that pins the old English (`ModelProfileSave16.test.tsx` 198) pins the new.

**H10.6 Narrow look.** `.jump-label`: `display: none`, `margin: 0 0 var(--space-1)`,
`font-weight: 700`, `font-size: var(--fs-md)`. `.jump-toggle`: `display: inline-flex`,
`align-items: center`, `gap: var(--space-2)`. `.jump-toggle[aria-expanded='true'] .icon`:
`rotate: 180deg`, no `transition`. `@media (max-width: 1279.98px)`: `.jump ul.collapsed { display: none }`.

**H10.7 Wide look.** `@media (min-width: 1280px)`: `.jump-label { display: block }`,
`.jump-toggle { display: none }`. The list shows whatever its class.

**H10.8 Order and reading.** The list follows its toggle in the DOM
([1.3.2](https://www.w3.org/TR/WCAG22/#meaningful-sequence), brief 04 criterion 11). Below 1280 px a
screen reader hears "On this page, navigation", "On this page, button, collapsed"; from 1280 px "On
this page, navigation", the label, the list.

**H10.9 Targets and ring.** The toggle is a 40 px pill (`.secondary`), with the global ring
([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum),
[2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible)).

**H10.10 Tab order** (jsdom, no CSS): back link, toggle, the ten buttons, then as today. In a
browser below 1280 px, closed: back link, toggle, then what follows the list.

**What the tester pins (H10)**: H10.1 to H10.5 and H10.10 on the rendered page (attributes, order,
class by state, focus after toggle and after a jump, the key's English); every Tab-order test of the
page gains the toggle before the ten buttons; H10.6 and H10.7 as stylesheet text (media blocks by
matched braces).

## 2. H11a: summary line and collapsed Assets

**H11a.1 Summary DOM.** When the model profile is shown, `Content` renders first (before the
`model-profile-off` line, so right after the `h1`) `p.model-profile-summary` with
`t('model-profile-summary', doc.engine, file)`, where `file` is the last non-empty segment of
`doc.values['model']` split on `/` and `\`; when the key is absent, empty, or has no non-empty
segment, `t('model-profile-summary-no-model', doc.engine)`. Not a heading, not a live region.

**H11a.2 Strings** (new): `model-profile-summary` "Engine {0}, model {1}",
`model-profile-summary-no-model` "Engine {0}, no model file".

**H11a.3 Assets DOM.** The Assets card (in place of today's `Card`) is
`section.card.assets-card` holding, in order: `h3#model-profile-card-assets` (`tabindex="-1"`, the
id `cardHeadingId('assets')` as today) whose only child is
`button#model-profile-assets-toggle` (`type="button"`, `aria-controls="model-profile-assets-body"`,
`aria-expanded`, text `model-profile-assets` then `Icon chevron-down`); then `p.intro` with
`model-profile-assets-read-only` (always shown); then `div#model-profile-assets-body`, carrying
`hidden` while closed, holding today's `dl.assets` unchanged.

**H11a.4 Initial state.** Closed (`aria-expanded="false"`, `hidden`) every time the form mounts.

**H11a.5 Toggle.** Click, Enter or Space flips `aria-expanded` and `hidden`; the focus stays on the
toggle. The body holds nothing focusable (2d.73), so closing never strands the focus.

**H11a.6 Jump to Assets.** The jump list's "Assets" button sets the card open, then focuses its `h3`
and scrolls it into view (the `jump` of 196); a card already open stays open. The other nine jumps
leave Assets as it is.

**H11a.7 Save unchanged.** A 422 naming an Assets key leaves the card as it is and is told in the bar
(`save-refused`, focus on Save changes as today, 2h 143); the client checks (`checkRows`) never name
an Assets key; `focusFailure` is unchanged.

**H11a.8 Names.** The `h3`'s name and the toggle's name are both "Assets"; the jump button "Assets"
unchanged ([2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name), [4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)).

**H11a.9 Look.** `.model-profile-summary`: `margin: 0 0 var(--space-4)`, `color: var(--muted)`,
`overflow-wrap: anywhere` (a 120-character file name wraps at 320 px). `.assets-card h3 button`:
`display: flex`, `width: 100%`, `justify-content: space-between`, `align-items: center`,
`min-height: 32px`, `padding: var(--space-1) 0`, `border: 0`, `background: transparent`,
`font: inherit`, `color: var(--text)`, `text-align: left`.
`.assets-card h3 button[aria-expanded='true'] .icon`: `rotate: 180deg`, no `transition`. The
`.card h3:focus-visible` ring rule unchanged.

**H11a.10 Reading.** "Assets, heading level 3", "Assets, button, collapsed", then the read-only note.

**H11a.11 Nothing else moves.** The Engine field (until H11b), the notice, the nine cards, the bar.

**What the tester pins (H11a)**: H11a.1 to H11a.8 on the rendered page (summary text for a path with
`/`, one with `\`, an absent `model`, an empty one, one ending in a separator; attributes and order;
the body `hidden` on load, shown after a click, Enter, Space; the jump opens and focuses the `h3`;
the 422 case); `ModelProfileContent.test.tsx` 2d.68 (the summary after the `h1`), 2d.73 (the card now
holds exactly one button, the toggle, in its `h3`, and nothing focusable in its body), 2d.73 to
2d.76 and 2d.79 open the card before reading its `dl`; the page's Tab-order tests gain the Assets
toggle after the Engine field; H11a.9 as stylesheet text.

## 3. H11b: Engine as a row of Assets

**H11b.1 DOM.** The `SelectField` with id `model-profile-engine` goes, with its About toggle and hint.
The Assets `dl` gains a first pair: `dt` `model-profile-engine` "Engine", `dd` `engineOption(doc)`
(the engine id, or "Strata (not available)" with `model-profiles-engine-not-available`), the `dd`
without the `mono` class. The other six pairs follow in today's order.

**H11b.2 The notice stays.** `model-profile-engine-unavailable` above the Assets card, unchanged
(2d.72), so a missing engine is seen with the card closed.

**H11b.3 Strings.** `model-profile-engine-hint` removed from `en_US.json` and `src/test/bff.ts` `EN`;
`model-profile-engine` kept (now the `dt`).

**H11b.4 Tab order** (jsdom): back link, jump toggle, ten jumps, Assets toggle, then the first
card's About toggle. The Engine About toggle and select are no longer in it.

**H11b.5 Tests retired.** 2d.70 and 2d.80 (the Engine select, its option, its About toggle, picking
it) are rewritten as H11b.1 and H11b.2: the `dt`/`dd` pair, the not-available text; 2e.85's "the
Engine select stays enabled" part and `ModelProfileSave16.test.tsx` 326 to 328 go; every Tab-order
test of the page drops the Engine About toggle and select (`ModelProfileCards1` to `5`,
`ModelProfileContent`, `ModelProfilePage`, `ModelProfileSave1`, `2`, `16`).

**H11b.6 Reading.** With the card open: "Engine", "llama.cpp", "Model file", the path, and so on.

**H11b.7 Nothing else moves.** The summary line, the disclosures, the save.

**What the tester pins (H11b)**: H11b.1 to H11b.4 on the rendered page; the edits of H11b.5.

## 4. UX check (`fe/ux/check.js`)

A state may carry `click: [selectors]`, clicked in order after the state's markers render and
before axe runs. New state `model-profile-open` (`session=admin`, the model profile hash, expect as
`model-profile`), `click: ['#model-profile-jump-toggle']` with H10, plus
`'#model-profile-assets-toggle'` with H11a. The existing `model-profile` state covers the closed page.

## 5. What only the designer's UX pass sees

At 320 px, both themes: the first editable control's top (target: under 900 px, the mockup's 705 plus
the real notice and the summary line; report the figure); the open list and the open card with no
sideways scroll; at 1280 the label visible, no toggle, the rail; the focus ring whole on the toggle
and on the jumped-to `h3`; 2.4.11 for the toggles under the save bar; forced colours: the toggles
readable (the jump toggle bordered as `.secondary`, the Assets toggle by its text and chevron).

## 6. What must not change

The ten jump buttons' names, order and focus target; the card headings' ids; the nine editable
cards; `focusFailure` and the save; the bar; every key except those named above.
