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
  Grep all of them when one changes.
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
  executable a script runs (`--version`, `sdk-devices`) or what xtask builds and packages. Scripts that
  still call a command on `pai-config.exe` (`installer/msi-stop-processes.ps1`) now get exit 2: say so
  where the docs describe them.
