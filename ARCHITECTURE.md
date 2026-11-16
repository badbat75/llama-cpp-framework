# Architecture

The map of the framework's components, how they talk, who may do what, and what data each one owns;
then the design records. Kept by the `architect` agent (`.opencode/agents/architect.md`). It points to
the guides that hold the details rather than copying them: when this page and a guide disagree, the
guide and the code win, and this page is fixed.

Part 1 is the baseline, what exists on 2026-11-15. Part 2 is the design of `PLAN.md` step 14 (R13, a
super-manager), proposed and waiting for the user's acceptance.

## Part 1: the baseline

### Components

```text
  browser ──HTTP (loopback)──> pai-bff ─┐                     ┌─> llama.cpp@<id>  (Linux unit)
                                (fe/)   │  D-Bus + polkit     │   llama.cpp_<id>  (Windows service)
  pai-config (GUI) ─────────────────────┼─────────────────> pai-manager ──systemd / SCM──┘
  pai (CLI) ────────────────────────────┘  named pipe + ACL   (root / LocalSystem)
                                                                  │ reads, writes
                                                                  v
                                                      the engine's instance files
```

| Component | What it is | Runs as | Guide |
| --- | --- | --- | --- |
| `pai-manager` (`crates/manager`) | The `private-ai-manager` service: lists, starts, stops the engine instances, reads and writes their files, runs every content check, decides who may do what. Keeps no state of its own: the service manager's state is the truth | root (Linux), LocalSystem (Windows); not on macOS | [crates/config/README.md](crates/config/README.md#through-the-manager-linux-windows) "Through the manager" |
| Engine instances | One OS service per instance id: `llama.cpp@<id>.service` (template unit shipped by the `llama.cpp` package), `llama.cpp_<id>` on Windows (created by the manager). Each runs `pai-manager run-instance <id>`, which reads `server@<id>.ini` and execs llama-server | `private-ai` (Linux, in `render`, `video`), `NT SERVICE\llama.cpp_<id>` (Windows) | `PLAN.md` step 1, [installer/AGENTS.md](installer/AGENTS.md) |
| `pai-bff` (`crates/bff`) + `fe/` | The web service: the API and the embedded React web UI. Loopback only until authentication and TLS exist | `private-ai-bff` (static system user, sandboxed), `NT SERVICE\private-ai-bff` | [crates/bff/README.md](crates/bff/README.md), `PLAN.md` step 10 |
| `pai-config` (`crates/config`) | The Slint GUI; a thin client of the manager on Linux and Windows, spawns llama-server itself on macOS | the desktop user | [crates/config/AGENTS.md](crates/config/AGENTS.md) |
| `pai` (`crates/cli`) | The same operations headless | the user | [crates/config/README.md](crates/config/README.md) |
| `pai-core`, `pai-app` | Shared libraries: paths, INI, checks (`checks.rs`), the manager client (`client.rs`); the configurator's logic without a GUI | (linked) | [crates/config/README.md](crates/config/README.md) |

