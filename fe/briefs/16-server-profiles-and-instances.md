# Brief 16: server profiles, instances and their associations (slice 17)

`PLAN.md` step 10 items 13 and 13a (profiles and instances end to end); `ARCHITECTURE.md` "Nodes and
instances", slices 14, 15 and 16 (all served: `crates/bff/README.md` "The server profile pool", "The
association routes", "Creating and removing an instance"; `fe/briefs/api-contract.md`). The web UI part
is slice 17. It is split in five sub-slices of at most 12 criteria each, built in this order, each with
its own tests; criteria are numbered **17a.n** to **17e.n** and test names cite them so (`17a.4`).

| Sub-slice | What | Criteria |
|---|---|---|
| 17a | The Server profiles list, a server profile's page with the Server form moved there, the nav entry; the instance page stops reading `/api/instances/{id}/server` | 12 |
| 17b | Create and delete a server profile | 10 |
| 17c | Create and remove an instance; the start refused for no model profile | 12 |
| 17d | The restart offer; the instance's server profile switch | 11 |
| 17e | The instance's model profiles | 11 |

The old routes `GET`/`PUT /api/instances/{id}/server` are removed from the bff in slice 17's commit
(`ARCHITECTURE.md` slice 14, "The old route stays"): after 17a no fe code calls them.

## 0. What exists (read on main at 3585c39)

- **Routes served** (all behind a session, 401 `session-required` without one):
  - `GET /api/profiles/server`: `[{"id", "engine", "port", "tags", "instance"}]`, sorted by id,
    `instance` the id of the instance whose record names it or `null`, `port` 0 for a file that cannot
    be read; 403 `session-not-administrator`, 503 `client-unavailable`, 502 `client-other`.
  - `POST /api/profiles/server` `{"engine", "id", "port"}`: 201 `{"id"}`; 400
    `server-profile-malformed`; 422 `unknown-engine` (`field` `engine`), `server-profile-bad-id`
    `[id]` (`field` `id`), `not-whole-number-in-range` `[key, text, min, max]` (`field` `port`); 409
    `client-server-profile-exists` `[id]`; 503 (outcome unknown), 502.
  - `GET /api/profiles/server/{id}`: today's server document plus `instance` (id or `null`);
    `defaults.StateDir` is `null` for a profile no instance runs; 404 `client-no-server-profile`
    `[id]`. `PUT` the same save as today's instance save (same body, statuses, keys), 200
    `{"revision", "running"}`, `running` for the instance that uses it.
  - `DELETE /api/profiles/server/{id}`: 204; 409 `client-server-profile-in-use` `[profile,
    instance]`; 404 `client-no-server-profile` `[id]`; 503, 502.
  - `GET /api/instances` items carry `serverProfile` (name or `null`) and `removable` (false only
    for `default`).
  - `POST /api/instances` `{"engine", "id", "serverProfile"}`: 201 `{"id"}`; refusals listed in 17c.
    `DELETE /api/instances/{id}`: 204; 409 `instance-protected`, `instance-running`; 404
    `client-no-instance`. Both: 503 (outcome unknown), 502 `client-service-failed`, 502 `client-other`.
  - `POST /api/instances/{id}/start` and `/restart`: 409 `instance-no-model-profiles` `[id]` for an
    instance that serves no model profile (a new instance has none).
  - `PUT /api/instances/{id}/profiles/server` `{"serverProfile"}`: 200 `{"running"}`.
    `GET /api/instances/{id}/profiles/model`: `{"all": "inherit"|"none", "profiles": [{"id",
    "engine", "engineAvailable", "enabled", "model", "value", "viaAll", "effective"}]}`.
    `PUT .../profiles/model/{profile}` `{"value"}`: 200 `{"effective", "running"}`;
    `PUT .../profiles/model` `{"all"}`: 200 `{"running"}`. None of the four answers 504.
  - `PUT /api/profiles/model/{id}` and `PUT /api/profiles/model/{id}/enabled` answer `restart`: the
    ids of the running instances to restart, `[]` with `running` true when a state could not be read.
- **The fe** (`fe/src/`): the instance page (`InstancePage.tsx`) holds the header (engine, port,
  state, Start / Stop / Restart, Start at boot), the Instance tags field (brief 15 turns it into chips;
  untouched here) and `ServerSection.tsx` / `ServerForm.tsx`, which read and save
  `/api/instances/{id}/server`. The nav's Profiles group holds Model profiles only
  (`fe/briefs/05b-profiles-nav.md`: "Server profiles" joins it when its page exists). The run controls'
  alert texts are `InstancePage.tsx` `failureText`; `instance-no-model-profiles` has none yet.
- **`pai-config` and `pai`** have no server profile pool, no instance create or remove and no
  assignment: they act on the instance `default` and the profile its record names. The web UI is first;
  the Server form itself keeps the GUI's Server tab fields, order and texts (`fe/briefs/01-server.md`).

## 1. Decisions (each a preference unless it cites a source)

1. **Nav**: the Profiles group lists **Server profiles** then **Model profiles** (an instance has one
   server profile and many model profiles: the one first). Server profiles has the `server` icon
   (Lucide `server`, path data of `lucide-static@1.74.0` `icons/server.svg`, as brief 08 H8 copies the
   others; licence already in `installer/licenses/lucide.txt`).
