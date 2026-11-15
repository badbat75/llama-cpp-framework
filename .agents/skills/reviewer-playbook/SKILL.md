---
name: reviewer-playbook
description: What the reviewer has learned about reviewing the coder's changes in this project. Use when reviewing a diff before it is committed.
---

# Reviewer playbook

- Defects found in past reviews (2026-11-15), worth checking again in similar code:
  - FFI: struct layouts and constants against the C header (Linux-PAM `_pam_types.h`), who frees
    what on every error path, handles always closed.
  - Packaging: the packages install what the code names (a PAM service file, a polkit rule, a group):
    grep `xtask/` for the path the code uses.
  - Windows account names: one account has several spellings (`user`, `.\user`, `HOST\user`, a UPN);
    an API that accepts one may not accept another.
  - `.deb` and `.rpm` are staged in one tree: a file written for one format must not reach the other.
