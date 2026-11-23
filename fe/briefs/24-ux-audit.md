# Brief 24: audit of the built pages against the principles (2026-11-23)

The user (2026-11-23), after four requests a designer should have caught ("the designer should know
these things, what UX consultancy is this?", and "pochi click per raggiungere un'azione", few clicks
to reach an action). I read the code of every built page: Overview (`Overview.tsx`, `Instances.tsx`),
Server profiles list and page (`ServerProfiles.tsx`, `ServerProfilePage.tsx`), Instance page
(`InstancePage.tsx`), Models list (`ModelProfiles.tsx`), model profile page (`ModelProfilePage.tsx`,
`ModelProfileCards.tsx`), Node info (`NodePage.tsx`) and About (`About.tsx`). I checked them against
`fe/DESIGN.md` principles 6 to 14 and common admin-console practice. I did not build them: `fe/` is
in use by 17f. Requests already briefed are listed, not repeated: the + spacing (21 TP1c), list adds
as + (16 17b2), Delete as − beside the status and the Save bar at the top (22 22D2).

## 1. Clicks from the Overview today

Desktop nav, with the profile reached through the list. At 320 px the menu button adds one click to
every task that starts in the nav.

| Task | Path today | Clicks | Target |
|---|---|---|---|
| Start, stop or restart an instance | tile link, then Start | 2 | **1**: the run action on the tile (A1) |
| Switch a model profile on or off | nav Model profiles, then the row's switch | 2 | 2 (fine) |
| Switch the profile you are looking at on or off | back to the list, find the row, switch | 3 or more, plus a filter | **1**: the switch in the page's status row (A2) |
| Delete a switched-on model profile | list, switch off, profile link, Delete, confirm | 5 | **3**: profile page, switch off, −, confirm (A2) |
| Edit one model profile setting | nav, profile, jump toggle (below 1280 px), jump, field, Save | 4 to 5 | 3 to 4 with a setting search (A6) |
| Create a server profile | nav, + (17b2), fill, Create | 3 | 3 (fine) |
| Create a model profile | nav, + (22C), fill, Create | 3 (22C) | 3 (fine) |
| Rename an instance | tile, Rename, Save (17f) | 3 (17f) | 3 (fine) |
| See which instance serves a model profile | nav, profile page: not shown there either; instance page | 3 or more, by elimination | **1**: a column on the Models list (A3) |
| See a GPU's memory | nowhere on a page (the data is in `GET /api/node/devices`) | not possible | **1**: Node info (A5) |

## 2. Findings, ranked by what a user hits first

Each finding gives its criteria (**23 A.n.m**) for the tester. Texts are proposed keys for
`fe/src/strings/en_US.json`. The user decides the order of the slices; I would take A1 to A3 first.

### A1. Run an instance from its tile (principle 7; the most common task, 2 clicks today)

- **23 A1.1** Each Overview tile ends with the instance page's run buttons for its state:
  - Start, when the state is `inactive` or `failed`;
  - Stop and Restart, when it is `active`.
  They are the same keys and icons as the instance page (`play`, `square`, `rotate-cw`), secondary,
  40 px high, wrapping inside the tile.
- **23 A1.2** One write at a time per tile. While a write runs, the tile's buttons are
  `aria-disabled`. The answer goes in a `role="status"` line, or a `role="alert"` line on failure,
  under the buttons, with the instance page's texts (Action feedback, `fe/DESIGN.md` 5.2); the focus
  stays on the pressed button. The state poll redraws the badge.