Packages (R12, `PLAN.md` step 11): the engine is its own package at its own version (`llama.cpp`
with the instance unit, the runtimes `llama.cpp-rocm|cuda|vulkan`; on Windows llama.cpp's own MSI,
which owns the instance services' lifecycle on repair and uninstall). The framework's
`private-ai-framework-core` (manager, its unit, D-Bus and polkit policies, sysusers), `-bff`, `-cli`,
`-config`; on Windows one framework MSI. macOS: split `.pkg`, no manager, no bff. Details:
[docs/build-packaging.md](docs/build-packaging.md), [installer/AGENTS.md](installer/AGENTS.md), the
Linux system files in `xtask/src/linux/system_files.rs`.

### Interfaces

- **Linux: D-Bus `org.privateai.Manager1`** on the system bus. Only root owns the name, everyone may
  send to it; polkit decides in each call (`dbus_policy()`, `polkit_policy()` in
  `xtask/src/linux/system_files.rs`). The caller is its unique bus name.
- **Windows: the named pipe `\\.\pipe\private-ai-manager`**, remote clients rejected, network logons
  denied by its SDDL; SYSTEM and Administrators full, interactive users and the bff's service SID
  read and write (`PIPE_SDDL` in `crates/manager/src/manager/windows.rs`). The caller is the SID of
  the client's token.
- **The manager protocol**: one request, one reply; on the pipe a u32 little-endian length and a JSON
  body of at most 1 MiB, the D-Bus method and error names (`crates/core/src/client/pipe.rs` header,
  the server in `crates/manager/src/manager/pipe.rs`). Methods: `ListInstances`, `Status`, `Start`,
  `Stop`, `Restart`, `ReadFile`, `WriteFile`, `BootEnabled`, `SetBootEnabled`, `Authenticate`, the
  `<Method>As` forms of the six guarded ones and `Version` (protocol 1; "Acting for a signed-in user"
  below). The client works over any `Read + Write` stream.
- **The bff's HTTP API**: the routes and error shape `{key, args}` in
  [crates/bff/README.md](crates/bff/README.md); the contract the web UI is built against in
  `fe/briefs/api-contract.md`.
- **The instance files**: `server@<id>.ini`, `presets.ini`, `presets.ini.disabled` in the engine's
  configuration directory, written only by the manager; read by `run-instance` and by llama-server
  (`--models-preset`).

### Identities and trust boundaries

| Identity | Holds | Trusted for |
| --- | --- | --- |
| The manager (root, LocalSystem) | Every right on the machine | Everything; the one authority over the instance files |
| `private-ai` / `NT SERVICE\llama.cpp_<id>` | Reads the instance files (group `private-ai`, mode 0640 for `server@<id>.ini`), its models and state | Running one engine; reaches no manager call that changes anything |
| `private-ai-bff` / `NT SERVICE\private-ai-bff` | No file but `bff.ini`; the manager's unauthenticated reads; no right of its own | Reads; asking the manager to check a password (`Authenticate`); naming the signed-in user on guarded calls (the `As` methods), the manager re-checking that user's `pai` membership at each call. The manager answers both only to this account ("Acting for a signed-in user") |
| A desktop user | Their own tree (`settings.ini`, `bench.db`) | `control` (start, stop, restart) in the active session without a password (Linux), as an interactive user (Windows) |
| An administrator | | `configure` (write a file, start at boot, read the API key) |

**Who is an administrator** (user, 2026-11-15, `PLAN.md` step 10 slice 6): a member of the group `pai`,
created empty by the packages, on every client. On Linux the shipped action `configure` defaults to
`no` and the rules file `50-private-ai.rules` answers `auth_self_keep` to a member of `pai`, `no` to
everyone else (`polkit_policy()`, `polkit_rules()` in `xtask/src/linux/system_files.rs`;
`subject.isInGroup`, [polkit(8)](https://polkit.pages.freedesktop.org/polkit/polkit.8.html)). On
Windows `configure` is a token in `pai` or the local system account, and an Administrators member
outside `pai` gets `control` only (`allowed` in `crates/manager/src/manager/windows.rs`). The web UI
is for administrators only: a sign-in succeeds only for a `pai` member.

Boundaries a call crosses: the browser to the bff (HTTP, a session cookie; loopback only); the bff,
`pai-config` and `pai` to the manager (the bus or the pipe, the caller identified by the OS, never by
anything it sends); the manager to systemd or the SCM; the instance to its files and GPUs. Nothing
crosses the machine today.

### Data and where it lives

| Data | Owner | Where (Linux / Windows) |
| --- | --- | --- |
| Instance files | the manager | `/etc/llama.cpp/` / `%ProgramData%\llama.cpp\config\` |
| The bff's config | the administrator | `/etc/private-ai/bff.ini` / `%ProgramData%\private-ai\config\bff.ini` |
| Instance state (run, boot) | systemd, the SCM | the service manager |
| Instance logs | the instance | the journal / `%ProgramData%\llama.cpp\logs\` |
| Models, slot snapshots | the instance's account | `/var/lib/...` per the shipped defaults / `%ProgramData%\llama.cpp\models`, `state` |
| `settings.ini`, `bench.db` | the desktop user | `~/.config/private-ai/`, `~/.local/share/private-ai/` / `%LOCALAPPDATA%\private-ai\` |
| Web sessions | the bff, in memory | lost at restart, or ended by sign-out, idling out or the manager's first refusal of the user |
| The record of who acted | the manager | the journal of `private-ai-manager` (its stderr) / `%ProgramData%\private-ai\logs\private-ai-manager.log` |
| Web users' shared state (preferences, `bench.db`, tokens) | decided machine-wide, place not planned (`PLAN.md` step 10) | |

The exact paths and modes are in `AGENTS.md` "Key conventions" and "Through the manager".

### Where the web sign-in stands (2026-11-16)

Shipped: `Authenticate(user, password) -> (ok, can_configure)` in the manager, answered only to the
bff's account (`Authority::is_bff`: the bus name's uid on Linux, the service SID on Windows), the
password checked through PAM service `private-ai` (`crates/manager/src/manager/accounts.rs`) or
`LogonUser` (`windows/logon.rs`), `can_configure` from membership of `pai` (`PasswordChecker::is_admin`
in `service.rs`); the packaging (part 2b) and the bff's call (part 2c, de07ddf: `SystemAuthenticator`,
every session a `pai` member, `session::Identity { user }`); and, below, every route behind a session
calling the manager for the session's user, so `GET /api/instances/{id}/server` answers a member.

### Acting for a signed-in user (step 14 slice 0 and step 10 slice 6b part 3)

Designed 2026-11-15, **built** 2026-11-16 (adab61c the core, 1d2e12c the pipe, b2f3777 D-Bus, c03d911
and 4216745 the log, then the bff's connect), reviewed against this text by the architect; the
Windows side is type-checked only. One design serves both: the bff names the signed-in user on each
guarded call, and the super-manager will later name its user the same way (Part 2, slice 4).

**The model.** The manager's core stops taking a bare `Caller` for a guarded call and takes a
`Requester { caller: Caller, principal: Option<Principal> }` (`service.rs`), `Principal(String)` a user
name some trusted caller vouches for. `Caller` stays the transport's identity (bus name, token SID);
the principal is never folded into it, and a later enrolled peer is one more kind of caller, not a
third way to name a user.

**How it travels.** A principal is carried only by new methods whose name says so, the same names on
both transports:

| D-Bus (`org.privateai.Manager1`) | Pipe request | Reply |
| --- | --- | --- |
| `StartAs(s principal, s id)`, `StopAs`, `RestartAs` (same) | `{"method":"StartAs","principal":"alice","id":"default"}` | as `Start` |
| `ReadFileAs(s principal, s name) -> (b exists, s text, b withheld)` | `{"method":"ReadFileAs","principal":..,"name":..}` | as `ReadFile` |
| `WriteFileAs(s principal, s name, s contents, b keep_api_key)` | `{"method":"WriteFileAs","principal":..,"name":..,"contents":..,"keep_api_key":..}` | as `WriteFile` |
| `SetBootEnabledAs(s principal, s id, b enabled)` | `{"method":"SetBootEnabledAs","principal":..,"id":..,"enabled":..}` | as `SetBootEnabled` |
| `Version() -> (s version, u protocol)` | `{"method":"Version"}` | `{"ok":{"version":"<framework>","protocol":1}}` |

`ListInstances`, `Status`, `BootEnabled` and `Authenticate` get no `As` form: they need no right, or
answer the bff alone. On the pipe a `principal` field on any other method is `BadRequest`, and an `As`
method without one is `BadRequest` too (a manager older than protocol 1 ignores unknown fields, which is
why the principal travels in the method's name).

| Option for the principal | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. New `<Method>As` methods, same names on both transports** | Six one-line methods per face; the existing methods and every desktop client untouched | None found: an old manager answers `UnknownMethod` (D-Bus) or `BadRequest` (pipe), so the principal can never be dropped | **Chosen** |
| B. An extra argument on the existing methods | Every client changes at once; an empty string as "no principal" is a sentinel | A mixed install fails every call | Rejected |
| C. An `a{sv}` options dict (D-Bus), an optional field (pipe) | One generic shape | An older pipe server ignores the unknown field and runs the call as the bff: safe today only because the bff has no right, silent escalation the day it gets one | Rejected |
| D. A ticket the manager issues at `Authenticate`, sent instead of a name | Session state in a manager that keeps none; lost at its restart | A compromised bff could act only for users who sign in while it is compromised, but it captures their passwords then anyway | Not now |

**Who may send a principal.** Only the bff's account, as for `Authenticate` (`Authority::is_bff`: the
uid behind the bus name, `GetConnectionUnixUser`, against `private-ai-bff`'s, both looked up per
call; on Windows the connection token's SID against the bff's service SID). Anyone else sending an
`As` method is refused `NotAuthorized` before anything else is asked, logged, and nothing is done:
refused, never downgraded to a call as the sender.

**What the manager checks for a principal**, in this order, each failure `NotAuthorized` with
nothing done and the later checks not asked:

1. the caller is the bff (above);
2. the name is well formed: 1 to 256 bytes (the bff's own `MAX_USER_BYTES`), no control character
   (so no NUL, no newline in a log line);
3. the principal is a member of `pai` **now**: `PasswordChecker::is_admin(principal)`, the same
   lookup `Authenticate` uses, at every call, never cached by the manager, never taken from the bff.
   Linux: `getgrouplist(3)` over NSS (`accounts::group_names`); Windows: `NetUserGetLocalGroups` with
   `LG_INCLUDE_INDIRECT` (`windows/logon.rs`). A failed lookup or an unknown account is false.

Both `control` and `configure` need membership (the web UI is for administrators only, user
2026-11-15), so the action does not change the answer; a principal's `ReadFileAs` of an instance file
is never withheld (`withheld: false`). polkit is never asked for a principal: its subjects are a
process, a session or a bus name
([polkit Authority interface](https://www.freedesktop.org/software/polkit/docs/latest/eggdbus-interface-org.freedesktop.PolicyKit1.Authority.html),
"Subject"), and the only one at hand is the bff's, so asking would authorize the bff, not the user.
The password polkit's `auth_self_keep` would ask was checked by the manager at sign-in; the session
(30 minutes idle) stands in for polkit's keep.

**A principal call does not need the bff's own rights**, and the bff keeps none: the right is the
pair (the bff's account, a current `pai` member). Requiring both would mean granting the bff
`control`/`configure` on its own, which a call without a principal could then use.

**Fail closed.** A user removed from `pai` is refused at their next call, and the bff ends that
session at the first refusal: the route answers 403 `client-not-authorized` and the same cookie is
then 401 `session-required` (user, 2026-11-16, reversing this design's earlier "the session lives on").
Only the manager's `NotAuthorized` ends it; an unavailable or slow manager, an unknown instance or any
other failure keeps the session, being the bff's news and not a refusal of the user. A deleted account
fails the lookup. A site caching NSS (`nscd`, `sssd`) delays
a removal by that cache's lifetime (nscd.conf(5), sssd.conf(5)): the manager adds no cache of its own.
An old manager refuses every `As` method (the bff answers 502 `client-other`); the Linux packages pin
`-bff` to `-core` at the same version and Windows ships one framework MSI, so that skew needs a
broken install.

**Version.** `Version` needs no right; `version` is the manager's framework version, `protocol` an
integer that grows with each additive change (1 = the `As` methods and `Version`; the TLS major of
Part 2 rides in ALPN). The client side is an inherent method on `DbusClient` and `PipeClient`, not on
`ManagerClient`: `version() -> Result<Option<ManagerVersion>, ClientError>`, `None` for a manager
older than the method (D-Bus `org.freedesktop.DBus.Error.UnknownMethod`; pipe `BadRequest`, after which
the server has closed the stream and that client is spent: open a new one). Nothing calls it in
production until the super-manager; the bff does not (same-version packages).

**The log of who acted.** Every guarded call, done or refused, writes one record through a seam
(`Manager::with_log`, an `ActionLog` trait, a fake in tests): `action`, `target` (id or file name),
`caller`, `principal` (`-` when none), `outcome` (`ok`, `refused`, or the error name). Values are
written with Rust's `{:?}` quoting, so nothing in a name can forge a record; never a password or file
contents. A sink never fails a call: a record it cannot write is lost, the action stands
(`crates/manager/src/manager/action_log.rs`). Linux: one line on stderr (`WriterLog`), which the unit
sends to the journal. Windows: one append write per record to
`%ProgramData%\private-ai\logs\private-ai-manager.log` (`FileActionLog`, `manager_log_path`), the
service having no console; the manager creates `logs\` at its first record, which inherits the
protected DACL the framework's MSI puts on `%ProgramData%\private-ai` (SYSTEM and Administrators full,
Users read: `MachineDataRoot` in `installer/private-ai-framework.wxs`), so any local user reads the
records and only SYSTEM and Administrators write them; a user cannot pre-create `logs\` to redirect
the manager's writes. The MSI does not know the folder, so the file outlives an uninstall. **No size
cap or rotation yet**: it grows with every guarded call. A record, read by administrators and scripts,
so a literal format, not a strings-file text (as the comments written into `server.ini`).

**The client and the bff.** `ManagerClient` is unchanged; `DbusClient` and `PipeClient` gain
`acting_for(user)`, a client bound to a principal whose guarded methods send the `As` forms (its
unguarded ones are unchanged), so `pai-config`, `pai` and `pai-app` change nothing. The bff's `Connect`
becomes `Fn(Option<&str>) -> Result<Client, ClientError>`: `system_connect(Some(user))` returns the
bound client. The rule for routes: **every route behind a session connects as the session's user**
(`Sessions::current`, never a name from the request), so a removed member is refused at once;
session-free routes (`/api/instances`, GET boot) and the authenticator connect with `None`, as the bff.
Built: `Connect` and `system_connect` in `crates/bff/src/lib.rs`, the server read in
`crates/bff/src/server.rs` (the only route behind a session today), `Sessions::end` in
`crates/bff/src/session.rs`, shared with sign-out. A route added behind a session follows the same
two rules: connect with the session's user, end the session on `NotAuthorized`.

**Trust boundaries crossed.** Browser to bff: the session cookie. Bff to manager: the kernel's peer
identity (bus uid, pipe token) authenticates the bff; the bff vouches for the principal; the manager
re-checks the principal's group itself. What each side can do if compromised: the bff, act as any
current `pai` member by naming them (accepted by the user, 2026-11-15: "the bff vouches"), never as
anyone else and never beyond `control`/`configure`; a local user who is not the bff, nothing new (every
`As` call refused and logged); a `pai` member, what they can already do from the desktop.

**Slices**, each test first, in order (all five built; kept as the record of what each pins):

1. **The core (step 14 slice 0).** `Requester`, `Principal`, the three checks, `read_file` unwithheld
   for a principal. Done when unit tests in `service.rs` with `FakeAuthority` and `FakePasswords` pin:
   the bff with member `alice` starts an instance, `is_admin("alice")` asked, `is_authorized` (polkit)
   never; a non-bff caller with a principal is refused, `is_admin` not asked, the service not called;
   the bff with a non-member is refused; a member removed between two calls is refused on the second;
   an empty, NUL-, newline-bearing or 257-byte name is refused with nothing asked; calls without a
   principal ask `is_authorized` as today (the existing tests unchanged).
2. **The pipe and `Version`.** `pipe.rs` parses the `As` methods and `Version`; `PipeClient::acting_for`
   and `version`. Done when: `acting_for("alice").start("default")` sends exactly
   `{"method":"StartAs","principal":"alice","id":"default"}` and the server reaches the core with that
   `Requester`; `principal` on plain `Start`, and `StartAs` without it, are `BadRequest`; `Version`
   answers `CARGO_PKG_VERSION` and 1; `version()` against a server answering `BadRequest` is `Ok(None)`.
3. **The D-Bus face.** The same methods on `ManagerInterface`, `DbusClient::acting_for` and `version`.
   Done when, over the peer connection of `manager_dbus_e2e`: a bound client's `StartAs` from the
   caller `FakeAuthority` marks as the bff starts the instance; from another caller it is
   `NotAuthorized` with nothing started; `Version` answers; `version()` against an object without it is
   `Ok(None)`.
4. **The log.** `ActionLog`, the Linux stderr writer, the Windows file writer (type-checked only).
   Done when a fake log holds one record per guarded call with caller, principal and outcome, refusals
   included, and a name with a quote or newline cannot produce a second record.
5. **The bff (step 10 slice 6b part 3).** `Connect` takes the principal. Done when: `GET
   /api/instances/{id}/server` with alice's session connects with `Some("alice")` and answers 200;
   without a session it answers 401 and never connects; `/api/instances` and the authenticator connect
   with `None`; the manager's `NotAuthorized` still maps to 403 `client-not-authorized` and ends that
   session alone, every other failure keeping it.

Then the docs agent updates `crates/bff/README.md`, "Through the manager" and the pipe client's header.
Later, not needed now: the manager returning the canonical account name at `Authenticate` (PAM may
fold case: a mismatch fails closed); renaming `is_admin` and `ADMIN_GROUP` (`PLAN.md` step 10
leftovers); a size cap or rotation for the Windows log file.

### Slot snapshots at a stop (step 10 slice 9b)

Designed 2026-11-16, accepted by the user the same day (before the MVP's run; the 5-minute budget
below; the restore at start as slice 9d); built the same day (commits 05a315f to 17a66a0, "As built"
below). Requirement: with `SaveStateOnShutdown` on, a stop or restart saves the
slot snapshots first, whoever asks (the web UI, `pai`, `pai-config`, `systemctl`, the SCM, a reboot).

**Before 9b** (read in the code): only the client saved. `pai-app` `slot_state::save_all` asks the router's
`GET /models` on `127.0.0.1:<Port>` for the loaded models and posts `/slots/0?action=save` per model
(the router proxies it; llama-server itself writes the file into its `--slot-save-path`), then prunes
the other dumps; the GUI (`gui.rs`, stop worker) and `pai server stop` (`cli.rs`
`snapshot_before_stop`) call it before asking the backend to stop. The manager and the bff do not
link `pai-app`. Under the manager, any other stop saves nothing: the Linux unit `llama.cpp@.service`
execs llama-server (`manager.rs` `run`, `cmd.exec()`) and has no `ExecStop=`; the Windows
`instance_main` (`windows/entry.rs`) kills the child on `Stop`, accepts neither preshutdown nor
shutdown. The restore runs only in the client after its own start (`restore_newest`), and on Linux it
lists `paths::instance_state_dir()`, under `/var/lib/private-ai` (mode 0750, `StateDirectoryMode` in
`instance_unit()`), which a desktop user outside the group `private-ai` cannot read: by the code the
restore under the Linux manager finds nothing (not run, `read_dir` failing reads as no dump).

**Options.**

| | (a) the manager saves in `Stop`/`Restart` | (b) the instance saves at its own stop | (c) the clients keep saving (bff gets a copy) |
| --- | --- | --- | --- |
| Stops covered | only those through the manager; not `systemctl`, the SCM, a reboot | every stop of the service, reboot included | only the clients' own |
| Who calls llama-server | root / LocalSystem over loopback; prunes, as root, a folder the service user owns | the service's own account, its own server | each client |
| Which port and folder | `server@<id>.ini` now, which may differ from what runs (a saved, not yet applied port: the usual "save, then restart") | the launched argv: exactly what runs | as (a) |
| Request timeouts | the manager call blocks for the save: the bff, `pai`, the GUI each need a limit above an unmeasured save | the stop is a job; the call returns once accepted, the existing 5 s fit | the bff's call blocks for the save |
| Protocol change | none, but the manager links the HTTP client | none | none |

**Recommendation: (b).** It is the only option that covers reboot and `systemctl stop`, it takes the
port and folder from what is running rather than from a file that may be ahead of it, it adds no
privilege (the save already writes as the service account, since llama-server writes the file), and it
keeps every manager call a short job submission.

**The design.**

1. **One save leg in `pai-core`.** `slot_state::save_all` moves from `pai-app` to `pai-core` with the
   loopback HTTP client it needs (`pai-app` `http.rs`; `bench::exec` keeps using it from there or the
   core, the coder's choice), taking `(port, dir, budget)` instead of a config; `pai-app` keeps a thin
   wrapper so macOS behaves as now. Two changes: a **budget**, a deadline for the whole save (each
   request's read timeout is the time left), so no save outlives it; and **save then rename**: the
   save asks for `<stem>.llamastate-partial` and renames it to `<stem>.llamastate` on success, so a
   save cut short never destroys the previous dump (the partial name passes llama.cpp's
   `fs_validate_filename`: no `..`, no trailing dot, `common/common.cpp` v1.0.0; its extension is not
   `llamastate`, so the restore never picks it). A pure `slot_target(args) -> Option<(port, dir)>`
   reads llama-server's argv: no `--slot-save-path` is `None` (the feature was off at launch, whatever
   the file says now), no `--port` is `server_cfg::DEFAULT_PORT`.
2. **Linux: `ExecStop=`.** The unit gains `ExecStop=pai-manager save-slots %i` and `TimeoutStopSec=`.
   The new subcommand reads `$MAINPID` (unset when systemd knows the main process already exited, so a
   crash saves nothing and returns at once), takes `/proc/<pid>/cmdline` through `slot_target`, saves
   within the budget, logs one line per model to the journal, and always exits 0. systemd runs
   `ExecStop=` only after a successful start, also when the process ended on its own, as the unit's
   `User=`, and bounds each command by `TimeoutStopSec=`, after which it skips to SIGTERM and later
   SIGKILL ([systemd.service(5)](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html),
   `ExecStop=`, `TimeoutStopSec=`, the `+` prefix). `RestartUnit` and shutdown go through the same stop.
3. **Windows: `run-instance` saves.** On `Stop`, and on `Preshutdown` (newly accepted), `instance_main`
   reports `StopPending`, saves on a worker thread with the target taken from the `Command` it
   spawned (`get_args`), raises `dwCheckPoint` within each `dwWaitHint` while it waits
   ([SERVICE_STATUS](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_status):
   a hint passing without a new checkpoint lets the SCM or the controller assume a failure), then
   kills the child as today. The control handler only signals, as now (the SCM wants it back within
   30 s, [HandlerEx](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nc-winsvc-lphandler_function_ex)).
   The manager sets each instance service's preshutdown timeout (`ChangeServiceConfig2`,
   `SERVICE_CONFIG_PRESHUTDOWN_INFO`) at creation and in its sync: the default is 10 s since build
   15063 ([SERVICE_PRESHUTDOWN_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_preshutdown_info)),
   too short for a GiB write; plain shutdown gets about 20 s, so it is not the path
   ([HandlerEx](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nc-winsvc-lphandler_function_ex), Remarks).
   `ScmService::restart` today waits 30 s (`STOP_WAIT`) for the stop inside the manager call; it
   answers once the stop is accepted and a thread waits (budget plus a margin) and starts the
   service, its failure going to the record of who acted: the same meaning as systemd's job.
4. **The clients stop saving under the manager.** The GUI and `pai server stop` call the save only on
   the local backend (macOS); under the manager the instance does it, and a second save would write
   the same GiB twice. The GUI's stop already waits `runstate::STOP_GRACE` (120 s) for the unit to
   leave `Deactivating`; a longer save shows as "still running", then settles.
5. **The bff: no change of limits.** `POST stop|restart` stays a job submission under the 5 s
   `MANAGER_TIMEOUT`; the save shows as `deactivating` in `GET /api/instances`, which the fe already
   polls. The longer limit `PLAN.md` step 10 item 9 asks for is not needed for 9b.

**Budget** (a preference; no save time is measured): 240 s for the save, `TimeoutStopSec=300` and a
preshutdown timeout of 300 s, so llama-server keeps a minute to exit after the save. A stop, a restart
and a reboot can then wait up to five minutes while a snapshot is written. The first run on a real
machine should time a save (the size: about 4.4 GiB at 128k tokens, `AGENTS.md`) and set the value
from it.

**Trust and failure.** The save crosses one boundary, loopback HTTP from the service's account to its
own llama-server, unauthenticated as today (the framework passes no `--api-key`; a preset carrying
`api-key` would make the children refuse the save with 401, logged, the stop going on). It sends no
secret. A local process that took the port after a crash could answer, but a crash leaves `$MAINPID`
unset (Linux) or ends the Windows loop before any save. Fails closed toward stopping: an unreachable
server, an error reply, a full disk or the budget passing log a line and the stop proceeds; a helper
that hangs anyway is killed by `TimeoutStopSec=` or by the preshutdown limit. The dump of a model that
failed is the previous one, untouched; the prune runs only over models that saved. The manager gains
no code path and no privilege; nothing new is reachable from the bff.

**Kept, moved, removed.** Kept: the save's semantics (every `loaded` model, slot 0, prune), the
restore in the clients, the protocol, the bff's limits. Moved: `save_all` and the HTTP client to
`pai-core`. Added: `pai-manager save-slots <id>`, the unit's `ExecStop=` and `TimeoutStopSec=`, the
Windows preshutdown handling and timeout, the asynchronous Windows restart. Removed: the clients' save
under the manager.

**As built** (reviewed 2026-11-16 against the code; Linux and the core tested, every Windows part
type-checked only, never run on Windows, and the save never run against a real llama-server).

1. **Core** (`pai-core` `slot_save.rs`, `http.rs`): `save_all(port, dir, budget)` with one deadline
   for the whole run (each request's connect, write and every read get what is left), the
   `<stem>.llamastate-partial` name renamed on success and removed on failure, `slot_target(args)`
   reading the last `--slot-save-path` and `--port` in their space form. `SAVE_BUDGET` (240 s) is the
   one constant every caller passes, `pai-app`'s wrapper included. A missing state folder saves
   nothing. The prune keeps the dump of every model the server reports loaded, saved or not.
2. **Linux**: the unit has `ExecStop=<manager> save-slots %i` and `TimeoutStopSec=300`
   (`xtask/src/linux/system_files.rs` `instance_unit`). `save-slots` reads `$MAINPID` and its
   `/proc/<pid>/cmdline` (`manager.rs` `save_slots_target`), writes one stderr line per model saved or
   failed (the unit's journal), always exits 0; off Linux it refuses.
3. **Windows instance** (`windows/entry.rs` `instance_main`): accepts Stop and Preshutdown while
   running (`controls_accepted`, `ends_instance`), reports `StopPending`, saves on a worker thread
   with the target from the spawned `Command`'s args, raises `dwCheckPoint` every `STOP_TICK` (2 s)
   under a `STOP_WAIT_HINT` of 10 s, writes one line per model to `llama-server@<id>.log`, then kills
   the child. The manager's own service accepts Stop only (`manager_controls_accepted`,
   `ends_manager`): it has nothing to save and is simply ended at a shutdown.
4. **Windows preshutdown timeout**: `PRESHUTDOWN_TIMEOUT` (300 s, `ChangeServiceConfig2W`) is set at
   creation and at each sync (a refusal is that instance's sync failure), and again at every start,
   where a refusal is ignored so an older service still starts.
5. **Windows restart** (`windows/scm.rs` `restart`): it answers once the stop is accepted; a
   detached thread polls the state up to `RESTART_STOP_WAIT` (300 s) and starts the service. A
   per-instance `RestartGate` (a generation counter, `windows.rs`) keeps it honest: `stop` raises the
   generation before `ControlService`, `restart` takes a new one after its own stop, and the thread
   starts only while its generation is current, so a Stop or a newer Restart during the wait stands
   and the stale thread records nothing. A start that fails, or a stop that outlasts the wait, is one
   line `action="restart_start" target=.. outcome=..` on the manager's action log (the same
   `FileActionLog` as the manager's records, shared by `Arc`; its own action name, since the `restart`
   call was already recorded). The thread dies with the manager process: a manager stop (an
   upgrade, `Stop-Service`) during the wait leaves the instance stopped, with no record.
6. **Clients**: `server_control::client_saves_snapshots` is true only for `Backend::Direct`; the
   GUI's stop worker and `pai server stop|restart|stop-and-close` save only then. The restore after a
   start is unchanged (slice 9d).

Departures, all accepted: the timeout set again at start (harmless redundancy for services an
older build created); the action log as the sink of the deferred failure; the manager service's
Stop-only rule, which the design left unsaid.

**Open after 9b.** (i) Under the Windows manager `restart` now returns while the old llama-server is
still saving, and `pai server restart` then restores at once (`cli.rs` `snapshot_after_start`):
`wait_listening` finds the old server still listening and asks it to load the previous dump, which
users can read in `state\`. Wasted I/O only (the server is killed after its save), never run; slice
9d removes the client restore under the manager. On Linux the same race finds no dump (the folder is
unreadable). (ii) The MSIs stop instances with `Stop-Service` (`installer/msi-stop-instances.ps1`,
`msi-remove-instances.ps1`), which waits for the stop, so a repair, upgrade or uninstall can now
wait up to the budget while snapshots are written. (iii) The first real run times a save and sets
`SAVE_BUDGET` and the 300 s limits from it.

Then the docs agent updates "Conversation snapshots" and "Through the manager" in
`crates/config/README.md`, `installer/AGENTS.md` (the preshutdown timeout) and the contract line.

**Then, slice 9d** (user, 2026-11-16): the restore at the instance's start (boot and the web's Start
included), which also fixes the Linux restore the desktop user cannot read; it loads the last model at
every start.

## Part 2: the super-manager (`PLAN.md` step 14, R13)

The user's words (2026-11-15): "Ci potrebbe essere un componente super partes tipo un supermanager,
installato su un server differente che può governare differenti manager. Se il bff si connette a questo
gli viene offerta anche la gestione dei nodi." Status: **accepted** by the user on 2026-11-15, who
answered the four questions at the end with the recommended option each.

### Decision summary

1. **A separate service, `pai-supermanager`**, on its own server, unprivileged, holding the node
   registry and the one key the nodes trust. It runs no engine and checks no content: each node's
   manager stays the authority over its machine.
2. **Nodes are managers.** The super-manager speaks to each node's `pai-manager` with **the existing
   manager protocol over TLS 1.3 with mutual authentication**, each side pinning the other's
   self-signed certificate. No new protocol, no CA, no D-Bus off the machine.
3. **A node joins by a one-time code** its own administrator creates, carrying the node's key
   fingerprint and a secret: both sides authenticated from the first byte.
4. **Rights are delegated, not mapped**: joining a node grants the super-manager `control` and
   `configure` on it; the super-manager admits only its local bff, which vouches for a signed-in `pai`
   member of the super-manager's machine. Each forwarded call names that person for the node's log.
   Per-node rights and a central identity wait.
5. **The bff gains a hub mode** (`bff.ini`), where it talks to the super-manager instead of the local
   manager and the API grows a node prefix; the local mode stays as it is.
6. **Nodes never depend on the super-manager**: their instances, boot settings and local clients work
   with it down or gone.

### Role and boundaries

| | Manager (on each node) | Super-manager | bff |
| --- | --- | --- | --- |
| Owns | The instances, their files, the checks, local authorization | The node registry (name, address, pinned fingerprint), its own key, the join, fan-out with per-node timeouts | Web sessions, the HTTP API, the web UI |
| Never | Knows the super-manager's users; accepts a password from off the machine | Caches a node's files as truth; runs checks; starts engines; sees passwords | Holds the node key; reaches a node directly |
| Talks to | systemd / SCM; local callers; the enrolled super-manager over TLS | Nodes over TLS; its local bff over a local socket | The local manager (local mode) or the super-manager (hub mode) |

The super-manager is separate from the bff (rather than a "hub mode" holding the registry inside the
bff) for one security reason and one operational one: the browser-facing process never holds the key
every node trusts, so a compromised bff can act only while it is compromised and only as a signed-in
session could, and cannot carry the key away; and the super-manager can be restarted, upgraded or
backed up without the web service. Cost: a third local service and a small local interface.

### Transport between machines

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. The manager protocol (u32 + JSON frames) over TLS 1.3, mutual, pinned** | rustls in the manager and the super-manager; a key pair per side (rcgen, [docs.rs/rcgen](https://docs.rs/rcgen/latest/rcgen/)); `PipeClient` reused as is over rustls' `StreamOwned`, which is `Read + Write` ([docs.rs](https://docs.rs/rustls/latest/rustls/struct.StreamOwned.html)); `manager::pipe`'s dispatch reused on the node | A network listener in the manager (below); pinning code is ours | **Recommended** |
| B. HTTPS + JSON (axum with TLS, mutual) | An HTTP mapping of every method, a second error shape | Same listener question, more code | No gain over A for a machine-to-machine call |
| C. gRPC (tonic, protobuf) | A schema, a code generator, a second protocol beside the pipe's | gRPC supports client certificates ([grpc.io auth guide](https://grpc.io/docs/guides/auth/)) | Worth it only for streaming (live logs to the web); later, if then |
| D. SSH with a forced command relaying frames to the local manager | No listener of ours; keys in `authorized_keys` with `restrict,command=` ([sshd(8)](https://man.openbsd.org/sshd#AUTHORIZED_KEYS_FILE_FORMAT)) | sshd required on every node: in the box only on Windows Server 2025, an optional feature on Windows 10/11 and Server 2019/2022 ([Microsoft Learn](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse)); enrolling writes another account's `authorized_keys`; the relay account becomes a new trusted caller | A good fallback if a network listener in the manager is refused |
| E. D-Bus over TCP | None | The specification: no integrity or confidentiality, non-local TCP deprecated ([D-Bus spec](https://dbus.freedesktop.org/doc/dbus-specification.html), "TCP Sockets", revision 0.33) | Rejected |

**Who listens.** The super-manager dials the nodes; each node listens. Option A puts the listener in
the manager (root, LocalSystem), the smallest design, since the manager already serves the protocol
and a node then needs only `-core` and the engine (a headless GPU box needs no bff). The alternatives
are a relay in the node's bff (which is sandboxed and unprivileged, but must then be installed on every
node and be trusted to grant `configure` on the super-manager's behalf, the same power with more code)
or a new node agent (another account, unit, package and trusted caller). The risk is bounded so:

- **Off by default.** No port is open until a node's administrator enables it (`pai node enable`,
  `configure`); `pai node leave` closes it and forgets the peer.
- **Nothing is parsed before the handshake proves the peer.** rustls' client-certificate hook
  (`ClientCertVerifier`, [docs.rs](https://docs.rs/rustls/latest/rustls/server/danger/trait.ClientCertVerifier.html))
  accepts only the pinned SPKI hash, the handshake signature still verified; a wrong peer is closed by
  TLS. The only exception is the join window (below): at most 15 minutes, the `Join` method alone,
  attempts limited.
- **TLS 1.3 only, ALPN `pai-manager/1`**: a client offering no matching protocol gets the fatal
  `no_application_protocol` alert ([RFC 7301](https://www.rfc-editor.org/rfc/rfc7301)), which also
  carries the protocol's major version.
- **Bounded**: a few connections at once, the existing 1 MiB frame, an idle timeout; the listener's
  threads never hold the manager's write lock while waiting on the network.
- Port: a proposal, 8592 (the bff is 8590), set in the node's config, opened in the firewall by the
  administrator (the packages open nothing).

### Identity and joining

Each side has one long-lived key pair and a self-signed certificate, generated on first need; the
identity is the SHA-256 of the certificate's SubjectPublicKeyInfo, as kubeadm pins its CA
([kubeadm join](https://kubernetes.io/docs/reference/setup-tools/kubeadm/kubeadm-join/),
`--discovery-token-ca-cert-hash`). Certificates are pinned, so their expiry and the clocks do not
matter. The node's key lives beside its config, readable by the manager only (`/etc/private-ai/node/`
mode 0600 root; `%ProgramData%\private-ai\node\` SYSTEM and Administrators); the super-manager's in its
own state directory.

The join, the kubeadm pattern turned around because the super-manager dials:

1. On the node, a `pai` member runs `pai node enable` (needs `configure`). The manager creates its key
   if missing, opens the listener in join mode and prints a **join code**: the node's fingerprint
   (128 bits are enough to pin) and a random 128-bit secret, valid 15 minutes, single use.
2. In the super-manager's web UI (or its CLI, later), an administrator adds the node: a name, the
   address, the code.
3. The super-manager connects with its own certificate and checks the node's against the code's
   fingerprint before sending anything: no trust on first use. It sends `Join{secret}`.
4. The node compares the secret in constant time, pins the presented client certificate as its
   super-manager, ends join mode. From then on only that certificate gets past the handshake.

A node has at most one super-manager (one pin): simpler rights, and a second governor would be a
question for the user, not a default. **Removal** is forgetting the pin on either side. **A node
reinstalled** has a new key: the super-manager refuses it as "identity changed" (like SSH's changed
host key) until it is joined again. **A super-manager key lost or stolen**: every node is joined again
(`pai node leave` then a new code); a CA with revocation is the answer only once fleets are large.

### Who may act on which node

| Option | Cost | Verdict |
| --- | --- | --- |
| **Delegation**: joining grants the super-manager `control` and `configure` on the node; the super-manager decides who among its users may act | Small: one new caller kind in the node's Authority | **Recommended now** |
| Per-node rights in the super-manager (user or group to node list) | A rights model and its UI | Later, on top of delegation, without touching nodes |
| The same accounts on every machine (a directory: LDAP, AD), the node checking the named user's local `pai` membership | Needs a shared directory; the node trusts the super-manager's word about who it is anyway | Only if the user's machines share one |
| A central identity provider (OIDC tokens verified by each node) | Token issuance, keys, expiry, an IdP to run | Later, if users come from outside |

How a web session maps to a node: the user signs in on the super-manager machine's bff, checked by
**that machine's** manager (`Authenticate`, so that machine has `-core`); only a `pai` member gets a
session. The bff sends each call to the super-manager with the session's principal
(`alice@<super-manager name>`); the super-manager admits only its bff (by peer credentials: `SO_PEERCRED`
on a Unix socket, [unix(7)](https://man7.org/linux/man-pages/man7/unix.7.html)) and forwards the call to
the node with the same principal as a field. The node's manager authorizes the enrolled peer and logs
the principal with every change. No password leaves the machine it was typed for.

What each side can do if compromised: the bff, act as any administrator while compromised (as today on
one machine); the super-manager, act on every joined node as an administrator until each node forgets
it; the network, nothing (mutual TLS); a node, nothing on the other nodes (it holds no credential to
them) and it cannot impersonate the super-manager to other nodes.

### The bff and the web UI

- `bff.ini` `[Bff] SuperManager = true` (default false) selects hub mode; explicit, so a super-manager
  that is down shows as down instead of the UI quietly dropping back to the local machine.
- `GET /api/version` gains `"nodes": true|false`, so the web UI knows the mode without a probe.
- Hub mode adds `GET /api/nodes` (`[{name, address, state: reachable|unreachable|identity-changed|incompatible, version}]`),
  `POST /api/nodes` (name, address, code) and `DELETE /api/nodes/{node}`, and serves every
  instance and preset route under `/api/nodes/{node}/...`, same bodies and errors, plus the keys
  `supermanager-unavailable` and `node-unavailable`. Local mode keeps today's routes; the web UI picks
  its base path from `nodes`. Everything is per node (instances, presets, models), so the node is only a
  prefix.
- The web UI: a Nodes list on the Overview and node-scoped pages; the designer's brief first, as for
  every slice users see.
- Practical prerequisite: a hub is usually reached from another machine, so the bff's LAN access (TLS
  for browsers, `Secure` and `__Host-` on the cookie: `PLAN.md` step 10 slice 6b leftovers) comes before
  the hub is useful beyond an SSH tunnel to its loopback.

### Failure modes

| Failure | What happens |
| --- | --- |
| A node down or unreachable | The super-manager's per-node timeout (shorter than the bff's) answers `node-unavailable` for it; the others answer as usual (fan-out in parallel) |
| A write whose reply is lost | The node's write is atomic (rename); the UI says the outcome is unknown and reloads; the revision check (409) makes a retry safe |
| The super-manager down | Nodes run on: systemd and the SCM own the instances and the boot settings, local `pai-config`, `pai` and a local bff work; the hub's web UI shows `supermanager-unavailable` |
| The super-manager gone for good | Each node's administrator runs `pai node leave`; nothing else on the node changes |
| A node's key changed | Refused as `identity-changed` until joined again |
| Version skew | Below |

### Packaging, upgrades and version skew

- **A separate package**: `private-ai-framework-supermanager` (Linux) with `pai-supermanager` in
  `/usr/libexec/private-ai`, its unit, its sysusers entry (`private-ai-super`), its socket in its
  runtime directory, group-readable by `private-ai-bff` only; requires `-core` (the sign-in) and
  usually sits beside `-bff`. Its own machine is a node only if joined like any other, over
  `127.0.0.1`, one code path. Windows: later (a feature of the framework's MSI, off by default, or not
  at all; a user question).
- **Nodes need nothing new**: the listener is part of `pai-manager` in `-core`, off until enabled.
- **Skew**: the ALPN name carries the protocol's major version (`pai-manager/1`); within a major,
  changes are additive (new methods, new optional fields), and a new `Version` method (no authorization)
  returns the node's framework version and protocol minor, so the super-manager shows it and avoids
  methods an older node lacks (today an unknown method ends the connection with `BadRequest`,
  `manager::pipe`). The super-manager supports the current and the previous major; upgrade it first.
  The join state lives in the node's config directory and survives package upgrades.

### Slices

Each test first; the coordinator may split further. Linux first; the Windows halves of 1 to 4 are
type-checked like the rest of the Windows code.

0. **Before step 10 slice 6b part 3 hardens** (no new behaviour): the acting principal is a field of
   the request beside the transport caller, not folded into `Caller` (below). Done when: a test pins
   that a call from the bff carrying a principal reaches the Authority as (caller, principal, action).
1. **Node identity.** `pai node show|enable` creates the key and prints the fingerprint. Done when:
   the fingerprint is stable across restarts, the key file is 0600 root, a non-`configure` caller is
   refused.
2. **The listener.** TLS 1.3, ALPN, pinned client certificate, `manager::pipe` dispatch with a new
   caller kind. Done when, over loopback with test keys: a pinned client lists instances; an unpinned
   one is closed before any frame; a wrong ALPN gets the alert; the bound on connections holds.
3. **The join.** Code, window, `Join`, pin, `leave`. Done when: a right code pins once; a wrong,
   expired or reused code is refused and counted; after the window no unpinned peer gets past the
   handshake; `leave` closes the port.
4. **Delegated rights and the log.** Done when: the pinned peer may `configure` (including the API
   key in reads), and each change it makes is logged with its principal.
5. **The super-manager.** Crate and binary, its key, the registry, `add` against an in-process node.
   Done when: add with a code pins both sides; a changed node key reads `identity-changed`; a
   stopped node reads `unreachable` within the timeout while another answers.
6. **Its local interface.** The protocol with `node` and principal on a Unix socket, the bff's uid
   only. Done when: another uid is refused; calls reach the right node; `ListNodes` fans out.
7. **The bff's hub mode.** `SuperManager`, `nodes` in `/api/version`, `/api/nodes`, the node-prefixed
   routes over a node seam. Done when: tests against a fake super-manager pass and every local-mode
   test still passes unchanged.
8. **The web UI.** Designer's brief, then nodes on the Overview and node-scoped pages.
9. **Packaging.** The Linux package; checked by `xtask package` and its contents; installing is the
   user's run, then the end-to-end check on two machines.

What can wait: per-node rights, a central identity, nodes that dial out (NAT), a CA, gRPC streaming
for live logs, the super-manager on Windows, a CLI for the super-manager, the gateway across nodes
(R11's routing across instances on several machines).

### What the current code should change before it hardens

- **The manager's caller model** (built: Part 1, "Acting for a signed-in user"). `Caller(String)` stays
  the transport identity and the user the bff vouches for is `Requester.principal`, carried by the
  `<Method>As` methods; the peer of slice 2 is one more kind of caller, not a third way to name a user.
- **The bff's identity.** `session::Identity { user }` (`can_configure` went with the 2026-11-15
  decision); let the user be a principal with its origin (the machine that checked the
  password), so a node's log can tell `alice@hub` from a local `alice`.
- **The bff's upstream.** Today one `ManagerClient` per request with one 5 s `MANAGER_TIMEOUT`. Put
  the upstream behind a seam that returns a client for a node (one node, "local", in local mode), and
  budget timeouts per hop (node < super-manager < bff < the web UI's), which slice 9 of step 10
  already needs for long stops.
- **The revision** (`PLAN.md` step 10 slice 8): key the hash before it crosses a network, as already
  planned.
- **A protocol version** (built: Part 1, "Acting for a signed-in user"): `Version` answers the
  framework version and protocol 1 to anybody; nothing calls it in production yet.

### Questions only the user can answer

1. **Reachability.** Can the super-manager always open a connection to each node (same LAN or VPN)?
   The design assumes yes; nodes that must dial out (behind NAT) need a different, larger transport.
2. **Rights.** Is "every administrator of the super-manager (member of `pai` on its server) administers
   every joined node, after the node's own administrator approved the join" enough for now, or must
   rights differ per node from the start?
3. **The listener.** Is a network port opened by the manager itself (off until a node joins, mutual
   TLS before anything is read) acceptable, or must the manager never listen on the network (then SSH,
   option D, or a relay in the node's bff)?
4. **Platforms.** The super-manager on Linux only at first, with Linux and Windows nodes?

Answers (user, 2026-11-15): 1. yes, the nodes are reachable (same LAN or VPN); 2. enough for now;
3. the manager's listener is acceptable; 4. Linux only at first, with Linux and Windows nodes.
