# The bff API the web UI is built against

The contract between `fe/` and `pai-bff` for slice 1 (the shell, sign-in and the Server page of one
instance: [01-server.md](01-server.md)). The fe is tested against these shapes with mocked replies;
the bff side is scheduled by the coordinator of `crates/`. A route marked **proposed** does not
exist yet. Change this file first when a shape changes.

## Rules for every route

- Everything is under `/api/`, JSON in and out.
- **Errors** are `{"key": "<strings key>", "args": [...]}` with a non-2xx status. The key is a
  configurator strings key where a check already exists (the refusals of `pai-app`
  `server_control` and `pai-core` `checks.rs`). The fe shows it as its own string `api-<key>`,
  filling `{N}` from `args`; a key it does not know, a malformed body or a network failure is shown
  as one generic failure. The one exception: a 422 with a key the fe does not know is shown as a
  refused value without its reason (`api-other`), since the status already says a check refused it.
- **401** means not signed in (no session, or it ended), on any route that needs one.
- **403** `client-not-authorized` means the manager refused the call for this user, on a route that
  acts for the signed-in user; the session lives on. No served route sends it today (since bff slice
  9e every refusal of the user ends the session, below); the fe keeps handling it. On a route that needs no sign-in
  (`GET /api/instances`, `GET .../boot`) the same key comes with **502**: there the manager refused
  the bff itself, not a user.
- **403** `session-not-administrator` on any route but `POST /api/session` (served since bff slice 9e, 5f0d405, on `GET .../server`, the three verbs and `PUT .../boot`; S1 of
  [01-server.md](01-server.md)): the manager refused the session's user, who is no longer a member
  of `pai`, and the bff has **ended that session** at once (user, 2026-11-16: removed from `pai`, the
  session ends). The next request with the cookie answers 401; no `Set-Cookie` clears it. The bff
  sends this key exactly when it ends a session on a refusal, and `client-not-authorized` only when
  the session lives on, so the fe needs no second read of `GET /api/session` to tell them apart.
  Where: `crates/bff/src/server.rs` `read` and `control.rs` `action_failure`. The session-free reads
  (`GET /api/instances`, `GET .../boot`) keep 502 `client-not-authorized`.
- **The guard's keys**, each 403 or 413 with `args: []`, answered before any session is looked at
  (`crates/bff/src/origin.rs`): `request-cross-origin` (a write whose `Origin` or `Sec-Fetch-Site`
  is not this bff), `request-bad-host` (a `Host` that is not `localhost`, `127.x.x.x` or `[::1]`),
  `payload-too-large` (413, a body over 64 KiB). The fe's own requests never get them: the guard
  stands in front of `/` and `/assets/*` too, so a page opened under another name never loads, and
  the fe's bodies are a few dozen bytes. The fe shows them as the generic failure of the place.
- **Idle deadline**: every request with a valid session moves it, except the session-free reads
  `GET /api/instances` and `GET /api/instances/{id}/boot`; the `POST`s and `PUT`s move it.
- **The fe picks its message by the key, never by the status**, apart from 401 (sign in).
- **503** `client-unavailable` means no manager to reach or no answer in time.
- **404** `client-no-instance` means the instance id does not exist.
- The session is an HttpOnly cookie set by the bff; the fe never reads it.

## Routes

