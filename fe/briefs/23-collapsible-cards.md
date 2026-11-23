# Brief 23: collapsible cards, Expand all and Collapse all (slice K1, K2)

The user, 2026-11-23 (translated): "the assets section opens collapsed; I don't mind it, it could be
applied to every section of the portal. Could you also add collapse-all and expand-all controls."
Criteria are numbered **23 K.n**, and test names cite them that way. There are two parts, built in
order: **K1** (the pattern, the model profile page and the controls; 12 criteria) and **K2** (the
server profile page; 5 criteria).

## 0. What exists (read on main, 2026-11-23)

- **Assets** is the one collapsible card (brief 08 H11a; `fe/DESIGN.md` 5.2 "Collapsed card";
  `ModelProfilePage.tsx` 1660 to 1700). Its `h3` (`tabindex="-1"`, the jump target) holds only a
  `button` with `aria-expanded` and `aria-controls` the body, the title and a `chevron-down`. The body
  is `hidden` when closed. The card is closed on every arrival, with nothing stored. A jump to it
  opens it in the same commit as the jump list's close (`jump`, 871 to 881).
- The other cards are `Card.tsx`: a `section.card` holding an `h3`, an optional intro and the fields.
  The model profile page has 11 of them (`ModelProfileCards.tsx` `CARDS`, each with a stable `id`:
  `gpu`, `tensor`, `hardware`, `resource-context`, `embeddings`, `kv-cache`, `chat-reasoning`,
  `sampling-overrides`, `advanced`, `vision`, `speculative-decoding`), plus Assets. The server profile
  page's form has 5 (`ServerForm.tsx` 631 to 790: Network, Resources, Advanced, Conversation
  snapshots, SDK tuning).
- The focus moves into a card from code in three places: the jump list (`jump`), a client check or a
  refusal on Save (`focusField`, `ModelProfilePage.tsx` 683, `ServerForm.tsx` 482), and the GPU
  card's weights (1417). No route or hash names a field: the app routes on the hash
  (`fe/DESIGN.md` 2.3), so a "link to a field" is one of these code moves, not an anchor.
- The only `localStorage` user is the theme (`theme.ts`, key `pai-theme`, every access in try/catch).
- The GUI (`pai-config`) shows each tab's groups always open, so here the web is first.

## 1. Decisions (preferences unless they cite a source)

1. **Only the cards of the two long forms collapse**: the model profile page (Assets and its 11
   cards) and the server profile page (its 5 cards). The user, asked, confirmed this on 2026-11-23 (Q2) after
   first suggesting "every section of the portal". The instance page (header with the run controls, tags,
   server profile, and later the model profiles), the Overview, the lists, About and Node info stay
   as they are. They are one screen or a table, and the instance header is a live state with
   controls that must stay in view. Closing them would add a click in front of every look and hide
   state, the empty chrome of principle 4. A section that grows long later can take the same
   pattern then.
2. **Open by default, Assets closed, and the last choice remembered per page kind.** The user
   confirmed this on 2026-11-23 (Q1). A
   settings card closed by default hides the values an administrator opens the page to read, and
   Save's refusals would keep opening cards. Collapse all, remembered, gives anyone who prefers the
   closed page exactly that at the next arrival. The memory is per viewer and per page kind
   (`pai-cards:model-profile`, `pai-cards:server-profile`), not per profile: one layout for every
   model profile. Storage is a convenience: it is read and written in try/catch, and the defaults
   apply when it is empty, unreadable or blocked.
3. **The H11a pattern, not `<details>`**: the `h3` keeps its heading role and holds the toggle
   button. A `summary` replaces the heading's role with its own (H11a section 0 chose this for Assets
   on that ground), and the jump list targets the `h3`. The body (intro and fields) is `hidden` when
   closed: out of the Tab order and the accessibility tree, so a closed card cannot hold the focus.
4. **A card opens before the focus goes into it**: the jump list, a client check, a refusal naming a
   field and the GPU weights all open the card in a `flushSync` before `focus()`. A user can close a
   card that holds an error; the save bar's alert still says what is wrong, and the next Save opens
   the card again.
5. **Expand all and Collapse all are two buttons, always active.** A single toggle would have to
   guess the mixed state. Two verbs say what each does, and an inactive one would only raise the
   question of why. Each press changes every card at once, leaves the focus on the button, and is
   announced by a polite status line.

## 2. The texts

| Key | English | Part |
|---|---|---|
| `cards-controls-label` | `Cards` | K1 |
| `cards-expand-all` | `Expand all` | K1 |
| `cards-collapse-all` | `Collapse all` | K1 |
| `cards-all-expanded` | `Every card is open.` | K1 |
| `cards-all-collapsed` | `Every card is closed.` | K1 |

Reused: every card title, `model-profile-assets`.

## 3. K1: the pattern, the model profile page, the controls (12)

**23 K.1 The card.** `Card` takes, when it is collapsible, `open`, `onToggle` and a `bodyId`. Its `h3`
(id and `tabindex="-1"` as today) holds only a `button[type="button"]` with `aria-expanded`,
`aria-controls` = `bodyId`, the title, then `svg.icon` `chevron-down` (`aria-hidden`). The intro and
the fields are inside `div#<bodyId>`, which carries `hidden` when the card is closed. A `Card` with
none of these props renders as today (the server profile page until K2).

**23 K.2 The look.** The toggle is the Assets toggle's (`fe/DESIGN.md` 5.2): no border or fill, the
heading's font, full width, at least 32 px high, the chevron at the far end, turned 180 degrees when
open, with no transition. `index.css` generalises the Assets rule to every card toggle rather than
adding a second one. Under forced colours it reads as a control by its text and chevron.

