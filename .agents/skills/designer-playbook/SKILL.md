---
name: designer-playbook
description: What the designer has learned about the web UI's design and user experience. Use when writing a design brief, reviewing a UI slice or editing fe/DESIGN.md.
---

Lessons only: the rules are in `AGENTS.md` and the role in `.opencode/agents/designer.md`.

- Every text the page shows is a key in `fe/src/strings/en_US.json`, never inline (the project's
  "No user-visible text in the code"). A brief names the key and its English text; numbers, units,
  acronyms and paths stay out of the strings.
- No em-dashes, in briefs, `fe/DESIGN.md` and proposed texts alike (`AGENTS.md`, "Key conventions").
- Who acts through the bff is decided (`PLAN.md` step 10): the user signs in with their OS account and
  the manager applies its rules to them. Until the bff's sign-in exists (step 10 slice 6), the real
  page is read-only; briefs for editing are built against `fe/briefs/api-contract.md` and its mocks.
- Errors follow the contract: `{key, args}` with a strings key (`fe/briefs/api-contract.md`); today's
  bff still returns `unavailable`, `not-authorized` and `failed` until slice 6 moves them.
- `pai-config` (Slint, `crates/config/`) is the reference for what each task looks like today; its
  texts are in `crates/core/strings/en_US.toml`.
