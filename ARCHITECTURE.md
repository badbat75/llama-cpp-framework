# Architecture

The map of the framework's components, how they talk, who may do what, and what data each one owns;
then the design records. Kept by the `architect` agent (`.opencode/agents/architect.md`). It points to
the guides that hold the details rather than copying them: when this page and a guide disagree, the
guide and the code win, and this page is fixed.

Part 1 is the baseline, what exists on 2026-11-15. Part 2 is the design of `PLAN.md` step 14 (R13, a
super-manager), proposed and waiting for the user's acceptance.

## Part 1: the baseline

### Components

```text
  browser ──HTTP (loopback)──> pai-bff ─┐                     ┌─> llama.cpp@<id>  (Linux unit)
                                (fe/)   │  D-Bus + polkit     │   llama.cpp_<id>  (Windows service)
  pai-config (GUI) ─────────────────────┼─────────────────> pai-manager ──systemd / SCM──┘
  pai (CLI) ────────────────────────────┘  named pipe + ACL   (root / LocalSystem)
                                                                  │ reads, writes
                                                                  v
                                                      the engine's instance files
```

| Component | What it is | Runs as | Guide |
| --- | --- | --- | --- |
| `pai-manager` (`crates/manager`) | The `private-ai-manager` service: lists, starts, stops the engine instances, reads and writes their files, runs every content check, decides who may do what. Keeps no state of its own: the service manager's state is the truth | root (Linux), LocalSystem (Windows); not on macOS | [crates/config/README.md](crates/config/README.md#through-the-manager-linux-windows) "Through the manager" |
| Engine instances | One OS service per instance id: `llama.cpp@<id>.service` (template unit shipped by the `llama.cpp` package), `llama.cpp_<id>` on Windows (created by the manager). Each runs `pai-manager run-instance <id>`, which reads `server@<id>.ini` and execs llama-server | `private-ai` (Linux, in `render`, `video`), `NT SERVICE\llama.cpp_<id>` (Windows) | `PLAN.md` step 1, [installer/AGENTS.md](installer/AGENTS.md) |
| `pai-bff` (`crates/bff`) + `fe/` | The web service: the API and the embedded React web UI. Loopback only until authentication and TLS exist | `private-ai-bff` (static system user, sandboxed), `NT SERVICE\private-ai-bff` | [crates/bff/README.md](crates/bff/README.md), `PLAN.md` step 10 |
| `pai-config` (`crates/config`) | The Slint GUI; a thin client of the manager on Linux and Windows, spawns llama-server itself on macOS | the desktop user | [crates/config/AGENTS.md](crates/config/AGENTS.md) |
| `pai` (`crates/cli`) | The same operations headless | the user | [crates/config/README.md](crates/config/README.md) |
| `pai-core`, `pai-app` | Shared libraries: paths, INI, checks (`checks.rs`), the manager client (`client.rs`); the configurator's logic without a GUI | (linked) | [crates/config/README.md](crates/config/README.md) |

