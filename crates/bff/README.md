# pai-bff and the web UI

`pai-bff` (package and binary `pai-bff`, over `pai-core`) is the backend-for-frontend: one local
HTTP service that serves the web UI built from `fe/` and the API it calls. Today it answers the
framework version, the manager's instances with their state, the run verbs, the start-at-boot flag
and, for a signed-in user, an instance's server file read and saved, the preset list and the preset
on/off switch; the manager checks its sign-in: the password and the group `pai` (`PLAN.md` step 10).
The web UI is for administrators only, members of the group `pai` (user, 2026-11-15): the Overview
(version, instances) shows without sign-in, a right password of a `pai` member signs in, a right
password of anyone else is 403 `session-not-administrator` (`fe/briefs/api-contract.md`). Behind
sign-in it has an instance page (the server file, Start, Stop, Restart, start at boot: all served)
and a Models page (the preset list and the on/off switch per preset: served,
[below](#the-preset-list-and-its-switch)); the preset page and its save are not
([below](#the-web-ui-fe)).

**It runs as a service on Linux and Windows, never on macOS.** `xtask build` compiles it into
`build/<os>/cargo-target/release/`; the packaging is in
[docs/build-packaging.md](../../docs/build-packaging.md).

- Linux: the package `private-ai-framework-bff` (which requires `private-ai-framework-core`, not the
  engine) puts it in `/usr/libexec/private-ai/pai-bff` with `private-ai-bff.service`, enabled and
  started at install, stopped before an upgrade and started again after, stopped on removal. It runs as the static system user
  `private-ai-bff` (no home, no shell; created by the package's own sysusers.d entry), hardened with
  the sandbox `DynamicUser=` would imply written out: a `DynamicUser=` cannot work, because dbus-daemon
  drops a dynamic user's connection at `Hello` (on Ubuntu 26.04, dbus-daemon 1.16.2, `/api/instances`
  answered 503; sources in the unit's test in `xtask/src/linux/system_files.rs`). Checked on the real
  install of 2.99.1 on Ubuntu 26.04 (2026-11-14, by the user): the service active, `/api/version`
  answered, `/api/instances` `[]` with 200, and 503 `unavailable` with the manager stopped.
- Windows: the framework's MSI installs the service `private-ai-bff` (`pai-bff.exe service`, automatic start,
  `NT SERVICE\private-ai-bff`, restarted after a failure). Its `Error:` line goes nowhere there: by
  the code, a bad `bff.ini` or a bind failure shows only in the service manager's state and exit code.
  In CI (run 43651545466, a `windows-2025` runner) it ran as that service, its path and account as
  declared, `/api/version` answering the framework version and `/api/instances` 200 (since run
  43654217880 listing exactly `default`, engine `llama.cpp`, state `inactive`); nothing else has
  run on Windows, and the open checks are in the test plan of
  [installer/AGENTS.md](../../installer/AGENTS.md).

**It stops gracefully on SIGTERM (how systemd stops it) and Ctrl-C**: it stops accepting, lets the
open requests finish and exits 0. Under the Windows service manager the stop control does the same.

## Running it

**The address is `[Bff] Listen` in `bff.ini`, in the system config directory**
(`paths::system_config_dir`: `/etc/private-ai/bff.ini` on Linux and macOS,
`%ProgramData%\private-ai\config\bff.ini` on Windows, `<root>/system-config/bff.ini` under
`PRIVATE_AI_DATA_ROOT`). A missing file or key means `127.0.0.1:8590`.

```ini
[Bff]
Listen = 127.0.0.1:8590
```

- The value is an `ip:port` address: a host name (`localhost:8590`) is refused, nothing is resolved.
- Only a loopback address is accepted. LAN access waits for authentication (a later slice).

| Exit code | When |
| --- | --- |
| 0 | `pai-bff --version`, which prints `pai-bff <version>`; a stop by SIGTERM, Ctrl-C or the service manager |
| 2 | Any other argument, or anything after `service` (usage error) |
| 1 | A refused `Listen` value, an unreadable `bff.ini`, a bind failure or a serving error; `pai-bff service` outside the Windows service manager |

Every failure prints one `Error: ...` line on stderr naming the value, the file or the address. The
texts are the `bff-*` keys of `crates/core/strings/cli.toml`, found by the installed program in
`/usr/share/private-ai/strings/` on Linux (`paths::installed_share_candidates`). `pai-bff service`
hands the process to the Windows service dispatcher, which runs the same server. It serves on a current-thread tokio
runtime (axum).

| Route | Answer |
| --- | --- |
| `GET /api/version` | `{"version": "<framework version>"}` |
| `GET /api/instances` | `[{"id": "<id>", "engine": "llama.cpp", "state": "<state>"}]` from `private-ai-manager` ([below](#the-instances-from-the-manager)) |
| `POST /api/session` | Signs in: `{"user"}` and the session cookie ([below](#the-session)) |
| `GET /api/session` | `{"user"}` of the cookie's session |
| `DELETE /api/session` | Signs out: 204, the cookie cleared |
| `GET /api/instances/{id}/server` | The instance's `server@<id>.ini` with its defaults and limits, for a signed-in user ([below](#the-server-file-of-an-instance)) |
| `PUT /api/instances/{id}/server` | The form's save of that file, for a signed-in user ([below](#saving-the-server-file)) |
| `POST /api/instances/{id}/start`, `/stop`, `/restart` | The run verb as the signed-in user: 204 once the manager accepted the job, no body, the state is not in the reply (the page polls `/api/instances`); 401 with no session, 403 ending the session when the manager refused that user, 404 for no such instance, 503 with no manager, 502 `client-service-failed` when the service manager refused the job, 502 `client-other` for anything else (`crates/bff/src/control.rs`) |
| `GET /api/instances/{id}/boot` | `{"enabled"}`, for anyone: the connect carries no principal, the session store is never touched, so the page can poll it; the failures are `/api/instances`' |
| `PUT /api/instances/{id}/boot` | `{"enabled": <bool>}` in, `{"enabled"}` out, read back from the manager on the same connect, for a signed-in user: 400 `boot-malformed` for a body that is not that JSON, the other answers as the verbs |
| `GET /api/presets` | The preset rows for a signed-in user: `[{"id": "<preset id>", "engine": "llama.cpp", "engineAvailable": true, "enabled": <bool>, "model": "<model file>"}]` ([below](#the-preset-list-and-its-switch)) |
| `PUT /api/presets/{id}/enabled` | `{"enabled": <bool>}` in, `{"enabled", "running"}` out, for a signed-in user: the preset switched on or off ([below](#the-preset-list-and-its-switch)) |
| `GET /` | The embedded `index.html` |
| `GET /assets/{*path}` | The embedded file at `assets/<path>` |

Other methods on these routes are 405, any other path 404. Only a path in the embedded table
matches, so `..` reaches nothing else. Content types go by extension: HTML, JavaScript
(`text/javascript`), CSS, anything else `application/octet-stream`.

Checked by hand on Linux from the debug build (2026-11-14): `--version` exits 0, an unknown argument
2, `0.0.0.0:8590` and `localhost:8590` exit 1; on `127.0.0.1` it answered `/api/version`, `/` as
`text/html`, 405 to a `POST` and 404 elsewhere. Under `PRIVATE_AI_DATA_ROOT` `/api/instances`
answered 503 `{"error":"unavailable"}` (the error shape before `{"key", "args"}`) and a `POST` to it 405.

Checked by hand on Linux from the debug build with the web UI embedded (2026-11-15, bff at ecdf076,
fe at 9c2c326, under `PRIVATE_AI_DATA_ROOT`): `/` served the built `index.html`, its two
`/assets/` files 200 as `text/javascript; charset=utf-8` and `text/css; charset=utf-8`, byte for
byte `fe/dist`; `GET /api/session` 401 `session-required`; a JSON `POST /api/session` 503
`session-unavailable`, a non-JSON body (or no `application/json`) 400 `session-malformed`, an empty
password 401 `session-bad-credentials`, none with a `Set-Cookie`; `/api/instances` 503
`client-unavailable`. In headless Firefox the Overview showed the version and "The manager is not
running or does not answer.", a sign-in attempt the session-unavailable message with the password
cleared and focused, and `#/instances/default` the sign-in form with "Sign in to see this page.".
That bff predates de07ddf: the manager checks sign-in now, so the 503 of a `POST` and the sign-in
form of an instance page are what a page shows only where there is no manager to ask (a test
redirect, or a machine with the manager down).

## The instances from the manager

`GET /api/instances` asks `private-ai-manager` for `ListInstances`, then `Status` of each instance:
the two calls the manager answers without authorization. What the bff calls beyond them is
`Authenticate`, what sign-in makes ([below](#the-session)), and the `As` form of a guarded method
naming the session's user, what a signed-in route makes ([below](#the-server-file-of-an-instance))
(`crates/bff/src/lib.rs`).

| Status | Body | When |
| --- | --- | --- |
| 200 | `[{"id", "engine", "state"}]` in the manager's order, `[]` when there is none | Every call answered; an instance removed between the two calls is left out |
| 503 | `{"key": "client-unavailable", "args": []}` | No manager to reach (none running, macOS), or no full answer within 5 s |
| 502 | `{"key": "client-not-authorized", "args": []}` | The manager refused a call |
| 502 | `{"key": "client-other", "args": []}` | Any other failure |

- **The state is the manager's name** (`InstanceState::name()` in `pai-core`: `active`, `inactive`,
  `activating`, `deactivating`, `failed`, or an unknown name passed on as is). The engine is
  `llama.cpp`, the only engine today (`ENGINE_LLAMA_CPP` in `lib.rs`). The error body is only the
  strings key with empty args, never the manager's detail.
- **A new connection per request**, so a manager started after the bff is seen at the next request.
  The connect and the calls run on a blocking thread under one 5 s timeout that covers the connect.
- **Under `PRIVATE_AI_DATA_ROOT` it dials nothing** and answers 503 `client-unavailable`, so a test never
  reaches the real manager.
- **Reaching it**: on Linux the system bus (`org.privateai.Manager1`), whose policy lets every user
  send to the manager; neither call asks polkit, and with no session the bff's user `private-ai-bff` gets no
  `control` (`auth_admin`). On Windows the pipe `\\.\pipe\private-ai-manager`, whose SDDL gives
  `NT SERVICE\private-ai-bff` the interactive client's rights; its token is neither interactive, nor an
  administrator's, nor a member of `pai`, so the manager refuses it `control` and `configure`
  ([installer/AGENTS.md](../../installer/AGENTS.md)): the bff has no right of its own on either OS.
  A guarded call of a signed-in route goes as the `As` form of the method naming that user, which the
  manager answers for this account alone and for a user it finds a member of `pai` at that call
  ("Through the manager" in [crates/config/README.md](../config/README.md#through-the-manager-linux-windows)).
  The two calls of `/api/instances` passed the pipe in CI (run 43651545466); `Authenticate`,
  sign-in's call, and an `As` call have not crossed it, and the refusals are type-checked only.

## The session

`POST /api/session` takes `{"username": <text>, "password": <text>}` as `application/json` and asks
the authenticator (`session::Authenticator` in `crates/bff/src/session.rs`) on a blocking thread.
The shipped one, `SystemAuthenticator`, opens a client per check through the same `Connect` as the
bff's other routes and calls the manager's `Authenticate` (PAM service `private-ai` on Linux,
`LogonUserW` on Windows: "Through the manager" in
[crates/config/README.md](../config/README.md#through-the-manager-linux-windows)), which answers
only the bff's account: on Linux the uid of `private-ai-bff`, on Windows its service SID, so on a
shipped install the web service is the one that asks. The check has `SIGN_IN_TIMEOUT` (10 s) to
answer: a manager that cannot be reached, or does not answer by the bound, is 503
`session-unavailable` and records no lockout entry. A right password of a member of `pai` signs in,
a right password of a non-member is 403 `session-not-administrator`, the contract as built: it
confirms the password, so it is no credential failure and joins only the refusals' own lockout
counter. Under `PRIVATE_AI_DATA_ROOT` `connect` dials nothing, so a test never asks the real system.

| Status | Body | When |
| --- | --- | --- |
| 200 | `{"user"}`, the typed name: the authenticator's second answer (the group `pai`) gates the sign-in, it is not in the reply | Signed in; `Set-Cookie: pai_session=<id>; HttpOnly; SameSite=Strict; Path=/` |
| 400 | `{"key": "session-malformed", "args": []}` | Not JSON, another content type, `username` or `password` missing or not text, an empty `username` |
| 401 | `{"key": "session-bad-credentials", "args": []}` | No such account or the wrong password (not told apart); an empty password, refused without asking |
| 403 | `{"key": "session-not-administrator", "args": []}` | The password was right, the account is not a member of `pai`. The refusal confirms the password, so it never joins the bad-password counter, only the refusals' own 5 |
| 429 | `{"key": "session-too-many-attempts", "args": [<seconds left>]}` | 5 bad passwords, and separately 5 right-password refusals, for one (typed user in lowercase, client
address) each lock that pair for 60 s from their 5th within a 60 s window; the lock is read before
the manager is asked, so a locked pair never reaches it. A sign-in still being checked counts toward both, so guesses sent at once cannot overrun the limit |
| 503 | `{"key": "session-unavailable", "args": []}` | No manager to reach, no answer within `SIGN_IN_TIMEOUT`, or no random id could be drawn; none of these records anything, and the check thread of a manager that is slow keeps running after the answer |

- **The cookie** carries an opaque id: 32 bytes from the OS's random source (the `getrandom` crate,
  since std has no stable one) in hex. No `Secure` while the bff serves plain HTTP on loopback (a
  browser would not send it back over `http`), no `Domain`. The password is neither kept nor sent back.
- **`GET` and `DELETE /api/session`** answer 401 `{"key": "session-required", "args": []}` with no
  cookie, an unknown id or an expired one. `DELETE` ends the session (a replayed id is refused) and
  clears the cookie (`Max-Age=0`); the user's other sessions stay.
- **A session ends after 30 minutes without a request that uses it**; each use moves the deadline.
- **In memory only**: a restart of the bff signs everyone out. The lockout reads the client's
  address from the connection (`serve.rs` serves `into_make_service_with_connect_info`).
- The time comes from a clock handed to `router_with_session` (`Instant::now` in the service), so
  the tests move it; `SIGN_IN_TIMEOUT` reaches the same seam (`session::routes_with_sign_in_timeout`
  takes a bound of its own).

## The server file of an instance

`GET /api/instances/{id}/server` (`crates/bff/src/server.rs`) first needs a session: with no cookie,
or an unknown or expired one, it answers 401 `{"key": "session-required", "args": []}` and does not
connect to the manager. `PUT` of the same path saves it ([below](#saving-the-server-file)).
Then it asks the manager for `ReadFileAs("server@<id>.ini")` naming the
session's user, through the same connect and 5 s timeout as `/api/instances` (3de9283): the route
connects as `Sessions::current`, never a name taken from the request. The manager answers a
principal for the bff's account alone, checks that user's membership of `pai` at every call (its own
lookup, never polkit, never a cached answer) and never withholds a principal's read
("Through the manager" in
[crates/config/README.md](../config/README.md#through-the-manager-linux-windows)). So on a shipped
install the read is no longer refused for the bff's own account, which has no right of its own: the
right is the pair (the bff's account, a current `pai` member). The bff keeps no role per session and
withholds nothing on its own: every session is a member of the group `pai`, sign-in refuses anyone
else 403. The API key stays the manager's to keep: it withholds it from a reader allowed `control`
but not `configure`, and the bff nulls the key and marks it withheld exactly when the manager's read
says so, even when the text it sent still names a key (`server.rs` `document`); on this route the
read arrives unwithheld. **When the manager refuses that user**, typically one removed from `pai`
since signing in, the route answers 403 `session-not-administrator` (5f0d405: the sign-in's key for
the same fact, so the web UI tells an ended session from a live one without a second read) and the
bff ends the session
(`Sessions::end`, shared with sign-out): the same cookie is then 401 `session-required`, so the page
signs out at its next call. Only the manager's refusal ends a session: an unavailable or slow
manager, an unknown instance and every other failure keep it, being the bff's news and not a refusal
of the user. The manager records every guarded call, this one included: the action, the target, the
caller, the principal and the outcome, never a password and never the file's contents (the same
guide).

| Status | Body | When |
| --- | --- | --- |
| 200 | `{"revision", "values", "defaults", "limits", "interfaces"}`, and `"withheld"` when the manager's read reported a value withheld | The file was read. A principal's read is never withheld, so on this route `"withheld"` is absent and `OpencodeApiKey` carries the saved key |
| 401 | `{"key": "session-required", "args": []}` | No session |
| 403 | `{"key": "session-not-administrator", "args": []}` | The manager refused the read as this user (a member of `pai` at sign-in, removed since): the session ends, so the same cookie answers 401 from then on. A session-free read (the instances list, `GET boot`) answers the same refusal 502 `client-not-authorized` and ends nothing: it has no session |
| 404 | `{"key": "client-no-instance", "args": []}` | No such file, or the manager says no such instance or refuses the id (`NoInstance`, `BadFileName`, `InvalidId`) |
| 503 | `{"key": "client-unavailable", "args": []}` | No manager to reach, or no answer within 5 s |
| 502 | `{"key": "client-other", "args": []}` | Any other failure, never with the manager's detail: a manager built before the `As` methods, which refuses `ReadFileAs`, lands here |

- **`values`**: the 19 `[Server]` keys of `pai_core::server_cfg::ServerConfig` by INI name, read as
  the configurator reads the file (`server_cfg::from_text`): a number, `true`/`false` or a string,
  `null` when unset or unreadable (`Port = abc`). `CudaAllreduce` is normalized to lowercase;
  `CudaDisableGraphs`, `AttnRotDisable` and `RocprofAttach` are `false` when absent;
  `RocblasUseHipblaslt` is `"on"`, `"off"` or `null`. `OpencodeApiKey` is `null` exactly when the
  manager's read reported the key withheld, the key in the file's text included.
- **`withheld`**: `["OpencodeApiKey"]` exactly when the manager's read reported the key withheld, so
  the web UI can say a key is saved without showing it; absent otherwise, and on this route always
  absent: the manager never withholds a principal's read. The bff never withholds on its own: every
  session is a `pai` member, it keeps no role per session.
- **`defaults`**: `Port` `server_cfg::DEFAULT_PORT`, `ModelsMax` and `ModelsMemoryMargin`
  `server_cfg::DEFAULT_MODELS_MAX` and `DEFAULT_MODELS_MEMORY_MARGIN` (the configurator's form uses
  the same), `OpencodeBaseUrl` the URL derived from the saved `Hostname` and `Port` even when the file
  overrides it, `ModelsDir` and `StateDir` the instance's (`paths::instance_models_dir`,
  `paths::instance_state_dir`), never the bff account's.
- **`limits`**: `[min, max]` of `Port`, `ModelsMax` and `ModelsMemoryMargin` as the manager checks
  them (`checks::server_int_range`), and of `Threads` and `ThreadsBatch` 1 to this machine's logical
  CPU count.
- **`interfaces`**: `net_ifaces::interfaces()` as `[{"value", "label"}]`.
- **`revision`**: a keyed SipHash-2-4 of the file's text, printed as 16 hex digits (`server.rs`):
   the key is a 16-byte secret drawn from the OS's random source at each bff start (`lib.rs`
   `try_router`). The same text through one bff run gives the same revision, any other text (a
   comment, the API key, included) gives another; a change detector, not an oracle: with a revision
   and not the file nobody can check a guess about its text. A bff start draws a new secret, so
   every open form is stale after it and reloads once.

### Saving the server file

`PUT /api/instances/{id}/server` (`server.rs` `save`) is the Server form's save, as the signed-in
user. The body is `{"revision", "values"}`: the revision the GET answered and the 19 typed values.
The body becomes the file's text through the one fields table the GET reads with, and passes
`pai_core::server_cfg::check` with this machine's CPU count, all before any manager is connected
to: a body that is not that JSON, or a value of the wrong type, is 400 `server-malformed`; a check
refusal is 422 `{"key", "args", "field"}`, `field` the INI key the form marks. A body over
`origin::MAX_BODY` (64 KiB) is refused by the extractor layer in front of the whole router:
413 `payload-too-large`, before the session is read.

The file is then read as the session's user, its text hashed with the run's secret and compared
with the sent revision, and written through the manager's `WriteFileIfAs`, which accepts only while
the file still holds exactly the text read: a write that lands between the read and the write is
refused by the manager, answered with the same 409, and nothing is written. The read, the write and
the status that answers `running` are three calls of one connect. The reply is 200
`{"revision", "running"}`: the revision of the text the file holds now, and whether the instance's
process runs (`active` or `activating`); a status read that fails after a write that succeeded
answers `running: true`. The 503 and the 504 turn on one fact: while the write was never asked a
time limit that runs out or a stream that dies is 503 `client-unavailable` and the file is what it
was; from the moment the write was sent its outcome is unknown and the answer is 504
`client-timeout`. A time limit does not stop the blocking task: the handler and that task share one
atomic (Idle, Asked, Cancelled in `server.rs`) and settle that fact with a compare and exchange, so
a task parked in the connect or the read finds the state Cancelled and never writes, and a handler
that finds the state Asked answers 504.

| Status | Body | When |
| --- | --- | --- |
| 200 | `{"revision", "running"}` | The file was written; `running` is whether the instance's process runs |
| 400 | `{"key": "server-malformed", "args": []}` | Not JSON, another content type, `revision` missing or not text, a value of the wrong type; the manager is not connected to |
| 401 | `{"key": "session-required", "args": []}` | No session; the manager is not connected to |
| 403 | `{"key": "session-not-administrator", "args": []}` | The manager refused the connect, the read or the write as this user: the session ends, as the read's 403 (a status it refuses after the write is not a failure: 200, `running: true`) |
| 404 | `{"key": "client-no-instance", "args": []}` | No such file, or the manager says no such instance or refuses the id |
| 409 | `{"key": "server-changed-elsewhere", "args": []}` | The file is not the one the form read: the sent revision does not match the read text, or the manager's compare and swap found it changed. Nothing is written, the session stays signed in |
| 413 | `{"key": "payload-too-large", "args": []}` | A body over 64 KiB, by the extractor, before the session is read |
| 422 | `{"key", "args", "field"}` | A value the checks refused: the bff's own (`server_cfg::check`, this machine's CPU count included), the manager's content check (`Invalid`), `client-address-in-use` (`field: "Port"`) or `client-unresolvable-host` (`field: "Hostname"`); the manager's detail stays out of the body |
| 503 | `{"key": "client-unavailable", "args": []}` | No manager reached and the write was never asked: the file is what it was |
| 504 | `{"key": "client-timeout", "args": []}` | The write was asked and its outcome is unknown: a time limit that ran out, or a manager lost at the write |
| 502 | `{"key": "client-other", "args": []}` | Any other failure. By the code a manager built before `WriteFileIf` answers that call with a refused frame that ends the connection, so it lands here: type-checked only, never run |

## The preset list and its switch

`GET /api/presets` and `PUT /api/presets/{id}/enabled` are the Models page's two routes
(`crates/bff/src/presets.rs`, built 2026-11-17; `PLAN.md` step 10 items 11 and 11b, the design
"The switch on and off (item 11b)" in `ARCHITECTURE.md`). Both are behind sign-in and both ask
`private-ai-manager` on **one connect as the signed-in user**, so it is that user's own membership
of `pai`, re-checked by the manager at the call, that answers: 401 `session-required` with no
session (the manager is not connected to, nothing is written), 403 `session-not-administrator` and
the end of the session when the manager refuses that user.

What they read and write is the manager's pair of preset files, `presets.ini` and
`presets.ini.disabled` in the engine's config directory (`paths::engine_config_dir`:
`/etc/llama.cpp/` on Linux, `%ProgramData%\llama.cpp\config\` on Windows): the same files the
configurator's Models tab and `pai preset` read and write, and the one the engine reads the enabled
sections of. The disabled file is never read by the engine. The two names are constants in the code:
the `{id}` of the URL is a section name, matched in any letter case, never a path.

### The list

One read of each file, and one row per section, `[{"id", "engine", "engineAvailable", "enabled",
"model"}]`:

- `id` as the file spells it; `engine` the engine whose preset file holds the section (`llama.cpp`,
  the only engine today) and `engineAvailable` true for every row, there being no engines route to
  check it against.
- `model` is the section's `model` key as the file holds it, no path resolution and no existence
  check, `""` when the section has no such key: every row carries a string.
- The order is the GUI's: the switched-on sections, then the switched-off ones, each group in file
  order; an id in both files is listed once, as on. The web UI does not sort.
- A missing file is an empty group, not an error: no `presets.ini` answers `[]`.

| Status | Body | When |
| --- | --- | --- |
| 200 | the rows, `[]` when there is no preset | the read answered |
| 401 | `session-required` | no session, the manager not asked |
| 403 | `session-not-administrator` | the manager refused the call for that user; the session ends |
| 503 | `client-unavailable` | no manager, or one that outlives the bff's deadline |
| 502 | `client-other` | anything else, never the manager's detail |

### The switch

The body is `{"enabled": true|false}`. Both files are read, the section is looked for in each, and
if it is in neither the answer is 404. Otherwise it moves, unchanged:

- **The file the preset lands in is written first**, because a preset in both files reads as
  enabled: switching off writes `presets.ini.disabled` and then `presets.ini`, switching on writes
  `presets.ini` and then the disabled file.
- Each write is a compare and swap (`write_file_if`) expecting the text read in this very request,
  so a write by the configurator, the CLI or a second page in between is refused and nothing moves.
- A `presets.ini.disabled` that does not exist cannot be swapped (the swap refuses a missing file):
  it is created by a plain `write_file`, carrying the header the configurator writes for that file
  (`DISABLED_FILE_HEADER` in `crates/app/src/presets.rs`, kept equal by the test
  `the_disabled_header_is_the_configurators_one`). A new `presets.ini` needs no header.
- An id already where it is asked for answers 200 and writes nothing.
- The answer is not what the bff computed: after the writes, `presets.ini` is read again and
  `enabled` is whether the section is in it.
- `running` is whether any instance is `active` or `activating` (asked in the list's order, stopping
  at the first that is, and true when a state cannot be read: the safe answer after a move that
  stands). It is the page's "restart to apply" note: the switch starts, stops and restarts nothing.

The moved text goes through the manager's content checks, the configurator's own
(`crates/core/src/checks.rs`, run by `check_contents` in `crates/manager/src/manager/service.rs`):
`presets_file` for `presets.ini`, `presets_disabled_file` for the disabled one, then
`preset_ids_free`, which keeps a section that is new to a file from taking an id the other file
holds, so ids stay unique across the pair. A refusal is 422 with the check's key, its values and
`field`, the preset key for the form to mark: `load-on-startup` for both
`load-on-startup-*` keys, the key named in the check's args for the others, and no `field` for a
refusal about the preset's name (`invalid-preset-id`, `preset-name-taken`). The destination is
written first, so a refusal leaves both files as they were.

| Status | Body | When |
| --- | --- | --- |
| 200 | `{"enabled", "running"}` | the move stands, or there was nothing to do |
| 400 | `presets-malformed` | a body that is not `{"enabled": <bool>}`, or not JSON, the manager not asked |
| 401 | `session-required` | no session, the manager not connected to |
| 403 | `session-not-administrator` | the manager refused the read or the write for that user; the session ends |
| 404 | `no-preset-named` | neither file has that section. `args: [id]` when the id is a name a preset can carry at all; `args: []` for one the id check refuses (a newline, a bracket, a path) and for a run of dots |
| 409 | `preset-changed-elsewhere` | a compare and swap found its file changed, nothing moved. When it answers the second write the preset is in both files: it reads on, and the next switch of that preset mends it |
| 413 | `payload-too-large` | a body over the bff's limit |
| 422 | `{"key", "args", "field"}` | the manager refused the moved section, nothing written |
| 503 | `client-unavailable` | no manager, with no write asked yet |
| 504 | `client-timeout` | from the moment a write was asked and its outcome is unknown, the rule the server save shares ([above](#saving-the-server-file)): the handler and its task settle the one atomic state together, so a limit that runs out before any write is 503 and the files are what they were |
| 502 | `client-other` | anything else, never the manager's detail |

### A preset in both files

An id in both files is listed once, as on. Switching it off writes the disabled file (its section is
already there, unchanged, which that file's check accepts) and takes it out of `presets.ini`: the
state is mended. Switching it back on adds to `presets.ini` a section whose `*` id the preset-id
check refuses: 422 `invalid-preset-id`, nothing written. Read from the code on 2026-11-17, not
tested: `PLAN.md` 11b lists the missing tests, and the plain write that creates a new disabled file
(racing a file that appeared in between) is its other leftover.

### What has run

The bff's own tests only: `tests/presets.rs` and `tests/presets_switch.rs`, whose fake manager
answers a compare and swap only when the expected text is what it answered for that file in that
test, and records every write, so the order of the two writes and the text of each are pinned.
Green 2026-11-17 (22 and 45 tests, `cargo test -p pai-bff`). **Nothing here has been called against
a real manager**: no signed-in route has (the real install of 2.99.1 on Ubuntu 26.04 answered
`/api/version` and `/api/instances`, [above](#running-it)), and on Windows the code is type-checked
for the GNU target, never run.

## The web UI (`fe/`)

React + Vite in TypeScript, exact versions in `fe/package.json` and a committed
`package-lock.json`, styled with plain CSS (`fe/src/index.css`). Its design guide is `fe/DESIGN.md`,
kept by the `designer`; the slice briefs and the API contract the fe is built against
(`api-contract.md`) are in `fe/briefs/`.

**A shell with hash routes** (`fe/src/App.tsx`, `fe/src/route.ts`), since the bff serves only `/`
and `/assets/`: a skip link, a banner (Sign in, or the user and Sign out, no role word: fe slice R4,
1827023), a nav (behind a Menu button below 960px; the instances listed once signed in) and the
route's page, whose heading takes the focus and names the document title on a change. Each page reads
its data once when it opens and never shows a reply's text.

| Route | Page | Who |
| --- | --- | --- |
| `#/` | The Overview: the version (`/api/version`, a fixed message on any failure) and the instances (`/api/instances`) as a table of instance, each a link to its page, and state (the five known states as Running, Stopped, Starting, Stopping, Failed; another as `Unknown (<name>)`), with an engine column only when they run on more than one engine; a message when there is none | anyone |
| `#/sign-in` | The sign-in form (`POST /api/session`) | anyone |
| `#/instances/<id>` | The instance's id, engine and state (from `/api/instances`, kept live), Start, Stop and Restart (`POST /api/instances/<id>/start`, `/stop`, `/restart`), Start at boot (`GET` and, for an administrator, `PUT /api/instances/<id>/boot`), then its server file (`GET /api/instances/<id>/server`) as the whole Server form of `pai-config`: editable, every signed-in session being a `pai` member (the administrator/user split is gone, fe slice R4, 1827023), with the GUI's field dependencies, a save bar (Save and Discard, a status line, an alert for a failure) and a save that sends `PUT /api/instances/<id>/server` (`fe/src/ServerForm.tsx`) | signed in; otherwise the sign-in form in its place |
| `#/models` | The presets of every engine (`GET /api/presets`) as a table of preset, engine (with "(not available)" when its engine is not) and status On or Off, each row with a switch named by its preset that sends `PUT /api/presets/<id>/enabled` at once, one write at a time; the status line says what was switched and, when an instance of the engine runs, to restart it; a failure shows its message above the table and the switch takes the state read again (`fe/src/Models.tsx`) | signed in; otherwise the sign-in form in its place |
| `#/models/<id>` | One preset, read-only: the way back to the list, the id as the heading, the Engine field, the Assets card as text and the settings cards with every control disabled, read once from `GET /api/presets/<id>` on a page that has nothing to save, reached from the row's id link in the Models table (`fe/src/PresetPage.tsx`) | signed in; otherwise the sign-in form in its place |
| anything else | Not found | anyone |

- **The session** (`fe/src/session.ts`) is read once at start from `GET /api/session`: only a 200
  `{"user"}` signs the page in (the reply carries nothing else: 296165f; a reply that still names a
  role field signs in the same way, the field ignored: 1827023), any other reply or failure leaves it
  signed out.
  Sign out is `DELETE /api/session`, signed out whatever it answers; a 401 from the server read
  signs the page out too, and a 403 from it ends the session at the bff, so the page's next call
  answers 401 ([above](#the-server-file-of-an-instance)).
- **A save** sends the file's revision and the typed values. Before it the form runs the client
  checks and marks the failing fields; the reply is read by its key: a refusal under the field it
  names, a stale revision (`server-changed-elsewhere`) with a Reload button, a 401 with a sign-in
  dialog that keeps the edits for the same administrator. Leaving the page with unsaved edits asks
  first: a link or Sign out in a dialog (`fe/src/Dialog.tsx`), closing or reloading the tab in the
  browser's own prompt; Back, Forward and a typed address do not ask yet.
- **The bff does not serve** `GET /api/presets/{id}` and `PUT /api/presets/{id}` (a preset's page and
  its save, `PLAN.md` items 11c and 11e): on a shipped bff the preset page's read gets a 404 with no
  key, so `#/models/<id>` shows "The preset could not be read." (`preset-load-failed`,
  `fe/src/PresetPage.tsx`), the page tested against `fe/briefs/api-contract.md` with mocked replies
  only. The Models page's two calls are served ([above](#the-preset-list-and-its-switch)); the fe is
  tested against that contract with mocked replies only, never against a real bff, so what those
  pages show takes a run on a machine. Everything the instance page makes is served: the
  server read and its save ([above](#the-server-file-of-an-instance)), the run verbs and
  the boot read and write ([above](#running-it)); the bff serves `/api/session` and the manager
  checks the sign-in, so the pages behind it open: on a shipped install an instance page's server
  read is answered for a signed-in `pai` member, the manager having checked them at that call
  ([above](#the-server-file-of-an-instance)), and by the code its save, its run controls and its
  boot switch are answered the same way for that member; no sign-in has been run against a real
  manager.
- **Errors are `{"key", "args"}`** (`apiError` in `fe/src/api.ts`, the bff's shape since
  `GET /api/instances` moved to it); the message is picked by the key, never by the status, apart
  from 401. For the instances: `client-unavailable` (the manager is not running or does not
  answer), `client-not-authorized` (it refused the web service), anything else, a malformed reply
  or no answer included, as "could not be read". The server read adds `client-no-instance` (with a
  link back to the Overview); the sign-in tells `session-bad-credentials`,
  `session-not-administrator` (the password right, the account not in `pai`) and
  `session-too-many-attempts` (its seconds from `args`) from a generic failure.
- **Text goes in `fe/src/strings/en_US.json`**, linked by key through `t(key, ...args)`
  (`fe/src/strings.ts`), with `pai-core`'s rules: `{N}` placeholders filled by position, a missing
  key shown as itself.
- **Tests**: Vitest and Testing Library in jsdom, `npm --prefix fe test` from the repo root
  (`npm --prefix fe ci` first when `fe/node_modules` is missing).
- **Checks**, none of which writes a file: `npm --prefix fe run typecheck` (`tsc` over the app,
  `vite.config.ts` and, through `fe/tsconfig.test.json`, the tests, which `npm run build` does not
  type-check), `npm --prefix fe run lint` (ESLint, `fe/eslint.config.js`, no warning allowed) and
  `npm --prefix fe run format:check` (Prettier, `fe/.prettierrc.json`; `fe/.prettierignore` leaves
  out `fe/dist` and the designer's Markdown, `DESIGN.md` and `briefs/`). TypeScript stays on 6.0:
  typescript-eslint does not load 7 yet ("Later, not planned yet" in [PLAN.md](../../PLAN.md)).
- **Built by `xtask build`**, embedded in the binary by `crates/bff/build.rs`: which directory it
  embeds and when it notices a new `fe/dist` are in
  ["The web UI and pai-bff"](../../docs/build-compiling.md#the-web-ui-and-pai-bff).

## Tests

`cargo test -p pai-bff` needs no node: without `fe/dist` it embeds the fixture
`crates/bff/fe-fixture/`. `tests/fe.rs` checks that the routes serve, byte for byte, the directory
the build embedded (`PAI_BFF_FE_DIR`); `tests/instances.rs` drives `/api/instances` against a fake
manager (`router_with`, a short timeout); `tests/session.rs` drives `/api/session` through
`router_with_session` with a fake authenticator and a clock the test moves;
`tests/authenticator.rs` drives `SystemAuthenticator` over a fake manager client, the same `Connect`
the bff's other routes use (never PAM, `LogonUserW`, the bus or the pipe), and pins how the
manager's two answers become `Identity` or `AuthError`; `tests/server.rs` drives
`GET` and `PUT /api/instances/{id}/server` the same way, over a fake manager; `tests/presets.rs` and
`tests/presets_switch.rs` drive the preset list and the switch over the same fake manager, whose
compare and swap lands only when the `expected` text is what the fake answered for that file in that
test (a missing file answers `Changed`, as the real manager does) and every write is recorded as
`(name, contents, expected)`, so the order of the two writes, the whole text of each and the channel
each used are pinned; `tests/pai_bff_binary.rs` runs the binary with
`PRIVATE_AI_DATA_ROOT` on the child and an ephemeral port, SIGTERM and SIGINT included; `src/tests/deps_lint.rs` keeps the GUI
packages (Slint) out of its dependency tree. It is in the Windows GNU type check ("Commands" in
[AGENTS.md](../../AGENTS.md#commands)); nothing of it has run on Windows.
