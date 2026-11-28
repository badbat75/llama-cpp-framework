---
name: docs-playbook
description: How the docs agent keeps the project docs accurate, and the pitfalls met so far. Use when editing README, AGENTS.md or docs/.
---

Rules are in `AGENTS.md`. Format of this file: `AGENTS.md`, under "Skills".

## How to

### Sourcing a fact

- For each fact, say whether it was measured here, read in a source, or reported by the user.
- When a measurement proves a note wrong, replace the note. Do not leave both.
- Verify a flag, a lint name or a cmdlet's syntax by running the tool or reading the vendor page, never
  from memory: PowerShell syntax on learn.microsoft.com with `?view=powershell-5.1` (Windows PowerShell
  5.1, what the runners and the custom actions use), citing the page; a Rust lint in a 4-line scratch
  crate under `/tmp/opencode` with `cargo clippy -- -D warnings` (works offline).
- Read the code that decides a rule before writing it. For the `pai` rule: `allowed` and `TokenFacts` in
  `crates/manager/src/manager/windows.rs` (control: interactive, Administrators or `pai`; configure:
  `pai` enabled and not deny-only, or LocalSystem), `ADMIN_GROUP = "pai"` in
  `crates/manager/src/manager/service.rs`, and the polkit rule in `xtask/src/linux/system_files.rs`.
  Check per-call authorization in `manager/service.rs` itself.
- A llama.cpp rule is readable offline in the build's clone (`build/linux/llama.cpp`, its release from
  `git describe`): the preset precedence (command line, then a model section, then `[*]`) is in v1.0.0's
  `tools/server/README.md`, not `server/README.md`, and `docs/preset.md` holds the `[*]` example.
- A llama.cpp source path moves between releases: the chat UI is `ui/apps/chat/` from v1.1.0 and was
  `tools/ui/` before. Read the path in the guard (`check_web_ui`, `xtask/src/common/llama.rs`) and in
  the clone, and the prebuilt fallback's repo and build number in the stamp beside the build
  (`build/linux/llama.cpp-cmake/ui/apps/chat/.ui-stamp`, `ggml-org/llama-ui|b12774` on 2026-11-26).
- Before writing that a manager default is live on this machine, read the render against the commit
  (`ls -l /etc/llama.cpp/models@default.ini`): GT5a's `[*] load-mode = none` was not in the render of 12:45,
  an hour before f545865, and the pool's model sections already carried `load-mode = none` from GT4's hand
  lever. The render is what the deployed manager writes; the pool's values can predate a slice.
- Read the client module headers (`crates/core/src/client/pipe.rs`, `crates/core/src/client/dbus.rs`:
  the `As` forms, `Version`, `acting_for`, what an older manager answers) before planning a comment
  edit: they usually need none, and the manager's own rules (who is checked, when, what is recorded)
  stay out of a client file.
- Read a string's current value in `crates/core/strings/*.toml` or the `.wxs` before paraphrasing it,
  and grep `PLAN.md` too, which quotes them. Quote a strings text as it is even when it names an old
  role (`api-key-withheld`): the string is code's to change; name the rule behind it (`configure`) and
  report the string.
- Count from the code, not from the prose: the guarded methods are
  `grep -c "async fn .*_as(" crates/manager/src/manager/dbus.rs`; the server document's keys are
  `FIELDS` in `crates/bff/src/server.rs`; a suite's tests are counted per file ("Running a check").