Packages (R12, `PLAN.md` step 11): the engine is its own package at its own version (`llama.cpp`
with the instance unit, the runtimes `llama.cpp-rocm|cuda|vulkan`; on Windows llama.cpp's own MSI,
which owns the instance services' lifecycle on repair and uninstall). The framework's
`private-ai-framework-core` (manager, its unit, D-Bus and polkit policies, sysusers), `-bff`, `-cli`,
`-config`; on Windows one framework MSI. macOS: split `.pkg`, no manager, no bff. Details:
[docs/build-packaging.md](docs/build-packaging.md), [installer/AGENTS.md](installer/AGENTS.md), the
Linux system files in `xtask/src/linux/system_files.rs`.

### Interfaces

- **Linux: D-Bus `org.privateai.Manager1`** on the system bus. Only root owns the name, everyone may
  send to it; polkit decides in each call (`dbus_policy()`, `polkit_policy()` in
  `xtask/src/linux/system_files.rs`). The caller is its unique bus name.
- **Windows: the named pipe `\\.\pipe\private-ai-manager`**, remote clients rejected, network logons
  denied by its SDDL; SYSTEM and Administrators full, interactive users and the bff's service SID
  read and write (`PIPE_SDDL` in `crates/manager/src/manager/windows.rs`). The caller is the SID of
  the client's token.
- **The manager protocol**: one request, one reply; on the pipe a u32 little-endian length and a JSON
  body of at most 1 MiB, the D-Bus method and error names (`crates/core/src/client/pipe.rs` header,
  the server in `crates/manager/src/manager/pipe.rs`). Methods: `ListInstances`, `Status`, `Start`,
  `Stop`, `Restart`, `ReadFile`, `WriteFile`, `BootEnabled`, `SetBootEnabled`, `Authenticate`. The client works over any `Read + Write` stream.
- **The bff's HTTP API**: the routes and error shape `{key, args}` in
  [crates/bff/README.md](crates/bff/README.md); the contract the web UI is built against in
  `fe/briefs/api-contract.md`.
- **The instance files**: `server@<id>.ini`, `presets.ini`, `presets.ini.disabled` in the engine's
  configuration directory, written only by the manager; read by `run-instance` and by llama-server
  (`--models-preset`).

### Identities and trust boundaries

| Identity | Holds | Trusted for |
| --- | --- | --- |
| The manager (root, LocalSystem) | Every right on the machine | Everything; the one authority over the instance files |
| `private-ai` / `NT SERVICE\llama.cpp_<id>` | Reads the instance files (group `private-ai`, mode 0640 for `server@<id>.ini`), its models and state | Running one engine; reaches no manager call that changes anything |
| `private-ai-bff` / `NT SERVICE\private-ai-bff` | No file but `bff.ini`; the manager's unauthenticated reads | Today: reads, and asking the manager to check a password (`Authenticate`, which answers only this account; the bff does not call it yet). Decided: vouching for the user it signed in |
| A desktop user | Their own tree (`settings.ini`, `bench.db`) | `control` (start, stop, restart) in the active session without a password (Linux), as an interactive user (Windows) |
| An administrator | | `configure` (write a file, start at boot, read the API key) |

**Who is an administrator** (user, 2026-11-15, `PLAN.md` step 10 slice 6): a member of the group `pai`,
created empty by the packages, on every client; on Linux `configure` asks the member's own password and
keeps it (polkit `auth_self_keep`, which a `.rules` file can grant by `subject.isInGroup("pai")`,
[polkit(8)](https://polkit.pages.freedesktop.org/polkit/polkit.8.html)); on Windows from the token's
membership. **Not yet in the packages:** the shipped polkit actions still say `auth_admin_keep` for
`configure` and Windows still grants it to Administrators. The web UI is for administrators only: a
sign-in succeeds only for a `pai` member.

Boundaries a call crosses: the browser to the bff (HTTP, a session cookie; loopback only); the bff,
`pai-config` and `pai` to the manager (the bus or the pipe, the caller identified by the OS, never by
anything it sends); the manager to systemd or the SCM; the instance to its files and GPUs. Nothing
crosses the machine today.

### Data and where it lives

| Data | Owner | Where (Linux / Windows) |
| --- | --- | --- |
| Instance files | the manager | `/etc/llama.cpp/` / `%ProgramData%\llama.cpp\config\` |
| The bff's config | the administrator | `/etc/private-ai/bff.ini` / `%ProgramData%\private-ai\config\bff.ini` |
| Instance state (run, boot) | systemd, the SCM | the service manager |
| Instance logs | the instance | the journal / `%ProgramData%\llama.cpp\logs\` |
| Models, slot snapshots | the instance's account | `/var/lib/...` per the shipped defaults / `%ProgramData%\llama.cpp\models`, `state` |
| `settings.ini`, `bench.db` | the desktop user | `~/.config/private-ai/`, `~/.local/share/private-ai/` / `%LOCALAPPDATA%\private-ai\` |
| Web sessions | the bff, in memory | lost at restart |
| Web users' shared state (preferences, `bench.db`, tokens) | decided machine-wide, place not planned (`PLAN.md` step 10) | |

The exact paths and modes are in `AGENTS.md` "Key conventions" and "Through the manager".

### Where the web sign-in stands (2026-11-15)

Shipped in the manager (04a7f9e, `PLAN.md` step 10 slice 6b part 2): `Authenticate(user, password) ->
(ok, can_configure)`, answered only to the bff's account (`Authority::is_bff`: the bus name's uid on
Linux, the service SID on Windows), the password checked through PAM service `private-ai`
(`crates/manager/src/manager/accounts.rs`) or `LogonUser` (`windows/logon.rs`), `can_configure` from
membership of `pai` (`PasswordChecker` in `service.rs`), on both transports. Next: part 2b, the
packaging (`/etc/pam.d/private-ai`, the group `pai`, the polkit rule); then 2c, the bff calls it.

## Part 2: the super-manager (`PLAN.md` step 14, R13)

The user's words (2026-11-15): "Ci potrebbe essere un componente super partes tipo un supermanager,
installato su un server differente che può governare differenti manager. Se il bff si connette a questo
gli viene offerta anche la gestione dei nodi." Status: **accepted** by the user on 2026-11-15, who
answered the four questions at the end with the recommended option each.

### Decision summary

1. **A separate service, `pai-supermanager`**, on its own server, unprivileged, holding the node
   registry and the one key the nodes trust. It runs no engine and checks no content: each node's
   manager stays the authority over its machine.
2. **Nodes are managers.** The super-manager speaks to each node's `pai-manager` with **the existing
   manager protocol over TLS 1.3 with mutual authentication**, each side pinning the other's
   self-signed certificate. No new protocol, no CA, no D-Bus off the machine.
3. **A node joins by a one-time code** its own administrator creates, carrying the node's key
   fingerprint and a secret: both sides authenticated from the first byte.
4. **Rights are delegated, not mapped**: joining a node grants the super-manager `control` and
   `configure` on it; the super-manager admits only its local bff, which vouches for a signed-in `pai`
   member of the super-manager's machine. Each forwarded call names that person for the node's log.
   Per-node rights and a central identity wait.
5. **The bff gains a hub mode** (`bff.ini`), where it talks to the super-manager instead of the local
   manager and the API grows a node prefix; the local mode stays as it is.
6. **Nodes never depend on the super-manager**: their instances, boot settings and local clients work
   with it down or gone.

### Role and boundaries

| | Manager (on each node) | Super-manager | bff |
| --- | --- | --- | --- |
| Owns | The instances, their files, the checks, local authorization | The node registry (name, address, pinned fingerprint), its own key, the join, fan-out with per-node timeouts | Web sessions, the HTTP API, the web UI |
| Never | Knows the super-manager's users; accepts a password from off the machine | Caches a node's files as truth; runs checks; starts engines; sees passwords | Holds the node key; reaches a node directly |
| Talks to | systemd / SCM; local callers; the enrolled super-manager over TLS | Nodes over TLS; its local bff over a local socket | The local manager (local mode) or the super-manager (hub mode) |

The super-manager is separate from the bff (rather than a "hub mode" holding the registry inside the
bff) for one security reason and one operational one: the browser-facing process never holds the key
every node trusts, so a compromised bff can act only while it is compromised and only as a signed-in
session could, and cannot carry the key away; and the super-manager can be restarted, upgraded or
backed up without the web service. Cost: a third local service and a small local interface.

### Transport between machines

| Option | Cost | Risk | Verdict |
| --- | --- | --- | --- |
| **A. The manager protocol (u32 + JSON frames) over TLS 1.3, mutual, pinned** | rustls in the manager and the super-manager; a key pair per side (rcgen, [docs.rs/rcgen](https://docs.rs/rcgen/latest/rcgen/)); `PipeClient` reused as is over rustls' `StreamOwned`, which is `Read + Write` ([docs.rs](https://docs.rs/rustls/latest/rustls/struct.StreamOwned.html)); `manager::pipe`'s dispatch reused on the node | A network listener in the manager (below); pinning code is ours | **Recommended** |
| B. HTTPS + JSON (axum with TLS, mutual) | An HTTP mapping of every method, a second error shape | Same listener question, more code | No gain over A for a machine-to-machine call |
| C. gRPC (tonic, protobuf) | A schema, a code generator, a second protocol beside the pipe's | gRPC supports client certificates ([grpc.io auth guide](https://grpc.io/docs/guides/auth/)) | Worth it only for streaming (live logs to the web); later, if then |
| D. SSH with a forced command relaying frames to the local manager | No listener of ours; keys in `authorized_keys` with `restrict,command=` ([sshd(8)](https://man.openbsd.org/sshd#AUTHORIZED_KEYS_FILE_FORMAT)) | sshd required on every node: in the box only on Windows Server 2025, an optional feature on Windows 10/11 and Server 2019/2022 ([Microsoft Learn](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse)); enrolling writes another account's `authorized_keys`; the relay account becomes a new trusted caller | A good fallback if a network listener in the manager is refused |
| E. D-Bus over TCP | None | The specification: no integrity or confidentiality, non-local TCP deprecated ([D-Bus spec](https://dbus.freedesktop.org/doc/dbus-specification.html), "TCP Sockets", revision 0.33) | Rejected |

**Who listens.** The super-manager dials the nodes; each node listens. Option A puts the listener in
the manager (root, LocalSystem), the smallest design, since the manager already serves the protocol
and a node then needs only `-core` and the engine (a headless GPU box needs no bff). The alternatives
are a relay in the node's bff (which is sandboxed and unprivileged, but must then be installed on every
node and be trusted to grant `configure` on the super-manager's behalf, the same power with more code)
or a new node agent (another account, unit, package and trusted caller). The risk is bounded so:

- **Off by default.** No port is open until a node's administrator enables it (`pai node enable`,
  `configure`); `pai node leave` closes it and forgets the peer.
- **Nothing is parsed before the handshake proves the peer.** rustls' client-certificate hook
  (`ClientCertVerifier`, [docs.rs](https://docs.rs/rustls/latest/rustls/server/danger/trait.ClientCertVerifier.html))
  accepts only the pinned SPKI hash, the handshake signature still verified; a wrong peer is closed by
  TLS. The only exception is the join window (below): at most 15 minutes, the `Join` method alone,
  attempts limited.
- **TLS 1.3 only, ALPN `pai-manager/1`**: a client offering no matching protocol gets the fatal
  `no_application_protocol` alert ([RFC 7301](https://www.rfc-editor.org/rfc/rfc7301)), which also
  carries the protocol's major version.
- **Bounded**: a few connections at once, the existing 1 MiB frame, an idle timeout; the listener's
  threads never hold the manager's write lock while waiting on the network.
- Port: a proposal, 8592 (the bff is 8590), set in the node's config, opened in the firewall by the
  administrator (the packages open nothing).

### Identity and joining

Each side has one long-lived key pair and a self-signed certificate, generated on first need; the
identity is the SHA-256 of the certificate's SubjectPublicKeyInfo, as kubeadm pins its CA
([kubeadm join](https://kubernetes.io/docs/reference/setup-tools/kubeadm/kubeadm-join/),
`--discovery-token-ca-cert-hash`). Certificates are pinned, so their expiry and the clocks do not
matter. The node's key lives beside its config, readable by the manager only (`/etc/private-ai/node/`
mode 0600 root; `%ProgramData%\private-ai\node\` SYSTEM and Administrators); the super-manager's in its
own state directory.

The join, the kubeadm pattern turned around because the super-manager dials:

1. On the node, a `pai` member runs `pai node enable` (needs `configure`). The manager creates its key
   if missing, opens the listener in join mode and prints a **join code**: the node's fingerprint
   (128 bits are enough to pin) and a random 128-bit secret, valid 15 minutes, single use.
2. In the super-manager's web UI (or its CLI, later), an administrator adds the node: a name, the
   address, the code.
3. The super-manager connects with its own certificate and checks the node's against the code's
   fingerprint before sending anything: no trust on first use. It sends `Join{secret}`.
4. The node compares the secret in constant time, pins the presented client certificate as its
   super-manager, ends join mode. From then on only that certificate gets past the handshake.

A node has at most one super-manager (one pin): simpler rights, and a second governor would be a
question for the user, not a default. **Removal** is forgetting the pin on either side. **A node
reinstalled** has a new key: the super-manager refuses it as "identity changed" (like SSH's changed
host key) until it is joined again. **A super-manager key lost or stolen**: every node is joined again
(`pai node leave` then a new code); a CA with revocation is the answer only once fleets are large.

### Who may act on which node

| Option | Cost | Verdict |
| --- | --- | --- |
| **Delegation**: joining grants the super-manager `control` and `configure` on the node; the super-manager decides who among its users may act | Small: one new caller kind in the node's Authority | **Recommended now** |
| Per-node rights in the super-manager (user or group to node list) | A rights model and its UI | Later, on top of delegation, without touching nodes |
| The same accounts on every machine (a directory: LDAP, AD), the node checking the named user's local `pai` membership | Needs a shared directory; the node trusts the super-manager's word about who it is anyway | Only if the user's machines share one |
| A central identity provider (OIDC tokens verified by each node) | Token issuance, keys, expiry, an IdP to run | Later, if users come from outside |

How a web session maps to a node: the user signs in on the super-manager machine's bff, checked by
**that machine's** manager (`Authenticate`, so that machine has `-core`); only a `pai` member gets a
session. The bff sends each call to the super-manager with the session's principal
(`alice@<super-manager name>`); the super-manager admits only its bff (by peer credentials: `SO_PEERCRED`
on a Unix socket, [unix(7)](https://man7.org/linux/man-pages/man7/unix.7.html)) and forwards the call to
the node with the same principal as a field. The node's manager authorizes the enrolled peer and logs
the principal with every change. No password leaves the machine it was typed for.

What each side can do if compromised: the bff, act as any administrator while compromised (as today on
one machine); the super-manager, act on every joined node as an administrator until each node forgets
it; the network, nothing (mutual TLS); a node, nothing on the other nodes (it holds no credential to
them) and it cannot impersonate the super-manager to other nodes.

### The bff and the web UI

- `bff.ini` `[Bff] SuperManager = true` (default false) selects hub mode; explicit, so a super-manager
  that is down shows as down instead of the UI quietly dropping back to the local machine.
- `GET /api/version` gains `"nodes": true|false`, so the web UI knows the mode without a probe.
- Hub mode adds `GET /api/nodes` (`[{name, address, state: reachable|unreachable|identity-changed|incompatible, version}]`),
  `POST /api/nodes` (name, address, code) and `DELETE /api/nodes/{node}`, and serves every
  instance and preset route under `/api/nodes/{node}/...`, same bodies and errors, plus the keys
  `supermanager-unavailable` and `node-unavailable`. Local mode keeps today's routes; the web UI picks
  its base path from `nodes`. Everything is per node (instances, presets, models), so the node is only a
  prefix.
- The web UI: a Nodes list on the Overview and node-scoped pages; the designer's brief first, as for
  every slice users see.
- Practical prerequisite: a hub is usually reached from another machine, so the bff's LAN access (TLS
  for browsers, `Secure` and `__Host-` on the cookie: `PLAN.md` step 10 slice 6b leftovers) comes before
  the hub is useful beyond an SSH tunnel to its loopback.

### Failure modes

| Failure | What happens |
| --- | --- |
| A node down or unreachable | The super-manager's per-node timeout (shorter than the bff's) answers `node-unavailable` for it; the others answer as usual (fan-out in parallel) |
| A write whose reply is lost | The node's write is atomic (rename); the UI says the outcome is unknown and reloads; the revision check (409) makes a retry safe |
| The super-manager down | Nodes run on: systemd and the SCM own the instances and the boot settings, local `pai-config`, `pai` and a local bff work; the hub's web UI shows `supermanager-unavailable` |
| The super-manager gone for good | Each node's administrator runs `pai node leave`; nothing else on the node changes |
| A node's key changed | Refused as `identity-changed` until joined again |
| Version skew | Below |

### Packaging, upgrades and version skew

- **A separate package**: `private-ai-framework-supermanager` (Linux) with `pai-supermanager` in
  `/usr/libexec/private-ai`, its unit, its sysusers entry (`private-ai-super`), its socket in its
  runtime directory, group-readable by `private-ai-bff` only; requires `-core` (the sign-in) and
  usually sits beside `-bff`. Its own machine is a node only if joined like any other, over
  `127.0.0.1`, one code path. Windows: later (a feature of the framework's MSI, off by default, or not
  at all; a user question).
- **Nodes need nothing new**: the listener is part of `pai-manager` in `-core`, off until enabled.
- **Skew**: the ALPN name carries the protocol's major version (`pai-manager/1`); within a major,
  changes are additive (new methods, new optional fields), and a new `Version` method (no authorization)
  returns the node's framework version and protocol minor, so the super-manager shows it and avoids
  methods an older node lacks (today an unknown method ends the connection with `BadRequest`,
  `manager::pipe`). The super-manager supports the current and the previous major; upgrade it first.
  The join state lives in the node's config directory and survives package upgrades.

### Slices

Each test first; the coordinator may split further. Linux first; the Windows halves of 1 to 4 are
type-checked like the rest of the Windows code.

0. **Before step 10 slice 6b part 3 hardens** (no new behaviour): the acting principal is a field of
   the request beside the transport caller, not folded into `Caller` (below). Done when: a test pins
   that a call from the bff carrying a principal reaches the Authority as (caller, principal, action).
1. **Node identity.** `pai node show|enable` creates the key and prints the fingerprint. Done when:
   the fingerprint is stable across restarts, the key file is 0600 root, a non-`configure` caller is
   refused.
2. **The listener.** TLS 1.3, ALPN, pinned client certificate, `manager::pipe` dispatch with a new
   caller kind. Done when, over loopback with test keys: a pinned client lists instances; an unpinned
   one is closed before any frame; a wrong ALPN gets the alert; the bound on connections holds.
3. **The join.** Code, window, `Join`, pin, `leave`. Done when: a right code pins once; a wrong,
   expired or reused code is refused and counted; after the window no unpinned peer gets past the
   handshake; `leave` closes the port.
4. **Delegated rights and the log.** Done when: the pinned peer may `configure` (including the API
   key in reads), and each change it makes is logged with its principal.
5. **The super-manager.** Crate and binary, its key, the registry, `add` against an in-process node.
   Done when: add with a code pins both sides; a changed node key reads `identity-changed`; a
   stopped node reads `unreachable` within the timeout while another answers.
6. **Its local interface.** The protocol with `node` and principal on a Unix socket, the bff's uid
   only. Done when: another uid is refused; calls reach the right node; `ListNodes` fans out.
7. **The bff's hub mode.** `SuperManager`, `nodes` in `/api/version`, `/api/nodes`, the node-prefixed
   routes over a node seam. Done when: tests against a fake super-manager pass and every local-mode
   test still passes unchanged.
8. **The web UI.** Designer's brief, then nodes on the Overview and node-scoped pages.
9. **Packaging.** The Linux package; checked by `xtask package` and its contents; installing is the
   user's run, then the end-to-end check on two machines.

What can wait: per-node rights, a central identity, nodes that dial out (NAT), a CA, gRPC streaming
for live logs, the super-manager on Windows, a CLI for the super-manager, the gateway across nodes
(R11's routing across instances on several machines).

### What the current code should change before it hardens

- **The manager's caller model.** `Caller(String)` is a transport identity (bus name or SID) and
  `Authority` answers per action. Step 10 slice 6b part 3 will add "the bff vouches for user X"; carry
  that user as an explicit, optional principal on the request (a pipe field, a D-Bus argument or an
  options dictionary), with the caller kind (local, bff, enrolled peer) decided by the transport, so the
  peer of slice 2 is one more kind and not a third way to name a user.
- **The bff's identity.** `session::Identity { user, can_configure }`: `can_configure` goes with the
  2026-11-15 decision; let the user be a principal with its origin (the machine that checked the
  password), so a node's log can tell `alice@hub` from a local `alice`.
- **The bff's upstream.** Today one `ManagerClient` per request with one 5 s `MANAGER_TIMEOUT`. Put
  the upstream behind a seam that returns a client for a node (one node, "local", in local mode), and
  budget timeouts per hop (node < super-manager < bff < the web UI's), which slice 9 of step 10
  already needs for long stops.
- **The revision** (`PLAN.md` step 10 slice 8): key the hash before it crosses a network, as already
  planned.
- **A protocol version.** Add the `Version` method to the manager protocol while the only clients are
  in this repository; it costs one method now and saves guessing later.

### Questions only the user can answer

1. **Reachability.** Can the super-manager always open a connection to each node (same LAN or VPN)?
   The design assumes yes; nodes that must dial out (behind NAT) need a different, larger transport.
2. **Rights.** Is "every administrator of the super-manager (member of `pai` on its server) administers
   every joined node, after the node's own administrator approved the join" enough for now, or must
   rights differ per node from the start?
3. **The listener.** Is a network port opened by the manager itself (off until a node joins, mutual
   TLS before anything is read) acceptable, or must the manager never listen on the network (then SSH,
   option D, or a relay in the node's bff)?
4. **Platforms.** The super-manager on Linux only at first, with Linux and Windows nodes?

Answers (user, 2026-11-15): 1. yes, the nodes are reachable (same LAN or VPN); 2. enough for now;
3. the manager's listener is acceptable; 4. Linux only at first, with Linux and Windows nodes.
