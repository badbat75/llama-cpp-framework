---
name: tester-playbook
description: How the tester writes reliable tests in this project, and the pitfalls met so far. Use when writing or debugging tests.
---

Rules are in `AGENTS.md`. Format of this file: `AGENTS.md`, under "Skills".

## How to

### Before you start

- Check a reported red against the checkout (`git cat-file -t <commit>`, `ls`, `git worktree list`) before
  working on it: a report naming a file or commit that is not here is not this session's red. Report the
  mismatch, write no test for a file that is not there, and finish the brief you were given.
- Set `CARGO_TARGET_DIR=$PWD/build/linux/cargo-target` before the first cargo run on Linux (the generated
  `.cargo/config.toml` pins `build/windows/cargo-target`, so a Linux run lands in the Windows tree), say
  which dir a run used, and count a cold dir as ~8 min of compiles, not a hung test.
- Name a test by the brief's criteria number, carried with the brief's own prefix: a page of brief 16
  cites `16 17b.9:`, never bare `17b.9:` (that id belongs to brief 17's UX pass). Cite a defect a review
  found in a closed slice by the review's label at the END of the title (`… (17b review D2)`): the
  criteria number says which rule is pinned, the label which defect it closes, so one rule can be pinned
  twice as it grows stricter.

### Proving a red

- Before reporting a list of reds, write a throwaway probe test that exercises the new signature, run it,
  delete it, then write the real tests: a Rust test that cannot compile is not a red. A probe proves
  reachability only (it compiles, a value is constructible), never that a call site exists, so call it a
  probe in the report.
- Stub a manager verb that does not exist yet with `ManagerError::Io(std::io::ErrorKind::Unsupported)`
  (`ManagerError` has no `Other`); make the phase's refusal helper take `key: &'static str`, because
  `checks::Refusal.key` is `&'static str` (a `&str` closure argument is a lifetime red, not a test red);
  write `serde_json::from_str::<serde_json::Value>` inside a
  `json!` literal (the macro's `Into<Value>` leaves `T` ambiguous).
- Before reporting a new field on a struct, grep `<Struct> {` and run
  `cargo check --workspace --all-targets --exclude pai-config`: a field on a struct other crates build as
  a literal is a COMPILE red there. Fill each literal with the value its own fixture implies. A new field
  on a wire struct reds every target that reads it: prove the field-independent rules with the one file
  put back at HEAD and report both lists.
- A new `ServerConfig` field reddens every literal with no `..Default::default()`: `bff/src/server.rs`
  `rich_cfg`, `cli/src/cli.rs` (the `server set` expected literal, its cfg, its destructure),
  `core/src/launch.rs` (the all-fields literal AND its destructure), `config/src/server_form.rs`. Beside
  the literals, `checks::KEYS` takes a new refusal key, and the CLI's `server show` key list,
  `fe/ux/stub.py` and `fe/briefs/api-contract.md` name every key.
- Give every wait a deadline (`recv_timeout`, a `try_wait` loop): a test must fail, not hang. Report a
  `Disconnected` (the thread panicked, e.g. a `todo!` stub) apart from a `Timeout`. Drive a timeout with
  `tokio::time::pause()`: a `tokio::time::sleep` on a current-thread runtime is real wall clock.
- Write one test per criterion and expect a split: a rule that is an ABSENCE is green on day one (a
  guard), the rule that adds an action is the red. Report each test's day-one state by name. A test the
  coder must EDIT to go green pins more than one that merely passes.
- When the coder's work already contains the rule, say so, and add an assertion that fails for a reason
  you cannot fix.
- For a defect a review found in a closed slice, pin the WORDS the review proposes: an existing test that
  compares two places (the tile's alert with the page's) is green when both carry the same wrong fallback,
  so it covers the defect's shape and not its fix.
- Assert an observable (a suffix, a pattern) where a name's shape is free (a generated temp name, an
  ephemeral id): a red is a contract, and the coder is free to pick the literal.
- Pin an ORDER rule ("the record is written last") with a call that fails on the LAST step (make the
  render's path a directory so the rename fails) and assert the whole-tree snapshot is unchanged.
- Report a bookkeeping pin (a log or call count over calls made by earlier pins) green today, deferred to
  the coder's green run. For a count over a seam the builder also reads, take `let base = ...` after the
  builder and assert deltas.
- Pin a byte bound on a pipe payload twice: in the arithmetic, and at the wire crossing, where the red
  lands in the client's own frame check, which is the bound's evidence.
- Write a file-name rule for one OS as a `cfg` split, not a table row.
- Prove a test exists with `-- --list | grep -c "^<name>:"`, never by running it.
- Judge red from the OUTPUT (`FAILED`, `panicked at`), never the exit code.

### Mutants and scratch proofs

- Write a mutant script that: copies the file to its own backup, applies ONE exact snippet
  (`src.count(old) == 1` or abort), runs one filtered test, prints the mutated line (indexed from the OLD
  snippet), the panic lines and the failing names, restores with plain `cp` and proves it with `cmp -s`.
- Include the verb's own first distinguishing line in the snippet of a gate copied verbatim into every
  guarded verb, so the snippet matches once.
- A guard of two disjuncts set in the SAME tick (`if (saving || sent.current) return`, the same line in the
  save and in the close) is pinned by ONE of them: dropping the state disjunct of the save SURVIVES (the ref
  set on the line below carries the second-press pin). Mutate the guard where the disjunct is alone (the
  close-while-saving one) and report the survivor as unpinned.
- Run nothing else with cargo on the crate while a prove loop rewrites the real source.
- A mutant that SKIPS a check (drop one disjunct of a compound trigger) reds the test that pins the
  check's own side effect (the listing was asked: `0 != 2`) even where the outcome pin would pass: report
  both names, the second is the "the check runs" half. A trigger condition repeated in two adjacent blocks
  of one function matches twice: put the block's first inner line in the snippet.
- After a mutant, solo-run the killed test on the restored source: the `Compiling` line proves the restored
  file is newer than the MUTANT binary (plain `cp` moves the mtime), so no mutant binary survives.
- A pin of a SEAM (a function the production code must call, whose stub the tester added) is red on the
  stub, and its teeth show only with the seam wired: the prove loop applies the simulated fix first, shows
  the pin green, then mutates (the mutant the brief names, and its twin at the other end of the seam), and
  restores. A mutant of a "the argv reaching the child" pin is invisible when it mutates the argv builder
  the pin compares against: mutate the CALLER, and let the equality pin face a builder that adds an arg.
- Prove a red that cannot be shown in the real tree (test-side reds behind an earlier red, pins behind a
  compile-blocked phase, pins in a long phase) in a scratch copy:
  `rsync -a --exclude build --exclude .git --exclude dist --exclude node_modules <repo> /tmp/<x>` plus a
  symlink of `fe/node_modules`, or a `git worktree add --detach` of HEAD for an inherited red. Report it
  as "seen in a scratch copy with X flipped", never as a green of the current tree, and name the scratch
  panic lines as such.
- For an fe slice, the scratch copy is the proof the reds are satisfiable: a python script applies a
  faithful stub of the feature (the strings keys, the new reader, the call sites), aborting when a snippet
  matches `!= 1` time, and the slice's files go green there. What stays red in the scratch names the part
  of the feature the stub left out (a form, a Used-by link), which is the coder's job, not a typo. Run the
  OLDER files the shared fake touches there too: that sweep finds the order lists and h1 lookups the brief
  did not list.
- Isolate one phase or pin with: a temporary `#[test]` calling just that phase; `#[cfg(any())]` over the
  blocked phases; a gate on an env var (`is_none()` wrapping the red pins' braces; an early `return`
  skips the whole phase); `// <SLICE>-PIN-<n> BEGIN/END` markers a script deletes. Restore byte for byte
  and prove it (`grep -c`, `cmp`, `git diff --stat`). Keep a delete-script's pin blocks self-contained
  (own snapshot reads, cleanup tolerant of files a deleted pin created).
- Reproduce a reported test-side red SOLO in the tree as it stands first: most "red left in the tests"
  claims are stale test code, and fixing it is the tester's.
- Never run `cargo fmt -p <crate>` (without `--check`) when the tree carries other roles' uncommitted
  lines. List the hunks with `--check | grep '^Diff in'`, hand-apply your own, and prove with `--check`
  answering 0 and a `git status --porcelain` before/after diff. Where only Rust lines are yours,
  `cargo fmt -p <crate>` is acceptable if a `git diff --stat` taken before and after moves the counts of
  YOUR files alone: keep both listings and quote them.

### Cargo runs

- Force the rebuild when a run has no `Compiling <crate>` line (add a `#[test]`, run, remove it, prove
  with `cmp` + `grep -c`).
- Prove a GREEN rerun with no `Compiling` is a replay of the coder's fresh build: each test binary's mtime
  (the `Running` lines) post-dates the newest `*.rs` of the crate, and report that beside the totals.
  Compare each binary to the newest of its OWN inputs (its own test file + the crate's non-test `*.rs` +
  `Cargo.toml`): the check answers 0 only per target.
  Here the test binaries sit under `debug/build/<crate>/<hash>/out/` (`debug/deps/` is absent):
  take the paths from the `Running` lines, never glob `deps`.
- Use `--no-fail-fast` for the slice's map (a target that fails to compile, or a test that fails, stops
  the run), written BEFORE the filter separator (`cargo test -p <crate> --lib --no-fail-fast`): after `--`
  it reaches the test harness, which dies with `Unrecognized option: 'no-fail-fast'` and prints no
  `test result:` at all. The map: every `test result:` line, and
  `awk '/^---- /{name=$2} /panicked at/{print name" | "$0}'` for one line per red; then run the untouched
  binaries in a second invocation to show them green.
- Filter test names with `cargo test -p <crate> <word>` (never target names); reach a file with
  `--test <name>`, and filter inside a target with `-- <filter>` after `--test`/`--lib`. Account for
  every `N filtered out` line. Say which invocation covers a `#[test] #[serial]` in a lib (not
  `--test <name>`). `--lib` of a binary crate runs unit tests, `--bins` the built binary.
- Write a long cargo log to a file, grep `test result|Running|^error`, then `sed -n` each block.
- Read a copy handed over as "green there" with `diff -rq crates <copy>`: it usually carries edits beyond
  the quoted diffs. Judge every hunk and name the incomplete list.
- Run the manager's D-Bus unit tests and the GPU-probe flooding test alone
  (`cargo test -p pai-manager --lib -- --test-threads=1 data_root_e2e`, or the full path with `--exact`),
  then judge with solos plus the full-run map. Never edit the probe.
- State a pre-existing failure of states or tests the slice did not touch (UX check, flaky full-suite
  files) as pre-existing, with a proof the file is untouched (`git diff <path>`); do not chase them.

### The e2e phases (data root, the configurator)

- Register a new phase in the same edit that writes it: `pai-core`, `pai-app`, `pai-cli` and
  `pai-manager` each have one `#[test] fn data_root_e2e()` in `src/tests/mod.rs` calling the phases in
  sequence, and the GUI's e2e is the one `#[test]` of `crates/config/src/tests/ui_bindings.rs`.
- Prove each phase alone when a refusal aborts at its own phase, and report the later ones as blocked,
  not passed.
- Prove each later red of a test that masks its own reds by cutting the earlier block out of the real
  source (a python line-range delete that asserts its two anchor lines first), running
  `--lib data_root_e2e` (~7 s incremental), and restoring with `cp` + `cmp -s`.
- Prove the several reds of ONE phase one at a time by wrapping each assert in `if false { // X-PIN-BEGIN`
  and `} // X-PIN-END` (a script that finds the message, walks up to its `assert`, down to its `);`, and
  asserts each walk is short): run, gate the red, run again, repeat. Then gate the feature's asserts ALL:
  the phase going green there proves the fixtures are sound and the reds are exactly the missing feature,
  not a typo. Restore by deleting the two marker lines and prove with `grep -c` answering 0.
- Pin a render rule through the public seam (`Manager::render_failures()` re-renders every instance from
  the files on disk and answers the failures), so a fixture the client write refuses arrives by
  `std::fs::write` of the pool plus one `render_failures()` call. A client `write_file` of `models.ini`
  refuses a `[*]` section (`invalid-model-profile-id`), so the pool's global section is a hand edit.
- Read the pure render first (`pai_core::assign::render`): it already copies the pool's `[*]` section
  verbatim, so "an explicit pool `[*]` value is copied as is, nothing added" is a day-one green pin, and
  the slice's red is only the line the render ADDS.
- Drive a real save through `config_seam_e2e` (redirect, save through the seam, reload, assert): a unit
  test over a temp dir passes with no seam at all and is not a data-root e2e.
- The GUI's Server tab has its own e2e home, `crates/config/src/gui/server_tab.rs`'s `#[cfg(test)] fn
  e2e()` (already called by ui_bindings' one `#[test]` through `gui::server_tab_e2e`), so a new phase
  there needs no registration. Take `let files = c.files.clone()` BEFORE `fake::scoped` moves the
  client: the test then reads the bytes the Save handed the manager and parses them with
  `server_cfg::from_text`.
- A widget's accessible name is what `find_by_accessible_label` answers: a std `CheckBox`'s is its own
  `text`, NOT the `LabeledField` label beside it, so a checkbox pin looks up the toggle's key and the
  label is a separate presence pin. Read both through `strings::t(key)`: a key missing from
  `en_US.toml` echoes itself, so the red names the key it needs.
- For an `Option<bool>` server key whose ABSENCE is the on state, the widget is a bool toggle (the
  `webui` shape), which means a GUI save of an unset key STARTS writing the line: pin that as
  `form_to_config(config_to_form(unset)) == Some(true)` so the red is a runtime assertion, and leave
  the form field's spelling (string tri-state vs bool) to the coder, pinning the checked state on the
  widget instead.
- Pin the env of `launch::launch_command` (and `instance_command`, which calls it) inside the
  `data_root_e2e` phase: `launch_command` calls `paths::data_root()`, and a unit test of it races the
  phase's process-wide `PRIVATE_AI_DATA_ROOT` redirect.
- A new DEFAULT export in `launch::env_vars` reddens every strict-equality env test of the OTHER knobs,
  in `crates/core/src/launch.rs` and in `crates/app/src/runstate.rs`: filter the new pair out of those
  (a `env_vars_no_gtt`-style helper), pin the pair in its own test, and assert by key presence, because
  the order inside `env_vars` is the coder's freedom.
- Pin names (present and absent), never counts, inside a phase that shares ONE tree; snapshot "the
  refusal wrote nothing" AFTER the last block allowed to write; assert "nothing left in the user tree" at
  the END of a phase. Pin a whole file the phase grows as `fixture + appended`.
- Snapshot at the phase's start and write back at its end every file a LATER phase pins or reads as its
  `base` (read the snapshot BEFORE the write loop). Prove the restore's teeth with a mutant that swaps the
  write for `let _ = found;`.
- Put a fixture a production path resolves as RELATIVE where that path says, computed by the SAME helper
  (`launch::instance_current_dir(..)`), never `dir.path().join(..)`.
- Keep at least one binary-format fixture where two header counts differ (a swapped order is invisible
  when they are equal), and mirror the reader's own test writer. List EVERY row the writer writes in a
  fixture over a file the test writes.
- Carry in a refusals-loop entry whose `why` says "an id taken by an earlier call" an id the phase really
  created. Keep a hand `* = on` in `assignments.ini` for the refusal pins (it serves every profile) and
  rewrite the file with one explicit key before an accepted delete.
- Let the design's wording decide between two of the slice's own pins of ONE rule that disagree (a design
  naming `ini::text::section_text` means its bytes verbatim), and retarget the loose pin.
- When a fixture text one phase accepts as the write collides with a neighbour section's taken value under
  a case-insensitive uniqueness rule, rename the NEIGHBOUR's text (keep the canonical accepted text),
  sweep the literal across EVERY file carrying that fixture with a count assert (`s.count(x)` per file),
  then hand-fix the case-variant texts of the taken pins, the reader expectations and the self-write. A
  refusal-block's record count is the number of refusals its loops make (bad + markers + taken): a later
  pin added to the block leaves the literal short.
- Sweep `grep -n <signature>` and the tuple types crate-wide when a wire shape grows (a D-Bus row
  signature): every pin of it and every tuple literal that reads it reds, in phases of older slices.
  Update the stale signature in the prose of the test's own header comment.
- Pin a rule about the KEYS of a request through the raw-wire tables (`manager/pipe.rs`,
  `manager/dbus.rs`): the client tables' variants carry fixed keys.
- Pin a manager verb in one phase over all three transports (direct, pipe, D-Bus).

### The bff (Rust)

- Add a new verb of `ManagerClient` to the `impl` block of the fake in every `crates/bff/tests/*.rs` (each
  file has its OWN fake): the red is then compile red, one `E0407` per verb, count and list them. The cell a second
  instance needs (`Live::instances`) exists in one file only.
- Read the trait before promising an `E0407`: a verb an earlier slice added WITH a default body (the
  older-protocol transports, e.g. `set_instance_tags`, `instances`) is overridable, so the fake compiles
  and the red is the router's 404 on the missing route. Count `E0407` only for a verb the trait does not
  declare.
- Enumerate the fan-out of a key added to a bff list row: every whole-row `json!` literal carries a key
  unique to the row (`grep -c '"removable"' crates/bff/tests/*.rs` finds them all), and the file's
  `row()`-style fake helper is where the new field gets its default. List them by file and count, patch
  none.
- Let the design decide who owns a suite's 405 loop when a verb lands on a path it pins as "not served":
  drop the served verb, keep the rest (day-one guards), point the doc comment at the file that pins the
  new verb.
- Set the `Live` cell (`call_pause_ms`, `connect_pause_ms`, `call_lost`, `connect_fails`) AFTER the router
  is built, so the fake logs the call and only then sleeps or fails: a 504 pin can then assert the manager
  received it, and a failed connect stays 503 with an empty log. Pin elapsed BOTH ways
  (`>= sleep && < limit`): the lower bound proves the success really ran past the read limit. A multi-call
  write sets the pause per call.
- List the source's route-list doc comments for the coder when you gate a route (session or permission):
  they go false with the gate, and a tester edits no `src`.
- The shape gate answers 400 `server-malformed` for a value of the WRONG JSON type; a 422 with `field`
  comes only from a check (the bff's own, or the manager's `Invalid`). So a design line "refuses a
  non-bool 422 with `field`" about a bool key means the manager's rule on the rendered file: feed the
  real `checks::server_file` refusal of that text into the fake's `Outcome::Invalid` (its `key` is
  `&'static str`, so it needs no leak) and assert `field`; pin the wrong-type case in the file's
  existing malformed-bodies list.
- Write a list criterion that keeps an item with defaulted fields, and an expected-rows array that omits
  it, as one fixture with opposite rows.

### Fakes, source lints and renames

- Write a source lint knowing it reads production code with whole-line comments dropped and that a name it
  matches is the lint's data; over the files of a package split, pin only the OWNERSHIP rule it names.
- A source lint that walks `read_dir(<crate>/src)` is non-recursive, so it skips that crate's `src/tests/`
  (the lib's own unit tests): for a rule about the production code that is exactly the scope, and the
  test's comment says so.
- Pin a decision as a pure function over its inputs and test the function, not a run that happens to
  reach it. Pin a reader over a fixture root with the fixture's exact names (the case rules compare
  exactly, and `write_file` refuses a `[*]` section).
- Give a shared fake prefix filters and assert the asked routes by exact list (`list, status <id>`), never
  by one substring: a fake that answers `?? DEFAULT` hides a missing answer.
- Rename a word from the ARCHITECTURE.md names table and enumerate every site by grepping the literal
  first: a word-internal map breaks identifiers and `mod` file names. A stale pointer in a comment is a
  defect.
- List every whole-object `json!` literal an additive wire key reddens; patch none.
- Rename and re-count when a ruling moves a key onto a list: it reddens the off-list 400 loop and the
  exclusion test whose NAME carries the count; grep `exclude_the_`.
- When a brief gives an existing key a placeholder (`Remove` to `Remove {0}`), change the value in the test
  mirror `EN` and sweep every `en('<key>')` call site: the ones that built the name by concatenating the
  legend move to the filled call (one `removeName(n)` helper per file), and the visible-text pins of the
  old shape are deleted, not amended. `App.test.tsx` `1a strings` stays red until the coder edits
  `en_US.json`: that red is the strings contract.
- Never add keys to `src/strings/en_US.json` and never weaken the strings guard: `App.test.tsx`'s
  `1a strings` compares the test mirror `EN` (`fe/src/test/bff.ts`) with `en_US.json` key by key, so a key
  you add to `EN` is the coder's signal and that one red is the intended one, not collateral damage from
  touching the fake. A key the brief REMOVES goes out of `EN` and into that test's `removed` list; a test
  that pins its words are on no page writes the sentence as a literal constant, because `en(key)` throws
  once the key leaves the map (the red then names your test, not the slice).

### The web UI (fe/): running and probing

- Run vitest as `npm --prefix fe test -- <file-part> [-t '<title>']`; the type check that sees test files
  is `npm --prefix fe run typecheck`. The filter is relative to `fe/`: `src/x.test.tsx` runs the file,
  `fe/src/x.test.tsx` answers `No test files found, exiting with code 1` (a red that reads as a missing
  test). Run `format:check` on your files: Prettier is part of the test edit.
  `prettier --write` on a file the coder left clean moves only your own lines: keep a `cp` of the file and
  `diff` it after.
- Probe a harness fact with a deliberately failing assert carrying the value, in a `.test.tsx` you delete
  afterwards (prove with `ls | grep -c`).
- Group fe reds by cause: strip ANSI and
  `grep -oE "(TestingLibraryElementError|AssertionError|TypeError): .{0,60}" | sort | uniq -c`.
- Judge a flaky fe red solo x3 before anything is touched; state a full-suite red that is green as a
  file, do not chase it. A red that appears ONLY in the full parallel run is invisible solo and in one
  plain run: reproduce it by running the flaky set at `--maxWorkers=4` under CPU load (N burners of
  `while :; do :; done`, ~16 on this 24 CPU box) 3 to 6 times, reading each run's `Test Files` / `Tests`
  line. The rate here is ~1 red per 3 runs of 12 files, so a green solo x3 proves nothing. Give each
  harness its own log prefix: two harnesses writing `r$i.log` overwrite each other's runs.

### The web UI (fe/): the fake bff and fixtures

- Run the whole fe suite ONCE after changing the shared fake (`src/test/bff.ts`).
- A key added to the shared `serverDoc()` reddens every strict body pin of the server form: `UNCHANGED`
  (compared with `toEqual` in eleven places of `ServerSave.test.tsx`), the explicit value lists of its
  `1b.32 (5.4)` pair, and the maps `SWITCHES_UNSET`, `SWITCHES` and `CARDS`/`FIELDS` of
  `InstancePage.test.tsx`. A new FOCUSABLE field moves every tab-order list by TWO stops (the field's
  About button, then its control): `ServerSave.test.tsx` `1b 6` and `InstancePage.test.tsx`
  `TAB_PATH_FORM`. Amend each in the same pass and name them in the report.
- Keep a fixture constant looked up by accessible name clear of every other control's name; call
  `cleanup()` per iteration in a test that renders in a loop (RTL cleans only between tests); put
  `Element.prototype.scrollIntoView = () => {}` in the `beforeEach` of a file whose test clicks a jump
  button.
- When you retarget an old pin onto another field or a moved read, amend the old test to the new rule and
  say in a comment which slice retired the old one. When a review SUPERSEDES a pinned declaration (a
  browser-measured fix the next defect undoes), the pair is two reds: the absence pin (`no rule names X
  declares contain:` → `toEqual([])`) is red because the sheet still carries the old declaration, and the
  replacement's positive pin is red because the sheet lacks the new one.
- When a slice changes how an accessible NAME is built (a name read off the item, not the id), find the
  files to amend by the FIXTURES, not by the brief's list: the shared `INSTANCES` fixture leaves out the
  field feeding the name (`port`, `label`), so a lookup by id keeps passing there, and only a fixture that
  sets it needs the name. `grep -n "port: [0-9]" *.test.tsx` is the sweep.
- Give a page-open helper the name it awaits as a parameter defaulting to the fixture's name: a case that
  changes the fixture (port 0, a label) awaits the name the change gives, and a hard-coded name reds it.
  The same for an open-form helper that PRESSES the opener: a test that presses first and then calls it
  presses twice, and the second press closes the form (the opener toggles).
- A tile's run verb is named `<word> <instanceName>` (the label when it has one, the id otherwise), so a
  helper that builds `${en(key)} ${id}` finds nothing on a labelled tile: pass the display name it shows.
- A 403 `session-not-administrator` on a WRITE is the field's alert `session-revoked` with a Sign in
  after it (15 C1.9, 17b), the form open with the typing: the brief's "403 → session-revoked" is the
  alert family, not the sign-in page.
- Pin a 422 whose `field` names a control the form checks itself (a port) with a value the check accepts,
  and the args exactly as the bff sends them: with a value that check refuses the reply is UNREACHABLE,
  because the check sends nothing.
- Pin an id encoding with an id whose encoded form DIFFERS from the raw one (a space and a `#`): the fake
  records the string `api.ts call` handed to fetch verbatim (no URL parse), so assert `calls[i].path`
  equals the encoded route, and take the calls off `bff.calls` by method so the red names the path, not a
  count. An id like `Qwen3-27B` pins nothing: `encodeURIComponent` is the identity on it. Open the page at
  the encoded hash (`startAt` keeps `%23` in the fragment, `parseRoute` decodes it) and settle on the
  cards' `h3` before pressing the page's own control: the h1 alone renders while the read is open.

### The web UI (fe/): async settling

- Never query before the async read settles: add an async finder, settle signed-out states by the sign-in
  `h1`, and settle on the page's `h1` before asserting loading text.
- A row or link that arrives WITH a read is found with `findByRole` after a navigation, never
  `getByRole`: the target page's h1 is on screen while its list read is still open, so a sync query right
  after the h1 reds only under load (`ServerSave.test.tsx`'s `1b.36b` nav to the server profile).
- The focus and the document title are written by `PageHeading`'s PASSIVE effects, which run AFTER the
  render that put the h1 in the tree, and an async finder resolves on that render. So a synchronous
  `document.activeElement` / `document.title` assertion after a `findByRole` is a load-only flake: use
  `arrived(h1, en('page-title', name))` from `src/test/bff.ts` (polls both), or `waitFor` around the title
  alone where no focus is owed (a direct load takes no focus, 09).
- A call count pinned as exact is read after `quiet(bff)` (`src/test/bff.ts`: returns once the fake has
  seen no new call for 40 ms of real time). A page sends its own read from an effect that runs after the
  render its h1 appears in, so a count read right after an async finder sees 0 where 1 is pinned
  (`NodeNav.test.tsx`'s `13 N.3`, `AboutPage.test.tsx`'s `18 AN.2`, `ModelProfilePage.test.tsx`'s `2d.63`).
  A count pinned as 0 is read the same way: the quiet app is the evidence nothing was sent. `quiet` needs
  REAL timers (under `vi.useFakeTimers()` its poll never runs) and never settles a page that POLLS (the
  instance page's idle poll): pin those counts on the read that lands. Measure its idle window on
  `performance.now()` (the moment the count last changed): `waitFor` re-runs the callback on every DOM
  mutation too, so a window counted in polls ends early under a burst of mutations.
- In a tab-order snapshot take EVERY async element (`await findByRole(..)`) first, THEN `tabbable(main)`,
  and settle the nav's instance links too: a snapshot taken earlier prints `indexOf = -1` for a real tab
  stop (about one run in two). Grep the suite and amend the tester's lists when you add a focusable
  control, each list moving by one position. Pin "nothing between A and B" with indices from the settled
  list. The Start-at-boot field renders only after its OWN read (`Boot` returns null while awaited): the
  page's h1 settles on the list read, so settle the boot group/checkbox with `findByRole` before querying
  it synchronously or taking the snapshot. A tab-order name map of `aria-label ?? textContent` never sees
  an input (its name comes from its label): map the element to the wanted name by identity, as
  `Boot.test.tsx` does.
- Read a state-dependent list (devices, a select that shows a placeholder while pending) only after the
  page's `landed()` helper, the deliberate pending case excepted. The probe also gates ACTIONS that read
  it (Add rule seeds the first probed GPU): clicking one before `landed()` seeds the pending-case value
  and the test reds on a correct component.
- Under `vi.useFakeTimers()` (it breaks RTL's `waitFor`/`findBy*`: a render that needs a real macrotask
  never happens) list `toFake` as `setTimeout/clearTimeout/setInterval/clearInterval/Date`,
  flush with `act(() => advanceTimersByTimeAsync(0))` repeated, put the answer's own timeout inside the
  fake clock, and pin delay boundaries tight (`advance(299)` absent, `advance(1)` present). Poll-driven
  rows never advance under real timers.
- Find `role="status"` with `getAllByRole('status').find(el => el.textContent?.trim() === text)`, and pin
  an absence with the same filter: it has no name from content, so `queryByRole(.., {name})` never
  matches. A page-level live region breaks every page-wide status count: pin the count the brief rules.

### The web UI (fe/): jsdom and CSS pins

- Pin "in the DOM and not shown" as two pins: the element's link to `.visually-hidden` (accepting both
  nestings), and the sheet's CSS text. Pin a note's order as `note.nextElementSibling === field`, and "the
  icon is the only element" as `children.length === 1`.
- Pin an order chain against EVERY element its message names: a message "after the intro, before the
  cards" that asserts only the `h2` and the card is a loosened pin (the 22D4 review). The Server page's
  intro is `getByText(en('server-profile-intro', `server@${id}.ini`))` (as 17a.5 reads it), never a
  class: pin `h2 → intro → bar → card` as three `before(..)`.
- Pin an absence with `queryAllByRole(..)` and `toHaveLength(0)` (greens both the no-list and the
  empty-list shape); never `.value` on a `queryByRole(..)!`; index options from `options.length - 1`,
  never a literal; pin a LATER sub-slice's key absence structurally (`nextElementSibling` is null), not
  with `en(key)`.
- Read a focus-at-open pin BEFORE the test presses any other control of the opened thing, and declare the
  control it names above it.
- Pin a text that must be there WHEN the focus lands (an error before the screen reader reads the field)
  with a `focus` listener that records, at that moment, the `aria-describedby` names and whether the text
  is in the region, then press.
- Pin "inactive" by pressing the control (an `aria-disabled` control is no shield: its handler runs on
  click) and asserting the open thing is still THERE (the same element), its status line unchanged, the focus not moved to the opener, and the request count still one; then
  resolve the request that was already in flight and pin the reply's outcome.
- Pin a terminal state (deleted, not found) as NOT carrying: after it, change the address to another id
  and pin the other page (its h1, its own rows, the read of its route) and the absence of the first id's
  paragraph. A keyed remount passes this on day one: it is a guard, not a red.
- Pin a refusal text that must outlive the re-read that follows it: wait for the re-read to land (the row
  names the new state), then look for the text in the element the criteria name
  (`p[role="alert"][tabindex="-1"]`), focused.
- Name in the report every class a CSS pin uses that the brief leaves to the coder (its section 6): the
  pin is an assumption written into the contract, the reader's `found 0` is the red, and the coder has to
  use exactly that class.
- Amend the frozen allowlists (compared with `toEqual`) in the same edit as the layout test and name them
  in the report, leaving none "for the coder": a new class reds `Reflow.test.ts` `CLASSES`, a new `overflow-wrap` reds its R1.1,
  a ruled size reds `HearthTokens.test.ts` H1a.12 (ordered list, one rule's entries in the test loop's
  order, per-card sets sorted on both sides), and a declaration added to a pinned rule reds that rule's
  "declares nothing else" list and its title. Read an array's own tail to see whether it is sorted.
`Reflow.test.ts`'s `declares` compares the property list IN the sheet's declaration order, so amending it
fixes where in the rule the new declaration sits: add it last, and name that in the report. Check the list
before amending it: H1a.12 collects `min-height` and `min-width` ONLY (a `max-width` or
`grid-template-columns` fix reds nothing there), and `CLASSES` keeps a slice namespace
(`instance-(rename|label|name)*`, `ot*`) outside the frozen list, so a class inside one needs no entry.
- A helper that reads a declaration's VALUE (`values(decls, 'position')` returns `fixed`) must be
  compared with a value literal: an expected `'position: fixed'` is a test-side red whose output reads
  "received 'fixed'", which is right. Compare like with like, and sweep every call site of the helper.
- A declaration filter must allow the space after the colon: the reader's `norm` leaves
  `position: fixed`, so `/^position:(absolute|fixed)$/` matches NOTHING and the pin passes vacuously.
  Write `/^position:\s*(absolute|fixed)$/` and prove the teeth with a css mutant that ADDS the
  declaration (a pin of an absence that stays green under that mutant is a pin of nothing).
- A frozen `:hover` (or any `toEqual`) allowlist sits after another in one test: the first red masks
  it, so fixing one list reveals the next (H2b.6's shadow list hid its hover list, which 22 D4.21's
  `.ot-add` hovers move). Run the whole test after each amendment, not just the assertion you came for.
- Pin a look the brief gives a control ("40 px, no border, no fill") as the CASCADE for the classes the
  element carries: read `button.classList` off the page, keep the top-level rules whose selector is the
  element's tag or a compound of those classes, order them by class count then sheet order, merge their
  declarations, and assert the effective value. The pin then follows the class names the coder chose, so
  it needs no assumption; accept the shorthand or its longhand parts (`border: none` or
  `border-style: none`) where the brief states the effect. Match a selector part by its LAST token against
  the element and its EARLIER tokens against the classes of the element's ancestors (read off the DOM): a
  rule requiring every token to be the element's own misses the scoped rule the coder writes
  (`.instance-header .x`), which is a red for a correct fix.
- Pin a cascade value that today's sheet already carries globally (the `h1`'s `overflow-wrap: anywhere`)
  as a presence pin on that global rule, not as a value of the scoped merge: the merge sees only the rules
  scoped to the new box, so a global declaration reads absent and the pin is red on a correct fix.
- A new box reusing the field classes needs a pin of the SELECTOR GROUP, not of a `> *` rule: the 720 px
  block places `.field-control, .full-value, .field-hint, .field-error` in column 2, and a top-level
  `.x > * { grid-column: 1 }` (0,1,0) BEFORE it loses on source order, so the box collapses (17f2 D1:
  the box 18 px at 1280 px). Pin that one top-level rule names both children (0,2,0) and declares the
  value: the specificity is the fix, so a source-order pin reds a correct fix.
- When a review names two sanctioned fixes (a sheet declaration, or a markup shape like a wrapper div),
  pin the DISJUNCTION: the cascade value for the sheet option, the parent chain read off the page for the
  markup option, `a || b` in one assertion. Forcing one shape writes the coder's fix into the contract.
- Pin a spacing the brief states as a token gap ("a `--space-5` gap after it") loosely: exactly one
  top-level rule naming the element carries the value in a spacing property (`gap`,
  `margin-right`/`margin-inline-end`), zero on its neighbours. The coder picks flex gap or a margin, and
  a strict cascade pin would write one of the two into the contract.
- Pin "the two controls share one rule" (a brief: share the class or the rule, not a copy) by reading the
  selector GROUP of the rule that styles the existing class and asserting the new control carries one of
  its parts: a comma group and a shared class both pass, a copy of the declarations does not.
- Pin a hover-reveal that must sit in the flow (not a popover) by what only the flow shape answers: the
  box's `mouseleave` hides it, a `mouseover` of the box (not only of the control) shows it, and no rule
  reaching it declares `position: absolute`/`fixed`. A popover that is a child of the same wrapper passes
  every "the text is in the box" pin, so the text pins alone prove nothing (22 D4.13).
- Prove the teeth of a CSS pin in a scratch css + scratch test pair (`cp` both, one string swap, `-t`).
  The test reads `index.css` beside itself, so a real-tree mutant of the css, restored with `cp` and
  proved with `cmp -s`, is enough. A snippet that ends at the rule's `}` puts the added declaration
  OUTSIDE the rule (the pin stays red on a correct fix): anchor inside, and read the rule back.
- Send a test that cannot fail in jsdom (real layout, focus movement, `fireEvent.click` moving no focus,
  clipping) to the UX check, or state it as unproven.
- Pin a script's PLACEMENT (a floating tip's `top`/`left`) in jsdom by stubbing the geometry it reads:
  `vi.spyOn(window, 'innerWidth', 'get')` works (probed: it is an accessor), assign
  `el.getBoundingClientRect = () => rect` per instance, give the box `offsetWidth`/`offsetHeight` with
  `Object.defineProperty` when the brief says "its own size" without naming a reading, then
  `fireEvent.scroll(<the scroll box>)` (a capturing window listener catches it) and
  `fireEvent(window, new Event('resize'))`, and parse `el.style.top`/`el.style.left`. Absent inline
  placement reads `NaN`, which is the right red before the script exists (22 D4.17).
- Pin a `preventDefault()` with the return of `fireEvent.keyDown(box, { key: 'Enter' })` (false when the
  handler prevented it): jsdom never runs the key's default action, so the implicit submit, and the Enter
  that presses the control the handler just focused, are invisible here and only the prevention is
  pinnable (16 17f review D2).
- A brief's "the hint is the field's description" is NOT `aria-describedby` in these forms: `describes()`
  names only the error (and a written-as line), and the hint is a hidden `div#<id>-hint` behind the
  field's About toggle. Pin it by clicking `About <label>` and reading the hint's `<p>` per `\n` of the
  key, and pin the box's name by role and name (the group is named by the label).
- `within(el).getByRole('group', { name })` never matches `el` itself: a `CheckboxField`'s `<fieldset>` IS
  the group, so look it up from `main` and compare identity with `toBe`.
- `closest()` answers `Element`: cast to `HTMLElement` where a helper takes one (typecheck TS2345).
- A link's accessible name joins the text of its children with NO separator, so a name built as text plus
  an inline `span` reads "Coding assistantID default": pin the space as a text node in the markup
  (`textContent` and the name both), and the span's own line in the sheet (16 17f review D1).

### The web UI (fe/): nav and order literals

- Work every row of the brief's "Older tests that change" section before reporting. Where a row retargets
  an old pin (a refusal that gains a control), delete that row from the old table and cite the new test
  file in a comment; where the prior sub-slice pinned an absence structurally (`nextElementSibling` is
  null), retarget the same selector onto the thing this slice adds.
- Amend `MODEL_PROFILE_CARDS` in `fe/src/test/modelProfileCards.ts` when a card gains always-visible
  controls (`extraStops`: a strings key is a button, `{about: key}` its Field's About toggle,
  `{input: key}` a text input, no button), and `EDITABLE_BY_ID` in `ModelProfileSave1.test.tsx` 2h.130,
  which keeps a LOCAL copy of the editable-field map ("a test file imports no other test file"), so a new
  editable control lands in both.
- A control that MOVES on the model profile page (a bar, a link) reds the whole-page tab-order snapshots in
  seven places: `2e.91` in each of `ModelProfileCards1..5.test.tsx`, `2d.71` and `2d.78` in
  `ModelProfileContent.test.tsx`, plus `H9b.4`/`H9b.5` in `HearthJumpRail.test.tsx` (the bar's place among
  the section's children). Fix the shared list once (`pageTopTabStops()` / `saveBarTabStops()` in
  `fe/src/test/modelProfileCards.ts`): when the bar moves the coder renames that shared list
  (`pageStartTabStops` → `pageTopTabStops`) and patches its importers, so the reds are the files carrying
  a LOCAL copy of the order. Grep the OLD helper name and the moved key literal
  (`grep -n "save-discard" fe/src/*.test.tsx`) to find them, and import `saveBarTabStops()` into each.
  Retiring a test moves the suite total by one: state the arithmetic. Run only those tests with
  `npm --prefix fe test -- <the files> -t '2h.135|2d.71|2d.78|2e.91'`: the five cards files are ~1500 lines
  each, and a full run of them is minutes of unrelated tests.
- Land a new nav entry in about a dozen order lists: sweep every standalone `en('nav-...'),` line (and
  the `EN[...]` variants), then check tails (`grep -L` for the new key; `grep -rn "slice(-[0-9]"` for
  windowed lists) and `NodePage`'s list. Then rewrite what the sweep cannot: a `parseRoute` pin of
  `not-found` for the new address, a group's href list, an `aria-current` case, a "gains no such key" pin.
- Pin an alias rule on both paths (address equals the state hash: no navigation; address moves:
  `setHash`).
- Find a tile or an instance link by its address (`a.tile-id[href="#/instances/<id>"]`), not by its text,
  in a helper shared by tests of a slice that renames it: the helper then survives the name rule, and the
  name is pinned where the brief states it.
- When a brief makes a read-only field editable, amend every fixture that pinned it read-only (the
  fan-out goes through the shared cards helper) and retarget the "nothing to focus" refusals.

### The web UI (fe/): the UX check

- Run `npm --prefix fe run build` then `npm --prefix fe/ux run check` (about 12 minutes) and report unique
  lines (`sed 's/^chromium //..' | sort | uniq -c`), not the summary's count: it prints one line per
  browser x width x theme (x text mode), so one unmet `expect` is 18 lines.
- For a new state whose reads the stub holds, use `waitUntil: domcontentloaded` and every key it can set
  in the reset query of `load()`; read the `expect` markers BEFORE `click`/`open`/`hover`; the nav at
  320 px needs the `menu: true` field.
- Name a disclosure the state opens by its production icon: `button:has(svg path[d="M5 12h14"])` (the
  plus; `M12 5v14` is shared with arrow-down, the table sort). `main form` is unique (the sign-in form sits in a dialog) and `main [role="group"]` outside the
  model pages. Pick the marker text per state: one interpolated raw from `S['key']` names the text
  "undefined" and fails the state today, pinning it the moment the key lands; a `?.`-guarded one degrades
  to a selector-only marker that passes loosely today.
- Verify the LENGTH of a repeated-character fixture literal with `python3`, not by eye.
- A `STATES` entry that clicks a control the new slice replaces is a pin in the tester's scope: retarget it
  on the new key (the raw `S['new-key']` marker names the text "undefined", so the state fails today and
  passes when the key lands) and retarget its text expectations on the new key too. `node --check
  fe/ux/check.js` is the cheap proof the edit parses: the full run needs the Playwright browsers, which
  `npm ci` does not install.

### Behaviour that cannot run here

- Pin Windows, xtask packaging and CI behaviour as text and type checks only, and state in the report what
  Linux can and cannot prove.

## Pitfalls

### A cargo run

- A run with no `Compiling <crate>` line judged a STALE binary: an assertion that passes in a run and
  fails in a probe of the same phase is the stale binary, never a flake.
- `cmd | tail` reports tail's 0, so a mutant that survives reads green.
- A cargo test binary does not inherit a variable set on the command line for a harness process; a unit
  test body does see it.
- Piping a long cargo log into `grep` dies on SIGPIPE.
- A background run killed by the shell's timeout leaves a partial log full of `test result: ok` and no
  `FAILED`: judge a run complete only by its own exit line, and rerun the crate whole.
- Setting `PRIVATE_AI_FE_DIST` reruns the bff's build script and recompiles every target of the crate (it
  falls back to `crates/bff/fe-fixture`): set it only to escape a stale `fe/dist`. `PAI_BFF_FE_DIR` in the
  shell does nothing.
