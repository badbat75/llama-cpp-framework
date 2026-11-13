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
