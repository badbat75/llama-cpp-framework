# pai-bff and the web UI

`pai-bff` (package and binary `pai-bff`, over `pai-core`) is the backend-for-frontend: one local
HTTP service that serves the web UI built from `fe/` and the API it calls. Today it answers the
framework version, the manager's instances with their state and, for a signed-in user, an
instance's server file, and the manager checks its sign-in: the password and the group `pai`
(`PLAN.md` step 10). The web UI is for administrators only, members of the group `pai` (user,
2026-11-15): the Overview (version, instances) shows without sign-in, a right password of a `pai`
member signs in, a right password of anyone else is 403 `session-not-administrator`
(`fe/briefs/api-contract.md`). Behind sign-in it already has an instance page (Start, Stop, Restart,
start at boot and a Server page) and a Models page with an on/off switch per preset, whose routes
the bff does not serve yet ([below](#the-web-ui-fe)): on the shipped bff they fail.

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
connect to the manager. Then it asks the manager for `ReadFileAs("server@<id>.ini")` naming the
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
- **`revision`**: the 64-bit FNV-1a hash of the file's text in hex (written in `server.rs`, no
  dependency): the same for the same text across restarts and builds, another for any change, a
  comment included.

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
- **The bff does not serve yet** `PUT /api/instances/{id}/server` (`PLAN.md` step 10, slice 8),
  the start, stop, restart and boot routes (slice 9), nor `GET /api/presets` (slice 11) and `PUT /api/presets/{id}/enabled` (no bff slice in `PLAN.md` yet); `GET` of the server
  file is served ([above](#the-server-file-of-an-instance)). All are in `fe/briefs/api-contract.md`,
  and the fe is tested against them with mocked replies only, never against a real bff, so on the
  shipped bff a save, a run control and start at boot fail and the Models page shows "The presets
  could not be read.". The bff
  serves `/api/session` and the manager checks the sign-in, so the pages behind it open: on a
  shipped install an instance page's server read is answered for a signed-in `pai` member, the
  manager having checked them at that call ([above](#the-server-file-of-an-instance)), and its run
  controls and boot
  switch sit on the routes above, not served; by the code that is what a signed-in `pai` member
  sees, and no sign-in has been run against a real manager.
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
`GET /api/instances/{id}/server` the same way, over a fake manager; `tests/pai_bff_binary.rs` runs the binary with
`PRIVATE_AI_DATA_ROOT` on the child and an ephemeral port, SIGTERM and SIGINT included; `src/tests/deps_lint.rs` keeps the GUI
packages (Slint) out of its dependency tree. It is in the Windows GNU type check ("Commands" in
[AGENTS.md](../../AGENTS.md#commands)); nothing of it has run on Windows.