**23 K.3 The model profile page.** All 12 cards are collapsible: Assets plus the 11 of `CARDS`, each
keyed by its `id` (Assets: `assets`). The Assets card keeps its intro outside the body (H11a: the
closed card still says why it only reads). For the others, the intro goes with the body.

**23 K.4 The defaults.** With nothing stored, Assets is closed and the 11 others are open.

**23 K.5 The memory.** Every toggle, Expand all and Collapse all writes to
`localStorage['pai-cards:model-profile']` the JSON array of the ids of the cards now closed, in
`CARDS` order (`[]` when all are open). On arrival the page reads it. A missing key, a value that is
not an array of strings, or a read or write that throws, means the defaults; ids the page does not
know are ignored. Save, Discard, Reload and a sign-in leave every card as it is.

**23 K.6 Opened before the focus.** The jump list, a client check that fails on Save, a 422 naming a
field (`focusField`), and the GPU weights check (`ModelProfilePage.tsx` 1417) each open the card
holding the target in the same commit (`flushSync`), then move the focus. The opened state is stored
as any toggle is. A refusal naming no field opens nothing.

**23 K.7 The controls.** After the summary line and the engine notice, before the Assets card, sits
a `div[role="group"]` named `cards-controls-label` (`aria-label`) holding two secondary buttons:
`cards-expand-all` with `chevron-down`, then `cards-collapse-all` with a `chevron-down` turned 180
degrees. Each is always active. Expand all opens all 12 cards, and Collapse all closes them. The
focus stays on the pressed button. A visually hidden `p[role="status"]` in the group then reads
`cards-all-expanded` or `cards-all-collapsed`, and is emptied by the next toggle of a single card.

**23 K.8 Keyboard.** Tab order: the jump list's controls, Expand all, Collapse all, then the Assets
toggle and the cards in page order. A card's toggle is a Tab stop and takes Enter and Space (native
`button`); a closed card's fields are not in the Tab order. Closing a card leaves the focus on its
toggle.

**23 K.9 Screen reader.** A toggle reads as "Network, button, expanded" (or "collapsed") inside
"heading level 3" ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value),
[1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships)). The two buttons' names are their
visible text ([2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name)), and their outcome is the status
line ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)). A jump or a refusal never lands on a
hidden element ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)).

**23 K.10 320 px and 200 %.** The two buttons sit in a wrapping row (`display: flex; flex-wrap: wrap;
gap: var(--space-2)`), one per line when they do not fit side by side. A long card title wraps inside
its toggle, with the chevron kept beside the last line. No sideways scroll
([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)). Targets stay at least 32 px high
([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).

**23 K.11 What does not change.** The jump list's entries and their order; the save bar, its dirty
rule and the leave question (a closed card's edits are still unsaved changes); every field's id,
label, hint and error. The Assets toggle's id `model-profile-assets-toggle` stays.

**23 K.12 Strings and CSS.** `en_US.json` gains exactly the five keys of section 2. `index.css` gains
the group's row rule and the generalised toggle rule, with no token. R1.8's class list and H1a.12's
`min-height` list are amended by name for whatever classes and minimum sizes the coder adds.

**Tests that change** (the tester names them before the red run): the Assets tests of 08 H11a
(`HearthH10.test.ts` and the H11a tests): "closed on every arrival, no storage" becomes 23 K.4 and
K.5. Tests that read a card's fields right after the page loads keep passing, since the cards are
open by default. Tests that find a card by its `h3` text now find a heading holding a button with
that text. `fe/ux/check.js` gains `model-profile-collapsed` (`#/profiles/model/<id>`, Collapse all
pressed, markers: every toggle has `aria-expanded="false"` and the status reads
`cards-all-collapsed`), seen at 320 px and 200 %.

**K1 and 2j.** K1 edits `Card.tsx`, `ModelProfileCards.tsx` (the `CARDS` render, around 2495) and
`ModelProfilePage.tsx`. 2j's TP2 and TP3 are editing `TensorPlacement.tsx` and the model profile
tests now, so K1's tester starts once the last 2j part is committed.

## 4. K2: the server profile page (5)

**23 K.13** The 5 cards of the Server form are collapsible as in K.1 and K.2, keyed `network`,
`resources`, `advanced`, `snapshots` and `sdk`. All are open by default. The memory is
`localStorage['pai-cards:server-profile']`, with the rules of K.5.

**23 K.14** The controls of K.7 sit after the Server section's intro (`server-profile-intro`), before
Network, with the same texts. Expand all and Collapse all act on the 5 cards.

**23 K.15** `focusField` (`ServerForm.tsx` 482) opens a field's card before moving the focus: client
checks, refusals with a `field`, and the Hostname rows.

**23 K.16** The Used by row, the Delete button and its confirm (brief 16 17b) stay outside every card
and never collapse.

**23 K.17** `fe/ux/check.js` gains `server-profile-collapsed`, as K1's state. Tests that change: the
server form's tests that find a card by its `h3`.

## 5. Out of scope

- Collapsing the instance page, the Overview, the lists, About or Node info (decision 1).
- A per-profile memory; a memory on the server (it would need a per-user store that the bff does not
  have: `fe/DESIGN.md` 2.1 "per-user state with no home yet").
- Marking a closed card that holds unsaved edits (U17e marks changed fields; that may add a count on
  the toggle later).
- The browser's find in page reaching the text of a closed card: Expand all is the way. Whether
  `hidden="until-found"` should be used is left for later; it was not checked against browser
  support here (unverified).
- Animation (the identity's calm: no transition).

## 6. Questions for the user (answered)

- Q1 is answered (user, 2026-11-23): the settings cards are open by default, Assets closed, and the
  last choice is remembered per page kind (decision 2).
- Q2 is answered (user, 2026-11-23): only the two long forms collapse (decision 1).
