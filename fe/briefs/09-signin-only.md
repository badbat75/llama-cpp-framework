# Brief 09: signed out, only the sign-in page

Requirement (user, 2026-11-19, `PLAN.md` step 10 item 13a): a visitor who has not signed in sees only
the sign-in page. No Overview, no framework version, no instances or their state.

The fe slices land **first**, against today's bff (which still answers `GET /api/version`,
`GET /api/instances` and `GET /api/instances/{id}/boot` without a session); then the bff answers
401 `session-required` on those three with no session (`api-contract.md`, "from 2026-11-19"). Every
criterion below holds against both: before sign-in the fe sends none of the three, so it never sees
what they would answer.

Criteria are numbered **S1a.n** and **S1b.n**. 01-server.md already has an "S1" (the revoked
session, `SessionRevoked.test.tsx`): test names cite this brief as `09 S1a.n` so the two never read
alike. Suggested test files: `SignInOnly.test.tsx` (S1a), `SessionEnded.test.tsx` (S1b).

## 0. Slices

| Slice | What | Criteria |
|---|---|---|
| **S1a** | signed out or pending: the sign-in page at every hash, no nav, no Menu, no banner Sign in; the hash rules; reworded strings; the Overview's `signedIn` prop goes | 12 |
| **S1b** | a session that ends while signed in on a page without a form: a 401 on the Overview's reads signs the app out with a "session ended" notice, the same notice for the reads of the other pages | 6 |

Why two: S1a retargets about 30 existing tests in about 20 files (section 5) and the stylesheet tests
of `a.session`; S1b adds a new state and a new string and retargets 5 assertions. One slice would
pass the ~12 criteria a slice can carry (`designer-playbook`). S1a first; S1b builds on it.

What does not change (both): the sign-in form itself (fields, errors, focus after a refusal, the
plain-HTTP notice, `08-hearth-h7-signin.md` H7.1 to H7.9), the session-ended **dialog** over a page
with a form (01-server.md 36, 2h 151, 43a), the leave guard (38, 38b to 38g, G1), the revoked
handling of 403 `session-not-administrator` (01-server.md S1.1 to S1.10, apart from the reworded
text), everything a signed-in visitor sees.

## 1. S1a: signed out, only the sign-in page