- `--test '*'` names no target for `pai-manager`.
- The manager's D-Bus unit tests and the GPU-probe flooding test are load-sensitive (slow under a loaded
  machine, bound reds at 0..1800 ms).
- A test that touches the live `bench.db` path hangs the crate's suites behind the machine-wide lock.
- An attribute glued to the end of a doc comment leaves the test unregistered: it never runs, and clippy
  says `dead_code`.
- `map.get(k).is_none()` is a clippy `unnecessary_get_then_check` red under `-D warnings`: pin a key's
  ABSENCE with `!map.contains_key(k)`.
- A source lint over a `cfg(windows)` file is green on inspection, never "test passed", and "nothing runs
  here" is not proof a later guard passes.
- A text red is often a missing strings key: `en()` throws `no English for <key> in the test`. Look the
  key up in `fe/src/strings/en_US.json` first.

### Mutants and scratch copies

- `cp -p` in a restore leaves the old mtime, and the next run reuses the MUTANT binary.
- A restore written `cp <file> <backup>` (direction reversed) overwrites the clean backup, and the
  `cmp -s <file> <backup>` after it compares the file with itself: the script prints `cmp ok` while the
  MUTANT survives into every later run. Prove a restore of a file clean at HEAD with `git diff -- <file>`
  answering empty, not with the script's own cmp.
