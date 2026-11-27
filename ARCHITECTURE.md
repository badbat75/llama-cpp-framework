# Architecture

The map of the framework's components, how they talk, who may do what, and what data each one owns;
then the design records. Kept by the `architect` agent (`.opencode/agents/architect.md`). It points to
the guides that hold the details rather than copying them: when this page and a guide disagree, the
guide and the code win, and this page is fixed.

Drawn views of this page: [docs/architecture/infrastructure.html](docs/architecture/infrastructure.html)
(deployment) and [docs/architecture/system.html](docs/architecture/system.html) (components and flows).

Part 1 is the baseline, what exists on 2026-11-15. Part 2 is the design of `PLAN.md` step 14 (R13, a
director), accepted by the user (2026-11-15, revised 2026-11-21 and 2026-11-22), not built.

## Part 1: the baseline

### Components

```text
  browser ──HTTP (plain)─────> pai-bff ─┐                     ┌─> llama.cpp@<id>  (Linux unit)
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
| `pai-bff` (`crates/bff`) + `fe/` | The web service: the API and the embedded React web UI. Plain HTTP on the address `[Bff] Listen`, any ip:port (the administrator's choice, 2026-11-17), every request under a `Host` guard, sign-in checked by the manager | `private-ai-bff` (static system user, sandboxed), `NT SERVICE\private-ai-bff` | [crates/bff/README.md](crates/bff/README.md), `PLAN.md` step 10 |
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
  `<Method>As` forms of the guarded ones (nine since slice 8: the seven of protocol 1, then
  `SetModelProfile` and `SetServerProfile`) and `Version` (protocol 1; "Acting for a signed-in
  user" below), plus `WriteFileIf` and `WriteFileIfAs`, the compare and swap of a write (protocol 2;
  "Saving the server file from the web" below). The client works over any `Read + Write` stream.
- **The bff's HTTP API**: the routes and error shape `{key, args}` in
  [crates/bff/README.md](crates/bff/README.md); the contract the web UI is built against in
  `fe/briefs/api-contract.md`.
- **The instance files**: `server@<id>.ini`, `presets.ini`, `presets.ini.disabled` in the engine's
  configuration directory, written only by the manager; read by `run-instance` and by llama-server
  (`--models-preset`). Designed to become a pool per node with assignments to instances, each instance
  given a file the manager renders, `presets@<id>.ini` ("Nodes and instances" below).

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

Boundaries a call crosses: the browser to the bff (plain HTTP, a session cookie, on the address the
administrator set: the loopback by default, a LAN address and the wildcard served when chosen, with
every request under a `Host` guard that answers only this bff's names); the bff,
`pai-config` and `pai` to the manager (the bus or the pipe, the caller identified by the OS, never by
anything it sends); the manager to systemd or the SCM; the instance to its files and GPUs. A browser
on another machine is the one way a request can come from outside this one, and it is chosen, not
default: the sign-in password and the cookie cross that wire in clear (no TLS in the bff, no
`Secure` on the cookie), which is what the administrator accepted when they chose it (2026-11-17).

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
guarded call, and the director will later name its user the same way (Part 2, slice 4).

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
| `WriteFileIfAs(s principal, s name, s contents, s expected)` | `{"method":"WriteFileIfAs","principal":..,"name":..,"contents":..,"expected":..}` | as `WriteFileIf`: `Changed` when the file's bytes are not `expected` |
| `SetBootEnabledAs(s principal, s id, b enabled)` | `{"method":"SetBootEnabledAs","principal":..,"id":..,"enabled":..}` | as `SetBootEnabled` |
| `Version() -> (s version, u protocol)` | `{"method":"Version"}` | `{"ok":{"version":"<framework>","protocol":3}}` |

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
session at the first refusal: the route answers 403 `session-not-administrator` (the sign-in's key for
the same fact, 5f0d405) and the same cookie is then 401 `session-required` (user, 2026-11-16, reversing
this design's earlier "the session lives on").
Only the manager's `NotAuthorized` ends it; an unavailable or slow manager, an unknown instance or any
other failure keeps the session, being the bff's news and not a refusal of the user. A deleted account
fails the lookup. A site caching NSS (`nscd`, `sssd`) delays
a removal by that cache's lifetime (nscd.conf(5), sssd.conf(5)): the manager adds no cache of its own.
An old manager refuses every `As` method (the bff answers 502 `client-other`); the Linux packages pin
`-bff` to `-core` at the same version and Windows ships one framework MSI, so that skew needs a
broken install.

**Version.** `Version` needs no right; `version` is the manager's framework version, `protocol` an
integer that grows with each additive change (1 = the `As` methods and `Version`; 2 = `WriteFileIf`
and `WriteFileIfAs`; the TLS major of
Part 2 rides in ALPN). The client side is an inherent method on `DbusClient` and `PipeClient`, not on
`ManagerClient`: `version() -> Result<Option<ManagerVersion>, ClientError>`, `None` for a manager
older than the method (D-Bus `org.freedesktop.DBus.Error.UnknownMethod`; pipe `BadRequest`, after which
the server has closed the stream and that client is spent: open a new one). Nothing calls it in
production until the director; the bff does not (same-version packages).

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
the session-gated reads that need no right of the user (`/api/version`, `/api/instances`, GET boot: 401
`session-required` with no session, checked with `Sessions::peek`, which moves no idle deadline; a600a46,
user 2026-11-19) and the authenticator connect with `None`, as the bff.
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

**As built** (eff112d closed the slice, the earlier parts ddaa95f, 11fabbf, c092d0e, 86dbcb3): as
designed, with one correction found in review, and one key that moved later (5f0d405). Slice 5's
`NotAuthorized` answer is recorded above as 403 `client-not-authorized`; today a signed-in route answers
403 `session-not-administrator`, the sign-in's key for the same fact (`control.rs` `action_failure`,
`server.rs`, `presets.rs`), and `client-not-authorized` is the 502 of a read that connects as the bff, the
instances list and the boot read (behind a session since a600a46) (`control.rs` `read_failure`, `lib.rs`). The 503/504 line above words a timeout
as 504; the
built rule turns on one fact, settled by one atomic shared by the handler and its blocking task: while
the write was never asked a time limit or a dead stream is 503 `client-unavailable` (the task then
never writes, the file is what it was), from the moment the write was sent it is 504
`client-timeout` (the outcome unknown). `crates/bff/README.md` and `crates/config/README.md` "Through
the manager" carry the built shapes; `fe/briefs/api-contract.md` keeps two lines the code settled: the
checks named there are `pai-core` `server_cfg::check`, and a done write with a failed status read is
a 200 `running: true`.
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

### Saving the server file from the web (step 10 slice 8)

Designed 2026-11-16, proposed. `PUT /api/instances/{id}/server`, body `{revision, values}` as
`fe/briefs/api-contract.md` words it: 200 `{revision, running}`, 422 `{key, args, field}`, 409
`server-changed-elsewhere`.

**What exists** (at design time). The read (`crates/bff/src/server.rs`) sends `revision`, the unkeyed
64-bit FNV-1a of the whole text, `OpencodeApiKey` included. The manager's `write_file` (`service.rs`)
takes `configure`, holds the `writing` mutex, runs `checks::server_file` and the address check, and
writes atomically; it has no notion of a revision, and the GUI and `pai` write last-writer-wins. The
render of a whole file from values is in `pai-core` (`server_cfg::render_for`); the thread limit check
was in `pai-app` (`server_cfg::check`), which the bff does not link; slice 2 moved it to `pai-core`.

**The key oracle, as it stands.** Every reader of the revision is a signed-in `pai` member whose read is
never withheld, so the same response already carries the key in `values`: today the hash leaks nothing
the reader lacks. It still must go: an unkeyed function of the key is an oracle for any later reader of
the revision without the key (a log, a director view, a future non-administrator), and FNV makes
no claim to resist an adversary ([draft-eastlake-fnv](https://datatracker.ietf.org/doc/html/draft-eastlake-fnv),
section 6: "No assertion of suitability for cryptographic applications"; its section 2.2 adds that a
secret offset basis is learnt by anyone who sees an output for a known input, so a secret prefix does
not make it a MAC).

| Option for the revision | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. A keyed PRF of the whole text, the key a per-process random secret of the bff** | `siphasher` (SipHash-2-4, 128-bit key: [docs.rs](https://docs.rs/siphasher/1.0.1/siphasher/sip/struct.SipHasher.html); already in `Cargo.lock` through usvg), 16 bytes from `getrandom` (already a bff dependency) at router construction | A bff restart makes every open form stale: one 409 and a Reload. Detects every change, the key's included | **Chosen** |
| B. An unkeyed hash of the text with the key's value left out | None: stable across restarts, computable by the manager too | Misses a change of the key alone: a form opened before another administrator changed the key sends the old key back (the fe returns values as read), and the save silently reverts it | Rejected |
| C. A keyed hash with a persistent secret | A secret file the sandboxed bff must own (it has no writable state today) and protect | More state for one avoided Reload per bff restart | Not now |

SipHash is a keyed short-input PRF by design
([Aumasson, Bernstein, "SipHash: a fast short-input PRF"](https://www.aumasson.jp/siphash/siphash.pdf));
HMAC-SHA-256 would also do but needs the `hmac` crate, new to the lock. Preference: SipHash. The
revision stays an opaque 16-hex-digit string, so the fe changes nothing.

**Where the compare-and-swap happens.** A bff-only compare (read, compare, write) leaves a window
between its read and its write in which a GUI or `pai` save, or a second web tab, is overwritten
silently. Closing it needs the compare under the manager's `writing` mutex. Since a keyed revision
cannot be checked by the manager, the two are split:

1. the bff, at the PUT, reads the file (`ReadFileAs`), computes A on that text and answers 409 when it
   is not the body's revision (covers everything up to that read, a bff restart included);
2. it then calls the new **`WriteFileIfAs(principal, name, contents, expected)`**, `expected` being the
   text it just read; the manager, under `writing`, reads the file and refuses **`Changed`** (nothing
   written) when it is missing or differs byte for byte from `expected`, else runs `write_file`'s
   checks and write. The plain text crosses only the bff-to-manager boundary, which already carries it.

| Option for the manager side | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **`WriteFileIf` and `WriteFileIfAs`, `expected` the whole text, protocol 2** | Two methods per face (the plain form for symmetry with `acting_for`'s name mapping, and for the GUI to adopt later), one error name, one `ManagerClient` method with a default body so the fakes do not change | None found: an old manager answers `UnknownMethod` or `BadRequest`, never an unchecked write | **Chosen** |
| An `expected` field on `WriteFileAs` | Fewer methods | An older pipe server ignores the field and writes unchecked (the reason the principal travels in the name) | Rejected |
| No manager compare | Nothing | The window above | Rejected |

`WriteFileIf` takes no `keep_api_key`: its caller sent the text it read whole. A reader the key was
withheld from would always get `Changed` (its text is blanked); no such caller exists, the bff's reads
being a principal's. The `writing` mutex already orders every write, so the compare is atomic against
any manager write; a hand edit of the file as root is outside every guarantee, as today.

**The API key in a save.** Every web read carries the key, so the body's `OpencodeApiKey` is the
value to write: a string sets it, `null` unsets it. The bff writes with no `keep_api_key`. `withheld`
no longer matters for the web: the manager never withholds a principal's read, and the fe treats a
withheld document as a failed read, so no save can start from one. The bff's `withheld` handling in
`document` stays (harmless, and the contract field), not extended to the PUT.

**The bff's steps for a PUT**, each failure answered before the next step:

1. session (401 `session-required`); body `{revision: string, values: object}` with each value the
   type the read gives, else 400 `server-malformed` (new key, as `boot-malformed`);
2. values to a `ServerConfig`: base URL through `server_cfg::normalize_base_url`, a blank `ModelsDir`
   or `StateDir` unset;
3. the checks the manager cannot run for it: `server_cfg::check` moves from `pai-app` to `pai-core`
   (`pai-app` keeps calling it there) and the bff runs it with this machine's CPU count;
4. render with `server_cfg::render_for(cfg, true)` (the bff runs only where dirs are left unset);
5. `ReadFileAs`, the revision compare (409 `server-changed-elsewhere`), `WriteFileIfAs` (409 on
   `Changed`), the manager running `checks::server_file` and the address check again;
6. `Status(id)` (unguarded, as `/api/instances`) for `running`; 200 `{revision: A(contents),
   running}`, the written text being exactly `contents`. A failed status read after a done write is
   still 200, with `running: true`, so the fe says "restart to apply" rather than nothing
   (preference: the save is done, and the safe hint costs at most a needless restart).

Errors: a refusal (`ClientError::Invalid`, or the bff's own check) is 422 `{key, args, field}`, `field`
`args[0]` for `not-whole-number-in-range` and `comment-markers`, `ModelsMax` for the two
`load-on-startup-*` keys, `Hostname` for `bind-all-with-others`; `AddressInUse` is 422
`client-address-in-use` `[id]` field `Port`, `UnresolvableHost` 422 `client-unresolvable-host` `[host]`
field `Hostname`; `NotAuthorized` 403 `session-not-administrator`, the session ended (slice 9e);
`NoInstance`, `InvalidId`, `BadFileName` 404 `client-no-instance`; unavailable or past the 5 s
`MANAGER_TIMEOUT` 503 `client-unavailable`; anything else 502 `client-other`. A write is a small file
plus at most one name lookup, so the read timeout fits; once the write was asked, a timeout or a lost
manager answers 504 `client-timeout` (the file may or may not be saved; `PLAN.md` item 8), before
that 503, and the fe's Reload shows the truth. The PUT moves the session's idle deadline; the
same-origin guard and the 64 KiB body cap already cover it.

**ModelsDir after a save.** What exists: the instances' default folders are made by packaging (Linux
tmpfiles `d /var/lib/llama.cpp/models 0755 private-ai private-ai`, `engine_tmpfiles` in
`xtask/src/linux/system_files.rs`; Windows llama.cpp's MSI `ENGINEMODELSFOLDER`), and a save leaves an
unset `ModelsDir` unset, so the default case needs nothing. `ModelsDir` reaches llama-server only as
`LLAMA_CACHE` (`launch.rs`), and llama.cpp creates the parents of a download itself
(`common/download.cpp`, `create_directories`, v1.0.0) when the instance's account may. Creating a
custom path in the manager would mean root making a folder owned by the instance's account wherever a
`pai` member (or a compromised bff) names it, `/etc` included. **Recommendation: the save creates
nothing**; a custom folder is the administrator's to create with the right owner, as from the GUI under
the manager today. A missing custom `StateDir` stops the instance from starting (llama.cpp refuses a
missing `--slot-save-path`): a check for it is later work, not this slice.

**For the user to decide.** (1) Drop "ModelsDir created for the instance after a save" from slice 8
(recommended, above), or have the manager create a custom folder owned by the instance's account.
(2) Accept one 409 and Reload for an open form after each bff restart (option A, recommended), or pay
for a persistent secret (C).

**Trust boundaries.** Browser to bff: the session cookie, the same-origin guard; the revision tells the
browser nothing it was not sent. Bff to manager: the kernel's peer identity, the principal re-checked
in `pai` at the call; the manager runs every content check again, so a compromised bff can write what
a `pai` member could, no more. What fails closed: a bad body, any check, a stale revision and a
missing file write nothing; an old manager refuses the new method.

**Slices**, each test first, in order:

1. **Keyed revision (bff).** `revision` becomes SipHash-2-4 under a 16-byte key from `getrandom`,
   held by the router; `router_with_session` (tests) can pass a fixed key. Done when: the same text
   read twice through one router gives the same revision; two routers give different ones; the
   revision is not the unkeyed FNV-1a of the text; a text differing only in `OpencodeApiKey` gives
   another revision.
2. **`check` in `pai-core`.** `server_cfg::check` moves to `pai-core`, `pai-app` calling it from there.
   Done when its existing tests pass from `pai-core` and `pai-app`'s save still refuses `Threads`
   above the CPU count.
3. **`write_file_if` in the manager's core.** Done when unit tests in `service.rs` pin: the text equal
   to `expected` is written and checked as `write_file` does; a different or missing file is `Changed`
   with nothing written and the record's outcome `Changed`; the principal and `configure` gates run
   first, as `write_file`'s.
4. **The faces and the client.** Pipe `WriteFileIf`/`WriteFileIfAs`, D-Bus the same, error name
   `Changed`, `ClientError::Changed`, `ManagerClient::write_file_if` (default body `Other`),
   `DbusClient` and `PipeClient` (bound: the `As` form), `Version` protocol 2. Done when:
   `acting_for("alice").write_file_if(..)` sends exactly the `WriteFileIfAs` request and reaches the core
   with that requester; `expected` missing is `BadRequest`; over `manager_dbus_e2e` a stale `expected` is
   `Changed`; `Version` answers 2.
5. **The body to a file (bff, no manager).** A pure function from the JSON body to the rendered text
   or a 400/422 answer. Done when: a well-typed body renders the text `render_for(_, true)` gives; a
   string `Port` is 400 `server-malformed`; `Threads` above the CPU count is 422 with `field`
   `Threads`; a `;` in `ModelsDir` is 422 `comment-markers` field `ModelsDir`; a base URL ending in
   `/v1/` is stored without it; a blank `StateDir` is left unset; `OpencodeApiKey: null` writes no key.
6. **The route.** `PUT /api/instances/{id}/server`. Done when, against the fake manager: a fresh
   revision writes through `write_file_if` with the text read as `expected` and answers 200 with the
   revision a GET then returns; a stale revision is 409 with no write; the manager's `Changed` is 409;
   `Invalid` is 422 with the mapped `field`; `AddressInUse` 422 `client-address-in-use` field `Port`;
   `NotAuthorized` 403 `session-not-administrator` and the session ended; no session 401 and no
   connect; `running` follows the fake's status.

Then the docs agent updates `crates/bff/README.md` ("The server file of an instance", the PUT and the
revision), the contract (409 and 400 keys; `bind-all-with-others` is not among the 422 keys the fe
knows, so it shows as `api-other` until the fe adds it), "Through the manager" (the new methods) and
the pipe client's header.

### Saving a preset from the web (step 10 items 11b, 11c, 11d, 2h)

Designed 2026-11-16, built: `PUT /api/presets/{id}` is served (3eb24b2, its follow-ups 26f5413), as
`fe/briefs/api-contract.md` proposed it, for the web UI's brief `fe/briefs/02h-preset-save.md`; with it
the switch `PUT /api/presets/{id}/enabled`
(item 11b). The user decided (2026-11-16): the save changes only the edited lines and keeps the rest
of the preset files byte for byte; body `{revision, values}` with only the changed keys (a string
sets, `null` removes), 200 `{revision, running}`; Fit to VRAM stays editable (the fe checks its value,
the manager does not); MoE CPU layers and MoE cache stay read-only on the web. The user also allowed
(same day) rewriting the whole section, as the configurator does, if in place is too costly: weighed
below.

**What existed when this was designed.** No preset route in the bff (items 11 and 11c open). `pai-core`
`ini::text` has `replace_key` (first occurrence, key matched in any letter case, the line rewritten as
`key = value` with its own line ending; a missing key appended after the section's last non-blank
line), `section_text`, `replace_section` and `delete_section`, but no key removal. The readers
(`ini::parse_sections`, `checks.rs` `RawSection::get`) take the LAST line of a key, exact case;
llama-server does the same (`parsed[section][key] = value`, `common/preset.cpp:243` in the v1.0.0
clone), treats keys as exact case (an unknown spelling is "not recognized", `:326-330`) and starts a
section afresh at a repeated header (`parsed[current_section] = {}`, `:238`). The configurator's save
renders the whole section from its `Preset` struct (`pai-app` `presets::render_section`, about 400
lines with the reader and `validate_for_save`); the struct has no slot for a key it does not model.
The manager's `check_contents` (`manager/service.rs`) runs `checks::presets_file` (or
`presets_disabled_file`) and `preset_ids_free` against the files on disk, under the `writing` lock,
for `WriteFileIf` as for `WriteFile`; `presets_file` checks every section that is
not in the old file with identical lines, so an edited section is checked whole and the untouched
ones are not.

**In place or the whole section.**

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. Edit the changed lines in place** | Two `ini::text` helpers (about 60 lines with the duplicate rule below) and one pure bff function applying `values` | An edited line is rewritten whole, so an inline comment on that line goes (said in the docs); nothing else moves | **Chosen** |
| B. Render the whole section, as `pai-config` | The `Preset` reader, `render_section` and the prunes must reach the bff: move about 1000 lines from `pai-app` to `pai-core`, or link `pai-app` (whose build script needs a llama.cpp clone); the fe would send a whole document instead of changes | Drops comments and every key the struct does not model (a llama-server option set by hand is lost silently), rewrites spellings the page shows ("Written as 0"), so untouched values change; breaks the user's first decision | Rejected |

A is both the smaller change and the one that keeps the user's decision; B is no fallback worth
keeping open.

**(a) The revision covers the section, not the file.** The manager's compare-and-swap compares whole
files, but that is the bff's guard for the window between its own read and its write; the revision is
the guard for the window between the page's read and the save. They can differ:

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **Section: SipHash-2-4 under the bff's per-start key of `<file name> LF <section text, trailing blank lines cut>`** | The same function as the server file's, over `ini::text::section_text`; the file name in the input so a switch between the page's read and the save is a change | None found: the bff applies the edit to the file as it reads it at the PUT, and the manager re-checks the section and the file-wide rules (the load-on-startup count, unique ids) against the files at the write | **Chosen** |
| File text, as `server.rs` | Nothing new | Every save of another preset in the same file, or a switch of another preset, gives this page a 409 and a Reload: spurious with many presets in one file | Rejected |

Trailing blank lines are cut because `replace_section` appending a new section after the last one
rewrites the blank lines before it. A bff restart still makes every open page stale (one 409, as for
the server file). Item 11c's GET sends this revision; 11c's "hash of the file text" is replaced.

**What the read's `values` are** (recorded 2026-11-17, as built in 36a892d): `GET /api/presets/{id}`
gives each key as `ini::parse_sections` reads it, the value cut at the first `;` or `#` and trimmed,
the last line of a key winning; not the raw bytes. That is what llama-server reads too (its preset
PEG ends a value at `[;#]`, `common/preset.cpp:201-202` in the v1.0.0 clone), so the page shows what
the engine gets. The save needs nothing more for it: an unchanged field is never sent, so a line with
an inline comment stays byte for byte; a changed one is rewritten whole (its comment goes, as said
above); `set_key`'s equal-value case compares with the same reading, so a value re-sent unchanged
writes nothing; and the revision hashes the raw section text, so an edit to a comment alone between
the read and the save is a 409, which is correct (the file changed) and costs one Reload.

**(b) The key-removal helper and the duplicate rule.** Two transforms in `pai-core` `ini::text`,
both matching a key line as `replace_key` does (`line_starts_with_key`: any letter case, never a
comment line, never a longer key such as `ctx-size-draft` for `ctx-size`), only inside the named
section:

- `remove_key(content, section, key) -> String`: every line of that key removed, its line ending with
  it; every other byte kept. No line of the key: the text unchanged.
- `set_key(content, section, key, value) -> String`: exactly one line of the key remains, at the
  first occurrence's place. The rule as built (59eaf68, set by the coordinator where this design was
  silent; reviewed and accepted 2026-11-17 with one change): a key the section holds once, whose value
  read as `parse_sections` reads it (after the first `=`, trimmed, cut at the first `;` or `#`)
  equals `value`, leaves the text byte for byte (indentation, spacing, inline comment kept); a
  differing value rewrites the line whole as `key = value` with the line's own ending (none on an
  unterminated last line); a key occurring more than once is always collapsed onto the first
  occurrence, even when every copy holds the value; a missing key is appended as `replace_key` does.
  **The change (slice 1b):** the byte-for-byte case applies only when that one line spells the key in
  the exact case given. `CTX-SIZE = 4096` set to `4096` is today left as it is, and llama-server
  refuses that spelling (`common/preset.cpp:326-330`), so a save that "sets" the key would leave a
  preset that does not start while answering 200; rewritten, it becomes `ctx-size = 4096`. Why the
  rest is right: the equal case is what keeps an unchanged key's spelling and comment when the fe
  re-sends a value already on disk (a second tab saved it first), and the bff then writes nothing.

Which occurrence an edit touches: the first keeps its place, every other goes, so the value written
is the one every reader takes. The bff normalizes, it does not refuse: a duplicate is a hand-edit
the web cannot otherwise mend, and llama-server already reads only the last line, so removing the
earlier ones changes nothing it reads, and the user saw the last value on the page. A line of the key
in another letter case (`CTX-SIZE = 1`) counts as an occurrence and goes too; llama-server refuses it
anyway (`:326-330`). A section repeated in the file (two `[X]` headers, any case) is not mended: the
bff edits the first, and the manager refuses the write with `preset-name-taken` (`preset_sections`),
a 422 without `field`.

**The body and what the bff refuses before the manager.** 400 `presets-malformed` when the body is not
`{revision: string, values: object}`, a value is neither a string nor `null`, a set value is empty
after trimming spaces and tabs (removing is `null`), holds a control character other than tab (CR and
LF included: they would end the line), or a key is not in the list of web-editable keys below
(matched in exact case: `CTX-SIZE` in a body is 400, since llama-server knows keys only in their own
case). The list is built once for all of 2h.1 to 2h.9, so the route needs no change per sub-slice.

**The web-editable keys** (corrected 2026-11-17; the earlier text pointed at "brief 02h's table
133", which lists about 20 keys by kind of control, not the list). **The rule:** every key
`pai-app` `presets::render_section` can write (75 keys, `crates/app/src/presets.rs` 1735-2116 at
59eaf68), minus the ten that no 2h sub-slice enables. That equals the keys of brief 02e's field
tables 92, 93, 96, 97, 98, 101, 102, 107 and 112 (the controls 2h.1 to 2h.9 enable), minus the same
ten; checked key by key on 2026-11-17, the two sets agree. **65 keys**, by card (each written by
`render_section` at the line given):

| Card (02e table) | Keys | Written at |
| --- | --- | --- |
| Resource / context (92) | `ctx-size`, `parallel`, `kv-unified`, `kv-unified-per-slot`, `batch-size`, `ubatch-size` | 1874, 1902, 1910-1913 |
| Embeddings (93) | `embeddings`, `pooling` | 1922, 1924 |
| KV cache (96) | `cache-type-k`, `cache-type-v`, `flash-attn`, `cache-ram`, `cache-reuse`, `ctx-checkpoints`, `checkpoint-min-step` | 1934-1936, 1942, 1949, 1956, 1957 |
| Sampling overrides (97) | `temp`, `top-k`, `top-p`, `min-p`, `repeat-penalty`, `presence-penalty`, `backend-sampling` | 2095-2100, 2109 |
| Advanced (98) | `load-on-startup`, `chat-template-kwargs` | 1762, 2112 |
| Chat / reasoning (101) | `jinja`, `prefill-assistant`, `reasoning`, `reasoning-format`, `reasoning-effort`, `reasoning-preserve`, `reasoning-budget`, `reasoning-budget-message`, `n-predict` | 1960, 1964, 1967, 1968, 1975, 1985, 2002, 2005, 2010 |
| Vision (102) | `mmproj-offload`, `mmproj-device`, `image-min-tokens`, `image-max-tokens` | 1776, 1777, 1781, 1782 |
| Hardware (107, MoE left out) | `n-gpu-layers`, `n-cpu-ffn`, `op-offload`, `repack`, `no-host`, `lazy-mode`, `load-mode`, `fit`, `fit-ctx`, `fit-target`, `device-draft`, `n-gpu-layers-draft` | 1875, 2026, 2054, 2063, 2070, 2077, 2083, 2087, 2091, 2092, 1871, 1870 |
| Speculative decoding (112) | `spec-draft-n-max`, `spec-draft-p-min`, `spec-draft-n-min`, `spec-draft-n-ubatch`, `spec-draft-type-k`, `spec-draft-type-v`, `spec-ngram-mod-n-match`, `spec-ngram-mod-n-min`, `spec-ngram-mod-n-max`, `spec-ngram-simple-size-n`, `spec-ngram-simple-size-m`, `spec-ngram-map-k-size-n`, `spec-ngram-map-k-size-m`, `spec-ngram-map-k4v-size-n`, `spec-ngram-map-k4v-size-m`, `spec-ngram-map-k4v-min-hits` | 1799-1861 |

**Written by the configurator, not web-editable (10)**, each until the slice named: `model`,
`mmproj`, `model-draft`, `spec-type` (the Assets card, 2i); `n-cpu-moe`, `moe-cache-mib` (need the
model's facts, 2g, user U3); `device`, `split-mode`, `tensor-split` (GPU distribution, 2f);
`override-tensor` (tensor placement, 2j). A slice that enables one moves it from this list to the
table, in the same commit as its fe control.

**Where the list lives and how a test pins it.**

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **The two lists as constants in `pai-core` (`checks::WEB_PRESET_KEYS`, `checks::PRESET_KEYS_NOT_ON_WEB`), the bff reading the first; a `pai-app` test pins them against `render_section`** | Two constants moved from the bff to `pai-core`; one test in `pai-app`, which has both `render_section` and the full fixture | None found: the keys of `render_section(&full_preset())` (`presets.rs` 2774, a struct literal with every field set and no `..Default`, so a new field cannot be left out) read with `ini::parse_sections` must equal the union of the two lists, and the two must be disjoint. A name that is not a key, a key the configurator stops writing, and a new key nobody classified each fail it | **Chosen** |
| The list in the bff, pinned against a literal copy in its own test, plus `pai-core`'s tables (`PRESET_INT_KEYS` without `moe-cache-mib`, `PRESET_FLOAT_KEYS`, `PRESET_ENUM_KEYS` without `split-mode` must be in it) | No move | The literal copy is the list again; the bool, text and device keys (`embeddings`, `fit`, `jinja`, `no-host`, ...) are in no `pai-core` table, so a wrong name among them passes. This is how about 40 wrong names got into the first draft | Rejected |

`pai-bff` cannot link `pai-app` (its build script needs a llama.cpp clone), which is why the pin sits
in `pai-app`; the constants go in `checks.rs` beside the other preset key tables the manager uses.
The bff test keeps the slice-3 cases (`model`, `n-cpu-moe`, `foo` are 400). The fe side (each enabled
control sends its key) is pinned by the fe tests of each sub-slice, against the same 02e tables.

Why a list rather than the contract's key syntax: the manager does not check unknown keys
and llama-server refuses to start on one, so a syntax rule would let any web caller stop the next
start; the list fails closed, and it also answers the contract's open point on `model: null` (400,
never sent by the fe). A `[` at a value's start needs no rule: the value never starts a line. Then
each set value goes through `checks::comment_markers(key, value)`, for every key, not only the text
keys the manager checks, since `;` and `#` cut a value silently for every reader: 422 `comment-markers`
field the key.

**(c) The manager checks the edited section whole: confirmed** (`presets_file`, above). An untouched
hand-edited bad value in that section is refused too; other sections are not checked. The 422 is
`{key, args, field}`, `field` given by one rule per key, in one bff table beside the server file's:
`args[0]` for `not-whole-number-in-range`, `comment-markers`, `not-a-number`, `not-one-of`,
`not-json-object` and `min-above-max` (its minimum key); `fit-target` for `fit-target-format`;
`moe-cache-mib` for `moe-cache-with-tensor-split`; `load-on-startup` for both `load-on-startup-*`;
`model` for `preset-no-model`; `override-tensor` for both `override-no-*`; no `field` for
`invalid-preset-id` and `preset-name-taken`. A field the page does not show as an enabled control is
the fe's business (brief 02h, 143 and 144).

**(c1) An inline comment on an untouched line (defect found 2026-11-17 in 3eb24b2; decided the same
day).** `preset_section` runs `comment_markers` on the RAW value of each `PRESET_TEXT_KEYS` line
(`checks.rs` 639-646; `RawSection` keeps the comment in the raw value, 571-628), so an edited section
holding `model = a.gguf ; Q8` (or a comment on `mmproj`, `model-draft`, `override-tensor`,
`chat-template-kwargs`, `reasoning-budget-message`, `fit-target`) is refused 422 `comment-markers` at
every web save, though the bff leaves that line byte for byte and llama-server reads it cut at the
comment (`common/preset.cpp:201-202`). The same rule refuses **switching such a preset on**: the
section is new to `presets.ini`, `presets_file` gets only `presets.ini` as `old`
(`service.rs` `check_contents`), so it is checked whole, raw lines included; that hits the web switch
and any client that moves a section unchanged. The GUI's save is not hit because it rewrites the
section from parsed (cut) values. Every other rule of `preset_section` reads `s.get` (the value cut
at the comment) or the header, so this is the only raw-line rule; the server file's `server_file`
also checks raw lines but is not reached by a byte-for-byte edit (outside this design).

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. `comment_markers` only on a key line no current preset file holds identically in the section of the same id** | `preset_section` takes the known sections; `presets_file` gains the other file's text, as `presets_disabled_file` already has; one call in `check_contents` | A sender can get past the check only with a `(key, raw value)` pair a root/LocalSystem-owned file already holds in that very section: it cannot introduce a value with `;` or `#`. Any changed key, value or spelling is checked | **Chosen** |
| B. `comment_markers` on the cut value | One line | Never fires (the cut value has no marker), so a value the GUI or `pai` is about to write with `;` reaches the file and is silently shortened; the check is gone in all but name | Rejected |
| C. Drop the manager's check, trust the writers' own (`pai-app` `validate_for_save`, the bff's `parse_body`) | Removes code | The manager stops checking a rule for any caller; breaks "the manager does not trust the bff" | Rejected |

**The rule.** A key line of a section in the new text is exempt from `comment_markers` when a section
with the same id (exact, as the header is kept byte for byte by both the save and the switch) in
`old` or in the other preset file as it is now holds a key line with the same key and the same raw
value (both trimmed, as `raw_sections` stores them). Every other rule of the section still runs on
the whole section (decision c). Lines `set_key` removed while collapsing duplicates need nothing: a
removed line is not checked. Applies to `WriteFile` and `WriteFileIf` alike (both go through
`check_contents`) and to both files: `presets_file(new, old, other, models_max)` with `other` =
`presets.ini.disabled`, `presets_disabled_file(new, old, enabled)` with `old` and `enabled` as known.
The bff keeps its own `comment_markers` on each set value (a 422 before the manager); `pai-app`
keeps `validate_for_save`; the manager's check stays the one that holds for every caller.

**(d) The keys and `running`.** `presets-malformed` (400) and `preset-changed-elsewhere` (409) are
accepted, on the pattern of `server-malformed` and `server-changed-elsewhere`. `running` is true when
the section was written to `presets.ini` and any instance serving it is `active` or `activating`.
Which instances serve a preset: every instance of the engine whose file holds it, since each
instance's llama-server gets the engine's one `presets.ini` (`launch::server_args`, `--models-preset`)
and the manager already counts load-on-startup against every instance's `ModelsMax` for that reason.
With one engine today, that is every instance: `list_instances`, then `status` of each until one is
active or activating. A failed status read after a done write gives `running: true`, as for the server
file. When engines arrive (item 10) the list is filtered by the engine. Superseded by "Nodes and
instances" (f): `running` reads the instances the preset is in effect on.

**The bff's steps for a PUT**, each failure answered before the next: session (401); the body (400,
then the comment-marker 422); read `presets.ini` as the user (`ReadFileAs`) and find the section
(`id` matched in any letter case), else read `presets.ini.disabled`, else 404 `no-preset-named`; the
section revision compare (409); apply the edits in key order (below), `null` by `remove_key`, a string by
`set_key` with the value trimmed; a text equal to the one read is 200 with the current revision and no
write (so `{}` with a fresh revision is 200; a stale one is 409 first); else `WriteFileIfAs` with the
text read as `expected` (`Changed` is 409), the manager re-running its checks (422); then `running`;
200 `{revision of the section as written, running}`. The other answers, the write's own time limit
and 504 `client-timeout` are the server file's (slice 8), shared code. The id is only matched against
section headers; the two file names are fixed, so no path comes from the URL.

**As built and reviewed (2026-11-17, `crates/bff/src/presets.rs` `save`, `parse_body`, `apply`;
tests `crates/bff/tests/preset_save.rs`, `src/tests/preset_body.rs`).** The route follows the steps
above, the 422 field table (`field_of`) and `running` (decision d, the switch's `running`). Decided
during the build and accepted:

- A body whose `Content-Type` is not `application/json` (a charset parameter allowed) is 400
  `presets-malformed` before any connect: the server save's and the switch's `json_body` gate.
- A set value is trimmed of spaces and tabs only; one whose first or last character is another
  Unicode whitespace (U+00A0, U+2028, ...) is 400, because `pai_core::ini` reads a value with
  `str::trim` (Unicode `White_Space`) and would read back a shorter value than the one written.
  Such characters inside a value are allowed.
- After a landed write, a refusal or failure of the state read, or a limit that runs out on it,
  answers 200 `running: true`, as the switch does: the preset is saved, only the state is unknown.
  A refusal there does not end the session; the next call that needs the right does.
- A preset present in both files: the `presets.ini` copy is edited, the one GET and the list show.
- The total limit is `write_timeout` from the start of the request, its reads within `timeout`;
  the write starts only while the answer is still wanted (`WriteState::may_write`).

Two small corrections, neither blocking the commit, each one test and a few lines:

- **Edit order.** The design said "body order", which the build cannot keep: `values` is a
  `serde_json::Map`, insertion-ordered only with serde_json's `preserve_order` feature, which this
  workspace enables through `pai-app`'s dependencies and not for `pai-bff` built alone
  (`serde_json::Map` docs, docs.rs/serde_json: a `BTreeMap` unless `preserve_order` is on). Edits of different keys commute (each touches only its own
  lines), but two keys missing from the section are appended in map order, so the bytes written
  depend on how the binary was built. Decision: the bff applies the edits sorted by key, so new keys
  land in a fixed order a test can pin.
- **A save that changes nothing** answers 503 `client-unavailable` when the state read outlasts the
  limit (`TimedOut::Cancelled`, no write asked). Nothing needed writing, so it should answer as the
  landed case does: 200 with the revision and `running: true`.

**(e) Reloading llama-server after a save: later, not now.** The v1.0.0 router re-reads its preset
file at `GET /models?reload=1` (any non-empty value, `tools/server/server-models.cpp:2166-2169`) and
unloads each running model whose preset changed or was removed (`:947-965`); it also reloads by itself
at the next request after a download finishes (`need_reload`, `:1124-1128`, `:1498`). Calling it from
the bff is a new boundary: bff to llama-server over HTTP, with the instance's address and, when one is
set, its API key; and it unloads a model mid-conversation. It needs its own design and a user choice
(apply now or on restart), so today `running` drives "restart to apply". Note for the docs: a running
router can pick up a saved preset before a restart, at its next reload.

**(f) The configurator's "Saved preset [id]".** It omits that a running router keeps the old preset
(until a restart or the reload above). Recommended as a small plan item, not part of this design: under
the manager the GUI already reads the instance's state, so "saved, restart to apply" when the preset is
in `presets.ini` and the instance is active is one status read and one string.

**The switch on and off (item 11b).** Two files cannot be replaced atomically together. Options:

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **The bff makes two compare-and-swap writes, destination first, as `presets::move_between`** | No manager change: read both files, `WriteFileIfAs` the destination with the section added (`text::replace_section` of the raw `section_text`), then `WriteFileIfAs` the source without it | A failure or `Changed` between the two leaves the preset in both files: read as enabled (`load_listed`'s rule), and the next switch mends it (the destination's twin is replaced, the source's deleted) | **Chosen** |
| A manager `MovePreset(As)` doing both writes under `writing` | Two methods per face, protocol 3 | Still not crash-atomic: the same two-file state after a crash; gains only that no other manager write lands between the two, which the two compares already detect | Not now |

The manager's checks were made for this order: switching off adds the section unchanged to the
disabled file, where `presets_disabled_file` and `preset_ids_free` let a section `presets.ini` holds
unchanged through, so it is never refused; switching on adds a section new to `presets.ini`, checked
whole with the load-on-startup count, and `preset_ids_free` lets it through because the disabled file
holds it unchanged. The switch takes no revision (it moves the section unchanged). A missing disabled
file is created with the configurator's header text; `WriteFileIf` refuses a missing file
(`Changed`), so the first switch off uses `WriteFileAs` for that one file only when the read found
none (a race there is two creators of the same header, harmless). Answers: 200 `{enabled, running}`,
already there 200 with no write, 409 `preset-changed-elsewhere` on `Changed` with "reload the list"
(new for the contract), 422 as the save, 404, 401, 403, 503, 504. A per-bff lock is not needed: the
compares catch every interleaving, the losing tab gets 409.

**Trust boundaries.** As the server file's: browser to bff, the session and the same-origin guard;
bff to manager, the kernel's peer identity and the principal re-checked in `pai`. A `pai` member (or a
compromised bff) can write any listed key with any one-line value, which can stop the engine's next
start (a bad `fit`, a flag value llama-server refuses): the same power the GUI and `pai` give that
user, no more. Fails closed: a bad body, an unlisted key, a stale section, a changed file, any check;
nothing is written.

**Order.** After slice 8 (its route gives the shared error and time-limit code), item 11 (the list)
and item 11c (the read, with the section revision); 11d is independent.

**Slices**, each test first, in order:

1. **`ini::text::remove_key` and `set_key` (pai-core).** Done when: removing a key drops every line of
   it in the named section, in any letter case, and only there (the same key in another section, a
   comment naming it and `ctx-size-draft` stay), each other byte equal, LF and CRLF files alike; an
   absent key leaves the text equal; setting a key with three lines leaves one, at the first's place,
   with the new value, the parsers reading it; setting a new key appends it as `replace_key` does.
2. **The section revision (bff, in item 11c's read).** Done when: the same section read twice gives the
   same revision; a change in another section, or a section appended after it, does not change it; a
   change in the section does; the same text in the other file gives another; two routers give
   different ones.
3. **The body to the new text (bff, pure, no manager).** A function from the body and the file text to
   the text to write or a 400/422. Done when: `{}` gives the text unchanged; a set key replaces only
   its line (the file's other bytes equal, a comment above it kept); `null` removes every line of the
   key; an unlisted key (`model`, `n-cpu-moe`, `foo`) is 400; a value with LF, CR or another control
   character, an empty value, a number, a missing `revision` are 400; `a;b` is 422 `comment-markers`
   field the key; `CTX-SIZE` in the body is 400.
   - **3a. The key lists in `pai-core` (before 3's list test).** `checks::WEB_PRESET_KEYS` (the 65
     above) and `checks::PRESET_KEYS_NOT_ON_WEB` (the 10). Done when a `pai-app` test passes: the keys
     of `render_section(&full_preset())`, read with `ini::parse_sections`, equal the union of the two
     lists, and the lists share no key; renaming one entry of either list (say `min-p` to
     `min-markers`) makes it fail. The bff's check reads `WEB_PRESET_KEYS`; its own list goes.
   - **1b. `set_key`'s equal-value case only for the exact spelling (pai-core).** Done when: a section
     with `CTX-SIZE = 4096`, set to `ctx-size` `4096`, gives `ctx-size = 4096` at that line; `ctx-size
     = 4096 ; note` set to `4096` stays byte for byte, as today.
4. **The route `PUT /api/presets/{id}`.** Done when, against the fake manager: a fresh revision writes
   through `write_file_if` with the text read as `expected` and answers 200 with the revision the GET
   then returns; a stale revision, the section moved to the other file, and the manager's `Changed`
   are 409 with no write; an id in neither file 404; a switched-off preset is written to
   `presets.ini.disabled` and answers `running: false`; each refusal key maps to its `field` (table
   above, one test per rule); `running` true for an enabled preset with an instance active or
   activating, false when every one is inactive, true when the status read fails after the write; no
   session 401 with no connect; the same text 200 with no write.
5. **The switch `PUT /api/presets/{id}/enabled` (item 11b).** Done when: switching off writes the
   disabled file first, then `presets.ini`, the section's bytes equal in its new file; a missing
   disabled file is created with the header; a `Changed` on the second write leaves the preset in both
   files and answers 409, and a second switch mends it; switching on a load-on-startup preset past
   `ModelsMax` is 422 with nothing written; already there is 200 with no write.
6. **Untouched inline comments pass the manager (pai-core `checks.rs`, one call in the manager's
   `check_contents`; decision c1).** Done when, as `checks` unit tests: `presets_file` with `old`
   holding `[a]` `model = a.gguf ; Q8` and `ctx-size = 4096`, and `new` the same with `ctx-size = 8192`,
   is Ok; the same with the model line changed to `model = b.gguf ; Q8` is 422 `comment-markers`
   `model`; a line added to that section, `reasoning-budget-message = a;b`, is refused; the model line
   identical but held only by another section `[b]` of `old` is refused; a section new to
   `presets.ini` that `other` (the disabled file) holds with the comment line, its header equal, is Ok
   (the switch on); `presets_disabled_file` passes the comment line known from `old` and from
   `enabled`; a section new to both files with `model = a;b.gguf` is still refused. `check_contents`
   passes `presets.ini.disabled` as `other`. `pai-app`'s
   `save_validation_rejects_comment_markers_in_free_text_fields` and the bff's 422 `comment-markers`
   test stay green unchanged. In the same slice the `comment-markers` text in
   `crates/core/strings/en_US.toml` and `cli.toml` becomes `{0} contains ';' or '#', which starts a
   comment in the INI format and cuts the value short; use a value without those characters` (it was
   "use a path", wrong for `chat-template-kwargs` or `fit-target`).

Then the docs agent updates the contract (the section revision and its 409 wording, the key list
instead of the key syntax, `presets-malformed` for an empty value, the switch's 409),
`crates/bff/README.md` and `ini.rs`'s header (the two transforms and the duplicate rule).

**For the user to decide.** Nothing blocks the slices. Two choices are recommendations the user may
overrule: (1) the section revision rather than the file's (recommended); (2) reloading llama-server
after a save stays a later design (recommended), with "restart to apply" meanwhile.

### Nodes and instances (step 10 item 13)

Designed 2026-11-17, rewritten the same day for the user's requirement change (presets are a shared
pool per node, assigned to instances); built after the web UI's preset cards (2h.5 to 2h.10). Revised again the same evening for profiles: "Revision: profiles" at the end of this
record wins over (a) to (j).

**The user's decisions (2026-11-17).** The web UI's top level is the node (this machine under its host
name), its instances inside, each with a Server tab and a Presets tab, no top-level Models entry (an old
`#/models` link opens the `default` instance's Presets); more nodes later without redoing the page.
Each instance is collapsible and shows, collapsed, its engine, its port and its tags; tags are labels
only, the router routes by model name; the Overview (behind sign-in since a600a46) shows port and tags. Instances are
added and removed from the web UI, llama.cpp first, each on its own port; `default` cannot be removed;
a running instance is stopped before its removal. Strata is a step of its own. **Presets are a shared
pool per node, assigned to instances, many to many**: removing an instance removes only its
assignments, never a preset; editing a preset applies to every instance it is assigned to; one preset
on several instances is one model served several times, and the router picks a running instance.
Before a preset is applied on an instance, a resource check: that instance's `ModelsMax`, an estimate of
GPU memory against the instance's GPUs, the model file present on the node, the preset's keys accepted
by the instance's engine. On and off: a preset has a global switch, and an assignment may override it
explicitly per instance ("Entrambi, per assignment se vuoi esplicitamente fare override"). One node
now, without closing assignments to instances on other machines (R13, step 14). No migration code.

**What exists.** An instance is a file: a new `server@<id>.ini` in the engine's directory adds it
(`WriteFile`, `service.rs`; `paths::instance_ids`); the Linux template unit runs any id; on Windows
`Start` creates the missing `llama.cpp_<id>` service and the manager's start (`scm.rs` `sync`) deletes
services with no file. There is no remove. Every instance gets the engine's one `presets.ini`
(`launch::instance_command`, `--models-preset`), so `check_contents` counts `load-on-startup` against
the lowest `ModelsMax` of all instances; the switch moves a section between `presets.ini` and
`presets.ini.disabled`; the GUI and `pai` read and write those two files (`pai-app`
`server_control::ConfigFile`, the instance `default` only). Ports are unique already (`check_address`,
`AddressInUse`). An unset `StateDir` is one shared folder, where snapshots are named by the model's file
stem (`slot_save.rs`). `GET /api/instances` is `{id, engine, state}` from `ListInstances` and `Status`,
the manager's two unauthenticated calls; `server@<id>.ini` needs `control` to read, which the bff's
account lacks. The inputs of a memory estimate are in `pai-core` since slice 10a (1694064):
`gguf_shape` (`local_shape`, a pure-Rust header reader of a regular file, `kv_per_token`,
`GPU_RESERVE`, `MAX_HEADER`, `REF_CTX`) and `devices` (the `--list-devices` parser); the GUI's
placement estimate (`hf_fit::estimate`), the GPU pool (`hf_fit::gpu_pool`) and the probe itself
(`devices::list`) stay in `pai-app`, which the manager does not link.

**(a) The pool, the assignments, and what each instance is given.**

- **The pool is today's pair**, `presets.ini` (switched on) and `presets.ini.disabled` (switched off) in
  the engine's directory: the global switch stays exactly the switch that exists. Nothing moves.
- **The assignments** are a new file of the manager's, `assignments.ini` beside them: one section per
  instance id, one key per assigned preset id, the value `inherit` (follow the global switch), `on` or
  `off` (the explicit override). The key `*` assigns every preset of the pool, an explicit key winning
  over it. A preset id has only letters, digits, `.`, `-`, `_` (`checks::valid_preset_id`), so `*` is
  never one.
- **Each instance is given a file the manager renders**, `presets@<id>.ini`: the pool's `[*]` section,
  then every preset assigned to the instance and in effect on (override, else the global switch), each
  section copied byte for byte from the pool file that holds it, under a header saying it is generated.
  The launch passes it (`--models-preset presets@<id>.ini`). The manager renders under its `writing`
  lock after every write of a pool file, of an assignment, or of an instance's create or remove, and
  renders every instance again at its own start (as `sync` heals the Windows services), so a write that
  failed halfway is mended. No client writes it (`BadFileName`).

| Option for what an instance is given | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. The manager renders `presets@<id>.ini` at each write** | One render function in `pai-core`, called by the manager's writes and its start | Several files per write: pool first, then the renders; a failure in between is an `Io` reply and is mended at the next write or start | **Chosen**: the file is always current, so a later reload of llama-server (`?reload=1`) sees an edit without a restart |
| B. `run-instance` renders it at the instance's start | No render in the write path | The instance's account must own a writable place (`/etc` is not); errors appear only at start; a reload sees nothing new | Rejected |
| C. Each instance reads the pool directly | Nothing to render | llama-server offers every section it reads: no assignment possible | Rejected |

| Option for the assignments | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. One `assignments.ini` per node** | A file and a manager method to change it | The remove of an instance edits it under the lock | **Chosen**: assignment toggles move no server revision, and the node's whole map is one read |
| B. A `Presets` key in each `server@<id>.ini` | No new file | Every toggle moves the server revision (409s on an open Server tab); the whole-file saves must carry it | Rejected |
| C. A key in each pool section | No new file | The pool would no longer be a file llama-server can read (macOS reads it directly) | Rejected |

**Today's machines.** The packages ship `assignments.ini` holding `[default]` and `* = inherit`, copied
only when missing (the defaults' rule), so an upgrade or a fresh install serves on `default` exactly
what `presets.ini` serves today, and the GUI and `pai`, which know nothing of assignments, keep their
meaning: a preset they add or switch on is served by `default`. An administrator who wants `default`
to serve less replaces the `*` with explicit keys. No file is moved by hand; the "move by hand once" of
the earlier version of this record is void.

**(b) The checks.** The pool files keep their content rules (`presets_file`, `presets_disabled_file`,
`preset_ids_free`: ids unique across the pool) without `ModelsMax`, which moves to the instances. The
resource checks run on each instance's render: for every write, the manager renders the instances the
write touches and checks, in each render, only the sections that are new or changed against the
current render (the rule `presets_file` already has), so a preset already applied is never re-judged.
A refusal names the preset and the instance (`args` `[preset, instance]`) and writes nothing.

1. **`ModelsMax`**: the render's `load-on-startup` count against that instance's `ModelsMax`
   (`startup_count`, as today, per instance).
2. **Memory: the full-GPU estimate** (user, 2026-11-17: refuse anything over it, even with `fit` on or
   a CPU placement). The estimate is what the preset would take if every tensor llama.cpp can put on a
   GPU were there: the tensor bytes of the `model` file (every shard), less the input embedding tables
   (`token_embd`, `per_layer_token_embd`: llama.cpp keeps them in host memory whatever the placement),
   plus the KV cache for `ctx-size` tokens at `cache-type-k`/`-v` (f16 when absent); an absent or `0`
   `ctx-size` is the model's trained context (`<arch>.context_length` in the header), as llama.cpp
   reads `n_ctx = 0` (`common/common.h:471` in the v1.0.0 clone). The placement keys (`fit`,
   `n-gpu-layers`, `n-cpu-moe`, `cpu-moe`, `override-tensor`, `device`, `tensor-split`) do not lower it.
   Refused (`model-profile-over-memory`, `args` `[profile, instance, needed MiB, available MiB]`) when
   it exceeds the sum over the node's GPUs of total memory less `GPU_RESERVE` each. Read: the GGUF
   headers (`pai_core::gguf_shape::local_shape`: tensor sizes, input embeddings, KV per token) and the
   GPUs (total MiB per card from the engine's `llama-server --list-devices`, one card seen by two
   backends counted once, as `gpu_pool` does). Not counted: the multimodal projector, a draft model,
   compute buffers, and other instances on the same GPUs: every instance is checked alone against all
   the node's GPUs (user, 2026-11-17). Root opens the path a `pai` member wrote: a regular file only
   (no FIFO or device, `local_shape` since 10a), the read capped at `MAX_HEADER`; the reply gives the
   two sizes, nothing of the file's content.

   *Slice 10b's decisions (2026-11-19, architect; each a preference unless sourced):*
   - **Where and which delta**: the same two places as the model-file check (`set_model_profile`, and
     `check_contents` for `Models` and `ModelsDisabled`), after it, per instance in id order. A section
     of the new render is judged on its effective keys (its own, else the render's `[*]` ones: llama.cpp
     cascades the global section into every profile, `tools/server/server-models.cpp` 766-769 in
     v1.0.0). Refused when `needed > available` **and** `needed` is larger than the same profile's
     estimate in today's render (0 when it is not in effect today); `needed` is that one profile's
     estimate, never a sum over the render. So a save of an unrelated key, or a switch off, never fails
     on a profile that was already over (GPUs swapped, a hand edit), and any write that makes a profile
     larger is judged. Today's estimate is found by section id (as built, 2026-11-19): a byte-identical
     section estimates the same, so it is not growth, and no byte compare skips it (every section in
     effect has its header read at each pool write, capped at `MAX_HEADER`). A client can never write
     `[*]` (refused `invalid-model-profile-id`), so the cascade reaches the check only from a
     hand-edited `[*]` kept by a save; no client write grows a profile through `[*]`. The GPUs are
     asked only once a section has grown.
   - **The keys**: `model` (resolved as in 3), `ctx-size` (absent, `0` or not an integer: the trained
     context, else `REF_CTX` when the header has none), `cache-type-k`, `cache-type-v` (absent: `f16`;
     the content checks already refuse a value outside `KV_CACHE_TYPES`). The KV term is the f16 bytes
     per token of `kv_per_token` times the context times the mean of the two types' sizes relative to
     f16, per 32 elements f32 128, f16 and bf16 64, f8_e4m3 32, q8_0 34, q5_1 24, q5_0 22, q4_1 20,
     q4_0 and iq4_nl 18 (`ggml/src/ggml-common.h` block `static_assert`s, `ggml/src/ggml.c` type traits
     for f8_e4m3; exact when K and V share a type or a head size). A layout `kv_per_token` cannot read
     costs `FALLBACK_KV`. Only those long spellings are read (`c = ...` escapes the check; the
     configurator and the web UI write `ctx-size`).
   - **Not judged** (no refusal, no estimate): a profile with a fetch key (`FETCH_KEYS`, as 3), no
     `model`, or a `model` whose header cannot be read (missing, not GGUF, over `MAX_HEADER`): the
     launch fails as it does today, and 3 already refuses a missing file the write adds.
   - **The GPUs**: the manager runs the engine's `llama-server --list-devices` (the exe the launch uses,
     `paths::llama_server_exe`, found beside the manager's own root-owned binary or through
     `HKLM\Software\llama.cpp`) **as itself** (root, LocalSystem), not as the instance's account: totals
     are per card, not per account, and dropping to `private-ai` with its `render video` groups needs
     supplementary groups, which std's `CommandExt` has only on nightly (`groups`), or on Windows a
     virtual account's token. Killed after 30 s. Cached in the `Manager` for its life, keyed by the
     exe's path and modification time (an engine upgrade replaces the file; a GPU change takes a reboot,
     which restarts the manager); a failed probe is not cached. Probed only when a section is judged.
     As built (`pai_core::proc::run_hidden_probe_within`): no stdin; stdout and stderr drained by one
     thread each, keeping 4 MiB of each, so a chatty engine never blocks on a full pipe; at the bound
     the child is killed and reaped and its readers left detached; after a normal exit each stream is
     awaited for a 2 s grace and taken as empty past it (a helper of the child that inherited the pipe
     would otherwise hold the manager past the bound). The exe path is resolved once, at the manager's
     start (`ProbedGpus::for_node`): llama.cpp installed after the manager started is seen at its next
     start.
   - **No GPU, or no list** (spawn failed, timeout, no card with a size): the check is skipped. A
     CPU-only node would otherwise refuse every profile, and a broken probe would block every save,
     the fix of the engine included; the check guards the node's memory, not a permission (the writer
     already holds `configure`), so skipping widens nobody's rights.
   - In tests the GPU totals come from a seam on `Manager` (as `FileGroup`, `PasswordChecker`).
3. **The model file**: a `model` path must be a regular file on the node (each shard of a split one); a
   preset with `hf-repo` instead is not refused (the engine downloads it). Readability by the
   instance's account is not checked (a file in a 0700 home passes and fails at load, as today).
4. **The engine's keys**: the pool belongs to llama.cpp, so a llama.cpp instance accepts every pool
   key (llama-server checks its own keys at load). The rule is written as "the pool's engine is the
   instance's engine" and becomes a key list per engine when Strata shares a pool (its step).

Which writes run them: a new assignment or override in effect on; the global switch on (every instance
where the preset is assigned without `off`); a preset's save (every instance where it is in effect);
a `ModelsMax` lowered in a server save (that instance). A switch off, an assignment removed, a `*`
narrowed never fail a resource check. The model-file check refuses only a missing file the write ADDS: per instance, the
(profile, resolved path) pairs missing from the new render minus those already missing from today's
(slice 9, 2026-11-19). So a save of another profile, or of a key beside `model`, lands while a profile
in effect has lost its file outside the framework (the launch then fails as before); a decision, the
alternative (refuse every write while any render misses a file) blocking the fix of an unrelated key.
A save writes whichever pool file holds the profile, `models.ini.disabled` included (a profile there
is in effect where an `on` override assigns it), so both files' writes run the check.

**(c) Port and tags** (unchanged). The port stays `Port` in the instance's server profile, `server@<profile>.ini`, the profile its `instances.ini` section names (`ServerProfile`), never `server@<instance id>.ini`. Tags go in the same file,
`[Server] Tags = coding, fast`, a framework key never passed to llama-server, a `ServerConfig` field so
the GUI's and the web's whole-file saves keep them; 0 to 8, each 1 to 32 of `a-z`, `0-9`, `-`, no
duplicates, refused `server-bad-tags` (a preference). A `PUT .../server` body without `Tags` is 400
`server-malformed`, like any other key the read gives (as built in slice 12, replacing "absent keeps
the file's tags": the web UI is embedded in the bff, so no older page sends a body without it, and a
refusal cannot erase tags); `[]` clears them. The Overview gets them, with the port, through
a new unauthenticated call **`Instances() -> [{id, engine, port, profile_tags, instance_tags}]`** (D-Bus
`a(ssqasas)`, pipe `Instances`; (d')), the configured port of that profile (a running instance may use the
previous one until a restart), port 0 and no profile tags when the profile file is missing.

**(d) Manager protocol 3**, each guarded method with its `As` form, under `configure` and the `writing`
lock, recorded like every write:

- `Instances()`, above.
- `SetAssignment(instance, preset, value)`, `value` one of `none` (unassign), `inherit`, `on`, `off`;
  the key `*` takes `none` or `inherit`. Refuses `NoInstance`, a preset id the pool does not hold
  (`no-preset-named`, except `*`), and the resource checks; then rewrites the instance's section and
  its render. `assignments.ini` is readable through `ReadFile` like the pool files, never written by
  `WriteFile`.
- `CreateInstance(engine, id, port, tags)`: `engine` must be `llama.cpp` (`unknown-engine`; the
  argument now, so Strata adds no method); `valid_instance_id`, unique across the engines' directories
  (one today); `InstanceExists` when `server@<id>.ini` exists; the port through `checks::server_file`
  and `check_address`, which also refuses the bff's own listen port (`[Bff] Listen` in `bff.ini`,
  `AddressInUse` with `private-ai-bff`). Writes `server@<id>.ini` = `render_for(ServerConfig { port,
  tags, ..Default }, true)` and an empty render; boot start off; starts nothing. A new instance is
  assigned nothing; the administrator assigns presets from the pool on its Presets tab.
- `RemoveInstance(id)`: `NoInstance`; `InstanceProtected` for `default`; `InstanceRunning` unless
  `inactive` or `failed`. Then `ServiceControl::remove` (Linux `DisableUnitFiles`; Windows
  `DeleteService`, whose service may stay marked for deletion until its handles close, so a create of
  the same id meanwhile is a service error:
  [DeleteService](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-deleteservice),
  remarks), its section of `assignments.ini`, its render, and `server@<id>.ini` last, so a failure
  halfway leaves an instance that lists and can be removed again. Presets, models, snapshots and the
  Windows log stay.
- `Start` and `Restart` refuse `NoPresets` before any stop when the render has no section but `[*]`
  (`launch::has_presets_in` changes the same way).

Trust: the methods cross the boundary every write crosses (the bus or the pipe, the caller identified
by the OS, `configure` from polkit or the token, or the bff vouching for a current `pai` member). A
compromised bff can, during a member's session, assign and unassign presets and create and remove
stopped instances other than `default`, as it can already rewrite the files. Every refusal writes
nothing.

**(e) Snapshots per instance** (unchanged). An unset `StateDir` becomes `instance_state_dir()/<id>`
(as built in slice 11, the launch passes `instance_state_dir().join(id)` to `with_instance_defaults`).
`default` included, on every OS (coordinator, 2026-11-20: pre-release, no migration of the old root
folder), so Linux's shipped `crates/core/defaults/server@default.ini` must stop setting `StateDir` to
the root (open at the slice 11/12 review: its line 8 still does), like Windows'
(`crates/core/defaults/windows/server@default.ini`), which sets neither folder; every reader follows
(as built): the client restore
(`pai-app` `slot_state::restore_newest_with`, `effective_state_dir` given the root), the GUI and CLI
hints and the bff server document's `defaults.StateDir` (`crates/bff/src/server.rs` 95). The instance's
cwd and the model-file and memory checks keep the root (`instance_current_dir`).

**(f) The bff.** Every node-scoped route sits under the node's prefix `api` (`GET /api/node`, behind a session like every read since a600a46:
`{"name": <host name>, "api": "/api"}`, plus `version` and `engines` since slice 13 (after (g')); hub mode answers `GET /api/nodes` with the same objects and
`api` = `/api/nodes/<node>`): `instances` and everything below it, `presets` and everything below it,
`engines`. At `/api` in every mode: `version`, `session`, `node`, `nodes`.

- `GET /api/instances` items `{id, engine, state, port, tags, removable}` (as built in slice 12, without
  `removable`: (f')). The handler asks `ListInstances` (the ids, in the manager's order), `Instances`
  (port and both tag lists; a failure of that call, an older manager's, leaves port 0 and no tags) and
  `Status` per id (the state, which `Instances` does not carry); a `NoInstance` status drops that item,
  any other failed status read, an unreachable manager or a refusal fails the list (as built since
  step 10; the earlier "sorted by id, state `unknown`" was never built).
- `POST /api/instances` and `DELETE /api/instances/{id}`, as in (i) below.
- **The pool stays at `/api/presets`**: the list, `GET`/`PUT /api/presets/{preset}` (the save applies to
  every instance it is in effect on) and `PUT /api/presets/{preset}/enabled` (the global switch), same
  bodies, revisions and errors; a resource refusal is 422 with `args` `[preset, instance]`; `running`
  is true when an instance the preset is in effect on is `active` or `activating`. The list rows gain
  `assignments` ((j) 5).
- **An instance's Presets tab** (its exact shape in (j)): `GET /api/instances/{id}/presets`, the pool
  rows with this instance's `value` (`none`, `inherit`, `on`, `off`) and `effective` (served by this instance after its next
  start); `PUT /api/instances/{id}/presets/{preset}` body `{"value": ...}`, 200 `{effective, running}`,
  the resource refusals 422, 404 `no-preset-named`, 404 `client-no-instance`.
- New keys: `client-instance-exists`, `client-instance-running`, `client-instance-protected`,
  `client-no-presets` (409 each), `server-bad-tags`, `preset-over-memory`, `preset-model-missing`,
  `preset-engine-mismatch` (422 each), `instance-bad-id`, `instance-malformed`.

The contract (`fe/briefs/api-contract.md`) is the web UI session's; the coordinator hands it this part.

**(g) GUI, CLI, Windows.** `pai-config` and `pai` keep the pool files as they read and write them today
(`presets.ini`, `presets.ini.disabled`), and through the shipped `* = inherit` their presets reach
`default`; showing or changing assignments, and choosing an instance, wait until asked. Their saves now
also meet the resource checks (refusals in their strings, as every manager check). macOS (`Direct`):
unchanged, llama-server reads the per-user `presets.ini`. Windows: the same files in
`%ProgramData%\llama.cpp\config\` (created by the manager, they inherit the folder's ACL),
`DeleteService` behind `ServiceControl::remove`, `GlobalMemoryStatusEx`; type-checked only.

**(h) The router and other nodes.** The router sends a request for model `m` to a running instance of
the node whose render serves `[m]`. Other nodes: an assignment always names a preset of its own node's
pool and is checked by that node's manager, where the GPUs and files are; assigning a preset to an
instance on another machine is, with the director, writing the preset into that node's pool (an
ordinary preset write there) and assigning it there. Keeping one preset equal across pools is step 14's
to design; nothing here assumes one node.

**(i) Answers to the web UI's open points** (`fe/briefs/api-contract.md`, "Planned: nodes and
instances", numbered as there):

- Rules: an instance id is 1 to 32 of `a-z`, `0-9`, `-`, not starting with `-`; a port 1 to 65535 (the
  server save's rule, one rule for create and save); tags as in (c), JSON arrays of strings on the wire.
- (1) `POST /api/instances` `{"engine": string, "id": string, "port": number, "tags": [string]}`,
  `tags` optional. 400 `instance-malformed`; 422 `{key, args, field}`, `field` the body's name:
  `instance-bad-id` (`id`), `unknown-engine` (`engine`), `not-whole-number-in-range` (`port`),
  `client-address-in-use` (`port`, `args` `[id]` or `["private-ai-bff"]`), `server-bad-tags` (`tags`);
  409 `client-instance-exists`. The rest as every write: 401, 403, 413, 503 before asking, 504
  `client-timeout` once asked, `WRITE_TIMEOUT` (15 s).
- (2) `DELETE`: the same rules; 504 `client-timeout` when the outcome is unknown; 404
  `client-no-instance`; 409 `client-instance-running`, `client-instance-protected`.
- (3) `removable` on each list item, false for `default`, whose name is one `pai-core` constant.
- (4) Tags kept when absent from a server save, as in (c).
- (5) A never-started instance is `inactive` on both OSes (Windows: `status_failure` reads a missing
  service as `Inactive`).
- (6) `client-no-presets` on `start` and `restart`, before the stop: the render has no section but
  `[*]`. Switched-off presets are not in the render.
- (7) An `/api/instances/{id}/...` route for a missing instance is 404 `client-no-instance`; a pool
  route for a missing preset 404 `no-preset-named`.
- (8) Sorted by id; one failed status is `unknown`.
- (9) As in (f).
- (10) Below 1024 not refused (Windows has no privileged ports; on Linux the instance fails at start);
  a port another program holds (Strata on 8580) shows at `start` as `failed`: a probe at create is racy.
- (11) Answered by the pool: a new instance gets presets by assignment.

**(j) Answers to the web UI's second round** (`fe/briefs/api-contract.md`, "Planned: nodes and
instances", open points numbered as there; the architect, 2026-11-17). They refine (f):

1. Accepted: `GET /api/instances/{id}/presets` answers `{"all": "inherit" | "none", "presets": [rows]}`;
   a row's `value` is the explicit key only (`none` when only `*` covers it) and `"viaAll": <bool>`;
   `PUT /api/instances/{id}/presets-all` `{"value": "inherit" | "none"}` sets `*` (the manager's
   `SetAssignment` with preset `*`).
2. Accepted: the pool row's fields plus `value`, `viaAll`, `effective`, in the pool's order
   (`presets.ini` in file order, then `presets.ini.disabled`, an id in both listed once as on); an
   administrator's session, moving the idle deadline as the pool list; the pool list's errors plus 404
   `client-no-instance`.
3. `off` has no other use than excluding one preset from `*`; `none` and `off` serve the same without
   `*`. So the fe's three options per row are right (with `all` `inherit`: inherit, on, off; with
   `none`: not assigned, inherit, on). The manager still accepts `off` in either state (no special
   case), and narrowing `*` leaves explicit `off` keys, which serve nothing.
4. Confirmed: with the global switch off, `inherit` is `effective` false and `on` true.
5. Accepted: pool rows' `assignments` are `[{instance, value, viaAll, effective}]`, sorted by instance
   id, listing every instance with an explicit key or covered by `*`; an instance with neither is not
   listed.
6. Accepted: the 200 of the preset save and of the switch gain `"restart": [ids]`, the instances where
   the preset is in effect before or after the write and that are `active` or `activating`, sorted;
   `running` is true when `restart` is not empty, or when a state could not be read (then `restart` is
   `[]`). The assignment PUT's `running` is true only when that instance is running and its render
   changed (the preset's `effective` differs before and after). The bff derives both from the
   assignments and the pool it read before the write and the states after: no protocol change.
7. Confirmed: the assignment PUT has no revision and never answers 409; the last write of one
   (instance, preset) value wins, and the 200 carries the fresh `effective` whatever another tab
   switched. It can still be 422 (a resource check) or 404.
8. `preset-over-memory` `args` `[preset, instance, needed, available]`, the sizes whole MiB in decimal
   digits (needed rounded up, available down); no `field`. The manager checks the instances in id
   order and answers the first refusal, as every check answers one; nothing is written.
9. Wanted later, not for the first slices: the instance's Presets reply gains `availableMiB` and each
   row `needMiB` (the same estimate, the headers read per row and cached by path, size and mtime): slice
   14 below.
10. `preset-model-missing` `args` `[preset, instance, path]` (the first missing file, a shard
    included), `field` `model` on a save; `preset-engine-mismatch` `[preset, instance, engine]` (the
    instance's engine), no `field`.
11. Accepted: `load-on-startup-over-max` `[count, max, instance]`, `load-on-startup-unlimited`
    `[count, instance]`, the instance appended last. The GUI's and `pai`'s texts keep working:
    `strings::fill` substitutes `{n}` by position and ignores an argument no text names
    (`crates/core/src/strings.rs` `substitute`).
12. `client-no-presets` `args` `[instance]`, as `client-instance-exists` names its id.

**Slices 1 to 3** (built 2026-11-17 under the old names):

1. **The render** (`pai-core`, pure): pool texts plus an instance's assignment section give
   `presets@<id>.ini`. Done when: `*`, explicit keys over `*`, overrides over the global switch,
   `[*]` always first, sections byte for byte, an assigned id missing from the pool rendering nothing.
2. **The manager renders and launches.** `assignments.ini` (read, never `WriteFile`), the render after
   every pool write and at start, `instance_command` on `presets@<id>.ini`, `has_presets_in` without
   `[*]`, `ModelsMax` per instance on the render. Done when: two instances with different assignments
   and `ModelsMax` get different renders, a pool write over one instance's `ModelsMax` is refused naming
   it, a client write of `presets@a.ini` is `BadFileName`.
3. **Packaging**: the shipped `assignments.ini` (`[default]`, `* = inherit`) in deb, rpm and MSI, copied
   when missing. Done when: `xtask`'s package-content tests name it.

**Revision: profiles (2026-11-17, step 10 item 13a).** The user's decisions of the same evening replace
parts of this record; where (a) to (j) above and this block disagree, this block wins, and the words
"preset" and "presets" above read as "model profile". Slices 1 to 3 (the render, the manager renders
and launches, `assignments.ini` shipped) are built under the old names; the slice list below replaces
the old slices 4 to 14.

*The decisions (user, 2026-11-17).* (1) "Preset" becomes "profile" everywhere: code, API routes, error
keys, CLI, strings, docs and files, with no migration (pre-release: the installed files are renamed by
hand); llama.cpp's own words (`--models-preset`, its `docs/preset.md`), the benchmark specs and
bench.db's recorded data stay. (2) The node's menu has "Profiles" with two children, "Server" and
"Model": two shared pools; an instance is given ONE server profile and N model profiles, and the
instance page only makes those associations. (3) The server profile holds the port and the tags, so
a server profile serves one instance at a time. (4) Both kinds are per engine (llama.cpp now, Strata
in 13c, whose own override files Strata also calls "profiles"). So `server@<id>.ini` stops being an
instance's own file.

*Names.* The table every slice and the web UI session work from:

| Old | New | Where |
| --- | --- | --- |
| preset (concept, displayed) | **model profile**; plural "model profiles" | everywhere; displayed in sentence case: "Model profile", "Model profiles" |
| the instance's server settings | **server profile**; "Server profile", "Server profiles" | everywhere |
| `presets.ini` | `models.ini` | engine dir (`/etc/llama.cpp`, `%ProgramData%\llama.cpp\config`); per user on macOS |
| `presets.ini.disabled` | `models.ini.disabled` | the same |
| `presets@<id>.ini` (render) | `models@<id>.ini` | engine dir, the manager's only |
| `server@<id>.ini` (the instance) | `server@<profile>.ini` (a server profile; same format, `[Server]`) | engine dir |
| (none) | `instances.ini` (what an instance is, below) | engine dir, the manager's only |
| `assignments.ini` | unchanged (model profiles per instance) | engine dir |
| Rust: `Preset*`, `preset_*`, `ConfigFile::Presets`, `PresetsDisabled`, `Instance(id)` | `ModelProfile*`, `model_profile_*`, `ConfigFile::Models`, `ModelsDisabled`, `ServerProfile(id)` | every crate |
| `ManagerError::NoPresets`, `LaunchError::NoPresets` | `NoModelProfiles` | manager, core |
| `pai preset <verb>` | `pai model-profile <verb>` (same verbs); `pai server show/set/boot` unchanged, acting on the server profile of `default` | CLI |
| strings keys `preset-*`, `presets-*` | `model-profile-*`, `model-profiles-*` (mechanical: `preset` -> `model-profile`) | `en_US.toml`, `cli.toml` |
| GUI tab "Presets", "Server" | "Model profiles", "Server profile" | GUI |

Never a bare "profile" in code names, routes or keys: always `server`/`model` with it, so Strata's
override files (13c) keep their own name, "Strata runtime profile" (`strata_runtime_profile` in
code), and never collide.

*(a') What an instance is now.* Options:

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. `instances.ini` per engine dir**: one section per instance, `[<id>]`, key `ServerProfile = <profile>` | one file the manager owns, read like `assignments.ini` | create and remove write two files (instances, then the render; remove the reverse), mended at the next write or start as today | **Chosen**: the engine is the directory that holds the record, as for both pools; the engine package ships its own `default` (no framework coupling to llama.cpp, item 11) |
| B. A reserved key in `assignments.ini` (`@server = big`) | no new file | a key namespace shared with model profile ids; the file stops meaning one thing | Rejected |
| C. `Instance = <id>` inside the server profile | no new file | the instance's record spread over profiles; a switch writes two profiles and moves both revisions (409 on open pages) | Rejected |
| D. One node file `/etc/private-ai/instances.ini` with `Engine =` | ids unique by construction | the framework package would ship llama.cpp's `default` | Rejected |

An instance is its section in its engine's `instances.ini`: id (the section), engine (the directory),
server profile (`ServerProfile`), model profiles (its section of `assignments.ini`). `paths::instance_ids`
reads `instances.ini` (valid ids only) instead of listing `server@*.ini`; the Windows `scm.rs` `sync`
follows it, and only from a record that reads and names at least one instance: `sync` deletes services,
so a missing, unreadable or empty `instances.ini` skips it and is logged, never read as "no instances"
(fails closed; `default` is never removed, so a healthy record always names one). Ids stay unique across
engines (the manager reads every engine's file; one today). Clients read `instances.ini` with `ReadFile` (as `assignments.ini`) and never write it (`BadFileName`).

*The server-profile pool.* Options: **one file per profile, `server@<profile>.ini`** (today's file,
format, `render_for`, whole-file revision, server checks and the bff's server document, unchanged: the
id simply becomes the profile's name; profile names follow `valid_instance_id`, a safe file name), or
one `servers.ini` with a section per profile (section-scoped revisions and saves to build, as for model
profiles, for nothing gained). **Chosen: one file per profile.** Unused profiles are allowed and serve
nothing.

*The launch* reads the instance's section, then `server@<ServerProfile>.ini` directly (no render: the
manager owns no copy that could drift), and `models@<id>.ini`. Unset `StateDir` becomes
`instance_state_dir()/<id>` (by instance, (e)) in slice 11, together with what names the old folder;
until then it stays `instance_state_dir()`. `LaunchError::NoConfig` names the missing profile file, or
`instances.ini` when the instance cannot name a valid profile (no file, section or key, an empty or
invalid value): the launch never falls back to `server@<id>.ini`. A profile changed or switched while the instance runs applies at its next start (`running` in replies).

*(b') The checks* (each refusal writes nothing):

1. **Port**: unique among the profiles **in use** by instances (every engine) and against `[Bff] Listen`;
   two unused profiles may share a port (alternatives for one instance). `check_address` iterates
   instances and their profiles instead of `server@*.ini`. Run at a save of a profile in use, at
   `SetServerProfile` and at `CreateInstance`; an unused profile may hold any address until then. The
   manager reads `bff.ini` through `pai-core` `bff_cfg`, the service's own parse: no file is the default
   address; an unreadable file or an invalid `Listen` holds no address, since the service does not start
   on it. Refusal `AddressInUse(<other instance>)`, key `client-address-in-use` `[instance]`
   or `["private-ai-bff"]`, unchanged.
2. **One instance per profile**: `SetServerProfile` and `CreateInstance` refuse a profile another
   instance uses: `ServerProfileInUse(profile, instance)`, `client-server-profile-in-use`
   `[profile, instance]`, 409.
3. **`ModelsMax`** is the profile's; the per-instance `load-on-startup` check reads the profile the
   instance uses: at a profile save (the instance using it, none when unused), at `SetServerProfile`
   (the new profile against the instance's render), at a model-profile write or assignment (as before).
4. **Engine**: structural. A server or model profile is the engine's file, and the manager looks a name
   up only in the instance's engine directory, so another engine's profile is simply not found
   (`client-no-server-profile`, `no-model-profile-named`). Check 4 of (b) and the key
   `preset-engine-mismatch` are dropped.
5. Model-file and memory checks ((b) 2, 3) unchanged, under the new names.

*(d') Manager protocol 3* (3 ships with the first new method, `SetModelProfile`; the other methods of this
table join 3 while no release carries it, and a method added after a release that shipped 3 raises the
number). Every guarded method has its `As` form, needs `configure`, runs under the `writing` lock and is
recorded:

| Method (pipe name = D-Bus name) | D-Bus signature | Refusals |
| --- | --- | --- |
| `Instances()`, no authorization | `-> a(ssqasas)`: `(id, engine, port, profile_tags, instance_tags)`, the state left to `Status`; pipe objects `{id, engine, port, profile_tags, instance_tags}` (slice 12, 2026-11-20; `server_profile` joins after `engine`, `a(sssqasas)`, in slice 13, while no release carries 3) | none; port 0 and no profile tags when the profile file is missing |
| `ServerProfiles()`, no authorization (slice 14) | `-> a(ssqass)`: `(id, engine, port, tags, instance)`, `instance` `""` when unused; pipe objects of the same fields | none; port 0 and no tags when the file cannot be read |
| `SetInstanceTags(instance, tags)` | `(sas)` | `NoInstance`, `server-bad-tags` (the rule of (c), one key for both lists: decision 2026-11-20 below); writes `Tags = ...` in the instance's `instances.ini` section, `[]` writes the key empty (as built; read as no tags) |
| `SetModelProfile(instance, profile, value)` (slice 4's `SetAssignment`, renamed) | `(sss)` | `NoInstance`, `no-model-profile-named` (exact id; not for `*`), `model-profile-bad-value` `[value]` (none/inherit/on/off; `*` takes none/inherit), `Io` when a file it reads cannot be read (only an absent file is empty), checks 1 and 3 of (b), model file and memory once built. `ModelsMax` is judged only when the call raises the instance's `load-on-startup` count, and not when its server profile cannot be read. Renders every instance, as every other write does |
| `SetServerProfile(instance, profile)` | `(ss)` | `NoInstance`, `NoServerProfile`, `ServerProfileInUse`, `AddressInUse`, `UnresolvableHost`, `load-on-startup-over-max` (the new cap against the instance's render, even when the count does not rise), `Io` when a file it reads cannot be read. The profile the instance already names answers Ok and writes nothing. Never restarts the instance: the switch applies at its next start, and until then the old address stays bound while the checks judge the new one, so another instance given the old address fails at its start, not at the save |
| `CreateServerProfile(engine, profile, port)` (port `0` = absent, answer 2 below) | `(ssq)` | `unknown-engine`, `server-profile-bad-id`, `ServerProfileExists`; writes `render_for(ServerConfig { port, ..Default }, true)` (a create-if-absent under the lock, so it never overwrites) |
| `DeleteServerProfile(engine, profile)` | `(ss)` | `NoServerProfile`, `ServerProfileInUse` |
| `CreateInstance(engine, id, serverProfile)` (replaces `(engine, id, port, tags)`: port and tags are the profile's) | `(sss)` | `unknown-engine`, `instance-bad-id`, `instance-exists` (keyed refusals, slice 16), `NoServerProfile`, `ServerProfileInUse`, `AddressInUse`; writes an empty render, then its `instances.ini` section last (slice 16); assigned no model profile; boot off |
| `RemoveInstance(id)` | `(s)` | as (d), the refusals keyed `instance-protected`, `instance-running` (slice 16); removes the service, the `assignments.ini` section, the render, then the `instances.ini` section last; the server profile stays in the pool |
| `ReadFile`/`WriteFile`/`WriteFileIf` | unchanged | names: `server@<profile>.ini` (needs `control` to read, as today), `models.ini`, `models.ini.disabled`; `instances.ini` and `assignments.ini` read only; `models@<id>.ini` `BadFileName` |

New `ManagerError`s: `NoServerProfile(profile)`, `ServerProfileInUse(profile, instance)`,
`ServerProfileExists(profile)`; `NoPresets` becomes `NoModelProfiles`. Trust: unchanged from (d); a
compromised bff can, during a member's session, also swap an instance's server profile (its port and
bind address) and create or delete unused profiles, as it can already rewrite those files.

*(f') The bff.* The routes, every one behind an administrator's session (`GET /api/instances` too, since a600a46):

| Route | Body / reply | Errors |
| --- | --- | --- |
| `GET /api/profiles/model` (was `/api/presets`) | `[{id, engine, engineAvailable, enabled, model, assignments}]` | as the old list |
| `GET`/`PUT /api/profiles/model/{id}` (was `/api/presets/{id}`) | unchanged bodies, `revision`, `restart`, `running` | `no-model-profile-named` 404, `model-profile-changed-elsewhere` 409, check keys 422 |
| `PUT /api/profiles/model/{id}/enabled` | unchanged | as the old switch |
| `GET /api/profiles/model/{id}/estimate` (later, slice 17) | as (j) 9 | |
| `GET /api/profiles/server` | `[{id, engine, port, tags, instance}]`, `instance` the id using it or `null`, sorted by id | 401, 503, 502 `client-other` (slice 14: read as the bff's own account, so no 403) |
| `GET`/`PUT /api/profiles/server/{id}` (was `/api/instances/{id}/server`) | today's server document and save, unchanged (`values`, `revision`, `defaults`, `limits`, `interfaces`, `withheld`; 200 `{revision, running}`, `running` for the instance using it) | `client-no-server-profile` 404 `[id]`, `server-changed-elsewhere` 409, `server-malformed` 400, check keys 422 |
| `POST /api/profiles/server` `{"engine", "id", "port"?}` | 201 `{id}` | `server-profile-bad-id` 422 `field` `id`, `unknown-engine` 422, `client-server-profile-exists` 409 `[id]` |
| `DELETE /api/profiles/server/{id}` | 204 | 404 `client-no-server-profile`, 409 `client-server-profile-in-use` `[id, instance]` |
| `GET /api/instances` | items `{id, engine, state, port, tags, instanceTags}` (slice 12), `boot` (slice 13); `serverProfile` and `removable` join with the slices that switch a server profile and create or remove instances: `tags` is what is shown (the profile's tags first, then the instance's, duplicates dropped, first kept), `instanceTags` the instance's own for editing (never the union: saving it back must not copy the profile's tags into the instance) | as (f) |
| `PUT /api/instances/{id}/tags` `{"tags": [string]}` | 204 | 404 `client-no-instance`, 422 `server-bad-tags` `field` `tags`, 400 `tags-malformed` |
| `PUT /api/instances/{id}/profiles/server` `{"serverProfile": <profile>}` | 200 `{running}` (true when the instance runs: restart to apply) | 404 `client-no-instance`, 404 `client-no-server-profile`, 409 `client-server-profile-in-use`, 422 `client-address-in-use`, `client-unresolvable-host` and `load-on-startup-over-max` (`field` `serverProfile` each) |
| `GET /api/instances/{id}/profiles/model` (was `.../presets`) | `{"all": "inherit" \| "none", "profiles": [rows]}` (field `presets` renamed `profiles`; rows as (j) 2) | as (j) 2 |
| `PUT /api/instances/{id}/profiles/model/{profile}` | `{"value"}` -> 200 `{effective, running}` | as (f), keys renamed |
| `PUT /api/instances/{id}/profiles/model` `{"all": "inherit" \| "none"}` (was `.../presets-all`) | 200 `{running}` | 404 `client-no-instance` |
| `POST /api/instances` `{"engine", "id", "serverProfile"}` (port and tags gone: the profile's) | 201 | (i) 1, minus the port and tag fields, plus 404 `client-no-server-profile` (`field` `serverProfile`), 409 `client-server-profile-in-use`, 422 `client-address-in-use` |
| `DELETE /api/instances/{id}` | unchanged | unchanged |

Renamed error keys (args unchanged unless said): `preset-name-taken` -> `model-profile-name-taken`;
`preset-no-model` -> `model-profile-no-model`; `invalid-preset-id` -> `invalid-model-profile-id`;
`no-preset-named` -> `no-model-profile-named`; `preset-changed-elsewhere` ->
`model-profile-changed-elsewhere`; `presets-malformed` -> `model-profiles-malformed`;
`preset-over-memory` -> `model-profile-over-memory` `[profile, instance, needed, available]`;
`preset-model-missing` -> `model-profile-model-missing` `[profile, instance, path]`;
`client-no-presets` -> `client-no-model-profiles` `[instance]`; `preset-engine-mismatch` dropped.
New: `client-no-server-profile` `[id]`, `client-server-profile-in-use` `[profile, instance]`,
`client-server-profile-exists` `[id]`, `server-profile-bad-id` `[id]`. Kept: every `server-*` key
(`server-malformed`, `server-changed-elsewhere`, `server-bad-tags`), `load-on-startup-*` (instance
last), `client-address-in-use`, `client-instance-*`, `instance-*`, `unknown-engine`. The tags rule of
(c) is unchanged, its key on the server profile.

*Answers to the web UI session (2026-11-17), refining the table above:*

1. Yes: the document of `GET /api/profiles/server/{id}` carries `"instance": <id> | null` (the instance
   using the profile, read from `instances.ini`), and the 200 of its `PUT` is
   `{revision, running, instance}`. Delete is offered only while `instance` is `null`; the manager still
   refuses a delete in use (409 `client-server-profile-in-use`), since the page may be stale.
2. `POST /api/profiles/server` takes an optional `"port"` (1 to 65535, the server save's rule, 422
   `not-whole-number-in-range` `field` `port`): `{"engine", "id", "port"?}`. The manager method is
   `CreateServerProfile(engine, profile, port)`, D-Bus `(ssq)`, `0` meaning absent (the file then has
   no `Port`, the default). No collision check at create: an unused profile serves nothing; the check
   runs when it is put in use. Create-then-edit stays possible for every other key.
3. The body of `PUT /api/instances/{id}/profiles/server` is `{"serverProfile": <profile>}`. On that
   route `client-address-in-use` and `load-on-startup-over-max` carry `"field": "serverProfile"` (the
   choice that causes them), as do `client-no-server-profile` and `client-server-profile-in-use` on
   `POST /api/instances`.
4. Confirmed: `no-preset-named` -> `no-model-profile-named`, `preset-changed-elsewhere` ->
   `model-profile-changed-elsewhere`, `preset-no-model` -> `model-profile-no-model`,
   `invalid-preset-id` -> `invalid-model-profile-id`, `preset-name-taken` -> `model-profile-name-taken`,
   `presets-malformed` -> `model-profiles-malformed`. `client-no-instance` stays on every
   `/api/instances/{id}/...` route for a missing instance; `client-no-server-profile` `[id]` is a new
   key, answered on the `/api/profiles/server/{id}` routes and wherever a body names a missing server
   profile (it does not replace `client-no-instance`).
5. (Grep safety.) The existing strings keys `profiler-attach*` (and the fe's `server-profiler-attach*`)
   share the letters `server-profile`; every new key has a hyphen right after `profile`
   (`server-profile-bad-id`, `client-server-profile-*`), which `profiler` never has, so grep for
   `server-profile-` (with the hyphen) or `-w`, never the bare `server-profile`.

*User decisions, second round (2026-11-17):*

- (a) The port stays in the server profile. A server profile is meant to be shared by instances on
  DIFFERENT nodes (the same port on different machines); on one node it serves one instance
  (`client-server-profile-in-use` and the single `instance` field stand). Sharing one profile across
  nodes is the director's concern (Part 2), not built now.
- (b) Tags are on the server profile AND on the instance (`Tags` beside `ServerProfile` in its
  `instances.ini` section, the rule of (c), its key `server-bad-tags`); an instance shows the union,
  the profile's tags first, then its own, duplicates dropped (`SetInstanceTags`,
  `PUT /api/instances/{id}/tags`, the tables above).

*Decision (2026-11-20, slices 11 and 12): one tags key.* The instance's tags and the profile's
`[Server] Tags` obey one rule (`pai-core` `checks.rs`), so they share one refusal, `server-bad-tags`
(args `[reason, list, most, least chars, most chars]`, `field` `tags`), not a new `instance-bad-tags`:
one string per strings file and per fe locale, already there (`en_US.toml`, `cli.toml`); the
alternative, a second key with the same text, costs a string in three places for no difference a user
sees. A body of `PUT /api/instances/{id}/tags` the bff cannot read is 400 `tags-malformed` (the route's
body, as `server-malformed` is the server save's), not `instance-malformed`, which stays
`POST /api/instances`'s; the web UI never sends one, so it needs no string. Both match the tests in
`crates/bff/tests/instances.rs` and the manager's (`service.rs` `manager_instance_tags_e2e`).
- (c) The instance page shows the state, Start, Stop, Restart, start at boot and the associations (one
  server profile, the model profiles); the server form lives on the server profile's page.
- (d) The Profiles menu entries are "Server profiles" and "Model profiles".
- (e) After a profile save, the message names the running instances that use it (on this node at most
  one for a server profile), each linked to its page, from the save's `running`/`instance` (server)
  and `restart` (model); no Restart button on a profile page.

Hash routes (a preference; the web UI session's designer decides): `#/profiles/server`,
`#/profiles/server/<id>`, `#/profiles/model`, `#/profiles/model/<id>`, `#/instances/<id>`; an old
`#/models` opens `#/profiles/model`.

*Packaging.* llama.cpp's packages (deb, rpm, MSI) ship, copied only when missing: `models.ini` (empty,
was `presets.ini`), `server@default.ini` (now the server profile `default`, same content),
`assignments.ini` (`[default]`, `* = inherit`) and `instances.ini` (`[default]`,
`ServerProfile = default`). On the installed machine the user renames `presets.ini` and
`presets.ini.disabled` by hand and deletes `presets@*.ini`; `instances.ini` arrives with the package and
`server@default.ini` already is the profile. Windows: the same files in `%ProgramData%\llama.cpp\config\`,
read by the instances through that folder's ACL (all services read, `S-1-5-80-0`); `sync` from
`instances.ini`; type-checked only. The `.deb` and `.rpm` of the framework require the engine's build
revision that ships the record; the framework MSI's `LlamaCppMin` can name only the release, because
Windows Installer compares three version fields
([ProductVersion](https://learn.microsoft.com/en-us/windows/win32/msi/productversion)). An engine MSI of
the same release without the record therefore installs beside it: the manager keeps the services, lists
no instance, logs it, and the launch refuses naming `instances.ini`, until the engine MSI is installed
again (same-version upgrades are allowed) or the next llama.cpp release raises the minimum. macOS (`Direct`): the per-user `presets.ini` pair becomes
`models.ini` likewise; `server.ini` stays; no instances.

*(g') GUI and CLI.* They keep acting on `default` only: its server profile is the one `instances.ini`
names (read through `Instances()`), its model profiles the pool as today; choosing an instance or a
profile waits until asked.

*Slice 13's decisions (2026-11-20, architect; each a preference unless sourced).* The slice also
carries two user requirements of 2026-11-19 (`PLAN.md` item 13): the About place (every installed
version) and a navigation of hosts holding their instances.

- **`Instances()` gains `server_profile`** now, after `engine` (D-Bus `a(sssqasas)`, pipe field
  `server_profile`), the `ServerProfile` of the instance's `instances.ini` section verbatim, also when
  its file is missing (then port 0, as today). It is protocol 3, which no release carries (the last tag
  is 2.4.0), so no version bump. Every client that needs an instance's server file asks `Instances()`
  and reads `server@<server_profile>.ini`:
  - `pai-app` `ServerControl` (`ConfigFile::Server`, `server_control.rs` 154) for `INSTANCE`, so the
    GUI and `pai`; and the import's target (`migration.rs` 74), which writes the per-user `server.ini`
    to that name and judges "the manager already has it" on that name. `Direct` (macOS) keeps
    `server.ini`.
  - The bff's server document, `GET`/`PUT /api/instances/{id}/server` (`crates/bff/src/server.rs` 42,
    616): the route stays where it is (its move to `/api/profiles/server/{id}` is slice 14's).
  - Failures, closed: `default` (or `{id}`) not in the list is `NoInstance` (the bff's 404
    `client-no-instance`); an `Instances()` that fails is its error (unreachable 503, an older
    manager's `Other` 502 `client-other`, the GUI its usual refusal), never a fall back to
    `server@<id>.ini`, which would read or write a file the instance does not use.
- **The installed versions are files the packages ship**, read by the bff. Each engine package and
  each GPU runtime package installs one file, `versions/<package>`, holding its own package version
  (one line, as `xtask package` gives it to the package): Linux
  `/usr/share/private-ai/llama.cpp/versions/{llama.cpp,llama.cpp-rocm,llama.cpp-cuda,llama.cpp-vulkan}`,
  each file owned by its own package (`components::of` maps `versions/<package>` to the component whose
  `package()` it is, ahead of the `usr/share/private-ai/llama.cpp/` rule); Windows
  `<InstallDir>\share\versions\` in llama.cpp's one MSI (`InstallDir` from `HKLM\Software\llama.cpp`,
  `paths::engine_install_dir`): `llama.cpp` with the MSI's version, and one file per runtime the
  package bundles, named as the Linux package (`llama.cpp-rocm`, `-cuda`, `-vulkan`), holding
  `components::dist_version` of the dist it was bundled from (empty when the dist is outside the
  layout). macOS: none (no bff). The framework's version is the bff's own (`CARGO_PKG_VERSION`, as
  `GET /api/version`): the packages pin the framework to one version.

  | Option for reading the versions | Cost | Risk | Verdict |
  | --- | --- | --- | --- |
  | **A. One version file per package, the bff reads the directory** | xtask writes the files (Linux, Windows), one `of` rule, a pure reader in `pai-core` | A file can only say what our build shipped: a hand-copied library is not seen (not a package, not in scope); packages built before this slice show no engine until reinstalled | **Chosen**: one mechanism on both OSes, no process spawned, no deb or rpm question, tested on a temp dir |
  | B. The package database (`dpkg-query -W`, `rpm -q --qf`) on Linux, registry values the MSI writes on Windows | No Linux packaging change | Two mechanisms; a machine can have both tools (this one has `/usr/bin/rpm` beside dpkg), so which database owns the install is a guess; rpm renders epoch and release apart from dpkg; a process per request | Rejected |
  | C. The manager reads, behind a new unauthenticated `Versions()` | Ready for the hub's remote nodes | A protocol method for public data the bff can read itself (Linux `/usr/share` under `ProtectSystem=strict` stays readable, systemd.exec(5); Windows' `Program Files` and `HKLM\Software` are readable by Users) | Waits for step 14: the reader lives in `pai-core`, so the remote node's manager wraps it then |

  The reader (`pai_core::versions::installed(dir)`): every regular file whose name is `llama.cpp` or
  `llama.cpp-<a-z0-9>+`, at most 128 bytes, its first line trimmed, printable ASCII, else the entry is
  skipped; empty is `null`. No directory (a tree build, an older package, no engine installed) gives
  no engine. The engine is listed only when its own file `llama.cpp` is there; its backends are the
  other files, sorted by name. Read at each request (an engine upgrade does not restart the bff). Trust:
  the directory is root's (Windows: the MSI's, under `Program Files`), the same trust as the binaries
  beside it; the reply is public package data, shown only to a session.
- **`GET /api/node`** (behind a session, 401 `session-required`, asks the manager nothing, moves no idle
  deadline, as `GET /api/version`):

      {"name": "<host name>", "api": "/api", "version": "2.99.1",
       "engines": [{"engine": "llama.cpp", "version": "1:1.0.0+pai2",
                    "backends": [{"name": "cuda", "version": "13.5.0+nccl.2.32.3"},
                                 {"name": "rocm", "version": "10.1.0"},
                                 {"name": "vulkan", "version": "1.4.363.1"}]}]}

  `name` is `listen::hostname()` (the DNS host name on Windows), `localhost` when it is empty; a
  backend's `name` is the package suffix (the display names are the fe's strings); `engines` is `[]`
  when none is found, `backends` `[]` for a CPU-only engine. No error case but 401: a missing or
  unreadable file is left out. Versions live here, not in a route of their own: About and the
  navigation read one document; whether hub mode's `GET /api/nodes` carries `engines` is step 14's.
- **`GET /api/instances` items gain `boot`**: `true`/`false` from `BootEnabled(id)` (unauthenticated,
  as the list's `Status`), `null` when that one read fails; a failed boot read never fails or shortens
  the list. `GET /api/instances/{id}/boot` stays (the instance page and after a `PUT`).
- *As built (2026-11-20 review, accepted):* `Instances()` answers in id order, as `ListInstances`
  (`service.rs`), not the record's; a section naming no `ServerProfile` answers `""` on the wire and
  the clients leave that row out (`client/dbus.rs`, `client/pipe.rs`), so its server file is
  `NoInstance` and the bff's list, driven by `ListInstances`, still shows the id with port 0 and no
  tags; an older manager's row shape is `Other`. The GUI's shown path asks `Instances()` and falls
  back to `server@default.ini` for display only, never for a read or write.

**Slice 13 done when** (test first; Linux, the Windows code type-checked):
`pai-manager`: an instance whose section names `big` lists `server_profile` `big` with `big`'s port, on
D-Bus and the pipe; a missing profile file lists the name with port 0. `pai-app`: with a fake client
whose `Instances()` names `big` for `default`, the server read and save go to `server@big.ini` and no
call names `server@default.ini`; `Instances()` failing reads and writes nothing; `default` absent is
`NoInstance`; the import writes `server.ini` to `server@big.ini`. `pai-bff`: `GET`/`PUT
/api/instances/default/server` read and write `server@big.ini`; an id absent from `Instances()` is 404,
`Instances()` unavailable 503; `GET /api/node` is 401 without a session and, from a versions-directory
seam, the document above; a missing directory gives `engines: []`, a file over 128 bytes, with a
control character or a foreign name is skipped, an engine file alone gives `backends: []`; list items
carry `boot`, `null` when one `BootEnabled` fails, the list still 200. `xtask`: the `.deb` and `.rpm` of
`llama.cpp` and of each runtime hold their own `versions/<package>` file with the package's version;
the Windows staging holds `share\versions\llama.cpp` and one file per bundled runtime with its dist
version. Each crate's clippy and fmt green.

*Slice 14's decisions (2026-11-20, architect; each a preference unless sourced).*

- **Manager calls**, each with its `As` form, `configure`, the `writing` lock, one action record
  (`create_server_profile`, `delete_server_profile`, target the profile), in this order, a refusal
  writing nothing:
  - `CreateServerProfile(engine, profile, port)`: `authorize_as`; `engine` other than `llama.cpp`
    (the one engine the manager serves) is `Invalid(unknown-engine [engine])`; a profile name that
    fails `paths::valid_instance_id` (the rule `config_file` already applies to `server@<x>.ini`, so
    no name builds a path outside the engine directory) is `Invalid(server-profile-bad-id [profile])`;
    any entry already at `server@<profile>.ini` (`symlink_metadata`, so a dangling link counts) is the
    new `ServerProfileExists(profile)`. Writes `render_for(&ServerConfig { port, ..Default }, true)`
    with `write_served_file` (mode 0640, group `pai`, as a save), `port` `0` = `None`; no address or
    cap check (answer 2). The check-then-write is safe because every manager write holds `writing`.
    `WriteFile` of a new `server@<x>.ini` still creates one too; Create adds the never-overwrite answer.
  - `DeleteServerProfile(engine, profile)`: `authorize_as`; `unknown-engine`; a bad name or no file is
    `NoServerProfile(profile)`; `instances.ini` read with `read_record` (absent = no instance, any other
    failure `Io`, never "unused"), a section naming it is `ServerProfileInUse(profile, instance)`;
    then `remove_file`. No special case for `default`: in use it is refused like any other; unused, it
    serves nothing, and a package that copies it when missing may bring it back, also harmless.
  - `ServerProfiles()`: unauthenticated, as `Instances()`; every `server@<x>.ini` whose name passes
    `config_file`, sorted by id, `port` as `Instances()` (default when unset), `tags` the profile's,
    `instance` the first section naming it. It exposes ports, tags and names of unused profiles to any
    local caller, the class of data `Instances()` already gives for used ones; it never reads past
    `[Server]` keys, so `OpencodeApiKey` stays behind `control`.
- **Errors and keys.** New `ManagerError::ServerProfileExists(profile)` (`error_reply`, `error_kind`,
  `ClientError::ServerProfileExists`, `client-server-profile-exists` `{0}` in both strings files and
  `client.rs`'s key table). `unknown-engine` and `server-profile-bad-id` join `checks::KEYS` with one
  value each, both strings files (slice 16's `CreateInstance` reuses `unknown-engine`). Protocol stays 3.
- **Wire.** D-Bus `CreateServerProfile(ssq)`, `DeleteServerProfile(ss)`, `ServerProfiles() ->
  a(ssqass)`, `CreateServerProfileAs(sssq)`, `DeleteServerProfileAs(sss)` (principal first, as every
  `As`). Pipe: `{"method": "CreateServerProfile", "engine", "profile", "port"}`, `port` an integer
  0 to 65535 (any other type or missing field `BadRequest`), `DeleteServerProfile {engine, profile}`,
  their `As` with `principal`, `ServerProfiles` answering an array of `{id, engine, port, tags,
  instance}`. `ManagerClient` gains `create_server_profile(engine, profile, port: Option<u16>)`,
  `delete_server_profile(engine, profile)` and `server_profiles() -> Vec<ServerProfileInfo>`
  (`instance: Option<String>`, `""` read as `None`), each with the default body `Other` as its
  siblings; the D-Bus and pipe clients pick the `As` name when `acting_for` set a principal.
- **bff routes**, each 401 `session-required` with no session and nothing asked, `sessions.current`
  (a page read or a write moves the idle deadline; only polls `peek`), `NotAuthorized` 403
  `session-not-administrator` ending the session, no manager 503 `client-unavailable`, a timeout 503
  as `PUT /api/instances/{id}/tags` (a retried create answers 409, a retried delete 404, so the
  ambiguity is visible), anything else 502 `client-other`:
  - `GET /api/profiles/server`: `ServerProfiles()` as the bff's own account; the (f') row.
  - `POST /api/profiles/server` `{"engine", "id", "port"?}`: the bff checks first, the manager again:
    400 `server-profile-malformed` for a body it cannot read (no string: the web UI never sends one,
    as `tags-malformed`), 422 `unknown-engine` `field` `engine`, 422 `server-profile-bad-id` `field`
    `id`, 422 `not-whole-number-in-range` `field` `port` (args as the server save's `Port`); 201
    `{id}`; 409 `client-server-profile-exists` `[id]`.
  - `DELETE /api/profiles/server/{id}`: 204; 404 `client-no-server-profile` `[id]`; 409
    `client-server-profile-in-use` `[profile, instance]`. The engine is `llama.cpp` (one per node).
  - `GET`/`PUT /api/profiles/server/{id}`: today's document and save keyed by the profile
    (`server@<id>.ini`, read and written `As`), plus answer 1's `instance` from `Instances()` (its
    failure is its error, never `null`, which would offer Delete); `running` is `Status(instance)`,
    `false` when unused; a missing file or bad name 404 `client-no-server-profile` `[id]`.
    `defaults.StateDir` is the using instance's folder, `null` when unused (the folder belongs to
    whichever instance runs it; never the bare root).
- **The old route stays.** `GET`/`PUT /api/instances/{id}/server` keep working unchanged (the fe's
  `ServerSection.tsx` 45 and `ServerForm.tsx` 366 call them). Moving the form to the server profile's
  page is (c)'s web UI and needs the designer's brief, so it is slice 17's; that slice switches the fe
  to `/api/profiles/server/{id}` and removes the old route in the same window. Slice 14 is backend only.
- **GUI and `pai`**: nothing now ((g')).
- *As built (2026-11-20 review, accepted):* a `server@<x>.ini` that cannot be read lists with port 0
  and no tags (`service.rs` `server_profiles`), as `Instances()` answers a missing file; the bff
  checks a create's `port` as an integer (`as_i64`), so a string or fraction is 400
  `server-profile-malformed`, not 422; a create or delete that outlives the write timeout answers 503
  `client-unavailable` (`server_profiles.rs`), as `control.rs` does, with no 504 split.

**Slice 14 done when** (test first; Linux, the Windows code type-checked): `pai-manager`, on D-Bus and
the pipe: Create writes `server@x.ini` with the given port (port 0: no `Port` line), mode 0640 group
`pai`, and lists in `ServerProfiles()`; a second Create of `x`, an existing file of other bytes or a
dangling link is `ServerProfileExists` and the bytes stay; `../x`, `X`, `a_b` and an engine `vllm`
are refused with their keys and nothing is created anywhere; Delete removes an unused profile, refuses
one an instance's section names (`ServerProfileInUse` naming it, file kept), a missing one
(`NoServerProfile`), and, with `instances.ini` unreadable, answers `Io` and keeps the file; a caller
without `configure`, and an `As` for a non-member or from a non-bff caller, change nothing; each call
writes one action record. `pai-core`: the new error decodes, its text and the two new keys are in both
strings files. `pai-bff`: the four routes answer 401 without a session with the manager not called;
the list, create, delete and the profile document answer the codes above from a fake client; the
document of `big` used by `default` carries `instance: "default"` and its PUT writes `server@big.ini`;
an unused profile's document has `instance: null`, `defaults.StateDir: null`, PUT `running: false`;
`/api/instances/default/server` still answers as before. Clippy and fmt green per crate.
Mutants (security code): `create_server_profile`, `delete_server_profile` and their pipe parse in the
manager (the gate, the never-overwrite, the in-use and `Io` refusals, the name rule), and the session
gate of the four bff routes.

*GPU distribution on the model profile page (`PLAN.md` item 13, fe brief 02 row 2f; architect,
2026-11-20; each a preference unless sourced).* The web edits a model profile's `device`, `split-mode`
and `tensor-split` as the GUI's GPU table does (`crates/app/src/gpu_split.rs`: the table and its four
states stay the GUI's; the web UI rebuilds them from the device list below). Two slices, **G1** then
**G2**, both after slice 14 is committed: they touch the same files (`checks.rs` `KEYS`, both strings
files, `client.rs`, `dbus.rs`, `pipe.rs`, `service.rs`, the bff router), so running beside it only buys
merge conflicts.

1. **Where the device list comes from: a manager read, `Devices()` and `DevicesAs(principal)`.**
   - *Options.* (a) The bff probes itself: rejected. It runs no engine and, on Linux, the system user
     `private-ai-bff` is in neither `render` nor `video`, so ROCm and most GPU nodes would list nothing;
     adding it would give the network-facing process GPU device access to read one list. It would also
     duplicate the manager's probe and cache, and across machines (Part 2) the devices are the node's,
     which only the node's manager can see. (b) A manager read with no authorization, as `Instances()`:
     any local account could then make root (LocalSystem) spawn the engine, again on every call while
     the probe fails. (c) **Chosen**: `Devices()` under `control` (the quiet check, no interaction, as
     `Status`) and `DevicesAs(principal)` as every `As` method (the bff's account only, a current member
     of `pai`; otherwise `NotAuthorized`, the session ends). A refused call runs no probe. Not recorded
     (a read), not under `writing`.
   - *The probe is the memory check's* (slice 10b-3, `service.rs` `ProbedGpus`), widened, not doubled:
     the cache holds the parsed `Vec<DeviceOption>` and the probe's time instead of the reduced pool;
     `gpus()` returns `gpu_pool` of it, `None` when that is empty (the memory check is unchanged). A run
     that completed and parsed at least one device line (the CPU line counts) is cached, so a CPU-only or
     iGPU-only node probes once; a spawn failure, a kill at `GPU_PROBE_TIMEOUT` or no device line at all
     stays uncached, as today. Probes are serialized: a second caller waits for the running probe and
     reads its result (a mutex held across the run, the cache re-read after taking it), so no caller can
     have more than one engine child running. The args are fixed (`--list-devices`), the exe is
     `paths::llama_server_exe()`: nothing from a caller reaches the command line.
   - *An entry*: `id` (the token `device` takes, as the probe prints it: `ROCm0`; llama.cpp matches it
     case-insensitively, `striequals` in `ggml_backend_dev_by_name`, `ggml/src/ggml-backend-reg.cpp` in
     v1.0.0), `name` (`DeviceOption.name`, the product name), `backend` (the id without its trailing
     digits, case kept: `ROCm`, `CUDA`, `Vulkan`), `total_mib`, `free_mib` (free AT THE PROBE: the cache
     lives until the manager restarts or the engine exe changes, so it is shown with `probed_at`, never
     as now). Every device the probe lists except the CPU pseudo-device (`DeviceOption::is_cpu`; llama.cpp
     refuses CPU in `device`, `common/arg.cpp` `parse_device_list`), in probe order, the duplicate views
     of one card (CUDA0 and Vulkan1) and integrated GPUs included: the GUI's table lists the same rows.
     Source of truth: the engine's own `--list-devices`, run as the manager. Caveat, as for the memory
     check: the instance runs as `private-ai` (Windows: its virtual account), which could see fewer
     devices than root; untested.
   - *Wire.* D-Bus `Devices() -> (t, a(ssstt))`: `probed_at` Unix seconds UTC (0 = no probe answered),
     then `(id, name, backend, total_mib, free_mib)`; `DevicesAs(s) ->` the same. Pipe: `{"method":
     "Devices"}`, `{"method": "DevicesAs", "principal"}`, answering `{"probed_at", "devices": [{"id",
     "name", "backend", "total_mib", "free_mib"}]}`. `ManagerClient::devices() -> Result<NodeDevices,
     ClientError>` (`probed_at: Option<u64>`, `0` read as `None`; `devices: Vec<DeviceInfo>`), default
     body `Other` as its siblings; the D-Bus and pipe clients pick `DevicesAs` when `acting_for` set a
     principal. Protocol stays 3 (no release carries it). A cold probe can last up to
     `GPU_PROBE_TIMEOUT` (30 s): zbus sets no method timeout unless asked (zbus 5.19.0,
     `connection/builder.rs`, `method_timeout: None`); dbus-daemon's own `reply_timeout` default is
     unverified. If a transport cuts the call, the probe still finishes and caches, and the next read is
     instant.

2. **The bff route: `GET /api/node/devices`** (beside `GET /api/node`; per node, so Part 2's node
   prefix fits it unchanged). 401 `session-required` with no session, the manager not connected to; a
   page read, so `sessions.current` (moves the idle deadline: the profile page fetches it once when it
   opens, never polls); `DevicesAs` as the signed-in user; `NotAuthorized` 403 `session-not-administrator`
   ending the session; no manager or no answer within `DEVICES_TIMEOUT` (`GPU_PROBE_TIMEOUT` plus 5 s,
   longer than a read's 5 s `MANAGER_TIMEOUT` for the cold probe) 503 `client-unavailable`, as the other
   reads; anything else 502 `client-other`. 200:

   ```json
   {"probedAt": "2026-11-20T09:14:03Z",
    "devices": [
      {"id": "ROCm0",   "name": "AMD Radeon AI PRO R9700",       "backend": "ROCm",   "totalMib": 32624, "freeMib": 32420},
      {"id": "CUDA0",   "name": "NVIDIA GeForce RTX 4070 SUPER", "backend": "CUDA",   "totalMib": 12281, "freeMib": 10844},
      {"id": "Vulkan0", "name": "AMD Radeon AI PRO R9700",       "backend": "Vulkan", "totalMib": 32624, "freeMib": 32420}]}
   ```

   Probe order, CPU left out, `probedAt` RFC 3339 UTC. No probe answer (no engine, a kill, no device
   line) is 200 `{"probedAt": null, "devices": []}`, and a CPU-only node is `devices: []` with a time:
   either way nothing to distribute, never an error page. The ids a profile names that the list lacks
   are the web UI's to show (as the GUI keeps a `(custom)` row, `crates/app/src/devices.rs`
   `build_options`), not the route's.

3. **The save path.** `device`, `split-mode` and `tensor-split` move from
   `checks::MODEL_PROFILE_KEYS_NOT_ON_WEB` to `checks::WEB_MODEL_PROFILE_KEYS` (a "GPU distribution card"
   group); `pai-app`'s `web_key_lists_are_exactly_what_render_section_writes` keeps pinning the union.
   The body carries them as strings, as every key (`"device": "ROCm0,CUDA0"`, `"tensor-split": "3,1"`,
   `"split-mode": "layer"`), the save `PUT /api/profiles/model/{id}` unchanged otherwise. What exists:
   `split-mode` is in `MODEL_PROFILE_ENUM_KEYS` (`not-one-of`), and `moe_cache_split` refuses a positive
   `moe-cache-mib` under `tensor` (`moe-cache-with-tensor-split`). There is **no** device-name check and
   **no** tensor-split check today (`checks.rs`), and both values make the model fail to load, so G1 adds
   two rules to `model_profile_section` (the manager judges; the bff maps the refusal to 422 with its
   key and `field` as every check key):
   - `device_list(key, value)`, for `device` and also `device-draft` and `mmproj-device` (already on
     the web, same parser): split on `,`; every entry non-empty, with no whitespace (llama.cpp does not
     trim an entry: `ROCm0, ROCm1` is `invalid device:  ROCm1`, `string_split` then `parse_device_list`,
     `common/arg.cpp` v1.0.0), none of `; # =`, not `CPU` in any case; `none` only alone. Key
     `device-bad-name` `[key, entry]`.
   - `tensor_split(value)`: entries split on `,` or `/` (llama.cpp's regex `[,/]+`), at most 15 (it
     throws at `>= llama_max_devices()`, 16, `common/arg.cpp`, `src/llama.cpp`), each a finite number
     `>= 0` (`std::stof` throws on a non-number; a negative weight is refused as nonsense, a preference).
     Key `tensor-split-format` `[value]`.
   - **Not judged against the probed list** (options: refuse an id the probe lacks, or skip when the
     probe failed). A card can vanish for a while (the ROCm runtime missing turns ROCm0 into Vulkan only,
     the "ROCm availability" note in `crates/app/src/devices.rs`), and since the rules run on a whole
     changed section, that would block saving any other key of every profile naming it, the GUI's
     included (it saves through the same manager and keeps unknown ids on purpose). The probe also runs
     as root, not as the instance. So a well-formed unknown id saves; the web UI flags it. The GUI's
     warnings (`split-tensor-no-gpu`, `split-tensor-mixed-backends`) stay warnings, the UI's.
   - The memory check still counts every card of the node, whatever `device` says (user, 2026-11-17;
     superseded 2026-11-21: only the server profile's `Devices`, "Resources belong to the server profile").

4. **Done when** (test first; Linux, the Windows code type-checked).
   - **G1** (`pai-core`, `pai-bff`): the three keys are in `WEB_MODEL_PROFILE_KEYS` and out of the
     other list, `pai-app`'s pin test green; `device-bad-name` and `tensor-split-format` in `KEYS`, both
     strings files and decode; refused, each naming its key: `device = ROCm0, ROCm1`, `CPU`, `cpu`,
     `ROCm0,,CUDA0`, `none,ROCm0`, `ROCm0;x`, `device-draft = a b`, `tensor-split = 3,x`, `-1,1`, `inf`,
     sixteen entries; accepted: `ROCm7` (not in any probe), `none`, `3/1`, `1.5,0`, fifteen entries. The
     bff's PUT saves `device`, `split-mode`, `tensor-split` from the body and answers 422 with the key
     and `field` from a refusing fake; `moe-cache-mib = 4096` with `split-mode = tensor` is still
     refused through it.
   - **G2** (`pai-manager`, `pai-core`, `pai-bff`): with the fake exe of `tests/gpu_probe.rs` printing
     `BLOCK`, `Devices()` answers its non-CPU lines in order with id, name, backend, both sizes and a
     non-zero time, and a `Devices()` plus a memory-checked write run the exe once; `NO_CARD` answers
     `[]` with a time and is cached (memory check still `None`); a killed or silent probe answers `[]`
     with time 0 and is not cached; two `Devices()` at once run the exe once; a caller without `control`,
     and `DevicesAs` from a non-bff caller or for a non-member, are `NotAuthorized` with the exe's
     counter at 0; D-Bus and pipe answer the same. The bff route answers 401 without a session (manager
     not called), the JSON above from a fake, `[]` and `null`, 403 ending the session, 503 for no manager
     and for an answer past its deadline, and moves the idle deadline. Clippy and fmt green per crate.
   - **Mutants** (security code): the gate of `Devices`/`DevicesAs` (`authorize`, `authorize_as`, the
     pipe parse of `DevicesAs` and its principal) and the route's session gate. Not security code: the
     two value rules and the key move (the move is pinned by `pai-app`'s test, the rules by their cases),
     the probe's cache and serialization.
   - **Decided by the user (2026-11-20)**: the memory check still counts every GPU of the node, not only
     those a profile's `device` names (the 2026-11-17 rule stands; superseded 2026-11-21 by the server
     profile's `Devices`, "Resources belong to the server profile"); no refresh: the page shows each
     card's free memory as of `probedAt`.
   - **As built (G1+G2 reviewed, 2026-11-21, uncommitted).** Met: the key move, both keys in `KEYS`
     and both strings files, `Devices`/`DevicesAs` under the quiet `control` check and the `As` gate
     (`service.rs` `Manager::devices`, `authorize_quietly`), the wire on D-Bus and pipe, the cache widened
     not doubled (`ProbedGpus::probe`), the route (`bff/src/node.rs` `devices`, `DEVICES_TIMEOUT` 35 s).
     Four rulings, each changing the design above:
     (a) *The device rule judges the value llama.cpp reads*, not the raw line: the preset reader
     (`common/preset.cpp` `parse_ini_from_file`, v1.0.0) drops the spaces after `=`, ends the value at
     trailing `[ \t]*` before a `;`/`#`/newline, and cuts it at the first `;` or `#`; that is
     `RawSection::get` (`cut_comment`). So `raws`/`raw()` go; `device = ROCm0 ` and `device = ROCm0 ; note`
     are taken, `ROCm0, ROCm1` and `ROCm0, ` still refused. A body value with `;`/`#` is already 422
     `comment-markers` at the bff (`model_profiles.rs` ~880), so the `ROCm0;x` case of G1 is pinned there,
     not as `device-bad-name`; `=` stays refused in an entry.
     (b) *tensor-split splits as llama.cpp does*: a run of `,`/`/` is one separator and a trailing one
     ends the list (`common/arg.cpp` `-ts`, regex `[,/]+` with `sregex_token_iterator` -1, which yields
     an empty leading piece but no empty trailing suffix, [re.tokiter.incr]
     https://eel.is/c++draft/re.tokiter.incr); so `3,,1`, `3//1`, `3,1,` are taken, `,3` refused. The
     stricter number rule stays (preference): negative, `inf`, `nan` and `3x` (which `std::stof` reads
     as 3) refused, as nonsense or a typo, never a value the GUI writes.
     (c) *A probe that parsed only the CPU line is answered but not cached* (`[]` with its time; the
     memory check `None`), restoring 10b's retry: a GPU backend that failed once is read again at the
     next page open or memory-checked write. A CPU-only node pays one quick `--list-devices` per such
     call. A partial failure (ROCm down, Vulkan up) still caches until the exe changes or the manager
     restarts: accepted.
     (d) *A reader that waited takes the answer of the probe it waited for, failure included*: no
     second probe after a failed or CPU-only one, so no reader waits past one `GPU_PROBE_TIMEOUT` and
     N queued readers never run N probes back to back.

*Slice 15's decisions (2026-11-21, architect; each a preference unless sourced).*

- **The manager has everything.** `SetServerProfile`, `SetModelProfile` (with `*` for the whole pool)
  and their `As` forms are built with every refusal of (d') (`service.rs` `set_server_profile`,
  `set_model_profile`), on D-Bus, the pipe and `ManagerClient` (`set_server_profile`,
  `set_model_profile`, `acting_for`); `ReadFile` serves `assignments.ini` and the pool to any caller;
  `Instances()` carries `server_profile` (slice 13). No protocol change; slice 15 is `pai-core` (one
  pure function) and `pai-bff`.
- **One rule for "served", in `pai-core`.** New `assign::rows(enabled, disabled, assignment) ->
  (all, Vec<AssignmentRow { id, value, via_all, effective }>)`, pool order as `render`, built on
  `render`'s own `assigned` and `is_on`: `value` the explicit key (`none` when absent or a word the
  render ignores), `via_all` no explicit key and `*` honoured, `effective` exactly "rendered". A test
  pins that the ids with `effective` are the sections `render` emits after `[*]`, so the bff never
  re-implements the render. `all` is `inherit` when the render honours `*`, else `none`.
- **Routes**, each: 401 `session-required` with no session and the manager not asked;
  `sessions.current` (reads and writes move the idle deadline); the writes and the reads of
  `assignments.ini` and the pool `As` the signed-in user, `Instances()` and `Status` as the bff's own
  account (unauthenticated calls); `NotAuthorized` 403 `session-not-administrator` ending the session;
  no manager 503 `client-unavailable`; a write past `write_timeout` 503 `client-unavailable` (slice 14's
  as-built rule, not the contract's 504: the docs agent aligns `fe/briefs/api-contract.md`); anything
  else 502 `client-other`. A refusal with `args` answers them as the manager gave them.
  - `PUT /api/instances/{id}/profiles/server` `{"serverProfile": string}` -> 200 `{running}`. 400
    `server-profile-malformed` for a body it cannot read (as slice 14's create); 404
    `client-no-instance`, 404 `client-no-server-profile` `[profile]`, 409 `client-server-profile-in-use`
    `[profile, instance]`, 422 `client-address-in-use`, `client-unresolvable-host`,
    `load-on-startup-over-max` with `field` `serverProfile`. `running`: the bff reads `Instances()`
    before the call; true when the instance named another profile (or that read failed) and `Status`
    after the call is `active` or `activating`, or `Status` failed (the safe answer); a switch to the
    profile it already runs is `false`.
  - `GET /api/instances/{id}/profiles/model` -> 200 `{"all", "profiles": [rows]}`: each row the pool
    row (`id`, `engine`, `engineAvailable`, `enabled`, `model`, without `assignments`) plus `value`,
    `viaAll`, `effective` from `assign::rows`; 404 `client-no-instance` when `Instances()` lacks the id;
    the pool list's errors otherwise. A page read, polled never.
  - `PUT /api/instances/{id}/profiles/model/{profile}` `{"value": "none"|"inherit"|"on"|"off"}` -> 200
    `{effective, running}`. 400 `assignment-malformed` for an unreadable body or a non-string value; 422
    `model-profile-bad-value` `[value]` `field` `value` for another string (the bff checks first, the
    manager again); `{profile}` `*` is 404 `no-model-profile-named` (the whole pool goes through the
    next route only); 404 `client-no-instance`, 404 `no-model-profile-named` `[profile]`; the resource
    refusals 422 with their `args` and no `field` (`model-profile-over-memory`,
    `model-profile-model-missing`, `load-on-startup-*`). No revision, never 409 ((j) 7). The bff reads
    the pool and `assignments.ini` before and after the write: `effective` is the after-read's (the
    fresh value whatever another tab did); `running` is true when before and after differ and `Status`
    is `active`/`activating`. An after-read that fails leaves the write standing: 200 with `effective`
    from the before-read and the value, `running` true.
  - `PUT /api/instances/{id}/profiles/model` `{"all": "inherit"|"none"}` -> 200 `{running}`
    (`SetModelProfile(instance, "*", all)`): 400 `assignment-malformed`, 422 `model-profile-bad-value`
    `field` `all` for another string, 404 `client-no-instance`, the resource refusals; `running` true
    when the set of effective ids changed and the instance runs.
- **Additions to served replies** (additive: the fe's type guards check fields present and ignore the
  rest, `fe/src/ModelProfiles.tsx` 40, so nothing it reads breaks):
  - `GET /api/profiles/model` rows gain `assignments: [{instance, value, viaAll, effective}]`, sorted by
    instance, every instance of `Instances()` with an explicit key or covered by `*` ((j) 5); a section
    of `assignments.ini` naming no instance is not listed. `assignments.ini` unreadable fails the list
    (502 `client-other`), never an empty map.
  - The 200 of `PUT /api/profiles/model/{id}` and of `.../enabled` gain `restart: [ids]`, sorted: the
    instances where the profile is `effective` before the write (the save) or before or after it (the
    switch, the after side computed from the same before-read with the switch flipped), whose `Status`
    is `active`/`activating`. `running` becomes `restart` non-empty, or true with `restart: []` when a
    read failed. This replaces today's "any instance up" (`model_profiles.rs` `running`), and fixes its
    blind spot: a switched-off profile served by an `on` override now names its instance.
  - `GET /api/instances` items gain `serverProfile` (`Instances()`'s, `null` when that call failed and
    the row is absent). `removable` waits for slice 16.
- **Security code (mutants):** the session gate of the four new routes (no session, manager never
  asked; 403 ends the session) and that both writes go `As` the signed-in user (a mutant dropping
  `acting_for` must fail a test: the fake client records the principal). Not security code:
  `assign::rows`, the `restart`/`running` derivation, the body parse (the manager re-judges every
  value under `configure`). Trust: unchanged from (d'); the bff vouches as before.
- **The web UI needs a slice (the designer's brief, slice 17's instance page and assignment table);
  the backend lands alone**: every route is new and every reply change additive.

**Slice 15 done when** (test first; Linux, the Windows code type-checked): `pai-core`: `assign::rows`
for `*` alone, explicit over `*`, `on` with the switch off, `off` under `*`, a hand-edited `* = on`, an
id in both pool files, no section; its `effective` ids equal `render`'s sections. `pai-bff`, from a
fake client recording each call and principal: the four routes answer 401 with no call made, 403
ending the session on `NotAuthorized`; the server switch answers `{running: true}` for a running
instance switched to another profile, `false` when stopped or already on it, each refusal of the list
with its status, key, `args` and `field`, and `SetServerProfileAs` carries the principal; the
instance's model rows carry `value`, `viaAll`, `effective` and `all` as `assign::rows` gives them, 404
for an unknown instance; the assignment PUT's 200 has the after-read's `effective`, `running` only when
`effective` changed on a running instance, `*` in the path is 404 and never reaches the manager, a bad
value 422 before any call; the `all` PUT calls `SetModelProfileAs(id, "*", value)`; pool rows carry
`assignments` sorted, an orphan section left out; the save and the switch answer `restart` with the
running instances the profile is (or becomes) effective on, `[]` plus `running: true` when a `Status`
fails; `GET /api/instances` carries `serverProfile`. Clippy and fmt green per crate; mutants on the
security code above.

*As built (slice 15 reviewed, 2026-11-21, uncommitted).* Met: `assign::rows` beside `render` with the
equality test; the four routes in `crates/bff/src/instance_profiles.rs` (gate first, writes and file
reads `As`, `Instances()`/`Status` on the bff's own connect, `*` in the path 404 before any call, bad
values 422 first); the pool rows' `assignments` filtered by `Instances()` (`model_profiles.rs` 234);
`restart` from `served` before/after, stale sections dropped by the manager's `ListInstances`
(`restarted`, 640); `serverProfile` in `GET /api/instances` (`lib.rs` 541). Accepted deviations: a
`NotAuthorized` on the bff's OWN call (`Instances()`, `Status`) is 502 `client-not-authorized` with the
session kept (`own_refused`, 671; the precedent of `control.rs` 320): only a refusal of an `As` call
ends a session; the server switch with `Instances()` unreadable answers `running: true` without asking
`Status` (safe); an after-read that fails answers `effective` from the text the write leaves
(`written`), not the before-read. Ruled out: `star_inherits` (537), a restart offered for an `on`/`off`
under `* = inherit` that leaves `effective` unchanged; the render emits the same sections, so the
router's file is the same; `running` stays "the effective set changed and the instance runs". Ruled
out: the save's `None if !changed => list_instances()` (`model_profiles.rs` 1062): no record means
no instance is given the profile, so `restart: []` and no state asked.

*Slice 16's decisions (2026-11-21, architect; each a preference unless sourced).*

- **One round**, the size of slice 14: the two manager methods with their `As` forms on D-Bus, the pipe
  and `ManagerClient` (`create_instance`, `remove_instance`), the start refusal, and the bff's two routes.
  Inside the round the tester orders: the start refusal first (a created instance has an empty render),
  then create, remove, the bff. If the red set is too big for one coder run, cut at the client: 16a the
  manager and its transports, 16b the bff. Protocol stays 3 (no release carries it; last tag 2.4.0).
- **Refusals are keyed `Invalid` refusals, not new variants** (changed from (d) and (d')'s
  `InstanceExists`, `InstanceProtected`, `InstanceRunning`, `NoModelProfiles`): `Refusal` already crosses
  both transports and `ClientError` decodes it, so no `ManagerError`/`ClientError`/D-Bus/pipe arm is
  added four times. New keys in `checks.rs`'s list, `Refusal::text` and both strings files, one value
  each, the id: `instance-bad-id`, `instance-exists`, `instance-protected`, `instance-running`,
  `instance-no-model-profiles`.
- **`Start`, `Restart`**: after `existing_id`, before any service call (so a restart never stops a
  running instance it cannot start again), `instance-no-model-profiles` [id] when
  `launch::has_model_profiles_in(paths::instance_models(id))` is false; a missing render is refused
  (fails closed, the launch would refuse it anyway, `launch.rs` 247).
- **`CreateInstance(engine, id, serverProfile)`** `(sss)`, under `configure` and `writing`, one action
  record (target the id). Checks in order, each writing nothing: `unknown-engine` (`known_engine`);
  `instance-bad-id` [id] when `valid_instance_id` refuses (a refusal, not `InvalidId`: it is the create's
  input, 422 with a field); the record read with `read_record` (`Io` on a failure, never "absent");
  `instance-exists` [id] when a section has the id; `NoServerProfile` (`paths::instance_ini` none or no
  file); `ServerProfileInUse` (`record_holder`); `UnresolvableHost` and `AddressInUse` from
  `check_binding(&binding(profile), |_| false)`, which also judges `[Bff] Listen` (slice 8's walk: the
  new instance has no current profile to skip). No `ModelsMax` check: the render loads nothing. Writes,
  record last: (1) a stale `[id]` section of `assignments.ini`, only from a hand edit, deleted
  (`ini::text::delete_section`; no write when absent), so the new instance is assigned nothing; (2) the
  render `models@<id>.ini` from `render_of` (empty after (1)); (3) the section `[id]` with
  `ServerProfile = <profile>` appended to `instances.ini` (a newline first when the text does not end
  with one). The record write is the instance's birth: a failure before it leaves no instance, only a
  render no launch reads (rewritten by the next create of that id). **No service call** on either OS:
  systemd's template `llama.cpp@.service` serves any id and a new id has no `wants/` link, so boot is
  off; Windows creates `llama.cpp_<id>` at its first `Start` or `SetBootEnabled` with a demand start
  (`scm.rs` `start`, `windows.rs` `service_spec`), and `sync` at the manager's start. A link left by hand
  for a reused id is the administrator's.
- **`RemoveInstance(id)`** `(s)`, same guard and record. Checks: `InvalidId`; record read (`Io`);
  `NoInstance`; `instance-protected` for `default`; `Status` must be `inactive` or `failed`, anything
  else (and a `Status` failure, `Service`) `instance-running` or the error: never remove what cannot be
  seen stopped. Then, each step idempotent (absent is success), record last: (1) new
  `ServiceControl::remove(id)`; (2) the `assignments.ini` section; (3) `remove_file` of the render, its
  path from `paths::instance_models` (the id checked first: no path is built from an unchecked id);
  (4) the `instances.ini` section (`delete_section`). A failure halfway leaves a listed, stopped
  instance that a second remove finishes. Stay: the server profile, the model profiles, `state/<id>`
  (a later instance of that id finds its snapshots), the Windows log, the journal. Accepted race: a
  `Start` (which does not take `writing`) between the status check and (4) leaves a running unit with
  no record; `Stop` needs only a valid id, so it can still be stopped, and its launch, reading the
  record, fails `NoConfig` if it starts after (4).
- **`ServiceControl::remove`**. Linux (`SystemdService`, the manager runs as root, no `User=` in
  `private-ai-manager.service`, so systemd asks polkit nothing, as for today's `EnableUnitFiles`):
  `DisableUnitFiles([llama.cpp@<id>.service], false)`, `Reload`, then `ResetFailedUnit`, its error
  ignored (a unit not loaded has nothing to reset), so a reused id does not read `failed`
  ([org.freedesktop.systemd1(5)](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.systemd1.html)).
  Windows (`ScmService`): `gate.cancel(id)` first (a restart's delayed start would otherwise
  re-create the service through `start`), then `DeleteService`; `ERROR_SERVICE_DOES_NOT_EXIST` (1060,
  never started) and `ERROR_SERVICE_MARKED_FOR_DELETE` (1072) are success; a create of the same id
  while the service is marked fails at its `Start` with 1072 ((d)). Type-checked only.
- **The bff**, both routes behind the session gate (401 `session-required`, manager never asked),
  `sessions.current`, the call `As` the signed-in user under `write_timeout`, `NotAuthorized` 403
  `session-not-administrator` ending the session, no manager or past the deadline 503
  `client-unavailable`, else 502 `client-other` (slice 15's rules):
  - `POST /api/instances` `{"engine", "id", "serverProfile"}` -> 201 `{id}`. 400 `instance-malformed`;
    422 `unknown-engine` `field` `engine`, `instance-bad-id` `field` `id`; 409 `instance-exists` [id];
    404 `client-no-server-profile` [profile] (as slice 15's switch); 409 `client-server-profile-in-use`
    [profile, instance]; 422 `client-address-in-use` [instance], `client-unresolvable-host` [host], `field`
    `serverProfile`.
  - `DELETE /api/instances/{id}` -> 204. 404 `client-no-instance` (`NoInstance`, `InvalidId`); 409
    `instance-protected` [id], 409 `instance-running` [id]; 502 `client-service-failed` (`Service`, the
    message kept out of the body, as `control.rs`).
  - `POST .../start` and `.../restart`: 409 `instance-no-model-profiles` [id].
  - `GET /api/instances` items gain `removable`: `id != "default"` (the one protected id; whether it
    runs is `state`, and the DELETE refuses a running one). Additive.
- **Security code (mutants):** both methods' `configure` gate and principal (`As` carries it; a mutant
  dropping `authorize_as` or `acting_for` fails a test), `valid_instance_id` before any path (a `../x`
  id deletes and writes nothing), `default` protected, a running or unreadable state refused, record
  last in both orders; the bff's session gate and `As`. Not security code: the body parse, `removable`.
  Trust: unchanged from (d): a compromised bff can, during a member's session, create instances on free
  addresses and remove stopped ones other than `default`.
- **The web UI needs a slice** (slice 17, the designer's brief: a create form and a remove action);
  this round is backend only, every reply change additive. A `pai instance create|remove` waits until
  asked (default taken, user away (2026-11-21)).

**Slice 16 done when** (test first; Linux, the Windows code type-checked): `pai-manager`, on D-Bus and
the pipe, with fakes: `Start` and `Restart` of an instance whose render holds only `[*]`, or none, are
`instance-no-model-profiles` with no service call (no stop for `Restart`); `CreateInstance` writes the
section naming the profile, an empty render and no service call, its new instance listed by
`Instances()` and `BootEnabled` false, and each refusal above writes no file (`unknown-engine`, `instance-bad-id`
for `../x` and `A`, `instance-exists`, `NoServerProfile`, `ServerProfileInUse`, `AddressInUse` against an
instance and against `[Bff] Listen`, a non-`configure` caller); a stale `assignments.ini` section of the
id is gone after a create; `RemoveInstance` of a stopped instance calls `remove` once, then removes the
`assignments.ini` section, the render and the record section, leaving the profile and other instances'
bytes; `default`, an active, activating or deactivating one, a `Status` failure, an unknown or invalid
id each write nothing and call no `remove`; a `remove` failure leaves the record section and a second
call finishes; the systemd stand-in records `DisableUnitFiles`, `Reload`, `ResetFailedUnit` (its error
ignored). `pai-bff`, from the recording fake: both routes 401 with no call, 403 ending the session,
each refusal's status, key, `args` and `field`, `CreateInstanceAs`/`RemoveInstanceAs` carrying the
principal; start and restart 409 `instance-no-model-profiles`; `removable` false for `default` only.
Clippy, fmt and the Windows check green per crate; mutants on the security code above.

**As built** (reviewed 2026-11-21 against the uncommitted change; Linux tested, Windows `ScmService::remove`
type-checked only): every decision above is met. `Manager::start`, `restart` (`has_profiles` after
`existing_id`, before the service), `create_instance`, `remove_instance` and `append_record` (the
file's own line ending first when the record ends without one) in `crates/manager/src/manager/service.rs`;
`SystemdService::remove` in `manager/dbus.rs`, `ScmService::remove` in `manager/windows/scm.rs`; the
`As` forms on D-Bus and the pipe; the two routes in `crates/bff/src/instances.rs`, the 409 of a start or
restart in `crates/bff/src/control.rs`, `removable` in `crates/bff/src/lib.rs`.

**Slices** (replacing the old 4 to 14), each test first; Linux, the Windows code type-checked.

4. **The rename** (13a), no behaviour change, across `pai-core`, `pai-manager`, `pai-app`, `pai-cli`,
   `pai-config`, `pai-bff` and `xtask`: files, Rust names, strings keys, error keys, CLI, the bff's
   routes as in (f') (the server document moves to `/api/profiles/server/{id}`, `default` today), the
   shipped `models.ini`. The web UI renames `fe/` at the same pause, so the bff and the fe land in
   one window. Done when: `git grep -i preset crates xtask` finds only llama.cpp's words (`--models-preset`,
   `docs/preset.md`), bench data and test fixtures quoting them; each crate's tests, clippy and fmt are
   green; the package-content tests name `models.ini`; an old route answers 404.
5. **`instances.ini`**: `paths::instance_ids` from it, the launch through `ServerProfile`, `ConfigFile::ServerProfile`,
   read-only `instances.ini`, the shipped file, Windows `sync`. Done when: an instance whose section names
   `big` launches with `big`'s port; a `server@x.ini` no section names lists no instance; a client
   write of `instances.ini` is `BadFileName`; xtask's package tests name it.
6. **Checks on profiles in use** ((b') 1 to 3). `check_address`, `check_renders` and `check_contents`
   stop reading `server@<instance id>.ini`: each goes instance, then the profile its section names; an
   instance whose profile cannot be read is skipped, never judged on the defaults of a missing file.
   Until this slice and 13, `default`'s profile must be named `default`. Done when: two unused profiles
   with one port save; a profile in use colliding with another instance's is refused naming it; a
   section whose profile file is missing refuses no save; `ModelsMax` lowered on a profile in use is
   refused naming the instance, on an unused one accepted.
7. **`SetModelProfile`** (old 4; protocol 3, D-Bus and pipe). Done when: each value re-renders, an
   unknown model profile or instance is refused, a non-`configure` caller writes nothing.
8. **`SetServerProfile`**. The target profile is unused at that moment, so `check_address` as slice 6
   left it judges nothing: split the walk from its in-use test and judge the target's address against
   every other instance's profile and `[Bff] Listen`, leaving out the switching instance's current
   profile (slice 16's `CreateInstance` calls the same walk). Done when: each refusal of (d') writes
   nothing; an unused profile on another instance's port or on the web service's port is refused at the
   switch; a switch to a profile on the instance's own current port succeeds; a success rewrites the
   section and the next launch uses the new profile.
9. **The model-file check** (old 5). Each file of `model` (every shard) must be a regular file. A
   relative `model` resolves against `launch::instance_current_dir`, the cwd the router and its children
   inherit. A profile with `hf-repo`, `model-url` or `docker-repo` is not checked: llama.cpp downloads
   to `model` or replaces it with the cache path (`common/arg.cpp` 427-441, 480-535 in v1.0.0). The
   refusal names the first instance in id order; the check needs no server profile, so it also runs
   for an instance whose profile is missing. Only a file the write adds refuses ((b), "Which writes
   run them"). 10. **The estimate to `pai-core`, then the memory check** (old 6), as (b) 2: 10a, the
   header reader and the device parser to `pai-core`, done (1694064). 10b-1, `pai-core` only, pure:
   `gpu_pool`, `Gpu` and `is_integrated` move to `pai_core::devices` (`pai-app` re-exports);
   `Shape` gains the trained context uncapped and the f16 KV bytes per token; a function of the full-GPU
   bytes of a shape for a context and two cache types, and one of the usable bytes of a pool. Done
   when: the GUI's tests are unchanged and green; the figures for f16/q8_0/q4_0, a context of 0, an
   unreadable layout and a card seen by CUDA and Vulkan are pinned. 10b-2, the manager check with a test
   seam for the GPU totals, and `model-profile-over-memory` in `checks.rs`'s keys and both strings
   files. Done when: an over profile switched on, assigned, or saved larger (its `ctx-size`, a cache
   type, `[*]`'s `ctx-size`) is refused naming the first instance, nothing written; a save of another
   key of an already-over profile, a switch off, a fetch-key profile, a non-GGUF `model`, no GPUs land.
   10b-3, the real probe (timeout, cache by exe path and mtime, failure not cached). Done when: a fake
   exe printing a device block gives the totals once for two checks, a changed mtime probes again, a
   hanging one is killed and skipped. 10b-2 and 10b-3 done as one slice (this commit); changed from
   the design: growth by section id with no byte-compare shortcut, `[*]`'s `ctx-size` saved larger
   dropped from "Done when" (no client writes `[*]`; the cascade pinned on a hand-edited one), the
   probe's 2 s reader grace.
11. **Snapshots per instance** (old 7). 12. **Tags** on the server profile (`[Server] Tags`) and on the instance (`Tags` in its `instances.ini`
    section), `SetInstanceTags` and `PUT /api/instances/{id}/tags`. Done when: each is saved and refused
    by the rule of (c); `Instances()` returns both lists; the shown `tags` are the union, profile first,
    deduplicated.
13. **`Instances()`**, `GET /api/instances` and `GET /api/node` (old 9); the GUI, the CLI and the bff's
    server document stop assuming `default`'s profile is named `default` and read it through
    `Instances()` ((g')), before slice 15 gives clients a way to switch it; with it the About versions
    in `GET /api/node`, `boot` in the list (decisions and "done when" after (g')).
14. **`CreateServerProfile`, `DeleteServerProfile`** and the bff's `GET`/`POST`/`DELETE /api/profiles/server`. Decisions and "done when" under "Slice 14's decisions" after (g'); backend only, the old route kept until 17.
15. **The bff's association routes**: `PUT .../profiles/server`, the model-profile routes under an
    instance, the pool rows' `assignments`, `restart`, resource refusals (old 10). The manager's
    `SetServerProfile` answers nothing about a restart: the route reads `Status` after the switch and
    answers `{running}`, true when the instance must be restarted to apply it. Decisions and "done
    when" under "Slice 15's decisions" before this list; backend only, the web UI in 17.
16. **`CreateInstance`, `RemoveInstance`**, the start refusal `instance-no-model-profiles`, and the bff's
    `POST` and `DELETE /api/instances` (old 11, 12): one round (cut at the client if too big). Decisions
    and "done when" under "Slice 16's decisions" before this list; backend only, the web UI in 17.
17. **The web UI** (the web UI session, designer's brief first), then **the estimate as a read** (old 13, 14).

Then the docs agent: AGENTS.md (the overview, "Key conventions" file names), "Through the manager",
`crates/bff/README.md`, `installer/AGENTS.md`, `docs/build-packaging.md`, and Part 1 of this page.

**Decided by the user (2026-11-17)** on the earlier version's two questions: the memory check refuses
anything over the full-GPU estimate, even with `fit` on or a CPU placement ((b) 2); every instance sees
all the node's GPUs and is checked alone against them (a per-instance `Devices` key waits until asked; asked and decided 2026-11-21: the server profile's
`Devices`, "Resources belong to the server profile").
On the revision's three questions (user, 2026-11-17): server profiles keep the file name
`server@<profile>.ini`; the CLI is `pai model-profile <verb>`; a server profile created from the web
starts from the defaults.

### Monitoring and log consultation (step 10 item 13d): accepted (user, 2026-11-21), approach (a)

The requirement (user, 2026-11-21, `PLAN.md` step 10 item 13d): the native GUI's HW Monitor streamed
to the web UI, a history chart of the same metrics, and a module that collects the metrics and routes
an administrator's log reads to the host that holds the log; logs stay on each host, read on demand,
no central collector yet. External tools welcome where they fit (OpenTelemetry named).

**What exists, and what the design keeps, moves or removes.**

- The sampler is `pai-app`'s `hwmon` (`crates/app/src/hwmon.rs`, `hwmon/linux.rs`, `hwmon/windows.rs`,
  `hwmon/sdk.rs`): one `Sampler::sample()` a second, needing no root on either system (the module
  header's tables): amdgpu sysfs (`gpu_busy_percent`, `mem_info_vram_*`, `mem_info_gtt_*`,
  `mem_busy_percent`), NVML loaded at runtime (busy, VRAM, memory activity, PCIe RX/TX, the
  throughput call measuring over 20 ms), `/proc/stat`, `/proc/meminfo`, `/sys/block/<disk>/stat`; on
  Windows PDH (`GPU Engine`, `GPU Adapter Memory`), D3DKMT, `GetSystemTimes`, `GetPerformanceInfo`,
  `IOCTL_DISK_PERFORMANCE`. It returns `Group`/`Bar`, already formatted text for the GUI, and the
  disk card follows one models directory. **Moved**: the readings (numbers with units, per device)
  go to a crate the manager can link; the GUI's formatting and peaks stay in `pai-app`. The manager
  cannot link `pai-app` today (it pulls `rusqlite`, `image`, a build script reading the llama.cpp
  clone: `crates/app/Cargo.toml`, `build.rs`).
- The manager already answers a hardware read: `Devices` (`service.rs` `devices`, gated by
  `authorize_quietly(.., Action::Control)`, the `As` form for the bff). Metrics follow that pattern.
- The bff (`crates/bff`) is tokio and axum, unprivileged, writes nothing, and its unit has
  `PrivateDevices=yes` (`xtask/src/linux/system_files.rs` `bff_unit`), which hides `/dev/nvidia*`
  from NVML. And in step 14 a node needs only `-core` and the engine, no bff (Part 2, "Who listens").
  So the bff cannot be the sampler; the node's collector must be in `-core`.
- Logs: the instance's journal on Linux (`llama.cpp@<id>.service`; readable by root and the groups
  `systemd-journal`, `adm`, `wheel`, [journalctl(1)](https://www.freedesktop.org/software/systemd/man/latest/journalctl.html));
  on Windows `%ProgramData%\llama.cpp\logs\llama-server@<id>.log` and `.previous.log`, which users
  already read (`AGENTS.md`, "Key conventions"). The bff's account is in none of those groups.
  **Kept**: logs stay where they are; nothing copies them.
- **Removed**: nothing.

**What GPU monitoring needs, and who gives it** (the GUI's bars are the bar to meet):

| Source | amdgpu busy | VRAM | GTT / shared | memory activity | NVIDIA | Windows |
| --- | --- | --- | --- | --- | --- | --- |
| Our `hwmon` | yes | yes | yes | yes | NVML: busy, VRAM, memory activity, PCIe | PDH + D3DKMT, every adapter |
| OTel Collector `hostmetrics` | no GPU scraper ([README](https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/main/receiver/hostmetricsreceiver/README.md)) | | | | | |
| OTel `windowsperfcounters` | | | | | | any PDH counter, so `GPU Engine` and `GPU Adapter Memory` by configuration; contrib only, beta ([README](https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/main/receiver/windowsperfcountersreceiver/README.md)) |
| node_exporter `drm` (off by default, amdgpu only) | yes | yes | yes | no | no (points to DCGM) | ([README](https://github.com/prometheus/node_exporter), [drm_linux.go](https://github.com/prometheus/node_exporter/blob/master/collector/drm_linux.go)) |
| windows_exporter `gpu` (off by default) | | | | | | engine time, dedicated, shared ([collector.gpu.md](https://github.com/prometheus-community/windows_exporter/blob/master/docs/collector.gpu.md)) |
| DCGM exporter | | | | | Linux only; GeForce "limited functionality" ([DCGM getting started](https://docs.nvidia.com/datacenter/dcgm/latest/user-guide/getting-started.html)) | no |
| AMD device-metrics-exporter | Instinct MI2xx/MI3xx, MI350P, "Radeon AI platform"; Ubuntu 22.04/24.04, in a container ([docs](https://instinct.docs.amd.com/projects/device-metrics-exporter/en/latest/)) | | | | | no |
| Telegraf `amd_rocm_smi` | yes | yes | not listed | yes | `nvidia_smi` input | needs the `rocm-smi` binary ([README](https://github.com/influxdata/telegraf/blob/master/plugins/inputs/amd_rocm_smi/README.md)) |
| Netdata `proc.plugin` amdgpu | yes | yes | yes | yes | `nvidia_smi` module | ([AMD GPU](https://learn.netdata.cloud/docs/data-collection/hardware-devices-and-sensors/amd-gpu)) |

No external tool covers the GUI's set on both systems alone: every external option needs at least
two agents per node (a host one and a GPU one, on Linux a different GPU one per vendor), and on Linux
NVIDIA's own exporter is the data-centre one.

#### Options

Licences and versions below are each project's GitHub record (checked 2026-11-21 with `gh api
repos/<repo>` and its latest release); sizes are the release archives' downloads.

**(a) The manager's own sampler, a small history store, a stream through the bff.**

- On a node: nothing new runs. A sampler thread in `pai-manager` (root, LocalSystem) calls the moved
  readings once a second into a ring in memory and folds them into the history tiers (below). The
  disk card follows the engine's default models folder only (user, 2026-11-21).
- Interface: three manager methods with their `As` forms, request and reply as every other one, so
  D-Bus, the pipe and step 14's mTLS link carry them unchanged: `Metrics(after: seq)` (the samples
  after a sequence number, from the ring; a gap says so), `MetricsHistory(from, to)` (the tier that
  fits the range, at most a bounded number of points), `ReadLog(instance, cursor, lines)` (below).
  The bff polls `MetricsAs` once a second with ONE task for all viewers and fans it out as
  server-sent events (`GET /api/node/metrics/stream`, axum's `sse`,
  [docs.rs](https://docs.rs/axum/latest/axum/response/sse/index.html)); the browser's `EventSource`
  reconnects by itself and sends `Last-Event-ID` ([HTML standard, server-sent events](https://html.spec.whatwg.org/multipage/server-sent-events.html)),
  which maps to `after`. SSE over WebSocket because the flow is one way, it is plain HTTP through the
  existing `Host` guard and session cookie, and it needs no new crate.
- Footprint: no process, account, port or package. The store: SQLite through `rusqlite` with
  `bundled`, already in the workspace (`crates/app/Cargo.toml`); SQLite is public domain
  ([sqlite.org/copyright](https://www.sqlite.org/copyright.html)). Fixed size (RRD-style tiers,
  below): about 1 MB whatever the uptime. The full estimate is "Resources of (a)" below.
- Linux and Windows: the sources exist and are tested in `pai-app`; on Windows they have run under a
  desktop user, never as LocalSystem in session 0 (a slice's check on the user's Windows build).
- Trust: the browser to the bff (session, a `pai` member); the bff to the manager (`MetricsAs`,
  `ReadLogAs`, the principal re-checked per call as every `As` verb); the manager reads sysfs, `/proc`,
  NVML, PDH and the journal itself. Cost: root now parses sysfs text and loads the NVIDIA library
  (`dlopen`) once a second; a panic in the sampler thread must not take the manager down (caught,
  the sampler restarted, readings missing until then). If that is judged too much, the sampler can
  later run as a child under the instance account (`private-ai`, groups `render video`) streaming
  lines to the manager; not proposed now (preference: simplest first, the sources need no root).
- Step 14: the director calls the same `MetricsAs`, `MetricsHistoryAs`, `ReadLogAs` on each node
  over the mTLS link (Part 2), the hub's bff under `/api/nodes/{node}/...`. History and logs stay on
  the node and are read on demand: the "log router" is the manager protocol itself. A central store,
  when wanted, is a later export (below), not a redesign.
- Test effort: low to moderate; the sources have tests; new are the ring, the fold, the store's
  pruning, three methods, the SSE route and its session ending.

**(b) OpenTelemetry.** Two separate pieces.

- (b1) The Rust SDK in the manager: metrics API, SDK and OTLP exporter **stable**, logs **stable** as a
  bridge (appenders for `log` and `tracing`, no new logging API), traces beta; latest
  `opentelemetry-0.33.1`, Apache-2.0 ([opentelemetry-rust README](https://github.com/open-telemetry/opentelemetry-rust)).
  It only emits: it gives the web UI neither a live stream nor history; something must receive and
  store. Worth it later as the export of (a).
- (b2) The Collector, a Go binary per node: `hostmetrics` (metrics **beta**, core and contrib, cpu,
  disk, filesystem, load, memory, network, paging on all systems; no GPU), `journald` (logs **alpha**,
  Linux only, runs `journalctl`, its user in `systemd-journal`;
  [README](https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/main/receiver/journaldreceiver/README.md)),
  `filelog` (logs beta, Windows supported with exceptions;
  [README](https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/main/receiver/filelogreceiver/README.md)),
  `windowsperfcounters` (contrib only). The distribution that has `journald` and
  `windowsperfcounters` is contrib: v0.165.0, 109 MiB `linux_amd64.tar.gz`, 97 MiB Windows MSI
  (core `otelcol` 45 MiB; [releases](https://github.com/open-telemetry/opentelemetry-collector-releases/releases)),
  Apache-2.0, with the licences of its Go dependencies to list for our packages (the rule in
  `installer/AGENTS.md`). On Linux still no amdgpu or NVML receiver in the tables above: the GPU
  half stays ours or another exporter. Plus a store: Prometheus 3.16.0 (108 MiB tarball, Apache-2.0,
  15 days by default, about 1 to 2 bytes a sample, [storage](https://prometheus.io/docs/prometheus/latest/storage/))
  or VictoriaMetrics 1.156.0 (Apache-2.0).
- Trust: a new service account per node, the collector's listeners and the store's HTTP API (bound to
  the loopback, or one more authenticated channel between machines beside step 14's mTLS); the log
  pipeline is push: it ships the journal somewhere continuously, the opposite of "read on demand", and
  needs a user in `systemd-journal` permanently. The on-demand read and the `pai` check would still
  be ours.
- Step 14: the natural central shape (every node exports OTLP to a central collector), at the cost of
  a second trust fabric to keep consistent with the manager's.

**(c) Prometheus-style exporters and a TSDB.** node_exporter 1.13.1 (Apache-2.0, 12 MiB, `drm` for
amdgpu busy, VRAM, GTT, off by default), windows_exporter 0.32.1 (MIT, 13 MiB MSI, `gpu` off by
default, Windows 10/11 21H2+ and Server 2016+, [README](https://github.com/prometheus-community/windows_exporter)),
for NVIDIA on Linux dcgm-exporter 4.9.0 (Apache-2.0, needs DCGM, Linux only), for AMD's own exporter
the Instinct-class list above; Prometheus or VictoriaMetrics to store and query. No logs (another
stack). Three to four HTTP listeners per node, each a separate package to install, update and
firewall; memory activity and NVIDIA PCIe throughput missing on Linux consumer cards; history by PromQL
from the bff. Step 14: Prometheus scrapes nodes over its own TLS, again a second trust fabric.

**(d) Other light agents.** Telegraf 1.40.3 (MIT, one Go agent with GPU inputs that shell out to
`rocm-smi` / `nvidia-smi`, Windows counters, its own outputs), Vector 0.62.0 (MPL-2.0, 55 MiB, host
metrics and journald sources, a pipeline not a store), Netdata 2.13.0 (GPL-3.0; amdgpu with GTT and
memory activity, NVIDIA; "under 5% of one CPU core and about 200 MB of RAM" by default,
[resource utilization](https://learn.netdata.cloud/docs/netdata-agent/resource-utilization)) with its
own dashboard and own access model beside our sign-in; a GPLv3 program can be bundled into the
framework's AGPL-3.0-or-later packages (AGPLv3 and GPLv3 section 13 let the two be combined,
https://www.gnu.org/licenses/agpl-3.0.html, https://www.gnu.org/licenses/gpl-3.0.html), its source
offered as GPLv3 requires; llama.cpp's MIT packages would carry the same source obligation. Each is a capable fit for an operator
who already runs it, none removes the need for (a)'s on-demand log read through the `pai` check.

#### Recommendation

**(a)**, with the readings' names and units chosen so a later export (b1's OTLP, or a Prometheus
text endpoint) is a mapping, not a redesign. Why: it is the only option that adds no process,
account, port or bundled Go binary per node, covers every bar the GUI has on both systems today,
keeps the one trust path (session, `As` method, the manager's `pai` check) for metrics and logs alike,
and rides step 14's mTLS link with no second fabric. The external stacks earn their cost when there
is a central store to feed; that requirement is explicitly not now.

**Who may do what.** Metrics: `control`, asked quietly, as `Devices` (hardware load is no secret, and
the GUI shows it to any desktop user). Logs: an administrator (a `pai` member) on the web, the
manager reading them as root; this widens Linux journal access from root and `systemd-journal`/`adm`/
`wheel` to `pai` for the instance units only: every `pai` member reads instance logs (user,
2026-11-21). Logs may hold request details; whether llama-server writes prompt text at its default
verbosity is unverified, which is part of why the read is admin-only.

**Fails closed.** `ReadLog` refuses an instance not in `instances.ini` before anything runs; on Linux
the manager runs `journalctl` with an argument vector it builds (no shell): `-u llama.cpp@<id>.service
-o json --no-pager -n <lines>` and `--after-cursor=<c>` when given (options as journalctl(1) above),
one at a time, with a timeout, lines and bytes capped under the 1 MiB frame; on Windows a byte-offset
cursor into the two files, read only, capped likewise. A sampler source that fails drops its readings,
never the sample; a store write that fails leaves a gap in history and the live stream runs on. The
SSE stream reads the session as a gate without moving its idle deadline (as the polled reads,
`crates/bff/README.md`) and closes when the session ends (sign-out, idle, the manager's refusal),
checked at each event. Bounds: one poller in the bff whatever the number of viewers, a cap on
concurrent streams per session, history replies capped in points.

**History: RRD-style tiers** (user, 2026-11-21: the resolution follows the range, about the same
number of points per chart, configurable). The live ring keeps 1 s samples for 15 minutes in memory
(the live view and an SSE catch-up, lost at a manager restart). History is a fixed set of tiers, each
a step and a retention, so a fixed number of buckets used as a ring (slot = time / step modulo the
count); each bucket holds, per series, min, average, max and the count of samples (0 is a gap, drawn
as one). The first tier folds the 1 s samples; each next tier folds whole buckets of the one before
(min of mins, max of maxes, the average weighted by count), which is exact only when each step is a
whole multiple of the previous one. Defaults, 720 points at each tier's full range:

| Tier | Step | Retention | Buckets |
| --- | --- | --- | --- |
| 1 | 5 s | 1 h | 720 |
| 2 | 20 s | 4 h | 720 |
| 3 | 2 min | 24 h | 720 |
| 4 | 15 min | 7 days | 672 |
| 5 | 1 h | 30 days | 720 |

(15 minutes, not the 14 that 720 points would give, so the 1 h step stays a whole multiple.) A query
`MetricsHistory(from, to)` takes the finest tier whose retention reaches back to `from`, and returns
its buckets in the range: 720 points when the range is a tier's retention, fewer for a range between
two (a 2 h range gets tier 2, 360 points); the bff never asks for more than the largest tier holds.

Configuration: `[Metrics] Tiers = 5s:1h, 20s:4h, 2m:24h, 15m:7d, 1h:30d` in a new manager file,
`/etc/private-ai/manager.ini` and `%ProgramData%\private-ai\config\manager.ini`, beside `bff.ini`,
written by the administrator by hand (no web edit in this step), read at the manager's start; no file
or no key means the defaults. Validation, in `pai-core` so a later editor shares it: 1 to 8 tiers;
steps whole seconds, ascending, each a whole multiple of the previous one; retentions ascending,
each a whole multiple of its step; 2 to 10 000 buckets per tier. A value that fails is refused with
its key in the manager's log, and the manager runs with the defaults: monitoring settings are no
security setting, and a typo must not stop instance management. A tier whose step or count changed
starts empty (its stored rows dropped, logged); unchanged tiers keep their data.

The store: SQLite, one table per tier keyed by slot, each row the bucket's start time and its values
for every series, overwritten in place, so the file does not grow; series are added by name as
devices appear. Writes are batched: one transaction a minute with the buckets that closed (at most
the last minute of data is lost in a crash), and one at a clean stop. Paths:
`/var/lib/private-ai-manager/metrics.db` (a new `StateDirectory=private-ai-manager`, mode 0700, not
`/var/lib/private-ai`, which is the instance user's home) and `%ProgramData%\private-ai\metrics\`
(inherits the root's protected DACL: the playbook's note on `MachineDataRoot`). An alternative to
SQLite is a fixed-size file of our own written at offsets: no dependency, but torn writes, checksums
and the schema change on a tier change become ours; SQLite is recommended (preference) for its
atomic commit.

**Resources of (a): estimates, none measured yet.** For the user's node (two amdgpu cards, no NVIDIA):
about 25 series (per GPU busy, VRAM, GTT, memory activity; CPU, I/O wait; RAM, cache, swap; disk read,
write, busy, wait).

| What | Estimate | Basis |
| --- | --- | --- |
| CPU per 1 s sample, Linux, amdgpu | well under 1 ms, so under 0.1% of one core | about 10 small sysfs files per GPU plus `/proc/stat`, `/proc/meminfo`, one `/sys/block/<disk>/stat`, each an open, read, close of a few hundred bytes |
| NVML, per NVIDIA GPU | over 40 ms of wall time per sample in the PCIe throughput calls (RX and TX, each measuring over 20 ms: the `hwmon.rs` header), the CPU share unknown; the other NVML calls cheap | the thread waits, so the sample's period must be kept by the clock, not by sleeping 1 s after each sample |
| PDH on Windows | a few ms per collect, unknown | one query for every adapter (`hwmon.rs` header) |
| Memory | live ring 900 samples x 25 values x 8 B = about 180 KB; tiers cached in memory 5 x about 720 x 25 x 4 values x 4 B = about 1.4 MB; under 2 MB in all | arithmetic |
| SQLite file | about 1.5 MB, fixed by the tiers (page overhead included), whatever the uptime | the arithmetic above |
| Disk writes | one commit a minute touching a few pages plus the WAL: of the order of 1 to 3 MiB an hour | estimate; writing every 5 s instead would be several times more, which is why commits are batched |
| Binary | `rusqlite` bundled adds SQLite's C library to `pai-manager` | not measured |

How to measure (a proposal): an example binary in the new crate, `cargo run -p pai-hwmon --example
sample-cost -- --samples 300`, that samples the real machine (reading only, no GPU load needed) and
prints per-sample wall time and CPU time (mean and 99th percentile, CPU from `getrusage` /
`GetProcessTimes`), split by source (sysfs and `/proc`, NVML, PDH), and its resident memory. It reads
real `/sys`, so it is not a test (tests never touch real systems: `AGENTS.md`); the user runs it, or an
agent with the user's go-ahead. The store's writes are measured in slice 3's test on a temp dir: a
simulated day of samples, then the file and WAL sizes and the bytes the process wrote
(`/proc/self/io` `write_bytes`) reported. **Slice 1 must report** the CPU time per sample (mean and
p99) on the user's node and on one NVIDIA machine if there is one, and the resident memory; slice 3
the file size and the bytes written per simulated hour. A number above the estimate by more than ten
times reopens the design.

**A dedicated monitoring crate** (the user asked whether to make one). Yes: `pai-hwmon`
(`crates/hwmon`), a library with the readings (numbers with units, per device, the Linux and Windows
sources, NVML), the live ring and the tier fold; no text, no formatting, no SQLite. The manager links
it with the store; `pai-config`'s HW window links it through `pai-app`, which keeps the formatting,
the peaks of the rate bars and the device checkboxes (`settings.ini` `HwMonitorHidden`). Why a crate
and not a `pai-core` module: `pai-core` is linked by `pai-bff` and `pai`, which would then compile the
samplers and their `windows-sys` features for nothing; why not leave it in `pai-app`: the manager
cannot link `pai-app`. What has to move with it: the readings use only a device's id (PCI address or
UUID); the GPU facts (`devices::PhysicalGpu`, `publish_gpus`, `sdk.rs`'s `--device` ids) stay in
`pai-app` unless slice 1 finds a reading that needs them. No reason against found beyond one more
crate in the workspace.

#### Slices

Test first, Linux first, the Windows halves type-checked; each a useful piece.

1. **Readings move.** `pai-hwmon` returns typed readings per device; `pai-app` formats them for the
   GUI as today. Done when: the GUI's hwmon tests pass unchanged, the readings come from a fake sysfs
   root (`/proc`, `/sys` rooted in a temp dir) with values and units pinned, and the `sample-cost`
   example's numbers above are reported.
2. **Live stream.** The manager's sampler and ring, `Metrics`/`MetricsAs`; the bff's poller and
   `GET /api/node/metrics/stream`; then the designer's brief and the live view. Done when: a fake
   source's samples arrive in order with their sequence; a reconnect with `Last-Event-ID` resumes
   without a duplicate and a gap is flagged; no session is 401 with the manager not asked; a refused
   principal ends the stream and the session; one poller serves two streams; a panicking source
   leaves the manager answering.
3. **History.** The tiers, `manager.ini` `[Metrics] Tiers` and its validation, the store,
   `MetricsHistory`/`As`, `GET /api/node/metrics/history`, the chart. Done when: simulated samples fold
   to the expected min, average, max and count in every tier, with gaps as count 0; slots wrap with no
   growth of the file; a range picks the documented tier (1 h tier 1, 2 h tier 2, 30 days tier 5);
   each validation rule refuses its case and the defaults apply; a changed tier starts empty and the
   others keep their rows; a store that cannot be written leaves the live stream running; the file
   size and the bytes written per simulated hour are reported.
4. **Log consultation.** `ReadLog`/`As`, `GET /api/instances/{id}/log`, the log view. Done when: an
   unknown instance is refused with no process spawned; the argument vector is exactly the one above
   (a fake runner); the cursor pages forward; caps hold; a non-`pai` principal is refused; the Windows
   reader pages across `.previous.log` and the current file.
5. Later, not now: an export (OTLP through b1, or a Prometheus endpoint) when a central store is
   wanted; the director's node-prefixed routes come with Part 2's slice 7.

**Open after this design:** the sampler as an unprivileged child (only if the review of slice 2 asks);
the manager's own action log in the log view; live log follow (polling the cursor is enough first);
a per-instance GPU attribution (which instance uses which card).

#### A future central log collector (not built in this step)

Asked by the user on 2026-11-21 so that today's choice does not block it. Logs stay on the hosts in
this step; this is the direction, judged now, built later (with or after Part 2).

| Piece | Choice | Source |
| --- | --- | --- |
| Common format | OTLP logs | VictoriaLogs ingests OpenTelemetry, Loki JSON, Elasticsearch bulk, journald export format and others ([data ingestion](https://docs.victoriametrics.com/victorialogs/data-ingestion/)) |
| Node agent, recommended | **Fluent Bit** v5.2.0, C, Apache-2.0 (GitHub record); Linux, macOS, Windows ([supported platforms](https://docs.fluentbit.io/manual/installation/supported-platforms)); `systemd` input reading the journal through libsystemd with a `_SYSTEMD_UNIT` filter and a cursor kept in its `DB` ([systemd input](https://docs.fluentbit.io/manual/pipeline/inputs/systemd)), `tail` for the Windows log files; OTLP out over HTTP (protobuf) or gRPC with `tls.crt_file`/`tls.key_file`/`tls.ca_file` ([OpenTelemetry output](https://docs.fluentbit.io/manual/pipeline/outputs/opentelemetry)) | as cited |
| Node agent, alternative | The OpenTelemetry Collector: one agent for metrics and logs only if metrics went OTel (option b), but its `journald` receiver is alpha, Linux only and shells out to `journalctl` (above), and contrib is 97 to 109 MiB | as cited |
| Central store | **VictoriaLogs** v1.54.0 single node, Apache-2.0 (GitHub record), 12 MiB `linux-amd64` archive, on the director's node; 7 days retention by default (`-retentionPeriod`), an optional disk cap; TLS and basic auth in the open binaries, **mTLS only in Enterprise** ([security](https://docs.victoriametrics.com/victorialogs/security-and-lb/), [flags](https://docs.victoriametrics.com/victorialogs/)) | as cited |
| Not chosen | Promtail: end of life on 2026-03-02, development moved to Grafana Alloy ([Promtail](https://grafana.com/docs/loki/latest/send-data/promtail/)). Loki: **AGPL-3.0** (GitHub record of `grafana/loki`, so the user's belief is confirmed), with Alloy (Apache-2.0) only if Grafana dashboards are wanted. Vector (MPL-2.0): valid, no gain over Fluent Bit here | as cited |

The trust boundary is the part to design then, not the tools: open VictoriaLogs cannot check a
client certificate, so it listens on the director's loopback only and the nodes reach it through
an ingest endpoint of the director that checks each node's pinned key (Part 2's identity, no
second fabric); the agent on each node is one more account reading the journal (Fluent Bit's user in
`systemd-journal`, read only, the same rule journalctl(1) states). Logs then leave the node
continuously, a change from "read on demand" the user approves when that step comes; the on-demand
`ReadLog` stays for a node that has no collector.

**What it changes now: nothing.** Option (a) leaves the logs where they are and in their native form
(journal fields, plain files), which is what Fluent Bit reads; the only care is that `ReadLog`'s
cursor stays the journal's own cursor on Linux, so the web view and a collector speak of the same
positions. Choosing OTel for metrics now (option b) would not make the log half easier: the
collector's journald receiver is the weakest piece. Not a decision for this step.

#### Decisions (user, 2026-11-21)

Approach (a): the manager's own sampler (`pai-hwmon`) and tiered store, an export (OTLP or
Prometheus) later; (b), (c) and (d) are not taken. The slices run in order: 1 readings move
(`pai-hwmon`, the `sample-cost` numbers reported), 2 live stream, 3 history, 4 log consultation;
5 (the export) waits until a central store is wanted. Also decided: history is RRD-style tiers with the resolution following the range
(5 s over 1 h, 20 s over 4 h, about 720 points), configurable (the tiers above); every `pai` member
reads instance logs; the disk card follows the engine's default models folder only.

### Resources belong to the server profile (step 10 item 13, "placement follows"): accepted (user, 2026-11-21)

**Decided by the user (2026-11-21)**, each the recommendation of (6) below: (1) option (a) of (3): the
GPUs in the server profile (`[Server] Devices` -> `--device`), the split weights and optional fixed
values (`n-gpu-layers`, `n-cpu-moe`, `fit-target`) in the model profile, `--fit` places the rest;
(2) a GPU is named by the probe's id (`ROCm0`), the bus id later only if ordinals move; (3) the memory
check counts only the GPUs of the instance's server profile (every GPU when `Devices` is unset): this
**replaces** the 2026-11-17 rule, confirmed 2026-11-20, that every instance is checked against every GPU
of the node ((b) 2 and G2's "Decided by the user" now read as superseded on this point); (4) RAM is
shown in the inventory, with no budget and no OS limit. The slices R1 to R5 of (5) run in order; the
drawn views (`docs/architecture/system.html`, "Saving a model profile, with the memory check") change
with R3, when the check does.

The user's words (2026-11-21, translated): "A bare model profile cannot know the GPU/CPU distribution
until it is applied to an instance. There is a server profile that associates the resources to use with
the profile, so a profile associated with that hardware has those resources. Maybe we also need to
profile the available hardware: GPU VRAM, RAM, GPU, CPU and the SDKs used?"

**What exists (read 2026-11-21 at bcac0a6).** A model profile (`models.ini` section) may carry `device`,
`split-mode`, `tensor-split` (the GD card, 3585c39, edits these three: `checks::WEB_MODEL_PROFILE_KEYS`),
`n-gpu-layers`, `fit`, `fit-target` (web, Hardware card) and `n-cpu-moe`, `override-tensor` (not on the
web). A server profile (`server@<p>.ini` `[Server]`, `ServerConfig`) already carries two resources:
`Threads` and `ThreadsBatch` (`-t`, `--threads-batch` on the router's argv, `launch::server_args`), and
`ModelsMemoryMargin`. The memory check ((b) 2) compares the full-GPU estimate of one model profile with
every GPU of the node (user, 2026-11-17 and 2026-11-20: "a per-instance `Devices` key waits until
asked"; it is asked now). The node's GPUs are the manager's probe (`Devices()`, G2); the installed engine
and runtime versions are the `versions/` files the bff reads (`GET /api/node`, slice 13); the bff takes
the thread limit of the server form from its own `available_parallelism` (`bff/src/server.rs` 104),
which is only right while the bff and the node are one machine; `pai-app`'s `hwmon::system_ram` reads
`MemTotal` (moving to `pai-hwmon` in 13d slice 1).

**Facts the design rests on** (llama.cpp v1.0.0, `build/linux/llama.cpp`, read 2026-11-21):

1. *The router's own argv wins over every model profile.* `load_models` cascades `[*]` into each section,
   then overlays the router's command-line options on every model's preset (`preset.merge(base_preset)`,
   `tools/server/server-models.cpp` 791-795; `common_preset::merge` overwrites, `common/preset.cpp` 136).
   Only the reserved options are dropped first (`unset_reserved_args`, 285-302: models dir, max, margin,
   preset, keys, TLS, connect; not `device`, `threads`, `tensor-split`). So a flag the server profile
   puts on the router's argv is the value of that option in every child, whatever a model profile says.
   That is already true today for `Threads`.
2. *`--device` is parsed by the router at its start*: `parse_device_list` loads the backends and throws
   `invalid device` for an unknown or CPU name (`common/arg.cpp` 1127-1145, option at 2701, env
   `LLAMA_ARG_DEVICE`); so an instance given a vanished device does not start (fails closed, the journal
   or the instance log says why).
3. *`--fit` is on by default* and "adjusts unset arguments to fit in device memory" (`common/arg.cpp`
   2852, `common/common.h` 497), with a per-device margin `--fit-target` (default 1024 MiB each,
   `common.h` 502). With `n-gpu-layers` and `tensor-split` left unset, llama.cpp itself derives the
   placement over the devices it is given.
4. *`tensor-split` is positional over the device list*: the first `n_devices` weights are used, the rest
   ignored; all zero means an even split (`src/llama-model.cpp` 1746-1765). A weight list written for
   two cards stays valid on one or three.
5. *`--list-devices` prints name, description and memory only* (`common/arg.cpp` 1149-1172). The PCI bus
   id exists in the device properties (`ggml_backend_dev_props.device_id`, set by the CUDA/HIP and
   Vulkan backends: `ggml/include/ggml-backend.h` 178-181, `ggml-cuda.cu` 6159, `ggml-vulkan.cpp` 15878)
   but is not printed: two identical cards (the user's two R9700) are told apart only by ordinal.
6. *No llama-server flag caps host memory* (none in `common/arg.cpp`); a RAM limit is an OS mechanism:
   systemd's `MemoryMax=` (systemd.resource-control(5),
   https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html) or a Windows
   job object's `JobMemoryLimit` (https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information),
   both of which kill or fail allocations rather than plan placement.

#### (1) The node inventory

What the node holds, one read: the GPUs (`id` as `device` takes it, `name`, `backend`, `totalMib`,
`freeMib` at `probedAt`: G2's rows, unchanged), `ramMib` (total physical memory), `cpuThreads` (logical
CPUs the manager may use: `std::thread::available_parallelism`,
https://doc.rust-lang.org/std/thread/fn.available_parallelism.html) and the installed engine and
runtimes (the `versions/` files, slice 13). The backends the engine can load are the distinct
`backend` values of the GPU rows: no second source.

- **Where from.** The manager, not the bff: across machines (Part 2) only the node's manager sees its
  hardware, and on one machine the bff's `PrivateDevices=yes` hides the GPUs anyway. `Devices()` /
  `DevicesAs()` gain `ram_mib` and `cpu_threads` (D-Bus `(t, a(ssstt), t, u)`, pipe fields of the same
  names; additive while protocol 3 is in no release); RAM from `hwmon::system_ram` once 13d slice 1 has
  moved it to `pai-hwmon` (the manager links it), else a reader of `MemTotal` (proc_meminfo(5)) and
  `GlobalMemoryStatusEx` (Microsoft Learn) in `pai-core`. The versions stay the bff's file read (the
  files are world-readable and per node; across machines they join the same reply in step 14).
- **The bff.** `GET /api/node/devices` gains `ramMib` and `cpuThreads` (same gate, same deadline); the
  server documents' `limits.Threads` and the save's cap stay on the bff's own count (amended at the R1
  review, 2026-11-21: `Devices()` runs the probe, up to 30 s, which the 5 s document read cannot carry);
  the Server form's thread fields read `cpuThreads` from this route in R5. Open note: design (2) keeps a
  `Threads` above `cpuThreads` legal, so the save's bff-count cap is to be relaxed later, not in R1.
- **Stable identity.** Options: (i) the ggml name (`ROCm0`), as the probe and `device` already use; free,
  but an ordinal can move when a runtime is missing (the "ROCm availability" note in
  `crates/app/src/devices.rs`) or a card is added; (ii) the PCI bus id, which needs a llama.cpp patch
  printing `device_id` in `--list-devices` (fact 5; one line, a candidate upstream PR) and a resolve
  step from bus id to the current name at every launch (the instance's `run-instance` cannot ask the
  manager's probe on Linux: it execs llama-server). **Recommended: (i) now**, shown with the card's
  name and size, (ii) when a moved ordinal is seen on a real node; fact 2 makes (i) fail closed.
- **Step 14.** The director reads each node's inventory through the same method over its transport
  and caches it per node; a server profile names devices of its own node only (profiles are per node and
  per engine), so nothing crosses nodes.

#### (2) The server profile's resource set

| Key in `[Server]` | Values | Maps to | Absent |
| --- | --- | --- | --- |
| `Devices` (new) | an ordered list of probe ids (`ROCm0,ROCm1`), or `none` (CPU only) | the router's `--device` (fact 1: wins over every model profile's `device`) | no flag: every GPU of the node, as today |
| `Threads`, `ThreadsBatch` (exist) | counts | `-t`, `--threads-batch` (already) | llama.cpp's default |
| `ModelsMemoryMargin` (exists) | MiB per device | `--models-memory-margin` | 0 |
| RAM budget | not now (decision 4) | | |
| Backend | not a key: the backend is in each id | | |

- **Validation at a save** (`checks.rs`, one refusal per rule, keyed): `Devices` by G1's `device_list`
  rule (no spaces, no CPU, `none` alone; key `device-bad-name` `[Devices, entry]`, already in both
  strings files). Not judged against the probe, for G1's reason (a card can vanish for a while and must
  not block saving other keys); the web UI flags an id the inventory lacks, and the instance then does
  not start (fact 2). Mixed backends (`ROCm0,Vulkan1`) stay a warning, the UI's, as in the GUI. A
  `Threads` above `cpuThreads` stays legal (llama.cpp takes it); the form's limit already caps it.
- **Who writes it**: `configure` (the server profile save, unchanged path: `render_for`, so `Devices`
  becomes a `ServerConfig` field or a save drops it). The value reaches llama-server as one argv entry
  (`Command::arg`, no shell), so the content rule is about llama.cpp's parse, not injection.

#### (3) The model profile's placement keys

| Option | What the model profile keeps | Cost | Risk |
| --- | --- | --- | --- |
| **(a) Intent in the model profile, devices in the server profile, placement by llama.cpp** | `split-mode`, `tensor-split` as positional weights over the server profile's `Devices` (fact 4), `n-gpu-layers`, `n-cpu-moe`, `fit-target` as optional pins; unset ones derived by `--fit` at load (fact 3) | one key and one flag (`Devices` -> `--device`); the render stays a byte copy; the memory check reads the pair | a profile tuned with pins for two cards keeps them on one card (an `n-cpu-moe` too low fails at load: the launch fails, as a bad pin does today); the cure is a second model profile for that hardware |
| (b) Per-assignment overrides | the same, plus placement per (instance, model profile) in `assignments.ini` values or a new file, injected into the render | a value grammar for assignments, a render that edits sections, new routes and a new card | the most precise; the tuning (optimize-preset) is then per pair, which the pool was meant to avoid |
| (c) Placement keys move to the server profile | nothing | the keys go on the router's argv | one `n-cpu-moe` or `tensor-split` for every model of an instance of N models: wrong for all but one; fits Strata's one model per server (13c) only |

**Recommended: (a)**, (b) only when a pinned profile is wanted on two different server profiles at once
(nothing needs it today). Under (a) the manager derives nothing itself: it passes `--device`, and
llama.cpp fits the rest.

**The memory estimate per (server profile, model profile) pair.** `needed` unchanged (the full-GPU
estimate of the model profile, (b) 2). `available` becomes the sum, over the cards the instance's
server profile names in `Devices`, of total less `GPU_RESERVE` (one card seen by two backends counted
once, as `gpu_pool`); `Devices` absent: every card, as today; `none`, or no named id in the probe:
skipped, as for a CPU-only node or a failed probe. Judged where it is today (model profile write,
assignment), plus two places where `available` can shrink: a save of a server profile in use that
changes `Devices`, and `SetServerProfile`; there a profile in the instance's render is refused when it
fits today's `available` and not the new one (the growth rule turned round: never re-judge what was
already over). Refusal `model-profile-over-memory` `[profile, instance, needed, available]`, unchanged.
This replaces the user's 2026-11-17 and 2026-11-20 rule "every GPU of the node" (decision 3). The read
(j) 9 becomes the instance's model profile rows with `needMiB` and the reply's `availableMiB` from the
pair; a bare model profile shows `needMiB` only.

#### (4) The GD card, brief 16 and the files in place

- **The GD card** (fe brief 14, built): `device` leaves the model profile page: the key moves from
  `WEB_MODEL_PROFILE_KEYS` back to `MODEL_PROFILE_KEYS_NOT_ON_WEB`; the order and the per-GPU list move
  to the server profile. The card keeps `split-mode` and the weights, as positions ("1st GPU, 2nd
  GPU") with a note that they apply to the GPUs of the server profile each instance uses, and, when the
  model profile is assigned, the names of those GPUs per instance (read from the server profiles; no
  new route). The free-memory line moves with the GPU list.
- **Brief 16.** 17a moves the Server form as it is (not blocked). A new sub-slice after 17a, the
  server profile's **GPUs** card: pick and order probe ids (the GD card's Move up / Move down reused),
  "All GPUs" (absent) and "None, CPU only" (`none`), each card's name, size and free memory as of
  `probedAt`, an id the inventory lacks flagged; the thread fields read `cpuThreads`. The designer's
  brief first.
- **The GUI and `pai`** keep editing a model profile's `device` (macOS runs llama-server itself, with no
  server profile pool); under the manager that `device` applies only while the server profile leaves
  `Devices` unset (fact 1). Whether the GUI gains the `Devices` field is a later step.
- **Existing files** (pre-release, no migration code): every key stays where it is and keeps its
  meaning. A server profile without `Devices` behaves exactly as today; a model profile's `device`,
  `tensor-split`, `n-gpu-layers`, `n-cpu-moe` still reach llama-server, `device` only until its server
  profile sets `Devices`. Nothing is moved or rewritten.

#### (5) Slices (each test first; Linux, the Windows code type-checked)

1. **R1 inventory** (`pai-core`, `pai-manager`, `pai-bff`): `Devices()` gains `ram_mib` and
   `cpu_threads`; the route gains `ramMib`, `cpuThreads`. Done when: a fake answer gives both fields on
   D-Bus and the pipe and in the route; neither server document read (instance, pool profile) calls
   `Devices()`, so their `limits.Threads` stays the bff's own count (amended at review, see (1)).
   *As built (2026-11-21, reviewed):* `pai_core::host` reads `MemTotal` / `GlobalMemoryStatusEx` and
   `available_parallelism`, 0 for no figure (`crates/core/src/host.rs`); the manager answers both
   whatever the probe did (`ProbedGpus::devices`, `service.rs`), D-Bus `(ta(ssstt)tu)`, pipe `ram_mib`,
   `cpu_threads` (a reply missing either is refused); the route adds `ramMib`, `cpuThreads`
   (`bff/src/node.rs`); the document reads ask no `devices` (test
   `the_document_reads_ask_the_manager_no_devices`). RAM is read in `pai-core`, not `pai-hwmon`.
2. **R2 `Devices`** (`pai-core`, `pai-bff`): the `ServerConfig` field, `render_for`, `launch::server_args`
   adds `--device <value>` (absent: no flag; `none`: `--device none`), the save refuses by
   `device_list`, the bff's server document carries it. Done when: the argv pins all three cases; a
   save of `ROCm0, ROCm1` is 422 `device-bad-name` naming `Devices`; a save of another key keeps
   `Devices`.
   *As built (2026-11-21, reviewed):* `ServerConfig::devices`, loaded blank-as-unset and rendered as a
   `Devices = ` line only when set (`crates/core/src/server_cfg.rs`); `launch::server_args` pushes
   `--device` and the value as two argv entries only when set, `none` passed as is (`launch.rs` 104-107);
   the rule is `checks::device_list`, shape only, in the manager's write (`checks::server_file`) and the
   bff's save (`server_cfg::check`); the bff's document and both save routes carry `Devices`. The GUI's
   Server tab carries it as a hidden form field and `pai server set` keeps it, so neither save drops it;
   `pai server show` has no `Devices` row yet. Since the GUI shares `server_args`, a `Devices` in a
   per-user `server.ini` (macOS) would also reach the configurator's own llama-server.
3. **R3 the pair check** (`pai-manager`): `available` from `Devices`; the two new judging points. Done
   when: a profile that fits two cards is refused at a switch to a one-card server profile and at a save
   narrowing `Devices`, nothing written; a profile already over is not re-judged; `none` and an
   unknown id skip.
   *As built (2026-11-21, reviewed):* `available` is `pai_core::devices::devices_usable` (`devices.rs`
   264): the sum of `usable()` over the cards `Devices` names, every card when unset, 0 for `none` or
   ids the probe knows none of (0 skips, as a node with no card); the existing check reads it through
   `instance_devices` (`service.rs` 2050, 2625). The two new points (`check_shrink_memory_of`, 2333)
   refuse only a model profile that fits today's cards and not the new ones: `set_server_profile`
   (1320) and a save of an in-use server profile whose `Devices` changed (`check_contents`, 2187); a
   save that keeps `Devices` judges nothing. Pinned by `manager_pair_memory_check_e2e` (R3-PIN-1..9).
   Drawn in `docs/architecture/system.html`.
4. **R4 keys and the read** (`pai-core`, `pai-bff`): `device` back to the not-on-web list (the pin test
   in `pai-app` green); the instance's model profile rows gain `needMiB`, the reply `availableMiB`.
5. **R5 web UI** (designer's brief first): the server profile's GPUs card; the GD card without the
   device list; the thread fields capped at `cpuThreads` of `GET /api/node/devices`.

Later, each when asked: the bus-id patch ((1) option ii), a RAM budget (decision 4), per-assignment
overrides ((3) b), CPU pinning between instances (`--cpu-mask`, `common/arg.cpp` 1508).

**Security.** No new boundary: the inventory rides `Devices`/`DevicesAs` (the quiet `control` check,
the `As` gate for the bff); `Devices` is written under `configure` as every server key. A `pai` member
can already name any device in a model profile; naming it in a server profile adds only that it
applies to every model of the instance. A wrong id fails the instance's start, never the manager. The
RAM and CPU counts are read by root and given only to callers the gate admits.

#### (6) Decisions for the user (recommendation first)

1. **Where placement lives**: (a) devices in the server profile, split weights and optional pins in the
   model profile, llama.cpp `--fit` derives the rest; (b) per-assignment overrides; (c) all placement
   keys in the server profile.
2. **How a server profile names a GPU**: the probe's name (`ROCm0`) now, the bus id later if ordinals
   move; or the PCI bus id now (a llama.cpp patch and a resolve step at launch).
3. **The memory check counts**: only the GPUs of the instance's server profile (every GPU when
   `Devices` is unset); or every GPU of the node, as decided on 2026-11-17.
4. **RAM**: shown in the inventory, no budget yet; or a budget the check reads (allowing a CPU share and
   relaxing the strict full-GPU rule); or a limit the OS enforces (`MemoryMax=`, a job object).

### Layers, MoE and tensor placement on the web model profile page (step 10, 2g and 2j): accepted (user, 2026-11-21)

**Decided by the user (2026-11-21)**, see (5): 1. the sliders read the saved `model`; 2. an `hf-repo` not
yet downloaded gives no layer facts now (the remote header read waits for M1/M2); 3. **not** the
recommendation: the manager **refuses** a model profile whose `override-tensor`, `device-draft` or
`mmproj-device` names a GPU outside its instance's server profile `Devices` ((3) "The device bound");
4. `moe-cache-mib` later, with its floor.

The requirement (`PLAN.md`, order of 2026-11-21): after C2 and before slice 17, the web model profile page
gets the GUI's per-GPU layer sliders (2g, which need the model's layer count), the Draft device and
Vision GPU lists from the probed GPUs, and tensor placement, `override-tensor` (2j). This section is the
backend; the designer's briefs follow it.

**What exists (read 2026-11-21 at c0dac10).**

- *The GUI's sliders are not a key of their own.* `pai-app` `gpu_split.rs` (module header, `Layout`,
  `block_counts`, `set_blocks`): the per-GPU slider edits the **`tensor-split` weights as block counts**.
  llama.cpp cuts `block_count + 1` positions (every block plus the output layer) by the normalized
  weights; under partial offload the first `positions - n-gpu-layers` go to the CPU and the split runs
  over the rest (`Layout::new(n_layer_all, n_gpu_layers)`, `n_gpu_layers` negative = all). A weight
  vector summing to `Layout.count` IS the per-device count, so `set_blocks` stores the counts as the
  weights (one slider up, the later rows give it back first). The view is per split mode: blocks only
  under `layer`; `row` and `tensor` edit ratios; `none` keeps the head row. With no model read
  (`Layout` is `None`) the table falls back to raw weights. The GUI reads `block_count`, `expert_count`,
  `expert_used` from the header (`gguf.rs` `ModelInfo`, `models_tab.rs` 1196-1199); its "GPU layers"
  and "MoE CPU layers" sliders run `0..=block_count` (`state.slint` 346, `models_page.slint` 483, 494),
  the MoE one usable only on a MoE model.
- *`override-tensor`* (`pai-app` `tensor_override.rs`): rules `<regex>=<buffer type>` joined by `,`; the
  GUI's table builds them from canned kinds (`KINDS`, a layer range written as `blk\.(10|11)...`) or a
  custom regex with `,` and `=` stripped (`sanitize_pattern`), the device from a dropdown of probed ids
  plus `CPU`, unknown ids kept as `(custom)`; its picker and coverage lines read the model's tensor list
  (`gguf::read_tensors`, through ggml). The shared rule today is `checks::override_tensor`: every rule has
  a pattern and a device (`override-no-pattern` `[n]`, `override-no-device` `[n, pattern]`), plus the `;`/`#`
  rule of `MODEL_PROFILE_TEXT_KEYS`.
- *Keys on the web* (`checks.rs` 263-361): `tensor-split`, `split-mode`, `device`, `n-gpu-layers`,
  `device-draft`, `mmproj-device`, `mmproj-offload` are already in `WEB_MODEL_PROFILE_KEYS`;
  `n-cpu-moe`, `moe-cache-mib`, `override-tensor` (and the file picks) are in
  `MODEL_PROFILE_KEYS_NOT_ON_WEB`. `n-cpu-moe` has no value rule (not in `MODEL_PROFILE_INT_KEYS`).
- *Reading a model as root already happens*: the memory check resolves a profile's `model` (own key,
  else the pool's `[*]`; absolute, else against the instances' working directory, which is node-wide:
  `launch::instance_current_dir`) and reads every shard's header with `gguf_shape::local_shape`
  (regular files only, header capped at `MAX_HEADER`), skipping a profile with a `FETCH_KEYS` key
  (`service.rs` `estimated_of`, 2656). `Shape` has `layers` (from `blk.N.` tensor names) and `moe`, not
  the header's `block_count` or expert counts; `Header.tensors` and `tensor_bytes` give every tensor's
  name and size with no ggml.
- *The bff cannot read the model*: `ProtectHome=yes`, and a `model` path may be anywhere (relative to
  the instances' directory, under `/var/lib/private-ai`, 0750 for its data root); across machines
  (Part 2) only the node sees its files. So the manager reads, as M3's `ModelFilesAs` does.

**Facts** (llama.cpp v1.0.0, `build/linux/llama.cpp`, read 2026-11-21):

1. `--n-cpu-moe N` throws below 0 and otherwise adds one CPU override per block `0..N`
   (`common/arg.cpp` 2730-2738, `common/common.h` 1291-1298): a value above the block count matches
   nothing, but the loop runs `N` times in the instance, so a huge `N` costs memory there.
2. `--n-gpu-layers` takes an integer, `auto` or `all` (`common/arg.cpp` 2773-2789).
3. `--override-tensor` splits on `,` and each rule at its first `=`, trims nothing, and looks the right
   side up **exactly** among the default buffer type names of every device ggml loaded
   (`common/arg.cpp` 229-259; `ROCm0`, `CUDA0`, `Vulkan0`, `CPU`): no `=` or an unknown name throws,
   the instance's model never loads. The list is every device, **not** the `--device` subset.
4. The loader compiles each pattern with `std::regex` (ECMAScript) and, on a match to a non-CPU buffer
   type, uses it as is (`src/llama-model-loader.cpp` 1330-1345): an `override-tensor` (and likewise a
   `device-draft` or `mmproj-device`, parsed by their own `parse_device_list`) can place weights on a
   GPU outside the server profile's `Devices`; the memory check (R3) counts only `Devices`. A pattern
   with a leading space (`a=CPU, b=ROCm0`) matches no tensor name and does nothing, silently.

#### (1) The model facts: a manager read (`ModelInfo` / `ModelInfoAs`)

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. The manager reads the saved profile's model**, by profile id | one method on D-Bus and the pipe, one bff route; reuses `estimated_of`'s resolution | root parses a header at a path a `pai` member wrote: already true of the memory check | **Recommended** |
| B. The manager reads a path the page sends (the unsaved form value) | the same plus a path rule (only under `paths::models_dir()`, canonicalized) | a probe of file names under that folder; a second resolution rule | later, with the M4 `model` picker, if the saved value proves too slow a loop |
| C. The bff reads the file | none on the manager | cannot: `ProtectHome`, and no access across machines | rejected |

**The method.** `ModelInfo(profile)` / `ModelInfoAs(principal, profile)`, the gate of reading
`models.ini` (`control` asked quietly; the `As` form for the bff's account and a current `pai` member;
refused with nothing read). The manager finds the section in `models.ini`, else `models.ini.disabled`
(a switched-off profile has sliders too); none, or `*`: keyed `Refusal` `no-model-profile-named` `[id]`
(already in `checks.rs` and both strings files). It resolves `model` exactly as `estimated_of` does (one
helper shared by both, so the sliders and the memory check read the same file). Outcomes, all a
success of the call:

| Case | Reply |
| --- | --- |
| a `FETCH_KEYS` key (`hf-repo`, `model-url`, `docker-repo`) in the section | `info` null, `reason` `model-info-fetched` `[key]`; nothing read |
| no `model` | `info` null, `reason` `model-profile-no-model` `[id]` (existing key) |
| the path (or a shard) does not exist | `info` null, `reason` `model-info-missing` `[path]` |
| not a regular file, not a GGUF, cut short, over 256 shards | `info` null, `reason` `model-info-unreadable` `[path]`; never the reader's text, so no byte of a non-GGUF file reaches the reply |
| read | `info` = `{blockCount, moe, expertCount, expertUsed}` |

`blockCount` is `<arch>.block_count`, else the number of `blk.N` blocks the tensors name (a new
`Shape.block_count`, read in `shape_of`); `moe` is `Shape.moe` (routed-expert tensors); `expertCount`
and `expertUsed` are `<arch>.expert_count` and `<arch>.expert_used_count`, 0 when absent (what the
GUI's `ModelInfo` reads). In 2j the reply gains `tensors`: `[{name, bytes}]` from `tensor_bytes` over
every shard, each shard's tensors in data-offset order (`tensor_bytes`), shard after shard, cut at the
first row that would pass either bound, then `tensorsTruncated: true`: 20 000 rows
(`gguf_shape::MAX_TENSOR_ROWS`, the one constant; `client.rs` uses it, no copy), and 917 504 bytes
(896 KiB, `gguf_shape::MAX_TENSOR_LIST_BYTES`) of the rows' JSON, each row counted as
`serde_json::to_string(&{"name","bytes"}).len() + 1`. The byte bound exists because the pipe's
`MAX_FRAME` is 1 MiB (`client/pipe.rs`) and a GGUF tensor name has no length bound in our reader (and
JSON escaping can grow it): the cut is in `pai-core` `tensor_rows`, so both transports answer the same
list, and the reply's other fields keep 128 KiB under the frame. Raising the frame was rejected: it
bounds what either side must buffer from the other. D-Bus and pipe carry
the same fields (`model` the resolved absolute path, `info`, `reason` as `{key, args}`); additive while
the protocol is in no release. As built (L1, reviewed 2026-11-22): `model` is set only with `info`; every `reason` row answers `model`
null (the path, when there is one, is the reason's arg); the pipe spells the counts in its snake_case
(`block_count`, `expert_count`, `expert_used`), the bus as `(s, (b has_info, t, b, t, t), (s, as))`
with the empty string for null; the bff's JSON is camelCase.

**The `hf-repo` profile not yet downloaded.** (i) **Recommended now**: no facts (`model-info-fetched`);
the page shows raw weights and plain number fields, as the GUI does with no model read. (ii) After M1 and
M2: the bff reads the repo's remote header by HTTP range through M1's fixed-endpoint curl profile
(`pai-app` `hf_fit` already does this for the GUI), so the facts come from huggingface.co; a later
slice, only if asked. (iii) Finding the downloaded file in llama.cpp's cache layout under
`paths::models_dir()`: rejected, the layout is llama.cpp's and moves between releases.

**The bff route.** `GET /api/profiles/model/{id}/model-info` behind a session, the manager asked as the
signed-in user (`ModelInfoAs`), on `MANAGER_TIMEOUT`; answers
`{"model":"/abs/x.gguf"|null,"info":{"blockCount":64,"moe":true,"expertCount":128,"expertUsed":8}|null,"reason":{"key":"…","args":[…]}|null}`
(2j adds `tensors`, `tensorsTruncated` inside `info`). Failures as every `As` read: 401 `session-*`, 403
and a refusal ending the session, 404 `no-model-profile-named` (any other keyed refusal 422), 503 `client-unavailable` for no manager
and for the deadline alike (the shared `read_answer` rule of the bff's `As` reads, no 504 split: a read
changes nothing, so an unknown outcome needs no own code), 502 `client-other`. It moves the idle deadline like any user read (it is asked on open and after a save that
changes `model`, not polled).

**Bounds and caching.** The header read is `local_shape`'s (8 MiB first read, `MAX_HEADER` cap per
shard) with a 256-shard cap; no cache in the manager or the bff (one read per page open or `model`
save; a `(path, size, mtime)` cache waits until a measured page is slow). Several concurrent reads are
bounded by the bff's existing per-session handling; nothing new.

#### (2) 2g: the keys and how the sliders map

- **Per-GPU layer sliders write `tensor-split`**, already a web key: the weights are the block counts
  of `gpu_split::Layout` under `split-mode` `layer` (absent = `layer`), exactly as the GUI stores them;
  under `row` and `tensor` the page edits ratios, under `none` nothing. The fe reimplements `Layout`,
  `block_counts` and `set_blocks` (pure arithmetic; the fe cannot link `pai-app`), pinned by the test
  vectors of `gpu_split.rs`'s unit tests copied into the fe's tests. No server rule changes: any weight
  list `tensor_split` accepts stays legal, and a hand-written ratio shows as its block counts.
- **`n-gpu-layers`** (web already): the slider `0..=blockCount`, "auto" = key absent, as the GUI.
- **`n-cpu-moe` moves to `WEB_MODEL_PROFILE_KEYS`** with a value rule: `("n-cpu-moe", 0..=4096)` in
  `MODEL_PROFILE_INT_KEYS` (fact 1: below 0 never loads; 4096 is `shape_of`'s block cap, so no real model
  is refused and a huge loop is). The slider shows only when `info.moe`; "auto" = absent.
- **`moe-cache-mib` stays off the web**: the GUI refuses a value under its floor (`moe_cache::floor`,
  `pai-app`), which needs the tensors and that code in `pai-core`; it comes with 2j's tensor list or
  later (decision 4).
- **Draft device and Vision GPU lists**: no backend change. `device-draft` and `mmproj-device` are web
  keys with `checks::device_list`; the lists come from `GET /api/node/devices` (G2), each id as the probe
  prints it, an id the probe lacks kept and flagged (G1's rule), `none` offered for `mmproj-device`.
- **With R4/R5 (after slice 17)**: `device` leaves the page; the slider rows become positions over the
  server profile's `Devices` (fact 4 of "Resources belong to the server profile"); the arithmetic is
  unchanged, so 2g is not redone.

#### (3) 2j: `override-tensor` on the web

- **The key moves to `WEB_MODEL_PROFILE_KEYS`**; the body carries the INI value (rules joined by `,`, no
  spaces), built by the page as the GUI's `render` does. The canned kinds: the fe keeps its own copy of
  `KINDS` (id, pattern, block suffix; llama.cpp regex constants that change with llama.cpp only), and an
  unknown pattern reads as custom, so a drift never rewrites a rule. Option B, serving `KINDS` from
  `pai-core`, costs a move and a route for a dozen constants: not now.
- **Value rules added to `checks::override_tensor`** (shape only, never against the probe, for G1's
  reason), each keyed:
  - `override-tensor-space` `[n]`: a rule whose pattern or device has a leading or trailing space
    (fact 3: the device then never matches; fact 4: the pattern matches nothing);
  - `override-bad-device` `[n, device]`: a device that is not `[A-Za-z0-9_]+`, or is `cpu` in another
    case than `CPU` (exact match, fact 3).
  Both run only when the key's line changed, through the same `known_key_line` exemption as
  `comment_markers` (decision c1): a hand-written value already in the file never blocks saving other
  keys of the section. The existing two rules keep running on the value. `pai-app`'s local
  pre-checks (`validate_for_save`, `tensor_override::validate`) pass `changed = false`: the GUI's
  table trims the device and sanitizes the pattern, and the manager still judges every write.
- **Not judged**: the regex itself. Rust's `regex` crate is not ECMAScript (`std::regex`), so it would
  refuse some patterns llama.cpp takes and pass some it throws on; a bad pattern fails the model's load
  in the instance, never the manager.
- **The tensor list** for the picker and the coverage lines is `info.tensors` of (1), read by
  `pai-core` (`tensor_bytes`), not ggml.
- **The device bound** (fact 4, decision 3). Refusal `model-profile-device-outside`
  `[profile, instance, key, device]` (`ManagerError::Invalid`, a keyed `checks::Refusal`, both strings
  files). The devices a model profile names: each entry of `device-draft` and `mmproj-device` (the
  section's value, else `[*]`'s, as llama.cpp cascades), and the device of each `override-tensor` rule;
  `CPU` and `none` name no GPU. An `-ot` right-hand side is a buffer-type name, but llama.cpp v1.0.0
  offers only each device's own buffer type, named as the device (`CUDA0`, `ROCm0`, `Vulkan0`, `CPU`),
  and refuses any other name, host buffers such as `CUDA_Host` included (`common/arg.cpp`
  `parse_tensor_buffer_overrides`: `buft_list` from `ggml_backend_dev_buffer_type`, "unknown buffer
  type"), so judging it as a device is right: a name outside `Devices` is refused, failing closed on a
  value the engine would refuse anyway. Compared case-insensitively with the entries of the instance's server
  profile `Devices` (`ggml_backend_dev_by_name` matches so; an `-ot` device in the wrong case is
  `override-bad-device`'s or the load's). `Devices` unset: every GPU, nothing refused; `Devices = none`:
  any GPU named is outside. Not judged against the probe (a vanished card is the launch's, G1). The
  `device` key itself is not judged: the router's `--device` beats it (fact 1 of "Resources belong to
  the server profile"). Judged where R3's memory check is, on the same render rule (`pai_core::assign`,
  every instance where the profile is in effect, the first offending instance in id order named):
  a model profile write (`check_contents` and `set_model_profile`, rendering from the new record text),
  an assignment, and the two shrinking points, `SetServerProfile` and a save of an in-use server profile
  that changes `Devices`, where, as in R3, only a profile that passes the old set and fails the new one
  is refused (never re-judge what was already outside). One function in `pai-core` (`checks.rs`,
  `devices_outside(section, devices) -> Option<(key, device)>`), called from the manager's existing
  R3 points; the bff relays the 422 as every refusal. The page still flags such a GPU before the save.

#### (4) Slices (each test first; Linux, the Windows code type-checked)

1. **L1 `ModelInfo` in the manager** (`pai-core`, `pai-manager`): `Shape.block_count`, the shared
   resolution helper, the method on D-Bus, the pipe and `ManagerClient`. Done when, on fixture GGUFs
   (`gguf_declared`): an absolute and a relative `model` give `blockCount`, `moe`, the expert counts;
   a switched-off profile is answered; `hf-repo` gives `model-info-fetched` and opens no file (a path
   that does not exist beside it still answers fetched); a missing file and a missing shard give
   `model-info-missing`; a non-GGUF text file gives `model-info-unreadable` with none of its bytes in the
   reply; an unknown id and `*` are `no-model-profile-named`; a non-`pai` principal is refused with
   nothing read; `estimated_of` still passes its tests through the shared helper.
2. **L2 the route** (`pai-bff`): `GET /api/profiles/model/{id}/model-info`. Done when, on a fake manager:
   no session is 401 and asks nothing; the three shapes of the reply are relayed; a 404 and a session
   ending refusal map as other `As` reads; `crates/bff/README.md`'s route table names it (docs agent).
3. **L3 `n-cpu-moe` on the web** (`pai-core`, `pai-bff`): the key move and its range. Done when: the
   `pai-app` pin test of the two lists is green; a body with `n-cpu-moe = -1` or `4097` is 422 with the
   range key naming `n-cpu-moe`, nothing written; `24` saves.
3b. **L4 the device bound for `device-draft` and `mmproj-device`** (`pai-core`, `pai-manager`):
   `devices_outside` and its calls at R3's points. Done when, on an instance whose server profile has
   `Devices = ROCm0`: a model profile save with `device-draft = ROCm1` is 422
   `model-profile-device-outside` `[p, inst, device-draft, ROCm1]`, nothing written; `rocm0`, `none`
   (`mmproj-device`) and an unassigned profile save; the same refusal at an assignment, at a
   `SetServerProfile` to a `Devices = ROCm0` profile and at a save narrowing `ROCm0,ROCm1` to `ROCm0`
   when the profile names `ROCm1`; a profile already outside before the narrowing is not re-judged;
   `Devices` unset refuses nothing; `Devices = none` refuses any GPU; a `[*]` `device-draft` counts; the
   `set_model_profile` path judges the new text (an e2e through it, not only `write_file`).
4. **2g web UI** (designer's brief first): the per-GPU sliders, `n-gpu-layers` and `n-cpu-moe` sliders,
   the Draft device and Vision GPU lists. Done when: the designer's review; the fe's block arithmetic
   passes the copied `gpu_split.rs` vectors.
5. **J1 `override-tensor`** (`pai-core`, `pai-manager`, `pai-bff`): `tensors` in the reply, the key move,
   the two rules with the exemption. Done when: a 3-shard fixture lists every tensor in file order; a
   header of 20 001 tensors gives 20 000 and `tensorsTruncated`; a header of 20 000 tensors with
   long names gives the rows within 917 504 bytes and `tensorsTruncated`, and its pipe reply round-trips
   under `MAX_FRAME`; `a=CPU, b=ROCm0` is 422
   `override-tensor-space` `[2]`; `x=cpu` is `override-bad-device` `[1, cpu]`; the same values already on
   disk in the section do not block a save of another key; `x(=CPU` is not refused; `devices_outside`
   reads `-ot` devices: on `Devices = ROCm0`, `blk\.0\.=ROCm1` is `model-profile-device-outside`
   `[p, inst, override-tensor, ROCm1]` at the four points of L4, `blk\.0\.=CPU` saves.
6. **2j web UI** (designer's brief first). Done when: the designer's review.

Security: no new boundary. The read is the memory check's own (root opening a path a `pai` member
wrote, regular files only, header bounded), now answered to a `pai` member through the `As` gate; the
reply carries counts, names and sizes of a GGUF, never other bytes. The device bound makes the server
profile's `Devices` a hard limit on every GPU a model profile names, so a `pai` member cannot grow an
instance's GPU set from a model profile; it fails closed (refused before writing). The new keys are written under
`configure` and judged by the manager as every model profile key; their failures fall on the
instance's model load (facts 1, 3, 4), never on the manager.

#### (5) Decisions (user, 2026-11-21): 1, 2 and 4 the recommendation, 3 the second option

The questions as they were asked:

1. **Which `model` the sliders read**: the saved one (the page re-reads after a save that changes
   `model`); or the unsaved form value, a path the manager reads only under the models folder.
2. **An `hf-repo` profile not yet downloaded**: no layer facts (raw weights, as the GUI with no model);
   or, after M1/M2, the facts from the repo's remote header read by the bff.
3. **A draft device, vision GPU or `override-tensor` target outside the server profile's `Devices`**:
   flagged on the page, saved; or refused by the manager at the pair (as R3's memory check), which
   also makes the server profile a hard bound on every device a model profile names.
4. **`moe-cache-mib` on the web**: later, with its floor moved to `pai-core` after J1; or in 2g with the
   range rule only (the GUI's floor not applied on the web).

### Choosing the model file on the web (step 10, "Choosing the model file"): accepted (user, 2026-11-21)

The requirement (user, 2026-11-21): on the web model profile page, `hf-repo` offers the GGUF files
(quants) of the Hugging Face repo instead of free text, and `model` offers the GGUF files already in
the models folder. A second source, the director's model store (Part 2, "The model store"), must
fit beside Hugging Face later without a redesign.

**What exists.** `pai-app` `hf.rs` (module header) does the HF side for the GUI and `pai`: `parse_spec`
and `valid_repo_id` (the repo id rule, also applied to every id the remote answers), `list_repo` (two
GETs, `api/models/<repo>/refs` then `tree/<commit>?recursive=true`, grouped into `Candidate`s by a mirror
of llama.cpp's file rules, each with its shards, size, sidecars and the tree's `lfs.oid`), all through
the system `curl` with the URL and headers on stdin (`-K -`), `-L`, `--connect-timeout 20`,
`--max-time 120`, a token from `HF_TOKEN` or the HF CLI file, and the endpoint from `MODEL_ENDPOINT` /
`HF_ENDPOINT`. `pai-bff` cannot link `pai-app` (playbook). The models folder an instance uses is its
server profile's `ModelsDir`, else `paths::instance_models_dir()` (`launch::with_instance_defaults`);
the bff's account cannot read it on Linux (`/var/lib/private-ai` is 0750 `private-ai`, and a
`ModelsDir` may be anywhere), the manager (root, LocalSystem) can. The manager's model file check
already resolves a `model` value against the instance's working directory (`service.rs`
`missing_model_files`) and skips a profile carrying `hf-repo` (`FETCH_KEYS`: llama-server fetches it).

**(1) Who calls Hugging Face.**

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. The bff** (unprivileged `private-ai-bff`, sandboxed unit, already `AF_INET`; Windows `NT SERVICE\private-ai-bff`) | Move the listing half of `hf.rs` to `pai-core` | A hostile or spoofed answer is parsed by the least privileged process; the caller is already a signed-in `pai` member | **Recommended** |
| B. The manager (root, LocalSystem) | Same move, a new method | Remote JSON parsed as root; the manager gains outbound network for a read the bff can do | Rejected |
| C. A helper run as the engine's account | A transient unit or a child per call, a new seam | No gain over A: the listing writes nothing | Rejected |
| D. The browser, straight to huggingface.co | No backend code | The bff's CSP and the token would have to reach the browser; the node's outbound policy no longer applies | Rejected |

A also suits step 14: a repo's file list is the same from any machine, so the hub's bff lists it and
only the download needs the node.

**(2) The outbound boundary (SSRF).** The browser sends a repo id only, never a URL or a host. The bff
holds it to `valid_repo_id` before any network (refused `hf-repo-invalid`, nothing fetched), and builds
the two URLs from a **fixed** endpoint `https://huggingface.co/`: the bff ignores `MODEL_ENDPOINT` and
`HF_ENDPOINT` (a mirror can come later as a `bff.ini` key). curl gets, on top of today's config:
`--proto =https`, **no `-L`** (a redirect answers `hf-repo-moved`, so no request reaches a host the
answer names; curl cannot restrict a redirect's host, only its protocol: `--proto-redir`, man curl),
`--connect-timeout 10`, `--max-time 20` (under a route deadline of 25 s), and `--max-filesize 16M` (curl
8.4.0 and later also abort an answer of unknown length at the limit; older copies only refuse a
declared length: man curl, `--max-filesize`). At most two HF calls run at once in the bff (`hf-busy`,
429, beyond). Every id and path in the answer stays held to `valid_repo_id` and `safe_repo_path`, as
today. Only a session reaches the route, so an anonymous visitor cannot make the node call out.

**(3) The token: public repos first.** The first slices send none: a gated, private or missing repo
answers HF's 401, 403 or 404, all one refusal `hf-repo-unavailable` (args: the repo). The token waits
for item 13's machine-wide state, because llama-server needs the same token when it fetches an
`hf-repo` at load: the proposal then is one secret the manager owns (beside `bff.ini`'s secret, root
and `private-ai-bff` read, written only through the manager by `configure`), handed to the instances
as `HF_TOKEN` in their environment and to the bff's curl on stdin, never in an argv or a reply.

**(4) Which models folder, and who reads it.** (Superseded in part on 2026-11-21: the walk lists the one
folder of "One models folder for every engine", below; the bounds stay.) Per **node**, not per instance: a model profile belongs
to the node's pool and may be assigned to several instances. The listing is the union of the distinct
effective `ModelsDir`s of the engine's server profiles (each `ModelsDir`, else
`paths::instance_models_dir()`), each folder with the server profiles that use it. The manager walks
them (new `ModelFiles` / `ModelFilesAs`; as built in M3, 2026-11-23: `control` asked quietly, as `Devices`, since the one folder is world-readable (0755 today, 2775 after F3) and a password prompt for a listing gains nothing; the `As` form for a current `pai` member as for every `As` method):
regular files ending `.gguf` (any case), recursive to depth 6, symlinked directories not followed,
dot-folders skipped (`.hf-staging`, the store's staging), at most 5000 rows listed and 50 000 entries visited (every entry, before filtering) per walk, either
bound ending it with `truncated: true` (as built in M3, 2026-11-23), a split model shown once as its first shard (`-00001-of-0000N.gguf`, what llama.cpp
loads), a folder that cannot be read answered as that folder's `error` and not as the route's failure.
The value offered is the **absolute** path, so it names the same file on every instance whatever its
working directory. Risk accepted: a `pai` member can point a `ModelsDir` anywhere and so list `.gguf`
names there through root; a `pai` member is an administrator.

**(5) The routes** (behind a session; errors as `{key, args}`).

- `GET /api/hf/files?repo=<org>/<repo>` answers
  `{"source":"hf","repo":"o/r","revision":"<commit>","files":[{"tag":"Q4_K_M","file":"<path in repo>","shards":1,"size":<bytes>,"sha256":"<lfs oid>","sidecars":{"mmproj":"<path>"|null},"value":{"hf-repo":"o/r:Q4_K_M"},"default":true}]}`.
  `value` is what a pick writes into the profile: `hf-repo = <repo>:<tag>`, plus `hf-file` only when the
  tag alone does not name one file (llama.cpp's `--hf-file`). The store will answer the same shape with
  `"source":"store"` at its own route (Part 2), so the picker reads either.
- `GET /api/node/model-files` answers
  `{"source":"local","dirs":[{"dir":"/abs","serverProfiles":["default"],"files":[{"path":"/abs/models/o/r/x.gguf","size":<bytes>,"shards":1,"category":"models"|"mmprojs"|"mtps"|"dflashs"|null}],"truncated":false,"error":null}]}`;
  `category` is the first subfolder under the folder (the GUI's `model_scan::Category::subdir` layout),
  so the `model` picker offers `models` and uncategorized files and the same reply can later feed the
  `mmproj` and drafter pickers.
- Keys: `hf-repo-invalid` 400, `hf-repo-unavailable` 404, `hf-repo-moved` 502 (args: the repo),
  `hf-unreachable` 504 (connect, timeout), `hf-bad-answer` 502 (not JSON, too large, no branch),
  `hf-tool-missing` 503 (no curl), `hf-busy` 429; the model files route relays the manager's refusals as
  every `As` route does (`session-*`, `client-*`), and the folder errors `models-dir-unreadable` (args:
  the folder) inside the reply.

**(6) Caching.** HF listings: in the bff's memory, keyed by repo, 10 minutes, at most 64 repos, gone at
restart (a listing is pinned to a commit, so a stale one still names files that exist). Local folders:
no cache; the walk is cheap and the page asks when the picker opens.

**(7) Slices** (each test first; Linux, the Windows code type-checked).

- **M1. The listing in `pai-core`.** `parse_spec`, `valid_repo_id`, `safe_repo_path`, the `Fetch` seam,
  `Curl` and `list_repo`/`build_listing` move to `pai-core` `hf`; `pai-app` keeps search, download and
  fit and calls the moved code. A curl profile for the bff: fixed endpoint, `--proto =https`, no `-L`,
  the bounds of (2). Done when: `pai-app`'s hf tests pass unchanged; new `pai-core` tests pin the bff
  profile's config lines (no `location`, `proto`, `max-filesize`, timeouts) and that the endpoint
  ignores `HF_ENDPOINT`.
- **M2. `GET /api/hf/files`.** Done when, against a fake `Fetch`: an invalid repo is 400 and fetches
  nothing; no session is 401 and fetches nothing; 401, 403 and 404 map to `hf-repo-unavailable`; a 3xx
  to `hf-repo-moved`; a second call within 10 minutes fetches nothing; a third concurrent call is 429;
  `value` carries `hf-file` only for an ambiguous tag.
- **M3. `ModelFilesAs` and `GET /api/node/model-files`** (amended 2026-11-21 by "One models folder for
  every engine" (3): one folder). The manager lists the node's one models folder:
  `paths::instance_models_dir()` until F1 introduces `paths::models_dir()` and points the listing at the
  new folder (a one-line change in F1). Reply `{"source":"local","dir":"<abs>","files":[...],
  "truncated":bool,"error":null|{key,args}}`. Done when, on a redirected temp root: files listed by
  absolute path, shards collapsed, the depth and entry bounds hold (`truncated` true past them), a
  symlinked folder is not followed, a dot-folder is skipped, a missing or unreadable folder is a 200
  with `files` empty and its keyed `error`, a non-`pai` principal is refused, and the bff relays the
  reply.
- **M3b. The save takes the model source, from the lists only** (added 2026-11-23, brief 22E; the user chose the check of `hf-file` against the repo's listing at save, accepting that a save fails while Hugging Face is unreachable and the cache is cold; after M2
  and M3, before M4). Today the web save refuses `model`, `hf-repo` and `hf-file` (they are in
  `checks::MODEL_PROFILE_KEYS_NOT_ON_WEB`). They move to `WEB_MODEL_PROFILE_KEYS`, with one rule on the
  section as the save leaves it: exactly one source, `model`, or `hf-repo` with an optional `hf-file`
  (the page sends the chosen source and `null` for the other keys); otherwise 422
  `model-profile-bad-source` (MP1's key) `field` `model`. **Who enforces "listed only"**: the bff, at
  the save, and only for a value that differs from the one on disk (a `model` the GUI or `pai` wrote
  elsewhere stays saveable while the page leaves it alone). `model`: a member of M3's listing, asked
  through `ModelFilesAs` in the same save (the manager's folder: `paths::instance_models_dir()` until F1,
  then `paths::models_dir()`, nothing in this slice changes); not listed is 422
  `model-profile-model-not-listed` `[path]` `field` `model`. `hf-repo`: `valid_repo_id` (422
  `hf-repo-invalid` `field` `hf-repo`); `hf-file`: a file of M2's listing of that repo, read through
  M2's cache (warm: the picker just listed it), not listed 422 `model-profile-file-not-listed` `[repo,
  file]` `field` `hf-file`; HF failing at that moment answers M2's keys (`hf-repo-unavailable`,
  `hf-repo-moved`, a 503 for no answer) with nothing written. The manager adds nothing: a `pai` member
  may already name any path through the GUI or `pai` (the same trust), and the manager has no network
  for the HF half; its existing checks still run (text, comment markers, the model-file check for a
  profile in effect). Done when: a save of a listed `model` writes it and drops `hf-repo`/`hf-file`;
  of a listed repo and file writes both and drops `model`; an unlisted path, an unlisted file, an
  invalid repo, both sources, and `hf-file` without `hf-repo` are each refused with the key and field
  above and no write asked; an unchanged out-of-folder `model` saves with another key changed; HF
  down at a changed `hf-file` refuses with nothing written; the key lists' pin test moves the three
  keys; each against the fake manager and a fake `Fetch`.
- **M4. The pickers.** The designer's brief, then the fe: `hf-repo` and `model` as pickers fed by M2 and
  M3 (whether free text stays is the brief's). Done when: the designer's review.
- **Later**: the token (item 13's state), the store source (Part 2), `mmproj` and drafter pickers from
  M3's `category`.

**(8) Decisions (user, 2026-11-21): the recommended answer to each.** 1. Public repos only now; the
token with item 13's machine-wide state. 2. Every models folder of the node's server profiles, grouped
by folder (superseded on 2026-11-21 by "One models folder for every engine": M3 lists that one folder). 3. Hugging Face only, a fixed endpoint, no mirror. The questions as they were asked:

1. **The token**: public repos only now, the token with item 13's machine-wide state (one secret the
   manager owns, given to the instances and the bff); or the token in this step already.
2. **Which folders**: every models folder of the node's server profiles, grouped by folder; or only the
   folder of the `default` instance's server profile.
3. **A mirror**: Hugging Face only (fixed endpoint); or an endpoint the administrator sets in `bff.ini`.

**(9) M5: the Hugging Face token by authentication (proposed, 2026-11-21).** The user's words
(translated): "Of course the token towards Hugging Face will then be handled through authentication."

What Hugging Face offers (huggingface.co/docs/hub/oauth and huggingface.co/.well-known/openid-configuration,
read 2026-11-21): the authorization code flow with PKCE (`S256` only), the **device code** flow
(`/oauth/device`, then `/oauth/token`), the `refresh_token` grant, and public apps with no client secret.
A `redirect_uri` must match a registered one exactly, except an `http` loopback URI, where any port is
accepted. The scopes this needs: `gated-repos` (public gated repos the user was granted) and, only
for private repos, `read-repos` (the user's own, and those of the organizations picked at consent). The
metadata names **no revocation endpoint**, and the page states no access token lifetime for these
flows (the token exchange's is 8 hours, with no refresh token).

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. "Sign in with Hugging Face" by the device code flow**, from the web UI: the bff asks `/oauth/device` with a public client id, the page shows the code and the link, the administrator approves on huggingface.co from any device, the bff polls `/oauth/token` | Two HF calls and a poll in the bff; one public OAuth app (no secret) | No redirect URI, so it works on any `[Bff] Listen` address, plain HTTP on a LAN included; a code phished to another person grants that person's repos, not ours (the page names the app and the scopes) | **Recommended** |
| B. Authorization code + PKCE, callback `/api/hf/oauth/callback` | A callback route, `state` bound to the session | The redirect URI must match exactly: a LAN address needs an OAuth app registered per installation; only `http://localhost` works with one shared app | Only if A is refused |
| C. An administrator pastes a token (a fine-grained read token) | One form | No refresh, no consent screen, the token's scope is whatever was pasted | Fallback, not "authentication" |

- **Where it lives**: one secret per node, item 13's machine-wide state, written only by the manager
  (`SetHfTokenAs`, `configure`, a current `pai` member): `[Hf]` `AccessToken`, `RefreshToken`,
  `ExpiresAt`, `Scopes`, `Account` (from `/api/whoami-v2`), in `/etc/private-ai/hf.ini` mode 0640
  `root:private-ai-bff` (Windows: `%ProgramData%\private-ai\config\hf.ini`, SYSTEM and Administrators
  full, the bff's service SID read). A new file, so `bff.ini`'s mode and readers do not change.
- **Who uses it**: the bff's listing (M2, on curl's stdin as today in `hf.rs`) and the instances'
  downloads of an `hf-repo` (llama-server reads `HF_TOKEN`): the template unit passes the file with
  `LoadCredential=` and `run-instance` sets `HF_TOKEN` for llama-server from it (Windows: the manager
  grants the instance's service SID read at create). Never in an argv, a log, a profile or bench.db.
- **Never to the browser**: `GET /api/hf/account` answers `{connected, account, scopes, expiresAt,
  state: ok|expired}` only; `POST /api/hf/account` starts the device flow (`{userCode, verificationUri,
  expiresIn}`) and its poll; `DELETE /api/hf/account` disconnects.
- **Refresh and revocation**: the bff refreshes shortly before `ExpiresAt` and once on a 401, writing
  the new pair through the manager; a failed refresh leaves `state: expired` (`hf-token-expired`) and
  the listing falls back to public repos. A disconnect deletes the file through the manager; since HF
  lists no revocation endpoint, the page tells the administrator to remove the app in their Hugging
  Face settings too. A running instance keeps the token it started with until its next start (known
  limit: a download after expiry is a 401 until a restart).
- **Slices**: M5a the manager's `hf.ini` and `SetHfTokenAs`/`ClearHfTokenAs` (modes, `pai` only);
  M5b the bff's device flow, refresh and `/api/hf/account` against a fake HF; M5c `LoadCredential=` and
  `HF_TOKEN` in `run-instance`; M5d the designer's brief and the page. Each done when its tests pin the
  above, the token absent from every reply and argv.
- **Questions for the user** (recommendation first): 1. **Flow**: device code; or authorization code
  with PKCE (one OAuth app per installation for a LAN address); or a pasted token. 2. **Scopes**:
  `gated-repos` only; or with `read-repos` for private repos. 3. **The OAuth app**: one public app the
  project registers, its client id the default in `bff.ini` and overridable; or each administrator
  registers their own. 4. **Whose account**: one Hugging Face account per node, shared by every
  administrator; or one per administrator (needs per-user state, which item 13 rules out).

### One models folder for every engine (user, 2026-11-21): accepted (user, 2026-11-21), `/var/lib/private-ai/models`

The user's words (2026-11-21, translated): one models folder per node shared by every engine
(llama.cpp now, Strata later), and `[Server] ModelsDir` leaves the server profiles ("unica e basta").
No migration of existing folders (pre-release). `StateDir` is untouched.

**What exists.** A model folder is a server profile key: `ServerConfig.models_dir`, rendered by
`render_for`, refused for a `;` in `checks.rs` (line 412), served by the bff's key list
(`crates/bff/src/server.rs` 289), shown in the GUI's Server tab and set by `pai`. An unset one becomes
the engine's folder under the manager (`launch::with_instance_defaults`, from
`paths::instance_models_dir()`: Linux `/var/lib/llama.cpp/models`, Windows
`%ProgramData%\llama.cpp\models`), else `server_cfg::default_models_dir()` (`~/.llama.cpp`, macOS and
a tree build). It reaches llama-server only as `LLAMA_CACHE` (`launch.rs` 200), so it is also where an
`hf-repo` download lands. The engine's packages create the folder: Linux `llama.cpp.conf` tmpfiles
`d /var/lib/llama.cpp/models 0755 private-ai private-ai` (`system_files.rs` 115), Windows
`llama.cpp.wxs` `EngineDataModels` (SYSTEM and Administrators full, ALL SERVICES `S-1-5-80-0` modify,
Users read). The shipped `server@default.ini` sets `ModelsDir = /var/lib/llama.cpp/models` (Linux).
A GUI download runs as the desktop user into the folder, which on Linux that user cannot write today.

**(1) The folder, owned by the framework.** One function, `pai_core::paths::models_dir()`, replacing
`instance_models_dir()` and `default_models_dir()`; `PRIVATE_AI_DATA_ROOT` redirects it to
`<root>/models` for the tests.

| OS | Path | Created by | Owner and rights |
| --- | --- | --- | --- |
| Linux | `/var/lib/private-ai/models` (user, 2026-11-21) | `private-ai-framework-core`, a tmpfiles.d entry `private-ai.conf` applied by its postinst as the engine's is today | `d /var/lib/private-ai 0751 private-ai private-ai -`, `d /var/lib/private-ai/models 2775 private-ai pai -` and `a+ /var/lib/private-ai/models - - - - d:group:pai:rwx` (tmpfiles.d(5): `d` adjusts an existing directory's mode, `a+` adds ACL entries): the instances' account `private-ai` writes (an `hf-repo` download), members of `pai` write (a GUI or `pai` download as themselves), setgid and the default ACL keep what either creates group `pai` and group writable, everyone reads as today's folder does |
| Windows | `%ProgramData%\private-ai\models` | the framework's MSI | SYSTEM and Administrators full, ALL SERVICES (`S-1-5-80-0`, every instance's virtual account) modify, the local group `pai` modify, Users read; removed only when empty |
| macOS | per user, `~/.llama.cpp` (today's default, unchanged) | nothing (`pai-config` creates it) | the user |

**Opening the service user's home.** `/var/lib/private-ai` is `private-ai`'s home (sysusers entry) and
today a `StateDirectory=` of the instance template with `StateDirectoryMode=0750`, which systemd
re-applies to every listed directory at each instance start (systemd.exec(5), "The innermost specified
directories will have their access mode adjusted"): a 0751 set by tmpfiles would be undone at the next
start. So the template lists only the data root, `StateDirectory=private-ai/.local/share/private-ai`
(still 0750, still re-applied), and the home's mode comes from the tmpfiles line above, 0751. The
models folder must never be a `StateDirectory=` either: systemd would chown it back to
`private-ai:private-ai` (same page, the recursive ownership change), dropping the group `pai`.

What 0751 exposes and what stays private:

- **Exposed**: any local user may *traverse* the home, not list it; an entry is reachable only by a
  name already known, and then only as far as that entry's own mode allows. The models folder is
  meant to be read (as `/var/lib/llama.cpp/models` 0755 is today). Other entries llama.cpp or the
  framework create in the home with the default umask (0755 folders, 0644 files) become readable to
  anyone who guesses the name; today that is an earlier `LLAMA_CACHE` default `~/.llama.cpp`, model
  files only. Any future file holding a secret must not sit loose in the home.
- **Private**: the data root `.local/share/private-ai/` (0750, re-applied at each start), with every
  instance's state folder and conversation snapshots under `state/<id>`; the home's listing; nothing
  else of the manager's lives there (its files are in `/etc/llama.cpp`, `/etc/private-ai`, root only).
  The parents `.local` and `.local/share` are created by systemd for the data root with its default
  mode (unverified: the slice checks it on a real start; 0755 there shows only that the data root
  exists).

The default ACL replaces a unit-wide `UMask=0002`: only what is created in the models folder becomes
group writable, so a `pai` member can delete what an instance downloaded while the instance's other
files keep today's modes. A later engine runs as `private-ai` too, or
reads by the world-readable mode. The engines' folders (`/var/lib/llama.cpp/models`,
`%ProgramData%\llama.cpp\models`) are no longer created; an existing one is left alone (no migration).

**(2) `ModelsDir` leaves the server profiles.**

| Option | What a file still holding `ModelsDir` does | Verdict |
| --- | --- | --- |
| **A. Dropped from `ServerConfig`**: read past, never rendered, so the line vanishes at the next save | Loads; the key changes nothing; gone after a save | **Recommended**: no save is blocked by an old line |
| B. Refused by the manager on a server file write | Every save of that profile fails until the line is removed by hand | Rejected |
| C. Kept and ignored | The form shows a key that does nothing | Rejected |

Per component with A: `pai-core` drops the field, `models_dir_or_default`, the `ModelsDir` entry of
`checks.rs`' `;` rule and the models half of `with_instance_defaults`; `launch` sets `LLAMA_CACHE` to
`paths::models_dir()` always; `server_control::dirs_left_unset` keeps only `StateDir`. The bff drops
`ModelsDir` from its key list (a body carrying it is filtered like any unknown key; the read no longer
answers it). `pai` refuses `server set ModelsDir` as an unknown key. The GUI's Server tab loses the
field; the Models tab's scans, Browse, downloads and the hardware window read `paths::models_dir()`
(`server_cfg::models_dir` goes). The import of an earlier `server.ini` drops the line by rendering.
The shipped defaults lose `ModelsDir`. A model profile's `model` stays a path anywhere (absolute or
relative to the instance's directory): only where the framework puts and lists files is fixed.

**(3) The picker's M3 amended.** `ModelFilesAs` lists the one models folder alone
(`paths::instance_models_dir()` until F1 introduces `paths::models_dir()`; slice order of 2026-11-23: M3
before F1), which supersedes
decision 2 of (8): `{"source":"local","dir":"/abs","files":[...],"truncated":false,"error":null}`, same
file rows. The manager keeps the walk (the bff could now read the folder itself, but the hub of step 14
lists a node's folder through the manager, so one path serves both).

**(4) Slices** (each test first; before M3).

- **F1. `pai-core`.** `paths::models_dir()` (introduced here, replacing `paths::instance_models_dir()`,
  which M3's listing uses until then), `ServerConfig` without `models_dir`, `LLAMA_CACHE` from it, and
  `ModelFilesAs` pointed at it. F1, F2 and F3 ship in one build and one deploy. Done when: a server file holding `ModelsDir` loads and its render has no `ModelsDir` line;
  `LLAMA_CACHE` equals `paths::models_dir()` whatever the file says; the redirected root gives
  `<root>/models`; `checks.rs` no longer names `ModelsDir`.
- **F2. The clients.** The bff, `pai`, the GUI and the manager callers. Done when: the bff's server read
  has no `ModelsDir` and a body with it saves a file without it; `pai server set ModelsDir x` is refused
  as unknown; the GUI's Server tab has no models field and the Models tab scans `paths::models_dir()`.
- **F3. Packaging.** `-core`'s tmpfiles entry (the home 0751, the models folder 2775 and its default
  ACL) and postinst, the instance template's `StateDirectory=` reduced to the data root, the engine's
  models line removed, the shipped defaults without `ModelsDir`; the framework MSI's folder and SDDL,
  `llama.cpp.wxs` without `models\`. Done when: the package content tests show the three tmpfiles
  lines, a `StateDirectory=` that no longer lists `private-ai` alone, and no `/var/lib/llama.cpp/models`;
  on the user's install, `stat` shows the home 0751 after an instance start and the models folder
  `drwxrwsr-x private-ai pai`; the MSI text checks show the
  new folder's SDDL with `pai` and `S-1-5-80-0` modify.
- **M3** as amended in (3) is built before F1 (order of 2026-11-23); F1 only repoints its folder.

**(5) Decisions (user, 2026-11-21).** 1. Linux path `/var/lib/private-ai/models`, the service user's
home opened to 0751 as above (the architect had recommended `/var/lib/private-ai-models`). 2. The
instances and the members of `pai` write. 3. macOS keeps the per-user `~/.llama.cpp`.

### Tags on model profiles (user, 2026-11-22, `PLAN.md` "Tags on model profiles"): accepted (user, 2026-11-22), a side record

The requirement: a model profile (one section of `models.ini` or `models.ini.disabled`) carries tags,
shown as chips with suggestions as instance tags are (fe brief 15), for (1) labels and a filter on the
web Models page, (2) routing: the router (`PLAN.md` step 10 item 14) and the director (step 14) pick a
model by tag, (3) the model list the API offers to clients. After slice 17, before R4 and R5.

**What exists (checked 2026-11-22).**
- llama.cpp v1.0.0 (the tagged clone, `build/linux/llama.cpp`) has a native option: `--tags STRING`,
  "informational, not used for routing", env `LLAMA_ARG_TAGS` (`common/arg.cpp` 3006-3017;
  `tools/server/README.md` line 192), so `tags` is a known preset key (`common/preset.cpp`
  `get_map_key_opt`, 251-262; an unknown key throws, 325-332). The router lists it as `"tags"` in its
  model list (`server-models.cpp` 471-479, 2202), and a reload unloads a running model whose `tags`
  changed (`preset_options_for_compare`, 884-890, 949-958). **The user chose not to use it** (below).
- The manager hands llama-server only the per-instance render `models@<id>.ini`
  (`pai_core::assign::render`); macOS's configurator passes the user's `models.ini` itself
  (`crates/app/src/runstate.rs` 490).
- The manager's own records beside the pool: `assignments.ini` and `instances.ini`, readable by any
  client and written only by the manager (`service.rs` `read_only_file`, ~2787); a missing record reads
  as empty (`read_record`). An assignment key naming an id the pool does not hold renders nothing, and
  nothing prunes it today.
- Instance and server profile tags: one rule `checks::tags_rule` (at most 8, each 1..32 characters of
  `a-z0-9-`, no duplicate; read by `tags_list`), one refusal key `server-bad-tags`; the instance's own
  written by `SetInstanceTags` / `SetInstanceTagsAs` (`service.rs` `set_instance_tags`: `configure`,
  the `writing` lock, the rule before any read, one action record) and `PUT /api/instances/{id}/tags`,
  each chip saved at once (brief 15, 1).
- Model profiles have no create, rename or delete method in the manager: the GUI renames and deletes
  by writing the pool files whole (`crates/app/src/model_profiles.rs` `rename_with`, `delete`, through
  the manager's `WriteFile`); the web has no create or delete yet. A missing model profile answers
  `no-model-profile-named` (`crates/bff/src/model_profiles.rs` `not_found`, `SetModelProfile`).

**(1) Where the tags live: decided, a side record.** Options weighed: (a) llama.cpp's `tags` key (the
architect's recommendation: no new method, the tags travel with the section); (b) a framework key
stripped at render; (c) a comment line; (d) a side record. **The user chose (d)** (2026-11-22). What it
gains: llama-server never sees a tag, so a tag change is never a preset change (no restart, no unload
at a reload), and `models.ini` keeps llama.cpp keys only. What it costs: a new manager method and
route, and keeping the record in step with the pool's ids (below).

- **The record**: `model-tags.ini` in the engine's config directory (`paths::engine_config_dir`:
  `/etc/llama.cpp/`, `%ProgramData%\llama.cpp\config\`), beside `assignments.ini`; tags are per engine
  because the profiles are. One section per model profile id, as the pool spells it, with one key:
  `[qwen-27b]` / `Tags = coding,fast`. No `[*]` section (`*` is not a profile). A section with no tags
  is removed, not left empty. No package ships it: a missing record is no tags, and the manager's first
  write creates it with the mode and group of every served file (`write_served_file`); on Windows it
  inherits the config folder's ACL (users have no access). It joins `read_only_file`: any client reads
  it through `ReadFile` / `ReadFileAs`, no client writes it. Removed with the engine's tree only when
  that uninstall's checkbox is ticked, as the other records.
- **Matching**: a section matches a profile id in any letter case (the render's rule: one id is one
  profile), the first section wins.
- **The pool is the source of ids.** A section whose id is in neither pool file is an orphan: every
  reader ignores it, and the manager prunes orphans after every write of a pool file (`WriteFile` or
  `WriteFileIf` of `models.ini` or `models.ini.disabled`; `SetModelProfile` writes only
  `assignments.ini` and does not prune), under the same
  `writing` lock, before releasing it. So a delete (the GUI's, a future web delete) drops the tags, and
  a profile created later under the same id starts with none. A hand edit of the pool by root leaves an
  orphan until the next pool write (read as nothing meanwhile; a hand re-create in between inherits it:
  accepted, a label only).
- **Rename**: a pool write cannot tell a rename from a delete plus a create, so the renamer carries the
  tags: the GUI's `rename_with`, through the manager backend, reads the old id's tags first, writes the
  pool (which prunes the old section), then calls `SetModelProfileTags(new, tags)`. A failure between
  the two loses the tags of that profile, nothing else (a label; the GUI shows the error). The
  alternative, a `RenameModelProfile` method doing both under one lock, is not worth a new method
  while the GUI is the only renamer; a web rename, when one is designed, should take it.
- **Switch on/off** moves the section between the pool files; the id stays in the pool, so the tags
  stay. Tags of a switched-off profile are shown, as its other values are.

**(2) Grammar and checks.** `checks::tags_rule` unchanged, refusal `server-bad-tags` (args as today,
`field` `tags`), applied to the list before the record is read, so a refused list writes nothing. The
record is written by the manager only, as `Tags = a,b` (no spaces); readers parse it with `tags_list`.
A hand-written bad list in the record is shown as read (it is root's file), and replaced whole at the
next write. The charset excludes `;`, `#`, `,` and spaces, so no comment-marker or injection case
reaches a value; the record never reaches llama-server's argv or a shell.

**(3) Manager, `pai-core`, bff.**
- `pai-core` (`model_tags.rs`, pure, texts in and out, shared by manager, bff and the router):
  `tags_of(record, id) -> Vec<String>`, `set(record, id, tags) -> String` (the section's `Tags`
  replaced, or the section removed for `[]`, or appended with a newline first when the text does not
  end with one), `prune(record, enabled, disabled) -> String` (orphans dropped, other bytes kept).
- **`SetModelProfileTags(profile, tags)`** `(sas)`, and `SetModelProfileTagsAs(principal, profile,
  tags)` for the bff (the `As` rule: answered only to the bff's account for a current member of
  `pai`). Guarded as `set_instance_tags`: `configure`, `writing` lock, one action record. Refusals:
  `server-bad-tags`; `no-model-profile-named` `[profile]` when neither pool file holds the id (any
  letter case; `*` is never a profile id); `Io`. Writes the record only: no render, no pool file
  touched. On D-Bus, the pipe and `ManagerClient` (`set_model_profile_tags`, `acting_for`).
- **Pool writes prune** the record as in (1); a failed prune write fails the call with `Io` after the
  pool write landed (the orphan is harmless and the next write retries).
- `GET /api/profiles/model`: each row gains `tags: [string]` from the record (one more `ReadFileAs`
  over the same connect; a missing record is `[]`; a failed read is the list's 502 as for the other
  files). `GET /api/profiles/model/{id}`: the document gains `tags: [string]` the same way.
- **`PUT /api/profiles/model/{id}/tags`** `{"tags": [string]}`: 204; 404 `no-model-profile-named`;
  422 `server-bad-tags` `field` `tags`; 400 `tags-malformed` (the route's body, as the instance route);
  401, 403 (ending the session), 503, 502 as every manager-backed route. No revision: the record is not
  the section, so a chip save leaves the page's section revision valid and a pending form edit is not
  refused as changed elsewhere. `tags` stays out of `WEB_MODEL_PROFILE_KEYS`.
- **Restart: none** (user, 2026-11-22). The record touches no `models.ini` and no render, so
  llama-server sees nothing and the reload caveat of the native key does not apply.

**(4) The GUI and macOS.** Under the manager (Linux, Windows) the GUI shows no tags in this step; its
saves rewrite only the section, so they never touch the record; its delete prunes through the
manager's pool write; its rename carries the tags as in (1). `pai` gets no tags command now. macOS has
no manager, no bff and no router, and keeps its files per user: it has no model profile tags in this
step (no record, nothing read). If a macOS GUI control comes later, a per-user `model-tags.ini` beside
`models.ini` with the same `pai-core` functions is the shape; not designed now.

**(5) Suggestions: one vocabulary** (user, 2026-11-22). The instance, server profile and model profile
tag fields all suggest the union of the three, from reads the fe already makes plus the Models list's
`tags`; no backend. The designer extends brief 15's rule.

**(6) The router and the client list** (built with item 14; recorded so it starts from here).
- llama-server's own `/v1/models` carries no framework tags (its `"tags"` field stays empty: no
  `tags` key is written). The gateway's `GET /v1/models` builds its own list: each model profile at
  least one instance serves (`pai_core::assign::rows`, effective) once, `{id, object: "model",
  created, owned_by, tags}`, `tags` from the record (`model_tags::tags_of`), the field name llama-server
  uses. A client talking to an instance's port directly sees no tags: documented. Tags are visible to
  every API client the gateway admits; documented with the route.
- **Routing by tag: `"model": "tag:<tag>"`** (user, 2026-11-22). A profile id never holds `:`
  (`checks::valid_model_profile_id`), so the prefix cannot shadow an id. Among several served profiles
  with the tag, item 14 sets the order (proposed: one already loaded on a running instance, then the
  first by id); none is the unknown-model answer.
- The director (step 14) reads and copies the record's sections with the profiles it holds and pushes
  (a profile copied without its section arrives untagged); its design takes this with it.

**Trust boundaries.** Unchanged in kind: tags are written only by a `pai` member through the manager
(`configure`; the bff as the signed-in user through the `As` method), judged by the manager, which
fails closed (a refused list writes nothing; an unknown id writes nothing). Readers are signed-in
administrators (the Models page), any manager client (the record is readable as `assignments.ini`
is), and later API clients through the gateway, who learn the tags and nothing else of the profile.

**(7) Slices (each test first; Linux, the Windows code type-checked).**
- **MT1. The record in `pai-core` and the manager.** `model_tags.rs`; `SetModelProfileTags` and its
  `As` on D-Bus, the pipe and `ManagerClient`; `model-tags.ini` in `read_only_file`; the prune after
  every pool write. Done when: `tags_of` matches any letter case and ignores orphans; `set` with `[]`
  removes the section and keeps every other byte; `prune` drops only ids in neither pool text; an e2e
  sets `["coding","fast"]` and reads `Tags = coding,fast` under `[<id>]`, through D-Bus and the pipe;
  nine tags or `Bad Tag` is `server-bad-tags` and the record is unchanged; an id in neither pool file,
  and `*`, is `no-model-profile-named`; a profile in `models.ini.disabled` is accepted; `SetModelProfileTagsAs`
  for a non-member is refused and for a non-bff caller too; a client `WriteFile("model-tags.ini")` is
  a bad name; a pool write that deletes a profile leaves the record without its section, a write that
  keeps it leaves the record byte for byte; neither `models.ini` nor any `models@<id>.ini` changes on a
  tags call.
- **MT2. The bff and the GUI's rename.** Rows and the page document carry `tags`; `PUT
  /api/profiles/model/{id}/tags`. Done when: `crates/bff/tests` show a row's `tags` from the stub's
  record, `[]` with no record; 204 on a save and the stub received the list; 404, 422 `field` `tags`,
  400 `tags-malformed`, 403 ending the session; the page's section revision is the same before and
  after a tags save. In `pai-app`, a rename through the manager backend leaves the new id with the old
  id's tags and the old section gone (a test against the backend seam). `fe/briefs/api-contract.md`
  gains the route and fields (docs agent).
- **MT3. The web UI** (designer's brief first): chips on the model profile page saved at once, labels
  and a tag filter on the Models page, the shared suggestions of (5). Done when: the brief's criteria
  pass and the UX check is green.
- Item 14 takes (6); step 14 takes the director's line of (6).

**(8) Decisions (user, 2026-11-22).** 1. A side record, not llama.cpp's `tags` key (the architect had
recommended the key). 2. A tags-only save asks no restart. 3. One shared vocabulary for instance,
server profile and model profile tags. 4. Routing by `"model": "tag:<tag>"`.

### About and node info (user, 2026-11-22, `PLAN.md` "About and node info"): accepted (user, 2026-11-22), the manager reads the node

The requirement: the web About page shows only the framework's version and the operating system, with
its version, of the machine the web service runs on (the node's bff today, the director's in step 14).
The host name and the engine and GPU runtime versions it shows today move to the node's info, which also
gains every engine's version, the node's OS and version, and the GPU driver versions: the kernel
driver's and the Vulkan driver's, per GPU.

**What exists (checked 2026-11-22).**
- `GET /api/version` answers `{"version"}`, `GET /api/node` answers `{"name", "api", "version",
  "engines"}` (`crates/bff/src/node.rs` `document`); both are session-gated, ask the manager nothing
  and leave the idle deadline alone (`Sessions::peek`, which still hands back the identity), and the
  shell polls them (`crates/bff/README.md` "The node"). `engines` already lists every engine that ships
  a versions file (`pai_core::versions::installed`), so Strata (13c) appears when its package writes
  `versions/strata`: no new field is needed for "every engine".
- A polled read that asks the manager already exists: `GET /api/instances/{id}/boot` peeks the session
  and asks under `MANAGER_TIMEOUT` (5 s), 503 `client-unavailable` on no answer
  (`crates/bff/src/control.rs` 146-162).
- The engine probe gives no driver version: `--list-devices` prints name, description and memory only
  (llama.cpp v1.0.0 `common/arg.cpp` 1149-1175), `ggml_backend_dev_props` has no driver field
  (`ggml/include/ggml-backend.h` 167-184), and Vulkan's `driverName` reaches only a debug log line
  (`ggml-vulkan.cpp` 5455). The manager's device probe (`ProbedGpus`, `service.rs` 500-635) runs
  `llama-server --list-devices` as the manager itself (root, LocalSystem), bounded by
  `GPU_PROBE_TIMEOUT` (30 s), through `pai_core::proc::run_hidden_probe_within`, and `Devices` is gated
  on `control` asked quietly, the `As` form for a current `pai` member (`service.rs` 1820-1832): so the
  engine's Vulkan ICDs are already loaded by a root child of the manager today.
- The OS is read today only by `pai-app` (`crates/app/src/bench/host.rs` 166). The host's RAM and CPU
  readers are `pai-core` `host.rs`, called by the manager (`devices`).
- `ash` 0.38.0 (MIT or Apache-2.0) is already in `Cargo.lock` through `wgpu-hal` (the GUI's Slint):
  with its `loaded` feature, `Entry::load` opens the platform's Vulkan loader at run time and returns
  `LoadingError` when there is none (docs.rs, ash 0.38 `Entry`), so nothing links against libvulkan.

**Facts and where the manager reads each.**

| Fact | Linux | Windows |
| --- | --- | --- |
| OS name, version | `/etc/os-release`, else `/usr/lib/os-release`, never both: `NAME`, `VERSION_ID` (os-release(5)) | `RtlGetVersion` (ntdll): `"Windows"`, `<major>.<minor>.<build>` (Microsoft Learn, RtlGetVersion: documented as the kernel routine, ntdll listed as an export; chosen over `GetVersionEx`, whose answer depends on the caller's manifest) |
| Kernel | `uname(2)` release | the same `<major>.<minor>.<build>` |
| Engines and runtimes | unchanged: the packages' versions files, read by the bff | unchanged |
| Kernel driver, one row per GPU | each `/sys/bus/pci/devices/*` whose `class` is `0x03xxxx` (VGA and 3D controllers, so a compute card, or NVIDIA without `nvidia-drm`, is listed): the bus id, `vendor`, the bound driver (the `driver` link's name) and `/sys/module/<driver>/version`, present only when the module sets `MODULE_VERSION` (kernel ABI `sysfs-module`): ROCm's DKMS amdgpu (`7.1.9.31600000` on this machine) and NVIDIA's `nvidia` (`kernel-open/nvidia/nv.c` 126) do, an in-tree amdgpu, i915 or xe does not, so `version` is `null` and the fe shows the kernel | each present device of the display class, through `CM_Get_DevNode_PropertyW`: `DEVPKEY_Device_DriverVersion` (the INF `DriverVer`, such as `32.0.15.6094`; Microsoft Learn, DEVPKEY_Device_DriverVersion), `DEVPKEY_Device_DriverProvider`, `DEVPKEY_NAME`, and the bus id from `DEVPKEY_Device_BusNumber` and `DEVPKEY_Device_Address` (domain taken as 0; unverified on hardware) |
| Vulkan driver, per GPU | a child process `pai-manager vulkan-probe` (below): per physical device not of type CPU, `vkGetPhysicalDeviceProperties2` with `VkPhysicalDeviceDriverProperties` (`driverName`, `driverInfo`, `driverID`; core in Vulkan 1.2, else `VK_KHR_driver_properties`) and `VkPhysicalDevicePCIBusInfoPropertiesEXT` (`VK_EXT_pci_bus_info`) for the bus id (docs.vulkan.org refpages of both) | the same child; the loader `vulkan-1.dll` comes with the GPU driver |

**Recorded at the AN1 review (2026-11-22).**
- **A row's `driver` and `version` are each OS's own driver identity, not one meaning.** Linux: the
  kernel module bound (`amdgpu`) and its `MODULE_VERSION`. Windows: `DEVPKEY_Device_DriverProvider`
  (the INF's provider, a company name such as `Advanced Micro Devices, Inc.`) and
  `DEVPKEY_Device_DriverVersion` (the INF `DriverVer`). Accepted: Windows exposes no kernel module
  name a user would recognise, the service name (`DEVPKEY_Device_Service`, `amdkmdag`) says less than
  the provider, and `DriverVer` is the version the vendor's installer and Device Manager show
  (Microsoft Learn, DEVPKEY_Device_DriverProvider, DEVPKEY_Device_DriverVersion). The fe labels the
  pair "driver" and "version", never "kernel module". `GpuDriver`'s field doc says "kernel driver
  bound", which is the Linux reading only (`crates/core/src/host.rs` the struct): a doc fix, no
  behaviour change.
- **The Windows bus id assumes domain 0** and splits `DEVPKEY_Device_Address` as device in the high
  word, function in the low word, the PCI bus driver's encoding (Microsoft Learn, DEVPKEY_Device_Address).
  Unverified on hardware: a host with a second PCI segment would show a wrong id and its Vulkan row
  would not join, which degrades to an unmatched row (the join's fallback), never a wrong driver on
  another GPU unless two segments carry the same bus, device and function. Check on the first Windows
  machine with a GPU (step 1's run).
- **The sysfs `driver` link** points at `../../../bus/pci/drivers/<name>`, not at `module/`
  (kernel ABI `sysfs-bus-pci`); the reader takes the link's last component only, so it is right. That
  driver name is taken as the module name for `/sys/module/<name>/version`, which holds for amdgpu
  and nvidia; a driver whose module is named otherwise gives `version: null`, a fact, never a wrong
  version. The doc comment and the test fixture in `host.rs` that write `module/<name>` are a doc fix.

**The Vulkan reader.**
- **Options.** (a) **A hidden subcommand `pai-manager vulkan-probe`, run as a child** with
  `run_hidden_probe_within` under an 8 s bound, printing one JSON document on stdout that the manager
  parses with caps (recommended). The child links `ash` (`loaded`): an ICD that hangs or crashes kills
  the child, not the manager, and the bound frees the caller. (b) **The same code in the manager's
  process**: no spawn, but every ICD (vendor code) is loaded into the long-lived root service, where a
  hang or a crash takes the manager down and its memory stays mapped. (c) **The engine's probe**:
  llama-server prints no driver field; a patch to `--list-devices` is ours to carry on every release.
  (d) **`vulkaninfo`**: not shipped by our packages (vulkan-tools). (a) has the same exposure as today:
  the device probe already loads the same ICDs as root; dropping the child to the `private-ai` user
  (which has `render` and `video`) is a possible later hardening, not needed for parity.
- **Fallbacks, each a fact, never an error:** no loader (`Entry::load` fails) or no ICD
  (`vkCreateInstance` answers `VK_ERROR_INCOMPATIBLE_DRIVER`) gives `"vulkan": "none"` in the reply
  and `vulkan: null` on every row; a child that times out, crashes or prints what does not parse gives
  `"vulkan": "failed"`; a device below 1.2 without `VK_KHR_driver_properties` gives a row's `vulkan` with
  `driver` and `info` `null`; a device without `VK_EXT_pci_bus_info` cannot be joined and becomes its own
  row (below). `driverVersion` is not sent: its encoding is the vendor's (`ggml-vulkan.cpp` 1874-1876
  decodes AMD's apart), while `driverInfo` already carries the readable version (Mesa's, NVIDIA's).
- **Join.** A Vulkan device whose bus id matches a kernel row fills that row's `vulkan` (and its `name`
  from `deviceName` when the row has none, as on Linux); an unmatched one is appended as a row with
  `driver` and `version` `null`. The rule lives in the manager (`pai-core`), so the director gets the
  same rows.
- **The child's document** (accepted at AN2's review, 2026-11-22; `pai_core::vulkan`): one JSON line on
  stdout, `{"vulkan": "ok"|"none"|"failed", "gpus": [{"pci", "name", "type", "driver", "info"}]}`;
  `pci` in sysfs' form (`0000:03:00.0`, lower-case hex), `type` Vulkan's device type, a key left out
  or a text over 256 bytes is `null`, a document over 4 MiB, not JSON, or with another status word
  is `failed`. A node whose only devices are of type `cpu` (llvmpipe) answers `none`: no GPU has a
  Vulkan driver, which is what the page needs to say. `VK_EXT_pci_bus_info` and
  `VK_KHR_driver_properties` are **device** extensions (Vulkan refpages, "Extension Type"), so the
  child asks `vkEnumerateDeviceExtensionProperties` per device (driver properties are also core from a
  device `apiVersion` of 1.2) before chaining either struct, and creates its instance with the
  `apiVersion` (1.1 or more) that lets it call the core `vkGetPhysicalDeviceProperties2`. The child
  runs as the manager's account (root, LocalSystem), inherits the manager's environment and loads
  only the platform's loader and the ICDs it names.
- **Cost and cache.** A child spawn plus loader and ICD start, well under the bound (a guess, not
  measured: the first slice's test run on this machine gives the figure for the README). The Vulkan
  answer (success, `none` or `failed`) is cached for 10 minutes, so a polled page spawns at most one
  child per 10 minutes and a userspace driver upgrade shows within that; concurrent callers share one
  run (the `ProbedGpus` serial-probe pattern). The OS, kernel and sysfs or CfgMgr32 reads are not
  cached: a few small reads per call.

**The manager method: `NodeInfo` / `NodeInfoAs`.** One read, on D-Bus, the pipe and `ManagerClient`,
gated exactly as `Devices` (`control` asked quietly; the `As` form only from the bff's account for a
current `pai` member), because it may spawn a child. Answer:
`{"os": {"name", "version", "kernel"}, "vulkan": "ok"|"none"|"failed", "gpus": [{"pci", "vendor",
"name", "driver", "version", "vulkan": null | {"driver", "info"}}]}`, every text field a string or
`null`; `vendor` the PCI vendor id as 4 lower-case hex digits (`1002`, `10de`, `8086`; the fe's strings
name them); rows in bus id order, kernel rows first. Bounds: os-release read up to 4 KiB, each sysfs
value up to 128 bytes and printable ASCII trimmed (the versions files' rule), each Vulkan string up to
256 bytes, at most 32 rows, the child's stdout capped by `PROBE_STREAM_CAP`; a value outside these is
`null`. A plain method rather than a field of `Devices`: the device list is the engine's view and its
cold probe can take 30 s, the node info is the host's and polled. Additive, so it follows the
playbook's protocol rule (local skew needs a broken install). Step 14: the director calls the node
manager's `NodeInfo` over the mutual TLS channel, under Part 2's "Who may act on which node", and
answers `GET /api/nodes/{node}` with the same object.

**The bff's routes.**
- `GET /api/version`, the About read (decision 3): `{"version", "name", "os": {"name", "version", "kernel"}}`.
  `name` is the host name of the bff's machine (`listen::hostname()`, the value `/api/node` already
  answers; added 2026-11-22 from brief 18 section 9): the shell reads the navigation's host name here, so
  it never waits on `/api/node`'s manager call (up to 10 s cold). About still shows only the version
  and the OS. Exposure unchanged: the same session-gated value `/api/node` gives, and the `Host` guard
  already accepts it. In step 14 this is the director's machine; a node's name comes from
  `/api/nodes`.
  The bff reads `os` itself with the same `pai-core` reader (it is the web service's machine, and the
  readers need no privilege: the bff's unit hides `/dev` and `/usr/lib/modules`, not `/etc` or `/sys`,
  systemd.exec(5)). It keeps its rules: no manager, idle deadline unmoved, 401 or 200. In step 14 the
  director's bff answers its own machine here, beside `nodes`.
- `GET /api/node`, the node's info: today's `{"name", "api", "version", "engines"}` from the bff, plus
  `os`, `vulkan` and `gpus` from `NodeInfoAs` as the signed-in user. Its rules, restated:
  - **Idle deadline: still left alone.** It is a polled read; the session is read with `peek`, whose
    identity is the `As` principal (as `boot_read` peeks).
  - **Manager down or slow: a partial 200, not 503** (preference: the navigation's host name and the
    versions must not vanish with the manager). `os`, `vulkan` and `gpus` are `null` (unknown, distinct
    from `gpus: []`, a node with no GPU), plus `"nodeInfo": {"key": "client-unavailable", "args": []}`
    so the page can say why; any other client failure is `"nodeInfo": {"key": "client-other", ...}`.
    Without a failure, `nodeInfo` is absent.
  - **A refusal of the user ends the session**, 403 `session-not-administrator`, as every `As` read.
  - **Bound:** the bff waits `NODE_INFO_TIMEOUT` = 10 s (the child's 8 s plus the reads); the fe's 15 s
    for a `GET` stays above it. No manager ask without a session: 401 `session-required` first.
- No new route. The About page reads `/api/version` only; the host name, engines and runtimes, OS and
  GPU drivers go to the node's info page or section (the designer's brief decides where).

**Trust boundaries.** The bff to the manager is the existing `As` boundary (the manager trusts the
bff's account for a principal and checks `pai` itself); the manager to its child is a process it
spawns from its own executable, reading only stdout, capped and parsed as untrusted text. What a
signed-in administrator learns is more fingerprinting detail (kernel, driver versions), behind the
same session; nothing reaches a visitor. A caller without `control` cannot make the manager spawn the
child (refused before the read, as `Devices`). Fails closed: an unreadable or malformed value is
`null`, never a guess, and no failure of the reader fails `/api/node` as a whole.

**Slices (each test first; Linux, the Windows code type-checked).**
- **AN1. The host readers in `pai-core`.** `host::os(root)` and `host::gpu_drivers(root)` over a
  fixture root; the Windows readers behind a seam. Done when: an `/etc/os-release` fixture gives
  `Ubuntu`/`26.04`; with only `/usr/lib/os-release` that one is read, with both only `/etc`'s; a missing
  or 5 KiB file is all `null`; a PCI tree with a `0x030000` amdgpu device and `module/amdgpu/version`
  gives one row with its version, a `0x030200` nvidia device a row, a `0x020000` NIC no row, a driver
  with no `version` file a row with `version: null`, an unbound device a row with `driver: null`; 40
  display devices give 32 rows; the Windows code type-checks.
- **AN2. The Vulkan probe and its join.** `pai-manager vulkan-probe` and its parser and join in
  `pai-core`. Done when: the parser turns a fixture document into rows and refuses (as `failed`) a
  document over the cap, a non-JSON one and strings over 256 bytes (cut to `null`); the join fills a
  kernel row with a matching bus id, appends an unmatched device as its own row, and skips a CPU-type
  device; a probe script that sleeps past the bound gives `failed` within bound plus 1 s; a script that
  prints `none` gives `none`; two concurrent calls run the script once and a call within 10 minutes
  runs it not at all (a clock seam). Run once by hand on this machine (the user's go-ahead, a GPU
  read) to record the real rows and the time in the README.
- **AN3. `NodeInfo` / `NodeInfoAs` in the manager.** Done when: an e2e over D-Bus and the pipe returns
  the document from fakes; a caller without quiet `control` is refused and the probe never ran; `As`
  for a non-member, and `As` from a caller that is not the bff, are refused; `ManagerClient::node_info`
  decodes it.
- **AN4. The bff's replies.** Done when: `crates/bff/tests` show `/api/version` with `os` from a
  fixture root and `name` equal to `/api/node`'s `name`, the fake manager never connected; `/api/node` with the unchanged fields plus `os`,
  `vulkan`, `gpus` from the fake manager, asked `NodeInfoAs` with the session's user; the idle
  deadline unchanged after either read; a fake manager that does not answer within the bound gives
  200 with the bff's fields, `os`/`vulkan`/`gpus` `null` and `nodeInfo` `client-unavailable`; a refusal
  gives 403 `session-not-administrator` and the session ended; 401 with no session and the manager
  never connected. `fe/briefs/api-contract.md` and `crates/bff/README.md` gain the fields (docs agent).
- **AN5. The web UI** (designer's brief first): About shows version and OS only; the node's info shows
  name, OS, engines and runtimes, and per GPU the kernel and Vulkan drivers. Done when: the brief's
  criteria pass and the UX check is green.
- Step 14 takes the director's call of `NodeInfo` and `GET /api/nodes/{node}`.

**Decisions (user, 2026-11-22).** 1. The manager reads the node facts, through `NodeInfo` /
`NodeInfoAs` added now (the architect had recommended the bff). 2. The Vulkan driver per GPU is included,
read by the manager (the architect had recommended leaving it out). 3. About reads `/api/version`
with `os` added. The architect's calls within them: a plain method gated as `Devices`, a child
process for the Vulkan read, a partial 200 for `/api/node` when the manager is down, a 10-minute cache
of the Vulkan answer.

### Create, clone and delete model profiles from the web (user, 2026-11-22, `PLAN.md` "Create and delete model profiles from the web"): accepted (user, 2026-11-22), the recommendations

The requirement: the web deletes a model profile only when it is switched off (its section in
`models.ini.disabled`) and assigned to no instance, otherwise the manager refuses with a keyed refusal
naming the reason (and the instances); the web creates a model profile from an id and its model file
or Hugging Face repo, born switched off; creating can also clone an existing profile, copying its
settings (user, 2026-11-22, same day); deleting drops its tags ("Tags on model profiles"). Built with
slice 17 (fe brief 16).

**What exists (checked 2026-11-22).**
- No manager method creates, renames or deletes a model profile. The GUI writes the pool files whole
  through `WriteFile` (`crates/app/src/model_profiles.rs` `delete` 1586, `rename_with` 1641, `save`
  1393); its delete removes the section from both files, switched on or not, assigned or not, and
  leaves any key naming it in `assignments.ini` (an orphan key: renders nothing while the pool lacks
  the id; `assign.rs` `render`).
- Every pool write goes through `check_and_write` (`service.rs` ~1062): `check_contents` (for
  `models.ini.disabled`: `checks::models_disabled_file`, `model_profile_ids_free`, the renders, the
  model files and memory of what renders), `write_served_file`, `render_all_locked`. The section rule
  (`checks::model_profile_section`, ~907) requires the id rule (`model_profile_id`: not empty, `a-zA-Z0-9.-_`,
  so `*` fails) and a model source (`model_profile_has_model`): a non-blank `model` or a non-blank key of
  `checks::FETCH_KEYS` (`hf-repo`, `model-url`, `docker-repo`), else `model-profile-no-model`; MP1 moved
  `FETCH_KEYS` into `pai-core` and loosened the rule. The configurator's save uses the same rule
  (`checks::model_profile_source`, since M3b), and the web save adds M3b's exactly-one-source rule. Ids are one profile in any letter case across both files
  (`model-profile-name-taken`).
- Assignment keys are exact case (`assign.rs` `assigned`); `assign::rows` gives per instance and per
  pool id the explicit `value` and `effective` (the render's one rule).
- `DeleteServerProfile` (slice 14, `service.rs` 1458) is the precedent: configure, the `writing`
  lock, `read_record` of `instances.ini` (unreadable is `Io`, never "unused"), a refusal naming the
  holder, one action record. The bff's model profile routes use `model-profiles-malformed` for a body
  they cannot read and refuse control characters in values (`crates/bff/src/model_profiles.rs` 1230);
  the manager does not, because no manager method so far built a line from a value.

**(1) One create method with an optional source, or a separate Clone.**

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. `CreateModelProfile(profile, from, values)`**: `from` a source id or `""`, `values` the file pick | One method, one route, one refusal set | Two shapes in one call: the shape rule must be exact | **Recommended** |
| B. `CreateModelProfile(profile, values)` and `CloneModelProfile(profile, from)` | Two methods, two `As`, two routes or one route dispatching | None beyond A's; more wire to test | Rejected: the checks, lock and writes are the same |
| C. No manager method: the bff writes the pool with `WriteFileIf` as the switch does | No manager code | The delete rule (switched off, unassigned) would be the bff's, judged on a stale read and bypassed by any `pai` member's own `WriteFile`; the create's "born off, never overwrites" too | Rejected: the rule belongs where the lock is (playbook: a compare belongs under `writing`) |

**(2) The manager methods** (each with its `As` form for the bff: answered only to the bff's account
for a current member of `pai`; `configure`; the `writing` lock; one action record,
`create_model_profile` / `delete_model_profile`, target the profile; a refusal writes nothing unless
said otherwise). No `engine` argument: model profile methods have none (`SetModelProfile`), one engine
per node today.

**`CreateModelProfile(profile, from, values)`**, D-Bus `(ssa(ss))`, `As` `(sssa(ss))` (principal first).
`values` is the file pick, the keys llama.cpp reads: `model`, or `hf-repo` with an optional `hf-file`
(the `value` object `GET /api/hf/files` already answers). Checks, in order:
1. Before the lock, no file read: the **shape**. Exactly one of `from` (non-empty) and `values`
   (non-empty); `values` keys only `model`, `hf-repo`, `hf-file`, each once; `model` xor `hf-repo`;
   `hf-file` only with `hf-repo`. Otherwise `Invalid(model-profile-bad-source [profile])` (new key).
   Then each value: not empty after trimming spaces and tabs, **no control character** (a newline
   would add lines, so a section, to the file the manager builds), no Unicode space at an end, else
   `Invalid(model-profile-bad-text [key])` (new key); a `;` or `#` is `comment-markers [key]` (the
   existing rule; `hf-repo` and `hf-file` join `MODEL_PROFILE_TEXT_KEYS`). This is the manager failing
   closed: the bff checks the same, but a `pai` member may call D-Bus directly.
2. Under the lock: the **id**, `model_profile_id` (`invalid-model-profile-id [profile]`; `*` and the
   empty id fall here). Both pool files read (absent is empty, any other failure `Io`).
3. **Taken**: either file holds the id in any letter case: `model-profile-name-taken [profile]`
   (existing key). A create never overwrites.
4. **The source** (clone only): `from` must be a pool id, looked up as the render does (`models.ini`
   first, then `models.ini.disabled`, any letter case); absent, or `*`, is `no-model-profile-named
   [from]` (existing key). `[*]` is the pool's global section and is never cloned.
5. **The section** built: a new one is `[<profile>]` and one `key = value` line per value, sorted by
   key (`hf-file`, `hf-repo`, `model`), in the ending style of `models.ini.disabled` (`detect_eol`;
   `\r\n` when the file is absent or empty, as the GUI writes). A clone is the source section's bytes
   as `ini::text::section_text` gives them (comments, key order, every key, `load-on-startup`
   included) with its header line replaced by `[<profile>]`, given the file's ending when its last line
   has none. Appended to `models.ini.disabled` (a newline first when the text does not end with one;
   a missing file is created holding the section alone). **Born switched off** whatever the source's
   state.
6. **Born unassigned**: before the pool write, every key naming the id in any letter case is removed
   from every section of `assignments.ini` (written only if it changed). These keys are orphans (the
   pool does not hold the id yet), so removing them changes no render; they exist only after a GUI
   delete of an assigned profile or a hand edit, and without this step a hand `on` key would serve
   the new profile at once although it is switched off. A create refused at step 7 keeps this
   cleanup (dead keys only).
7. **The pool write**: `check_and_write(ModelsDisabled, new text)`, every existing check included (the
   section rule, ids free, the renders). A `model` file is not checked for existence here: a switched-
   off, unassigned profile renders nowhere, and the existing checks judge the file when a switch-on
   or an assignment makes it render.
8. **Tags** (once "Tags on model profiles" MT1 is built): the record is set for the new id with
   `model_tags::set`, the source's tags for a clone (question 2), `[]` for a new profile (which also
   drops a hand-left orphan section). A failure here is `Io` after the profile landed (a label; the
   page shows the error), as MT1's prune.

What a clone **never** copies: assignments (step 6 makes sure of it), the switch state (step 5), the
revision (the new section has its own). What it copies: every line of the section, and the tags
(question 2). An edge accepted: a source section holding a hand-written `;` or `#` in a free-text
value (allowed in place by decision c1, which exempts only lines a section of the same id holds) is
refused at clone as `comment-markers [key]`; the administrator edits the source first.

**`DeleteModelProfile(profile)`**, D-Bus `(s)`, `As` `(ss)`. Checks, under the lock, in order:
1. The id rule fails, or the id is `*`: `no-model-profile-named [profile]` (no path is built from it
   anyway; the pool files are fixed names).
2. Both pool files read (absent empty, other failure `Io`). `models.ini` holds the id in any letter
   case: **`model-profile-enabled [profile]`** (new key; switch it off first). A twin left in both files
   by an interrupted switch is this refusal too (switching off clears the twin). Neither file holds
   it: `no-model-profile-named [profile]`.
3. `instances.ini` with `read_record` (unreadable `Io`, never "unused") and `assignments.ini` (absent
   empty, other failure `Io`). For each instance of the record, in record order, the instance
   **assigns** the profile when its section holds a key naming the id in any letter case with
   `inherit`, `on` or `off`, or when `assign::rows` gives the id `effective` (a hand `* = on` serves a
   switched-off profile; `* = inherit` does not, so it does not block). Any such instance:
   **`model-profile-assigned [profile, "<a>, <b>"]`** (new key; the instance ids joined with `, `,
   which no instance id contains). An explicit `off` counts: it is a row the admin set, and deleting
   under it would leave a key that a later profile of the same id inherits.
4. The new `models.ini.disabled` is the text with every section of that id removed (any letter case,
   repeated `delete_section`), written with `check_and_write(ModelsDisabled, …)` (renders included,
   none changes). Tags: MT1's prune after every pool write drops the section in the same call (built
   with MT1; before it there is no record).

**Errors and keys.** All refusals are keyed `checks::Refusal` (`ManagerError::Invalid`): no new
`ManagerError` variant, so D-Bus, the pipe and `ClientError` carry them unchanged (playbook). New keys
in `checks::KEYS`, `Refusal::text` and both strings files: `model-profile-bad-source [id]`,
`model-profile-bad-text [key]`, `model-profile-enabled [id]`, `model-profile-assigned [id, instances]`.
Reused: `invalid-model-profile-id`, `model-profile-name-taken`, `no-model-profile-named`,
`comment-markers`, `model-profile-no-model`.

**The `model` rule loosened** (needed for an `hf-repo` create, question 1): `model_profile_section`
requires a non-blank `model` **or** one non-blank `FETCH_KEYS` key (`hf-repo`, `model-url`,
`docker-repo`); `FETCH_KEYS` moves to `pai-core` `checks.rs`, the manager's model file and memory
checks read it there. It applies to every pool write (the GUI's too); the GUI's own pre-check
(`model_profiles.rs` 1452) stays strict, and the GUI's `ModelProfile` has no `hf-repo` field, so a GUI
save of an `hf-repo` profile is refused `model-profile-no-model` before anything is written, never a
silent drop of the key (fail closed; a GUI field waits for a request).

**Wire.** Pipe: `{"method": "CreateModelProfile", "profile", "from", "values": {<key>: <string>}}`
(`from` a string, `""` for none; `values` an object of strings, `{}` for none; any other type or a
missing field `BadRequest`), `{"method": "DeleteModelProfile", "profile"}`, their `As` with
`principal`. The manager sorts `values` by key before building anything (playbook: JSON object order
differs per build). `ManagerClient`: `create_model_profile(profile, source: &ModelProfileSource)`, the
enum `From(id)` / `Model(path)` / `Hf { repo, file: Option }`, and `delete_model_profile(profile)`,
default body `Other` as their siblings, the `As` name when `acting_for` set a principal. Protocol stays 3
(additive methods, as slice 14: the bff and the manager ship together).

**Atomicity and the lock.** Each call is one critical section under `writing`: every check, the
assignment cleanup, the pool write, the renders and the tags write see the same files, and no other
manager write interleaves. Crash points: create killed after step 6 leaves dead keys removed (no
render changes); after step 7 and before step 8 a profile with no tags; delete is one file write
(atomic replace) plus MT1's prune (an orphan section is harmless and the next pool write prunes it).

**Revisions: none** on either route. A create never overwrites (step 3). A delete is judged on the
state under the lock (switched off, unassigned), which is what makes it safe; a section revision
would only refuse a delete because someone edited a profile that is about to go, as
`DeleteServerProfile` takes none. A retried create answers 409, a retried delete 404, so a timeout's
ambiguity is visible (slice 14's rule).

**(3) The bff routes** (mirroring the server profile pool; each 401 `session-required` with no
session and nothing asked, `sessions.current`, 403 `session-not-administrator` ending the session,
503 `client-unavailable` (no manager, or the limit ran out before the call was handed over), 504
`client-timeout` once the call was handed over and its outcome is unknown (the server save's rule,
`crates/bff/README.md`; corrected 2026-11-23, slice W1), anything else 502 `client-other`; the write
timeout of `POST /api/profiles/server`):
- **`POST /api/profiles/model`** `{"id": string, "from": string}` or `{"id": string, "values":
  {"model": string} | {"hf-repo": string, "hf-file"?: string}}`: 201 `{id}`. 400
  `model-profiles-malformed` for a body it cannot read (not an object, a non-string, an unknown
  field, both or neither of `from` and `values`, a `values` key outside the three, a control
  character, the existing route family's key); 422 `invalid-model-profile-id` `field` `id`; 422
  `comment-markers` `field` the key; 422 `no-model-profile-named` `field` `from`; 422
  `model-profile-no-model` / `model-profile-bad-source` (`field` `values`) if one ever reaches the
  manager; 409 `model-profile-name-taken` `[id]`. Values trimmed of spaces and tabs as the save does.
- **`DELETE /api/profiles/model/{id}`**: 204; 404 `no-model-profile-named` `[id]`; 409
  `model-profile-enabled` `[id]`; 409 `model-profile-assigned` `[id, instances]`.
- The list (`GET /api/profiles/model`) needs nothing new: the new profile is a row with `enabled:
  false` and no assignments. Whether a row offers Delete is the fe's reading of `enabled` and the
  row's assignments; the manager's refusal stands for a stale page.

**(4) The GUI and `pai`: unchanged in this step** (`PLAN.md`: "the GUI keeps writing the pool files
whole"). Moving the GUI's delete to `DeleteModelProfile` would put the web's rule on the desktop (a
profile switched on, or assigned, could no longer be deleted in one click), and macOS has no manager,
so `model_profiles.rs` keeps both paths anyway. The asymmetry is accepted and documented: the GUI's
delete can remove a switched-on or assigned profile and leaves orphan assignment keys, which the web's
create cleans (step 6). If the user wants one rule everywhere later, the GUI's Linux and Windows
backend calls the two methods and macOS keeps the file path: a small slice, not designed now.

**Trust boundaries.** Unchanged in kind. Browser to bff: the session and `pai` membership as every
write route. bff to manager: the `As` rule (only the bff's account, only for a current `pai` member,
re-checked per call). Local client to manager: `configure` (polkit, the member's own password on
Linux; `pai` on Windows). The manager builds file lines from caller text for the first time, so it
judges every byte itself (shape, control characters, comment markers, the id charset) before the
lock and refuses before writing; ids never build a path (the pool files are fixed names). An attacker
with a session can create switched-off profiles (nothing runs until an assignment or switch, both
already `configure`) and delete unused ones (exactly what the rule allows); without a session, nothing.

**(5) Slices (each test first; Linux, the Windows code type-checked).**
- **MP1. The manager.** The loosened `model` rule and `FETCH_KEYS` in `pai-core`; `CreateModelProfile`,
  `DeleteModelProfile` and their `As` on D-Bus, the pipe and `ManagerClient`; the four keys. Done when:
  a create with `model` writes `[x]` / `model = /m/x.gguf` at the end of `models.ini.disabled`, every
  other byte kept, and nothing in `models.ini`; with `hf-repo` only it is accepted; a clone of a
  switched-on source lands in the disabled file with the source's lines byte for byte under the new
  header, its trailing blank lines left out (the separator before the next section is layout, not the
  source's lines: a mid-file and a last source clone alike, and the call's keys follow the last line
  with no blank between); an `assignments.ini` holding `X = on` for the new id `x` under an instance loses that key and
  the render of that instance stays `[*]`-only; a taken id (other case, either file) is
  `model-profile-name-taken`, `*` and `a b` are `invalid-model-profile-id`, `from` absent or `*` is
  `no-model-profile-named`, both or neither source and `hf-file` alone are `model-profile-bad-source`,
  a value with `\n` is `model-profile-bad-text` and no file changed; delete of a switched-on profile
  is `model-profile-enabled`, of one with an explicit key under two instances is `model-profile-assigned`
  naming both, of one served only through a hand `* = on` is refused too, under `* = inherit` it
  is removed; an unreadable `instances.ini` is `Io` and nothing removed; the `As` forms refuse a
  non-member and a non-bff caller; through D-Bus and the pipe each; each crate's clippy and fmt green.
- **MP2. The bff.** The two routes. Done when: `crates/bff/tests` show 201 `{id}` and the stub received
  the call with `from` or sorted `values`; each 400 case above; 422 `field` per key; 409 and 404 relayed
  with their args; 403 ending the session; 401 asking nothing. `fe/briefs/api-contract.md` gains both
  routes (docs agent).
- **MP3. The web UI** (slice 17, the designer's brief 16 first): a create form (id, the model or
  Hugging Face pick from M2/M3, or "clone from"), Delete offered on a switched-off, unassigned row,
  the refusals shown in their strings. Done when: the brief's criteria and the UX check.
- **W1. One timeout answer for every bff write** (added 2026-11-23). Today the revision-checked saves
  answer 504 `client-timeout` once the write was asked (`TimedOut`, `crates/bff/src/server.rs`), while
  the single-call writes answer a plain `tokio::time::timeout` with 503 `client-unavailable`, telling
  the page nothing changed when the write may have landed: `model_profiles::create` and `remove`,
  `server_profiles::create` and `remove`, `instances::create` and `remove`, `control::act`,
  `boot_write` and `tags_write`. W1 gives them the save's rule: 503 when no manager answers the
  connect or the limit runs out before the call is handed over, 504 `client-timeout` after it.
  Reads keep 503. Done when: per route, a fake manager that blocks past `write_timeout` after
  receiving the call gives 504 `client-timeout` and the session lives on; a fake connect that never
  returns gives 503; the existing tests stay green. The docs agent then fixes `crates/bff/README.md`:
  the 504 row on those routes, and the three places that say the 413 comes "before the session is
  read" (every write handler reads the session first and turns the body cap's rejection into 413
  after it, so no session is 401 whatever the body: the gate's rule, which stays).
- **W2. Every write under the write limit, and a lost manager after the hand-over is 504** (added
  2026-11-23, from W1's review). (a) `control::routes` gets the read limit (`MANAGER_TIMEOUT`, 5 s:
  `crates/bff/src/lib.rs` ~435), so the run verbs, `boot_write` and `tags_write` run under it; that is
  history (the routes predate `WRITE_TIMEOUT`), not a choice, and a boot switch (enable or disable plus
  a daemon reload, and from P1 a hop through the helper) or a tags write under the manager's write lock
  is a write like the others. `routes` takes both limits: `boot_read` keeps `MANAGER_TIMEOUT`, `act`,
  `boot_write` and `tags_write` take `write_timeout`. (b) A manager `Unavailable` (a broken stream, a
  missing reply) that comes back AFTER the call was handed over leaves the outcome unknown, so it is
  504 `client-timeout`, as the saves answer through `state.sent()`; before the hand-over (the connect
  failed) it stays 503. Applies to `write_failure`, `create_failure`, `action_failure` and the other
  failure mappers of W1's nine routes. Done when: per route, a fake manager that answers `Unavailable`
  after receiving the call gives 504 and the session lives on, and a connect that fails gives 503; a
  verb, a boot write and a tags write whose fake manager takes longer than the read limit but less
  than `write_timeout` answer their success, while the boot read still answers 503 past the read limit;
  the existing tests stay green. The docs agent then updates `crates/bff/README.md` (the "same 5 s
  timeout as the read" of the tags and boot writes, the verbs' rows, the 504 rows).
- **With MT1** (tags, after slice 17): MT1's done-when gains "a delete through `DeleteModelProfile`
  leaves the record without its section" and "a clone carries the source's tags; a create of a new id
  leaves no section for it".

**(6) Decisions for the user (recommendation first).**
1. **Create from `hf-repo` now**: (a) yes, with the `model` rule loosened as above (recommended: the
   accepted M4 picker writes `hf-repo` anyway and llama.cpp downloads it; the GUI refuses such a
   profile at its save rather than dropping the key); (b) `model` only now, `hf-repo` with M4.
2. **A clone copies the tags**: (a) yes (recommended: tags label the settings being copied, and a
   clone is born switched off and unassigned, so `tag:` routing cannot reach it until an admin acts);
   (b) no, a clone starts untagged.

**Decided (user, 2026-11-22):** 1 (a), create from `hf-repo` now with the `model` rule loosened; 2 (a),
a clone copies the source's tags.

### Instance labels (user, 2026-11-23): accepted (user, 2026-11-23), a label beside a fixed id

The user's words (2026-11-23, translated): "at some point let me rename the instance too (if it still
makes sense, since for me the instance is engine + port)". **Kept**: the id, which names the unit
(`llama.cpp@<id>`), the Windows service and its virtual account, the sections of `instances.ini` and
`assignments.ini`, the state folder and the log; it never changes after create. **Added**: one
framework key, `Label`, in the instance's `instances.ini` section, written only by the manager.
**Removed**: nothing. Rejected: a real rename of the id (a new Windows account SID, state and log
moved, journal history left under the old unit, a halfway rename to recover) and a name derived
only from engine and port (it changes with the server profile, and two stopped instances can share a
port).

- **What the web UI shows**: the label when set; otherwise `<engine> :<port>` (`llama.cpp :9931`) from
  the fields `ListInstances` already answers (`engine`, `port` of the instance's server profile), the id
  shown as the secondary, stable name. The wording and layout are the designer's (brief 16, 17f).
- **The text rule** (`pai-core` `checks::instance_label`): spaces and tabs trimmed at both ends; then
  empty, or 1 to 64 characters with no control character and no `;` or `#` (`checks::comment_markers`:
  the INI reader would cut the value). Refusal `instance-bad-label` `[label]`, or `comment-markers`
  `[label]`.
- **Empty clears it**: `SetInstanceLabel(id, "")` (after the trim) removes the `Label` line from the
  section, so the reader sees no label and the UI falls back to engine and port; never an empty key.
- **Unique on the node** (preference: two rows with one name defeat the point of a name): a label
  equal to another instance's label, ignoring letter case, is refused `instance-label-taken`
  `[label, other-id]`, judged under the manager's write lock. It is not compared with the fallback
  text nor with ids.
- **Who**: `configure`, as `SetInstanceTags`; works while the instance runs (nothing the engine reads
  changes, no render, no restart). The `As` form for the bff, answered only to the bff's account for a
  current `pai` member. One action-log record per call. Trust boundaries unchanged.
- **Protocol**: `SetInstanceLabel(instance, label)` `(ss)` and `SetInstanceLabelAs` (principal first, as
  every `As`), `NoInstance` for an id the record lacks; `ListInstances` rows gain `label` (empty string
  when unset on D-Bus, `null` on the pipe). The D-Bus row signature grows by one field: local skew needs
  a broken install (the packages pin `-core`), and the director's link will negotiate `Version`.

**Slices** (test first; Linux, the Windows code type-checked):
- **IL1. The manager and `pai-core`.** `checks::instance_label`, `InstanceInfo.label`, the reader of
  `Label`, `SetInstanceLabel` and `SetInstanceLabelAs` on D-Bus, the pipe and `ManagerClient`. Done
  when: `"  Coding box "` writes `Label = Coding box` under `[<id>]`, every other byte of
  `instances.ini` kept, and `ListInstances` answers it with `engine` and `port`; `""` and `"   "` remove
  the line and the row's label is empty/`null`; 65 characters, a control character are
  `instance-bad-label`, `a;b` is `comment-markers`, `CODING BOX` on another instance is
  `instance-label-taken` `[CODING BOX, <other>]`, an unknown id is `NoInstance`, each with no file
  changed; a running instance is relabelled without a stop; the `As` forms refuse a non-member and a
  non-bff caller; through D-Bus and the pipe each; each crate's clippy and fmt green.
- **IL2. The bff.** `PUT /api/instances/{id}/label` `{"label": string}`: 204; 400 `instance-malformed`
  for a body that is not that object; 422 `instance-bad-label` / `comment-markers` with `field`
  `label`; 409 `instance-label-taken` with its args; 404 `client-no-instance`; 403 ending the session;
  401 asking nothing; the write limit and W1/W2's rule (503 before the hand-over, 504 `client-timeout`
  after it, a lost manager after the hand-over included). `GET /api/instances` rows gain `label`
  (string or `null`). Done when: `crates/bff/tests` pin each answer above against the fake manager,
  the stub received the trimmed label, and the list carries `label`; the docs agent adds the route to
  `fe/briefs/api-contract.md` and `crates/bff/README.md`.

### ROCm host memory as GTT (user, 2026-11-24): accepted (user, 2026-11-24), every instance, confirmed under load

The finding and its measures are `PLAN.md` "ROCm host memory as GTT, under test": with
`HSA_USERPTR_FOR_PAGED_MEM=0` and `HSA_SVM_FOR_PAGED_MEM=0`, ROCm's paged host memory is driver GTT
(libhsakmt `fmm.c` `fmm_allocate_host_gpu`), so compaction and reclaim no longer evict the process's GPU
queues. Both must be `0`: `HSA_USERPTR_FOR_PAGED_MEM=0` alone falls back to SVM, which reclaim moves too.
**Decision (user, 2026-11-24): both variables on every instance, every backend.**

- **Where: the launch, not the unit.** `pai_core::launch::env_vars` already turns server profile keys
  into llama-server's environment (`ROCBLAS_USE_HIPBLASLT`, `GGML_CUDA_*`), inherited by the router's
  per-model children, and `launch_command` serves both `run-instance` (Linux exec, Windows child) and
  the macOS spawn. The unit's `Environment=` (llama.cpp's package) was the other option: rejected, it is
  Linux only, needs an engine package release, and could not be turned off per instance.
- **Every backend**: the two variables are read only by ROCm's runtime (libhsakmt); CUDA, Vulkan and
  Metal never read them (preference: one rule, no backend detection; harmless where unread). Windows: the
  HIP SDK there does not run on libhsakmt/KFD, so no effect is expected (unverified; the type check is
  all that runs there).
- **Turning it off**: a server profile key `RocmHostMemoryGtt` (bool), unset or `true` exporting both
  as `0`, `false` exporting neither (the runtime's defaults return). Why a switch: that memory is no
  longer swappable and GTT is capped near half the RAM, so a large pinned host allocation can fail
  where it used to succeed. The key follows `RocblasUseHipblaslt`'s pattern (commented line in the
  render, the bff's key list, `checks.rs`' bool rule); the web Server page shows it with the designer's
  wording.
- **Confirmed under load** (Strata, 2026-11-24): 0 KFD evictions from 20:41 to 21:33 through two
  Playwright runs and a cargo build (`PLAN.md` "ROCm host memory as GTT"). The docs state it as measured
  there, with that date; no "under test" mark.

**Slice GT1** (test first; Linux, Windows type-checked). `ServerConfig.rocm_host_memory_gtt:
Option<bool>`, read and rendered as `RocmHostMemoryGtt`; `env_vars` exports
`HSA_USERPTR_FOR_PAGED_MEM=0` and `HSA_SVM_FOR_PAGED_MEM=0` unless it is `Some(false)`; the bff's server
keys and `checks.rs` take it. Done when: a profile without the key, and one with `true`, launch with both
variables `0` (the `launch_command` env, also through `instance_command`); `false` launches with
neither; the render writes the key only when set and its commented default line otherwise; the bff reads
and saves it and refuses a non-bool 422 with `field`; each crate's clippy and fmt green. Check on this
machine after the deploy and the user's next instance start (starting llama-server is the user's):
`/proc/<pid>/environ` holds both; `/proc/<pid>/smaps` shows no large `/dev/zero (deleted)` rw-s
mappings; `/sys/class/kfd/kfd/proc/<pid>/stats_*/evicted_ms` stays flat under load.

## ROCm instances load models with `load-mode = none` (GT5, accepted 2026-11-25)

Why (`PLAN.md` "ROCm host memory as GTT", GT4): on the R9700s an mmapped model's weights are pinned
by ROCclr and registered as SVM ranges whatever GT1's variables say (722 MiB for a 0.6B Q8_0
embedder), and compaction and reclaim evict them (about 1.5 s per GPU per hour). With `load-mode =
none` the same embedder registered 147 MiB and had 0 ms of evictions over 3 h. The user's decision
(2026-11-25): `none` is the default on ROCm, changeable per model profile, plus a guide.

- **What is ROCm** (Linux only; Windows and every other node unchanged): the instance's server profile
  `Devices` names a device whose backend is `rocm` (`devices::backend_of`), or leaves `Devices` unset
  (every card) and the engine's installed backends (`versions::installed`, the package files) hold
  `rocm`. `Devices = none` (CPU) or a list of only Vulkan or CUDA devices is not ROCm. Rejected: the
  engine's `--list-devices` probe, which spawns the engine at every render, the manager's start among
  them, for an answer the two files already give.
- **Where**: the manager's render of `models@<id>.ini` (`render_all_locked`, `render_of`), the one
  place that already knows the instance. For a ROCm instance whose pool `[*]` carries no `load-mode`,
  the render's `[*]` carries `load-mode = none` (the `[*]` section is created when the pool has none).
  The pool files are never written. An explicit value wins by llama-server's own precedence: a model
  section over `[*]`, and the launch passes no `--load-mode` (`launch.rs` keeps it per preset).
  Rejected: an env `LLAMA_ARG_LOAD_MODE` or a router flag in `launch.rs` (a command line beats every
  preset value, so the per-profile choice would be lost); writing the key into the pool (the pages
  would then show a value nobody chose, and a profile moved to a Vulkan instance would keep it). Every
  served-file write re-renders, a server profile save and an instance's profile switch included, so a
  change of `Devices` takes effect at the instance's next start.
- **The trade-off**: with `none` the weights are read into anonymous memory, not the page cache, so
  tensors a profile keeps on the CPU (`n-cpu-moe`, `override-tensor` to `CPU`) are no longer shared
  or dropped under pressure, and the load reads the whole file. Applied on every ROCm instance anyway:
  the evictions stall every model, and the profile that wants `mmap` sets it on its page. Rejected:
  applying it only when every weight is on the GPU (the render would have to judge the tensor rules).
- **What the pages show**: the pool's value, as now; an unset `load-mode` is shown with a hint saying
  it means `none` on a ROCm instance and `auto` elsewhere (static wording, no new API field; the
  designer words it, GUI and web).

**Slice GT5a** (test first; `pai-manager`, Linux; Windows type-checked). The render adds `load-mode =
none` to `[*]` for a ROCm instance as defined above. Done when: an instance whose profile names
`ROCm0` renders it; unset `Devices` with a `rocm` backend file renders it, without one does not;
`Devices = none` and `Vulkan0,Vulkan1` do not; a pool `[*]` with its own `load-mode` is copied as it is
and nothing is added; a model section's `load-mode = mmap` is kept; the pool files are byte-for-byte
unchanged; a server profile save that adds `ROCm0` re-renders with the line; on Windows nothing is
added; clippy and fmt green. Check after the deploy and the user's next instance start (starting
llama-server is the user's): `models@default.ini` holds the line; the embedder's KFD `evicted_ms`
stays flat under load.

**Slice GT5b** (designer brief first; web UI, then the GUI's model profile tab): the unset hint on the
`load-mode` field; strings only.

**Slice GT5c** (docs agent): the R9700 guide states the default, the GT4 measurement with its date and
when to set `mmap` instead.

## An instance is started once its router answers (RS, accepted 2026-11-25)

Why (the owner's requirement, relayed by the strata-framework agent): Strata moves to its own system
unit, `After=llama.cpp@default.service` with no `Wants=`, and sizes its expert cache on the VRAM free
at its start. Today `llama.cpp@.service` is `Type=exec` (`xtask/src/linux/system_files.rs`
`instance_unit`): the start job ends as soon as `pai-manager run-instance` has exec'd llama-server,
before the router listens. The unit's main process is llama-server (`run-instance` execs it), and
`save-slots`, the unit's `ExecStop=`, reads the port from `$MAINPID`'s cmdline
(`crates/manager/src/manager.rs` `save_slots_target`); whatever signals readiness must leave both alone.

**The user's decisions (2026-11-25):**
- **Shape: `Type=exec` kept, plus `ExecStartPost={MANAGER_EXE} wait-ready %i`.** The start job ends only
  when that command exits, and `After=` waits for it ("the execution of ExecStartPost= is taken into
  account for the purpose of Before=/After= ordering constraints", systemd.service(5),
  https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html). The main PID,
  `save-slots`, signals and `Restart=` stay as they are; the manager's `Start` is unchanged (`StartUnit`
  returns its job at once, `dbus.rs` `unit_job`); the unit reads `activating` meanwhile
  (`UnitState::Activating`). Rejected: `Type=notify` with the manager as the parent (the main PID
  changes, `save-slots` and signal forwarding rebuilt), a poller forked before the exec, a llama-server
  patch.
- **Ready = the router answers `GET /health` with 200**, not the load-on-startup models loaded. The
  trade-off accepted: Strata may start 6 to 30 s before the embedder holds its VRAM (the router answers
  before its children have loaded), in exchange for a check that needs no preset parse and no model
  states. `/health` answers 200 `{"status": "ok"}` when ready and 503 while loading
  (`tools/server/README.md` "GET /health", llama.cpp v1.0.0); that page describes a single-model server,
  so what the router answers during its start is checked live in RS2 (unverified).
- **Deadline 10 min, then the start fails**: `wait-ready` exits non-zero, systemd marks the start failed
  and runs `ExecStopPost=` only, skipping `ExecStop=` (same page). The trade-off accepted: the start of
  the morning of 2026-11-25 (11 min 46 s, disk contention) would have failed. A non-zero exit is not
  abnormal, so `Restart=on-abnormal` does not restart it; `TimeoutStartSec=11min` is the backstop
  (a systemd timeout would be abnormal and restart a slow load, so the command's own deadline must come
  first).
- **No `-` prefix, so the package must not pair a new unit with an old manager** (an old `pai-manager`
  answers `wait-ready` with its usage text and a non-zero code: every start would fail). The llama.cpp
  packages keep not requiring the framework (the unit's `ConditionFileIsExecutable=` skips it with no
  manager), and gain `Breaks: private-ai-framework-core (<< V)` on the `.deb` and `Conflicts:
  private-ai-framework-core < V` on the `.rpm`, V the framework's workspace version (`Cargo.toml`
  `[workspace.package] version`, read by `xtask/src/common/version.rs` `framework()`), 2.99.1 by the
  user's decision of 2026-11-25: no bump, since no release shipped since and the only 2.99.1 core
  without `wait-ready` was this machine's, replaced by that day's deploy
  (Debian Policy 7.4, https://www.debian.org/doc/debian-policy/ch-relationships.html; RPM dependencies,
  https://rpm-software-management.github.io/rpm/manual/dependencies.html). The framework side needs
  nothing: a new manager with an old unit simply never runs `wait-ready`.
- `Restart=`: an automatic restart runs `ExecStartPost=` again, so units ordered after it wait again.
- **Windows: unchanged.** The SCM has no `After=` between Strata and an instance, and no consumer asks
  for it.

**`llama.cpp.target`: one name for every instance** (the owner's second request, 2026-11-25, relayed by
the strata-framework agent; `After=` takes no globs). Strata orders `After=llama.cpp.target` with no
`Wants=`; at boot the target is reached once every enabled instance's `wait-ready` has exited (the owner
accepts). Ordering holds between two jobs of one transaction whoever pulled them in
(systemd.unit(5) `Before=`/`After=`,
https://www.freedesktop.org/software/systemd/man/latest/systemd.unit.html), so the target only has to
be in the boot transaction and to be ordered after the instances.
- **Recommended (T0): ordering only.** `llama.cpp@.service` gains `Before=llama.cpp.target`, its
  `[Install] WantedBy=multi-user.target` unchanged; `llama.cpp.target` ships in llama.cpp's package,
  static (no `[Install]`), pulled in by a `multi-user.target.wants/llama.cpp.target` link the package
  ships under `/usr/lib/systemd/system/` (the way systemd's own static units are pulled in), so no
  enable step and no state. Nothing in the manager changes: `SetBootEnabled` keeps `EnableUnitFiles`
  and `DisableUnitFiles` on the instance (`crates/manager/src/manager/dbus.rs` `set_boot_enabled`),
  `BootEnabled` keeps reading `GetUnitFileState == "enabled"`, and the enabled instances' links in
  `/etc/systemd/system/multi-user.target.wants/` stay valid through the upgrade, as do the purge and
  `%preun` scripts that remove them (`system_files.rs` `INSTANCE_LINKS`).
- Rejected (T1): `[Install] WantedBy=llama.cpp.target`, so the target pulls the instances in and
  `systemctl start llama.cpp.target` starts them. It moves every enabled instance's link: the upgrade
  must re-enable each linked instance, `INSTANCE_LINKS` and both packages' removal scripts change, and
  the target itself needs enabling. Nobody asks for starting through the target; it can follow.
- **No `PartOf=`**: stopping or restarting the target must not stop the instances, which the manager
  starts and stops one by one (`StartUnit`/`StopUnit` per instance).
- What the target does not give: an instance started by hand after boot is not waited for by a unit
  already started, and a disabled instance is not in the boot transaction. Both as asked.

**Slice RS1** (test first; `pai-manager`, Linux; Windows type-checked). `pai-manager wait-ready <id>`:
the port and the address from `$MAINPID`'s cmdline: `--port`, or llama-server's default
(`server_cfg::DEFAULT_PORT`) when the launch left it out, as `slot_save::slot_target` does (`launch.rs`
omits `--port` for an unset `Port`); the address is `--host`, loopback for `0.0.0.0` or `::`, since an
instance bound to one LAN address does not answer on `127.0.0.1` (amended at the RS1 review,
2026-11-25). `GET /health` every 2 s, the HTTP read and the clock behind seams. Done when: 200 exits 0;
503 and connection refused are retried; the 10-min deadline exits non-zero with a journal line naming
the instance; a launch without `--port` polls the default port; a launch on a LAN address polls that
address; no `$MAINPID` exits non-zero at once; the argument parser takes the command beside
`run-instance` and `save-slots`; clippy and fmt green.

**Slice RS2** (test first; `xtask`, the instance unit and the llama.cpp packages): `ExecStartPost=
{MANAGER_EXE} wait-ready %i` and `TimeoutStartSec=11min` in `instance_unit`; the `.deb` `Breaks:` and the
`.rpm` `Conflicts:` on `private-ai-framework-core` below the workspace version (`<< 2.99.1` and
`< 2.99.1` today, taken from `version::framework()`, not a separate constant); `Before=llama.cpp.target`
in `instance_unit`, `llama.cpp.target` (a description, no `[Install]`) and its
`usr/lib/systemd/system/multi-user.target.wants/llama.cpp.target` link in llama.cpp's `.deb` and `.rpm`
(Component::Llama). Done when the unit test pins the three lines, the package tests pin both relations with the workspace version,
the target file and the link (a symlink to `../llama.cpp.target`; if a package format cannot ship the
link, its install script runs `systemctl add-wants multi-user.target llama.cpp.target` and its removal
undoes it), and `instance_unit`'s `[Install]` still reads `WantedBy=multi-user.target`. Live, after the
install and the user's reboot or restart: `systemctl list-dependencies --after llama.cpp.target` lists
`llama.cpp@default.service`, and `systemctl is-enabled llama.cpp@default` reads as it did before. Needs a llama.cpp package build and
install, beyond the framework-only deploy go-ahead. Check after the user's restart of
`llama.cpp@default` (starting llama-server is the user's): `systemctl status` shows `activating
(start-post)` until the router answers, then `active`; `curl -s -o /dev/null -w '%{http_code}'
127.0.0.1:<port>/health` answers 200 by then (and what it answered while the router started is noted
in the record).

**Slice RS3** (docs agent): the packaging guide's instance unit, the 10-min limit, and how another unit
orders itself after an instance (`After=` only).

## Instance profiles (user, 2026-11-26): accepted (user, 2026-11-26), the recommendations

The request (user, relayed 2026-11-26, for 2.99.1):
- An **instance profile** is one server profile plus a list of model profiles. Its page adds existing
  model profiles with a + and removes them with a minus.
- An instance is assigned one instance profile, so the same bundle can go onto several instances, on
  several nodes later.
- The Models list loses its on/off switches and becomes a plain list, and the model profile page
  loses its switch.

A data model change. The options, every place that reads or writes today's model, the invariants, the
migration and rollback and the director's view were laid out first; the user accepted the
recommendation on all six questions on 2026-11-26 (reference; the pool file; P1; O; backups plus a
documented restore; the migrated instance profile `default`). The decision and the slices close the
section.

### Today's model (checked 2026-11-26)

- `instances.ini`: one section per instance; `ServerProfile = <s>` names `server@<s>.ini`. It is read
  in one place, `paths::instance_server_profile`, which launch, `profile_file`, GT5's
  `instance_devices` and the address checks go through.
- `assignments.ini`: one section per instance. There is one key per model profile id (`on`, `off`,
  `inherit`), and `*` stands for the whole pool. `assign::render` cuts the pool into
  `models@<id>.ini`, and `assign::rows` computes what the page shows.
- The pool is two files. `models.ini` holds the switched-on profiles and `models.ini.disabled` the
  switched-off ones. `inherit` follows that switch.
- Exclusivity: one server profile per instance on a node. `SetServerProfile` refuses
  `ServerProfileInUse` when another instance holds the profile (`service.rs` ~1730), and
  `check_binding` refuses two instances on one address.
- The shipped defaults: the llama.cpp package ships `instances.ini` (`[default] ServerProfile =
  default`) and `assignments.ini` (`[default] * = inherit`) under `/usr/share/private-ai/llama.cpp/`.
  Its scripts copy them into `/etc/llama.cpp/` only when missing. llama.cpp's MSI does the same into
  `%ProgramData%\llama.cpp\config\`.

### What reads or writes today's model

| Place | What touches the model |
| --- | --- |
| `pai-core` | `paths.rs` (`instances_ini`, `instance_server_profile`, `instance_models`, the pool paths); `assign.rs` (`render`, `rows`, `assigned`); `checks.rs` (`models_disabled_file`, `model_profile_ids_free` across the two files, `instance-no-model-profiles`, the instance and server profile keys); `launch.rs` (`instance_command` through `instance_server_profile`); `client.rs`, `client/pipe.rs` (the wire of `SetServerProfile`, the assignment methods, the `ModelsDisabled` file kind) |
| `pai-manager` | `service.rs`: `check_contents` per pool file, `render_all_locked`/`render_of`, `set_server_profile` with `profile_holder`/`profile_in_use`, the assignment writes, `create_model_profile` (lands in `.disabled`), `delete_model_profile` (`model-profile-enabled`, `model-profile-assigned`), `create_instance`/remove, `DeleteServerProfile` (in use by an instance), `check_address`, GT5 `instance_devices`. Also `dbus.rs`, the Windows pipe and `windows/entry.rs` (render at start), `windows/logon.rs` (import), and the tests (`dbus_client`, `pipe_client`, `windows_sync`, `tests/mod.rs`) |
| `pai-bff` and the contract | `instance_profiles.rs` (17d `PUT /api/instances/{id}/profiles/server`, 17e `GET`/`PUT .../profiles/model[/{profile}]`); `model_profiles.rs` (`PUT .../{id}/enabled`, the `enabled` and `assignments` fields, create and delete); `POST /api/instances` (takes a server profile); tests `instance_profiles`, `model_profiles`, `model_profiles_switch`, `model_profile_read`, `model_profile_save`; `fe/briefs/api-contract.md` rows 84-94 and the association section |
| web UI | the Models list's switch, the model profile page's switch, brief 17d2 (the instance page's server profile switch, briefed, not built) |
| `pai` | `cli.rs` `model-profile enable/disable` and the listing's `enabled`, `cli.toml` |
| `pai-config` | `models_tab.rs` (the switch); `pai-app` `model_profiles.rs` (`set_enabled`, delete and rename across both files), `server_control.rs`, `linked_files.rs`, `migration.rs` (imports `presets.ini.disabled` as `models.ini.disabled`); tests `save_flow`, `control_lint`. macOS keeps its own per-user files and runs llama-server itself, with no instances |
| packages | `xtask/src/linux/system_files.rs` and `package.rs` (the defaults and the copy-when-missing scripts), `components.rs`; `installer/llama.cpp.wxs`, `xtask/src/common/msi.rs`, `ci_windows.rs`, `msi/tests/two_installers.rs` |

### Options

**Where the instance profile lives.**

| Option | Cost | Risk |
| --- | --- | --- |
| **A. A pool file `instance-profiles.ini`**, one section per instance profile: `ServerProfile = <s>`, `Models = a, b` (model ids hold no comma or space). `instances.ini` names it with `InstanceProfile = <p>` | One new `ConfigFile` kind, written only by manager methods under the `writing` lock, like `assignments.ini` today. The migration is a re-key of `assignments.ini` plus one line. `ini::text` helpers reused | Every profile shares one file: no per-profile revision, but every edit is a method under the lock, so none is needed |
| B. One file per profile, `instance-profile@<p>.ini` (like `server@<s>.ini`) | A directory scan and per-file CRUD, as server profiles have (`DeleteServerProfile`). The director copies one file | More file names in the engine folder; a new file kind with its own path rule and ACL; nothing that A lacks for one node |
| C. A section in an existing file: `assignments.ini` re-keyed by profile, with the server profile as a key | No new file | `ServerProfile` is a valid model profile id, so the key namespaces collide (a reserved id or a list form is needed); the file's name then lies |

**What an instance holds: a reference or a copy.**

| Option | Cost | Risk |
| --- | --- | --- |
| **R. Reference**: `instances.ini` names the instance profile, and every read resolves through it (`instance_server_profile` becomes profile, then server profile; the render reads the profile's `Models`) | One resolution function changes; a save of a profile re-renders every instance that uses it | A save reaches every instance using the profile at its next start; in-use rules for three kinds of profile |
| T. Template: applying an instance profile copies its server profile name and model list into the instance's own record (today's `ServerProfile` plus an assignment) | Almost no change to the instance side | Edits do not propagate: the reuse the user asks for becomes "apply again" on each instance; two sources of truth after the first edit |

### Invariants (for A + R)

1. **An instance always has exactly one instance profile** (user, 2026-11-26). Creating an instance
   takes one; the manager refuses to clear it. A record naming none or an unknown one, written by
   hand, cannot start (today's "names no profile").
2. An instance profile names exactly one server profile and zero or more model profiles. A start with
   none is refused, as today (`instance-no-model-profiles`, 409).
3. **Ports: two instances sharing a profile on one node.** The user's lead (2026-11-26): "the port
   could perhaps be auto-assigned". Today the address (`Hostname` and `Port`) is in the server profile,
   and an instance's server profile is exclusive on its node (`ServerProfileInUse`, `check_binding`;
   `client-address-in-use` is the client's text for the manager's `AddressInUse`, `client.rs` 142).
   What needs the port:
   - The router binds `--port` from the launch. Its children are its own business, behind it.
   - `wait-ready` (RS1) reads `--port` from the launch's argv, so it follows whatever the launch
     passes.
   - `save-slots` reads the port the same way.
   - Strata serves `:8580` and talks to instance `default` on `:8581`, and opencode's provider entry
     names the same port. Both are fixed in other programs' files, so **a client needs a port that
     stays the same across restarts**.
   - `check_binding` compares only the instances' own addresses. It cannot see a foreign listener
     such as Strata.

   | Option | Cost | Risk |
   | --- | --- | --- |
   | **P1. The address moves to the instance record**: `Hostname` and `Port` in the instance's section of `instances.ini`, out of the server profile. At create the manager proposes the lowest port from 8581 that no instance record holds and that nothing listens on at that moment; the administrator may change it. The record is the one truth | The migration moves `Hostname` and `Port` from each instance's server profile into its record; launch, `binding`, `check_address` and `AddressInUse` read the record; the server profile page loses two fields, and the instance page gains them; a new `SetInstanceAddress(As)` | A server profile no longer says where it listens, so the director picks the port per node at deploy (which it must do anyway). A foreign listener that starts later on the same port is still unseen, as today |
   | P2. Auto-pick only when the profile leaves `Port` unset, recorded in `instances.ini` at create | No page change for profiles that set a port | Two sources for the port (profile or record) and a rule for which wins; a profile with a port is still exclusive on its node, so sharing works only for some profiles |
   | P3. A free port picked at every start | Nothing recorded | Strata and opencode lose the instance at every restart. Rejected |

   **Decided: P1** (user, 2026-11-26). It makes the address what it is, a property of the instance on its node. It
   removes the exclusivity rule for profiles altogether: any number of instances on one node may
   share an instance profile or a server profile, and `AddressInUse` is judged on the records alone.
   It also gives the director the per-node port it needs. Migrated instance `default` keeps `:8581`,
   because the migration copies its server profile's `Port`, so Strata and opencode change nothing.
4. **A save of a profile that running instances use** (an instance profile's model list or server
   profile, a server profile, a model profile listed in one) takes effect only at each instance's next
   start (user, 2026-11-26: they must be restarted). The checks run on the render of every affected
   instance, and one failure refuses the whole save.

   | Option | Cost | Risk |
   | --- | --- | --- |
   | M. The manager restarts each running affected instance itself, as part of the save | No new page action | A save becomes a disruptive act that also needs `control`. It cuts in-flight requests and reloads models (minutes; RS's 10-minute limit can fail a start), on every instance at once. A partial failure leaves some restarted and some not, after a write that already landed |
   | **O. The answer lists the affected running instances; the page offers "Restart" for them, as 17d's restart offer does today** | The save answers `running: [ids]` instead of a bool; the page's offer calls the existing Restart per instance | An administrator who dismisses the offer runs the old settings until the next restart; the offer says so |

   **Decided: O** (user, 2026-11-26), the way every save works today. The administrator chooses when to cut the
   traffic, and the save stays a `configure` write with no service action. A "restart needed" mark that
   outlives the page is a later step, not part of this.
5. Deletable:
   - a **model profile** when no instance profile lists it (replacing "switched off and unassigned");
   - a **server profile** when no instance profile names it (today: no instance);
   - an **instance profile** when no instance names it;
   - an **instance** by today's rule, its instance profile kept for reuse.
6. The pool is one file, `models.ini`. `models.ini.disabled` and the switch go. A created model
   profile lands in `models.ini`, listed in no instance profile, so it renders nowhere. On macOS the
   configurator keeps its switch over its own per-user files, which have no instances.

### Migration and rollback

- **Upgrade from 2.99.0 files.** At the manager's start, before the renders, idempotent and
  render-preserving:
  1. Back up `instances.ini`, `assignments.ini`, `models.ini` and `models.ini.disabled` as
     `<name>.2.99.0`, once.
  2. For each instance without `InstanceProfile`, write an instance profile with the instance's own id
     (`default`). Its `ServerProfile` is the instance's, and its `Models` are the ids that instance's
     current render holds, computed from both pool files and its assignment. That covers `inherit`,
     `*` and `off` exactly.
  3. Write `InstanceProfile = <id>` into the instance's section and drop `ServerProfile`. With P1,
     also copy the server profile's `Hostname` and `Port` into the section (the effective values, so
     an unset `Port` becomes `9931`, `server_cfg::DEFAULT_PORT`), then drop them from the server
     profile.
  4. Append `models.ini.disabled`'s sections to `models.ini`, after the switched-on ones (the render
     order is the pool order, so the render keeps its order).
  5. Remove `assignments.ini` and `models.ini.disabled`.
  - The order keeps every crash point consistent: steps 2 and 3 resolve through the new profile with
    the same ids, and a twin left between 4 and 5 renders the first copy, which has the same bytes.
  - **Done when:** every `models@<id>.ini` is byte-identical before and after, every instance
    resolves the same `server@<s>.ini`, and every launch passes the same `--host` and `--port`.
- **The package pitfall.** llama.cpp's package and MSI copy a missing `assignments.ini` back at their
  next upgrade, because the migration removed it. So the manager must delete a stray
  `assignments.ini` once every instance names an instance profile, and llama.cpp's package must ship
  `instance-profiles.ini` (`[default] ServerProfile = default`, `Models =`) and the new
  `instances.ini` in place of `assignments.ini`. That package change needs a llama.cpp package build
  and install.
- **Downgrade.** A 2.99.0 manager would find no `ServerProfile`, so instance `default` would not
  start. It would also find no `assignments.ini`, so every render would hold `[*]` alone.
  - Recommended: the backups of step 1 plus one documented restore command. The `.deb` allows a
    downgrade; the framework's MSI refuses one.
  - Rejected: keeping `ServerProfile` and `assignments.ini` as mirrors, two sources of truth to keep
    in step.

### The director (step 14), not built

An instance profile is the unit the director deploys. "Profile P on node N as instance X" means:
- copy `server@<s>.ini`, the model sections P lists, P's section, and the model files (through the
  model store);
- then `CreateInstance(X, P)` on N.

Ids are shared across the director's nodes. A node that already holds the same id with different
content refuses rather than renames. The single-node pool file of option A copies as sections; option
B would copy as files. Neither changes this flow.

### Decision (user, 2026-11-26)

A + R + P1 + O:
- **Storage and reference.** The pool file `instance-profiles.ini` in the engine's config folder, one
  section per instance profile with `ServerProfile = <s>` and `Models = a, b`. It is written only by
  manager methods under the `writing` lock. Each instance's section of `instances.ini` holds
  `InstanceProfile = <p>`, `Hostname` and `Port`, plus `Label` as today. `paths::instance_server_profile`
  resolves through the instance profile, and the render reads its `Models`.
- **What goes.** `assignments.ini`, `models.ini.disabled`, the switch, the 17d and 17e methods and
  routes, and the `Hostname` and `Port` keys of a server profile.
- **The migration** follows "Migration and rollback", with backups and a documented restore.
- **The restart** is offered by the page.

Briefs for the designer:
- **16** (17d and 17d2): the instance page's server profile switch becomes the instance profile picker
  and the address fields; 17d2 is withdrawn.
- **22**: the Models list without switches, and the model profile page without its switch.
- **New, 23**: the instance profile list and page (+ and minus over the model profiles, the server
  profile picker, delete), and the restart offer listing every affected running instance.

### Slices (smallest first; Windows type-checked only)

1. **IP1** (`pai-core`, pure, no I/O): `instance_profiles` parse and section edit; `assign::render`
   from an id list; `migrate_model(old texts) -> new texts` (the instance profile per instance, the
   merged pool, the address moved into the record, `assignments.ini` and `.disabled` gone).
   Done when: for fixtures covering `inherit`, `on`, `off`, `* = inherit`, `* = on`, a switched-off
   profile and an unset `Port`, every old render equals the new one byte for byte; the address is the
   effective one (unset `Port` is 9931); a second run changes nothing; a crash-point fixture (profile
   written, record not) completes to the same result.
2. **IP2** (`pai-manager`): the migration at start, before the renders: the `<name>.2.99.0` backups
   once, then writes in the record's crash-safe order, and a stray `assignments.ini` deleted once every
   instance names a profile. Launch, `binding`, `check_address` and GT5's `instance_devices` resolve
   through the record and the instance profile. `WriteFile` of `assignments.ini` and
   `models.ini.disabled` is refused with a key. Done when: the manager tests' fixture install starts
   with identical renders and launch argv (`--host`, `--port`, `--models-preset`); the backups exist
   once; a re-created `assignments.ini` is removed at the next start. **Real-machine check** after the
   deploy and the user's restart of `llama.cpp@default`: `/etc/llama.cpp` migrated, the backups there,
   `default` up on `:8581`, Strata still served.
3. **IP3** (`pai-manager`, `pai-core` client): methods, each with its `As` form, `configure`, one
   action record:
   - `ListInstanceProfiles`, `CreateInstanceProfile(p, server)`, `DeleteInstanceProfile(p)`
     (refused while an instance names it), `SetInstanceProfileServer(p, s)`,
     `AddInstanceProfileModel(p, m)`, `RemoveInstanceProfileModel(p, m)`;
   - `SetInstanceProfile(instance, p)` and `SetInstanceAddress(instance, host, port)`;
   - `CreateInstance(id, p)` with the proposed free port from 8581 (no record holds it, a bind
     test passes).
   - The delete rules change: a model profile when no instance profile lists it, a server profile when
     no instance profile names it.
   - `CreateModelProfile` writes into `models.ini`; the 17d and 17e methods are removed.
   - Every save answers the running instances it affects.
   Done when: each method's success, refusal keys and render effect are tested over D-Bus and the
   pipe; a save failing one affected instance's check writes nothing; two instances sharing one
   profile on different ports both render.
4. **IP4** (`pai-bff`, `fe/briefs/api-contract.md`): `GET|POST /api/profiles/instance`,
   `GET|DELETE /api/profiles/instance/{p}`, `PUT .../{p}/server`, `PUT|DELETE .../{p}/models/{m}`,
   `PUT /api/instances/{id}/profile`, `PUT /api/instances/{id}/address`, `POST /api/instances` taking
   `instanceProfile`. The switch route, `enabled`, the 17d and 17e routes and the server profile's
   `Hostname` and `Port` fields are dropped. The saves answer `running: [ids]`. Done when: each route's
   200 and refusal map are pinned in the bff tests; the removed routes answer 404; the contract matches.
5. **IP5** (`fe`, brief 22): the plain Models list and the model profile page without its switch.
   Done when: the brief's tests are green and the designer's UX check passes.
6. **IP6** (`fe`, briefs 23 and 16): the instance profile list and page, the instance page's picker
   and address, and the restart offer. Done when: the briefs' tests are green and the designer's UX
   check passes.
7. **IP7** (`pai-cli`, `pai-app`; superseded by the amendment's IP7): `pai instance-profile list|show|create|delete|server|
   add|remove` and `pai instance profile|address`; under the manager, `pai model-profile
   enable|disable` and the Models tab's switch are gone (macOS keeps them on its own files); the
   import of `presets.ini.disabled` appends to `models.ini`; the Server tab shows the address from the
   instance. Done when: the CLI and app tests pin each command and the import; the GUI's switch is
   absent under the manager and present on macOS.
8. **IP8** (`xtask`, llama.cpp's `.deb`, `.rpm` and MSI): ship `instance-profiles.ini` (`[default]
   ServerProfile = default`, `Models =`) and the new `instances.ini` (`InstanceProfile = default`,
   `Hostname`, `Port`); stop shipping `assignments.ini`; the server profile default without
   `Hostname` and `Port`. Done when: the package tests pin the files and the absence; the MSI text
   checks pass. **Needs a llama.cpp package build and install** (beyond the framework-only deploy) and
   then the user's restart.
9. **IP9** (docs agent): `AGENTS.md`'s file list, `crates/bff/README.md`, the packaging guide and the
   restore command for a downgrade.

### Amendment: placement and memory per model inside the instance profile (user, 2026-11-26): accepted (user, 2026-11-26)

The user's extension (relayed 2026-11-26): layer distribution, split type and tensor assignment are set
per model inside the instance profile, because an instance profile depends on the hardware it runs on.
They leave the model profiles, which become hardware-independent. IP1 was stopped before it built
anything, so the slices above are revised here once this is accepted.

**The decisions (user, 2026-11-26).** K2: the memory-sized keys move with placement. Unassigned
models' placement goes to a migration-made instance profile `unassigned`. `pai-config` hides its
placement controls under the manager, with a pointer to the web UI (macOS keeps them); superseded the
same day by the GUI's removal. The relaxed
migration check is accepted: the same keys and values per model, in the same section order.

**The key split** (every key of `checks::WEB_MODEL_PROFILE_KEYS` and `MODEL_PROFILE_KEYS_NOT_ON_WEB`,
2026-11-26). A key moves when it says where the weights go or is sized by the memory of the hardware.
- **Moves to the entry `[<p>/<m>]`:**
  - placement: `device`, `split-mode`, `tensor-split`, `main-gpu`, `override-tensor`, `n-gpu-layers`,
    `n-cpu-moe`, `n-cpu-ffn`, `moe-cache-mib`, `op-offload`, `repack`, `no-host`, `lazy-mode`,
    `load-mode`, `fit`, `fit-ctx`, `fit-target`, `device-draft`, `n-gpu-layers-draft`,
    `mmproj-device`, `mmproj-offload`;
  - memory-sized: `ctx-size`, `parallel`, `kv-unified`, `kv-unified-per-slot`, `batch-size`,
    `ubatch-size`, `cache-type-k`, `cache-type-v`, `flash-attn`, `cache-ram`, `ctx-checkpoints`,
    `checkpoint-min-step`, `spec-draft-n-ubatch`, `spec-draft-type-k`, `spec-draft-type-v`;
  - `load-on-startup`: which models an instance preloads is a decision of that deployment, judged
    against its server profile's `ModelsMax` (`load-on-startup-over-max`). This one is my call, not
    stated by the user; it follows their rule (the model profile keeps the file, sampling and
    behaviour).
- **Stays in the model profile:**
  - the file and the assets: `model`, `hf-repo`, `hf-file`, `mmproj`, `model-draft`;
  - sampling: `temp`, `top-k`, `top-p`, `min-p`, `repeat-penalty`, `presence-penalty`,
    `backend-sampling`, `n-predict`;
  - behaviour:
    - chat and reasoning: `jinja`, `chat-template-kwargs`, `prefill-assistant`, `reasoning`,
      `reasoning-format`, `reasoning-effort`, `reasoning-preserve`, `reasoning-budget`,
      `reasoning-budget-message`;
    - embeddings: `embeddings`, `pooling`;
    - caching: `cache-reuse` (a reuse chunk, not a size);
    - vision: `image-min-tokens`, `image-max-tokens` (they size the vision buffer too, but they
      set what the model sees: kept as behaviour);
    - speculation: `spec-type`, `spec-draft-n-max`, `spec-draft-n-min`, `spec-draft-p-min` and
      every `spec-ngram-*` key.
- A key added to the lists later is classed in the same commit as its control (`checks.rs` gains the
  two lists, `PLACEMENT_KEYS` and the rest, and a test that their union is the whole key set).

**File shape.** Options:
- **S1 (decided): sections in `instance-profiles.ini`**, `[<p>]` with `ServerProfile`, and one
  `[<p>/<m>]` section per model entry holding that model's P keys. The id rules of both kinds exclude
  `/`, so the name cannot be misread. Membership is the presence of the entry section, empty when the
  model has no placement. The `Models` key of the accepted design goes, because the sections are the
  list, in their order, and one truth is better than two. + adds an empty `[<p>/<m>]`; minus removes
  it. The director copies `[<p>]` and every `[<p>/*]` together, one file, adjacent sections.
- S2: a second pool, `placements.ini`. Two files to keep in step on every create, delete and copy, for
  nothing S1 lacks. Rejected.
- S3: inline lists in `[<p>]`. Unreadable and unparseable by the INI reader. Rejected.

**The render.** For each entry `[<p>/<m>]`, the section of `models@<id>.ini` is the model profile's
section from `models.ini` (bytes kept) followed by the entry's key lines. The model profile no longer
holds a P key: after the migration the manager refuses a `models.ini` write carrying one
(`model-profile-placement-key [key]`), so no precedence rule is needed. GT5's ROCm `load-mode` default
stays a render rule for an entry that leaves it unset.
- **Byte identity cannot hold.** Moving keys out and appending them changes the key order inside a
  section, and a comment line above a moved key stays behind in `models.ini`. The relaxed check below
  is accepted (user, 2026-11-26).
- llama-server reads a section as a set of keys (`tools/server/README.md`, presets; a duplicate key is
  what order would change, and the refusal above excludes it). So the migration's done-when becomes:
  every rendered section parses to the same key and value set as before (`ini::parse_sections`), and
  the sections come in the same order.

**The migration** extends IP1's function:
1. For each migrated instance profile (one per instance, `default` today) and each model its render
   holds, move that model's P keys from its `models.ini` section into `[<p>/<m>]`.
2. A model held by two instances' renders gets the keys in both entries (copies).
3. A model that no render holds has placement keys that fit no instance profile. Options:
   - (a) drop them, kept only in the `models.ini.2.99.0` backup and named in the journal;
   - **(b, decided)** collect them in a migration-made instance profile `unassigned`
     (`ServerProfile = default`, the only server profile they could have been set against), one entry
     per such model. It is visible, deletable when no instance names it, and keeps tuned but
     switched-off presets usable by copying the entry;
   - (c) keep them in `models.ini`, which breaks the placement-key refusal. Rejected.
   `unassigned` is created only when such a model exists.
4. P keys then leave every `models.ini` section.

**The checks.**
- The device-bound check (`model-profile-device-outside`) becomes local: an entry's device keys
  against its own profile's server profile `Devices`, in the same file. A server switch of the profile
  re-checks every entry.
- The memory check (`check_profile_memory`) keeps judging renders. It now reads the entry's
  placement with the model profile's size and V keys.
- The `n-cpu-moe` range and the tensor-rule checks (`checks.rs`) move with their keys to the entry
  write.
- The model-info read stays on the model profile (it is the model file's facts), so the entry page
  reads `GET /api/profiles/model/{m}/model-info` as the model page does today.

**What changes beyond the accepted slices.**
- **Manager:** `SetInstanceProfileModel(p, m, values, revision)` for an entry's P keys, with its `As`
  form and the checks above. `AddInstanceProfileModel` adds an empty entry. `CreateModelProfile` with
  `from` copies B and V keys only.
- **bff:** `GET|PUT /api/profiles/instance/{p}/models/{m}` (the P keys and a revision). `PUT
  /api/profiles/model/{id}` refuses a P key with 422 and `field`.
- **Web UI:**
  - The model profile page loses the GPU distribution, Tensor placement and Hardware cards and the
    vision placement keys (briefs 02e and 22).
  - A per-model entry page under the instance profile, `#/profiles/instance/<p>/models/<m>`, takes
    those cards (briefs 14 GPU distribution, 19 layer sliders and 21 tensor placement move there).
  - The new instance profile brief is **26**, since 23 is "collapsible cards".
- **`pai-config` and `pai`** (superseded 2026-11-26: `pai-config` is removed in 2.99.1, so nothing is
  hidden or kept on macOS; "Removing pai-config"): under the manager, the Models tab's placement controls (the GPU split
  table) leave the model profile. Recommended: hidden there, with a hint that placement is set on the
  instance profile in the web UI (or `pai instance-profile placement <p> <m> key=value`). macOS keeps
  them on its own `models.ini`, which has no instances. Decided (user, 2026-11-26): hidden, with the
  pointer.
- **Benchmark sweeps.** `pai-app` `bench/sweep.rs` writes the swept key into `models.ini`. A sweep of a
  P key (`tensor-split`, `n-cpu-moe`, `override-tensor`) must write the instance profile's entry
  instead, through the manager, and put it back. The `optimize-preset` skill then tunes an entry, not
  a model profile section.

**Slices** (these replace IP1 to IP9 above; smallest first; Windows type-checked only):
1. **IP1** (`pai-core`, pure):
   - `instance_profiles` with the `[<p>]` and `[<p>/<m>]` shape;
   - `PLACEMENT_KEYS` and the stay list, their union tested to be the whole key set;
   - the render: the model section followed by the entry;
   - the migration function: the instance profile per instance, the moved keys per held model,
     copies for a model held twice, `unassigned` for the rest, the merged pool, the address moved
     into the record.
   Done when: each rendered section parses to the same key and value set as before, in the same
   section order; a second run changes nothing; a crash-point fixture completes.
2. **IP2** (`pai-manager`):
   - the migration at start, with the backups and the stray `assignments.ini` rule;
   - the resolution through the record (launch, binding, GT5);
   - `WriteFile` of `assignments.ini` and `.disabled` refused, and a `models.ini` write carrying a
     moved key refused (`model-profile-placement-key [key]`).
   Done when: the fixture install starts with equal renders and launch argv. **Real-machine check**
   after the deploy and the user's restart: `default` up on `:8581`, Strata served, the backups and
   `unassigned` present.
3. **IP3** (`pai-manager`, client):
   - the instance profile methods; `SetInstanceProfileModel(p, m, values, revision)` with the moved
     keys' checks (ranges, tensor rules, the local device check against the profile's `Devices`,
     memory on every affected render); `SetInstanceProfile`, `SetInstanceAddress`;
   - `CreateInstance` with a free port from 8581; the delete rules; `CreateModelProfile` into
     `models.ini`, copying the stay keys only; saves answer the affected running instances.
   Done when: each method's success, refusals and render effect are tested over D-Bus and the pipe.
4. **IP4** (`pai-bff`, contract; reconciled with briefs 26 section 0 and 22 IP5.0, 2026-11-27).
   The routes, each over the IP3 method named:
   - `GET /api/profiles/instance` (`instance_profiles`): rows `{id, server, models, usedBy}`.
   - `POST /api/profiles/instance` `{id, server}` (`create_instance_profile`): 201.
   - `GET /api/profiles/instance/{p}` (`instance_profile`): `{id, server, models, usedBy, revision}`.
   - `DELETE /api/profiles/instance/{p}` (`delete_instance_profile`): 204, or 409
     `instance-profile-in-use`.
   - **`POST /api/profiles/instance/{p}/models` `{model}` adds an entry**
     (`add_instance_profile_model`).
   - `DELETE .../models/{m}` removes one (`remove_instance_profile_model`).
   - **`PUT .../models/{m}` `{values, revision}` only saves an entry's keys** (IP3b's
     `SetInstanceProfileModel`). The single PUT for both is withdrawn: the brief's split is right, an
     add creates and a save replaces.
   - `PUT .../server` `{server}` (IP3b's server switch).
   - `GET .../models/{m}`: `{profile, server, model, values, revision}`.
   - `PUT /api/instances/{id}/profile` `{instanceProfile, revision}` (`set_instance_profile`).
   - `PUT /api/instances/{id}/address` `{hostname, port, revision}` (`set_instance_address`).
   - `POST /api/instances` `{id, instanceProfile}`: 201 `{id, port}`.
   - The model profile rows of `GET /api/profiles/model` and the page `GET /api/profiles/model/{id}`
     gain **`usedBy`**, the instance profiles that list the model, in pool order. The bff computes it
     from `instance_profiles`, and it matches the delete refusal `model-profile-in-use`.
   - `PUT /api/profiles/model/{id}` refuses a moved key: 422 `model-profile-placement-key`, `field`.
   - The switch route, `enabled`, `assignments` and the 17d and 17e routes are removed (404).

   **Writes that act at once (the brief's page has no Save bar).**
   - Add, remove and the server switch carry **no revision**, as IP3a's add and remove take none.
     Each is one set-like change refused on its own conflict (`instance-profile-model-listed`,
     `no-instance-profile-model`, `model-profile-device-outside`), so a stale page loses nothing by
     acting, and a revision would only turn a concurrent unrelated edit into a 409.
   - Each answers `{running: [<instance id>…]}`, the running instances that use the profile. The page
     keeps one restart offer and updates its list after each change, then re-reads the profile.
   - Only the entry save (`PUT .../models/{m}`), the instance's profile and its address carry a
     revision: each replaces values the page read.
   - **The revision on the wire is a hash, not the text.** IP3a's revision is the whole pool text
     (`instance_profile` answers it). The bff answers `revision` as a SHA-256 hex of the profile's
     own sections (`[<p>]` and its `[<p>/*]`), so an edit of another profile does not stale this
     page. On a write it reads the pool again, compares that hash, and hands the text it read to the
     manager, which compares the whole text under the `writing` lock. A change elsewhere in that
     short window is a rare 409 `stale-revision`, read again.
   - The instance's own revision is the same over `instances.ini`'s section of that instance.

   Done when: every route's answer and refusal is pinned in the bff tests; the removed routes answer
   404; a write with no revision is accepted where none is asked and refused 422
   `model-profiles-malformed` where one is; `usedBy` is pinned on both model profile reads.
5. **IP5** (`fe`, briefs 22 and 02e): the plain Models list; the model profile page without the
   switch and without the moved keys' cards and fields. Done when: the brief's tests are green and
   the designer's UX check passes.
6. **IP6** (`fe`, new brief 26 and brief 16, with 14, 19 and 21 moved): the instance profile list and
   page, the per-model entry page with the placement and memory cards, the instance page's picker,
   address and restart offer. Done when: the briefs' tests are green and the UX check passes.
7. **IP7** (`pai-cli`, `pai-app`; the GUI part dropped, user 2026-11-26: `pai-config` is removed in
   2.99.1, "Removing pai-config"):
   - `pai instance-profile ...` including `placement <p> <m> key=value`;
   - `pai model-profile enable|disable` removed;
   - the import appending to `models.ini`.
   Done when: the CLI and app tests pin the commands and the import.
8. **IP7b** (`pai-app` bench): a sweep of a moved key writes and restores the instance profile entry
   through the manager and leaves `models.ini` untouched. Done when: the sweep tests pin it.
   **Real-machine check**: one short sweep on the GPUs with the user's go-ahead.
9. **IP8** (`xtask`, llama.cpp's packages and MSI): the new default files and no `assignments.ini`.
   **Needs a llama.cpp package build and install**, then the user's restart.
10. **IP9** (docs agent): `AGENTS.md`, `crates/bff/README.md`, the packaging guide, the restore
    command, and the `optimize-preset` skill (it tunes an entry now).

### Convergence: idempotent apply, drift and the last good render (user, 2026-11-27): accepted (user, 2026-11-27)

**The requirement** (user, 2026-11-27). Applying a profile to an instance is idempotent. When a
model profile or a server profile that an instance profile uses changes, every node that uses that
instance profile checks the conditions and applies the deltas to reach the final state. The user
chose:
1. Re-render at once on every node; the same profiles in always give the same files out. A running
   instance shows **pending restart** (drift), and the page, or later the director, offers Restart.
   Nothing restarts on its own.
2. A node where the changed profile no longer fits (GPUs or memory fail the checks) refuses the new
   render, keeps running the **last applied** one, and reports the instance **not converged** with
   the check's reason. The save still lands where it fits.

**Checked on this node (2026-11-27, the tree after IP1, IP2 and IP3a).**
- A model profile save re-renders every instance whose instance profile has an entry for it:
  **met**. Every write goes through `check_and_write` and then `render_all_locked` (`service.rs`
  3108), which renders every instance from its instance profile. A render whose text is unchanged is
  left as it stands.
- A server profile save re-renders every instance whose instance profile names it: **met for what
  is rendered** (GT5's `load-mode` follows `Devices` through the same `render_all_locked`). The
  server profile itself is not rendered: the launch reads `server@<s>.ini` at start.
- Both answer the affected running instances: **not met**.
  - The model profile save's `running`/`restart` comes from `pai_core::assign::rows` over
    `assignments.ini` (`crates/bff/src/model_profiles.rs` 260, 612), a file IP2's migration removes,
    so after the migration it names no instance.
  - The server profile save answers one instance, the profile's single holder
    (`crates/bff/src/server_profiles.rs` 20-27), the exclusivity rule P1 removed.
  - This is a regression of the restart offer from IP2 on, and it must be fixed in 2.99.1.
- Today a fit failure on any instance refuses the whole save (`check_contents` judges every render),
  which is the opposite of choice 2.
- llama-server reads `models@<id>.ini` only at its start or on an explicit `GET /models?reload=1`
  (`tools/server/README.md`, "GET /models", llama.cpp v1.0.0), so a re-render changes nothing in a
  running instance until it restarts.

**The design.**
- **Two checks, two outcomes.**
  - *Intrinsic* checks judge the profile alone: the section rule, ids, key types and ranges, the
    tensor-rule syntax, the placement-key refusal. They refuse the save, as today.
  - *Fit* checks judge one instance on this node: the model files present, the memory estimate,
    `ModelsMax` and `load-on-startup`, the entry's devices against the server profile's `Devices`,
    the address. They no longer refuse the save. They decide, per instance, whether its new render is
    written.
- **The rendered pair is the last good state.** Each instance gets two rendered files beside
  `models@<id>.ini`: the existing one, and **`launch@<id>.ini`**. The latter is the server profile's
  text with the instance's address, which the launch reads in place of resolving `server@<s>.ini`
  (`launch::instance_command`, falling back to the resolution before the first render). After any
  write, `render_all_locked` builds both texts for every instance:
  - Fit passes: both are written (unchanged text still left alone).
  - Fit fails: neither is written. The pair on disk stays the last good one, and the instance is
    **not converged** with the first failing check's keyed refusal.
  - Same inputs, same pair: the render is a pure function of the profiles.
- **Not converged is derived, not stored.** It lives in the manager's memory, recomputed at its
  start (which renders every instance) and at every write. A file would only add a second truth.
  `ListInstances` gains `converged` (bool) and `reason` (`{key, args}` or null).
- **Drift (pending restart) is derived too.** `pai-manager run-instance` writes the SHA-256 of the
  pair it launches with into the instance's state folder (`.../state/<id>/applied`), which the
  instance's account can write and the manager can read (Linux before the exec; Windows the parent).
  `ListInstances` gains **`pendingRestart`**: the instance is running and the hash of the pair on
  disk differs from `applied`. A change reverted before a restart therefore shows no drift. Boot
  starts by systemd or the SCM need no manager bookkeeping.
- **The save's answer.** Every save that can re-render (model profile, server profile, instance
  profile and its entries, the instance's profile and address) answers `{running: [<instance id>…],
  notConverged: [{instance, key, args}…]}`. `running` lists the affected instances that are up and
  now drift. `notConverged` lists the affected ones that kept their last good pair. The manager
  computes both, not the bff (`assign::rows` leaves the bff).
- **The page.** The instance rows (Overview tile, instance page) show a "Pending restart" mark with
  the existing Restart, and a "Not converged" mark with the reason's text and a link to the instance
  profile. A save's status lists both sets, and the restart offer covers `running`. Wording is the
  designer's (briefs 16, 26 and the Overview's).
- **Later, not now.** A model-only change could apply without a restart through `GET
  /models?reload=1`, which unloads only the changed running models. It is one more apply mode to
  test; it waits for a need.

**Slices.**
- **IP3b** (building): the entry save, the server switch, and also the model and server profile saves
  answer `running` from the manager over the instance profiles. **Must land in 2.99.1** (it fixes the
  regression).
- **IP3c** (new, `pai-core` and `pai-manager`, test first):
  - the intrinsic and fit split in `check_contents`;
  - `launch@<id>.ini` rendered and read by the launch;
  - per-instance writes in `render_all_locked`, with not-converged in memory;
  - the `applied` hash from `run-instance`;
  - `ListInstances`' three fields; the saves' `notConverged`.
  Done when:
  - a model profile save that fits instance A and fails B's memory check lands: A's pair rewritten,
    B's pair byte-identical, B reported not converged with the memory refusal;
  - a later save that fits B converges it;
  - a running instance whose pair changed reads `pendingRestart`, and one whose change was reverted
    does not;
  - a manager restart reports the same states.
  Recommended for 2.99.1: the release changes the data model and the save semantics now.
  **Real-machine check:** after the deploy, a model profile save on this machine shows "pending
  restart" on `default` until the user restarts it.
- **IP4**: the instance rows gain `converged`, `reason`, `pendingRestart`; every re-rendering save
  answers `running` and `notConverged`.
- **IP6** (briefs 26, 16 and the Overview): the two marks and the status lists.

## Removing pai-config (user, 2026-11-26/27): accepted (user, 2026-11-27)

The requirement (user, relayed 2026-11-26, narrowed 2026-11-27): the native GUI `pai-config` is
deprecated and removed from every OS in 2.99.1. The release follows Linux first and Windows second.
macOS is an advanced placeholder: after the removal it keeps only `pai`, which still runs llama-server
itself there over per-user files, until the manager and bff port lands. The port is sketched at the
end of this section and blocks no release.

### What exists (checked 2026-11-27)

- **Workspace**: `crates/config` (`pai-config`, Slint, the tray, the e2e tests that need Slint debug
  info) is a member in `Cargo.toml`, and only it links Slint. `crates/config/README.md` holds every
  crate's source map and conventions; `crates/config/AGENTS.md` the GUI's checklists.
- **Shared code**: `pai-app` holds the configurator's logic for the GUI and the CLI. `pai-core`'s
  `crates/core/strings/en_US.toml` is the GUI's strings file, shipped as
  `share/strings/en_US.toml` with the GUI component (`components.rs` 401); `cli.toml` is the CLI's.
- **The icon**: `resources/llama.ico` is generated by `crates/config`'s `build.rs` (README "The build
  script"), and both MSIs use it.
- **xtask**: `Component::Config` (`components.rs`), `common/configurator.rs` (builds the four
  binaries), the Linux, Windows and macOS package and install code, `licenses.rs`, `names_lint.rs`,
  `ci_macos.rs` and the MSI tests all name it.
- **Windows**: the framework MSI ships `pai-config.exe`, a Start Menu shortcut feature
  (`private-ai-framework.wxs` 300-327), and dialog text naming the configurator (462).
- **Per-user state the GUI made**: the start-at-login entries (HKCU Run `org.privateai.Config`,
  `~/.config/autostart/org.privateai.Config.desktop`), `settings.ini` (which `pai` also reads:
  `BenchDb`, `BenchPromptFile`), and `bench/`.
- **macOS CI** (`installers-macos.yml`, `ci_macos.rs`) checks four `.pkg` receipts and that a
  reinstall restores a removed `pai-config`.

### Decisions (user, 2026-11-27)

The user accepted the removal and answered the three questions: `pai` removes the stale start-at-login
entries it can see; the Windows Start Menu opens the web UI at the bff's configured address; the user
starts the Windows and macOS CI runs for GR4 and GR5.

- **The Start Menu shortcut** targets `pai.exe web` (`Show="minimized"`, so the console never covers
  the desktop). The MSI does not learn the address: an address read at install (an `IniFileSearch`
  of `bff.ini`) goes stale when an administrator changes `[Bff] Listen`, and a wildcard host
  (`0.0.0.0`, `[::]`) is no address a browser should open. `pai web` reads `[Bff] Listen` from the
  framework's `bff.ini` at each click (`bff_cfg::listen`). The default is `127.0.0.1:8590`, and a
  wildcard host becomes loopback with the same port. It opens `http://<address>/` with the system's
  handler (`ShellExecuteW` on Windows, `xdg-open` on Linux), so the same command serves Linux too.

- **What `pai-app` keeps**: what `pai` reads (`model_profiles`, `server_control`, `migration` and the
  import, `bench`, `settings`, and whatever else a reference audit finds); everything only the GUI
  reads goes. On macOS `pai` keeps its own llama-server path untouched.
- **Strings**: every `en_US.toml` key a non-GUI reader fills moves to `cli.toml`; then
  `en_US.toml`, its shipped copy, `Reader::Gui` and the AGENTS.md em-dash exception (`gpu_split.rs`,
  `components.slint`, `binding_lint.rs`) go.
- **The icon**: generated by xtask (the same script, `resources/generate-llama-ico.mjs`) before the
  crate goes, since both MSIs still need it.
- **Upgrades remove the old GUI**:
  - Linux: `-config` is no longer built; `-core` gains `Conflicts:` plus `Replaces:` (deb) and
    `Obsoletes:` (rpm) on `private-ai-framework-config`, so the package manager removes it (Debian
    Policy 7.4 and 7.6, https://www.debian.org/doc/debian-policy/ch-relationships.html; RPM
    dependencies, https://rpm-software-management.github.io/rpm/manual/dependencies.html).
  - Windows: MajorUpgrade removes the previous version's files first, so the new MSI simply
    carries no `pai-config.exe` and no shortcut feature.
- **Start-at-login entries left behind**: `pai` removes the ones it can see (user, 2026-11-27): at
  each start, the running user's `~/.config/autostart/org.privateai.Config.desktop` (under
  `$XDG_CONFIG_HOME` when set) on Linux, and the HKCU Run value `org.privateai.Config` on Windows.
  Each is removed only when it names `pai-config`. A missing entry or a failed removal is silent and
  never stops the command. Other users' entries stay until they run `pai`; the release notes say so.

### Slices (smallest first; L = checked on Linux, W = Windows CI, C = macOS CI)

1. **GR1** (L, docs agent and xtask): move the crate source map and conventions from
   `crates/config/README.md` to `crates/README.md`; move the icon generation into xtask. Done when:
   `xtask build framework` regenerates a missing `llama.ico` with no `crates/config`, and the map is
   in its new place.
2. **GR2** (L, `pai-core`, `pai-app`, `pai-cli`): the reference audit. GUI-only modules out of
   `pai-app`, the shared keys moved to `cli.toml`, `en_US.toml` and `Reader::Gui` out of `pai-core`.
   Done when: `pai-app`, `pai-cli`, `pai-core`, `pai-manager` and `pai-bff` build and their tests pass
   with no `en_US.toml`; the strings tests pin that every key a remaining reader fills is in
   `cli.toml`.
   - **GR2b** (L, `pai-cli`, test first): the stale-entry removal above and `pai web`. The tests
     redirect the data tree (`PRIVATE_AI_DATA_ROOT`, `XDG_CONFIG_HOME`) and put the registry behind a
     seam. Done when:
     - an autostart file naming `pai-config` is removed, one naming anything else is kept, and a
       missing one is silent;
     - the Windows path is type-checked with its seam tested;
     - `pai web`'s address is pinned for the default, a set `Listen`, `0.0.0.0:p` and `[::]:p`, with
       the opener behind a seam.
3. **GR3** (L, workspace and xtask Linux): `crates/config` and Slint out of the workspace;
   `Component::Config` out; `configurator.rs` builds three binaries; the Linux packages without
   `-config`, `-core` with the relations above; `names_lint`, `licenses` and the package tests
   updated. Done when: the xtask tests pin the three binaries, the absent package and the relations.
   The coordinator's deploy then installs the core, bff and cli packages only. **Real-machine
   check** on this machine at that deploy: the old `-config` package is removed by the upgrade.
4. **GR4** (L then W, xtask Windows and the MSI): the `pai-config.exe` file and the dialog text out of
   `private-ai-framework.wxs`; the Start Menu shortcut kept, now targeting `[BINFOLDER]pai.exe` with
   `Arguments="web"` and `Show="minimized"`, named for the web UI; the MSI text tests and
   `two_installers` updated. Done when: the MSI text and type checks pass on Linux, and the Windows
   CI's silent install, repair and uninstall pass (the user starts the run).
5. **GR5** (L then C, xtask macOS): the `-config` component out of the `.pkg` set (three receipts),
   `ci_macos.rs` and `installers-macos.yml` checking that a reinstall restores `pai` instead of
   `pai-config`. Done when: the text tests pass on Linux and the macOS CI run is green (the user
   starts it).
6. **GR6** (docs agent): AGENTS.md (overview, map, Commands: `pai-config` no longer a crate, the
   Slint debug-info and Windows-check notes), `docs/build-*.md`, `docs/build-macos.md`, the release
   skill's package list, the playbooks' GUI notes, and the release notes line about the
   start-at-login entries.

GR1 and GR2 can run beside the instance profile slices; GR3 should follow IP7, so the CLI holds the
instance profile commands before the GUI goes.

### Sketch: the manager and the bff on macOS (Proposed, not accepted; no slices in the release order)

A placeholder for later, nothing scheduled:
- **Instances**: one launchd job per instance, written by the manager
  (`/Library/LaunchDaemons/org.privateai.llama.<id>.plist`, `UserName _privateai`, `pai-manager
  run-instance <id>`), behind a `ServiceControl` over `launchctl`. `Stop` saves the slots itself,
  since launchd has no `ExecStop`.
- **Daemons**: `org.privateai.manager` (root at first) and `org.privateai.bff` (`_paibff`).
- **Paths**: `/Library/Application Support/llama.cpp/config/` for the engine,
  `/Library/Application Support/private-ai/` for the framework and the models, and
  `/Library/Logs/llama.cpp/` for the logs.
- **Permission**: the Windows pipe's framing over a Unix socket. The manager judges `getpeereid`'s
  uid by group membership through `mbr_check_membership`: `control` for `admin` or `pai`,
  `configure` for `pai`, `As` only from `_paibff`.
- **Sign-in**: PAM through `libpam.2.dylib`.
- **Devices**: Metal rows (`MTL0`) through the existing probe parse; NodeInfo from `sw_vers` and
  `sysctl`.
- **Unverified until a macOS CI run**: every launchctl flag, PAM and Metal on the runner. A real Mac
  is needed for a model on Metal.

## Part 2: the director (`PLAN.md` step 14, R13)

The user's words (2026-11-15): "Ci potrebbe essere un componente super partes tipo un supermanager,
installato su un server differente che può governare differenti manager. Se il bff si connette a questo
gli viene offerta anche la gestione dei nodi." Status: **accepted** by the user on 2026-11-15, who
answered the four questions at the end with the recommended option each. Renamed **the director**
(user, 2026-11-21); the links were revised the same day to a private CA
("Identity and joining"). Revised again on 2026-11-22 (user): one codebase and one binary, the director
being **a separate instance of `pai-manager` with another role**, not a crate or binary of its own:
"intendo tenere unito il codice ma il director è un'istanza separata del manager con un ruolo diverso",
and "se un manager viene promosso a director può connettersi ai manager invece che al servizio" ("One
binary, two roles").

### Decision summary

1. **A separate service of the same binary**: `pai-manager director`, the unit
   `private-ai-director.service` under its own unprivileged account `private-ai-director`, holding the
   node registry and the one key the nodes trust in its own state directory. It runs no engine and
   checks no content: each node's manager stays the authority over its machine. It never runs inside
   the node's manager process (root or LocalSystem today, its own unprivileged account once "The node's
   manager without root or LocalSystem" is built): the role is the subcommand the unit starts, and a
   director started as root refuses to run.
2. **Nodes are managers.** The director speaks to each node's `pai-manager` with **the existing
   manager protocol over TLS 1.3 with mutual authentication**, every certificate issued by **a
   private CA the director holds** (90-day leaves, renewed by the nodes from 30 days before expiry; revoked by refused renewal and
   a deny list). No new protocol, no public CA, no D-Bus off the machine.
3. **A node joins by a one-time code** its own administrator creates, carrying the node's key
   fingerprint and a secret; the node sends a CSR bound to the code and receives its certificate and
   the CA.
4. **Rights are delegated, not mapped**: joining a node grants the director `control` and
   `configure` on it; the director admits only its local bff, which vouches for a signed-in `pai`
   member of the director's machine. Each forwarded call names that person for the node's log.
   Per-node rights and a central identity wait.
5. **The bff gains a hub mode** (`bff.ini`), where it talks to the director instead of the local
   manager and the API grows a node prefix; the local mode stays as it is.
6. **Nodes never depend on the director**: their instances, boot settings and local clients work
   with it down or gone.
7. **A node is promoted, not reinstalled** (user, 2026-11-22): `-core` ships the director's unit
   disabled; `pai node promote` on a node enables it, joins that machine's own manager to it over
   loopback TLS like any other node, and switches the local bff to hub mode; `pai node demote` undoes
   the switch and stops the director, keeping its state.
8. **No node manager runs as root or LocalSystem** (user, 2026-11-22; accepted the same day): its own account on
   each OS, polkit or per-service DACLs scoped to the instances, and a small privileged helper keeping
   only what cannot be delegated (a password check on Linux, boot on and off, service creation on
   Windows), all before the manager listens on the network.

### Role and boundaries

| | Manager (on each node) | Director | bff |
| --- | --- | --- | --- |
| Runs as | `pai-manager serve`, unit `private-ai-manager.service`, root today, `private-ai-manager` as accepted (Windows: the service `private-ai-manager`, LocalSystem today, `NT SERVICE\private-ai-manager` as accepted) | `pai-manager director`, unit `private-ai-director.service`, the system user `private-ai-director` (Windows: later) | `pai-bff`, unit `private-ai-bff.service`, the system user `private-ai-bff` |
| Owns | The instances, their files, the checks, local authorization | The node registry (name, address, certificate serial and expiry), the CA and its keys, the deny list, the join and renewals, fan-out with per-node timeouts | Web sessions, the HTTP API, the web UI |
| Never | Knows the director's users; accepts a password from off the machine | Caches a node's files as truth; runs checks; starts engines; sees passwords | Holds the node key; reaches a node directly |
| Talks to | systemd / SCM; local callers; the enrolled director over TLS | Nodes over TLS; its local bff over a local socket | The local manager (local mode) or the director (hub mode) |

The director is separate from the bff (rather than a "hub mode" holding the registry inside the
bff) for one security reason and one operational one: the browser-facing process never holds the key
every node trusts, so a compromised bff can act only while it is compromised and only as a signed-in
session could, and cannot carry the key away; and the director can be restarted, upgraded or
backed up without the web service. Cost: a third local service and a small local interface. The same
two reasons keep it out of the node's manager process: the process facing the network with the fleet's
key is not root, and a fault in the director (a parser, the store's HTTP) cannot take the machine's own
instances with it. One binary changes neither: what separates them is the process, its account and its
files, not the crate.

### One binary, two roles (user, 2026-11-22)

**What stays from the accepted design.** The director is its own process, unit and account
(`private-ai-director`), with its own state (`/var/lib/private-ai-director/`: the CA key 0600 in a 0700
directory, the registry, the deny list, the store) that no other account reads; the node's manager
never opens that directory and never holds the CA key; the machine's own manager stays a node and joins
its director over TLS like any other (loopback, one code path); nodes never depend on the director.
**What moves**: the code, from a planned crate `pai-director` into `crates/manager` as a role of
`pai-manager`; the packaging, from a planned package `private-ai-framework-director` into `-core`.
**What is removed**: the separate binary and package.

**How the role is chosen.**

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. A subcommand, `pai-manager director`**, beside `serve`, `run-instance`, `save-slots` (`crates/manager/src/manager.rs` `parse_args`) | One arm in `parse_args`, one unit | None new: the role is fixed by the unit's `ExecStart=`, before any file is read | **Chosen**: the pattern the binary already has (`run-instance` is already the same binary under another account) |
| B. A flag, `pai-manager serve --role director` | The same, plus a flag parser for one value | `serve` grows two meanings; a drop-in that edits arguments flips roles | No gain over A |
| C. A key in a config file (`[Manager] Role = director`) read by `serve` | A file to own and a reload path | The role would follow a writable file: one write and the root process takes the director's role (the CA key in a root process, against the reasons above) | Rejected |

Fail closed at start: `pai-manager director` exits with an error when its effective uid is 0 (Windows,
when later: when its token is LocalSystem), and `serve` never reads the director's state directory, so
neither process can be made to take the other's role by a unit drop-in alone. Unit and names:

| | Linux | Windows (later: the director is Linux only at first, answer 4) |
| --- | --- | --- |
| Service | `private-ai-director.service`, `ExecStart=/usr/libexec/private-ai/pai-manager director` | `private-ai-director`, `pai-manager.exe director`, manual start |
| Account | `User=private-ai-director` (a static sysusers entry, preference: a stable owner of the CA key across restarts and backups, as for the bff), hardened like the bff (`ProtectSystem=strict`, `NoNewPrivileges=`, no capabilities) | the virtual account `NT SERVICE\private-ai-director` |
| State | `StateDirectory=private-ai-director`, `StateDirectoryMode=0700` | `%ProgramData%\private-ai-director\`, SYSTEM and the virtual account only |
| Local interface | `RuntimeDirectory=private-ai-director`, the socket `director.sock` mode 0660 group `private-ai-bff` | a named pipe `\\.\pipe\private-ai-director`, ACL the bff's account and SYSTEM |
| Listeners | the store and renewal listener, 8593 | the same |

**Packaging.**

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. `-core` ships the director's unit disabled** (no preset, no enable in the postinst) and its sysusers entry | Every node carries one unit file and one idle system account | Nothing runs and nothing listens until a promote; the account owns nothing until then | **Chosen**: the binary is in `-core` anyway, and "promote" then needs no package install, only a `configure` call |
| B. A package `private-ai-framework-director` with only the unit and the sysusers entry | A fifth package, its version tied to `-core` | Promote becomes an install (an action with real effects, run by the user); a package mismatch is one more skew | Only if the user wants no director account on plain nodes |

**Promote.** `pai node promote [--name <node>]` (and the same in the GUI later), a `configure` call to the
local manager (a `pai` member, polkit `auth_self_keep` as for any `configure`), which, in order,
each step idempotent so a failed promote is resumed by running it again:

1. enables `private-ai-director.service` (through the helper's `SetDirector` once the manager is
   unprivileged) and starts it; at its first start the director creates the CA,
   its own leaf and the signer (the profile above) and opens its socket;
2. enables the node's own listener on `127.0.0.1:8592` when it is not already enabled (`pai node enable`'s
   key and join code, kept in the manager's memory, never printed);
3. enrols itself: `AddNode{name, 127.0.0.1:8592, code}` on the director's socket, with the promoting
   user as principal; the director dials the node and runs the join exactly as for a remote node. The
   name defaults to the host name held to the registry rule;
4. writes `Director = true` into `bff.ini` (`/etc/private-ai/`, the manager's configuration directory)
   and restarts `private-ai-bff.service`, last, so the web UI never shows a hub without its own node.

A machine already joined to another director is refused (`node-has-director`: one director per node);
`pai node leave` first.

**Demote.** `pai node demote` (`configure`): writes `Director = false` and restarts the bff first (the web
UI goes back to the local machine), then stops and disables the director. The director's state is kept,
so a later promote restores the same CA and every node stays joined; the local node stays joined too
(`pai node leave` forgets it). Deleting the state, which makes every node join again, is a manual act on
the director's machine, not an option of demote (preference: no one-word command that unjoins a fleet).
Moving the director to another machine is `ca export` there, promote and `ca import` here.

**Who may call the director's socket.** The bff's uid (each call naming a session's principal) and
the manager's (for promote: uid 0 today, `private-ai-manager` once the manager drops root; root already
reads the CA key, so admitting it adds no power), both by `SO_PEERCRED`; everyone else is refused before a
frame is parsed.

**What is shared and what is one role's** (all in `crates/manager`, plus the client in `pai-core`):

| Shared | Node only (`serve`) | Director only (`director`) |
| --- | --- | --- |
| The protocol: frames, methods, errors (`manager::pipe` and its dispatch), `pai_core::client::pipe::PipeClient`; the TLS configuration (TLS 1.3, ALPN, the name rules under `pai.invalid`, the certificate profile checks); the signed-record format (built by the director, verified by the node) | The listener on 8592 and its caller kind, the join responder, the key and CSR, renewal as a client, the Authority, systemd and the SCM, the store's pull client | The CA, issuing and the deny list (CRL), the registry, the fan-out with per-node timeouts, the local socket, the renewal and store listener on 8593, the signer |

A module boundary (`manager::director`) keeps the director's code apart; the node's code never calls it.
Cost of one binary, accepted: the node's binary carries the director's code and its crates (rcgen's
issuing half, the store's HTTP server), a larger binary on every node; its security effect is nil, since
code a process never runs grants nothing, and the keys stay where the accounts put them.

### The node's manager without root or LocalSystem (user, 2026-11-22): accepted (user, 2026-11-22)

The user's words (2026-11-22): the node's manager should not run as root nor as LocalSystem, "lo farei
sia per Linux che per Windows", before the manager opens to the network (slice 2 below). Status:
**accepted** by the user on 2026-11-22, the defaults taken; no migration (pre-release: the packages are installed fresh).

**Why it runs privileged today, and what each reason becomes.** The reasons are the unit's
(`xtask/src/linux/system_files.rs` `manager_unit` and its test) and the calls the code makes
(`crates/manager/src/manager/dbus.rs`, `accounts.rs`, `windows/scm.rs`, `windows/logon.rs`):

| What needs privilege today | Linux | Windows |
| --- | --- | --- |
| Write the instance files and `bff.ini` | `/etc/llama.cpp` and `/etc/private-ai` owned by the manager's account (below) | `%ProgramData%\llama.cpp\config\` and `%ProgramData%\private-ai\config\` granted to the manager's service SID |
| Start, stop, restart, reset an instance | polkit rule on `manage-units` for `llama.cpp@*.service` only (below) | per-service DACL on `llama.cpp_<id>`: start, stop, query |
| Enable or disable an instance at boot | **the helper** (no polkit detail exists for it, below) | **the helper** (`SERVICE_CHANGE_CONFIG` is admin-equivalent, below) |
| Create and delete an instance service | nothing: a template unit `llama.cpp@.service` needs no creation | **the helper** (`SC_MANAGER_CREATE_SERVICE`, below) |
| Check a caller with polkit | the `org.freedesktop.policykit.owner` annotation on our two actions | nothing: the pipe client's token is read at identification level |
| Check another user's password (`Authenticate`) | **the helper** (pam_unix refuses it unprivileged, below) | stays in the manager: `LogonUserW` needs no privilege |
| Read model files at any path a `pai` member names | what the account can read: the instances' reach (below) | the same |

The instances keep their own accounts as they are (`private-ai` on Linux, `NT SERVICE\llama.cpp_<id>` on
Windows), and so does `run-instance`.

#### Linux

**The account.** A static system user `private-ai-manager` (sysusers.d in `-core`), member of
`private-ai` (to give the files it writes that group: chown(2), "the owner of a file may change the group
of the file to any group of which that owner is a member", as `service.rs` already does) and of `render`
and `video` (the Vulkan probe child, `vulkan_child.rs`). Static, not `DynamicUser=`, for the reason the bff
already records: dbus-daemon drops a dynamic user at Hello (the bff unit's comment in `system_files.rs`).

**The configuration directories.** systemd makes a `ConfigurationDirectory=` owned by root whatever
`User=` says (systemd.exec(5), quoted in the unit's test), so the manager can no longer write
`/etc/llama.cpp` or `/etc/private-ai` through it. `-core` ships tmpfiles.d lines
`d /etc/llama.cpp 0755 private-ai-manager private-ai -` and `d /etc/private-ai 0755 private-ai-manager
private-ai -` (tmpfiles.d(5): `d` adjusts the mode and ownership of an existing directory), the unit
dropping `ConfigurationDirectory=` for `ReadWritePaths=`. Files the engine package copied in as root stay
readable (group `private-ai`) and are replaced by rename, which needs only the directory. The node key
(`/etc/private-ai/node/`, slice 1) is created 0600 by `private-ai-manager`, not root.

**The instances' units: polkit details exist for start and stop, not for enable.** Since systemd 226 a
unit job sends polkit the unit name and the verb ([NEWS, "CHANGES WITH 226"](https://github.com/systemd/systemd/blob/main/NEWS):
"it will now add additional fields to the request, including unit name and desired operation"); the
current code passes `unit` and `verb` (`start`, `stop`, `restart`, `reset-failed`) for every unit job
(`src/core/dbus-unit.c`, `unit_polkit_details` in `src/core/unit.c`), read in a rule with
`action.lookup("unit")` ([polkit(8)](https://www.freedesktop.org/software/polkit/docs/latest/polkit.8.html)).
`EnableUnitFiles` and `DisableUnitFiles` need `manage-unit-files`
([org.freedesktop.systemd1(5)](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.systemd1.html), "Security"),
which systemd checks **with no details** (`bus_verify_manage_unit_files_async` passes `details = NULL`,
`src/core/dbus-util.c`), and `Reload` needs `reload-daemon`. So a rule can scope start and stop to
`llama.cpp@*.service`, but not enable. Granting `manage-unit-files` whole would let the account enable any
installed unit, among them `debug-shell.service`, a root shell on tty9
([systemd-debug-generator(8)](https://www.freedesktop.org/software/systemd/man/latest/systemd-debug-generator.html)):
root by another door, rejected. Every supported distribution ships systemd far past 226 (this machine: 259).
The rule `-core` ships beside `50-private-ai.rules`:

```js
polkit.addRule(function (action, subject) {
  if (action.id != "org.freedesktop.systemd1.manage-units" || subject.user != "private-ai-manager") return;
  var unit = action.lookup("unit"), verb = action.lookup("verb");
  var ok = /^llama\.cpp@[A-Za-z0-9_-]+\.service$/.test(unit)
    || unit == "private-ai-bff.service" || unit == "private-ai-director.service";
  if (ok && ["start", "stop", "restart", "reset-failed"].indexOf(verb) >= 0) return polkit.Result.YES;
});
```

(the instance id pattern to match `valid_instance_id`; the bff and the director for the promote).

**Checking its callers with polkit, unprivileged.** polkitd lets a caller check a subject of another
user only when the caller is uid 0 or is named in the action's `org.freedesktop.policykit.owner`
annotation (polkit(8): "If this annotation is not specified then only root can query whether a client
running as a different user is authorized for an action", "A typical use of this annotation is for a
daemon process that runs as a system user rather than root"; the code:
`may_identity_check_authorization`, `src/polkitbackend/polkitbackendinteractiveauthority.c`). Our
`org.privateai.manager.control` and `configure` gain
`<annotate key="org.freedesktop.policykit.owner">unix-user:private-ai-manager</annotate>`. The manager
passes no details (`dbus.rs` `PolkitAuthority::check`), so nothing else changes.

**The bus name.** The D-Bus policy's `<policy user="root"><allow own=...>` becomes
`<policy user="private-ai-manager">`; everyone may still send, polkit deciding per call.

**Authenticate: pam_unix cannot check another user's password unprivileged.** Unless built with the
non-default `pam_unix-try-getspnam` (`meson_options.txt`, default false), pam_unix hands a shadowed
account to `unix_chkpwd` (`modules/pam_unix/passverify.c`), and the helper, called by a non-root user for
another user's name, drops to the caller and fails (`unix_chkpwd.c`: "if the caller specifies the
username, verify that user matches it"), as the playbook's first fact already says
([linux-pam](https://github.com/linux-pam/linux-pam)). Options:

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. A root helper, socket-activated per call** | `pai-manager helper` (the same binary), `private-ai-helper.socket` (`ListenStream=/run/private-ai-helper.sock`, `SocketUser=root`, `SocketGroup=private-ai-manager`, `SocketMode=0660`, `Accept=yes`: [systemd.socket(5)](https://www.freedesktop.org/software/systemd/man/latest/systemd.socket.html)) and `private-ai-helper@.service` | Root code remains, but only a fixed verb set taking a validated id or a credential pair, one process per connection, nothing listening on the network, peer checked by `SO_PEERCRED` | **Recommended** |
| B. Membership in `shadow` and our own crypt check | No root process | Bypasses the PAM stack (faillock, account expiry, sssd); the account reads every hash for offline cracking | Rejected |
| C. Keep `Authenticate` and enable in a root manager | None | The requirement is not met | Rejected |

**The helper's verbs** (Linux): `Authenticate{user, password}` (the PAM `private-ai` service, as
`accounts.rs` today; the manager then reads the groups itself, which needs no privilege);
`SetBoot{id, on}` (`EnableUnitFiles` or `DisableUnitFiles` of `llama.cpp@<id>.service` and `Reload`, the
id held to `valid_instance_id`); `SetDirector{on}` (the same for `private-ai-director.service`, for
promote). Nothing else: no path, no unit name, no argument passes through. The unit: root, hardened
(`NoNewPrivileges=`, `ProtectSystem=strict`, `PrivateNetwork=yes`, `RestrictAddressFamilies=AF_UNIX`; the
capability set reduced to what the PAM stack reads, to be measured in the slice: `CAP_DAC_READ_SEARCH`
where `/etc/shadow` is mode 0000, unverified per distribution).

**The unit.** `User=private-ai-manager`, `Group=private-ai-manager`,
`SupplementaryGroups=private-ai render video`, `ReadWritePaths=/etc/llama.cpp /etc/private-ai`,
`StateDirectory=private-ai-manager` (now owned by the account, as the metrics design already assumes),
`Environment=HOME=/var/lib/private-ai` kept, and the bff's hardening (`ProtectSystem=strict`,
`ProtectHome=read-only` (file modes decide, as for the instances), `PrivateTmp=`, `NoNewPrivileges=`,
`CapabilityBoundingSet=` empty, `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6` for slice 2's
listener), but not `PrivateDevices=` (the GPU probe).

**Model files.** The manager reads (GGUF header, scans) only what `private-ai-manager` may: the shared
folder `/var/lib/private-ai/models` (group `pai`, world readable), any world-readable path, and what group
`private-ai` reads. It can no longer read a file under a user's home that is not readable by others
(current Ubuntu and Fedora create homes not readable by others: unverified per release). Those files were
never usable anyway: the instance account `private-ai` cannot read them either, so the manager's reach
becomes the instances' reach. What the user does: keep models in the shared folder (the recommended
place), or grant read to the group `private-ai` on their own folder. The refusal is the existing
not-found or unreadable answer for that path.

#### Windows

**The account.** The virtual account `NT SERVICE\private-ai-manager` (the service's own SID, no
password, local; on a domain it reaches the network as the computer account:
[Service accounts](https://learn.microsoft.com/en-us/windows-server/identity/ad-ds/manage/understand-service-accounts)),
recommended over a (group) managed service account, which needs Active Directory, and over
`LocalService`, which every such service shares. The service's required privileges are set to none
beyond `SeChangeNotifyPrivilege`, which the SCM always keeps
([SERVICE_REQUIRED_PRIVILEGES_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_required_privileges_infow):
"the SCM will remove the privileges that are not required from the process token"), so the token drops
`SeImpersonatePrivilege`, which every SCM service holds through the Service group
([Impersonate a client after authentication](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-10/security/threat-protection/security-policy-settings/impersonate-a-client-after-authentication))
and which Microsoft's own page names as an elevation path.

**The pipe and the token read.** The pipe's DACL (`PIPE_SDDL`) stays; the manager creates it as its
own account. Reading the client's token needs no privilege when the client connects at identification
level: impersonation is allowed when "the requested impersonation level of the token is less than
SecurityImpersonation" ([ImpersonateNamedPipeClient](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-impersonatenamedpipeclient)),
and at `SecurityIdentification` "the server can get the identity and privileges of the client"
([Impersonation levels](https://learn.microsoft.com/en-us/windows/win32/secauthz/impersonation-levels),
which also says the client chooses it in `CreateFile`; impersonation is the default). So `PipeClient`
opens the pipe with `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`; a client that does not is refused
(fail closed: a failed `ImpersonateNamedPipeClient` or a token above identification level answers
nothing). `pai` membership by name (`NetUserGetLocalGroups`) works for any authenticated caller on a
workstation or member server ([NetUserGetLocalGroups](https://learn.microsoft.com/en-us/windows/win32/api/lmaccess/nf-lmaccess-netusergetlocalgroups),
Remarks), and `LogonUserW` needs no privilege: "The SE_TCB_NAME privilege is not required for this
function unless you are logging onto a Passport account" ([LogonUserW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-logonuserw)).

**The instance services.** By default only Administrators may create services or change a service's
configuration, and `SERVICE_CHANGE_CONFIG` "grants the caller the right to change the executable file
that the system runs, it should be granted only to administrators"
([Service security and access rights](https://learn.microsoft.com/en-us/windows/win32/services/service-security-and-access-rights));
`DeleteService` needs `DELETE`. The manager creates and deletes `llama.cpp_<id>` (`windows/scm.rs`) and
changes its start type for boot. Options:

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| A. Grant the manager `SC_MANAGER_CREATE_SERVICE` on the SCM's descriptor (settable since Server 2003 SP1, same page) | One descriptor change at install | Whoever creates a service chooses its binary and account, LocalSystem included: the manager would be SYSTEM-equivalent | Rejected |
| **B. A small LocalSystem helper**: `pai-manager helper` as the service `private-ai-helper`, a pipe whose DACL admits only the manager's service SID | One more service in the framework's MSI | Root-equivalent code remains, but with a fixed verb set taking only a validated id; it builds the whole service spec itself (binary from `HKLM\Software\private-ai` `InstallDir`, account `NT SERVICE\llama.cpp_<id>`, `run-instance <id>`) | **Recommended**, the Linux helper's twin |
| C. Services pre-created by the MSI (fixed slots) | No runtime creation | Instance ids are user-chosen and name the service, its account and its log; slots break all three | Rejected |
| D. Instances created only by the MSI | No helper | Removes the web UI's instance creation (`POST /api/instances`) | Rejected |

**The helper's verbs** (Windows): `Ensure{id}` (create `llama.cpp_<id>` or repair it to the fixed spec,
with its DACL: SYSTEM and Administrators as by default, plus the manager's SID with start, stop, query
status, query config, interrogate, read control; never change config, `WRITE_DAC` or delete);
`Delete{id}`; `SetBoot{id, on}` (the start type only). Running or not: auto start and idle on its pipe
(preference: no service DACL for the manager to start it on demand, whose WiX expression is to be
confirmed); a demand start can come later.

**The folders.** `%ProgramData%\llama.cpp\config\` and `%ProgramData%\private-ai\config\` add the
manager's service SID with modify (both MSIs; service SIDs are computed from the service name, so the
engine's MSI can name it as it names `S-1-5-80-0` today); `%ProgramData%\private-ai\node\` (slice 1)
grants SYSTEM and the manager's SID only; `logs\` and `models\` stay as they are.

#### What the director role gains

On a promoted machine no process facing the network runs as root or LocalSystem: the node listener
(8592) is `private-ai-manager`, the director's (8593) `private-ai-director`, the web service
`private-ai-bff`. The director's socket admits the manager's uid instead of uid 0 for promote, so the
socket's rule becomes "the bff and the manager" ("One binary, two roles", updated). A compromised node
manager acts on its instances and files (as any `configure` caller could) and can ask the helper its
three verbs; it cannot become root.

#### Slices (each leaves a working install, since every closed slice is deployed on this machine)

P1. **Linux helper, still root.** `pai-manager helper` with `Authenticate` and `SetBoot`, its socket and
    unit; the manager calls it. Done when: a peer other than `private-ai-manager` (and, while the manager
    is still root, uid 0) is refused before a frame is read; an id outside `valid_instance_id` is refused;
    `Authenticate` and boot on/off behave as today through a fake helper; the unit files are checked by
    `system_files.rs` tests.
P2. **Linux rights in place, still root.** The account (sysusers), the tmpfiles lines, the polkit rule,
    the owner annotation, the D-Bus policy for `private-ai-manager`. Done when: the files' tests pin each
    line; the rule's id pattern matches `valid_instance_id` in a test over the same cases.
P3. **Linux: drop root.** `User=` and the hardening in the unit. Done when: the unit test asserts the
    lines; after `xtask install` (the user's run, or the standing deploy) the services check (start,
    stop, save, boot on and off, sign-in) passes.
P4. **Windows helper, still LocalSystem.** `private-ai-helper` with `Ensure`, `Delete`, `SetBoot`. Done
    when: the spec it builds is pinned by tests from an id alone; an invalid id is refused; the
    DACL string grants the manager no change-config, `WRITE_DAC` or delete; type-checked.
P5. **Windows clients at identification level.** `PipeClient` opens with SQOS identification; the server
    refuses a higher level. Done when: tests pin the flags; type-checked.
P6. **Windows: drop LocalSystem.** The virtual account, the required privileges, the folders' ACLs in both
    MSIs. Done when: the MSI text checks pin them; the CI install, repair and uninstall pass.

**Where in the plan** (recommended): at the head of step 14, before its slice 1, since slice 1 creates
the node key as the manager's account; Linux (P1 to P3) then Windows (P4 to P6). The alternative, after
step 14's slice 4, would build the listener and the join on root and move them.

### Transport between machines

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. The manager protocol (u32 + JSON frames) over TLS 1.3, mutual, certificates from the director's CA** | rustls in the manager and the director; keys, CSRs and issuing with rcgen ( [docs.rs/rcgen](https://docs.rs/rcgen/latest/rcgen/)); `PipeClient` reused as is over rustls' `StreamOwned`, which is `Read + Write` ([docs.rs](https://docs.rs/rustls/latest/rustls/struct.StreamOwned.html)); `manager::pipe`'s dispatch reused on the node | A network listener in the manager (below); the join and renewal code is ours | **Recommended** |
| B. HTTPS + JSON (axum with TLS, mutual) | An HTTP mapping of every method, a second error shape | Same listener question, more code | No gain over A for a machine-to-machine call |
| C. gRPC (tonic, protobuf) | A schema, a code generator, a second protocol beside the pipe's | gRPC supports client certificates ([grpc.io auth guide](https://grpc.io/docs/guides/auth/)) | Worth it only for streaming (live logs to the web); later, if then |
| D. SSH with a forced command relaying frames to the local manager | No listener of ours; keys in `authorized_keys` with `restrict,command=` ([sshd(8)](https://man.openbsd.org/sshd#AUTHORIZED_KEYS_FILE_FORMAT)) | sshd required on every node: in the box only on Windows Server 2025, an optional feature on Windows 10/11 and Server 2019/2022 ([Microsoft Learn](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse)); enrolling writes another account's `authorized_keys`; the relay account becomes a new trusted caller | A good fallback if a network listener in the manager is refused |
| E. D-Bus over TCP | None | The specification: no integrity or confidentiality, non-local TCP deprecated ([D-Bus spec](https://dbus.freedesktop.org/doc/dbus-specification.html), "TCP Sockets", revision 0.33) | Rejected |

**Who listens.** The director dials the nodes; each node listens. Option A puts the listener in
the manager (unprivileged once "The node's manager without root or LocalSystem" is built, which comes first), the smallest design, since the manager already serves the protocol
and a node then needs only `-core` and the engine (a headless GPU box needs no bff). The alternatives
are a relay in the node's bff (which is sandboxed and unprivileged, but must then be installed on every
node and be trusted to grant `configure` on the director's behalf, the same power with more code)
or a new node agent (another account, unit, package and trusted caller). The risk is bounded so:

- **Off by default.** No port is open until a node's administrator enables it (`pai node enable`,
  `configure`); `pai node leave` closes it and forgets the peer.
- **Nothing is parsed before the handshake proves the peer.** rustls' client-certificate hook
  (`ClientCertVerifier`, [docs.rs](https://docs.rs/rustls/latest/rustls/server/danger/trait.ClientCertVerifier.html))
  accepts only a certificate chaining to the node's CA bundle with the name `director.pai.invalid` and
  the `clientAuth` EKU, valid now; a wrong peer is closed by TLS. The only exception is the join window (below): at most 15 minutes, the `Join` method alone,
  attempts limited.
- **TLS 1.3 only, ALPN `pai-manager/1`**: a client offering no matching protocol gets the fatal
  `no_application_protocol` alert ([RFC 7301](https://www.rfc-editor.org/rfc/rfc7301)), which also
  carries the protocol's major version.
- **Bounded**: a few connections at once, the existing 1 MiB frame, an idle timeout; the listener's
  threads never hold the manager's write lock while waiting on the network.
- Port: a proposal, 8592 (the bff is 8590), set in the node's config, opened in the firewall by the
  administrator (the packages open nothing).

### Identity and joining: a private CA held by the director: accepted (user, 2026-11-21)

Revised 2026-11-21 (user, `PLAN.md` step 14): links between the nodes and the director are secured by
**a private CA the director holds**; this replaces the pinned self-signed fingerprints of 2026-11-15.
TLS 1.3 with mutual authentication gives confidentiality and integrity on the wire; **signatures** are
added only where proof is needed afterwards: the director's commands as recorded in a node's log, and
the model store's manifest.

**Crates** (all already the plan's, versions as read 2026-11-21): `rustls` 0.23 for TLS 1.3, ALPN, and
the verifiers (`WebPkiClientVerifier::builder(roots)`, which takes CRLs with `.with_crls`:
[docs.rs](https://docs.rs/rustls/latest/rustls/server/struct.WebPkiClientVerifier.html)), and
`export_keying_material` (RFC 5705 exporter, after the handshake:
[docs.rs](https://docs.rs/rustls/latest/rustls/struct.ConnectionCommon.html)); `rustls-webpki` under
it for chains, names and validity; `rcgen` 0.14 for keys, CSRs (`CertificateParams::serialize_request`)
and issuing (`CertificateSigningRequestParams::from_der`, feature `x509-parser`, then `signed_by` an
`Issuer`: [docs.rs](https://docs.rs/rcgen/latest/rcgen/struct.CertificateSigningRequestParams.html),
[CertificateParams](https://docs.rs/rcgen/latest/rcgen/struct.CertificateParams.html)). The CRL writer
is to be confirmed in rcgen at slice 5 (unverified); a serial set checked in a verifier wrapper is the
fallback.

**Names, not addresses.** Every certificate names a role under the reserved TLD `.invalid` (RFC 6761,
section 6.4), and the TLS client asks for that name whatever address it dials (rustls takes the
`ServerName` apart from the socket): `director.pai.invalid` for the director, `<node>.node.pai.invalid`
for a node, `<node>` being the registry name (letters, digits, `-`, at most 63). An address can change
with no new certificate; no public name or public CA is involved.

**Certificate profile** (user, 2026-11-21: ECDSA P-256; the CA 5 years, rotated at 4):

| Certificate | Key | SAN | Key usage / EKU | Lifetime |
| --- | --- | --- | --- | --- |
| CA | P-256, on the director only | none (subject `pai director CA <id>`) | `keyCertSign`, `cRLSign`; `is_ca`, path length 0 | 5 years, rotated at 4 |
| Director | P-256 | `director.pai.invalid` | `digitalSignature`; `serverAuth` (store and renewal listener) and `clientAuth` (dialing nodes) | 90 days, renewed locally from 30 days before expiry |
| Director signer | P-256, a key of its own (no TLS key signs records) | `signer.director.pai.invalid` | `digitalSignature`; one private EKU under the UUID arc `2.25.<uuid>` (ITU-T X.667), so no TLS verifier accepts it | 90 days, renewed locally from 30 days before expiry |
| Node | P-256, generated on the node, never leaves it | `<node>.node.pai.invalid` | `digitalSignature`; `serverAuth` (its listener) and `clientAuth` (renewal, store) | 90 days, renewed by the node from 30 days before expiry |

Every leaf is issued with `notBefore` one hour in the past (preference: absorbs small clock skew) and
the authority key identifier. A node trusts **only** the CA certificates of its trust bundle, and from
them only the name `director.pai.invalid` (as server, at the director's listener) or the same name as
client (at its own listener); the director trusts only its CA and, per node, the registry's name.

**No offline root** (user, 2026-11-21: "if the director is compromised there are no other problems left
anyway"): one CA, its key on the director only; no intermediate, no root kept elsewhere.

**Where the CA key lives** (user, 2026-11-21: a plain file). `/var/lib/private-ai-director/ca/ca.key`,
mode 0600, owner and group `private-ai-director` (the director's system user), in a directory of mode
0700 with the same owner, under the director's `StateDirectory=` (so systemd re-applies its owner and
mode at each start). No systemd credential and no TPM binding. Consequences, accepted: whoever can read
that file, root on the director's machine or any backup of its disk, holds the CA and so the fleet;
backups of the director's disk carry the key and must be kept like it. An optional manual export,
`pai-manager director ca export <file>` (run as the director's account, `sudo -u private-ai-director`, so
by an administrator of the director's machine: revised 2026-11-22, the key being readable by that account
alone), writes a copy encrypted with
a passphrase typed at the prompt, for an offline copy; restoring it on a new director keeps every node
joined. The process is unprivileged and hardened like the bff; a hardware key (PKCS#11) can come later.

**The join** (the director still dials, the code still comes from the node):

1. On the node, a `pai` member runs `pai node enable` (`configure`). The manager generates the node's
   P-256 key (`/etc/private-ai/node/`, 0600 `private-ai-manager`; Windows `%ProgramData%\private-ai\node\`,
   SYSTEM and the manager's service SID: "The node's manager without root or LocalSystem"), opens the listener in join mode with a **temporary self-signed certificate of that
   key**, and prints the **join code**: the key's SPKI SHA-256 (truncated to 128 bits) and a random
   128-bit secret, valid 15 minutes, single use.
2. In the director's web UI an administrator adds the node: its name, its address, the code.
3. The director connects, checks the node's temporary certificate against the code's SPKI hash before
   sending anything, and sends `Join{ca_bundle, director_name, mac}` with
   `mac = HMAC-SHA256(secret, ca_bundle || exporter)`, `exporter` being 32 bytes of
   `export_keying_material` with the label `pai-join`: the node learns the CA from the one party that
   knows the secret, on this very connection (a relay has another exporter value).
4. The node checks the MAC in constant time, then answers `Csr{csr, mac2}`, the CSR of **the same
   key** for `<name>.node.pai.invalid`, `mac2 = HMAC-SHA256(secret, csr || exporter)`.
5. The director checks `mac2`, that the CSR's public key is the one the code named, and that the CSR's
   name is the one being added, ignores every other requested extension (the profile above is the
   director's), signs, and sends `Certificate{chain}`. The node installs it with the CA bundle, leaves
   join mode, and from then on its listener verifies clients against the CA only.

**A joined node trusts only its cache, and fails closed** (user, 2026-11-21). Once joined, the node's
manager accepts on its network listener no connection whose client certificate does not chain to the
CA bundle in its local cache (`/etc/private-ai/node/`, beside its key) with the director's name, and
runs no forwarded command whose signature does not verify against that same cache. There is no
fallback: no plain listener, no trust on first use after the join, no other CA, no system store, and
the join window does not reopen by itself (only `pai node enable` by a local `pai` member, which starts
a new join). A missing or unreadable cache keeps the listener closed. Local clients (D-Bus, the named
pipe, the local bff) are unaffected: they keep today's OS identity, polkit and `pai` rules.

**Renewal, automatic and early** (user, 2026-11-21: 90-day certificates, renewal from 30 days before
expiry, every 5 minutes while connected, at once on reconnection). From 30 days before `notAfter` (day
60 of 90) the node's manager generates a new key and CSR and calls `Renew` on the director's listener
(port 8593, the store's; ALPN `pai-director/1`), authenticated by its current certificate. While the
director is **connected** (its listener answers the TLS handshake), a refused or failed renewal is
retried every 5 minutes, fixed, no backoff. While it is **disconnected** (the connection itself fails)
the node keeps a connection attempt every 5 minutes as its probe, and renews **at once** on
reconnection: the first successful connection to the director's listener, or the first authenticated
connection the director opens to the node's listener, whichever comes first. The new certificate
replaces the old one atomically (written, then renamed) and the old key is deleted. What the node says
as expiry nears: a failed renewal is a warning in its log (`node-cert-renewal-failed`, with the reason),
written when the reason changes and at most once an hour, so a 5-minute retry does not flood the log;
a daily `node-cert-expiring` warning from 30 days before `notAfter` (recommended over 7: a renewal
normally succeeds at its first attempt, so the warning only ever shows when something is wrong, and
then a month ahead); `pai node show` and the node's `GET /api/node` report the certificate's expiry and
the last renewal attempt, and the director's hub shows `certExpires` per node. The director refuses a node
on the deny list, a removed node, a CSR whose name is not the caller's, and an expired certificate.
Each reply carries the director's clock; a node whose clock differs by more than 5 minutes logs
`node-clock-skew` (operational requirement: NTP on every machine, since `rustls-webpki` judges validity
by the local clock).

**When a node's certificate expires** (the director unreachable for the whole renewal window, 30 days, or refusing it): the node runs
on (its instances, boot settings and local clients never depend on the director); the director's
handshakes to it fail and it reads `expired` in the hub; it cannot renew with an expired certificate.
It comes back by a new join (`pai node enable` prints a new code; the instances and files are kept);
there is no grace period (user, 2026-11-21).

**Revocation: refused renewal plus a deny list.** A refused renewal alone takes effect when the
current certificate expires, within 90 days at most; the deny list is what revokes at once. Removing a node puts its name and current serial on
the director's deny list, published as a CRL the CA signs and checked at **every** handshake the
director takes part in (as server: `WebPkiClientVerifier` `.with_crls`; as client: the same list
checked on the node's certificate), and refuses its renewals; an entry is pruned once the denied
certificate's `notAfter` has passed. Nodes keep no deny list: they trust one director name under one
CA, and a compromised director is a compromised CA (rotation below).

**Signed records.** Every command the director forwards carries `{node, principal, method, args
SHA-256, time, nonce}` signed by the signer key, with the signer's certificate; the node checks the
chain to its CA, the private EKU and the name, refuses a time outside 5 minutes of its clock or a nonce
seen in the last 10 minutes, and stores the signature with the log entry, so the log proves later
which director asked what for whom. The store's manifest is signed the same way.

**CA rotation.** Planned: the director creates the new CA and pushes the bundle {old, new} to every node
(`SetTrustBundle`, a signed command); once every reachable node has it, the director's own leaf and new
issuances move to the new CA; after 90 days every live node certificate is from the new CA and the
bundle {new} is pushed. A node that was unreachable meanwhile renews under the new CA if it still holds
the bundle, else joins again. A **compromised** CA key: every node joins again (`pai node leave`, a new
code), since the attacker could push bundles too.

A node has at most one director (one CA bundle, one director name): simpler rights, and a second
governor would be a question for the user. **Removal**: the deny list on the director, `pai node
leave` on the node (forgets the bundle and the certificate, closes the port). **A node reinstalled**
has a new key and no certificate: it joins again.

### Who may act on which node

| Option | Cost | Verdict |
| --- | --- | --- |
| **Delegation**: joining grants the director `control` and `configure` on the node; the director decides who among its users may act | Small: one new caller kind in the node's Authority | **Recommended now** |
| Per-node rights in the director (user or group to node list) | A rights model and its UI | Later, on top of delegation, without touching nodes |
| The same accounts on every machine (a directory: LDAP, AD), the node checking the named user's local `pai` membership | Needs a shared directory; the node trusts the director's word about who it is anyway | Only if the user's machines share one |
| A central identity provider (OIDC tokens verified by each node) | Token issuance, keys, expiry, an IdP to run | Later, if users come from outside |

How a web session maps to a node: the user signs in on the director machine's bff, checked by
**that machine's** manager (`Authenticate`, so that machine has `-core`); only a `pai` member gets a
session. The bff sends each call to the director with the session's principal
(`alice@<director name>`); the director admits only its bff, and the manager for promote (by peer credentials:
`SO_PEERCRED` on a Unix socket, [unix(7)](https://man7.org/linux/man-pages/man7/unix.7.html); "One binary,
two roles") and forwards the call to
the node with the same principal as a field. The node's manager authorizes the enrolled peer and logs
the principal with every change. No password leaves the machine it was typed for.

What each side can do if compromised: the bff, act as any administrator while compromised (as today on
one machine); the director, act on every joined node as an administrator until each node forgets
it; the network, nothing (mutual TLS); a node, nothing on the other nodes (it holds no credential to
them) and it cannot impersonate the director to other nodes.

### The bff and the web UI

- `bff.ini` `[Bff] Director = true` (default false) selects hub mode; explicit, so a director
  that is down shows as down instead of the UI quietly dropping back to the local machine.
- `GET /api/version` gains `"nodes": true|false`, so the web UI knows the mode without a probe.
- Hub mode adds `GET /api/nodes` (`[{name, address, state: reachable|unreachable|untrusted|expired|removed|incompatible, version, certExpires}]`),
  `POST /api/nodes` (name, address, code) and `DELETE /api/nodes/{node}`, and serves every
  instance and preset route under `/api/nodes/{node}/...`, same bodies and errors, plus the keys
  `director-unavailable` and `node-unavailable`. Local mode keeps today's routes; the web UI picks
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
| A node down or unreachable | The director's per-node timeout (shorter than the bff's) answers `node-unavailable` for it; the others answer as usual (fan-out in parallel) |
| A write whose reply is lost | The node's write is atomic (rename); the UI says the outcome is unknown and reloads; the revision check (409) makes a retry safe |
| The director down | Nodes run on: systemd and the SCM own the instances and the boot settings, local `pai-config`, `pai` and a local bff work; the hub's web UI shows `director-unavailable` |
| The director gone for good | Each node's administrator runs `pai node leave`; nothing else on the node changes |
| A node reinstalled, or its certificate expired or denied | Read as `untrusted`, `expired` or `removed`; it joins again |
| The director's CA key lost | Every node joins again, unless the optional manual export restores it |
| Version skew | Below |

### Packaging, upgrades and version skew

- **No package of its own** (revised 2026-11-22, "One binary, two roles"): `-core` ships
  `pai-manager` (both roles), the director's unit disabled and its sysusers entry
  (`private-ai-director`); a hub also needs `-bff`. Its own machine is a node, joined by the promote over
  `127.0.0.1`, one code path. Windows: later (the same service shape in the framework's MSI, installed
  manual and stopped, or not at all; a user question).
- **Nodes need nothing new**: the listener is part of `pai-manager` in `-core`, off until enabled.
- **Upgrades**: one package upgrades both roles at once on the director's machine, so its director and
  its own node never skew; the postinst restarts `private-ai-director.service` only when it was running
  (`deb-systemd-invoke` as for the other units), and never enables it.
- **Skew**: the ALPN name carries the protocol's major version (`pai-manager/1`); within a major,
  changes are additive (new methods, new optional fields), and a new `Version` method (no authorization)
  returns the node's framework version and protocol minor, so the director shows it and avoids
  methods an older node lacks (today an unknown method ends the connection with `BadRequest`,
  `manager::pipe`). The director supports the current and the previous major; upgrade it first.
  The join state lives in the node's config directory and survives package upgrades.

### The model store (R13, user 2026-11-21): accepted (user, 2026-11-21), the nodes pull

The user's words (2026-11-21, translated): "The director will host the models for all the nodes
and offer an endpoint to the hosts to download them locally."

- **What it is.** A folder of GGUF files the director keeps, each with a manifest entry
  `{name, size, sha256, origin}` (`origin`: the HF repo, commit and path it was taken from, or
  "uploaded"). For a file taken from Hugging Face the sha256 is checked at ingestion against the tree's
  `lfs.oid`, which `hf.rs` already reads; the manifest is the store's truth.
- **Who may download.** Only a joined node: a valid certificate from the director's CA, its name in
  the registry and not on the deny list.
  A node fetches only when told to by a `configure` call the director forwards for a signed-in
  administrator (`StoreFetch{name}`, its principal logged by the node like every change), or by its own
  `pai` member; nobody else, and never anonymously.
- **Channel and identity.**

  | Option | Cost | Risk | Verdict |
  | --- | --- | --- | --- |
  | **A. The node pulls: HTTPS `GET /store/<name>` on a director listener, TLS 1.3 mutual, certificates from the director's CA (the listener that also takes renewals)** | A listener in the director (unprivileged, its own port, a proposal: 8593); HTTP range requests give resume for free (RFC 9110, "Range Requests") | The director must be reachable from the nodes, the reverse of the answer to question 1 | **Recommended**: it is what the user asked ("an endpoint to download"), and a big transfer is a plain stream, not 1 MiB JSON frames |
  | B. The director pushes over the node's listener (`StoreWrite{name, offset, bytes}` frames) | No new listener | Base64 in 1 MiB JSON frames (a third larger), a long-held connection on the node's manager, resume by offset hand-made | Fallback if nodes cannot reach the director |

  The node's manager (its own account, `private-ai-manager` or `NT SERVICE\private-ai-manager`) does the
  transfer, since only it holds the node key (`/etc/private-ai/node/`, 0600 that account). It parses only TLS and HTTP headers; the body is hashed and
  written, never interpreted.
- **Integrity.** The node writes `<ModelsDir>/.store-staging/<sha256>.partial`, hashes while writing,
  and renames into `<ModelsDir>/models/store/<name>` (owned by the engine's account) only when size and
  sha256 match the manifest entry it read first over the same channel; a mismatch deletes the partial
  and refuses `store-hash-mismatch`. A name is held to the repo-path rule (`safe_repo_path`), so no
  manifest entry writes outside the staging and target folders.
- **Resume.** A failed or cancelled transfer keeps its partial; the next fetch sends `Range: bytes=<len>-`
  with `If-Range` on the sha256 as entity tag, so a changed file restarts from zero.
- **Storage and quotas.** The store lives in the director's state directory
  (`/var/lib/private-ai-director/store/`), capped by `[Store] MaxBytes` (refused `store-full`); a node
  checks its free space against the entry's size before it starts (`node-disk-full`). No per-node quota
  until a user asks for one.
- **Beside a node's own HF downloads.** Both stay: a profile with `hf-repo` still makes llama-server
  fetch from Hugging Face on the node, and the GUI's download still works. The store is a second source
  in the picker (`"source":"store"`, the same shape as `GET /api/hf/files`, Part 1 "Choosing the model
  file on the web"); once on disk a store file is a local file like any other, listed by
  `GET /api/node/model-files` and written as an absolute `model` path. A file already present with the
  same sha256 is not fetched again.

### Instance profiles across nodes (user, 2026-11-26; not built)

The director reuses the node's instance profiles ("Instance profiles", part 1) as the unit it deploys.
"Profile P on node N as instance X" means:
- copy `server@<s>.ini`, the model sections P lists, and P's section of `instance-profiles.ini`;
- pull the model files through the model store;
- then `CreateInstance(X, P)` on N, the port proposed by N's manager (P1: the address is the
  instance's, per node).

Ids are shared across the director's nodes. A node holding the same id with different content refuses
rather than renames. Nothing of this is built before step 14.


**Desired state and reconciliation** (user, 2026-11-27; not built). The director pushes desired state:
the instance profile, its model sections and its server profile. Each node's manager applies it as
on one node ("Convergence", part 1): the intrinsic checks refuse a profile the director should
never have sent, the fit checks decide per instance, and each instance converges or keeps its last
good pair with a reason. The node reports `converged`, `reason` and `pendingRestart` per instance
through the same `ListInstances` fields. The director shows them per node and offers Restart. It
never restarts on its own. A node that missed a push converges at the next one, because the apply is
idempotent.

### Slices

Each test first; the coordinator may split further. Linux first; the Windows halves of 1 to 4 are
type-checked like the rest of the Windows code. P1 to P6 ("The node's manager without root or
LocalSystem") come before slice 1.

0. **Before step 10 slice 6b part 3 hardens** (no new behaviour): the acting principal is a field of
   the request beside the transport caller, not folded into `Caller` (below). Done when: a test pins
   that a call from the bff carrying a principal reaches the Authority as (caller, principal, action).
1. **Node identity.** `pai node show|enable` creates the P-256 key and the temporary certificate and
   prints the code (after P1 to P6, "The node's manager without root or LocalSystem"). Done when: the key
   file is 0600 `private-ai-manager` (SYSTEM and the manager's service SID on Windows), the
   code's hash is the key's SPKI, a non-`configure` caller is refused.
2. **The listener.** TLS 1.3, ALPN, client certificates verified against the node's CA bundle with the
   name `director.pai.invalid` and `clientAuth`, `manager::pipe` dispatch with a new caller kind. Done
   when, over loopback with a test CA: a director certificate from it lists instances; a certificate
   from another CA, an expired one, one with another name or without `clientAuth` is closed before any
   frame; a wrong ALPN gets the alert; the bound on connections holds; with the cache missing or
   unreadable the listener stays closed, and local clients still work.
3. **The join.** Code, window, `Join` with its MAC over the exporter, the CSR bound to the code's key,
   issuing, install, `leave`. Done when: a right code leaves the node trusting the test CA and holding a
   90-day certificate for `<name>.node.pai.invalid` with `serverAuth` and `clientAuth`; a wrong, expired
   or reused code is refused and counted; a CSR of another key, or for another name, is refused; a MAC
   computed on another connection (a relay) is refused; requested extensions outside the profile are
   not issued; after the window only CA-issued clients pass; `leave` closes the port and forgets the
   bundle.
3b. **Renewal and the deny list.** `Renew` on the director's listener, the CRL at every handshake. Done
   when: a renewal from 30 days before expiry gives a new key and certificate; a removed node's renewal and handshake
   are refused; an expired certificate cannot renew; a skew over 5 minutes is logged; a denied entry is
   pruned after its `notAfter`; a refused renewal is retried every 5 minutes while the director
   answers; a node disconnected renews at once on reconnection (either direction); the failure warning
   is written at most once an hour; `node-cert-expiring` appears daily from 30 days before expiry.
4. **Delegated rights and the signed log.** Done when: the CA-authenticated director may `configure`
   (including the API key in reads); each change it makes is logged with its principal and the
   signer's signature; a command unsigned, signed by a certificate without the private EKU, outside
   the 5-minute window, or with a replayed nonce is refused.
5. **The director.** The role `pai-manager director` (module `manager::director`, no new crate), the CA
   (a plain 0600 file), its leaves and the signer, the registry, `add` against an in-process node. Done
   when: `parse_args` gives the director command for `director` and refuses an extra argument; the
   director started with effective uid 0 exits with an error before touching its state; the CA key is created as `ca.key` 0600
   `private-ai-director` in a 0700 directory; `ca export` writes a passphrase-encrypted copy that a
   fresh director imports and then still verifies the nodes' certificates; `add` with a code issues the node's certificate; a node presenting a
   certificate of another CA reads `untrusted`, an expired one `expired`; a stopped node reads
   `unreachable` within the timeout while another answers.
6. **Its local interface.** The protocol with `node` and principal on a Unix socket, the bff's uid and
   the manager's uid only. Done when: another uid is refused before a frame is parsed; calls reach the right node;
   `ListNodes` fans out.
7. **The bff's hub mode.** `Director`, `nodes` in `/api/version`, `/api/nodes`, the node-prefixed
   routes over a node seam. Done when: tests against a fake director pass and every local-mode
   test still passes unchanged.
7b. **Promote and demote.** `Promote` and `Demote` on the manager (`configure`), `pai node promote|demote`,
   over seams for systemd, the director's socket and `bff.ini`. Done when: a non-`configure` caller is
   refused; promote enables and starts the director unit, enrols the local node at `127.0.0.1:8592` (the
   fake director receives `AddNode` with the code the node's listener holds), writes `Director = true`
   and restarts the bff in that order; a promote failed at any step leaves the bff in local mode and a
   second run completes it; a node joined to another director is refused `node-has-director`; demote
   writes `Director = false` and restarts the bff before it stops and disables the director, and leaves
   the director's state directory in place.
8. **The web UI.** Designer's brief, then nodes on the Overview and node-scoped pages.
9. **Packaging.** In `-core`: the director's unit (`User=private-ai-director`, `StateDirectory=` 0700,
   `RuntimeDirectory=`, the hardening of the bff's unit) and its sysusers entry; checked by `xtask package`
   and its contents: the unit is in `-core`, not enabled by any preset or maintainer script, and restarted
   on upgrade only when active; installing is the user's run, then the end-to-end check on two machines
   (one promoted).
10. **The model store** (after 9). The manifest, ingestion from Hugging Face with the sha256 check, the
   pull listener, `StoreFetch` on the node with staging, hash and rename, range resume, `GET /api/store/files`.
   Done when, with a test CA over loopback: a node without a valid certificate is closed before any
   byte; a node with one fetches a file whose sha256 matches and it appears in `GET /api/node/model-files`; a corrupted body
   is refused `store-hash-mismatch` and leaves no file at the target; a cut transfer resumes from its
   partial; a full store refuses `store-full`; a manifest with a bad signature is refused.
11. **CA rotation** (after 10). Done when: with the bundle {old, new} pushed, nodes accept the
   director's leaf from either CA; renewals then come from the new CA; the bundle {new} leaves no node
   that renewed unable to connect, and a node still on the old CA alone reads `untrusted`.

What can wait: per-node rights, a central identity, nodes that dial out (NAT), a hardware CA key, gRPC streaming
for live logs, the director on Windows, promote and demote in the GUI, a CLI for the node list beyond
promote and demote, the gateway across nodes
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
  budget timeouts per hop (node < director < bff < the web UI's), which slice 9 of step 10
  already needs for long stops.
- **The revision** (`PLAN.md` step 10 slice 8): key the hash before it crosses a network, as already
  planned.
- **A protocol version** (built: Part 1, "Acting for a signed-in user"): `Version` answers the
  framework version and protocol 1 to anybody; nothing calls it in production yet.

### Questions only the user can answer

1. **Reachability.** Can the director always open a connection to each node (same LAN or VPN)?
   The design assumes yes; nodes that must dial out (behind NAT) need a different, larger transport.
2. **Rights.** Is "every administrator of the director (member of `pai` on its server) administers
   every joined node, after the node's own administrator approved the join" enough for now, or must
   rights differ per node from the start?
3. **The listener.** Is a network port opened by the manager itself (off until a node joins, mutual
   TLS before anything is read) acceptable, or must the manager never listen on the network (then SSH,
   option D, or a relay in the node's bff)?
4. **Platforms.** The director on Linux only at first, with Linux and Windows nodes?

Answers (user, 2026-11-15): 1. yes, the nodes are reachable (same LAN or VPN); 2. enough for now;
3. the manager's listener is acceptable; 4. Linux only at first, with Linux and Windows nodes.

The model store (2026-11-21), recommendation first: 5. **Direction**: the nodes pull from a listener on
the director (it must be reachable from them); or the director pushes over the node's
listener. 6. **Ingestion**: the store takes files from Hugging Face (sha256 checked against the tree)
and by upload; or Hugging Face only. 7. **Quota**: one cap on the store; or a cap per node as well.
Answers (user, 2026-11-21): 5. the nodes pull; 6. from Hugging Face (sha256 checked) and by upload;
7. one cap on the store, no per-node quota.

The private CA (2026-11-21), recommendation first: 8. **The CA key at rest**: an encrypted systemd
credential (host key, plus TPM2 when present) with a passphrase-encrypted offline export; or a plain
0600 file. 9. **A node whose certificate expired**: it joins again with a new code; or the director
renews an expired certificate within a grace period (for example 30 more days) when the node is not
denied, which weakens revocation by as much. 10. **Key type**: ECDSA P-256; or Ed25519. 11. **CA
lifetime**: 5 years, rotated at 4; or 10 years.
Answers (user, 2026-11-21): 8. a plain 0600 file (not the architect's recommendation), with the optional
manual export; 9. it joins again, no grace; 10. ECDSA P-256; 11. 5 years, rotated at 4.

One binary, two roles (2026-11-22), notes that block nothing, the default taken: 12. **Packaging**: the
director's unit and account in `-core`, disabled (default); or a small `-director` package so plain nodes
carry no director account. 13. **Demote and the CA**: demote keeps the director's state, so a promote
restores the fleet (default); or demote also deletes it, every node then joining again.

The node's manager without root or LocalSystem (2026-11-22), notes that block nothing, the default
taken: 14. **The remaining privileged part**: a helper with three fixed verbs per OS (default); or keep
those operations by hand (an administrator runs `systemctl enable` and the web UI loses instance
creation on Windows), with no privileged code at all. 15. **Windows helper**: always running and idle on
its pipe (default); or started on demand by the manager. 16. **Plan order**: at the head of step 14
(default); or as a step of its own before step 13's remaining work.
