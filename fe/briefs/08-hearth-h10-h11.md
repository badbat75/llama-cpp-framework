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

**H10.4 A jump closes it.** Activating one of the ten buttons first closes the list
(`aria-expanded="false"`, `ul.collapsed`) and commits that to the DOM, and only then moves the focus
to the card's `h3` and scrolls it into view as today (196). The dirty form asks nothing (as today).
Corrected at the review (2026-11-19): the first wording (focus, scroll, then close) leaves every
heading 332 px above the viewport from 960 to 1279 px (the side-menu shell) in Chromium, Firefox
and WebKit, while 320 to 959 px land at 12 px (**measured**, `build/ux-review/h10j.js`, `h10b.js`):
the 320 px list vanishes above a heading already scrolled to the top, and only the narrow layout
happens to be saved by the browsers' scroll anchoring. Emulating close-first lands at 12 px at
960 and 1279 px in Chromium and WebKit (`h10k.js`).
One way: `flushSync(() => setJumpOpen(false))` before `focus()` and `scrollIntoView()`
([react.dev flushSync](https://react.dev/reference/react-dom/flushSync)). The tester pins the
order: at the moment `scrollIntoView` is called (a spy), the list already has class `collapsed`.

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

## 7. Review of H10 (2026-11-19)

Read from the uncommitted `ModelProfilePage.tsx`, `index.css`, `en_US.json`, and **measured** in
Playwright 1.65.1 (Chromium, Firefox, WebKit) at 1440, 1280, 1279, 960, 480 and 320 px, light and
dark (`build/ux-review/h10.js`, output `h10.txt`; probes `h10j.js`, `h10b.js`, `h10k.js`; stub
`stub8.py`). Verdict: **rejected** on H10.4 alone; everything else met.

- H10.1, H10.2, H10.5: met. Label, toggle (`aria-controls` resolves in both states), list with
  `collapsed` only while closed; closed on every arrival; "On this page".
- H10.3: met. Click, Enter and Space each flip `aria-expanded` and the chevron (`rotate: 180deg`),
  focus stays on the toggle. Escape does nothing, as the brief says: the disclosure pattern needs no
  Escape ([APG disclosure](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/)).
- H10.4: **not met** (`ModelProfilePage.tsx` `jump`, about line 617): see the correction under H10.4.
  From 960 to 1279 px every jump puts the focused `h3` 332 px above the viewport, in the three
  engines ([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum),
  [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)). At 320 and 480 px it lands at 12 px, under
  no bar, list closed, hash unchanged; a dirty form keeps its edit and opens no dialog.
- H10.6, H10.9: met. Toggle 158 to 159 x 40 px, `.secondary` border, chevron after the text, no
  transition (also under reduced motion). Closed, the first card's top at 320 px is 691 (693 WebKit):
  296 px above H9a (987), 246 above the page before H9a (937). Open: the list under the toggle,
  ten 32 px underlined entries, full width, no sideways scroll.
- H10.7: met. At 1280 and 1440 no toggle, the label 15 px bold above the rail, the list shown and
  tabbable though its class is `collapsed`; rail x 1000 (1160 at 1440), width 240, sticky top 16,
  as H9b. Resizing: closed 1280 to 1279 shows the closed toggle; open at 1279, up to 1280 and back
  keeps it open (the state survives; no reset needed). Fresh arrival at any width: closed.
- H10.8, H10.10: met. Closed below 1280, Tab goes back link, toggle, About Engine, the Engine select:
  the hidden entries are out of the tab order. Nav name: Chromium's own accessibility tree gives
  navigation "On this page" at 320 and at 1280 (and button "On this page", expanded false/true);
  for Firefox and WebKit only Playwright's computed snapshot was read (the same names), not the
  engines' own trees. A hidden element referenced by `aria-labelledby` still names
  ([accname 1.2, 2B](https://www.w3.org/TR/accname-1.2/#comp_labelledby)). The nav and its button
  share the words: **preference**, acceptable, the button's expanded state tells them apart.
- 200 % text and the 1.4.12 spacing at 320 and 1280: nothing clipped, no sideways scroll (entries
  grow to 101 px when a title wraps). Forced colours (Chromium, Firefox): toggle bordered, text and
  chevron `ButtonText`, entries `LinkText`. Other `.secondary` buttons unchanged by the rescoped rule
  (Menu 104x40, theme 40x40, Sign out 124x40, Discard 90x40; the rule never matched outside `.jump`).
- axe-core (WCAG 2.0 to 2.2 A, AA): 0 at every width, both themes, closed and open, three engines.
  `npm --prefix fe/ux run check`: 9 states x 2 widths x 2 themes + 3 stored = 48 per browser,
  0 failing.

**The UX check's state, still to add** (section 4; `fe/ux/check.js`): a state field
`click: [selectors]`; after `rendered(page, state)` succeeds and before `sleep(SETTLE_MS)`, for each
selector, click it only when `page.locator(sel).isVisible()` (the jump toggle is `display: none`
at 1280 px, where the list is always shown), and then wait for `[aria-expanded="true"]` on it; a
selector that is visible but never expands is a finding. New state after `model-profile`:
`{ name: 'model-profile-open', set: 'session=admin', hash: the model profile hash, expect: as
'model-profile', click: ['#model-profile-jump-toggle'] }`, with `'#model-profile-assets-toggle'`
appended in H11a. The summary then reads 10 states x 2 widths x 2 themes = 40, plus the 3
stored-choice states = 52 per browser.

**Re-review after the fix (2026-11-19): Approved.** `ModelProfilePage.tsx` `jump` (lines 618 to
624) closes the list with `flushSync` before `focus()` and `scrollIntoView()`; `fe/ux/check.js`
adds `click` (comment line 51, `clicked()` lines 225 to 245: skips a hidden selector, reports a
visible one that never reaches `aria-expanded="true"`) and the state `model-profile-open` (line
135), as specified. **Measured** (`build/ux-review/h10r.js`, stub `stub8.py`, Chromium, Firefox,
WebKit): at 1279, 1100, 960, 959, 480 and 320 px, clean and with a dirty form (save bar shown),
each of the ten entries (seven by click, three by Enter) puts the focused `h3` at 12 px, fully
visible and above the save bar, the list closed, the hash unchanged, and scrollY the same at 150 ms
as after the settle (one scroll, no second jump). H10.4: met. `npm --prefix fe/ux run check`: 52 per
browser, 0 failing. axe-core: 0 at 1280, 960 and 320 on the model profile page (closed and, below
1280, open) and the Overview, light in the three engines and dark at 1280.

**Review of H11a (2026-11-19): Approved.** Read: `ModelProfilePage.tsx` (`modelFileName`, the
summary `p`, `assetsOpen`, `jump`, the `section.card.assets-card`), `index.css` (H11a.9 rules),
`en_US.json` (the two keys), `fe/ux/check.js` (the `click` list of `model-profile-open`).
**Measured** (`build/ux-review/h11a.js`, `h11a-aria.js`, stub `stub8.py`; Chromium, Firefox,
WebKit; 1280, 960, 480, 320 px, light and dark):
- H11a.1, H11a.2: met. Summaries: a POSIX path "Engine llama.cpp, model Qwen3-27B-Q8_0.gguf"; a
  Windows path with a 119-character file name wraps inside the column (3 to 7 lines), no sideways
  scroll at any width; `\\server\share\` "Engine llama.cpp, model share"; no `model` "Engine
  llama.cpp, no model file". The trailing-separator case is a broken profile either way and the
  card's Model file row shows the whole value, so "share" stays (preference; no correction).
- H11a.3, H11a.4: met. Card children `h3#model-profile-card-assets > button`, `p.intro`,
  `div#model-profile-assets-body`; closed on arrival, after a reload and after the list and back.
  Closed card 143 px high at 1280 and 960, 169 at 480, 222 at 320 (open 426 to 845). The first
  editable control below it (`model-profile-n-gpu-layers-box`) at 320 px: 845 to 847 px with a
  one-segment file name (target under 900: met; open, as before H11a, 1549 to 1602), 952 to 978 px
  with the 119-character name; at 1280: 623 to 665 px (open 986 to 1001).
- H11a.5: met. Click, Enter, Space flip `aria-expanded` and `hidden`, focus stays on the toggle;
  closed, Tab from the toggle goes to the next card's About; the body holds 0 focusables.
- H11a.6: met. At 320, 960, 1279 (behind the disclosure) and 1280 (rail), clean and dirty, by click
  and by Enter: the Assets jump opens the card, the focused `h3` lands at 12 px, visible, above the
  save bar, the list closed, scrollY equal at 0, 50 and 150 ms; another jump leaves Assets open or
  closed as it was. Focus is on the `h3` (as specified); its reading starts "Assets, heading level 3".
- H11a.7: met for Save (200) and Discard with the card open: it stays open, the dirty value kept;
  the 422 case is the tester's.
- H11a.8, H11a.10: met. Aria snapshot: `heading "Assets" [level=3]` holding `button "Assets"`,
  `[expanded]` once open.
- H11a.9: met. Toggle 646x36 at 1280, 238x36 at 320; ring 3px solid at 2px offset, whole, inside
  the card; chevron `rotate: 180deg` open; transition 0s with reduced motion. 200 % text and 1.4.12
  at 320: toggle 238x62 to 64, not clipped, chevron inside the card, no sideways scroll. Forced
  colours (Chromium, Firefox): text and chevron `ButtonText`, summary visible, no border (as
  specified).
- H11a.11: met. The Engine field stays above the card.
- Section 4: `npm --prefix fe/ux run check`: 10 states x 2 widths x 2 themes + 3 = 52 per browser,
  0 failing; `clicked()` fails a visible toggle that never reaches `aria-expanded="true"`, so in
  `model-profile-open` both disclosures were open at 320 (the Assets click lands after the list
  opens) and the Assets one at 1280. axe-core: 0 at every width, both themes, closed and open.
