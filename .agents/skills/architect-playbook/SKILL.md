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
  2025 (Microsoft Learn, OpenSSH first use); kubeadm's SPKI-hash check is the precedent for join
  codes. Leaves 90 days, renewal from 30 days before, every 5 min while connected, at once on
  reconnection (user, 2026-11-21). Since 2026-11-21 the super-manager is **the director** (`pai-director`) and nodes use its
  private CA, not pinning (ARCHITECTURE.md Part 2): rustls 0.23 `WebPkiClientVerifier` takes CRLs
  (`with_crls`), `export_keying_material` binds a MAC to one connection, rcgen 0.14 parses CSRs
  (`from_der`, feature `x509-parser`) and signs with `signed_by`; rcgen's CRL writer is unverified.
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
- Writes and revisions (checked 2026-11-16): the manager serializes every write with its `writing`
  mutex (`service.rs` `write_file`), so a compare-and-swap belongs there, not in the bff. `ModelsDir`
  reaches llama-server only as `LLAMA_CACHE` (`launch.rs`), and llama.cpp creates a download's parents
  itself. Keyed hashes: `siphasher` (SipHash-2-4) and `sha2` are in `Cargo.lock`, `hmac` is not; FNV
  is not a MAC, even with a secret basis (draft-eastlake-fnv sections 2.2 and 6). The render of
  `server@<id>.ini` from values is in `pai-core` (`server_cfg::render_for`).
- Since slice 9b a stop can last minutes (the snapshot save): any caller that stops and then acts on
  the instance (client restore after a restart, the MSIs' `Stop-Service`, the Windows `restart`
  thread) must be checked for a race with a server that is still saving. Windows `restart` is
  asynchronous (`scm.rs`, `RestartGate`); systemd's `RestartUnit` is a queued job.
- Preset files as llama-server reads them (v1.0.0, checked 2026-11-16): the last line of a key wins,
  keys are exact case, a repeated `[X]` header starts the section afresh (`common/preset.cpp`
  `parse_ini_from_file`); every arg spelling is a key (`c` and `ctx-size` are one option), which no
  reader of ours models. `ini::text::replace_key` edits the FIRST line, any case: an edit that leaves
  a later duplicate changes nothing llama-server reads. The router re-reads `presets.ini` at
  `GET /models?reload=1` and after a finished download (`need_reload`), not only at start.
- The manager's preset checks (`checks::presets_file`) run only on sections whose lines changed, so a
  section-scoped revision in the bff loses no check; the switch's two writes rely on
  `presets_disabled_file`/`preset_ids_free` letting an unchanged twin through, destination first.
- `WriteFileIf` refuses a missing file (`Changed`): a design that may create a file needs the plain
  write for that case.
