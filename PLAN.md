# Plan

What the user asked for, when work counts as done, and what is left to do, in order. Rules in
`AGENTS.md`, under "Working from this plan".

This plan is not set in stone: the coordinator or the user can change any part of it when needed.

## User requirements

These stay here when met: unlike steps, they are not removed for being done. "Met" means checked as far
as the column says. A requirement still open must be covered by a step.

### Product

| ID | Requirement | State |
|---|---|---|
| R1 | Build llama.cpp (newest release tag, plus `patches/llama.cpp/`) with CUDA, Vulkan and ROCm/HIP on Windows and Linux; every phase a cargo command | Met: release 2.4.0 |
| R2 | Package it: an MSI on Windows, split `.deb` and `.rpm` on Linux, self-contained but for the GPU drivers | Met: release 2.4.0 |
| R3 | The same on macOS, Apple Silicon only (Metal + Vulkan through MoltenVK, split `.pkg`) | Minimal build (Metal only) built, installed and reinstalled on GitHub's macos-15 runner (step 12); Vulkan, the GUI and a model run never run on a Mac |
| R4 | A GUI + CLI configurator (`llama-cpp-config`, becoming `private-ai-config`: step 2) for `server.ini` and the per-model `presets.ini` | Met: release 2.4.0 |
| R5 | Every measurement in `benchmarks/bench.db`, under a versioned spec, public: no credentials, no local paths | Met |
| R6 | Per-GPU and per-model guides where every value traces to a campaign, published as a site | Met, but their figures predate v0.9.0: step 5 |
| R7 | The fastest presets for the test machine's GPUs: port an out-of-tree optimization when its gain pays for its upkeep | Open: steps 4 to 9 |
| R8 | llama-server runs as an OS service per instance (`llama.cpp@<id>` on Linux, config `server@<id>.ini`, a Windows service) managed by the runtime service `private-ai-manager`; the OS starts at boot the instances enabled for it (systemd, the SCM start type), set through the manager, which is what the GUI and CLI talk to (D-Bus plus polkit on Linux, a named pipe with an ACL on Windows); units started by hand stay under its management; the manager owns every check on the config it writes and the GUI is a thin client; later engines (Strata) are to fit the same manager | Open: step 1 |
| R9 | The project is `private-ai-framework`, its executables `pai-config` and `pai-manager` (decided 2026-11-13), with upgrades from the old packages and the user's data carried over | Open: step 2 |
| R10 | The GUI and the CLI are two executables over one shared library: GUI `pai-config`, CLI `pai`, the shared logic in a new crate without Slint (decided with the user on 2026-11-13) | Open: step 3 |
| R11 | A backend-for-frontend (`bff`) running as a local service beside `private-ai-manager`, in the same packages: the web UI of the configurator (a `fe` it serves), a gateway in front of the llama-server instances (one OpenAI-compatible endpoint, authentication, routing across instances and later engines), and the benchmark portal over `bench.db` (decided with the user on 2026-11-14) | Open: step 10 |
| R12 | The framework is built and versioned apart from its engines: each engine (llama.cpp today, Strata later) is named explicitly and has its own package at the engine's version, the framework's packages carry only the framework's version and a minimum-version dependency on the engine (decided with the user on 2026-11-14) | Open: step 11 |
| R13 | The director: an optional component, installed on its own server, that governs several `private-ai-manager`s on other machines (nodes) and hosts the models they download (user, 2026-11-21); a bff connected to it also offers the management of the nodes (user, 2026-11-15) | Open: step 14 |
| R14 | A minimal install image (like the appliance images hypervisors ship) with only the node's `private-ai-manager` installed; that manager then installs the packages the node needs (the engines: llama.cpp, Strata). Asked by the user on 2026-11-22, last in the order. | Not started: step 15. |

### How we work

| ID | Requirement | State |
|---|---|---|
| W1 | Test driven, always: tests first, then the code | Applies to every step |
| W2 | Keep it simple | Applies to every step |
| W3 | One agent each for coordination, code, tests and documentation | Met: `.opencode/agents/`, plus the read-only `spec-reviewer` |
| W4 | AI scaffolding as standard as possible, readable by other harnesses | Met: `AGENTS.md`, shared role files, `.agents/skills/` |
| W5 | A maintained plan: closed points removed, evidence in the commit log, so an agent starting from zero knows what to do | Met: this file and `git log` |
| W6 | The coordinator or the user can change the plan when needed | Met: rule in `AGENTS.md` |
| W7 | Each agent has its own skill, saying how it works in this project and the pitfalls met so far, which it can update itself or the coordinator can | Met: `.agents/skills/` |
| W8 | Everything in English; only the conversation with the user may be in their language | Applies to every step |
| W9 | Displayed text in a dedicated file, ready for more languages | Met: `crates/config/strings/` |
| W10 | Agents run only the tests of the part they work on, plus linter and format check; the user runs the full suite once, on main after the merge, before installing | Applies to every step |
| W11 | No em-dashes anywhere in the repo ("Key conventions" in `AGENTS.md`) | Applies to every step |
| W12 | Every change to a benchmark spec is reviewed by `spec-reviewer` before it is committed | Applies to every step |
| W13 | The coordinator only coordinates: the tester writes and reruns the tests, the coder runs the linter and the format check, the coordinator reads their reports and commits | Met: `AGENTS.md` "Roles", `.opencode/agents/` |
| W14 | Every release until 3.0.0 is a 2.99.x pre-release (2.99.0, 2.99.1, ...), decided on 2026-11-13; 3.0.0 is the user's call | Applies to every release |
| W15 | Windows-only code is type-checked on Linux by the coder, `cargo check` and `cargo clippy` with `--target x86_64-pc-windows-gnu`, for pai-core, pai-app, pai-cli, pai-bff, pai-manager and xtask (MinGW installed by the user; no crate code uses `cfg(target_env)`); pai-config is left out because slint's Skia renderer builds only with MSVC, so its Windows code is checked by the user's build on Windows (user, 2026-11-13) | Applies to every slice ("Commands" in `AGENTS.md`) |

## Definition of Done

Every step must meet all of this, as well as its own "Done when". What cannot be checked is reported
as not checked, and the step stays open.

1. **Test first.** Each behaviour has a test that was seen failing for the right reason, then passing.
   A step that only measures has no code to test: its evidence is the campaign in bench.db.
2. **All green.** The agent commands under "Commands" in `AGENTS.md` pass: the tests of the part that
   changed, the linter and the format check. The full suite is the user's, after the merge.
3. **Tried for real.** Behaviour is checked in the real thing, not only in tests: the configurator run
   (CLI or GUI) with `LLAMA_CPP_CONFIG_DATA_ROOT` on a scratch directory; an xtask phase with
   `--dry-run` where it has one, otherwise run by the user; a guide through
   `cargo run -p docs-site -- --check`; a measurement as a `bench run`, with the user's go-ahead.
4. **Safe.** The user's config and data (`server.ini`, `presets.ini`, `settings.ini`, `bench.db`) are
   never lost or silently rewritten; an upgrade keeps them. The HIP variables are never set
   machine-wide. Nothing private reaches the committed bench.db. Failure paths are tested first.
5. **Requirements hold.** The step meets those it covers and breaks none of the others. Their "State"
   above is updated.
6. **Language rules hold.** Everything is in English, with no em-dashes, and displayed text is in its
   strings file.
7. **Docs are true.** README, the `AGENTS.md` files and `docs/` match what now exists. Facts have a
   source, and guesses are marked as guesses.
8. **Plan and history.** The step is removed from this file, in the commit with the work, whose message
   says what was closed and how it was checked.

## Where things stand

**The web UI's MVP** (agreed with the user, 2026-11-15): on a real install, a member of `pai` signs
in, sees the instances, starts, stops and restarts them, edits and saves the server file, sees the
presets and switches them on or off, and (user, 2026-11-17, relayed by the web UI session and
confirmed here) opens a preset and saves it. Built on the bff (2026-11-17, the preset save the last); left:
the preset save on the fe (2h), then the user's run on their machine. After it: 11d,
11f.

Release 2.4.0 bundles llama.cpp v1.0.0 with patches `0001`, `0005` and `0006`; `0002`, `0003` and `0004`
were dropped at v0.9.0 and performance dropped with them (step 4). The test machine has two R9700 (one in
an x4 slot with no AtomicOps); the RTX 4070 SUPER is out of it, so the mixed-pair measurements wait for it.
No run in bench.db is on v0.9.0 or later. What was checked among the out-of-tree candidates
(`overdoingism/llama-cpp-rdna-boosts`), and found superseded or not applicable, is in
[patches/llama.cpp/candidates.md](patches/llama.cpp/candidates.md): read it before reopening one.

## Next steps

### 1. Run llama-server as a service under `private-ai-manager`

Covers: R8.

