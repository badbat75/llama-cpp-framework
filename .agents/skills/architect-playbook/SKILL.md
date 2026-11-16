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
  a vouched user is `Requester.principal`, carried by `<Method>As` methods (ARCHITECTURE.md, "Acting
  for a signed-in user"). Why method names and not an optional field: the pipe parser ignores unknown
  JSON fields, so an older server would silently run the call as the sender.
- polkit cannot authorize a user who has no process: its subjects are a process, a session or a bus
  name (polkit Authority interface, "Subject"). For a vouched user the manager checks `pai` itself.
- Local version skew between the bff and the manager needs a broken install: the Linux packages pin
  every framework package to `-core` at the same version (`xtask/src/linux/package.rs`), Windows has
  one framework MSI. `Version` matters only across machines.
- Sources already checked for cross-machine designs (2026-11-15): D-Bus non-local TCP is deprecated
  and unprotected (D-Bus spec, "TCP Sockets"); OpenSSH server is in the box only on Windows Server
  2025 (Microsoft Learn, OpenSSH first use); kubeadm's SPKI-hash pinning is the precedent for join
  codes; rustls' `ClientCertVerifier` is the hook for pinning a client.
- A file the Windows manager creates under `%ProgramData%\private-ai` inherits the MSI's protected DACL
  on that root (SYSTEM, Administrators full, Users read: `MachineDataRoot` in
  `installer/private-ai-framework.wxs`), so users cannot pre-create a subfolder to redirect a
  LocalSystem write; a new folder under bare `%ProgramData%` would not have that protection (its ACL
  lets users create subfolders). The MSI removes the root only when empty, so such files outlive an
  uninstall.
- Reviewing a slice: `git show <commit> -- <path>` per commit and `git diff HEAD -- crates/<x>` for the
  working tree; the core's guarded methods are in `service.rs` around `authorize_as`,
  `check_principal`, `record_action`, the pipe parser in `manager/pipe.rs` `Request::parse`.
- Instance lifecycle facts (checked 2026-11-16): on Linux `run-instance` execs llama-server (no parent
  left; code at stop needs `ExecStop=`, which gets `$MAINPID`), on Windows `instance_main`
  (`windows/entry.rs`) stays the parent and kills the child on `Stop`. A value the running server
  uses (port, `--slot-save-path`) is in its argv; `server@<id>.ini` may already be ahead of it (saved,
  not applied). The client restore of snapshots lists `/var/lib/private-ai/...` (0750), unreadable
  to a desktop user on Linux.
- Windows service timeouts: preshutdown default 10 s since build 15063 (SERVICE_PRESHUTDOWN_INFO),
  plain shutdown about 20 s, control handler back within 30 s (HandlerEx remarks).
- Since slice 9b a stop can last minutes (the snapshot save): any caller that stops and then acts on
  the instance (client restore after a restart, the MSIs' `Stop-Service`, the Windows `restart`
  thread) must be checked for a race with a server that is still saving. Windows `restart` is
  asynchronous (`scm.rs`, `RestartGate`); systemd's `RestartUnit` is a queued job.
