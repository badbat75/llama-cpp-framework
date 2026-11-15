# Brief 01: the app shell and the Server page of instance `default`

Written by the designer on 2026-11-14 for `PLAN.md` step 10, "First editing slice". Applies
`fe/DESIGN.md`; section numbers like "D5.2" point there. Each numbered line is an acceptance
criterion the tester turns into a Vitest + Testing Library test. The reference is `pai-config`'s
Server tab (`crates/config/ui/server_page.slint`, `crates/config/src/server_form.rs`) and its texts
(`crates/core/strings/en_US.toml`).

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

- `GET /api/session`: 200 `{"user": "<name>", "canConfigure": <bool>}`, 401 when signed out.
- `POST /api/session` `{"username", "password"}`: 200 as above (sets the cookie); 401
  `{"key":"session-bad-credentials"}`; 429 `{"key":"session-too-many-attempts","args":["<seconds>"]}`;
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
- 1c: `POST /api/instances/{id}/start|stop|restart`: 202 or 204; 401, 403, 404, 503, other. Boot:
  `GET|PUT /api/instances/{id}/boot` `{"enabled": <bool>}` (shape to be fixed by the contract).

## 2. Slice 1a: shell, sign-in, Overview, read-only Server page

### 2.1 Shell

1. The page has, in DOM order: a link "Skip to content" (`skip-to-content`) targeting `main`; a
   `banner` with the product name (`app-name`) linking to `#/`; a `navigation` named `nav-label`; a
   `main`. ([2.4.1](https://www.w3.org/TR/WCAG22/#bypass-blocks), [1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships))
2. On start the fe asks `GET /api/session` once. Until it answers, `main` shows
   `<p role="status">` `loading`, and the nav shows only Overview.
3. **Signed out** (401): the banner shows a button or link **Sign in** (`session-sign-in`); the nav
   holds only **Overview** (`nav-overview`).
4. **Signed in**: the banner shows `session-signed-in-as` with the user's name, then
   `session-role-admin` when `canConfigure` is true or `session-role-user` otherwise, then a button
   **Sign out** (`session-sign-out`). The nav holds Overview, then a heading-like label **Instances**
   (`nav-instances`, a list labelled by it) with one link per instance from `GET /api/instances`, the
   link text being the id, in the reply's order. A failed instances read leaves the list out of the
   nav (the Overview shows the error).
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
    `session-ended` and `session-ended-run` where edits or an action are at stake (36, 43).
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
    (5.2). **In 1a every control is disabled for every role.** A user (`canConfigure: false`) also sees
    the Notice `server-read-only` above the first card. No save bar in 1a.
26. Each field's hint toggle (D5.2) works in 1a: button named "About `<label>`" (`field-about` plus
    the label), `aria-expanded` false, then true on Enter, Space or click, revealing the hint with one
    paragraph per line of the text.
27. **Superseded by 27a to 27e (2.8)** for a user: the API key reaches administrators only (user,
    2026-11-15). For an administrator it still holds: the key shows masked; its **Show** button
    (`field-show`, `aria-pressed`) reveals it.

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
- **Show** (27): enabled for every role in 1a, also while No key is ticked; 5.3 governs it from 1b.
  **Superseded by 27b and 27c (2.8)**: for a user, Show is not rendered when the key is withheld and
  is disabled when there is no key.
- **Reads of `/api/instances`**: in 1a the nav, the Overview and the instance page each read it on
  their own. 1c's polling (41, 42) must share one read across them, so the nav and the badge never
  disagree.
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

### 2.8 The API key reaches administrators only (user, 2026-11-15)

The bff no longer sends `OpencodeApiKey` to a session whose `canConfigure` is false: its value is
always `null`, and the document carries `"withheld": ["OpencodeApiKey"]` when the file has a key
(`fe/briefs/api-contract.md`, `GET /api/instances/{id}/server`, last bullet). These criteria replace
27 for a user and the Show bullet of 2.6; they change only F4.

27a. **The three views of F4.** The fe picks one from the session's `canConfigure` and the
    document's `withheld`. "Withheld" means `withheld` is an array that contains the string
    `"OpencodeApiKey"`; anything else (absent, `[]`, an array of other names, `null`, not an array)
    is "not withheld". Names in `withheld` the fe does not know are ignored.
    - user (`canConfigure` false), withheld: 27b;
    - user, not withheld: 27c;
    - administrator (`canConfigure` true): 27d.
27b. **User, key withheld.** The group "API key" (`server-api-key`, the `fieldset` of 5.1) holds, in
    this order: the checkbox "No key" (`field-no-key`) **not checked** and disabled; then a paragraph
    `server-api-key-withheld` ("A key is set. Only an administrator can see it."). There is **no
    password input and no Show button** in this view: nothing that looks like a hidden value the user
    could uncover. The hint toggle "About API key" is as on every field (26). The group has
    `aria-describedby` pointing at the paragraph, so a screen reader that enters the group (for
    example by tabbing to its hint toggle) also reads "A key is set. Only an administrator can see
    it."; in reading order the paragraph follows the checkbox. The paragraph is plain text, not a live
    region (it comes with the page) and not `--danger` (it is not an error); its text is drawn in
    `--muted`, like the values of disabled controls (`fe/DESIGN.md`, "Disabled controls stay readable";
    **(preference)**). The view is the same whatever `values.OpencodeApiKey` holds: if a bff sent a
    value anyway, withheld wins and the value is rendered nowhere.
    ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
    [1.3.2](https://www.w3.org/TR/WCAG22/#meaningful-sequence),
    [4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value): the checkbox's state must be true, so it
    is not checked when a key exists; `aria-describedby` is a global attribute:
    [WAI-ARIA 1.2](https://www.w3.org/TR/wai-aria-1.2/#aria-describedby))
27c. **User, not withheld.** As 5.2: `values.OpencodeApiKey` `null` or empty, so "No key" checked
    and disabled, the password input disabled and empty. **Show follows 5.3 for every role**: here it
    is rendered, `disabled`, `aria-pressed="false"`, since No key is ticked (it would reveal an empty
    box). The 2.6 rule "enabled for every role, also while No key is ticked" no longer applies. (A
    user given a value with no `withheld`, which the contract rules out, sees it masked with Show
    enabled, as 5.2 and 5.3 say: the bff decides who reads the key, not the fe.)
27d. **Administrator.** Unchanged: 5.2, 5.3 and 1b's editing; the key masked, Show reveals it. An
    administrator's document never carries `withheld` naming `OpencodeApiKey` (the contract). If one
    does anyway, the fe treats the whole read as failed: `server-load-failed` in place of the form
    (24), no save bar. Why not ignore it: the value would be `null`, so the form would show No key
    ticked and a save would send `null`, erasing the key from the file. Why not show 27b inside an
    editable form: it adds a state to the form (what does a save send for F4?) for a case the
    contract excludes. A failed read loses nothing; reopening the page reads again.
27e. **Nothing in the page holds a key or a stand-in for one in 27b.** No `input` of any type for F4
    (no password, no hidden, no text input), no `value`, `title`, `placeholder`, `data-*` or
    `aria-*` attribute carrying a key, and no mask string (no run of `•`, `*` or similar characters):
    a fake mask would read as a secret's length. Testable by stubbing a user document with
    `"withheld": ["OpencodeApiKey"]` and, as a contract breach, `values.OpencodeApiKey: "sk-test"`:
    the rendered HTML contains neither `sk-test` nor `•`, and the group holds exactly one `input`,
    the checkbox.

Differs from the GUI on purpose: `pai-config` and `pai` read the file through the manager, which
gives it whole to any caller allowed `control` (`crates/manager/src/manager/pipe.rs`, the test
`reading_an_instance_file_asks_control_and_a_refused_caller_reads_nothing`), so they show the key to
every user who may read the file. On the web the bff withholds it (`crates/bff/src/server.rs`), by the
user's decision. Whether the GUI follows is out of scope. A blank key in the file is unset
(`server_cfg.rs`, `opt_nonblank`), so a user never sees 27b for an empty key.

Strings added:

| Key | English |
|---|---|
| `server-api-key-withheld` | A key is set. Only an administrator can see it. |

## 3. Slice 1b: editing and saving (administrators)

28. With `canConfigure: true` the controls are enabled, under the dependencies of 5.3, and the save
    bar (D5.2) is rendered at the end of the form: status line (`role="status"`), **Discard**
    (`save-discard`), **Save changes** (`save-button`), both `aria-disabled="true"` while clean.
    A user keeps 1a's read-only form and no bar.
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
    edits were made in. Same `user` and `canConfigure` true: 36. A different `user`, or
    `canConfigure` false: the edits belong to another session and are dropped. The dialog closes, the
    banner shows the new session (4), the page reads the file again (one `GET`, its states as 24) and
    shows it for the new role, clean: editable with the save bar for an administrator, read-only with
    the Notice `server-read-only` for a user (25). Above the first card (above that Notice) a
    `role="alert"` paragraph reads `server-edits-dropped`, until the page is left; focus goes to the
    page's `h1`. Nothing is sent but the read.
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
    A click on the link of the page already shown asks nothing (nothing would be left). **Known gap
    of 1b:** the browser's Back and Forward buttons and an address typed in the bar change the hash
    without a click, so they leave without asking; closing it is open (the question below, section 3.1).
38a. **Escape in the discard dialog is Keep editing**: it closes the dialog (the native `cancel` of a
    modal `<dialog>`), nothing is discarded, the route does not change, focus returns to the control
    that opened it (the link, Sign out, or Reload of 35). The same for every dialog of the slice:
    Escape is the choice that changes nothing.
    ([2.1.2](https://www.w3.org/TR/WCAG22/#no-keyboard-trap); D5.2 Dialog)
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
- **Open, for the user**: guard Back, Forward and a typed address while dirty (38, known gap)? It
  needs the app to undo the hash change and ask; it can ride along with 1c or wait.

## 4. Slice 1c: run controls and start at boot

40. Under the header's state, three buttons in this order: **Start** (`run-start`), **Stop**
    (`run-stop`), **Restart** (`run-restart`), always rendered for a signed-in user (manager
    `control`), `aria-disabled` when not applicable: `active` Start disabled; `inactive` and `failed`
    Stop and Restart disabled; `activating`, `deactivating` or a request in flight: all disabled; an
    unknown state: all enabled (the manager decides). Focus never drops.
41. A click sends `POST /api/instances/<id>/<action>`; while it runs a status line (`role="status"`)
    reads `run-starting`, `run-stopping` or `run-restarting`. The fe then reads the instances every
    1 s until the state is neither `activating` nor `deactivating`, at most 60 s, updating the badge.
    It ends with `run-started` / `run-stopped` / `run-restarted`, or `run-ended-failed` when the state
    is `failed`, or `run-timeout` after 60 s.
42. While the instance page is open and no action runs, the state is re-read every 5 s; a failed
    read keeps the last state and shows nothing new.
43. Errors of the action (`role="alert"`): 401 `session-ended-run` with `session-sign-in` (dialog as
    36); 403 `api-client-not-authorized`; 404 `instance-not-found`; 503 `api-client-unavailable`;
    other `run-failed`.
44. Stop needs no confirmation (as the GUI). With Save on shutdown on, the bff saves the snapshots
    before it stops (the GUI does it client side); the fe only shows `run-stopping`.
45. **Start at boot**: a checkbox `boot-toggle` in a field labelled `boot-label` with hint
    `boot-hint`, in the header area below the buttons. Read with the page; enabled only for an
    administrator, disabled for a user. Changing it sends the write at once (it is not part of the save
    bar: it is the service manager's state, as in the GUI's Settings tab); the box shows the new state
    while waiting; on success the status reads `boot-on` or `boot-off`; on any error it goes back to
    the state read again from the bff and shows the error as in 43.

## 5. The Server form

### 5.1 Cards and fields, in order

Same cards and order as the GUI. Card headings are `h3`; the SDK tuning groups are `h4` (`CUDA`,
`ROCm` and `llama.cpp` are names, written as data; "CUDA and ROCm" is `server-cuda-and-rocm`).

| # | Card | Field label (key) | Control | Accessible structure | INI key |
|---|---|---|---|---|---|
| F1 | Network (`server-network`) | Port (`server-port`) | default box + text, numeric | group "Port": checkbox `field-default`, textbox "Port" | `Port` |
| F2 | | Bind to (`server-bind-to`) | checkbox list | group "Bind to": one checkbox per row (5.2) | `Hostname` |
| F3 | | Base URL (`server-base-url`) | default box + text | group: checkbox `field-default`, textbox | `OpencodeBaseUrl` |
| F4 | | API key (`server-api-key`) | No key box + secret + Show | group: checkbox `field-no-key`, password input labelled "API key" (found by label: a password input has no ARIA role), button `field-show`; for a user whose key is withheld, the checkbox and the paragraph `server-api-key-withheld` instead (27b) | `OpencodeApiKey` |
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
  masked. For a user whose key is withheld: 27b (No key clear, no password box, no Show).
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
- F4: password box and Show enabled only while No key is clear, for every role (27c); in a user's
  read-only form the password box stays disabled and only Show follows this rule.
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
| `session-role-admin` | Administrator |
| `session-role-user` | User |
| `signin-title` | Sign in |
| `signin-intro` | Use the user name and password of your account on the computer that runs Private AI Framework. |
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
| `server-read-only` | You can view these settings. Saving them needs an administrator account. |
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
| `api-load-on-startup-over-max` | copy |
| `api-load-on-startup-unlimited` | copy |
| `api-client-address-in-use` | Another instance, {0}, already uses that address and port. (copy) |
| `api-client-unresolvable-host` | The host name {0} does not resolve to an address. (copy) |
| `api-server-changed-elsewhere` | These settings were changed elsewhere after you opened them. Reload to see them; your edits here will be lost. |
| `api-other` | The request was refused ({0}). |
| `server-edits-dropped` | Your unsaved changes were dropped: you signed in with a different account, or with one that cannot save these settings. The form shows the saved settings. (36b) |

Field of a refusal when the bff gives none (fallback the fe applies): `not-whole-number-in-range` and
`comment-markers` → the field whose INI key is `args[0]`; `load-on-startup-*` → F7;
`client-address-in-use` → F1; `client-unresolvable-host` → F2.

The client-side messages differ from the GUI on purpose: the GUI shows one composite footer line
(`not-saved-invalid` + `number-*`, naming INI keys); the web shows each message under its own field,
with a suggestion ([3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion)). Server refusals keep the
GUI's words.

### 5.7 Strings, slice 1c

| Key | English |
|---|---|
| `run-start` | Start (copy of `start`) |
| `run-stop` | Stop (copy) |
| `run-restart` | Restart |
| `run-starting` | Starting llama-server… (copy of `server-starting`) |
| `run-stopping` | Stopping llama-server… |
| `run-restarting` | Restarting llama-server… |
| `run-started` | llama-server started. (copy) |
| `run-stopped` | llama-server stopped. (copy) |
| `run-restarted` | llama-server restarted. |
| `run-ended-failed` | llama-server did not keep running: its state is Failed. Its log on the server says why. |
| `run-timeout` | llama-server is still changing state after a minute. The state above updates when it settles. |
| `run-failed` | The request did not go through. Try again. |
| `session-ended-run` | Your session has ended. Sign in again, then retry. |
| `boot-label` | Start at boot |
| `boot-toggle` | start llama-server when the system starts (copy of `start-at-boot-toggle`) |
| `boot-hint` | Starts llama-server when the system starts, before anyone logs in: the instance's service is enabled (Linux: systemctl enable llama.cpp@default; Windows: its start type is Automatic).\nChanging it needs an administrator. (adapted: the GUI's names its footer) |
| `boot-on` | llama-server will start when the system starts. (copy of `start-at-boot-on`) |
| `boot-off` | llama-server will no longer start when the system starts. (copy) |

## 6. Keyboard, focus and screen reader

- Tab order follows the DOM: skip link, product link, Menu (narrow), Sign in or Sign out, nav links,
  then main: h1 (programmatic focus only), run buttons and boot (1c), Notice, the fields card by card
  in table order (default box before its textbox; hint toggle after the label), the save bar.
  ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order), [2.1.1](https://www.w3.org/TR/WCAG22/#keyboard))
- Checkboxes toggle with Space, selects open with the keyboard (native controls); no custom widget.
- Live regions, each present in the DOM before its text changes: the save bar status, the run
  status. The loading paragraphs are the exception: they appear with their page as
  `<p role="status">` (the focus on the page's `h1` already announces the page). Errors after an
  action use `role="alert"`.
  ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
- Names the tests query (English shown): banner link "Private AI Framework"; navigation "Main"; link
  "Overview"; button "Sign in" / "Sign out"; textbox "User name"; "Password" (by label); heading level
  1 "default"; heading level 2 "Server"; headings level 3 per card; group "Port" with checkbox
  "default" and textbox "Port"; group "Bind to" with checkbox "localhost (only the computer running
  the server)"; combobox "Log level"; button "About Port" (`aria-expanded`); group "API key" with
  description "A key is set. Only an administrator can see it." (27b); button "Show" (`aria-pressed`);
  buttons "Save changes", "Discard", "Start", "Stop", "Restart"; checkbox "start llama-server when
  the system starts"; dialog "Discard unsaved changes?".
- Every requirement of D8 applies; contrast, targets and reflow are checked in review.