- Cite a CI run by its id with the commit it ran on and the commit reporting it (a run's id and result
  come from the coordinator's commit, no web fetch), never a date inferred from that commit's date.
- Settle only what a run's steps ran: CI's install, repair and uninstall are silent, so a green run says
  nothing about a full-UI install, an upgrade, the full-UI uninstall, a `configure` as a member, or the
  token's next-logon rule.
- Say "type-checked only, not run on Windows" beside each Windows fact until the user's Windows run (the
  GNU target checks `pai-core`, `pai-app`, `pai-cli`, `pai-manager`, `pai-bff` and `xtask`, and not
  `pai-config`'s Windows arms), and put what only that run can settle in `installer/AGENTS.md`'s test
  plan, not in the prose as fact.
- Mark planned work as planned: the accepted `/var/lib/private-ai/models` (`ARCHITECTURE.md` "One models
  folder for every engine") is never named as the current folder.
- Before writing "X no longer writes Y", grep every caller of the default behind it
  (`models_dir_or_default`, `set_server_key("ModelsDir"`): scope the claim to what the commit changed
  and list the rest.
- Write a refusal as the code's mapping plus the shape a test pins, never as measured, when no test
  carries it: the bff's 409 `instance-label-taken` (the pair `[label, other-id]`) has no test of its
  own, only `client_error_codes_map_to_status_and_message` in `tests/profile_switch.rs` pinning 409, the
  key and an empty `field`.
- A design's parenthetical is not an assertion: `ARCHITECTURE.md`'s "(today the parser ignores unknown
  fields)" left the pipe's answer to a `principal` on a plain method undecidable with no test open, so
  that rule stayed out of the docs and was reported.

### Where a fact belongs

- One page states a fact in full; the others carry one line and point there. The full statements:
  - who may configure: `installer/AGENTS.md`'s pipe bullet (the READMEs carry one line each);
  - a manager method's arguments and reply: "Through the manager" of `crates/config/README.md` (the
    D-Bus signature and the pipe JSON), which `installer/AGENTS.md`'s pipe bullet and
    `crates/bff/README.md` point to;
  - the node's device list: the manager's shape (D-Bus `(t probed_at, a(ssstt) devices, t ram_mib,
    u cpu_threads)`, the pipe's keys, who is asked, `probed_at` 0 when no probe answered) in "Through
    the manager" beside `ServerProfiles`, the bff's JSON and status codes in `crates/bff/README.md`'s
    "The node";
  - the probe's own rules (one at a time behind a `probing` lock, only a probe that read a non-CPU card
    cached, keyed by the exe's mtime, the same probe the memory check measures against) in the "manager
    runs every content check" bullet, where `ProbedGpus` is already described, not in a new bullet;
  - the `server.ini` rules (`checks::server_file`, plus `pai-app` `server_cfg::check`'s thread bound),
    the model profile rules (`checks::models_file`, `models_disabled_file`, `model_profile_ids_free`,
    `model_profiles::validate_for_save` and its callers `model_profiles::save` and
    `hf::save_model_profile`), the value rules (`checks::model_profile_values`: float, enum,
    `spec-type`, JSON, min <= max) and the MoE cache floor (`model_profiles::validate_moe_cache_floor`,
    save only, needs the GGUF): all in the "manager runs every content check" bullet of
    `crates/config/README.md`. The `pai server set` paragraph under its command block, the Models tab's
    "Ids are unique across both files" bullet and the root README's headless line point there; `limits`
    in `crates/bff/README.md` follows `checks::server_int_range` (no `LogVerbosity` in it).
  - the save-time address check, in that same bullet: host must resolve or run; the port is judged only
    for a profile in use, against the other instances' profiles and the web service's `[Bff] Listen`
    read through `pai_core::bff_cfg`; an unreadable profile judges nothing; a refusal names the instance
    or `private-ai-bff`;
  - the content of the shipped `crates/core/defaults/server@default.ini` (starting values only, pinned
    by `server_cfg.rs` tests): `build-packaging.md`'s "No config file is shipped" paragraph alone;
    `AGENTS.md`, the READMEs and `PLAN.md` only name the file;
  - a setting that is the service manager's own state (start at login, start at boot): "Through the
    manager" of `crates/config/README.md`; `AGENTS.md`, the README and `installer/AGENTS.md`'s test plan
    carry one line each;
  - the CI-only minimal build (`configure --minimal`): "The minimal build" of `build-compiling.md`;
    `build-pipeline.md` (Options), `build-packaging.md` (file names), `build-macos.md` and the `release`
    skill (step 8) carry a line each and link there;
  - the installer CI workflows (`.github/workflows/installers-*.yml`, started by hand):
    `docs/build-macos.md` ("CI.") and `installer/AGENTS.md`'s Status paragraph;
  - a sign-in reply's shape, status keys and lockout: `crates/bff/README.md`.
- A playbook bullet about an outside product (a licence, a version, a vendor limit) is a pointer, not a
  copy: the findings with their citations live in `ARCHITECTURE.md` ("Monitoring and log consultation",
  part 2 for the CA and the Hugging Face OAuth), so the playbook keeps the consequence for a design and
  names the record. The shape the role playbooks follow is in the read-only reference copies
  `/mnt/data/git/project_agents_structure/.agents/skills/*/SKILL.md` (the intro line, `## How to` with
  subsections, `## Pitfalls`); it has no designer copy.
- "The README" for a manager fact is `crates/config/README.md` (its "Through the manager" bullets per
  method, its source map per file): there is no `crates/manager/README.md`. A manager piece no method
  calls yet goes in the source map row, saying which slice will call it.
- The bff's rows say what a route answers and link to `crates/config/README.md` for what the manager
  does: keep that split. The server profile pool's five routes are `crates/bff/README.md`'s
  `## The server profile pool` (the list, the create, the document and the save, the delete), and the
  manager's three verbs are their own bullets in `crates/config/README.md`'s "Through the manager",
  beside the `SetServerProfile` pair.
- `AGENTS.md` is loaded into every session (keep it under ~40k characters): its Map bullets are pointers
  (what the area is, the guide, a safety rule the guide lacks), never a copy of the guide. A new fact
  goes in the guide; before cutting one from `AGENTS.md`, grep the guide for it and move it there if
  missing. Task workflows (the release) live in skills.
- In the web docs "administrator" means a `pai` member: `canConfigure` is the second bool of the
  manager's `Authenticate` (`PasswordChecker::is_admin` answers `pai` membership on both OSes). Define
  it where `canConfigure` first appears and let the later uses inherit.
- The route names the server file after the instance id (`server@<id>.ini`); it names the profile in use
  only because the shipped defaults carry `[default] ServerProfile = default`
  (`paths::instance_server_profile`). Both READMEs say this in one sentence: keep them so.
- Do not merge the two CPU counts: the server document's `limits.Threads` / `ThreadsBatch` maximum is
  the bff's own (`server.rs` `own_cpu_count`, 1 when the host answers nothing; the two saves repeat
  `available_parallelism` inline), not the node's `cpuThreads` of `GET /api/node/devices`; the reason is
  a bound, not a choice (`Devices` can run the engine's probe for 30 s and the document reads keep 5 s).
  The Server form reading `cpuThreads` is R5, open work in `PLAN.md`: say it is open, and name whose
  count the page reads today.
- The memory an estimate is measured against is the server profile's, not the node's (R3):
  `devices_usable` over the `[Server] Devices` names `instance_devices` reads, `pool_usable` only when
  the key is absent, and 0 (`none`, ids the probe knows none of) judges nothing. The refusal points are
  the growth checks, the server profile switch, and a save of an in-use profile that changes `Devices`
  (`check_shrink_memory_of`), where the rule inverts (only a profile that fits today's cards and not the
  new ones) and the probe runs only once a render has an estimate.
- A new manager refusal reaching the model profile save gets its 422 `field` from `field_of` in
  `crates/bff/src/model_profiles.rs`, which falls back to `args[0]`: check that a new key has a case
  there before writing which field it marks. A key whose args name a VALUE and not a key
  (`model-profile-no-model`, `model-profile-model-missing`, the `load-on-startup-*` pair) and a key that
  marks NOTHING (`model-profile-over-memory`: args the model profile, an instance, two MiB figures) each
  need an entry there, so the switch's 422 paragraph (which the save's table points at) and the save's
  422 row list them. The check's own wording goes in the config README's checks bullet (right after the
  model-file sentences) and its `SetModelProfile` bullet, and the probe's figures in the source map's
  `..\core\src\proc.rs` row (the 30 s bound, the 4 MiB of each stream kept, the 2 s grace).
- The bff README's prose wraps near 100 columns (median 96, p90 102 of its non-table lines): a 101 to
  106 column prose line is in style; its status tables are single long rows and need no wrap. Prettier
  does not cover `fe/briefs/*.md`, so brief prose is wrapped by hand.
- A markdown table row is ONE line: join a wrapped cell with `sed -i 'N{N;s/\n/ /}'`, then check every
  `^|` line ends with `|`.
- A name the user decides lands in `ARCHITECTURE.md` and the `PLAN.md` step's prose first: the root
  `AGENTS.md` Map bullet and the requirement's `R` row keep the old name. Grep the old word in every
  `.md` and say "formerly the old name" once, in the `R` row, not in every file.
- `ARCHITECTURE.md` design records go stale the day the slice is built and read as open work: the
  heading's "Designed <date>, proposed" becomes "built (<sha>, its follow-ups <sha>)", the "**What
  exists.**" paragraph is relabelled "**What existed when this was designed.**" (its text kept), and a
  key that moved after the design is recorded in that section's own "**As built**" paragraph, leaving
  the record's "Done when" text alone. Grep `proposed.`, `What exists.`, `is open`, `are open` at the
  start of each section.
- When an item of `installer/AGENTS.md`'s "Known gaps" closes in code, remove it there AND turn it into
  a test plan check (what a Windows run must show), since the fix is only type-checked.
- A Windows service's open behaviour (what the SCM shows on a failed start) goes in
  `installer/AGENTS.md`'s open checks, phrased "by the code".
- A folder a service creates at run time (the manager's `logs\` under `%ProgramData%\private-ai`): the
  MSI does not know it, so say it inherits the parent's DACL, outlives the uninstall and has no size cap
  (an open check), and add the test plan checks that show it: a fresh install has no such folder, the
  first guarded save makes it, a standard user reads the file (the manager records every guarded call,
  done or refused).
- A standalone HTML file (a flyer, a printable sheet) is the docs agent's to write, and the site ignores
  it: `page.rs` `load_all` reads only top-level `.md` of `docs/`, and `main.rs` copies only
  `docs/assets` (`copy_dir`), so a subdirectory of `docs/` publishes nothing.
- `docs/architecture/*.html` are hand-drawn SVG: `.t-small` is 10.5px (about 5.5 px a character),
  `.t-title` 13px bold (about 7.5); fit a new label to its `rect` width before writing it. Their
  "Source: ... <date>" line is when the built parts were checked: an edit of planned items only says
  "(the planned parts <date>)" instead of moving the date.
- The fe can run ahead of the bff: a route the fe calls is served only if `router_with` in
  `crates/bff/src/lib.rs` has it (`fe/briefs/api-contract.md` marks the rest "proposed"). Describe such
  a page as built against the contract, and say what today's bff makes of it (404, no JSON body).
- When `fe/` is off-limits, the fe commit's message (`git log -1 --format=%B <sha>`) settles what the fe
  shows now: cite the sha, and paraphrase a string rather than quoting it, its text living in
  `fe/src/strings/en_US.json`.
- The count of a model profile page's editable cards moves with every fe slice: with `fe/` off-limits,
  run `git log --oneline <last-known>..HEAD` and read the `2h.<n>` messages before writing a count.
  Write the rule, not the count ("every settings card is editable, the Engine field, the Assets card
  and the file picks read-only"): a count goes stale at the next slice.
- The nav's shape lives in `crates/bff/README.md` ("A shell with hash routes"): read `fe/src/App.tsx`
  (`NavHost`) before writing it.
- The UX check's duration and its states repeat in `AGENTS.md` "Commands", `fe/ux/README.md` (count the
  states from `STATES` in `fe/ux/check.js`, never from prose) and the tester, coder and designer
  playbooks (not ours: report their stale figures).

### Grep the repeats

- When a rule about who does what changes, grep it in: `AGENTS.md` ("Commands", "Roles", the "Cycle:"
  line, and the coordinator's "done only on" sentence in the Roles bullet), the role bodies and the
  `description:` lines of `.opencode/agents/*.md`, the playbooks, and `PLAN.md`'s Definition of Done.
- When a rule about who may configure changes, grep `Administrators`, `administrator` and `pai` in every
  `.md`: `AGENTS.md` (the Windows and the Linux machine-tree bullets), `installer/AGENTS.md` (the pipe
  bullet, the features bullet, the group's own bullet, "What CI covers", the test plan steps),
  `crates/config/README.md` ("Through the manager"), `crates/bff/README.md`, the root `README.md`,
  `docs/build-packaging.md`, `ARCHITECTURE.md`, `fe/DESIGN.md`, `fe/briefs/`. The manager's side is
  pinned by `cargo test -p pai-manager -- an_admin_member_controls_but_configures_only_in_pai` (the
  Windows decisions compile and run on Linux: `manager.rs` keeps `mod windows` unconditional, the Win32
  glue is `cfg(windows)`).
- When an authorization rule changes (polkit defaults, rules files, groups), grep `auth_admin`,
  `auth_self`, `administrator` and the action id across `AGENTS.md`, both READMEs of `crates/config`
  and `crates/bff`, the root README and `build-packaging.md`; `ARCHITECTURE.md` and `fe/briefs/` repeat
  them too and are not ours: report them.
- When the web UI gains a view, what it shows is repeated in `crates/bff/README.md` (intro, route table,
  "The web UI"), the `AGENTS.md` overview paragraph and the root `README.md` intro. After an fe slice,
  check the bff README's route table against `fe/src/route.ts` and the `call(` paths in `fe/src/*.tsx`,
  and the served routes with `grep -rn '"/api' crates/bff/src`.
- A model profile route change is repeated in eight spots of `crates/bff/README.md`: the intro's served
  list, the route table's model profile rows, the `## The model profile routes` lead, its `### The list`
  / `### The switch` / `### The model profile page` / `### The save` sections, `### What has run` (the
  suite names and test counts), the fe route table's `#/profiles/model/<id>` row, the fe "Every model
  profile call of the contract is served" bullet, and `## Tests`.
- A listen-address or `Host`-guard fact is in five files: `crates/bff/README.md` (intro, "Running it",
  the dated hand checks, the session cookie bullet), `AGENTS.md` (overview and the Map bullet),
  `ARCHITECTURE.md` (the diagram's browser arrow, the `pai-bff` row, "Boundaries a call crosses"), the
  root `README.md` (intro and the `crates\bff\` row), and `installer/AGENTS.md` (the web service bullet
  and the test plan steps that call a listen).
- Sign-in's mechanics (who signs in, the status and key of each reply, the lockout) sit in
  `crates/bff/README.md` in five spots: intro, the `SystemAuthenticator` paragraph, the status table,
  the "In memory only" lockout bullet, the web UI's "The bff does not serve yet" bullet, plus the
  sentence naming the calls the bff makes to the manager. Replace the forward-looking wording in all
  five, and mark any dated hand-check whose replies no longer hold with the commit it predates. Files
  outside our scope repeat them and lag: `ARCHITECTURE.md`'s bff row, `crates/config/README.md`'s
  source-map manager row, `fe/briefs/api-contract.md`, `fe/briefs/01-server.md`, `PLAN.md`'s step 10
  note: report them.
- A sign-in reply-shape change is repeated in nine places of the bff README: the route table's two
  `/api/session` rows, the session status table's 200 row, the server-read paragraph's session check,
  the 200 row and the `values`/`withheld` bullets of the server table, the fe shell's banner sentence,
  the fe route table's instance row (its administrator/user pair) and the fe "The session" bullet. Grep
  `canConfigure`, `role`, `withheld`, `administrat` there. The API key's withholding sits in three of
  them: when the rule moves, fix all three and name who decides.
- The sign-in-only rule (a visitor sees only the sign-in page) is in `crates/bff/README.md` (intro, the
  route table's rows, the instances status table, the session's idle bullet, the dated hand checks, the
  fe route table's `#/` row) and `AGENTS.md`'s overview, and outside those in the root `README.md`'s
  overview sentence, `ARCHITECTURE.md` (the `Connect` paragraph's "session-free routes", a "session-free
  Overview", the node-routes record) and `PLAN.md`: grep `without sign-in`, `session-free`, `for anyone`
  in every `.md`. The gate is `Sessions::peek`: the version, the instances and `GET boot` are gated and
  connect as the bff's own account, so the session section's "each use moves the deadline" bullet must
  name the polled reads that leave it where it was.
- The association routes are one shape in three files: `crates/bff/README.md` (the intro's served list,
  the route table's rows, the instances row's `serverProfile`, the pool's `assignments`, the switch's
  and save's `restart`, and a `## The association routes` of their own), `AGENTS.md` (the overview's
  served list, and BOTH machine-tree bullets' "which clients read and never write" of `assignments.ini`
  and `instances.ini`), and `fe/briefs/api-contract.md` (its route table, the served block, and its
  numbered "Open now" list). Grep the route name and `assignments.ini` in all three. In the contract a
  closed open point KEEPS its number and starts "Closed at <sha>:", because a line above cites it as
  "open point 4 below".
- A `[Server]` key is listed in five places: `crates/bff/README.md` (the read's `values` bullet, the save
  paragraph, "What has run"), `fe/briefs/api-contract.md` (the GET's `values`, the PUT's `values`, the
  `server_cfg::check` bullet, the 422 row's `field` rule) and `crates/config/README.md` (the Server
  tab's field list, the checks bullet). Count the keys from `FIELDS` in `crates/bff/src/server.rs` and
  grep the contract's count in words.
- The save-time address check repeats in six places: the checks bullet of `crates/config/README.md`, the
  bff README's "Running it" bullet, its 422 row, its "server file of an instance" lead, the `paths.rs`
  and `bff_cfg.rs` source-map rows, and the `AGENTS.md` machine-tree sentence.
- The memory rule's wording repeats in three files: `crates/config/README.md` (the checks bullet, the
  `SetModelProfile` and `SetServerProfile` bullets, the Server tab's `Devices` bullet, the
  `..\core\src\devices.rs` source-map row), `crates/bff/README.md` (the switch's paragraph, the two save
  422 rows, the association-routes 422 row) and `fe/briefs/api-contract.md` (the `Devices` value bullet,
  the 422 `field` rule, the switch route, the server-profile save's bullet, the resource-checks bullet).
  Grep `pool_usable`, `the node's GPU` and `the MiB the node has`.
- The instance's snapshot folder (`instance_state_dir()/<id>`, `launch::with_instance_defaults`) is in
  five sentences: `AGENTS.md` (the Windows and the Linux machine-tree bullets), `build-packaging.md`
  (the Windows folder paragraph, the shipped `server@default.ini` paragraph) and
  `crates/config/README.md` (the models-dir bullet, the snapshots card, the "Models and snapshots by
  default" row).
- After any models-folder change, grep `/var/lib/private-ai/.llama.cpp` (root `AGENTS.md`, three cells
  of `crates/config/README.md`, `fe/briefs/configurator-inventory.md`; the dated `fe/briefs/02-models.md`
  Q&A and the example model paths in `02-models.md` and `api-contract.md` still carry the old one).
  Today's Linux models default is `/var/lib/llama.cpp/models` (`paths::instance_models_dir`, the
  engine's tmpfiles.d entry, filled by `launch::with_instance_defaults`).
- The Windows installer's facts repeat in: `installer/AGENTS.md` (all, with the test plan),
  `build-packaging.md` (intro and file table, "The MSIs (Windows)", the Install table and bullets),
  `build-pipeline.md` (intro, Options, re-run table, OS table, "Where things land", footguns), the
  README (intro, features, steps, tree, config table, command table, Packaging), `AGENTS.md` (overview,
  the conventions on `%ProgramData%`), the `release` skill (steps 3 and 8, the names table),
  `crates/config/README.md` ("Through the manager", the source map's manager row),
  `crates/config/AGENTS.md` (log source) and `crates/bff/README.md`. Per-product build files are named
  from the staging dir (`stage.with_extension`): `build\windows\staging\<product>.license.rtf`,
  `.wix-obj\`.
- A new folder under `%ProgramData%\llama.cpp` or a new custom action of llama.cpp's MSI: update
  `installer/AGENTS.md`'s product table, ACL table, defines line, scripts bullet, the test plan's log
  list and `icacls` step, and reread the removal dialog's text in `llama.cpp.wxs` (it names what
  `DeleteEngineData` deletes).
- When a package is split or a file changes package, recheck what moved with the maintainer scripts: a
  "the scripts restart the services" sentence holds only for the package that has them. Each package's
  scripts are in `system_files.rs` (`deb_scripts`, `rpm_scriptlets`, one function per package), its rpm
  `Requires(<tag>)` in `package.rs`. A unit's package is named in four places: `build-packaging.md`
  (Holds table, paths table, system files heading and row), the README package table, and the manager
  row of `crates/config/README.md`'s source map. `components.rs` splits Linux and macOS alike: fix the
  path tables of `build-packaging.md` AND `build-macos.md`, and the README's package table.
 - An instance's readiness rule (RS) is stated once, in `build-packaging.md`'s system files table: the
   `llama.cpp@.service` row (the `ExecStartPost= wait-ready`, the 2 s poll, the 10-min deadline, the
   `TimeoutStartSec=11min` backstop, `activating (start-post)`), the `llama.cpp.target` row beside it,
   and the ordering paragraph under the table (`After=` only, and what it does not give). The manager
   verb goes in `crates/config/README.md`'s manager source-map row beside `save-slots`, the package
   relation in the relations table's `llama.cpp` rows, and the README package table names the target
   only.
- A maintainer-script change is described in `build-packaging.md`'s "When" table (Removal, Purge rows),
  the `DPKG_ROOT` sentence below it and the rpm `Requires(<tag>)` sentence; the "not run for real" line
  of the install log stays until a real removal or purge.
- When a package is added, grep the counts in words (`five`, `three`, `all five`) as well as the file
  lists: README, `AGENTS.md`, `docs/build-pipeline.md` (intro and the OS table), `build-packaging.md`
  (relations table, `.deb`/`.rpm` scan bullets) and `build-macos.md` each repeat them. A package with no
  2.4 predecessor is an exception in every "each replaces" sentence.
- When a crate is added, its name goes in: `AGENTS.md` Commands (crate list, Windows GNU list), the
  `release` skill's step 1 version list, the Map's crate list, `.opencode/agents/coordinator.md`
  (`cargo clean -p`), `coder.md` (GNU list), the source map's workspace row (`the six pai-* crates`),
  the README tree and command table, and `PLAN.md` W15 (the coordinator's: report it).
- When an xtask phase's arguments change, they are named in: `build-pipeline.md` (Options and "re-run"
  tables), the README command table, `build-compiling.md` (node/npm line, web UI section, Linux "built
  last"), `build-packaging.md` (MSI and `.deb`/`.rpm` intros), `build-macos.md` ("Package."), and the
  `scripts/0N-*` usage headers (not ours: report them).
- When an OS gap closes (macOS's release-tag refusal, its build dir), grep `does not yet`,
  `Unlike Linux`, `macOS does not` and the "Open, by the code" list: the `release` skill's steps,
  `build-packaging.md`'s file-name bullets and `build-macos.md` each repeat the gap.
- When a CI run goes green, also grep "untested", "never run on a Mac", "type-checked only", "not run on
  Windows" and "compiled by WiX" in `AGENTS.md` (overview, the `pai-manager` sentence, Map), the root
  `README.md` (intro, the `xtask install` row, Packaging), `build-pipeline.md` (intro, the OS table's
  Status and Packages rows), `build-packaging.md` (MSIs intro, the Install bullets), `build-compiling.md`
  ("The minimal build"), `crates/bff/README.md`, `installer/AGENTS.md` (the manager bullet too), the
  `release` skill (step 4) and `PLAN.md` R3. The Windows test plan opens with "What CI covers": update it
  with the workflow.
- The sccache nvcc gate is three lines: the `sccache` section's lead and its gate paragraph of
  `build-compiling.md` (the full statement, with the "not yet run" line naming the first build through
  the wrapper), and `build-pipeline.md`'s footgun row, which names the version and goes stale with the
  gate. The code's gate is `SCCACHE_NVCC_MIN` in `xtask/src/common/llama.rs`, read from
  `sccache --version`'s first line; the Linux configure prints `nvcc: cached` / `nvcc: not cached`.
- One run's Windows coverage sits in six sentences: `installer/AGENTS.md`'s Status ("the latest green
  run"), the manager bullet ("only CI has run it"), the group bullet ("Text-checked only"), "What CI
  covers" (the run list, its "no green run has executed them"), the Open checks' `PaiGroup` item ("have
  never run") and `AGENTS.md`'s Map bullet (the id beside "what that leaves unchecked"): grep "latest
  green run", "no green run", "have never run", "no Windows run has" beside the ids.
- When a manager protocol changes, the same change moves FOUR numbers on `crates/config/README.md`: the
  `As` bullet's count of guarded methods and its signature list, its `Version` protocol number and what
  each added, and the source map's manager row ("the `As` form of each of the N guarded methods"). Grep
  `guarded method`, `protocol ` and the count in words. `ARCHITECTURE.md`'s protocol table and its word
  count for the guarded methods repeat it and are the architect's. A slice that adds verbs without
  bumping `PROTOCOL` needs a sentence saying the number does not name them, so a manager at the old
  number answers them as an unknown method: grep `protocol ` for a sentence that implies the number is
  the proof.
- A reply widened in place (a pair added to a reply with no `PROTOCOL` bump) touches four places: the
  manager's bullet in `crates/config/README.md` (both signatures, the strict decode, and the sentence
  saying `PROTOCOL` does not name the pair, beside the same sentence for the pool and instance verbs),
  the pipe and D-Bus module headers (the coder's), the device bullet and route row of
  `crates/bff/README.md`, and the route of `fe/briefs/api-contract.md`. The suite that pins a widened
  reply keeps its own count, so the "What has run" number is what moves: quote the commit's own counts.
- A row widened with a string key (`label` on the instance row, `a(sssqasas)` for `a(sssqasass)`) is the
  widest spread of them all: grep the OLD signature in every `.md` and `.rs` (the decode and its
  signature assertion of `crates/core/src/client/dbus.rs`, its header comment too, the doc and test
  assertions of `crates/manager/src/manager/dbus.rs`, the `Instances()` paragraph and the source-map row
  of `crates/config/README.md`), and count the tuple yourself, the prose has been wrong before.
- A read with a bound of its own breaks two absolute sentences in `crates/bff/README.md`: "The reads of
  the router keep the 5 s `MANAGER_TIMEOUT`" (grep `keep the 5 s`) and "The limits the service runs
  with, 5 s of read and 15 s of write" in "What has run" (grep `limits the service runs with`), whose
  pinning test is `src/tests/timeouts.rs` alone; a route's own bound is pinned inside its suite
  (`the_devices_deadline_is_the_probe_bound_plus_five_seconds` in `tests/node_devices.rs`), so name two
  tests. `DEVICES_TIMEOUT` is `GPU_PROBE_TIMEOUT` plus the transport's 5 s: say which two, not just 35.
- The bff's two time limits are `MANAGER_TIMEOUT` (5 s, reads and control verbs) and `WRITE_TIMEOUT`
  (15 s, the server save and the model profile switch), both `lib.rs`, pinned by
  `the_write_limit_outlasts_the_read_limit` (`src/tests/timeouts.rs`). The 5 s sits in five bff README
  spots (the instances 503 row, the connect bullet, the server-read paragraph and its 503 row, the
  list's 503 row): grep `5 s` and say which routes the longer limit covers. Grep `504` in the same file
  when a commit changes what a 504 means.
- Two sentences of the bff README go false the day a route is added, both by naming a list: "The two
  routes that write a file" (grep `routes that write a file`) and "the three calls the manager answers
  without authorization" (grep `without authorization`).
- The Host guard's names are NOT read only at the start: with no `[Bff] Hosts`, a `Host` in none of
  them, not `localhost` and not an address, makes the guard read the host name and the resolver's search
  domains once and replace its list (`origin.rs` `reread`, the source from `lib.rs` `try_router_at`); a
  `[Bff] Hosts` list is fixed and never re-read. So a sentence saying the names are "read at the bff's
  start" is stale: grep `start` in that paragraph of `crates/bff/README.md` ("Running it"). Its builders
  name the difference: `router_with_session_and_hosts` is a fixed list, `..._and_host_source` a machine
  the test changes.
- The bff README carries the manager's refusal as two keys, one per kind of route, and grep both there
  before writing one: `session-not-administrator` (403) is a signed-in route's answer when the manager
  refuses its user, and the session ends with it (so the web UI tells an ended session from a live one
  without a second read); `client-not-authorized` (502) is what a session-free read answers when the
  bff's own account is refused (the instances list, `GET boot`: `control.rs` `read_failure`, `lib.rs`).
- The association routes' calls run on TWO principals: the file write as the signed-in user (its refusal
  is 403 `session-not-administrator` and ends the session), the instances list and the state read on the
  bff's own account (their refusal is 502 `client-not-authorized`, the session lives on; a state read
  that fails after a write that stands answers `running` true). Say both, once.
- The estimate's inputs are in `pai-core`: `pai_core::gguf_shape` (`parse_header`, `read_local_header`,
  `local_shape`, `shape_of`, `kv_per_token`, `GPU_RESERVE`, `REF_CTX`, `MAX_HEADER`) and
  `pai_core::devices` (`parse`, `DeviceOption`); `pai-app`'s `hf_fit.rs` and `devices.rs` re-export every
  name, so a doc naming those modules for the arithmetic is still true, while a source-map row that says
  the file HOLDS the parser is not: name the crate that does. The rows are in
  `crates/config/README.md`'s source layout; `ARCHITECTURE.md`'s are the architect's.
- The release process (`.agents/skills/release/SKILL.md`) lists every package file per OS and the upload
  globs: when `components.rs`, `msi::file_name` or `macos_pkg::file_name` change a name, fix it there too.
- When a package starts shipping defaults copied "only when missing", recheck every flow keyed on a file
  being absent: `migration::import_offered` skips a file the manager already has, so the import of a 2.4
  `server.ini`/`presets.ini` stops being offered on a packaged Linux install. Say which tree a named
  file sits in, and name the offer's subject as the old-named user file whose manager-named twin is
  missing. Grep `engine_config_dir`, `system_config_dir`, `/etc/llama.cpp` and `/etc/private-ai` after a
  layout change.
- When files move out of the per-user tree (to the manager's machine directory), check what still keys on
  them or names them: the MSI's user-data dialog (`LLAMA_USERDATA_FOUND` and the dialog text in the
  `.wxs`, described in `installer/AGENTS.md` with its test plan step), the root README's config table and
  uninstall line.
- A new web UI check goes in `AGENTS.md` Commands, `coder.md` and the bff README's web UI section.
- A Windows manager message key is named only in `installer/AGENTS.md`'s manager bullet: grep `cli.toml`'s
  old key in every `.md` after a rename.
- A renamed heading renames its anchor: grep `#<old-slug>` in every `.md` (the root README links into
  `crates/config/README.md`) before and after.
- When a `PLAN.md` step closes, grep `step <N>` (not only `PLAN.md step <N>`) in every `.md` and in
  `.github/workflows/*.yml` comments, and point to the file that now holds it.
- Before citing a `PLAN.md` slice for a bff route, check the slice lists that route.
- A licence is PER PRODUCT and the code decides it: `readme()`, `installer_page()` and `rpm_license()` of
  `xtask/src/common/licenses.rs`, with `components.rs` `product()` putting the engine and the GPU runtimes
  on `Product::Llama` (so a runtime package names no framework licence at all). The prose repeats it in
  `docs/build-packaging.md` (the MSI licence page, the `copyright`/`Homepage` paragraph, the rpm `License:`
  paragraph), `installer/AGENTS.md`'s Features and UI bullet and the root README's Licence section: grep
  `MIT` and `AGPL` in those. The framework's copyright holder is in the old LICENSE line only
  (`git show <sha>^:LICENSE`), the AGPL text carries no holder line.

### Editing a file

- Re-read a file right before editing it: the coordinator edits `AGENTS.md` and `PLAN.md` while you work.
- Scope every `git diff` to your own paths (`git diff -- crates/bff/README.md ...`) before reporting a
  change.
- The `AGENTS.md` and README lines are paragraphs thousands of characters long: grep with a short context
  window (a small Python `re` script) instead of printing whole lines, and replace with an exact,
  count-checked substring.
- Start an Edit `oldString` mid-line: the Read tool shows `N: ` plus a space before each line, so a
  file's 2-space indent reads as 3 and an `oldString` that starts with that leading whitespace never
  matches.
- Re-read the file after a scripted write: a `python3 - <<PY` write of a whole file can land silently
  unwritten (`>>` and `sed -i` landed). Prefer the edit tool for a line edit.
- Use a raw string for a Windows-style path in a Python script, and `grep -c $'\x08'` the files after
  such an edit.
- Check the em-dash glyph in `git diff`, not in the whole file, and keep a `—` only inside a code span
  quoting data.
- Pruning a playbook: the line count only falls on whole-bullet deletions and re-wraps, a trim that keeps
  the same number of lines buys nothing, so `wc -l` after every pass; grep the code for every file or
  function a bullet names before keeping it.
- Proving a playbook rewrite dropped no command: diff the set of code spans (`re.findall(r"`([^`]+)`", s)`)
  of `git show HEAD:<file>` against the new file, then grep each span the diff calls missing: one that
  wrapped at a new column reads as missing when it is there.

### Running a check

- `cargo run -p docs-site -- --check` is for `docs/` only: `tools/site` builds pages from `docs/`
  (`tools/site/src/main.rs`), so a change to the root `README.md`, `AGENTS.md`, `ARCHITECTURE.md` or a
  crate README needs no check.
- An upstream npm claim is reproducible offline: copy the package's `package.json` and
  `package-lock.json` to a scratch dir under `/tmp/opencode` and run `npm ci --dry-run` there (the
  lock-sync check fails before any fetch, writes nothing; `--offline` is not a `npm ci` flag on npm 9).
- For an anchor, a small Python `re` pass over `](#...)` against the headings of the same file settles
  it. The site turns a link out of `docs/` into a GitHub URL, so an anchor into a README is GitHub's slug
  (`### Migration from 2.4` is `#migration-from-24`): check it in the built site (`--out <dir>`).
- `cargo test -p pai-bff` prints one block per suite and the tail shows only the last: scope it with
  `cargo test -p pai-bff --test model_profiles --test model_profiles_switch` and grep `test result`. A
  **lib** test (`src/tests/model_profile_body.rs`) needs `cargo test -p pai-bff --lib`: a `--test <name>`
  flag does not reach it.
- Count a suite's tests with `grep -c '#\[test\]\|#\[tokio::test\]'` on the file; a whole-crate total
  wants `grep -cE '#\[(tokio::)?test(\(|\])'` over the crate's files. Say which it is: it counts
  attributes, not a run, and a run can be larger (`tests/hf_files.rs`: 16 attributes, 18 tests run).
- A README's per-suite count is the count of the tree it cites: recount a suite only when a commit
  changed it (`git show <sha>:<suite> | grep -c` settles the old number) and leave an untouched suite at
  its dated count. To date one test of a suite to the commit that added it, use
  `git log --oneline -1 -S "<fn name>" -- <suite>`.
- The fe checks run as `npm --prefix fe test` (Vitest, seconds, no network), `npm --prefix fe run
  typecheck`, `lint` and `format:check` (seconds each, write nothing).
- Check what the Server form sends with `git show HEAD:fe/src/ServerForm.tsx`, grepping `toValues` and
  `merged` there: the form sends the values it read with its edits over them (`ServerForm.tsx` `merged`,
  pinned by `fe/src/ServerSaveKeys.test.tsx`), so `Tags` and `Devices`, the keys with no control,
  survive a Server save; keep saying that a hand-built body which omits a key still clears it.
- A route that reaches the manager can be checked by hand with no manager: run the debug `pai-bff` with
  `PRIVATE_AI_DATA_ROOT` (and `<root>/system-config/bff.ini` for a port); it answers 503 `unavailable`.
- Check a standalone HTML page offline:
  `~/.cache/ms-playwright/chromium-*/chrome-linux64/chrome --headless --no-sandbox
  --no-pdf-header-footer --print-to-pdf <f> file://<f>`, then `pdfinfo` for the page count; give each run
  its own `--user-data-dir`, and add `--window-size=W,H --screenshot` to see the screen view.
- Render a print page (`docs/flyer/`) with Playwright loaded from
  `require('<repo>/fe/ux/node_modules/playwright')` in a script under `/tmp`:
  `page.pdf({preferCSSPageSize: true, printBackground: true})`, count pages with
  `re.findall(rb'/Type\s*/Page[^s]', pdf)`, look at `pdftoppm -r 60 -png`.
- `cargo clean -p <crate>` with no `--release`/`--profile` removes only that crate's dev-profile
  artifacts in the configured target dir; `-n` is a safe dry run (shown by `cargo clean -n -v`).

### Renames and moves

- After a directory move, grep for the old path in both slash styles and for `-p <old-package>`, and fix
  the relative links inside the moved files too (`crates/config/*.md` reaches the repo root with
  `../../`). Binary names are a separate rename: leave them unless the brief says so.
- After a crate split, take the package list from `cargo metadata --no-deps` (the Commands list, the
  coordinator's `cargo clean -p`), and grep `module::item` names (`runstate::server_args`) as well as
  paths.
- After modules move between crates, the crate docs name files bare (`model_profiles.rs`) or
  crate-relative (`src\model_profiles.rs`): fix the crate-relative ones, say once at the top which crate
  holds the bare ones, and recheck claims tied to the old crate (`pai-config` not GNU-checked,
  `system_connect`'s Windows arm, which `build.rs` emits what, the test crate a command needs:
  `cargo test -p <new>`).
- After a product rename, grep the scripts the names lints do not scan (`tools/*.ps1`) for the old data
  paths, and describe what the script does today rather than the new name.
- After an install-layout change, grep the old prefix (`/opt/private-ai`) in the root `README.md`,
  `docs/build-*.md`, `AGENTS.md` and `crates/*/README.md`: the root README repeats the paths.
- After the command line moved to `pai` (package `pai-cli`), grep
  `pai-config (server|model-profile|control|bench|config)` in every `.md` and in skill `description:`
  lines; `pai-config` stays where it is the GUI, the executable a script runs (`--version`,
  `sdk-devices`) or what xtask builds and packages.
- A repo-wide word rename (`preset` to `model profile`): grep the word and mark what stays BEFORE
  editing. What stays: llama.cpp's own words (`--models-preset`, `base_preset`, `common/preset.cpp`,
  `common_preset::to_args`), what is stored as data (bench.db's `run_param.scope` is `preset`, so a
  `bench-chart` comment and an `x`/`series`/`where` key keep the word: `sqlite3 benchmarks/bench.db
  "select distinct scope from run_param"`), a WiX `Component Id` (`DefaultPresetsIni`), a skill's own
  `name:`, a quoted design title (`ARCHITECTURE.md` "Saving a preset from the web"), and a run log of an
  older layout. Sweep with `sed` on a word boundary, then read every changed line: "the tensor preset"
  needs more than the substitution ("the tensor-split model profile").

## Pitfalls

- A source about something similar looks like a source, but is only a hint.
- Code doc comments and names lag the rule (`PIPE_SDDL`'s doc says "neither interactive nor an
  administrator's", `is_admin` is named for the old one): code's to fix, report them.
- A test plan's "as an administrator" never says which: a manager call needs the group (`configure`), a
  hand edit of `%ProgramData%\llama.cpp\config\` needs an elevated Administrators member (the folder's
  ACL, the group has nothing there).
- The Windows manager reads the client's TOKEN, so a group added after a logon counts at the **next
  logon**, while the sign-in's `Authenticate` reads the group database (`NetUserGetLocalGroups`,
  `logon::in_local_group`) and sees it at once: two answers, no run has settled the first.
- A group the MSI creates is machine data: `util:Group` wants a per-machine key path (an HKLM value of
  the product's key) and `RemoveOnUninstall="no"`, because Util's default deletes it at every uninstall
  and `MajorUpgrade` uninstalls first, so each upgrade would empty what the administrator filled. The
  `.wxs` test in `xtask/src/common/msi.rs` reads the group's name from the manager's `ADMIN_GROUP` const,
  so renaming the group fails a test: grep the const too.
- `PLAN.md` can name a file that does not exist (it lists `crates/manager/README.md`): check with
  `git ls-files` before promising to edit a file, and report the line.
- `grep` here is ugrep: a long `.{0,120}` context regex fails with "exceeds complexity limits". Use a
  Python `re` script instead.
- `AGENTS.md`'s own em-dash rule quotes the glyph it bans, so `grep -c $'\u2014' AGENTS.md` is 1 on a
  clean file.
- `"docs\build-macos.md"` in a Python string puts a backspace byte in the file (one sat in `AGENTS.md`
  as `docs\x08uild-macos.md`).
- A `python3 - <<PY` write of a whole file can land silently unwritten: three line-level rewrites of
  `crates/bff/README.md` reported success and left the bytes.
- Mid-session the tree gains whole files you never touched (another agent's slice: `fe/src/*`,
  `fe/ux/*`, the other playbooks), and the working tree can be ahead of HEAD in `fe/` (the coder's slice
  in progress).
- A slice in progress sits in the working tree as untracked files, so a grep of the working copy can
  document unbuilt work: read `fe/` with `git show main:<path>`, and check a file is tracked
  (`git ls-files --error-unmatch`) before citing it.
- **`main` moves while a docs task runs**: the coordinator commits slices between your tool calls. The
  22D4 delete was an untracked brief when the reading began and shipped (eb042d8) before the last
  sentence was written, so a "not built" sentence written early became false inside the same session.
  Re-read `git log --oneline -5 main` and `git status --porcelain` before the final pass, and re-check
  every "not built" / "no page of its own" claim against the new HEAD.
- A slice in progress sits in `crates/` as much as in `fe/`: the whole IP5 tree carried uncommitted
  manager, bff and core edits, so a route's served JSON and a check's key list were read with
  `git show HEAD:<path>`, never from the working copy.
- The `fe/` route table of `crates/bff/README.md` is one line per page and lags the briefs: the model
  page's row still said the Save bar opened the section, sticky to the viewport's top, after 22 L2.B1
  (the user's ruling of 2026-11-26) moved it to the section's last child, sticky to the bottom, and
  L2.A2 put Delete last in its group. A later ruling in the same brief reverses an earlier item, so
  re-check every layout claim of a row you touch against the component and `fe/src/index.css`
  (`.save-bar { position: sticky; bottom: 0 }`), and cite the ruling, not the item it reversed.
- A Rust list mirrored in `fe/` goes by a shorter name: `checks::MODEL_PROFILE_STAY_KEYS` is
  `STAY_KEYS` in `fe/src/ModelProfileCards.tsx`. Grep the fe file for the const before naming it; the
  brief's name is the Rust one.
- The last docs commit is not the baseline for a served/not-served claim: `AGENTS.md` still said the
  server profile pool had "no page of its own" long after the pages landed (56d5f66 predates the docs
  commit 4d4a55f). Check every such claim against `fe/src/route.ts` and the `call(` paths on `main`, not
  only against the commits since the last docs commit.
- A gate of the form "run the docs-site check only if `pgrep -fc 'cargo|rustc'` is 0" never passes: the
  brief's own argv contains the word, so the launcher shell, the parent `opencode run` and the pgrep
  shell are all counted. Ask for the programs: `pgrep -xc cargo` and `pgrep -xc rustc`.
- Plan step numbers are reused, so a bare `step 12` may mean an older step, and the installer test plan
  has its own numbered steps.
- A commit message's reason can be wrong in a detail: one blamed the instance unit's `ProtectHome`, which
  only `private-ai-bff.service` sets (`xtask/src/linux/system_files.rs`).
- `xtask` messages can lag the build: check the phase that needs a tool, not the one that reports it.
  Whether `xtask configure` fails on a tool is `rows.miss` (a gap, fails) against `rows.opt` (a warning)
  in `xtask/src/<os>/configure.rs`.
- `xtask install` takes no product on any OS (only files); `package` and `build` do. `version::llama_build`
  refuses a non-release clone on every OS: there is no `b<N>` package name.
- `xtask install`'s "predates the build" warning keys on the build tree's `llama-server` mtime only
  (`linux/install.rs`): recheck it when what a build updates changes.
- Only Windows refuses a 2 GiB release asset (`windows/package.rs`), and nothing pins the llama.cpp tag
  across the three build machines (`common/llama.rs` takes the newest release at build time).
- A build script that reads `build/<os>/...` of the TARGET OS (pai-app's clone lookup) fails the GNU
  check on Linux: the docs give the override with an absolute path (the script runs in its crate dir).
- A command run on a GUI-only `pai-config.exe` gets exit 2; the MSI's stopper tries `pai.exe` first for
  that reason.
- A check done by hand on an old layout (an `strace`, a run) is not true of the new one: say it was not
  repeated. Sizes measured on an old package split are not the new packages'.
- Absolute sentences ("No config file is shipped", "overwritten on every upgrade") break when a package
  gains a conffile.
- `components::file_stem` and `of()` are shared with macOS: a Linux split renames and resplits the `.pkg`
  set too, even when the macOS slice is "later"; say so on `build-macos.md` rather than leave its table
  looking current.
- The per-user tree and the manager's tree name the model profile files DIFFERENTLY (per-user
  `presets.ini` and `presets.ini.disabled`, manager `models.ini` and `models.ini.disabled`): the import
  is the rename point, `migration::USER_FILES` pairs each pair, `migrate.rs`'s `OURS` and
  `OUR_CONFIG_FILES` hold the 2.4 names, and a packaged machine has both sets on disk (here
  `/etc/llama.cpp` holds `presets.ini` beside `models.ini`).
- "Text-checked" in `installer/AGENTS.md` means xtask's tests read the `.wxs` and evaluate its conditions
  (`msi_tests/engine_lifecycle.rs`), never a WiX compile.
- The GUI's state placeholder is the ROOT, id not joined (`gui.rs`'s `InstanceDirs`), while the launch and
  the readers join the id (`launch::instance_command`, `slot_state::restore_newest`, `cli.rs` with
  `default`): "the placeholder names the instance's folder" is wrong, and the shipped Linux file names
  `ModelsDir` and no `StateDir`.
- The bff's 504 means only a write's **unknown** outcome: a write that landed is a 200 with
  `running: true` even when the limit ran out on the read back.
- The association routes are the only writes that never answer 504: the answer needs the state read, so a
  write that outlives the 15 s is 503 `client-unavailable` (the model profile switch and save keep their
  504).
- `DefaultBodyLimit` (`lib.rs`) only caps: the 413 is the `Bytes` rejection a write handler turns into
  `payload-too-large` after `sessions.current`, so no session is 401 whatever the body. Every write's
  503/504 follows `server.rs` `timeout_answer` and `state.sent()` (504 once handed over).
- `WriteState` is **per request** (shared by the handler and its task, `crates/bff/src/server.rs`), so the
  save and the switch share the 503/504 rule, not a lock.
- A bff read route runs no content check at all (a 422 comes from the manager's write), and which checks
  run per file is `check_contents` in `crates/manager/src/manager/service.rs` (`models_file` then
  `model_profile_ids_free` for `models.ini`, `models_disabled_file` then `model_profile_ids_free` for
  `models.ini.disabled`, each `?` first).
- A write that puts a model profile in effect also runs the manager's model-file check, which is not in
  `pai-core` `checks.rs` (`check_model_files` in the manager's `check_contents`, after `check_renders`,
  and `check_model_files_of` inside `set_model_profile`); `ProbedGpus` and `check_render_memory` are the
  source of the probe's figures, none of it is in `checks.rs`.
- The TWO `field_of` functions differ on `model-profile-over-memory`: `model_profiles.rs` answers NO
  field, `server.rs` falls through to `args[0]`, the model profile's id, at a server-profile save, where
  the form has no control for it: read both before writing one sentence about it.
- Three server-profile-pool facts read backwards if you do not check the code: the list is sorted by id
  in the **manager** (`server_profiles`, the bff keeps its order); a profile no instance runs is judged
  against **no** address, so it can carry a running instance's port and meets the instances only at
  `SetServerProfile`; and `CreateServerProfile`'s `port` of `0` means "no `Port` key" (`port_of`), not
  port 0.
- The two transports read "no label" differently and the docs must say both: the bus has no null so the
  manager answers the empty string and the client maps it, while the pipe's missing key is `None` and an
  older row of the OLD shape is a strict-decode failure (`ClientError::Other`, a 502), never a list of
  empty labels. The bff's own key order is alphabetical and its rows keep the manager's order, so a
  widened row moves no key.
- A text rule that clears with the empty text repeats the tags rule (`remove_key`, `set_key`, the trim of
  spaces and tabs on the way in): the trim is the bff's `label_text` and the check's own, so a 422's
  `args` carry the trimmed text, the one the file would have held.
- A 0 that means "no figure" needs saying on all three pages: the host reads (`crates/core/src/host.rs`)
  answer 0 when they fail, the bff scales nothing, and the fe shows neither `0 MiB` nor a 0 cap.
- The session cookie binds to the host, never to a port (`session.rs` sets no `Domain`); the guard's 403
  is `request-bad-host`, the cross-origin one `request-cross-origin` (`origin.rs`), both named in
  `crates/core/strings/cli.toml`, which no longer carries `bff-listen-not-loopback`.
- The instances list answers 401 BEFORE the manager is asked: a dated check that logged a 503 there needs
  its date inside the sentence, not beside it.
- `Devices` is `Action::Control` asked **quietly** (`authorize_quietly`): the polkit action's description
  in `xtask/src/linux/system_files.rs` still says "start, stop, restart", so `build-packaging.md`'s row,
  which quotes that file, stays as it is and the rule goes in the authorization bullet (report the lag,
  it is code's).
- Before writing WHICH refusal a value breaking two rules gets, read the order in `server_cfg::check`:
  the text keys' `comment_markers` loop runs before `device_list`, so a `;` in `Devices` is
  `comment-markers`, and `ini.rs` cuts the value at the marker, so the file reloads as what stands
  before it.
- The model profile page's route is `#/profiles/model/<id>` (`fe/src/route.ts`), not `#/presets/<id>`
  (the old one, redirected), and `GET` of an unregistered path is a bare 404 with no JSON body
  (`crates/bff/src/lib.rs`), which the fe reads as its generic "could not be read".
- A brief can name a nav entry the code does not have yet, and a JSON body quoted in a brief may be
  shortened: check the route's fields in the code before quoting them.
- A brief's "not built" line is a plan statement, and it goes stale the day the slice lands: the later
  slice amends the brief (brief 16 lists its own "Superseded:" criteria, 17f2's pencil replacing 17f's
  Rename), so read the brief for that note before calling it stale, and rewrite the line as built with
  the commit that landed it (`fe/briefs/22-model-sources.md` said "17f, not built" after f70d631).
- A string a slice drops stays in the repo: a test pins its absence (`DROPPED_DELETE` in
  `fe/src/ModelProfileSwitch.test.tsx` holds "To delete it, switch it off first."). Grep
  `fe/src/strings/en_US.json` for the key, not the whole tree for the text, before saying a page shows
  a sentence.
- A `](#slug)` into a BOLDED bullet of a README resolves nowhere: only headings get a slug. Link the
  heading that holds the bullet (`[Code conventions](#code-conventions)`) and name the bullet in words.
- A hand-read figure sitting in an UNCOMMITTED page line can be from another engine and process than
  the one the page is about (the R9700 page's `evicted_ms` 0 was read on Strata, not `llama-server`).
  Before keeping it, settle which process it names, and cite the engine, the process, the model and
  the date of the trace; label it an observation, not a campaign.
- The 5 s read and 15 s write of the save are counted from the request (`save` takes
  `write_timeout.saturating_sub(elapsed)`).
- `cargo test -p pai-bff`'s coarse `^async fn` and `^#[test]` pattern over-counts helper fns and doc
  text, and MISSES `#[tokio::test(flavor = ...)]`.
- Inline SVG paints in document order: a later filled `rect` covers a `text` drawn before it, which a
  screenshot catches and a PDF text dump does not.
- A fixed `height: 297mm` with `overflow: hidden` hides overflow from the page count: compare the sheet's
  `scrollHeight` and `clientHeight` under `emulateMedia({media: 'print'})` at 794x1123 px.
- The print flag is `--no-pdf-header-footer`; `--print-to-pdf-no-header` leaves the date and URL in.
- Never run `find /` (or any search from the root) to locate a source file: it scans every disk, hangs in D state on the busy data disk and stays behind the run. Cite the upstream repository's URL (for ROCm sources, github.com/ROCm/rocm-systems at the commit named) instead.
