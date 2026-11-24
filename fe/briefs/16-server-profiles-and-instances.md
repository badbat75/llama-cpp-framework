# Brief 16: server profiles, instances and their associations (slice 17)

`PLAN.md` step 10 items 13 and 13a (profiles and instances end to end); `ARCHITECTURE.md` "Nodes and
instances", slices 14, 15 and 16 (all served: `crates/bff/README.md` "The server profile pool", "The
association routes", "Creating and removing an instance"; `fe/briefs/api-contract.md`). The web UI part
is slice 17. It is split in five sub-slices of at most 12 criteria each, built in the order of the table (17f, added on 2026-11-23, comes before 17c), each with
its own tests. Criteria are numbered **17a.n** to **17e.n** here, and **test names cite them with the
brief's number first, `16 17a.4`**, because brief 17's UX pass already has tests named `17a.n`
(`U17aSkeletons.test.tsx`, its skeletons), as brief 20 cites `20 N.4`.

| Sub-slice | What | Criteria | Waits for |
|---|---|---|---|
| 17a | The Server profiles list, a server profile's page with the Server form moved there, the nav entry; the instance page stops reading `/api/instances/{id}/server` | 12 | TP1 (2j) committed: shared files, section 3 "17a and 2j" |
| 17b | Create and delete a server profile | 10 | 17a |
| 17f | The instance's name: a free label, else engine and port, the id small; Rename; the "not confirmed" texts of a 504 | 12 | 17b (it amends `writeFailure`, which 17b brings); the bff's label route (`ARCHITECTURE.md`, the architect's slice) built first |
| 17c | Create and remove an instance; the start refused for no model profile | 12 | 17a, 17f |
| 17d | The restart offer; the instance's server profile switch | 11 | 17a, 17f, and the whole of 2j (17d.4 edits the model profile page) |
| 17e | The instance's model profiles | 11 | 17a, 17f |

The old routes `GET`/`PUT /api/instances/{id}/server` are removed from the bff once no fe code calls
them (`ARCHITECTURE.md` slice 14, "The old route stays"): after 17a none does, so the removal may ride
in 17a's commit or a later one (the coordinator's call); 17a's tests pin that the fe sends nothing to
them.

## 0. What exists (read on main at f666c49; `fe/` unchanged at 1dbcde8)

- **Routes served** (all behind a session, 401 `session-required` without one; `crates/bff/README.md`
  "The server profile pool"):
  - `GET /api/profiles/server`: `[{"id", "engine", "port", "tags", "instance"}]`, sorted by id,
    `instance` the id of the instance whose record names it or `null`, `port` 0 and `tags` `[]` for a
    file that cannot be read; 403 `session-not-administrator`, 503 `client-unavailable`, 502
    `client-other`.
  - `POST /api/profiles/server` `{"engine", "id", "port"}`: 201 `{"id"}`; 400
    `server-profile-malformed`; 422 `unknown-engine` (`field` `engine`), `server-profile-bad-id`
    `[id]` (`field` `id`), `not-whole-number-in-range` `[key, text, min, max]` (`field` `port`); 409
    `client-server-profile-exists` `[id]`; 503 (outcome unknown), 502.
  - `GET /api/profiles/server/{id}`: the instance route's server document plus `instance` (id or
    `null`); `defaults.StateDir` is `null` for a profile no instance runs; 404
    `client-no-server-profile` `[id]` (where the instance route answers `client-no-instance`).
    `PUT` is the instance's save (same body, statuses and keys, 504 `client-timeout` included), 200
    `{"revision", "running"}`, `running` for the instance that uses it (`false` for none, `true` when
    its state cannot be read). Since R3 (5fb7b0e) the save that **changes `Devices`** of a profile an
    instance runs can be 422 `model-profile-over-memory` `[profile, instance, needed, available]`
    with `field` the model profile's id, which names no control of the form. The form has no
    `Devices` control until R5 and sends the read's `Devices` back unchanged (`ServerForm.tsx` 429 to
    431), so this refusal cannot come from it today.
  - `DELETE /api/profiles/server/{id}`: 204; 409 `client-server-profile-in-use` `[profile,
    instance]`; 404 `client-no-server-profile` `[id]`; 503, 502.
  - `GET /api/instances` items carry `serverProfile` (name or `null`) and `removable` (false only
    for `default`).
  - `POST /api/instances` `{"engine", "id", "serverProfile"}`: 201 `{"id"}`; refusals listed in 17c.
    `DELETE /api/instances/{id}`: 204; 409 `instance-protected`, `instance-running`; 404
    `client-no-instance`. Both: 503 (outcome unknown), 502 `client-service-failed`, 502 `client-other`.
  - `POST /api/instances/{id}/start` and `/restart`: 409 `instance-no-model-profiles` `[id]` for an
    instance that serves no model profile (a new instance has none).
  - `PUT /api/instances/{id}/profiles/server` `{"serverProfile"}`: 200 `{"running"}`; since R3 also
    422 `model-profile-over-memory` with `field` `serverProfile`.
    `GET /api/instances/{id}/profiles/model`: `{"all": "inherit"|"none", "profiles": [{"id",
    "engine", "engineAvailable", "enabled", "model", "value", "viaAll", "effective"}]}`.
    `PUT .../profiles/model/{profile}` `{"value"}`: 200 `{"effective", "running"}`;
    `PUT .../profiles/model` `{"all"}`: 200 `{"running"}`. None of the four answers 504.
  - `PUT /api/profiles/model/{id}` and `PUT /api/profiles/model/{id}/enabled` answer `restart`: the
    ids of the running instances to restart, `[]` with `running` true when a state could not be read.