- A script that dies between write and restore leaves the file mutated: restore from the backup and
  `grep -n` before running anything else.
- A gate copied verbatim into every guarded verb matches N times, so it cannot be mutated in place.

### The e2e phases

- A helper that trims a line's end (`str::trim_end`) erases a trailing non-breaking space: pin a
  Unicode-whitespace value with the raw line, never with the file's `keys_of`-style helper.
- One `data_root_e2e` masks its own later reds: `pai-manager`'s service phases AND its D-Bus/pipe client
  phases are calls of ONE test, so a full run prints only the first panic. To show a LATER phase's
  day-one state, comment out the earlier phase's one CALL LINE in `src/tests/mod.rs` (a `// GATE`
  prefix), run, restore with `cp` + `cmp -s` and `grep -c "// GATE"` answering 0: one line per phase,
  no block surgery.
- Pin a walk's bound of ENTRIES VISITED with a marker file under a subfolder: the bound is consumed
  inside the first folder's loop, so the marker's ABSENCE proves where the walk stopped, and a tree
  below the bound proves it did not stop early. `paths::instance_models_dir()` under a redirect is
  `<root>/engine-models`; a bound fixture takes a fresh `redirect_for_tests` per block (the bound
  counts the tree, so no block may inherit another's files).
- A phase that rewrites a file a LATER phase reads as its `base` reds that phase when its payload equals
  what the earlier phase left: a compare-and-swap then accepts a stale content.
- A mutant of a validation gate shows only on a tree green in that phase first: with an unrelated fixture
  red upstream the panic you print is the fixture, not the gate.
- A bound a client checks (frame size) reds in the client, not the manager.
- An uncalled e2e phase is invisible: its solo run looks green.
- `pai_core::bff_cfg::versions_dir()` answers `None` under a test redirect (it resolves only when the
  config dir is exactly `/etc/llama.cpp`), so the engine's installed backends (`versions::installed`,
  the `llama.cpp` and `llama.cpp-<backend>` package files) cannot be pinned through a real path in an
  e2e phase: the rule needs a seam the test can point at a fixture dir, and a fixture that writes every
  plausible dir to cover the coder's choice is an assumption to name in the report.
