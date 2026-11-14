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
  `{"key":"session-bad-credentials"}`; 429 `{"key":"session-too-many-attempts","args":["<seconds>"]}`.
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
6. Below 960px a button **Menu** (`nav-menu`) with `aria-expanded` shows and hides the nav; Escape
   closes it and returns focus to the button. (Layout itself is checked in review, not in jsdom.)
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
11. Empty user name or password: no request; the empty field shows `signin-empty-username` or
    `signin-empty-password` under it (`aria-invalid`, `aria-describedby`); focus moves to the first
    empty field.
12. While the request runs the button reads `signin-busy` and a second submit sends nothing.
13. 200: the session state updates (banner and nav as in 4), focus goes to the destination's `h1`:
    the protected page it replaced, else the Overview.
14. 401 `session-bad-credentials`: a `role="alert"` paragraph above the fields with
    `api-session-bad-credentials`; the password field is cleared, the user name kept, focus on the
    password field. 429 `session-too-many-attempts`: `api-session-too-many-attempts` with the seconds.
    Anything else, a network failure included: `signin-failed`. Never the reply's body.
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification), [4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
15. **Sign out**: sends `DELETE /api/session`, then shows the signed-out shell on the Overview
    whatever the answer (the cookie may already be gone). In 1b, with unsaved changes, it first asks
    with the discard dialog (24).

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
27. The API key shows masked; its **Show** button (`field-show`, `aria-pressed`) reveals it, also
    for a user (a user may read the file: manager `control`).

## 3. Slice 1b: editing and saving (administrators)

28. With `canConfigure: true` the controls are enabled, under the dependencies of 5.3, and the save
    bar (D5.2) is rendered at the end of the form: status line (`role="status"`), **Discard**
    (`save-discard`), **Save changes** (`save-button`), both `aria-disabled="true"` while clean.
    A user keeps 1a's read-only form and no bar.
29. **Dirty**: any value differing from the loaded one (after the same normalisation as the save,
    5.4) makes the form dirty: the status reads `save-unsaved`, both buttons enabled. Changing it back
    makes it clean again.
30. **Discard**: puts every control back to the loaded values; focus stays on the button; status
    empty. No request.
31. **Save**, client checks first (5.4). Any failure: no request; each failing field shows its error
    (`aria-invalid`, `aria-describedby`); focus moves to the first failing control in DOM order; the
    save bar shows `save-invalid` as `role="alert"`. Editing a field clears its error.
    ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification), [3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion))
32. Checks pass: `PUT` with the loaded `revision` and every value (unset ones as `null`). While it
    runs the Save button reads `save-saving`, is `aria-disabled`, and a second click sends nothing.
33. 200: the reply's revision and the sent values become the loaded ones (clean); status
    `save-saved`, or `save-saved-restart` when `running` is true. A save never restarts the server (as
    the GUI).
34. 422 with `field`: `api-<key>` filled with `args` under that field (`aria-invalid`), focus on it,
    the bar shows `save-refused` with the same text as `role="alert"`. Without `field`, or a field the
    form lacks: only the bar. A key the fe does not know: `api-other` with the key. The keys and texts
    of 5.6.
35. 409 `server-changed-elsewhere`: the bar shows `api-server-changed-elsewhere` and a button
    **Reload** (`save-reload`); Reload asks the discard dialog, then reads the file again. The edits
    stay until then.
36. 401: the bar shows `session-ended` and a button `session-sign-in`, which opens the sign-in form in
    a dialog (D5.2 Dialog, `h2` `signin-title`); after a successful sign-in the dialog closes, focus
    returns to Save changes, the edits are intact, nothing is resent by itself.
    ([3.3.7](https://www.w3.org/TR/WCAG22/#redundant-entry))
37. 403: `api-client-not-authorized` in the bar. 503: `api-client-unavailable`. Anything else:
    `save-failed`. Edits kept in every case.
38. **Leaving dirty**: following a nav link, the product link or Sign out opens a dialog `h2`
    `discard-title`, text `discard-description`, buttons **Keep editing** (`discard-keep`, focused
    first) and **Discard changes** (`discard-confirm`). Keep: stays, focus back on the link. Discard:
    goes on. Closing or reloading the browser tab: `beforeunload` is armed only while dirty.
39. No keyboard shortcut for Save (D8).

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
  A number → box clear, textbox shows it. Unticking keeps the shown text as the starting value.
- **F3**: `null` → box ticked, textbox disabled showing `defaults.OpencodeBaseUrl` (computed by the
  bff from the saved file; it does not follow unsaved edits of Port or Bind to, said in the hint).
- **F4**: `null` or empty → No key ticked, password box disabled and empty. Else No key clear, the key
  masked.
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

Sent values: default/auto/No key ticked → `null`; numbers as integers; F2 as the checked values joined
by `,` in row order; F3 trimmed, with a trailing `/` and then a trailing `/v1` removed (as the GUI's
`form_to_config`); F4 an empty text → `null`; F9, F14 trimmed, empty → `null`; F12 the level number;
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
- Live regions, each present in the DOM before its text changes: the loading paragraphs, the save
  bar status, the run status. Errors after an action use `role="alert"`.
  ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages))
- Names the tests query (English shown): banner link "Private AI Framework"; navigation "Main"; link
  "Overview"; button "Sign in" / "Sign out"; textbox "User name"; "Password" (by label); heading level
  1 "default"; heading level 2 "Server"; headings level 3 per card; group "Port" with checkbox
  "default" and textbox "Port"; group "Bind to" with checkbox "localhost (only the computer running
  the server)"; combobox "Log level"; button "About Port" (`aria-expanded`); button "Show"
  (`aria-pressed`); buttons "Save changes", "Discard", "Start", "Stop", "Restart"; checkbox "start
  llama-server when the system starts"; dialog "Discard unsaved changes?".
- Every requirement of D8 applies; contrast, targets and reflow are checked in review.
