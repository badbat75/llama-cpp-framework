# The bff API the web UI is built against

The contract between `fe/` and `pai-bff` for slice 1 (the shell, sign-in and the Server page of one
instance: [01-server.md](01-server.md)). The fe is tested against these shapes with mocked replies;
the bff side is scheduled by the coordinator of `crates/`. A route marked **proposed** does not
exist yet. Change this file first when a shape changes.

## Rules for every route

- Everything is under `/api/`, JSON in and out.
- **Errors** are `{"key": "<strings key>", "args": [...]}` with a non-2xx status. The key is a
  configurator strings key where a check already exists (the refusals of `pai-core`
  `server_cfg::check` and `checks.rs`). The fe shows it as its own string `api-<key>`,
  filling `{N}` from `args`; a key it does not know, a malformed body or a network failure is shown
  as one generic failure. The one exception: a 422 with a key the fe does not know is shown as a
  refused value without its reason (`api-other`), since the status already says a check refused it.
- **401** means not signed in (no session, or it ended), on any route that needs one.
- **Without a session** (from 2026-11-19; the bff follows the fe slice, `fe/briefs/09-signin-only.md`):
  no route but the static web UI and `POST`, `GET` and `DELETE /api/session` answers without a
  session; every other route answers 401 `session-required`, never 403. `GET /api/version`,
  `GET /api/instances` and `GET /api/instances/{id}/boot` do not move the idle deadline. Until the
  bff follows, those three still answer without a session, as the lines below say; the fe never
  sends them before a sign-in.