- **The fe** (`fe/src/`):
  - **Nav** (N1, brief 20; `App.tsx` 453 to 522 and 585 to 663): Overview alone; **Nodes** > the host
    label (`computer`) > one link per instance (`box`, seen `default · qwen-27b`, heard "default,
    server profile qwen-27b", from the list item's `serverProfile`) then Node info (`cpu`);
    **Profiles** (`p#nav-profiles` and `ul[aria-labelledby="nav-profiles"]`) with Model profiles
    (`layers`) only, as 20 N.8 rules "until 17a"; About last, alone. `Instance.serverProfile` is
    already read (`Instances.tsx`, 20 N.2): 17a adds no field to the list reader.
  - **Routes** (`route.ts`): `#/profiles/model`, `#/profiles/model/<id>`, `#/instances/<id>`,
    `#/node`, `#/about`; no `#/profiles/server`. `modelProfileIdUnreadable` is the `.`/`..` rule.
  - **Instance page** (`InstancePage.tsx` 440 to 512): the header (engine, port, state, Start / Stop /
    Restart, run status and alert, Start at boot), the Instance tags field as chips (brief 15 C1, C2:
    its own chips and the "From the server profile" ones, the pill suggesting from
    `readServerProfiles` in `Instances.tsx` 154 to 165, a silent read), then `ServerSection` /
    `ServerForm`, which read and save `/api/instances/{id}/server` (`ServerSection.tsx` 46,
    `ServerForm.tsx` 366). The run controls' alert texts are `failureText` (`InstancePage.tsx` 152);
    `instance-no-model-profiles` has none yet. The page's unsaved-changes rule tracks both sections
    (`report('server' | 'tags')`, 333 to 349).
  - **Loading** (U17a, `Skeleton.tsx`): every page's first read shows its loading text visually
    hidden in a `role="status"` at once and a skeleton after 300 ms (`Loading`, `Lines`, `CardBox`).
    The instance page's skeleton today is three header lines plus the Server section's card box
    (`U17aSkeletons.test.tsx` 241 and 262).
  - **Tags as chips** (`Chip.tsx`: `ChipList`, `Chip`), read-only on the Overview's tiles (U17b),
    one line with an ellipsis (user, 2026-11-21); a tile's chip takes no Tab stop because the tile's
    id link leads to a page showing every tag whole (`Chip.tsx` 67 to 70).
  - **The Server form's save** (`ServerForm.tsx` 545 to 585): a refusal whose `field` names a control
    marks it, any other refusal shows `save-refused` in the bar's alert; `serverKey` maps
    `client-no-instance` to `instance-not-found` and `server-changed-elsewhere` to Reload.
  - Icons present (`Icon.tsx`): `plus`, `x`, `rotate-cw`, `chevron-left`, `layers`, `computer`, `box`,
    `cpu` among them; **`server` is not there yet** (17a adds it).
- **`pai-config` and `pai`** have no server profile pool, no instance create or remove and no
  assignment: they act on the instance `default` and the profile its record names. The web UI is first;
  the Server form itself keeps the GUI's Server tab fields, order and texts (`fe/briefs/01-server.md`).

## 1. Decisions (each a preference unless it cites a source)

1. **Nav**: the Profiles group of N1 (brief 20) lists **Server profiles** then **Model profiles** (an
   instance has one server profile and many model profiles: the one first; brief 20 decision 6 and
   Q1, `fe/DESIGN.md` 2.3). Server profiles has the `server` icon (Lucide `server`, path data of
   `lucide-static@1.74.0` `icons/server.svg`, checked to answer 200 on 2026-11-22: two `rect`s of
   20 x 8 at y 2 and 14 with `rx`/`ry` 2, two `line`s at x 6 to 6.01, y 6 and 18; licence already in
   `installer/licenses/lucide.txt`). The Nodes group is unchanged: instance links keep `id · profile`.
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
| `instance-name-auto` | `{0} :{1}` (engine, port) | 17f |
| `instance-id-shown` | `ID {0}` | 17f |
| `instance-id-said` | `, instance {0}` | 17f |
| `instance-rename` | `Rename` | 17f |
| `instance-label` | `Label` | 17f |
| `instance-label-hint` | `A name of your own for this instance, shown in the menu, on the overview and on this page. Leave it empty to name the instance by its engine and port again. The ID does not change.` | 17f |
| `instance-label-save` | `Save label` | 17f |
| `instance-label-cancel` | `Cancel` | 17f |
| `instance-label-saving` | `Saving the label…` | 17f |
| `instance-label-saved` | `The instance is now named {0}.` | 17f |
| `instance-label-cleared` | `The label is removed: the instance is named {0} again.` | 17f |
| `api-instance-bad-label` | ``invalid instance label `{0}`: use 1 to 64 characters with no control character, or leave it empty to remove the label`` (proposed for the toml line IL1 adds; copied from what IL1 commits) | 17f |
| `api-instance-label-taken` | ``the label `{0}` is already used by instance {1}: pick another one`` (proposed, as above) | 17f |
| `write-timeout` | `Not confirmed: the manager did not answer in time, so this may or may not have been done. The page shows what the manager holds once it answers.` | 17f |
| `delete-unknown` | `Not confirmed: the manager did not answer in time, so it may or may not have been removed. The list shows it if it is still there.` | 17f |

Reused, unchanged: `server-title`, `server-loading`, `server-load-failed`, `server-port`, the whole
Server form's keys, `save-*`, `instance-not-found`, `session-ended-retry`, `session-revoked`,
`session-sign-in`, `api-client-unavailable`, `api-client-not-authorized`,
`api-client-service-failed`, `api-client-address-in-use`, `api-client-unresolvable-host`,
`api-not-whole-number-in-range`, `api-other`, `request-failed`, `field-error-number`,
`model-profiles-column-name`, `model-profiles-status-on`, `model-profiles-status-off`,
`api-no-model-profile-named`, `api-model-profile-over-memory`, `api-model-profile-model-missing`,
`api-load-on-startup-over-max`, `api-load-on-startup-unlimited`. `server-intro` is no longer used
after 17a and is removed then (`strings.test.ts`: every key used).

**One failure mapping for every new write** (`writeFailure(key, args)`, used by 17b to 17f; 17f.10
adds its 504 rule: a 504 `client-timeout` is "not confirmed", never `request-failed`): 401 →
`session-ended-retry` and the Sign in button; `session-not-administrator` → `session-revoked`; a key
listed with a text in this brief or already in `en_US.json` as `api-<key>` → that text filled with
`args`; `client-no-instance` → `instance-not-found` with the instance id; `client-unavailable` →
`api-client-unavailable` (for a create, `create-unknown` instead); any other 422 → `api-other` with the
key; anything else (400, 413, an unknown key, a malformed body, a network failure) → `request-failed`.

## 3. Criteria

### 17a: the list, the page, the form moved (12)

**17a.1 Routes.** `#/profiles/server` is the list page and `#/profiles/server/<id>` a profile's page
(the id percent-decoded; a bad escape is the not-found page, as `#/profiles/model/<id>`); `route.ts`
gains `serverProfileHref(id)` (`#/profiles/server/` plus `encodeURIComponent(id)`). Titles:
`Server profiles | Private AI Framework` and `<id> | Private AI Framework`; each `h1` takes the focus on
arrival. Signed out, the sign-in page shows (`signin-required`) and nothing is read. An id `.` or `..`
shows `server-profile-not-found` with no request (`modelProfileIdUnreadable`'s rule).

**17a.2 Nav.** Signed in, `ul[aria-labelledby="nav-profiles"]` holds two links in order:
`nav-server-profiles` to `#/profiles/server` with a first child `svg.icon` of `server`
(`aria-hidden`), then `nav-model-profiles` (unchanged). Server profiles has `aria-current="page"` on
the list and `aria-current="true"` on a profile's page, none elsewhere; Model profiles keeps its rule
(2d.64). Nothing else of the nav changes (20 N.3 to N.9; the Nodes group, its `id · profile` links and
About as they are). Tab order at 1280 px with two instances: Skip to content, home link, theme switch,
Sign out, Overview, the instance links, Node info, **Server profiles**, Model profiles, About, then the
page; below 960 px a click on Server profiles closes the open menu (20 N.10's rule). `Icon.tsx` gains
`server` (decision 1), with a comment citing `16 17a.2`.

**17a.3 List read.** The list page sends one `GET /api/profiles/server` per arrival (its own read with
the failures of 17a.4; `readServerProfiles`, the tag pill's silent read, is not reused and stays as
is). While pending, U17a's rule (`Loading`): `server-profiles-loading` visually hidden in a
`role="status"` at once, and after 300 ms three `.skeleton` lines in the table's place, as Model
profiles. Then `h1` `server-profiles-title`, the paragraph `server-profiles-intro`, and a table (Table
component, in `div.table-scroll`) with the columns `server-profiles-column-name`, `-port`, `-tags`,
`-used-by`, one row per item in the reply's order: the id as a link to `serverProfileHref(id)`; the port
as text (empty cell for 0); the tags as text joined by ", " in the reply's order (empty cell for none:
not chips, because a tile's chips take no Tab stop on the promise that the id link leads to a page
showing every tag whole, and the profile page does not show the tags until 05d; text wraps and hides
nothing); the instance id as a link to `instanceHref(id)`,
or `server-profiles-unused` with the class `muted`. `[]` shows `server-profiles-empty` and no table.
The columns do not sort (section 5).

**17a.4 List failures.** 401 signs the app out (sign-in page at the same hash, `signin-session-ended`,
brief 09 S1b.1); 403 `session-not-administrator` signs out with `session-revoked`; 503
`client-unavailable` shows `api-client-unavailable`; anything else, a network failure, or a reply that
is not an array shows `server-profiles-failed`, each a plain paragraph in place of the table. An item
without a non-empty string `id` is left out; a non-number `port` reads as 0, a non-array `tags` as
none (non-string entries dropped), a non-string `instance` as `null`.

**17a.5 The profile page.** Before the `h1`, a link `server-profile-back` to `#/profiles/server`
(as `model-profile-back`, `chevron-left` icon). After it the `h1` (the id), then, once the document
answered, a Value list (`dl.assets`) with one row: `server-profile-used-by` / the document's `instance`
as a link to `instanceHref(instance)`, or `server-profiles-unused` (class `muted`). Then the Server
section: `h2` `server-title`, the paragraph `server-profile-intro` with `server@<id>.ini`, and today's
Server form, its cards, fields, order and texts unchanged.

**17a.6 Read.** The page sends one `GET /api/profiles/server/<id>` (the id percent-encoded) per
arrival and none to `/api/instances/<id>/server`. Loading, U17a's rule: `server-loading` visually
hidden in a `role="status"` at once, and after 300 ms one `.skeleton` line (the Used by row) then
`CardBox` (a `.card` holding a `.skeleton-box`). 404 `client-no-server-profile` shows
`server-profile-not-found` with the id and a link `server-profile-back`, no `dl` and no form; 401 and
403 as 17a.4; 503 `api-client-unavailable`; anything else, or a body that is not a server document
(`isServerDoc`), `server-load-failed`.

**17a.7 Save.** Save sends `PUT /api/profiles/server/<id>` with today's body; every reply the
instance save handled is handled the same (statuses, keys, field marks, Reload after 409
`server-changed-elsewhere`, the Sign in Dialog after 401, the 504 and timeout texts). Two keys differ:
404 `client-no-server-profile` shows `server-profile-not-found` (id) in the bar's alert, where the
instance page showed `instance-not-found`; and a 422 whose `field` names no control of the form (R3's
`model-profile-over-memory`, `field` a model profile id) shows `save-refused` with its text
(`api-model-profile-over-memory`, args as sent) in the alert and marks no field. `running: false`
shows `save-saved`; `running: true` shows `save-saved-restart` and keeps it until the next edit (no
instance state is watched on this page; 17d replaces it with the offer).

**17a.8 Folders of an unused profile.** When the document's `defaults.StateDir` (or
`defaults.ModelsDir`) is `null`, that folder field's placeholder reads `server-folder-unused-default`;
a string default keeps `server-folder-instance-default`.

**17a.9 Leaving.** The profile page's unsaved-changes question (the Dialog of `fe/DESIGN.md` 2.3)
works as it did on the instance page: a nav link, Back, or a typed hash with unsaved edits asks first.

**17a.10 The instance page loses the form.** At `#/instances/<id>` no request goes to
`/api/instances/<id>/server`, no `h2` `server-title` and no save bar render, and leaving asks only for
the tags (brief 15's rule; the page's `report` keeps the tags alone). After the tags field, a
`section` with `h2` `instance-server-title` holds a Value list row `instance-server-title` / the list
item's `serverProfile` as a link to `serverProfileHref(name)`, or `instance-server-none` (class
`muted`) when it is `null`; it follows each list answer. The page's loading skeleton is the three
header lines only (U17a's card box went with the Server section).

**17a.11 Wrapping.** At 320 px and at 200 % text, a 32-character id and 8 tags of 32 characters cause
no sideways scroll on the list page: the table scrolls in its box (`fe/DESIGN.md` Table) and the tags
cell wraps between tags; on the profile page the `h1` and the `dd` wrap (Value
list, `overflow-wrap: anywhere` on the `dd` as `.assets dd` has it). No token. (Amended in the 17a
review, D1: the table takes the class `server-profiles` so its headers, port and Used by do not
break and its tags break at spaces; the box then scrolls.)

**17a.12 Strings.** The strings file gains exactly 17a's keys of section 2 and loses `server-intro`.

**Tests that change** (the tester names them before the red run):
- The Server form moves from `#/instances/default` and `/api/instances/default/server` to
  `#/profiles/server/default` and `/api/profiles/server/default` (the document gains `instance`):
  `ServerSave`, `ServerSaveKeys`, `SaveAlertClean` and `test/savealert.tsx`, `LeaveGuard`, `Timeout`,
  `SessionEnded`, `SessionRevoked`, `HearthSaveBarScroll`, `SignIn`, `SignInOnly`, `HearthSignIn`,
  `Boot`, `ModelProfileFullValue` (its Server form part, 688 to 700) and `ModelProfileSave7` (2h.159,
  359); the Server parts of `InstancePage`, `RunControls`, `InstanceTags`.
- `InstanceTagChips` (320 to 335, 511) and `InstanceTagSuggest` (207) place the tags before the
  `h2` `server-title`: that anchor becomes the `h2` `instance-server-title`; their `SERVER` stubs go.
- `U17aSkeletons.test.tsx` 241 (the instance page: three lines, no card box) and 262 (the server read's
  skeleton moves to the profile page, 17a.6).
- The nav's order lists gain Server profiles before Model profiles: `App.test.tsx`, `ProfilesNav`
  (5b.2 at 360 "nothing says Server profiles" and the not-found cases at 556 to 557 are retired),
  `HearthNav`, `HearthIcons` (H8a.2's nav icons gain `server`), `NodeNav` (13 N.11 order),
  `NodesMenu1`, `NodesMenu2` (20 N.8 "Model profiles only" and 20 N.10's order), `NodePage`,
  `ModelProfiles`, `ModelProfilesFilter`, `ModelProfileContent`, `InstancePage` (the lines brief 20
  section 5 lists).
- `test/bff.ts` gains `GET /api/profiles/server/{id}` and `PUT` there, and lets the list's reply be set
  per test (it has `GET /api/profiles/server` at 1537 for the pill).

**The UX check** (`fe/ux/`; the tester edits, the designer runs it):
- `stub.py`: `GET` and `PUT /api/profiles/server/default` answer today's instance document plus
  `"instance": "default"` (and `/api/instances/default/server` may go); `pool=long` adds to
  `SERVER_PROFILES` a row with a 32-character id, port 9931, 8 tags of 32 characters (the `LONG_OWN`
  list) and `"instance": null`, so the pill's suggestions in the other states are unchanged.
- `check.js`: the state `instance-server` becomes `server-profile` (`#/profiles/server/default`, the
  same markers, `h1` `default` and the Port label; renamed in `CHOICE_STATES` too); `instance-page`
  (`#/instances/default`, markers the `h1` and the `h2` `instance-server-title`); `server-profiles`
  (`pool=long`, `#/profiles/server`, markers the `h1` `server-profiles-title` and the long id's link).
  The 8 known target-size findings on the Hostname rows move with the form to `server-profile`
  (section 6, note N2).

**17a and 2j (brief 21, in progress).** 17a's tester starts once TP1 is committed: the working tree
holds TP1's uncommitted edits to `fe/src/test/bff.ts`, `fe/ux/stub.py` and `fe/ux/check.js`, which
17a also edits, and both add keys to `en_US.json`. One `fe/` chain at a time (`PLAN.md`); if TP2 or TP3
runs before 17a lands, the same holds. 17a does **not** touch `ModelProfilePage.tsx`,
`ModelProfileCards.tsx`, `GpuSplit.tsx`, `gpuBlocks.ts`, the tensor rule module,
`test/modelProfileCards.ts`, nor the `ModelProfileGpu*`, `ModelProfileLayers*`, `TensorPlacement*`,
`tensorRules` tests. The two model-profile-named tests it edits (`ModelProfileFullValue`,
`ModelProfileSave7`) touch only their Server form cases, which 2j does not edit.

### 17b: create and delete a server profile (10)

**17b.1 The create form opens.** On the list page, after the intro and before the table (or the empty
text), a secondary button `server-profiles-new` with the `plus` icon (in `Icon.tsx` since brief 15) and `aria-expanded="false"`. Pressing it sets `true` and shows right after it a `form`
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
(then the document is read again, so the Used by row names the instance and the button goes; the
in-use text then stays in a `p[role="alert"][tabindex="-1"]` right after the Value list, which takes
the focus: 17b review D2); 404 `client-no-server-profile` (the page then reads again and shows
`server-profile-not-found`, that paragraph `tabindex="-1"` and focused); 503, 502,
401 (Sign in button, focus back to `confirm-delete` after it), 403.

**17b.10 Strings.** The strings file gains exactly 17b's keys of section 2.

Tests that change: none beyond the new ones (`test/bff.ts` gains the POST and DELETE). The 17b.1
test asserts Port's value for the four example lists and that opening the form sends no request; the
17b.3 test clears Port before Create.

### 17f: the instance's name and the "not confirmed" texts (12)

Decided by the user, 2026-11-23: an instance can be given a free label; until it has one the web UI
names it by its engine and its server profile's port (`llama.cpp :9931`), and the id, which never
changes, is shown small beside it as the stable key. This replaces brief 20 decision 5 (the nav's
`id · server profile`): the menu entry is the name only (user, 2026-11-23); the server profile shows on
the instance page (17a.10).

**The API** (`ARCHITECTURE.md` "Instance labels", slice IL2; the tester takes the key names from
there and from this paragraph, which copies them): `GET /api/instances` items gain `label`, a string
or `null` (`engine` and `port` are already there, the port being the server profile's, 0 when
unread). `PUT /api/instances/{id}/label` takes `{"label": <string>}` and answers **204 with no
body**. The manager trims spaces and tabs at both ends; an empty label after the trim **removes**
the label (there is no `null` in the body). The rule (`checks::instance_label`): empty, or 1 to 64
characters with no control character and no `;` or `#`. Refusals: 400 `instance-malformed` (a body
that is not that object); 422 `instance-bad-label` `[label]` or `comment-markers` `[label]`, both with
`field` `label`; 409 `instance-label-taken` `[label, other-id]` (another instance has that label,
letter case ignored; not compared with ids or the engine-and-port name); 404 `client-no-instance`;
401 (nothing asked); 403 `session-not-administrator` (the session ends); 503 `client-unavailable`
(before the hand-over: nothing done); 504 `client-timeout` (after it: outcome unknown); 502
`client-other`. It works while the instance runs and needs no restart.

**17f.1 The name.** One function, `instanceName(item)`, used by every place below: the item's
`label` when it is a non-empty string (as the bff gives it, not translated); else
`instance-name-auto` with the engine and the port when the port is above 0; else the id. The reader
takes `label` as a non-empty string or `null` (missing, `null`, `""`, not a string). The id is shown beside
the name everywhere the name shows, except when the name IS the id (the last case), where it would
say the same twice.

**17f.2 The nav.** Each instance link of the host list (20 N.5) holds the `box` icon then
`span.nav-instance` holding **the name only** (user, 2026-11-23, closing N3): the label, or
`llama.cpp :9931`, or the id when that is the name. No server profile and no id in the entry; the
link's accessible name is the name. The `nav-instance-profile` and `nav-instance-profile-shown` keys
and the hidden and `aria-hidden` spans of 20 N.5 go. Labels are unique on the node (the manager's
rule), but two unlabelled instances can show the same `engine :port` (two stopped instances on one
port): the entries stay told apart by their order and their page, where the id shows (17f.4); the
menu does not add the id (the user's choice).

**17f.3 The tiles.** The Overview tile's link (`a.tile-id`) reads the name; under the tile's head a
`p.instance-id` reads `instance-id-shown` with the id (absent when the name is the id). The link's
accessible name is the name plus `instance-id-said` (visually hidden span inside the link), so two
tiles labelled alike are told apart (2.4.4).

**17f.4 The instance page.** The `h1` reads the name; right after it a `p.instance-id` reads
`instance-id-shown` (absent when the name is the id); the title is `<name> | Private AI Framework`.
The route stays `#/instances/<id>`. The Server profiles list's Used by cell and the profile page's
Used by row (17a.3, 17a.5) link with the name, the id after it in a `span.instance-id`
(`instance-id-shown`), so the list says which instance by the word the menu uses. Messages that
name an instance (`Restart {0}`, `instance-not-found`, the removal and switch texts) keep the id.

**17f.5 Rename opens.** In the instance header, after Start at boot, a secondary button
`instance-rename` with `aria-expanded="false"`. Pressing it sets `true` and shows right after it a
`form` with the field Label (`instance-label`, hint `instance-label-hint`, text input,
`autocomplete="off"`, `spellcheck="false"`), holding the current label (empty when none), its text
selected, the focus in it; then `instance-label-save` (primary) and `instance-label-cancel`; then a
`role="status"` line and the alert place. Cancel, Escape in the form, or the opener pressed again
closes it, drops the typing and puts the focus on the opener. Only one form is open at a time on the
page: the form is part of the header, not a page edit, so it does not arm the unsaved-changes
question.

**17f.6 The save.** Enter or `instance-label-save` sends `PUT /api/instances/<id>/label` with
`{"label": <the text trimmed>}`, `""` when it is empty (the clear). A trimmed text equal to the current
label (or empty when there is none) sends nothing and closes the form. Meanwhile the status reads `instance-label-saving` and both
buttons are `aria-disabled`; a second press sends nothing. 204: the form closes, the focus goes to
the opener, the status after the opener reads `instance-label-saved` with the trimmed text sent, or
`instance-label-cleared` with the name the item then has (`instance-name-auto`, or the id) for `""`; the app's instance list is read
again, so the `h1`, the title, the nav and the tiles follow (no optimistic name).

**17f.7 Refusals.** By `writeFailure`: 422 `instance-bad-label` → `api-instance-bad-label`, 422
`comment-markers` → `api-comment-markers` (both `field` `label`) and 409 `instance-label-taken` →
`api-instance-label-taken` (label, other id), each under the field, `aria-invalid="true"`,
`aria-describedby` the error, focus to the field; any other 422 → `api-other` in the alert; 400
`instance-malformed` → `request-failed`; 404 `client-no-instance` → `instance-not-found`; 503 → `api-client-unavailable`; 504
`client-timeout` → `write-timeout` in the alert, then the list is read again (the label shown is the
manager's); 401 → `session-ended-retry` and the Sign in button, focus back to `instance-label-save`
after the sign-in, the typing kept (3.3.7); 403 → `session-revoked`. The form stays open with what was
typed after every failure.

**17f.8 Long labels.** A label of the longest the rule allows (64 characters with no break point, and
one of 64 with spaces) and a 32-character id cause no sideways scroll at 320 px and 200 %
text: the `h1` wraps (`overflow-wrap: anywhere` as today), the nav's name breaks inside the pill
(`.nav-instance` keeps `anywhere` and its `flex: 1 1 4rem`), the tile's link and `p.instance-id` wrap in the tile
(`overflow-wrap: anywhere` on both: they are grid or flex items, the min-content trap of
`fe/DESIGN.md` 4), and the field takes the form's width. `index.css` gains `.instance-id` (`--fs-sm`,
`--muted`, `overflow-wrap: anywhere`, margin 0), which joins R1.8's list; `--muted` on `--bg` and
`--surface` is an existing token pair ([1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum)).

**17f.9 Keyboard and names.** The header's Tab order: Start, Stop, Restart, Start at boot,
Rename, then (open) Label, Save label, Cancel. The opener exposes `aria-expanded`
([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)); the field's name is its visible label and
its hint and error are described ([3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions));
outcomes are the status line, failures the alert
([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)); the focus never drops when the form closes
or its buttons go `aria-disabled` ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)).

**17f.10 "Not confirmed" for every write.** A 504 `client-timeout` (the bff's W1, W2: the call
reached the manager and its outcome is unknown) shows, in the alert of the write that got it:
- Start, Stop, Restart (`failureText`, `InstancePage.tsx` 152), Start at boot, the tags field, the
  label, and 17d's and 17e's writes: `write-timeout`; then what that place shows is read again (the
  list for the run verbs, tags and label, which the state poll already does; the boot read for Start
  at boot; the section's read in 17d and 17e).
- A create (17b, 17c): `create-unknown`, the list read again, as for a 503 today.
- A delete or remove (17b, 17c): `delete-unknown`, the document or the list read again.
A 503 `client-unavailable` stays `api-client-unavailable` (not asked, nothing done). `writeFailure`
gains the rule; the three run controls, boot and the tags field, which do not use it, get the same.
No 504 reaches `request-failed`.

**17f.11 The texts.** The strings file gains exactly 17f's keys of section 2 and loses
`nav-instance-profile` and `nav-instance-profile-shown`.

**17f.12 The UX check.** `fe/ux/stub.py`: the one instance answers `"label": null` and port 9931
(named `llama.cpp :9931`); `list=many` gives `default` a label `Coding assistant`, `coder` none with
a port, `embed` none and port 0 (named by its id), and the 40-character `MANY_ID` a 64-character
label with no break point; `PUT /api/instances/{id}/label` answers 204.
`check.js`: `instance-page` expects the `h1` `llama.cpp :9931` and `p.instance-id`; a new state
`instance-rename` (`#/instances/default`, click Rename, expect the Label field); `nav-open` expects the
64-character label in the menu; `overview-tiles` expects it and the tiles' small ids. The designer runs it.

**Tests that change**: every test that finds an instance link, a tile link or the instance `h1` by
the id (`App`, `NodeNav`, `NodesMenu1`, `NodesMenu2`, `HearthNav`, `HearthIcons`, `InstanceTiles`,
`HearthTiles`, `InstanceTileChips`, `Instances`, `InstancePage`, `RunControls`, `InstanceTags`,
`InstanceTagChips`, `ServerProfiles1`, `ServerProfilePage1`): with a fixture of `port` 0 and no label
the name is the id and they keep passing; the fixtures that set a port need the name. The 20 N.5
tests of `id · profile` are retired by 17f.2 (the entry is the name only). `test/bff.ts` gains `label` and the `PUT`.

**Out of scope of 17f**: a label in the create form (17c: the new instance is named by engine and
port until renamed); a label for a server profile; sorting or searching by label; the director's
view of several nodes (the same rule then, per node).

### 17c: create and remove an instance (12)

**17c.1 The create form opens.** On the Overview, after the tiles (or `instances-empty`), inside the
instances `section`, the list's + icon button named `instances-new` (`aria-expanded`), as 17b2.2; it opens a
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
whose `h1` takes the focus; the nav's host list shows the new instance as `<id> · <server profile>`
(20 N.5) once that read answered.

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
again (the nav drops the link; the host label and Node info stay, 20 N.6), and everything after the `h1` is replaced by the paragraph
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

Tests that change: `HearthTiles`/`InstanceTiles`, `InstanceTileChips` and `Instances` (the button after
the tiles; U17a's Overview skeleton is unchanged, the button not shown while loading),
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
`client-unresolvable-host`, `load-on-startup-over-max`, R3's `model-profile-over-memory` with its
text `api-model-profile-over-memory`, and any other 422 with that field) → the text
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
`restart` ids); the `-restart` fallbacks stay. 17d.4 edits `ModelProfilePage.tsx` (the save bar), so
17d starts only once the whole of 2j (TP1 to TP3) is committed.

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
- Sorting or filtering the Server profiles table (U17c sorts Model profiles; the pool is short and the
  bff sorts it by id). The server profile's `Devices` (R5's GPUs card, after slice 17).
- The Hostname rows' target size (note N2): 17a moves the form and changes none of its controls.

## 6. Questions for the user (notes: nothing waits on them)

- **Q1.** "Served here" hides the words `inherit` / `on` / `off` / `none` behind one switch
  (decision 7). Enough, or should an administrator be able to pin `on` on a profile switched on in the
  list (so switching it off in the list keeps it here)? Recommended: enough for now.
- Q2 is answered (user, 2026-11-21): the form fills in the first free port from 9931 (decision 9).
- **N2 (for the coordinator).** `PLAN.md` (step 10, the C1 note) asks to fix the Server form's Hostname
  checkboxes, an axe 2.5.8 target-size finding (8 per run), "in U17e or 17a, whichever touches the form
  first". 17a moves the form but changes no control, and its 12 criteria are full, so this brief leaves
  the fix to U17e, which edits the form's fields. If the fix should come with slice 17 instead, it fits
  17b (10 criteria) as one more criterion, and the designer writes it after measuring the rows.

- N3 is answered (user, 2026-11-23): the menu entry shows the name only, the label or engine and port,
  with no server profile and no id (17f.2).

## 17a review (designer, 2026-11-23)

I read the 17a diff (`ServerProfiles.tsx`, `ServerProfilePage.tsx`, `App.tsx` 491 to 508 and 759 to 773,
`InstancePage.tsx`, `ServerForm.tsx`, `route.ts`, `Icon.tsx`, `en_US.json`, `fe/ux/`) and drove it in
Chromium, Firefox and WebKit at 1280 and 320 px, light and dark, 100 % and 200 % text, with a Playwright
route serving the profile document (`build/ux-review/sp17a.js`, shots in `build/ux-review/sp17a/`).
**Verdict: two defects, D1 and D2; the rest is met.**

- 17a.1 met: both routes, titles `Server profiles | ...` and `<id> | ...`, the `h1` focused on arrival,
  `..` shows `server-profile-not-found` with no request, a bad escape is not found.
- 17a.2 met: Profiles holds Server profiles (`server` icon, path data as lucide-static 1.74.0) then
  Model profiles; `aria-current` `page` on the list and `true` on a profile's page. Tab order (WebKit,
  from the top): Skip, home, theme, Sign out, Overview, the instance, Node info, Server profiles, Model
  profiles, About. At 320 px a click on Server profiles closes the menu, focus on the `h1`.
- 17a.3, 17a.4 met in the code and the tests (`ServerProfiles.tsx` 52 to 70, 112 to 172): one read, the
  hidden status and three skeleton lines, the four columns, tags joined by ", ", `muted` No instance.
- 17a.5 to 17a.10 met: back link, `h1`, Used by row (a link or `muted` No instance), `h2` Server,
  `server@<id>.ini` in the intro, the form unchanged; the read and the save go to
  `/api/profiles/server/<id>`; 404 shows the not-found text; `client-no-server-profile` and R3's
  over-memory keys in the save (`ServerForm.tsx` 363, 386 to 401); `null` folders show
  `server-folder-unused-default`; the leave question (tests 378, 406); the instance page has no form,
  its Server profile section links to `#/profiles/server/qwen-27b` and its skeleton is three lines.
  Minor, accepted: the skeleton's Used by line renders under the `h2` Server rather than above it
  (`ServerProfilePage.tsx` 297 to 306).
- 17a.12 met: 18 keys added, `server-intro` gone.
- 17a.11 **not met, D1**: at 320 px the list does not scroll in its box; it squeezes. The base rule
  `th, td { overflow-wrap: anywhere }` (`index.css` 728 to 733) lets every column shrink to one
  character: the headers "Server profile" and "Used by", the port `8080` and the 32-character id stand
  one letter per line (`sp17a/webkit-320-dark-list.png`). No sideways page scroll anywhere (144 of 144
  pages), but the table is unreadable. The brief's "no new CSS class" caused it; amended: the table
  takes a class (`server-profiles`, beside `.model-profiles`) with `th { white-space: nowrap;
  overflow-wrap: normal }`, `td:nth-child(2), td:nth-child(4) { white-space: nowrap; overflow-wrap:
  normal }` and `td:nth-child(3) { overflow-wrap: normal }` (tags break at the ", " spaces; the id keeps
  `anywhere`). Measured by injection in the three engines at 320 px: the box scrolls (590 of 286 px at
  100 %, 1097 at 200 %), the id column 112 px, no sideways page scroll (`build/ux-review/sp17a-fix.js`,
  `sp17a/fix-firefox-false.png`). R1.8's class list gains `server-profiles`; a CSS test pins the rules.
- **D2 (fe/ux)**: `fe/ux/stub.py` 408 matches `p.count("/") == 5`, but `/api/profiles/server/default`
  has 4 slashes, so the stub never serves the profile document and the `server-profile` state cannot
  show the form; 416 answers 404 `no-server-profile-named` where the bff answers
  `client-no-server-profile`; no `PUT` is served there. Fix: `== 4`, the bff's key, and a `PUT` answering
  `{"revision", "running": false}`.
- `npm --prefix fe/ux run check` (156 states per browser, 2026-11-23): 30 failures, all D2. In each
  engine, width, theme and text mode the `server-profile` state shows no Port label because the stub
  serves no document. `server-profiles` and `instance-page` are clean in every engine, and the other
  states are unchanged. The 8 known Hostname target-size findings did not show only because the form
  never rendered; they come back once D2 is fixed (note N2). Re-run after D1 and D2.

**Re-check (2026-11-23): D1 and D2 fixed, 17a accepted.** `table.server-profiles` gets the nowrap rules
(`index.css` 798-819, `ServerProfiles.tsx` 139). At 320 px, measured in Chromium, Firefox and WebKit
with the long pool, the box scrolls: 590 of 286 px at 100 % text and 1092 to 1097 at 200 %. The
32-character id wraps in a 112 px column (202 px at 200 %), and nothing scrolls the page sideways
(`build/ux-review/sp17a/fix2-*.png`). `npm --prefix fe/ux run check`: 156 states per browser, 0
failing, `server-profile` among them. The 8 Hostname target-size findings of note N2 did not show in
this run. I have not checked why, so N2 stays open until U17e.

## 17b review (designer, 2026-11-23)

I read the 17b diff (`ServerProfiles.tsx` 165 to 605, `ServerProfilePage.tsx` 127 to 510,
`en_US.json`, `fe/ux/`) and drove it in Chromium, Firefox and WebKit (`build/ux-review/17b.js`,
shots in `build/ux-review/17b/`). **Verdict: two defects, D1 and D2.**

- 17b.1 and 17b.2 met. The opener has `aria-expanded`, and the form is named by its `h2`, with Name
  and Port (each with its About toggle), Create, Cancel, status and alert. The focus lands on Name.
  Port opens on 9932 when the pool holds 8080 and 9931. Cancel, Escape and the opener all close the
  form, with the focus back on the opener.
- 17b.4 met: the body, `aria-disabled` while sending, and a 201 going to the new page.
- 17b.6 to 17b.8 met. Delete shows only while the profile is unused. The group is named by its
  question, the focus is on Cancel, and Shift+Tab reaches Delete. A 204 replaces the page with the
  focused paragraph and the back link. Nothing scrolls sideways at 320 px and 200 % with the 32-character id
  (form and group).
- 17b.10 met, and the 17b keys are in the strings file. The tester's states `server-profiles-new` (the form open on the long
  pool) and `server-profile-delete` (the group open on the unused long profile) match 17b.1 and 17b.7.
