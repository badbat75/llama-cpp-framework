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
  as one generic failure.
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

### `GET /api/instances`

Today's reply gains `engine` (the engine id, `llama.cpp` today) and its errors move from
`{"error": "unavailable"}` to the common shape: 503 `client-unavailable`, 502
`client-not-authorized`, 502 `client-other` for any other failure, each with empty `args` (the
manager's detail never reaches the body). Until `/api/session` exists it answers 404, which the fe
reads as signed out.

### The session

- `canConfigure` says whether the manager would allow `configure` for this user. It is a hint for
  showing the page read-only; the manager still decides every request.
- `POST /api/session` takes `{"username", "password"}`. Failures the fe tells apart: 401
  `session-bad-credentials`; 429 `session-too-many-attempts` with `args: [seconds]`. Both keys are
  proposed.

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

- A stop with `SaveStateOnShutdown` on saves the slot snapshots first, on the server (the GUI does
  this itself today, `slot_state::save_all`).
- `PUT /api/instances/{id}/boot` takes `{"enabled"}`.

### `GET /api/engines`

The available engines, for the preset editor's engine field. A preset write naming an engine
outside the list is refused with a key of its own (proposed: `unknown-engine`, `args: [name]`). A
preset with no engine is a llama.cpp preset and the key is not written back for it.

## Open on the bff side

- **Linux `configure` is `auth_admin_keep`**: how a signed-in administrator's save passes polkit
  without the bff keeping a password is not designed. If a save can ask for the password again, the
  fe needs a status and key for it.
- **Nothing creates `ModelsDir` after a save** (the GUI creates it as the logged-in user).
- **`ModelsMax` accepts negatives today** and the render drops them silently; the fe shows whatever
  `limits` says.
- **A bind list mixing `0.0.0.0` with other addresses** is checked nowhere today.
