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
  acts for the signed-in user. On a route that needs no sign-in (`GET /api/instances`) the same key
  comes with **502**: there the manager refused the bff itself, not a user.
- **The fe picks its message by the key, never by the status**, apart from 401 (sign in).
- **503** `client-unavailable` means no manager to reach or no answer in time.
- **404** `client-no-instance` means the instance id does not exist.
- The session is an HttpOnly cookie set by the bff; the fe never reads it.

## Routes

| Route | Who | Slice | Success |
| --- | --- | --- | --- |
| `GET /api/version` | anyone | exists | `{"version"}` |
| `GET /api/instances` | anyone | 1a, changed | `[{"id", "engine", "state"}]` |
| `GET /api/session` | anyone | 1a, proposed | 200 `{"user", "canConfigure"}`, or 401 |
| `POST /api/session` | anyone | 1a, proposed | 200 `{"user", "canConfigure"}` and the cookie |
| `DELETE /api/session` | signed in | 1a, proposed | 204 |
| `GET /api/instances/{id}/server` | signed in (`control`) | 1a, proposed | the server document below |
| `PUT /api/instances/{id}/server` | administrator (`configure`) | 1b, proposed | `{"revision", "running"}` |
| `POST /api/instances/{id}/start`, `/stop`, `/restart` | signed in (`control`) | 1c, proposed | 204 |
| `GET /api/instances/{id}/boot` | anyone | 1c, proposed | `{"enabled"}` |
| `PUT /api/instances/{id}/boot` | administrator (`configure`) | 1c, proposed | `{"enabled"}` |
| `GET /api/engines` | anyone | presets slice, proposed | `[{"id", "name"}]` |
| `GET /api/presets` | signed in | 2a, 2b, proposed | `[{"id", "engine", "engineAvailable", "enabled", "model"}]` |
| `PUT /api/presets/{id}/enabled` | administrator (`configure`) | 2c, proposed | `{"enabled", "running"}` |

### `GET /api/instances`

Today's reply gains `engine` (the engine id, `llama.cpp` today) and its errors move from
`{"error": "unavailable"}` to the common shape: 503 `client-unavailable`, 502
`client-not-authorized`, 502 `client-other` for any other failure, each with empty `args` (the
manager's detail never reaches the body).

### The session

- `GET /api/session` answers 404 until the bff has the route; the fe reads any reply but 200 as
  signed out.
- `canConfigure` says whether the manager would allow `configure` for this user. It is a hint for
  showing the page read-only; the manager still decides every request.
- `POST /api/session` takes `{"username", "password"}`. Failures:
  - 401 `session-bad-credentials`: the same reply for an unknown user and a wrong password.
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
- `revision` is opaque; the fe sends it back on a save.
- **`OpencodeApiKey` goes to administrators only** (user, 2026-11-15). For a session whose
  `canConfigure` is false, `values.OpencodeApiKey` is always `null`, and the document gains
  `"withheld": ["OpencodeApiKey"]` when the file has a key; `withheld` is absent or `[]` when the
  file has none. The read-only page shows that a key is set, never its value. An administrator
  gets the value as before and no `withheld`. The bff decides from the session's `canConfigure`.

### `PUT /api/instances/{id}/server`

Body `{"revision", "values"}`.

- The bff (or the manager behind it) runs every check again, including the ones that today exist
  only in the GUI layer: the number ranges, the log level kept in 0..5, a trailing `/` and `/v1`
  stripped from the base URL, threads at most the CPU count.
- A blank `ModelsDir` or `StateDir` is left unset in the file, on every OS.
- 200 `{"revision", "running"}`: `running` chooses between "Saved." and "Saved: restart
  llama-server to apply."
- 422 `{"key", "args", "field"?}`: a refused value; `field` is the INI key it belongs to, absent for
  a form-level refusal. Keys the fe knows: `not-whole-number-in-range`, `comment-markers`,
  `load-on-startup-over-max`, `load-on-startup-unlimited`, `client-address-in-use`,
  `client-unresolvable-host`.
- 409 `server-changed-elsewhere` (proposed key): the revision is stale, the file changed since it
  was read.

### Start, stop, restart, boot

- `POST /api/instances/{id}/start`, `/stop` and `/restart` take no body and answer 204 once the
  service manager has accepted the job; the state read just after can still be the previous one, so
  the fe polls `GET /api/instances` (any 2xx counts as accepted). Errors: 401; 403
  `client-not-authorized`; 404 `client-no-instance`; 503 `client-unavailable`; 502
  `client-service-failed` (proposed key) with empty `args` when the service manager refused or the
  unit failed (its message stays in the bff's log); 502 `client-other` for anything else.
- A stop or a restart with `SaveStateOnShutdown` on saves the slot snapshots first, on the server
  (the GUI does this itself today, `slot_state::save_all`).
- `GET /api/instances/{id}/boot` answers 200 `{"enabled": <bool>}`. Errors: 503
  `client-unavailable`, 502 `client-not-authorized` (a route that needs no sign-in, like
  `GET /api/instances`), 404 `client-no-instance`, 502 `client-other`.
- `PUT /api/instances/{id}/boot` takes `{"enabled": <bool>}` and answers 200 `{"enabled"}`, the
  state read back after the write. Errors: 401, 403 `client-not-authorized`, 404, 503, 502
  `client-service-failed` or `client-other`.
- `GET /api/instances` and `GET /api/instances/{id}/boot` do not move a session's idle deadline:
  the fe reads the instances every 5 s while an instance page is open, and an open tab must not
  keep a session alive.

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

## Open on the bff side

- **An administrator's save needs no second password** (user, 2026-11-15): the bff vouches for the
  user it authenticated, and the manager grants `configure` to members of `sudo` or `wheel`
  (Administrators on Windows), `control` to any signed-in user. No status asks for the password
  again.
- **Nothing creates `ModelsDir` after a save** (the GUI creates it as the logged-in user).
- **`ModelsMax` accepts negatives today** and the render drops them silently; the fe shows whatever
  `limits` says.
- **A bind list mixing `0.0.0.0` with other addresses** is checked nowhere today.
