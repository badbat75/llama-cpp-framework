# 05b: the nav's Profiles group and the `#/profiles/model` routes

Navigation and names only: the Models page becomes the **Model profiles** page under a **Profiles**
group in the nav, at `#/profiles/model`, and a model profile's page moves to
`#/profiles/model/<id>`. No request, reply, save, switch, filter or field changes; no restyle (Hearth's
nav look is H5, `fe/briefs/04-visual-identity.md` section 2); no server profile page (05c).

Sources: `fe/briefs/05-profiles-rename.md` (the slice list and the user's answers of 2026-11-18: menu
entries "Server profiles" and "Model profiles"); `fe/briefs/04-visual-identity.md` 1.2 (the nav tree
and its keys) and 1.7; `ARCHITECTURE.md`, "Nodes and instances", "Revision: profiles" (routes
`#/profiles/model`, `#/profiles/model/<id>`, "an old `#/models` opens `#/profiles/model`"); the user's
answer of 2026-11-17 (old bookmarks keep working, `PLAN.md` step 10); `fe/DESIGN.md` 1 (principle 4,
"No empty chrome") and 2.3 (navigation).

## 0. Decisions

- **"Server profiles" is absent in 05b.** The bff serves no `/api/profiles/server` and 05c builds the
  page, so the entry would lead nowhere. `fe/DESIGN.md` principle 4 already rules it out ("a nav entry
  for a page that does not exist yet: not shown"), and brief 04 1.2 says "05c adds it". A disabled
  entry was considered and not chosen: an `<a>` without `href` is not a link to assistive technology,
  `aria-disabled` on a link announces a control that does nothing, and a disabled control is exempt from
  the contrast minimum ([1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum), its "Incidental"
  note), so its text may be hard to read; a reason text beside it would be chrome for a page the user
  cannot reach. The group's place, label and markup are fixed now, so 05c adds one `li` and nothing
  else moves ([3.2.3 Consistent Navigation](https://www.w3.org/TR/WCAG22/#consistent-navigation)).
  Open for 05c (not here): where the Server profiles entry points before 05f serves the list.
- **The group** copies the Instances group's markup (a `p.nav-label` with an id, a `ul` labelled by
  it), not a disclosure: two entries do not need hiding. Signed in only, as Models is today.
- **No node line** in 05b: `GET /api/node` is not served; it comes with 05e (brief 04 1.2).
- **The list page drops its `h2`.** With the `h1` "Model profiles", the `h2` "Model profiles" would
  repeat it (a preference: [2.4.6 Headings and Labels](https://www.w3.org/TR/WCAG22/#headings-and-labels)
  asks headings to describe their topic, and one heading does). The table is labelled by the `h1`, so
  its accessible name stays "Model profiles".
- **Old links** are rewritten, not served: `#/models` and `#/models/<rest>` become `#/profiles/model`
  and `#/profiles/model/<rest>`, `<rest>` byte for byte, by `history.replaceState` (no history entry,
  no `hashchange`: [HTML Standard, `replaceState`](https://html.spec.whatwg.org/multipage/nav-history-apis.html#dom-history-replacestate)
  updates the URL without a navigation). The new address is then parsed by the new rules, so every
  outcome (page, not-found, the `.` and `..` rule) is the one the new address has.
- **Where the GUI differs on purpose**: `pai-config`'s tab is "Models" (`crates/core/strings/en_US.toml`
  `models`) and `crates/core/strings/cli.toml` says "Models page" (`all-presets-off`,
  `presets-none-configured`); the web follows the user's menu answer ("Model profiles"). The GUI's
  wording is the platform side's to change, not this slice's.

## 1. Criteria

**5b.1 The nav, signed in.** Inside the `<nav>` named "Main", in DOM order: the Overview list
(unchanged); the Instances group when the read gave instances (unchanged); then the **Profiles**
group: `p#nav-profiles.nav-label` with `nav-profiles`, followed by a `ul` with
`aria-labelledby="nav-profiles"` holding one `li` with the link `nav-model-profiles` to
`#/profiles/model`. The group shows whatever the instances read gives (loading, failed, empty), right
after Overview when there is no Instances group. So `getByRole('list', { name: 'Profiles' })` finds
it, and the nav's links read, in order, Overview, each instance, Model profiles
([1.3.1 Info and Relationships](https://www.w3.org/TR/WCAG22/#info-and-relationships),
[4.1.2 Name, Role, Value](https://www.w3.org/TR/WCAG22/#name-role-value)). Signed out, the nav holds
Overview only, no Profiles label (as today).

**5b.2 No dead entry.** No element in the app has the text "Server profiles" or links to a hash
starting `#/profiles/server`; no key `nav-server-profiles` is added in 05b (05c adds it).

**5b.3 Current page.** The Model profiles link has `aria-current="page"` on `#/profiles/model`,
`aria-current="true"` on `#/profiles/model/<id>` (any id, found or not), and none on every other route
(Overview, an instance, sign-in, the not-found page, including `#/profiles/model/`). The label
"Profiles" is not a link and never has `aria-current`
([WAI-ARIA 1.2, `aria-current`](https://www.w3.org/TR/wai-aria-1.2/#aria-current); `fe/DESIGN.md` 2.3).

**5b.4 The narrow menu.** Below 960 px the Profiles group is inside the nav behind Menu, as every
other entry: with the menu open (`aria-expanded="true"`), activating Model profiles closes it
(`aria-expanded="false"`), opens the list and moves the focus to its `h1`; Escape in the open nav
closes it with the focus on Menu (today's 1a.6). Keyboard: Tab reaches the link after the instance
links, Enter follows it ([2.1.1 Keyboard](https://www.w3.org/TR/WCAG22/#keyboard),
[2.4.3 Focus Order](https://www.w3.org/TR/WCAG22/#focus-order)). At 320 px no sideways scroll
([1.4.10 Reflow](https://www.w3.org/TR/WCAG22/#reflow)): the UX check's.

**5b.5 The list page at `#/profiles/model`.** Signed in: `h1#page-heading` with
`model-profiles-title` ("Model profiles"); `document.title` "Model profiles | Private AI Framework"
(`page-title`; [2.4.2 Page Titled](https://www.w3.org/TR/WCAG22/#page-titled)); no `h2` on the page
and no element with the id `model-profiles-title`; the table has
`aria-labelledby="page-heading"`, so `getByRole('table', { name: 'Model profiles' })` finds it. Under
the `h1`, in today's order: the intro (`model-profiles-intro`), the filter, the status lines, the table
(or its loading, error or empty message), the off note. Signed out: the sign-in form with
`signin-required`, the address kept, no read (today's 2a.4); after sign-in, the list with the focus on
its `h1` and one read.

**5b.6 A model profile's page at `#/profiles/model/<id>`.** `<id>` is one path segment,
percent-decoded (today's rules of 2d.56 and 59a, unchanged). Each row's link in the list is
`#/profiles/model/` + `encodeURIComponent(id)` (`modelProfileHref`); the page's back link
(`model-profile-back`, "All model profiles", class `model-profile-back`) is `#/profiles/model`. An id
with a space, `+`, `%` or a non-ASCII letter round-trips: the row link's href encodes it, the page's
`h1` shows the decoded id and its read is `GET /api/profiles/model/<encoded id>` (as today).
`#/profiles/model/.`, `/..` and `/%2E%2E` show today's "no such model profile" text with no read
(59a). ([2.4.4 Link Purpose (In Context)](https://www.w3.org/TR/WCAG22/#link-purpose-in-context).)

**5b.7 Unknown addresses under `#/profiles`.** `#/profiles`, `#/profiles/`, `#/profiles/server`,
`#/profiles/server/default`, `#/profiles/other`, `#/profiles/model/`, `#/profiles/model/a/b` and
`#/profiles/model/%E0%A4%A` (does not decode) show today's not-found page (`not-found`), signed in or
out, with no read; the Model profiles link has no `aria-current` there. (`#/profiles/server...` becomes
a page in 05c and 05f.)

**5b.8 Old `#/models` links.** When the app starts at `#/models`, or a `hashchange` brings it
(a typed address, Back to an old entry), the address becomes `#/profiles/model` by
`history.replaceState` (same path and query): `window.location.hash` reads `#/profiles/model`,
`history.length` does not grow, and the list page shows (5b.5). Signed out, the address is rewritten
first and the sign-in form shows; after sign-in the list opens. `#/modelsx` and `#/models-a` are not
rewritten (not-found page).

**5b.9 Old `#/models/<rest>` links.** `#/models/<rest>` becomes `#/profiles/model/<rest>`, `<rest>`
copied byte for byte (never decoded and re-encoded), then parsed by 5b.6 and 5b.7:
`#/models/Qwen3-27B` opens that page; `#/models/Qwen%203` opens the page of `Qwen 3` (read
`/api/profiles/model/Qwen%203`); `#/models/%2E%2E` gives the 59a text with no read;
`#/models/`, `#/models/a/b` and `#/models/%E0%A4%A` give the not-found page at the rewritten address.
With unsaved edits, a `hashchange` to an old address passes today's leave guard (38b) on the
**rewritten** address: the old address of the page already shown (`#/models/X` while on
`#/profiles/model/X`) asks nothing and keeps the edits, with the address `#/profiles/model/X`; an old
address of another page asks first, as any other address does.

**5b.10 Focus and announcements, as today** (`fe/DESIGN.md` 2.3, `PageHeading.tsx`): on every route
change the app makes (a link, Back, Forward, a typed address, a rewritten old address arriving by
`hashchange`), the new page's `h1` (`#page-heading`, `tabindex="-1"`) takes the focus, so a screen
reader announces its name, and `document.title` is `<h1 text> | Private AI Framework`; the menu closes.
On the first load (including a rewritten one) the focus does not move, as today
([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order), [2.4.2](https://www.w3.org/TR/WCAG22/#page-titled)).
List to page: the page's `h1` is the id; page to list via "All model profiles" or the nav: the `h1`
"Model profiles", and the filter text typed before is still there (02's 62).

**5b.11 Strings** (`fe/src/strings/en_US.json`, exactly; the copies in `fe/src/test/bff.ts` follow):

| Key | English | Change |
| --- | --- | --- |
| `nav-profiles` | Profiles | new |
| `nav-model-profiles` | Model profiles | new (replaces `nav-models`) |
| `nav-models` | (Models) | removed |
| `models-title` | (Models) | removed (the `h1` uses `model-profiles-title`) |
| `model-profiles-title` | Model profiles | unchanged text; now the `h1`, the document title and the table's name |
| `api-no-model-profile-named` | There is no model profile named {0} any more: it was renamed or deleted elsewhere. Open the list of all model profiles again to see them as they are. | changed (was "... Open the Models page again to see the model profiles as they are."; the words follow 05a's `model-profile-off`) |
| `model-profile-back` | All model profiles | unchanged |
| `model-profile-off` | (05a's text) | unchanged: it already says "the list of all model profiles" |

No other text says "Models" as a page: `server-models-folder`, `server-max-models` and the hints name
the folder or llama-server's flag and stay. Key renames: `nav-models` -> `nav-model-profiles`;
`models-title` removed without successor. ([3.2.4 Consistent Identification](https://www.w3.org/TR/WCAG22/#consistent-identification):
the nav entry, the `h1` and the document title all say "Model profiles".)

**5b.13 The address already shown is no navigation** (decided after the review of 2026-11-18). When
the address to route on, after the rewrite of 5b.8 and 5b.9, equals the one the app already shows
(`#/models/X` arriving while on `#/profiles/model/X`), the app does nothing beyond the rewrite: no
route change, no focus move, no title change, no read, the menu left as it is, the leave guard not
asked, a "session ended" sign-in text kept; clean or dirty alike. One rule for every way to the page
shown: a click on the current nav entry or on any link to the page shown already does this (the
browser fires no `hashchange` for an unchanged fragment,
[HTML Standard, "navigate to a fragment"](https://html.spec.whatwg.org/multipage/browsing-the-web.html#navigate-fragid),
and measured 2026-11-18: the focus stays on the link in Chromium, Firefox and WebKit); only the click's
own effect of closing the open menu stays (activating a link in the menu, 1a.6). Why: the user did
not leave; moving the focus to the `h1` would take a user typing in a field away from their place
([2.4.3 Focus Order](https://www.w3.org/TR/WCAG22/#focus-order),
[3.2.1 On Focus](https://www.w3.org/TR/WCAG22/#on-focus) and
[3.2.2 On Input](https://www.w3.org/TR/WCAG22/#on-input): no change of context the user did not ask
for). This narrows 5b.10: "a rewritten old address arriving by `hashchange`" moves the focus only
when it names another page.

**5b.12 What must NOT change.** Every request (method, path, body), every reply read, every state,
live region, dialog and focus move of the list (filter, switch, sign-in dialog), of the model profile
page (cards, jump list, save bar, save, leave guard) and of the instance page (Server form, run verbs,
boot); the Overview; the instances nav; the banner; the styles (`index.css` gains nothing: `.nav-label`
already exists). The code names below are renamed by `git mv` and the imports follow.

## 2. Names in the code

| Old | New |
| --- | --- |
| `fe/src/Models.tsx` (component `Models`) | `fe/src/ModelProfiles.tsx` (component `ModelProfiles`) |
| `fe/src/Models.test.tsx` | `fe/src/ModelProfiles.test.tsx` |
| `fe/src/ModelsFilter.test.tsx` | `fe/src/ModelProfilesFilter.test.tsx` |
| `fe/src/ModelsSwitch.test.tsx` | `fe/src/ModelProfilesSwitch.test.tsx` |
| route value `{ page: 'models' }` | `{ page: 'model-profiles' }` |
| `modelProfileHref` returns `#/models/<enc>` | returns `#/profiles/model/<enc>` (same name) |
| `h2#model-profiles-title` | removed; the table's `aria-labelledby` is `page-heading` |
| (none) | `p#nav-profiles.nav-label` |

Comments that say "the Models page" or `#/models` follow (the route's own redirect excepted). Every
`#/models...` hash in the tests becomes the new address, except the tests of 5b.8 and 5b.9.

## 3. Tests the tester writes or changes

New (red first):

- 5b.1: the Profiles list by name, its one link and href, the nav's link order with and without
  instances, signed out without the label.
- 5b.2: no "Server profiles" text, no `#/profiles/server` href, no `nav-server-profiles` key.
- 5b.3: `aria-current` on the list, on a found and a missing model profile's page, and none on the
  Overview, an instance, `#/profiles/model/`.
- 5b.4: Menu open, click Model profiles: `aria-expanded="false"`, focus on the `h1` "Model profiles".
- 5b.5: `h1`, title, no `h2`, no `#model-profiles-title`, the table's name.
- 5b.6: row link and back link hrefs; an id with a space and `%` round-trips (href, `h1`, read path).
- 5b.7: each address of the list shows `not-found` and reads nothing.
- 5b.8: start at `#/models` (signed in, signed out then sign in); `hashchange` to `#/models` from the
  Overview (focus on the `h1`); `history.length` unchanged; `#/modelsx` not rewritten.
- 5b.9: `#/models/Qwen3-27B`, `#/models/Qwen%203`, `#/models/%2E%2E`, `#/models/`, `#/models/a/b`,
  `#/models/%E0%A4%A`; dirty page plus `hashchange` to its own old address (no dialog, edits kept) and
  to another old address (the dialog).
- 5b.11: `en_US.json` has the table's keys and texts exactly, and neither `nav-models` nor
  `models-title`; the new `api-no-model-profile-named` text shown on a 404 `no-model-profile-named`
  of the switch (the list's alert, `ModelProfiles.tsx` `failureText`, 2c.48) and of the save (the
  save bar's alert, `ModelProfilePage.tsx` `modelProfileKey`, 146). A 404 of the page's READ is not
  this text: it shows `model-profile-not-found` ("There is no model profile named {0}."), unchanged,
  as do the ids `.` and `..` (59a).
- 5b.13: on a clean and on a dirty model profile page, with the focus in a field and (below 960 px)
  the menu open, a `hashchange` to the page's own old address: the address reads the new one, the
  focus is still on the field, `aria-expanded` is still `"true"`, `document.title` unchanged, no
  dialog, no new read, the draft kept.

Changed (names only, same count): every existing test's hash, key (`nav-models`, `models-title`) and
heading query (the `h2` queries become the `h1` or the table) by this brief; the three test files
renamed. Check after the coder: this grep, from the repo root, prints only `route.ts`'s rewrite and the
tests of 5b.8 and 5b.9:

    grep -rn "#/models" fe/src fe/ux/check.js fe/ux/README.md

Only the UX check sees: the Profiles label and its link in the open 320 px menu (no sideways scroll,
contrast, focus ring), axe on the labelled list, the real engines' handling of the rewrite.

## 4. The UX check (`fe/ux/`)

`check.js`: the state `models` becomes `model-profiles` at `#/profiles/model` (expect
`#model-profiles-filter`, and `#page-heading` with `S['model-profiles-title']`); `model-profile` and
`model-profile-missing` keep their names at `#/profiles/model/<id>`. Still seven states. `stub.py`:
nothing (no route changes). `README.md`: its words.

## 5. After the slice (designer)

`fe/DESIGN.md` 2.1: the row "Models" becomes "Profiles > Model profiles", `#/profiles/model`,
`#/profiles/model/<id>`, with the old links' rewrite; 2.3: "a model profile's page under Models"
becomes "under Model profiles", and the Profiles group joins the nav's description.

## 6. Out of scope

Server profiles entry and page (05c), the node line (05e), Hearth's nav look and icons (H5, H8), any
text of the GUI or the CLI.
