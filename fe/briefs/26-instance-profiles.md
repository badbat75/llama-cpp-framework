# Brief 26: instance profiles (IP6)

The designer's brief for slice IP6 (`PLAN.md`, `ARCHITECTURE.md` "Instance profiles", accepted
2026-11-26, with its "placement and memory" amendment). It covers:
- the instance profile list and page;
- the per-model entry page, which takes the placement and memory cards;
- the instance page's instance profile picker and address;
- the restart offer.

It revises brief 16 (17a's create form, 17d, 17d2 withdrawn, 17e gone, the server profile's address)
and moves briefs 14 (GPU distribution), 19 (layer sliders) and 21 (tensor placement) from the model
profile page to the entry page. Brief 22's IP5 is the model profile half and lands first.

Patterns reused unchanged (`fe/DESIGN.md` 5.2):
- the list's `+` after the table, which opens a create form in place;
- the row's trash can with its tip and its Confirm in place (CD's red verb);
- the sticky bottom Save bar (L2.B1), with a switched-off Delete's tip (L2.B2);
- names by the id alone (D4.37);
- the restart offer (17d.4a);
- Action feedback for one-control writes.

## 0. What the bff serves (IP4)

The contract is settled: `fe/briefs/api-contract.md` "IP4: the instance profile routes (proposed)",
copied from `ARCHITECTURE.md` IP4 (60188bde), which is the source. The table below is this brief's
view of the same routes, in the brief's keys; where the two differ, `api-contract.md` is what the bff
is built to. **None of these routes is served yet** (the bff's slices are IP4c and IP4d), so the
tester stubs them all.

| Route | Answer | Refusals the page shows |
|---|---|---|
| `GET /api/profiles/instance` | `{rows: [{id, server, models: [<model id>…], usedBy: [<instance id>…]}]}`, in file order | 503, 401, 403 as every list |
| `POST /api/profiles/instance` `{id, server}` | 201 `{id}` | 422 `invalid-instance-profile-id` (field `id`), 409 `instance-profile-name-taken` (field `id`), 422 `no-server-profile-named` (field `server`) |
| `GET /api/profiles/instance/{p}` | `{id, server, models: [{id, keys: <count of entry keys>}], usedBy: […], revision}` | 404 `no-instance-profile-named` |
| `DELETE /api/profiles/instance/{p}` | 204 | 409 `instance-profile-in-use` `[instance]` |
| `PUT /api/profiles/instance/{p}/server` `{server}` (no revision) | `{running: [<instance id>…], notConverged: [{instance, key, args}…]}` | 422 `no-server-profile-named` (field `server`), 404 `no-instance-profile-named`. The device rule answers nothing: an instance that no longer fits is a `notConverged` row |
| `POST /api/profiles/instance/{p}/models` `{model}` (no revision) | 201, the affected answer as above | 422 `no-model-profile-named` (field `model`), 409 `instance-profile-model-listed`, 404 |
| `DELETE /api/profiles/instance/{p}/models/{m}` | the affected answer | 404 `no-instance-profile-model` |
| `GET /api/profiles/instance/{p}/models/{m}` | `{profile: p, server, model: m, values: {<placement key>: <value>…}, revision}` | 404 as above |
| `PUT /api/profiles/instance/{p}/models/{m}` `{values, revision}` | `{revision, running: […], notConverged: […]}` | 404 `no-instance-profile-model`, 422 `model-profile-stay-key` `[key]`, the moved keys' value refusals (ranges, tensor rules, the enums), 409 `stale-revision`. The device and memory rules answer nothing: those instances are `notConverged` rows |
| `PUT /api/instances/{id}/profile` `{instanceProfile}` (no revision) | the affected answer | 422 `no-instance-profile-named`, 409 `address-in-use` `[instance]` |
| `PUT /api/instances/{id}/address` `{hostname, port}` (no revision) | the affected answer | 409 `address-in-use` `[instance]`, 422 `invalid-port`, 422 `invalid-hostname` |
| `POST /api/instances` `{engine, id, instanceProfile}` | 201 `{id, port}` (the proposed free port from 8581) | 409 `instance-exists`, 422 `no-instance-profile-named`, 503 `no-free-port` |

`running` is the instances that use the profile and are up, `notConverged` the instances the write
leaves not yet fitted: the page's restart offer lists one button per `running` id and says the
`notConverged` ones by their key (`ARCHITECTURE.md`, "Convergence"). Every one of them is an array:
a reader tests `running.length`. The instance profile page's `revision` is a hash of that profile's
own sections, and only the entry's save carries one.

The entry page also reads `GET /api/profiles/model/{m}` (the files that enable fields: `model-draft`,
`mmproj`), `GET /api/profiles/model/{m}/model-info`, `GET /api/node/devices` and
`GET /api/profiles/server/{s}` (its `Devices`), as the model profile page read them before.

## IP6a review: fixes for the next slice (designer, 2026-11-28)

Measured on the IP6a build (`build/ux-review/ip6a/shots.js`, `nav.js`), Chromium 1280 and 320 px.

- **R1** (26.1) The nav column is 248 px wide at 960 px and up (`index.css` `.layout`, was 224):
  "Instance profiles" no longer fits beside its icon at 224 (the entry is 64 px high, its siblings
  34); at 248 every Profiles entry is one 34 px line. `fe/DESIGN.md` 4 says 248 when it lands.
- **R2** (26.2) Below 480 px the name cell's muted second line holds the server profile and the
  model count (the server link, ` · `, then the count text of 26.2; no new key), and the Model profiles column hides as the Server profile column does. The link in it takes
  `--muted`, so a row does not show two identical purple links.
- **R3** (26.2) No cell of this table breaks inside a word at 320 px: the columns left (name, Used
  by, trash) give the name about 110 px; the head reads "Instance profile", "Used by" on whole words.
- **R5** (26.2, re-review) At 320 px the table is exactly as wide as its scroll box in every engine
  (measured after round 1: box 286, table 287 in Chromium, 288.45 in Firefox and WebKit). The heads
  wrap between whole words (`white-space: normal`, `overflow-wrap: normal`) and are not held to one
  line: the one-line "Instance profile" head is what pushes the table past the box.
- **R4** (26.4) `api-invalid-instance-profile-id` takes the text above (sentence case, a full stop,
  no backticks).

## IP6b review: fixes for the next round (designer, 2026-11-28)

Measured on the IP6b build (`build/ux-review/ip6b/shots.js`), Chromium 1280 and 320 px, both themes.

- **R6** (26.8) The entry row's control is a **minus** (`Icon` `minus`, the list-remove sign of
  DESIGN.md 5.2), not the trash can: the trash can reads as deleting the model profile itself, which
  stays.