**S1a.1 Every hash, signed out.** With `GET /api/session` answering 401, `main` holds the sign-in
page (`div.signin-page`, `h1#page-heading` "Sign in" from `signin-title`, the form) and nothing else,
at `#/`, `#/instances/x`, `#/profiles/model`, `#/profiles/model/x`, `#/sign-in`, an unknown hash
(`#/nope`) and an empty hash. No Overview, no version line, no `ul.tiles`, no not-found page. The
same when the session read fails (a network failure or a 5xx: `readSession` gives null, as today;
signing in then fails with today's `signin-failed`, whose text already names the web service).

**S1a.2 Requests before sign-in.** From the load until a sign-in succeeds, the only API request the
fe sends is `GET /api/session` (once), plus the `POST /api/session` of each submit. Never
`/api/version`, `/api/instances`, `/api/instances/*/boot`, `/api/profiles/*` or `.../server`, at
any of the hashes of S1a.1.

**S1a.3 While the session is read.** `main` holds today's `<p role="status">` "Loading…"
(`loading`), nothing else; no `nav`, no Menu button (S1a.4); the banner holds the home link and the
theme button only. No request but `GET /api/session` is sent meanwhile.

**S1a.4 The shell, signed out and pending.** No `nav` element at all (so no `navigation` landmark:
fine, there is nothing to navigate to); no `#menu-button`; no `a.session` (the banner's Sign in pill
goes: the page IS the form, and the pill would be one more Tab stop pointing at where the focus
already is). The header keeps the home link (mark and `app-name`, `href="#/"`) and the theme button;
`div.banner-tools` holds the theme button only. Signed in, the shell is exactly today's (Menu, theme,
`div.session` with the user and Sign out; the nav with Overview, Instances, Profiles).
Landmarks signed out: `banner`, `main`; the skip link stays first and still targets `#main`
([2.4.1 Bypass Blocks](https://www.w3.org/TR/WCAG22/#bypass-blocks)).

**S1a.5 Tab order signed out.** Skip link, home link, Dark theme, then the user name field, the
password field, Sign in ([2.4.3 Focus Order](https://www.w3.org/TR/WCAG22/#focus-order)). Nothing in
`main` before the user name takes Tab (the `h1` has `tabindex="-1"`, the art is hidden: H7.9).

**S1a.6 Title and heading.** Signed out, at every hash, `document.title` is
`page-title` filled with `signin-title` ("Sign in | Private AI Framework"); the one `h1` is "Sign in".
Nothing names "Overview" (no text, no title, no accessible name)
([2.4.2 Page Titled](https://www.w3.org/TR/WCAG22/#page-titled)).

**S1a.7 The notice.** `signin-required` "Sign in to see this page." (unchanged) shows, with its
`info` icon as today (H8b.3), when the hash is a place other than the Overview and the sign-in page:
`#/instances/x`, `#/profiles/model`, `#/profiles/model/x`, an unknown hash. At `#/`, an empty hash
and `#/sign-in`: no notice (every page needs a sign-in now, so "this page" says something only when
the visitor asked for one). The revoked alert (`session-revoked`) still replaces it as today.

**S1a.8 The address is kept.** Signed out, the hash stays what the visitor asked for; nothing
rewrites it to `#/sign-in`, nothing adds a history entry. (The old `#/models` bookmark is still
rewritten to `#/profiles/model` by 5b.8, signed out too.)

**S1a.9 After a sign-in.** The page of the kept hash shows in place of the form, with the focus on
its `h1` (today's focus request). No history entry is added, so Back leaves the place the visitor
came from and never meets a sign-in page that would bounce. At `#/sign-in` (and only there) the
address becomes `#/` by `history.replaceState` (no entry added, `history.length` unchanged, no
hashchange of its own) and the Overview shows, focus on its `h1` "Overview". This replaces today's
`go('#/')`, which pushed an entry. An unknown hash shows the not-found page after sign-in, as for a
signed-in visitor.

**S1a.10 `#/sign-in` while signed in.** Reached by a typed address, Back or Forward: the address
becomes `#/` by `replaceState` and the Overview shows, focus on its `h1` (an alias of `#/`, as an old
bookmark is of its new address, 5b.8). With unsaved edits the leave guard asks first, as for any
address (38b); Discard then lands on `#/`.

**S1a.11 Sign out.** Unchanged: with a clean page the app signs out and goes to `#/` (`go`, an entry
pushed, as today); with unsaved edits 38's dialog asks first. It lands on the sign-in page with no
notice (S1a.7, at `#/`), the focus on its `h1` "Sign in", the nav and the Menu button gone (S1a.4).

**S1a.12 Code removed and strings.** `Overview` and `Instances` lose the `signedIn` prop: the
Overview renders only signed in, so every tile sends its boot read when it mounts (H6b.2 unchanged
for a signed-in visitor). `SessionBar` renders nothing when `session` is `null` or `undefined`; the
`current` prop goes. The CSS selectors `a.session` and `a.session[aria-current='page']` (top level and
in the forced-colours block) go, with nothing else of their rules. At 960 px and up, `main` spans the
whole `.layout` row when it is its only child: `.layout > main:only-child { grid-column: 1 / -1; }`
inside the `min-width: 960px` block (without it, `main` falls into the 224 px nav column).
Strings: no key is deleted (`session-sign-in` stays: the save bar, the run alert and the list use
it; `signin-required` and `not-found-home` stay). Three texts drop their last clause, which promised
an Overview to the signed out:

| Key | English (new) |
|---|---|
| `api-session-unavailable` | Signing in is not available on this server right now. |
| `api-session-not-administrator` | Only members of the pai group can sign in, and this account is not one. Ask whoever administers this computer to add it. |
| `session-revoked` | You have been signed out: this account is no longer a member of the pai group. Ask whoever administers this computer to add it again. |

**Focus on a direct load (S1a, accessibility).** Unchanged from H7.9: a direct load leaves the focus
where the browser puts it (the document); every in-app arrival (sign out, a hash change, S1b's end
of a session) moves it to the `h1`. No `autofocus` on the user name: it would skip the `h1`, the
intro and the notices for a screen-reader user, who then hears the field before learning why they
are there ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order),
[3.2.1 On Focus](https://www.w3.org/TR/WCAG22/#on-focus)); the user name is the first Tab stop in
`main` anyway (S1a.5). A preference recorded with it: one Tab more for a sighted keyboard user is the
price of a page that reads in order.

## 2. S1b: a session that ends on a page without a form

**S1b.1 The Overview's reads.** Signed in on the Overview, a **401** (by status, the one exception
of the contract's "by the key") on `GET /api/version`, `GET /api/instances` or any tile's
`GET /api/instances/{id}/boot` signs the app out: the sign-in page shows at `#/` with the notice of
S1b.3, the focus on its `h1` "Sign in", the nav and the Menu button gone, no dialog. Several 401s
in a row (the three reads answer together) give one sign-in page and one focus move. The `hash`
stays `#/`; after a sign-in the Overview reads again (S1a.9).

**S1b.2 Not a session end.** On those three reads a 403 `session-not-administrator`, a 403 or 502
`client-not-authorized`, a 503, a 404, a malformed body or a network failure keep today's handling
(version: `version-unavailable`; instances: `errorText`; a tile: no row) and the session; the bff
answers 401, never 403, to a request with no session (platform side, 2026-11-19). On the instance
page, the polled `GET /api/instances` answering 401 changes nothing: the page keeps its form and
the run controls, and its own requests open the session-ended dialog as today (01-server.md 36, 43a).

**S1b.3 The notice.** A `p.notice` with `role="alert"` and the `triangle-alert` icon, the markup of
the revoked notice (H8b.3), with the new text `signin-session-ended`, in the place of
`signin-required` and before the plain-HTTP notice. It shows whenever the app signs out because a
**read** answered 401 while signed in: the Overview (S1b.1), and the reads of the server page, the
model profiles list and the model profile page, which today sign the app out and show
`signin-required` (2a.16, 2d.66, the server read's 401). It never shows after Sign out or on a
first load. Like the revoked notice it goes at the next address change and at a successful sign-in
([4.1.3 Status Messages](https://www.w3.org/TR/WCAG22/#status-messages): `role="alert"` is
announced although the focus goes to the `h1`).

**S1b.4 String** (new): `signin-session-ended` "Your session has ended. Sign in again to continue."
(Today's `session-ended` and `session-ended-retry` speak of saving and retrying an action: they stay
for the dialog and the run alert.)

**S1b.5 Precedence.** The revoked notice (403 `session-not-administrator` on a read, S1.9) wins over
S1b.3: at most one of `session-revoked`, `signin-session-ended`, `signin-required` shows.

**S1b.6 Nothing else moves.** The tiles carry no live region (H6b.4); the instance page's boot field
and its `bootReadText`; the dialog flows of 36, 43a, 151.

## 3. Brief corrections

- `08-hearth-h6-overview.md` H6b.1 and H6b.4: superseded by this brief (a note there): signed out
  there is no Overview, and a 401 on the boot read says the session ended (S1b.1).
- `api-contract.md`: the session-free reads become "no route but the static web UI and `POST`, `GET`
  and `DELETE /api/session` answers without a session" (from 2026-11-19).
- `fe/DESIGN.md` 2.1, 2.2 and 3: the "anyone" rows and the signed-out Overview, marked "from slice
  S1".

The framework version stays on the signed-in Overview until the About place (item 13) holds it.

## 4. UX check (`fe/ux/`)

- `stub.py`: with `session=out`, every `/api/` path but `/api/session` answers 401
  `{"key": "session-required", "args": []}` (the bff after item 13a).
- `check.js`: remove `overview-signed-out` and `overview-tiles-signed-out` (their expect of
  `a.session` and of signed-out tiles is gone). Add `sign-in-at-home` (`session=out`, `#/`, expect
  `#page-heading` with `signin-title` and `input[type="password"]`) and `sign-in-protected`
  (`session=out`, `#/instances/default`, expect `input[type="password"]` and a `p.notice` with
  `signin-required`). Keep `sign-in` (`#/sign-in`). The signed-in states are unchanged.
- The tester's unit tests pin the absences (no `nav`, no request); the check proves the pages at rest.

## 5. Tests to retarget or delete (S1a unless noted)

Every test that renders `App` signed out or pending and expects anything in `main` but the sign-in
page or `loading`, a `nav`, the Menu button or `a.session`:

- `App.test.tsx`: 1a.2 (pending: no nav), 1a.3 (signed out: no Sign in, no nav), 1b.5a (delete: the
  pill is gone), 1a.8 (an unknown route is not-found only signed in), 1a.19 "Overview without sign-in"
  (delete; S1a.1 and S1a.2 replace it), 1a.1 order (signed out: skip link, banner, main).
- `SignIn.test.tsx`: 1a.13 (2.6) and 1a.13 at `#/sign-in` (S1a.9, S1a.10: replaceState, no entry),
  1a.14b "the Overview link still works" (delete), 14a/14b texts (S1a.12).
- `HearthBanner.test.tsx` H5a.1 signed out and pending; `HearthIcons.test.tsx` H8a.3 signed out;
  `HearthTheme.test.tsx` H4.1 (three states), H4.2 signed out; `HearthMark.test.tsx` H3.3 (the mark
  on every route, signed out included: kept, retarget only what it expects in `main`).
- `HearthTilesBoot.test.tsx` H6b.1 signed out (delete), H6b.1 pending (no list: keep), H6b.4
  unreachable (signed in), H6b.2 "signing in from the banner" (sign in from the page at `#/`; Sign out
  removes the whole Overview). `HearthTiles.test.tsx` and `Instances.test.tsx`: the signed-out
  `fakeBff` becomes signed in.
- `InsecureSignIn.test.tsx` I7 "Overview signed out" (delete; the banner part: no notice in the
  banner, kept signed in). `ProfilesNav.test.tsx` 5b.1 signed out (no nav), 5b.5 and 5b.8 signed out
  (unchanged but for the nav). `ModelProfiles.test.tsx` 2a.1 pending and signed out (no nav).
- Stylesheet tests naming `a.session`: `HearthH5a.test.ts` H5a.4 (delete) and the current-entry list,
  `HearthH5b.test.ts`, `HearthH8a.test.ts`, `HearthTokens.test.ts`, `HearthSaveBarScroll.test.tsx`.
- `SessionRevoked.test.tsx` S1.10 (the reworded `session-revoked`).
- S1b: the 401-on-read assertions of `signin-required` in `InstancePage.test.tsx` 295,
  `ModelProfilePage.test.tsx` 377, 837, 862 and `RunControls.test.tsx` 801 become
  `signin-session-ended`.

## 6. What the designer measures at review

- Signed out at 1280 and 320 px, light and dark, Chromium, Firefox, WebKit: no sideways scroll; the
  banner one row at 1280 (home link, theme button); `main` starts at the page gutter, not at x 224;
  the sign-in panel and art as H7 measured them.
- Tab order of S1a.5 by keyboard; the focus ring of the user name not obscured
  ([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)).
- Back after a sign-in at `#/instances/default` and at `#/sign-in` (S1a.9); `#/sign-in` typed while
  signed in (S1a.10).
- S1b: on the signed-in Overview, `/__set?session=out`, then a hash change away and back to `#/`
  (the Overview reads again): one sign-in page, the alert read once.
- axe-core clean on the new states (section 4).

## 7. Review of S1a (designer, 2026-11-19)

**Approved.** Measured on the uncommitted change in Chromium, Firefox and WebKit (Playwright, a copy
of the UX stub with a request log and two modes: `today`, where `/api/version`, `/api/instances` and
the boot read answer 200 with no session, and `future`, where every `/api/` path but the session
answers 401 `session-required`), at 1280, 960, 480 and 320 px, light and dark. Scripts:
`build/ux-review/s1a.js`, `s1a-skip.js`, `stub9.py`; output `s1a.txt`.

- UX check (`fe/ux`): 52 states per browser, 0 failing; `sign-in-at-home`, `sign-in-protected` and
  `sign-in` render as section 4 says.
- S1a.1, S1a.4, S1a.6 met (`App.tsx` 331, 350, 399): 252 loads (7 hashes, both modes): `main` holds
  `div.signin-page` only, one `h1` "Sign in", title "Sign in | Private AI Framework", no `nav`, no
  `#menu-button`, no `a.session`, `div.banner-tools` holds the theme button only. Unreachable bff
  (requests aborted): the sign-in page, and a submit shows `signin-failed`.
- S1a.2 met, both modes: the only request before a sign-in is one `GET /api/session`, at every hash.
- S1a.3 met: session read held 1 s: "Loading…", no nav, no Menu, theme button only, then the form;
  no flash of nav or Overview in 100 ms samples.
- S1a.7 met (`App.tsx` 406): the notice at `#/instances/default`, `#/profiles/model`,
  `#/profiles/model/x`, `#/nope`; none at `#/`, the empty hash, `#/sign-in`.
- S1a.8, S1a.9 met: sign-in at each of the 7 hashes lands on that page (an unknown hash: not-found;
  `#/sign-in`: `#/` Overview), focus on its `h1`, nav present; `history.length` unchanged in all 84
  runs (3 engines, 2 widths, 2 modes); Back goes to the entry before the visit, Forward returns to
  the page. The jsdom gap the tester left is closed here.
- S1a.10 met (`App.tsx` 101, 132, 159): `#/sign-in` set while signed in on an instance page:
  `history.length` grows by one (the typed entry itself) and that entry reads `#/`, the Overview
  with its nav entry current, focus on its `h1`; Back returns to the instance page, Forward to `#/`.
  A direct load of `#/sign-in` signed in shows `#/` with no entry added.
- S1a.11 met: Sign out from the Overview and a clean instance page: `#/`, the sign-in page, no
  notice, focus on "Sign in", no nav. Dirty form: the discard dialog; Keep editing keeps the page
  and session; Discard changes signs out as above. Revoked read: the reworded `session-revoked`
  alert, axe 0.
- S1a.12 met: `signedIn` gone from `Overview.tsx` and `Instances.tsx`; `a.session` gone from
  `index.css`; `.layout > main:only-child` at `index.css` 660: signed out `main` is x 0, full width
  at 960 and 1280 (signed in: x 224, 736 and 1056); art and panel side by side, 420 px each, as H7
  (at 960 now 420 px columns instead of H7's 308, the nav column being gone). Strings
  `en_US.json` 30 to 32 as the table.
- S1a.5 met: Tab order skip link, home, Dark theme, user name, password, Sign in in all three
  engines at 1280 and 320, at `#/` and a protected hash; the skip link moves the focus to `main`
  and keeps the hash, the next Tab is the user name. The header is static, so the focus is not
  obscured (2.4.11). 200 % text: stacked, no sideways scroll, nothing clipped. Forced colours
  (Chromium): the panel edge, fields and button seen; axe 0. axe 0 on 72 sign-in loads (both
  hashes, every width, both themes).
- Unchanged and checked: a 401 on a save keeps the form and shows the save bar's
  `session-ended` alert.
- For S1b, measured today: signed in on the Overview, the session gone and the bff at `future`,
  a hash change away and back gives the Overview with "The version could not be read from the
  service." and "The instances could not be read.", the nav and Sign out still shown, 2 requests
  in 3 s (no loop, no boot read). With today's bff the Overview reads as if signed in.
- Observation, a preference and not a condition: at 1280 the art and panel sit at x 40 to 920, the
  right 360 px empty under a banner whose theme button is at the far right. Centring the pair could
  be a later polish; nothing in H7 or this brief asks for it.