- A tree with the earlier phase call lines gated prints ~33 `dead_code` warnings (each gated phase is now
  uncalled): run clippy only on the restored tree, and read those warnings as the gate.

### The bff (Rust)

- A route whose id the fake does not list answers `no_instance`: that is the gate, not a broken harness.
- `Unavailable` is 503 only where nothing was handed over (failed connect, a read) and 504 on a write
  that crossed; reads keep 503. Before renaming a test ending `is_unavailable`, read which of the two it
  drives.
- A test that signs in to reach a route: the sign-in opens a manager connect, so a "nothing was asked"
  pin takes the connect count AFTER signing in and asserts the delta (a bare `== 0` stays red on a
  correct route). The fake's `authenticate` records nothing, so the manager log stays a clean pin.
- A refusal travels as `{"key","args":[<strings>]}` built by each route module's own
  `keyed(status, key, &[&str])`; `session::error` takes `&[u64]`, so a string arg (a repo, a path) needs
  the route's helper, not the session's.
- `cargo test -p pai-bff <word>` filters test NAMES: naming the test FILE runs 0 tests and prints
  `N filtered out` for every target, which reads as a clean run. Reach a file with `--test <name>`.
- The fake HF fetch answers its canned `(status, body)` list in CALL ORDER, whatever the url: a test that
  switches repo pins WHICH repo was listed through the recorded urls (`hf_asked()`), never through the
  body it got back.
