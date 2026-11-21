---
name: docs-playbook
description: What the docs agent has learned about keeping project docs accurate. Use when editing README, AGENTS.md or docs/.
---

Lessons only: the rules are in `AGENTS.md`.

- Keep three things apart: measured here, read in a source, reported by the user. Say which each fact is.
  A source about something similar is only a hint.
- When a measurement proves a note wrong, replace the note. Do not leave both.
- A rule about who does what lives in several places: `AGENTS.md` ("Commands", "Roles", the "Cycle:"
  line), the role bodies in `.opencode/agents/*.md`, the playbooks, and `PLAN.md`'s Definition of Done.
  Grep all of them when one changes, the role files' `description:` lines too (`tester`: "Use first in
  every slice"), and the coordinator's "done only on" sentence in the `AGENTS.md` Roles bullet.
- A rule about **who may configure** repeats in: `AGENTS.md` (the Windows and the Linux machine-tree
  bullets), `installer/AGENTS.md` (the pipe bullet, the features bullet, the group's own bullet,
  "What CI covers", the test plan steps), `crates/config/README.md` ("Through the manager"),
  `crates/bff/README.md`, the root `README.md`, `docs/build-packaging.md`, `ARCHITECTURE.md`,
  `fe/DESIGN.md`, `fe/briefs/`. Grep `Administrators`, `administrator` and the group's name in every
  `.md`. The manager's side is pinned by test names worth running:
  `cargo test -p pai-manager -- an_admin_member_controls_but_configures_only_in_pai` (the Windows
  decisions compile and run on Linux: `manager.rs` keeps `mod windows` unconditional, the Win32 glue
  is `cfg(windows)`).
- For the `pai` rule the sources to read before writing: `allowed` and `TokenFacts` in
  `crates/manager/src/manager/windows.rs` (control: interactive, Administrators or `pai`; configure:
  `pai` enabled and not deny-only, or LocalSystem), `ADMIN_GROUP = "pai"` in
  `crates/manager/src/manager/service.rs`, and the polkit rule in `xtask/src/linux/system_files.rs`.
  `installer/AGENTS.md`'s pipe bullet is the full Windows wording, so the READMEs carry one line each
  and point there. In the web docs "administrator" means a `pai` member: `canConfigure` is the second
  bool of the manager's `Authenticate` (`PasswordChecker::is_admin` answers `pai` membership on both
  OSes), so define it where `canConfigure` first appears and let the later uses inherit. Code doc
  comments and names lag the rule (the `PIPE_SDDL` doc says "neither interactive nor an
  administrator's", `is_admin` is named for the old one): code's to fix, report them.
- The Windows manager reads the client's TOKEN, so a group membership added after a logon counts at the
  **next logon**; the web sign-in's `Authenticate` reads the group database (`NetUserGetLocalGroups`,
  `logon::in_local_group`) and sees it at once. Two answers, no run has settled the first: say so.
