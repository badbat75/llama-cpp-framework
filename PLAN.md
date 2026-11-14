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
| R3 | The same on macOS, Apple Silicon only (Metal + Vulkan through MoltenVK, split `.pkg`) | Written, never run on a Mac |
| R4 | A GUI + CLI configurator (`llama-cpp-config`, becoming `private-ai-config`: step 2) for `server.ini` and the per-model `presets.ini` | Met: release 2.4.0 |
| R5 | Every measurement in `benchmarks/bench.db`, under a versioned spec, public: no credentials, no local paths | Met |
| R6 | Per-GPU and per-model guides where every value traces to a campaign, published as a site | Met, but their figures predate v0.9.0: step 5 |
| R7 | The fastest presets for the test machine's GPUs: port an out-of-tree optimization when its gain pays for its upkeep | Open: steps 4 to 9 |
| R8 | llama-server runs as an OS service per instance (`llama.cpp@<id>` on Linux, config `server@<id>.ini`, a Windows service) managed by the runtime service `private-ai-manager`; the OS starts at boot the instances enabled for it (systemd, the SCM start type), set through the manager, which is what the GUI and CLI talk to (D-Bus plus polkit on Linux, a named pipe with an ACL on Windows); units started by hand stay under its management; the manager owns every check on the config it writes and the GUI is a thin client; later engines (Strata) are to fit the same manager | Open: step 1 |
| R9 | The project is `private-ai-framework`, its executables `pai-config` and `pai-manager` (decided 2026-11-13), with upgrades from the old packages and the user's data carried over | Open: step 2 |
| R10 | The GUI and the CLI are two executables over one shared library: GUI `pai-config`, CLI `pai`, the shared logic in a new crate without Slint (decided with the user on 2026-11-13) | Open: step 3 |
| R11 | A backend-for-frontend (`bff`) running as a local service beside `private-ai-manager`, in the same packages: the web UI of the configurator (a `fe` it serves), a gateway in front of the llama-server instances (one OpenAI-compatible endpoint, authentication, routing across instances and later engines), and the benchmark portal over `bench.db` (decided with the user on 2026-11-14) | Open: step 10 |
| R12 | The framework is built and versioned apart from its engines: each engine (llama.cpp today, Strata later) is named explicitly and has its own package at the engine's version, the framework's packages carry only the framework's version and a minimum-version dependency on the engine (decided with the user on 2026-11-14) | Open: step 11 |

### How we work

| ID | Requirement | State |
|---|---|---|
| W1 | Test driven, always: tests first, then the code | Applies to every step |
| W2 | Keep it simple | Applies to every step |
| W3 | One agent each for coordination, code, tests and documentation | Met: `.opencode/agents/`, plus the read-only `spec-reviewer` |
| W4 | AI scaffolding as standard as possible, readable by other harnesses | Met: `AGENTS.md`, shared role files, `.agents/skills/` |
| W5 | A maintained plan: closed points removed, evidence in the commit log, so an agent starting from zero knows what to do | Met: this file and `git log` |
| W6 | The coordinator or the user can change the plan when needed | Met: rule in `AGENTS.md` |
| W7 | Each agent has its own skills, which it can update itself or the coordinator can | Met: `.agents/skills/` |
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
   interactive user and `configure` a member of the local Administrators group, elevated or not
   (user, 2026-11-13: no prompt can cross the pipe); each instance's service and virtual account are named
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
  when the patches or the build options change on the same release), and the epoch `1:` because Ubuntu
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
- **Small xtask leftovers** (found 2026-11-14 by the docs agent, with slice 3): the headers of
  `scripts/02-build.{sh,ps1}` and `03-package.{sh,ps1}` still say `[llama | config]` and "pai-config";
  `xtask install` on Linux warns of a stale package only against `llama-server`'s date, so after
  `build framework` an old framework package gets no warning; `common/configurator.rs` says the
  framework build needs the clone "for the icon" (it is for `pai-app`'s architecture tables).
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
  2.4-chosen install folder. Left: the docs of 3c; the real WiX build and the installer test plan on
  Windows (the user); the GUI and `pai` write the user's `~\.llama.cpp` as ModelsDir into the manager's
  file on Windows instead of leaving it unset; an instance's working directory is still `paths::data_root()`
  of the instance's virtual account (a system profile); the engine's removal dialog says
  "configuration, logs and models" but also deletes `state\`; `msi-stop-processes.ps1` matches llama-server only under the framework's `bin\`; the release
  assets.) Folders (user, 2026-11-14): the program in `%ProgramFiles%\llama.cpp\` (with the
  read-only defaults); everything of the engine's instances under `%ProgramData%\llama.cpp\`:
  `config\` (`server@<id>.ini`, `presets.ini`, `presets.ini.disabled`; `paths::engine_config_dir()`),
  `logs\` (`llama-server@<id>.log` and its `.previous.log`) and `models\` (the default `ModelsDir`);
  `%ProgramData%\private-ai\` keeps only the framework's (`config\bff.ini`). Uninstalling llama.cpp's
  MSI stops and deletes the instance services and **asks whether to remove its configuration too**
  (user, 2026-11-14): a checkbox, cleared by default, as the framework's user-data question; a silent
  or basic-UI uninstall, and every upgrade, keeps `%ProgramData%\llama.cpp\`. Still to settle: the
  release assets (two installers per release).
- **macOS:** the same split of the `.pkg` (an engine `.pkg` beside the framework's).
- **The build is split too:** the engine and the framework are separate builds, so either can be
  rebuilt alone. The framework still reads the engine's source at build time (`pai-app`'s architecture
  tables); what bench.db records as the software stack must come from the installed engine, not from
  the framework's build: an open point for the slice.

The 2.99.x packages were never published, so no upgrade relation names them. Requirement change: R1,
R2 and the release process in `AGENTS.md` (installer name, release title) follow this.

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
**4** the macOS `.pkg`; **5** docs and the release process.

- Done when: `xtask package` on Linux makes the framework packages at `2.99.1` requiring `llama.cpp (>=
  1:1.0.0)` and the engine package at `1:1.0.0+pai1`; the engine or the framework can be rebuilt alone;
  the two Windows installers and the `.pkg` follow the same split; the docs describe it.

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

Later slices (not yet written): editing through the manager, the gateway, the benchmark portal, LAN
access with authentication.

## Waiting for the user

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
  default, loopback only until authentication exists; it has its own package `private-ai-framework-bff`
  (requires `-core` since step 11), runs as the static system user `private-ai-bff` on Linux (first `DynamicUser=yes`, replaced on 2026-11-14 after the real install: dbus-daemon drops a dynamic user at Hello) and as the virtual account
  `NT SERVICE\private-ai-bff` on Windows, enabled and started at install; macOS has none. Before any
  slice that changes something through the bff, a design of who is acting (today any local user could
  act through the bff's account). `cli.toml` moved to `-llama`, and with step 11 to `-core`, which every package requires (the
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

- **TypeScript 7 for the web UI:** `fe/` stays on TypeScript 6.0.x (user, 2026-11-14) because
  typescript-eslint supports `>=4.8.4 <6.1.0` and TS 7.0 has no API for external tools (expected
  with 7.1: typescript-eslint issue #12518, Microsoft's TS 7.0 announcement). Move to 7 only when
  every tool of the `fe/` stack supports it (typescript-eslint first), checked in each tool's own
  docs.
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