- A fake that parks a blocking call with `while !*released { cvar.wait_timeout(guard, T) }` re-arms `T`
  forever: the parked task never wakes on its own, and the `#[tokio::test]` runtime drop blocks on the
  abandoned `spawn_blocking` task, so the binary hangs past every deadline (no `test result:` line).
  Bound the WHOLE wait with an `Instant` deadline, above the route's own timeout.
- A route that wraps `spawn_blocking` in `tokio::time::timeout` holds the semaphore permit until the
  abandoned task ends, so a slot-comes-back pin must keep the fetch hung across the follow-up calls: a
  released fetch puts the slots back by itself and the pin passes on the bug.

### The web UI (fe/)

- `npx vitest` from the repo root resolves another config and the node environment: every test reds with
  `window is not defined`.
- `--reporter=basic` (vitest 5 dropped it) prints NOTHING, so a grep for the test names answers empty and
  reads as "the tests do not exist": run with the default reporter and read `Test Files` / `Tests`.
- `vitest run --list` (vitest 5) dies with `CACError: Unknown option --list`, and the default reporter of a
  green run prints NO per-file line, so a grep for a file's name answers empty: prove a test is registered
  with `-t '<title part>'` and read the `Tests N passed | M skipped` split.
- In this environment `npm --prefix fe test` exits 1 with an `EISDIR ... getPackageScopeConfig` and a
  `jsdom` message while every file passed: judge from the `Test Files` / `Tests` lines.
