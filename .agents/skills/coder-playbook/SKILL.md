---
name: coder-playbook
description: How the coder implements and runs code safely in this project, and the pitfalls met so far. Use when changing production code.
---

Rules are in `AGENTS.md`. Format of this file: `AGENTS.md`, under "Skills".

## How to

- Run every ad-hoc script under a hard timeout: `timeout 120 python3 - <<'EOF'`, never a bare `python3 -` heredoc. Two rewrite scripts with backtracking regexes outlived their run and spun at 100% CPU for 13.5 h (2026-11-26); prefer plain string edits to multi-line regexes over whole files.

### Working in the shared tree

- Before starting: `ls` the named test files, `git log --oneline -3`, `git status`. A slice with no test
  file has nothing to turn green, so report that and write no test. Verify your whole set with
  `git diff <paths>` right before the test run.
- Snapshot outside the repo before a sweeping pass (`cp -a crates xtask <snap>`), then `ls` the snapshot.
  `git stash`, `git checkout` and `git restore` are off limits: a stash measures the tree without your
  slice.
- Anchor a scripted edit on exact text (`s.index(anchor)`, `assert count == 1`), never on a line number
  read a moment ago. When a scripted pass both deletes line ranges and rewrites the header above them,
  delete FIRST: a header rewrite moves every later line number.
- Moving a module's half into another crate while its inline `mod tests` must stay byte-identical:
  re-export every name the tests and the callers use (`pub use` in the old module), and a tuple field a
  test constructs (`Token("x".into())`) has to become public, its doc saying so. Prove the tests
  unchanged: `git show HEAD:<file> | sed -n '/^#\[cfg(test)\]$/,$p'` diffed against the working file.
- For a baseline red list without a stash: `git worktree add --detach /tmp/<x> HEAD`, copy in the
  tester's untracked files, symlink `fe/node_modules`, run there, `git worktree remove --force`.
  With no git side effect at all: `git archive HEAD | tar -x -C /tmp/<x>` (copy the untracked tests in
  by hand; the copy has no generated `.cargo/config.toml`, so it builds into its own `target/`).
- To prove a hypothesis, copy the tree (`rsync -a --exclude build --exclude .git --exclude dist
  --exclude node_modules <repo> /tmp/<x>`), use a fresh or shared `CARGO_TARGET_DIR` (registry crates are
  reused; only workspace crates rebuild), patch there, rerun. Never patch the repo's tests.
- To enumerate the reds a panic hides in a one-test e2e, work in a tree copy: rewrite each
  `assert!(cond, msg)` to a non-fatal `eprintln!` mark, run with `--nocapture` (captured stdout prints
  only on failure), and grep the marks.
- Tests the tester owns that the linter or formatter reddens are mostly not yours: apply only the
  mechanical, assertion-neutral fix (`.expect_err(msg)`, a `_` prefix on a never-read binding, a
  `#[cfg(unix)]` gate on a helper only unix tests call), name it, and report the file. Anything that
  changes what a test asserts stays the tester's.
- Several reds unsatisfiable together (two pins of one value, an `ends_with` against a trailing newline,
  a count that two tests read differently): prove it in a copy in minutes and report the pair, do not pad
  production. Name the witness pair, the two calls whose texts differ in one letter's case, one expected
  accepted and one refused: no rule separates them. To show the rest of the suite IS satisfiable, patch
  the fixtures in the copy (the colliding text, a `want` count the block's own refusals outgrow, a wire
  signature a slice grew) and rerun to green; every patch is one defect to report, with its line. A
  phase's COMMENT about its fixture ("the node holds 8581 and 8692") is not the fixture, read the seed's
  bytes, and add up every phase that pins the same rule before reporting: five pins agreed with the rule
  and two did not, and the two sat in the one phase whose comment was stale.
