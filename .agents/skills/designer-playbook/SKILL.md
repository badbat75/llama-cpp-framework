---
name: designer-playbook
description: What the designer has learned about the web UI's design and user experience. Use when writing a design brief, reviewing a UI slice or editing fe/DESIGN.md.
---

Lessons only: the rules are in `AGENTS.md` and the role in `.opencode/agents/designer.md`.

- Every text the page shows is a key in `fe/src/strings/en_US.json`, never inline (the project's
  "No user-visible text in the code"). A brief names the key and its English text; numbers, units,
  acronyms and paths stay out of the strings.
- No em-dashes, in briefs, `fe/DESIGN.md` and proposed texts alike (`AGENTS.md`, "Key conventions").
- The web UI is read-only until the design of who is acting through the bff exists (`PLAN.md` step 10,
  "Waiting for the user"): a brief for anything that changes state waits for it.
- The bff's error keys today are `unavailable` (the manager is not reachable), `not-authorized` and
  `failed`: each is a state the page must show (`crates/bff/README.md`).
- `pai-config` (Slint, `crates/config/`) is the reference for what each task looks like today; its
  texts are in `crates/core/strings/en_US.toml`.
