# pai-bff and the web UI

`pai-bff` (package and binary `pai-bff`, over `pai-core`) is the backend-for-frontend: one
HTTP service, plain HTTP on the address the administrator chose (`[Bff] Listen`), serving the web UI
built from `fe/` and the API it calls. Today every API route
beside the sign-in is for a signed-in user: the framework version, this host's name with the engine and
runtime versions its packages installed, the node's compute devices ([below](#the-node)), the manager's
instances with their state, the run verbs, the start-at-boot flag, an instance's tags, an instance's server file read and saved, the model profile
list, the model profile on/off switch and one model profile, read and saved, the server profile
pool: its list, a new profile, one profile read and saved, and its delete
([below](#the-server-profile-pool)), and an instance's associations: its server profile switched, and
its model profiles listed and assigned, one at a time or as the whole pool
([below](#the-association-routes)), and an instance created and its record removed
([below](#creating-and-removing-an-instance)); the manager checks its sign-in: the password and the
group `pai` (`PLAN.md` step 10).
Only the web UI's page at `/`, its files under `/assets/` and the three session routes answer without
a session: any other API route is 401 `session-required`, asking the manager nothing (user,
2026-11-19: a visitor who has not signed in sees only the sign-in page). The web UI is for
administrators only, members of the group `pai` (user, 2026-11-15): a right password of a `pai`
member signs in, a right password of anyone else is 403 `session-not-administrator`
(`fe/briefs/api-contract.md`). Behind
sign-in it has the Overview (version, instances), the About page (this host's name, the framework's version and each
installed engine's version and backends, [below](#the-node)), an instance page (the server file, Start, Stop, Restart, start at boot: all served)
and the Model profiles page (the model profile list, the on/off switch per model profile and one model profile's own
page, read and saved: all served, [below](#the-model-profile-routes)); on that page four cards are editable and the
rest read-only, the fe's save being the open `PLAN.md` item 2h ([below](#the-web-ui-fe)).

**It runs as a service on Linux and Windows, never on macOS.** `xtask build` compiles it into
`build/<os>/cargo-target/release/`; the packaging is in
[docs/build-packaging.md](../../docs/build-packaging.md).

- Linux: the package `private-ai-framework-bff` (which requires `private-ai-framework-core`, not the
  engine) puts it in `/usr/libexec/private-ai/pai-bff` with `private-ai-bff.service`, enabled and
  started at install, stopped before an upgrade and started again after, stopped on removal. It runs as the static system user
  `private-ai-bff` (no home, no shell; created by the package's own sysusers.d entry), hardened with
  the sandbox `DynamicUser=` would imply written out: a `DynamicUser=` cannot work, because dbus-daemon
  drops a dynamic user's connection at `Hello` (on Ubuntu 26.04, dbus-daemon 1.16.2, `/api/instances`
  answered 503, that read being open to a request with no session then; sources in the unit's test in
  `xtask/src/linux/system_files.rs`). Checked on the real
  install of 2.99.1 on Ubuntu 26.04 (2026-11-14, by the user, a build from before the session gate of
  2026-11-19, which answered a request with no session): the service active, `/api/version`
  answered, `/api/instances` `[]` with 200, and 503 `unavailable` with the manager stopped.
- Windows: the framework's MSI installs the service `private-ai-bff` (`pai-bff.exe service`, automatic start,
  `NT SERVICE\private-ai-bff`, restarted after a failure). Its `Error:` line goes nowhere there: by
  the code, a bad `bff.ini` or a bind failure shows only in the service manager's state and exit code.
  In CI (run 43651545466, a `windows-2025` runner) it ran as that service, its path and account as
  declared, `/api/version` answering the framework version and `/api/instances` 200 (since run
  43654217880 listing exactly `default`, engine `llama.cpp`, state `inactive`, those builds answering a
  session-free request); nothing else has
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
- **The manager reads this same value** (`pai_core::bff_cfg`, the one place the file is parsed, shared
  with `crates/bff/src/listen.rs`): it holds this address against a server profile an instance runs, so
  saving such a profile on this port is refused naming `private-ai-bff`. No `bff.ini` is the default
  above; a file that cannot be read, or a `Listen` the service will not start on, answers no address
  and refuses nothing.
- Any address is accepted, a LAN address and the wildcard (`0.0.0.0`, `[::]`) included, served plain
  HTTP. The administrator's choice (user, 2026-11-17, a requirement change): told that the sign-in
  password then crosses the LAN in clear, they chose it over TLS in the bff and over an SSH tunnel.
  The web UI says so on the sign-in page and dialog when the connection is not encrypted (fe,
  b6710c7).

**The name a request is addressed at is checked on every request**, the reads and the sign-in
included (`origin.rs` `guard`), and a `Host` line that is absent is answered (HTTP/1.0 sends none).
It names this bff when it is one of the machine's names (1aac8e1, c919ccd): its host name
(`gethostname(2)`; on Windows `GetComputerNameExW`, type-checked only), `<name>.local`, and
`<name>.<domain>` for each `search`/`domain` line of `/etc/resolv.conf`; `[Bff] Hosts`,
comma-separated, replaces those when it names at least one name. The names are read at the bff's
start and, when `[Bff] Hosts` sets none, again on demand (42e5c23): a `Host` that is in none of the
names and is not `localhost` and not an address makes the guard read the host name and the
resolver's search domains once (`lib.rs` `try_router_at`'s source, no lock held across the read),
replace its list with the answer and answer the name when the fresh list carries it, so a search
domain that arrives with DHCP after the start is served without a restart; a name that stays
unknown is refused, and the machine is asked once per request. `localhost`, an IP literal and a
name already in the list ask it nothing. A `[Bff] Hosts` list is the whole list: the machine is
never read again. A call of `listen::load_hosts` reads it, and a `bff.ini` that cannot be read is
no refusal there: the machine's names stand, and `load` is the call that reports the file.
Beside the names: any IP literal (an IPv4 address, an IPv6
one in brackets, a zone id is no address), `localhost` and the loopback addresses (`127.0.0.0/8`,
`[::1]`) only when the listen serves the loopback, bound to a loopback address or to the wildcard,
never when bound to one LAN address. Matches are exact, in any letter case, one trailing dot ignored
(the root of the DNS tree), so `aorus.bombicci.local.evil.example` and `aorus.bombicci.local..` are
out: that is the DNS-rebinding guard. Anything else is 403 `request-bad-host`, and the session cookie
is bound to the host that set it, never to a port, so a rebinding carries no session. A method that
is not a read is refused in addition, 403 `request-cross-origin`, when its `Sec-Fetch-Site` is
neither `same-origin` nor `none`, or its `Origin` is not the `Host`'s host and port.

**Every reply carries `X-Frame-Options: DENY` and `X-Content-Type-Options: nosniff`** (1aac8e1), the
guard's 403, the body cap's 413 and the router's own 404 and 405 included: the layer is outside the
guard and writes with `insert`, so each header stands once. No Content-Security-Policy yet.

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
| `GET /api/version` | `{"version": "<framework version>"}`, for a signed-in user; 401 `session-required` with no session, the manager not asked. The read asks the manager nothing and leaves the session's idle deadline where it was |
| `GET /api/instances` | `[{"id": "<id>", "engine": "llama.cpp", "serverProfile": "<profile>|null", "state": "<state>", "port": <n>, "tags": [<text>, ...], "instanceTags": [<text>, ...], "boot": <bool>, "removable": <bool>}]` from `private-ai-manager`, for a signed-in user; 401 `session-required` with no session, the manager not connected to. The session gates the read and is not its principal: the list, the states, the server profiles, the ports, the tags and the start-at-boot flag are what the manager answers of the bff's own account, and the idle deadline stays where it was, the page polling this read ([below](#the-instances-from-the-manager)) |
| `POST /api/instances` | `{"engine", "id", "serverProfile"}` in, each a string: 201 `{"id"}` with the id the body sent, once the manager wrote the instance, for a signed-in user, as that user, under the write's 15 s. A body that is not those three strings is 400 `instance-malformed`, the manager not connected to; every rule of the instance itself is the manager's and keeps its key ([below](#creating-and-removing-an-instance)) |
| `DELETE /api/instances/{id}` | 204 with no body once the manager took the instance away, for a signed-in user, as that user, under the write's 15 s: 409 `instance-protected` `[id]` for `default` and `instance-running` `[id]` for one that is up, 404 `client-no-instance` for an id the record does not name ([below](#creating-and-removing-an-instance)) |
| `GET /api/node` | `{"name": "<host name>", "api": "/api", "version": "<framework version>", "engines": [{"engine": "<engine>", "version": <text|null>, "backends": [{"name": "<suffix>", "version": <text|null>}]}]}`, for a signed-in user; 401 `session-required` with no session, the manager not asked. Like `GET /api/version`, the read asks the manager nothing and leaves the session's idle deadline where it was ([below](#the-node)) |
| `GET /api/node/devices` | `{"probedAt": "<UTC text>|null", "ramMib": <int>, "cpuThreads": <int>, "devices": [{"id", "name", "backend", "totalMib", "freeMib"}]}`: the node's compute devices beside the host's RAM and logical CPUs, asked of the manager as the signed-in user, on a bound long enough for the engine probe it runs (35 s); 401 `session-required` with no session, the manager not asked ([below](#the-node)) |
| `POST /api/session` | Signs in, for anyone: `{"user"}` and the session cookie ([below](#the-session)) |
| `GET /api/session` | `{"user"}` of the cookie's session, for anyone |
| `DELETE /api/session` | Signs out, for anyone: 204, the cookie cleared |
| `GET /api/instances/{id}/server` | The instance's `server@<server_profile>.ini`, the server profile its record names, with its defaults and limits, for a signed-in user ([below](#the-server-file-of-an-instance)) |
| `PUT /api/instances/{id}/server` | The form's save of that file, for a signed-in user ([below](#saving-the-server-file)) |
| `POST /api/instances/{id}/start`, `/stop`, `/restart` | The run verb as the signed-in user: 204 once the manager accepted the job, no body, the state is not in the reply (the page polls `/api/instances`); 401 with no session, 403 ending the session when the manager refused that user, 404 for no such instance, 503 with no manager, 502 `client-service-failed` when the service manager refused the job, 502 `client-other` for anything else, and 409 `{"key", "args"}`, no `field`, for a refusal of the state the job is asked over: a start or a restart of an instance with no model profile is `instance-no-model-profiles` `[id]` and the session lives on (`crates/bff/src/control.rs`) |
| `GET /api/instances/{id}/boot` | `{"enabled"}`, for a signed-in user; 401 `session-required` with no session, the manager not connected to. The read reads the session as a gate and never moves its idle deadline, so the page can poll it, and connects as the bff's own account: the manager's refusal of that connect is of the bff's account, so it ends no session. The failures are `/api/instances`' |
| `PUT /api/instances/{id}/boot` | `{"enabled": <bool>}` in, `{"enabled"}` out, read back from the manager on the same connect, for a signed-in user: 400 `boot-malformed` for a body that is not that JSON, the other answers as the verbs |
| `PUT /api/instances/{id}/tags` | `{"tags": [<text>, ...]}` in, 204 with no body once the manager wrote the instance's own list, for a signed-in user: 400 `tags-malformed` for a body that is not that list, 422 `server-bad-tags` (`field: "tags"`) for a list the manager's rule refuses ([below](#the-tags-of-an-instance)) |
| `PUT /api/instances/{id}/profiles/server` | `{"serverProfile": "<profile>"}` in, `{"running"}` out, for a signed-in user: the record now names that `server@<profile>.ini` as the file the instance runs, nothing started, stopped or restarted ([below](#the-association-routes)) |
| `GET /api/instances/{id}/profiles/model` | `{"all", "profiles"}` for a signed-in user: the model profile pool's rows as this instance's section of `assignments.ini` makes them, each with `value`, `viaAll` and `effective` ([below](#the-association-routes)) |
| `PUT /api/instances/{id}/profiles/model/{profile}` | `{"value": "on"|"off"|"none"|"inherit"}` in, `{"effective", "running"}` out, for a signed-in user: the one key of the instance's section of `assignments.ini`, `effective` read back after the write ([below](#the-association-routes)) |
| `PUT /api/instances/{id}/profiles/model` | `{"all": "inherit"|"none"}` in, `{"running"}` out, for a signed-in user: the `*` of that section, the whole pool ([below](#the-association-routes)) |
| `GET /api/profiles/model` | The model profile rows for a signed-in user, llama-server's global `[*]` never among them: `[{"id": "<model profile id>", "engine": "llama.cpp", "engineAvailable": true, "enabled": <bool>, "model": "<model file>", "assignments": [{"instance", "value", "viaAll", "effective"}, ...]}]` ([below](#the-model-profile-routes)) |
| `GET /api/profiles/model/{id}` | One model profile for a signed-in user: `{"id", "engine", "engineAvailable", "enabled", "values", "projectUrl", "revision"}` ([below](#the-model-profile-page)) |
| `PUT /api/profiles/model/{id}` | The model profile page's save of the keys that changed: `{"revision", "values"}` in, `{"revision", "running", "restart"}` out, for a signed-in user, the section rewritten in place in the file that holds it ([below](#the-save)) |
| `PUT /api/profiles/model/{id}/enabled` | `{"enabled": <bool>}` in, `{"enabled", "running", "restart"}` out, for a signed-in user: the model profile switched on or off ([below](#the-model-profile-routes)) |
| `GET /api/profiles/server` | The node's server profiles, the manager's rows in its order, for a signed-in user: `[{"id", "engine", "port", "tags", "instance"}]`, `instance` the id of the instance using the profile, `null` when none does ([below](#the-server-profile-pool)) |
| `POST /api/profiles/server` | `{"engine", "id", "port"}` in: a new profile of the defaults, 201 `{"id"}`, for a signed-in user ([below](#the-server-profile-pool)) |
| `GET /api/profiles/server/{id}` | The profile's own `server@<id>.ini` as the instance route's document, gaining `instance` and a `defaults.StateDir` of the instance that runs it, for a signed-in user ([below](#the-server-profile-pool)) |
| `PUT /api/profiles/server/{id}` | The form's save of that file, `{"revision", "values"}` in, `{"revision", "running"}` out with the state of the instance using the profile, for a signed-in user ([below](#the-server-profile-pool)) |
| `DELETE /api/profiles/server/{id}` | 204 with no body once the manager removed a profile no instance names, for a signed-in user ([below](#the-server-profile-pool)) |
| `GET /` | The embedded `index.html`, for anyone |
| `GET /assets/{*path}` | The embedded file at `assets/<path>`, for anyone |

Other methods on these routes are 405, any other path 404. Only a path in the embedded table
matches, so `..` reaches nothing else. Content types go by extension: HTML, JavaScript
(`text/javascript`), CSS, anything else `application/octet-stream`.

Checked by hand on Linux from the debug build (2026-11-14): `--version` exits 0, an unknown argument
2, `0.0.0.0:8590` and `localhost:8590` exit 1; on `127.0.0.1` it answered `/api/version`, `/` as
`text/html`, 405 to a `POST` and 404 elsewhere. Under `PRIVATE_AI_DATA_ROOT` `/api/instances`
answered 503 `{"error":"unavailable"}` (the error shape before `{"key", "args"}`) and a `POST` to it
405. That bff predates 1aac8e1, and the session gate of 2026-11-19: `/api/version` and
`/api/instances` answered a request with no session in that build, and answer 401
`session-required` now. `0.0.0.0:8590` is an address it serves now (pinned by
`parse_accepts_an_address_off_loopback`, `tests/listen.rs`), and nothing here was repeated against a
LAN address.

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
redirect, or a machine with the manager down). It also predates the session gate of 2026-11-19: that
`/api/instances` 503, and the Overview showing the version with no sign-in, are what a request with no
session got then; the read answers 401 `session-required` now and the Overview is behind sign-in.

## The instances from the manager

`GET /api/instances` asks `private-ai-manager` for `ListInstances` and `Instances`, then `Status` of
each instance, and `GET /api/profiles/server` asks `ServerProfiles` ([below](#the-server-profile-pool)):
the four calls the manager answers without authorization. What the bff calls beyond them is
`Authenticate`, what sign-in makes ([below](#the-session)), the `As` form of a guarded method naming
the session's user, what a signed-in route makes ([below](#the-server-file-of-an-instance)), the tags
write ([below](#the-tags-of-an-instance)), the pool's create, delete, document and save
([below](#the-server-profile-pool)), the three association writes with the reads of `assignments.ini`
and of the two model profile files ([below](#the-association-routes)), the instance's create and
removal ([below](#creating-and-removing-an-instance)), and the model profile list,
switch and save on one connect as the user ([below](#the-model-profile-routes))
(`crates/bff/src/lib.rs`). The association routes ask `Instances` and `Status` as well, on a connect
of the bff's own account ([below](#the-association-routes)).

| Status | Body | When |
| --- | --- | --- |
| 200 | `[{"id", "engine", "serverProfile", "state", "port", "tags", "instanceTags", "boot", "removable"}]` in the manager's order, `[]` when there is none | Every call answered; an instance removed between the calls is left out |
| 401 | `{"key": "session-required", "args": []}` | The request's cookie names no live session: `connect` is never called, no `Set-Cookie` goes out |
| 503 | `{"key": "client-unavailable", "args": []}` | No manager to reach (none running, macOS), or no full answer within 5 s |
| 502 | `{"key": "client-not-authorized", "args": []}` | The manager refused a call |
| 502 | `{"key": "client-other", "args": []}` | Any other failure |

- **The state is the manager's name** (`InstanceState::name()` in `pai-core`: `active`, `inactive`,
  `activating`, `deactivating`, `failed`, or an unknown name passed on as is). The engine is
  `llama.cpp`, the only engine today (`ENGINE_LLAMA_CPP` in `lib.rs`). The error body is only the
  strings key with empty args, never the manager's detail.
- **`serverProfile` is the server profile the instance runs**: the name the record of `instances.ini`
  carries, as the manager spells it, `null` when its section names none, from the same `Instances`
  call as the port and the tags.
- **The port and the tags come from `Instances`**: `port` is the port the instance's server profile
  names, `0` when its section names no profile or the record cannot be read; `tags` is the server
  profile's `Tags` followed by the instance's own, in that order with the repeats removed
  (`tags_of_row` in `lib.rs`), `instanceTags` the record's own list. An instance with no row is
  listed with `0` and the two empty lists, and a `Instances` call that fails is no rows for any of
  them: the read still answers 200.
- **`boot` is that instance's start at boot**, read from the manager's `BootEnabled` for each id the
  list carries, over the same connect as the state. One that cannot be read is `null` on that item
  and fails nothing: the list is answered in full, and the page shows no start-at-boot row for that
  tile. `GET /api/instances/{id}/boot` stays: the instance page reads it, and reads it again after a
  write.
- **`removable` is the page's offer to remove the instance**: false only for `default`, the id the
  engine's install writes and the removal refuses (`instances::removable` in
  `crates/bff/src/instances.rs`, whose `PROTECTED_INSTANCE` is the id the manager protects in
  `service.rs`), whatever its state: whether it runs is `state`, and the removal refuses a running one
  anyway ([below](#creating-and-removing-an-instance)).
- **A new connection per request**, so a manager started after the bff is seen at the next request.
  The connect and the calls run on a blocking thread under one 5 s timeout (`MANAGER_TIMEOUT` in
  `lib.rs`) that covers the connect. The routes that write a file, the server save, the model profile
  switch and save, the pool's create, save and delete, the instance's create and removal, and the three
  association writes, run under the longer `WRITE_TIMEOUT`
  ([below](#saving-the-server-file), [below](#the-association-routes)).
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
  The two calls of `/api/instances` passed the pipe in CI (run 43651545466); its `Instances` call,
  `Authenticate`, sign-in's call, and an `As` call have not crossed it, and the refusals are
  type-checked only.

### The tags of an instance

`PUT /api/instances/{id}/tags` takes `{"tags": [<text>, ...]}` and asks the manager's
`SetInstanceTags` as the signed-in user, under the same 5 s timeout as the read (`control.rs`). The
session is checked first, so no request reaches the manager without one.

| Status | Body | When |
| --- | --- | --- |
| 204 | no body | The manager wrote the list. Nothing is read back: the browser refreshes when it wants the tags |
| 400 | `{"key": "tags-malformed", "args": []}` | The body is not `application/json`, is not read, its `tags` is not a list, or an item is not a string; the manager is not connected to |
| 401 | `{"key": "session-required", "args": []}` | No session; the manager is not connected to, so a body of the wrong shape is 401 and not 400 |
| 403 | `{"key": "session-not-administrator", "args": []}` | The manager refused the write as this user (a member of `pai` at sign-in, removed since): the session ends, as on every guarded write |
| 404 | `{"key": "client-no-instance", "args": []}` | No such instance, or the manager refuses the id (`NoInstance`, `InvalidId`) |
| 413 | `{"key": "payload-too-large", "args": []}` | A body over 64 KiB, by the extractor in front of the whole router, before the manager is connected to |
| 422 | `{"key", "args", "field": "tags"}` | The manager's rule refused the list: `checks::tags_rule`, answering `server-bad-tags` with its reasons, the same rule and the same words as the server profile's tags ([below](#saving-the-server-file)). The session stays signed in: a refusal of a value is no refusal of the user |
| 503 | `{"key": "client-unavailable", "args": []}` | No manager to reach, or no full answer within 5 s |
| 502 | `{"key": "client-service-failed", "args": []}` | The system's write of the record failed (the manager's `Service`) |
| 502 | `{"key": "client-other", "args": []}` | Any other failure, never with the manager's detail |

- **The manager writes `Tags` into the instance's own section of `instances.ini`**, one action record
  for the whole call, and applies the rule before it reads the record: a list it refuses leaves every
  file untouched (`set_instance_tags` in
  [crates/manager/src/manager/service.rs](../manager/src/manager/service.rs)).
- **The route is pinned by `tests/instances.rs`**, over the fake manager: the write of a list answers
  204, an empty list clears the tags, no session is 401, no such instance 404, a list the manager's
  check refuses is 422 with the check's key and values, a body it cannot read is 400 with the manager
  never connected to, no manager 503. The 413 of a body over the cap is the extractor's, by the code.

## The node

`GET /api/node` (`crates/bff/src/node.rs`) answers what this host is and what its packages installed.
Like `GET /api/version` it is gated by a session, asks the manager nothing and leaves the session's
idle deadline where it was: 401 `session-required` with no live session, and no other answer. The web
UI reads it for the labels of the navigation and for the About page
([below](#the-web-ui-fe)).

| Field | From |
| --- | --- |
| `name` | `listen::hostname`, the same host name the `Host` guard reads: `gethostname(2)` on Unix, `GetComputerNameExW(ComputerNameDnsHostname)` on Windows (type-checked only); `localhost` when the machine names itself nothing |
| `api` | `/api`, a constant: the fe builds a route out of it |
| `version` | The framework's version, the same one `GET /api/version` answers |
| `engines` | `pai_core::versions::installed` of the versions folder, read at every request, so an engine upgrade shows without a restart of the service |

- **The versions are the packages' own files**, one per package at `versions/<package>`
  ([docs/build-packaging.md](../../docs/build-packaging.md)): the engine's `llama.cpp` and one per
  runtime `llama.cpp-<suffix>`, the suffix lower-case letters and digits (`cuda`, `rocm`, `vulkan`).
  Only a regular file of at most 128 bytes counts, a symlink is not a file here; its version is the
  file's first line trimmed and printable ASCII, an empty file is a `null` version, and an engine is
  listed only when its own file is there: its `backends` are the other files, sorted by name.
- **The folder is the install's** (`pai_core::bff_cfg::versions_dir`, `crates/core/src/bff_cfg.rs`):
  `/usr/share/private-ai/llama.cpp/versions` on Linux (only when the engine's config folder is
  `/etc/llama.cpp`, the packaged install), `<engine install dir>\share\versions` on Windows, the
  engine's install folder from `HKLM\Software\llama.cpp` `InstallDir`. Anywhere else is no folder,
  a tree build and macOS included, and a folder that cannot be read answers the same as an empty
  one: `engines` is `[]`.
- **The compute devices are `GET /api/node/devices`** (`node.rs` `devices`, 66b2bb3, the design's G2 in
  [ARCHITECTURE.md](../../ARCHITECTURE.md)): unlike the document, this read asks the manager, as the
  signed-in user through `DevicesAs`, in a blocking task over one connect and one call, and
  `Sessions::current` moves the session's idle deadline (it is a page read, not a poll). The answer is
  the manager's probe of `llama-server --list-devices` in the probe's own order, the CPU pseudo-device
  already left out by the manager, each row the id `device` takes, the product name, the backend (the
  id with the digits the engine appends taken off: `ROCm0` is `ROCm`), and the device memory in MiB
  **as it was at the probe**: `probedAt` is that moment as UTC text (`2026-11-20T09:14:03Z`), `null`
  when no probe answered. No list is 200 with `devices: []`, never an error page: a node with no card
  has nothing to distribute. Beside the list travels the host's own inventory (a1a3458): `ramMib`,
  the node's total RAM in whole MiB, and `cpuThreads`, the logical CPUs its process may use, read by
  the manager off the machine it runs on (`crates/core/src/host.rs`, put into the document by
  `devices_document`: `/proc/meminfo`'s `MemTotal`, `GlobalMemoryStatusEx`, `available_parallelism`)
  and answered whatever the probe did, the two being the host's and not the engine's. Each 0 is the
  host's own "no figure" (a reading that failed) and travels as it is: the bff scales, rounds and
  interprets nothing. The bound is `DEVICES_TIMEOUT` (35 s, `lib.rs`): the manager's own probe bound
  `GPU_PROBE_TIMEOUT` (30 s) plus the transport's 5 s, because the first read of a node starts a
  llama-server on it to read its cards. Its failures are the model profile reads'
  (`model_profiles::read_answer`): 403 `session-not-administrator` and the session ended when the
  manager refused that user, 503 `client-unavailable` for no manager or an answer that does not land
  within the bound, 502 `client-other` for anything else, never the manager's detail. **No page calls
  it yet**: the fe's GPU distribution card is the open brief `fe/briefs/14-gpu-distribution.md`.

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
| 429 | `{"key": "session-too-many-attempts", "args": [<seconds left>]}` | 5 bad passwords, and separately 5 right-password refusals, for one (typed user in lowercase, client address) each lock that pair for 60 s from their 5th within a 60 s window; the lock is read before the manager is asked, so a locked pair never reaches it. A sign-in still being checked counts toward both, so guesses sent at once cannot overrun the limit |
| 503 | `{"key": "session-unavailable", "args": []}` | No manager to reach, no answer within `SIGN_IN_TIMEOUT`, or no random id could be drawn; none of these records anything, and the check thread of a manager that is slow keeps running after the answer |

- **The cookie** carries an opaque id: 32 bytes from the OS's random source (the `getrandom` crate,
  since std has no stable one) in hex. No `Secure` while the bff serves plain HTTP (a
  browser would not send it back over `http`), no `Domain`: the browser binds the cookie to the host
  that set it, never to a port, so a rebinding to another host carries no session, and a move to
  another port on the same host keeps it ([above](#running-it)). The password is neither kept nor
  sent back.
- **`GET` and `DELETE /api/session`** answer 401 `{"key": "session-required", "args": []}` with no
  cookie, an unknown id or an expired one. `DELETE` ends the session (a replayed id is refused) and
  clears the cookie (`Max-Age=0`); the user's other sessions stay.
- **A session ends after 30 minutes without a request that uses it**; each use moves the deadline,
  apart from the four reads a page polls (`GET /api/version`, `GET /api/node`, `GET /api/instances` and
  `GET /api/instances/{id}/boot`): those read the session as a gate and leave the deadline where it
  was (`Sessions::peek` in `crates/bff/src/session.rs`), so a page kept open by those calls alone
  signs itself out at the 30 minutes.
- **In memory only**: a restart of the bff signs everyone out. The lockout reads the client's
  address from the connection (`serve.rs` serves `into_make_service_with_connect_info`).
- The time comes from a clock handed to `router_with_session` (`Instant::now` in the service), so
  the tests move it; `SIGN_IN_TIMEOUT` reaches the same seam (`session::routes_with_sign_in_timeout`
  takes a bound of its own).

## The server file of an instance

`GET /api/instances/{id}/server` (`crates/bff/src/server.rs`) first needs a session: with no cookie,
or an unknown or expired one, it answers 401 `{"key": "session-required", "args": []}` and does not
connect to the manager. `PUT` of the same path saves it ([below](#saving-the-server-file)). The file
is the server profile the record names for the instance, `server@<server_profile>.ini`
(`profile_file`): the route asks `Instances()` before any file and never opens one named after the id,
so an id the record does not list, and a section naming no profile, are 404 with nothing read. The
shipped record has `[default]` with `ServerProfile = default`, so the shipped instance's file is
`server@default.ini`, and a save there is judged as a profile in use (a `server@<x>.ini` no
section names as its profile is a spare profile, judged against nothing,
[crates/config/README.md](../config/README.md#through-the-manager-linux-windows)).
Then it asks the manager for `ReadFileAs` of that name naming the
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
| 403 | `{"key": "session-not-administrator", "args": []}` | The manager refused the read as this user (a member of `pai` at sign-in, removed since): the session ends, so the same cookie answers 401 from then on. A read gated by a session whose connect carries no principal (the instances list, `GET boot`) answers the same refusal 502 `client-not-authorized` and ends nothing: the refusal is of the bff's account, not of the signed-in user |
| 404 | `{"key": "client-no-instance", "args": []}` | The record names no server profile for the id (`profile_file`, nothing read), the manager says no such instance or refuses the id, or there is no such file (`NoInstance`, `BadFileName`, `InvalidId`) |
| 503 | `{"key": "client-unavailable", "args": []}` | No manager to reach, or no answer within 5 s |
| 502 | `{"key": "client-other", "args": []}` | Any other failure, never with the manager's detail: a manager built before the `As` methods, which refuses `ReadFileAs`, lands here |

- **`values`**: the 21 `[Server]` keys of `pai_core::server_cfg::ServerConfig` by INI name, read as
  the configurator reads the file (`server_cfg::from_text`): a number, `true`/`false` or a string,
  `null` when unset or unreadable (`Port = abc`). `CudaAllreduce` is normalized to lowercase;
  `CudaDisableGraphs`, `AttnRotDisable` and `RocprofAttach` are `false` when absent;
  `RocblasUseHipblaslt` is `"on"`, `"off"` or `null`. `Devices` is the file's GPU list in one string
  (`ROCm0,ROCm1`, the order `--device` takes, or `none`), `null` when the file names no key: every GPU
  of the node. `Tags` is always a list of strings, `[]` when
  the file has no key. `OpencodeApiKey` is `null` exactly when the
  manager's read reported the key withheld, the key in the file's text included.
- **`withheld`**: `["OpencodeApiKey"]` exactly when the manager's read reported the key withheld, so
  the web UI can say a key is saved without showing it; absent otherwise, and on this route always
  absent: the manager never withholds a principal's read. The bff never withholds on its own: every
  session is a `pai` member, it keeps no role per session.
- **`defaults`**: `Port` `server_cfg::DEFAULT_PORT`, `ModelsMax` and `ModelsMemoryMargin`
  `server_cfg::DEFAULT_MODELS_MAX` and `DEFAULT_MODELS_MEMORY_MARGIN` (the configurator's form uses
  the same), `OpencodeBaseUrl` the URL derived from the saved `Hostname` and `Port` even when the file
  overrides it, `ModelsDir` the instance's (`paths::instance_models_dir`), `StateDir` the instance's own folder
  (`paths::instance_state_dir()` joined with the id), never the bff account's.
- **`limits`**: `[min, max]` of `Port`, `ModelsMax` and `ModelsMemoryMargin` as the manager checks
  them (`checks::server_int_range`), and of `Threads` and `ThreadsBatch` 1 to the logical CPU count
  of the machine this service runs on (`server.rs` `own_cpu_count`, `available_parallelism`; a host
  answering nothing is 1). The two saves cap those fields at the same count (`available_parallelism`
  in `server.rs` `save` and `server_profiles.rs` `save`). That count is the bff's own, not the
  node's: the server reads ask the manager no device list (a1a3458), because `Devices` can start the
  engine's probe and these reads keep the 5 s bound, so the node's `cpuThreads`
  ([above](#the-node)) is not read here. The Server form reading it is R5, open work in
  [PLAN.md](../../PLAN.md).
- **`interfaces`**: `net_ifaces::interfaces()` as `[{"value", "label"}]`.
- **`revision`**: a keyed SipHash-2-4 of the file's text, printed as 16 hex digits (`server.rs`):
   the key is a 16-byte secret drawn from the OS's random source at each bff start (`lib.rs`
   `try_router`). The same text through one bff run gives the same revision, any other text (a
   comment, the API key, included) gives another; a change detector, not an oracle: with a revision
   and not the file nobody can check a guess about its text. A bff start draws a new secret, so
   every open form is stale after it and reloads once.

### Saving the server file

`PUT /api/instances/{id}/server` (`server.rs` `save`) is the Server form's save, as the signed-in
user. The body is `{"revision", "values"}`: the revision the GET answered and the 21 typed values, `Tags` a list of
strings.
The body becomes the file's text through the one fields table the GET reads with, and passes
`pai_core::server_cfg::check` with this machine's CPU count, all before any manager is connected
to: a body that is not that JSON, or a value of the wrong type, is 400 `server-malformed`; a check
refusal is 422 `{"key", "args", "field"}`, `field` the INI key the form marks. A body over
`origin::MAX_BODY` (64 KiB) is refused by the extractor layer in front of the whole router:
413 `payload-too-large`, before the session is read. `Devices` is judged by the shape of its list
only, never against the node's device list (`checks::device_list`): `ROCm0, ROCm1` is 422
`device-bad-name` with args `["Devices", " ROCm1"]` and field `Devices`, and nothing is written,
while an id the node has no GPU for (`ROCm7`) is taken, the instance that names it failing to start.
`Devices` is one of the free-text server keys (`checks::SERVER_TEXT_KEYS`), so a `;` or a `#` in the
list is the marker refusal and not the list's: 422 `comment-markers` with args `["Devices"]` and
field `Devices`, the shape rule never seeing a marker (its `ROCm0;x` is one entry of the right
shape) while the INI reader cuts the value at the marker, so the file would reload as `ROCm0`, GPUs
nobody typed. A `null` `Devices`, like a blank, is no key in the file: every GPU.

The file is then read as the session's user (the record's file, as the read resolves it), its text hashed with the run's secret and compared
with the sent revision, and written through the manager's `WriteFileIfAs`, which accepts only while
the file still holds exactly the text read: a write that lands between the read and the write is
refused by the manager, answered with the same 409, and nothing is written. The read, the write and
the status that answers `running` are three calls of one connect, and the whole save runs under the
write's own limit: `WRITE_TIMEOUT`, 15 s (`lib.rs`), an estimate that a measured save's figure will
replace. The reads of the router keep the 5 s `MANAGER_TIMEOUT`, the device list ([below](#the-node)) the
one read on a longer bound. The reply is 200
`{"revision", "running"}`: the revision of the text the file holds now, and whether the instance's
process runs (`active` or `activating`); a status read that fails after a write that succeeded
answers `running: true`. The 503, the 504 and the 200 of a call nobody waited for turn on one fact:
while the write was never asked, a time limit that runs out or a stream that dies is 503
`client-unavailable` and the file is what it was; from the moment the write was sent its outcome is
unknown and the answer is 504 `client-timeout`; once the write landed the save is done, so a limit
that runs out on the status read that follows is the save's 200 with `running` true, the safe answer
for a state the bff cannot read: a 504 there tells the user to look at the server and decide
whether the file is saved, when the bff knows it is. A time limit does not stop the blocking task:
the handler and that task share one atomic (Idle, Asked, Done, Cancelled in `server.rs`) and settle
that fact with a compare and exchange, so a task parked in the connect or the read finds the state
Cancelled and never writes, a handler that finds the state Asked answers 504, and one that finds it
Done answers the 200.

| Status | Body | When |
| --- | --- | --- |
| 200 | `{"revision", "running"}` | The file was written; `running` is whether the instance's process runs, and `true` when the status read failed, or the limit ran out after the write landed, that state being one the bff cannot read |
| 400 | `{"key": "server-malformed", "args": []}` | Not JSON, another content type, `revision` missing or not text, a value of the wrong type; the manager is not connected to |
| 401 | `{"key": "session-required", "args": []}` | No session; the manager is not connected to |
| 403 | `{"key": "session-not-administrator", "args": []}` | The manager refused the connect, the read or the write as this user: the session ends, as the read's 403 (a status it refuses after the write is not a failure: 200, `running: true`) |
| 404 | `{"key": "client-no-instance", "args": []}` | No such file, or the manager says no such instance or refuses the id |
| 409 | `{"key": "server-changed-elsewhere", "args": []}` | The file is not the one the form read: the sent revision does not match the read text, or the manager's compare and swap found it changed. Nothing is written, the session stays signed in |
| 413 | `{"key": "payload-too-large", "args": []}` | A body over 64 KiB, by the extractor, before the session is read |
| 422 | `{"key", "args", "field"}` | A value the checks refused: the bff's own (`server_cfg::check`, this machine's CPU count included), the manager's content check (`Invalid`), `client-address-in-use` (`field: "Port"`, its `args` naming who holds the address: the other instance, or `private-ai-bff`) or `client-unresolvable-host` (`field: "Hostname"`); the manager's detail stays out of the body. A tags list the rule refuses is 422 `server-bad-tags` with `field: "tags"` (the form's control, lowercase, the only key not spelled as the INI writes it): the rule is `checks::tags_rule`, run by `server_cfg::check` on the body's list |
| 503 | `{"key": "client-unavailable", "args": []}` | No manager reached and the write was never asked: the file is what it was |
| 504 | `{"key": "client-timeout", "args": []}` | The write was asked and its outcome is unknown: a time limit that ran out at the write, or a manager lost at the write. A limit that runs out after the write landed is the 200 above |
| 502 | `{"key": "client-other", "args": []}` | Any other failure. By the code a manager built before `WriteFileIf` answers that call with a refused frame that ends the connection, so it lands here: type-checked only, never run |

## The server profile pool

`GET`/`POST /api/profiles/server` and `GET`/`PUT`/`DELETE /api/profiles/server/{id}`
(`crates/bff/src/server_profiles.rs`, b535304, `ARCHITECTURE.md`'s slice 14): the node's server
profiles, the `server@<id>.ini` files of the engine's config directory, listed, created, read, saved
and deleted. The `{id}` is a profile's own name, and the engine is always `llama.cpp`
(`ENGINE_LLAMA_CPP`), the node's one engine: the create takes it in the body and refuses any other. The
instance's Server route is unchanged ([above](#the-server-file-of-an-instance)): the web UI's instance
page still reads and saves it, and no page uses the pool yet.

All five sit behind sign-in: with no live session each answers 401 `session-required`, connecting to no
manager, and each reads the session with `Sessions::current`, so every one of them moves the idle
deadline, unlike the four polled reads ([above](#the-session)). The list asks nothing of who it acts
for: `ServerProfiles()` is answered to any caller, as `Instances` is, so the connect carries no
principal and the bff's own account asks, the session being the whole gate. The create, the delete, the
document and the save go as the `As` forms naming the signed-in user, the manager re-checking their
membership of `pai` at every call, so a refusal of that user is 403 `session-not-administrator` and ends
the session, as on the instance's Server read. The create and the delete are the manager's `configure`
writes, and its own rules for them, what file it writes and with what mode and group, when a name is
taken, when a profile is in use and what a record it cannot read answers, are "Through the manager" in
[crates/config/README.md](../config/README.md#through-the-manager-linux-windows).

### The list

`GET /api/profiles/server` answers the manager's rows in its order, sorted by id. A file whose name the
id rule refuses is no row, the list being of files; a file that cannot be read answers port 0 and no
tags, the answer the manager gives for a profile whose file is missing, and fails nothing. A row
carries the port and the `[Server]` tags and nothing else of the file, so a key it holds, its API key
included, stays behind a read of it.

| Status | Body | When |
| --- | --- | --- |
| 200 | the rows, `[]` when the engine's directory holds no profile | the read answered |
| 401 | `session-required` | no session, the manager not connected to |
| 403 | `session-not-administrator` | the manager refused the call, and the session ends. `ServerProfiles` is answered to any caller, so on today's manager only a refusal of the bff's own account reaches this |
| 503 | `client-unavailable` | no manager, or no full answer within 5 s |
| 502 | `client-other` | anything else, never the manager's detail |

### The create

`POST /api/profiles/server` takes `{"engine", "id", "port"}` and answers 201 `{"id"}`. The body is read
before anything is asked, so a body nothing can be made of reaches no manager: not `application/json`,
not read, `engine` or `id` not text, or a `port` that is no whole number, is 400
`server-profile-malformed`; a body with no `port` and one with `port: null` are both the file with no
`Port` key. A body over 64 KiB is the extractor's 413, before the session is read. The bff then applies
the rules itself and the manager applies them again, being the writer: an engine other than `llama.cpp`
is 422 `unknown-engine` (`field: "engine"`); a name the instance-id rule refuses is 422
`server-profile-bad-id` (`field: "id"`: 1 to 32 of `a-z`, `0-9` and `-`, starting with a letter or a
digit); a port outside `checks::server_int_range("Port")` is 422 `not-whole-number-in-range`
(`field: "port"`, its args the key, the text and the two edges). A name already there is 409
`client-server-profile-exists` `[id]`, and the session lives on. A manager that is unreachable, or that
outlives the write's 15 s, is 503 `client-unavailable` with the outcome unknown: a retried create is
then 409, which is how the ambiguity shows.

### The document and the save

`GET /api/profiles/server/{id}` is today's Server document of the profile's own file, with the fields
[above](#the-server-file-of-an-instance) and `instance`: the id of the instance whose record names the
profile, `null` when no section does. Two differences from the instance's document: `defaults.StateDir`
is the folder of the instance that runs the profile and `null` for a profile none runs, the folder
belonging to whichever instance runs it; and a name the manager refuses (`BadFileName`, `InvalidId`)
and a file that is not there are 404 `client-no-server-profile` `[id]`, where the instance route
answers `client-no-instance`. The file and the record are two calls of one connect made as the
session's user, so a record the manager cannot read answers the record's own failure, never
`instance: null`, which would offer the delete of a profile that is in use. The read is a principal's
read, as on the instance route, so `withheld` is absent and `OpencodeApiKey` carries the saved key.

`PUT /api/profiles/server/{id}` is the same form save as the instance's
([above](#saving-the-server-file)): the same `{"revision", "values"}` body, the same fields table and
checks, the same compare-and-swap write, the same 400, 409, 413, 422, 503, 504 and 502, and the same
200 of a call nobody waited for. It answers `{"revision", "running"}`, `running` being the state of the
instance that uses the profile: `false` for a profile none runs, with no state asked, and `true`
whenever the state cannot be read, the safe answer, which covers a record read that fails once the
write has landed (the write done, no instance named). The address check follows the profile as it
does there: in use, the port judged against the profiles the other instances run and against the web
service's `[Bff] Listen`; unused, judged against nothing.

### The delete

`DELETE /api/profiles/server/{id}` answers 204 with no body once the manager removed the file. A
profile a section of `instances.ini` names is 409 `client-server-profile-in-use` `[profile, instance]`
and the file stays; no such profile is 404 `client-no-server-profile` `[id]`; a record the manager
cannot read is 502 `client-other`, never the "unused" answer, so a profile a running instance reads is
never removed on a record read failure. A retried delete after a 503 is 404.

## The model profile routes

`GET /api/profiles/model`, `GET /api/profiles/model/{id}`, `PUT /api/profiles/model/{id}` and
`PUT /api/profiles/model/{id}/enabled`
(`crates/bff/src/model_profiles.rs`, built 2026-11-17; `PLAN.md` step 10 items 11, 11b, 11c and 11e, the
designs "The switch on and off (item 11b)" and "Saving a preset from the web" in `ARCHITECTURE.md`):
the Model profiles page's list and switch, and the read and the save of one model profile. All four are behind
sign-in and all four ask `private-ai-manager` on **one connect as the signed-in user**, so it is
that user's own membership of `pai`, re-checked by the
manager at the call, that answers: 401 `session-required` with no session (the manager is not
connected to, nothing is written), 403 `session-not-administrator` and the end of the session when
the manager refuses that user. Since the association routes (2d9a4c8) the list reads the manager's
record `assignments.ini` on the same connect, for each row's `assignments`, and the switch and the
save read it once their writes land, for the instances they offer to restart: the record is described
with [the association routes](#the-association-routes).

What they read and write is the manager's pair of model profile files, `models.ini` and
`models.ini.disabled` in the engine's config directory (`paths::engine_config_dir`:
`/etc/llama.cpp/` on Linux, `%ProgramData%\llama.cpp\config\` on Windows): the same files the
configurator's Models tab and `pai model-profile` read and write, and the one the engine reads the enabled
sections of. The disabled file is never read by the engine. The two names are constants in the code:
the `{id}` of the URL is a section name, matched in any letter case, never a path.

### The list

One read of each file, and one row per section except llama-server's global `[*]`,
`[{"id", "engine", "engineAvailable", "enabled", "model", "assignments"}]`:

- `id` as the file spells it; `engine` the engine whose model profile file holds the section (`llama.cpp`,
  the only engine today) and `engineAvailable` true for every row, there being no engines route to
  check it against.
- `model` is the section's `model` key as the file holds it, no path resolution and no existence
  check, `""` when the section has no such key: every row carries a string.
- **`assignments`** is the instances this model profile is given to, as the record of
  `assignments.ini` speaks of it: one entry per section of the record that the manager lists, the
  instance as the record spells it, sorted by instance with the letter case aside, each entry
  `{"instance", "value", "viaAll", "effective"}`: `value` the section's key of its own, or the `*` as
  that section spells it when it has none, `viaAll` when the `*` speaks for the model profile,
  `effective` where the render gives it to the instance, the three as
  [`pai_core::assign::rows`](#the-association-routes) resolves them. A section that says nothing of
  the model profile, with the `*` and without it, is no entry: a model profile no instance is given
  carries `[]`, and a section naming an instance the manager does not list says nothing about a
  machine that no longer has it. The record and the manager's list are asked on the same connect as
  the two files, so the list carries four calls. A machine with no `assignments.ini` is no failure:
  every row answers `[]`. A record read that fails, and an instances list answered as a failure,
  fail the whole list, never answering empty `assignments` lists, which would tell the page that no
  instance serves anything.
- The order is the GUI's: the switched-on sections, then the switched-off ones, each group in file
  order; an id in both files is listed once, as on. The web UI does not sort.
- llama-server's global `[*]` is not a model profile, it is the defaults every model profile starts from, so it is
  never a row, in either file, and the sections beside it keep their place. The switch answers 404
  for it ([below](#the-switch)).
- A missing file is an empty group, not an error: no `models.ini` answers `[]`.

| Status | Body | When |
| --- | --- | --- |
| 200 | the rows, `[]` when there is no model profile | the read answered |
| 401 | `session-required` | no session, the manager not asked |
| 403 | `session-not-administrator` | the manager refused the call for that user; the session ends |
| 503 | `client-unavailable` | no manager, or one that outlives the bff's 5 s read deadline |
| 502 | `client-other` | anything else, never the manager's detail: among them a record of `assignments.ini` the manager answers as unreadable, and an instances list answered as a failure |

### The switch

The body is `{"enabled": true|false}`. Both files are read, the section is looked for in each, and
if it is in neither, or the name is llama-server's global `[*]`, the answer is 404
`no-model-profile-named` and nothing is written. Otherwise the section moves, unchanged. Every manager call
of the move, the two reads, the record, the two writes, the read back and the states, runs under one bound, the
write's 15 s `WRITE_TIMEOUT` ([above](#saving-the-server-file)), not the reads' 5 s:

- **The file the model profile lands in is written first**, because a model profile in both files reads as
  enabled: switching off writes `models.ini.disabled` and then `models.ini`, switching on writes
  `models.ini` and then the disabled file.
- Each write is a compare and swap (`write_file_if`) expecting the text read in this very request,
  so a write by the configurator, the CLI or a second page in between is refused and nothing moves.
- A `models.ini.disabled` that does not exist cannot be swapped (the swap refuses a missing file):
  it is created by a plain `write_file`, carrying the header the configurator writes for that file
  (`DISABLED_FILE_HEADER` in `crates/app/src/model_profiles.rs`, kept equal by the test
  `the_disabled_header_is_the_configurators_one`). A new `models.ini` needs no header.
- An id already where it is asked for answers 200 and writes nothing.
- The answer is not what the bff computed: after the writes, `models.ini` is read again and
  `enabled` is whether the section is in it. Once both writes landed the move stands: a limit that
  runs out on that read back, or on the states asked after it, and a read back that fails, answer the
  switch's 200, `enabled` the value the move produced and `running` true. A finished move is not a
  timeout.
- **`restart` is the instances the page is offered to restart, and `running` is that list not being
  empty**: the record of `assignments.ini` is read once, and the ids kept are its sections the model
  profile is effective on, in the pool as it stands before the move or as the move leaves it (an
  explicit `on`, or an `inherit` following the pool's own state on either side), whose state is
  `active` or `activating`, sorted. An id the manager does not list is left out before its state is
  asked, a section naming an instance that is gone being a stale record, not a machine, and no state
  is asked at all, and no list, for a model profile effective nowhere: a profile no instance is given
  restarts nothing, and a machine with no record at all says nothing about who serves one, so it
  names no instance and asks no state. An id already where it was asked for names the instances it is
  effective on now, with nothing written.
- A state that cannot be read answers `running` true with `restart: []`, the safe answer after a move
  that stands: at most it costs a needless restart. It is the page's "restart to apply" note: the
  switch starts, stops and restarts nothing.

The moved text goes through the manager's content checks, the configurator's own
(`crates/core/src/checks.rs`, run by `check_contents` in `crates/manager/src/manager/service.rs`):
`models_file` for `models.ini`, `models_disabled_file` for the disabled one, then
`model_profile_ids_free`, which keeps a section that is new to a file from taking an id the other file
holds, so ids stay unique across the pair. A refusal is 422 with the check's key, its values and
`field`, the model profile key for the form to mark: `load-on-startup` for both
`load-on-startup-*` keys, `model` for `model-profile-no-model` and `model-profile-model-missing`
(their args name the model profile, the latter's also the instance and the file, never a key),
`tensor-split` for `tensor-split-format` (its args are the whole value), the key named in the check's
args for the others (so `device-bad-name` marks `device`: its args are the key and the entry), and no
`field` for a refusal about the model profile's name (`invalid-model-profile-id`,
`model-profile-name-taken`) or about the node's GPU memory (`model-profile-over-memory`, its args the
model profile, an instance and two memory figures, none of them a key). The destination is
written first, so a refusal leaves both files as they were.

| Status | Body | When |
| --- | --- | --- |
| 200 | `{"enabled", "running", "restart"}` | the move stands, or there was nothing to do, or the limit ran out on the read back and the states after both writes landed: `enabled` the value the move produced, `running` true and `restart: []` when the states cannot be read |
| 400 | `model-profiles-malformed` | a body that is not `{"enabled": <bool>}`, or not JSON, the manager not asked |
| 401 | `session-required` | no session, the manager not connected to |
| 403 | `session-not-administrator` | the manager refused the read or the write for that user; the session ends |
| 404 | `no-model-profile-named` | neither file has that section, or the name is llama-server's global `*`, not a model profile, for which nothing is moved. `args: [id]` when the id is a name a model profile can carry at all; `args: []` for one the id check refuses (a newline, a bracket, a path, the global `*`) and for a run of dots |
| 409 | `model-profile-changed-elsewhere` | a compare and swap found its file changed, nothing moved. When it answers the second write the model profile is in both files: it reads on, and the next switch of that model profile mends it |
| 413 | `payload-too-large` | a body over the bff's 64 KiB limit ([above](#saving-the-server-file)) |
| 422 | `{"key", "args", "field"}` | the manager refused the moved section, nothing written |
| 503 | `client-unavailable` | no manager, with no write asked yet |
| 504 | `client-timeout` | from the moment a write was asked and its outcome is unknown, the rule the server save shares ([above](#saving-the-server-file)): the handler and its task settle the one atomic state together, so a limit that runs out before any write is 503 and the files are what they were, and one that runs out after both writes landed is the 200 above |
| 502 | `client-other` | anything else, never the manager's detail |

### The model profile page

`GET /api/profiles/model/{id}` (`model_profiles.rs` `read`) is the read of one model profile, the route
`#/profiles/model/<id>` reads (`fe/src/ModelProfilePage.tsx`, reached from the model profiles table's id link): the page
whose save is [below](#the-save). This route only reads. One connect as the
session's user, one read of each file, under the reads'
5 s `MANAGER_TIMEOUT`. The `{id}` is a section name, matched in any letter case, never a path, and
the enabled file is read first: a section in both files answers the `models.ini` copy, as the list
does the same id. The answer is exactly seven fields:

- `id`, `engine`, `engineAvailable`: the list row's, the id as the file spells it.
- `enabled`: which of the two files the section was found in.
- `values`: the section's keys as that file's parser reads them (`ini::parse_sections`), an object of
  strings, no path resolution and no check: the key as the file spells it, the value trimmed and cut
  at the first `;` or `#` (`q8#0` reads as `q8`), a key repeated in the section keeping its last
  line, and two keys differing only in case being two entries. Verbatim on purpose: a model profile the
  manager would refuse at a save is one the page can still show.
- `projectUrl`: the section's `; url = ` comment, `""` when it has none. It is a whole-line comment
  of the section, read by the configurator's reader, now in `pai-core` (`ini::project_url_of` over
  `ini::comment_value`, 36a892d), so the GUI and the page answer one URL: `;` or `#`, any spacing
  around the `=`, the first such comment, the word whole (`; urls are below` is prose).
- `revision`: the server read's keyed SipHash-2-4 of a text, printed as 16 hex digits
  ([above](#the-server-file-of-an-instance)), the text being the file's name, a newline, and the
  section as the file holds it: from its header through its last non-blank line, without that line's
  own ending (`section_revision`). In it: the section's keys, values and comments in the file's
  spelling and order, and which of the two files holds it, so the model profile switched off is another
  model profile to the page. Not in it: a section changed beside it, a section appended to the file, the
  file's final line break, the blank lines after the section's last line. The save consumes it: the
  page sends it back at the `PUT` and a revision that is no longer the section's is 409
  ([below](#the-save)).

| Status | Body | When |
| --- | --- | --- |
| 200 | the seven fields | the read answered |
| 401 | `session-required` | no session, the manager not connected to |
| 403 | `session-not-administrator` | the manager refused the read for that user; the session ends, as the list's |
| 404 | `no-model-profile-named` | neither file has that section, or the name is llama-server's global `*`, never a model profile. `args` as the switch's ([above](#the-switch)): `[id]` for a name a model profile can carry at all, `[]` for one the id check refuses and for a run of dots |
| 503 | `client-unavailable` | no manager, one that went away during the read, or one that outlives the 5 s deadline |
| 502 | `client-other` | anything else, never the manager's detail |

A read runs no content check and writes nothing. `POST` and `DELETE` on the path are 405, and a path
below it, `/api/profiles/model/alpha/*`, is the ROUTER's own 404: no body and no content type, no key, and
no connect to the manager.

### The save

`PUT /api/profiles/model/{id}` (`model_profiles.rs` `save`, 3eb24b2, the design "Saving a preset from the web" in
`ARCHITECTURE.md`) is the model profile page's save: `{"revision", "values"}` in, `{"revision", "running",
"restart"}` out. `values` is an object of the keys the page touched, a string the text to write and `null` the
key to remove, and the section is rewritten in place: the keys the body does not list, the comments
and the rest of the file stand. The keys the body may touch are `checks::WEB_MODEL_PROFILE_KEYS`: the
GPU distribution keys `device`, `split-mode` and `tensor-split` joined them at 66b2bb3 (the design's
G1), the page
having no control for them yet (`fe/briefs/14-gpu-distribution.md`).

The body is read in full before a file is opened, and is refused whole:

- Not `application/json`, not JSON, no `revision` string, no `values` object, a value that is
  neither a string nor `null`, a key outside `checks::WEB_MODEL_PROFILE_KEYS` in its exact spelling:
  400 `model-profiles-malformed`, the manager not asked.
- A value empty, holding a control character beyond a tab, or starting or ending with a Unicode
  space beyond a space or a tab (the writer keeps that byte, the parser reads it away):
  400 `model-profiles-malformed`. Values arrive trimmed of the spaces and tabs no reader keeps.
- A value holding a `;` or a `#` (`checks::comment_markers`): 422 `comment-markers`, the keys in
  `args` sorted, the first of them as `field`.

Then one connect as the signed-in user reads `models.ini` and `models.ini.disabled` (the reads'
5 s), the file order the list's, and answers the first section of that name in the pair; llama-server's
global `[*]` is no model profile, so 404 `no-model-profile-named` as the switch's. The section's revision
([above](#the-model-profile-page)) must equal the body's, else 409 `model-profile-changed-elsewhere` with nothing
written: the page's copy is stale.

The edits go to the file sorted by key, so the bytes written do not depend on the order of the
body's map, through `ini::text::set_key` and `text::remove_key`: the key line in the section's own
spelling, a line that already holds the value left byte for byte, a key the section holds twice
collapsed onto its first occurrence. The whole new text goes to the manager as a compare and swap
against the text that was read, so a file that changed in between is the same 409. The manager runs
its content checks on the write, and its refusal is the 422 shape of the switch ([above](#the-switch)):
its key, its args, and `field`, the key for the form to mark. One line is exempt from its
comment-marker check (26f5413, decision c1): a key line a section of either file holds byte for
byte, id, key and raw value, so an untouched commented line carried over whole is no refusal. A body
whose edits leave the text as it is writes nothing and answers 200.

The 200 carries the section's revision as the file now holds it, `restart` and `running`:
the record of `assignments.ini` is read once the write stands, and `restart` is its sections the
model profile is effective on in the pool as the save leaves it (an explicit `on`, or an `inherit`
following the profile's own state), whose state is `active` or `activating`, sorted; `running` is
that list not being empty. A section naming an instance the manager does not list is left out before
its state is asked, and a model profile served nowhere, a switched-off profile no instance is
explicitly given among them, asks no state at all. A machine with no record at all says nothing about
who serves a profile: `restart: []`, `running` false, and no state and no instance list asked. A
state that cannot be read answers `running` true with `restart: []`,
the safe answer after a write that stands, as the switch's.

| Status | Body | When |
| --- | --- | --- |
| 200 | `{"revision", "running", "restart"}` | the write landed and the manager accepted it, the revision the section as the file now holds it; also a body that asked for the text the file already holds, nothing written; and a limit that ran out after the write landed, `running` true and `restart: []` |
| 400 | `model-profiles-malformed` | the body is not a model profile save, the manager not asked |
| 401 | `session-required` | no session, the manager not asked |
| 403 | `session-not-administrator` | the manager refused the read or the write for that user; the session ends |
| 404 | `no-model-profile-named` | neither file has that section, or the name is llama-server's global `*`. `args` as the switch's ([above](#the-switch)) |
| 409 | `model-profile-changed-elsewhere` | the body's revision is not the section's, or the compare and swap found its file changed: nothing written |
| 413 | `payload-too-large` | a body over the bff's 64 KiB limit ([above](#saving-the-server-file)) |
| 422 | `{"key", "args", "field"}` | the bff's comment-marker check of a value, or the manager's refusal of the new file, among them `model-profile-model-missing` (args the model profile, the instance and the file, `field` `model`: a file the new render puts in effect on an instance is not on the node), `model-profile-over-memory` (args the model profile, the instance, the MiB it needs and the MiB the node has, no `field`: a model profile the write makes larger does not fit the node's usable GPU memory), `device-bad-name` (args the key and the entry, `field` the key: a `device`, `device-draft` or `mmproj-device` entry llama.cpp's parser throws on, judged on the value the preset reader sees, never against the node's probe) and `tensor-split-format` (args the whole value, `field` `tensor-split`: weights the engine's parser throws on) |
| 503 | `client-unavailable` | no manager, with no write asked yet |
| 504 | `client-timeout` | from the moment a write was asked and its outcome is unknown, the rule the server save shares ([above](#saving-the-server-file)): a limit that runs out before any write is 503 and the file is what it was, one that runs out after the write landed is the 200 above. The writes' 15 s are counted from the request, the reads' 5 s spent first taken off |
| 502 | `client-other` | anything else, never the manager's detail |

### A model profile in both files

An id in both files is listed once, as on. Switching it off writes the disabled file (its section is
already there, unchanged, which that file's check accepts) and takes it out of `models.ini`: the
state is mended. The global `*` is the one name that can never be in that state through these
routes: it is not a model profile, the list has no row for it in either file and the switch answers 404
with nothing written, so only the configurator, which still lists `[*]` as a model profile and switches it
off (`PLAN.md` 11b), can put it in the disabled file, from which the model-profile-id check then refuses to
move it back. Read from the code on 2026-11-17, not tested: `PLAN.md` 11b lists the missing tests,
and the plain write that creates a new disabled file (racing a file that appeared in between) is its
other leftover.

### What has run

The bff's own tests only: `tests/model_profiles.rs`, `tests/model_profiles_switch.rs` and `tests/server.rs`, whose
fake manager answers a compare and swap only when the expected text is what it answered for that file
in that
test, and records every write, so the order of the two writes and the text of each are pinned.
Green 2026-11-17 at 594e15e, rerun the same day (23, 48 and 64 tests in the three suites,
`cargo test -p pai-bff --test model_profiles --test model_profiles_switch --test server`); after the
preset to model profile rename, `model_profiles.rs` (24 tests) and `model_profiles_switch.rs` (48)
were green again on 2026-11-18 at 8a2dc5b, with the read, the save and `--lib` (32, 47 and 44), and
`server.rs` was not rerun. The section read adds
`tests/model_profile_read.rs`, over the same fake manager: 32 tests, green 2026-11-17 on the tree at
36a892d, pinning the document's seven fields, the values as the parser holds them, a repeated key's last line, the
url comment, the global `*`, the router's own 404 below the path, and the revision stable across a
neighbour section and a final line break (`cargo test -p pai-bff --test model_profile_read`). The save adds
`tests/model_profile_save.rs`, over the same fake manager: 52 tests (3eb24b2, 26f5413, plus 16e6693's
over-memory 422 carrying no `field`, plus 66b2bb3's four: the GPU distribution keys landing in the
section, a refusal of one marking its control (`device-bad-name` the key it names, `tensor-split-format`
`tensor-split`), a device the probe does not know being saved, and a `;` in a device value staying the
bff's own comment-marker 422), and the body read on
its own in `src/tests/model_profile_body.rs`: 24 tests, pinning the body's shape and its refusals, the key
set, the comment-marker 422 and its `field`, the revision compare and the compare and swap's 409, the
edits in key order and the untouched commented line, and the 200 that writes nothing; both rerun green
2026-11-17 on the tree at c919ccd (`cargo test -p pai-bff --test model_profile_save --lib`), the save
suite at 48 and the whole bff (434) green again at 16e6693 (its report). The pool list's
`assignments`, and the `restart` of the switch and of the save, are pinned in those same suites: 28,
52 and 55 tests, beside the body module's 24, green with the whole bff (583 tests, the count of the
`#[test]` and `#[tokio::test]` attributes of the crate's 21 targets) at 2d9a4c8 (the suite's report,
not a rerun here): the list's four calls and the record's refusals, and which
instances a switch or a save names to restart, a record section naming a gone instance among them.
The listen
address and the `Host` guard are `tests/listen.rs` (17 tests) and `tests/origin.rs` (40), rerun
green 2026-11-19 on the tree at 42e5c23, the guard's re-read of the machine's names included
(`cargo test -p pai-bff --test listen --test origin`); the hand check of a
`0.0.0.0` listen above predates them. The `; url =`
reader moved with it, into `pai-core`, and its two tests went too
(`cargo test -p pai-core -- project_url`). In the list and the switch suites: the list
skipping `[*]` in both files
(`the_global_section_is_not_a_model_profile`), the switch's 404 for it with nothing written
(`switch_for_the_global_section_is_404_and_moves_nothing`), and the 200 for a move whose writes
landed when the read back, or the states, run out. A read back that fails after a landed move, and
the ids `**` and ` * `, have no test (`PLAN.md` 11b). The limits the service runs with, 5 s of read,
15 s of write and the device read's 35 s, are pinned by `the_write_limit_outlasts_the_read_limit`
(`src/tests/timeouts.rs`) and `the_devices_deadline_is_the_probe_bound_plus_five_seconds`
(`tests/node_devices.rs`). The list with its port, tags, server profile and boot flag, and the tags
write, are
`tests/instances.rs` (36 tests: 33 of them green 2026-11-20 inside the whole bff suite (455) at
ff406b8, the two of the boot flag at b5fdf9e, the one of `serverProfile`
(`the_row_carries_the_server_profile_the_manager_names`) with the whole bff (583) at 2d9a4c8); the
rule's refusals of a server profile's tags, and their `field`, are pinned in `src/server.rs`'s own
tests. The server file's name is the record's, for the read and the save, and the 404 that reads
nothing for an id the record does not name or a section naming no profile, are
`the_read_is_the_profile_the_record_names`, `the_save_is_the_profile_the_record_names`,
`a_row_naming_no_profile_is_no_instance` and
`an_id_the_record_does_not_name_is_no_instance_and_asks_no_file` (`tests/server.rs`), green
2026-11-20 at b5fdf9e. The node read is `tests/node.rs` (15 tests), green 2026-11-20 at b5fdf9e inside the whole bff
(467): the document, an empty host name answered as `localhost`, the version files read at each request
(an empty file a null version, a non-file, a file over the cap, a line that is not printable ASCII and a
package of no engine all left out, the backends sorted), `engines: []` with no folder, and 401 with no
session, the manager never asked. The device list read is `tests/node_devices.rs` (12 tests,
66b2bb3), reported green with the whole bff (596) at a1a3458 (its report, not a rerun here): the 401
that connects to nothing, the connect naming the signed-in user and the one `DevicesAs` call, the
document's rows in the probe's order with the probe's moment beside them and the host's `ramMib` and
`cpuThreads` as the manager answered them, a CPU-only node and a node no probe answered answering an
empty list with 200 (the two numbers answered all the same), the 403 that ends the session, the 503
of a manager that outlives the 35 s bound, and the 502 that carries no detail. The pool's five routes
are `tests/server_profiles.rs` (27 tests),
reported green with the whole bff (504) at b535304 (its report, not a rerun here). The `[Server]
Devices` key (R2, 3aaa4dc) is pinned in both suites, the instance's and the pool's: the read carrying
the file's list as one string and `null` when the file names no key, a save of the list landing
verbatim (`none` included), a save of `null` writing no key, a save of another key keeping it, the 422
`device-bad-name` naming `Devices` with nothing written, and the same key's `;`/`#` as 422
`comment-markers` naming `Devices`. Green with the whole bff (605) at 3aaa4dc (its report): on that
tree `tests/server.rs` holds 83 and `tests/server_profiles.rs` 30 test attributes (a count of the
attributes, not a run). **Nothing here has been called against
a real manager**: no signed-in route has (the real install of 2.99.1 on Ubuntu 26.04 answered
`/api/version` and `/api/instances` (that read of 2026-11-14 predates the `Instances` call, which no
real manager has answered), [above](#running-it)), and on Windows the code is type-checked
for the GNU target, never run.

## The association routes

`PUT /api/instances/{id}/profiles/server`, `GET /api/instances/{id}/profiles/model`,
`PUT /api/instances/{id}/profiles/model/{profile}` and `PUT /api/instances/{id}/profiles/model`
(`crates/bff/src/instance_profiles.rs`, 2d9a4c8, `PLAN.md` step 10 item 13a, `ARCHITECTURE.md`'s
slice 15): the server profile one instance runs on, and the model profiles it is given. All four sit
behind sign-in, and the write with the file reads go on a connect **as the signed-in user**, so it is
that user's own membership of `pai`, re-checked by the manager at the call, that answers: 401
`session-required` with no session (no manager connected to, nothing written), 403
`session-not-administrator` and the end of the session when the manager refuses that user. Each reads
the session with `Sessions::current`, so each moves the idle deadline, unlike the four polled reads
([above](#the-session)). No page uses them yet.

What they write is the manager's record `assignments.ini`, in the engine's config directory beside the
two model profile files: one section per instance, one key per model profile, and the key `*` for the
whole pool. The words are `on`, `off`, `inherit` (follow the pool's own switch) and `none` (no key:
the model profile is given to the instance by nothing), and a `*` of `inherit` gives every model
profile of the pool while `none` takes the key away; an explicit key wins over the `*`, and a word
the render does not know is an absent key. The bff never renders: the row's fields come from
[`pai_core::assign::rows`](../core/src/assign.rs), the one rule the manager's render of
`models@<id>.ini` follows, pinned to it.

The record of the instances (`Instances`) and the states behind `running` are asked on a connect of
the bff's **own** account, as `GET /api/instances` does: a refusal there is 502
`client-not-authorized` and the session lives on, the service refusing the service, not the user. The
three writes and every file read go on the user's connect under the write's 15 s `WRITE_TIMEOUT`
([above](#saving-the-server-file)), and the read of the pool under the reads' 5 s, so a write that
takes longer than 5 s is answered at 15 s. These four routes never answer 504: a manager that
outlives the limit, before a write or after one, is 503 `client-unavailable`. The model profile switch
and save keep their own 504, the answer of a write whose outcome is unknown
([above](#the-switch)).

**`PUT /api/instances/{id}/profiles/server`** with `{"serverProfile": <name>}` is the manager's
`SetServerProfile`: the instance's section of `instances.ini` names the `server@<name>.ini` it reads
at its next start. The record is read first, on the bff's own connect, so an id it does not name is
404 with nothing written. Nothing is started, stopped or restarted, and the server profile pool is
untouched: a name the pool lacks is the manager's 404 `client-no-server-profile`, a profile another
instance holds its 409 `client-server-profile-in-use`, and a port or a host the write makes unusable
the 422 `client-address-in-use` or `client-unresolvable-host`, each with `field: "serverProfile"`.
The answer is `{"running"}`, the instance's state read after the write: true only when the record
named another profile (a record naming none is a change) and the instance is up, the page's offer to
restart an instance whose record changed under it. A record that already names that profile answers
false and asks no state, and a record, or a state, that cannot be read is the safe true.

**`GET /api/instances/{id}/profiles/model`** is the pool seen from one instance: the list's rows in
the list's own order ([above](#the-model-profile-routes)), each with `value`, `viaAll` and `effective`
as its instance's section gives it, beside `all`, the section's `*` as the page shows it (`inherit`
when the section carries the key, `none` when it does not). Four calls on one connect: the instances
list, then
`assignments.ini`, `models.ini` and `models.ini.disabled` as the user, so an id the record does not
name is 404 with no file read. An instance with no section answers `all: "none"` and every row
`value: "none"` with `viaAll` and `effective` false.

**`PUT /api/instances/{id}/profiles/model/{profile}`** with `{"value": <word>}` writes one key of
that section and **`PUT /api/instances/{id}/profiles/model`** with `{"all": inherit|none}` writes its
`*`, the whole pool; the `*` route answers only `{"running"}`. The word is the bff's own check, before
any call: outside the four (the per-profile route) or the two (the `*` route) it is 422
`model-profile-bad-value` with the word in `args` and `field` `value` or `all`, and the path's `*`,
llama-server's global section, is 404 `no-model-profile-named` with the manager asked nothing. Both
answer on one rule:
- `running` is the instance's state, asked only when the write changed the model profiles the
  instance is given: the ids effective on it before and after are compared, and the same set answers
  false with no state asked, whatever the instance's state.
- the per-profile route answers `effective` too, that row read back from the record the manager
  wrote, so the page sees what the word left the model profile on (a record that cannot be read back
  leaves the text the write would have left, the manager's own rule for the key). No revision is sent
  and 409 never answers: the last write wins, as on the tags.
- a record that cannot be read back leaves the write standing with its after side unknown, and
  `running` true, the safe answer.

| Status | Body | When |
| --- | --- | --- |
| 200 | `{"running"}`, plus `effective` on the per-profile write | the write landed, or nothing changed; and a record unreadable before the write, or one that cannot be read back after it, `running` true |
| 400 | `server-profile-malformed`, `assignment-malformed` | the body is not `{"serverProfile": <string>}` on the server route, and not `{"value"}` / `{"all"}` with a string on the model ones, the manager not asked (the JSON body rule of the server save, [above](#saving-the-server-file)) |
| 401 | `session-required` | no session, no manager connected to, nothing written |
| 403 | `session-not-administrator` | the manager refused the user's connect, read or write; the session ends |
| 404 | `client-no-instance` | the record of the instances does not name the id, nothing written, no file read |
| 404 | `no-model-profile-named` | the path's `*` is not a model profile (`args: ["*"]`, the manager asked nothing), or the manager refused a model profile its pool does not hold (`args` the name) |
| 404 | `client-no-server-profile` | the server profile the body names is not in the pool, `args` the name |
| 409 | `client-server-profile-in-use` | another instance holds that server profile, `args` the profile and the instance |
| 422 | `model-profile-bad-value` | a word the route does not write, `args` the word, `field` `value` or `all`, before any call |
| 422 | `client-address-in-use`, `client-unresolvable-host` | the server profile the write selects makes the instance's address unusable (`args` the port) or its host unresolvable (`args` the host), `field` `serverProfile` |
| 422 | `{"key", "args", "field"}` | the manager's check refused the file it was asked to write, nothing written: among them `load-on-startup-over-max`, `field` `serverProfile` on the server route and no `field` on the model ones |
| 502 | `client-not-authorized` | the manager refused the record of the instances, asked before the write, on the bff's own account; the session lives on, nothing written. A refused state read after a write that stands is `running` true |
| 502 | `client-other` | anything else, never the manager's detail |
| 503 | `client-unavailable` | no manager, or one that outlives the route's limit, the reads' 5 s and the writes' 15 s |

The four routes, their principals and every status key are `tests/instance_profiles.rs` (49 tests),
green at 2d9a4c8 (the tester's report, not a rerun here). **Nothing
here has been called against a real manager.**

## Creating and removing an instance

The two writes that make an instance and take its record away, each on one connect as the signed-in
user, under the write's 15 s ([above](#the-instances-from-the-manager)). The manager is the only
writer of the records, and it is the one that does the work: a create appends the section of
`instances.ini`, writes the render `models@<id>.ini` off the pool, and drops the leftovers of a
same-named instance removed before the manager existed; a removal stops asking the service manager
about the instance, and removes its `assignments.ini` section, its render and its record section,
leaving the server profile it named in the pool. (`CreateInstance` / `RemoveInstance`, their checks
and their order: `crates/config/README.md` "Through the manager (Linux, Windows)".)

| Route | Answers |
| --- | --- |
| `POST /api/instances` | `{"engine", "id", "serverProfile"}` in, three strings. 201 `{"id"}` with the id the body sent and `args: []`: the instance has no model profile, its start at boot is off, and nothing is started. 400 `instance-malformed` with no args, when the body is not JSON, not an object, missing one of the three keys, or one of them is not text; the manager is not connected to. A body over 64 KiB is the extractor's 413 `payload-too-large`, before the session is read ([above](#saving-the-server-file)). 422 `unknown-engine` `[engine]` with `field` `engine`, 422 `instance-bad-id` `[id]` with `field` `id` (the instance-id rule: 1 to 32 of `a-z`, `0-9` and `-`, starting with a letter or a digit), 422 `client-address-in-use` `[the other instance]` with `field` `serverProfile` when the address of the profile asked for is the address of another instance's server profile, 422 `client-unresolvable-host` `[host]` with `field` `serverProfile` when its host is a name this machine cannot look up. 404 `client-no-server-profile` `[name]` for a profile whose `server@<name>.ini` is not there, 409 `client-server-profile-in-use` `[profile, instance]` for a profile a running instance has, 409 `instance-exists` `[id]` for an id the record already has a section for. Any other check the manager ran keeps its own key and values, as a 422 with no `field`. 503 `client-unavailable` with no manager, including one that outlives the 15 s. 502 `client-service-failed` with no args when the service manager refused, 502 `client-other` with no args for anything else the manager failed as. |
| `DELETE /api/instances/{id}` | 204 with no body. 409 `instance-protected` `[id]` for `default`, 409 `instance-running` `[id]` for an instance that is up, 404 `client-no-instance` with no args for an id the record does not name and for an id that is not a name at all (the manager's `InvalidId`), 503 `client-unavailable`, and the two 502s as above. A removal is refused while the instance runs, so the caller stops it first; the fe does not ask, its Remove is offered for a stopped one. |

- **The checks are the manager's and the bff keeps the key.** `instance-bad-id`, `instance-exists`,
  `instance-protected`, `instance-running` and `unknown-engine` are keys of
  `crates/core/strings/cli.toml`, and the manager's refusal names in `args` the value it refused. A
  refusal of the `serverProfile` value is 422 with a `field`, so the fe marks the field of the create
  form; a refusal about the instance (an id already in the record, a profile a running instance holds)
  is 409 with no `field`, because it is not about a value of the form.
- **Both write as the signed-in user, and a refusal of the user ends the session**, exactly as the
  server save ([above](#saving-the-server-file)): the session is the proof, the principal rides the
  call, and the same 403 `session-not-administrator` is answered for an account the manager no longer
  accepts, ending the session.
- **A write that goes past the 15 s is 503 `client-unavailable` with no args, never 504, and the
  outcome is unknown**: the bff cannot know whether the manager wrote. A retried create of an instance
  the first one did write is 409 `instance-exists`, which is how the ambiguity shows, and the read of
  the list is the truth.
- **The reply's id is the id the body sent**, read before the manager is asked: a create answers no
  document, and the page re-reads the list.
- **`crates/bff/tests/instances.rs` is 49 tests** over these two routes and the `removable` of the
  list: the shape of the replies, the exact `args` of each key, the 422 with a `field` and the 409
  without one, the 503 of a manager that stops answering mid-write, and the 403 that ends the session.
  A refusal test asserts the `args` and, where the key has one, the `field`: that is all the fe has to
  show. **Nothing here has been called against a real manager.**

## The web UI (`fe/`)

React + Vite in TypeScript, exact versions in `fe/package.json` and a committed
`package-lock.json`, styled with plain CSS (`fe/src/index.css`). Its design guide is `fe/DESIGN.md`,
kept by the `designer`; the slice briefs and the API contract the fe is built against
(`api-contract.md`) are in `fe/briefs/`.

**A shell with hash routes** (`fe/src/App.tsx`, `fe/src/route.ts`), since the bff serves only `/`
and `/assets/`: a skip link, a banner (Sign in, or the user and Sign out, no role word: fe slice R4,
1827023), a nav (behind a Menu button below 960px; the instances listed once signed in, under a heading
named by the host, fe slice N: `nav-instances`; under a **Profiles** heading the **Model profiles**
entry, fe slice 05b: `nav-profiles`, `nav-model-profiles`; the **Server** entry waits on the server
profile page, `fe/briefs/05b-profiles-nav.md`; and last the **About** entry, the info icon, `nav-about`,
fe slice N) and the route's page, whose heading takes the focus and names the document title on a
change. Each page reads
its data once when it opens and never shows a reply's text. The shell reads `GET /api/node` once per
session (`readNode` in `fe/src/node.ts`): the host name it answers names the nav's instance group and the
Overview's section, the plain words standing when the read has nothing to say, and a failure signs
nobody out; a sign-out and a sign-in read it again. An old `#/models` bookmark is rewritten to
`#/profiles/model` (`legacyHref`, 05b.8 and 05b.9), the rest of the address copied byte for byte.

| Route | Page | Who |
| --- | --- | --- |
| `#/` | The Overview: the version (`/api/version`, a fixed message on any failure) and the instances (`/api/instances`) as a table of instance, each a link to its page, and state (the five known states as Running, Stopped, Starting, Stopping, Failed; another as `Unknown (<name>)`), with an engine column only when they run on more than one engine; a message when there is none. The section is named by the host the shell read. Each tile carries its facts with the list, no read of its own: the port, the tags, and a Start at boot row from `boot`, absent when that is `null` (fe slice 12 T1, boot from 13 N.1). Both reads are 401 with no session, so an unsigned visitor has nothing to show (user, 2026-11-19; the sign-in-only shell the web UI shows here is its own slice, `fe/briefs/09-signin-only.md`, uncommitted when this was written) | signed in; otherwise the sign-in form in its place |
| `#/sign-in` | The sign-in form (`POST /api/session`) | anyone |
| `#/instances/<id>` | The instance's id, engine and state (from `/api/instances`, kept live), Start, Stop and Restart (`POST /api/instances/<id>/start`, `/stop`, `/restart`), Start at boot (`GET` and `PUT /api/instances/<id>/boot`, both behind the session), then its server file (`GET /api/instances/<id>/server`) as the whole Server form of `pai-config`: editable, every signed-in session being a `pai` member (the administrator/user split is gone, fe slice R4, 1827023), with the GUI's field dependencies, a save bar (Save and Discard, a status line, an alert for a failure) and a save that sends `PUT /api/instances/<id>/server` (`fe/src/ServerForm.tsx`) | signed in; otherwise the sign-in form in its place |
| `#/profiles/model` | The model profiles of every engine (`GET /api/profiles/model`) as a table of model profile, engine (with "(not available)" when its engine is not) and status On or Off, each row with a switch named by its model profile that sends `PUT /api/profiles/model/<id>/enabled` at once, one write at a time; the status line says what was switched and, when an instance of the engine runs, to restart it; a failure shows its message above the table and the switch takes the state read again (`fe/src/ModelProfiles.tsx`) | signed in; otherwise the sign-in form in its place |
| `#/profiles/model/<id>` | One model profile, read once from `GET /api/profiles/model/<id>`, reached from the row's id link in that table (`fe/src/ModelProfilePage.tsx`): the way back to the list, the id as the heading, the Engine field and the Assets card as text; of the settings cards the Embeddings, Resource / context, KV cache (its first part) and Sampling overrides ones are editable (fe slices 2h.1, 2h.4, 2h.5: 57e5573, 39529ef, 40d19a7, dc1ff69) and the rest read-only with every control disabled, a save that sends `PUT /api/profiles/model/<id>` with the page's revision and the keys that changed (2h.2, 9ea579d) | signed in; otherwise the sign-in form in its place |
| `#/about` | The host and what is installed on it, read again at every arrival from `GET /api/node` (an engine upgrade does not restart the web service, `fe/src/About.tsx`): the page heading "About", the host's name as the section heading, the framework's version, then one section per installed engine with its version and a row per GPU runtime package (the three known suffixes by their own word, any other as sent). A `null` version shows as Unknown, an engine with no runtime as None, CPU only, and `engines: []` as a paragraph that there is no engine; a read with nothing to show is a fixed message, and a 401 ends the session | signed in; otherwise the sign-in form in its place |
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
- **Every model profile call of the contract is served**: the page's read and its save
  ([above](#the-save)), and the Model profiles page's list and switch ([above](#the-model-profile-routes)); what is
  open is the fe's own save (`PLAN.md` item 2h). The fe is tested against that contract with mocked
  replies only, never against a real bff, so what those pages show takes a run on a machine. Everything the
  instance page makes is served: the
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
the build embedded (`PAI_BFF_FE_DIR`); `tests/instances.rs` drives `/api/instances` and its tags
write (`PUT /api/instances/{id}/tags`) against a fake
manager (`router_with`, a short timeout); `tests/session.rs` drives `/api/session` through
`router_with_session` with a fake authenticator and a clock the test moves;
`tests/authenticator.rs` drives `SystemAuthenticator` over a fake manager client, the same `Connect`
the bff's other routes use (never PAM, `LogonUserW`, the bus or the pipe), and pins how the
manager's two answers become `Identity` or `AuthError`; `tests/server.rs` drives
`GET` and `PUT /api/instances/{id}/server` the same way, over a fake manager; `tests/model_profiles.rs`,
`tests/model_profiles_switch.rs`, `tests/model_profile_read.rs` and `tests/model_profile_save.rs` drive the model profile list,
the switch, the section read and the save over the same fake manager, whose
compare and swap lands only when the `expected` text is what the fake answered for that file in that
test (a missing file answers `Changed`, as the real manager does) and every write is recorded as
`(name, contents, expected)`, so the order of the two writes, the whole text of each and the channel
each used are pinned; `src/tests/model_profile_body.rs` reads a save's body on its own, with the file's text
handed in and no manager; `tests/listen.rs` reads `Listen` and `Hosts` from an INI text and a tempdir
and builds the machine's names from a host name and a `resolv.conf` fixture, never the real host name
nor the real `/etc/resolv.conf`, and `tests/origin.rs` drives the `Host` guard and the payload cap
through `router_with_session` and `router_with_session_and_hosts`
(a name list the test hands, the whole list, as `[Bff] Hosts` gives it) with the same fake manager,
and the re-read of the machine's names through `router_with_session_and_host_source`, whose source
the test changes (`a_domain_that_arrives_after_the_start_is_answered`,
`a_fixed_hosts_list_never_reads_the_machine`);
`tests/pai_bff_binary.rs` runs the binary with
`PRIVATE_AI_DATA_ROOT` on the child and an ephemeral port, SIGTERM and SIGINT included; `tests/node.rs`
drives `GET /api/node` through `router_with_session_and_versions_dir` with a versions directory of the
test (a tempdir, never a real install) and a fake manager it asserts is never called;
`tests/node_devices.rs` drives `GET /api/node/devices` through `router_with_session` over a fake manager
client whose `devices()` answers a fixed list and a fixed host inventory (numbers no real host has, so a
document carrying them carries the fake's answer), is delayed past the route's bound for the 503, and
pins the principal of each connect, never a bus, pipe, PAM, LogonUser or an engine;
`tests/server_profiles.rs` drives the five pool routes through `router_with_session` over the same
fake manager and a fake authenticator (27 tests, b535304): the session gate of all five and the idle
deadline each moves, the 403 that ends the session, the list as the manager's rows in its order, the
create's 201, its 400 that asks nothing, its 422 fields and the port's edges, the delete's 204 and its
409 naming the instance, the document of a used and of an unused profile with the record's own failure,
the save of the profile's own file answering `running` false, true on a state read that failed and true
on a record read that failed after the write, and the instance's Server route answering as before;
`src/tests/deps_lint.rs` keeps the GUI
packages (Slint) out of its dependency tree. It is in the Windows GNU type check ("Commands" in
[AGENTS.md](../../AGENTS.md#commands)); nothing of it has run on Windows.
