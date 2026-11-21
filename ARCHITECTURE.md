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
the revision without the key (a log, a super-manager view, a future non-administrator), and FNV makes
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
instance on another machine is, with the super-manager, writing the preset into that node's pool (an
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
  nodes is the super-manager's concern (Part 2), not built now.
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
   - The memory check still counts every card of the node, whatever `device` says (user, 2026-11-17).

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
     those a profile's `device` names (the 2026-11-17 rule stands); no refresh: the page shows each
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
all the node's GPUs and is checked alone against them (a per-instance `Devices` key waits until asked).
On the revision's three questions (user, 2026-11-17): server profiles keep the file name
`server@<profile>.ini`; the CLI is `pai model-profile <verb>`; a server profile created from the web
starts from the defaults.

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
