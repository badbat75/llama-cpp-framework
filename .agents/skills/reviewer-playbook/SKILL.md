---
name: reviewer-playbook
description: How the reviewer reviews the coder's changes in this project, and the pitfalls met so far. Use when reviewing a diff before it is committed.
---

Rules are in `AGENTS.md`. Format of this file: `AGENTS.md`, under "Skills".

## How to

- Check a change's FFI against the C header it mirrors: struct layouts and constants (Linux-PAM's
  `_pam_types.h`), who frees what on every error path, and that every handle is closed on every path.
- Check that the packages install what the code names (a PAM service file, a polkit rule, a group): grep
  `xtask/` for the path the code uses.

## Pitfalls

- Windows account names: one account has several spellings (`user`, `.\user`, `HOST\user`, a UPN); an
  API that accepts one may not accept another.
- `.deb` and `.rpm` are staged in one tree: a file written for one format must not reach the other.