- Flakes: reproduce solo, state it, move on. The manager's D-Bus and gpu-probe tests under load, the
  e2e's colliding-writes gate phase (`the first write returns`, a 5 s `GATE_WAIT`) when a cargo build
  runs beside the suite (the suite alone: 142 s, a solo e2e beside a build: 669 s), xtask's
  script-sandbox tests (one temp dir per process), the bff's `tests/session.rs` timing tests, `fe`'s
  `ServerSave.test.tsx` navigation test `1b.36b: the edits-dropped alert stays until the page is left`
  (one of 3100 under `--maxWorkers=4`, green solo and on the rerun), the `fe` full suite's one red
  "nothing is sent / one GET more" race per run (a late `GET /api/node/devices` in ModelProfileContent
  `2d.70`, a late session read in Nodes `13 N.3` and AboutPage `18 AN.2`: 1 to 2 of 3166, a DIFFERENT
  one each run, green solo, and green in the same run as the tester's stable reds), the UX
  check's pre-existing states. `App.test.tsx` `1a.4` and `ModelProfileDelete.test.tsx` `22 D.6`
  joined that list (2 of 3212 under `--maxWorkers=12`, green solo).

### Running the checks

- Capture a loop's output before judging it: `out=$(eval "$c" 2>&1); code=$?`, then print `exit=$code`
  and the warning count. Judge from `FAILED`/`panicked`/`error`.
- Enumerate a crate's test targets with `--no-fail-fast`.
- On Linux export `CARGO_TARGET_DIR=$PWD/build/linux/cargo-target` for every cargo command: the checked-in
  `.cargo/config.toml` pins `target-dir = "build/windows/cargo-target"`, so a bare `cargo test` rebuilds the
  whole workspace there.
- The configurator's e2e phases (`gui::server_tab_e2e`, `settings_tab_e2e`, `log_window_e2e`) run inside the
  ONE `#[test] editable_widgets_track_model_after_edit` of `src/tests/ui_bindings.rs`: a filter of `e2e`
  matches 0 tests, filter by that name.
- Gate a test-only wrapper `#[cfg(test)]` (never `allow(dead_code)`), avoid a `#[cfg(test)]` variant that
  a production `match` names, and gate a `#[cfg(unix)]`-only helper too.
- Before adding a public field, enumerate the crates that write a full struct literal of that type;
  report those sites as the tester's, and prove production with clippy without `--all-targets`.
- Sort edits so a serialised file's order never depends on a map; reproduce an order-only red with both
  packages in one invocation.
- `cargo fmt -p <crate>` rewrites the crate's tests and every file: when others have uncommitted lines,
  apply `--check`'s diff to your own file by hand, and prove with `git status --porcelain -- '*.rs'`
  before and after that no other file moved. After a rename that lengthens identifiers, run fmt per
  touched crate; `cargo fmt -p a -p b` takes several crates.
- A mechanical clippy fix of a whole crate: `cargo clippy --fix --allow-dirty -p <crate> --all-targets`,
  with a snapshot and a `diff` showing only the intended edit.
- Windows-only code is type-checked with `cargo check` then `clippy --all-targets --target
  x86_64-pc-windows-gnu` (prefix `PRIVATE_AI_LLAMA_CPP_DIR` for `pai-app`/`pai-cli`): nothing runs, and
  the report says so. A lib-root `use` needed only in `cfg(target_os = "linux")` code is `unused_imports`
  elsewhere: gate it, and put a windows-sys feature a crate calls in that crate's own manifest. Say when a file the check does not
  compile is involved (`crates/core/src/client/dbus.rs` is Linux-only).
- To run ONE phase of the manager's `tests::data_root_e2e` solo, add a probe `#[test]` to
  `crates/manager/src/tests/mod.rs` in a copy (`crate::manager::service::<phase>()`;
  `self::<mod>::<phase>()` for a phase of a `tests/` submodule). To prove a rule inside a phase, extract
  the phase's `{ … }` block into a `pub(crate) fn` with the giant fn's scope bindings re-added (`sys`,
  `record`, `alice`, `bob`).
- Arithmetic with a ceiling: "saturating, divide last" is `checked_mul(..).and_then(..).map_or(MAX, |x|
  x / d)`, not a `saturating_mul` chain.

### Refreshing the llama.cpp patches

- Never touch `build/<os>/llama.cpp` (xtask owns it, it carries the applied patches and its revert
  stashes): `git -C build/linux/llama.cpp worktree add /tmp/<x> <tag>`, work there, `git -C
  build/linux/llama.cpp worktree remove --force /tmp/<x>` at the end.
- `git apply --check` every `.patch` on the pristine tag first: the ones that pass need nothing, and
  their sha256 in `patches/llama.cpp/README.md` stays. A hunk usually fails because upstream added a
  member BELOW the inserted line, so the hunk's trailing context moved, not because the code changed:
  read the target region before rewriting anything.
- A bare `git diff` patch splits: `git apply --include='src/models/x.cpp'` applies the part that still
  fits, edit the header by hand, regenerate the whole file with `git diff` (the index lines and the
  hunk numbers come out right). Keep each patch's own format: `0001` is a bare `git diff`, `0005` and
  `0006` are `git format-patch` bodies.
- xtask applies every `*.patch` in the folder sorted by name (the extension filter skips README.md and
  candidates.md), so prove the sequence in that order: check, apply, check, apply, then
  `git diff --stat`. Nothing pins a patch's sha256 in code, the README is the record.

### Source lints and renames

- Before renaming anything the GUI binds, check the `.slint` files. A source-text pin reads code without
  whole-line comments and ends a method at its first brace; the production region of a file ends at the
  first `#[cfg(test)]` line; a name a lint matches is the lint's data (`control_lint.rs`).
- Renaming a word: the ARCHITECTURE.md names table is the authority; enumerate every site by grepping the
  literal first. Apply the map to comment text only, not to code tokens, and fix pointers to a renamed
  identifier in the same slice. `.slint` comments cap at 80 columns. Prove a comment-only pass: every
  added diff line trimmed of `&*-` starts with `//`.
- A test that pins a phrase written inside a multi-line string literal reads the RENDERED text: keep the
  phrase contiguous and rewrap around it (a trailing space before `\` is kept, the newline and the next
  line's indent are stripped), and count the RENDERED line widths, not the source's, when the file is a
  wrapped text (the `readme()` of `xtask/src/common/licenses.rs`, a staged README.txt at ~79 columns).
  Case matters: a pin of an ALL-CAPS licence title is not met by the mixed-case name in prose.
- A strings-file VALUE is pinned by its callers' tests (and is a region's accessible name in the web UI).
  A refusal a client decodes must be a key of `checks::KEYS` with a `text()` arm; a `checks` refusal text
  is pinned by core tests over BOTH strings files. Prose in a constant whose bytes are DATA is data.

### INI, config, the manager, packaging

- A new `[Server]` key carried by the web UI touches several places (the `ServerConfig` field,
  `from_keys`, the web key list, the const-generic array length, the strings, the bff's off-list test);
  a key moved onto `WEB_MODEL_PROFILE_KEYS` reddens the bff's exhaustive off-list test. A model profile
  key moved between the two `checks` lists reddens two pins no bff test names: `pai-app`'s
  `web_key_lists_are_exactly_what_render_section_writes` (the union of the two lists is what
  `render_section` writes: a key `pai-app` never writes cannot join either list without the tester's
  change, and a key it starts writing needs a NON-EMPTY value in `full_model_profile` and in the
  `form.rs` rich fixture (both full struct literals; an empty value makes the round trips vacuous) and
  the bff's `save_key_outside_the_web_list_is_400_and_reaches_no_manager` (it lists `model`).
  `ServerForm` is a Slint struct: a `ServerConfig` field needs its mirror. So is `ModelProfileForm`: a
  model profile field the GUI has NO widget for (`hf-repo`, `hf-file`) still rides it in BOTH
  conversions, or a GUI save of a profile the web wrote drops the key, and `form.rs`'s
  `rich_model_profile_round_trips` reddens. One shipped engine default file lives in several
  places (`crates/core/defaults/` and the package tables), pinned a third way by the bff's server test.
- A model profile key `render_section` makes OPTIONAL (written only when its value is not blank, M3b's
  `model`) keeps its own comment line INSIDE the same guard, and the reader treats the key's absence as
  the blank: a blank `model =` line beside `hf-repo` reads back as a second source, which the web's save
  refuses (`model-profile-bad-source`) and `llama-server` takes as an empty `-m`.
- A merge that appends sections must SKIP what the base already holds (read it back with
  `ini::text::section_text`) and must not trim the base's trailing blank lines when it appends nothing:
  a writer that trims and re-appends an untouched input reddens the rerun's "answers byte for byte" pin.
  Normalise the appended text to the base's `detect_eol`, never to a literal `\n`.
- A migration's "already migrated" gate keys on the PRESENCE of the new marker (every section carries
  `InstanceProfile`), never on the absence of the old one: an old record that carries an address and
  names no profile is not migrated. A crash-point rerun reads the file it already wrote as the base
  (a profile section on disk wins over what the stripped pool could rebuild: the pool has lost the
  moved keys), and never invents a value from an input that already lost it (a server profile whose
  `Port` line moved out gives the record no `DEFAULT_PORT`).
- A migration fixture that proves "a backup that stands is never overwritten" puts its sentinel on a
  name the tree has NO live file for (a file the migration only backs up, never writes, is overwritten
  by its own backup): a sentinel on a file the seed writes collides with the same block's pin that that
  backup holds the seed's bytes.
- A start-time backup is keyed on the TEXT the start writes for the file: one that already holds it is
  not an old-shape file and gets no `.2.99.0` (the migration is idempotent, so backing up the new shape
  as the install's text is a lie, and `instance-profiles.ini`, created by the first start, has no old
  text at all). The copy goes to `<name>.2.99.0.tmp` and is renamed, so a death mid-copy leaves no
  truncated backup for the next start to keep, and a failed copy leaves no entry.
- An idempotent-start pin ("a second start adds no file and changes no byte and no mtime") needs the
  skip in EVERY writer that runs at the manager's start, not just the migration's: `render_all_locked`
  renders every instance every start, so `models@<id>.ini` moves both ways. Gate the render's skip on
  the record being migrated: a tree the migration has not written is rendered as always, because the
  phase that pins a render that failed at that start needs the write attempted.
- The server profile files the migration strips the address out of are named through
  `paths::instance_server_profile` (the record section's `ServerProfile` while unmigrated, the instance
  profile's from the migration on), never by reading `ServerProfile` off the record: a tree that stopped
  with `instances.ini` already written has no such key left, and reading it names no profile, leaving
  the address in the file the clean migration strips.
- A migration fixture names keys from `checks::PLACEMENT_KEYS` and nothing else (core's own fixture uses
  `n-cpu-moe`, `device`, `ctx-size`): a rule keyed on "this model has a moved key" (the `unassigned`
  profile) never fires for a section whose only extra key is not in that list, and a list membership a
  fixture implies cannot be added on the strength of a doc line, the union of `WEB_MODEL_PROFILE_KEYS`
  and `MODEL_PROFILE_KEYS_NOT_ON_WEB` being pinned twice (core's key-set test, and `pai-app`'s
  `web_key_lists_are_exactly_what_render_section_writes`). `ARCHITECTURE.md`'s moves list names
  `main-gpu`, which is no key of this project at all, so a pin on it has no production fix.
- `set_key` and `remove_key` keep the section's trailing blank separator line (a trimming writer eats it);
  `parse_sections` drops key lines before the first header; a section copied from a file with no final
  newline needs one appended; `ini::detect_eol` styles a writer's appended section. Writers outside core
  use the shared helpers.
- `ini::text::delete_section` normalizes a CRLF file to LF, and `set_key` writes the whole file in the
  style it detects. A module that rebuilds whole sections (`instance_profiles.rs`) strips the section,
  splices its own lines, and re-applies the file's `detect_eol` to the spliced text (its `normalize` +
  `append_section` pair), the way every `ini.rs` writer carries a "preserves lfs-only files" test.
- `assign::render` is a FILTER: an assignment naming a section the pool lacks is `Err(NotFound)`, which
  the public `render()` answers as the EMPTY text, which the GUI's `rows_of` reads as "every on-profile
  model". So a per-instance model list derived from a render's sections cannot exclude a model the pool
  holds: the empty render spreads it over every instance.
- A call to the running engine is `pai_core::http::request(port, method, path, body, timeout)`: the
  hand-rolled loopback HTTP/1.1 client (`Connection: close`, the timeout is the WHOLE request's
  deadline), so a poll needs no new dependency; `request_host(host, ..)` dials the address the caller
  names, and the caller maps the bound-everywhere `0.0.0.0` and `::` to `http::LOOPBACK` itself; its
  `Host:` header brackets an IPv6 host (bare, the last colon reads as the port separator). A
  unit-command verb (`wait-ready`) takes its `/proc` read, clock, sleep and HTTP read as arguments, so
  a test drives the 2 s interval and the 10 min deadline in 0.01 s. Its target is read AGAIN every
  round (the unit is `Type=exec`, so the first `/proc` read is `pai-manager run-instance <id>`), and
  ONLY an argv whose argv[0] basename is `paths::binary_name("llama-server")` is asked anything: the
  LAST space-form `--port` (the `--port=N` form the framework never writes names no port, so the
  default `server_cfg::DEFAULT_PORT`, as `slot_save::slot_target`, and that default is the fallback of
  the ENGINE's argv, never of a pre-exec one) and the `--host` the engine bound. A pre-exec round asks
  NOTHING and is counted not ready, retried to the deadline: a listener on the default port is another
  server, not this instance. Only a `$MAINPID` that is unset, no number, a pid that is gone, or an
  engine `--port` that is no listener fails at once.
- A record file the migration rewrites (`instances.ini`) is edited IN PLACE with
  `ini::text::set_key` / `remove_key`: a re-render from `parse_sections` drops the file's comments and
  the order of its keys, both pinned. In `assign::render_profile` the entry wins: the pool section's own
  line of a key the entry names too is dropped (`ini::line_starts_with_key`, a key line only, never a
  comment) and the entry's line appended, so the key reaches the engine once, with the entry's value.
- A record (`assignments.ini`, `instances.ini`) is written through `write_served_file(..,
  &ConfigFile::Models, ..)`: mode 0644, temp file renamed over. A render written after a record change
  turns its failure into the call's answer. `assign::render` takes both pool files, so a profile in
  `models.ini.disabled` is in effect. An instance IS a section of `instances.ini`.
- `paths::instance_ids()` filters ids through `valid_instance_id` (lowercase, digits, `-`): the address
  and the port walks take it (`check_binding`, `proposed_port`, neither taking a record text), because
  a section whose name is no id is NEVER run (`launch::instance_command`'s `InvalidId`), so it holds no
  address. A manager fixture id with a `_` is invisible to those walks: the fixtures keep valid ids
  (`wide-one`).
- A per-instance render rule goes in `manager/service.rs`'s `render_of` (the one place that knows the
  instance: `instance_devices` reads its server profile's `Devices`, `paths::instance_server_profile`
  the profile file). A Linux-only rule needs a `#[cfg(not(target_os = "linux"))]` twin of the same name
  so `render_of` calls it on Windows with no `cfg` at the call site. `ini::text::set_key` answers the
  text UNCHANGED when the section is missing: a rule that creates a section writes the header itself.
- `versions::installed` lists backends ONLY when the engine's own `llama.cpp` file sits in the folder,
  and `bff_cfg::versions_dir()` is `None` under the test redirect (its Linux arm wants
  `/etc/llama.cpp`): a manager read of the installed backends needs a redirect-aware fallback, and a
  fixture writes the engine file beside every backend file.
- A manager source-text lint (`tests/gt5a_windows_gate.rs`) reads each `src/manager*.rs` production
  region up to its FIRST `#[cfg(test)]` line, so code added after that (the e2e phases) is out of its
  scope, and the gate it reads is the `#[...]` block directly above the item (doc comments are dropped,
  so an attribute under a doc comment is seen, one under a `let` is not).
- Manager verbs, refusals and records: e2e phases are `pub(crate) fn` called in sequence by ONE `#[test]`
  (the GUI's `save_flow::run` likewise); create/remove verbs have no test of their own. A value check the
  manager answers needs an arm in the bff's 422 field map. A shard-count bound is name math
  (`gguf_shape::split_shard_suffix` gives `(base, index)`, NEVER the total: read it from
  `stem.rsplit_once("-of-")`). `manager/service.rs` has no module-scope `use std::path::Path`, so a free
  fn there writes `&std::path::Path`. Judge a
  write by the render it produces (the startup count only when the render starts MORE than the one on
  disk; a profile switch changes no model profile). The model-file check is a DELTA. A probe cache keeps
  the parsed rows; a serial probe answers queued readers with the probe they waited for.
- A manager rule is pinned a second time by the refusal tables of `manager/dbus.rs` and `manager/pipe.rs`
  (the wire phase lists `(args, kind, needle)` rows). Change the rule and those rows are the last reds of
  `data_root_e2e`, one row at a time, and a row that pins the case the new rule accepts is a witness pair
  with the phase that pins the new rule: report the pair, change no test.
- A verb added to the transports rides BOTH clients' acting frame, the READS included:
  `crates/core/src/client/pipe.rs` builds it with `self.guarded(method, &[])` (the `As` method name plus
  the `principal` field) and `crates/core/src/client/dbus.rs` with `match self.acting()`. A read written
  as a plain call answers an acting client's read on the CALLER's rights, and the bff's own connection is
  authorized, so a non-admin `As` read returns `Ok` and only the `asked_admin` count shows the call asked
  at all.
- The address rule of IP3 (port option P1, `ARCHITECTURE.md`): `Hostname`/`Port` live in the instance's
  record section, `AddressInUse` is judged on the records alone, and "any number of instances on one node
  may share an instance profile or a server profile". So `CreateInstance`'s `ServerProfileInUse` fires only
  for a record section that runs the profile AT the profile's address (a `ServerProfile` key and no
  `Hostname`/`Port` of its own, the pre-IP3 shape, which is what the IP2 refusal tables seed), and the
  create's `check_binding` walk skips every section that owns an address; the create writes
  `Port = proposed_port` (the lowest port from 8581 no record section holds) and no `Hostname`, so the
  profile's bind stands.
- Linux bundle loop (the GPU runtimes): every directory of a dist that is mirrored under
  `RT_DIR` needs its own `RUNPATH` entry, and a runtime the dist reaches through a symlink
  of its own (`lib/llvm/lib/libomp.so` naming the file in `x86_64-unknown-linux-gnu/`) stages
  in the LINK's directory with the file's bytes (`rocm_stage_path`): the file's own directory
  is off the RUNPATH, and the installed engine fails with "libomp.so: cannot open shared
  object file". The link counts only when it canonicalizes to that file (a dangling one
  leaves the file's own place). `components::of` hands `llvm/lib/libomp.so` to llama.cpp,
  and the bundle loop accepts an owner of Llama beside the dist's.
- Packaging: `components::of` is a PREFIX ladder (a rule for a file inside an owned folder must come
  first); `windows/package.rs`'s `bundled` map is keyed by the dist label; the engine revision is
  `components::LLAMA_CPP_REVISION` and a bump moves the packages' Depends. Pin a file that must land in
  one package by reusing the test module's helpers.
- Linux system files: `system_files::files()` is the list of PLAIN TEXT files and `stage()` writes each
  as 0644, so a symlink (a target's `multi-user.target.wants/` link) is linked OUTSIDE that loop, is not
  in `files()`, and needs its own named line in rpm's `%files` (a bare system directory line is refused).
  The engine's unit runs the framework's binary, so the engine's `.deb`/`.rpm` carry the version bound on
  the core, from `version::framework()` (the same read `run` does before staging).

### The bff (Rust)

- Every body route gates and parses with the one helper (`session::json_body`), answering the route's own
  `*-malformed` 400 first; the session is read before the body (an over-cap body with no session is the
  session's 401); a write route with nothing to show gates the body BEFORE connecting. Framing headers
  are one layer; a per-router middleware takes its facts through `from_fn_with_state`.
- Answers by one mapping: a write's timeout answer is the shared 3-arm mapper (`timeout_answer`), `done`
  is built from what the route asked for, never from a read it can no longer make. A write that crossed
  and lost the manager is 504; a failed connect or a read is 503; a manager `NotAuthorized` on an `As`
  route is 403; the manager's keyed refusal is relayed with its own `key` and `values`; the two-limit
  write shape runs ONE blocking task under `timeout`.
- A save's post-write state read never `ok().flatten()`s the record read; the calls list of a save ends
  `list, status <id>` (ask `status` on a FRESH `connect(None)`); a page-load route asks the manager no
  `devices`; a polled read that acts as the signed-in user ends the session by the session's id and
  leaves its idle deadline alone.
- A route's BODY-SHAPE gate outranks a record-resolution pin. A list item's extra fields come from rows
  found once per id above the loop. Fakes implement only the trait methods their routes call, the rest
  are defaults.
- An additive wire key reddens every whole-row `json!` literal: list them for the tester. Prove the list
  in a copy with one regex on the id pair (`("id": "<x>", )(?="engine")` inserts the key) and count
  literals apart from tests: 21 literals in `tests/instances.rs` and 10 in `tests/origin.rs` reddened 11
  and 13 tests, so the report is longer than the literal count and no assertion changes.
- A refusal with no control to mark (a state: 409 with key and args, no `field`) needs no arm in a
  422-with-field mapper: `control::action_failure`'s `Invalid` arm IS the keyed 409, so the mapper guards
  `key != "<that key>"` and falls through to it.
- A write on a path a read already owns is added beside the existing handler. One `Host` rule serves both
  address families (strip the brackets and parse `IpAddr`). An axum handler takes `Path` first and
  `Bytes` last.
- `crates/bff/build.rs` embeds the web UI from `PRIVATE_AI_FE_DIST` (absolute) or the fixture.
- Every public router builder funnels into the one private `router_with_policy`, and the session store is
  built there: a route with an injected dependency takes ONE more parameter there and the production
  runner at each of the five other builders, so it is served by the service and by every test router.
- A fake that answers from one shared FIFO queue cannot serve two concurrent callers in the canned order:
  a concurrency test whose answers are `[refs, tree, refs, tree]` and which asserts both listings answer
  200 is racy (the second listing's first call pops the tree body, which names no commit). Prove it in a
  copy over N runs and report the count; no production shape fixes it, and serialising the calls breaks
  the test's own "two in flight" assertion.
- After a change, look for stale text: the route list in doc comments and the brief's own failure lines
  (a 503 the design later made 504) can be wrong; follow the design, report the stale pin.
- A concurrency slot is held by the async handler, never moved into `spawn_blocking`: the timeout path
  drops the handler, its permit goes with it, the abandoned task keeps none (hf.rs, the G1 pin).
- A cache keyed by repo keeps what no caller owns (the tree: commit + files), never the built answer:
  every request applies its own tag and file with `build_listing`, so no caller's tag leaks (hf.rs D1).
- The `Hf` state is built ONCE per router (`hf::state`) and handed to the picker's route and to the
  model profile save, so one cache and one slot pool serve both; the route's body is `hf.tree(repo)`
  and the save's check is `file_is_listed`, never a second copy of the fetch. The save's `hf-file`
  check runs when a file is present and the repo OR the file changed (a file kept beside a NEW repo
  was listed in the OLD one), and answers `hf-unreachable` / `hf-tool-missing` at 503 through
  `Refused::save_response` (nothing was handed over to write, so no 504).
- A test reads one function's body as text (`fn_body`): the caller of a builder seam names no
  `Command::new` and no `.args(`, so the seam builds the command and the caller only spawns it (hf.rs G2).

### The web UI (fe/): running and CSS

- Run a filtered file as `npm --prefix fe test -- <file> -t '<title>'` (cwd `fe/`, so the jsdom config is
  read). The filter is matched against the path RELATIVE TO `fe/`: `-- fe/src/X.test.tsx` matches
  nothing and exits 1 `No test files found`, write `-- src/X.test.tsx`. `format:check` writes nothing: fix the named files with `npx prettier --write <file>` run from
  `fe/` (from the repo root `npx --prefix fe prettier --write src/x.tsx` answers "No files matching the
  pattern": the paths resolve against the cwd, not the prefix; it lists
  every file it processes: check `git status --porcelain` after). Prove a prettier rewrite of an
  UNTRACKED file with a copy INSIDE `fe/src/` (config resolves): expand your edit, `npx prettier
  --write src/probe-tmp.tsx`, `diff` it with the real file, delete the probe: a copy outside `fe/` is
  formatted with prettier's DEFAULTS and rewrites every quote and semicolon. The full suite under load
  reddens scores of files with `Test timed out in 5000ms` (vitest spawns one worker per file, 139
  here): rerun
  with `npm --prefix fe test -- --maxWorkers=4` (user, 2026-11-24: back to 4 after Strata hung under RAM and disk pressure; 12 was tried that day), and rerun a named file solo, before
  believing any of it. Green baseline today: 3536 tests, 143 files, 107 s at
  `--maxWorkers=4` (2026-11-27; 3487 tests, 139 files at `--maxWorkers=2` on
  2026-11-26). There is no `basic` reporter here (`--reporter=basic` exits 1
  `Failed to load custom Reporter`): the default reporter's `×` lines list the reds.
- The package is `fe/package.json`, there is no root one: a copy of the tree runs as
  `npm --prefix <copy>/fe test -- src/X.test.tsx` (`--prefix <copy>` exits 254 `ENOENT package.json`).
- A picked quant writes its tagged value (`<repo>:TAG`) into the repo field (22 P.9), so a test that
  reads that field after a pick expects the row's `hf-repo`, never the bare `HF_REPO` (22 C.7 line 526
  against C.6 line 463, one witness pair, the pick's own pin wins).
- New CSS is a test edit: `Reflow.test.ts` (`CLASSES`, `overflow-wrap`, `box-shadow`),
  `HearthTokens.test.ts` (`min-height`/`min-width` per rule, an exact ordered list), `HearthFonts.test.ts`
  (`font-family`) are exhaustive per property; a CSS test pins the SELECTOR as a string and a rule's
  "declares nothing else" list. Append (sorted) and say the title or comment is stale rather than
  rewriting it. A test-side CSS helper must start counting braces after the opening `{` at depth 1.
  `declares` compares the property ORDER as well, so a declaration added to a rule it pins sits where
  the tester's list carries it (22 D6: `position: relative` last).
  A pin of the form `decls.filter((d) => d.startsWith('x:')).toEqual(['x: v'])`
  (22 D5, D6, D7 in `ModelProfilesDelete1.test.tsx`) reads one property of one rule and pins no order:
  append the declaration last in the rule, with the one-line comment inside it above the declaration,
  the way 22 D6 put `position: relative` in `.table-scroll`. `contain` has no sheet-wide list.
  `white-space` and `text-align` have no sheet-wide list. R1.8's id list is a
  `Set` and its class list allows a card's own namespace: a new rule naming an id
  the sheet already selects, or a class of that namespace, is free; a new id
  outside it is red.
  `gap`, `display`, `flex-basis` and `align-items` have NO sheet-wide list: a spacing rule goes in the
  sheet, never an inline style (no test reads `element.style`, so an inline gap answers no CSS pin and
  the geometry test stays red, 22C2 A2). A radio label's "text wraps to the right of the radio" is
  `display: grid; grid-template-columns: auto minmax(0, 1fr)`, and one label per line inside the
  wrapping flex `.field-control` is `flex-basis: 100%` on a class of the allowed namespace (22C2 A1:
  the sheet's `hf-` classes are free, R1.8's `ofQuants`).
  The browser's own margin on a native radio adds to the label's gap (the text lands 10 to 11 px after
  the box, not 8): zero it on a rule whose subject IS the input, `.field-control label >
  input[type='radio'] { margin: 0 }` (the checkbox's precedent), NOT inside `:has(...)`, which styles
  the label. `font-size: inherit` there is red in HearthTokens H1a.7 (every `font-size` outside `:root`
  is a `var(--fs-*)`), and `1lh` on a native input is ITS UA font's line box, not the label's text
  line: a first-line offset for the radio is `margin-top: calc((var(--lh) * var(--fs-body) - 1em) / 2)`
  (22C2 re-check D2 and adviser note a).
- A field child the 720 px grid must place in the control column (22C1 review D1): wrap the children in
  ONE element and add one rule placing it INSIDE the one 720 px block that styles `.field` (the sheet's
  LS1 and TP1 rules are there). Two of the sheet's own pins bind that rule: 19 LS1 D1 takes
  `single(...)` of the rules whose compact selector is `.field` (no second `.field` rule), and 21 TP1 D1
  takes `single(...)` of the block's rules that declare `grid-column` and whose selector names `div`
  (name the new rule by a CLASS: `div.x` is the second one and reddens it). Render the wrapper only when
  it has content (an always-rendered empty one is a grid item: a 0-high row plus two gaps under the
  control), and a new class the brief does not name is allowed by R1.8 only inside a namespace the
  tester's `of…` regex carries.
- The sheet readers come in two shapes and each has a trap: `values(decls, 'x')` STRIPS the property
  (`d.slice(prop.length + 1)`), so its expectation is `['v']`, and an expectation written
  `['x: v']` is unsatisfiable by any CSS (22D4 amendment: `ListDeleteTip.test.tsx` 333, 601). A
  `decls.filter((d) => /^x:(a|b)$/.test(d))` must spell the space the sheet writes: `/^position:
  (absolute|fixed)$/` never matches `position: fixed`, so the filter is empty and the assertion passes
  vacuously (`ModelProfileDelete3.test.tsx` 275, `ServerProfilePage3.test.tsx`). A `declsOf(sel)` that
  splits each rule's selector list on `,` and compares a part to `sel` EXACTLY never matches an
  argument with a comma: `declsOf('th, td')` is `[]` while `declsOf('td')` returns the `th, td` rule,
  so such an assertion is unsatisfiable by any sheet (22 D4.27: `ListColumns.test.tsx` 663).
- A heading's room scoped as `form > h3:first-child` is free of every sheet pin: the
  cascade readers drop pseudo-classes (`compounds` reads `form` then `h3`), the rule names
  no class (R1.8's class list untouched) and declares only `margin-top` (no exhaustive
  property list reads it). A bare `h2, h3 { margin-top }` pair is not: it outranks the
  sheet's heading rules, which declare the `margin` SHORTHAND, so it moves every h2 on
  every page and gives every h3 a room it never had (17c re-check D4).
- Sharing a rule by adding a class to its selector list is red when the rule is pinned by
  `topBy(tight('.x'))` (HearthH8b.5 pins `.run-actions` by its EXACT selector, and the tester left
  it alone): carry the class on the element instead (`restart-offer-buttons run-actions`), which the
  cascade readers read as that rule's own, and the sheet gains no class (17d review D1).
- An element whose class is outside R1.8's namespaces (`ot|tensor|placement`,
  `instance-(rename|label|name)`, `instance-models`, `restart-offer`) gets its rule from the tags
  and classes the sheet already names (17d adviser note 1: `.field-control select + button`), and
  the rule carries no `min-height`/`min-width` (HearthTokens H1a.12 lists every one, in sheet order).
- `box-shadow: var(--shadow)` is pinned as an EXHAUSTIVE list in two places at once
  (`HearthH2b.test.ts` H2b.6, `Reflow.test.ts` 10 R1.6): a new lifted box needs its selector added to
  both lists, so a brief that lifts a new box leaves those two reds for the tester.
- `topStyling(sel)` (exactly one top-level rule per selector) and `Reflow.test.ts`'s
  `declaring([...])` ("declares exactly these properties and nothing else") read the SAME rule: a
  selector pinned by both cannot gain a declaration, and the pair is a test-side conflict to report.
- A forced-colours rule joins an EXISTING `@media (forced-colors: active)` block (the count is pinned
  exactly, five) and its colour must be in `HearthTokens.test.ts` H1a.12's `SYSTEM` list: a system
  keyword the list does not carry (`ButtonText`) is red there, and the rule must sit after the
  top-level one it answers, a media query adding no specificity.
- From 1280 px the page's `section` with a jump list is a grid and stretches every direct child
  (`section:has(> .jump) > *`), so a page-level button there is a pill across the column: wrap it (and
  its confirm) in one `div`, the grid item, and the button keeps the width of its words. A `#id {
  justify-self: start }` rule does the same but is red in R1.8, whose id list is a fixed set.
- State by CLASS, never the `hidden` attribute, when a wider window must override it; scope a shorthand
  rule by renaming its selector.
- Reusing one list-add class (`.ot-add`) on another page is free: R1.8's class list allows the card's
  whole namespace, and HearthTokens' size filter `/ot-|tensor|placement/` reads only `min-height` and
  `min-width`, so a `margin-top` there is unpinned and a new size there is red. A page with no `.jump`
  child is not the 1280 px grid, so its page-level `+` keeps the width of its words. The sheet tests
  that read a heading inside a box (`rulesInside('h1', classes)`) resolve `h1` through the GLOBAL rule
  only, so a `min-width: 0` for a heading made a flex item goes on the global `h1` rule, and joins
  HearthTokens H1a.12's exact ordered list of every `min-height`/`min-width` in the sheet (17f2.9).
- A compound-with-combinator rule (`p + .ot-add`) is free of the cascade pins too: the sheet tests'
  `cascade(classes)` keeps only a selector whose every token is `button` or one of the element's own
  classes, so it never applies, and the element keeps the plain `.ot-add` declaration in those pins.
  So a declaration a cascade pin READS must sit on a rule naming only that element: write the status
  row as `div.page-status` and `p.page-status` (two rules), never `.page-status p`. A `td` whose layout
  a pin reads (a flex row for a new button) needs a class of its own (`.status-cell`): a
  `.model-profiles td:nth-child(3)` rule is invisible to that reader, and a `.confirm-row td` rule is
  free of it (a descendant selector), which is where a confirm cell's `white-space: normal` goes.
- A pin that a control is NOT stretched (`RestartOffer17d.test.tsx` 17d.6a, re-check D3) uses a
  SIBLING-aware reader, `rulesReaching`: `cascade`/`scopedBy` read every earlier compound as an
  ancestor, so a `+` rule never reaches the element and an absence pin through them passes with
  nothing to check. It reads the `@media` rules too and takes a `flex` shorthand's basis as its third
  token, so the only fix is to delete the rule (and removing a rule an uncommitted slice added leaves
  the file byte-identical to HEAD: `git status` clean for it is the check, not a lost edit).
- A brief that says "delete this rule" and a tester that pins the rule's DECLARATIONS: delete only the
  declaration the test reads and keep the one that carries the layout, and say which reading you took
  (22 D3 of 2026-11-27: `flex: 1` out of `.card-heading > h3`, its `margin: 0` kept, since a
  `margin-bottom` on a flex item shifts the title off the row's `align-items: center` middle). Both
  readings go green, so the brief's literal wording is not the tie-breaker: the geometry is.
- `ListColumns.test.tsx`'s `cascade(el)` reads a rule that ENDS in the element: a pseudo-class on a
  NON-last token is dropped and `>` is read as a descendant, so scoping the Actions rules to
  `tr:not(.confirm-row) > td:last-child` leaves them reaching the confirm td in that read (22 D4.4, L1
  review D1). Raise the confirm cell instead: add `.confirm-row td:last-child` to the selector list of
  the `.confirm-row td` rule (the plain part keeps `declsOf('.confirm-row td')` reading it) and put the
  overrides there; the reader's spec is the count of `.` and `#` per part, so a one-class part ties with
  `.model-profiles td:last-child` and the later source order wins, as it does in a browser where both
  are (0,2,1).
- Moving a shared component (the Save bar: one `SaveBar.tsx` used by both form pages) reddens every
  HEAD test that pins its PLACE, not its behaviour: the tab-stop lists (`2d.67`, `2d.71`, `2d.78`,
  `2e.91`, `H9b.4`, `H10.10`, `H11b.4`, `2h.197`, `1b 6`, `6`), "the bar is the section's last child"
  (`2h.135`), "the bar's buttons are the last controls" (`2h.141`), and the bar's own geometry
  (`H2b.4`: `bottom: 0`, `border-top`, the top corners). The tester retargets the ones the brief
  names; the rest are test-side. Prove each in a `git archive HEAD` copy with the slice's production
  files copied in (`ln -s` the real `fe/node_modules`): flip ONE production bit back at a time (bar
  last, CSS at the bottom, row inside the section), the named test goes green, that is the proof, and
  no test in the repo is touched.

### The web UI (fe/): shared helpers and pins

- Shared logic lives in ONE exported function both pages import (default-box mechanics in
  `fe/src/boxed.ts`; a rule that spans a group is data on the row; the send rule of a checkbox row is
  data `onSend`/`offSend`); a helper shared by two pages takes each caller's rule as a named parameter.
  Per-field checks of a save run BEFORE the `!dirty` guard. A new 422 refusal key needs a `case` in each
  of the two tables (`REFUSALS.text` and its sibling), and the tables are PER PAGE: the Server
  form's carried no `not-one-of` while the model profile page's does, so a manager bool refusal
  there showed `api-other` and marked nothing. The bff's `field_of` names `field` for every server
  422, so the Server form needs the `text` arm only, no `field` fallback.
- Making a read-only card editable: amend the shared list under `fe/src/test/` that spells the editable
  set and derives tab stops from it (one fix point; an editable number field is two tab stops). On the
  model profile page that list is `MODEL_PROFILE_CARDS` in `fe/src/test/modelProfileCards.ts` (`fields`
  per card, `extraStops` for a control that is no 2e field): one new `Field` there reddens ten tests
  until the tester adds it: `2d.67` (ModelProfilePage, the card button list), `2d.71` and `2d.78`
  (ModelProfileContent), `2h.130` (ModelProfileSave1, which sweeps every `[id]` control and reads its
  enabled state from `EDITABLE_BY_ID`, so an unknown id is expected DISABLED), and `2e.91` in
  ModelProfileCards1 to 5. A card of a new sub-slice that adds a field is that list's job.
- One new focusable control reddens by one position every document-wide "tab stops are exactly" test
  (a page's, or the banner's): amend the tester's lists, never production. The lists are not only
  page-wide: a card owns one too (`GD1.12` in ModelProfileGpu3, `21 T.9` and `21 T.13` in
  TensorPlacement1/2, the page start in HearthAssetsEngine), and each reads only the controls inside
  ITS card, so a control added to a card head (a card's own About toggle, 22 D4.31) is the card list's
  FIRST entry, while `fieldTabStops` / `cardButtons` of `fe/src/test/modelProfileCards.ts` already
  carry it. Both shapes must be amended, never the control's place.
- Each of those lists names the controls ITS OWN way: a `nameOf`-based list names an input by its
  `aria-labelledby` (a switch named by the h1 appears as the profile's id), a textContent-based one gives
  an input `''`, and HearthJumpRail compares the elements. Amend each list with what ITS mapping yields.
  `pageTopTabStops()` in `fe/src/test/modelProfileCards.ts` is the shared one; the inline lists sit in
  HearthAssetsEngine, HearthJumpToggle, ModelProfileSave16, ModelProfileSave2 and ModelProfileDelete2.
- A control of the model profile page that is no 2e field must be skipped by `2h.130`
  (ModelProfileSave1): it sweeps every `input, select, textarea` and expects an id it does not know to be
  natively DISABLED. A `[role="status"]` outside the save bar reddens `2d.79` (ModelProfileContent),
  which pins the statuses outside the bar as the card move region alone.
- `SaveBar`: the Delete's reason line AND its `aria-describedby` follow `deleteHint`, not
  `deleteDisabled`, so a page that holds the verb for a reason with no words passes no hint
  (24 A2 review D2); Save's extra hold is the `saveDisabled` prop (a write of the page's own,
  24 A2 review D1), Discard keeps `!dirty || saving`, and the page's `save()` refuses too
  (an `aria-disabled` button's onClick runs).
- A strings key is pinned in THREE places: `1a strings` (App.test.tsx), `5b.11` (ProfilesNav) and the
  slice's own strings test (e.g. `22 D2.10` in ModelProfileDelete2). A key added or removed needs all
  three.
- `strings.test.ts`'s "every value is used" scans the code for `t(` IMMEDIATELY followed by a `'literal'`:
  a key picked by a ternary INSIDE the call (`t(on ? 'a' : 'b', id)`) is invisible to it, so the key is a
  "value no code uses" and the test is red. Write `on ? t('a', id) : t('b', id)`, the repo's style
  (`InstancePage.tsx`'s `run-timeout` pair).
- A 401 on a page read while the app believes it is signed in signs out with `signin-session-ended`;
  `signin-required` is ONLY a first load signed out (pinned by `InstancePage.test.tsx` 2.2's comment and
  assertion). A slice's own 401 test that expects `signin-required` mid-session is a witness pair with it.
- A Save check keyed by a control id must gate on that control being rendered (a Layers text the row
  keeps while its kind hides the field): a mark on an id with no element is invisible and `focusMarked`
  has nothing to focus.
- The in-flow combobox (brief 22 P.3, the 22C1 clarification): `listOpen` starts FALSE and the folder's
  reply lands without opening it; open on Down, an edit, a click in the box, close on Escape, a pick,
  `onBlur`. A pick-as-selection survives a reopen (`shown` keeps the whole list while the text equals
  the picked path). A real click on an option never blurs the box (its `onMouseDown` preventDefault),
  and a test's `focus(); click()` pair does blur it and still picks: React dispatches the click on the
  now-`hidden` option.
- A Create check that must block the send and mark nothing (an empty Model file box, 22 C.4 pinned
  against a marked one two tests apart) needs its own flag beside the marks: gate `send` on the flag,
  not on the mark, and focus the field. A manager 409 that names no `field` (409
  `model-profile-name-taken` marking Name) is the refusal table's `field:` arm, not a new alert arm.

### The web UI (fe/): React and the harness

- jsdom fires no `focusin` for `el.focus()` on the element that already has the focus (one `focusin`
  for two `focus()` calls, proven with a document listener): a test that focuses a control, presses
  Escape to hide a focus-driven hint, and focuses the SAME control again expecting it back is a
  witness pair, the hide would have to blur the control. Prove it in a copy by adding `el.blur()`
  plus a flush between the two focuses: the file goes green, no production shape does.
- A wrapper owns its child control's hover and focus: React's `onFocus`/`onBlur` on the wrapper fire
  from a child's `focus()` (focusin/focusout bubble), `onMouseOver`/`onMouseOut` from a child's
  (mouseover/mouseout bubble), and `onMouseLeave` fires from `fireEvent.mouseLeave(wrapper)`, the
  non-bubbling event React listens for in capture at the root.
- A test file that renders needs its own `cleanup`.
- To print data from a scratch test (the default reporter hides `console.log` in a piped run): assert it
  against an impossible value, `expect(rows.join(' / '), 'PROOF').toBe('X')`, and read the received
  string.
- Write a comment about a `switch` group ABOVE the group's first `case` label.
- A control that moves optimistically and is written must be put back to the value it SHOWED, in the
  same handler, when the write is refused: a re-read that fails brings no value back, so the optimistic
  state sits on screen. And a re-read that fails must not replace the write's alert: use the functional
  updater (`setAlert((a) => a === null ? {…} : { ...a, signIn: … })`), which sees the alert the same
  handler batch set, and raise `signIn` off the RE-READ's kind (401/403), not the write's.
- A restart offer the NEXT write replaces (17d.5b) is cleared where the ANSWER lands (success: the
  reply's ids, `[]` included; refusal: `[]`), never at the send. The test that asserts the offer gone
  right after the write is recorded still passes: the fake bff's reply resolves in the microtask before
  the assertion runs.
- `flushSync` is the only way a committed state is readable in the next statement; an
  `aria-activedescendant` index over a DERIVED list must carry the list it was walked onto; a text field
  that may not read keeps its LAST READABLE value in the row state.
- An effect that should run "when X was raised" cannot re-arm off a counter that only rises (the second
  raise equals the first, the effect sleeps): do the follow-up call from the handler that raised it, in
  the same batch, so the alert and the fresh read commit together, before the row is gone.
- Guard the shared handler itself on the same ref the send sets (a disclosure's Cancel, its opener and
  its Escape all call one close), so the whole close, focus move included, is inert while the request
  runs.
- Name a ref a parent owns and a child WRITES `*Ref`.
- "The focus never ends on body" when the focused node leaves with the answer: keep the answer's words in
  a state that OUTLIVES that node (a confirm group unmounts when the read again takes the verb away),
  render them as `p[role="alert"][tabindex="-1"]` where the node stood, and focus it from an effect whose
  deps carry the read that lands. A test that reads `within(main).getByText(text)` wants that text on
  screen ONCE, so the group's alert and the surviving paragraph must never both carry it when the test
  looks.
- In a page split outer read + inner form (`ModelProfilePage`, `InstancePage`), put state that must
  outlive the FORM (a delete's 204, the focus move a 404 re-read lands on) in the OUTER component: a read
  that answers no doc unmounts the form. State that outlives only a confirm group lives in the form,
  which a re-read beside the form keeps mounted (a `reread()` that bumps `reads` and leaves `answer`
  set). The form's `key` is the sign-in count, NOT `reads`: a re-read keeps the draft, so an edit typed
  during a write is still there when the read lands. A rule that must adopt what a re-read brought keys
  on the DOC's identity (a render-time `seen !== doc` check) with a marker raised beside the re-read:
  the fixtures keep one revision, so a revision check never fires, and the marker stays down for a write
  that answered, whose own word is the value the page shows (24 A2 review D3).
- Reading helpers shared across pages point one way (no import cycles between readers).
- A section that reads on appearance and for every sign-in the page answers (`renewals`, as
  `InstanceModels.tsx` does): a module-level fn mapping the read's answer to the two state values, a
  `useCallback` applier over it, and `useEffect(() => { void read().then(apply) }, [apply, renewals])`.
  A `useCallback` that setStates called directly in the effect body (`void reread()`) is
  `react-hooks/set-state-in-effect`, and an effect with `[renewals]` alone needs the disable comment the
  repo already writes. Reuse the pool read of a list page (`readPool` of `Instances.tsx`) rather than
  writing a second one.
- A child with its own alert and Sign in: the alert carries `at: renewals` and is SHOWN as
  `alert.at === renewals` (a render-time derive, no effect, no `set-state-in-effect`), so the page's
  kept sign-in drops it; the child keeps the failed verb's id in a ref and hands it to `onSignIn`, and
  the page keeps that in a ref its `signedIn` focuses (`askSignIn.close(offer ?? saveId)`). A page with
  no sign-in counter of its own (a list) adds one raised beside `setAlert(null)`.
- A child that stays mounted while its data changes (a list's offer, no `dirty` gate) keeps its own
  `status` and `alert`: key it by the data's identity (`key={ids.join(',')}`) so a new one starts clean.

### The web UI (fe/): the UX check

- Run `npm --prefix fe/ux run check` (all three browsers on the BUILT `fe/dist`, build first, against
  `fe/ux/stub.py`, about 12 minutes): it proves wrap and sideways-scroll criteria. Carry in the stub's
  documents every value a control needs to render, or that control is unmeasured (the tag vocabulary is
  the API's). Its summary prints `axe: <rule> x<count>`, not the node: add a probe in a scratch copy of
  the check to learn the node.
- `fe/ux/` is in `fe/.prettierignore` (its own npm package, "checked by its own hand"): `format:check`
  never reads `fe/ux/check.js`, ESLint does. A state's `set` names `stub.py`'s `STATE` keys, and each key
  feeds only the route it was written for (`off=1` switches the ONE profile's document, the list keeps
  its rows), so a list state needs no key beyond `session=admin` when the stub's pool already carries a
  switched-off row. The per-state hooks are `click`, `open`, `hover` and `focus`, each called in the
  normal pass and in the text pass.
- Read the stub before changing it: `python3 fe/ux/stub.py fe/dist &` prints the port it bound, and
  `curl "http://127.0.0.1:<port>/__set?session=admin&pool=long"` then `curl .../api/profiles/server/default`
  shows what a state really gets. A review note saying "the stub answers X with an error" is a claim to
  measure first: `pool` switches only the pool LIST, every page read answers the same under every pool.
- A state's icon selector names a stroke NO other icon carries: the plus's `M12 5v14`, never its
  `M5 12h14`, which is the whole of the minus. A `click` whose selector matches two buttons is a
  Playwright strict-mode violation, in every browser, at every width.

## Pitfalls

### The shared tree

- `pkill -f PATTERN` and `pgrep -af PATTERN` also match the shell running them.
- A screenshot that comes out blank proves nothing about a GUI.
- The tree moves mid-task (other roles, a history rewrite, files erased, a "landed" test file that is not
  there). A failure quoting an identifier your own edit removed is the tree moving: re-read the whole
  file and apply what is left in one scripted pass. A tester's file can also go back to HEAD under you
  (`git status` shows it).
- A `cp ... && echo ok` piped through `tail` can hide a failure.
- The sandbox clock jumps: content, not mtime, is the state (`md5sum`, `grep -n` the exact text).
- A script that moves a block can drop the item header between it and its anchor.
- An `edit` of a whole function whose `oldString` stops mid-body leaves the old tail sitting after the
  new `}`: a duplicated block, and the compiler says only `unexpected closing delimiter: }`. Re-read the
  whole function after a large rewrite, before running anything.
- The `edit` tool intermittently misses text `awk` shows verbatim: append with a heredoc and check
  `git diff --stat`. After a rejected edit call, `grep -n "parameter>" <file>`.
- A comment that narrates the shape the design REJECTED ("byte identity, which the render does not
  keep") reads as a defect to a later reader and invites re-proposing it: the contract in code states
  what the code does, the rejected shape and its consequence go in `ARCHITECTURE.md`, which is where
  the design is kept.

### A cargo run

- `cmd | tail; echo $?` reports tail's 0.
- `cargo test -p <crate> --test a --test b` without `--no-fail-fast` stops at the first failing test
  target: the second target's `test result` line is missing, not green.
- `cargo clippy -p <crate> --all-targets -- -D warnings` builds the lib WITHOUT tests: an item only
  `#[cfg(test)]` code calls is dead there.
- `cargo test -p <crate>` compiles the test modules of path dependencies, so a new public field reddens
  every crate that writes a full struct literal (E0063).
- `serde_json`'s `preserve_order` is ON when `pai-app` is built in the same invocation and OFF in a bare
  `cargo test -p pai-bff`.
- `mod windows` is `#[cfg(windows)]`, so no file under `xtask/src/windows/` is compiled on Linux. A
  `pub fn` only Linux code calls is `dead_code` on windows-gnu. `cargo clippy -p xtask` prints two
  `xtask (manifest)` warnings that are not yours.
- Every phase of the manager's e2e sets the process-wide `PRIVATE_AI_DATA_ROOT`, so several probes in one
  run MUST use `--test-threads=1`: in parallel they read each other's temp dirs, and a probe that fails
  at an early assertion of a phase it never reached is that cross-talk, not a red.
- rustfmt: `use` groups sort a plain `use` before a `pub use` of the same prefix and lowercase items
  before `CamelCase` ones; consecutive `mod` lines are sorted; a one-line `if/else` past 50 characters is
written multi-line; a `return Err(format!(..))` at deep indent is written broken; a `push_str("<INI
  comment literal>\r\n")` past 100 columns is rewritten as a call whose literal sits on its own
  line.
- clippy: `s == ""` is `comparison_to_empty` (use `is_empty()`); `|| other()` as a fallback is
  `redundant_closure` (pass `&other`); `iter().any(|x| *x == y)` is `manual_contains`; a `Result` whose
  error carries an axum `Response` is `result_large_err`; a pub builder over 7 args takes
  `#[allow(clippy::too_many_arguments)]`; a tuple past `type_complexity` becomes a module-level `type`; a
  `///` essay followed by a blank line before the item is a lint; a test's `assert!(CONST > CONST)` is
  `assertions_on_constants`, and the assertion-neutral fix is clippy's own `const { assert!(..) }` block
  (condition and message byte-identical, the pin moved to compile time).

### Source lints and strings

- A word-internal rename map breaks `mod` file names, multi-item `use` lists, names interpolated in
  format strings, raw or multi-line strings and WiX component Ids (MSI identity, not prose); rewrapped
  prose defeats exact pairs.
- `binding_lint` reddens the whole `-p pai-config` run when `strings::t` keys and the language file
  differ. A new refusal key needs its arm in `checks::Refusal::text`, its keys in `en_US.toml` AND
  `cli.toml`, and passes `binding_lint`'s `cli_print_paths_hold_no_language` only when the strings
  carry no path: reuse an existing key (`not-one-of` names the offending key as its first value). It
  also needs the key in `checks::KEYS` (core's `every_refusal_key_is_known_and_in_both_strings_files`).
  `binding_lint` is a `pai-config` test, so a `-p pai-core` run never shows it.
- `pai-config`'s `names_lint::no_code_names_the_old_project` scans `crates/{app,bff,cli,config,core,manager}`
  and `OLD_NAMES` holds `join("llama.cpp")`, so a manager test fixture that writes the engine's version
  file reddens `cargo test -p pai-config`, naming a `crates/manager` line in a crate you did not touch.
  Prove it is pre-existing with `git show HEAD:<file> | sed -n '<n>p'` plus `git diff --name-only -- crates/manager`.
- A new `pai-manager` verb reddens `pai-core`'s `strings::tests::the_manager_usage_names_pai_manager`:
  it pins `manager-usage` by EXACT equality with the old verb list, so it and a manager test that
  requires the new verb in the same key are a witness pair no strings value satisfies. The core pin is
  the tester's to amend, and `cargo test -p pai-core` is the run that shows it.
- A new field in the Slint `ServerForm` struct reddens every EXHAUSTIVE `ServerForm {…}` literal in
  `server_form.rs`'s tests (they carry no `..Default::default()`), and `rich_server_config_round_trips`
  fails unless `config_to_form` and `form_to_config` both carry it: the tri-state pair
  (`form::tri_state` / `form::tri_bool`) carries an `Option<bool>` with no widget, like `tags`.
- A D-Bus signature pin is zbus's own rendering (a struct has no commas); a wire change with new REQUIRED
  fields reddens a sibling test in the same file whose fixture omits them. A signature a COMMENT states is
  provable in a copy: `panic!("{}", <Row as zbus::zvariant::Type>::SIGNATURE)` in a scratch test (core's
  `client/dbus.rs` tests pin rows that way, and a reply's own signature prints as
  `(sa(sttbs)b(sas))`).

### The bff (Rust)

- The test builders pass a 200 ms read limit, so a route bound by a longer limit needs the test's own
  pause.
- A `Box<dyn ManagerClient + Send + Sync>` does not coerce to `&dyn ManagerClient`: pass `&*boxed`.
- Setting `PRIVATE_AI_FE_DIST` reruns the bff's build script and recompiles every target of the crate.
- `server::FIELDS` is the whitelist for `server-malformed` AND for the 422's `field`: a new server key
  needs its `Field` entry, and `server::tests::the_values_keys_are_the_web_s_field_names` pins the
  exact key set of the GET body as a literal `vec!`, so add the key there too. Nothing in `fe/` has to
  declare a key for the API to read and save it (no test reads `fe/src/api.ts`).
- A new `Kind::Bool` field needs no bff gate: `Kind::reads` already rejects a string or number body
  with `server-malformed`, and `null` writes no line.

### The web UI (fe/)

- `npx vitest` from the repo root has no config and reds every test with `window is not defined`.
- `npm --prefix fe run <name>` where `fe/package.json` has no such script prints `No script found,
  pointing "npm run <name>" to "<name>"` and runs `<name>` from the PATH: in this workspace that is the
  OpenDesign CLI, whose token report (`Found 2 potential issues in src/index.css at 1554:23, 1558:1`)
  is NOT a project check and quotes lines of whatever `src/index.css` the cwd holds (2026-11-27). The
  fe checks are exactly `test`, `typecheck`, `lint`, `format:check`.
- A card's heading row (22 D4.31 review A2: the card's About beside its title) is a `div` beside the
  heading, NOT the `h3` itself and not the card `section`: `L2PageHead.test.tsx` A2 pins
  `toggle.closest('h3') === null`, `toggle.parentElement === h3.parentElement` and that the parent is
  no section, with `h3.nextElementSibling === toggle`. `Card.tsx` renders it only when there is a
  control, so a plain card keeps its `h3` as the section's first child. The stale pin this leaves is
  `HearthAssets.test.tsx` H11a.11, which counts the Assets page's cards as `section.card > h3`: the two
  cards that carry an About drop out, so the selector wants `section.card h3`.
- `HearthTokens.test.ts` H1a.12 pins an EXACT array of every sheet rule that sets a height,
  `min-height` or `min-width` (49 entries). A new rule that sets one of those three, even
  `min-height: 24px` on `.card-heading .about` or `min-width: 0` on `.card h3`, reddens it: reach for a
  property the registry does not watch (`padding-inline`, `margin`, `border-color`), or the entry goes
  in the registry with the tester.
- A button whose words swap by width (`SaveBar`'s Delete, 22 D4.30 review A1) is read by jsdom with
  BOTH spans, since no test reads CSS: `button.textContent` is the short word plus the full label
  ("Delete model profileDelete"). The tester's new pin (`L2SaveBar.test.tsx` 866-903) DEMANDS both
  spans, the short one's text `confirm-delete` and `aria-label` the full key, and its `words()` prefers
  `aria-label`; the 16 files of Tab-order and word pins (`nameOf`, per file, `textContent` only:
  `ModelProfileCards1` to `5`, `ModelProfileContent` 2d.71/2d.78, `ModelProfilePage` 2d.67,
  `ModelProfileSave2` 2h.141, `ModelProfileSave16` 2h.197, `ModelProfileDelete` 22 D.1,
  `ModelProfileDelete2` D2.2/D2.7/D2.8, `ListDeleteTip` 22 D4.21, `HearthJumpToggle` H10.10,
  `HearthAssetsEngine` H11b.4, `InstancePage` 6) ignore `aria-label`, so no DOM text satisfies both
  sides: `nameOf` wants the label first, as L2SaveBar's `words()` already does. Report it, never pick a
  side by dropping a span.
- A card-head control the tab-stop lists do not name (the card About toggle of 22 D4.31) must carry
  `tabIndex={-1}`: the tester's amended `fieldTabStops` (`fe/src/test/modelProfileCards.ts`) excludes it,
  and 2e.91 of `ModelProfileCards1` to `5` (503 tests) reddens the moment it joins the Tab path.
- What goes into the Models list row's naming span (`${uid}-name-${i}`, the `aria-labelledby` target of
  its switch) lands in that switch's accessible name AND in `row.querySelector('td').textContent`, which
  the list readers use as the row id (`idsIn`, `cellText`, `MODEL_LIST.find(m => m.id === text(cells[0]))`).
  A second line under the name (22 D4.35's engine) therefore reddens every file that names a row control
  by its id. jsdom applies NO CSS, so a line a media query hides is in the name at every width. The
  pre-existing pins 2c.38 and 2d.60 force the line INSIDE that span (the label's text must equal the
  cell's), so the names cannot stay id-only: count the files and report them, never touch the tests.
- A tab-stop assertion whose slice is one element shorter than the expected array can NEVER pass: a
  disclosure plus N entries is N+1 stops, so `stops.slice(2, buttons.length + 2)` against
  `[toggle, ...buttons]` is off by one (24 A2.1's switch takes a stop too). Check the arithmetic before
  hunting a production cause; report it. A test pinning `stops.slice(2, n+3)` is the shape that passes.
- A vitest file filter is a SUBSTRING of the path: `ModelProfiles` also runs `ModelProfilesFilter`,
  `ModelProfilesOverMemory` and `ModelProfileSave16`. To tell your reds from another slice's uncommitted
  work, build a second `git archive HEAD` copy with ONLY that slice's production files plus the working
  tree's tests: the reds that reproduce there are not yours.
- Vitest here has no globals and no setup file: RTL's auto-cleanup never runs by itself.
- The Server page's `sectionStatuses` (`ServerSave.test.tsx`) and `status()` (`ServerProfilePage1`)
  require the bar's `role="status"` line AFTER the section's `h2`, and inside `SaveBar` that line
  precedes `.save-actions`, so a test pinning the bar's Delete `before(button, h2)` and
  `closest('section')` null (the bar outside, above the section) is unsatisfiable with them: one
  witness pair, the tester's to retarget. Prove it by moving the page's `<section>` after `<Body>`
  in a copy: that test goes green, 73 of those two files go red.
- `fireEvent.click` does not move focus like a real click.
- A test's `type` (`fireEvent.change`) moves NO focus, and the tests' `press` (`el.focus()` + click)
  leaves the focus on what it clicked: an "the focus did not move" assertion after a `type` lands on the
  LAST `press`, so a helper that pressed Show quants and then typed in a field can never read the field
  as focused (`ModelProfilesNew5` 22 P.11, line 382). No production shape moves the focus on a change
  event without breaking the brief's own "the edit moves no focus".
- A `deferred()` gate wired to a READ option (`hfFiles: () => gate.promise`) never answers that read, so
  every assertion that its rows arrived is unsatisfiable; the gate belongs on the WRITE option
  (`modelProfileCreate`, whose default is 201). `ModelProfilesNew4` 22 P.9/C.5, line 543.
- A key the tester added to `EN` (`fe/src/test/bff.ts`) must join `en_US.json` for `1a strings`, and if
  no source file calls `t('<key>')` yet (its use is a later slice, as `hf-repo-kept` is 22 E.2),
  `strings.test.ts` "every value is used" is red with NO production fix: report the pair, the key leaves
  `EN` and the file, or its use lands in the same slice.
- A new fieldset of radios needs no new class and no id: `<fieldset className="field">` +
  `<legend className="field-label">` + `<div className="field-control">` (the `GroupField` shape) takes
  the 720 px grid placement the sheet already gives `.field-control`, and `Reflow.test.ts` 10 R1.8 and
  `HearthTokens.test.ts` H1a.12 stay green (22 P.9's quants).
- A cell pin that finds a text by "the first element in the cell whose `textContent` is X"
  (`ModelProfilesSwitch.test.tsx` 2c.39) returns the OUTERMOST wrapper carrying that text, so a brief
  that wraps a cell's controls in a `span` (22 D4.25) leaves it red: the wrapper is not the
  `aria-hidden` word and precedes the switch. The behaviour holds, the witness is the tester's to
  retarget.
- A Models list row read with raw cell text (`rows()` of ModelProfilesOverMemory, and 2c.38 comparing
  the element `aria-labelledby` points at) carries 22 D4.35's muted engine line, so `['qwen-27b',
  'llama.cpp', …]` and a label saying `id + engine` are unsatisfiable together with D4.35: the tester's
  `rowName(cell)` of `fe/src/test/bff.ts` drops `.muted`, and the designer's ruling of 2026-11-27 makes
  the id ALONE the name of the row's switch, link and delete.
- Specificity: `button` and `.secondary` are (0,1,0), `.jump button` is (0,1,1); `input[role='switch']`
  and `input[type='checkbox']` tie, so source order decides.
- A form that reuses the field-shape classes (`.field-hint` in 17f2) inherits the 720 px block's
  `.field-control, .full-value, .field-hint, .field-error` grid placement: the hint lands in column 2,
  the form gains an implicit second column, the label sits BESIDE the box at 1280 px and the hint's
  unbroken id sets the column width (sideways scroll at 320 px). The measured fix is the form's own
  `grid-template-columns: minmax(0, 1fr)` plus `.form > * { grid-column: 1 }` plus
  `overflow-wrap: anywhere` on the hint (17f2 review D1/D2). The `> *` rule is (0,1,0) and sits BEFORE
  the 720 px one, so source order hands the hint column 2 back and column 1 collapses (box 18 px at
  720 px): name the hint AND the error at (0,2,0), the specificity winning at any source order (17f2
  re-check D1b).
- The `EN` table of `fe/src/test/bff.ts` is a second pin of `en_US.json` (`App.test.tsx` `1a strings`);
  "every value is used" scans for `t(` + a string literal, so a key that reaches `t` otherwise reds it.
  A strings key id makes a good `aria-labelledby` target. A key the tester put in `en_US.json` but NOT
  in `EN` makes every render test naming it throw `no English for <key> in the test`: the fix is the
  mirror entry with the file's exact text (assertion-neutral, the tester's file), no production shape.
- Queries run before an async read settles: a test that reads a control a device read fills, or
  snapshots tab stops before the awaited element lands, is the TEST's race (`indexOf = -1`, a
  `Loading...` status quoted where a page's line is wanted). A page that returns `null` while its read is
  pending is not the defect. State the shape, do not touch production.
- The same race has a second shape: a page that heads with the ROUTE id while its read is pending
  (`InstancePage`: `instance === undefined ? id : instanceName(instance)`) makes a heading finder resolve
  on the LOADING render, so `openPage(opts, 'embed')` returns before the instance is known, and every
  control the read fills is `null`; the Overview's `h2` rides the tile skeleton the same way. Prove it in a
  copy by awaiting the control's own text, never by widening production.
- `en()` of `fe/src/test/bff.ts` substitutes `{0}`/`{1}`: a strings pin that compares the RAW
  `en_US.json` value of a parameterised key with `en(key, 'a', 'b')` can never pass, and the render test
  that names those args needs the placeholders kept: a witness pair to report (17f2.10 against 17f2.7).
- A save rule of "an unchanged text sends nothing" (16 17f2.6) reddens a refusal-text test that clicks
  Save on an UNTOUCHED box, whose text is the value the record holds: no request goes, so no refusal
  shows. The test types a change first.
- A field's read moved UP to its page so a test's synchronous `getByRole` finds the field is that same
  race answered in production: revert it, and prove the revert in a copy (the slice's files plus the
  field's own test file). `git diff --stat` over the slice's files is the check that nothing out of
  scope is left behind.
- `vi.useFakeTimers()` breaks RTL's `findBy*`; a test that holds a request and advances the clock sees
  only what the flush after `advance` renders. A fixture handing ONE `Response` to several readers of an
  endpoint starves the second (React runs a child's effect before its parent's); a count over one
  endpoint counts every reader.
- ESLint `no-fallthrough` reddens a comment written BETWEEN grouped `case` labels of a `switch` ("Expected
  a 'break' statement before 'case'"). `querySelectorAll` over a CSS selector yields `Element`, so a loop
  that calls `.focus()` needs `querySelectorAll<HTMLElement>`.
- The react-hooks lint answers `set-state-in-effect` for a state update that answers a prop change:
  derive it during render or key the component. The same lint reddens an effect that closes a form in
  the commit after its outcome: close it from the handler with `flushSync(() => setOutcome(line))` then
  the close, which is also what a test holding the form's node needs (it reads the node's LAST
  COMMITTED children once the form is gone, so the outcome must be committed while the form is open).
- `document.getElementById` is not generic (TS2558): narrow with `if (el instanceof HTMLInputElement)`
  to reach `value` and `setSelectionRange`.
- A criterion amended in ONE test file reddens every UNAMENDED test that finds the same element another
  way. A strings key that becomes a link's text (`instance-server-open`: "Open {0}") reddens each
  `getByRole('link', {name: <bare id>})` of that link in other files; a region moved in the DOM reddens
  each "above the table" assertion in the file that tests the verb living in it; a new control reddens
  each document-wide tab list; a read added on page load reddens each "reads it only when X opens"
  count; a save bar that gains a second `role="status"` (the offer's own) reddens each "one status line
  in the section" helper. Before implementing an amended criterion, grep all of `fe/src` for the key, the
  element's name and the region's place, and report the unamended files as test-side reds (17d: 11 of
  them, 5 files).
- `el.enabled` is not a DOM property (use `disabled`).
- A row whose `group` is `null` has no `<key>-box` / `<key>-control` id; an editable text field with no
  default box is a plain labelled field, not a `role="group"`. A jump target is named by a slug carried
  in the card's own data. A page-level `role="status"` is counted by the tests: a "...ing..." line is a
  plain `<p>`, and an announcement line carries `aria-live` only while it has text.
- In `fe/`, a `<fieldset>` IS `role="group"`: `GroupField` (and the `GpuSplit` fieldset) put 14 groups on
  a Server-form page, so a test's unnamed `within(main).queryByRole('group')` can never be `null` there
  and can never find one. Prove it by listing `getAllByRole('group')` in a copy. A `role="region"` with
  no name IS named by its contents, so a "right after the opener" node can answer a named query.
- An `aria-disabled` button is no shield, its `onClick` runs.
- The react-hooks `immutability` lint reddens `x.current = true` when `x` arrived as a prop (it trusts
  only a `useRef` local of that very function).
- A sub-slice's page-wide test helpers (`h3s(main)`, a "no other word" loop over `document.body`) are
  reddened by the NEXT sub-slice's new heading or nav label: the brief amends them; prove it in a copy.
  The same for a new focusable control in a disclosure's own file: a helper that clicks the opener AND
  awaits the form reddens every test that clicked the opener first (the toggle closes it again), and
  `openPage` awaiting the heading of a labelled fixture reddens every test that passes `label: null`.
  Prove each in a scratch copy (delete the click, pass the heading, add the `cleanup()` between two
  `openPage()` calls in one loop turn) and report each as its own defect.
- A new slice file's own opener helper that asserts the loaded state (`expect(queryByRole('table')).not
  .toBeNull()`, "the table is loaded") reddens EVERY test in the file that calls it with a fixture that
  has no such state (a failed read, `[]` rows): the test asserts the opposite two lines later. 14 of 16
  reds in 17e were that one assertion; the helper needs a tolerant twin and no production shape serves
  both. Prove it by deleting the assertion in a copy: the file goes green.
- A tab-order test that names a control `aria-label ?? textContent` names NO checkbox and NO text box
  (both have an empty `textContent`): its `indexOf` is `-1` and no production shape names them. A test
  that compares `at.every((i) => i >= 0)` with `order.map(() => true)` compares a boolean with an array
  and can never pass. Both are test-side: prove them in a copy.
- Two tests in one file can pin incompatible DOM for the same fixture: report the pair.
- A shared tree means shared output: one `npm --prefix fe test` tail can interleave another role's
  vitest run (its counts, a file that exists nowhere in this tree). Judge a run by the failing file names
  you can open.
- A test that clicks a control whose value comes from the device probe (Add rule's Goes on) without
  awaiting the probe's as-of line is the test's race: one production answers the pending read and the
  answered one. Witness pair: pending to the fallback, after the page's `landed` helper to the first
  probed id. Prove it in a copy by adding the await, never by widening production.
- Do not leave a duplicate listener (`useWarnOnLeave` takes ONE per dirty section; report dirty from the
  field's `onChange` and the save handler, in the turn that changes it).
- `fe/ux/check.js` clicks a page's verb by `#id` (`model-profile-delete`, `server-profile-delete`): keep
  the id on the element the click lands on (the − box), not on a wrapper, and give the check a state per
  page state it must measure.
- A test's `shownText` skips `.visually-hidden`: an icon button whose words are a
  `span.visually-hidden` passes "nothing is written on the box" (WCAG 2.5.3) and keeps its name.
  The same helper's `isHidden` is `closest('[hidden]')`, so a span a test pins as NOT SHOWN needs the
  `hidden` attribute: `.visually-hidden` alone is visible to it.
- A `[hidden]` ancestor takes a control out of EVERY `getByRole` / `queryByRole` (in
  `@testing-library/dom` `isSubtreeInaccessible` reads `element.hidden` and `isInaccessible` walks
  `parentElement`): no implementation lets a test name the Model file box while the copy radio is
  checked (`ModelProfilesNew1` 22 P.1, line 475, asks for that combobox and pins it hidden at line 460),
  and a helper that counts options filtering `!isHidden` sees none in a CLOSED listbox, so a test that
  counts options must open the list first (`ModelProfilesNew2` 22 P.3, line 249).
- A `<dialog>` rendered INSIDE a `<form>`: jsdom bubbles the sign-in form's `submit` up to the outer
  form, so clicking Sign in runs the outer form's action as well (`ModelProfilesNew3` 22 C.7 counted a
  second create until the Dialog became a SIBLING of the form).
- The react-hooks `set-state-in-effect` lint follows a setState through a component function the effect
  calls, even past its first `await` (it reddens `void readFolder()`): the read `useCallback` ANSWERS a
  value and writes nothing, and the effect's own `.then` writes the state, as `ServerProfiles.tsx` 124.
- A `role="status"` line a test compares with `.toBe` carries ONLY its text: a retry and a Sign in
  belong beside it, not in it (brief 22, 22 P.6). A create field's About toggle is named
  `${field-about} ${label}` by `aria-label` with the key as its visible words: the field's label may not
  be repeated in a `span.visually-hidden`, which `getByText(label)` pins as one match.
- The older list tests read a cell's RAW `textContent` (`rows()` in `ModelProfilesSwitch`,
  `ModelProfilesFilter`, `ModelProfiles.test.tsx`; only `cellText` clones drop `button` children), so a
  new icon button whose name is a `span.visually-hidden` lands in every one of those pins: one − in the
  Status cell reddened 26 tests across four files. The cell-text pins and the hidden-name-in-the-cell
  rule are a pair to report, not to satisfy by moving the name to `aria-label` (the brief forbids it).
- A test helper that finds a `+` by the plus icon's horizontal stroke (`PLUS[0]` is `M5 12h14`, the same
  path as the `minus` icon) counts every − as a list add; the rows' − is `trash-2` since 22 D4.21, so
  the stroke names only the adds, and a comment saying a row carries the minus is stale.
- A test that spies on `HTMLElement.prototype`'s `offsetWidth`/`offsetHeight` (22 D4.17 D1) gives EVERY
  element the stub size and records the inline style at each read: a placement script must reset its own
  inline `top`/`left` before measuring, and it cannot read any other element's real size in that file.
- The react-hooks `refs` lint reddens `ref.current = props` written in the render body ("Cannot update ref
  during render"): keep the latest props object in a ref from a plain `useEffect`, whose callbacks run
  after mount.
- One full run in ten reddens `AboutPage.test.tsx` 18 AN.3 and `NodesMenu1.test.tsx` (no timeout line):
  both are green solo and in five paired runs. Rerun the file solo before reporting them.
- `Reflow.test.ts` 10 R1.1 lists EVERY rule of `index.css` that sets `overflow-wrap`
  (`sets('overflow-wrap')`, the expected array is `.sort()`ed, so the entry goes anywhere): a new rule
  that sets it is red until its `"selector = value"` line joins the list with a comment naming the
  slice, and the assertion's message names each addition too. Growing that list is the test's own
  design, not a weakened pin.
- `Reflow.test.ts` 10 R1.8 freezes the CLASS NAMES of `index.css` (the `CLASSES` list; only the `ot-*`
  and `instance-name-*` namespaces are open): a new look reuses an existing class, and a class the brief
  names is the tester's to add. A rule of a NARROWER selector (`.tile .run-actions`) is fine: the
  cascade tests match a selector compacted with its spaces dropped, so `topBy(tight('.run-actions'))`
  still finds one rule.
- The Overview tile is pinned outside its own files: its LAST element child (the engine line in
  `HearthTiles` H6a.2, `dl.tile-facts` in `HearthTilesBoot` H6b.3, `NodeBoot` 13 N.1, `OverviewHost`
  13 N.12), "the head and nothing else" (H6a.2), "a tile carries no live region" (H6b.4, `SessionEnded`
  S1b.6, `HearthTheme` H4.7 "the page has no live region to start with"), "the tile has its one tab
  stop, the id link" (`InstanceTileChips` 17b.1) and `within(main).getByText(<id>)` being UNIQUE
  (`NodesMenu1` 20 N.2). Anything added to a tile reddens that set: the brief amends them, report the
  list, write no tile test. Since 23 A1.1 17b.1 pins only the controls OUTSIDE the run row (the id
  link alone), so a verb or a status link inside `.run-actions` is free, and H6b.4 reads the tile's
  live regions as the run row's empty status line alone.
- dom-accessibility-api trims each child's text before joining, so a `span.visually-hidden` whose text
  is `' default'` contributes `default` with NO separator: the name comes out `Startdefault`. Put a
  `{' '}` text node between the visible word and the span, keeping the span's text `' <name>'` when a
  test pins it.
- When one test pins a row as the tile's LAST element child and another pins the row's status line as
  `before(row, status)`, put the line INSIDE the row: `compareDocumentPosition` reports FOLLOWING for a
  descendant, and the row stays the last element. A `flex-basis: 100%` child of the wrapping row takes
  a line of its own and no room while empty.
- `npm --prefix fe run format:check` reds the files you just edited: `npx --prefix fe prettier --write
  <files>` before reporting (`fe/ux/` is Prettier-ignored, being its own package).
- A full-suite red that is green solo three times is a load flake, not yours: state it and move on.
  `ServerSave` `1b.36b` (a nav link clicked after a route change) reds under `--maxWorkers=4`.
- A test that aborts at its first bad assertion hides every pin that follows it. To prove the rest of the
  file without touching the tester's, copy it beside itself (`fe/src/ZzProbe.test.tsx`), fix the defect in
  the copy, run that one filter, delete the copy.
- The fake bff's GET answer options (`serverProfiles`, `modelProfiles`, `instances`, …) are functions of
  `init` ONLY, with no call number (unlike the write options' `(body, n, init)`), so a test that pins a
  read failing and a re-read of the SAME path succeeding is unsatisfiable with a stateless
  `() => reply(401, …)`: every read answers 401 and the re-read's rows never arrive (the 17c review's R2).
  The counter belongs in the test's closure, so it is the tester's fix; prove the production arm (the
  re-read fires) in the copy with `let n = 0; () => (++n === 1 ? reply(401, …) : reply(200, ROWS))`.

### The UX check

- The stub is `python3.14` on a random loopback port; the machine's long-standing `python3.12` listener
  on `0.0.0.0:8580` is the user's own service, never the check's. After a killed run, `ss -ltnp |
  grep python3.14` is the check that no stub is left; `pkill -f ux/stub.py` kills the shell running it.
- axe: `aria-required-children` fires on a plain `<div>` child of a `<ul role="list">` (webkit only);
  `target-size` (24 px) is satisfied by a small target fully clear of others, not by one partly covered,
  so a page a few px taller shifts the result; a `<button>` with no class gets a UA grey that fails
  webkit `color-contrast` (give it `.secondary` or `.primary`; the sheet styles `[aria-disabled='true']`
  for both).
- A wrapping chip list capped by `max-width: 100%` on the chip is capped by its PARENT: the `ul` of a
  flex row needs `min-width: 0`.
- The check's stub prints `BrokenPipeError` tracebacks when a browser closes a page mid-read: that is
  noise, the run is judged by its `summary: ... N failing` line.
- A backgrounded `cmd 2>&1 | tail -N` can finish with a 0-byte capture file (the pipe dies with the
  shell): redirect the run to a log file and read that.

### Rust library traps

- A test listener that writes its answer and drops the socket WITHOUT reading the request closes with
  unread data: the kernel sends RST and the client loses the answer it had queued ("read failed:
  Connection reset by peer after 40 bytes"). A "real listener" test is flaky that way (2 of 8 solo);
  the listener that drains the request head to `\r\n\r\n` first passes every time.
- A `server_cfg::render` template is one `format!` string: a `"` in the INI text it carries breaks the
  parse (Rust 2021 reads the word after a closed string as a literal prefix, "prefix `GTT` is unknown"),
  so a `;` comment that names a doc section states it without quotes.
- `nix` features (`fs` for `OFlag`, `feature` for `uname`); `Command::get_args()` yields `&OsStr`;
  chaining onto a temporary in a `?`-terminated statement is E0716 (bind first); `&unsafe { call() }?`
  is E0716; a module fn shadowed by a same-named local is E0618; ash in loaded mode needs a `Drop` guard
  owning the instance, declared after the entry, so every path destroys it. A child process drained for
  output uses `wait_with_output()` after taking its pipes, under a bound; a cache window over a caller's
  clock is `now.saturating_sub(ran_at) < WINDOW`.