- **D1 (17b.3, 17b.5): the focus moves before the error renders.** `ServerProfiles.tsx` 366 and
  424/429 set the errors and call `focus()` in the same tick, so the field is focused while it has
  no `aria-describedby` yet, and a screen reader can read "Name, edit text" without the error.
  There is no alert for a field error, so the error may never be heard. 17b.3 says "after its
  error renders". Fix: set the errors inside `flushSync`, then focus, as `ServerForm.tsx` 559 to
  579 does.
- **D2 (17b.9): after a 409 `client-server-profile-in-use`, the focus drops to `body` and the
  alert vanishes.** The read again (`ServerProfilePage.tsx` 271 to 279) names the instance, so the
  Delete button and its group unmount, with the focused button and the alert inside them. All three
  engines end with `document.activeElement` = `body` and no alert on the page
  ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order),
  [4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)). The 404 path (281 to 285) loses the
  focus the same way when the page turns into `server-profile-not-found`. 17b.9 is amended: after
  the 409's read again, the in-use text stays in a `p[role="alert"][tabindex="-1"]` right after the
  Value list (where the button was), and that paragraph takes the focus; after the 404's read
  again, the `server-profile-not-found` paragraph takes the focus (`tabindex="-1"`).
- `npm --prefix fe/ux run check`: 168 states per browser, 0 failing, the two new 17b states
  included. After D1 and D2 are fixed, a re-check of the focus is enough (`build/ux-review/17b.js`).

