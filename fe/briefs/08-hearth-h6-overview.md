# 08 H6: the Overview's instances as tiles

Direction B's Overview (brief `fe/briefs/04-visual-identity.md` 1.3 and section 2 row H6) on what the
bff serves today. Two test-first cycles:

| Cycle | What | DOM | Criteria |
|---|---|---|---|
| **H6a** | the table becomes a list of tiles: id link, state badge, engine line | yes | 9 |
| **H6b** | the Start at boot row, signed in only, read per tile | yes | 8 |

Order: H6a, then H6b (both edit `Instances.tsx`). Neither collides with H7 (`SignIn.tsx`) or H9a (the
jump list rules), so those can run beside them.

Labels: **measured**, **read** (with its URL), **assumed**, **preference**.

## 0. Decisions

- **Only what the API serves.** `GET /api/instances` answers `[{"id", "engine", "state"}]`
  (`fe/briefs/api-contract.md`, "Routes"; `fe/src/Instances.tsx` `isInstance`). Port, tags and the
  node's name are the **planned** reply of 05e (api-contract "Planned: profiles, nodes and
  instances": `serverProfile`, `port`, `tags`, `GET /api/node`); they are **not** in H6. The tile's
  `dl` of facts holds only Start at boot (H6b) until 05e adds Port and Tags to it.
- **Start at boot: after sign-in only** (the user's answer to brief 04 question 1, 2026-11-18). It
  is served: `GET /api/instances/{id}/boot`, anyone, 200 `{"enabled"}` (api-contract, "Start, stop,
  restart, boot"), already read by the instance page (`InstancePage.tsx` `readBoot`). Signed-out
  visitors get no row and no boot request; `GET /api/instances` stays as it is.
- **The failed hint is not carried.** Brief 04 named "today's hint line" for Failed: the Overview has
  none today (the Failed text `run-ended-failed` lives on the instance page). The badge says
  "Failed" with its cross shape; the instance page says why. Nothing added.
- **The version stays a line, not a tile.** One fact does not need a tile; brief 04 1.3 moves it
  into the node panel with 05e. `Version.tsx` and its `p` are unchanged (a preference).
- **A list of links, not a grid of divs.** `ul.tiles` of `li.tile`; the list semantics give "list,
  N items" and the item count, the link gives the way in
  ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships)). The id stays a plain link, not a
  heading: the page has one `h2` over the list, as today (a preference: brief 04 1.3).
- **The badge is a pill of text plus shape.** `StateShape` and `stateLabel` as in the table cell
  today, inside a `span.badge`. Never colour alone: the text and the shape carry the state
  ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)). One fill for every state, `--surface`
  (a preference over the mockup's `--accent-soft` for Running: one rule, and every shape colour is
  already 3:1 or more on `--surface`: success 5.30 / 8.13, warning 5.54 / 8.08, danger 5.72 / 7.45,
  muted 5.55 / 6.45, brief 04 1.1 table, light / dark,
  [1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast)).

## 1. H6a: tiles

**H6a.1 DOM of the list.** When the reply is a non-empty list, `Instances` renders, after the `h2`
`instances-title` (unchanged), one `ul` with class `tiles` and no `role`, holding one `li` with class
`tile` per instance, **in the order received**. No `table`, `thead`, `th` or `td` is left in the
Overview.

**H6a.2 DOM of a tile.** Each `li.tile` holds, in order:

1. `div.tile-head` holding `a.tile-id` (`href` = `instanceHref(id)`, text = the id, nothing else
   inside) then `span.badge` holding `StateShape` (the `span.shape` with `aria-hidden="true"`, as
   today) then the text `stateLabel(state)`;
2. only when the reply holds two or more distinct engines (`manyEngines`, unchanged rule):
   `p.tile-engine` with `t('instance-engine', engine)` ("llama.cpp instance").

Nothing else. The link's accessible name is exactly the id; the badge's text is exactly the label
("Running", "Unknown (reloading)" for a state the page does not know)
([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)).

**H6a.3 States unchanged.** Loading: nothing under the `h2` (as today). Empty list: `p`
`instances-empty`. Errors: `p` with `instances-manager-unavailable` (503 `client-unavailable`),
`instances-not-authorized` (502 `client-not-authorized`), `instances-failed` (anything else, a
malformed body, a network failure). No `ul` in any of them. The existing 1a.16 failure tests hold.

