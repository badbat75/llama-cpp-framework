---
name: coder-playbook
description: What the coder has learned about implementing and running code safely. Use when changing production code.
---

Lessons only: the rules are in `AGENTS.md`.

- `pkill -f PATTERN` also matches the shell running it, and killed that shell. Use the task's own stop
  control, or a pattern that cannot match itself.
- When a test fails, check whether the test or the code is wrong before changing either.
- A screenshot that comes out blank proves nothing about a GUI. Check that it really shows the window.
- `llama-cpp-config` is a binary crate: `pub` does not silence dead_code, and `clippy --all-targets`
  also builds the non-test bin, so a function only tests call fails `-D warnings` until a caller lands.
- Code comments never cite PLAN.md step numbers: they change on renumbering. Name the feature or caller instead.