- A test plan's "as an administrator" splits: a manager call needs the group (`configure`), a hand edit
  of `%ProgramData%\llama.cpp\config\` needs an elevated Administrators member (the folder's ACL, the
  group has nothing there). Re-read every one: the phrase never says which.
- A group the MSI creates is machine data: `util:Group` wants a per-machine key path (an HKLM value of
  the product's key) and `RemoveOnUninstall="no"`, because Util's default deletes it at every uninstall
  and `MajorUpgrade` uninstalls first, so each upgrade would empty what the administrator filled. The
  `.wxs` test in `xtask/src/common/msi.rs` reads the group's name from the manager's `ADMIN_GROUP`
  const, so renaming the group fails a test: grep the const too.
- PowerShell cmdlet syntax: verify on learn.microsoft.com with `?view=powershell-5.1` (Windows
  PowerShell 5.1, what the runners and the custom actions use) and cite the page.
- Docs quoting a strings-file text that names an old role (`api-key-withheld`: "Only an administrator
  can see it.") quote it as it is: the string is code's to change. Name the rule behind it
  (`configure`), report the string.
- `PLAN.md` can name a file that does not exist: it lists `crates/manager/README.md` as naming
  Administrators, and `git ls-files crates/manager` has no README and never had one. Check with
  `git ls-files` before promising to edit a file, and report the line.
- The `AGENTS.md` and README lines are paragraphs thousands of characters long: grep with a short
  context window (a small Python `re` script) instead of printing whole lines, and replace with an
  exact, count-checked substring. Re-read a file right before editing it: the coordinator edits
  `AGENTS.md` and `PLAN.md` while you work.
- Verify a tool's flag or lint name by running the tool, not from memory: a 4-line scratch crate under
  `/tmp/opencode` and `cargo clippy -- -D warnings` settles which lint a shape triggers (it is
  `clippy::comparison_to_empty` that refuses `s == ""`, not `clippy::is_empty`; `clippy::err_expect` and
  `clippy::single_element_loop` confirmed the same way, offline).
- Pruning a playbook: the line count only falls on whole-bullet deletions and re-wraps, a trim that keeps
  the same number of lines buys nothing, so `wc -l` after every pass. Grep the code for every file or
  function a bullet names before keeping it, and keep a `—` only inside a code span quoting data.
- `cargo clean -p <crate>` with no `--release`/`--profile` removes only that crate's dev-profile
  artifacts in the configured target dir (checked with `cargo clean -n -v`); `-n` is a safe dry run.
- After a directory move, grep for the old path in both slash styles and for `-p <old-package>`, and fix
  the relative links inside the moved files too (`crates/config/*.md` reaches the repo root with `../../`).
  Binary names are a separate rename: leave them unless the brief says so.
- After a crate split, take the package list from `cargo metadata --no-deps` (Commands list, the
  coordinator's `cargo clean -p`), and grep for `module::item` names (`runstate::server_args`) as well
  as paths: code doc comments lag the move too, and they are code's to fix, so report them.
- `grep` here is ugrep: a long `.{0,120}` context regex fails with "exceeds complexity limits". Use a
  Python `re` script instead.
- `updated` in a `gpu`/`model` page's front matter is the last change to a measured value: a rename
  or wording fix leaves it alone. Reference pages (`build-*.md`) take the day of any change.
- `benchmarks/specs/*.ini` are pinned by sha256 (`standard.versions`): never edit one, not even a
  comment; report a stale name in it instead. bench.db and `benchmarks/prompts/` are recorded data.
- The site turns a link out of `docs/` into a GitHub URL, so an anchor into a README is GitHub's slug
  (`### Migration from 2.4` is `#migration-from-24`). Check it in the built site (`--out <dir>`).
- After a product rename, grep the scripts the names lints do not scan (`tools/*.ps1`) for the old
  data paths, and describe what the script does today rather than the new name.
- After an install-layout change, grep the old prefix (`/opt/private-ai`) in the root `README.md`,
  `docs/build-*.md`, `AGENTS.md` and `crates/*/README.md`: the root README repeats the paths. A check
  done by hand on the old layout (an `strace`, a run) is not true of the new one: say it was not repeated.
- Windows code is type-checked with the GNU target only (`pai-core`, `pai-app`, `pai-cli`, `pai-manager`,
  `xtask`),
  and `pai-config`'s Windows arms not even that: say "type-checked only, not run on Windows" beside each
  Windows fact until the user's Windows run, and put what only that run can settle in the installer
  test plan (`installer/AGENTS.md`), not in the prose as fact.
- When files move out of the per-user tree (to the manager's machine directory), check what still
  keys on them or names them: the MSI's user-data dialog (`LLAMA_USERDATA_FOUND` and the dialog text in
  the `.wxs`, described in `installer/AGENTS.md` with its test plan step), the root README's config
  table and uninstall line.
- A string the docs paraphrase (the import offer, a dialog's text): read its current value in
  `crates/core/strings/*.toml` or the `.wxs` before writing, and grep `PLAN.md` too, which quotes them.
- A renamed heading renames its anchor: grep `#<old-slug>` in every `.md` (the root README links into
  `crates/config/README.md`) before and after.
- After modules move between crates, the crate docs name files bare (`model_profiles.rs`) or crate-relative
  (`src\model_profiles.rs`): fix the crate-relative ones, say once at the top which crate holds the bare
  ones, and recheck claims tied to the old crate (`pai-config` not GNU-checked, `system_connect`'s
  Windows arm, which `build.rs` emits what, the test crate a command needs: `cargo test -p <new>`).
- A repo-wide word rename (`preset` to `model profile`): grep the word and mark what stays BEFORE
  editing. What stays: llama.cpp's own words (`--models-preset`, `base_preset`, `common/preset.cpp`,
  `common_preset::to_args`), what is stored as data (bench.db's `run_param.scope` is `preset`, so a
  `bench-chart` comment and an `x`/`series`/`where` key keep the word: `sqlite3 benchmarks/bench.db
  "select distinct scope from run_param"`), a WiX `Component Id` (`DefaultPresetsIni`), a skill's own
  `name:`, a quoted design title (`ARCHITECTURE.md` "Saving a preset from the web"), and a run log of
  an older layout. Sweep with `sed` on a word boundary, then read every changed line: "the tensor
  preset" needs more than the substitution ("the tensor-split model profile").
- A build script that reads `build/<os>/...` of the TARGET OS (pai-app's clone lookup) fails the GNU
  check on Linux: the docs give the override with an absolute path (the script runs in its crate dir).
- After the command line moved to `pai` (package `pai-cli`), grep `pai-config (server|model-profile|control|bench|config)`
  in every `.md` and in skill `description:` lines; `pai-config` stays where it is the GUI, the
  executable a script runs (`--version`, `sdk-devices`) or what xtask builds and packages. A command run
  on a GUI-only `pai-config.exe` gets exit 2; the MSI's stopper tries `pai.exe` first for that reason.
- When a package is added, grep the counts in words (`five`, `three`, `all five`) as well as the file
  lists: README, `AGENTS.md`, `docs/build-pipeline.md` (intro and the OS table), `build-packaging.md`
  (relations table, `.deb`/`.rpm` scan bullets) and `build-macos.md` each repeat them. A package with
  no 2.4 predecessor is an exception in every "each replaces" sentence. Sizes measured on the old split
  are not the new packages': say so instead of carrying them over.
- When a crate is added, its name goes in: `AGENTS.md` Commands (crate list, Windows GNU list), the `release`
  skill's step 1 version list, the Map's crate list, `.opencode/agents/coordinator.md` (`cargo clean -p`), `coder.md` (GNU
  list), the source map's workspace row (`the six pai-* crates`), the README tree and command table,
  and `PLAN.md` W15 (the coordinator's: report it). The coder's GNU list lagged one crate already once.
- `xtask` messages can lag the build: check the phase that needs a tool, not the one that reports it.
  Whether `xtask configure` fails on a tool is `rows.miss` (a gap, fails) against `rows.opt` (a warning)
  in `xtask/src/<os>/configure.rs`. Vitest runs as `npm --prefix fe test` (seconds, no network).
- The fe checks run as `npm --prefix fe run typecheck`, `lint`, `format:check` (seconds each, write
  nothing): a new one goes in `AGENTS.md` Commands, `coder.md` and the bff README's web UI section.
- `components.rs` splits Linux and macOS alike: when a file changes package, fix the path tables of
  `build-packaging.md` AND `build-macos.md`, and the README's package table. A Windows service's open
  behaviour (what the SCM shows on a failed start) goes in `installer/AGENTS.md`'s open checks, phrased
  "by the code".
- When the web UI gains a view, what it shows is repeated in four places: `crates/bff/README.md`
  (intro, route table, "The web UI"), the `AGENTS.md` overview paragraph, and
  the root `README.md` intro. A route that reaches the manager can be checked by hand with no manager:
  run the debug `pai-bff` with `PRIVATE_AI_DATA_ROOT` (and `<root>/system-config/bff.ini` for a port);
  it answers 503 `unavailable`.
- A model profile route change is repeated in eight spots of `crates/bff/README.md`: the intro's served list,
  the route table's model profile rows, the `## The model profile routes` lead, its `### The list` / `### The switch`
  / `### The model profile page` / `### The save` sections, `### What has run` (the suite names and test
  counts), the fe route table's `#/profiles/model/<id>` row, the fe "Every model profile call of the contract is
  served" bullet, and `## Tests`. Renaming the H2
  renames its anchor: the root README and `AGENTS.md` carry one sentence each. The `; url =` reader's
  home is `crates/core/src/ini.rs` (`project_url_of` over `comment_value`) since 36a892d, its tests
  `cargo test -p pai-core -- project_url`; `crates/config/README.md`'s source map does not name it.
- A listen-address or `Host`-guard fact is in five files: `crates/bff/README.md` (intro, "Running it",
  the dated hand checks, the session cookie bullet), `AGENTS.md` (overview and the Map bullet),
  `ARCHITECTURE.md` (the diagram's browser arrow, the `pai-bff` row, the "Boundaries a call crosses"
  paragraph), the root `README.md` (intro and the `crates\bff\` row), and `installer/AGENTS.md` (the
  web service bullet, and test plan steps that call a `0.0.0.0` listen "refused", true before 1aac8e1
  and false since: not ours, report it). The cookie binds to the host, never to a port (`session.rs`
  sets no `Domain`), and the guard's 403 is `request-bad-host`, the cross-origin one
  `request-cross-origin` (`origin.rs`): both named in `crates/core/strings/cli.toml`, which no longer
  `bff-listen-not-loopback`.
- The `Host` guard's names are NOT read only at the start (42e5c23): with no `[Bff] Hosts`, a `Host` in
  none of them, not `localhost` and not an address, makes the guard read the host name and the
  resolver's search domains once and replace its list (`origin.rs` `reread`, the source from
  `lib.rs` `try_router_at`); a `[Bff] Hosts` list is fixed and never re-read. So a sentence saying the
  names are "read at the bff's start" is stale: grep `start` in that paragraph of
  `crates/bff/README.md` ("Running it"). Its builders name the difference:
  `router_with_session_and_hosts` is a fixed list, `..._and_host_source` a machine the test changes.
- The estimate's inputs moved to `pai-core` (1694064): `pai_core::gguf_shape` (`parse_header`,
  `read_local_header`, `local_shape`, `shape_of`, `kv_per_token`, `GPU_RESERVE`, `REF_CTX`,
  `MAX_HEADER`) and `pai_core::devices` (`parse`, `DeviceOption`). `pai-app`'s `hf_fit.rs` and
  `devices.rs` re-export every name, so a doc naming those modules for the arithmetic is still true;
  a source-map row that says the file HOLDS the parser is not: name the crate that does. The rows are
  in `crates/config/README.md`'s source layout, and `ARCHITECTURE.md` (the memory-check record's
  "`hf_fit`'s `GPU_RESERVE`", "`hf_fit::local_shape`", its "move from `pai-app` to `pai-core`"
  future tense, and "exists in `pai-app` ... which the manager does not link") is the architect's.
- The bff suites and what reaches them: `tests/model_profile_save.rs` (47), `tests/model_profile_read.rs` (32),
  `tests/listen.rs` (17), `tests/origin.rs` (40 since 42e5c23), and `src/tests/model_profile_body.rs` (24), a **lib** test:
  `cargo test -p pai-bff --lib` reaches it, a `--test <name>` flag does not. The 5 s reads and the 15 s
  write of the save are counted from the request (`save` takes `write_timeout.saturating_sub(elapsed)`).
- The server profile pool (b535304): the five routes are `crates/bff/README.md`'s
  `## The server profile pool` (the list, the create, the document and the save, the delete), and the
  manager's three verbs are their own bullets in `crates/config/README.md`'s "Through the manager",
  beside the `SetServerProfile` pair. Keep that split: the bff's rows say what a route answers and link
  there for what the manager does. Three facts that read backwards if you do not check the code: the
  list is sorted by id in the **manager** (`server_profiles`, the bff keeps its order); a profile no
  instance runs is judged against **no** address, so it can carry a running instance's port and meets
  the instances only at `SetServerProfile`; and `CreateServerProfile`'s `port` of `0` means "no `Port`
  key" (`port_of`), not port 0.
- `tools/site` builds pages from `docs/` only (`tools/site/src/main.rs`), so a change to the root
  `README.md`, `AGENTS.md`, `ARCHITECTURE.md` or a crate README needs no
  `cargo run -p docs-site -- --check`. For an anchor, a small Python `re` pass over `](#...)` against
  the headings of the same file settles it.
- The count of a model profile page's editable cards moves with every fe slice (three at c69511e, four at
  dc1ff69, the same day): with `fe/` off-limits, run `git log --oneline <last-known>..HEAD` and read
  the `2h.<n>` messages before writing a count.
- The fe can run ahead of the bff: a route the fe calls is served only if `router_with` in
  `crates/bff/src/lib.rs` has it (`fe/briefs/api-contract.md` marks the rest "proposed"). Describe
  such a page as built against the contract, and say what today's bff makes of it (404, no JSON body).
- When a package is split or a file changes package, recheck what moved with the maintainer scripts:
  a "the scripts restart the services" sentence holds only for the package that has them. Each
  package's scripts are in `system_files.rs` (`deb_scripts`, `rpm_scriptlets`, one function per
  package), its rpm `Requires(<tag>)` in `package.rs`. A unit's package is named in four places:
  `build-packaging.md` (Holds table, paths table, system files heading and row), the README package table, and the manager row of `crates/config/README.md`'s source map.
- A setting that is the service manager's own state (start at login, start at boot) is described
  once in full in "Through the manager" of `crates/config/README.md`; `AGENTS.md`, the README and
  `installer/AGENTS.md` (its test plan) carry one line each. `components::file_stem` and
  `of()` are shared with macOS: a Linux split renames and resplits the `.pkg` set too, even when the
  macOS slice is "later"; say so on `build-macos.md` rather than leave its table looking current.
- When a package starts shipping defaults copied "only when missing", recheck every flow keyed on a
  file being absent: `migration::import_offered` skips a file the manager already has, so the import
  of a 2.4 `server.ini`/`presets.ini` stops being offered on a packaged Linux install. The per-user
  tree and the manager's tree name the model profile files DIFFERENTLY (per-user `presets.ini` and
  `presets.ini.disabled`, manager `models.ini` and `models.ini.disabled`): the import is the rename
  point, `migration::USER_FILES` pairs each pair, `migrate.rs`'s `OURS` and `OUR_CONFIG_FILES` hold
  the 2.4 names, and a packaged machine has both sets on disk (here `/etc/llama.cpp` holds
  `presets.ini` beside `models.ini`). Say which tree a named file sits in, and name the offer's
  subject as the old-named user file whose manager-named twin is missing. The instance
  files' directory is `paths::engine_config_dir` (`/etc/llama.cpp` on Linux); `system_config_dir`
  is the framework's (`bff.ini`); grep both names and both paths after a layout change.
- The content of the engine's shipped `crates/core/defaults/server@default.ini` (starting values only,
  pinned by `server_cfg.rs` tests) is described in one place, `build-packaging.md` ("No config file is
  shipped"); `AGENTS.md`, the READMEs and `PLAN.md` only name the file. Change that paragraph alone.
- A maintainer-script change is described in `build-packaging.md`'s "When" table (Removal, Purge rows),
  the `DPKG_ROOT` sentence below it and the rpm `Requires(<tag>)` sentence; the "not run for real" line
  of the install log stays until a real removal or purge. A Windows manager message key is named only in
  `installer/AGENTS.md`'s manager bullet: grep `cli.toml`'s old key in every `.md` after a rename.
- When an xtask phase's arguments change, they are named in: `build-pipeline.md` (Options and "re-run"
  tables), the README command table,
  `build-compiling.md` (node/npm line, web UI section, Linux "built last"), `build-packaging.md`
  (MSI and `.deb`/`.rpm` intros), `build-macos.md` ("Package."), and the `scripts/0N-*` usage headers
  (not ours: report them). `xtask install`'s "predates the build" warning keys on the build tree's
  `llama-server` mtime only (`linux/install.rs`): recheck it when what a build updates changes.
- The Windows installer's facts repeat in: `installer/AGENTS.md` (all, with the test plan),
  `build-packaging.md` (intro and file table, "The MSIs (Windows)", the Install table and bullets),
  `build-pipeline.md` (intro, Options, re-run table, OS table, "Where things land", footguns),
  the README (intro, features, steps, tree, config table, command table, Packaging), `AGENTS.md`
  (overview, the conventions on `%ProgramData%`), the `release` skill (steps 3 and 8, the names table), `crates/config/README.md` ("Through
  the manager", the source map's manager row), `crates/config/AGENTS.md` (log source) and
  `crates/bff/README.md`. Per-product build files are named from the staging dir
  (`stage.with_extension`): `build\windows\staging\<product>.license.rtf`, `.wix-obj\`.
- A new folder under `%ProgramData%\llama.cpp` or a new custom action of llama.cpp's MSI: update the
  `installer/AGENTS.md` product table, ACL table, defines line, scripts bullet, the test plan's log
  list and `icacls` step, and reread the removal dialog's text in `llama.cpp.wxs` (it names what
  `DeleteEngineData` deletes, and lagged `state\`). "Text-checked" there means xtask's tests read
  the `.wxs` and evaluate its conditions (`msi/tests/engine_lifecycle.rs`), never a WiX compile. A
  folder a service creates at run time (the manager's `logs\` under `%ProgramData%\private-ai`): the
  MSI does not know it, so say it inherits the parent's DACL, outlives the uninstall and has no size
  cap (an open check), and add the test plan checks that show it: a fresh install has no such folder,
  the first guarded save makes it, a standard user reads the file (the manager records every guarded
  call, done or refused).
- Windows-style paths written through a Python script need a raw string: `"docs\build-macos.md"` puts a
  backspace byte in the file (one sat in `AGENTS.md` as `docs\x08uild-macos.md`). `grep -c $'\x08'`
  the files after such an edit.
- `xtask install` takes no product on any OS (only files); `package` and `build` do.
  `version::llama_build` refuses a non-release clone on every OS: there is no `b<N>` package name.
- Before writing "X no longer writes Y" (a closed gap), grep every caller of the default behind it
  (`models_dir_or_default`, `set_server_key("ModelsDir"`): `hf::save_preset` still pinned the user's
  folder after the Server tab stopped. Scope the claim to what the commit changed, list the rest.
- The release process (`.agents/skills/release/SKILL.md`) lists every package file per OS and the upload globs:
  when `components.rs`, `msi::file_name` or `macos_pkg::file_name` change a name, fix it there too. Only
  Windows refuses a 2 GiB asset (`windows/package.rs`); nothing pins the llama.cpp tag across the
  three build machines (`common/llama.rs` takes the newest release at build time).
- When an OS gap closes (macOS's release-tag refusal, its build dir), grep `does not yet`, `Unlike Linux`,
  `macOS does not` and the "Open, by the code" list: the `release` skill's steps, `build-packaging.md`'s
  file-name bullets and `build-macos.md` each repeat the gap.
- When an item of `installer/AGENTS.md`'s "Known gaps" closes in code, remove it there AND turn it into
  a test plan check (what a Windows run must show), since the fix is only type-checked.
- `AGENTS.md` is loaded into every session (keep it under ~40k characters): its Map bullets are
  pointers (what the area is, the guide, a safety rule the guide lacks), never a copy of the guide.
  A new fact goes in the guide; before cutting one from `AGENTS.md`, grep the guide for it and move it
  there if missing. Task workflows (the release) live in skills.
- The CI-only minimal build (`configure --minimal`) is described once, in "The minimal build" of
  `build-compiling.md`; `build-pipeline.md` (Options), `build-packaging.md` (file names),
  `build-macos.md` and the `release` skill (step 8) carry a line each and link there.
- The installer CI workflows (`.github/workflows/installers-*.yml`, started by hand) are described in
  `docs/build-macos.md` ("CI.") and `installer/AGENTS.md` (the Status paragraph). A run's id and result
  come from the coordinator's commit (no web fetch): cite the commit. When one goes green, also grep
  "untested", "never run on a Mac", "type-checked only", "not run on Windows" and "compiled by WiX" in
  `AGENTS.md` (overview, the `pai-manager` sentence, Map), the root `README.md` (intro, the `xtask
  install` row, Packaging), `build-pipeline.md` (intro, the OS table's Status and Packages rows),
  `build-packaging.md` (MSIs intro, the Install bullets), `build-compiling.md` ("The minimal build"),
  `crates/bff/README.md`, `installer/AGENTS.md` (the manager bullet too), the `release` skill (step 4)
  and `PLAN.md` R3. The Windows test plan opens with "What CI covers": update it with the workflow.
  A JSON body quoted in a brief may be shortened: check the route's fields in the code before quoting.
- A run's date and duration are NOT in the report commit: cite the run id with the commit it ran on
  and the commit reporting it, never a date inferred from that commit's date. One run's Windows
  coverage sits in six sentences: `installer/AGENTS.md`'s Status ("the latest green run"), the manager
  bullet ("only CI has run it"), the group bullet ("Text-checked only"), "What CI covers" (the run
  list, its "no green run has executed them"), the Open checks' `PaiGroup` item ("have never run") and
  `AGENTS.md`'s Map bullet (the id beside "what that leaves unchecked"): grep "latest green run",
  "no green run", "have never run", "no Windows run has" beside the ids. Settle only what the run's
  steps ran: CI's install, repair and uninstall are silent, so a green run says nothing about a
  full-UI install (step 4, the empty group), an upgrade (step 9), the full-UI uninstall (step 12), a
  `configure` as a member (step 18) or the token's next-logon rule (step 18 stays "unverified").
- A commit message's reason can be wrong in a detail: check it in the code before repeating it
  (dbc43cb blamed the instance unit's `ProtectHome`, which only `private-ai-bff.service` sets:
  `xtask/src/linux/system_files.rs`).
- When a `PLAN.md` step closes, grep `step <N>` (not only `PLAN.md step <N>`) in every `.md` and in
  `.github/workflows/*.yml` comments: plan numbers are reused, so a bare `step 12` may mean an older
  step, and the installer test plan has its own numbered steps. Point to the file that now holds it.
- A manager protocol change (a method's arguments or reply) is described once, in "Through the manager"
  of `crates/config/README.md` (D-Bus signature and pipe JSON); `installer/AGENTS.md`'s pipe bullet and
  `crates/bff/README.md` point there. Check per-call authorization in `manager/service.rs` itself:
  the README said "reads need none" long after 7af81f4 made an instance file's read need `control`.
  The same change moves FOUR numbers on that one page: the `As` bullet's count of guarded methods and its
  signature list, its `Version` protocol number and what each added, and the source map's manager row
  ("the `As` form of each of the N guarded methods", a list that lagged `WriteFileIf` for one protocol).
  Grep `guarded method`, `protocol ` and the count in words. `ARCHITECTURE.md`'s protocol table and its
  "the seven guarded ones" repeat it and are the architect's. Count the guarded methods from the code,
  not from the prose: `grep -c "async fn .*_as(" crates/manager/src/manager/dbus.rs` (twelve at
  b535304, thirteen at 66b2bb3), and note the page carried TWO word counts (the `As` bullet's "ten", the source map's "nine",
  already inconsistent before the slice). A slice that adds verbs without bumping `PROTOCOL` (b535304
  added `CreateServerProfile`, `DeleteServerProfile` and `ServerProfiles` on 3) needs a sentence saying
  the number does not name them, so a manager at 3 from before the slice answers them as an unknown
  method: grep `protocol ` for a sentence that implies the number is the proof.
- The node's device list (66b2bb3) splits across two pages on purpose: the manager's shape (D-Bus
  `(t probed_at, a(ssstt) devices, t ram_mib, u cpu_threads)`, the
  pipe's keys, who is asked, `probed_at` 0 when no probe
  answered) goes in `crates/config/README.md`'s "Through the manager" beside `ServerProfiles`, and the
  bff's JSON and its status codes in `crates/bff/README.md`'s "The node". The probe's own rules (one at
  a time behind a `probing` lock, only a probe that read a non-CPU card cached, keyed by the exe's
  mtime, the same probe the memory check measures against) are the manager's, so they go in the
  "manager runs every content check" bullet where `ProbedGpus` is already described, not in a new
  bullet. `Devices` is `Action::Control` asked **quietly** (`authorize_quietly`): the polkit action's
  description in `xtask/src/linux/system_files.rs` still says "start, stop, restart", so
  `build-packaging.md`'s row, which quotes that file, stays as it is and the rule goes in the
  authorization bullet (report the lag, it is code's).
- A read with a bound of its own breaks two absolute sentences in `crates/bff/README.md`: "The reads of
  the router keep the 5 s `MANAGER_TIMEOUT`" (grep `keep the 5 s`) and "The limits the service runs
  with, 5 s of read and 15 s of write" in "What has run" (grep `limits the service runs with`) whose
  pinning test is `src/tests/timeouts.rs` alone; a route's own bound is pinned inside its suite
  (`the_devices_deadline_is_the_probe_bound_plus_five_seconds` in `tests/node_devices.rs`), so name two
  tests. `DEVICES_TIMEOUT` is `GPU_PROBE_TIMEOUT` plus the transport's 5 s: say which two, not just 35.
- A reply widened in place (a1a3458 added `ram_mib` / `cpu_threads` to the `Devices` reply at
  protocol 3, no bump) touches four places: the manager's `Devices()` bullet in
  `crates/config/README.md` (both signatures, the strict decode, and the sentence saying
  `PROTOCOL` does not name the pair, beside the same sentence for the pool and instance verbs),
  the pipe and D-Bus module headers (the coder's), the device bullet and route row of
  `crates/bff/README.md`, and the route of `fe/briefs/api-contract.md`. The suite that pins a
  widened reply keeps its own count, so the "What has run" number is what moves (`node_devices.rs`
  stayed 12 tests while the bff went 470 to 596): quote the commit's own counts. A 0 that means
  "no figure" needs saying on all three pages: the host reads (`crates/core/src/host.rs`) answer 0
  when they fail, the bff scales nothing, and the fe shows neither `0 MiB` nor a 0 cap.
- Two CPU counts live side by side and the docs must not merge them: the server document's
  `limits.Threads` / `ThreadsBatch` maximum is the bff's own (`server.rs` `own_cpu_count`, 1 when
  the host answers nothing; the two saves repeat `available_parallelism` inline), not the node's
  `cpuThreads` of `GET /api/node/devices`. The reason is a bound, not a choice: `Devices` can run
  the engine's probe for 30 s and the document reads keep 5 s. The Server form reading
  `cpuThreads` is R5, open work in `PLAN.md`: say it is open, and name whose count the page reads
  today.
- The client module headers carry the transport's shape as soon as the coder builds it
  (`crates/core/src/client/pipe.rs`, `crates/core/src/client/dbus.rs`: the `As` forms, `Version`,
  `acting_for`, what an older manager answers): read them before planning a comment edit, they usually
  need none, and keep the manager's own rules (who is checked, when, what is recorded) out of a client
  file. A design's parenthetical is not an assertion: ARCHITECTURE.md's "(today the parser ignores
  unknown fields)" left the pipe's answer to a `principal` on a plain method undecidable with no test
  open, so that rule stayed out of the docs and was reported.
- The server.ini rules (`checks::server_file`, plus `pai-app` `server_cfg::check`'s thread bound) are
  listed once, in the "manager runs every content check" bullet of `crates/config/README.md`; the
  `pai server set` paragraph under its command block and the root README's headless line point there.
  `limits` in `crates/bff/README.md` follows `checks::server_int_range` (no `LogVerbosity` in it).
  The model profile rules (`checks::models_file`, `models_disabled_file`, `model_profile_ids_free`) and the save
  side (`model_profiles::validate_for_save`, its callers `model_profiles::save` and `hf::save_model_profile`) sit in the same
  bullet; the "Ids are unique across both files" bullet of the Models tab points there.
  The value rules (`checks::model_profile_values`: float, enum, `spec-type`, JSON, min <= max) are in that
  bullet too; the MoE cache floor (`model_profiles::validate_moe_cache_floor`, save only, needs the GGUF) is
  named there, in the Hardware config bullet and the `moe_cache.rs` source-map row; a sweep's
  up-front check in "The key is typed" bullet and the `sweep.rs` row.
- The save-time address check follows the profile, not the instance: its rules (host must resolve
  or run; port judged only for a profile in use, against the other instances' profiles and the web
  service's `[Bff] Listen` read through `pai_core::bff_cfg`; an unreadable profile judges nothing;
  a refusal names the instance or `private-ai-bff`) are stated once, in the "manager runs every
  content check" bullet of `crates/config/README.md`. The same fact repeats in six places: that
  bullet, the bff README's "Running it" bullet, its 422 row, its "server file of an instance" lead,
  the `paths.rs` and `bff_cfg.rs` source-map rows, and the `AGENTS.md` machine-tree sentence. When
  the profile lookup changes, check all of them.
- The route names the server file after the instance id (`server@<id>.ini`); it names the profile
  in use only because the shipped defaults carry `[default] ServerProfile = default` (the lookup:
  `paths::instance_server_profile`). Both READMEs now say this in one sentence; keep them so.
- After an fe slice, check the bff README's web UI route table against `fe/src/route.ts` and the
  `call(` paths in `fe/src/*.tsx`, and the served routes with `grep -rn '"/api' crates/bff/src`: the
  table lagged two fe slices once (run controls, start at boot). The overview sentences in `AGENTS.md`
  and the root README repeat the page list.
- Sign-in's mechanics (who signs in, the status/key of each reply, the lockout) sit in the bff README
  in five spots: intro, the `SystemAuthenticator` paragraph, the status table, the "In memory only"
  lockout bullet and the web UI's "The bff does not serve yet" bullet, plus the sentence naming the
  calls the bff makes to the manager. When sign-in changes, replace the forward-looking wording in
  all five, and mark any dated hand-check whose replies no longer hold with the commit it predates.
  Files outside the docs agent's scope repeat them and lag: `ARCHITECTURE.md`'s bff row,
  `crates/config/README.md`'s source-map manager row ("the bff does not call it yet"),
  `fe/briefs/api-contract.md` and `fe/briefs/01-server.md` ("answers this to every sign-in"),
  `PLAN.md`'s step 10 6a note.
- A sign-in reply-shape change is repeated in the bff README in nine places: the route table's two
  `/api/session` rows, the session status table's 200 row, the server-read paragraph's session check,
  the 200 row and the `values`/`withheld` bullets of the server table, the fe shell's banner sentence,
  the fe route table's instance row (its administrator/user pair) and the fe "The session" bullet.
  Grep `canConfigure`, `role`, `withheld`, `administrat` there (296165f dropped `canConfigure`). The
  API key's withholding sits in three of them: when the rule moves (6217308: the manager's
  `read_file` flag alone, never the session), fix all three and name who decides.
- The bff README carries the manager's refusal as two keys, one per kind of route, and grep both there
  before writing one: `session-not-administrator` (403) is a signed-in route's answer when the manager
  refuses its user, and the session ends with it (5f0d405: the sign-in's own key, so the web UI tells an
  ended session from a live one without a second read); `client-not-authorized` (502) is what a
  session-free read answers when the bff's own account is refused (the instances list, `GET boot`:
  `control.rs` `read_failure`, `lib.rs`).
- A bff route that writes two files at once (the model profile switch): read the handler and its blocking task
  before writing the order. What is not obvious and broke a draft: `WriteState` is **per request**
  (shared by the handler and its task, `crates/bff/src/server.rs`), so the save and the switch share
  the 503/504 rule, not a lock; a bff read route runs no content check at all (a 422 comes from the
  manager's write), and which checks run per file is `check_contents` in
  `crates/manager/src/manager/service.rs` (`models_file` then `model_profile_ids_free` for
  `models.ini`, `models_disabled_file` then `model_profile_ids_free` for `models.ini.disabled`, each `?` first).
  A write that puts a model profile in effect also runs the manager's model-file check, which is not in
  `pai-core` `checks.rs` (`check_model_files` in the manager's `check_contents`, after `check_renders`, and
  `check_model_files_of` inside `set_model_profile`). Its 422 `field` is `field_of` in `crates/bff/src/model_profiles.rs`:
  a key whose args name a VALUE and not a key (`model-profile-no-model`, `model-profile-model-missing`, the
  `load-on-startup-*` pair) needs an entry there, so the prose that lists them (the switch's 422 paragraph,
  which the save's table points at) must list it too. A key that marks NOTHING (the memory check's
  `model-profile-over-memory`, 16e6693: args the model profile, an instance, two MiB figures) needs an
  entry there as well, so that paragraph's no-`field` list and the save's 422 row both gain it; the
  check's own wording goes in the config README's checks bullet (right after the model-file sentences)
  and its `SetModelProfile` bullet, and the probe's figures in the source map's `..\core\src\proc.rs`
  row (the 30 s bound, the 4 MiB of each stream kept, the 2 s grace): `ProbedGpus` and
  `check_render_memory` are the source, none of it is in `checks.rs`.
- The bff's two time limits: `MANAGER_TIMEOUT` (5 s, reads and control verbs) and `WRITE_TIMEOUT`
  (15 s, the server save and the model profile switch), both `lib.rs`, pinned by
  `the_write_limit_outlasts_the_read_limit` (`src/tests/timeouts.rs`). The 5 s sits in five bff README
  spots (the instances 503 row, the connect bullet, the server-read paragraph and its 503 row, the
  list's 503 row): grep `5 s` and say which routes the longer limit covers. A 504 there means only a
  write's **unknown** outcome: a write that landed is a 200 with `running: true` even when the limit
  ran out on the read back, so grep `504` in the same file when a commit changes that rule. Two
  sentences there go false the day a route is added, both by naming a list: "The two routes that write
  a file, the server save and the model profile switch" (grep `routes that write a file`; at b535304 it
  became six, the pool's list keeping the 5 s) and "the three calls the manager answers without
  authorization" (grep `without authorization`; `ServerProfiles` made it four).
- The association routes (2d9a4c8) are one shape in three files: `crates/bff/README.md` (the intro's
  served list, the route table's four new rows, the instances row's `serverProfile`, the pool's
  `assignments`, the switch's and save's `restart`, and a `## The association routes` of their own),
  `AGENTS.md` (the overview's served list, and BOTH machine-tree bullets' "which clients read and never
  write" of `assignments.ini` and `instances.ini`, which the web UI now writes through the manager), and
  `fe/briefs/api-contract.md` (its route table, the served block, and its numbered "Open now" list).
  Grep the route name and `assignments.ini` in all three. In the contract a closed open point KEEPS its
  number and starts "Closed at <sha>:", because a line above cites it as "open point 4 below".
- What those four write is the manager's record, and their calls run on TWO principals: the file write as
  the signed-in user (its refusal is 403 `session-not-administrator` and ends the session), the instances
  list and the state read on the bff's own account (their refusal is 502 `client-not-authorized`, the
  session lives on; a state read that fails after a write that stands answers `running` true). Say both,
  once. They are also the only writes that never answer 504: the answer needs the state read, so a write
  that outlives the 15 s is 503 `client-unavailable` (the model profile switch and save keep their 504).
- `AGENTS.md`'s own em-dash rule quotes the glyph it bans, so `grep -c $'\u2014' AGENTS.md` is 1 on a
  clean file: check the glyph in `git diff`, not in the whole file.
- The working tree can be ahead of HEAD in `fe/` (the coder's slice in progress): check
  `git show HEAD:fe/src/<file>` before saying what a page shows, and route the fact through the
  strings key (`model-profile-load-failed`). The model profile page's route is `#/profiles/model/<id>` (`fe/src/route.ts`),
  not `#/presets/<id>` (the old one, redirected), and `GET` of an unregistered path is a bare 404 with no JSON body
  (`crates/bff/src/lib.rs`), which the fe reads as its generic "could not be read".
- `cargo test -p pai-bff` prints one block per suite and the tail shows only the last: scope it with
  `cargo test -p pai-bff --test model_profiles --test model_profiles_switch` and grep `test result` to see the
  suites you mean. When no shell is available, count the test functions in the suite file
  (`^async fn` and `^#[test]`) to check a README's test count, and keep the green run's date cited to
  the commit whose report it is. The exact count of one file is `grep -c '#\[test\]\|#\[tokio::test\]'`
  (the coarse pattern over-counts, helper fns and doc text, and MISSES
  `#[tokio::test(flavor = ...)]`, so a whole-crate total wants
  `grep -cE '#\[(tokio::)?test(\(|\])'` over the crate's files, and is a count of attributes, not a
  run, so say which it is (the bff: 583 across its 21 targets at 2d9a4c8, 19 integration files plus the
  lib and the bin)): a README's per-suite count is the count of
  the tree it cites, so recount a suite only when a commit changed it (`git show <sha>:<suite> | grep -c`
  settles the old number) and leave an untouched suite at its dated count. To date one test of a suite
  to the commit that added it, for "the two of X at <sha>", use
  `git log --oneline -1 -S "<fn name>" -- <suite>` (the pickaxe finds the added function).
- When `fe/` is off-limits, the fe commit's message (`git log -1 --format=%B <sha>`) settles what the
  fe shows now (1827023: no role word in the banner, the administrator/user pairs collapse to one
  case): cite the sha, and paraphrase a string rather than quoting it, its text living in
  `fe/src/strings/en_US.json`.
- The Read tool shows `N: ` plus a space before each line, so a file's 2-space indent reads as 3:
  an Edit `oldString` that starts with that leading whitespace never matches. Start `oldString`
  mid-line.
- When an authorization rule changes (polkit defaults, rules files, groups), grep `auth_admin`,
  `auth_self`, `administrator` and the action id across `AGENTS.md`, both READMEs of `crates/config`
  and `crates/bff`, the root README and `build-packaging.md`; `ARCHITECTURE.md` and `fe/briefs/` repeat
  them too but are not ours: report them. Absolute sentences ("No config file is shipped", "overwritten
  on every upgrade") break when a package gains a conffile. Before citing a `PLAN.md` slice for a bff
  route, check the slice lists that route.
- `ARCHITECTURE.md` design records go stale the day the slice is built and read as open work: the
  heading's "Designed <date>, proposed" becomes "built (<sha>, its follow-ups <sha>)", the "**What
  exists.**" paragraph is relabelled "**What existed when this was designed.**" (its text kept), and a
  key that moved after the design is recorded in that section's own "**As built**" paragraph, leaving
  the record's "Done when" text alone (the model profile save: 3eb24b2, follow-ups 26f5413; 403 key: 5f0d405).
  Grep `proposed.`, `What exists.`, `is open`, `are open` at the start of each section.
- The sign-in-only rule (user, 2026-11-19: a visitor sees only the sign-in page) is in
  `crates/bff/README.md` (intro, the route table's rows, the instances status table, the session's
  idle bullet, the dated hand checks, the fe route table's `#/` row) and `AGENTS.md`'s overview, and
  outside those in the root `README.md`'s overview sentence, `ARCHITECTURE.md` (the `Connect`
  paragraph's "session-free routes", a "session-free Overview", the node-routes record) and
  `PLAN.md`: grep `without sign-in`, `session-free`, `for anyone` in every `.md`. The gate is
  `Sessions::peek`: the version, the instances and `GET boot` are gated and connect as the bff's own
  account, so the session section's "each use moves the deadline" bullet must name the three polled
  reads that leave it where it was, and the instances list answers 401 BEFORE the manager is asked: a
  dated check that logged a 503 there needs its date inside the sentence, not beside it.
- A markdown table row is ONE line: cell text that wraps breaks the table. Join with
  `sed -i 'N{N;s/\n/ /}'`, then check every `^|` line ends with `|`.
- A gate of the form "run the docs-site check only if `pgrep -fc 'cargo|rustc'` is 0" never passes:
  the brief's own argv contains the word, so the launcher shell, the parent `opencode run` and the
  pgrep shell are all counted. Ask for the programs: `pgrep -xc cargo` and `pgrep -xc rustc`.
- The instance's snapshot folder (`instance_state_dir()/<id>`, `launch::with_instance_defaults`) is in
  five sentences: `AGENTS.md` (the Windows and the Linux machine-tree bullets), `build-packaging.md`
  (the Windows folder paragraph, the shipped `server@default.ini` paragraph) and
  `crates/config/README.md` (the models-dir bullet, the snapshots card, the "Models and snapshots by
  default" row). Read the code before phrasing: the GUI's placeholder is the ROOT, id not joined
  (`gui.rs`'s `InstanceDirs`), the launch and the readers join the id (`launch::instance_command`,
  `slot_state::restore_newest`, `cli.rs` with `default`), so "the placeholder names the instance's
  folder" is wrong; the shipped Linux file names `ModelsDir` (`/var/lib/llama.cpp/models`, the
  engine's tmpfiles.d entry) and no `StateDir`.
- A `python3 - <<PY` write of a whole file can land silently unwritten (three line-level rewrites of
  `crates/bff/README.md` reported success and left the bytes; `>>` and `sed -i` landed): re-read the
  file after a scripted write, and prefer the edit tool for a line edit.
- A standalone HTML file (a flyer, a printable sheet) is the docs agent's to write, and the site
  ignores it: `page.rs` `load_all` reads only top-level `.md` of `docs/`, and `main.rs` copies only
  `docs/assets` (`copy_dir`), so a subdirectory of `docs/` publishes nothing. Check such a file
  offline: `~/.cache/ms-playwright/chromium-*/chrome-linux64/chrome --headless --no-sandbox
  --no-pdf-header-footer --print-to-pdf <f> file://<f>` then `pdfinfo` for the page count (the flag
  is `--no-pdf-header-footer`; `--print-to-pdf-no-header` leaves the date and URL in), give each run
  its own `--user-data-dir`, and add `--window-size=W,H --screenshot` to see the screen view.
  Inline SVG paints in document order: a later filled `rect` covers a `text` drawn before it, which
  a screenshot catches and a PDF text dump does not.
- Rendering a print page (`docs/flyer/`): Playwright loads from `require('<repo>/fe/ux/node_modules/playwright')`
  in a script under `/tmp`; `page.pdf({preferCSSPageSize: true, printBackground: true})`, count pages with
  `re.findall(rb'/Type\s*/Page[^s]', pdf)`, look at `pdftoppm -r 60 -png`. A fixed `height: 297mm` with
  `overflow: hidden` hides overflow from the page count: compare the sheet's `scrollHeight` and
  `clientHeight` under `emulateMedia({media: 'print'})` at 794x1123 px.
