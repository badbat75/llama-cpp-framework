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
- The `AGENTS.md` and README lines are paragraphs thousands of characters long: grep with a short
  context window (a small Python `re` script) instead of printing whole lines, and replace with an
  exact, count-checked substring. Re-read a file right before editing it: the coordinator edits
  `AGENTS.md` and `PLAN.md` while you work.
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
- After modules move between crates, the crate docs name files bare (`presets.rs`) or crate-relative
  (`src\presets.rs`): fix the crate-relative ones, say once at the top which crate holds the bare
  ones, and recheck claims tied to the old crate (`pai-config` not GNU-checked, `system_connect`'s
  Windows arm, which `build.rs` emits what, the test crate a command needs: `cargo test -p <new>`).
- A build script that reads `build/<os>/...` of the TARGET OS (pai-app's clone lookup) fails the GNU
  check on Linux: the docs give the override with an absolute path (the script runs in its crate dir).
- After the command line moved to `pai` (package `pai-cli`), grep `pai-config (server|preset|control|bench|config)`
  in every `.md` and in skill `description:` lines; `pai-config` stays where it is the GUI, the
  executable a script runs (`--version`, `sdk-devices`) or what xtask builds and packages. A command run
  on a GUI-only `pai-config.exe` gets exit 2; the MSI's stopper tries `pai.exe` first for that reason.
- When a package is added, grep the counts in words (`five`, `three`, `all five`) as well as the file
  lists: README, `AGENTS.md`, `docs/build-pipeline.md` (intro and the OS table), `build-packaging.md`
  (relations table, `.deb`/`.rpm` scan bullets) and `build-macos.md` each repeat them. A package with
  no 2.4 predecessor is an exception in every "each replaces" sentence. Sizes measured on the old split
  are not the new packages': say so instead of carrying them over.
- When a crate is added, its name goes in: `AGENTS.md` Commands (crate list, Windows GNU list), release
  step 1's version list, the Map, `.opencode/agents/coordinator.md` (`cargo clean -p`), `coder.md` (GNU
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
  (intro, route table, "The web UI"), the `AGENTS.md` overview paragraph and its bff Map bullet, and
  the root `README.md` intro. A route that reaches the manager can be checked by hand with no manager:
  run the debug `pai-bff` with `PRIVATE_AI_DATA_ROOT` (and `<root>/system-config/bff.ini` for a port);
  it answers 503 `unavailable`.
- When a package is split or a file changes package, recheck what moved with the maintainer scripts:
  a "the scripts restart the services" sentence holds only for the package that has them. Each
  package's scripts are in `system_files.rs` (`deb_scripts`, `rpm_scriptlets`, one function per
  package), its rpm `Requires(<tag>)` in `package.rs`. A unit's package is named in five places:
  `build-packaging.md` (Holds table, paths table, system files heading and row), `AGENTS.md`'s Linux
  bullet, the README package table, and the manager row of `crates/config/README.md`'s source map.
- A setting that is the service manager's own state (start at login, start at boot) is described
  once in full in "Through the manager" of `crates/config/README.md`; `AGENTS.md`, the README and
  `installer/AGENTS.md` (its test plan) carry one line each. `components::file_stem` and
  `of()` are shared with macOS: a Linux split renames and resplits the `.pkg` set too, even when the
  macOS slice is "later"; say so on `build-macos.md` rather than leave its table looking current.
- When a package starts shipping defaults copied "only when missing", recheck every flow keyed on a
  file being absent: `migration::import_offered` skips a file the manager already has, so the import
  of a 2.4 `server.ini`/`presets.ini` stops being offered on a packaged Linux install. The instance
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
  tables), the README command table, `AGENTS.md`'s Windows `xtask build` bullet and Linux bullet,
  `build-compiling.md` (node/npm line, web UI section, Linux "built last"), `build-packaging.md`
  (MSI and `.deb`/`.rpm` intros), `build-macos.md` ("Package."), and the `scripts/0N-*` usage headers
  (not ours: report them). `xtask install`'s "predates the build" warning keys on the build tree's
  `llama-server` mtime only (`linux/install.rs`): recheck it when what a build updates changes.