**Re-check (2026-11-23): D1 and D2 fixed, 17b accepted.** Checked in Chromium, Firefox and WebKit
(`build/ux-review/17b.js`, with a `focusin` log of each field's `aria-describedby` at the moment it
takes the focus):
- After a refused id, and after an empty Name, Name already carries its error when it is focused.
- After a 409, the focus is on `p#server-profile-in-use[role="alert"]` reading the in-use text, and
  Used by names `coder`.
- After a 404, the focus is on the `server-profile-missing` paragraph.
- Nothing scrolls sideways at 320 px and 200 %.

No UX check state changed, so the check was not re-run.

## 17b2: the + at the end of a list (user feedback, 2026-11-23)

The user, on the deployed build: "anche new server profile dovrebbe essere un + in coda alla lista"
(New server profile should be a + at the end of the list too). This follows the rule they set for
the tensor rules: an x and a + hold for every list (`fe/DESIGN.md` 5.2 "Icon buttons of a list").
Today the opener is a text pill with a `plus` icon, placed between the intro and the table
(`ServerProfiles.tsx` 172 to 175, 292 to 302). One sub-slice of 5 criteria, after 17f (both touch
`ServerProfiles.tsx` and `index.css`).

**17b2.1 Place.** The opener comes after the table, or after `server-profiles-empty` when the pool is
empty, outside the `.table-scroll` box. The create form opens right after it, below the list. Nothing
stands between the intro and the table any more. As before, there is no opener while loading or
after a failed read.