| Route | Who | Slice | Success |
| --- | --- | --- | --- |
| `GET /api/version` | anyone | exists | `{"version"}` |
| `GET /api/instances` | anyone | 1a, changed | `[{"id", "engine", "state"}]` |
| `GET /api/session` | anyone | 1a | 200 `{"user"}`, or 401 |
| `POST /api/session` | administrator (`pai` group) | 1a | 200 `{"user"}` and the cookie |
| `DELETE /api/session` | signed in | 1a, proposed | 204 |
| `GET /api/instances/{id}/server` | administrator | 1a, proposed | the server document below |
| `PUT /api/instances/{id}/server` | administrator (`configure`) | 1b, proposed | `{"revision", "running"}` |
| `POST /api/instances/{id}/start`, `/stop`, `/restart` | administrator | 1c, served (9a) | 204 |
| `GET /api/instances/{id}/boot` | anyone | 1c, served (9a) | `{"enabled"}` |
| `PUT /api/instances/{id}/boot` | administrator (`configure`) | 1c, served (9a) | `{"enabled"}` |
| `GET /api/engines` | anyone | presets slice, proposed | `[{"id", "name"}]` |
| `GET /api/presets` | administrator | 2a, 2b, proposed | `[{"id", "engine", "engineAvailable", "enabled", "model"}]` |
| `PUT /api/presets/{id}/enabled` | administrator (`configure`) | 2c, proposed | `{"enabled", "running"}` |
| `GET /api/presets/{id}` | administrator | 2d, proposed | `{"id", "engine", "engineAvailable", "enabled", "values", "projectUrl", "revision"}` |

### `GET /api/instances`

