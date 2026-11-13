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