**17b2.2 Look.** The opener is the list's Add icon button (`fe/DESIGN.md` "Icon buttons of a list"):
- only the `plus` icon (`aria-hidden`), and a `span.visually-hidden` holding `server-profiles-new`;
- a 40 px square with the `--control` outline, drawn by the same rule as `.ot-add` (share the class
  or the rule, not a copy), left-aligned;
- `margin-top: var(--space-3)` from the table or the empty line, as 21 T.24;
- `ButtonText` under forced colours.
The id `server-profiles-new` and `aria-expanded` stay. Its accessible name stays exactly "New server
profile" ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)): a screen reader announces "New
server profile, button, collapsed", and a voice user can say "click New server profile". 2.5.3 does
not apply: there is no visible text.

**17b2.3 Behaviour unchanged.** 17b.1 to 17b.5 stand as written, apart from the place:
- opening moves the focus to Name;
- Cancel, Escape and the opener each close the form and put the focus back on the +;
- Port's default is computed as before.
The Tab order follows the page: the table's links, then the +, then the form when it is open
([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)). Tests that find the opener by its name or id
keep working; a test that asserts it comes before the table changes.

**17b2.4 The same rule on every list with an add.** Each opener is the + after its list, named by its
existing key:
- the Overview's New instance (17c.1, not built): after the tiles or `instances-empty`;
- the Models list's New model profile (brief 22, 22 C.1, not built): after the table or the empty
  line, outside the filter.