- **23 A1.3** The accessible names carry the instance ("Start default", "Stop default"): a visually
  hidden ` default` after the visible verb, so the Tab order through several tiles is unambiguous
  ([2.4.6](https://www.w3.org/TR/WCAG22/#headings-and-labels),
  [2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name): the visible word starts the name).
- **23 A1.4** The 17c start refusal (`instance-no-model-profiles`) shows in the tile's alert with a
  link to the instance page.

### A2. The model profile page carries its own switch (principles 6, 7, 10)

Today the page only says "switch it on in the list of all model profiles" (`model-profile-off`), so
a page reached from a search or a link sends the user back to a list.

- **23 A2.1** The 22D2 status row holds, before the −, the same on/off switch as the Models list's row
  (the same component and the same `PUT /api/profiles/model/{id}/enabled`, with its restart note and
  its feedback in the row's status line). The status paragraph becomes short: `model-profile-on-short`
  (`Switched on: its engine serves it.`) or `model-profile-off-short` (`Switched off: kept, but not
  served.`).
- **23 A2.2** Switching it off brings the − into the row without moving the focus. Switching it on
  removes the −, and if the confirm group is open it closes; the focus stays on the switch.
- **23 A2.3** The switch's write takes the page's revision into account: after a switch the page
  reads the document again, so a later Save uses the new revision. An unsaved edit is kept, and the
  switch is `aria-disabled` while the page is dirty, with the reason in its description
  (`model-profile-switch-dirty`: `Save or discard your changes first.`).

### A3. The Models list shows what serves each profile (principle 11)

The list's rows already carry `assignments` (`GET /api/profiles/model`), but no column shows them;
the Server profiles list already has "Used by".

- **23 A3.1** A column `model-profiles-column-served-by` (`Served by`) after Status lists the
  instances whose `effective` is true, each a link to its instance page (in the instance's 17f name),
  joined by `, `. `model-profiles-served-by-none` (`No instance`) is shown in `--muted`.
- **23 A3.2** The filter also matches these instance names.
- **23 A3.3** At 320 px the column wraps inside the table's scroll box, as the other columns do; the
  table keeps its caption and sort.

### A4. The instance names its address (principle 13)

- **23 A4.1** The instance page's header gains an "Address" row: `http://<host>:<port>/`, where host
  and port are the server profile's `Host` and `Port`, and a loopback or `0.0.0.0` host is replaced by
  the host name the page was loaded from. Beside it is a 40 px `copy` icon button named `Copy
  address`; its `role="status"` line says `Copied.`, or `Not copied: select the address and copy it.`
  when the clipboard is refused.
- **23 A4.2** When the server profile binds only to loopback and the browser is on another host, the
  row says so (`instance-address-local`: `Reachable only from the server machine itself.`) instead of
  offering a link.
- **23 A4.3** The same address, without the copy button, shows on the Overview tile under the port.

### A5. Node info shows the GPUs' memory (principles 10, 12)

`GET /api/node/devices` gives each GPU's total and free memory at the probe, with `probedAt`. The user
ruled out a refresh (2026-11-20), so this shows the probe and dates it.

- **23 A5.1** Each GPU section of Node info gains `node-gpu-memory` (`Memory`): `{free} MiB free of
  {total} MiB` and a meter (`role="meter"`, `aria-valuenow` the used MiB, `aria-valuemax` the total,
  its name the GPU), with `node-gpus-probed` (`As of {0}`, the local time of `probedAt`) under the
  GPUs' heading.
- **23 A5.2** The meter's fill keeps 3:1 against its track in both themes
  ([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast)), and the numbers are text, so colour is
  never the only carrier ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)).

### A6. Find a setting on the model profile page (principle 7; the page has 12 cards)

- **23 A6.1** At the top of the jump list (and open by default at every width, so below 1280 px the
  jump list toggle is no longer a click on the way), a search field `model-profile-find`
  (`Find a setting`), `type="search"`. Typing filters the jump list to the cards that hold a field
  whose label or key contains the text. Enter goes to the first matching field, opening its card when
  collapsed, and focuses it.
- **23 A6.2** A `role="status"` line says `{0} settings match.` or `No setting matches.`.

### A7. The instance page has a back link like the profile pages (consistency)

- **23 A7.1** `InstancePage.tsx` gains the `chevron-left` back link to the Overview
  (`instance-back`: `Overview`) above the `h1`, as the model and server profile pages have. At 320 px
  the nav sits behind the menu, so today the only way back is the menu or the browser's Back.

### A8. Save from the keyboard (admin-console practice)

- **23 A8.1** On a page with the Save bar, Ctrl+S (Cmd+S on macOS) presses Save changes when the form
  is dirty and does nothing otherwise, and never opens the browser's save dialog. The bar's Save
  button names it in its description (`save-shortcut`: `Shortcut: Ctrl+S`), and
  `aria-keyshortcuts="Control+S Meta+S"` is set
  ([WAI-ARIA 1.2](https://www.w3.org/TR/wai-aria-1.2/#aria-keyshortcuts)).

### A9. Empty states offer their action (principle 12)

- **23 A9.1** `instances-empty`, `server-profiles-empty` and `model-profiles-empty` are each followed
  by their list's + (17b2, 22C), already open-able from there. Each text gains its next step: `No
  instances yet: add one with the + below.` and the like. A failed read (`server-profiles-failed`,
  `model-profiles-load-failed`, the instances failure) gains a `Read again` button
  (`read-again`) that repeats the read and puts the focus on the page heading after it.

### A10. Freshness of what a page shows (admin-console practice)

- **23 A10.1** Node info dates its facts with `node-read-at` (`Read at {0}`, local time) and offers
  `read-again`. About needs nothing: its facts change only at an install.

## 3. Out of scope

Live GPU utilisation, log viewing (the journal), and per-row actions on the Server profiles list
beyond the + all wait for a request: none is a common task today.