- A key list that is a permission rule must be pinned against code, not against a brief: the preset
  keys the configurator writes are reachable as `render_section(&full_preset())` in `pai-app`'s tests
  (`full_preset` has no `..Default`, so it sets every field); `pai-bff` cannot link `pai-app`, so a
  shared list goes in `pai-core` `checks.rs`. Brief criteria (such as 02h's 133) describe kinds of
  controls; the field tables of 02e (92 to 112) name the keys.
- `ini::text::line_starts_with_key` matches any letter case, llama-server only the exact one: any
  "already equal, leave it" shortcut must also require the exact spelling.
- `serde_json::Map` iterates in key order unless `preserve_order` is on, and that feature differs
  per build in this workspace (on through `pai-app`, off for `pai-bff` alone): never let file bytes
  depend on the order of a JSON object's keys; sort first.
- `RawSection.keys` holds the RAW value (inline comment included); only `comment_markers` reads it,
  every other preset rule reads `s.get` (cut). Any raw-line rule must exempt lines a current preset
  file already holds in the same section, or a byte-for-byte edit (web save, switch on) of a
  hand-commented line is refused (decision c1). `presets_file` sees only `presets.ini` as `old`
  unless given the disabled file: a switch on checks the moved section whole.
- Instances (checked 2026-11-17): a new `server@<id>.ini` IS the instance (no create method; Windows
  `Start` creates the SCM service, `scm.rs` `sync` deletes orphans at the manager's start); there is no
  remove. Both the GUI's and the bff's server saves render the WHOLE file from `ServerConfig`
  (`render_for`), so any new server key must be a `ServerConfig` field or a save drops it. The shipped
  `server@default.ini` sets `StateDir` explicitly; snapshots are named by model stem, so a shared
  state folder collides across instances. `server@<id>.ini` needs `control` to read: anything the
  session-free Overview shows from it needs an unauthenticated manager call.
- Profiles (user, 2026-11-17, supersedes "presets"): two per-engine pools, server profiles
  (`server@<profile>.ini`, one instance each) and model profiles (`models.ini` pair); an instance is its
  section of the engine's `instances.ini` (ARCHITECTURE.md "Revision: profiles"). Before that slice
  lands, `paths::instance_ids` still lists `server@*.ini` and `check_address` compares those files: both
  move to `instances.ini`. A per-engine directory is the engine (no `Engine` key), so cross-engine
  mismatch checks are structural. Since 10a the GGUF header reader (`pai_core::gguf_shape`) and the
  `--list-devices` parser (`pai_core::devices`) are in `pai-core`; `hf_fit::estimate`, `gpu_pool`
  and the probe (`devices::list`) are still `pai-app`'s, which the manager cannot link.
- `Shape.kv` is f16 at a context capped at `REF_CTX` and `Shape.ctx` is that cap, not the trained
  context: a check needing the full context or another cache type needs `kv_per_token` per token.
  `--list-devices` lists one card once per backend (CUDA0 and Vulkan0): sum only after `gpu_pool`.
- llama-server cascades the pool's `[*]` into every profile (`server-models.cpp` `cascade`): a rule on
  a profile's effective value reads the section, else `[*]`.
- A tester's "red tests in the tree" may be only a `todo!()` stub: `git status --untracked-files=all`
  and grep the method name before reporting what stands.
- A render-based check has two write paths: the pool files (`check_contents`, record on disk) and
  `set_model_profile` (new record `text`, pool on disk). `render_of` always reads the record on disk,
  so the second path must render from `text` (as `check_model_files_of` does) or it judges nothing;
  review both, and look for an e2e that calls `set_model_profile`, not only `write_file`.
- Error keys: one rule, one key (`server-bad-tags` serves the profile's and the instance's tags); a
  `*-malformed` key names the route's body (`server-malformed`, `tags-malformed`). Before a slice,
  grep the tester's keys and wire fields against the record: the tests often land first and differ.
- Since `instances.ini` (slice 5) an instance's server file is `server@<ServerProfile>.ini`, not
  `server@<id>.ini`: in a review, check every new reader goes through `paths::instance_server_profile`,
  and that its test names a profile different from the instance id (fixtures with `[gpu-1]
  ServerProfile = gpu-1` hide the wrong lookup; slice 12's `instances()` had it).
- Windows' shipped `server@default.ini` (`crates/core/defaults/windows/`) sets no `ModelsDir` or
  `StateDir`, Linux's sets both: a change to an instance default moves `default`'s folders on Windows only.
- Installed versions (slice 13, checked 2026-11-20): the Linux runtime packages' versions are the
  SDK's (`components::dist_version` of the dist path, e.g. `llama.cpp-rocm 10.1.0`), the engine's
  `1:<release>+pai<n>`; Windows bundles the runtimes inside llama.cpp's one MSI, so nothing installed
  records their versions unless the package writes it. This machine has both dpkg and `/usr/bin/rpm`.
- The bff's server document is still at `/api/instances/{id}/server` (`crates/bff/src/server.rs`,
  called by `fe/src/ServerSection.tsx` and `ServerForm.tsx`); slice 14 adds `/api/profiles/server/{id}`
  beside it, and slice 17 (the fe) switches and removes the old one. The manager has no list of server
  profiles before slice 14's `ServerProfiles()`, and no `unknown-engine` refusal before it either.
- Device keys as llama.cpp v1.0.0 reads them (checked 2026-11-20): `parse_device_list` splits `device`,
  `device-draft`, `mmproj-device` on `,` with no trim (a space after the comma is `invalid device`),
  refuses CPU, takes `none` alone, matches names case-insensitively (`ggml_backend_dev_by_name`);
  `tensor-split` splits on `[,/]+`, throws at 16 entries and on a non-number (`common/arg.cpp`). The
  bff runs no value checks itself: it filters body keys and relays the manager's refusals.
- zbus 5.19.0 connections have no method timeout unless built with one; a manager call that can run a
  cold `--list-devices` (30 s bound) needs a bff deadline above the 5 s read `MANAGER_TIMEOUT`.
- A value rule judges what llama.cpp's preset reader yields (`common/preset.cpp`, a PEG grammar):
  spaces after `=` and before a `;`/`#`/newline dropped, the value cut at the first `;` or `#`; that is
  `checks.rs` `RawSection::get`. Never judge the raw line: a hand-edit then blocks every save of the
  section. `-ts`'s `[,/]+` split gives an empty leading piece but no empty trailing one.
- "Is this model profile served on that instance" has one rule, `pai_core::assign` (`render`, and
  `rows` from slice 15): the bff derives `effective`, `restart` and `running` from it, never from the
  global switch alone (an `on` override serves a switched-off profile). The fe's type guards check
  the fields they need and ignore extra ones (`fe/src/ModelProfiles.tsx`), so additive reply fields
  let a backend slice land without the fe.
- A new refusal is cheapest as a keyed `checks::Refusal` (`ManagerError::Invalid`): it already crosses
  D-Bus and the pipe and `ClientError` decodes it; a new `ManagerError` variant needs an arm in
  `service.rs`, `dbus.rs`, `pipe.rs` and `client.rs`. Add the key to `checks.rs`'s list, `Refusal::text`
  and both strings files. Windows `ScmService::start` and `set_boot_enabled` create a missing service,
  so a remove must cancel the restart gate first; systemd's template needs no create (checked 2026-11-21).
- A post-build review checks every edge-case clause in the design text against the code, not only
  the "done when" list: slice 16's "a newline first when the text does not end with one" was in the
  design, missing in `append_record`, and in no done-when item (2026-11-21).
- Monitoring sources checked 2026-11-21 (ARCHITECTURE.md "Monitoring and log consultation"): no OTel
  Collector receiver reads amdgpu or NVML (hostmetrics has no GPU scraper; journald receiver alpha,
  Linux only, runs journalctl); node_exporter `drm` (off by default) has amdgpu busy, VRAM, GTT but no
  `mem_busy_percent`; DCGM is Linux only, GeForce limited. OTel Rust: metrics and logs stable, traces
  beta (0.33.1). Licences: Loki AGPL-3.0, Netdata GPL-3.0, Fluent Bit and VictoriaLogs Apache-2.0;
  VictoriaLogs' mTLS is Enterprise only; Promtail EOL 2026-03-02. The bff's unit has
  `PrivateDevices=yes` (no `/dev/nvidia*`) and a step 14 node has no bff: node-side collection
  belongs in `-core`.
- Drawn views live in `docs/architecture/{infrastructure,system}.html` (hand SVG, colours from CSS
  tokens, A4 `@page`): update them when a component, account, port or route group changes.
- Placement facts (llama.cpp v1.0.0, checked 2026-11-21): the router overlays its own argv on every
  model's preset after the `[*]` cascade (`server-models.cpp` 791-795, `merge` overwrites), so a
  server-profile flag beats any model profile key (only `unset_reserved_args` ones are dropped);
  `--device` is parsed at the router's start (unknown name: it exits); `--fit` is on by default and
  fills unset placement; `tensor-split` is positional, extra weights ignored; `--list-devices` prints
  no PCI bus id (the props have it), so identical cards differ only by ordinal.
- Layer and tensor facts (llama.cpp v1.0.0, checked 2026-11-21): the GUI's per-GPU "layer sliders"
  are `tensor-split` weights stored as block counts (`gpu_split.rs` `Layout`, `set_blocks`), not a key
  of their own. `-ot`'s device is an exact buffer-type name looked up among EVERY device, not the
  `--device` subset (`common/arg.cpp` 229-259, `llama-model-loader.cpp` 1330-1345), so `-ot`,
  `device-draft`, `mmproj-device` can escape a server profile's `Devices`. `--n-cpu-moe N` loops N
  times (`common.h` 1291). `Shape` has no `block_count`/expert KVs; `Header.tensors` + `tensor_bytes`
  list tensors without ggml. The user chose a hard bound (2026-11-21): `model-profile-device-outside`
  at R3's judging points, so any new device-naming model key must join `devices_outside`.
- History tiers (13d) must have each step a whole multiple of the previous one, or the fold is not
  exact: 720 points over 7 days would be 14 min, which 1 h does not divide (15 min used).
- HF and models-folder facts (checked 2026-11-21): `hf.rs` curl runs with `-L` and takes the endpoint
  from `MODEL_ENDPOINT`/`HF_ENDPOINT`, fine for a desktop user, an SSRF lever in a service: a service
  caller needs a fixed endpoint and no redirects (curl limits a redirect's protocol, never its host).
  `--max-filesize` aborts an unknown-length answer only from curl 8.4.0 (man curl). The tree answer's
  `lfs.oid` is the file's sha256 (read in `files_from_tree`). `paths::instance_models_dir()` on Linux
  is `/var/lib/llama.cpp/models`, while root `AGENTS.md` still says `/var/lib/private-ai/.llama.cpp`:
  trust the code.
- Hugging Face OAuth (huggingface.co/docs/hub/oauth and its openid-configuration, 2026-11-21): device
  code, authorization code with PKCE S256 and refresh_token grants; public apps without a secret;
  redirect URIs exact except `http` loopback (any port); no revocation endpoint; scopes `gated-repos`
  (public gated, granted) and `read-repos` (private). A bff on a LAN address fits the device flow.
- Models folder today (2026-11-21): created by the ENGINE's packages (Linux tmpfiles
  `llama.cpp.conf`, 0755 private-ai; Windows `EngineDataModels`, ALL SERVICES modify, Users read), so a
  desktop download on Linux cannot write it; `/var/lib/private-ai` is the service user's home at 0750,
  so nothing a `pai` member must reach can sit under it.
- systemd re-applies `StateDirectoryMode=` to EVERY listed `StateDirectory=` at each start and chowns
  one recursively to `User=`/`Group=` when the owner differs (systemd.exec(5)): a mode or group set by
  tmpfiles on such a folder is undone; keep shared folders (the models folder, group `pai`) out of it.
- Director CA decisions (user, 2026-11-21): plain 0600 key file (backups carry it; manual passphrase
  export only), no grace for an expired node (rejoin), P-256, CA 5 years rotated at 4, no offline root.
  The user overrode two security recommendations this day (path, key at rest): state the exposure
  plainly and let them choose.
- The director is a role of `pai-manager` (user, 2026-11-22): `pai-manager director`, its own unit and
  account `private-ai-director`, unit shipped disabled in `-core`, `pai node promote|demote`. The user
  wants one codebase; the separation that matters is process, account and files, so keep it there
  (a director at euid 0 refuses to start; `serve` never reads its state). Roles by subcommand, never by
  a writable config key: a file would let one write move the CA into the root process.
- Before inventing a framework key for `models.ini`, grep llama.cpp's `common/arg.cpp` for a native
  option: v1.0.0 has `--tags` (preset key `tags`, listed in the router's `/models`) and `--alias`
  (checked 2026-11-22). A router reload unloads a running model whose preset options changed, `tags`
  included (`server-models.cpp` `preset_options_for_compare` strips only host, port, alias, log
  colours). The GUI's model profile save rewrites the whole section from `ModelProfile`, so any key
  the web writes needs a `ModelProfile` field or a GUI save drops it.
- Model profile tags live in the manager's record `model-tags.ini` (user, 2026-11-22, against the
  native `tags` key): the manager has no create/rename/delete of model profiles (the GUI writes the
  pool whole), so a record keyed by profile id needs a prune after every pool write and the renamer
  carrying the tags; review both when a slice adds a new way to write the pool.
- Node facts (checked 2026-11-22): `--list-devices` and `ggml_backend_dev_props` carry no driver version;
  a kernel driver's version is `/sys/module/<driver>/version` only when it sets `MODULE_VERSION`
  (DKMS amdgpu, NVIDIA `nv.c` yes; in-tree amdgpu, i915, xe no: show the kernel). The bff's unit
  hardening hides `/dev` and `/usr/lib/modules`, not `/sys` or `/proc/driver` (systemd.exec(5)), so
  sysfs and os-release reads work as `private-ai-bff`; Windows per-adapter driver versions are
  `DEVPKEY_Device_DriverVersion` via `CM_Get_DevNode_PropertyW`.
  The device probe runs `llama-server --list-devices` as the manager itself (root/LocalSystem), so ICDs
  already load as root in a child; `ash` 0.38 is in `Cargo.lock` through `wgpu-hal` (`loaded`
  feature: run-time loader, no link). A polled read may ask the manager: peek the session and use
  its identity as the `As` principal (`control.rs` `boot_read` precedent).
- Model profile create/delete facts (checked 2026-11-22): the manager's section rule requires a
  non-blank `model` (`checks::model_profile_section`), so an `hf-repo`-only section is refused unless
  the rule is loosened; `FETCH_KEYS` is private to `service.rs`. The control-character check on values
  lives only in the bff (`model_profiles.rs` `parse_body`): any manager method that BUILDS a line from
  caller text must judge it itself, or a newline injects a section. The GUI's delete leaves orphan
  assignment keys, which bind to a later profile of the same id: a create must clear them.
- A value the web shell needs on every page (the host name) must come from a read that asks the
  manager nothing (`/api/version`): since AN4, `/api/node` waits on `NodeInfoAs` (10 s cold).
- Vulkan (checked 2026-11-22, docs.vulkan.org refpages): `VK_EXT_pci_bus_info` and
  `VK_KHR_driver_properties` are DEVICE extensions; a probe that looks for them in the instance
  extension list never finds them. Script-driven tests of a child cannot catch this: review the
  child's Vulkan calls by hand and make the first real run show `pci` on every row.
