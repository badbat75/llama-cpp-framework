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
- **403** `client-not-authorized` means the manager refused the call for this user, on a route that
  acts for the signed-in user; the session lives on. No served route sends it today (since bff slice
  9e every refusal of the user ends the session, below); the fe keeps handling it. On a route that needs no sign-in
  (`GET /api/instances`, `GET .../boot`) the same key comes with **502**: there the manager refused
  the bff itself, not a user.
- **403** `session-not-administrator` on any route but `POST /api/session` (served since bff slice 9e, 5f0d405, on `GET .../server`, the three verbs and `PUT .../boot`; on `PUT .../server` since eff112d; on `GET /api/presets`, `PUT /api/presets/{id}/enabled` and `GET /api/presets/{id}` since they were served, 257147e, 3a63244, 36a892d; S1 of
  [01-server.md](01-server.md)): the manager refused the session's user, who is no longer a member
  of `pai`, and the bff has **ended that session** at once (user, 2026-11-16: removed from `pai`, the
  session ends). The next request with the cookie answers 401; no `Set-Cookie` clears it. The bff
  sends this key exactly when it ends a session on a refusal, and `client-not-authorized` only when
  the session lives on, so the fe needs no second read of `GET /api/session` to tell them apart.
  Where: `crates/bff/src/server.rs` `read` and `save_failure`, `control.rs` `action_failure`,
  `presets.rs` `read_answer` and `switch_failure`. The session-free reads
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
- **Writes and their time limit** (the server save and the preset switch since 594e15e; the preset
  save will share the code): the manager's calls of a write get **15 s** (`lib.rs`
  `WRITE_TIMEOUT`); reads, the three verbs and the boot write keep **5 s** (`MANAGER_TIMEOUT`).
  503 `client-unavailable` only while no write was asked; **504 `client-timeout`** once one was
  sent and its outcome is unknown; a write that **landed** whose following read (the status, the
  switch's read-back) is slow or fails answers **200 with `running: true`**, never 504
  (`server.rs` `WriteState`, `TimedOut`).
- **404** `client-no-instance` means the instance id does not exist.
- The session is an HttpOnly cookie set by the bff; the fe never reads it.

## Routes

| Route | Who | Slice | Success |
| --- | --- | --- | --- |
| `GET /api/version` | anyone | exists | `{"version"}` |
| `GET /api/instances` | anyone | 1a, changed | `[{"id", "engine", "state"}]` |
| `GET /api/session` | anyone | 1a | 200 `{"user"}`, or 401 |
| `POST /api/session` | administrator (`pai` group) | 1a | 200 `{"user"}` and the cookie |
| `DELETE /api/session` | signed in | 1a, served | 204 |
| `GET /api/instances/{id}/server` | administrator | 1a, served | the server document below |
| `PUT /api/instances/{id}/server` | administrator (`configure`) | 1b, served (8) | `{"revision", "running"}` |
| `POST /api/instances/{id}/start`, `/stop`, `/restart` | administrator | 1c, served (9a) | 204 |
| `GET /api/instances/{id}/boot` | anyone | 1c, served (9a) | `{"enabled"}` |
| `PUT /api/instances/{id}/boot` | administrator (`configure`) | 1c, served (9a) | `{"enabled"}` |
| `GET /api/engines` | anyone | presets slice, proposed | `[{"id", "name"}]` |
| `GET /api/presets` | administrator | 2a, 2b, served (257147e) | `[{"id", "engine", "engineAvailable", "enabled", "model"}]` |
| `PUT /api/presets/{id}/enabled` | administrator (`configure`) | 2c, served (3a63244) | `{"enabled", "running"}` |
| `GET /api/presets/{id}` | administrator | 2d, served (36a892d) | `{"id", "engine", "engineAvailable", "enabled", "values", "projectUrl", "revision"}` |
| `PUT /api/presets/{id}` | administrator (`configure`) | 2h, accepted, not served yet | `{"revision", "running"}` |

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
- **`values`**: the 19 keys of the read, typed as the read gives them, `null` the key unset: numbers
  (whole, within `i32`) for `Port`, `Threads`, `ThreadsBatch`, `ModelsMax`, `ModelsMemoryMargin`,
  `LogVerbosity`; booleans for `CudaDisableGraphs`, `AttnRotDisable`, `RocprofAttach`, `Webui`,
  `WebuiMcpProxy`, `SaveStateOnShutdown`; `"on"`/`"off"` for `RocblasUseHipblaslt`; strings for
  `Hostname`, `ModelsDir`, `CudaAllreduce`, `StateDir`, `OpencodeBaseUrl`, `OpencodeApiKey`. A value of
  another type (a fraction, a numeric string, another word for the tri-state) is 400
  `server-malformed`. **A key absent from `values` is read as `null`: that key is unset in the file**,
  so the fe sends every key at every save (it does: `toValues` builds all 19). Unknown keys are
  ignored. For the three off-by-default switches `null` and `false` write the same.
- The bff runs `server_cfg::check` (the number ranges, `LogVerbosity` 0..5, threads 1 to the CPU
  count, `;`/`#` in `ModelsDir`, `StateDir`, `OpencodeBaseUrl`, `OpencodeApiKey` **whatever
  `SaveStateOnShutdown` says**, `0.0.0.0` alone in `Hostname`), normalizes the base URL (a trailing
  `/` and `/v1` stripped), then the manager runs its file checks and the address check again.
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
  belongs to: `args[0]` for `not-whole-number-in-range` (`[key, value, min, max]`) and
  `comment-markers` (`[key]`), `ModelsMax` for `load-on-startup-over-max` and
  `load-on-startup-unlimited`, `Hostname` for `bind-all-with-others` (`args: []`) and
  `client-unresolvable-host` (`[host]`), `Port` for `client-address-in-use` (`[id]`). For any other
  key of the manager's checks `field` is `args[0]`, or `""` when there are no args.
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
user: `[{"id", "engine", "engineAvailable", "enabled", "model"}]`. Served since 257147e
(`crates/bff/src/presets.rs` `list`, `rows`, `row`); checked against the fe's `Models.tsx` on
2026-11-17 (02-models.md, "Checked against the bff").

- **llama-server's global section `[*]` is not listed**: it holds the defaults every preset starts
  from, not a preset (`presets.rs` `is_global`). The switch and the read answer 404 for it.
- **A missing preset file is an empty group**: no file at all is 200 `[]`, never an error.

- `engine` is the id of the engine whose preset file holds the section. No key in the file names
  it: llama-server refuses a `--models-preset` file with a key it does not know (llama.cpp v1.0.0:
  `common/preset.cpp`, "option not recognized in preset"), so each engine keeps its own preset
  files (llama.cpp's `presets.ini` and `presets.ini.disabled`).
- `engineAvailable` says whether that engine is in `GET /api/engines`; the bff sends true for every
  row while llama.cpp is the only engine.
- `enabled` is true for a section of `presets.ini`, false for one of `presets.ini.disabled`.
- The order is the GUI's (`presets::load_listed`): the switched-on presets, then the switched-off
  ones, each in file order; an id in both files is listed once, as on; a missing file is an empty
  group. The fe does not sort.
- `model` (slice 2b, for the list's filter) is the value of the section's `model` key as the INI
  reader gives it (trimmed, cut at the first `;` or `#`, as for `GET /api/presets/{id}`'s
  `values`), a string the bff does not otherwise change (no path resolution, no check that the
  file exists, the server machine's separators kept); `""` when the section has no `model` key or
  it is empty. The fe treats a row without a string `model` as a malformed reply.
- Errors, each `{"key", "args": []}`: 401 `session-required`; 403 `session-not-administrator`
  (the session is ended, rules above; `client-not-authorized` is not sent); 503
  `client-unavailable` (no manager, or no answer within 5 s); 502 `client-other`.

### `PUT /api/presets/{id}/enabled`

Switches a preset on or off ([02-models.md](02-models.md), slice 2c), as `presets::set_enabled`
does: the section moves, unchanged, between `presets.ini` and `presets.ini.disabled` (the
destination written first; a missing disabled file created with the GUI's header). The id is
matched case-insensitively. Body `{"enabled": <bool>}`. Served since 3a63244, time-outs as now
since 594e15e (`crates/bff/src/presets.rs` `set_enabled`, `switch`, `switch_failure`, `field_of`,
`not_found`); checked against the fe's `Models.tsx` on 2026-11-17.

- 200 `{"enabled", "running"}`: `enabled` is read back after the writes (`presets.ini` read again);
  `running` is true when any instance is `active` or `activating` (the fe then says to restart it;
  the bff restarts nothing). A preset already where it was asked to go answers 200 and nothing is
  written. When both writes landed but the read-back or the states failed or outran the limit,
  the answer is still 200: `enabled` the value asked for, `running` true.
- Switching on runs the checks the GUI runs: the load-on-startup presets counted against the lowest
  `ModelsMax` of the instances, and every preset check on the moved section.
- The bff runs one preset-file write at a time: a move is two compare-and-swap writes (the
  destination first, then the source). An interruption between them leaves the preset in both
  files: it then reads enabled, and the next switch cleans up.
- 409 `preset-changed-elsewhere`, `args: []`: either compare-and-swap write found its file changed
  after the bff's read; when it was the second, the move is half done (the preset then in both
  files, read as enabled).
- 404 `no-preset-named` when neither file holds the id, **and for `*`** (the global section).
  `args: [id]` only when the id is one a preset can carry (`checks::preset_id`, not only dots);
  otherwise `args: []`. The fe fills the text with the id it sent, never with `args`.
- 422 `{"key", "args", "field"?}` from the manager's checks on switching on:
  `load-on-startup-over-max` `[count, max]` or `load-on-startup-unlimited` `[count]` (`field`
  `load-on-startup`), any other preset check failing on a hand-edited section (`field` its
  `args[0]`; the fe shows a key it does not know as `api-other`), and **no `field`** for
  `invalid-preset-id` and `preset-name-taken`. The page has no field to mark: the fe ignores it.
- 503 `client-unavailable` only while no write was asked; 504 `client-timeout` once one was sent
  and its outcome is unknown (the rule at the top, "Writes and their time limit").
- Other errors, each `{"key", "args": []}`: 401 `session-required` (checked first); 403
  `session-not-administrator` (the session is ended; `client-not-authorized` is not sent); 400
  `presets-malformed` for a body that is not `{"enabled": <bool>}` (the fe never sends one); 413
  `payload-too-large`; 502 `client-other`.
- A preset id is unique across every engine (user, 2026-11-15), so the id alone names the preset.
  The ids `.` and `..` cannot travel in a URL path (URL Standard, dot path segments); the user
  decided (2026-11-15) that the common id check refuses them, for the GUI and the CLI too. A file
  edited by hand can still hold one: the fe sends no request for it (02-models.md 59a, 2c-E).

### `GET /api/presets/{id}`

One preset, for its page ([02-models.md](02-models.md), slice 2d, section 2d.12 there with the
reasons). Served since 36a892d (`crates/bff/src/presets.rs` `read`, `document`,
`section_revision`); checked against the fe's `PresetPage.tsx` on 2026-11-17. `{id}` is one
percent-decoded path segment, matched case-insensitively; `presets.ini` is searched first, so an
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

- `id` as the file spells it; `engine`, `engineAvailable`, `enabled` as in `GET /api/presets`.
- `values`: every key of the section, keyed by the preset file's key name **in the file's own
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
  differs per file, another preset's edit does not change it, and switching the preset on or off,
  or editing a comment in it, does; the key is drawn at each bff start, so it does not survive a
  restart.
- It grows without breaking: later slices read more keys from `values`, and add top-level fields
  (a comment such as `; placement =` as `placementNote`, server-computed data) or routes; no field
  is removed or retyped, and the fe ignores fields it does not know.
- Errors, each `{"key", "args"}`: 401 `session-required` (the manager not asked); 404
  `no-preset-named` for an unknown id, for `*` (the global section) and for an id no preset can
  carry, `args: [id]` only for an id a preset can carry, else `[]` (as the switch); 403
  `session-not-administrator` (the session is ended; `client-not-authorized` is not sent); 503
  `client-unavailable` (no manager, or no answer within 5 s); 502 `client-other` with empty `args`.

### `PUT /api/presets/{id}` (accepted, not served yet)

Saves the edits of one preset's page ([02h-preset-save.md](02h-preset-save.md)). Designed in
`ARCHITECTURE.md`, "Saving a preset from the web". `{id}` as in the `GET`; the section is edited in
the file that holds it (`presets.ini` first, as the read), so a switched-off preset stays switched
off, as `presets::save` does.

Body `{"revision", "values"}`. **`values` holds only the keys the user changed**: a string sets the
key to that value, `null` removes the key. A key not in `values` is not touched.

- **Edited in place, not re-rendered.** Only the changed keys' lines are written; an edited line is
  rewritten whole (`key = value`), so an inline comment on that line is lost. Every other line of
  both files stays byte for byte: comments, keys the web UI does not show, their order and
  spelling, the line endings. A new key is appended to the section. **Duplicates of an edited key**
  (any letter case) are cleaned up, not refused: a set keeps one line, at the first one's place; a
  removal drops every line of the key. **Differs on purpose from the Server PUT and from the
  configurator's save**, which render the whole section from their struct
  (`presets::render_section`) and so drop comments and unknown keys and rewrite spellings (02h D-a;
  the user chose this, 2026-11-16).
- **Accepted keys**: a fixed list of **65**, the keys 2h.1 to 2h.9 enable: every key the
  configurator writes (`render_section`) minus ten; the list, card by card, is in `ARCHITECTURE.md`
  "Saving a preset from the web" (the constants `checks::WEB_PRESET_KEYS` and
  `PRESET_KEYS_NOT_ON_WEB`). **The ten answer 400 `presets-malformed`**: `model`, `mmproj`,
  `model-draft`, `spec-type` (Assets, until 2i), `n-cpu-moe`, `moe-cache-mib` (MoE, until 2g),
  `device`, `split-mode`, `tensor-split` (GPU distribution, until 2f), `override-tensor` (Tensor
  placement, until 2j). A slice that makes one editable moves it to the list in the same commit.
  **Body keys match in exact case** (`CTX-SIZE` is 400): the fe sends each key as 02e's tables
  spell it.
- **400 `presets-malformed`**: a body not of that shape, a key outside the list, a value that is
  neither a string nor `null`, an empty string (clearing a key is `null`), or a value holding a
  control character other than tab. **422 `comment-markers`**, `field` the key, for a value holding
  `;` or `#`. `{}` (no change): 200, nothing written, the revision still compared (409 if stale).
- Then the manager checks the edited section **whole** (`checks::presets_file` or
  `presets_disabled_file`): integer ranges, floats, enums, JSON object, `fit-target`,
  `override-tensor`, min <= max, `moe-cache-mib` with `split-mode tensor`, a `model`, and, in
  `presets.ini`, the load-on-startup count against `ModelsMax`. So a hand-edited bad value the user
  did not touch is refused too, on its own field.
- The write: one preset-file write at a time, by the manager's compare-and-swap write as the
  signed-in user (`WriteFileIfAs`, protocol 2), given the text the bff read.
- **200 `{"revision", "running"}`**: `revision` of the section as now written (what a `GET` now
  returns); `running` is true when the section is in `presets.ini` and any instance is `active` or
  `activating` (every instance serves the one preset file; it read the file when its llama-server
  started), and also when the status read after a done write failed; false for a switched-off
  preset. The fe then says "Saved: restart llama-server to apply." The bff restarts and reloads
  nothing (no `GET /models?reload=1` for now). Deliberate difference: the configurator's preset save
  says only "Saved preset [id]".
- 422 `{"key", "args", "field"}`: `field` is the INI key the refusal names: `args[0]` for
  `not-whole-number-in-range`, `comment-markers`, `not-a-number`, `not-one-of`, `not-json-object`,
  `min-above-max` (the minimum's key); `fit-target` for `fit-target-format`; `moe-cache-mib` for
  `moe-cache-with-tensor-split`; `load-on-startup` for `load-on-startup-over-max` and `-unlimited`;
  `model` for `preset-no-model`; `override-tensor` for `override-no-pattern` and
  `override-no-device`; **no field** for `invalid-preset-id` and `preset-name-taken` (the section is
  present twice in the file). Nothing is written.
- 409 `preset-changed-elsewhere`: the revision is stale (the section changed, it was switched on or
  off, or the bff restarted since the read), or the compare-and-swap write found the file changed
  after the bff's read. A save of **another** preset in the same file gives no 409. Nothing is
  written.
- Errors, each `{"key", "args"}`: 401 `session-required`; 403 `session-not-administrator`
  (session ended; the served routes send no `client-not-authorized`, the fe still handles it); 404
  `no-preset-named` (`args` as the read's); 413 `payload-too-large`; 503 `client-unavailable` (no
  write asked); 504 `client-timeout` (the write was sent and its outcome is unknown; a write that
  landed is the 200, `running` true, whatever follows); 502 `client-other`. The time limit is the
  writes' 15 s (rules at the top).

## Open on the bff side

- **An administrator's save needs no second password** (user, 2026-11-15): the bff vouches for the
  user it authenticated, and the manager grants `configure` to members of the `pai` group. No
  status asks for the password again.
- **Nothing creates `ModelsDir` after a save** (the GUI creates it as the logged-in user); the design
  recommends keeping it so, for the user to decide (`ARCHITECTURE.md`, "ModelsDir after a save").
- **A bind list mixing `0.0.0.0` with other addresses** is refused by the save's `server_cfg::check`
  (422 `bind-all-with-others`, field `Hostname`), served since eff112d.
- Closed 2026-11-17 (594e15e): a done write whose status read is slow or fails answers 200 with
  `running: true`, and writes get 15 s ("Writes and their time limit", above). A negative
  `ModelsMax` is refused by the served `PUT` (422 `not-whole-number-in-range`, field `ModelsMax`,
  `server_cfg::check`).