**H6a.4 Strings.** Removed from `fe/src/strings/en_US.json` (no longer used; `strings.test.ts`
requires every value to be used): `instances-column-id`, `instances-column-engine`,
`instances-column-state`. Reused: `instances-title`, `instance-engine`, the `instance-state-*` labels,
the four messages of H6a.3. No key added.

**H6a.4a Screen reader** (by the DOM above, not a test of its own): "Instances, heading level 2",
"list, 2 items", "default, link", "Running", next item. The badge shape is silent.

**H6a.5 `.tiles`.** `list-style: none`, `margin: 0`, `padding: 0`, `display: grid`,
`gap: var(--space-4)`, `grid-template-columns: repeat(auto-fill, minmax(min(100%, 15rem), 1fr))`
(one column at 320 px, no sideways scroll: [1.4.10](https://www.w3.org/TR/WCAG22/#reflow)).

**H6a.6 `.tile`.** `background: var(--bg)`, `border: 1px solid transparent` (the edge that survives
forced colours, as `.card`), `border-radius: 18px`, `box-shadow: var(--shadow)`,
`padding: var(--space-4) var(--space-5) var(--space-5)`, `display: flex`, `flex-direction: column`,
`gap: var(--space-2)`. No `height` (only content sizes it:
[1.4.12](https://www.w3.org/TR/WCAG22/#text-spacing)).

**H6a.7 `.tile-head`, `.tile-id`, `.tile-engine`.** `.tile-head`: `display: flex`, `flex-wrap: wrap`,
`align-items: center`, `justify-content: space-between`, `gap: var(--space-2)`. `.tile-id`:
`font-size: 1.25rem`, `font-weight: 700`, `color: var(--accent)`, `text-decoration: none`,
`overflow-wrap: anywhere`, `min-width: 0`; `.tile-id:hover` `text-decoration: underline`. `.tile-engine`:
`margin: 0`, `color: var(--muted)`, `font-size: var(--fs-md)`. The link is not in a block of text,
so colour without underline is allowed (WCAG failure
[F73](https://www.w3.org/WAI/WCAG22/Techniques/failures/F73) concerns links within text).

**H6a.8 `.badge`.** `display: inline-flex`, `align-items: center`, `min-height: 28px`,
`padding: 0 var(--space-3)`, `border-radius: var(--radius-pill)`, `background: var(--surface)`,
`border: 1px solid transparent`, `font-size: var(--fs-sm)`, `font-weight: 600`, `color: var(--text)`.
The `.shape` rules are unchanged.

**H6a.9 Nothing else moves.** The single read of `/api/instances` (41), the nav's instance links, the
h1 and its focus, `Version`.

**What the tester pins (H6a)**: H6a.1 to H6a.3 on the rendered `App` at `#/` (rewrite the row and
cell queries of `fe/src/Instances.test.tsx` 1a.16 to 1a.18 to `list` / `listitem` queries: order
received, link name and `href`, badge text, shape `aria-hidden`, engine line only with two engines,
no `table`); `fe/src/App.test.tsx` lines 131 and 439 and `fe/src/RunControls.test.tsx` line 621 look
for the Overview's `table`: they look for the `list` inside the `Instances` section instead (the
`main` holds no other `list`); the removed keys gone from `src/test/bff.ts` `EN`; H6a.4; H6a.5 to
H6a.8 as declarations of the named rules (stylesheet text, as `HearthH2b.test.ts`).

## 2. H6b: Start at boot on the tiles, signed in

**H6b.1 Who sees it.** `Overview` takes `signedIn: boolean` from `App` (`session` neither `null` nor
`undefined`). Signed out, and while the session is read, no tile sends `GET /api/instances/{id}/boot`
and no tile has a `dl`.

**H6b.2 The read.** Signed in, each tile sends **one** `GET /api/instances/{encodeURIComponent(id)}/boot`
when it mounts, and again only when the tile mounts again (a sign-in while on the Overview mounts the
rows; a sign-out removes them). Not polled; the Overview's `/api/instances` read count stays one
(41, `RunControls.test.tsx` 1c.41 holds).

**H6b.3 DOM.** After a 200 `{"enabled": <bool>}`, the tile's last child is `dl.tile-facts` holding
one `div` with a `dt` (`boot-label` "Start at boot", the instance page's key, reused) and a `dd`
(`instances-boot-on` "On" or `instances-boot-off` "Off"). The `div` wrapper inside `dl` is valid
HTML ([HTML, the `dl` element](https://html.spec.whatwg.org/multipage/grouping-content.html#the-dl-element)).

**H6b.4 Until it answers, and on failure.** No `dl` while the read is pending; no `dl` after any
failure (503, 502, 404 `client-no-instance`, a 401 or 403, a body that is not `{"enabled": <bool>}`,
a network failure; the route is public, "anyone", so a 401 or 403 there says nothing about the
session and the banner is left alone): the Overview is a glance, and the instance page says why the read failed (`bootReadText`).
No alert, no status, no live region: rows appear silently (nothing is announced on arrival, as for
the list itself).

**H6b.5 Strings** (new): `instances-boot-on` "On", `instances-boot-off` "Off". Reused: `boot-label`.

**H6b.6 `.tile-facts`.** `margin: var(--space-2) 0 0`, `display: grid`, `gap: var(--space-2)`;
`.tile-facts > div`: `display: flex`, `flex-wrap: wrap`, `justify-content: space-between`,
`gap: var(--space-1) var(--space-3)`, `padding-top: var(--space-2)`,
`border-top: 1px solid var(--line)` (decorative: brief 04 criterion 4); `.tile-facts dt`:
`color: var(--muted)`; `.tile-facts dd`: `margin: 0`, `font-weight: 600`.

**H6b.7 Text not colour.** "On" and "Off" are words; nothing else marks the state
([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)).

**H6b.8 Nothing else moves.** H6a's DOM before the `dl`; the instance page's boot field.

**What the tester pins (H6b)**: H6b.1 to H6b.5 on the rendered `App` at `#/` with the fake bff:
signed out no boot request and no `dl`; signed in one request per instance with the id encoded (an id
with a space), the `dt` and `dd` texts for `true` and `false`, no `dl` for each failure kind listed
in H6b.4, no `dl` while pending; sign-in from the Overview's banner adds the rows, Sign out removes
them; no element with `role="status"`, `role="alert"` or `aria-live` in a tile. H6b.6 as stylesheet
text.

## 3. UX check states to add (`fe/ux/check.js`, `fe/ux/stub.py`)

The stub gains `list` (default `one`, today's single instance). With `list=many`, `/api/instances`
answers four instances: `default` (`active`, `llama.cpp`), `coder` (`inactive`, `llama.cpp`), `embed`
(`failed`, `llama.cpp`), and a 40-character id without a break point (`activating`, engine `strata`,
so the engine line shows); `/api/instances/<id>/boot` answers `{"enabled": true}` for `default`,
`false` for the others. Two states: `overview-tiles` (`session=admin&list=many`, `#/`, expect
`li.tile` four times and `dl.tile-facts`), `overview-tiles-signed-out` (`session=out&list=many`,
expect `li.tile`, and no `dl.tile-facts`: the check's `expect` only finds, so this one asserts the
tiles; the absent `dl` is pinned by the unit test).

## 4. What only the designer's UX pass sees

By eye at 1280 and 320 px, both themes: tiles in a grid at 1280 (three across in an 880 px column),
one column at 320; the 40-character id breaks inside its tile; the badge wraps under the id when the
row is narrow; the shapes readable on `--surface`; forced colours: tile and badge outlines (the
transparent borders) show, the shapes keep their system colours, text stays. Targets: the id link at
least 24 px high ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).

**Review of H6a (2026-11-19).** H6a.1 to H6a.9 met (`fe/src/Instances.tsx` `Body`, `fe/src/index.css`
`.tiles` to `.badge`). Measured in Chromium, Firefox and WebKit, light and dark, signed in and out,
1, 2, 5 and 12 instances: 3 columns at 1280, 2 at 960, 1 at 480 and 320; tiles of a row the same
height (grid stretch); no sideways scroll; id link at least 31 px each way, the 40-character id
breaks inside its tile; focus ring 3 px at offset 2 px, whole (no clipping ancestor); headings h1
"Overview", h2 "Instances", nothing else; axe 0 at every width and theme. Badge contrast on
`--surface`, lowest shape 5.30 light (Running) and 6.45 dark (Stopped), text 13.78 / 12.70. One
point **not met**, older than H6a (the `.shape` rules are unchanged since the table): under forced
colours (Chromium emulation, both schemes) the shapes collapse: Running's dot and Failed's cross are
painted in `Canvas` and vanish, Starting and Stopping lose their half fill and look like Stopped.
The text still names every state, so [1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color) holds; the
section 4 expectation does not. Fix (measured injected, both schemes: five distinct shapes, Stopped
and Unknown alike as intended), a follow-up of its own:

**H6a.10 (follow-up) Shapes under forced colours.** One block `@media (forced-colors: active)`:
`.shape { forced-color-adjust: none }`, `.shape-running { background: CanvasText }`,
`.shape-stopped, .shape-changing { border-color: CanvasText }`,
`.shape-changing { background: linear-gradient(90deg, CanvasText 50%, transparent 50%) }`,
`.shape-failed::before, .shape-failed::after { background: CanvasText }`. System colours keep the
user's palette ([CSS Color Adjustment 1, 3.2](https://www.w3.org/TR/css-color-adjust-1/#forced-color-adjust-prop)).
Tester pins the block as stylesheet text.

**Review of H6b and H6a.10 (2026-11-19).** H6b.1 to H6b.8 and H6a.10 met (`fe/src/Instances.tsx`
`Tile`, `readBoot`; `fe/src/App.tsx` `signedIn`; `fe/src/index.css` `.tile-facts`, the forced-colours
block). Measured in Chromium, Firefox and WebKit at 1280, 960, 480 and 320, light and dark: one boot
read per tile, the id encoded (`with%20space`); "On" and "Off" both shown; `dl` the last child; the
`dd` has `margin: 0` (no user-agent indent: its right edge on the row's right edge, `dt` on the
left); contrast `dt` 6.24 light / 7.03 dark, `dd` 15.51 / 13.84; at 320 with 200 % text the `dd`
wraps under the `dt` inside the tile, no sideways scroll; axe 0 everywhere. Tree (Playwright's
aria snapshot, the same in all three): listitem, link "default", text "Running", paragraph
"llama.cpp instance", term "Start at boot", definition "On". Mixed outcomes (200, 503, 404, 401,
502): one row, tiles of a row still equal, no error text, no live region. Signed out 0 boot
requests; sign-in from the banner returns to `#/` with the rows (5 requests); Sign out removes them
(0 new). Twelve instances: twelve reads at once, all rows within 80 ms on the stub. Forced colours,
Chromium emulation and Firefox (`browser.display.document_color_use` 2), both schemes: five
distinct shapes (Stopped and Unknown alike, as intended), text and outlines there; Chromium's
pixels equal H6a's injected measurement; normal mode keeps the token colours.

**Layout shift: accepted, no reserved height (decision).** Boot reads held 1 s: the list grows by
one row per tile row (+103 px at 1280, +257 px at 320), a focused link in the second row moves
down 51 px, the first row does not move; Chromium's cumulative layout shift 0.015 at 1280 and 0.078
at 320, under the 0.1 that web.dev calls good ([CLS](https://web.dev/articles/cls), not a WCAG
criterion). Reserving the row would leave an empty band on every tile whose read fails, and the
real read is a local call that answers before a pointer gets there. Reopen if the bff's boot read
becomes slow.

**`role="list"` on `.tiles`: add it, a follow-up DOM change.** WebKit drops list semantics for a
`ul` with `list-style: none` on purpose and advises an explicit `role="list"`
([WebKit bug 170179](https://bugs.webkit.org/show_bug.cgi?id=170179), comment 1); WebKit keeps them
inside `nav` (unverified). Playwright 1.65 has no engine accessibility tree for WebKit (its
`ariaSnapshot` computes roles itself and reports a list in every engine), so this cannot be
measured here. The count of instances is what a glance wants, so `<ul className="tiles"
role="list">`; the tester pins the attribute. Not a condition of H6b.

**Later, for the bff (a note, not a condition):** with many instances the Overview sends one boot
read per tile; a batch answer (the boot flag in `GET /api/instances`, signed in) would make it one.

## 5. What must not change

The `h1` and its focus on arrival; `Version`; the `h2` `instances-title`; the messages of H6a.3; the
one `/api/instances` read shared with the nav; the nav; the instance page; every key except those of
H6a.4 and H6b.5. `fe/DESIGN.md` 5.2 ("State badge", "Overview") is updated by the designer at the
review of H6b.