- `npx --prefix fe prettier --check src/x.test.tsx` matches NO file and still ends "All matched files use
  Prettier code style!": judge from its `[error] No files matching` line, and run prettier with the cwd
  in `fe/` (or paths from the repo root).
- vitest swallows `console.log`.
- A `TypeError: unexpected <route>` is a fixture gap (the fake bff was asked a route its defaults do not
  cover), not the slice's red.
- A straight apostrophe inside a single-quoted assertion message (`'no icon, as 17b's'`) is a syntax error:
  the file fails to load, printing no test names. This suite writes the typographic `’` in those strings.
- A page-open helper that calls `fakeBff()` itself stubs the fetch with its own fake: a second fake means
  a second set of answers.
- A fixture's `startAt(n)` gates every route, not the one it means; `startAt` is `history.replaceState`,
  so the app keeps one entry.
- A route read by several pages counts every reader, and React runs a child's effect before its parent's:
  a fixture handing ONE `Response` to every call starves the second reader.
- A red assertion in the middle of a test hides every assertion AFTER it (they never run): put the
  day-one guards of a multi-page test in their own test and run it, before reporting the split.
- A helper that answers a BOOLEAN (`el.getAttribute('aria-disabled') === 'true'`) pinned `.toBe('true')`
  is a test-side red: the suite's `.toBe('true')` idiom is for a raw `getAttribute` string. Compare like
  with like, and sweep the helper's CALL SITES, not only the reported one (a red hides its twins below).