- **403** `client-not-authorized` means the manager refused the call for this user, on a route that
  acts for the signed-in user; the session lives on. No served route sends it today (since bff slice
  9e every refusal of the user ends the session, below); the fe keeps handling it. On `GET /api/instances` and `GET .../boot`
  (no sign-in needed until the bff follows 2026-11-19's rule, above) the same key comes with **502**: there the manager refused
  the bff itself, not a user.
- **403** `session-not-administrator` on any route but `POST /api/session` (served since bff slice 9e, 5f0d405, on `GET .../server`, the three verbs and `PUT .../boot`; on `PUT .../server` since eff112d; on `GET /api/profiles/model`, `PUT /api/profiles/model/{id}/enabled`, `GET /api/profiles/model/{id}` and `PUT /api/profiles/model/{id}` since they were served, 257147e, 3a63244, 36a892d, 3eb24b2; S1 of
  [01-server.md](01-server.md)): the manager refused the session's user, who is no longer a member
  of `pai`, and the bff has **ended that session** at once (user, 2026-11-16: removed from `pai`, the
  session ends). The next request with the cookie answers 401; no `Set-Cookie` clears it. The bff
  sends this key exactly when it ends a session on a refusal, and `client-not-authorized` only when
  the session lives on, so the fe needs no second read of `GET /api/session` to tell them apart.
  Where: `crates/bff/src/server.rs` `read` and `save_failure`, `control.rs` `action_failure`,
  `presets.rs` `read_answer` and `switch_failure`. The reads
  `GET /api/instances` and `GET .../boot` keep 502 `client-not-authorized`.
- **The guard's keys**, each 403 or 413 with `args: []` (`crates/bff/src/origin.rs`); the 403s are
  answered before any session is looked at, the 413 by the write's handler after its session check
  (no session is 401 whatever the body): `request-cross-origin` (a write whose `Origin` or `Sec-Fetch-Site`
  is not this bff), `request-bad-host` (a `Host` that is not `localhost`, `127.x.x.x` or `[::1]`),
  `payload-too-large` (413, a body over 64 KiB). The fe's own requests never get them: the guard
  stands in front of `/` and `/assets/*` too, so a page opened under another name never loads, and
  the fe's bodies are a few dozen bytes. The fe shows them as the generic failure of the place.
- **Idle deadline**: every request with a valid session moves it, except the reads
  `GET /api/version`, `GET /api/instances` and `GET /api/instances/{id}/boot` (from 2026-11-19; until then the first is not listed); the `POST`s and `PUT`s move it.
- **The fe picks its message by the key, never by the status**, apart from 401 (sign in).
- **503** `client-unavailable` means no manager to reach or no answer in time.
- **Writes and their time limit** (the server save and the model profile switch since 594e15e, the model
  profile save since 3eb24b2, which gives its read 5 s and the whole save 15 s, and the four association
  routes since 2d9a4c8, every other write since 57666cb): the manager's calls of a
  write get **15 s** (`lib.rs` `WRITE_TIMEOUT`), the three verbs, the boot write and the tags write
  included; reads, the boot read among them, keep **5 s** (`MANAGER_TIMEOUT`).
  503 `client-unavailable` only while no write was asked; **504 `client-timeout`** once one was
  sent and its outcome is unknown (the limit ran out, or the manager was lost after the hand-over),
  the session kept (since d1f3375 and 57666cb on the single-call writes: the model and server profile
  create and delete, the instance create and removal, the verbs, the boot write, the tags write); a write that **landed** whose following read (the status, the
  switch's read-back) is slow or fails answers **200 with `running: true`**, never 504
  (`server.rs` `WriteState`, `TimedOut`).
  The four association routes never answer 504: a manager that outlives the limit, before or after a
  write, is 503 `client-unavailable` (`crates/bff/src/instance_profiles.rs`).
- **404** `client-no-instance` means the instance id does not exist.
- The session is an HttpOnly cookie set by the bff; the fe never reads it.

## Routes

| Route | Who | Slice | Success |
| --- | --- | --- | --- |
| `GET /api/version` | signed in | exists; `name`, `os` AN4, served (e52480d) | `{"version", "name", "os"}` |
| `GET /api/node` | signed in | 13; `os`, `vulkan`, `gpus`, `nodeInfo` AN4, served (e52480d) | `{"name", "api", "version", "engines", "os", "vulkan", "gpus"}` (+ `nodeInfo` on a manager failure) |
| `GET /api/node/devices` | signed in | GD (bff slice G2, served 66b2bb3), `ramMib`/`cpuThreads` served (a1a3458) | `{"probedAt", "ramMib", "cpuThreads", "devices"}` |
| `GET /api/instances` | anyone | 1a, changed; T1 being built (bff slices 11, 12); `boot` 13; `removable` 16, served (e262708) | `[{"id", "engine", "serverProfile", "state", "port", "tags", "instanceTags", "boot", "removable"}]` |
| `GET /api/session` | anyone | 1a | 200 `{"user"}`, or 401 |
| `POST /api/session` | administrator (`pai` group) | 1a | 200 `{"user"}` and the cookie |
| `DELETE /api/session` | signed in | 1a, served | 204 |
| `GET /api/instances/{id}/server` | administrator | 1a, served; **removed in slice 17** (the fe reads `/api/profiles/server/{id}` from 17a) | the server document below |
| `PUT /api/instances/{id}/server` | administrator (`configure`) | 1b, served (8); **removed in slice 17** | `{"revision", "running"}` |
| `POST /api/instances/{id}/start`, `/stop`, `/restart` | administrator | 1c, served (9a); 409 `instance-no-model-profiles` 16, served (e262708) | 204 |
| `GET /api/instances/{id}/boot` | anyone | 1c, served (9a) | `{"enabled"}` |
| `PUT /api/instances/{id}/boot` | administrator (`configure`) | 1c, served (9a) | `{"enabled"}` |
| `PUT /api/instances/{id}/tags` | administrator (`configure`) | T1, being built (bff slice 12) | 204 |
| `GET /api/engines` | anyone | model profiles slice, proposed | `[{"id", "name"}]` |
| `GET /api/profiles/model` | administrator | 2a, 2b, served (257147e); `assignments` 15 | `[{"id", "engine", "engineAvailable", "enabled", "model", "assignments"}]` |
| `PUT /api/profiles/model/{id}/enabled` | administrator (`configure`) | 2c, served (3a63244); `restart` 15 | `{"enabled", "running", "restart"}` |
| `GET /api/profiles/model/{id}` | administrator | 2d, served (36a892d) | `{"id", "engine", "engineAvailable", "enabled", "values", "projectUrl", "revision"}` |
| `PUT /api/profiles/model/{id}` | administrator (`configure`) | 2h, served (3eb24b2); `restart` 15 | `{"revision", "running", "restart"}` |
| `POST /api/profiles/model` | administrator (`configure`) | MP2, served (76163d1) | 201 `{"id"}` |
| `DELETE /api/profiles/model/{id}` | administrator (`configure`) | MP2, served (76163d1) | 204 |
| `GET /api/profiles/model/{id}/model-info` | administrator | L2, served (e1de6be); `tensors` J1 (6570557) | `{"model", "info", "reason"}`: [crates/bff/README.md](../../crates/bff/README.md#the-model-info) |
| `PUT /api/instances/{id}/profiles/server` | administrator (`configure`) | 15, served (2d9a4c8) | `{"running"}` |
| `GET /api/instances/{id}/profiles/model` | administrator | 15, served (2d9a4c8) | `{"all", "profiles": [rows]}` |
| `PUT /api/instances/{id}/profiles/model/{profile}` | administrator (`configure`) | 15, served (2d9a4c8) | `{"effective", "running"}` |
| `PUT /api/instances/{id}/profiles/model` | administrator (`configure`) | 15, served (2d9a4c8) | `{"running"}` |
| `POST /api/instances` | administrator (`configure`) | 16, served (e262708) | 201 `{"id"}` |
| `DELETE /api/instances/{id}` | administrator (`configure`) | 16, served (e262708) | 204 |

The four model profile routes were served at `/api/presets...` until 5d54590 (step 4b, below), with
the keys `no-preset-named`, `preset-changed-elsewhere`, `presets-malformed`, `invalid-preset-id`,
`preset-no-model` and `preset-name-taken`; since then the old paths are the router's own 404, with no
body and no manager call (`crates/bff/tests/presets.rs`,
`the_old_model_profile_paths_are_the_routers_404_with_no_body`). Bodies, fields, statuses and `args`
did not change. The fe moved in ec6dbd0. The files are the engine's `models.ini` and
`models.ini.disabled` since 872553d (were `presets.ini`, `presets.ini.disabled`).

### `GET /api/instances`

Today's reply gains `engine` (the engine id, `llama.cpp` today) and its errors move from
`{"error": "unavailable"}` to the common shape: 503 `client-unavailable`, 502
`client-not-authorized`, 502 `client-other` for any other failure, each with empty `args` (the
manager's detail never reaches the body).

**T1 (`fe/briefs/12-instance-tags.md`; bff slices 11 and 12, being built).** Each item gains
`port` (number: the configured port of its server profile, 0 when that file is missing), `tags`
(the union shown: the server profile's tags first, then the instance's own, a tag in both kept once
at its first place; `[]` when the manager answers no rows) and **`instanceTags`** (the instance's own
list, for the editor; asked of the bff, not yet in its tests: brief 12 question C1). The fe reads a
missing or mistyped `port` as 0 and a missing or mistyped list as `[]`, never failing the list.

**13 (`fe/briefs/13-node-about.md`; bff slice 13, being built).** Each item gains `boot`: `true` or
`false` from the manager's `BootEnabled(id)`, `null` when that one read failed; a failed boot read never
fails or shortens the list. The fe reads anything but a boolean as `null` (no Start at boot row on the
tile). `GET /api/instances/{id}/boot` stays (the instance page, and after a `PUT`). Source:
`ARCHITECTURE.md` "Slice 13's decisions".

**15 (bff slice 15, served at 2d9a4c8).** Each item gains **`serverProfile`**: the server profile name
the record of `instances.ini` carries, as the manager spells it, `null` when the instance's section
names none. It comes from the same `Instances` call as the port and the tags, so a row the manager
**15 (bff slice 15, served at 2d9a4c8).** Each item gains **`serverProfile`**: the server profile name
the record of `instances.ini` carries, as the manager spells it, `null` when the instance's section
names none. It comes from the same `Instances` call as the port and the tags, so a row the manager
does not answer is listed with `null` and never fails the list.

**16 (bff slice 16, served at e262708).** Each item gains **`removable`**: `true` for every instance,
`false` for `default`, the id the removal refuses. It is the bff's own rule on the id (`removable` in
`crates/bff/src/instances.rs`), not a manager read, so every row carries a boolean, whatever its
state. The fe offers Remove only where it is true: the removal refuses a running instance and `default`
([below](#as-designed-after-4b), "Instances"; `crates/bff/README.md` "Creating and removing an
instance").

### `GET /api/version` (`name` and `os`: AN4, served e52480d)

Behind a session: 401 `session-required` without one; it asks the manager nothing and does not move the
idle deadline. 200:

    {"version": "2.99.1", "name": "gpu-box",
     "os": {"name": "Ubuntu", "version": "26.04", "kernel": "7.0.0-43-generic"}}

`name` is the host name, as `GET /api/node` answers it; `os` is the machine the bff runs on, read by
the bff itself (os-release's `NAME` and `VERSION_ID`, the kernel release), each field a string or `null`.

### `GET /api/node` (13; the host's facts AN4, served e52480d)

Behind a session: 401 `session-required` without one; it does not move the idle deadline, as
`GET /api/version`. A missing or unreadable version file is left out. 200:

    {"name": "gpu-box", "api": "/api", "version": "2.99.1",
     "engines": [{"engine": "llama.cpp", "version": "1:1.0.0+pai2",
                  "backends": [{"name": "cuda", "version": "13.5.0+nccl.2.32.3"},
                               {"name": "rocm", "version": "10.1.0"},
                               {"name": "vulkan", "version": "1.4.363.1"}]}]}

`name` is the host name (`localhost` when it is empty); `version` is the framework's (the bff's own, as
`GET /api/version`); each engine and backend `version` is a string or `null` (an empty version file);
`engines` is `[]` when none is installed (or a package from before slice 13), `backends` `[]` for a
CPU-only engine, sorted by name; a backend's `name` is the package suffix (`cuda`, `rocm`, `vulkan`),
its display name the fe's string. Read at each request. The fe does not read `api` yet. Source:
`ARCHITECTURE.md` "Slice 13's decisions".

Since AN4 the reply also carries the host's facts, asked of the manager's `NodeInfoAs` as the
signed-in user within 10 s (`ARCHITECTURE.md` "About and node info"):

    "os": {"name": "Ubuntu", "version": "26.04", "kernel": "7.0.0-43-generic"},
    "vulkan": "ok",
    "gpus": [{"pci": "0000:03:00.0", "vendor": "1002", "name": "AMD Radeon AI PRO R9700",
              "driver": "amdgpu", "version": null,
              "vulkan": {"driver": "radv", "info": "Mesa 26.0.8-1ubuntu0.3"}}]

Every text is a string or `null`; `vulkan` is `ok`, `none` or `failed`; rows in the manager's order, at
most 32, sent as the manager answered them. A row's `version` `null` means the driver module sets no
version (an in-tree driver), `driver` `null` no driver bound; a row's `vulkan` `null` with
`"vulkan": "ok"` means Vulkan did not see that device, its `driver` and `info` both `null` a device
below Vulkan 1.2 without `VK_KHR_driver_properties`; a Vulkan device not joined by bus id is its own
row with `pci`, `driver` and `version` `null`. When the manager is not there or does not answer within
10 s, still 200 with `os`, `vulkan` and `gpus` `null` and `"nodeInfo": {"key": "client-unavailable",
"args": []}`; any other client failure the same with `client-other`; with no failure `nodeInfo` is
absent. The manager refusing the user is 403 `session-not-administrator`, and the session ends.

### `GET /api/node/devices` (GD, bff slice G2, served 66b2bb3; the inventory fields a1a3458)

The host's GPUs for the model profile page's GPU distribution card (`fe/briefs/14-gpu-distribution.md`).
A page read behind a session (moves the idle deadline); the bff asks the manager's `DevicesAs` as the
signed-in user, which runs the engine's `--list-devices` once and caches it until the manager
restarts. 200:

    {"probedAt": "2026-11-20T09:14:03Z", "ramMib": 65536, "cpuThreads": 16,
     "devices": [{"id": "ROCm0", "name": "AMD Radeon AI PRO R9700", "backend": "ROCm",
                  "totalMib": 32624, "freeMib": 32420}]}

Probe order, the CPU left out, duplicate views of one card and integrated GPUs included; `freeMib` is
free memory at the probe, never now (the fe dates it with `probedAt`; no refresh, user 2026-11-20).
`ramMib` is the node's total RAM in whole MiB and `cpuThreads` the logical CPUs its process may use,
read by the manager off the machine it runs on and answered whatever the probe did (a1a3458): the
host's, not the engine's. Each 0 is the node's "no figure", a reading that failed, so the fe shows
nothing for it: never `0 MiB`, never a thread field capped at 0. `{"probedAt": null, "ramMib": <int>,
"cpuThreads": <int>, "devices": []}` when no probe answered; a CPU-only host is `devices: []` with a
time. Errors: 401 `session-required` (manager not asked); 403 `session-not-administrator` (the session
is ended); 503 `client-unavailable` (no manager, or no answer within 35 s, the cold probe's 30 s plus
5); 502 `client-other`. The fe gives this `GET` 40 s, not 15. This is not what the Server form reads
today: its `limits` thread maximum is the web service's own CPU count, and reading `cpuThreads` there
is R5, open work in `PLAN.md`. Source: `ARCHITECTURE.md`, the design block "GPU distribution on the
model profile page" (in "Nodes and instances") and "Resources belong to the server profile",
"(1) The node inventory" (its R1 bullets).

### `PUT /api/instances/{id}/tags` (T1, being built)

Body `{"tags": [string]}`, the instance's own tags in order (`[]` clears them); the server
profile's tags are not touched. 204 with no body. Errors: 401 `session-required`; 403
`session-not-administrator` (the session ended); 404 `client-no-instance`; 422 `{"key":
"server-bad-tags", "args": [reason, list joined by ",", most tags, least chars, most chars], "field":
"tags"}` (`pai-core` `checks::tags_rule`; the reason is an English phrase of the code); 400
`tags-malformed` for a body that is not that shape (the fe never sends one); 503
`client-unavailable`; 504 `client-timeout` (the list handed over, its outcome unknown). Source: `crates/bff/tests/instances.rs` in the bff's working tree.
`ARCHITECTURE.md` (f') names the keys `instance-bad-tags` and `instance-malformed`: brief 12
question C2.

### The session

- `GET /api/session` answers 404 until the bff has the route; the fe reads any reply but 200 as
  signed out.
- **Only administrators sign in** (user, 2026-11-15): an administrator is a member of the OS group
  `pai` on the server machine. The session reply is `{"user"}`, which is all the bff sends since
  commit 296165f; the fe still accepts a reply that carries `canConfigure` and ignores the field
  (an older bff sent it). The manager still decides
  every request; a refusal of the user ends the session (403 `session-not-administrator`, above).
- `POST /api/session` takes `{"username", "password"}`. Failures:
  - 401 `session-bad-credentials`: the same reply for an unknown user and a wrong password.
  - 403 `session-not-administrator`: the password was right, but the account is not in the `pai`
    group. Answered only after the password check; the 429 limit is checked first.
  - 429 `session-too-many-attempts` with `args: [seconds left]`: after 5 failures for one user from
    one client address within 60 s.
  - 503 `session-unavailable`: the bff has no way to check a password (no authenticator, or it
    failed). The bff shipped before the OS login exists answers this to every sign-in.
  - 400 `session-malformed`: a malformed body or an empty user name. The fe checks both fields
    before sending, so it shows this as the generic failure.
- `GET` or `DELETE /api/session` without a valid session is 401 `session-required`; the fe signs
  out whatever a `DELETE` answers.
- A session ends after 30 minutes without a request; the next call answers 401.
- The cookie is `HttpOnly`, `SameSite=Strict`, `Path=/`, without `Secure` while the bff is
  loopback only.

### `GET /api/instances/{id}/server`

```json
{
  "revision": "<opaque>",
  "values": { "<INI key>": 8580 | true | "<text>" | null },
  "defaults": { "Port": 0, "ModelsMax": 0, "ModelsMemoryMargin": 0, "OpencodeBaseUrl": "",
                "ModelsDir": "", "StateDir": "" },
  "limits": { "Port": [0, 0], "ModelsMax": [0, 0], "ModelsMemoryMargin": [0, 0],
              "Threads": [0, 0], "ThreadsBatch": [0, 0] },
  "interfaces": [{ "value": "", "label": "" }]
}
```

- `values` is keyed by the INI key name, so a refusal's field maps straight to a control. Each
  value is typed JSON: a number for a numeric key (`8580`), `true` or `false` for a switch, a string
  otherwise (`"nccl"`), `null` for an unset key. A save sends them back the same way.
  `Devices` (R2, 3aaa4dc) is a string: the server profile's GPUs in one value, the probe's names in
  the order llama-server's `--device` takes (`"ROCm0,ROCm1"`), or `"none"` for CPU only; `null` is
  the key absent, which is every GPU of the node: these are the cards the memory check measures the
  instances that run this profile against (5fb7b0e).
- `defaults` are computed on the server from the saved file. `ModelsDir` and `StateDir` are the
  instance's own defaults, never a folder of the bff's account.
- `limits` are `[min, max]`; the thread maximum is the server machine's logical CPU count.
- `interfaces` are the server machine's network interfaces (`net_ifaces::interfaces()`).
- `revision` is opaque (16 lowercase hex digits today: SipHash-2-4 of the whole text under a key the
  bff draws at each start, `crates/bff/src/server.rs` `revision`); the fe sends it back on a save and
  never compares or parses it. It does not survive a bff restart: a form opened before one gets 409
  `server-changed-elsewhere` at its next save, and a Reload (user, 2026-11-16).
- **`withheld`**: a document read without the right to see `OpencodeApiKey` has
  `values.OpencodeApiKey` `null` and `"withheld": ["OpencodeApiKey"]` when the file has a key
  (`withheld` absent or `[]` otherwise). Every web session is an administrator's, so the fe never
  shows a withheld view: a document that withholds the key counts as a failed read
  (`server-load-failed`), so that a save cannot erase the key. It can still arrive while the bff
  reads as itself (until the bff acts for the signed-in user).

### `PUT /api/instances/{id}/server`

Served since bff slice 8 (eff112d; `crates/bff/src/server.rs` `save`, `body_to_file`, `FIELDS`,
`field_of`, `save_failure`). Designed in `ARCHITECTURE.md`, "Saving the server file from the web".
Checked against the fe's `ServerForm.tsx` (`toValues`, `sendSave`) on 2026-11-17: they agree.

- **Request**: `Content-Type: application/json` (any other type, or none, is 400
  `server-malformed`), body at most 64 KiB, `{"revision": <string>, "values": {...}}`. The same-origin
  guard applies (a `PUT` is a write): the browser's own `Origin` and `Sec-Fetch-Site` on a same-origin
  `fetch` pass it.
- **`values`**: the 21 keys of the read, typed as the read gives them, `null` the key unset: numbers
  (whole, within `i32`) for `Port`, `Threads`, `ThreadsBatch`, `ModelsMax`, `ModelsMemoryMargin`,
  `LogVerbosity`; booleans for `CudaDisableGraphs`, `AttnRotDisable`, `RocprofAttach`, `Webui`,
  `WebuiMcpProxy`, `SaveStateOnShutdown`; `"on"`/`"off"` for `RocblasUseHipblaslt`; a list of strings
  for `Tags`; strings for `Hostname`, `ModelsDir`, `CudaAllreduce`, `StateDir`, `OpencodeBaseUrl`,
  `OpencodeApiKey`, `Devices`. A value of
  another type (a fraction, a numeric string, another word for the tri-state) is 400
  `server-malformed`. **A key absent from `values` is read as `null`: that key is unset in the file**,
  so the fe must send every key at every save. The form does: since a130769 the body is the values the
  read returned with the form's edits over them (`ServerForm.tsx` `merged`, the same merge the dirty
  compare uses), so a Server save keeps `Tags` and `Devices`, the two keys it has no control for
  (pinned by `fe/src/ServerSaveKeys.test.tsx`; at 3aaa4dc the body carried only the 19 of `toValues`
  and a save cleared the other two). The GPUs card is R5, open work in
  [PLAN.md](../../PLAN.md); the GUI's Server tab carries the list with no widget so its save keeps it
  (`server_form.rs`). Unknown keys are ignored. For the three off-by-default switches `null` and
  `false` write the same.
- The bff runs `server_cfg::check` (the number ranges, `LogVerbosity` 0..5, threads 1 to the CPU
  count, `;`/`#` in `ModelsDir`, `StateDir`, `OpencodeBaseUrl`, `OpencodeApiKey`, `Devices`
  **whatever `SaveStateOnShutdown` says**, the shape of every entry of `Devices`
  (`checks::device_list`: an empty entry, an entry with a space, one carrying `=`, `CPU` in any case,
  and `none` beside another name; judged on shape only, never against the node's GPU list, so an id
  the node has no card for is saved and the instance that names it fails to start), `0.0.0.0` alone
  in `Hostname`), normalizes the base URL (a trailing `/` and `/v1` stripped), then the manager runs
  its file checks and the address check again. The marker rule is checked before the list's, so a
  `;` or a `#` anywhere in `Devices` answers `comment-markers`, not `device-bad-name`.
- A blank (empty or spaces only) text is left unset; a blank `ModelsDir` or `StateDir` is written as
  its commented hint, on every OS; the save creates no folder.
- `OpencodeApiKey`: a string sets it (as sent, not trimmed), `null` or blank unsets it.
- The file is rendered whole from the values (`server_cfg::render_for`): unknown keys and the file's
  own comments are not kept, as in the GUI's save. The manager writes the text as rendered.
- 200 `{"revision", "running"}`: `revision` is the written file's (the hash of the rendered text),
  the one a `GET` now returns and the next `PUT` must send. `running` is true only when the instance
  is `active` or `activating` (a llama-server already launched with the old file), and true too when
  the status read after a done write failed or outran the time limit (594e15e); `deactivating`,
  `inactive` and `failed` give false.
- 422 `{"key", "args", "field"}`: a refused value; `args` are strings; `field` is the INI key it
  belongs to: `args[0]` for `not-whole-number-in-range` (`[key, value, min, max]`),
  `comment-markers` (`[key]`) and `device-bad-name` (`[key, entry]`, R2: a `Devices` entry the shape
  rule refuses, so the field is `Devices`), `ModelsMax` for `load-on-startup-over-max` and
  `load-on-startup-unlimited`, `Hostname` for `bind-all-with-others` (`args: []`) and
  `client-unresolvable-host` (`[host]`), `Port` for `client-address-in-use` (`[id]`). For any other
  key of the manager's checks `field` is `args[0]`, or `""` when there are no args. So since 5fb7b0e
  `model-profile-over-memory`, which a save of an in-use profile that narrows its `Devices` can raise,
  answers with `field` the model profile's id (`args[0]`): the server form has no control for it.
- 409 `server-changed-elsewhere`: the revision is stale (the file changed, or the bff restarted,
  since the read), or the manager found the file changed between the bff's read and its write.
  Nothing is written.
- Errors, each `{"key", "args": []}`: 401 `session-required` (checked first); 413
  `payload-too-large`; 400 `server-malformed` (the body's type, JSON or shape); 403
  `session-not-administrator` (the PUT's own read or the write refused: the session is ended); 404
  `client-no-instance`; 503 `client-unavailable` only while the write was never asked (no manager, or
  the time limit ran out in the connect or the read); 504 `client-timeout` only while the sent
  write's own outcome is unknown (a write that landed is the 200 above, whatever follows); 502
  `client-other`; and the guard's 403s. One time limit (15 s, `lib.rs` `WRITE_TIMEOUT`, since
  594e15e) covers the connect, the read, the write with the manager's checks, and the status read.

### Start, stop, restart, boot

- `POST /api/instances/{id}/start`, `/stop` and `/restart` take no body and answer 204 once the
  service manager has accepted the job; the state read just after can still be the previous one, so
  the fe polls `GET /api/instances` (any 2xx counts as accepted). Errors: 401; 403
  `client-not-authorized`; 403 `session-not-administrator` (above: the session ended);
  404 `client-no-instance`; 503 `client-unavailable`; 504 `client-timeout` (the verb handed over,
  not answered within 15 s or the manager lost: the job may have been taken); 502 `client-service-failed` with
  `args: []` when the service manager refused or the unit failed (its message stays in the bff's
  log); 502 `client-other` for anything else.
- **A start or a restart of an instance that serves no model profile** is refused 409
  `instance-no-model-profiles` `[id]`, no `field`, the session living on (served since e262708, bff
  slice 16: `crates/bff/README.md` "The instances from the manager"). The refusal comes before any
  stop, so a refused restart stops nothing, and `/stop` is never refused there. The fe's create leaves
  the new instance with no model profile, so its first Start answers this: the fe sends the user to the
  instance's model profiles first (05j).
- **A long stop** (bff slice 9b, done): with `SaveStateOnShutdown` on, the instance's own stop saves
  the slot snapshots first, up to 300 s (`TimeoutStopSec=300`, `xtask/src/linux/system_files.rs`;
  `RESTART_STOP_WAIT`, `crates/manager/src/manager/windows.rs`). `/stop` and `/restart` still
  answer 204 at once (on Windows `/restart` when the stop is accepted); meanwhile
  `GET /api/instances` reports the instance `deactivating`, then `inactive` (a restart then
  `activating`, `active`). The fe waits for it: 01-server.md S2.
- `GET /api/instances/{id}/boot` answers 200 `{"enabled": <bool>}`. Errors: 503
  `client-unavailable`, 502 `client-not-authorized` (the manager refused the bff itself, like
  `GET /api/instances`; 401 `session-required` without a session from 2026-11-19, above), 404 `client-no-instance`, 502 `client-other`.
- `PUT /api/instances/{id}/boot` takes `{"enabled": <bool>}` and answers 200 `{"enabled"}`, the
  state read back after the write. Errors: 401, 403 `client-not-authorized`, 403
  `session-not-administrator`, 404, 503, 504 `client-timeout`, 502 `client-service-failed` or `client-other`
  (each with `args: []`), 400 `boot-malformed` for a body that is not `{"enabled": <bool>}` (the fe
  never sends one), 413 `payload-too-large`.
- `GET /api/instances` and `GET /api/instances/{id}/boot` (and from 2026-11-19 `GET /api/version`) do not move a session's idle deadline:
  the fe reads the instances every 5 s while an instance page is open, and an open tab must not
  keep a session alive. The `POST`s and the `PUT` do move it.

### `GET /api/engines`

The available engines, for the model profile editor's engine field. A model profile write naming an engine
outside the list is refused with a key of its own (proposed: `unknown-engine`, `args: [name]`). A
model profile's engine is the engine whose model profile file holds it, never a key in the file (see
`GET /api/profiles/model`).

### `GET /api/profiles/model`

The one model profile list of the Models page ([02-models.md](02-models.md), slice 2a), for any signed-in
user: `[{"id", "engine", "engineAvailable", "enabled", "model", "assignments"}]`. Served since 257147e
(`crates/bff/src/presets.rs` `list`, `rows`, `row`); checked against the fe's `Models.tsx` on
2026-11-17 (02-models.md, "Checked against the bff").

- **llama-server's global section `[*]` is not listed**: it holds the defaults every model profile
  starts from, not a model profile (`presets.rs` `is_global`). The switch and the read answer 404 for it.
- **A missing model profile file is an empty group**: no file at all is 200 `[]`, never an error.

- `engine` is the id of the engine whose model profile file holds the section. No key in the file names
  it: llama-server refuses a `--models-preset` file with a key it does not know (llama.cpp v1.0.0:
  `common/preset.cpp`, "option not recognized in preset"), so each engine keeps its own model
  profile files (llama.cpp's `models.ini` and `models.ini.disabled`).
- `engineAvailable` says whether that engine is in `GET /api/engines`; the bff sends true for every
  row while llama.cpp is the only engine.
- `enabled` is true for a section of `models.ini`, false for one of `models.ini.disabled`.
- The order is the GUI's (`presets::load_listed`): the switched-on model profiles, then the
  switched-off ones, each in file order; an id in both files is listed once, as on; a missing file is an empty
  group. The fe does not sort.
- `model` (slice 2b, for the list's filter) is the value of the section's `model` key as the INI
  reader gives it (trimmed, cut at the first `;` or `#`, as for `GET /api/profiles/model/{id}`'s
  `values`), a string the bff does not otherwise change (no path resolution, no check that the
  file exists, the server machine's separators kept); `""` when the section has no `model` key or
  it is empty. The fe treats a row without a string `model` as a malformed reply.
- **`assignments`** (slice 15, 2d9a4c8) is the instances this model profile is given to, as the
  manager's record `assignments.ini` speaks of it: `[{"instance", "value", "viaAll", "effective"}]`,
  one entry per section of the record the manager lists, sorted by instance with the letter case
  aside; `value` the section's key of its own, or the `*` as that section spells it when it has none,
  `viaAll` when the `*` speaks for this model profile, `effective` where the render gives the model
  profile to that instance (`pai_core::assign::rows`, the one rule the engine's render follows). A
  section saying nothing of the model profile is no entry, so a model profile no instance is given
  carries `[]`, and a model profile no instance is given to answers `[]` in every row. The record and
  the instances are read on the same connect as the two files (four calls); a machine with no
  `assignments.ini` is 200 with `[]` in every row, while a record read that fails and an instances
  list answered as a failure fail the whole list. `crates/bff/README.md` "The model profile routes".
- Errors, each `{"key", "args": []}`: 401 `session-required`; 403 `session-not-administrator`
  (the session is ended, rules above; `client-not-authorized` is not sent); 503
  `client-unavailable` (no manager, or no answer within 5 s); 502 `client-other`.

### `PUT /api/profiles/model/{id}/enabled`

Switches a model profile on or off ([02-models.md](02-models.md), slice 2c), as `presets::set_enabled`
does: the section moves, unchanged, between `models.ini` and `models.ini.disabled` (the
destination written first; a missing disabled file created with the GUI's header). The id is
matched case-insensitively. Body `{"enabled": <bool>}`. Served since 3a63244, time-outs as now
since 594e15e (`crates/bff/src/presets.rs` `set_enabled`, `switch`, `switch_failure`, `field_of`,
`not_found`); checked against the fe's `Models.tsx` on 2026-11-17.

- 200 `{"enabled", "running", "restart"}`: `enabled` is read back after the writes (`models.ini` read again).
  Since slice 15 (2d9a4c8) `restart` is the instances the page's "restart to apply" note is about, and
  `running` is that list not being empty (the fe then says to restart them; the bff restarts nothing):
  the instances the record of `assignments.ini` gives this model profile to, effective in the pool as
  the move leaves it, that the manager lists and that are `active` or `activating`, sorted. A section
  naming an instance the manager no longer lists is a stale record, not a machine: left out, asking no
  state. No `assignments.ini`, and a model profile no instance is given, answer `running` false and
  `restart` `[]`, asking no state at all; a state that cannot be read answers `running` true with
  `restart` `[]`. A model profile already where it was asked to go answers 200 with nothing written,
  its `restart` the instances it is effective on now. When both writes landed but the read-back or the
  states failed or outran the limit, the answer is still 200: `enabled` the value asked for,
  `running` true.
- Switching on runs the checks the GUI runs: the load-on-startup model profiles counted against the lowest
  `ModelsMax` of the instances, and every model profile check on the moved section.
- The bff runs one model-profile-file write at a time: a move is two compare-and-swap writes (the
  destination first, then the source). An interruption between them leaves the model profile in
  both files: it then reads enabled, and the next switch cleans up.
- 409 `model-profile-changed-elsewhere`, `args: []`: either compare-and-swap write found its file changed
  after the bff's read; when it was the second, the move is half done (the model profile then in both
  files, read as enabled).
- 404 `no-model-profile-named` when neither file holds the id, **and for `*`** (the global section).
  `args: [id]` only when the id is one a model profile can carry (`checks::model_profile_id`, not only dots);
  otherwise `args: []`. The fe fills the text with the id it sent, never with `args`.
- 422 `{"key", "args", "field"?}` from the manager's checks on switching on:
  `load-on-startup-over-max` `[count, max]` or `load-on-startup-unlimited` `[count]` (`field`
  `load-on-startup`), any other model profile check failing on a hand-edited section (`field` its
  `args[0]`; the fe shows a key it does not know as `api-other`), and **no `field`** for
  `invalid-model-profile-id` and `model-profile-name-taken`. The page has no field to mark: the fe ignores it.
- 503 `client-unavailable` only while no write was asked; 504 `client-timeout` once one was sent
  and its outcome is unknown (the rule at the top, "Writes and their time limit").
- Other errors, each `{"key", "args": []}`: 401 `session-required` (checked first); 403
  `session-not-administrator` (the session is ended; `client-not-authorized` is not sent); 400
  `model-profiles-malformed` for a body that is not `{"enabled": <bool>}` (the fe never sends one); 413
  `payload-too-large`; 502 `client-other`.
- A model profile id is unique across every engine (user, 2026-11-15), so the id alone names the
  model profile.
  The ids `.` and `..` cannot travel in a URL path (URL Standard, dot path segments); the user
  decided (2026-11-15) that the common id check refuses them, for the GUI and the CLI too. A file
  edited by hand can still hold one: the fe sends no request for it (02-models.md 59a, 2c-E).

### `GET /api/profiles/model/{id}`

One model profile, for its page ([02-models.md](02-models.md), slice 2d, section 2d.12 there with the
reasons). Served since 36a892d (`crates/bff/src/presets.rs` `read`, `document`,
`section_revision`); checked against the fe's `ModelProfilePage.tsx` on 2026-11-17. `{id}` is one
percent-decoded path segment, matched case-insensitively; `models.ini` is searched first, so an
id in both files is the enabled copy, as in the list. Needs a session; read as the signed-in user,
with the 5 s limit of a read.

```json
{
  "id": "Qwen3-27B",
  "engine": "llama.cpp",
  "engineAvailable": true,
  "enabled": true,
  "values": {
    "model": "/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0.gguf",
    "spec-type": "ngram-mod,draft-mtp",
    "ctx-size": "172032"
  },
  "projectUrl": "https://huggingface.co/org/repo",
  "revision": "<opaque>"
}
```

- `id` as the file spells it; `engine`, `engineAvailable`, `enabled` as in `GET /api/profiles/model`.
- `values`: every key of the section, keyed by the model profile file's key name **in the file's own
  letter case** (`CTX-SIZE` stays `CTX-SIZE`; the fe reads keys in exact case, as llama-server
  does), each value a string **as the INI parser reads it, as llama-server reads the file**
  (`pai-core` `ini::parse_sections`): trimmed, **cut at the first `;` or `#`** (an inline comment
  is never part of a value, nor is anything after a `#` inside the value), quotes kept; a key
  repeated in the section gives its last occurrence; the keys sorted (a `BTreeMap`: no file
  order). No typing, no path resolution, no defaults; a key the section lacks is absent, never
  `null`.
- `projectUrl`: the section's `; url =` comment (`pai-core` `ini::project_url_of`), `""` when the
  section has none.
- `revision`: opaque, sent back by the save (`PUT`, below); required from 2h on (a read without a
  string `revision` is a failed read). A keyed SipHash-2-4 over the name of the file holding the
  section and the section's raw text (trailing blank lines and the last line's ending cut), so it
  differs per file, another model profile's edit does not change it, and switching the model profile on or off,
  or editing a comment in it, does; the key is drawn at each bff start, so it does not survive a
  restart.
- It grows without breaking: later slices read more keys from `values`, and add top-level fields
  (a comment such as `; placement =` as `placementNote`, server-computed data) or routes; no field
  is removed or retyped, and the fe ignores fields it does not know.
- Errors, each `{"key", "args"}`: 401 `session-required` (the manager not asked); 404
  `no-model-profile-named` for an unknown id, for `*` (the global section) and for an id no model
  profile can carry, `args: [id]` only for an id a model profile can carry, else `[]` (as the switch); 403
  `session-not-administrator` (the session is ended; `client-not-authorized` is not sent); 503
  `client-unavailable` (no manager, or no answer within 5 s); 502 `client-other` with empty `args`.

### `PUT /api/profiles/model/{id}`

Saves the edits of one model profile's page ([02h-model-profile-save.md](02h-model-profile-save.md)). Served since
3eb24b2 (`crates/bff/src/presets.rs` `save`, `parse_body`, `apply`, `field_of`); checked against
the fe's `ModelProfilePage.tsx` (2h.1) on 2026-11-17. Designed in `ARCHITECTURE.md`, "Saving a preset
from the web". `{id}` as in the `GET`; the section is edited in the file that holds it
(`models.ini` first, as the read), so a switched-off model profile stays switched off, as
`presets::save` does.

Body `{"revision": <string>, "values": {<key>: <string> | null}}` with `Content-Type:
application/json` (any other type is 400). **`values` holds only the keys the user changed**: a
string sets the key, `null` removes it (a key the section lacks: nothing to remove). A key not in
`values` is not touched.

The answers come in this order: 401 `session-required`; 413 `payload-too-large`; the body (400,
then 422 `comment-markers`), with no manager call; the read (403, 503, 502); 404; the revision
(409); the write (409, 422 from the manager's checks, 403, 504, 502); 200.

- **Edited in place, not re-rendered.** Only the changed keys' lines are written; an edited line is
  rewritten whole (`key = value`), so an inline comment on that line is lost; a line that already
  spells the key and holds the value is left byte for byte. Every other line of both files stays
  byte for byte: comments, keys the web UI does not show, their order and spelling, the line
  endings. A new key is appended to the section. **Duplicates of an edited key** are cleaned up,
  not refused: a set keeps one line, at the first one's place; a removal drops every line of the
  key. **Differs on purpose from the Server PUT and from the configurator's save**, which render
  the whole section from their struct (`presets::render_section`) and so drop comments and unknown
  keys and rewrite spellings (02h D-a; the user chose this, 2026-11-16).
- **Accepted keys**: a fixed list of **65**, exactly the keys 2h.1 to 2h.9 enable (`pai-core`
  `checks::WEB_MODEL_PROFILE_KEYS`; checked against 02e's tables on 2026-11-17): every key the
  configurator writes minus the ten of `MODEL_PROFILE_KEYS_NOT_ON_WEB`: `model`, `mmproj`, `model-draft`,
  `spec-type` (Assets, until 2i), `n-cpu-moe`, `moe-cache-mib` (MoE, until 2g), `device`,
  `split-mode`, `tensor-split` (GPU distribution, until 2f), `override-tensor` (Tensor placement,
  until 2j). **From bff slice G1** `device`, `split-mode` and `tensor-split` move to the
  accepted list (68 keys, seven left out), for the GPU distribution card (`fe/briefs/14-gpu-distribution.md`); `n-cpu-moe` (0 to 4096) from L3 (4cfa909) and `override-tensor` from J1 (6570557): 70 keys, five left out. A slice that makes one editable moves it to the list in the same commit. **Body keys
  match in exact case** (`CTX-SIZE` is 400): the fe sends each key as 02e's tables spell it.
- **400 `model-profiles-malformed`**, `args: []`: a non-JSON body or another content type; `revision` not
  a string or `values` not an object; a key outside the list; a value neither a string nor `null`;
  a value empty after trimming spaces and tabs (clearing a key is `null`); a value holding a
  control character other than tab; a value that starts or ends with another Unicode white space
  (U+00A0, U+2028, U+3000 ...: the file's reader trims those, so it would read back shorter). A
  value is written trimmed of spaces and tabs.
- **422 `comment-markers`** for set values holding `;` or `#`: `args` every such key of the body,
  sorted, `field` the first of them (the strings take `{0}` only). Nothing is written.
- `{}` (no change), or edits that leave the file's text as it is: 200 with nothing written, the
  revision still compared (409 if stale), `running` asked as after a write.
- Then the manager checks the edited section **whole** (`checks::models_file` or
  `models_disabled_file`, `model_profile_section`): integer ranges, floats, enums (compared **exactly**,
  case included, as llama.cpp does: `load-mode = MLOCK` is refused), JSON object, `fit-target`,
  `override-tensor`, min <= max, `moe-cache-mib` with `split-mode tensor`, a `model`, and, in
  `models.ini`, the load-on-startup count against `ModelsMax`. So a hand-edited bad value the user
  did not touch is refused too, on its own field. **The text keys** (`model`, `mmproj`,
  `model-draft`, `chat-template-kwargs`, `override-tensor`, `reasoning-budget-message`,
  `fit-target`) are checked for `;` and `#` on the **raw line, inline comment included**
  (`checks.rs` `RawSection`, `model_profile_section`): an untouched `model = a.gguf ; Q8` makes every web
  save of that model profile 422 `comment-markers`, field `model` (open on the bff side, below). `fit` is
  not checked.
- The write: the manager's compare-and-swap write as the signed-in user, given the text the bff
  read; it is asked only while the answer is still wanted.
- **200 `{"revision", "running", "restart"}`**: `revision` of the section as now written, computed by the same
  function as the `GET`'s over the text written, so the `GET` that follows returns it and the next
  save meets no 409. Since slice 15 (2d9a4c8) `restart` is the instances the page's "restart to apply"
  note is about, and `running` is that list not being empty: the instances the record of
  `assignments.ini` gives this model profile to, effective in the pool as the save leaves it (a
  switched-off profile is effective where the record's `*` is `on`), that the manager lists and that
  are `active` or `activating`, sorted. A machine with no `assignments.ini` names none and asks no
  state, a record section naming an instance the manager no longer lists is left out, and a state that
  cannot be read answers `running` true with `restart` `[]`; a save that wrote nothing runs the same
  rule on the text the file already holds. The fe says "Saved: restart llama-server
  to apply." for true, "Saved." for false. The bff restarts and reloads nothing. Deliberate
  difference: the configurator's model profile save says only "Saved model profile [id]"
  (`preset-saved` in `crates/core/strings/en_US.toml`).
- **422 `{"key", "args", "field"}`** from the manager's checks, `field` the INI key to mark:
  `args[0]` for `not-whole-number-in-range` (args key, value, min, max), `comment-markers` (key),
  `not-a-number` (key, value), `not-one-of` (key, value, the allowed list joined by `, `),
  `not-json-object` (key), `min-above-max` (min key, min, max key, max: the minimum's key is
  marked); `fit-target` for `fit-target-format` (value, `MiB`); `moe-cache-mib` for
  `moe-cache-with-tensor-split` (no args); `load-on-startup` for `load-on-startup-over-max` (count,
  max) and `-unlimited` (count); `model` for `model-profile-no-model` (id); `override-tensor` for
  `override-no-pattern` (position) and `override-no-device` (position, pattern), and from J1
  `override-tensor-space` (position) and `override-bad-device` (position, device); from L4,
  `model-profile-device-outside` (profile, instance, key, device), its `field` `args[2]`, the key; from G1, `args[0]`
  (`device`, `device-draft` or `mmproj-device`) for `device-bad-name` (key, entry) and `tensor-split`
  for `tensor-split-format` (value); neither is judged against the probed GPUs; **no `field`** for
  `invalid-model-profile-id` and `model-profile-name-taken` (id). `bind-all-with-others` cannot come from a
  model profile. Nothing is written.
- 409 `model-profile-changed-elsewhere`, `args: []`: the revision is stale (the section changed, it was
  switched on or off, or the bff restarted since the read), or the compare-and-swap write found
  the file changed after the bff's read. A save of **another** model profile in the same file gives no
  409. Nothing is written.
- Errors, each `{"key", "args"}`: 401 `session-required`; 403 `session-not-administrator` (at the
  read or the write; the session is ended; `client-not-authorized` is not sent, the fe still
  handles it); 404 `no-model-profile-named` (`args` as the read's) for an id neither file holds and for
  `*`; 413 `payload-too-large`; 503 `client-unavailable` (no write asked: the read failed or ran
  out of its 5 s, or the 15 s ran out before the write); 504 `client-timeout` (the write was sent
  and its outcome is unknown; a write that landed is the 200, `running` true, whatever follows);
  502 `client-other`.

### `POST /api/profiles/model` and `DELETE /api/profiles/model/{id}` (MP2, served 76163d1)

Served with no page of their own yet; the design is "(3) The bff routes" in `ARCHITECTURE.md`, the
detail in [crates/bff/README.md](../../crates/bff/README.md#the-create-and-the-delete).

- **Create**: `{"id": string, "from": string}` (a copy of a pool profile) or `{"id": string,
  "values": {"model": string} | {"hf-repo": string, "hf-file"?: string}}`. 201 `{"id"}`; the new
  profile is a list row with `enabled: false` and no assignments. Texts are trimmed of spaces and tabs.
- **Delete**: no body. 204.
- Errors, each `{"key", "args"}` (422 adds `field`): 400 `model-profiles-malformed` for a body the bff
  cannot read (not JSON, an unknown field, both or neither of `from` and `values`, a `values` key
  outside the three, a non-string, an empty text, a control character), the manager not asked; 413
  `payload-too-large`; create: 422 `invalid-model-profile-id` (`field` `id`), `no-model-profile-named`
  (`field` `from`), `model-profile-no-model` or `model-profile-bad-source` (`field` `values`),
  `comment-markers` (`field` the key), 409 `model-profile-name-taken` `[id]`; delete: 404
  `no-model-profile-named` `[id]`, 409 `model-profile-enabled` `[id]`, 409 `model-profile-assigned`
  `[id, instances]`; both: 401 `session-required` (the manager not asked), 403
  `session-not-administrator` (the session ended), 503 `client-unavailable` (no manager, or the 15 s
  ran out before the call was handed over), 504 `client-timeout` (the call handed over, its outcome
  unknown: a retried create is then 409, a retried delete 404), 502 `client-other`.

## Open on the bff side

- **An administrator's save needs no second password** (user, 2026-11-15): the bff vouches for the
  user it authenticated, and the manager grants `configure` to members of the `pai` group. No
  status asks for the password again.
- **The text keys' `;` / `#` check reads the raw line** (`checks.rs` `model_profile_section`), so an
  inline comment the user never touched on `model`, `mmproj`, `model-draft`, `override-tensor`,
  `chat-template-kwargs`, `reasoning-budget-message` or `fit-target` blocks every web save of that
  model profile (the configurator's save re-renders the line and is not affected). Proposed: check the
  value as read (`RawSection::get`) for lines the save did not change. Platform's decision.
- **Nothing creates `ModelsDir` after a save** (the GUI creates it as the logged-in user); the design
  recommends keeping it so, for the user to decide (`ARCHITECTURE.md`, "ModelsDir after a save").
- **A bind list mixing `0.0.0.0` with other addresses** is refused by the save's `server_cfg::check`
  (422 `bind-all-with-others`, field `Hostname`), served since eff112d.
- Closed 2026-11-17 (594e15e): a done write whose status read is slow or fails answers 200 with
  `running: true`, and writes get 15 s ("Writes and their time limit", above). A negative
  `ModelsMax` is refused by the served `PUT` (422 `not-whole-number-in-range`, field `ModelsMax`,
  `server_cfg::check`).

## Planned: profiles, nodes and instances (PLAN step 10 items 13 and 13a), not built

**What of this section is served is stated where it is.** The server profile pool is (b535304,
`crates/bff/README.md` "The server profile pool"), and so are the four routes of "An instance's
associations" (bff slice 15, 2d9a4c8, `crates/bff/README.md` "The association routes"): that block
below is the design, the README is what the bff answers today, and the two differ in the details noted
there. So are the two instance routes below, the `removable` of the list and the start's 409
`instance-no-model-profiles` (bff slice 16, e262708, `crates/bff/README.md` "Creating and removing an
instance"). Everything else is not built. It records the routes as designed in `ARCHITECTURE.md`, "Nodes
and instances", block "Revision: profiles" (commit f3a519c), which wins over (a) to (j) of the same
record, where "preset" reads as "model profile". The record is the authority; this section is the
designer's reading of it against the fe on 2026-11-17, and replaces the earlier version of this section
(the pool at `/api/presets`, the instance's Server and Presets tabs), which is void. The sections above
stay the contract of what is built until a slice moves a route from here into them.

### The words

- **Model profile** (was preset): one model's settings, a section of the engine's `models.ini` (was
  `presets.ini`) or `models.ini.disabled`. **Server profile** (was the instance's server settings): one
  `server@<profile>.ini`, same format and document as today's server file. Both are **shared pools per
  engine** (llama.cpp now). Displayed in sentence case: "Model profile", "Model profiles", "Server
  profile", "Server profiles". Never a bare "profile" in a route or key.
- **An instance** is given ONE server profile (its port, tags, `ModelsMax` and the rest) and N model
  profiles (`assignments.ini`). A server profile serves **one instance at a time**; unused profiles are
  allowed and serve nothing. A profile changed while its instance runs applies at the next start.

### Step 4b: the wire rename (the platform's slice 4, no behaviour change)

**The model profile part is DONE**: built in the bff by 5d54590 and in the fe by ec6dbd0
([05-profiles-rename.md](05-profiles-rename.md)); its routes and keys are in the sections above
(the note under "Routes" lists the old names). What 4b planned and is **not built**: the server
route, served today at `/api/instances/{id}/server` with `client-no-instance` (open point 4 below).

| Built today | Planned |
| --- | --- |
| `GET`, `PUT /api/instances/{id}/server` | `GET`, `PUT /api/profiles/server/{id}` (`default` today: the profile is still the instance's own file) |
| `client-no-instance` on the server routes | `client-no-server-profile` `[id]` |

Bodies, replies, revisions, statuses and time limits unchanged. Unchanged routes:
`GET /api/instances`, `POST /api/instances/{id}/start|stop|restart`, `GET`/`PUT
/api/instances/{id}/boot`, the session, `GET /api/version`.

### As designed (after 4b)

- **Node prefix.** `GET /api/node`, signed in since slice 13 (built above, with `version` and `engines`): `{"name": <host name>, "api": "/api"}`; the node-scoped
  routes (`instances`, `profiles`, `engines`) sit under `api`; hub mode answers `GET /api/nodes` with the
  same objects and `api` = `/api/nodes/<node>`. The fe accepts only a same-origin path equal to `/api`
  or starting with `/api/`.
- **`GET /api/instances`**, anyone: `[{"id", "engine", "state", "serverProfile", "port", "tags",
  "removable"}]`, sorted by id; `port` and `tags` are the server profile's (port 0 and no tags when its
  file is missing); `removable` false for `default`, served since e262708 (the built read, above); one
  failed status read gives that item `"state": "unknown"`; a never-started instance is `inactive`.
- **Model profile pool**, administrator: `GET /api/profiles/model` rows `{"id", "engine",
  "engineAvailable", "enabled", "model", "assignments"}`, `assignments` = `[{"instance", "value",
  "viaAll", "effective"}]` sorted by instance, every instance with an explicit key or covered by `*`;
  `GET`/`PUT /api/profiles/model/{id}` and `PUT .../enabled` as built (sections above), their 200 gaining
  `"restart": [<instance id>]` (the instances where the profile is in effect as the write left it, and
  running; `running` true when it is non-empty or a state could not be read, then `restart` is `[]`).
- **Server profile pool**, administrator:
  - `GET /api/profiles/server`: `[{"id", "engine", "port", "tags", "instance"}]`, `instance` the id
    using it or `null`, sorted by id. Errors 401, 403, 503.
  - `GET`/`PUT /api/profiles/server/{id}`: today's server document and save (`values`, `revision`,
    `defaults`, `limits`, `interfaces`, `withheld`; 200 `{"revision", "running"}`, `running` for the
    instance using it). 404 `client-no-server-profile` `[id]`, 409 `server-changed-elsewhere`, 400
    `server-malformed`, the check keys 422 (`client-address-in-use` `[instance]` when the profile is in
    use; two unused profiles may share a port; and since 5fb7b0e, when an instance runs the profile and
    the save moves its `Devices`, `model-profile-over-memory` `[profile, instance, needed MiB,
    available MiB]`). `Tags` (0 to 8, each 1 to 32 of `a-z`, `0-9`, `-`, no
    duplicates, `server-bad-tags`) is kept when absent from the body, `[]` clears it.
  - `POST /api/profiles/server` `{"engine", "id"}`: 201 `{"id"}`; the new profile is the defaults
    (`render_for(ServerConfig::default(), true)`). 422 `server-profile-bad-id` `[id]` (`field` `id`;
    the instance id rule: 1 to 32 of `a-z`, `0-9`, `-`, not starting with `-`), 422 `unknown-engine`,
    409 `client-server-profile-exists` `[id]`.
  - `DELETE /api/profiles/server/{id}`: 204; 404 `client-no-server-profile`, 409
    `client-server-profile-in-use` `[id, instance]`.
- **An instance's associations**, administrator (`configure`), **served since 2d9a4c8** (bff slice 15,
  `crates/bff/src/instance_profiles.rs`; what the bff answers today is `crates/bff/README.md` "The
  association routes", the statuses are pinned by `crates/bff/tests/instance_profiles.rs`). They write
  the manager's record `assignments.ini`: one section per instance, one key per model profile, `*` for
  the whole pool. The words are `on`, `off`, `inherit` (follow the pool's own switch) and `none` (no
  key). The server write's body is **`{"serverProfile": <name>}`**, not the `id` of this design, and
  the refusals that name it carry `field: "serverProfile"`, not `id`.
  - `PUT /api/instances/{id}/profiles/server` `{"serverProfile": <name>}`: 200 `{"running"}`, true
    only when the record of `instances.ini` named another profile (a record naming none is a change)
    and the instance is up; a record that already names it answers false and asks no state, a record
    or a state that cannot be read answers the safe true. Nothing is started, stopped or restarted.
    400 `server-profile-malformed`; 404 `client-no-instance` (the record of the instances is read
    first, on the bff's own account, nothing written); 404 `client-no-server-profile` `[profile]`; 409
    `client-server-profile-in-use` `[profile, instance]`; 422 `client-address-in-use` (`args` the
    port) and `client-unresolvable-host` (`args` the host), both `field` `serverProfile`; 422
    `load-on-startup-over-max` and the other checks, `field` `serverProfile`, among them since 5fb7b0e
    `model-profile-over-memory` `[profile, instance, needed MiB, available MiB]`: a model profile of
    the instance that fits the GPUs of the profile it runs today and not those of the target.
  - `GET /api/instances/{id}/profiles/model`: 200 `{"all": "inherit" | "none", "profiles": [rows]}`,
    `all` the section's `*` as the page shows it (`inherit` when the section carries the key, `none`
    when it does not), rows the pool row's fields **without `assignments`** plus `value` (the explicit
    key only: `none` when only `*` covers it), `viaAll`, `effective` (served by this instance after
    its next start), in the pool's order. Four calls on one connect as the user, under the reads' 5 s:
    the instances list (the bff's own account, first), `assignments.ini`, `models.ini`,
    `models.ini.disabled`. An instance with no section answers `all: "none"` and every row
    `value: "none"` with `viaAll` and `effective` false. 404 `client-no-instance` with no file read;
    the pool list's errors otherwise. Moves the idle deadline.
  - `PUT /api/instances/{id}/profiles/model/{profile}` `{"value": "none" | "inherit" | "on" | "off"}`:
    200 `{"effective", "running"}`: `effective` is the row of the record as the write left it;
    `running` is the instance's state, asked only when the write changed the set of model profiles the
    instance is given, so a key landing where the profile already stood answers false with no state
    asked, and a record that cannot be read back answers the safe true. No revision, never 409: the
    last write wins. 400 `assignment-malformed` and 422 `model-profile-bad-value` (`args` the word,
    `field` `value`) before any call; 404 `no-model-profile-named` (`args` the name, and `args: ["*"]`
    for the path's `*`, llama-server's global section, the manager asked nothing), 404
    `client-no-instance`, the resource refusals 422 (no `field`).
  - `PUT /api/instances/{id}/profiles/model` `{"all": "inherit" | "none"}` (sets `*`): 200
    `{"running"}`, the same `running` rule; 400 `assignment-malformed`, 422 `model-profile-bad-value`
    (`field` `all`), 404 `client-no-instance`.
  - Every one of the four: 401 `session-required`; 403 `session-not-administrator` (the session ended,
    the manager refusing the user's connect, file read, write or record read); 502
    `client-not-authorized` (the record of the instances refused on the bff's own account, the session
    lives on, nothing written), 502 `client-other`; 503 `client-unavailable` (no manager, the reads'
    5 s, the writes' 15 s), **never 504**: a write that outlives the limit is 503 here, the rule at the
    top, "Writes and their time limit". A state read after a write that stands, on the bff's own
    account, never ends the session: refused, timed out or failed, it answers `running` true.
  - Meaning: `inherit` follows the profile's global switch; `on` and `off` override it (`on` serves it
    with the switch off). The fe offers three per row: with `all` `inherit`: inherit, on, off; with
    `none`: not assigned, inherit, on. No page calls these routes yet.
- **Instances**, administrator (`configure`), **served since e262708** (bff slice 16,
  `crates/bff/src/instances.rs`; what the bff answers is `crates/bff/README.md` "Creating and removing
  an instance", the statuses pinned by `crates/bff/tests/instances.rs`). They write the manager's
  records through `CreateInstance` / `RemoveInstance`
  (`crates/config/README.md` "Through the manager (Linux, Windows)"), as the signed-in user, under the
  writes' 15 s.
  - `POST /api/instances` `{"engine", "id", "serverProfile"}`: 201 `{"id"}` with the id the body sent.
    The new instance has no model profile, start at boot off, and nothing is started, so its first
    Start is refused until a model profile is assigned. 400 `instance-malformed` (the body is not
    those three strings, no manager connected to), 413 over the bff's 64 KiB; 422 `instance-bad-id`
    (`args` the id, `field` `id`), `unknown-engine` (`args` the engine, `field` `engine`),
    `client-address-in-use` (`args` the other instance) and `client-unresolvable-host` (`args` the
    host), both `field` `serverProfile`; 404 `client-no-server-profile` (`args` the name, **no
    `field`**: the manager's refusal is of the profile, not of a control the form can mark); 409
    `instance-exists` (`args` the id, no `field`), `client-server-profile-in-use` (`args` the profile
    and the instance that runs it). The manager's other checks keep their key as a 422 with no `field`.
  - `DELETE /api/instances/{id}`: 204. 404 `client-no-instance` (no `args`, an id the record does not
    name and an id that is no name at all); 409 `instance-protected` (`args` the id, `default`) and
    `instance-running` (`args` the id, an instance that is up: stop it first, the fe offers Remove only
    for a stopped one); the server profile it ran stays in the pool.
  - Both: 401, 403 `session-not-administrator` (the session ended), 503 `client-unavailable` (no
    manager, or the 15 s ran out before the call was handed over), 504 `client-timeout` (the call
    handed over, its outcome unknown: a retried create is then 409 `instance-exists`), 502
    `client-service-failed` (the service manager refused) and 502 `client-other`.
- **Start and restart** answer 409 `instance-no-model-profiles` `[id]`, no `field`, the session living
  on, (before any stop, so a refused restart stops nothing) when the instance serves no model profile:
  **served since e262708** (`crates/bff/src/control.rs`, `crates/bff/README.md` "The instances from the
  manager"). `Stop` is never refused.
- **The resource checks**, only on what a write puts into effect (an assignment or override on, the
  global switch on, a model profile's save, a server profile assigned, or saved while in use: at the
  last two the memory moves and not the need, so only a model profile that fits the cards named today
  and not the new ones refuses, 5fb7b0e); every refusal writes nothing and names the profile and the
  instance: `model-profile-over-memory` `[profile, instance, needed MiB, available MiB]` (whole MiB;
  no `field` on the model routes, `field` `serverProfile` on the server profile switch, whose 422
  refusals all carry it), the memory being what the cards that server profile's `Devices` names: every
  GPU of the node when the key is absent, and nothing to judge for `none` or ids the probe knows none
  of; `model-profile-model-missing` `[profile, instance, path]` (`field` `model` on a save),
  `load-on-startup-over-max` `[count, max, instance]`, `load-on-startup-unlimited` `[count, instance]`
  (the instance appended last, so the built texts keep working). `preset-engine-mismatch` is dropped:
  another engine's profile is simply not found.
- Every write: 401, 403 `session-not-administrator` (the session ended), 413, the guard's 403s, 503
  before asking, 504 `client-timeout` once asked (15 s). The four association routes (served) keep every part
  of that except the 504: a manager that outlives their limit, before or after a write, is 503
  `client-unavailable`.

### The web UI's plan (designer, 2026-11-17; a proposal until the user answers)

**Superseded by [16-server-profiles-and-instances.md](16-server-profiles-and-instances.md)
(2026-11-21)**, which builds it with two changes: a model profile on an instance is one "Served
here" switch, not a three-word select (its decision 7), and the server profile select switches only
on a button (its decision 6). On `client-address-in-use`: `crates/bff/README.md` says the association
route's `args` is the port, but `instance_profiles.rs` passes the manager's message, as the create and
the save do (the other instance's id); the fe fills `api-client-address-in-use` with `args[0]` either
way (to confirm on the bff side).

It replaces three earlier decisions: the instance's Server and Presets tabs (now: the server form is a
server profile's page, the instance page makes the associations), the node-level Models entry (now:
Profiles > Model), `#/models` as the pool (now `#/profiles/model`).

- **Nav**: Overview; the node's `name` as a group heading, its instances; a **Profiles** group with
  two links, Server and Model (their exact text: user question), each list labelled by its group
  heading (`aria-labelledby`, as the Instances list), a profile's page marking its list entry
  `aria-current="true"` (as the preset page marks Models today).
- **Routes**: `#/instances/<id>` the instance: header (state, Start / Stop / Restart, start at boot:
  user question), its server profile (a select over the engine's unused profiles plus its own, each
  option with its port; profiles used by another instance listed disabled with that instance's id), its
  model profiles (the assignment table). `#/profiles/server` the list (id, port, tags, used by),
  `#/profiles/server/<id>` today's Server form and save, with "Used by <instance>" and a Delete while
  unused. `#/profiles/model`, `#/profiles/model/<id>`: today's Models list and preset page. Old
  `#/models` and `#/models/<id>` links: user question.
- **The assignment table** keeps the earlier proposal: one row per pool profile, its id (a link to
  its page), model, global switch (read-only here), a select "On this instance" saving at once
  ([3.2.2 On Input](https://www.w3.org/TR/WCAG22/#on-input): a save is not a change of context), "Served
  here" (Yes / No from `effective`), the outcome and any restart in a polite live region ([4.1.3 Status
  Messages](https://www.w3.org/TR/WCAG22/#status-messages)).
- **A refusal on a select** (409 in use, 422 address or memory) is shown under that control and linked
  by `aria-describedby` ([3.3.1 Error Identification](https://www.w3.org/TR/WCAG22/#error-identification)),
  the select back on the value the bff holds.
- Build order: [05-profiles-rename.md](05-profiles-rename.md), "The slices that follow".

### Open points for the platform side

The second round (old 1 to 12) is answered in the record's (j) and carried above under the new names.
Open now:

1. **Who uses a server profile, on its page.** The document of `GET /api/profiles/server/{id}` and the
   200 of its `PUT` do not name the instance using it; the page needs it to say "Used by <id>", to
   offer Delete only while unused, and to say which instance to restart after a save. Proposed:
   `"instance": <id> | null` in both, as in the list.
2. **A new server profile's port.** `render_for(ServerConfig::default(), true)` leaves `Port` unset,
   so every new profile reads `server_cfg::DEFAULT_PORT` (9931); a second one assigned while another is
   in use is refused `client-address-in-use`. Proposed: `POST /api/profiles/server` takes an optional
   `"port"` (checked as a save's), or the user creates, then edits the port before assigning: which?
3. Closed at 2d9a4c8, as proposed: the body of `PUT /api/instances/{id}/profiles/server` is
   `{"serverProfile": <profile>}` (400 `server-profile-malformed` for anything else) and the refusals
   that name it, the 422s of the address and of the checks, carry `field` `serverProfile`.
4. **Step 4b's server route.** Confirm: `/api/profiles/server/{id}` keys on today's `server@<id>.ini`
   (so `default`), `running` as today, and a missing file answers 404 `client-no-server-profile` `[id]`
   where it answered `client-no-instance` (the fe shows today's "no instance" text until the server
   profile page exists).
5. **The strings of the renamed keys.** The fe copies the GUI's `en_US.toml` texts for its `api-*`
   keys; the fe's rename proposes "preset" -> "model profile" in them
   ([05-profiles-rename.md](05-profiles-rename.md), section 2). Confirm the toml uses the same words.
6. Closed at 2d9a4c8: on `PUT /api/instances/{id}/profiles/server` every 422 carries `field`
   `serverProfile` (the address refusals and the manager's checks), the 404 and 409 carry none.
7. **Deleting the profile `default`.** Allowed once no instance uses it? The GUI and `pai` act on the
   profile `instances.ini` names for `default`, so nothing else needs the name. Confirm no protection.
8. **The run verbs and boot**: unchanged at `/api/instances/{id}/...` (not listed in (f')). Confirm.
9. **`GET /api/profiles/server` errors and session**: 502 `client-other` too, and it moves the idle
   deadline like the model profile list? The fe filters by the instance's `engine` itself.
10. Closed at 2d9a4c8: the body of `PUT /api/instances/{id}/profiles/model` is `{"all": "inherit" |
    "none"}` (422 `model-profile-bad-value`, `field` `all`, for any other word), as the fe builds it.