Today `llama-cpp-config` spawns llama-server as its own child (`runstate::start`, `stop`, `is_running`,
called by the GUI, the CLI's `server` commands and the benchmarks). That moves into OS services.
Decisions taken with the user on 2026-11-13:

- **Instances have an ID, not a port, as their name.** Each instance has its own config file,
  `server@<id>.ini`, and its own unit, `llama.cpp@<id>`; the port is a key inside the file like any
  other. The coordinator chose this over naming by port (the user left it open, 2026-11-13): changing
  the port, a common edit in the Server tab, would otherwise rename the unit, the config file, the log
  and the state, with a stop and start in between. The ID is short, lowercase letters, digits and `-`
  (safe in a unit name with no escaping); the first instance, and the one a migrated config becomes,
  is `default`. The ID also leaves room for two instances on the same port bound to different
  interfaces (the user's case, 2026-11-13): the manager refuses two instances only when their address
  and port collide, a wildcard address (`0.0.0.0`, `::`) colliding with every address on that port. `presets.ini` stays one file shared
  by every instance (no per-instance option until one is needed, W2).
- **Linux.** A template unit `llama.cpp@.service` (`llama.cpp@default`). It runs as a dedicated system
  user (`llama-cpp`, `private-ai` after step 2; in the GPU groups) and its `ExecStart` is a launcher that reads `server@<id>.ini`
  itself and execs llama-server with `runstate::server_args()`, so a unit started by hand
  (`systemctl start llama.cpp@default`) runs the same command as one the manager starts.
  `private-ai-manager.service` owns the D-Bus name on the system bus, lists the instances it
  finds as `server@<id>.ini` files (systemd starts at boot the enabled ones) and starts, stops and reports them through systemd's
  own D-Bus API. It reads instance state from systemd every time and keeps none of its own, so a unit a
  user started or stopped with `systemctl` is still one it manages. Every call that changes something
  is checked with polkit (`CheckAuthorization`, subject = the caller's bus name).
- **Windows.** The same split as Windows services, registered by the MSI (WiX `ServiceInstall`). The
  GUI and CLI reach the manager over a local named pipe whose security descriptor is the polkit
  counterpart: SYSTEM, Administrators and interactive users.
- **macOS** stays on today's direct spawn: R3 is untested and launchd is not in scope here.
- **Scope of the first round:** the serving instance only. Benchmarks and the HF fit probes still spawn
  their own short-lived llama-server; where they stop and restart the instance today
  (`bench/exec.rs`, `bench/sweep.rs`) they go through the manager instead.

**Requirement change: the server config moves out of the user's tree.** A service started at boot
cannot read `~/.config/llama.cpp/`. `server.ini` (renamed `server@<id>.ini`, one per instance) and
`presets.ini` (and `presets.ini.disabled`) move to
`/etc/llama.cpp/` on Linux and `%ProgramData%\llama.cpp\config\` on Windows, written only through the
manager. `settings.ini` (the configurator's preferences) and `bench.db` stay per user. An existing
per-user config is migrated once, never lost (Definition of Done 4).

Slices, in order, each test first (slice 1, the system config directory and the instance paths in
`paths.rs`, and slice 2, the launcher `private-ai-manager run-instance <id>`, are done; the numbers of
the others stay as they are cited). The `llama-cpp-config` crate now has a library target (`src/lib.rs`)
so the second binary, `private-ai-manager`, shares its modules: a `[[bin]]` alone could not reach them.

3. **The manager behind a seam.** Only `private-ai-manager` writes the INI files of the system
   directory (`server@<id>.ini`, `presets.ini`, `presets.ini.disabled`): it exposes reading and writing
   them over D-Bus / the pipe, under polkit / the ACL. Every writer in the configurator goes through
   it: the Server tab's save, the presets editor and `presets::set_enabled`, the benchmark sweep that
   sets a key and puts it back, the migration. The configurator never writes those files itself; the
   launcher only reads them. Until this slice the configurator keeps writing its per-user files, since
   the manager does not exist yet.
   Split in three. **3a, done:** the OS-neutral core, `manager::service::Manager` over the
   `ServiceControl`, `Authority` and `Resolver` traits (polkit check first on every change, the file
   whitelist, atomic writes, collisions on resolved addresses with a wildcard colliding with every
   address on its port, start at boot), tested with fakes. **3b, done:** `private-ai-manager serve`, the
   D-Bus service on the system bus (`org.privateai.Manager1`, zbus 5 blocking) with the systemd and
   polkit implementations of the traits, tested over a peer-to-peer connection with the fakes; the
   systemd and polkit proxies and `serve` itself wait for a real run (slice 5). Left: `serve extra`
   prints the run-instance usage hint.
   The manager answers while polkit waits (done: a write lock inside the core taken after
   authorization, `async fn` D-Bus methods running the core off zbus's executor thread).
   **3c**, in four pieces, each test first: **3c-1** (done) the content checks in `pai-core`
   (`checks.rs`), refused by the manager with a strings key and its values; **3c-2** (done) the client `pai_core::client` with its D-Bus
   implementation; **3c-3** (done, `server_control.rs`) the
   configurator picks a backend, the manager on Linux and today's direct spawn and per-user files on
   macOS and, until slice 6, Windows, and `runstate`'s callers (GUI, CLI `server`, benchmarks) go
   through it for the `default` instance; **3c-4** (done) every INI reader and writer goes through it. The
   configurator's existing per-user `server.ini` and `presets.ini` are handed to the manager by slice 7.
   The manager owns every check on the
   content it writes (decided with the user on 2026-11-13): before a write it runs all the checks the
   configurator runs today (`server_form.rs`, the presets checks) and refuses invalid content with an
   error carrying a strings key and its values, which the configurator shows in its own language. The
   configurator is a thin client: it keeps no check of its own; it may call the same shared code for
   feedback while the user types, but the manager's answer decides. The Windows SCM and pipe
   implementations come with slice 6.
4. **Logs, through journald** (user, 2026-11-13; done on the configurator's side: the unit itself
   comes with slice 5). On Linux the unit writes both streams to the journal
   (`StandardOutput=journal`, systemd's default), journald rotates them, and `LogRotate` and
   `LogRotateKb` no longer apply there (they stay for the Direct backend). The GUI's Log window and
   the tray's Open log read `journalctl -u llama.cpp@<id> -f -o cat` through the backend seam;
   reading a system unit's journal needs root or the `systemd-journal`, `adm` or `wheel` group
   (journalctl(1)); without it the window says so through a strings key. The first users of Ubuntu and
   Fedora are usually in `adm` and `wheel`: check on the user's machine. Windows keeps a file under
   `%ProgramData%\private-ai\logs\` (slice 6).
5. **Linux packaging.** The `-llama` package ships the two units, the D-Bus policy
   (`/usr/share/dbus-1/system.d/`), the polkit action file (`/usr/share/polkit-1/actions/`) and a
   sysusers.d entry for `llama-cpp`. This adds the first maintainer scripts to the `.deb` and `.rpm`
   (enable the manager, reload systemd), so the "no post-install script" rule in `AGENTS.md` changes.
   Checked with `xtask package` and the contents of the packages; installing them is the user's run.
   **Requirement change (user, 2026-11-13): the Linux packages use the standard layout under `/usr`,
   not `/opt/private-ai`.** `pai-config` and llama.cpp's executables (`llama-server`, `llama-bench`,
   ...) in `/usr/bin` (the packages keep their Conflicts with the distros' llama.cpp, so no file is
   shared); `pai-manager` in `/usr/libexec/private-ai` (internal: FHS 3.0, adopted by Debian Policy
   9.1.1, and Fedora), called by the units by full path; libllama, the ggml backends and the bundled
   ROCm, CUDA and Vulkan runtimes in the private `/usr/lib/private-ai` (RPATH), never in the system
   library directories; data in `/usr/share/private-ai` (the benchmark specs and prompts, the licences),
   the desktop entry and icon where they are today. The per-executable links in `/usr/bin` go. The
   configurator's install lookups (`llama-server`, `share/benchmarks`) follow. macOS keeps
   `/opt/private-ai`.
   llama-server finds its ggml backends in `/usr/lib/private-ai` through `GGML_BACKEND_DIR`, compiled in
   (user, 2026-11-13); the footgun, a tree-built llama-server loading the installed backends first while
   the package is installed, goes in the footguns of `docs/build-packaging.md`. Split in **5a** (done: the
   layout, `pai-manager` in `-llama`) and **5b** (done: units, D-Bus policy, polkit, sysusers, the first
   maintainer scripts; `xtask install` is not blocked by the services, which the scripts restart). Left:
   the user's real run (`xtask package`, `xtask install`, then the step's "Done when" checks).
6. **Windows.** Split in four, each test first: **6a** (done) the OS-neutral protocol the manager serves
   over the named pipe (`pai_core::client::pipe`, `manager::pipe`: a u32 length and a JSON body of at
   most 1 MiB, the D-Bus method and error names), tested on Linux over a socket pair; **6b** (done, type-checked only) the
   Windows-only parts (the pipe server and client with the security descriptor, the caller from the
   client token, the Authority over it, the SCM `ServiceControl`, `serve` and `run-instance` as
   services), type-checked here and tried on the user's Windows build; on Windows `control` is any
   interactive user and `configure` a member of the local group `pai` (user, 2026-11-15; Administrators
   until 1e4094a), elevated or not (user, 2026-11-13: no prompt can cross the pipe); each instance's service and virtual account are named
   `llama.cpp_<id>` (`NT SERVICE\llama.cpp_<id>`: `_`, not systemd's `@`; user, 2026-11-14), the files keep
   `server@<id>.ini` and `llama-server@<id>.log` on both OSes; the manager service and its pipe are named `private-ai-manager`
   (`\\.\pipe\private-ai-manager`), as the Linux unit; Stop kills llama-server outright (user, 2026-11-14); **6c** (done; its Windows arm compiled first by the user's build) the configurator's
   manager backend on Windows and the log file; **6d** (done, checked on the .wxs text; the installer test plan on Windows is the user's run) the
   MSI, below.
   **Windows packaging.** The MSI registers the manager service (automatic start) and creates
   `%ProgramData%\private-ai\` with the right ACL. SCM has no template services, so the manager creates
   one service per instance (`llama.cpp_<id>`, manual start) when the instance is added and removes it
   with the instance; the uninstall removes them. An upgrade keeps the config. Runs the installer test
   plan of `installer/AGENTS.md`.
7. **Migration** (done on the configurator's side; tried for real with the packages of slice 5).
   When the backend is the manager, the user has a per-user `server.ini` and some per-user file has no
   system copy, the GUI offers the import at start (Import / Not now, asked again next start) and
   `pai config import` does it: `server@default.ini`, then `presets.ini`, then
   `presets.ini.disabled` (the manager checks presets against the instances' `ModelsMax`), exact bytes,
   never overwriting a system file, stopping at a refusal and resuming on the next run; the user's
   files are left in place.

Points of design, and what is still open:

- The service user must be able to read `ModelsDir` (and write it, as `LLAMA_CACHE`, for downloads by
  name). A models directory under a home with mode 0700 is not readable by `llama-cpp`: the manager
  reports it rather than changing permissions (agreed with the user on 2026-11-13).
- Polkit defaults, agreed with the user on 2026-11-13: start, stop and status for the active local
  session without a password, config writes `auth_admin_keep`.
- `SaveStateOnShutdown` and `StateDir`: the slot snapshots are written by the server, so by the service
  user; their default directory moves with it.
- `OpencodeApiKey` sits in `server@<id>.ini` (user, 2026-11-14): the server files are mode 0640
  root:private-ai (the instance still reads them; on Windows the ACL counterpart), and reading them
  through the manager needs the `control` authorization (done: the manager writes them so, with
  the group set before the rename, and refuses a save when the group is missing; the preset files stay
  0644 and open to read; `/etc/llama.cpp`, where they now live, and `/etc/private-ai` stay 0755). A file an earlier version wrote 0644 is
  tightened at its next save.
- Windows accounts and logs (user, 2026-11-14): each instance runs as its virtual account
  `NT SERVICE\llama.cpp_<id>`, LocalService the fallback if GPU compute fails from session 0; a
  ModelsDir it cannot read is reported, and the MSI may create a default models folder under
  `%ProgramData%\private-ai\models` readable by it (6d). The instance log is a file keeping one
  previous run; the Event Log is not used (llama-server's output is too verbose for it), and a launch
  error is written into the instance log.
- The D-Bus name and polkit action ids, agreed with the user on 2026-11-13: `org.privateai.Manager1`,
  `org.privateai.manager.*`.
- **A stop that hangs** (seen 2026-11-23, a deploy's `prerm upgrade`): `systemctl stop llama.cpp@default`
  ran out `TimeoutStopSec=300` and systemd sent SIGKILL. The router's child (`Qwen3-Embedding-0.6B-Q8_0`)
  was force-killed and exited within 11 s, but the router process itself never exited (journal of
  `llama.cpp@default`, 17:45:45 to 17:50:45). Find out whether llama-server's router mode, the
  manager's `run-instance` or `save-slots` holds the stop, and make a stop end without the kill.

- Done when: on Linux, after installing the packages, the instance in `server@default.ini` starts at
  boot, `systemctl start/stop llama.cpp@default` and the GUI agree on its state, and a user without
  authorization is refused by polkit; on Windows the same with the services and the pipe; the user's
  previous config has been migrated without loss; `AGENTS.md` and the guides describe the new layout.

### 2. Rename the project to private-ai-framework

Covers: R9.

Decided with the user on 2026-11-13: the project becomes `private-ai-framework` (it was
`llama.cpp-framework`), the configurator `private-ai-config` (it was `llama-cpp-config`); today it supports
llama.cpp, later other engines (Strata). Done between slices 3b and 3c of step 1, so 3c and the packaging
slices are written with the new names. The GitHub repository rename, and with it the Pages site URL, is
the user's to do.

What changes:

- **Code:** the crates and their directories (`crates/`, slice 1), the binary, the window title and the
  strings that name the product; `private-ai-manager` already has its final name.
- **Packages:** `private-ai-framework-config`, `-llama`, `-rocm`, `-cuda`, `-vulkan` (`.deb`, `.rpm`), the
  macOS `.pkg` names, the MSI `private-ai-framework-v<version>-v<llama tag>-x64.msi`. An upgrade replaces
  the old packages: `Replaces`/`Conflicts`/`Provides` on the `.deb`, `Obsoletes`/`Provides` on the `.rpm`,
  the same `UpgradeCode` on the MSI (so MajorUpgrade still removes the old product).
- **Paths:** install `/usr` (Linux, its `private-ai` subdirectories), `/opt/private-ai` (macOS) and
  the Windows install dir; user data `~/.config/private-ai`, `~/.local/share/private-ai`, `%LOCALAPPDATA%\private-ai`; the registry key; the
  system config dir of step 1 (`/etc/private-ai`, `%ProgramData%\private-ai\config`), and on Linux the engine's
  `/etc/llama.cpp` for the instance files (`paths::engine_config_dir`). The engine's names
  keep `llama.cpp`: the unit `llama.cpp@<id>`, `llama-server`, the `-llama` package's contents.
- **Migration:** the first run of the new version moves the old user tree to the new one once, when the
  new one does not exist yet, and leaves the old one untouched if anything fails (Definition of Done 4).
- **Not rewritten:** the committed `benchmarks/bench.db`: its recorded software names are history.
- **Docs:** README, every `AGENTS.md`, the guides, `PLAN.md`.
- **AI scaffolding:** the role files (`.opencode/agents/`), every skill in `.agents/skills/` (role
  playbooks and task skills: today `coordinator.md`, `coder-playbook`, `tester-playbook`, `bench-spec`,
  `optimize-preset` and `write-docs` name the old project or crate), `.claude/settings.json` and
  `.opencode/opencode.json`, the commands and paths the skills give (`cargo test -p private-ai-config`,
  the data root variable), so an agent starting from zero finds nothing stale.
- **Repository, the user's action:** rename it on GitHub (Settings > Repository name). GitHub redirects
  the web pages and git operations to the new name but not the Pages site, whose URL changes, so every
  link to the site and `.github/workflows/docs-site.yml` are checked; never create a new repository
  under the old name (it breaks the redirects); `git remote set-url origin` on each clone. Renaming the
  local folder too moves Claude Code's per-project memory, which is keyed by the folder's path: copy it
  over when that happens.

Decided with the user on 2026-11-13: the service user is `private-ai` (it was `llama-cpp` in step 1);
the environment variables become `PRIVATE_AI_DIST_ROOT` and `PRIVATE_AI_DATA_ROOT` (they were
`LLAMA_FW_DIST_ROOT` and `LLAMA_CPP_CONFIG_DATA_ROOT`), with no fallback to the old names.

Slices, in order:

Slices 1 to 5 are done (`git log`): the crate layout (`crates/core`, `crates/manager`, `crates/config`,
packages `pai-*`, one workspace version), the names that behave (executables `pai-config` and
`pai-manager`; display name "Private AI Framework"; identifiers `org.privateai.Config`; paths
`private-ai` under `~/.config`, `~/.local/share`, `%LOCALAPPDATA%`, `/etc`, `%ProgramData%`, `/opt`,
`C:\Program Files`; registry `Software\private-ai`; every `LLAMA_FW_*` variable now `PRIVATE_AI_*`)
the migration of the old user tree and start-at-login entry, and the packages
(`private-ai-framework-*`; deb `Conflicts`/`Replaces`/`Provides` and rpm `Obsoletes`/`Provides` of
the old package of each component; the MSI keeps its UpgradeCode, keeps a directory chosen in 2.4
and moves the old default; `installer/private-ai-framework.wxs`).

What is left is the user's: the in-place upgrade of an installed 2.4.0 on each OS, the Windows
build and installer test plan ("Waiting for the user"), and renaming the GitHub repository (then the
repository URLs in `README.md`, `docs/README.md` and `tools/site/src/main.rs` follow). The specs
`standard-1.0.ini` and `standard-1.1.ini` keep the old binary name in a comment: frozen by sha256,
they change only with a new spec version.

- Done when: nothing outside the engine's own names and bench.db history says `llama.cpp-framework` or
  `llama-cpp-config`; the packages and the MSI upgrade an installed 2.4.0 in place (checked by the user on
  each OS); a user's existing config and data are found after the upgrade.

### 3. Split the configurator into a GUI and a CLI

Covers: R10.

Decided with the user on 2026-11-13, to do after step 1 reaches parity: the GUI keeps the name
`pai-config` (desktop entry, start at login, single instance and MSI shortcut unchanged); the CLI
becomes `pai` (`pai bench run`, `pai preset enable ...`), short to type over SSH. The logic both share
(presets, benchmarks, HF, GGUF, the configuration, the server control seam) moves into a new crate
without Slint, `crates/app` (package `pai-app`); `pai-core` stays what the manager links (paths, INI,
checks, client, launch). Why: a headless server administered over SSH needs the CLI without Slint,
Skia and the graphics libraries; on Windows a console executable replaces the console attach of
today's one binary; the CLI and the shared crate become type-checkable on the Windows GNU target
(W15), which Skia blocks today.

Slices, each test first: **1** (done: `pai-app` holds the shared modules, with a `test-support`
feature for the fakes pai-config's tests use; it passes the Windows GNU check given
`PRIVATE_AI_LLAMA_CPP_DIR`); **2** (done) `crates/cli` (package `pai-cli`,
binary `pai`) with today's CLI, `pai-config` left GUI-only (no argument parsing beyond what the GUI
needs, no console attach); **3** (done) packaging: `pai` in its own `-cli` package on Linux and macOS,
`/usr/bin/pai`, `pai.exe` on the MSI's PATH, and `installer/msi-stop-processes.ps1` running `pai.exe
control stop-and-close`; **4** (done) docs, skills and guides from `pai-config <cmd>` to `pai <cmd>`,
and how `pai` is built, packaged, installed and put on PATH on each OS.

Left: "the packages ship both" holds in xtask's code and tests only. No package with `pai` has been
built yet (`dist/` holds only 2.4.0 packages, `xtask/src/macos/package.rs` is not compiled on Linux):
the Linux packages in step 1's real run, the MSI in the Windows build of step 2 (the installer test
plan's `pai --version` and graceful-stop checks). The step closes with that run.

- Done when: `pai` runs every command `pai-config` runs today with the same output; `pai-config` opens
  the GUI only; `pai-app` and `pai-cli` link no Slint and pass the Windows GNU check; the packages ship
  both; the docs name `pai` for commands.

### 11. Decouple the framework from its engines

Covers: R12. Comes before step 1's real run, so the packages installed for real are the new ones.

Decided with the user on 2026-11-14:

- **The engine is its own package, named after it.** Linux: `llama.cpp`, at version
  `1:<release>+pai<N>` (`1:1.0.0+pai1`): the engine's release, `+pai<N>` our build revision (raised
  when anything but llama.cpp's code changes on the same release: the patches, the build options, the
  package's scripts or files; back to 1 at every new llama.cpp release; user, 2026-11-14), and the epoch `1:` because Ubuntu
  26.04 ships its own `llama.cpp` at `8681+dfsg-1`, which apt would otherwise rank above `1.0.0`. It
  holds llama.cpp's executables and libraries, the ggml backends and libomp, nothing of the framework.
  It must refuse to sit beside the distro's packages that carry the same files (`llama.cpp-tools` on
  Ubuntu, `llama-cpp` on Fedora: check the names and files in the slice) and replaces 2.4's
  `llama-cpp-framework-llama`.
- **The runtimes belong to the engine:** `llama.cpp-rocm`, `llama.cpp-cuda`, `llama.cpp-vulkan`,
  versioned by their SDK as today, recommended by `llama.cpp`; each replaces its 2.4 predecessor.
- **A new `private-ai-framework-core`** holds what the framework puts in `-llama` today: pai-manager,
  `private-ai-manager.service`, the D-Bus and polkit policies, the sysusers entry, `cli.toml` and the
  manager's maintainer scripts. Every framework package requires it.
- **The engine's instance unit belongs to the engine** (user, 2026-11-14): `llama.cpp@.service` ships in
  the `llama.cpp` package, which stops its running instances before its files are replaced and starts
  them again afterwards. Its `ExecStart` stays `pai-manager run-instance %i`, which the engine does not
  require (core requires the engine, not the reverse): the unit carries
  `ConditionFileIsExecutable=/usr/libexec/private-ai/pai-manager`, so without the framework it does not
  run. A later engine brings its own unit the same way (`strata@.service`).
- **The engine's configuration lives in the engine's directory** (user, 2026-11-14): on Linux the
  instance files (`server@<id>.ini`, `presets.ini`, `presets.ini.disabled`) move from `/etc/private-ai/`
  to **`/etc/llama.cpp/`**; `/etc/private-ai/` keeps the framework's own (`bff.ini`). The manager reads
  and writes them there, with the same modes and checks. The `llama.cpp` package ships the defaults
  read-only in `/usr/share/private-ai/llama.cpp/` (`server@default.ini` with a few starting values,
  an empty `presets.ini`) and its postinst copies each to `/etc/llama.cpp/` only when missing
  (`server@default.ini` 0640 root:private-ai), so an upgrade never touches what the user or the manager
  wrote and dpkg asks nothing; the instance `default` then exists from the first install, disabled at
  boot. No migration between pre-release layouts (user, 2026-11-14): nothing moves files
  from `/etc/private-ai/`. Windows and macOS: to settle with the
  user in their slices (the same split would be `%ProgramData%\llama.cpp\`).
  **The format belongs to the framework, the engine ships only starting values** (user,
  2026-11-14): the engine's `server@default.ini` holds a few `[Server]` values (port, host, models
  folder) and no copy of pai's template; the manager reads it like any file (missing keys take pai's
  defaults) and rewrites it in the installed pai's template at its first write. A change of pai's
  template no longer touches the engine package, and the engine's starting values no longer follow
  pai's render (the test that pinned the file equal to the render becomes "the manager accepts it and
  reads these values").
- **Framework packages carry only the framework's version** (`2.99.1`, no `+llama.cpp...`) and
  require `llama.cpp (>= 1:<release built against>)`; a newer llama.cpp installs without a framework
  rebuild. Package files: `private-ai-framework-<component>-v<version>-<arch>`, and
  `llama.cpp-v<release>+pai<N>-<arch>` for the engine (and its runtimes by SDK version).
- **Windows: two installers** (user, 2026-11-14, replacing "one MSI with two features"): the
  framework's MSI, named by the framework's version only (`private-ai-framework-v<version>-x64.msi`),
  and **llama.cpp's own MSI** with its runtimes, named and versioned by the engine's release alone, no
  `+pai<N>` (user, 2026-11-14): `llama.cpp-v<release>-x64.msi`, ProductVersion `<release>`;
  each with its own UpgradeCode, upgraded on its own. The framework's MSI refuses to install without
  llama.cpp at the minimum version it was built against (a launch condition over the engine's
  registration). llama.cpp's MSI has **its own install folder** (e.g. `%ProgramFiles%\llama.cpp\`),
  **owns the instance services** `llama.cpp_<id>` and ships **its own default configuration**
  (`server@default.ini`, an empty `presets.ini`, read-only, copied into the engine's configuration
  folder, e.g. `%ProgramData%\llama.cpp\`, only when missing; `paths::engine_config_dir()` follows on
  Windows). Two small Windows manager fixes go with it (found 2026-11-14): `windows::entry::serve`'s doc
  comment still says "or started", and `scm.rs` `sync` reports an SCM-wide failure with an empty
  instance id ("instance ``"). (Slice 3a done: the Rust paths below and the two manager fixes. Slice 3b done, type-checked and
  text-checked only, never compiled by WiX nor run: `installer/llama.cpp.wxs` and the framework's
  `.wxs` without the engine, `xtask package|install` per product on Windows; llama.cpp's MSI removes a
  2.4 install; the framework no longer carries over a 2.4 install folder. Slice 3c done the same way (type- and text-checked only): llama.cpp's MSI stops the running
  instances before an upgrade or repair and starts them again (`instances.running` under
  `%ProgramData%\llama.cpp`), keeps the instance services on a downgrade through `xtask install`
  (`KEEPINSTANCES=1`), guards against a non-MSI `Uninstall\llama.cpp` entry and creates `state\`;
  Windows instances without ModelsDir/StateDir use `%ProgramData%\llama.cpp\{models,state}`; `xtask
  install <engine> <framework>`; two stale mentions gone. Accepted as is: llama.cpp's MSI takes a
  2.4-chosen install folder. Left: the real WiX build and the installer test plan on
  Windows (the user); under the manager the Server form shows both folders blank with the instance
  default as hint (user, 2026-11-14).) Folders (user, 2026-11-14): the program in `%ProgramFiles%\llama.cpp\` (with the
  read-only defaults); everything of the engine's instances under `%ProgramData%\llama.cpp\`:
  `config\` (`server@<id>.ini`, `presets.ini`, `presets.ini.disabled`; `paths::engine_config_dir()`),
  `logs\` (`llama-server@<id>.log` and its `.previous.log`) and `models\` (the default `ModelsDir`);
  `%ProgramData%\private-ai\` keeps only the framework's (`config\bff.ini`). Uninstalling llama.cpp's
  MSI stops and deletes the instance services and **asks whether to remove its configuration too**
  (user, 2026-11-14): a checkbox, cleared by default, as the framework's user-data question; a silent
  or basic-UI uninstall, and every upgrade, keeps `%ProgramData%\llama.cpp\`.
- **macOS:** the same split of the `.pkg` (an engine `.pkg` beside the framework's).
- **The build is split too:** the engine and the framework are separate builds, so either can be
  rebuilt alone. The framework still reads the engine's source at build time (`pai-app`'s architecture
  tables); what bench.db records as the software stack must come from the installed engine, not from
  the framework's build: an open point for the slice.

The 2.99.x packages were never published, so no upgrade relation names them. Requirement change: R1,
R2 and the release process (the `release` skill) (installer name, release title) follow this.

Asked by the user on 2026-11-14: **the packages stop the services they hold** before their files are
replaced, on upgrade as on removal, and start again afterwards what was running: `core` stops the
manager, `llama.cpp` its running instances (`llama.cpp@*`), `bff` its service (deb `prerm upgrade`, rpm `%pre`
of the new package, since rpm runs the old `%preun` after the new files are in place), and their
`postinst` / `%posttrans` start the manager and bring back the instances that were running.
`xtask install` then no longer refuses because of the framework's own services: only a llama-server
outside them (started by hand) stops it; an open `pai-config` or `pai` is named in a warning, not a
refusal.

Found after slice 1: the engine package had no maintainer scripts, so upgrading `llama.cpp` alone left
running instances on the old llama-server; settled by the engine owning its unit and its scripts
(above).

Slices, each test first: **1** (done) the Linux split (`core`, the engine and its runtimes,
versions, relations, file names; a non-release llama.cpp build is refused; the runtimes' copyright
files and the engine's homepage are llama.cpp's); **2** (done, Linux) the packages stop the services they hold (the engine its instances, with its unit), and the build split in xtask (`xtask build|package [llama|framework]`); **2b** (done, Linux) the engine's configuration in `/etc/llama.cpp/` with its defaults copied when missing; **3** the two Windows installers (the framework's and llama.cpp's);
**4** (done, type-checked only: the engine's `.pkg` in `/opt/llama.cpp`, `org.privateai.llama.cpp`, `llama.cpp-v<release>-arm64.pkg` and its `-vulkan` runtime; the framework's `config`, `cli`, `core` in `/opt/private-ai`; `xtask package` per product, `install` the engine first; left: a 2.4 macOS install in `/opt/llama.cpp` is not removed) the macOS `.pkg`; **5** (release process written in the `release` skill) docs and the release process: **one GitHub release per framework release, with every package** (the framework's and llama.cpp's at the release it was built against, on every OS), the title kept as `vX.Y.Z (llama.cpp vA.B.C)` (user, 2026-11-14); a llama.cpp-only rebuild (`+pai<N+1>`) goes out with the next framework release or as re-uploaded assets. Open: nothing pins the llama.cpp release across the three build machines (`common/llama.rs` takes the newest tag at build time), so one release could carry different engines per OS.

- Done when: `xtask package` on Linux makes the framework packages at `2.99.1` requiring `llama.cpp (>=
  1:1.0.0)` and the engine package at `1:1.0.0+pai1`; the engine or the framework can be rebuilt alone;
  the two Windows installers and the `.pkg` follow the same split; the docs describe it.

### 13. One set of checks for the GUI, `pai`, the manager and the bff

Found by the web UI session's read-only inventory of the configurator (2026-11-14, section 3 of its
`configurator-inventory.md`, kept in `fe/briefs/`): some checks live only in the Slint layer, so `pai`,
macOS (no manager) and a future bff write what the GUI would refuse. The bff's server and preset writes
(step 10, slices 7 and later) need these checks in `pai-app` or `pai-core`, not copied into `fe/`.

Slices, each test first, in order:

1. **Left from slice 1**: `slot_state::save_all` (the snapshot save before a stop) still creates and
   prunes the user's own state folder under the manager, where the instance's account owns the state
   folder: decide whether the configurator saves there at all. (Done: under the Linux manager, as on
   Windows, an unset ModelsDir or StateDir is not written and means the instance folders; the shared
   `/var/lib/llama.cpp/models` decided with the user on 2026-11-15, created by the engine's tmpfiles.d
   entry; `pai server show` and the restore read the instance folders.)
2. (Done: Port, ModelsMax not negative, ModelsMemoryMargin, LogVerbosity 0..5 and the bind list
   (`bind-all-with-others`) in `pai-core` `checks.rs`, which the manager runs on a write; threads at most
   the CPU count in `pai-core` `server_cfg::check` (moved from `pai-app` in step 10 slice 8.2), run by every save; the base URL normalised in one
   helper shared by the GUI and the save. Left, small: a hand-edited file with `ModelsMax = -1` makes
   every unrelated `pai server set` fail, and `pai` has no way back to unset; a negative
   `--models-max` could clear it, as `--models-memory-margin` does; its `--help` still says "Stored
   as-is".)
3. (Done: `validate_for_save` requires a model, holds integer keys to their range, refuses non-finite
   floats, values outside the enums, `spec-type` lists llama.cpp would not split, non-object
   `chat-template-kwargs` and min > max pairs, and the MoE cache floor from the model's header; the
   manager's file check runs the same `checks::preset_values` (not the floor, which needs the GGUF);
   a bench sweep runs the save's check on every probe before any GPU time. Optional: the GUI's floor
   hint and the save measure tensor bytes differently, which can differ by 1 MiB only for a made-up
   GGUF.)
4. (Done, user 2026-11-15: the manager blanks `OpencodeApiKey` for a caller without `configure`,
   asked without a prompt, and says so (`ManagerFile.withheld`); a write with `keep_api_key` puts the
   stored key back, which pai-app asks for once a read in the process was withheld and the text has no
   key; `pai server show` prints `(set, withheld)`, the GUI a note; the Integrations tab leaves
   opencode.json's `apiKey` as it is. Accepted: a process that saw the key withheld cannot clear it; an
   administrator sees it only while polkit keeps an authorization (no read prompts). The bff reads as
   itself until step 10 slice 6b, so a web administrator gets the withheld document too.)
5. **Docs and strings**: the "Adding a per-model preset field" checklist (doc comment atop
   `crates/app/src/presets.rs`) to name `checks::PRESET_FLOAT_KEYS`, `PRESET_ENUM_KEYS` and
   `PRESET_MIN_MAX_KEYS`, or a new key goes unchecked; the README's stale draft pin, Model info and `DefaultSpinBox` mentions; the
   literals that break "no text in code" (`model_scan.rs`, `gpu_split.rs`, `bench/exec.rs`,
   `bench.rs`); GUI keys a non-GUI client needs copied to `cli.toml` when that client shows them.

Accepted as is unless the user says otherwise: greyed-out preset values still written (only the draft,
ngram and MoE keys are pruned); GUI and CLI defaults of the benchmark differ; delete with shared files
(GUI keeps them, CLI refuses without `--even-if-shared`): the web UI picks the GUI's side.

- Done when: each check above is enforced below the GUI, with a test through `pai-app` (and the
  manager where it applies), and the docs say what is checked where.

### 14. The director governing several managers

Covers: R13. Asked by the user on 2026-11-15: a component above the managers, on a different server,
that governs several of them; a bff connected to it is offered node management as well.

Revised by the user on 2026-11-22: the director is a role of the one `pai-manager` binary
(`pai-manager director`, its own unit and unprivileged account: `ARCHITECTURE.md` part 2, f3c7cde), and
the web UI is one interface for manager and director (N1's Nodes and Profiles menu). Promoting and
adding nodes from the web: a **Nodes page** (an overview of the nodes: name, state, version, instances)
with a **+ Add node** button; on a single node the + explains that this machine becomes the director,
asks to confirm, promotes it (`pai node promote`, which also stays on the command line for headless
nodes and scripts), then asks for the one-time code made on the other node. Designer's brief first. A dedicated
director (a small machine that runs no models) is the recommended setup (user, 2026-11-22): the + flow
says so, with the reason, before promoting a node that runs instances (a fully compromised machine
gives away the CA with it), and the install docs present it first; promoting such a node stays allowed.

The node's manager without root or LocalSystem (user, 2026-11-22: "sia per Linux che per Windows";
`ARCHITECTURE.md` part 2, "The node's manager without root or LocalSystem", accepted by the user on 2026-11-22): the manager runs
as `private-ai-manager` (Linux) and `NT SERVICE\private-ai-manager` (Windows); a small privileged
helper keeps the few verbs that cannot be scoped (Linux: password check, boot on/off, director on/off;
Windows: create/repair, delete, boot on/off of instance services). Slices P1 to P6 at the head of this
step, before slice 1, which would otherwise create the node key as root. Models are then read only
where the instances can read them (`/var/lib/private-ai/models`, or a folder readable by `private-ai`).

Design accepted by the user on 2026-11-15: `ARCHITECTURE.md` part 2 (a separate unprivileged
process, the `director` role of `pai-manager`; the manager protocol over mutual TLS 1.3 with pinned fingerprints, off on a node
until its administrator enables it; join by a one-time code; every `pai` member of the director's
machine administers every joined node; Linux director first, Linux and Windows nodes). The
slices, each test first, with the outcome each shows, are in `ARCHITECTURE.md` "Slices":

1. Node identity (`pai node show|enable`). 2. The TLS listener with pinned fingerprints. 3. The join
   (code, `Join`, `leave`). 4. Delegated rights and per-change logging of who acted. 5. The
   director: its key, the node list, adding a node. 6. Its local socket, open only to the bff's
   account. 7. The bff's hub mode against a fake director, local mode unchanged. 8. The web UI
   (designer's brief first). 9. Linux packaging, then the user's run on two machines.

- Done when: a bff in hub mode lists two joined nodes and starts and stops an instance on each, on
  the user's two machines, and every local-mode test still passes.

Added (user, 2026-11-21): **the director hosts the models for every node** and offers the joined
nodes an endpoint to download them into their local models folder. To design with the architect (who
may download, over which channel and identity, integrity checks, storage and quotas on the
director, resumable transfers, how it relates to a node's own Hugging Face downloads), then a
slice after 9; the done-when gains: a node downloads a model from the director into its models
folder and an instance serves it.
Decided (user, 2026-11-21): **the director is the name** (since 2026-11-22 a role of `pai-manager`, `pai-manager director`, not a binary of its own: above; started by its own unit under its own account, not as a child of the manager, which without root could only give it the manager's account and so the CA key: confirmed by the user on 2026-11-22); **its links
with the nodes are secured by a private CA in the director**: at the join (one-time code) a node sends a
CSR and gets a short-lived certificate (about 30 days) renewed by itself over the mutual TLS channel;
the director's own certificate comes from the same CA; revocation is a refused renewal plus a deny list.
This replaces the pinned fingerprints of the 2026-11-15 design. mTLS 1.3 encrypts and authenticates
every link; signatures only where proof is needed afterwards: the director's commands recorded in a
node's log and the model store's manifest. The architect revises Part 2 and its slices 2 and 3.
Also (user, 2026-11-21): no offline root (a compromised director is already total compromise); a node
joined to a director accepts no communication that is not authenticated with the director's
certificates it holds in its cache (no fallback, no plain or unknown peer); renewal is automatic, well
before expiry.
Answers (user, 2026-11-21): the CA key in a 0600 file on the director; an expired node joins again (no
grace); ECDSA P-256; the CA lasts 5 years, rotated at 4. Designed: ARCHITECTURE.md Part 2 "Identity and
joining" (slices 1 to 5, 3b, 10, 11). Renewal (user, 2026-11-21): node and director certificates last 90
days and renewal starts 30 days before expiry; while the node is connected it retries every 5 minutes
(fixed, no backoff); a node that was disconnected tries at once when it reconnects.
Decided (user, 2026-11-21, ARCHITECTURE.md "The model store", slice 10): nodes pull from the
director over the step's mTLS channel; the store is filled from Hugging Face (checksum checked)
and by upload; one size cap on the store.

### 4. Decide whether to bring back patches 0002 to 0004

Covers: R7.

Patches `0002`, `0003` and `0004` were removed to reduce the maintenance cost, in the same change that moved
to v0.9.0, and performance dropped. Swift-1.5-Qwen3.8-27B on 2x R9700, Linux, preset temperature, one
repetition per prompt on 2026-11-04 (a quick check, not recorded in bench.db), against the 5 repetitions of
campaign `swift27b-standard-1.0-2xr9700-linux-20261031` on v0.8.0 with the patches:

| Prompt | Prefill t/s, before | Prefill t/s, now | Decode t/s, before | Decode t/s, now |
| --- | --- | --- | --- | --- |
| 43k | 1125.4 (1124.5-1128.2) | 1077.2 (-4.3%) | 59.3 (57.0-60.8) | 56.1 (-5.4%) |
| 110k | 960.1 (958.7-962.1) | 893.4 (-6.9%) | 59.2 (48.7-67.0) | 59.6 (+0.7%) |

The prefill drop is outside the spread of the old runs. Decode at 43k is just below it on one repetition and
decode at 110k says nothing, so decode is still open. Another job was running on the machine during the check.

**Not established yet: how much comes from the missing patches and how much from v0.9.0.** The two changed
together. To separate them:

1. Repeat both prompts on an idle machine with temperature 0 and no drafter (the `engine` pass of the standard
   spec; v0.8.0 recorded prefill 1210.3 and 1025.5, decode 27.1 and 24.5 t/s).
2. If the drop holds, build v0.9.0 with the patches back and measure again, one patch at a time.
3. Weigh each patch's gain against its upkeep, and bring back the ones that pay.

**Bringing a patch back.** Take it from the last commit that has it (`git show 54396fb:patches/llama.cpp/<name>`),
with its section of `patches/llama.cpp/README.md`, and refresh it for v0.9.0. `0002` applies as it is. `0003`
needs a context change in `fattn-mma-f16.cuh` (`nullptr` became `indices`, #29510). `0004` loses the hunk adding
`<algorithm>`, which upstream now includes. Bringing `0003` back also means restoring the `HipFaBand` option of
the configurator, removed with it.

- Done when: each of the three patches is either back (refreshed, measured, recorded in bench.db) or
  left out with the measurement that says it does not pay, and `patches/llama.cpp/README.md` says which.

### 5. Re-measure the presets on the current release

Covers: R6, R7.

Do it after step 4, on the patch set it settles on: the figures below were written for v0.9.0
without patches 0002 to 0004.

Every recorded figure the guides hold from v0.7.0 and v0.8.0 ran with the FlashAttention band (0003),
the sampler probe (0002) and, from 2026-10-21, host buffers pinned for every runtime (0004), except
campaigns `27b-kv-f16-20261016`, `coder390-device-order-20261019`, `coder390-windows-vs-linux-20261020`,
`flash-next-moe-cache-20261016` and `flash-next-no-host-20261016`. No run in bench.db is on v0.9.0. The
guides say so under their "In short" and in a placeholder row; both go, and the figures change, once
this is done.

- Run the two-R9700 presets under the same spec version as their baselines: the Swift 27B on Linux and
  Windows against campaign `swift27b-standard-1.1-2xr9700-linux-20261101` and
  campaign `swift27b-standard-1.1-2xr9700-windows-20261101`, Swift Flash-Next on Linux against
  campaign `swift-flash-next-standard-1.1-2xr9700-linux-20261101`. The mixed pair needs the RTX 4070 SUPER
  in the test machine.
- The biggest unknown is decode with accepted drafts: the band's gain at 2 to 4 queries per step rests
  on `test-backend-ops`, and no run that accepts drafts has a leg without the band. For Flash-Next, repeat
  the preset of campaign `flash-next-mtp-graphs-2xr9700-linux-20261030` (`spec-draft-n-max` 3: 54.9 t/s,
  488 of 821 drafts accepted) at 43.7k, same text, temperature 0, 3 repetitions.
- Check that both legs write the same reply before comparing decode: a different reply changes the
  acceptance, and with it the rate. The tile kernel's split changes with the query count, so MTP and
  unassisted text may part (`docs/qwen3.8_27b.md` section 1).
- That Flash-Next preset's KV is q8_0 (MTP does not load at f16 with split 1,1) and the GQA 12 `perf`
  figures are f16, while at GQA 6 quantized K/V gained nothing on the band's path: a null result may be
  the KV type.
- A change of release sits in the same comparison, v0.8.0 to v0.9.0, so a difference is not the patches
  alone.

- Done when: each preset above has a campaign on the current release, and the guides' figures, their
  "In short" note and the placeholder row are updated from it.

### 6. Find out why the drafter benchmarks look too favorable

Covers: R6, R7.

From `docs/qwen3.8_flash_next.md` section 5 (2026-10-30).

The live sweeps rank Flash-Next's drafters far above what the coding agent showed live
(n-gram alone 274.5 t/s and the MTP chain 280.1 on the code prompt, MTP +77% on prose, against
+8% for the n-gram drafter measured on real agent turns at 130-160k). Not yet explained; check
before trusting the size of the gains (the ranking itself may hold):

- Temperature 0 (greedy) against the preset's temp 1 / top-k 20: greedy output is what the
  drafters predict best, so acceptance (98% on the code prompt) may be far above real sampling.
  Checked on Swift-1.5-Qwen3.8-27B at its temperature 1 (2026-10-30, campaigns
  `swift27b-drafter-2xr9700-linux-20261030` and `-code1536-`): the drafters still double prose
  decode (15.4 to 28.8-32.2 t/s) and the n-gram chain gives 120-127 against 18.6 on the code
  rewrite, so sampling alone does not explain the gap; not yet repeated on Flash-Next.
- The code prompt is a pure copy task (rename one function, output the file): the best case
  for an n-gram drafter, not an agent turn. Replay recorded agent turns instead.
- What the "decode" rate counts: the answer phase reads 348-375 t/s, above what a verify
  batch per token can run on this model; check the engine's timing against wall time and the
  server's own `timings` (`predicted_per_second`, drafted / accepted counts).
- Cold start per leg and a 128 to 6000-token output cap: the rate may be taken over a short
  window that favors the drafter. Partly checked on Swift (same campaigns): with 256 tokens out the code
  reply ends inside the reasoning and the n-gram stage shows no gain; with 1536 it does.
- Depth: the prose prompt is 43.7k; the agent runs at 100-180k, where the MTP head's own
  attention and the verify batches cost more.

- Done when: the gap between the sweeps and the live agent turns is explained by a measurement, and the
  guide states the drafter gains under the conditions that hold for real use.

### 7. Check the KV-split width patch on the current release

Covers: R7.

`upstream/UPSTREAM-PR-fa-kv-split-width.patch` pins the KV split for `Q->ne[1] <= 8` so a verify batch
reduces the KV like plain decode. Found not applicable while patch 0003's band was in; without the band
the R9700's decode takes the branch it changes. What was measured is in
[candidates.md](patches/llama.cpp/candidates.md), "KV-split width determinism".

- Done when: it is known whether the patch applies to the current tag and what it changes on the R9700
  (byte-identical replies across `spec-draft-n-max`, and the speed cost), and it is ported or dropped.

### 8. Compare block 12's all-reduce with upstream's internal AllReduce

Covers: R7.

Dropped on 2026-10-21 as not applicable, when the machine was one R9700 plus one 4070 SUPER. It
now has two R9700s and the Linux presets that split by tensor use the internal AllReduce, so the
block's mechanism applies. The fork's current README calls it block 01 (V2.0 numbering), not the
block 01 of "Adaptive MTP depth" under "Later, not planned yet".

The block is not RCCL alone: `auto`, its default on Linux/HIP, takes an internal pipeline for the
small tensors reduced at every decode step, and picks the large-tensor path once per model: the
copy engine for a dense model, RCCL's ring when experts sit in host memory. Its figures, copy engine
against RCCL (the link of these runs is not stated; read through a summary of the first 100k of 223k
characters of `patches/README.md`, so check them against the original before citing them in a guide):

| shape | copy engine | RCCL |
| --- | --- | --- |
| 27B `pp2048` | 2178 | 2018 |
| Flash-Next all in VRAM, decode | 67.2 | 59.9 |
| Flash-Next IQ4_NL, 2 GPUs, `-ncmoe 20` | 604 | 809 |
| Flash-Next IQ4_NL, 2 GPUs, `-ncmoe 32` | 414 | 555 |

Enabling peer access alone cost 42.25 to 38.77 t/s of dense decode on 3x R9700, and it cannot be
undone once RCCL has made its mappings.

- Compare the block's small-tensor path and its copy engine with upstream's internal AllReduce
  (`ggml/src/ggml-cuda/allreduce.cu`): what is left to gain over v0.9.0 is not known. This part
  needs no RCCL, so it could run on the current board. RCCL itself is under "Later, not planned yet".

- Done when: block 12's small-tensor path and copy engine are measured against
  `ggml/src/ggml-cuda/allreduce.cu` on the two R9700 (no RCCL needed), and ported or dropped.

### 9. Settle the open Windows checks

Covers: R6, R7.

From the Windows work of 2026-10-21, `docs/qwen3.8_27b.md` section 9.

- `tensor-split 53,13` for the Windows Coder390 preset at ctx 180224.
- The HIP API log on Linux: is the Linux prefill lead the copy rate alone?
- A pinned staging buffer in `ggml_backend_tensor_copy`'s slow path (the cross-card hops staged
  through `malloc`: 2.5 s of the 5.24 s of blocking copies per 43.7k prefill with the R9700 second,
  measured on v0.7.0; the other 2.7 s were what patch 0004, since removed, took away).
- The Swift preset's device order on Windows: prefill is settled, the `real` decode lead of the
  x4 card first needs repetitions ([Qwen3.8-27B, section 9](docs/qwen3.8_27b.md#9-not-yet-measured-placeholders)).
- The Swift preset's prefill gap to Linux on Windows: Memory Integrity, `ThreadsBatch` and the
  power plan do not explain it; the hypervisor and the driver and HIP costs are unmeasured
  ([Qwen3.8-27B, section 6](docs/qwen3.8_27b.md#6-linux-against-windows)).

- Done when: each check above has a measurement in bench.db, or a recorded reason it cannot run here,
  and the guide's placeholders are filled or retired.

### 10. A backend-for-frontend: web UI, gateway and benchmark portal

Covers: R11. Started once the steps the coordinator can do alone are done (user, 2026-11-14).

**When step 10 is done (user, 2026-11-21):** cut the first pre-release, 2.99.1 (W14), with the `release`
skill: every OS's packages, the release commit, the tag `v2.99.1` and a GitHub pre-release (the tag and
the release still get the user's go-ahead at that moment, "Actions with real effects"); then bump the
framework to 2.99.2 and carry on toward it.

Decided with the user on 2026-11-14: one local service on the machine, installed beside the manager by
the same packages, listening on localhost and, when configured, on the LAN with authentication. It
serves three things: the configurator's web UI (what `pai-config` does: server, presets, start and
stop, logs, benchmarks), talking to `private-ai-manager` like the GUI does; a gateway in front of the
instances; the benchmark portal over `bench.db`. The `fe` is developed with React and Vite in TypeScript and
built to static files (user, 2026-11-14). The rest is the coordinator's proposal, to review with the
user: the bff in Rust (axum, over `pai-core` and the `pai-app` crate of step 3), the `fe` built at
package time and embedded in the bff, so one binary, the same tests and the
same packaging.

Slices, each test first (written by the coordinator on 2026-11-14; names, port and account are
proposals to review with the user):

1. (done) **The service skeleton.** A crate `crates/bff` (package `pai-bff`, binary `pai-bff`) over
   `pai-core` and `pai-app`: axum on `127.0.0.1:8590` by default (the address from its own config,
   nothing on the LAN yet), `GET /api/version` (the framework version) and a placeholder page at `/`
   embedded in the binary; tests bind an ephemeral port. It links no Slint (the lock-walk lint).
2. (done) **The `fe` scaffold.** `fe/` at the repo root: React + Vite in TypeScript, one page that reads
   `/api/version`; `xtask build` builds it with npm (already a build requirement for the icon) and
   the bff embeds `fe/dist`; the bff's tests use a fixture when `fe/dist` is absent.
3. (done) **Packaging.** Left for the user's runs: on Windows a bad `bff.ini` or a bind failure of
   the service is reported only as an SCM start failure (no log yet: write it to a file as the
   instances do, a later slice), and whether the bff's virtual account reads `config\` (the
   ALL SERVICES check in `installer/AGENTS.md`).
4. (done) **The first read-only page.** The instances and the state of each (`ListInstances`, `Status`: the
   calls the manager answers without authorization), shown read-only through `private-ai-manager`.
   The bff's account must reach the manager: the D-Bus policy on Linux, the pipe's SDDL on Windows
   (today SYSTEM, Administrators and interactive users only). The server settings are not shown yet:
   reading them needs `control`, which waits for the design of who is acting.
5. (done) **The web UI's own checks.** `npm --prefix fe run typecheck` (the tests included), `lint`
   (ESLint, typescript-eslint, React hooks rules) and `format:check` (Prettier), in "Commands" of
   `AGENTS.md`; the coder runs and reports them for every `fe/` change. `fe/` is on TypeScript 6
   (below, "Later, not planned yet").

Every slice that changes what users see starts with a brief from the `designer` agent and ends with its
review (user, 2026-11-14).

Decided with the user on 2026-11-14 (relayed by the web UI session, confirmed here):

- **Who is acting.** The user logs in to the web UI with their OS account (PAM on Linux, `LogonUser` on
  Windows); the bff forwards that identity to `private-ai-manager`, which applies today's rules
  (`control` for any user, `configure` for administrators). Before this exists, nothing changes
  anything through the bff.
- **Engines.** Each preset declares its engine (llama.cpp today, Strata later); the web UI shows one
  mixed preset list, and each engine keeps its own file (llama.cpp's `presets.ini` as today). Only
  llama.cpp for now. A preset with no engine declared is a llama.cpp preset, so nothing migrates and
  today's `presets.ini` stays valid as it is (user, 2026-11-14, relayed by the web UI session). The
  engine is chosen from the list of available engines, not typed (user, 2026-11-14, relayed): the bff
  exposes that list (e.g. `GET /api/engines`, today one entry) and a check refuses a write naming an
  engine outside it. The coordinator's choice: a preset without the key is read as llama.cpp and the
  key is not written back for it, so no file changes until the user picks another engine.
  Corrected on 2026-11-15: a preset names its engine by the file that holds it, never by a key;
  llama-server refuses an unknown key in `--models-preset` (llama.cpp v1.0.0
  `tools/server/server-models.cpp:415` leaves `ignore_unknown_keys` false; `common/preset.cpp:325`
  throws), so an `engine` key would stop it loading its presets.
- **First editing slice.** The whole UI's structure in `fe/DESIGN.md`, then the Server page of instance
  `default` with the same controls and checks as `pai-config`. The web UI session (`fe/` only) sends
  the coordinator the bff API contract it needs (login and session, reading and writing the server and
  preset files, start, stop, restart, with error keys) and the checks that live only in the Slint layer
  and must be exposed from `pai-app`, the manager or the bff rather than copied into `fe/`; the bff
  side is scheduled from that contract.

The web UI's side (the web UI session, `fe/` only): slice 1a done (e19f3d2: the shell with hash routes,
sign-in against `/api/session`, the read-only Server page, built against mocks; not yet tried against a
real bff until d9bbe90's hand check, which found no mismatch); 1b done (4255577: editing and save for
administrators, save bar, client checks, refusals beside their field, 409, 401 mid-edit; against
mocks, no PUT route yet); 1c part 1 done (396d25c: Start, Stop, Restart and live state, against
mocks); 1c part 2 done (47e2c67: Start at boot, against mocks), so fe slice 1 is complete on the
web UI's side; the bff routes it needs are slices 6b to 9. Models page (`fe/briefs/02-models.md`):
2a done (4731d88: the read-only preset list at `#/models`, against mocks of slice 11); 2b done
(c06c90f: a filter by preset id or model path); next 2c (an administrator's on/off switch per row,
which needs a write route for a preset's on/off state, contract to come). Open with the user:
Back, Forward or a typed address leave a dirty form without asking.

The bff side of the web UI's slice 1, from the contract `fe/briefs/api-contract.md` (the web UI
session's, following the coordinator's shape; the fe is built against mocks), each test first:

6. (6a done: `GET /api/instances` items `{id, engine, state}`, errors `{key, args}`; 6b part 1 done: the
   session routes over an `Authenticator` seam, the shipped bff answering 503 `session-unavailable`
   until the real backend; left for the backend part: an abandoned request's check keeps running
   unrecorded (bound concurrent checks), LogonUser spellings of one account (`.\user`, `HOST\user`,
   `user@host`) need one lock key, a per-address cap across user names (spraying), `Secure` and
   `__Host-` on the cookie once the bff serves HTTPS: since 2026-11-17 it may listen on the LAN in plain HTTP). 6b part 2a done: the manager's `Authenticate(user, password) -> (authenticated, can_configure)`
   (D-Bus `(ss)->(bb)`, pipe `Authenticate`), answering only the bff's account (Linux: the uid of
   `private-ai-bff`; Windows: its service SID), through PAM service `private-ai` (Linux, root manager:
   `unix_chkpwd(8)` checks only the caller's own password, so the bff cannot) and `LogonUserW` network
   logon (Windows), `can_configure` = member of `pai`. 6b part 2b, Linux, done: the core package
   ships `/etc/pam.d/private-ai` (a conffile; `.deb` `common-auth` + `common-account`, `.rpm`
   `system-auth`: Fedora's `other` is `pam_deny`), the empty group `pai` (sysusers.d), and the polkit
   rule `50-private-ai.rules` (`configure`: `auth_self_keep` for `pai`, `no` otherwise; the action's
   own defaults `no`); root still configures (polkit always authorizes uid 0); a polkit without
   JavaScript rules (0.105) leaves only root. 6b part 2b, Windows, done: the framework MSI creates the
   empty local group `pai` and never removes it (`util:Group RemoveOnUninstall="no"`: Util's default
   deletes it at every uninstall, so a MajorUpgrade would empty it); the pipe's `configure` is `pai`
   enabled in the client's token (looked up per connection as a local alias of this computer only,
   never a domain group or user named `pai`) or LocalSystem; `control` adds `pai` members; the
   installers workflow checks the group and that a member survives repair and uninstall (green on
   run 43743455408). Left from it (no string names Administrators, checked):
   `client-not-authorized` could say how to join `pai`; rename `ADMIN_GROUP` and
   `PasswordChecker::is_admin` (they read as Administrators); the doc comments of `PIPE_SDDL` (manager
   `windows.rs`) and `crates/bff/src/server.rs` near line 48 still describe the old rule. 6b part
   2c done: the bff's `SystemAuthenticator` calls `Authenticate` (a client per check, bounded by
   `SIGN_IN_TIMEOUT`, 10 s: a timeout is 503 and counts nothing); a right password of a non-member
   is 403 `session-not-administrator`, counted in its own refusals list per (lowercased user, IP),
   5 per window then 429, apart from the bad-password failures. Left from its review: after a
   timeout the blocking thread keeps running, so a hung manager piles threads up to tokio's pool
   (the "bound concurrent checks" item above); the session reply is `{user}` (canConfigure dropped); `Identity.can_configure` and the bff's own `OpencodeApiKey` withholding are gone (every session is a `pai` member; the manager's flag alone withholds the key); Step 10 slice 6b part 3 (the bff calls the manager for the
   signed-in user) is built (adab61c, 1d2e12c, b2f3777, c03d911, 4216745 and the bff commit; ARCHITECTURE.md
   "Acting for a signed-in user"). Left from its reviews: after a `BadRequest` or `Unavailable` the pipe
   connection is spent for every `acting_for` sibling, so a caller opens a new one (say so in
   `PipeClient::version`'s doc); a bound call against an old manager is a plain `Other`; `BAD_REQUEST`
   is defined in both `client/pipe.rs` and `manager/pipe.rs`; `DbusClient::version` matches only
   `zbus::Error::MethodError` for `UnknownMethod`, which zbus 6 reports as `Error::FDO`: match both
   before a zbus bump; the doc comment of `requester_of` (`manager/dbus.rs`) still says the D-Bus face
   carries no principal; `error_kind` repeats the table of `error_reply` (`service.rs`); a test with
   U+2028 in a caller would pin the quoting; the Windows log
   `%ProgramData%\private-ai\logs\private-ai-manager.log` grows without a cap (about 100 bytes a
   record; a cap later), is readable by Users like the engine's logs (kept so, 2026-11-16), and
   outlives an uninstall with `config\` (documented in `installer\AGENTS.md`; CI runs no guarded call, so
   it cannot see the file). From
   the review of 2a, still open: a UPN or `.\user` passes `LogonUserW` but `NetUserGetLocalGroups`
   may not find it (fails closed: refused as not a member; fix with the spellings item above); read
   back `PAM_USER` after `pam_authenticate` (modules may fold case); set `PAM_RHOST`; `PAM_MAXTRIES`
   is an error, arguably a refusal; the password is not zeroed after use (Microsoft asks for it on
   `LogonUserW`); a network logon needs "Access this computer from the network", which hardened
   baselines deny to local accounts. **Sign-in.** `GET`/`POST`/`DELETE /api/session` (`{user, canConfigure}`, an HttpOnly cookie,
   `session-bad-credentials` 401, `session-too-many-attempts` 429), against the OS accounts (PAM on
   Linux, `LogonUser` on Windows); the manager learns who acts. Decided with the user on 2026-11-15: the
   bff vouches for the user it authenticated: a polkit rule lets `private-ai-bff` call the manager, the
   bff passes the user name, and the manager allows `configure` only when that user is in `sudo` or
   `wheel` (Windows: Administrators), `control` for any signed-in user; no password is kept.
   **Replaced on 2026-11-15 (user):** the web UI is for administrators only, and an administrator is
   a member of the group `pai` (created empty by the packages: sysusers.d on Linux, the MSI on
   Windows), on every client: the manager grants `configure` to `pai` members only (sudo/wheel and
   Administrators no longer suffice), on Linux asking the member's own password and keeping it
   (polkit `auth_self_keep`), on Windows from the token's membership. Sign-in succeeds only for a
   `pai` member (403 `session-not-administrator` after a right password otherwise); the session reply
   drops `canConfigure`; every route acting on an instance or file needs `configure`; the Overview
   (`/api/version`, `/api/instances`, GET boot) needs a session too (item 13a, 2026-11-19); the web UI drops its read-only
   views. Web users' state (user, 2026-11-15, relayed and confirmed): one machine-wide state
   (preferences, `bench.db`, HF token, `opencode.json`) shared by the administrators, not per user; it
   unblocks the Downloads, Benchmarks, Integrations and Settings pages, where it lives is to plan.
   Presets (user, 2026-11-15, relayed and confirmed): `checks::preset_id` refuses `.` and `..` for
   every client; a preset id is unique across engines; deleting a preset from the web offers to
   delete its model files, as the GUI does (fe slice 2n). `GET /api/instances` gains `engine` and moves its errors to `{key, args}` with the
   `client-*` keys, together with the fe build that reads them.
7. (Done: `GET /api/instances/{id}/server`, values by INI key, the instance's defaults, limits,
   interfaces, a revision (a keyed SipHash since slice 8); the bff reads as itself. Left, before the real sign-in: the
   `OpencodeApiKey` goes to every signed-in user, also one with `canConfigure` false, while the
   manager keeps the file at mode 0640 because of that key; decided with the user on 2026-11-15:
   administrators only, anyone else gets null and `withheld: ["OpencodeApiKey"]` when a key is set
   (done). Optional: refuse an id
   failing `valid_instance_id` before connecting; a test for `..%2Fx`.)
8. (Done, 2026-11-17: `PUT /api/instances/{id}/server`, `ARCHITECTURE.md` "Saving the server file from
   the web". Left from it: the docs pass (`crates/bff/README.md`, "Through the manager", the pipe
   client's header and `ARCHITECTURE.md`'s protocol 1 lines; the contract is the web UI session's);
   the control verbs and the boot write (`control.rs`) answer 503 at their time limit without
   cancelling the blocking task, which may still act afterwards: give them the save's
   Idle/Asked/Cancelled state; pai-bff's `pai_bff_binary` failed once of 13 runs, likely its
   `free_port` bind-release-rebind race; the write limit of 15 s (`WRITE_TIMEOUT`, built 2026-11-17 with the 200 for a
   done write whose status read is slow) is an estimate: set it from the first real save.)
9. **Control and boot.** Split (coordinator, 2026-11-16): 9a (`POST start|stop|restart` and
   `GET`/`PUT boot`, answered once the service manager accepted the job) is built; 9b, a stop with `SaveStateOnShutdown` saves the slot snapshots first, designed in
   `ARCHITECTURE.md` "Slot snapshots at a stop" (2026-11-16): the instance saves at its own stop, so
   every stop path does (the web, `pai`, systemctl or the SCM, a reboot); four slices there; built: the save in
   `pai-core` (05a315f, d1234fa) and Linux `ExecStop=pai-manager save-slots %i`; Windows `run-instance` saving at Stop and Preshutdown (3a) and the SCM side (3b: each instance
   service's preshutdown timeout of 300 s, `restart` answering once the stop is accepted, a per-instance
   `RestartGate` so a Stop during the delayed start's wait stands); and the clients saving only on their own
   llama-server (slice 4: `server_control::client_saves_snapshots`, macOS). The architect's review found every
   design point met (ARCHITECTURE.md, "As built"). Left from it: 9d drops the client's restore under the
   manager (on Windows `pai server restart` now returns while the old server still saves, and its restore
   asks that server to load the previous dump); the docs (an MSI repair, upgrade or uninstall stops the
   instances and can now wait minutes; `crates/config/README.md` "Conversation snapshots" and "Through
   the manager"); and the first real run times a save and sets the 240 s and 300 s values from it.
   Seen in a rerun, not this slice's: xtask `the_engine_prerm_records_the_running_instances_and_stops_them`
   failed once (fixture root shared by its loop), green alone and in four reruns. The
   user decided (2026-11-16): before the MVP's run; a stop may wait 5 minutes in all (240 s for the
   save; set from the first measured save); and every instance start restores the newest snapshot,
   as slice 9d after 9b (it also mends the Linux restore, which by the code cannot read
   `/var/lib/private-ai`). The bff keeps its 5 s limit: stop and restart stay job submissions. The routes: `GET`/`PUT /api/instances/{id}/boot`, as the contract's
   "Start, stop, restart, boot" (204 once the job is accepted, the new key `client-service-failed`;
   these reads move no session). The 5 s `MANAGER_TIMEOUT` fits reads only: a save, and a stop
   or restart that saves GiB-sized slot snapshots first, need their own limit, chosen from a measured
   snapshot save (none measured yet), under the fe's 120 s for non-GET calls (fe 19d9594: 15 s for
   a GET, 120 s otherwise, an estimate until this value exists). Optional: answer start and restart after systemd's
   `JobRemoved`, so the first read after a restart is not stale.
9c. **Left from the same-origin guard** (built with 9a's review; the bff refuses a foreign `Origin` or
   `Sec-Fetch-Site` on every method but GET, HEAD and OPTIONS, and any request whose `Host` is not a
   loopback name, which stops DNS rebinding; one 64 KiB body cap): the `Service` message the contract
   says "stays in the bff's log" is dropped (log it, or reword the contract and `control.rs`); the
   manager-error table is in both `server.rs` and `control.rs`: one helper. Before any non-loopback
   bind the `Host` rule must change with it. A page opened through a hostname alias gets the 403 JSON for `/` too, so no
   fe code runs to explain it: a plain answer naming the localhost address would help (not needed now).
10. **Engines.** `GET /api/engines`, `unknown-engine` refused on a preset write. To revisit with item 13:
    a model is assigned to an instance on a node, not to an engine.
11b. (Done, 2026-11-17: `PUT /api/presets/{id}/enabled`, `crates/bff/src/presets.rs`, two
   compare-and-swap writes, destination first. Left from it: `checks::preset_id` refuses `.` and `..`
   (pai-core); `DISABLED_FILE_HEADER` moves to `pai-core` (the bff holds a copy a test keeps equal);
   a missing destination file is created with a plain write, so one created by someone else between
   the read and the write is overwritten (needs a create-if-absent in the manager); the GUI still lists `[*]` (llama-server's
   global section) as a preset and lets it be switched off, after which its id check refuses to
   switch it back on (the web hides it and answers 404 since 2026-11-17); tests missing: `Unavailable` and the time limit between the two writes, asked on while in both
   files, a read-back that fails after a landed move (200), the ids `**` and ` * `; the docs pass (`crates/bff/README.md`, AGENTS.md's overview).)
11c. (Done, 2026-11-17: `GET /api/presets/{id}`, `crates/bff/src/presets.rs` `read`, with the section
   revision `section_revision` the preset save reuses (header through the last non-blank line, without
   that line's ending); the `; url =` reader is in `pai-core` (`ini::project_url_of`). Left from it:
   `values` are the INI parser's, cut at the first `;` or `#` as llama-server reads them, while the
   contract says raw (told to the web UI session: decide before 11e whether the save needs the
   untruncated text); a last line ending in a lone `\r` moves the revision when a `\n` is appended;
   a line `[broken` with no `]` ends the revision's text while the parser reads on; the docs pass.)
11e. (Done, 2026-11-17: `PUT /api/presets/{id}`, `crates/bff/src/presets.rs` `save`, designed in
   `ARCHITECTURE.md` "Saving a preset from the web"; the edits sorted by key; an untouched line already
   in a preset file skips the manager's comment-marker check (decision c1). Left from it, small:
   `parse_sections` should read a value through `value_of_line`; `checks.rs` `known_sections` clones
   both section lists per check (two slices would do); a test for the exemption with a duplicate id;
   the docs pass. Reload of llama-server after a save: a later design; until then `running` drives
   "restart to apply".)
11f. **The configurator's "Saved preset" says when a restart is needed** (from the architect,
   2026-11-16): when the preset is in `presets.ini` and the instance is active, the GUI's `model-profile-saved`
   message adds "restart to apply" (one status read, one string).
11d. **A preset flag reads as llama-server reads it** (found by the web UI session's designer,
   2026-11-16; checked in the v1.0.0 clone: `common/preset.cpp` turns a flag whose value is one of
   `off`, `disabled`, `false`, `0` into its `--no-` form, any other value into the plain flag,
   `common_arg_utils::is_falsey` in `common/arg.cpp`). The configurator reads a preset's booleans with
   `ini::parse_bool` (`model_profiles.rs`), which knows only `true`/`false`, so a hand-written `jinja = 0` runs
   without jinja while the GUI shows the field's default (on). Rule, per key, in `presets.ini`
   (values trimmed of spaces and tabs, as llama.cpp's INI grammar does, `common/preset.cpp`): a plain flag
   (`jinja`, `prefill-assistant`, `mmproj-offload`, `kv-unified`, `reasoning-preserve`, `op-offload`,
   `repack`, `backend-sampling`, `embeddings`, `no-host`) is off for exactly those four values and on for
   any other; `load-on-startup`, the router's own key, is on only for `on`, `enabled`, `true`, `1`
   (`is_truthy`, `tools/server/server-models.cpp`) and off for any other; valued options (`flash-attn`
   and `reasoning` on/off/auto) are not flags and keep their own reading. `fit` is valued too
   (`common/arg.cpp`): on for `on`, `enabled`, `true`, `1`, off for the four falsey values, any other value
   refused by llama-server, and an absent key means llama.cpp's default, on (`common.h` `fit_params`;
   the launch passes no `-fit` then); the configurator today reads `on`/`true` in any case as on and an
   absent key as off. The user decided (2026-11-16, through the web UI session): a preset
   with no `fit` shows Fit to VRAM on, as the engine does; the launch keeps passing no `-fit`. The rule
   that Fit is allowed only with `ctx-size` and `n-gpu-layers` at their defaults is unchanged. `server.ini` and
   `settings.ini` keep `parse_bool`. The web UI applies the same rule (its brief 02e criteria 101, 102, 104). Small, in
   `pai-core` with `pai-app`'s reader; before the MVP's run.
11. (Done, 2026-11-17: `GET /api/presets`, `crates/bff/src/presets.rs`. A `[*]` section lists as a
    preset named `*`, as in the GUI.)

12. **LAN access** (code done, 2026-11-17: `[Bff] Listen` takes any `ip:port`; the `Host` guard
    accepts any IP literal beside `localhost` and refuses every other name; every reply carries
    `X-Frame-Options: DENY` and `X-Content-Type-Options: nosniff`. User, 2026-11-17, a requirement
    change: plain HTTP on the LAN, told the sign-in password then crosses it in clear). Left: build,
    install and `Listen = 0.0.0.0:8590` in `/etc/private-ai/bff.ini` with the user's go-ahead; the docs
    pass (`crates/bff/README.md` lines 3, 19-20, 43-60, `ARCHITECTURE.md` ~29, 84, 97, 182, 245; say the
    session cookie is bound to the host, not the port); pin more hostile hosts in `origin.rs`'s
    (the hostile hosts are pinned since 12b). **12b. The machine's names**
    (code done 2026-11-17; left: build, install, the docs pass, and an item of `[Bff] Hosts` or a
    resolv.conf domain ending in two dots keeps one, e.g. `foo..` gives `foo.`: admin config only)
    (user, 2026-11-17): beside the IP literals and `localhost`, the `Host` guard accepts this machine's
    names, read at the bff's start: its host name, `<name>.local` and `<name>.<domain>` for each
    `search`/`domain` of `/etc/resolv.conf` (Linux; on Windows the computer name and its DNS domain);
    `[Bff] Hosts` (comma-separated names) in `bff.ini`, when set, replaces those automatic names. Any
    other name stays refused. `localhost` and the loopback addresses are accepted when the bff listens on
    a loopback or an unspecified address (`0.0.0.0`, `[::]`), not when it is bound to one LAN address
    only (user, 2026-11-17). A Content-Security-Policy
    is agreed with the web UI session first. Left from the web UI session's UX pass: hashed assets with
    no ETag or long max-age; an unknown `/api/<path>` is 404 with an empty body, not `{key,args}`;
    `/index.html` is 404 (only `/` serves the page, unintended).
13a. **Next milestone (user, 2026-11-18): profiles and instances working end to end**: item 13's
    slices 8 to 17 together (the Profiles menu, several instances on a node, one server
    profile and N model profiles per instance, the resource checks, the web UI). The designer's full UX
    check closes the milestone.
    **First: a visitor who has not signed in sees only the sign-in page** (user, 2026-11-19, a requirement
    change: the Overview, the framework version and the instances' state were shown without sign-in). The bff
    answers 401 `session-required` with no session on `GET /api/version`, `GET /api/instances` and
    `GET /api/instances/{id}/boot`, as on every other route; only the static web UI, `GET /api/session` and the
    sign-in stay open. These reads keep not moving the idle deadline, so a page left open still ends its
    session. The fe shows the sign-in page for every place until a session exists (the web UI session's
    part). The About place (item 13) is behind sign-in by the same rule.
    **Deploy when the web UI's new visual identity is ready** (user, 2026-11-18): once its last slice is on
    `main` and the web UI session says fe is clean, build and install; that request is the go-ahead for this one
    install.
    Seen once each under a cold parallel build, not reproduced in five solo runs: a failure in
    `crates/bff/tests/session.rs` (wall-clock bounds) and one in xtask's `linux::system_files` script tests; the
    cause is not established.
13. **Nodes and instances** (user, 2026-11-17, after the preset cards and LAN access). The web UI's top
    level is the node: this machine under its host name, its instances inside (`default`: llama.cpp);
    the same page takes more nodes when the director comes (R13, step 14). Each instance is
    collapsible; collapsed it shows, for now, only its engine, its port and its tags (user, 2026-11-20: the
    Overview tile is the collapsed view and the instance page the open one, no fold control; the engine
    shows on a tile only when the instances run two or more engines), one or more
    labels the administrator assigns to it (user, 2026-11-17). Built on the bff and the manager (slices 11
    and 12): `GET /api/instances` items `{id, engine, state, port, tags, instanceTags}`, tags in the server
    profile's `[Server] Tags` (read-only in the web UI until the server profile page, user 2026-11-20) and in
    the instance's own `Tags`, set by `PUT /api/instances/{id}/tags`; tags are labels only. The web UI part is
    fe brief 12 (slice T1). Left: a GUI Server save refused for hand-written bad tags has no field to fix
    them (the CLI or the file does).
    The About page, the host name in the navigation and each instance's start at boot in
    `GET /api/instances` are done (slices 13 and N, 2026-11-20). Left from their reviews: the bff reads every
    instance's boot flag inside the list's one read timeout, so many instances or a slow service manager
    turn the whole list into a 503 instead of per-item nulls; `GET /api/node` reads its folder in the async
    handler (tiny, not `spawn_blocking`); `installer/llama.cpp.wxs` ~121 describes the payload without
    `share\versions`; `fe/briefs/api-contract.md` still calls `/api/node` and `boot` "being built".
    Creating and removing instances is done in the manager and the bff (slice 16, 2026-11-21; the web UI
    is slice 17). Left from its review: on a CRLF `instances.ini` the appended section is LF, so the file
    has mixed line endings (the manager reads both); `"default"` as the protected id is written in both
    `service.rs` and `crates/bff/src/instances.rs`; `crates/bff/README.md` says the association route's
    `client-address-in-use` carries the port, the code passes the other instance's id.
    **React's own features for the UX** (user, 2026-11-21: "abbiamo React come framework, utilizziamo le
    feature che offre per migliorare la UX"): the web UI (React 19.3.1) uses useOptimistic for instant
    feedback on writes with rollback on refusal, useTransition/useActionState for pending saves,
    useDeferredValue for filtering as you type, createPortal for overlays; the designer records the
    patterns in `fe/DESIGN.md` and every brief from C1 on applies them. Then the whole GUI (user,
    2026-11-21: "in generale usa tutte le feature per tutta la gui, al momento è molto basica"): a UX
    pass over every page (sign-in, Overview, instance, model profiles, About, the shell), from the
    designer's audit of what is basic today and a phased brief, built in slices after C1; after U17b it
    waits for C2, About and node info, 2g/2j, slice 17, model profile tags, then R4 and R5 (user, 2026-11-21,
    2026-11-22)
    (fe brief 17: U17a skeletons, U17c sorting and U17b tile chips built; left U17h, U17e, U17d, U17f, U17g in that order).
    **One models folder for every engine** (user, 2026-11-21: "usa una folder unica per i modelli tra i
    differenti motori"; asked whether a server profile may still name another: "unica e basta"): one
    models folder per node, shared by llama.cpp and later Strata, owned by the framework rather than an
    engine's tree; `[Server] ModelsDir` leaves the server profiles. No migration of existing folders
    (pre-release). The architect designs the path per OS, the owner and permissions, what the engines'
    packages and the model file picker (M3 lists that one folder) change; slices before M3. Decided
    (user, 2026-11-21, ARCHITECTURE.md "One models folder for every engine"): Linux
    `/var/lib/private-ai/models` (the service user's home opened just enough to reach it), Windows
    `%ProgramData%\private-ai\models`, macOS keeps `~/.llama.cpp`; the instances and `pai` members write
    it. Slices F1 `pai-core` (the path, `ModelsDir` dropped, `LLAMA_CACHE`), F2 the clients, F3 packaging.
    **Choosing the model file on the web model profile page** (user, 2026-11-21: "a quando la selezione
    degli asset nel repo dei modelli? (ora testo libero)"): `hf-repo` offers the GGUF files (quants) of
    the Hugging Face repo, read by the node as the GUI's `crates/app/src/hf.rs` does; `model` offers the
    GGUF files already in the models folder. After slice 17 and the rest of the UX pass; the architect's
    design first (the node calls huggingface.co: outbound network, the HF token in the machine-wide
    state still to place), then the designer's brief, then tester and coder rounds. Decided (user,
    2026-11-21, ARCHITECTURE.md "Choosing the model file on the web"): public repos only (the token comes
    with item 13's machine-wide state), the endpoint fixed to huggingface.co, every models folder of the
    node's server profiles grouped by folder (superseded the same day: one models folder per node). Slices M1 listing into `pai-core`, M2 `GET /api/hf/files`,
    M3 the manager's `ModelFilesAs` and `GET /api/node/model-files`, M4 the pickers (designer first).
    Moved ahead (user, 2026-11-23, asked again): M1 then M2, M3 in parallel listing today's
    `paths::instance_models_dir()`, then M4 with MP3's create form sharing the picker; F1 to F3 later as
    one unit in one deploy. M1, M2, M3 and M3b built 2026-11-23 and 24.
    Then M3b (`ARCHITECTURE.md` ~3038): the bff's model profile save takes `model`, `hf-repo` and
    `hf-file`, only from the lists (user, 2026-11-23: no free text; `hf-file` checked against the repo's
    listing at save). The web side is brief 22 (`fe/briefs/22-model-sources.md`, MP3 and M4 together):
    22D delete built (2026-11-23), 22P the shared picker (after M2, M3), 22C the New form with Copy as a third
    "Start from" (after 22P), 22E the picker on the Assets card (after M3b). Decided (user, 2026-11-23):
    Copy inside New; Delete shown on any switched-off profile, the 409 naming the instances; only
    listed items.
    **Collapsible cards** (user, 2026-11-23: "the assets section opens collapsed ... it could be applied
    to every section, with collapse-all and expand-all controls"): brief 23
    (`fe/briefs/23-collapsible-cards.md`): the cards of the two long forms collapse, each card's state
    remembered per page kind, Expand all and Collapse all before the first card. Decided (user,
    2026-11-23): cards start open (Assets closed), only the model profile and server profile pages.
    Slices K1 (pattern, model profile page, controls; after 2j's last part) and K2 (server profile
    page).
    Then (user, 2026-11-21): the Hugging Face token is handled through authentication, for gated and
    private repos and downloads: the architect designs it with item 13's machine-wide state (options:
    "Sign in with Hugging Face" OAuth from the web UI, or a token an administrator enters; stored once by
    the manager, used by the bff's listing and the instances' downloads, never shown back), a slice M5.
    **Tags as chips** (user, 2026-11-21): the instance tags field becomes a row of chips: a round "+"
    that, clicked, opens an oval input for one label; confirmed, it becomes a chip and a new "+"
    follows. While typing, the labels already used (on any instance or server profile) are suggested.
    No edit of a chip: remove it and add it again. How a chip is removed is the designer's (the user
    had no preference). Web UI only (`PUT /api/instances/{id}/tags` unchanged); designer's brief first.
    Long tags (user, 2026-11-21): shortened with an ellipsis on one line; a smaller font if needed;
    wrapping only when nothing else fits. Hovering a shortened tag (and focusing it from the keyboard)
    opens a temporary larger pill with the whole text. C1 (chips, "+", x, the long-tag rule, the hover
    pill) and C2 (the suggestions) are built, brief 15 done
    (user, 2026-11-21). Test flakes seen at C1, to fix test-side when
    a slice touches them: `ModelProfilePage.test.tsx` 2d.63 (title asserted before the effect writes
    it), `NodeNav.test.tsx` ~211 (a read count), `SignInOnly.test.tsx` S1a.10.
    **Create and delete model profiles from the web** (user, 2026-11-22): a model profile can be deleted
    only when it is switched off (in `models.ini.disabled`) and assigned to no instance; otherwise the
    manager refuses with a keyed refusal naming the reason (the instances it is assigned to). The web
    also creates a model profile (an id and its model file), born switched off; creating can clone an
    existing profile, copying its settings into the new one (user, 2026-11-22). Deleting drops its tags
    (`model-tags.ini`). Built with slice 17; the architect's design first (new manager methods, since the
    manager has none for create or delete today; the GUI keeps writing the pool files whole), then the
    designer's brief. Decided (user, 2026-11-22, ARCHITECTURE.md "Create, clone and delete model profiles
    from the web"): `CreateModelProfile(As)` (from a model file, an `hf-repo`, or a clone of another
    profile's section and tags, never its assignments) and `DeleteModelProfile(As)`; the manager's
    "needs `model`" rule loosened to `model` or a download key. Slices MP1 to MP3; MP1 (the manager) and MP2 (the bff routes) built 2026-11-23; MP3 with slice 17.
    **Instance labels** (user, 2026-11-23: "let me rename the instance, if it still makes sense, since for
    me the instance is engine + port"; chosen: a label beside a fixed id, built inside slice 17): an
    instance shows its label when set, otherwise its engine and port ("llama.cpp :9931"); the id never
    changes and is shown small. Design `ARCHITECTURE.md` "Instance labels" (a `Label` key in
    `instances.ini`, 1 to 64 characters, unique on the node ignoring case, empty clears it); slices IL1
    (manager and `pai-core`) and IL2 (bff), then the web UI as brief 16's 17f. IL1, IL2 and 17f built 2026-11-23 (left: WebKit at 1280 px puts Start at boot's checkbox on its own row, not caused by 17f).
    W1 built 2026-11-23: the bff's nine single-call writes answer 504 `client-timeout` once the call
    reached the manager (`timeout_answer`, shared with the saves). W2 built 2026-11-23: the run verbs, `boot_write` and `tags_write` move to
    `write_timeout` (`boot_read` keeps the read limit), and a manager `Unavailable` after the call was
    handed over is 504 as in the saves. Left: the web UI shows the generic `request-failed` for a 504 on the verbs, boot,
    tags and the creates and deletes (only the server save reads `client-timeout`), worded by the
    designer with slice 17.
    **About and node info** (user, 2026-11-22): the About page keeps only the framework's version and
    the operating system, with its version, of the machine the service runs on (the node's bff, or the
    director's); everything else it shows today (host name, engine and GPU runtime versions) moves to the
    node's info, which also gains the version of every engine (llama.cpp, Strata, ...), the operating
    system and its version, and the GPU driver versions. Right after C2, before 2g/2j: the architect's
    design for the new `GET /api/node` fields (where each version is read, and from which account),
    then the designer's brief for both pages. Decided (user, 2026-11-22, ARCHITECTURE.md "About and
    node info"): the manager reads the node facts (new `NodeInfo`/`NodeInfoAs`, which the director
    will call too), the Vulkan driver per GPU included (a `pai-manager vulkan-probe` child; its first
    run on this machine needs the user's go-ahead), About reads `/api/version` with `os`. Slices AN1 to
    AN5 (brief 18 for AN5, split AN5a and AN5b). Built: AN1, AN2 (the Vulkan child and join; its first real run on
    this machine still waits for the user's go-ahead). Built: AN5, the About and Node info pages (2026-11-22; they show real node facts once AN3 and AN4 land). First real Vulkan run (user's go-ahead, 2026-11-22): 3 rows, PCI ids as lspci, radv, Mesa 26.0.8, 2.2 s. Built: AN3, the manager's `NodeInfo`/`NodeInfoAs` (2026-11-22). Built: AN4, the bff's `/api/version` (name, os) and `/api/node` relay (2026-11-22).
    **Tags on model profiles** (user, 2026-11-22): a model profile carries tags too, as chips with the
    same suggestions (C1, C2), for three uses: labels and a filter on the Models page, routing (the router
    and the director pick a model by tag, step 14 and item 14), and shown to clients in the model list
    the API offers. After slice 17 (it redoes the profile pages), before R4 and R5; the architect's
    design first (where the key lives in `models.ini` and whether llama-server tolerates it, the
    manager's checks, how the router and the client list read it), then the designer's brief.
    Decided (user, 2026-11-22, ARCHITECTURE.md "Tags on model profiles"): a manager-owned side record
    `model-tags.ini`, no restart for a tags-only save, one shared tag vocabulary, routing by
    `"model": "tag:<t>"`; no tags on macOS yet. Slices MT1 to MT3.
    **Resources belong to the server profile, placement follows** (user, 2026-11-21): a bare model profile
    cannot know its GPU/CPU distribution until it runs on an instance. The server profile should bind the
    resources an instance may use (which GPUs, RAM, CPU threads, the backend SDK), so a model profile
    served under that server profile gets those resources; and the node's hardware may need a profile of
    its own (GPUs with VRAM, RAM, CPU, the installed SDKs and runtimes). First the architect's design: how
    the server profile's resource set relates to the model profile's `device`, `tensor-split`,
    `n-gpu-layers`, `n-cpu-moe` (defaults, overrides, or derived at render), what the manager renders into
    `models@<id>.ini`, where the memory estimate runs, the node inventory (G2's probe, the versions files,
    pai-hwmon), and what changes in the GD card and brief 16. Before slice 17's sub-slices that touch the
    server profile form (17a).
    Decided (user, 2026-11-21, ARCHITECTURE.md "Resources belong to the server profile"): a server profile
    key `[Server] Devices` (the probe's GPU names in order, or `none`; absent = every GPU) passed to the
    router as `--device`; model profiles keep `split-mode`, the `tensor-split` weights (in the server
    profile's GPU order) and optional fixed `n-gpu-layers`/`n-cpu-moe`, llama.cpp's `--fit` places the
    rest; GPUs are named as the probe prints them; the memory check counts only the instance's server
    profile's GPUs (this replaces the 2026-11-17 decision that it counts every GPU of the node); host RAM
    and CPU threads are shown in the node inventory with no budget. R1 (inventory, a1a3458), R2
    (the `Devices` key, 3aaa4dc) and R3 (the per-pair memory check) are built; left: R4 `device` off the web model profile
    page, R5 the web UI (a GPUs card on the server profile page, designer's brief first, after the whole of slice 17 (user, 2026-11-21); it also
    gives `pai server show` a Devices row). R4 is built together with R5, not before: taking `device` off
    the web keys first would make the model profile page's GPU card send a key the bff refuses.
    With R4/R5 also: fe briefs 11 and 14 and the doc comments at `service.rs` ~8660 and ~8878 and
    `devices.rs`:5 still describe the retired every-GPU memory rule.
    **Also (found by the designer at C1, 2026-11-21):** the Server form's Hostname checkboxes are 20x20,
    an axe target-size (WCAG 2.5.8) finding at 1280 px; fix in U17e or 17a, whichever touches the form first.
    **GPU distribution on the model profile page** (user, 2026-11-20: "Non vedo gli slider per la
    spartizione dei layer"): the web UI edits `device`, `split-mode` and `tensor-split` as the GUI's GPU
    table does (fe slice 2f of brief 02, never scheduled until now). Built beside slice 14: first the
    architect's read of the server's GPU list through the bff (the manager probes it for the memory
    check), then the designer's brief, then one tester and coder round on both sides; the three keys move
    to `checks::WEB_MODEL_PROFILE_KEYS`. Tensor placement (`override-tensor`, 2j) stays later.
    Designed: `ARCHITECTURE.md` "GPU distribution on the model profile page" (G1, G2) and fe brief 14
    (GD1, GD2). User decisions (2026-11-20): a number field per GPU with its share shown, Move up / Move
    down for the order; per-GPU layer sliders like the GUI's come with 2g (the model's layer count);
    Draft device and Vision GPU list the probed GPUs in a small later follow-up.
    Order (user, 2026-11-21): 2g (per-GPU layer sliders, the model's layer count read from the GGUF
    header), the Draft device and Vision GPU lists, and 2j (tensor placement, `override-tensor`) come
    right after C2 and before slice 17; the architect's read of the layer count first, then the
    designer's brief.
    Decided (user, 2026-11-21, ARCHITECTURE.md "Layers, MoE and tensor placement on the web model profile
    page"): the sliders read the saved `model` (manager `ModelInfoAs`, `GET
    /api/profiles/model/{id}/model-info`); an `hf-repo` not yet downloaded gives no layer facts for now;
    the manager refuses `override-tensor`, `device-draft` or `mmproj-device` naming a GPU outside the
    instance's server profile `Devices`; `moe-cache-mib` stays off the web until its floor rule.
    Built (2026-11-22, `git log` holds each): L1 to L4, J1 (the tensor list in `ModelInfo`), 2g (brief
    19, LS1, LS2a, LS2b) and N1 (brief 20, the side menu as Nodes and Profiles). Left from them: the
    warning before Save for a Draft device or Vision GPU outside `Devices` (with R4/R5; the save refusal
    already names the GPU), and Server profiles entering the menu with its page (slice 17a).
    In progress: 2j, brief 21 (`fe/briefs/21-tensor-placement.md`, 2026-11-22: a Tensor placement card
    of rule rows, tensor kind, layers, device, the raw rules editable as text; TP1 T.1 to T.9 built
    2026-11-23; TP2 T.10 to T.13 built 2026-11-23; TP1b T.18 to T.23 (the rule row aligned, an x to remove and a + to add: the user's feedback on the deployed page, 2026-11-23) built 2026-11-23; left TP3 T.14 to T.17, and the designer's D4 (WebKit draws every select 42 px: `height: var(--control-h)` in the global `select` rule); the brief's two questions, a
    tensor picker and Move up/down for rules, taken as recommended: not now). Slice 17 started 2026-11-22 at the user's request, beside 2j: the designer refreshes brief 16 against main first, brief 16 refreshed (60eb2bd); 17a (the Server profiles list and page, the form moved off the instance
    page) built 2026-11-23; 17b (create and delete a server profile) built 2026-11-23; next 17f (instance labels and the
    504 texts, after IL2) and TP2, one `fe/` user at a time; 17c to 17e wait for 17f.
    TP1c and 17b2 built 2026-11-24. After 17f (user, 2026-11-23: "the + touches the row above; New server profile should be a + at the end
    of the list"): TP1c (brief 21, 21 T.24, room above the +) and 17b2 (brief 16, 17b2.1 to 17b2.5, a list's
    add is a + after the list, its accessible name unchanged; the same rule amended into 17c.1 and 22 C.1).
    Then 22D4 (brief 22, 22 D4.1 to D4.12; user, 2026-11-24, on the deployed 22D2: "the minus goes in the
    model profiles list beside each model, not in the model page; there Delete beside Save and Discard"):
    a 40 px minus on each row of the Models and server profile lists, disabled with a hint while the
    profile is on or used; on both profile pages Delete as a text button at the start of the top Save
    bar, disabled with the same hint when it cannot act; 22D2's status row and minus and all of 22D3
    superseded. The user confirmed the layout from a preview before the build.
    Then 17f2 (brief 16, 17f2.1 to 17f2.10: the display name set from a pencil after the page heading, the
    hint saying the ID, service, files and address stay, Rename removed; user, 2026-11-24: "Rename is not
    clear, we need another UX", the pencil chosen over a facts row), after 22D2.
    22D2 built 2026-11-24. Then 22D2 (brief 22, 22 D2.1 to D2.10: Delete as a − beside a status row on both profile pages, the Save
    bar sticky at the top; user, 2026-11-23: "put a minus right of the status and the edit commands at the
    top", the bar at the top confirmed) and the UX audit, brief 24 (A1 to A10, 26 criteria, ranked: A1 run
    an instance from its tile, A2 the switch on the model profile page, A3 "Served by" on the Models list),
    after the user's "the designer should know these things" and "few clicks to reach an action"; the
    principles are DESIGN.md section 1, 6 to 14.
    Built: G1 and G2 (66b2bb3) and the web UI's GPU card, GD1 and GD2 (2026-11-21). Left, the
    designer's preferences for a later pass: at 1280 px Move down drops under Move up, and the "Free
    memory as of" line and Auto/Even sit in the form's label column, away from the weights.
    **Presets are a shared pool per node, assigned to instances** (user, 2026-11-17, a requirement
    change that replaces "a model belongs to one instance" of the same day): a preset is assigned to
    one or more instances; removing an instance removes only its assignments; editing a preset applies
    to every instance it is assigned to; the on/off is global with an explicit per-assignment override;
    before a preset is applied to an instance the manager checks the instance's `ModelsMax`, the
    full-GPU memory estimate against all the node's GPUs (strict: refused even with `fit` or a CPU
    placement), the model file on the node and the engine's keys; other nodes later (step 14). The
    navigation: node > instances, each with Server and Presets tabs, then Models (the shared pool)
    under the node; `#/models` opens the pool as today (user, 2026-11-17, through the web UI session). Designed: `ARCHITECTURE.md` "Nodes and instances" (the pool is today's
    `presets.ini`, assignments in `assignments.ini`, the manager renders `presets@<id>.ini` per
    instance), thirteen slices in its order, after the preset cards. Slices 1 (the render,
    `pai-core` `assign.rs`), 2 (the manager renders and launches) and 3 (`assignments.ini` shipped,
    `After=private-ai-manager.service` on the instance unit) done 2026-11-17. Left: the manager never
    re-renders when `assignments.ini` appears on disk (an engine-only upgrade, or an rpm transaction whose
    framework %posttrans runs first, leaves `default` with `[*]` only: the deb order is safe); on Windows
    the instance services the manager creates have no dependency on `private-ai-manager` (set it at
    creation); the docs (`installer/AGENTS.md` ~64 and ~92, `docs/build-packaging.md`) name two defaults; the e2e test of a failed render pins its log line only by parts;
    seen at the install of ad525d3 (2026-11-17): apt configured the engine first, its postinst started
    `llama.cpp@default` while the framework's upgrade had the manager stopped, so no manager job was queued,
    `After=` did nothing, and the instance found no `presets@default.ini` yet (`NoPresets`); a restart once
    the manager had rendered fixed it. It bites only while a render is missing (the first upgrade), so it
    is noted, not planned (no migrations before the release); a server write re-renders every
    instance (only the written one would do); missing tests: a render failing after the pool write, the
    second instance over its max, the D-Bus and pipe `BadFileName` for `presets@<id>.ini`. Clients still
    cannot write a `[*]` section (decided 2026-11-17); a hand-edited `[*]` is rendered.
13c. **Strata as the second engine** (user, 2026-11-17): the manager runs and configures a Strata
    instance beside the llama.cpp ones, after item 13's manager slices (4 to 12), since it needs
    `CreateInstance`. Decided: the manager writes and launches Strata's own run config and profile
    (`strata serve` / `strata runtime start --profile`), not `strata.conf` + `strata-start`; the first
    milestone runs and configures (start, stop, restart; server and model keys read and saved through
    the manager and the web UI); the Strata serving :8580 today stays untouched until the user decides
    the switch (a separate go-ahead). One Strata server holds one model, set at start (no router, no
    models preset). First the architect's design (engine abstraction in the manager, the run config and
    profile as the instance's files, identities and the API key, packaging or a found install). Input: a
    read-only inventory of Strata 0.1.60.1's server and model parameters, kept locally (not committed:
    third-party facts, mappings partly guessed) at `build/linux/strata-params.md`; open there: the
    engine's own flag list (its `--help` was not run), the JSON-only run-config keys, health and props
    endpoints. Also (user, 2026-11-17): integrate Strata's build scripts from
    `/mnt/data/git/strata-framework` (`00`..`04` `.sh`/`.ps1`: the newest Strata release tag, llama.cpp
    at the commit Strata pins, the engine per backend `hip|cuda`, one at a time, and the Terminal; the
    `strata-framework-<backend>` `.deb`/`.rpm`; `/opt/strata-<backend>`) into xtask, and split every
    phase by three product arguments: `llama.cpp`, `strata` and `pai` (today's `framework`), so each
    builds, packages and installs on its own. The design decides the backend argument, the package
    names, Strata's licence in the packages and what happens to the strata-framework repo.
    Also (user, 2026-11-24): keep systemd-oomd from killing an engine instance. This machine runs
    systemd-oomd (systemd 259, Ubuntu): `user@.service` ships `ManagedOOMMemoryPressure=kill` at 50 %
    (`/usr/lib/systemd/system/user@.service.d/10-oomd-user-service-defaults.conf`), which the user
    turned off here with `ManagedOOMMemoryPressure=auto` in `/etc/systemd/system/user@.service.d/
    90-oomd-pressure-off.conf`, so the Strata started from the user's session is not killed under
    pressure. The instance units (`llama.cpp@.service`, later `strata@.service`) run in `system.slice`,
    where nothing sets `kill` today, so they already read `auto` (`systemctl show`). Read in
    `systemd.resource-control(5)`: `auto` on a unit does not shield it when an ancestor slice has
    `kill`; the shield is `ManagedOOMPreference=avoid` or `omit` (root-owned cgroup, xattr support).
    The architect decides which keys the instance units ship (`ManagedOOMMemoryPressure=auto`
    explicitly and `ManagedOOMPreference=avoid` or `omit`), and whether `pai-manager` and `pai-bff`
    take them too.
13d. **Hardware monitoring, metrics and log consultation** (user, 2026-11-21; built after 13c, user's order). In the web UI: a live
    view of the node's hardware streamed as the native GUI's HW Monitor does (`crates/app/src/hwmon.rs`:
    GPU busy, VRAM, GTT, memory activity, PCIe, CPU, I/O wait, RAM, swap, disk, one sample a second),
    and a history chart of the same metrics. Behind it a monitoring and logging module that collects
    the metrics and acts as a log router when an administrator consults logs: the logs stay on each
    host for now (no central log collector yet; the journal on Linux, the instance log files on
    Windows), read on demand. External tools are welcome where they fit (the user named OpenTelemetry,
    which also carries logs). First the architect's design: options (OpenTelemetry collector and SDK,
    Prometheus-style exporters and a time-series store, or the manager's own sampler with a small
    history store), what runs on each node, retention, the trust boundary (who reads logs, the group
    `pai`), how it reaches the director (step 14) later, packaging on Linux and Windows; then a
    designer brief for the live view and the chart.
    Decided (user, 2026-11-21): the manager's own sampler in a new crate `pai-hwmon` (shared with
    pai-config's HW window), an in-memory 1 s ring and RRD-style history tiers in SQLite (5s:1h, 20s:4h,
    2m:24h, 15m:7d, 1h:30d, configurable in `manager.ini`), an SSE stream through the bff; every `pai`
    member reads instance logs; the disk card follows the default models folder only; an OTLP export
    later. Slices: 1 pai-hwmon with measured sample cost, 2 live stream, 3 history, 4 log consultation.
    A future central log collector (not now): OTLP, Fluent Bit on the nodes, VictoriaLogs behind the
    director (ARCHITECTURE.md 13d).
14. **The router**: R11's gateway, one OpenAI-compatible endpoint in the bff with authentication that
    sends each request to the instance serving the model asked for, across instances and later engines
    (user, 2026-11-17: after nodes and instances). It logs every request (user, 2026-11-18): the
    client's address, from the connection or from `X-Forwarded-For` / `Forwarded` only when configured
    (which proxies are trusted), the User-Agent, and what else helps: the authenticated caller, the
    model asked for and the instance that served it, status, latency, prompt and completion tokens; never
    the prompt or the reply text, nor a key. No time field where the record goes to the journal, which stamps it
    (user, 2026-11-18); a log file (Windows has no journal) carries the time. The design decides the
    format, where the log goes and its rotation.

Later slices (not yet written): the benchmark portal.

### 15. A minimal install image with the manager (last)

Covers: R14. Asked by the user on 2026-11-22, to come at the very end of the order: a minimal
installation image, like a hypervisor appliance, with the manager already installed; the manager then
installs the other packages the node needs (llama.cpp, Strata). Architect's design first. Open, for
then: the base OS and the image formats (an installer ISO, a VM disk such as qcow2/VHDX, a cloud
image); where the manager takes the engine packages from and how it verifies them; installing
packages is a new privileged verb, so it sits with the helper of step 14's "manager without root"
(P1 to P6); the GPU drivers and runtimes the engines need; how the first administrator is created
(the group `pai`) and how the node is reached before the web UI is installed.


## Waiting for the user

- **ROCm host memory as GTT, confirmed under load on Strata 2026-11-24** (found by the strata-framework agent, 2026-11-24; the user
  asked it to be shared and carried into the product, marked under test until confirmed under load).
  On a discrete AMD GPU, ROCm's "paged" host memory (hipHostMalloc, the system memory pool) is an
  anonymous shared mapping registered as a KFD userptr (`HSA_USERPTR_FOR_PAGED_MEM`, on by default) or
  as SVM (`HSA_SVM_FOR_PAGED_MEM`, on by default): both sit on the kernel LRU, so compaction and
  reclaim move or swap it, and each move evicts all the process's GPU queues (bpftrace stack: kcompactd
  or kswapd -> mmu_notifier -> amdgpu_hmm_invalidate_hsa -> amdgpu_amdkfd_evict_userptr ->
  kfd_process_evict_queues). Read in libhsakmt `fmm.c` `fmm_allocate_host_gpu` (ROCm/rocm-systems,
  `projects/rocr-runtime/libhsakmt/src/fmm.c`, commit b0c0561). With both set to `0` it takes the
  `KFD_IOC_ALLOC_MEM_FLAGS_GTT` path, driver-owned memory that reclaim and compaction leave alone.
  Measured on Strata (2x R9700 gfx1201, Linux 7.0.0-44, TheRock 10.2.0): queue suspensions of 1 s to
  48 s and engine restarts before; after setting both to 0 (19:50), the engine's `/dev/zero` rw-s
  mappings went from ~2.6 GB to 0 and GTT rose by ~2.7 GB, 0 evictions since the restart. Not yet
  confirmed under a Playwright run or a build, nor its decode speed. Cost: that memory is no longer
  swappable and GTT is capped near half the RAM, so a large pinned host allocation can fail where it
  used to succeed. For the product: llama-server's ROCm instances could carry both variables in their
  environment (HIP backend only); first check a running instance's `/proc/<pid>/smaps` for large
  `/dev/zero (deleted)` rw-s mappings and `/sys/class/kfd/kfd/proc/<pid>/stats_*/evicted_ms`. Waiting
  for the Strata result under load. Confirmed under load (strata-framework agent, 2026-11-24 20:41-21:33,
  through two of our Playwright runs and a cargo build, with STRATA_RESIDENT_PIN=1 beside both
  variables): 0 KFD queue evictions (evicted_ms 0 on every GPU, no kfd_process_evict_queues in
  bpftrace), against up to 13 episodes of up to 48 s in 50 min under the same load earlier; host
  memory as GTT (17.3 GB on GPU 1, RssShmem 0); speed not yet compared on a quiet machine. Both must be
  0: USERPTR=0 alone falls back to SVM, which evicts the same way. Seen live on our llama-server too
  (strata-framework agent, 2026-11-24 22:00, `default` started without the variables): its paged host
  memory is SVM (bpftrace: svm_range_cpu_invalidate_pagetables -> kfd_process_evict_queues), and its
  queues were suspended ~6 s per KFD node within 3 minutes of start, mostly while Strata allocated its
  GTT. The user confirmed the finding (2026-11-24), for every instance
  whatever its backend: the architect places
  it (the instance unit's environment or `launch.rs`), then a test-first slice (GT1, in progress).

- Step 1, taken by the coordinator on 2026-11-14, approved by the user on 2026-11-14:
  - Windows instance services are removed on every uninstall, repair and upgrade by
    `pai-manager uninstall-services` (a deferred custom action) and recreated by the new manager from
    the instances on disk; an instance that was running without boot start comes back stopped.
  - `%ProgramData%\private-ai\logs` and `models` are writable by `NT SERVICE\ALL SERVICES` (every
    service on the machine), simpler than per-instance entries set by the manager.
  - The MSI creates `%ProgramData%\private-ai\models`; making it pai-config's default ModelsDir on
    Windows is a follow-up (the virtual accounts cannot read the user's home).
  - The manager service restarts after a failure, as the Linux unit does.
- Step 3, taken by the coordinator on 2026-11-14, approved by the user on 2026-11-14: `pai` gets its own package,
  `private-ai-framework-cli`, on Linux and macOS, with no dependency on the configurator or the
  graphics libraries (a headless server installs only the command line); on Windows `pai.exe` is in
  the one MSI beside `pai-config.exe`, on the PATH. `pai-config` given a command prints one line
  naming `pai <args>` and exits 2.
  `-cli` requires `-llama` at its own version (pai runs llama-bench and llama-server and talks to
  the manager) and ships the benchmark method and `cli.toml`, which both programs read, so `-config`
  requires `-cli`: a desktop install gets `pai` too. The MSI's process stopper also stops a `pai`
  running from `bin\` (an interrupted sweep puts its key back at the next start).
- Step 10, taken by the coordinator on 2026-11-14, approved by the user on 2026-11-14: the bff listens on `127.0.0.1:8590` by
  default (on the LAN only as the administrator's choice, `[Bff] Listen`, since 2026-11-17); it has its own package `private-ai-framework-bff`
  (requires `-core` since step 11), runs as the static system user `private-ai-bff` on Linux (first `DynamicUser=yes`, replaced on 2026-11-14 after the real install: dbus-daemon drops a dynamic user at Hello) and as the virtual account
  `NT SERVICE\private-ai-bff` on Windows, enabled and started at install; macOS has none. Who is acting:
  decided (step 10). `cli.toml` moved to `-llama`, and with step 11 to `-core`, which every package requires (the
  manager and the bff print from it).
- Step 1: the real run of the Linux packages, after the Windows slices (user, 2026-11-14), together
  with the Windows build: `xtask build`, `package`, `install`, then the step's checks.
- Step 2: a Windows build of the renamed MSI and the installer test plan of `installer/AGENTS.md`,
  which also settles two points no test here can: `ProgramFiles64Folder` is set before `AppSearch`,
  and the `Type="directory"` search returns the trailing backslash the old-default comparison needs.
- Step 2: pai-config's Windows code (start at login with the old-entry migration and Task Manager's
  off state, single instance, the migration's Windows branch) is compiled by nobody here (W15): build
  it on Windows and run `cargo test -p pai-config startup -- --ignored` (registry round trips that
  put the starting state back).
- Steps 4 to 9: every benchmark run and every llama-server start on the GPUs needs the user's
  go-ahead (they hold the GPUs and may collide with other jobs).
- Steps 5 and 9 partly need the RTX 4070 SUPER back in the test machine.

## Later, not planned yet

- **A Docker image with every package and only the manager running** (user, 2026-11-22, kept pending):
  a container has no systemd or SCM, while the manager runs each instance as a systemd unit, so it needs
  an instance runner of its own (the manager spawning llama-server, as on macOS) or an init in the
  container; open as well: GPU passthrough (`/dev/kfd`, `/dev/dri`; the NVIDIA Container Toolkit), the
  `pai` administrators and polkit inside the container, models and config as volumes. Decide after
  step 15's install image.

- **TypeScript 7 for the web UI:** `fe/` stays on TypeScript 6.0.x (user, 2026-11-14) because
  typescript-eslint supports `>=4.8.4 <6.1.0` and TS 7.0 has no API for external tools (expected
  with 7.1: typescript-eslint issue #12518, Microsoft's TS 7.0 announcement). Move to 7 only when
  every tool of the `fe/` stack supports it (typescript-eslint first), checked in each tool's own
  docs.
- **CI leftovers from step 12** (both installer workflows green: Windows run 43654217880, macOS
  43645311957): `find_openssl` knows only `OpenSSL-Win64`, not the runner's `Program Files\OpenSSL`
  (the workflow puts it first on PATH); on macOS xtask builds with four dead-code warnings
  (`components.rs`: `OLD_BASE_NAME`, `LLAMA_CPP_REVISION`, `llama_cpp_version`, `llama_cpp_min`, used
  only by the Linux and Windows packaging); both workflows' header comments still cite "PLAN.md step 12".
- **macOS build stack, unverified:** `crates/app/build.rs` `emit_build_stack` reads the CMake
  cache under `build/windows` or `build/linux` only, so on a Mac the recorded SDK versions would come
  out empty (found by reading; nothing has run on a Mac, R3).
- **Adaptive MTP depth** (block 01, `draft-mtp-adaptive`, 84 KB, host side, built on
  upstream PR #27210): a decode lever for Coder390, where the guide records acceptance
  moving between 0.49 and 0.65 across tasks and decode with it by 20-30 %. Their figure:
  +13.6 % overall on the 27B Q8_0 with `ngram-mod` combined. Not in v0.7.0; portable in
  principle (no kernel code). Parked on 2026-10-21 as too invasive for its expected gain
  (84 KB of speculative-decoding plumbing to re-cut at every tag bump); revisit if upstream
  merges #27210 or something like it, when the measurement becomes a sweep at 43.7k and
  115k, both temperatures, rather than a port.

- **RCCL against the internal AllReduce** (block 12): RCCL cannot run while the x4 card's slot has
  no AtomicOps. With both cards on CPU slots, `GGML_CUDA_ALLREDUCE` compares `internal` and `nccl`
  with no rebuild; upstream's init tries NCCL first, so keep `internal` set until that is measured.
  Our presets keep every expert in VRAM, the shape where RCCL loses; it matters only if a preset
  moves experts to host memory.
- **Watch items**: the upstream PRs listed in [candidates.md](patches/llama.cpp/candidates.md),
  "Watch items", need action only if their symptom appears.