- A pin of a percent-encoded id asserts the FULL route the contract names (`.../enabled`), not the bare
  resource route: the component writes the sub-route, so the shorter expectation never matches.
- `fakeBff()` returns the fake, not `main`: a test that renders a second page needs
  `screen.getByRole('main')` after `render(<App />)`. Vitest runs it red as `Property 'main' does not
  exist`; `npm --prefix fe run typecheck` is what names it before the run.
- A page check that does not exist reds as "Unable to find the bar's alert text", never as a request
  count: the check is absent so the save went out. That is the right red when a sibling test of the same
  slice proves, on the same fixture, that the PUT does go out.
- Every shell part that arrives with the session (nav, instance links, banner) reds a synchronous
  `getByRole('navigation')` helper.
- A page-open helper that only renders keeps the PREVIOUS test's hash: a second `openPage` in the same
  test for another id renders the first id's page, and the DOM dump reads "There is no instance named
  <first id>" with the h1 of the first id. `startAt('#/instances/<id>')` before each render.
- Pressing Save with the box holding the loaded value sends NOTHING (the unchanged-text rule), so a
  refusal fixture on that press is UNREACHABLE and the red is `Unable to find ... <refusal text>`: type a
  new value before the press that pins a refusal.
- `en(key, a, b)` FILLS the mirror's `{0}`/`{1}`, so a strings pin `table[key] === en(key, a, b)` can
  never pass (the file keeps the template, the bff fills at runtime): pin the template with `en(key)` and
  the fill where the text renders.