Delete becomes a − icon button beside the page's status, still with Confirm in place (user, 2026-11-23: brief 22, 22D2, 22 D2.8 for the server profile page).
Buttons that are not a list's add keep their words (Show quants, the Save bar).

**17b2.5 The UX check.** The `server-profiles-new` state still opens the form through the + (the
selector by id or by name, the hidden text included), and nothing scrolls sideways at 320 px or
200 %.

## 17f review (designer, 2026-11-23)

Read on the uncommitted 17f diff in `fe/` and driven with Playwright against `fe/ux/stub.py`
(`list=many`, `pool=long`), with the label route fulfilled per case: Chromium, Firefox and WebKit at 1280
and 320 px, light and dark, and at 200 % text (scripts `build/ux-review/17f*.js`, not committed).

Met:

- 17f.1 to 17f.3: the nav reads the name only ("Coding assistant", "llama.cpp :8580", "embed", the
  64-character label); the tiles read the name plus the hidden ", instance <id>", and the `p.instance-id`
  sits under the head, absent for `embed` (name = id).
- 17f.4: the `h1` "Coding assistant", the "ID default" line, the title "Coding assistant | Private AI
  Framework"; the route stays the id.
- 17f.5, 17f.6: the opener has `aria-expanded`; the box opens prefilled, selected and focused; Escape and
  Cancel close it with the focus on Rename; one PUT for Enter pressed twice while saving (both
  buttons `aria-disabled`); a 204 closes the form, the focus goes to Rename, the status reads the saved
  text and the list is read again; the clear sends `{"label":""}` and says "named llama.cpp :8080 again".