- **R7** (26.7) At 320 px no cell of the models table breaks inside a word ("Placemen t", "llama.cp
  p's" today): R3 and R5's rules (heads and cells wrap between whole words only, the table no wider
  than its box); an id still breaks at its own `-`, `.` or `_`.
- **R8** (26.6) The switch's options are the server profile ids alone, not `instance-server-option`
  ("{0} (port {1})"): the port belongs to the instance since IP6 (26.16).
- **R9** (26.6) `model-profile-device-outside` on this page cannot use today's
  `api-model-profile-device-outside`: that text takes four args (profile, instance, key, device) and
  names "instance {1}'s server profile", so with the three args of section 0 it shows a raw `{3}`.
  New key `api-instance-profile-device-outside`, args (model, key, device): "Model profile {0} sets
  {1} = {2}, a GPU outside the Devices of the chosen server profile. Change that setting on the
  model's entry, or choose another server profile." The coordinator settles the args with IP4.
- **R10** (26.8) `confirm-deleting` ("Deleting…") is the wrong word while a remove runs: the
  ConfirmRow's status during a remove is `instance-profile-model-removing` ("Removing…").
- **R11** (strings) `en_US.json` holds `api-no-model-profile-named` twice (lines 211 and 450); keep
  the existing entry (line 450, there before IP6b) and drop the new one at 211. Apply section 3's "Texts the first draft left out".

## 1. The nav and the map

**26.1** The Profiles group lists, in this order:
- Instance profiles (`nav-instance-profiles`, "Instance profiles", route `#/profiles/instance`);
- Server profiles;
- Model profiles.

An instance profile composes the other two, so it comes first. `aria-current` and the group's look
are as for the other two. `fe/DESIGN.md` 2.1 gains the row.

## 2. The instance profile list (`#/profiles/instance`)

**26.2 The page.**
- `h1` `instance-profiles-title` ("Instance profiles"), then the intro `instance-profiles-intro`:
  "An instance profile is one server profile and the model profiles it serves, with where each
  model runs on this machine. Every instance runs one instance profile; several instances may run
  the same one."
- Then the table:

| Column | Key | Cell |
|---|---|---|
| Instance profile | `instance-profiles-column-name` | link to `#/profiles/instance/<p>`, named by the id |
| Server profile | `instance-profiles-column-server` | link to `#/profiles/server/<s>`, named by the id |
| Model profiles | `instance-profiles-column-models` | the count, `instance-profiles-models-count` ("{0} models"; `instance-profiles-models-count-one` "1 model"; `instance-profiles-models-none` "None", muted) |
| Used by | `instance-profiles-column-used-by` | instance links named by the instance's name (`InstanceLink`, D4.37), comma-separated; muted `instance-profiles-unused` ("No instance") |
| Actions | (visually hidden header as on the other lists) | the row's trash can |

- Loading, failed and empty: `instance-profiles-loading`, `instance-profiles-failed`, and
  `instance-profiles-empty` ("There are no instance profiles yet."), with the same skeleton and
  message as the server list (which has no retry button).
- Below 480 px the table needs no inner scroll: the Server profile column becomes a muted second
  line under the name, as D4.35 does for Engine.

**26.3 The row's trash can.**
- It is named `instance-profiles-delete-row` ("Delete instance profile {0}") and can act when
  `usedBy` is empty.
- Otherwise it is `aria-disabled`, with the tip `instance-profiles-delete-in-use`: "An instance
  uses it: give that instance another instance profile to delete this one."
- Its confirm question is `instance-profiles-delete-question`: "Delete the instance profile {0}?
  Its model entries and their placement settings are removed; the server and model profiles stay.
  This cannot be undone."
- Its done line is `instance-profiles-deleted`, and a 409 `instance-profile-in-use` gives
  `api-instance-profile-in-use` ("An instance uses the instance profile {0}: give it another one
  first."). The rest is as the server list's delete (22D4, CD).

**26.4 New instance profile.**
- The `+` after the table (`instance-profiles-new`, "New instance profile"; the form's `h2`
  `instance-profiles-new-title`) opens a form in place with two fields:
  - **Name**: `instance-profile-name`, its hint `instance-profile-name-hint`, "Letters, digits, ., _
    and -. It cannot be changed later.";
  - **Server profile**: a `select` of the pool's server profiles by id, the first option
    `instance-profile-server-choose` ("Choose a server profile").
- Create, Cancel, the status and the alert follow the server profiles' create form (17b, 17b2).
- Page checks, in order:
  - empty Name → `field-error-required`;
  - Name invalid → `api-invalid-instance-profile-id` ("{0} cannot be an instance profile name: use
    letters, digits, '.', '_' and '-', starting with a letter or a digit.", the shape of
    `api-invalid-model-profile-id`);
  - Name taken (letter case aside) → `api-instance-profile-name-taken`;
  - no server profile chosen → `field-error-choose`.
- A 201 goes to `#/profiles/instance/<id>`, with its `h1` focused.
- With an empty server profile pool the select is replaced by `instance-profile-no-server` ("Create
  a server profile first.") with a link to `#/profiles/server`, and Create stays: it then fails the
  `field-error-choose` check.

## 3. One instance profile (`#/profiles/instance/<p>`)

This page writes each change at once, like the instance page, so it has **no Save bar**. The
switch, the add and the remove send **no** `revision` (coordinator's ruling, IP6b, after
`fe/briefs/api-contract.md` IP4): each names one thing (the server profile, one model) and the
manager refuses one that is no longer there. Only the entry page's save (26.11) sends the read's
`revision`; its 409 `stale-revision` reads the page again and shows `api-stale-revision-reread`
("This instance profile changed elsewhere; it has been read again.") in the Save bar's alert.

Texts the first draft left out (IP6b review):
- `instance-profile-server-switching`: "Switching the server profile…";
- `instance-profile-model-removed`: "{0} removed.";
- `api-no-server-profile-named`: "There is no server profile {0}: it was deleted elsewhere.";
- `api-no-instance-profile-named`: "There is no instance profile {0}: it was deleted elsewhere.";
- `api-no-model-profile-named`: the existing key and text (`en_US.json`), not a second entry;
- `api-instance-profile-model-listed`: "This instance profile already lists {0}.";
- `api-no-instance-profile-model`: "This instance profile no longer lists {0}: it was removed
  elsewhere."

**26.5 The head.**
- The back link `instance-profile-back` ("All instance profiles"), the `h1` (the id), then a
  `dl.assets` row **Used by** (`instance-profiles-column-used-by`) with the `InstanceLink`s, or
  muted "No instance".
- 404 → the page's `instance-profile-missing` message ("There is no instance profile {0}.") with
  the back link.

**26.6 Server profile section.**
- `h2` `instance-profile-server-title` ("Server profile"). Under it, a `p` with the active server
  profile's link, named by its id (17d2.2's rule, H80).
- Then the field `instance-profile-server-switch-label` ("Switch to"): a `select` of every other
  server profile of the pool (no "free" rule any more: P1), the button
  `instance-profile-server-switch` ("Use this server profile"), and the hint
  `instance-profile-server-switch-hint`: "Every instance running this instance profile uses it from
  its next start. The models' devices are checked against its devices."
- The field shows only when the pool holds another server profile (17d2.3).
- **The send.** It sends `PUT .../server`; while it runs the status reads
  `instance-profile-server-switching`, the button is `aria-disabled`, and a second press sends
  nothing.
- **Success.** The link changes, and the status reads `instance-profile-server-switched` ("This
  instance profile now uses the server profile {0}.").
- **Refusals, in the section's alert:**
  - `model-profile-device-outside` → `api-model-profile-device-outside` (today's text);
  - `no-server-profile-named` → `api-no-server-profile-named`;
  - 401 → `session-ended-retry` with Sign in;
  - 403 → `session-revoked`;
  - anything else → `request-failed`.
- **The restart offer.** It follows the status when `running` is not empty (26.12).

**26.7 Model profiles section.**
- `h2` `instance-profile-models-title` ("Model profiles"), the intro `instance-profile-models-intro`:
  "The models every instance running this profile serves, in this order. Open one to set where it
  runs and its memory."
- Then a table:

| Column | Key | Cell |
|---|---|---|
| Model profile | `instance-profile-models-column-name` | link to the entry page `#/profiles/instance/<p>/models/<m>`, named by the model id |
| Placement | `instance-profile-models-column-placement` | `instance-profile-placement-default` ("llama.cpp's defaults", muted) when `keys` is 0, else `instance-profile-placement-count` ("{0} settings"; `-one` "1 setting") |
| Actions | visually hidden | the row's minus |

- Below the table, a secondary link per row is not needed: the model profile itself is reached
  from the entry page (26.9).
- Empty: `instance-profile-models-empty` ("This instance profile serves no model yet. An instance
  running it cannot start until it serves one."), then the `+`.

**26.8 Add and remove a model.**
- **The add.** The `+` after the table (`instance-profile-models-add`, "Add a model profile"; the
  form's `h3` `instance-profile-models-add-title`, "Add a model profile") opens a form in place:
  - one `select` labelled `instance-profile-models-add-label` ("Model profile") of the pool's model
    profiles not yet listed, by id;
  - Add (`instance-profile-models-add-button`, primary) and Cancel.
- With none left to add, the `+` stays and the form shows `instance-profile-models-add-none` ("Every
  model profile is already listed. Create one on the Model profiles page.") with a link to
  `#/profiles/model`, and no select.
- **Add's check and success.** No choice gives `field-error-choose`. A success closes the form,
  appends the row, puts the focus on its link, and shows the status `instance-profile-model-added`
  ("{0} added. It runs with llama.cpp's defaults until you set its placement.") and the restart
  offer if `running`.
- **The remove.** The row's minus (`instance-profile-models-remove-row`, "Remove {0} from this
  instance profile") is always able to act and opens a Confirm in place:
  - its question is `instance-profile-models-remove-question`: "Remove {0} from this instance
    profile? Its placement settings here are removed; the model profile stays.";
  - its red verb is `instance-profile-models-remove` ("Remove"), with CD's look.
- **Remove's success.** The row goes, the focus moves to the next row's link (or the `+` when it
  was the last), and the status reads `instance-profile-model-removed`, with the restart offer.
- **Refusals** go in the section's alert, by their `api-` texts (`no-model-profile-named`,
  `instance-profile-model-listed`, `no-instance-profile-model`), and the session answers as in
  26.6.

## 4. The model entry page (`#/profiles/instance/<p>/models/<m>`)

**26.9 The head.**
- The back link `instance-profile-entry-back` ("Instance profile {0}") to `#/profiles/instance/<p>`.
- The `h1` is the model id.
- A summary line `instance-profile-entry-summary`: "In the instance profile {0}, on the server
  profile {1}." Both are links, named by their ids.
- Then a `p` with the link `instance-profile-entry-model-link` ("Model profile {0}") to
  `#/profiles/model/<m>`.
- 404 → `instance-profile-entry-missing` ("{0} is not in the instance profile {1}.") with the back
  link.

**26.10 The cards.**
- **The rule.** The page shows the model profile page's cards, with each card keeping only the
  fields and notes whose key is in `PLACEMENT_KEYS`; a card left with none is not drawn. The cards
  keep the model page's order:
  1. **GPU distribution**: the model facts, split mode, GPUs and weights. Brief 14 applies as
     written, read for the entry.
  2. **Tensor placement**: brief 21 as written.
  3. **Hardware config**: the layer sliders of brief 19, the MoE fields and notes, the offload,
     repack, host, lazy and load modes, fit, and the draft's device and layers.
  4. **Resource / context**: `ctx-size`, `parallel`, `kv-unified`, `kv-unified-per-slot`,
     `batch-size`, `ubatch-size`.
  5. **KV cache**: the cache types, `flash-attn`, `cache-ram`, the checkpoints.
  6. **Advanced**: `load-on-startup`.
  7. **Vision**: `mmproj-offload`, `mmproj-device` (its `needsFile` note on `mmproj`, read from the
     model profile).
  8. **Speculative decoding**: `spec-draft-n-ubatch`, `draft-cache-note`, `spec-draft-type-k`,
     `spec-draft-type-v`.
- A field's `enabledWhen` that names a model profile key (`model-draft`, `mmproj`) reads it from
  the model profile document, which this page does not save.
- The cards' strings, intros and About panels are the existing keys, moved, with no new text.
- **The devices.** The GPU list and every device select check against the server profile's
  `Devices` of the entry's instance profile (GT5's rule, local now). The device hint names that
  server profile: `instance-profile-entry-devices-of` ("The devices of the server profile {0}.").
- The jump list, the collapsible cards (brief 23) and the card About toggles work as on the model
  page.

**26.11 The Save bar.**
- It is sticky at the bottom with Discard and Save changes, and no Delete: the entry is removed on
  the profile page.
- Save sends `PUT .../models/{m}` with the changed values (null for a cleared key) and the
  revision.
- Refusals with a `field` go under that field, and the rest go in the bar's alert, with the same
  keys and texts the model profile save used for these keys. 409 `stale-revision` is the bar's
  existing Reload answer.
- A save answering `running` non-empty shows the restart offer in the bar (17d.4a), one Restart
  per instance.

## 5. The restart offer (all IP6 writes)

**26.12** Every write that answers `running` (a list) shows, after its status line, the existing
offer: one secondary button `restart-offer-button` per running instance ("Restart {0}", where `{0}`
is the instance's name), each calling that instance's Restart. Its sending and sent texts are
`restart-offer-running` and `restart-offer-sent`. An empty list shows no offer. A write answering
`running: true` on the instance page shows its existing `...-restart` status text, since the page's
own Restart sits in its head (17d.6a).

## 6. The instance page (revises brief 16)

**26.13 Instance profile section** (replaces 17d's Server profile section; 17d2 is withdrawn).
- `h2` `instance-profile-title` ("Instance profile"), then a `p` with the active instance profile's
  link, named by its id (17d2.2's rule).
- Then the field `instance-profile-switch-label` ("Switch to"): a `select` of every other instance
  profile, and the button `instance-profile-switch` ("Use this instance profile"). Its hint is
  `instance-profile-switch-hint`: "The instance reads its instance profile when it starts."
- The field shows only when another one exists.
- `PUT /api/instances/{id}/profile` gives the statuses `instance-profile-switched` and
  `instance-profile-switched-restart` ("This instance now uses the instance profile {0}: restart it
  to apply."). Refusals and session answers are as 26.6.
- **What goes.** The 17e Model profiles section (the per-instance switches) goes; the instance
  profile's page holds the list now. The keys of 17d, 17e and 17d2 that no code uses any more are
  removed: the `instance-server-*` keys, the `instance-models-*` keys, and `instance-server-open`.

**26.14 Address section** (P1: the address is the instance's).
- `h2` `instance-address-title` ("Address"), then two fields in the Field pattern:
  - Host (`instance-address-host`, a text input, its hint `instance-address-host-hint`: "The address
    it listens on: 127.0.0.1 for this machine only, 0.0.0.0 for every interface.");
  - Port (`instance-address-port`, `inputmode="numeric"`, 1 to 65535).
- After them comes the button `instance-address-save` ("Save address"), then a status line and an
  alert (Action feedback).
- Page checks: port empty or out of range → `field-error-port`; host empty → `field-error-required`.
- `PUT /api/instances/{id}/address` gives `instance-address-saved` ("Saved: it listens on {0}:{1}.")
  or `instance-address-saved-restart` ("Saved: restart it to listen on {0}:{1}.").
- Refusals:
  - 409 `address-in-use` → `api-address-in-use` ("The instance {0} already uses this address.")
    under Port;
  - 422 `invalid-port` and `invalid-hostname` → under their fields, with the existing
    `field-error-port` and `api-invalid-hostname` texts;
  - the session answers are as 26.6.
- The Overview tile and the nav keep showing the port, now from the instance record.

**26.15 New instance** (revises 17a's create form).
- The Server profile select becomes **Instance profile**: `instance-new-instance-profile`, its hint
  `instance-new-instance-profile-hint` ("The instance profile it runs: its server profile and its
  models. Several instances may run the same one."), and the first option
  `instance-new-instance-profile-choose`.
- Every instance profile is listed, with no "free" rule.
- The note `instance-new-note` becomes: "The new instance gets the first free port from 8581, which
  its page can change. It does not start at boot until you set it there."
- On a 201, the instance's page shows the port the answer gives.
- `instance-new-server`, `instance-new-server-hint`, `instance-new-server-choose` and
  `instance-new-no-free` go. `instance-new-profiles-link` points to `#/profiles/instance` and reads
  "Instance profiles".

## 7. The server profiles (revises briefs 01 and 16)

**26.16**
- **The list.** The Port column goes; the columns are Server profile, Tags, Used by, Actions.
- **Used by.** It now lists the instance profiles that name the server profile, as links named by
  their ids. The instance links go, with `server-profiles-unused` reading "No instance profile".
- **Delete.** The tip and the refusal read `server-profiles-delete-in-use`: "An instance profile
  uses it: give that instance profile another server profile to delete this one."
- **The page.** The server profile page loses its `Hostname` and `Port` fields, and its Used by row
  lists instance profiles.
- **The intro.** `server-profiles-intro` drops "its port, its addresses": "Each server profile holds
  the llama-server settings an instance profile runs with: its devices, its tags and the rest.
  Several instance profiles may use the same one."

## 8. Moved briefs

**26.17** Briefs 14, 19 and 21 keep their criteria; their page becomes the entry page (26.10),
where:
- "the model profile" now means "the entry";
- "the instance's server profile" now means "the server profile of the entry's instance profile";
- "save" now means the entry's `PUT`.

Their tests move with their cards. Each brief gets a line at its top pointing here.

## 9. Accessibility

- Every new page has one `h1`, sections under `h2`, and the add form under `h3`
  ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships)).
- Links are named by ids in context: the column header, or the heading right before
  ([2.4.4](https://www.w3.org/TR/WCAG22/#link-purpose-in-context), H80).
- Every write moves no focus except where stated, and its status is a polite live region
  ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)).
- Every control is reachable by Tab in reading order ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)),
  and every target is at least 24 by 24 px ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).
- No sideways scroll at 320 px or at 200 % text ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)).
- A switched-off control stays focusable, with its reason as its description (L2.B2).

## 10. Acceptance list (the tester's)

1. The nav's Profiles group reads Instance profiles, Server profiles, Model profiles, and the first
   leads to `#/profiles/instance` (26.1).
2. The list's columns and cells follow 26.2: count texts (0, 1, many), Used by links by name, and the
   loading, failed and empty states. At 320 px there is no inner scroll, and the server sits under
   the name.
3. The trash can follows 26.3 for a used row and an unused one, with the confirm verb `primary
   danger`.
4. The create form follows 26.4: two fields, the checks in their order with the focus on the first
   invalid field, the 201 route with its `h1` focused, and the empty pool case.
5. The page head and its 404 follow 26.5.
6. The server switch follows 26.6: shown only with another server profile; status, refusals and
   restart offer; the second press ignored.
7. The models table follows 26.7: the placement text for 0, 1 and many keys, and the empty line.
8. Add and remove follow 26.8: the focus after each, the remove confirm with the red verb, the
   none-left case, and the restart offer.
9. The entry page:
   - its head and 404 follow 26.9;
   - its card titles follow 26.10 in order, and its fields are exactly the `PLACEMENT_KEYS` that
     have controls;
   - every `enabledWhen` on a model profile key reads the model profile document;
   - the device list is the server profile's.
10. The entry Save bar follows 26.11: the body sent, a field refusal, the alert, Reload, and the
    restart offer.
11. 26.12: one Restart per running instance, by name, and none for an empty list.
12. The instance page:
    - 26.13: the picker, its statuses, and the 17e section's absence;
    - 26.14: the address fields, checks, statuses and refusals;
    - 26.15: the create form's select and note.
13. The server profiles follow 26.16: no Port column or fields, and Used by lists instance profiles
    with the new delete reason.
14. Strings: the new keys of this brief exist with these texts, and the removed ones are gone from
    `en_US.json` and the code.
15. `fe/ux/check.js` gains `instance-profiles` (the list, `list=many`), `instance-profiles-new`,
    `instance-profile` (the page with three entries), `instance-profile-add`,
    `instance-profile-remove` (the confirm open), `instance-profile-entry` (the entry page), and
    `instance-address`. `model-profile-blocks` and `model-profile-placement` move to the entry page.
    The stub serves the routes of section 0.

## 11. Out of scope

- Instance profiles across nodes, and the director (`ARCHITECTURE.md` step 14).
- Renaming an instance profile (copying one is slice CP, section 12).
- Reordering the model entries: they keep the add order.
- A "restart needed" mark that outlives the page (invariant 4's later step).

## Decisions taken overnight (recommended options, for the user to overrule)

1. **No Save bar on the instance profile page.** Each control writes at once, as on the instance
   page: a page made of a picker and a list has no form to batch.
2. **Delete only from the list's trash can.** The page carries no Delete. One place is enough,
   since the page has no Save bar to hold it.
3. **The entry page takes the moved fields by the same card filter as IP5.** One rule, the key
   lists, decides which page shows a field, in the cards and order users already know.
4. **The instance page loses its 17e Models section.** The instance profile page is now the one
   place for the list.
5. **The add uses a select of the not-yet-listed profiles.** With none left, it points to the Model
   profiles page.
6. **The add route, settled by IP4 (60188bde).** The page has `POST .../models` for the add (a body
   of `{model}`, its 201 answers the affected pair) and `PUT .../models/{m}` for the entry's values
   (with its `revision`): the add and the save stay two routes, as this brief asked.

## 12. CP: every profile kind can be created as a copy (user, 2026-11-27)

The user: every profile kind can be created as a copy. Model profiles already can (brief 22, the
"A copy of another model profile" choice, 22 C). CP gives the same choice to the server profile and
instance profile create forms. It lands with or after IP6, since the server profile form loses its
Port field in IP6 (26.16).

**CP.0 What the bff serves.** The create routes take `from`, as the model profile create does
(`POST /api/profiles/model` `{"id", "from"}`):
- `POST /api/profiles/server` `{"engine", "id"}` creates from the defaults. With `{"engine", "id",
  "from"}` it copies `server@<from>.ini` byte for byte, comments included, under the new id.
- `POST /api/profiles/instance` `{"id", "server"}` creates as today (26.4). With `{"id", "from"}` it
  copies the `[<from>]` section's `ServerProfile` and every `[<from>/<m>]` entry, with its placement
  and memory keys, in the same order. `server` and `from` together are refused 422
  `instance-profile-bad-source` (field `values`); the form never sends both.
- **Refusals for `from`.** 422 `no-server-profile-named` or `no-instance-profile-named` (field
  `from`) when the source is gone, and the create's other refusals as today. No instance uses the
  copy, so it renders nowhere.

**CP.1 The choice.**
- **Where.** Each form gains a `fieldset` `model-profile-start-from` ("Start from", the existing key)
  right after Name. It holds two native radios with the Radio group look (8 px gap, `fe/DESIGN.md`):
  - server profiles: `server-source-defaults` ("llama-server's defaults") and `server-source-copy`
    ("A copy of another server profile");
  - instance profiles: `instance-profile-source-new` ("A server profile") and
    `instance-profile-source-copy` ("A copy of another instance profile").
- **Checked on open.** The first radio.
- **Switching.** It keeps each choice's value and moves no focus
  ([3.2.2](https://www.w3.org/TR/WCAG22/#on-input)), and only the checked choice's control shows
  (22 P.1).
- **The copy radio.** It shows only when the pool holds at least one profile of that kind, as
  22 C.1's copy radio does.

**CP.2 The copy select.**
- **Server profiles.** The field is `server-profile-copy-from` ("Server profile to copy"), its hint
  `server-profile-copy-hint`: "Every setting of that profile is copied, comments included. No
  instance profile uses the copy until you choose it in one." Its first option is
  `server-profile-copy-choose` ("Choose a server profile"), and the rest are the pool's ids.
- **Instance profiles.** The field is `instance-profile-copy-from` ("Instance profile to copy"), its
  hint `instance-profile-copy-hint`: "Its server profile and every model with its placement and
  memory settings are copied. No instance runs the copy until you choose it on an instance's page."
  Its first option is `instance-profile-copy-choose` ("Choose an instance profile").
- For the instance profile form, the "A server profile" choice holds 26.4's Server profile select,
  unchanged.

**CP.3 The checks and the request.**
- **The checks.** The page checks run in 26.4's order, the Name checks first. Copy with the empty
  option selected gives `field-error-choose` under the copy select, with `aria-invalid`, and the
  focus moves there once the error has rendered (22 C.4).
- **Bodies:**
  - server, from the defaults: `{"engine": "llama.cpp", "id"}`;
  - server, as a copy: `{"engine": "llama.cpp", "id", "from"}`;
  - instance, from a server profile: `{"id", "server"}`;
  - instance, as a copy: `{"id", "from"}`.
- **Refusals.** A refusal with `field` `from` goes under the copy select, through `writeFailure`
  with the existing `api-no-server-profile-named` and `api-no-instance-profile-named`.
  `instance-profile-bad-source` goes in the alert as `api-other`.
- **Success.** A 201 goes to the new profile's page with its `h1` focused, as today.

**CP.4 Strings.**
- New: `server-source-defaults`, `server-source-copy`, `server-profile-copy-from`,
  `server-profile-copy-hint`, `server-profile-copy-choose`, `instance-profile-source-new`,
  `instance-profile-source-copy`, `instance-profile-copy-from`, `instance-profile-copy-hint`,
  `instance-profile-copy-choose`.
- Reused: `model-profile-start-from`, `field-error-choose`, the create texts.

**CP acceptance list.**
1. Each form has the Start from fieldset after Name, with the two radios in CP.1's order, the first
   checked on open. With an empty pool of that kind there is no copy radio.
2. Switching keeps both values and moves no focus. Only the checked choice's control is shown.
3. Copy with nothing chosen gives `field-error-choose` under the select, focused after it renders,
   and sends nothing.
4. Each of the four bodies of CP.3 is sent exactly as written.
5. A 422 with `field` `from` lands under the copy select. A 201 lands on the new page with its `h1`
   focused.
6. At 320 px and at 200 % text the radios wrap with no sideways scroll, each label at least 24 px
   high ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow),
   [2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).
7. The strings of CP.4 exist with these texts.
8. `fe/ux/check.js` gains `server-profiles-new-copy` and `instance-profiles-new-copy` (the form open,
   copy checked).
