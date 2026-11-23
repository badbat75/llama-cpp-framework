---
name: architect-playbook
description: How the architect designs this project's components, interfaces, trust boundaries and deployment, and the pitfalls met so far. Use when designing or reviewing a change to them, or editing ARCHITECTURE.md.
---

Rules are in `AGENTS.md`. Format of this file: `AGENTS.md`, under "Skills".

## How to

### Before designing, and when reviewing a slice

- Read `PLAN.md` step 10: it holds the bff's decisions with their dates. Read it before designing
  anything the web UI touches.
- Before inventing a framework key for `models.ini`, grep llama.cpp's `common/arg.cpp` for a native
  option: v1.0.0 has `--tags` (preset key `tags`, listed in the router's `/models`) and `--alias`.
- Write the design in `ARCHITECTURE.md`, then update the drawn views ("Drawn views").
- Read each commit with `git show <commit> -- <path>`, and the working tree with
  `git diff HEAD -- crates/<x>`. The manager's guarded methods sit in `service.rs` around `authorize_as`,
  `check_principal`, `record_action`; the pipe parser is `manager/pipe.rs` `Request::parse`.
- Check every edge-case clause of the design text against the code, not only the "done when" list: a
  clause in the design and in no done-when item is the one that goes unbuilt.
- Check every new reader of an instance's server file goes through `paths::instance_server_profile`,
  and that its test names a server profile different from the instance id.
- For a render-based check, look for an e2e that calls `set_model_profile`, not only `write_file`.

### The manager and its protocol

- `pai_core::client::pipe::PipeClient` works over any `Read + Write` stream, so the protocol can ride
  any byte stream (a socket pair in tests, rustls' `StreamOwned` across machines) without a new
  client. The server's unknown method answers `BadRequest` and ends the connection (`manager::pipe`),
  so an additive protocol change needs a version a client can ask first.
- The manager's `Caller` is a transport identity only (bus name on Linux, token SID on Windows); a
  vouched user is `Requester.principal`, carried by `<Method>As` methods (ARCHITECTURE.md, "Acting for
  a signed-in user"). The method name carries it, not an optional field: the pipe parser ignores
  unknown JSON fields, so an older server would silently run the call as the sender.
- Local version skew between the bff and the manager needs a broken install: the Linux packages pin
  every framework package to `-core` at the same version (`xtask/src/linux/package.rs`) and Windows
  has one framework MSI. `Version` matters only across machines.
- A new refusal is cheapest as a keyed `checks::Refusal` (`ManagerError::Invalid`): it already crosses
  D-Bus and the pipe and `ClientError` decodes it. A new `ManagerError` variant needs an arm in
  `service.rs`, `dbus.rs`, `pipe.rs` and `client.rs`. Add the key to `checks.rs`'s list,
  `Refusal::text` and both strings files. One rule gets one key (`server-bad-tags` serves the profile's
  and the instance's tags); a `*-malformed` key names the route's body (`server-malformed`).

- The manager serializes every write with its `writing` mutex (`service.rs` `write_file`): a
  compare-and-swap belongs there, not in the bff.
- `WriteFileIf` refuses a missing file (`Changed`): a design that may create a file needs the plain
  write for that case.
- A stop can last minutes (the snapshot save): any caller that stops and then acts on the instance
  (client restore after a restart, the MSIs' `Stop-Service`, the Windows `restart` thread) needs a
  check for a race with a server that is still saving. Windows `restart` is asynchronous (`scm.rs`,
  `RestartGate`); systemd's `RestartUnit` is a queued job.
- Windows `ScmService::start` and `set_boot_enabled` create a missing service, so a remove must cancel
  the restart gate first; systemd's template needs no create.
- A value the web shell needs on every page (the host name) must come from a read that asks the manager
  nothing (`/api/version`): `/api/node` waits on `NodeInfoAs` (10 s cold).
- A polled read may ask the manager: peek the session and use its identity as the `As` principal
  (`control.rs` `boot_read` precedent).
- Keyed hashes: `siphasher` (SipHash-2-4) and `sha2` are in `Cargo.lock`, `hmac` is not; FNV is not a
  MAC, even with a secret basis (draft-eastlake-fnv sections 2.2 and 6).

### The manager's checks

- A value rule judges what llama.cpp's preset reader yields (`common/preset.cpp`, a PEG grammar): spaces
  after `=` and before a `;`/`#`/newline dropped, the value cut at the first `;` or `#`; that is
  `checks.rs` `RawSection::get`. Never judge the raw line: a hand-edit then blocks every save.
- `RawSection.keys` holds the RAW value (inline comment included); only `comment_markers` reads it, every
  other rule reads `s.get` (cut). Any raw-line rule must exempt lines the current file already holds in
  the same section, or a byte-for-byte edit (web save, switch on) of a hand-commented line is refused.
- `models_file` sees only `models.ini` as `old` unless given the disabled file: a switch on checks the
  moved section whole.
- The checks run only on sections whose lines changed, so a section-scoped revision in the bff loses no
  check; the switch's two writes rely on `models_disabled_file` / `model_profile_ids_free` letting an
  unchanged twin through, destination first.
- A render-based check has two write paths: the pool files (`check_contents`, record on disk) and
  `set_model_profile` (new record `text`, pool on disk). `render_of` always reads the record on disk,
  so the second path must render from `text` (as `check_model_files_of` does) or it judges nothing.
- Any manager method that BUILDS a line from caller text must judge control characters itself, or a
  newline injects a section: today only the bff does (`model_profiles.rs` `parse_body`).
- The section rule requires a non-blank `model` (`checks::model_profile_section`), so an `hf-repo`-only
  section is refused unless the rule is loosened; `FETCH_KEYS` is in `pai_core::checks`.
- A key list that is a permission rule must be pinned against code, not against a brief: the model
  profile keys the configurator writes are reachable as `render_section(&full_preset())` in `pai-app`'s
  tests (`full_preset` has no `..Default`, so it sets every field); `pai-bff` cannot link `pai-app`, so a
  shared list goes in `pai-core` `checks.rs`. A brief's field tables name the keys, its criteria only the
  kinds of controls.
- The GUI's model profile save rewrites the whole section from `ModelProfile`, and every server save
  renders the WHOLE file from `ServerConfig` (`server_cfg::render_for`): any key a client writes needs
  a field in the struct that renders it, or a save drops it.
- The GUI's delete leaves orphan assignment keys, which bind to a later profile of the same id: a
  create must clear them.

### Config files as llama.cpp v1.0.0 reads them

- The last line of a key wins, keys are exact case, a repeated `[X]` header starts the section afresh
  (`common/preset.cpp` `parse_ini_from_file`); every arg spelling is a key (`c` and `ctx-size` are one
  option), which no reader of ours models.
- `ini::text::replace_key` edits the FIRST line, any case: an edit that leaves a later duplicate
  changes nothing llama-server reads.
- `ini::line_starts_with_key` matches any letter case, llama-server only the exact one: any "already
  equal, leave it" shortcut must also require the exact spelling.
- The router re-reads the model profile file at `GET /models?reload=1` and after a finished download
  (`need_reload`), not only at start, and cascades the pool's `[*]` into every profile
  (`server-models.cpp` `cascade`): a rule on a profile's effective value reads the section, else `[*]`.
- A router reload unloads a running model whose preset options changed, `tags` included
  (`server-models.cpp` `preset_options_for_compare` strips only host, port, alias, log colours).
- The router refuses an unknown key in `--models-preset` (`server-models.cpp` 415, `common/preset.cpp`
  325): a model profile's engine is the file that holds it, never a key.

### Instances, profiles and placement

- A new `server@<id>.ini` IS the instance (no create method; Windows `Start` creates the SCM service,
  `scm.rs` `sync` deletes orphans at the manager's start); there is no remove.
- Two pools per engine: server profiles (`server@<profile>.ini`, one instance each) and model profiles
  (the `models.ini` pair); an instance is its section of the engine's `instances.ini`. A per-engine
  directory is the engine (no `Engine` key), so cross-engine mismatch checks are structural.
- "Is this model profile served on that instance" has one rule, `pai_core::assign` (`render`, `rows`):
  the bff derives `effective`, `restart` and `running` from it, never from the global switch alone (an
  `on` override serves a switched-off profile).
- On Linux `run-instance` execs llama-server (no parent left; what the code needs at stop is
  `ExecStop=`, which gets `$MAINPID`); on Windows `instance_main` (`windows/entry.rs`) stays the parent
  and kills the child on `Stop`.
- A value the running server uses (port, `--slot-save-path`) is in its argv; the server file may
  already be ahead of it (saved, not applied).
- The shipped `server@default.ini` files differ per OS: Linux's sets `ModelsDir`
  (`/var/lib/llama.cpp/models`), Windows' sets neither `ModelsDir` nor `StateDir`. A change to an
  instance default moves `default`'s folders only where the key is unset.
- Snapshots are named by model stem, so a shared state folder collides across instances.
- `server@<id>.ini` needs `control` to read: anything the session-free Overview shows from it needs an
  unauthenticated manager call.
- `ModelsDir` reaches llama-server only as `LLAMA_CACHE` (`launch.rs`), and llama.cpp creates a
  download's parents itself.
- Placement: the router overlays its own argv on every model's preset after the `[*]` cascade
  (`server-models.cpp`, `merge` overwrites), so a server-profile flag beats any model profile key (only
  the `unset_reserved_args` ones are dropped); `--device` is parsed at the router's start (an unknown
  name exits); `--fit` is on by default and fills unset placement; `tensor-split` is positional, extra
  weights ignored.
- Device keys: `parse_device_list` splits `device`, `device-draft`, `mmproj-device` on `,` with no trim
  (a space after the comma is `invalid device`), refuses CPU, takes `none` alone, matches names
  case-insensitively (`ggml_backend_dev_by_name`); `tensor-split` splits on `[,/]+` (an empty leading
  piece, no empty trailing one), throws at 16 entries and on a non-number (`common/arg.cpp`).
- `-ot`'s device is an exact buffer-type name looked up among EVERY device, not the `--device` subset
  (`common/arg.cpp` 229-259, `llama-model-loader.cpp` 1330-1345), so `-ot`, `device-draft` and
  `mmproj-device` can escape a server profile's `Devices`. The user chose a hard bound (2026-11-21):
  `model-profile-device-outside` at R3's judging points, so any new device-naming model key must join
  `devices_outside`.
- The GUI's per-GPU layer sliders are `tensor-split` weights stored as block counts (`gpu_split.rs`
  `Layout`, `set_blocks`), not a key of their own.
- Model profile tags are designed to live in the manager's record `model-tags.ini` (ARCHITECTURE.md),
  not built: the manager has no create/rename/delete of model profiles (the GUI writes the pool whole),
  so a record keyed by profile id needs a prune after every pool write and the renamer carrying the
  tags; review both when a slice adds a new way to write the pool.
- The server document is served at `/api/instances/{id}/server` and at `/api/profiles/server/{id}`
  (`crates/bff/src/lib.rs`); the fe reads the profile one (`ServerForm.tsx`).
- The bff runs no value checks itself: it filters body keys and relays the manager's refusals.
- The fe's type guards check the fields they need and ignore extra ones (`fe/src/ModelProfiles.tsx`),
  so additive reply fields let a backend slice land without the fe.

### Memory, devices and the node

- `Shape.kv` is f16 at a context capped at `REF_CTX` and `Shape.ctx` is that cap, not the trained
  context: a check needing the full context or another cache type needs `kv_per_token` per token.
  `Shape` has no `block_count` and no expert KVs; `Header.tensors` + `tensor_bytes` list tensors
  without ggml.
- The GGUF header reader (`pai_core::gguf_shape`) and the `--list-devices` parse (`pai_core::devices`,
  with `gpu_pool`) are in `pai-core`; `hf_fit::estimate` and the probe (`pai-app` `devices::list`) are
  `pai-app`'s, which the manager cannot link.
- `--list-devices` lists one card once per backend (CUDA0 and Vulkan0): sum only after `gpu_pool`, and
  prints no PCI bus id (the props have it), so identical cards differ only by ordinal.
- `--n-cpu-moe N` loops N times (`common.h` 1291).
- zbus 5.19.0 connections have no method timeout unless built with one; a manager call that can run a
  cold `--list-devices` (30 s bound) needs a bff deadline above the 5 s read `MANAGER_TIMEOUT`.
- `--list-devices` and `ggml_backend_dev_props` carry no driver version; a kernel driver's version is
  `/sys/module/<driver>/version` only when it sets `MODULE_VERSION` (DKMS amdgpu and NVIDIA `nv.c`
  yes; in-tree amdgpu, i915, xe no: show the kernel). Windows per-adapter driver versions are
  `DEVPKEY_Device_DriverVersion` via `CM_Get_DevNode_PropertyW`.
- The device probe runs `llama-server --list-devices` as the manager itself (root / LocalSystem), so
  ICDs already load as root in a child; `ash` 0.38 is in `Cargo.lock` through `wgpu-hal` (`loaded`
  feature: run-time loader, no link).
- Vulkan (docs.vulkan.org refpages): `VK_EXT_pci_bus_info` and `VK_KHR_driver_properties` are DEVICE
  extensions.
- Installed versions: the Linux runtime packages' versions are the SDK's (`components::dist_version` of
  the dist path, e.g. `llama.cpp-rocm 10.1.0`) and the engine's `1:<release>+pai<n>`; Windows bundles
  the runtimes inside llama.cpp's one MSI, so nothing installed records their versions unless the
  package writes it. This machine has both dpkg and `/usr/bin/rpm`.
- Monitoring, checked 2026-11-21 (ARCHITECTURE.md "Monitoring and log consultation" has the tool,
  version and licence findings): no OTel Collector receiver reads amdgpu or NVML, and node_exporter's
  `drm` collector (off by default) has amdgpu busy, VRAM and GTT but no `mem_busy_percent`. The bff's
  unit has `PrivateDevices=yes` (no `/dev/nvidia*`) and a step 14 node has no bff, so node-side
  collection belongs in `-core`.
- History tiers must have each step a whole multiple of the previous one, or the fold is not exact: 720
  points over 7 days would be 14 min, which 1 h does not divide (15 min used).

### The platform: polkit, systemd, Windows

- `pai-bff` runs as the unprivileged system user `private-ai-bff`; pam_unix's helper `unix_chkpwd`
  verifies only the calling user's own password (unix_chkpwd(8)), so password checks for other accounts
  go through the manager (root on Linux, LocalSystem on Windows).
- polkit cannot authorize a user who has no process: its subjects are a process, a session or a bus
  name (polkit Authority interface, "Subject"). For a vouched user the manager checks `pai` itself.
- Dropping root, checked in sources: systemd sends polkit the `unit` and `verb` for unit jobs (v226+,
  `unit_polkit_details`) but NO details for `manage-unit-files` (`dbus-util.c`), so enable cannot be
  scoped to a unit; polkit lets a non-root daemon check another user's subject only through the
  action's `org.freedesktop.policykit.owner` annotation; Windows `SERVICE_CHANGE_CONFIG` and
  `SC_MANAGER_CREATE_SERVICE` are admin-equivalent, while `LogonUserW` and `NetUserGetLocalGroups` need
  no privilege; a pipe client at `SECURITY_IDENTIFICATION` needs no `SeImpersonatePrivilege` on the
  server.
- systemd re-applies `StateDirectoryMode=` to EVERY listed `StateDirectory=` at each start and chowns
  one recursively to `User=`/`Group=` when the owner differs (systemd.exec(5)): a mode or group set by
  tmpfiles on such a folder is undone, so keep shared folders (the models folder, group `pai`) out of it.
- The models folder is created by the ENGINE's packages (Linux tmpfiles `llama.cpp.conf`, 0755
  private-ai; Windows `EngineDataModels`, ALL SERVICES modify, Users read), so a desktop download on
  Linux cannot write it; `/var/lib/private-ai` is the service user's home at 0750, so nothing a `pai`
  member must reach can sit under it. `paths::instance_models_dir()` on Linux is
  `/var/lib/llama.cpp/models`. The client restore of snapshots lists `/var/lib/private-ai/...` (0750),
  unreadable to a desktop user on Linux.
- A file the Windows manager creates under `%ProgramData%\private-ai` inherits the MSI's protected DACL
  on that root (SYSTEM, Administrators full, Users read: `MachineDataRoot` in
  `installer/private-ai-framework.wxs`), so users cannot pre-create a subfolder to redirect a
  LocalSystem write; a new folder under bare `%ProgramData%` would not have that protection (its ACL
  lets users create subfolders). The MSI removes the root only when empty, so such files outlive an
  uninstall.
- Windows service timeouts: preshutdown default 10 s since build 15063 (SERVICE_PRESHUTDOWN_INFO),
  plain shutdown about 20 s, control handler back within 30 s (HandlerEx remarks).
- The bff's unit hardening hides `/dev` and `/usr/lib/modules`, not `/sys` or `/proc/driver`
  (systemd.exec(5)), so sysfs and os-release reads work as `private-ai-bff`.

### Cross-machine, downloads and the director

- Cross-machine designs: ARCHITECTURE.md part 2 has the sources and the decisions (D-Bus non-local TCP
  is deprecated and unprotected, so it is rejected; the OpenSSH server is in the box only on Windows
  Server 2025; kubeadm's SPKI-hash check is the precedent for join codes). The nodes use the director's
  private CA, not pinning (since 2026-11-21): rustls 0.23 `WebPkiClientVerifier` takes CRLs
  (`with_crls`), `export_keying_material` binds a MAC to one connection, rcgen 0.14 parses CSRs
  (`from_der`, feature `x509-parser`); rcgen's CRL writer is unverified. Leaves last 90 days, renewed
  from 30 days before, every 5 min while connected, at once on reconnection. CA decisions (user,
  2026-11-21): plain 0600 key file (backups carry it, manual passphrase export only), no grace for an
  expired node (rejoin), P-256, CA 5 years rotated at 4, no offline root. The user overrode two
  security recommendations that day (the key's path, the key at rest): state the exposure plainly and
  let them choose.
- The user wants one codebase for the director, so keep the separation in process, account and files (a
  director at euid 0 refuses to start; `serve` never reads its state). Roles come by subcommand, never
  by a writable config key: a file would let one write move the CA into the root process.
- Hugging Face OAuth (the full findings, with sources, in ARCHITECTURE.md, read 2026-11-21): the device
  code flow needs no redirect URI, so it fits a bff on any `[Bff] Listen` address; the authorization
  code flow's redirect URI must match exactly, except `http` loopback (any port). No revocation
  endpoint, so a token cannot be dropped server side.
- `hf.rs`'s curl runs with `-L` and takes the endpoint from `MODEL_ENDPOINT`/`HF_ENDPOINT`: fine for a
  desktop user, an SSRF lever in a service, where a fixed endpoint and no redirects are needed (curl
  limits a redirect's protocol, never its host). `--max-filesize` aborts an unknown-length answer only
  from curl 8.4.0 (man curl). The tree answer's `lfs.oid` is the file's sha256 (`files_from_tree`).

### Drawn views

- The views live in `docs/architecture/{infrastructure,system}.html` (hand SVG, colours from CSS
  tokens, A4 `@page`): update them when a component, account, port or route group changes.
- They always draw the TARGET architecture (user, 2026-11-22): every accepted design as plain boxes, no
  "today" or "not built" labels inside an SVG, dashed only for designs still proposed. What runs now
  goes in the "Built today" note under each figure and in the tables' status column.

## Pitfalls

- A tester's "red tests in the tree" may be only a `todo!()` stub: `git status --untracked-files=all`
  and grep the method name before reporting what stands.
- Before a slice, grep the tester's error keys and wire fields against the design record: the tests
  often land first and differ.
- A fixture where the server profile id equals the instance id (`[gpu-1] ServerProfile = gpu-1`) hides
  a wrong lookup.
- Script-driven tests of a child process cannot catch a device extension looked up in the instance's
  Vulkan list: review the child's Vulkan calls by hand and make the first real run show `pci` on every
  row.
- `serde_json::Map` iterates in key order unless `preserve_order` is on, and that feature differs per
  build in this workspace (on through `pai-app`, off for `pai-bff` alone): never let file bytes depend
  on the order of a JSON object's keys, sort first.