2. **Routes**: `#/profiles/server` (the list) and `#/profiles/server/<id>` (one profile: its "Used by"
   fact, Delete while unused, the Server form). The instance page keeps run controls, boot and tags,
   and gains a **Server profile** section (17a shows it, 17d switches it) and a **Model profiles**
   section (17e), then **Remove instance** (17c) last.
3. **New things are made where their list is**: New server profile on the Server profiles page, New
   instance on the Overview under the tiles. Each is a **Create form in place** (new component, section
   5 of `fe/DESIGN.md`): a secondary button with the `plus` icon and `aria-expanded`, revealing a form
   right after it. A create that succeeds goes to the new thing's page, whose `h1` takes the focus.
4. **Destructive actions ask in the page, never in a browser dialog** (coordinator, 2026-11-21): the
   **Confirm in place** component replaces `fe/DESIGN.md`'s "destructive actions ask in a dialog". A
   two-button question needs no modal, and in the flow it reflows at 320 px and 200 % text.
5. **Every new write follows Action feedback** (`fe/DESIGN.md` 5.2): a status line, an alert for a
   failure, and after a 401 a **Sign in** button opening the sign-in Dialog, so nothing typed is lost
   ([3.3.7](https://www.w3.org/TR/WCAG22/#redundant-entry)); a 403 `session-not-administrator` shows
   `session-revoked` with no button, as the run controls do. Page reads keep the page-read rule: a
   401 signs the app out, a 403 `session-not-administrator` signs out as revoked (brief 09 S1b.1).
6. **A server profile's select switches only on a button** (Use this server profile): a closed native
   `select` fires `change` on each arrow key in Chromium and Firefox, which would write each profile it
   passes ([3.2.2 On Input](https://www.w3.org/TR/WCAG22/#on-input) is met either way; the button
   avoids the stray writes).
7. **A model profile on an instance is one Switch, "Served here"**, its state the row's `effective`
   (replaces the three-word select of the 2026-11-17 plan in `api-contract.md`): the four words stay
   the bff's, the page picks the word (17e.6). The GUI has no such control; the words `inherit` and
   `none` are not shown.
8. **Restart is offered, never done for the user**: wherever a write answers that a running instance
   must restart, a **Restart offer** (new component) shows one button per instance to restart.
9. **The create form fills in the port, and it stays required** (user, 2026-11-21: "if not specified,
   fill in the default yourself"): it opens on the first port from 9931 upward that no server profile
   of the list uses, which the administrator may change; the bff allows none, but two profiles on the
   default port cannot both run, so a port is always sent.
10. **After a delete or a remove the page stays and says so**: its content is replaced by one
    paragraph that takes the focus, with a link to the list. No message is carried to another page.

## 2. The texts (all sub-slices)

`…` is U+2026. Every key below is new unless marked reused. `{0}`-style placeholders by position.

| Key | English | Sub-slice |
|---|---|---|
| `nav-server-profiles` | `Server profiles` | 17a |
| `server-profiles-title` | `Server profiles` | 17a |
| `server-profiles-intro` | `Each server profile holds the llama-server settings of one instance: its port, its addresses, its tags and the rest. An instance runs one server profile, and a server profile serves one instance at a time.` | 17a |
| `server-profiles-loading` | `Loading the server profiles…` | 17a |
| `server-profiles-failed` | `The server profiles could not be read.` | 17a |
| `server-profiles-empty` | `There are no server profiles yet.` | 17a |
| `server-profiles-column-name` | `Server profile` | 17a |
| `server-profiles-column-port` | `Port` | 17a |
| `server-profiles-column-tags` | `Tags` | 17a |
| `server-profiles-column-used-by` | `Used by` | 17a |
| `server-profiles-unused` | `No instance` | 17a |
| `server-profile-back` | `All server profiles` | 17a |
| `server-profile-used-by` | `Used by` | 17a |
| `server-profile-not-found` | `There is no server profile named {0}.` | 17a |
| `server-profile-intro` | `llama-server settings of the instance that runs this server profile, shared by every model it serves ({0}). Per-model settings belong to the model profiles.` | 17a |
| `server-folder-unused-default` | `unset: the instance that runs this profile uses its own folder` | 17a |
| `instance-server-title` | `Server profile` | 17a |
| `instance-server-none` | `None` | 17a |
| `server-profiles-new` | `New server profile` | 17b |
| `server-profiles-new-title` | `New server profile` | 17b |
| `server-profile-name` | `Name` | 17b |
| `server-profile-name-hint` | `1 to 32 characters: a-z, 0-9 and -, starting with a letter or a digit. It names the file server@<name>.ini and cannot be changed later.` | 17b |
| `server-profile-port-hint` | `The network port llama-server listens on. Filled in with the first port from 9931 up that no server profile uses; change it if you like. Two instances cannot run on the same address and port.` | 17b |
| `create-button` | `Create` | 17b |
| `create-cancel` | `Cancel` | 17b |
| `create-creating` | `Creating…` | 17b |
| `field-error-required` | `Enter a value.` | 17b |
| `create-unknown` | `The manager did not answer in time, so it may or may not have been created. The list shows it if it was.` | 17b |
| `server-profile-delete` | `Delete server profile` | 17b |
| `server-profile-delete-question` | `Delete the server profile {0}? Its settings file is removed, with any unsaved changes on this page. This cannot be undone.` | 17b |
| `confirm-delete` | `Delete` | 17b |
| `confirm-cancel` | `Cancel` | 17b |
| `confirm-deleting` | `Deleting…` | 17b |
| `server-profile-deleted` | `The server profile {0} was deleted.` | 17b |
| `api-client-server-profile-exists` | `The server profile {0} is already there.` (en_US.toml 1119) | 17b |
| `api-client-server-profile-in-use` | `The server profile {0} is already used by instance {1}.` (en_US.toml 1120) | 17b |
| `api-client-no-server-profile` | `There is no server profile of that name.` (en_US.toml 1116) | 17b |
| `api-server-profile-bad-id` | ``invalid server profile id `{0}`: use lowercase letters, digits and '-', starting with a letter or digit, up to 32 characters`` (en_US.toml 1025) | 17b |
| `api-unknown-engine` | ``unknown engine `{0}`: this manager serves the engine llama.cpp`` (en_US.toml 1024) | 17b |
| `instances-new` | `New instance` | 17c |
| `instances-new-title` | `New instance` | 17c |
| `instance-name` | `Name` | 17c |
| `instance-name-hint` | `1 to 32 characters: a-z, 0-9 and -, starting with a letter or a digit. It names the instance's service and cannot be changed later.` | 17c |
| `instance-new-server` | `Server profile` | 17c |
| `instance-new-server-hint` | `The server profile it runs: its port, addresses and settings. Only server profiles no other instance uses are listed.` | 17c |
| `instance-new-server-choose` | `Choose a server profile` | 17c |
| `instance-server-option` | `{0} (port {1})` | 17c |
| `instance-new-loading` | `Loading the server profiles…` | 17c |
| `instance-new-no-free` | `Every server profile is in use, or there is none. Create a server profile first.` | 17c |
| `instance-new-profiles-link` | `Server profiles` | 17c |
| `instance-new-note` | `The new instance serves no model profile and does not start at boot until you set them on its page.` | 17c |
| `instance-remove` | `Remove instance` | 17c |
| `instance-remove-stop-first` | `Stop it first to remove it.` | 17c |
| `instance-remove-question` | `Remove the instance {0}? Its service and its model profile choices are removed; its server profile stays in the list of server profiles. This cannot be undone.` | 17c |
| `confirm-remove` | `Remove` | 17c |
| `confirm-removing` | `Removing…` | 17c |
| `instance-removed` | `The instance {0} was removed.` | 17c |
| `api-instance-bad-id` | ``invalid instance id `{0}`: use lowercase letters, digits and '-', starting with a letter or digit, up to 32 characters`` (en_US.toml 1028) | 17c |
| `api-instance-exists` | ``an instance named `{0}` already exists`` (en_US.toml 1029) | 17c |
| `api-instance-protected` | ``the instance `{0}` cannot be removed: it is the instance the configurator and the web UI use`` (en_US.toml 1030) | 17c |
| `api-instance-running` | ``the instance `{0}` is running: stop it before removing it`` (en_US.toml 1031) | 17c |
| `api-instance-no-model-profiles` | ``the instance `{0}` has no model profiles: assign it one before starting it. Switch one on under Model profiles on this page.`` (en_US.toml 1032, plus the fix: `fe/DESIGN.md` 7) | 17c |
| `restart-offer-button` | `Restart {0}` | 17d |
| `restart-offer-running` | `Restarting {0}…` | 17d |
| `restart-offer-sent` | `{0} is restarting: its page shows when it is running again.` | 17d |
| `save-saved-restart-ids` | `Saved: restart {0} to apply.` | 17d |
| `model-profiles-switched-on-restart-ids` | `Switched {0} on: restart {1} to apply.` | 17d |
| `model-profiles-switched-off-restart-ids` | `Switched {0} off: restart {1} to apply.` | 17d |
| `instance-server-open` | `Open {0}` | 17d |
| `instance-server-switch-label` | `Switch to` | 17d |
| `instance-server-switch-hint` | `Only server profiles no other instance uses are listed. The instance reads its server profile when it starts.` | 17d |
| `instance-server-switch` | `Use this server profile` | 17d |
| `instance-server-switching` | `Switching the server profile…` | 17d |
| `instance-server-switched` | `This instance now uses the server profile {0}.` | 17d |
| `instance-server-switched-restart` | `This instance now uses the server profile {0}: restart it to apply.` | 17d |
| `instance-server-no-free` | `No other server profile is free to switch to.` | 17d |
| `instance-models-title` | `Model profiles` | 17e |
| `instance-models-intro` | `The model profiles this instance serves from its next start. A model profile switched off in the list of all model profiles is still served where you switch it on here.` | 17e |
| `instance-models-loading` | `Loading the model profiles…` | 17e |
| `instance-models-failed` | `The model profiles of this instance could not be read.` | 17e |
| `instance-models-empty` | `There are no model profiles yet.` | 17e |
| `instance-models-all` | `serve every model profile switched on in the list, including new ones` | 17e |
| `instance-models-all-label` | `All switched-on model profiles` | 17e |
| `instance-models-column-pool` | `In the list` | 17e |
| `instance-models-column-here` | `Served here` | 17e |
| `instance-models-saving` | `Saving…` | 17e |
| `instance-models-on` | `{0} is served here from the next start.` | 17e |
| `instance-models-off` | `{0} is no longer served here from the next start.` | 17e |
| `instance-models-on-restart` | `{0} is served here: restart the instance to apply.` | 17e |
| `instance-models-off-restart` | `{0} is no longer served here: restart the instance to apply.` | 17e |
| `instance-models-all-on` | `Every switched-on model profile is served here from the next start.` | 17e |
| `instance-models-all-off` | `Only the model profiles switched on below are served here from the next start.` | 17e |
| `instance-models-all-restart` | `Saved: restart the instance to apply.` | 17e |

Reused, unchanged: `server-title`, `server-loading`, `server-load-failed`, `server-port`, the whole
Server form's keys, `save-*`, `instance-not-found`, `session-ended-retry`, `session-revoked`,
`session-sign-in`, `api-client-unavailable`, `api-client-not-authorized`,
`api-client-service-failed`, `api-client-address-in-use`, `api-client-unresolvable-host`,
`api-not-whole-number-in-range`, `api-other`, `request-failed`, `field-error-number`,
`model-profiles-column-name`, `model-profiles-status-on`, `model-profiles-status-off`,
`api-no-model-profile-named`, `api-model-profile-over-memory`, `api-model-profile-model-missing`,
`api-load-on-startup-over-max`, `api-load-on-startup-unlimited`. `server-intro` is no longer used
after 17a and is removed then (`strings.test.ts`: every key used).

**One failure mapping for every new write** (`writeFailure(key, args)`, used by 17b to 17e): 401 →
`session-ended-retry` and the Sign in button; `session-not-administrator` → `session-revoked`; a key
listed with a text in this brief or already in `en_US.json` as `api-<key>` → that text filled with
`args`; `client-no-instance` → `instance-not-found` with the instance id; `client-unavailable` →
`api-client-unavailable` (for a create, `create-unknown` instead); any other 422 → `api-other` with the
key; anything else (400, 413, an unknown key, a malformed body, a network failure) → `request-failed`.

## 3. Criteria

### 17a: the list, the page, the form moved (12)

**17a.1 Routes.** `#/profiles/server` is the list page and `#/profiles/server/<id>` a profile's page
(the id percent-decoded; a bad escape is the not-found page, as `#/profiles/model/<id>`). Titles:
`Server profiles | Private AI Framework` and `<id> | Private AI Framework`; each `h1` takes the focus on
arrival. Signed out, the sign-in page shows (`signin-required`) and nothing is read. An id `.` or `..`
shows `server-profile-not-found` with no request (`modelProfileIdUnreadable`'s rule).

**17a.2 Nav.** Signed in, the Profiles list holds two links in order: `nav-server-profiles` to
`#/profiles/server` with the `server` icon (`svg.icon`, `aria-hidden`), then `nav-model-profiles`.
Server profiles has `aria-current="page"` on the list and `aria-current="true"` on a profile's page,
none elsewhere; Model profiles keeps its own rule. The Tab order of the nav gains exactly that link.

**17a.3 List read.** The list page sends one `GET /api/profiles/server` per arrival. While pending:
`server-profiles-loading` in a `role="status"` paragraph. Then `h1` `server-profiles-title`, the
paragraph `server-profiles-intro`, and a table (Table component, in `div.table-scroll`) with the
columns `server-profiles-column-name`, `-port`, `-tags`, `-used-by`, one row per item in the reply's
order: the id as a link to `#/profiles/server/<id>`; the port as text (empty cell for 0); the tags
joined by ", " (empty cell for none); the instance id as a link to `#/instances/<id>`, or
`server-profiles-unused` with the class `muted`. `[]` shows `server-profiles-empty` and no table.

**17a.4 List failures.** 401 signs the app out (sign-in page at the same hash, `signin-session-ended`,
brief 09 S1b.1); 403 `session-not-administrator` signs out with `session-revoked`; 503
`client-unavailable` shows `api-client-unavailable`; anything else, a network failure, or a reply that
is not an array shows `server-profiles-failed`, each a plain paragraph in place of the table. An item
without a non-empty string `id` is left out; a non-number `port` reads as 0, a non-array `tags` as
none, a non-string `instance` as `null`.

**17a.5 The profile page.** Before the `h1`, a link `server-profile-back` to `#/profiles/server`
(as `model-profile-back`, `chevron-left` icon). After it the `h1` (the id), then, once the document
answered, a Value list (`dl.assets`) with one row: `server-profile-used-by` / the document's `instance`
as a link to its page, or `server-profiles-unused` (class `muted`). Then the Server section: `h2`
`server-title`, the paragraph `server-profile-intro` with `server@<id>.ini`, and today's Server form.

**17a.6 Read.** The page sends one `GET /api/profiles/server/<id>` (the id percent-encoded) per
arrival and none to `/api/instances/<id>/server`. Loading: `server-loading` (`role="status"`). 404
`client-no-server-profile` shows `server-profile-not-found` with the id and a link
`server-profile-back`, no `dl` and no form; 401 and 403 as 17a.4; 503 `api-client-unavailable`;
anything else `server-load-failed`.

**17a.7 Save.** Save sends `PUT /api/profiles/server/<id>` with today's body; every reply the
instance save handled is handled the same (statuses, keys, field marks, Reload after 409, the Sign in
Dialog after 401, the timeout text). `running: false` shows `save-saved`; `running: true` shows
`save-saved-restart` and keeps it until the next edit (no instance state is watched on this page; 17d
replaces it with the offer).

**17a.8 Folders of an unused profile.** When the document's `defaults.StateDir` (or
`defaults.ModelsDir`) is `null`, that folder field's placeholder reads `server-folder-unused-default`;
a string default keeps `server-folder-instance-default`.

**17a.9 Leaving.** The profile page's unsaved-changes question (the Dialog of `fe/DESIGN.md` 2.3)
works as it did on the instance page: a nav link, Back, or a typed hash with unsaved edits asks first.

**17a.10 The instance page loses the form.** At `#/instances/<id>` no request goes to
`/api/instances/<id>/server`, no `h2` `server-title` and no save bar render, and leaving asks only for
the tags (brief 15's rule). After the tags field, a `section` with `h2` `instance-server-title` holds a
Value list row `instance-server-title` / the list item's `serverProfile` as a link to
`#/profiles/server/<name>`, or `instance-server-none` (class `muted`) when it is `null` or absent. It
follows each list answer.

**17a.11 Wrapping.** At 320 px and at 200 % text, a 32-character id and 8 tags cause no sideways
scroll: the table scrolls in its box (`fe/DESIGN.md` Table), the `h1` and the `dd` wrap (Value list).
No new CSS class.

**17a.12 Strings.** The strings file gains exactly 17a's keys of section 2 and loses `server-intro`.

Tests that change: every test that opens the Server form at `#/instances/<id>` or stubs
`/api/instances/<id>/server` moves to `#/profiles/server/<id>` and `/api/profiles/server/<id>`
(`ServerSave`, `SaveAlertClean` and `test/savealert.tsx`, `LeaveGuard`, `Timeout`, `SessionEnded`,
`SessionRevoked`, `HearthSaveBarScroll`, `SignIn`, `SignInOnly`, `HearthSignIn`, and the Server parts of
`InstancePage`, `InstanceTags`, `Boot`, `RunControls`); `test/bff.ts` gains the two server profile
reads; `ProfilesNav.test.tsx` and `HearthNav.test.tsx` gain the second Profiles link; `05b`'s 5b.2 ("no
Server profiles text") is retired.

### 17b: create and delete a server profile (10)

**17b.1 The create form opens.** On the list page, after the intro and before the table (or the empty
text), a secondary button `server-profiles-new` with the `plus` icon (brief 15's; added here if 15 is
not built yet) and `aria-expanded="false"`. Pressing it sets `true` and shows right after it a `form`
named by its `h2` `server-profiles-new-title`, with the fields Name (`server-profile-name`,
hint `server-profile-name-hint`, text input, `autocomplete="off"`) and Port (`server-port`, hint
`server-profile-port-hint`, `inputmode="numeric"`), then the buttons `create-button` (primary) and
`create-cancel`, then a `role="status"` line and the alert place. The focus moves to Name. Shown also
when the list is empty; not shown while loading or after a failed read. Each time the form opens, Port
holds the default: the smallest whole number from 9931 to 65535 that is not the `port` of any row of
the list the page last read (rows with port 0 count for nothing), computed in the page with no request;
empty only if every one is taken. Examples: `[]` → `9931`; ports 9931 and 9932 → `9933`; ports 9931
and 9933 → `9932`; port 9930 only → `9931`. The administrator may change or clear it.

**17b.2 Cancel.** Cancel, or pressing the opener again, hides the form, clears its fields, errors and
alert, sets `aria-expanded="false"` and puts the focus on the opener; opening it again fills Port with
the default anew (17b.1).

**17b.3 Client checks.** On Create: an empty Name shows `field-error-required` under it; a Port that is
not a whole number from 1 to 65535 (empty included, so a cleared default is refused in words) shows
`field-error-number` with 1 and 65535. Each
invalid field gets `aria-invalid="true"` and `aria-describedby` its error; the focus moves to the first
invalid field after its error renders; nothing is sent.

**17b.4 The request.** A valid form sends `POST /api/profiles/server` with
`{"engine": "llama.cpp", "id": <Name as typed, trimmed>, "port": <number>}`. Meanwhile the status
reads `create-creating` and both buttons are `aria-disabled`; a second Create sends nothing. 201 goes
to `#/profiles/server/<id>` (the reply's `id`), whose `h1` takes the focus.

**17b.5 Refusals.** A refusal with `field` `id` or `port` shows its text (section 2,
`writeFailure`) under Name or Port, marks it as 17b.3 and moves the focus there; `field` `engine` and
every refusal without a field show the text in the form's alert (`role="alert"`). 409
`client-server-profile-exists` → `api-client-server-profile-exists` in the alert. 503 → `create-unknown`
in the alert and the list is read again. 401 → `session-ended-retry` with a Sign in button opening the
sign-in Dialog; after a sign-in the focus returns to Create and the typed values stay. The fields keep
what was typed through every failure.

**17b.6 Delete is offered while unused.** On a profile's page, when the document's `instance` is
`null`, a secondary button `server-profile-delete` follows the Value list; when it names an instance
there is no button. It follows each document read.

**17b.7 Confirm in place.** Pressing it shows, right after it, a `div` with `role="group"` named by
(`aria-labelledby`) its paragraph `server-profile-delete-question` (the id filled in), then the
buttons `confirm-delete` and `confirm-cancel`; the focus moves to `confirm-cancel`. Cancel or Escape
inside the group hides it and puts the focus on the opener. Nothing is sent before `confirm-delete`.

**17b.8 The delete.** `confirm-delete` sends `DELETE /api/profiles/server/<id>`; meanwhile a
`role="status"` line in the group reads `confirm-deleting` and both buttons are `aria-disabled`. 204
replaces everything after the `h1` (the Value list, the form, the save bar) by a paragraph
`server-profile-deleted` (`tabindex="-1"`, takes the focus) and a link `server-profile-back`; the
unsaved-changes question no longer asks (the file is gone).

**17b.9 Delete refusals.** In the group's alert, by `writeFailure`: 409 `client-server-profile-in-use`
(then the document is read again, so the Used by row names the instance and the button goes); 404
`client-no-server-profile` (the page then reads again and shows `server-profile-not-found`); 503, 502,
401 (Sign in button, focus back to `confirm-delete` after it), 403.

**17b.10 Strings.** The strings file gains exactly 17b's keys of section 2.

Tests that change: none beyond the new ones (`test/bff.ts` gains the POST and DELETE). The 17b.1
test asserts Port's value for the four example lists and that opening the form sends no request; the
17b.3 test clears Port before Create.

### 17c: create and remove an instance (12)

**17c.1 The create form opens.** On the Overview, after the tiles (or `instances-empty`), inside the
instances `section`, a secondary button `instances-new` (`plus`, `aria-expanded`), as 17b.1; it opens a
`form` named by an `h3` `instances-new-title` (the section's heading is the `h2`), with
Name (`instance-name`, hint `instance-name-hint`) and Server profile (`instance-new-server`, hint
`instance-new-server-hint`, a native `select`), the paragraph `instance-new-note`, Create and Cancel,
status and alert. Focus to Name; Cancel as 17b.2. Not shown while the list loads or after it failed.

**17c.2 The server profiles of the form.** Opening the form sends one `GET /api/profiles/server`
(while pending, `instance-new-loading` in place of the select, `role="status"`). The select's first
option is `instance-new-server-choose` (value empty), then one option per row whose `instance` is
`null`, in the reply's order, text `instance-server-option` (id, port), or the id alone for port 0.
No such row: in place of the select the paragraph `instance-new-no-free` and a link
`instance-new-profiles-link` to `#/profiles/server`, and Create is `aria-disabled`. A failed read: the
text of 17a.4 for that reply in place of the select (a 401 shows the Sign in button of Action feedback,
not a sign-out), Create `aria-disabled`.

**17c.3 Client checks.** An empty Name or the empty option shows `field-error-required` under that
field, as 17b.3; nothing is sent.

**17c.4 The request.** `POST /api/instances` with `{"engine": "llama.cpp", "id": <Name trimmed>,
"serverProfile": <chosen id>}`; status `create-creating`, buttons `aria-disabled`. 201: the app's
instance list is read again, then (whatever that read answers) the app goes to `#/instances/<id>`,
whose `h1` takes the focus; the nav lists the new instance once that read answered.

**17c.5 Create refusals.** By `writeFailure`: `field` `id` → under Name; `field` `serverProfile`
(`client-address-in-use`, `client-unresolvable-host`) → under the select; `unknown-engine` and every
refusal with no field (`instance-exists`, `client-server-profile-in-use`, `client-no-server-profile`,
`client-service-failed`, another 422 as `api-other`) → the alert; after `client-server-profile-in-use`
and `client-no-server-profile` the profiles are read again (the chosen one may leave the select, which
then shows `instance-new-server-choose`). 503 → `create-unknown` and the instance list is read again.
401 / 403 as 17b.5. Typed values stay.

**17c.6 Remove is offered.** On the instance page, after every section, a secondary button
`instance-remove` when the list item's `removable` is `true`; none when it is `false` or absent. It is
`aria-disabled` unless the state is `inactive` or `failed`, and then described by (`aria-describedby`)
a paragraph after it, `instance-remove-stop-first`, shown only while it is inactive that way. It
follows the polled state.

**17c.7 Confirm and remove.** Pressing an active Remove shows the Confirm in place group of 17b.7 with
`instance-remove-question` (id), `confirm-remove` and `confirm-cancel`, focus on Cancel. `confirm-remove`
sends `DELETE /api/instances/<id>`, status `confirm-removing`. 204: the app's instance list is read
again (the nav drops the link), and everything after the `h1` is replaced by the paragraph
`instance-removed` (`tabindex="-1"`, focused) and a link `not-found-home` to `#/`; the page does not
then show `instance-not-found` though the list no longer has the id.

**17c.8 Remove refusals.** In the group's alert, by `writeFailure`: 409 `instance-running`, 409
`instance-protected`, 404 → `instance-not-found`, 503, 502 `client-service-failed`, 502, 401 (Sign in,
focus back to `confirm-remove`), 403. The group stays open after a refusal.

**17c.9 Start without a model profile.** A Start or Restart answered 409 `instance-no-model-profiles`
shows `api-instance-no-model-profiles` with the id in the run alert (today: `request-failed`).

**17c.10 Keyboard.** The Overview's Tab order after the tiles' links: New instance, then (open) Name,
Server profile, Create, Cancel. The instance page's Tab order ends with Remove instance, then (open)
`confirm-remove`, `confirm-cancel` (DOM order: question, Remove, Cancel; the focus lands on Cancel,
and Shift+Tab reaches Remove). The same order holds for 17b's group.

**17c.11 Wrapping.** At 320 px and 200 % text the forms and the group cause no sideways scroll; a
32-character option text wraps or truncates inside the native select, never widening the page
(`select { max-width: 100% }` if needed).

**17c.12 Strings.** The strings file gains exactly 17c's keys of section 2.

Tests that change: `HearthTiles`/`InstanceTiles` and `Instances` (the button after the tiles),
`RunControls` (the 409), `InstancePage` (the Remove button after the sections); `test/bff.ts` gains
`removable`, `serverProfile`, POST and DELETE.

### 17d: the restart offer and the server profile switch (11)

**17d.1 Restart offer.** A component given a list of instance ids renders, for each, a secondary button
`restart-offer-button` (id) with the `rotate-cw` icon, in a wrapping row, then one `role="status"` line
and the alert place (Action feedback). An empty list renders nothing.

**17d.2 Its click.** A button sends `POST /api/instances/<id>/restart`; meanwhile every button of the
offer is `aria-disabled` and the status reads `restart-offer-running` (id). 204 → `restart-offer-sent`
(id). Failures by `writeFailure` plus `instance-no-model-profiles` (17c.9); 401 shows the Sign in
button, focus back to the clicked button after it. The offer stays until the next write it belongs to.

**17d.3 Server profile page.** A save answering `running: true` with the document's `instance` set
shows `save-saved-restart-ids` (that id) in the save bar and the offer for it right after the save bar;
with `instance` `null`, `save-saved-restart` and no offer. An edit removes the offer.

**17d.4 Model profile page.** A save answering `restart` with ids shows `save-saved-restart-ids` (ids
joined by ", ") and the offer for them after the save bar; `running: true` with `restart: []` keeps
`save-saved-restart` and no offer; `running: false` `save-saved`. An edit removes the offer.

**17d.5 Model profiles list.** A switch answering `restart` with ids shows
`model-profiles-switched-on-restart-ids` or `-off-restart-ids` (id, ids joined) in the table's status
region and the offer right after it; `running: true` with `restart: []` keeps today's texts; the next
switch removes the offer.

**17d.6 The switch control.** In the instance page's Server profile section (17a.10), when a profile
is set the link's text becomes `instance-server-open` (name); the row's `dt` stays. After the Value
list: a `select` labelled `instance-server-switch-label`, hinted `instance-server-switch-hint`, its
options `instance-new-server-choose` then the rows of `GET /api/profiles/server` (read when the page
opens and after each switch) whose `instance` is `null`, as 17c.2; then the button
`instance-server-switch`, `aria-disabled` while the empty option is chosen; then status and alert. No
such row: `instance-server-no-free` and the link `instance-new-profiles-link`, no select, no button.

**17d.7 The switch.** The button sends `PUT /api/instances/<id>/profiles/server` with
`{"serverProfile": <chosen>}`; status `instance-server-switching`, button `aria-disabled`. 200
`running: false` → `instance-server-switched` (name); `running: true` →
`instance-server-switched-restart` (name) and the offer for this instance. Then the instance list and
the profiles are read again, the select back on its empty option. Changing the select sends nothing.

**17d.8 Switch refusals.** `field` `serverProfile` (`client-address-in-use`,
`client-unresolvable-host`, `load-on-startup-over-max` and any other 422 with that field) → the text
under the select, `aria-invalid`, focus to the select; no field → the alert. After 404
`client-no-server-profile` and 409 `client-server-profile-in-use` the profiles are read again. 404
`client-no-instance` → `instance-not-found`. 503, 502, 401 (Sign in, focus back to the button), 403.

**17d.9 Keyboard and focus.** The offer's buttons follow the status line they belong to in DOM order;
no focus moves when an offer appears (it is announced by the status line). The focus stays on the
switch button through a switch.

**17d.10 Wrapping.** At 320 px and 200 % text the offer's buttons wrap, one per line if needed; no
sideways scroll.

**17d.11 Strings.** The strings file gains exactly 17d's keys of section 2.

Tests that change: `ModelProfileSave2`, `ModelProfilesSwitch` and `ServerSave` (the restart texts with
`restart` ids); the `-restart` fallbacks stay.

### 17e: the instance's model profiles (11)

**17e.1 Read.** The instance page, once the list has the instance, sends one
`GET /api/instances/<id>/profiles/model` (and again after each write's failure, and after a sign-in as
another user). A `section` with `h2` `instance-models-title`, the paragraph `instance-models-intro`;
while pending `instance-models-loading` (`role="status"`).

**17e.2 Read failures.** 401 signs the app out and 403 `session-not-administrator` as revoked, as the
page's other reads; 404 `client-no-instance` → `instance-not-found`; 503 `api-client-unavailable`;
502 `client-not-authorized` → `api-client-not-authorized`; anything else, or a reply without an `all`
string and a `profiles` array → `instance-models-failed`, a plain paragraph. A row without a non-empty
string `id` is left out; a non-boolean `effective` or `enabled` reads `false`.

**17e.3 All switched-on.** A checkbox (a statement: `fe/DESIGN.md` Switch) in a group labelled
`instance-models-all-label`, its own label `instance-models-all`, checked when `all` is `inherit`.
Changing it sends `PUT /api/instances/<id>/profiles/model` with `{"all": "inherit"}` or
`{"all": "none"}`; 200 → `instance-models-all-on` / `-all-off`, or `instance-models-all-restart` with
the offer for this instance when `running` is true; then the table is read again.

**17e.4 The table.** After the checkbox, a Table with the columns `model-profiles-column-name`,
`instance-models-column-pool`, `instance-models-column-here`, one row per profile in the reply's
order: the id as a link to `#/profiles/model/<id>`; `model-profiles-status-on` or `-off` from
`enabled`; a Switch (`role="switch"`) named by the id (`aria-label` the id, as the list's switch),
checked when `effective`, with the word On / Off beside it. `profiles: []` → `instance-models-empty`
and no table. The table's status region (one, just above it) and alert follow Action feedback.

**17e.5 One write at a time.** While any write of the section runs, the checkbox and every switch are
`aria-disabled` and a click sends nothing; the status reads `instance-models-saving`.

**17e.6 The word sent.** Switching a row on sends `PUT /api/instances/<id>/profiles/model/<profile>`
(percent-encoded) with `{"value": "inherit"}` when its `enabled` is true, else `{"value": "on"}`.
Switching off sends `{"value": "off"}` when `all` is `inherit` and `enabled` is true, else
`{"value": "none"}`. (Each leaves `effective` as asked under the rule of `pai_core::assign`.)

**17e.7 The answer.** 200: the row's switch takes the reply's `effective` (not the one asked); the
status reads `instance-models-on` / `-off` (id) by that `effective`, or `-on-restart` / `-off-restart`
with the offer for this instance when `running` is true. No re-read on success.

**17e.8 Refusals.** In the alert, by `writeFailure`: 422 `model-profile-over-memory`,
`model-profile-model-missing`, `load-on-startup-over-max`, `load-on-startup-unlimited` (their texts
exist), another 422 → `api-other`; 404 `no-model-profile-named`; 404 `client-no-instance`; 503, 502
`client-not-authorized`, 502; 401 (Sign in, focus back to the switch used), 403. After any failure the
table is read again, so each switch shows what the record holds; the focus stays on the switch used.

**17e.9 The checkbox's failures.** A failed `all` write shows its text in the same alert by
`writeFailure` (400 `assignment-malformed` → `request-failed`), the section is read again, so the
checkbox shows what the record holds, and the focus stays on the checkbox. The page never blocks Start
on its own count of served profiles: the manager decides (17c.9).

**17e.10 Wrapping and keyboard.** At 320 px and 200 % text the table scrolls in its box; Tab goes
checkbox, then per row the id link and the switch; Space toggles a focused switch.

**17e.11 Strings.** The strings file gains exactly 17e's keys of section 2.

Tests that change: `InstancePage` (a section after Server profile), `test/bff.ts` (the three routes).

## 4. Accessibility (WCAG 2.2 AA)

- Headings: list page h1 > table; profile page h1 > h2 Server > h3 cards; instance page h1 > h2
  Server profile, h2 Model profiles; the Overview's create form an `h3` under the `h2` host
  ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
  [2.4.6](https://www.w3.org/TR/WCAG22/#headings-and-labels)).
- Every field has a visible label that is its accessible name; hints and errors are linked by
  `aria-describedby`; errors are words beside the field, with the rule in them
  ([3.3.1](https://www.w3.org/TR/WCAG22/#error-identification),
  [3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions),
  [3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion)).
- The opener of a create form exposes `aria-expanded`
  ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value),
  [APG Disclosure](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/)); the Confirm in place group
  is named by its question, so focus on Cancel reads "Cancel, button, group, Delete the server profile
  x?…" ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships)); focus never drops when a
  control turns inactive (`aria-disabled`) or a group closes
  ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)).
- Every outcome is a status line, every failure after an action an alert
  ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)); the Restart offer appears silently next to
  the announced status that names the instance to restart.
- Delete and remove ask first and say what is lost
  ([3.3.4 Error Prevention](https://www.w3.org/TR/WCAG22/#error-prevention-legal-financial-data) is
  for legal and financial data; applied here by choice).
- A 401 on a write keeps what was typed and offers sign-in in place
  ([3.3.7](https://www.w3.org/TR/WCAG22/#redundant-entry)).
- Switches carry the word On / Off; "Served here" never by colour alone
  ([1.4.1](https://www.w3.org/TR/WCAG22/#use-of-color)); the Restart buttons' names start with their
  visible text ([2.5.3](https://www.w3.org/TR/WCAG22/#label-in-name)).
- The same nav in the same order on every page ([3.2.3](https://www.w3.org/TR/WCAG22/#consistent-navigation));
  reflow at 320 px and 200 % text ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow),
  [1.4.4](https://www.w3.org/TR/WCAG22/#resize-text)); every target at least 24 px
  ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).

## 5. Out of scope

- Renaming a server profile or an instance; copying a server profile; a server profile of another
  engine (one engine per node).
- The server profile's Tags as an editable field (05d); the tags chips (brief 15).
- Creating, renaming or deleting a model profile; `GET /api/profiles/model/{id}/estimate` (not served).
- Choosing `inherit` versus `on` explicitly (decision 7), an engine control in the create forms.
- Hub mode (several nodes); start at boot of a new instance (its page has it, off).
- Watching the state of an instance restarted from the offer (its page does).

## 6. Questions for the user

- **Q1.** "Served here" hides the words `inherit` / `on` / `off` / `none` behind one switch
  (decision 7). Enough, or should an administrator be able to pin `on` on a profile switched on in the
  list (so switching it off in the list keeps it here)?
- Q2 is answered (user, 2026-11-21): the form fills in the first free port from 9931 (decision 9).
