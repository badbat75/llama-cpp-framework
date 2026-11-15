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
- `pai_core::client::pipe::PipeClient` works over any `Read + Write` stream, so the manager protocol
  can ride any byte stream (a socket pair in tests, rustls' `StreamOwned` across machines) without a
  new client; the server side's unknown method ends the connection with `BadRequest`
  (`manager::pipe`), so additive protocol changes need a version a client can ask first.
- The manager's `Caller` is a transport identity only (bus name on Linux, token SID on Windows);
  who acts on whose behalf is not modelled yet (step 10 slice 6b part 3). Keep a vouched user a
  separate request field, never folded into `Caller` (ARCHITECTURE.md, part 2).
- Sources already checked for cross-machine designs (2026-11-15): D-Bus non-local TCP is deprecated
  and unprotected (D-Bus spec, "TCP Sockets"); OpenSSH server is in the box only on Windows Server
  2025 (Microsoft Learn, OpenSSH first use); kubeadm's SPKI-hash pinning is the precedent for join
  codes; rustls' `ClientCertVerifier` is the hook for pinning a client.