Today's reply gains `engine` (the engine id, `llama.cpp` today) and its errors move from
`{"error": "unavailable"}` to the common shape: 503 `client-unavailable`, 502
`client-not-authorized`, 502 `client-other` for any other failure, each with empty `args` (the
manager's detail never reaches the body).

### The session

- `GET /api/session` answers 404 until the bff has the route; the fe reads any reply but 200 as
  signed out.
- **Only administrators sign in** (user, 2026-11-15): an administrator is a member of the OS group
  `pai` on the server machine. The session reply is `{"user"}`, which is all the bff sends since
  commit 296165f; the fe still accepts a reply that carries `canConfigure` and ignores the field
  (an older bff sent it). The manager still decides
  every request, and a refusal is 403 `client-not-authorized`.
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

Body `{"revision", "values"}`, `values` keyed and typed as the read gives them (`null` unsets a key);
every key of the read is sent. Designed in `ARCHITECTURE.md`, "Saving the server file from the web";
not served yet (bff slice 8, sub-slice 6).

- The bff runs `server_cfg::check` (the number ranges, `LogVerbosity` 0..5, threads at most the CPU
  count, `0.0.0.0` alone in `Hostname`), strips a trailing `/` and `/v1` from the base URL, then the
  manager runs `checks::server_file` and the address check again.
- A blank `ModelsDir` or `StateDir` is left unset in the file, on every OS; the save creates no folder.
- `OpencodeApiKey`: a string sets it, `null` unsets it.
- The file is rendered whole from the values (`server_cfg::render_for`): unknown keys and the file's
  own comments are not kept, as in the GUI's save.
- 200 `{"revision", "running"}`: `revision` is the new file's, the one a `GET` now returns;
  `running` chooses between "Saved." and "Saved: restart llama-server to apply." (`true` also when
  the status read after the write failed).
- 422 `{"key", "args", "field"}`: a refused value; `field` is the INI key it belongs to: `args[0]`
  for `not-whole-number-in-range` and `comment-markers`, `ModelsMax` for `load-on-startup-over-max`
  and `load-on-startup-unlimited`, `Hostname` for `bind-all-with-others` (`args: []`) and
  `client-unresolvable-host` (`[host]`), `Port` for `client-address-in-use` (`[id]`).
- 409 `server-changed-elsewhere`: the revision is stale (the file changed, or the bff restarted,
  since the read), or the manager found the file changed between the bff's read and its write.
  Nothing is written.
- Errors, each `{"key", "args"}`: 401 `session-required`; 400 `server-malformed` (a body or a value
  not of the read's type); 403 `session-not-administrator` (the session is ended); 404
  `client-no-instance`; 503 `client-unavailable` (also a time-out after the manager may have
  written until 504 is served); 502 `client-other`.
- 504 `client-timeout`, `args: []` (accepted, not served yet): the time limit ran out after the
  write was sent to the manager, so the file may or may not be saved; 503 `client-unavailable` stays
  for a manager never reached.

### Start, stop, restart, boot

- `POST /api/instances/{id}/start`, `/stop` and `/restart` take no body and answer 204 once the
  service manager has accepted the job; the state read just after can still be the previous one, so
  the fe polls `GET /api/instances` (any 2xx counts as accepted). Errors: 401; 403
  `client-not-authorized`; 403 `session-not-administrator` (above: the session ended);
  404 `client-no-instance`; 503 `client-unavailable`; 502 `client-service-failed` with
  `args: []` when the service manager refused or the unit failed (its message stays in the bff's
  log); 502 `client-other` for anything else.
- **A long stop** (bff slice 9b, done): with `SaveStateOnShutdown` on, the instance's own stop saves
  the slot snapshots first, up to 300 s (`TimeoutStopSec=300`, `xtask/src/linux/system_files.rs`;
  `RESTART_STOP_WAIT`, `crates/manager/src/manager/windows.rs`). `/stop` and `/restart` still
  answer 204 at once (on Windows `/restart` when the stop is accepted); meanwhile
  `GET /api/instances` reports the instance `deactivating`, then `inactive` (a restart then
  `activating`, `active`). The fe waits for it: 01-server.md S2.
- `GET /api/instances/{id}/boot` answers 200 `{"enabled": <bool>}`. Errors: 503
  `client-unavailable`, 502 `client-not-authorized` (a route that needs no sign-in, like
  `GET /api/instances`), 404 `client-no-instance`, 502 `client-other`.
- `PUT /api/instances/{id}/boot` takes `{"enabled": <bool>}` and answers 200 `{"enabled"}`, the
  state read back after the write. Errors: 401, 403 `client-not-authorized`, 403
  `session-not-administrator`, 404, 503, 502 `client-service-failed` or `client-other`
  (each with `args: []`), 400 `boot-malformed` for a body that is not `{"enabled": <bool>}` (the fe
  never sends one), 413 `payload-too-large`.
- `GET /api/instances` and `GET /api/instances/{id}/boot` do not move a session's idle deadline:
  the fe reads the instances every 5 s while an instance page is open, and an open tab must not
  keep a session alive. The `POST`s and the `PUT` do move it.

### `GET /api/engines`

The available engines, for the preset editor's engine field. A preset write naming an engine
outside the list is refused with a key of its own (proposed: `unknown-engine`, `args: [name]`). A
preset's engine is the engine whose preset file holds it, never a key in the file (see
`GET /api/presets`).

### `GET /api/presets`

The one preset list of the Models page ([02-models.md](02-models.md), slice 2a), for any signed-in
user: `[{"id", "engine", "engineAvailable", "enabled", "model"}]`.

- `engine` is the id of the engine whose preset file holds the section. No key in the file names
  it: llama-server refuses a `--models-preset` file with a key it does not know (llama.cpp v1.0.0:
  `common/preset.cpp`, "option not recognized in preset"), so each engine keeps its own preset
  files (llama.cpp's `presets.ini` and `presets.ini.disabled`).
- `engineAvailable` says whether that engine is in `GET /api/engines`; true for every preset while
  llama.cpp is the only engine.
- `enabled` is true for a section of `presets.ini`, false for one of `presets.ini.disabled`.
- The order is the GUI's (`presets::load_listed`): the switched-on presets, then the switched-off
  ones, each in file order; an id in both files is listed once, as on; a missing file is an empty
  group. The fe does not sort.
- `model` (slice 2b, for the list's filter) is the value of the section's `model` key as the preset
  file holds it, a string the bff does not change (no path resolution, no check that the file
  exists, the server machine's separators kept); `""` when the section has no `model` key or it
  is empty. The fe treats a row without a string `model` as a malformed reply. Any signed-in user
  receives it, as the manager already lets any caller read the preset files.
- Errors, each with empty `args`: 401; 403 `client-not-authorized`; 503 `client-unavailable`; 502
  `client-other`.

### `PUT /api/presets/{id}/enabled`

Switches a preset on or off ([02-models.md](02-models.md), slice 2c), as `presets::set_enabled`
does: the section moves, unchanged, between `presets.ini` and `presets.ini.disabled` (the
destination written first; a missing disabled file created with the GUI's header). The id is
matched case-insensitively. Body `{"enabled": <bool>}`.

- 200 `{"enabled", "running"}`: `enabled` is read back after the write; `running` is true when an
  instance of the preset's engine is `active` or `activating` (the fe then says to restart it; the
  bff restarts nothing). A preset already where it was asked to go answers 200 and nothing is
  written.
- Switching on runs the checks the GUI runs: the load-on-startup presets counted against the lowest
  `ModelsMax` of the instances, and every preset check on the moved section.
- The bff runs one preset-file write at a time: a move is two writes, and two moves at once would
  lose one.
- Errors: 401; 403 `client-not-authorized`; 404 `no-preset-named` with `args: [id]` when neither
  file holds the id; 422 `load-on-startup-over-max` `[count, max]` or `load-on-startup-unlimited`
  `[count]`; 422 with the key of a preset check failing on a hand-edited section (the fe shows a
  key it does not know as `api-other`); 400 `presets-malformed` (proposed key) for a bad body; 503
  `client-unavailable`; 502 `client-other`.
- A preset id is unique across every engine (user, 2026-11-15), so the id alone names the preset.
  The ids `.` and `..` cannot travel in a URL path (URL Standard, dot path segments); the user
  decided (2026-11-15) that the common id check refuses them, for the GUI and the CLI too.

### `GET /api/presets/{id}` (proposed)

One preset, for its page ([02-models.md](02-models.md), slice 2d, section 2d.12 there with the
reasons). `{id}` is one percent-decoded path segment, matched case-insensitively; `presets.ini` is
searched first, so an id in both files is the enabled copy, as in the list. Needs a session; the
manager's `ReadFile` needs no right for the preset files.

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

- `id` as the file spells it; `engine`, `engineAvailable`, `enabled` as in `GET /api/presets`.
- `values`: every key of the section, keyed by the preset file's key name, each value the string
  the file holds after the INI reader's trim (no typing, no path resolution, no defaults); a key the
  section lacks is absent, never `null`.
- `projectUrl`: the section's `; url =` comment (`presets::project_url_of`), `""` when none.
- `revision`: opaque, for the saves of 2h; the fe ignores it until then. Proposed: the hash of the
  text of the file holding the section, as `server.rs` `revision` does.
- It grows without breaking: later slices read more keys from `values`, and add top-level fields
  (a comment such as `; placement =` as `placementNote`, server-computed data) or routes; no field
  is removed or retyped, and the fe ignores fields it does not know.
- Errors, each `{"key", "args"}`: 401 `session-required`; 404 `no-preset-named` with `args: [id]`
  when neither file holds the id; 403 `client-not-authorized`; 503 `client-unavailable`; 502
  `client-other` with empty `args`.

## Open on the bff side

- **An administrator's save needs no second password** (user, 2026-11-15): the bff vouches for the
  user it authenticated, and the manager grants `configure` to members of the `pai` group. No
  status asks for the password again.
- **Nothing creates `ModelsDir` after a save** (the GUI creates it as the logged-in user); the design
  recommends keeping it so, for the user to decide (`ARCHITECTURE.md`, "ModelsDir after a save").
- **`ModelsMax` accepts negatives today** and the render drops them silently; the fe shows whatever
  `limits` says.
- **A bind list mixing `0.0.0.0` with other addresses** is refused by the save's `server_cfg::check`
  (422 `bind-all-with-others`, field `Hostname`), once the PUT is served.
