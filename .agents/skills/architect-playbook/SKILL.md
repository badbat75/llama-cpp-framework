---
name: architect-playbook
description: What the architect has learned about this project's components, interfaces, trust boundaries and deployment. Use when designing or reviewing a change to them, or editing ARCHITECTURE.md.
---

# Architect playbook

- The facts the first designs rest on, each checked in the code or a primary source on 2026-11-15:
  - `pai-bff` runs as the unprivileged system user `private-ai-bff`; pam_unix's helper `unix_chkpwd`
    verifies only the calling user's own password (unix_chkpwd(8)), so password checks for other
    accounts go through the manager (root on Linux, LocalSystem on Windows).
  - llama-server's router refuses an unknown key in `--models-preset` (llama.cpp v1.0.0
    `tools/server/server-models.cpp:415`, `common/preset.cpp:325`): a preset's engine is the file that
    holds it, never a key.
  - An administrator is a member of the group `pai` on every client (user, 2026-11-15); `configure`
    asks the member's own password on Linux (polkit `auth_self_keep`).
- PLAN.md's step 10 holds the bff's decisions with dates; read it before designing anything the web
  UI touches.