- A `loaded()`-style helper that accepts any shape of a read's answer (a table, or the empty text) must
  poll with `waitFor` over a predicate: one `queryByRole` snapshot races the reply and reds with a DOM
  dump showing the element it sought.
- jsdom applies no stylesheet, and `closest` answers a complex selector.
- `queryByRole('group')` is never unique on a page with a form: every field's `<fieldset>` is a group, so
  a disclosure's open and gone pins carry the group's accessible name.
- Asserting a text and the focus after the fact is GREEN when both rendered in the same tick with the
  focus first, which is the defect.
- A CSS-reading block walker starts at depth 1 after the opening `{`; a rule inside `@media` also matches
  a flat regex; match descendant rules by their WHOLE selector (whitespace normalised); an at-rule's body
  is a list of RULES; a pin of a declaration's exact text must tolerate inner newlines.
- A cascade or selector-ordering probe whose ancestor ORDER is inverted never matches; specificity ties
  go to source order.
- `fireEvent.mouseOut(child, { relatedTarget })` DOES reach an ancestor's React `onMouseLeave` (React
  derives leave from the bubbling mouseout): `relatedTarget: document.body` fires it, a `relatedTarget`
  inside the ancestor does not, and `fireEvent.mouseLeave(box)` fires it alone. Probe a 12-line component
  once before pinning a pointer-leaves-the-box rule.
- An amendment that removes the LAST call of a file's local helper (the fe stylesheet readers' `cascade`,
  `prop`) reds `npm --prefix fe run typecheck` with TS6133 "declared but its value is never read":
  module-level locals count. Delete the helper in the same edit.
- A save rule pinned as a comparison (the render of the rows differs from the loaded one) cannot be
  satisfied by a `dirty` flag: keep the brief's case (type, type back, expect 0 PUTs).
- What the stub (`stub.py`) serves decides what the UX pass sees: a control that renders only for certain
  values is unmeasured until the stub carries them.
- Playwright `waitFor()` counts a `.visually-hidden` element as visible, and `hasText` is a normalised
  substring of `textContent`: one marker cannot pin a `count` together with a `text` only one element
  carries, so split them. A `hover: { on, then }` acts on the FIRST match of `on`.
- A test file whose line count moves between two reads is being written by ANOTHER agent right now (the
  same slice briefed twice): stop, never rewrite it, report the collision. The shared edits (`src/test/bff.ts`,
  `fe/ux/`) survive the overwrite, the test file does not.
- A new icon button with a visually-hidden label (and a hidden reason span) inside a table cell lands in
  every existing cell-text pin of that cell: strip the buttons in those pins (clone the cell, remove
  `button`, read `textContent`), and the same for the row's own text.
- Dropping a key from the test's `EN` mirror reds EVERY test that calls `en(key)` for it: that full run's
  `FAIL` list IS the census of the pins to report. A pin that reads `en_US.json` directly (a file's own
  `S['key']`) stays green until the coder removes the key from the file, so it is missing from that census.
- A full-suite run reds the heaviest page tests with `Test timed out in 5000ms` under the load (they pass
  in isolation, at baseline and with the change): rerun those files alone before reporting a timeout.
- vitest's `toBe` takes ONE argument: a failure message passed to it is a typecheck red, not a test red.
  The message is the second argument of `expect(x, 'msg')`.
- An ENABLED save or Discard button carries no `aria-disabled` attribute at all (`getAttribute` answers
  `null`), while a clean form sets it to `"true"`: a dirty pin asserts `getAttribute(..) ?? 'false'`.
- No jest-dom matchers are configured: `toContainElement` is a typecheck red (TS2339), not a test red.
  Pin containment with `expect(row.contains(el), 'msg').toBe(true)`.
- Never leave CPU burners running and never start them unless the brief asks for a load test: they compete with the local model and Strata on the same machine. Always name every red test of a full run (grep the log for FAIL), never "name lost".