- 17f.7: 409 `instance-label-taken` goes under the field, with `aria-invalid`, described and focused,
  the same after Save label; a 504 shows `write-timeout` in the alert and reads the list once more; a 403
  shows `session-revoked` and a 401 shows `session-ended-retry`, both with Sign in; the typed text stays
  in all three engines.
- 17f.8 with the form closed: no sideways scroll on the Overview, the default page, the 40-character id's page or the pool,
  in any engine, width, theme or text size; the long label and id wrap in the nav, the `h1` and the
  id line.
- 17f.9: the Tab order is Start, Stop, Restart, (About), Start at boot, Rename, then the form.
- 17f.10, 17f.11: `client-timeout` is mapped for the run verbs (`failureText`), boot, tags, the label,
  the create (17b.5) and the profile delete (`delete-unknown`); the strings are added and the two
  `nav-instance-profile*` keys removed.
- The + / minus rule (`fe/DESIGN.md` principle 8) does not reach Rename: it neither adds to a list nor
  deletes.

Defects:

- **D1 (17f.4) The Used by link runs the name into the id.** `InstanceLink` puts
  `span.instance-id` inline right after the name with no space (`fe/src/Instances.tsx:336-338`), so the
  cell shows "Coding assistantID default" and the link's accessible name says the same, in the three
  engines ([2.4.4](https://www.w3.org/TR/WCAG22/#link-purpose-in-context)). Fix: a space before the
  span, and the span on its own line (`display: block`) as on the tile and the page.
- **D2 (17f.6) Enter on an unchanged label reopens the form in Chromium and WebKit.** The Enter branch
  of the box's `onKeyDown` (`fe/src/InstancePage.tsx:958`) closes the form and moves the focus to
  Rename without `preventDefault()`, so the same key then presses Rename and the form opens again with
  the text selected (Firefox closes it). Fix: `e.preventDefault()` in that branch, which also stops the
  implicit submit from calling `save` a second time.
- **D3 (17f.5, `fe/DESIGN.md` principle 9) The rename form has no layout.** The `<form>`
  (`fe/src/InstancePage.tsx:937`) has no class: the Label text sits inline against the box (the label
  ends at x 305 and the box starts at 305 at 1280 px, and at 57 px at 320 px), so the focus ring covers
  its last letter; the form starts 0 px under Rename (principle 9 asks `--space-3`); Save label and
  Cancel touch. At 1280 px, opening it also moves Start at boot's checkbox to a row of its own. With the form open, the UX check's `instance-rename` state scrolls sideways at 320 px and 200 % text in the three engines (scrollWidth 411 to 586 > 320; the box's own width beside the inline label), against 17f.8 "the field takes the form's width" ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)). Fix: a
  class on the form that stacks like the page's other fields: the label above the box (`display:
  block`, `--space-1` under it), the box at the form's width up to the field width the Server form uses,
  then the hint, the error, and the two buttons in a row with `--space-2` between them; `margin-top:
  var(--space-3)` from Rename; the form taking the header's full row.

UX check: 6 failing of 180 states per engine, all six this overflow (D3); nothing else, no axe finding.

Accepted once D1 to D3 are fixed and re-checked.

### 17f re-check (designer, 2026-11-24)

The fixes were re-checked in Chromium, Firefox and WebKit at 1280 and 320 px and at 200 % text.

- D1 met: the Used by link reads "Coding assistant ID default", with the id on its own line
  (`.server-profiles .instance-id`, `display: block`).
- D2 met: Enter on an unchanged label sends nothing, closes the form and leaves the focus on Rename,
  in all three engines.
- D3 met: `.instance-label-form` starts 12 px (`--space-3`) under Rename, and the buttons are 8 px
  (`--space-2`) apart. At 1280 px the label and the box take the same two columns as the header's
  other fields; at 320 px the label sits above the box; at 320 px and 200 % the buttons wrap. Nothing
  scrolls sideways.
- Start at boot's checkbox is at the same height before and after the form opens. The row of its own
  that WebKit gives it at 1280 px was there before 17f.

UX check: 0 failing of 180 states per engine.

**17f accepted.**
