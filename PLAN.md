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
| R6 | Per-GPU and per-model guides where every value traces to a campaign, published as a site | Met, but their figures predate v0.9.0: step 4 |
| R7 | The fastest presets for the test machine's GPUs: port an out-of-tree optimization when its gain pays for its upkeep | Open: steps 3 to 8 |
| R8 | llama-server runs as an OS service per instance (`llama.cpp@<id>` on Linux, config `server@<id>.ini`, a Windows service) managed by the runtime service `private-ai-manager`, which starts the instances at boot and is what the GUI and CLI talk to (D-Bus plus polkit on Linux, a named pipe with an ACL on Windows); units started by hand stay under its management; the manager owns every check on the config it writes and the GUI is a thin client; later engines (Strata) are to fit the same manager | Open: step 1 |
| R9 | The project is `private-ai-framework` and the configurator `private-ai-config` (decided 2026-11-13), with upgrades from the old packages and the user's data carried over | Open: step 2 |

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
were dropped at v0.9.0 and performance dropped with them (step 3). The test machine has two R9700 (one in
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
  `private-ai-manager.service` owns the D-Bus name on the system bus, starts at boot the instances it
  finds as `server@<id>.ini` files (one for now) and starts, stops and reports them through systemd's
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
   prints the run-instance usage hint. Before 3c: the D-Bus service must keep answering while polkit waits for a user to authenticate.
   Today the core's mutex is held during `CheckAuthorization` and zbus runs a connection's handlers on
   one executor thread, so every other call, status reads included, waits for the password prompt.
   **3c** (after step 2, the rename):
   the configurator's start, stop, status and every INI write
   go through the manager, `runstate`'s callers switched to it. The manager owns every check on the
   content it writes (decided with the user on 2026-11-13): before a write it runs all the checks the
   configurator runs today (`server_form.rs`, the presets checks) and refuses invalid content with an
   error carrying a strings key and its values, which the configurator shows in its own language. The
   configurator is a thin client: it keeps no check of its own; it may call the same shared code for
   feedback while the user types, but the manager's answer decides. The Windows SCM and pipe
   implementations come with slice 6.
4. **Logs.** The unit appends both streams to `/var/log/llama.cpp/llama-server@<id>.log`
   (`StandardOutput=append:`), the Windows service to `%ProgramData%\llama.cpp\logs\`; the closing and
   rotation of `runstate.rs` move to the manager, and the GUI's Log window reads the new path.
5. **Linux packaging.** The `-llama` package ships the two units, the D-Bus policy
   (`/usr/share/dbus-1/system.d/`), the polkit action file (`/usr/share/polkit-1/actions/`) and a
   sysusers.d entry for `llama-cpp`. This adds the first maintainer scripts to the `.deb` and `.rpm`
   (enable the manager, reload systemd), so the "no post-install script" rule in `AGENTS.md` changes.
   Checked with `xtask package` and the contents of the packages; installing them is the user's run.
6. **Windows packaging.** The MSI registers the manager service (automatic start) and creates
   `%ProgramData%\llama.cpp\` with the right ACL. SCM has no template services, so the manager creates
   one service per instance (`llama.cpp@<id>`, manual start) when the instance is added and removes it
   with the instance; the uninstall removes them. An upgrade keeps the config. Runs the installer test
   plan of `installer/AGENTS.md`.
7. **Migration.** The first start of the new configurator offers to copy the user's
   `server.ini` (as `server@default.ini`) and `presets.ini` into the system directory through the manager; the user's files are left
   in place.

Points of design, and what is still open:

- The service user must be able to read `ModelsDir` (and write it, as `LLAMA_CACHE`, for downloads by
  name). A models directory under a home with mode 0700 is not readable by `llama-cpp`: the manager
  reports it rather than changing permissions (agreed with the user on 2026-11-13).
- Polkit defaults, agreed with the user on 2026-11-13: start, stop and status for the active local
  session without a password, config writes `auth_admin_keep`.
- `SaveStateOnShutdown` and `StateDir`: the slot snapshots are written by the server, so by the service
  user; their default directory moves with it.
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
- **Paths:** install `/opt/private-ai` (Linux, macOS) and the Windows install dir; user data
  `~/.config/private-ai`, `~/.local/share/private-ai`, `%LOCALAPPDATA%\private-ai`; the registry key; the
  system config dir of step 1 (`/etc/private-ai`, `%ProgramData%\private-ai\config`). The engine's names
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

1. **The layout** (decided with the user on 2026-11-13: `private-ai-config` is one piece of the
   framework, beside the manager and the future bff and fe): a
   `crates/` directory with one crate per piece and a shared library. In two moves, each with no
   behaviour change and the existing tests green on the tester's rerun:
   Folders carry no prefix and packages are named `pai-*` (decided with the user on 2026-11-13; a
   package named bare `core` would clash with Rust's `core` crate).
   1. Done: `llama-cpp-config/` is `crates/config/`, the package `pai-config`.
   2. The shared modules (paths, INI and server config, the launch code, strings, ...) move to
      `crates/core` (package `pai-core`), and the manager becomes its own crate, `crates/manager`
      (package `pai-manager`), depending on the core only, so the service links no GUI library.
      The manager's tests move with it, with their own e2e test under the data-root rule.
2. **The names that behave:** the executables take the `pai-` acronym (decided with the user on
   2026-11-13): `pai-config` (the GUI and CLI, today `llama-cpp-config`), `pai-manager` (today
   `private-ai-manager`, so the unit runs `pai-manager run-instance <id>`), later `pai-bff` and `pai-fe`;
   the services (`private-ai-manager.service`, `llama.cpp@<id>.service`) and the D-Bus name keep their
   names. Also the window title and product strings, the environment variables,
   the user and system paths, the registry key, the service user; tests pin the new names first.
3. **Migration** of the old user tree, tests first.
4. **Packages:** names, `Replaces`/`Conflicts`/`Provides`, `Obsoletes`, the MSI's UpgradeCode kept;
   xtask tests first, `--dry-run` where it exists.
5. **Docs and scaffolding**, including the manager docs left from slices 3a and 3b of step 1.

- Done when: nothing outside the engine's own names and bench.db history says `llama.cpp-framework` or
  `llama-cpp-config`; the packages and the MSI upgrade an installed 2.4.0 in place (checked by the user on
  each OS); a user's existing config and data are found after the upgrade.

### 3. Decide whether to bring back patches 0002 to 0004

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

### 4. Re-measure the presets on the current release

Covers: R6, R7.

Do it after step 3, on the patch set it settles on: the figures below were written for v0.9.0
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

### 5. Find out why the drafter benchmarks look too favorable

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

### 6. Check the KV-split width patch on the current release

Covers: R7.

`upstream/UPSTREAM-PR-fa-kv-split-width.patch` pins the KV split for `Q->ne[1] <= 8` so a verify batch
reduces the KV like plain decode. Found not applicable while patch 0003's band was in; without the band
the R9700's decode takes the branch it changes. What was measured is in
[candidates.md](patches/llama.cpp/candidates.md), "KV-split width determinism".

- Done when: it is known whether the patch applies to the current tag and what it changes on the R9700
  (byte-identical replies across `spec-draft-n-max`, and the speed cost), and it is ported or dropped.

### 7. Compare block 12's all-reduce with upstream's internal AllReduce

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

### 8. Settle the open Windows checks

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

## Waiting for the user

- Steps 3 to 8: every benchmark run and every llama-server start on the GPUs needs the user's
  go-ahead (they hold the GPUs and may collide with other jobs).
- Steps 4 and 8 partly need the RTX 4070 SUPER back in the test machine.

## Later, not planned yet

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
