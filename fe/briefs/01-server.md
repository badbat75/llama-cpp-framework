# Brief 01: the app shell and the Server page of instance `default`

Written by the designer on 2026-11-14 for `PLAN.md` step 10, "First editing slice". Applies
`fe/DESIGN.md`; section numbers like "D5.2" point there. Each numbered line is an acceptance
criterion the tester turns into a Vitest + Testing Library test. The reference is `pai-config`'s
Server tab (`crates/config/ui/server_page.slint`, `crates/config/src/server_form.rs`) and its texts
(`crates/core/strings/en_US.toml`).

## Administrators only (user, 2026-11-15)

The whole web UI is run by administrators (`fe/DESIGN.md` section 3): sign-in succeeds only for a
member of the OS group `pai` on the server machine, and anyone else is refused (14b, in 2.2). The
Overview stays visible without sign-in (19). There is one signed-in role: no page has a read-only view,
and the fe reads no role from the session. The criteria below are written for that rule.

Record: the code built for a non-administrator was removed in four slices, all built: **R1** the
sign-in refusal and `signin-intro` (14b, 9); **R2** the read-only Server page and a user's views of the
API key (25, 27, 27a to 27e, 28, 5.1 to 5.3 F4; 27b and 27c removed); **R3** the disabled boot box and
the "same role" half of the sign-in rule (45b removed; 45c, 45g, 36b, 43b); **R4** the session reply
without `canConfigure` and the role words (1, 4, 5.6). Models (brief 02) had no code to remove.

## 0. The split

The whole page is too big for one slice. Three slices, each shippable alone:

| Slice | Holds | Needs from the bff |
|---|---|---|
| **1a** | App shell, sign-in and sign-out, the Overview (today's page moved, new error shape), the instance page header (id, engine, state), the Server page **read-only** for every role | session routes, instances with engine, server read |
| **1b** | Editing and saving for administrators: dirty state, save bar, client checks, server refusals beside their field, discard guard, session that ends mid-edit | server write |
| **1c** | Start, Stop, Restart, live state, start at boot | start/stop/restart, boot read/write |

**Out of slice 1** (later slices, named so nobody builds them now): the Command line card (needs a
bff render of `launch::server_args`), the ROCm warning strip (a device probe on the server: the
Hardware page), Logs, "Open chat UI", the import dialog of an earlier per-user install, any instance
but those the manager lists, creating or deleting instances, every other nav page of D2.1, LAN access
and TLS.

Each slice adds exactly the strings keys it uses (`fe/src/strings.test.ts` fails on an unused key).
The keys below are tagged with their slice.

## 1. Data the slices read (the tests stub these)

The bff's shape (coordinator, 2026-11-14): everything under `/api/`; errors are
`{"key": "<strings key>", "args": [...]}`; 401 not signed in, 403 the manager refused. Exact bodies are
the coordinator's contract; the brief assumes these:

- `GET /api/session`: 200 `{"user": "<name>"}`, 401 when signed out. A reply that still carries
  `canConfigure` is accepted and the field ignored (the bff sends `{"user"}` alone since commit
  296165f; an older bff sent the field).
- `POST /api/session` `{"username", "password"}`: 200 as above (sets the cookie); 401
  `{"key":"session-bad-credentials"}`; 429 `{"key":"session-too-many-attempts","args":["<seconds>"]}`;
  403 `{"key":"session-not-administrator","args":[]}` (14b);
  503 `{"key":"session-unavailable","args":[]}` (no way to check a password: the bff shipped before
  the OS login answers it to every sign-in); 400 `session-malformed`. A session ends after 30 minutes
  without a request; the next call answers 401 (`session-required` on `/api/session`).
- `DELETE /api/session`: 204.
- `GET /api/version`: as today.
- `GET /api/instances`: 200 `[{"id", "engine", "state"}]`; errors with keys `client-unavailable`,
  `client-not-authorized`, anything else.
- `GET /api/instances/{id}/server`: 200 with `values` (INI key names: `Port`, `Hostname`,
  `OpencodeBaseUrl`, `OpencodeApiKey`, `Threads`, `ThreadsBatch`, `ModelsMax`, `ModelsMemoryMargin`,
  `ModelsDir`, `Webui`, `WebuiMcpProxy`, `LogVerbosity`, `SaveStateOnShutdown`, `StateDir`,
  `RocblasUseHipblaslt`, `RocprofAttach`, `CudaAllreduce`, `CudaDisableGraphs`, `AttnRotDisable`;
  `null` = unset), `defaults` (`Port`, `ModelsMax`, `ModelsMemoryMargin`, `OpencodeBaseUrl`,
  `ModelsDir`, `StateDir`: the instance's own, resolved on the server), `limits` (`[min, max]` for
  `Port`, `ModelsMax`, `ModelsMemoryMargin`, `Threads`, `ThreadsBatch`; the thread max is the server
  machine's logical CPU count), `interfaces` (`[{"value", "label"}]`, the server machine's IPv4
  interfaces, labels as data), `revision` (opaque). Errors: 401, 403 `client-not-authorized`, 404
  `client-no-instance`, 503 `client-unavailable`, anything else.
- 1b: `PUT /api/instances/{id}/server` `{"revision", "values"}`: 200 `{"revision", "running": <bool>}`;
  422 a check's `{"key", "args", "field"?}`; 409 `{"key":"server-changed-elsewhere"}`; 401; 403;
  503.
- 1c: `POST /api/instances/{id}/start|stop|restart` (signed in): 204; errors by key (43). Boot:
  `GET /api/instances/{id}/boot` (anyone) and `PUT` (administrator) `{"enabled": <bool>}`, both
  answering `{"enabled"}` (`fe/briefs/api-contract.md`; 45).

## 2. Slice 1a: shell, sign-in, Overview, read-only Server page

### 2.1 Shell

1. The page has, in DOM order: a link "Skip to content" (`skip-to-content`) targeting `main`; a
   `banner` with the product name (`app-name`) linking to `#/`; a `navigation` named `nav-label`; a
   `main`. ([2.4.1](https://www.w3.org/TR/WCAG22/#bypass-blocks), [1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships))
2. On start the fe asks `GET /api/session` once. Until it answers, `main` shows
   `<p role="status">` `loading`, and the nav shows only Overview.
3. **Signed out** (401): the banner shows a button or link **Sign in** (`session-sign-in`); the nav
   holds only **Overview** (`nav-overview`).
4. **Signed in**: the banner shows `session-signed-in-as` with the user's name,
   then a button **Sign out** (`session-sign-out`); no role word. The nav holds Overview, then a
   heading-like label **Instances** (`nav-instances`, a list labelled by it) with one link per instance from `GET /api/instances`, the
   link text being the id, in the reply's order. A failed instances read leaves the list out of the
   nav (the Overview shows the error); from 1c, only until a read succeeds (41a).
5. The link of the current page has `aria-current="page"`.
6. Below 960px a button **Menu** (`nav-menu`) with `aria-expanded` shows and hides the nav; Escape,
   pressed on the Menu button or anywhere inside the open nav, closes it and returns focus to the
   button. (Layout itself is checked in review, not in jsdom.)
7. A route change sets `document.title` to `page-title` with the page name (`overview-title`,
   `signin-title`, or the instance id) and moves focus to the page's `h1`.
   ([2.4.2](https://www.w3.org/TR/WCAG22/#page-titled), [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order))
8. An unknown route shows `not-found` in `main` with a link to the Overview (`not-found-home`).

### 2.2 Sign-in

Route `#/sign-in`, and in place of any protected page while signed out (then without changing the
route, and returning to it after sign-in).

9. `h1` `signin-title`; intro `signin-intro`; a text field labelled `signin-username`
   (`autocomplete="username"`), a password field labelled `signin-password`
   (`autocomplete="current-password"`), a submit button `signin-submit`. Inside a `form`; Enter in
   either field submits. Paste is not blocked.
   ([3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions), [3.3.8](https://www.w3.org/TR/WCAG22/#accessible-authentication-minimum))
10. On a protected route the form shows `signin-required` above the fields.
10a. A session that ended while idle (30 minutes without a request) is a 401 like any other: in 1a the
    next read signs the app out and shows this form with `signin-required` (2.6, "401 on the server
    read"); the wording fits, since 1a has no edits to lose. Nothing changes; 1b and 1c show
    `session-ended` and `session-ended-retry` where edits or an action are at stake (36, 43).
11. Empty user name or password: no request; the empty field shows `signin-empty-username` or
    `signin-empty-password` under it (`aria-invalid`, `aria-describedby`); focus moves to the first
    empty field.
12. While the request runs the button reads `signin-busy` and a second submit sends nothing.
13. 200: the session state updates (banner and nav as in 4), focus goes to the destination's `h1`:
    the protected page it replaced, else the Overview.
14. 401 `session-bad-credentials`: a `role="alert"` paragraph above the fields with
    `api-session-bad-credentials`; the password field is cleared, the user name kept, focus on the
    password field. 429 `session-too-many-attempts`: `api-session-too-many-attempts` with the seconds.
    Anything else, a network failure and 400 `session-malformed` included: `signin-failed`. Never the
    reply's body.
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification), [4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
14a. 503 `session-unavailable`: the same `role="alert"` paragraph above the fields, with
    `api-session-unavailable` (not `signin-failed`, which tells the user to try again). As in 14: the
    password field is cleared, the user name kept, focus on the password field; the form stays usable
    (the alert does not ask for a retry and does not block one: the same key covers a server with no sign-in yet and a check that failed for a moment). Tested with the body
    `{"key":"session-unavailable","args":[]}`.
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification), [3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion), [4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
14b. **Not an administrator** (added 2026-11-15, administrators only). 403
    `{"key":"session-not-administrator","args":[]}`: the password was right but the account is not a
    member of the server machine's `pai` group. The same `role="alert"` paragraph above the fields, with
    `api-session-not-administrator` ("Only members of the pai group can sign in, and this account is
    not one. Ask whoever administers this computer to add it; you can still see the overview without
    signing in.").
    The password field is cleared, the user name kept, **focus on the user name field** (unlike 14
    and 14a: what needs changing is the account, not the password). The form stays usable for another
    try; nothing else changes: the app stays signed out, the banner keeps **Sign in**, the nav keeps
    Overview, and the Overview link works (19). By key, not status: a 403 with another key or no key
    is `signin-failed`, as 14. Never the reply's body. In the sign-in dialog of 36 and 43a the same:
    the alert inside the dialog, the dialog stays open, its Cancel works.
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification), [3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion), [4.1.3](https://www.w3.org/TR/WCAG22/#status-messages), [2.4.3](https://www.w3.org/TR/WCAG22/#focus-order))

    Telling it apart from `session-bad-credentials` is decided (user, 2026-11-15, on the designer's
    recommendation): the reply comes only after the password is verified, so it tells nothing to
    someone who does not already hold the account's password; the 429 limit is checked first, as
    today, and bounds guessing; a single message would leave a non-administrator retrying a correct
    password into the 429 lockout with no hint of what to do (3.3.3). The cost: a guesser who hits a
    non-administrator's password learns it is right, a credential that may work elsewhere on the
    machine (SSH). The generic-message advice of the
    [OWASP Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html#authentication-and-error-messages)
    is about not telling which of user name or password is wrong, which 401 still follows.

    Strings: `api-session-not-administrator` | Only members of the pai group can sign in, and this
    account is not one. Ask whoever administers this computer to add it; you can still see the
    overview without signing in. (`pai` is the group's name as written on the machine, left literal
    like the command in `boot-hint`.)
15. **Sign out**: sends `DELETE /api/session`, then shows the signed-out shell on the Overview
    whatever the answer (the cookie may already be gone). In 1b, with unsaved changes, it first asks
    with the discard dialog (38).

### 2.3 Overview (`#/`)

16. `h1` `overview-title`, then today's version line and instances section unchanged in text,
    except: the reply's error shape is now `{"key"}`: `client-unavailable` shows
    `instances-manager-unavailable`, `client-not-authorized` shows `instances-not-authorized`,
    anything else `instances-failed` (today's tests move to the new shape).
17. Each instance id in the table is a link to `#/instances/<id>`.
18. An Engine column (`instances-column-engine`, the engine name as data) shows only when the
    instances have two or more distinct engines; with one engine the table has the two columns of
    today.
19. Visible signed in or not.

### 2.4 Instance page header (`#/instances/<id>`, signed in)

20. `h1` is the id; under it `instance-engine` with the engine name ("llama.cpp instance"); then the
    state as `instance-state` filled with today's state label (`instance-state-active` ...
    `instance-state-other`), with the D5.2 shape (`aria-hidden`). Data from `GET /api/instances`;
    read once on open in 1a.
21. An id the instances list lacks, or a server read answering 404 `client-no-instance`:
    `instance-not-found` with the id, and the link `not-found-home`.
22. No tab list (one page only, D1.4): the Server section follows the header directly.

### 2.5 Server section, read-only

23. `h2` `server-title`, intro `server-intro` filled with `server@<id>.ini`.
24. Loading: `<p role="status">` `server-loading`. Errors in place of the form: 401 the sign-in form
    (2.2); 403 `api-client-not-authorized`; 404 as 21; 503 `api-client-unavailable`; anything else,
    malformed included, `server-load-failed`.
25. Loaded: the cards and fields of section 5, in that order, each control showing the file's value
    (5.2); no Notice. (In 1a every control was disabled; 1b and 28 enable them.)
26. Each field's hint toggle (D5.2) works in 1a: button named "About `<label>`" (`field-about` plus
    the label), `aria-expanded` false, then true on Enter, Space or click, revealing the hint with one
    paragraph per line of the text.
27. The key shows masked; its **Show** button (`field-show`, `aria-pressed`) reveals it, under 5.3.
    A withheld document: 27a, 27d, 27e (2.8).

### 2.6 Decided at the review of 1a (2026-11-15)

Points the brief left open, settled by the designer after reading the built slice; they are now part
of the brief.

- **Not-found page** (8): its `h1` is the `not-found` sentence, so the document title is
  "There is no page at this address. | Private AI Framework". No separate short title key.
- **Instance page while `/api/instances` fails** (20): `h1` the id, then the Overview's message for
  the key (`instances-manager-unavailable`, `instances-not-authorized`, `instances-failed`), and no
  Server section (without the list the page cannot tell the id exists).
- **Instance page while loading** (20): the `h1` (id) shows at once, then `<p role="status">`
  `loading`; when the list answers, the engine and state, then the Server section.
- **401 on the server read** (24): the whole app becomes signed out: the sign-in form with
  `signin-required` in place of the page, the banner's **Sign in**, the nav without its instances;
  after sign-in the page reads again. (1b keeps this for a read; a 401 on a save is 36.)
- **Server-read errors** (24): only the form is replaced; the `h2` and intro stay. 404 shows
  `instance-not-found` and `not-found-home` inside the section.
- **`#/sign-in` while signed in**: the form is shown (it lets the user switch account); after it,
  the Overview.
- **Sign-in busy** (12): the button stays enabled (no `disabled`, no `aria-disabled`, so focus
  never drops); its text is `signin-busy` and a second submit sends nothing.
- **Show** (27): 5.3 governs it (disabled while No key is ticked).
- **Reads of `/api/instances`**: in 1a the nav, the Overview and the instance page each read it on
  their own. 1c's polling must share one read across them, so the nav and the badge never
  disagree (41, 41a).
- **Overview columns** (18): Instance, then Engine (when shown), then State: the order of the
  instance page's header (id, engine, state).

### 2.7 Decided after the first run against the real bff (2026-11-15), built in slice 1b

Two observations from running 1a against the real bff. Both ride along with slice 1b.

5a. **Sign in on its own page.** The banner's **Sign in** link stays where it is on `#/sign-in` (the
    banner keeps the same items in the same place on every page), and there it carries
    `aria-current="page"` with the current-page style of the nav links, as criterion 5 says for every
    link to the current page. It does not carry it while the form stands in for a protected page (the
    route is that page's). The current-page style is the same rule as `.nav a[aria-current='page']`
    (accent colour, weight 600, the 3px inset bar), applied to the banner link too. ([3.2.3](https://www.w3.org/TR/WCAG22/#consistent-navigation),
    [1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships))
8a. **Icon.** `fe/index.html` declares one icon, inline, so the browser stops asking the bff for
    `/favicon.ico` (a 404 today; the bff serves only `/` and `/assets/*`, so the icon cannot be a file
    of its own). The exact element, in the `head` after the viewport `meta`:

    ```html
    <link rel="icon" type="image/svg+xml" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'%3E%3Crect width='16' height='16' rx='3' fill='%230a5cc2'/%3E%3Cpath d='M4 12 8 4l4 8M5.5 9h5' fill='none' stroke='%23fff' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E" />
    ```

    A letter A drawn in white strokes on a rounded square of `--accent` (light value; white on it is
    6.3:1). Drawn with strokes, not a font, so it looks the same everywhere. It carries no text and
    is not part of the page, so nothing for a screen reader. **(preference)**: an empty `href="data:,"`
    would also stop the request; an icon is chosen so the tab can be told apart. Tested by reading
    `fe/index.html` (one `link rel="icon"`, a `data:image/svg+xml` href) or checked in review.
    Source: [MDN, `rel="icon"`](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Attributes/rel#icon).
    Unverified: which browsers ignore an SVG icon and still ask for `/favicon.ico`.

### 2.8 A withheld API key (user, 2026-11-15)

Every signed-in session is an administrator ("Administrators only" at the top), and F4 is the same
for all (5.1 to 5.3). The reply shape keeps `withheld` (`fe/briefs/api-contract.md`,
`GET /api/instances/{id}/server`): an administrator's document can still arrive withheld until the bff
acts for the signed-in user (step 10 slice 6b) or when the manager withholds without a polkit
authorization (`PLAN.md` step 10, point 4).

27a. **Withheld or not.** "Withheld" means `withheld` is an array that contains the string
    `"OpencodeApiKey"`; anything else (absent, `[]`, an array of other names, `null`, not an array)
    is "not withheld". Names the fe does not know are ignored. Not withheld: the form of 5.2.
    Withheld: 27d.
27b, 27c. Removed (2026-11-15): they were a non-administrator's views of F4.
27d. **Withheld fails the read**, for every session: `server-load-failed` in place of the form (24),
    no save bar. Why: the value would be `null`, so the form would show No key ticked and a save would
    send `null`, erasing the key from the file. A failed read loses nothing; reopening the page reads
    again.
27e. **Nothing in the page holds a key or a stand-in for one** when the document is withheld: no F4
    `input` of any type, no attribute carrying a key, no mask string (a fake mask would read as a
    secret's length). Tested with `"withheld": ["OpencodeApiKey"]` and, as a contract breach,
    `values.OpencodeApiKey: "sk-test"`: the rendered HTML contains neither `sk-test` nor `•`, and
    `server-load-failed` shows.

## 3. Slice 1b: editing and saving (administrators)

28. For every signed-in session the controls are enabled, under the dependencies of 5.3, and the save
    bar (D5.2) is rendered at the end of the form: status line (`role="status"`), **Discard**
    (`save-discard`), **Save changes** (`save-button`), both `aria-disabled="true"` while clean.
28a. **Ticking and unticking a default box** (F1, F3, F7, F8; also auto on F5, F6 and No key on F4):
    ticked, the textbox shows what 5.2 says (the default, or empty); unticked, it shows again the text
    it held just before it was ticked. On a field loaded on default that text is the default shown
    (5.2), on one loaded on auto or No key it is empty; on a field the user typed in, it is what they
    typed, so ticking by mistake loses nothing. Discard and a successful save reset that memory to the
    loaded values. Unticking moves focus only on F5 and F6 (5.2: their box comes back empty).
    ([3.3.7](https://www.w3.org/TR/WCAG22/#redundant-entry), in its spirit: the user never types the
    same value twice; the rule itself is a **(preference)**)
29. **Dirty**: any value differing from the loaded one (after the same normalisation as the save,
    5.4) makes the form dirty: the status reads `save-unsaved`, both buttons enabled. Changing it back
    makes it clean again.
30. **Discard**: puts every control back to the loaded values; focus stays on the button; status
    empty. No request.
30a. **The bar's two lines.** The status line (`role="status"`) says what state the form is in, and
    only that: empty while clean, `save-unsaved` while dirty, `save-saving` while the PUT runs,
    `save-saved` or `save-saved-restart` after a successful save until the next edit. A save that
    fails (client checks, a refusal, any error of 34 to 37) puts it back to `save-unsaved`: the edits
    are still unsaved. The failure itself is the bar's alert, a separate element after the status line
    and before the buttons (`role="alert"`, rendered only while it has text). Every Save click on a
    dirty form replaces the previous alert: a click that fails the client checks shows `save-invalid`
    in place of whatever alert was there (31); a click that passes them clears the alert at once, at
    the click, not when the answer arrives, and the answer then shows its own (or none, on a 200).
    Otherwise the alert stays until Discard, a confirmed Reload (35) or a successful sign-in in the
    dialog (36); editing a field clears only that field's error (31), not the alert.
    ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
31. **Save**, client checks first (5.4). Any failure: no request; each failing field shows its error
    (`aria-invalid`, `aria-describedby`); focus moves to the first failing control in DOM order; the
    save bar shows `save-invalid` as `role="alert"`. Editing a field clears its error.
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification), [3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion))
32. Checks pass: `PUT` with the loaded `revision` and every value (unset ones as `null`). While it
    runs the Save button reads `save-saving`, is `aria-disabled`, and a second click sends nothing;
    the status line reads `save-saving` too (30a). **Discard is `aria-disabled` too while the PUT
    runs** (decided at the 1b review): the save cannot be called back, so Discard must not look as
    if it undid it; a click does nothing. The fields stay editable (D6, Saving).
33. 200: the reply's revision and the sent values become the loaded ones (clean); status
    `save-saved`, or `save-saved-restart` when `running` is true. A save never restarts the server (as
    the GUI).
33a. **After a 200 the controls show what was sent** (5.4), which is what the file now holds: Base URL
    without its trailing `/` and `/v1`, texts trimmed, numbers as written by the save (`08581` shows
    `8581`), a blank folder empty with its placeholder, an empty API key with No key ticked. Focus
    stays on Save changes. `defaults` are not read again (the file is not re-read, 33): a default box
    keeps showing the default read with the page until the next read (Reload, or opening the page
    again); the Base URL hint already says its default is that of the saved settings.
33b. **Edits made while the PUT runs are kept** (decided at the 1b review). The sent values become
    the loaded ones as in 33, but if the user changed any control after the click, the controls keep
    what the user has now (not the values sent), so the form is dirty again against the new loaded
    values and the status reads `save-unsaved`. Only when nothing changed during the PUT do the
    controls take the values sent (33a). Nothing typed is ever overwritten by an answer.
    ([3.3.7](https://www.w3.org/TR/WCAG22/#redundant-entry), in its spirit)

**Answers to the PUT: the key decides, never the status, except 401** (`fe/briefs/api-contract.md`,
"Rules for every route"). The reply's status only says whether it is an error; the fe reads the body's
`key` and picks the message by it, in this order: 401 whatever the body (36); `server-changed-elsewhere`
(35); `client-not-authorized`, `client-unavailable`, `client-no-instance` (37, 37a); a check key of 5.6
(34). A 422 is a refusal by definition (contract): its unknown key shows `api-other` (34). Any other
reply, a body with no key, an unknown key outside a 422, a body that is not JSON, a network failure or
a 2xx that is not `{"revision", "running"}`: `save-failed` (37). A 2xx is a save only when its body
is an object whose `revision` is a string and whose `running` is a boolean; any other 2xx (no body,
a missing or mistyped member) is `save-failed`, the form stays dirty. Never the body's text.

34. **A refusal** (a check key of 5.6, normally with 422): `api-<key>` filled with `args` under the
    field `field` names, or the field of the fallback table under 5.6 when `field` is absent
    (`aria-invalid`, `aria-describedby`), focus on it, and the bar's alert shows `save-refused` with
    the same text. A `field` the form lacks, or no field and no fallback: only the bar. A 422 whose key
    the fe does not know: the bar's alert `save-refused` with `api-other` filled with the key, no field
    marked.
34a. **A refusal on Bind to** (`field` `Hostname`, or the fallback of `client-unresolvable-host`): Bind
    to is a group of checkboxes, so the error paragraph sits inside its `fieldset`, after the last row
    (D5.2 Checkbox list). Every checked row gets `aria-invalid="true"` and `aria-describedby` the error
    (the checked rows are the value refused); focus goes to the first checked row in row order, which
    is always enabled (5.3: a checked row is `0.0.0.0` or a row that `0.0.0.0` does not disable).
    Checking or unchecking any row clears the error, as editing clears a field's error (31).
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification),
    [4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value); `aria-invalid` is allowed on a checkbox:
    [WAI-ARIA 1.2](https://www.w3.org/TR/wai-aria-1.2/#aria-invalid))
34b. **A refusal on a disabled control** (Max models with default ticked, as `load-on-startup-*` can
    be; Snapshot folder with Save on shutdown off, as `comment-markers` can be, since the server
    checks a kept value the client check skips): the error paragraph sits under the field as usual and
    its main control gets `aria-invalid` and `aria-describedby` (still exposed when disabled), but
    focus cannot go to a disabled control: it goes to the field's first enabled input control (Max
    models: its default box; never its About button), or, when the field has none, to the switch that
    enables it (5.3: Save on shutdown for Snapshot folder; by the same rule Web UI for MCP proxy, though
    no check key of 5.6 names MCP proxy today, so a test of it uses a made-up `field` and is optional).
    The bar's alert already reads the text. Nothing is ticked or enabled for the user.
    ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order), [3.3.1](https://www.w3.org/TR/WCAG22/#error-identification))
35. `server-changed-elsewhere` (normally 409): the bar shows `api-server-changed-elsewhere` and a button
    **Reload** (`save-reload`); Reload asks the discard dialog, then reads the file again. The edits
    stay until then.
36. 401: the bar shows `session-ended` and a button `session-sign-in`, which opens the sign-in form in
    a dialog (D5.2 Dialog, `h2` `signin-title`); after a successful sign-in the dialog closes, focus
    returns to Save changes, the edits are intact, nothing is resent by itself.
    ([3.3.7](https://www.w3.org/TR/WCAG22/#redundant-entry))
36a. **While the session has ended** (between the 401 and a sign-in) the banner and the nav stay as
    they were (the user's name, role and Sign out; the instance links): the page does not switch to
    signed out while the user still has edits on it. Sign out there works as 15 (with the discard
    dialog when dirty). Escape, or the dialog's **Cancel** button (`signin-cancel`, secondary,
    `type="button"`, after the Sign in button, shown only in the dialog): it closes, focus returns to
    the bar's Sign in button, the alert and the edits stay, nothing is sent. Cancel exists so a touch
    or pointer user, who has no Escape key, can leave the dialog without signing in (decided at the 1b
    review; the
    [APG modal dialog pattern](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/) recommends a
    visible button that closes the dialog in its tab sequence).
36b. **A sign-in in the dialog as another account.** The fe compares the reply with the session the
    edits were made in, by `user` only. Same `user`: 36. A different `user`: the edits belong to
    another session and are dropped. The dialog closes, the banner shows the new session (4), the page
    reads the file again (one `GET`, its states as 24) and shows it editable with the save bar, clean
    (25, 28). Above the first card a `role="alert"` paragraph reads `server-edits-dropped`, until the page is left; focus goes to the
    page's `h1`. Nothing is sent but the read. (1c states the rule for every dialog of the page: 43b.)
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification), [4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
37. By key, in the bar's alert: `client-not-authorized` (normally 403) `api-client-not-authorized`;
    `client-unavailable` (normally 503) `api-client-unavailable`; anything else of the rule above:
    `save-failed`. Edits kept in every case, the form stays dirty, still signed in.
37a. `client-no-instance` (normally 404: the instance was removed while the page was open): the bar's
    alert shows `instance-not-found` with the id. The form and its edits stay on screen (the user can
    still read what they entered) and the form stays dirty; a new Save asks again. The page is not
    replaced (that is 21, for a read).
38. **Leaving dirty**: following a nav link, the product link or Sign out opens a dialog `h2`
    `discard-title`, text `discard-description`, buttons **Keep editing** (`discard-keep`, focused
    first) and **Discard changes** (`discard-confirm`). Keep: stays, focus back on the link. Discard:
    goes on. Closing or reloading the browser tab: `beforeunload` is armed only while dirty; its
    handler calls `preventDefault()` and also sets `returnValue` to `true`, as MDN recommends for
    browsers that predate `preventDefault()` there (Chrome and Edge before 119;
    [MDN, `beforeunload`](https://developer.mozilla.org/en-US/docs/Web/API/Window/beforeunload_event)).
    A click on the link of the page already shown asks nothing (nothing would be left). The browser's
    Back and Forward and a hash typed in the address bar ask too: slice G1, 38b to 38h.
38a. **Escape in the discard dialog is Keep editing**: it closes the dialog (the native `cancel` of a
    modal `<dialog>`), nothing is discarded, the route does not change, focus returns to the control
    that opened it (the link, Sign out, or Reload of 35). The same for every dialog of the slice:
    Escape is the choice that changes nothing.
    ([2.1.2](https://www.w3.org/TR/WCAG22/#no-keyboard-trap); D5.2 Dialog)

**Slice G1: Back, Forward and a typed address while dirty** (user, 2026-11-15: they must ask too).
The technique: a `hashchange` fires only after the fragment has changed and cannot be held back
([MDN, `hashchange`](https://developer.mozilla.org/en-US/docs/Web/API/Window/hashchange_event): "fired
when the fragment identifier of the URL has changed"). So while dirty the app's `hashchange` handler
does not show the new page: it puts the address back with `history.replaceState(null, '', <the
previous hash>)`, which fires no `hashchange` of its own (same MDN page: "does not fire when the hash
is modified using `history.pushState()` or `history.replaceState()`"), and opens the discard dialog
of 38. "The previous hash" is the hash of the page the app shows (its route state), not the event's
`oldURL`, so the tests need no `HashChangeEvent` members. No string is new: the dialog and its texts
are 38's.

38b. **The question.** On a `hashchange` while the form is dirty, to a hash other than the one shown:
    `location.hash` is the instance page's again (`#/instances/<id>`, as before the change), the page
    is not remounted (same form, every edit and field error, the save bar's status and alert as they
    were), `document.title` and the nav's `aria-current` do not change, focus does not move to the
    `h1`, and the discard dialog of 38 is open with Keep editing focused.
38c. **Keep editing** (or Escape, 38a): the dialog closes, the address stays the instance page's, the
    edits stay. Focus returns to the element that had it when the `hashchange` came, if it is still in
    the document and is not `body`; otherwise to the page's `h1`. Nothing is sent.
    ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order))
38d. **Discard changes**: the app goes to the hash the user asked for (`location.hash` set to it), the
    edits are dropped, and that page shows as after any route change (2.1, 7: title, `aria-current`,
    focus on its `h1`). It does not ask a second time: the `hashchange` that this navigation fires
    passes. The same holds for 38's Discard on a link and for Sign out's return to `#/` (today these
    also set the hash while the form still reads dirty).
38e. **Clean form**: Back, Forward or a typed hash change the page as today, no dialog. In particular
    after a successful save (33, clean) and after Discard (30), Back leaves without asking. A form
    dirty again by 33b asks.
38f. **While the session has ended** (36a): the same as 38b to 38d (the edits are still on screen).
    Signed out there is no form, so nothing differs.
38g. **A second change while the dialog is open** (Back pressed again, or another hash typed): the
    address is put back again, the one dialog stays open (not a second one), focus stays in it, and
    its Discard now goes to the latest hash asked for. A dialog opened by a link (38) takes the latest
    hash too.
38h. **What is not guarded**: a hash equal to the one shown fires no `hashchange` and does nothing; a
    full address to another page (not a hash) is `beforeunload` (38).

Side effects, accepted: `replaceState` rewrites the history entry the browser moved to, so after Back
and Keep editing that entry holds the instance page's address and the entry the user left is still
there after it (Back a second time goes one entry further back than the user might expect; Forward
goes to the instance page with no change). After Discard the new page is a new entry (setting
`location.hash` pushes one), which drops the Forward entries. A typed hash and Keep editing leaves two
entries with the instance page's address. **Checked only in a real browser** (jsdom has no browser
history UI): the history entries above, that the browser's Back and Forward over same-document
entries fire `hashchange`, and that `document.activeElement` is the page's element and not the
browser's toolbar when the event arrives. The tests drive 38b to 38h by setting `location.hash` (or
`history.back()` after a `location.hash` change) while dirty.

Out of scope for G1: `popstate`, a guard on any page but the instance page (no other page has a
form), the Models page (it has no unsaved state yet), and a dirty form under another open dialog of
the page (43a, 45e): the discard dialog opens on top of it, as for a link.
39. No keyboard shortcut for Save (D8).

### 3.1 Decided at the review of 1b (2026-11-15)

Choices the coder made where the brief was silent, and the designer's ruling on each. The rulings
that changed a criterion are written into it (30a, 32, 33b, 34b, 36a, 38, 5.4); the rest are recorded
here and are part of the brief.

- **Missing or malformed `limits` for a key**: the client check of that number is skipped; the bff
  checks it (D1.2, the server decides). Accepted.
- **A blank Base URL with default clear** is sent as `null` and, after the 200, shows default ticked
  with the default read with the page (33a). Accepted: the GUI saves a blank URL as unset too
  (`server_form.rs` `form_to_config`, `opt_nonblank`).
- **API key**: sent as typed, not trimmed; a blank one (empty or only spaces) is `null` (5.4), as the
  GUI's `opt_nonblank`. Accepted, with the spaces-only case added.
- **Keep editing primary, Discard changes secondary** in the discard dialog. Accepted: the choice that
  loses nothing is the main one and is focused first (38).
- **Show uses `disabled`** while No key is ticked. Accepted: it is a dependent control (5.3), like a
  default box's input; `aria-disabled` is the save bar's rule (D5.2), where focus sits on the button
  when it turns inactive.
- **Reload and the bar's Sign in** are buttons right after the alert paragraph, before Discard. Accepted:
  the answer to the alert comes next in reading and tab order.
- **The dropped-edits alert (36b) shows while the re-read loads.** Accepted: it says at once why the
  form went away.
- **Sticky save bar and focus** (D4): the 7rem padding is too small once the bar holds an alert
  (measured at the review: 194 to 242 px at 375 px wide). The scroll padding follows the bar's
  rendered height, and below a viewport height of 32rem the bar is not sticky (D4).
  ([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum))
- **Focus after a failed Save** (31, 34) moves only after the field's error is rendered, so the focused
  control is announced with `aria-invalid` and its error description.
  ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification))
- **Discard clears the field errors as well as the alert** (30): an error describes a value that
  Discard has just thrown away. Decided at the second 1b review.
- **A number with no `limits` for its key** is not checked by the fe and is sent as typed (an integer
  when it is a whole number, else the trimmed text), with no new message: `limits` missing is a
  breach of the contract (`api-contract.md`, `GET .../server`), and the bff runs every range check
  again and answers `not-whole-number-in-range` on the field (34). Decided at the second 1b review.
- **An edit made and undone while the PUT runs** counts as an edit (33b): the controls keep what the
  user has; since it equals what was sent, the form ends clean and the status reads `save-saved` (or
  `save-saved-restart`). The controls then show the text as the user left it, not 33a's normalised
  form (`08581` stays `08581`); accepted, nothing typed is overwritten. Decided at the second 1b
  review.
- **The banner's Sign in link** has the nav links' padding and radius, so its current-page bar does
  not touch the text. Accepted at the second 1b review; the link widens by about 3 px when bold on
  `#/sign-in` (right-aligned, so its left edge moves), the banner's height does not change.
- **Back, Forward and a typed address while dirty** (was open, for the user): decided by the user on
  2026-11-15, they ask too. Built as slice G1, 38b to 38h.

## 4. Slice 1c: run controls, live state and start at boot

Rewritten by the designer on 2026-11-15 against the code on main (ee4cbe5: `InstancePage.tsx`,
`App.tsx`, `ServerSection.tsx`, `ServerForm.tsx`) and `fe/briefs/api-contract.md`. Everything here
applies to an instance page whose header is shown (2.4: the list read and the id in it). The GUI
reference: the rail's Start and Stop (`crates/config/ui/app.slint`, one button at a time) and the
Settings tab's start at boot (`crates/config/src/gui/settings_tab.rs`,
`crates/app/src/server_control.rs` `toggle_boot`).

### 4.1 The header's controls

40. **Order.** Inside the header, after the state line, in DOM order: (1) three buttons, **Start**
    (`run-start`), **Stop** (`run-stop`), **Restart** (`run-restart`), secondary style (the view's
    one primary button is Save changes, D5.2); (2) the run status, `<p role="status">`, always in the
    DOM, empty until an action; (3) the run alert, `<p role="alert">`, rendered only while it has
    text; (4) after a 401 only, a button **Sign in** (`session-sign-in`, 43a); (5) the Start at boot
    field (45). Then the Server section. The buttons are rendered for every signed-in user, both
    roles (the manager's `control`, D3).
40a. **Which buttons are inactive**, from the state the header shows:

    | State | Start | Stop | Restart |
    |---|---|---|---|
    | `active` | inactive | | |
    | `inactive`, `failed` | | inactive | inactive |
    | `activating`, `deactivating` | inactive | inactive | inactive |
    | any other string | | | |

    While an action runs (from the click until it settles, times out or fails: 42, 43) all three are
    inactive. For an unknown state the manager decides.
40b. **Inactive means `aria-disabled="true"`, never `disabled`.** A click, Enter or Space on an
    inactive button sends nothing and changes neither the status nor the alert. It stays in the tab
    sequence, and focus stays on a button that turns inactive while focused (the one just clicked,
    usually). Drawn as the save bar's inactive buttons (D5.2 Buttons). The APG recommends keeping a
    disabled control focusable with `aria-disabled` when it must stay discoverable, and MDN names "a
    button which is important to keep in the page's focus order, but its action is presently
    unavailable"; MDN also says `aria-disabled` suppresses nothing by itself, so the click handler
    must do nothing.
    ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order), [4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value);
    [APG, focusability of disabled controls](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/#focusabilityofdisabledcontrols),
    [MDN, `aria-disabled`](https://developer.mozilla.org/en-US/docs/Web/Accessibility/ARIA/Reference/Attributes/aria-disabled))
40c. Differs from the GUI on purpose: the GUI shows Start or Stop in one place and swaps them; the web
    keeps all three in place, so nothing moves under the pointer and focus never sits on a control
    that was removed. Restart exists on the web (D2.1).

### 4.2 One read of the instances, and when it runs

41. **One shared read** (asked by 2.6). The app keeps the last answer of `GET /api/instances`; the
    nav, the Overview's table and the instance header all show from it. At most one request is in
    flight: a read asked while one runs gets that read's answer, without a second request (one
    exception, the action poll's reads: 41c). A read is
    asked when the Overview opens, when an instance page opens, when a signed-in session starts (for
    the nav), and by the polls of 41b and 41c. Testable: signed in, opening `#/instances/default`
    sends exactly one `GET /api/instances` before the first poll (1a and 1b send two: the nav's and
    the page's).
41a. **What each shows.** The Overview and an instance page, when they open, show the answer of the
    read asked at opening (loading until then, as 16 and 2.6). The nav shows the latest successful
    list since the session started: none before the first one, and a later failed read keeps it
    (this refines 4: "a failed read leaves the list out" now holds only until a read succeeds). On
    the instance page, after the header is shown, every successful read that lists the id updates
    the state line and the buttons (40a); a failed read, or a list without the id, changes nothing
    on the page and shows nothing (the last state stays; a later save or action answers
    `client-no-instance` if the instance is really gone: 37a, 43). A successful list without the id
    is still a successful list for the nav, which takes it, so the nav can drop this page's link
    while the header keeps the last state (decided by the coordinator at the review of 1c, part 1).
41b. **Idle poll.** Starts when the header is first shown: one read 5 s after the previous read of
    this page answered (a chain of timeouts, not `setInterval`, so reads never overlap). It does not
    start when the page shows a message or `instance-not-found` in place of the header. It stops
    when the page is left (any route change) and when the app signs out (the page is replaced:
    2.6, "401 on the server read"). It pauses while an action runs (41c). It goes on while the
    session has ended but the page stays (36a, 43a): `GET /api/instances` needs no session and does
    not move a session's idle deadline (`crates/bff/src/lib.rs`, `router_with`: no session lookup on
    that route), so an open tab never keeps a session alive. It also goes on while the browser tab is
    hidden **(preference)**: the read is cheap and a pause adds a state for no user benefit. The
    Overview is not polled.
41c. **An action's poll replaces the idle one.** At a click on an active run button the pending idle
    timer is cancelled, and an idle read still in flight is ignored by the page (its answer updates
    neither the header nor the action; the nav may take it). While the `POST` runs, nothing is read.
    After the `POST` answers 2xx: one read 1 s later, then one 1 s after each answer, until the
    action settles (42a) or its deadline comes. Every read
    of this poll is a request sent after the `POST` answered: when its time comes while a read asked
    before that answer is still in flight (the ignored idle read, or the nav's), it waits for that
    read to answer or to be aborted at its limit (47c: at most 15 s after that read was sent, so
    always before the deadline) and then sends its own at once; the abort of the ignored idle read
    schedules no idle read (the idle poll stays paused). It never takes the older read's answer, which can show
    the state from before the action (a `failed` instance clicked Start would read
    `run-ended-failed` at once, a Restart `run-restarted`). A failed read in this poll keeps the
    last state and the poll goes on.
    **The deadline** is 60 s after the `POST`'s 2xx, whatever is pending then (an action read in
    flight): it runs on its own timer, not when the next read
    is due, since a read stuck on the bff would otherwise hold the status and the inactive buttons
    until its time limit (47) (decided by the coordinator after the tester's
    rerun of 1c, part 1). At that moment the status reads `run-timeout` and the buttons follow 40a
    for the state the header shows; an answer arriving later for that action settles nothing (the
    status stays `run-timeout`). No action read is sent at or after the deadline, nor after the
    page is left, even one that was waiting behind an older read. A click on an active button right
    after the timeout starts a new action at once, even while the old read is still pending (that
    read is then an older read for the new action, as above).
    When the action ends, the idle chain resumes with its next read
    5 s after the action ended (decided by the coordinator at the review of 1c, part 1): 5 s after
    the `POST`'s failing answer (43), 5 s after the answer of the read that settled it (42a), or, on
    a timeout, 5 s after the 60 s mark; if a request is still in flight then, that idle read shares
    it (41) rather than sending a second one.
41d. **For the tests (fake timers, `fetch` stubbed).** With instant answers: after the opening read,
    advancing 4999 ms sends nothing and 5000 ms sends one `GET`. A route change, then 10 s: no `GET`.
    A click on Stop: one `POST`, and while it is pending 10 s send no `GET`; after its 204, 999 ms
    send nothing, 1000 ms one `GET`; answered `deactivating`, the state line reads Stopping and the
    status still `run-stopping`; the next `GET` 1000 ms later, answered `inactive`, gives
    `run-stopped`, Stop and Restart `aria-disabled`, Start not; the next `GET` comes 5000 ms later,
    not 1000. Every action read answering `activating`: 59 `GET`s after the 2xx, `run-timeout` at
    60 000 ms, the next `GET` 5000 ms after that. An idle read held pending across a click and then
    answered `inactive` leaves the state line unchanged. State `failed`, an idle read held pending,
    a click on Start, its 204, 1000 ms: no second `GET` yet; the held read answers `failed`: the
    status still reads `run-starting`, and a new `GET` is sent at once; answered `active`, it gives
    `run-started`. A successful list without this page's id: the nav loses the link, the state line
    and buttons stay. A click on an inactive button sends no request at all.
    The deadline (times from the 2xx): (a) state `failed`; 5 000 ms after the opening read the idle
    read is sent and held (it never answers); a click on Start in the same tick, its 204 at once
    (so the 2xx is at 5 000 ms after opening, and the held read was sent 0 ms before it). Up to
    14 999 ms no second `GET` (the action read due at 1 000 ms waits behind the held read: 41c); at
    15 000 ms the held read is aborted (47c), the status still reads `run-starting`, and one action
    `GET` is sent at once, with no idle read scheduled by the abort. (a1) That read answers
    `active`: `run-started`, and the next `GET` is sent 5 000 ms after that answer (20 000 ms
    after the 2xx if it answers at once). (a2) Every action read held too: the read sent at 15 000
    ms is aborted at 30 000 ms, the next sent at 31 000 ms, aborted at 46 000 ms, the next sent at
    47 000 ms; at 60 000 ms the status reads `run-timeout` and Start is not `aria-disabled` (Stop and
    Restart are: 40a for `failed`); the read sent at 47 000 ms answers `active` at 60 500 ms
    (before its abort at 62 000 ms): the status still reads `run-timeout`, the state line is
    unchanged, no `GET` is sent then, and the next `GET` is sent at 65 000 ms. In both, no action
    is settled by the held idle read, and `run-timeout` comes only from the 60 s timer. (b) Every action read never
    answers: replaced by 47e (reads aborted at 15 s, so new `GET`s go on); a click on Start right
    after 60 000 ms sends one `POST` and the status reads `run-starting`. (c) An idle read held pending, a
    click on Stop, its 204, 1000 ms (the action read waits behind the held read), a route change,
    the held read answers: then 10 s send no `GET`.

### 4.3 What an action shows

42. A click on an active button sends `POST /api/instances/<id>/start` (or `/stop`, `/restart`), no
    body. At the click: the run alert and its Sign in button go, and the status reads `run-starting`,
    `run-stopping` or `run-restarting`, kept through the poll. Any 2xx is accepted, whatever its body
    (the contract answers 204).
42a. **When an action settles**, from the state a read gives:

    | Action | Settles on | Status |
    |---|---|---|
    | Start | `active` | `run-started` |
    | Restart | `active` | `run-restarted` |
    | Stop | `inactive` | `run-stopped` |
    | any | `failed` | `run-ended-failed` |

    Any other state keeps the poll going, including the state the instance had before the click: the
    service manager may not have run the job when the `POST` answers (systemd's `StartUnit` queues a
    job: `crates/manager/src/manager/dbus.rs`, `SystemdService`). No settled state by the deadline,
    60 s after the `POST`'s 2xx whatever is pending (41c): `run-timeout`, and an answer arriving
    later for that action changes it no more. Focus stays on the clicked button throughout.
42b. **What is announced.** The outcome stays in the status until the next click on a run button or
    until the page is left. Only the run status is a live region; the state line is not, so state
    changes seen by the idle poll (another user, a crash) show without being announced
    **(preference)**: a poll every 5 s would otherwise speak over the user's work.
    ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
42c. **Known limit of Restart on Linux**: a read 1 s after the 204 can still see the old process
    `active`, and the status then reads `run-restarted` before the restart happened. On Windows the
    manager's restart waits for the stop before it answers (`crates/manager/src/manager/windows/scm.rs`,
    `restart`). Removing it needs the bff to answer after the job ran (a question in the report, not a
    criterion).

### 4.4 Errors of an action

43. **By key, never by status, except 401** (contract, "Rules for every route"). The message goes in
    the run alert, the status is emptied, the buttons follow 40a for the last state, focus stays on
    the clicked button, and the idle poll resumes (41c):
    - 401, whatever the body: `session-ended-retry` and the Sign in button (43a);
    - `client-not-authorized`: `api-client-not-authorized`;
    - `client-no-instance`: `instance-not-found` with the id;
    - `client-unavailable`: `api-client-unavailable`;
    - `client-service-failed`: `api-client-service-failed` (proposed for the contract, see the
      report);
    - anything else (another key, no key, a body that is not JSON, a network failure):
      `request-failed`. Never the body's text, and no `api-other` (that is for a 422 only).
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification), [4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
43a. **401** is 36a's state: the banner, the nav and the page stay, the Server form and its edits are
    untouched. Sign in opens the same sign-in dialog as 36 (`h2` `signin-title`, Cancel). Escape or
    Cancel: it closes, focus returns to this Sign in button, the alert stays, nothing is sent.
    Nothing is resent after a sign-in: the user clicks again.
    ([3.3.7](https://www.w3.org/TR/WCAG22/#redundant-entry), [2.1.2](https://www.w3.org/TR/WCAG22/#no-keyboard-trap))
43b. **A sign-in in any of the page's dialogs** (the save bar's, 36; the run alert's, 43a; Start at
    boot's, 45e) follows one rule, comparing the reply's `user` with the session the page had:
    - same `user`: the dialog closes; every session-ended alert of the page
      (save bar, run, boot) goes with its Sign in button; focus goes to the control whose request got
      the 401: Save changes (36), the run button clicked (43a), the Start at boot checkbox (45e). The
      edits are intact.
    - otherwise: the banner shows the new session (4); every session-ended alert goes; the Server
      section reads the file again (24, 25), with `server-edits-dropped` above it only when the form
      was dirty (36b); the Start at boot checkbox keeps its value, without a new read; focus goes to
      the page's `h1`.

### 4.5 With the Server form

44. **Run actions never touch the form**: the draft, the dirty state, the field errors and the save
    bar's alert stay as they are, and no discard dialog asks (nothing is left). Stop and Restart with
    unsaved edits are allowed: the save bar's `save-unsaved` already says the running server does not
    have them **(preference)**.
44a. **"Saved: restart llama-server to apply." ends with an action.** When an action started after
    that save settles (any row of 42a, `failed` included), the save bar's remembered
    `save-saved-restart` ends: the status shows what a clean form shows, empty (30a), not
    `save-saved`, and a later undo of an edit does not bring it back. Empty rather than "Saved." so a
    screen reader hears only the run's outcome: a live region announces additions and text changes,
    not removals, by default (`aria-relevant` default `additions text`,
    [WAI-ARIA 1.2](https://www.w3.org/TR/wai-aria-1.2/#aria-relevant)). On a timeout or an error it
    stays.
44b. **No confirmation** for Stop or Restart, as the GUI's Stop. With Save on shutdown on, the bff
    saves the snapshots before it stops (contract); the fe shows only `run-stopping`. Pending
    question for the user (report).

### 4.6 Start at boot

45. **The field**: D5.2 Field with a checkbox (the `CheckboxField` of `Field.tsx`): group named
    `boot-label`, the hint toggle "About Start at boot" with `boot-hint`, the checkbox `boot-toggle`.
    It is not part of the Server form: it never makes the form dirty, Discard and Save ignore it,
    `beforeunload` ignores it (it is the service manager's state, written at once, as the GUI's
    Settings tab).
45a. **Read**: `GET /api/instances/<id>/boot` once, when the header is first shown (beside the Server
    read); not polled. Until it answers the field is not rendered: a checkbox showing a state not yet
    known would state a false value ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)). A 2xx
    whose body is `{"enabled": <boolean>}`: the checkbox, checked when `enabled` is true. Anything
    else: the group with its legend and hint toggle and, in place of the checkbox, a plain paragraph
    (not an alert: it comes with the page) by key: `client-unavailable` `api-client-unavailable`;
    `client-not-authorized` `api-client-not-authorized`; anything else (a 2xx of another shape,
    `client-no-instance`, a network failure) `boot-load-failed`. No retry button: opening the page
    again reads again.
45b. Removed (2026-11-15): the checkbox is enabled for every signed-in session.
45c. **Write**: a change shows the new value at once and sends
    `PUT /api/instances/<id>/boot` `{"enabled": <new value>}`. At the change the field's status and
    alert are emptied. While the `PUT` runs the checkbox is `aria-disabled="true"`: a click or Space
    changes nothing and sends nothing (the native toggle is prevented), focus stays on it. No text
    while it runs **(preference)**: one quick call, and the box already shows the value asked for.
    **Drawn** as a disabled checkbox (D5.2, "Disabled controls stay readable": box border and tick in
    `--muted` on `--bg`), cursor default, and under `@media (forced-colors: active)` border and tick
    in `GrayText`, as the inactive buttons (D5.2 Buttons). Never with `opacity` on the input: it
    dims the focus ring too, to 2.8:1 on `--bg` in light at 0.6, under the 3:1 floor of D5.1
    ([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast), [2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible))
    (added at the review of 1c part 2, where the coder had to choose).
45d. **Success**: a 2xx whose body is `{"enabled": <boolean>}`: the checkbox takes the reply's value;
    the field's status reads `boot-on` or `boot-off` by that value. A 2xx of another shape is a
    failure (45e, `request-failed`).
45e. **Failure**, by key, in the field's alert: 401 `session-ended-retry` and a Sign in button
    (the dialog of 43a; Cancel or Escape returns focus to this button; a sign-in follows 43b);
    `client-not-authorized` `api-client-not-authorized`; `client-unavailable`
    `api-client-unavailable`; `client-no-instance` `instance-not-found` with the id;
    `client-service-failed` `api-client-service-failed`; anything else `request-failed`. The
    checkbox then shows the state read again with `GET .../boot` (still `aria-disabled` until that
    answers), or, when that read fails, the value it had before the change; the status stays empty.
    As the GUI, which shows `boot_enabled()` read again after a failed toggle (`toggle_boot`).
45f. **Where its messages are**: inside the group, after the checkbox: the status `<p role="status">`
    (always in the DOM), the alert `<p role="alert">` (only while it has text), then the Sign in
    button after a 401. Both texts go at the next change of the box or when the page is left; the
    alert also at a sign-in (43b).
45g. Differs from the GUI on purpose: on the instance header, not in Settings (D2.1); a failed
    read says so, where the GUI shows the box off (`settings_tab.rs`: `boot_enabled().unwrap_or(false)`), which would state a false value.

### 4.7 Narrow width and keyboard

46. **Layout** (checked in review): the three buttons in one row that wraps, gap `--space-2`, each at
    least 24 by 24 CSS px ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)); at 375 px they
    fit on one line, at 320 px they may wrap, with no horizontal scroll
    ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)). The run messages take the full width under them,
    the status starting `--space-2` below the buttons (as the save bar's gap), so a one-line status
    does not touch them.
    The boot field follows D4's form layout (label above the control below 720 px). A focused header
    control is never under the sticky save bar (D4 scroll padding,
    [2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)).
46a. **Tab order**: `h1` (programmatic focus only), Start, Stop, Restart (inactive ones included),
    the run alert's Sign in when shown, About Start at boot, the Start at boot checkbox, its Sign in
    when shown, then the Server section (section 6). Inactive buttons and the boot checkbox while its
    write runs stay in the sequence (40b, 45c).
    ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order), [2.1.1](https://www.w3.org/TR/WCAG22/#keyboard))

### 4.8 A time limit for every request

Today `call()` (`fe/src/api.ts`) waits for ever: a bff stuck on the manager leaves a read, a save, a
run action's `POST`, the boot read or write or a sign-in pending for good, and after `run-timeout`
the idle poll shares the pending read for good (41c, 41d (b)). These criteria end that.

47. **Two limits, by method, in `call()` only** (no caller sets its own):
    - a `GET`: **15 s**. The bff gives up on the manager after 5 s for every read it serves today
      (`crates/bff/src/lib.rs`, `MANAGER_TIMEOUT`), so a read that has not answered in 15 s is stuck,
      not slow. 15 s is also well inside the action deadline (60 s, 41c), so a stuck action read
      leaves room for later ones.
    - any other method (`POST`, `PUT`, `DELETE`: sign-in and sign-out, a save, a run action, the boot
      write): **120 s**. A stop or a restart with Save on shutdown on answers only after the bff has
      saved the slot snapshots (contract, "Start, stop, restart, boot"), several GiB per slot;
      how long that takes is not measured, 120 s is an assumption (report).
    The time runs from the call until the body has been read, not only the headers.
47a. **How**: an `AbortController` per call, aborted by a `setTimeout` that is cleared when the body
    has been read. Not `AbortSignal.timeout()`: it "does not provide a way to cancel its timeout", and
    MDN recommends `AbortController` with `setTimeout()`/`clearTimeout()` when the timeout must be
    cancelled early ([MDN, `AbortSignal.timeout()`](https://developer.mozilla.org/en-US/docs/Web/API/AbortSignal/timeout_static));
    `abort()` aborts the fetch and the consumption of its body
    ([MDN, `AbortController.abort()`](https://developer.mozilla.org/en-US/docs/Web/API/AbortController/abort)),
    and `setTimeout` is what the tests' fake timers drive. The controller is aborted with a
    `DOMException` named `TimeoutError` (the name `AbortSignal.timeout()` itself uses, same MDN page),
    so `call()` rejects with it, and a caller can tell a timeout from an unreachable service by
    `name === 'TimeoutError'`. The request is gone: no late answer reaches the page.
47b. **A timed-out call is that call's network failure**: the page shows exactly what it shows today
    when `call()` rejects, with no new string except the save's (47d):
    - sign-in (14): `signin-failed`;
    - the Overview's instances read (16): `instances-failed`; the version read: `version-unavailable`;
    - the server read (24): `server-load-failed`;
    - a run action's `POST` (43): `request-failed` in the run alert, the status emptied, the buttons
      per 40a for the last state, the idle poll resumes 5 s later (41c), which then shows the real
      state (the job may have run);
    - the boot read (45a): `boot-load-failed`; the boot write (45e): `request-failed`, then the state
      read again (45e), which shows whether the write happened.
47c. **The polls**: a timed-out idle read is a failed read (41a): the last state stays, nothing is
    shown, the next idle read goes 5 s after the abort. A timed-out action read is a failed read of
    the action's poll (41c): it settles nothing, the next action read goes 1 s after the abort, and
    the 60 s deadline is unchanged (it does not wait for the limit). A read asked while one runs still
    shares it (41) until that one answers or is aborted; a share never outlives the limit.
47d. **A save that times out** (`PUT .../server`): the file may or may not have been written, so
    `save-failed` ("Not saved") could be false. The bar's alert shows `save-timeout` (the one new
    string, 5.7); everything else is 37: edits kept, the form stays dirty, the revision the page holds
    does not change, still signed in. If the first write did land, the next Save answers 409
    `server-changed-elsewhere` and 35 takes over (Reload shows the file, the user's own values in it).
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification): the error is described as it is
    known; [4.1.3](https://www.w3.org/TR/WCAG22/#status-messages): it is in the bar's `role="alert"`)
47e. **For the tests (fake timers, `fetch` stubbed with a promise that never settles unless the
    signal aborts; the stub rejects with the signal's `reason`)**:
    - a `GET` held: at 14 999 ms not aborted, at 15 000 ms its signal is aborted with a `TimeoutError`;
      a `PUT` held: at 119 999 ms not aborted, at 120 000 ms aborted. A reply at 1 000 ms: no abort
      later (the timer was cleared).
    - idle poll: the read after the opening one held: at 15 000 ms after it was sent it is aborted,
      the state line unchanged, nothing shown; 5 000 ms later one new `GET`.
    - 41d (b) now reads: every action read held; `GET`s at 1 000, 17 000, 33 000 and 49 000 ms after
      the 2xx; at 60 000 ms `run-timeout`, buttons per 40a; the read sent at 49 000 ms is aborted at
      64 000 ms; at 65 000 ms one new idle `GET` is sent (no longer a shared one).
    - a save held to 120 000 ms: the bar's alert reads `save-timeout`, the form stays dirty, the next
      Save sends the same revision; a run `POST` held to 120 000 ms: `request-failed`, the status
      empty, a `GET` 5 000 ms later; sign-in held: `signin-failed`, the button usable again.

**Out of 1c**: logs, per-action progress, "Open chat UI", polling the Overview or start at boot, a
retry button for a failed boot read, guarding Back and Forward (3.1, still open).

## 4b. Later slice B1: a long user name in the banner (reviewed 2026-11-15, not built)

Found at the review of R4; it predates R4. The signed-in span in `.session` has no break rule, so a
user name with no break point keeps its full width and can push the page wider than the viewport.
`useradd` takes user names of up to 32 characters ([useradd(8), CAVEATS](https://man7.org/linux/man-pages/man8/useradd.8.html));
testing at that length is a preference, since names made by other means can be longer. One CSS rule, no new string, no new token:
`overflow-wrap: anywhere` on the signed-in span, as `.nav a` and `td` already have.

46b. **Long user name** (checked in review, rendered: jsdom does no layout): signed in as a
    32-character user name without a break point (e.g. `abcdefghijklmnopqrstuvwxyz012345`), at a
    viewport 320 CSS px wide, the page does not scroll sideways; the name wraps inside the banner and
    Sign out stays visible and reachable by Tab
    ([1.4.10 Reflow](https://www.w3.org/TR/WCAG22/#reflow)). The tester may pin the rule only by a
    static check of `fe/src/index.css`, if one is wanted; the review renders it.

Out of scope: truncating the name with an ellipsis (it would hide part of who is signed in).

## 4c. Later slice S1: the real bff's new answers, and a session ended by the manager

Written 2026-11-16 against the bff on main (2d69d2f and after: `crates/bff/src/control.rs`,
`server.rs`, `origin.rs`) and `fe/briefs/api-contract.md` as updated the same day. No new pattern,
component or token: every message goes in an alert the page already has, and the session-ended
state of 36a, 43a, 45e and 2c's 49 is reused as it is. **One new string** (S1.10).

**The signal for "removed from `pai`"** (designer's decision, for the bff's coordinator). User
decision (2026-11-16): when the manager refuses the session's user, the bff ends the session. The fe
asks the bff to say so with its own key: **403 `session-not-administrator`** on the session routes,
the key the sign-in already sends for the same fact (contract, "Rules for every route"), and
`client-not-authorized` only for a refusal after which the session lives on. Why not "on a 403
`client-not-authorized`, read `GET /api/session` again": that is a second request at every refusal
place (seven), with its own failures to handle (a 503 or a time-out on the re-read says nothing
either way); and a refusal that is not about the user (the bff's own account lacking a right on a
shipped install, before a later bff slice) must keep the plain `api-client-not-authorized` and the
session, which only the bff knows. The fe picks by key (contract), so one key per meaning is the
smallest correct design. Not a cookie clear: the fe never reads the cookie (it is HttpOnly).
**Until the bff sends the key** nothing in S1.3 to S1.9 triggers: the page shows
`api-client-not-authorized` as today and meets the 401 at its next request (36, 43a, 45e, 49),
correct but one step late.

S1.1. **The guard's keys and `boot-malformed`** (`request-cross-origin` 403, `request-bad-host` 403,
    `payload-too-large` 413, `boot-malformed` 400, each `args: []`) are the generic failure of the
    place where they land, with no new string: run actions `request-failed` (43); the boot write
    `request-failed`, then the state read again (45e); the Server save `save-failed`, edits kept
    (37); the preset switch `request-failed`, then the row read again (2c's 48); sign-in
    `signin-failed` (14); the reads: `server-load-failed` (24), `presets-load-failed` (2a's 17),
    `preset-load-failed` (2d's 66), `boot-load-failed` (45a), `instances-failed` on the Overview.
    Why no message for `request-bad-host` (asked by the coordinator): the fe can never show it.
    The guard stands in front of the whole router, `/` and `/assets/*` included
    (`crates/bff/src/lib.rs` `router_with_session`, the `origin::guard` layer), so a page opened
    under a host name alias gets the 403 JSON body in place of the page and no fe code runs. Telling
    that user to open `http://localhost:<port>/` is the bff's own answer to a `GET /` with a bad
    `Host` (report). Tests: one per row is enough for run, boot write, save and sign-in.
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification))
S1.2. **`client-service-failed` with `args: []`** shows `api-client-service-failed` as it is (it has
    no `{N}`), in the run alert (43) and the boot alert (45e); no placeholder text is left.
S1.3. **Which answer means "signed out by the manager"**: 403 with the key
    `session-not-administrator` from any route but `POST /api/session`. By key, as every rule
    (contract); a 403 `client-not-authorized` keeps today's handling in every place, the session
    taken as alive. The session-free reads (`GET /api/instances`, `GET .../boot`) never send it; if
    one did, it is that read's generic failure (S1.1's list).
S1.4. **Run controls** (43): exactly the 401 row of 43 and 43a, with `session-revoked` in the run
    alert in place of `session-ended-retry`: the status emptied, the buttons per 40a, focus stays
    on the clicked button, the **Sign in** button after the alert, the idle poll resumes (it needs
    no session).
S1.5. **Start at boot write** (45e): the 401 row, with `session-revoked` in the field's alert; the
    box shows the state read again with `GET .../boot` (a session-free read, so it still answers);
    focus stays on the checkbox; the Sign in button after the alert.
S1.6. **Server save** (36, 36a): as a 401, with `session-revoked` in the bar's alert in place of
    `session-ended`: the edits stay, the form stays dirty and the status reads `save-unsaved` (30a),
    focus stays on Save changes, the bar's Sign in button follows the alert. The banner and nav stay
    as they were (36a); Sign out works as 15 with the discard dialog when dirty.
S1.7. **Preset switch** (2c's 48, 49): as the 401 row, with `session-revoked` in the page's alert: the
    switch shows its value before the change, nothing is read again, focus stays on the switch, the
    Sign in button after the alert.
S1.8. **The Sign in dialog** opened from any of S1.4 to S1.7 is 43a's, unchanged: Escape or Cancel
    returns focus to that Sign in button with the alert kept; a sign-in follows 43b (and 2c's 49).
    Signing in again as the same account gets 403 `session-not-administrator` from
    `POST /api/session`, which is 14b inside the dialog: `api-session-not-administrator`, password
    cleared, focus on the user name field, the dialog stays open. Another administrator: 43b's other
    branch (the Server form reads again, `server-edits-dropped` when it was dirty). One test pins
    the same-account path.
S1.9. **On a read** (the server read 24, the presets list 2a's 16, the preset page 2d's 66): the
    whole app becomes signed out, exactly as a 401 on that read (2.6), except that the paragraph
    above the sign-in fields reads `session-revoked`, as a `role="alert"` (14's alert paragraph), in
    place of `signin-required`. It stays until a sign-in succeeds or the route changes; focus does
    as for a 401 read. Nothing is lost: these reads run only when nothing is unsaved (35's Reload
    asks first). After a sign-in by another administrator the page reads again (as 2.6).
    ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages), [3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion))
S1.10. **Strings, slice S1.** New: `session-revoked` | You have been signed out: this account is no
    longer a member of the pai group. Ask whoever administers this computer to add it again; you can
    still see the overview without signing in. (Not `api-session-not-administrator` itself: that one
    speaks of signing in, and a user who was signed in must first hear that they no longer are,
    [3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion); `pai` left literal as in 14b.) Reused,
    unchanged: `request-failed`, `save-failed`, `signin-failed`, `server-load-failed`,
    `presets-load-failed`, `preset-load-failed`, `boot-load-failed`, `instances-failed`,
    `api-client-service-failed`, `api-session-not-administrator`, `session-sign-in`.

**Decided, no criterion.**
- **Snapshots at a web Stop or Restart** (the bff does not save them yet, contract): the fe says
  nothing. The gap is the bff's and temporary: `PLAN.md` step 9b moves the save into the instance's
  own stop before the MVP's run (user, 2026-11-16), and today a `systemctl stop` skips it as well. A
  note would be a string added now and removed at 9b, on a page only administrators open
  **(preference)**. 44b is unchanged.
- **The idle deadline** (`GET .../boot` does not move it, the writes do): nothing for the fe, which
  keeps no idle timer of its own; the next request after 30 minutes is a 401 as before.

**Out of S1**: a message for `request-bad-host` served by the bff on `GET /` (bff side, report); an
explicit sign-out of the banner on S1.4 to S1.7 (36a keeps the page as it was, whatever ended the
session).

## 4d. Later slice S2: a Stop or Restart that saves snapshots for minutes

Written 2026-11-16 against bff slice 9b (contract, "A long stop"). Today a Stop or Restart with
`SaveStateOnShutdown` on lasts up to 300 s in `deactivating`, but the action's deadline is 60 s
(41c; `ACTION_LIMIT_MS`, `fe/src/InstancePage.tsx` line 38): at 60 s the status reads `run-timeout`
("did not reach the expected state within a minute"), which reads as a failure while the stop is
going as planned. The state line (`instance-state-deactivating`, Stopping), the inactive buttons
(40a) and the rest of the page are already right. No new pattern, component or token.

S2.1. **Deadline per action.** The deadline of 41c is 360 s after the `POST`'s 2xx for Stop and
    Restart (the 300 s stop budget plus Start's 60 s), and stays 60 s for Start. Everything else in
    41c, 41d and 47 is unchanged (the action poll still reads every 1000 ms).
S2.2. **Check (Stop):** every action read answering `deactivating`: 359 `GET`s after the 2xx; at
    60 000 ms and at 359 999 ms the status still reads `run-stopping`, all three buttons are
    `aria-disabled`; at 360 000 ms the status reads `run-timeout-stop`, the buttons follow 40a, the
    next `GET` 5000 ms later. A read answering `inactive` at 240 000 ms gives `run-stopped`.
S2.3. **Check (Restart):** reads answering `deactivating` to 200 000 ms, `activating` to 290 000 ms,
    then `active`: `run-restarting` throughout, then `run-restarted`; no `run-timeout*` at 60 000 ms.
S2.4. **Check (Start):** unchanged, `run-timeout` at 60 000 ms (41c).
S2.5. **Announcement**: the status stays one `role="status"` region; the text changes only at the
    outcome or the deadline, so nothing is announced each second
    ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)). Focus as 40b.
S2.6. **Strings, slice S2.** New: `run-timeout-stop` | llama-server did not reach the expected
    state within six minutes. The state above keeps updating. Reused: `run-stopping`,
    `run-restarting`, `run-stopped`, `run-restarted`, `run-timeout`, `instance-state-deactivating`.

S2 replaces S1's "Snapshots at a web Stop or Restart": the save now happens, and the fe says nothing
more than Stopping **(preference)**: the Save on shutdown hint (`server-snapshots-intro`) already
says stopping waits for the write. **Out of S2**: a status that clears when an idle read later shows
the expected state after a timeout (true of Start today as well).

## 4e. Later slice S3: the save's answers as the bff will send them

Written 2026-11-16 against `ARCHITECTURE.md`, "Saving the server file from the web" (contract,
`PUT .../server`). The save of 1b already sends what the design reads; two answers read wrong.
`ServerForm.tsx` `refusalText` and `fallbackField` (lines 296 to 331) do not know
`bind-all-with-others`, so a hand-edited file listing `0.0.0.0` with another address (which
`fromValues` loads as is) is refused at every save as "The request was refused
(bind-all-with-others)." with no field marked. And since the revision does not survive a bff
restart, the 409 text "changed elsewhere" can name a change that did not happen. No new pattern,
component or token.

S3.1. **Text.** A 422 `{key: "bind-all-with-others", args: [], field: "Hostname"}` shows
    `save-refused` filled with `api-bind-all-with-others` in the bar's alert.
S3.2. **Field.** The same answer marks Bind to (`Hostname`) with `api-bind-all-with-others` as its
    error and moves the focus to it, as `client-unresolvable-host` does (34b); without `field` the
    fallback is `Hostname` too.
S3.3. **Edit clears it.** Changing a Bind to row removes the error, as for every field (33a).
S3.4. **409 text.** A 409 `server-changed-elsewhere` shows the new English text below, with Reload,
    as before; nothing else of the 409 changes.
S3.5. **Announcement**: unchanged: the alert is `role="alert"`
    ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)), the marked field names its error
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification)).
S3.6. **Strings, slice S3.** New: `api-bind-all-with-others` | Hostname lists 0.0.0.0 with other
    addresses; 0.0.0.0 already covers every address, so bind it alone. (verbatim from
    `crates/core/strings/en_US.toml` line 1010). Changed: `api-server-changed-elsewhere` | These
    settings changed elsewhere, or the web service restarted, after you opened them. Reload to see
    them; your edits here will be lost.

**Out of S3**: a client check for `0.0.0.0` with other rows (the rows already prevent making one);
`server-malformed` (400) and `client-other` (502), which keep `save-failed`.

## 5. The Server form

### 5.1 Cards and fields, in order

Same cards and order as the GUI. Card headings are `h3`; the SDK tuning groups are `h4` (`CUDA`,
`ROCm` and `llama.cpp` are names, written as data; "CUDA and ROCm" is `server-cuda-and-rocm`).

| # | Card | Field label (key) | Control | Accessible structure | INI key |
|---|---|---|---|---|---|
| F1 | Network (`server-network`) | Port (`server-port`) | default box + text, numeric | group "Port": checkbox `field-default`, textbox "Port" | `Port` |
| F2 | | Bind to (`server-bind-to`) | checkbox list | group "Bind to": one checkbox per row (5.2) | `Hostname` |
| F3 | | Base URL (`server-base-url`) | default box + text | group: checkbox `field-default`, textbox | `OpencodeBaseUrl` |
| F4 | | API key (`server-api-key`) | No key box + secret + Show | group: checkbox `field-no-key`, password input labelled "API key" (found by label: a password input has no ARIA role), button `field-show` | `OpencodeApiKey` |
| F5 | Resources (`server-resources`) | CPU threads (`server-cpu-threads`) | auto box + text, numeric | group: checkbox `field-auto`, textbox | `Threads` |
| F6 | | Batch threads (`server-batch-threads`) | auto box + text, numeric | same | `ThreadsBatch` |
| F7 | | Max models (`server-max-models`) | default box + text, numeric | group: checkbox `field-default`, textbox | `ModelsMax` |
| F8 | | Memory margin (`server-memory-margin`) | default box + text, numeric, unit MiB after it | same | `ModelsMemoryMargin` |
| F9 | | Models folder (`server-models-folder`) | text | label + textbox, placeholder 5.2 | `ModelsDir` |
| F10 | Advanced (`server-advanced`) | Web UI (`server-web-ui`) | checkbox `server-web-ui-toggle` | group "Web UI": the checkbox | `Webui` |
| F11 | | Web UI MCP proxy (`server-mcp-proxy`) | checkbox `server-mcp-proxy-toggle` | group | `WebuiMcpProxy` |
| F12 | | Log level (`server-log-level`) | select | combobox "Log level" | `LogVerbosity` |
| F13 | Conversation snapshots (`server-snapshots`, intro `server-snapshots-intro`) | Save on shutdown (`server-save-on-shutdown`) | checkbox `server-save-on-shutdown-toggle` | group | `SaveStateOnShutdown` |
| F14 | | Snapshot folder (`server-snapshot-folder`) | text | label + textbox | `StateDir` |
| F15 | SDK tuning (`server-sdk`, intro `server-sdk-intro`); group CUDA: paragraph `server-cuda-nothing`; group ROCm | hipBLASLt (data) | select default/on/off | combobox "hipBLASLt" | `RocblasUseHipblaslt` |
| F16 | | Profiler attach (`server-profiler-attach`) | checkbox `server-profiler-attach-toggle` | group | `RocprofAttach` |
| F17 | group CUDA and ROCm | Tensor-split AllReduce (`server-allreduce`) | select default/nccl/internal/none | combobox | `CudaAllreduce` |
| F18 | | Disable GPU graphs (`server-gpu-graphs`) | checkbox `server-gpu-graphs-toggle` | group | `CudaDisableGraphs` |
| F19 | group llama.cpp | KV rotation (`server-kv-rotation`) | checkbox `server-kv-rotation-toggle` | group | `AttnRotDisable` |

Every field also has its hint toggle (`field-about`) and hint `<field key>-hint` (5.5).

### 5.2 What each control shows from the file

- **F1, F7, F8**: value `null` → box ticked, textbox disabled showing `defaults.<key>` (9931, 4, 0).
  A number → box clear, textbox shows it. Unticking brings back the text held before ticking, the
  default shown for a field loaded on default (28a).
- **F3**: `null` → box ticked, textbox disabled showing `defaults.OpencodeBaseUrl` (computed by the
  bff from the saved file; it does not follow unsaved edits of Port or Bind to, said in the hint).
- **F4**: `null` or empty → No key ticked, password box disabled and empty. Else No key clear, the key
  masked. A withheld document: 27d.
- **F5, F6**: `null` → auto ticked, textbox disabled and empty. A number → shown. Unticking leaves
  the textbox empty and focused. Differs from the GUI on purpose: a text field, not a slider (exact
  entry, works on touch, the same Field as the rest); same range, 1 to the server's CPU count.
- **F2**: rows in this order: `localhost` labelled `server-bind-localhost`; `0.0.0.0` labelled
  `server-bind-all`; each value of `Hostname` matching no row, labelled `server-bind-gone` with the
  value; then `interfaces` with their labels as data. A row is checked when its value is in the
  comma list (case-insensitive). `null` or empty checks localhost. While `0.0.0.0` is checked every
  other row is disabled (as `net_ifaces::build_rows`).
- **F9, F14**: `null` → empty textbox with placeholder `server-folder-instance-default` filled with
  `defaults.ModelsDir` / `defaults.StateDir`. The fe never fills in a folder itself (D1.3). A value →
  shown.
- **F12**: options `OUTPUT:0`, `ERROR:1`, `WARN:2`, `INFO:3`, `TRACE:4`, `DEBUG:5` (data); value
  `n` selects option `n`, clamped to 0..5 as the GUI does (`server_form.rs` `log_level_label`);
  `null` selects `TRACE:4`.
- **F15**: options `option-default`, `option-on`, `option-off`, values `default`/`on`/`off`; `null`
  is `default`.
- **F17**: options `option-default`, `nccl` (data), `option-internal`, `option-none`; `null` or an
  unknown value is `default` (as the GUI).
- **Checkboxes**: `true` checked; `null` or `false` clear.

**An unset switch shows the configurator's default, not "off"** (found by the bff's tester,
2026-11-15): `Webui` unset (`null`, a file with no `Webui` line, as the shipped
`server@default.ini`) shows the Web UI checkbox ticked, as the GUI does (`webui_or_default` in
`crates/core/src/server_cfg.rs`); every other switch is off when unset. A file with `Webui` unset
loads clean, and a save then sends `true`.

### 5.3 Dependencies (1b; in 1a everything is disabled anyway)

- F1, F3, F7, F8: textbox enabled only while its default box is clear.
- F4: password box and Show enabled only while No key is clear.
- F5, F6: textbox enabled only while auto is clear.
- F2: checking `0.0.0.0` unchecks every other row and disables them; unchecking it re-enables them
  and leaves localhost checked. Checking another row unchecks `0.0.0.0`. Unchecking the last checked
  row checks localhost (as `net_ifaces::toggle`). So at least one row is always checked.
- F11 enabled only while F10 is checked; F14 enabled only while F13 is checked. Disabled, the value
  is kept and saved (as the GUI).

### 5.4 Client checks at Save, and what is sent

| Field | Check (only when the field is enabled and not on default/auto/No key) | Error text |
|---|---|---|
| F1, F7, F8 | trimmed text is a whole number (optional leading `-`, digits) within `limits.<key>` | `field-error-number` with min and max; when max is 2147483647, `field-error-number-min` with min |
| F5, F6 | same, `limits.Threads` / `limits.ThreadsBatch` | same |
| F3, F4, F9, F14 | contains no `;` and no `#` (`checks::comment_markers`) | `field-error-comment-markers` |

A number check is skipped when `limits` gives no `[min, max]` pair of numbers for its key (3.1).

Sent values: default/auto/No key ticked → `null`; numbers as integers; F2 as the checked values joined
by `,` in row order; F3 trimmed, then every trailing `/` removed, then a trailing `/v1`, then every
trailing `/` again (as the GUI's `form_to_config`: `https://gw.example.com/v1/` and `…//` both
become `https://gw.example.com`), blank → `null`; F4 as typed (not trimmed), blank (empty or only
spaces) → `null`; F9, F14 trimmed, empty → `null`; F12 the level number;
F15, F17 `null` for `default`, else the value; checkboxes booleans. Dirty (29) compares these sent
values with those of the loaded file.

### 5.5 Strings, slice 1a (shell, sign-in, Overview, header, read-only form)

Hints marked "adapted" differ from the GUI's text where it was wrong on the web; the rest are copied
(`crates/core/strings/en_US.toml`, the key in brackets). `\n` is a paragraph break.

| Key | English |
|---|---|
| `app-name` | Private AI Framework |
| `page-title` | {0} \| Private AI Framework |
| `skip-to-content` | Skip to content |
| `nav-label` | Main |
| `nav-menu` | Menu |
| `nav-overview` | Overview |
| `nav-instances` | Instances |
| `loading` | Loading… |
| `not-found` | There is no page at this address. |
| `not-found-home` | Go to the overview |
| `session-sign-in` | Sign in |
| `session-sign-out` | Sign out |
| `session-signed-in-as` | Signed in as {0} |
| `signin-title` | Sign in |
| `signin-intro` | Use the user name and password of your account on the computer that runs Private AI Framework. Only members of its pai group can sign in. |
| `signin-required` | Sign in to see this page. |
| `signin-username` | User name |
| `signin-password` | Password |
| `signin-submit` | Sign in |
| `signin-busy` | Signing in… |
| `signin-empty-username` | Enter your user name. |
| `signin-empty-password` | Enter your password. |
| `signin-failed` | Signing in failed. Try again; if it keeps failing, the web service may not be running. |
| `api-session-bad-credentials` | The user name or password is not correct. |
| `api-session-too-many-attempts` | Too many attempts. Try again in {0} seconds. |
| `api-session-unavailable` | Signing in is not available on this server right now. You can still see the overview without signing in. |
| `overview-title` | Overview |
| `instances-column-engine` | Engine |
| `instance-engine` | {0} instance |
| `instance-state` | State: {0} |
| `instance-not-found` | There is no instance named {0}. |
| `server-title` | Server |
| `server-intro` | llama-server settings of this instance, shared by every model it serves ({0}). Per-model settings belong to the presets. (adapted from `server-intro`) |
| `server-loading` | Loading the server settings… |
| `server-load-failed` | The server settings could not be read. |
| `api-client-not-authorized` | Not authorized: the system refused this request. |
| `api-client-unavailable` | The Private AI manager service is not running or not reachable. |
| `field-default` | default |
| `field-auto` | auto |
| `field-no-key` | No key |
| `field-about` | About |
| `field-show` | Show |
| `option-default` | default |
| `option-on` | on |
| `option-off` | off |
| `option-internal` | internal |
| `option-none` | none |
| `server-network` | Network |
| `server-resources` | Resources |
| `server-advanced` | Advanced |
| `server-snapshots` | Conversation snapshots |
| `server-snapshots-intro` | copy of `snapshots-intro` |
| `server-sdk` | SDK tuning |
| `server-sdk-intro` | Settings of the GPU vendor's own libraries, passed to llama-server as environment variables at launch. They are not llama.cpp flags: nothing checks their values, and each one only affects its own backend. (adapted: the GUI's last sentence names the Command line card, not in slice 1) |
| `server-cuda-nothing` | copy of `cuda-nothing-to-tune` |
| `server-cuda-and-rocm` | CUDA and ROCm |
| `server-port` / `-hint` | Port / copy of `port-hint` |
| `server-bind-to` / `-hint` | Bind to / copy of `bind-to-hint`, with "localhost = this machine only." changed to "localhost = only the computer running the server." (adapted) |
| `server-bind-localhost` | localhost (only the computer running the server) (adapted from `bind-localhost`) |
| `server-bind-all` | 0.0.0.0 (all interfaces, LAN-reachable) |
| `server-bind-gone` | {0} (no longer present) |
| `server-base-url` / `-hint` | Base URL / The address integrations use (opencode.json and Claude Code).\nDefault: built from host and port, with /v1 added for opencode.json; the default shown is that of the saved settings. Untick default to enter your own, for a proxy or gateway. (adapted) |
| `server-api-key` / `-hint` | API key / copy of `api-key-hint` |
| `server-cpu-threads` / `-hint` | CPU threads / How many CPU threads generate tokens. More won't help once the GPU is the limit.\nDefault: llama.cpp uses the physical core count. Maximum = the logical processors of the computer running the server.\nFlag: --threads (adapted) |
| `server-batch-threads` / `-hint` | Batch threads / How many CPU threads read the prompt (prefill).\nDefault: the same as CPU threads. Maximum = the logical processors of the computer running the server.\nFlag: --threads-batch (adapted) |
| `server-max-models` / `-hint` | Max models / copy of `max-models-hint` |
| `server-memory-margin` / `-hint` | Memory margin / copy of `memory-margin-hint` |
| `server-models-folder` / `-hint` | Models folder / The folder holding your models, on the computer running the server. The pickers read its subfolders: models, mmprojs, mtps and dflashs.\nLeft empty, the instance uses its own folder, shown in the field. (adapted) |
| `server-folder-instance-default` | unset: the instance uses {0} (copy of `models-folder-instance-placeholder`) |
| `server-web-ui` / `-hint` | Web UI / Serves the built-in chat page at /. With it off, the API keeps working.\nDefault: on (llama.cpp's own).\nFlag: --no-ui (adapted: the GUI's names its own tabs) |
| `server-web-ui-toggle` | serve the built-in chat UI |
| `server-mcp-proxy` / `-hint` | Web UI MCP proxy / copy of `web-ui-mcp-proxy-hint` |
| `server-mcp-proxy-toggle` | serve the web UI's MCP proxy endpoint |
| `server-log-level` / `-hint` | Log level / How much llama-server writes to its log. Each level includes the ones below it: OUTPUT:0 writes almost nothing, DEBUG:5 everything.\nDefault: TRACE:4, which records each request in the instance's log. DEBUG:5 also logs the per-tensor lines tools\report-memory.ps1 reads.\nFlag: --log-verbosity (adapted: the GUI names logs/llama-server.log, which only macOS writes) |
| `server-save-on-shutdown` / `-hint` | Save on shutdown / copy of `save-on-shutdown-hint` |
| `server-save-on-shutdown-toggle` | snapshot the KV cache on stop and restore it on start |
| `server-snapshot-folder` / `-hint` | Snapshot folder / copy of `snapshot-folder-hint`, its line "The default is on the system drive: point it at a data disk if that is tight." kept |
| `server-hipblaslt-hint` | copy of `hipblaslt-hint` |
| `server-profiler-attach` / `-hint` | Profiler attach / copy of `profiler-attach-hint` |
| `server-profiler-attach-toggle` | allow rocprofv3 --attach |
| `server-allreduce` / `-hint` | Tensor-split AllReduce / copy of `tensor-split-allreduce-hint` |
| `server-gpu-graphs` / `-hint` | Disable GPU graphs / copy of `disable-gpu-graphs-hint` |
| `server-gpu-graphs-toggle` | launch kernels one by one |
| `server-kv-rotation` / `-hint` | KV rotation / copy of `kv-rotation-hint` |
| `server-kv-rotation-toggle` | disable the KV-cache rotation |

The existing `version-*`, `instances-*` and `instance-state-*` keys stay as they are.

### 5.6 Strings, slice 1b

| Key | English |
|---|---|
| `save-button` | Save changes |
| `save-discard` | Discard |
| `save-saving` | Saving… |
| `save-unsaved` | You have unsaved changes. |
| `save-saved` | Saved. |
| `save-saved-restart` | Saved: restart llama-server to apply. |
| `save-invalid` | Not saved: correct the marked fields. |
| `save-refused` | Not saved: {0} |
| `save-failed` | Not saved: the service did not answer as expected. Your changes are kept; try again. |
| `save-reload` | Reload |
| `session-ended` | Your session has ended. Sign in again to save; your changes are kept. |
| `signin-cancel` | Cancel (copy of `cancel`; the sign-in dialog's close button, 36a) |
| `discard-title` | Discard unsaved changes? (copy) |
| `discard-description` | You have unsaved edits that will be lost if you continue. (copy) |
| `discard-keep` | Keep editing |
| `discard-confirm` | Discard changes (copy of `discard-changes`) |
| `field-error-number` | Enter a whole number from {0} to {1}. |
| `field-error-number-min` | Enter a whole number of at least {0}. |
| `field-error-comment-markers` | Remove the ; and # characters: the INI file would cut the value there. |
| `api-not-whole-number-in-range` | {0} = `{1}` is not a whole number from {2} to {3} (copy) |
| `api-comment-markers` | copy of `comment-markers` |
| `api-load-on-startup-over-max` | {0} enabled presets would load on startup, but the server loads at most {1} models at once (ModelsMax) and would not start. Turn Load on startup off on another preset, or raise ModelsMax (copy of `load-on-startup-over-max`, verbatim: no closing full stop, like every copied server check text in this table and in the configurator) |
| `api-load-on-startup-unlimited` | {0} enabled preset(s) would load on startup with ModelsMax = 0, which llama-server rejects at startup (it reads 0 as a limit there, not as unlimited). Set ModelsMax to at least {0}, or turn Load on startup off (copy of `load-on-startup-unlimited`, verbatim: no closing full stop) |
| `api-client-address-in-use` | Another instance, {0}, already uses that address and port. (copy) |
| `api-client-unresolvable-host` | The host name {0} does not resolve to an address. (copy) |
| `api-server-changed-elsewhere` | These settings were changed elsewhere after you opened them. Reload to see them; your edits here will be lost. |
| `api-other` | The request was refused ({0}). |
| `server-edits-dropped` | Your unsaved changes were dropped: you signed in with a different account. The form shows the saved settings. (36b) |

Field of a refusal when the bff gives none (fallback the fe applies): `not-whole-number-in-range` and
`comment-markers` → the field whose INI key is `args[0]`; `load-on-startup-*` → F7;
`client-address-in-use` → F1; `client-unresolvable-host` → F2.

The client-side messages differ from the GUI on purpose: the GUI shows one composite footer line
(`not-saved-invalid` + `number-*`, naming INI keys); the web shows each message under its own field,
with a suggestion ([3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion)). Server refusals keep the
GUI's words.

### 5.7 Strings, slice 1c

Checked against `fe/src/strings/en_US.json` on main (ee4cbe5): none of these keys exists yet. Reused,
not added: `session-sign-in`, `signin-title`, `signin-cancel`, `instance-not-found`,
`api-client-not-authorized`, `api-client-unavailable`.

| Key | English |
|---|---|
| `run-start` | Start (copy of `start`) |
| `run-stop` | Stop (copy of `stop`) |
| `run-restart` | Restart |
| `run-starting` | Starting llama-server… (copy of `server-starting`) |
| `run-stopping` | Stopping llama-server… (copy of `server-stopping`) |
| `run-restarting` | Restarting llama-server… |
| `run-started` | llama-server started. (copy of `server-started`) |
| `run-stopped` | llama-server stopped. (copy of `server-stopped`) |
| `run-restarted` | llama-server restarted. |
| `run-ended-failed` | llama-server is in the Failed state. Its log on the server says why. |
| `run-timeout` | llama-server did not reach the expected state within a minute. The state above keeps updating. |
| `request-failed` | The request did not go through. Try again. (run actions and the boot write) |
| `session-ended-retry` | Your session has ended. Sign in again, then retry. (run actions and the boot write) |
| `api-client-service-failed` | The service manager reported an error. The instance's log on the server says why. (adapted from `client-service-failed`: no `{0}`, the bff keeps the manager's detail out of the body) |
| `boot-label` | Start at boot |
| `boot-toggle` | start llama-server when the system starts (copy of `start-at-boot-toggle`) |
| `boot-hint` | Starts llama-server when the system starts, before anyone logs in: the instance's service is enabled (Linux: systemctl enable llama.cpp@default; Windows: its start type is Automatic). (adapted from `start-at-boot-hint`: the GUI's last sentence names its footer; one line since the web UI is for administrators only, user, 2026-11-15) |
| `boot-on` | llama-server will start when the system starts. (copy of `start-at-boot-on`) |
| `boot-off` | llama-server will no longer start when the system starts. (copy of `start-at-boot-off`) |
| `boot-load-failed` | Whether llama-server starts at boot could not be read. |
| `save-timeout` | Not confirmed: the service did not answer in time, so the settings may or may not have been saved. Your changes are kept; save again to be sure. (4.8, 47d) |

Renamed from the first draft of this brief, before any code used them: `run-failed` is
`request-failed` and `session-ended-run` is `session-ended-retry` (both serve the boot write too).

## 6. Keyboard, focus and screen reader

- Tab order follows the DOM: skip link, product link, Menu (narrow), Sign in or Sign out, nav links,
  then main: h1 (programmatic focus only), run buttons, their Sign in, boot hint toggle and checkbox,
  its Sign in (1c, 46a), the fields card by card
  in table order (default box before its textbox; hint toggle after the label), the save bar.
  ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order), [2.1.1](https://www.w3.org/TR/WCAG22/#keyboard))
- Checkboxes toggle with Space, selects open with the keyboard (native controls); no custom widget.
- Live regions, each present in the DOM before its text changes: the save bar status, the run
  status, the Start at boot status (1c). The state line is not one (42b). The loading paragraphs are the exception: they appear with their page as
  `<p role="status">` (the focus on the page's `h1` already announces the page). Errors after an
  action use `role="alert"`.
  ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
- Names the tests query (English shown): banner link "Private AI Framework"; navigation "Main"; link
  "Overview"; button "Sign in" / "Sign out"; textbox "User name"; "Password" (by label); heading level
  1 "default"; heading level 2 "Server"; headings level 3 per card; group "Port" with checkbox
  "default" and textbox "Port"; group "Bind to" with checkbox "localhost (only the computer running
  the server)"; combobox "Log level"; button "About Port" (`aria-expanded`); group "API key"; button "Show" (`aria-pressed`);
  buttons "Save changes", "Discard", "Start", "Stop", "Restart"; group "Start at boot" with button
  "About Start at boot" and checkbox "start llama-server when the system starts"; dialog "Discard
  unsaved changes?"; dialog "Sign in".
- Every requirement of D8 applies; contrast, targets and reflow are checked in review.
