---
name: tester-playbook
description: How the tester writes reliable tests in this project, and the pitfalls met so far. Use when writing or debugging tests.
---

Rules are in `AGENTS.md`. Format of this file: `AGENTS.md`, under "Skills".

## How to

- Run every ad-hoc script under a hard timeout: `timeout 120 python3 - <<'EOF'`, never a bare `python3 -` heredoc. Two rewrite scripts with backtracking regexes outlived their run and spun at 100% CPU for 13.5 h (2026-11-26); prefer plain string edits to multi-line regexes over whole files.

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
- A seam whose SIGNATURE changes (a Rust fn's argument list) is a COMPILE red, not a test red: add the
  smallest stub of the new signature in production (`let _ = new_arg;`, the body unchanged, marked
  `// <SLICE> STUB`), so the pins red on today's behaviour for the right reason; amend every existing
  call site of the seam in the same pass, and run clippy (the stub's unused argument is a `-D warnings`
  red). A retired source-text grep's note goes in `//` lines, not `///`: a doc block followed by a blank
  line and another doc block is clippy `empty_line_after_doc_comments`.
- clippy's `needless_borrows_for_generic_args` reds, under `-D warnings`, a `std::fs::write(p, &format!(..))`
  and a `std::fs::remove_file(&dir.join(f))`: pass them by value.
- Stub a manager verb that does not exist yet with `ManagerError::Io(std::io::ErrorKind::Unsupported)`
  (`ManagerError` has no `Other`); make the phase's refusal helper take `key: &'static str`, because
  `checks::Refusal.key` is `&'static str` (a `&str` closure argument is a lifetime red, not a test red);
  write `serde_json::from_str::<serde_json::Value>` inside a
  `json!` literal (the macro's `Into<Value>` leaves `T` ambiguous).
- A verb whose ANSWER grows a value drops that value at the transport wrappers in the slice that changes
  the signature (`manager/dbus.rs`'s `Ok(self.core(move |m| m.verb(..)).await?)`, `manager/pipe.rs`'s
  `.and_then(..).map(|()| Value::Null)`): map it away there (`|_| ()`, `|_| Value::Null`) and keep the `?`,
  since the wire shape is a later slice's. A scripted edit that adds `.map(|_| ())` and loses the `?` is
  six `E0308: expected Vec<String>, found ()`.
- A new `pai-manager` subcommand is stubbed in the existing `manager.rs` (no new production file): the
  `ManagerCommand` arm, the seam fns with a `// <SLICE> STUB` body, and the arm in `run()`. The parser
  tests need no stub (`UnknownCommand` is today's red). Keep the seam's argument list at 7: clippy
  `too_many_arguments` is a red under `-D warnings`, so pass the id to the caller (which builds the
  strings sentence) and let the seam return a plain failure enum.
- A fixture struct that holds a crate enum cannot `#[derive(Default)]`: write the fields out. A stub that
  answers the value a test expects is green BY THE STUB (a stubbed `Unreachable` passes the refused-port
  pin): name those tests in the report, their teeth show when the seam is wired.
   The same holds the other way: an empty stub answers empty on both sides, so a "a second run changes"
   nothing" compare over whole files is green before the code exists.
 - A new `crates/<crate>/src/<name>.rs` that holds tests needs `pub mod <name>;` in the crate's `lib.rs`:
   unregistered, its tests are not compiled and not run, and the filter reports green with no test named.
   Register it first, then read the test names in the output.
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
- A review rule that tells one process from another by argv[0] reddens every fixture that starts at the
  first flag: sweep the fixtures and add argv[0] in the same pass, and retarget the pins that read the old
  shape into shape-tolerant ones (every ask is X, `len >= 2`) when the fix's round count is the coder's.
- For a defect a review found in a closed slice, pin the WORDS the review proposes: an existing test that
  compares two places (the tile's alert with the page's) is green when both carry the same wrong fallback,
  so it covers the defect's shape and not its fix.
- Read an existing pin of the same rule for teeth before trusting it as a base: a GUI label compared with
  `Strings.t("key")` is a TAUTOLOGY (both sides read the same language file), so the amended words need a
  literal in the assert.
- Assert an observable (a suffix, a pattern) where a name's shape is free (a generated temp name, an
  ephemeral id): a red is a contract, and the coder is free to pick the literal.
- Pin an ORDER rule ("the record is written last") with a call that fails on the LAST step (make the
  render's path a directory so the rename fails) and assert the whole-tree snapshot is unchanged.
- Pin a migration's "a second run changes nothing" by replaying the files left ON DISK, not the clean
  tree: hand the rerun the merged file AND the file it was merged from (`models.ini` merged +
  `models.ini.disabled` still there, `assignments.ini` still there). A rerun helper that answers
  `String::new()` for the leftovers is green over the duplicate-section defect it is meant to catch.
- Pin a crash point's "the next run answers the clean run's files" ONLY where every value the clean
  result carries still exists in the inputs. A state that has already stripped the value's source (the
  server profile lost `Hostname`/`Port`, the pool lost the moved keys) makes the whole-file equality
  unsatisfiable: pin the parts that are reachable (the entry keys of the file already on disk, and that
  no value is INVENTED, e.g. the record's `Port` is not the reader's 9931 fallback) and report the
  equality as unachievable for that file.
- Report a bookkeeping pin (a log or call count over calls made by earlier pins) green today, deferred to
  the coder's green run. For a count over a seam the builder also reads, take `let base = ...` after the
  builder and assert deltas. Where the phase is red at its FIRST verb, the count pin is unreachable and no
  probe measures it: get the number by script, counting the verb-call sites in the phase's own text
  (`(?<![A-Za-z_])(verb)\(`, so `create_instance_profile(` is not counted as `instance_profile(`) and
  adding each loop's extra rounds.
- Pin a byte bound on a pipe payload twice: in the arithmetic, and at the wire crossing, where the red
  lands in the client's own frame check, which is the bound's evidence.
- Write a file-name rule for one OS as a `cfg` split, not a table row.
- In a manager fixture, the fit checks judge the instances the service answers RUNNING: keep an instance
  that must fail fit OUT of `FakeService::running` (as `ip3c_fit_split_e2e` does), else the phase's own
  setup save is refused `model-profile-over-memory` and the phase reds at its setup, not at its pins.
- The instance-profile verbs are split across the transports: `SetInstanceProfileModel` has a service fn
  and a D-Bus method and NO pipe arm; `SetModelProfile`'s frame carries the on/off switch, not a key
  value; the model profile and server profile pages save with `WriteFileIf`, whose frame REQUIRES
  `expected` (missing it is answered `BadRequest`, which `convergence.rs`'s `call` reports as
  "the connection ends clean"). Pin a bus answer with a raw `call_method` and a
  `zbus::zvariant::OwnedValue` decode: a body of `()` panics on the deserialize and a verb the bus does
  not carry panics on the call, each naming its own gap, so a D-Bus value red needs no `src` stub.
  `zbus::blocking::connection::Builder::async_io_unix_stream` on a `UnixStream::pair` serves the
  interface peer to peer; `bus.inner().set_unique_name(":1.42")` is the caller.
- The bus has no JSON: a re-rendering save's answer is the `a{sv}` of `affected_answer`
  (`manager/dbus.rs`), so decode `BTreeMap<String, OwnedValue>` (it is `Type` + `Deserialize`), read
  `running` as `Value::from(owned.try_clone()).downcast::<Vec<String>>()` and `notConverged` as an `Array`
  whose `inner()` rows downcast to `Dict`. `serde_json::to_value(&*owned)` on it panics the deserialize,
  which reads as "its body is `()`". `Dict::get::<str, String>(key)` is `E0277: the size for values of type
  str cannot be known` (the key type is `Sized`), and `String::try_from(&Value)` on a variant field answers
  `""` with NO error, silently emptying every row: match the key with `iter()` against `Value::new(key)` and
  `try_clone().downcast::<String>()` the value.
- Keep the connection a bus-serving thread hands back (`let _server = served.join()..`, never `let _ =`):
  dropping it closes the socket, and every raw call of the phase dies `I/O error: Broken pipe (os error 32)`,
  which reads as "the bus carries no such verb".
- Seed an instance profile through its verbs (`create_instance_profile`, `add_instance_profile_model`,
  `set_instance_profile_model`), never `write_file_if` of `instance-profiles.ini`: the served write has no
  arm for a file the manager writes itself, so a hand-written second profile never arrives.
- Prove a test exists with `-- --list | grep -c "^<name>:"`, never by running it.
- Judge red from the OUTPUT (`FAILED`, `panicked at`), never the exit code.

### Enumerating EVERY red of the one `data_root_e2e` in one pass

- In a scratch copy (own `CARGO_TARGET_DIR`, never the repo's), rewrite every check into a NON-FATAL mark
  with a python script, run the one `#[test]` once, and `grep IP3C-RED`: a sequential `data_root_e2e`
  prints one panic, this prints every site. The marks: `assert!(c, msg)` →
  `if !(c) { eprintln!("IP3C-RED file:line kind | {}", msg); }`, `assert_eq!(a, b, msg)` →
  `{ match (&(a), &(b)) { (ip3c_a, ip3c_b) => if ip3c_a != ip3c_b { eprintln!(…) } } }`, a bare `panic!`
  → mark + `panic!("IP3C-ABORT")` (its type divergence keeps the types). Convert the phases' files AND the
  `#[cfg(test)]` regions of the production files (the manager's 8 bus phases live inside `manager/dbus.rs`:
  converting only `tests/*.rs` loses a whole phase's marks to its first raw `assert`). Wrap each phase call
  in `tests/mod.rs` in `catch_unwind(AssertUnwindSafe(…))` printing `PANIC-IN <phase>`, and print
  `IP3C-PHASE <label>` BEFORE each call: labels go in before the first run, else the marks after a phase
  abort are unattributed and the run repeats.
- The converter must be string/comment-masked, and mask the turbofish `::<…>` lists too (fill them with
  neutral bytes): a `body::<Vec<(String, …)>>(&r)` turbofish's inner commas and `>` break the delimiter
  matcher and split the `assert_eq!` in the middle. Apply nested sites (an `assert_eq!` whose operand holds
  an `.unwrap()`) innermost-out, tracking byte deltas. `.lock().unwrap()` → a helper returning
  `MutexGuard-poison.into_inner()`: poisoning the shared fakes' mutex kills every later phase.
- For `.expect`/`.unwrap`, a helper `.ip3c(..)` that marks and hands back `T::default()` lets the phase run
  on; sites whose `T` is not `Default` (a `TempDir`, a zbus `Connection`) fail the method's bound, so harvest
  the `E0599` sites from the cargo log (`the method `ip3c` exists for …`) and rewrite them to `.ip3c_fatal(..)`
  (mark + `panic!("IP3C-ABORT")`), looping rebuild-harvest until the copy compiles. Those sites then show as
  `PANIC-IN <phase>`, not as marks: convert `.expect_err` and friends too, or enumerate what fires and
  hand-convert the firing site (one `.expect_err` was the sole abort left in the final run).
- Count marks by UNIQUE SITE, not lines (an `assert!` in a 5-field loop marks 5 times).
- A harness `use` under `#[cfg(test)]` inserted at the top of a production file SHIFTS that file's own
  source-pin: `manager_action_log_e2e` slices `include_str!(service.rs)` at the FIRST `#[cfg(test)]`, so its
  5 literal pins red in the copy as an ARTIFACT of the harness, not of the tree. Say so in the report.

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
- In a bash mutant script use `set -uo pipefail` and end the cargo pipeline with `|| true`, never `set -e`:
  a KILLED test exits nonzero, so with `-e` and `pipefail` the script dies before its `cp` restore and
  leaves the MUTANT in the real source (hit here: `let merged = merged;` survived in
  `migrate_model.rs`, found by reading the mutated line back before the next run).
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
  symlink of `fe/node_modules`, or a `git worktree add --detach` of HEAD for an inherited red. Give the copy its own `CARGO_TARGET_DIR`, never the repo's (strings are baked in by path: a shared target dir leaves binaries reading the copy's strings), and `rm -rf` it before you report. Report it
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
  YOUR files alone: keep both listings and quote them. Quicker and exact: `git status --porcelain` to list
  which `.rs` are yours, `rustfmt --edition 2021 <those files>`, then prove
  `cargo fmt -p <crate> --check` prints nothing. Where a whole-crate `cargo fmt` already ran over a dirty
  tree, prove it touched only your files by their mtimes: every other dirty file keeps the coder's own
  mtime, so fmt rewrote nothing there.

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
  `--test <name>`, and filter inside a target with `-- <filter>` after `--test`/`--lib`. Cargo takes ONE
  positional TESTNAME: a second bare word is `error: unexpected argument ... found`, so several filters
  all go after `--`. Account for
  every `N filtered out` line. `-- --exact` with a bare name runs NOTHING (`0 passed; 0 failed`, which
  reads as a missing test): exact compares the FULL path, so `-- --exact manager::service::the_probe`.
  Say which invocation covers a `#[test] #[serial]` in a lib (not
  `--test <name>`). `--lib` of a binary crate runs unit tests, `--bins` the built binary.
- Write a long cargo log to a file, grep `test result|Running|^error`, then `sed -n` each block.
- Read a copy handed over as "green there" with `diff -rq crates <copy>`: it usually carries edits beyond
  the quoted diffs. Judge every hunk and name the incomplete list.
- Run the manager's D-Bus unit tests and the GPU-probe flooding test alone
  (`cargo test -p pai-manager --lib -- --test-threads=1 data_root_e2e`, or the full path with `--exact`),
  then judge with solos plus the full-run map. Never edit the probe.
- State a pre-existing failure of states or tests the slice did not touch (UX check, flaky full-suite
  files) as pre-existing, with a proof the file is untouched (`git diff <path>`); do not chase them.
- Measure the temp-dir leak of a suite, do not hunt it by eye: `T=$(mktemp -d -p /mnt/data/tmp-tests);
  TMPDIR=$T cargo test -p <crate> --no-fail-fast; find $T -mindepth 1 -maxdepth 1 | wc -l` (target 0, and
  `find $T -mindepth 2 -maxdepth 3` names the fixture files, so the signature names the producing test).
  The guard of the class is `crates/core/src/tests/redirect_lint.rs` (scans every crate: a redirect with
  no guard bound in the four lines above, `into_path()`, `keep()`, a `TempDir` in a static).

### The e2e phases (data root, the configurator)

- Register a new phase in the same edit that writes it: `pai-core`, `pai-app`, `pai-cli` and
  `pai-manager` each have one `#[test] fn data_root_e2e()` in `src/tests/mod.rs` calling the phases in
  sequence, and the GUI's e2e is the one `#[test]` of `crates/config/src/tests/ui_bindings.rs`.
- Judge a manager D-Bus e2e red by its DURATION: `data_root_e2e` runs ~30 s, and a red whose panic is the
  `call` helper's `not a method error reply: InputOutput(Custom { kind: TimedOut, .. })` in
  `manager/dbus.rs` is zbus's 25 s call timeout crossed while the box is loaded (measured on this box:
  the SAME test 23 s, 150 s, 170 s, 293 s, 432 s, green and red, at load 6-12). Show it is not the slice
  by interleaving solo runs in the tree and in a HEAD worktree (own `CARGO_TARGET_DIR`, removed after)
  and by showing no symbol of the slice sits on the verb's path.
- A raw D-Bus call (a pin's `Connection::call_method`, not the client's `call` helper) to a method the
  interface does not serve fails at once with `I/O error: Broken pipe (os error 32)`, so the pin's own
  message is the red and the phase's `test result: finished in` is seconds, not the 25 s zbus timeout.
- Prove each phase alone when a refusal aborts at its own phase, and report the later ones as blocked,
  not passed. For a manager phase (all in `service.rs`, one `data_root_e2e`): a temporary
  `tests/ip3a_probe.rs`-style module with one `#[test]` per phase, run
  `--lib -- --test-threads=1 <probe-filter>`, shows every phase's day-one panic at once (each phase
  redirects the data root to its own tempdir, so solos are safe); delete the module before reporting.
  A probe in the slice's OWN test file, gated on an env var naming the phase, is the same proof with no
  new module to register, and needs `--test-threads=1` (it redirects the root too).
- A slice that changes what an OLDER slice's phase pins (an idempotent add that becomes a refusal, a verb
  grown from `Result<()>` to a list) reddens that older phase, which runs FIRST: the suite's `panicked at`
  names `service.rs` and an older phase's message, not the new test file, so the new phases' own reds are
  only visible in the solo probe. Sweep the older phases in the same pass: every
  `assert_eq!(verb(..), Ok(()))` of the changed verb, and each typed `Vec<(&str, Result<.., ManagerError>)>`
  of a gate phase (its annotation moves, and the verbs that keep `()` need a `.map(|()| Vec::new())`).
- A slice that moves a verb's ANSWER to a list built from the service manager retargets the older
  transports' "no verb touches a unit" pins (IP3b): the entry add, the entry removal, the entry save and
  the profile's server switch each call `running_using_profile`, which asks `status` of every instance
  running that profile, so `calls(&c)` answers `[Status(<id>)]` per instance per accepted write. A record
  verb (`SetInstanceProfile`, `SetInstanceAddress`, the creates) asks nothing, a refused write asks
  nothing, and an add whose profile no instance runs yet asks nothing. `core(true, &[])`'s `FakeService`
  answers `status` Active for EVERY id (the `running` list is empty), so with a record of two instances
  running one profile, an entry write of it is TWO calls and a write of a profile one instance runs is
  one: state that rule in the pin's message, the counts are fixture-derived.
- A rule the slice retires reds the client phases' refusal tables (IP3b): removing an entry the profile
  does not list is the keyed `no-instance-profile-model`, naming the profile and the model, not today's
  accepted no-op; a delete's `model-profile-assigned` values are the entry owners (the instance profiles
  listing it, joined by `, `), not the `assignments.ini` instances. Sweep
  `grep -n "model-profile-enabled\|assignments.ini\|models.ini.disabled" crates/manager/src/tests/*.rs`
  in the same pass: the client-transport phases seed the retired files and pin "the verb wrote it back".
- Show the masked reds of several `pub(crate) fn` e2e phases in ONE run by a temporary `#[test]` over
  each (`-- <full-path>` with `--exact`, or a bare word filter for the whole family): a sequential
  `data_root_e2e` prints one panic, and the parallel full run stops each file's phases at their own
  first. Strip the markers with a script that matches `#[test]` whose NEXT line starts the `pub(crate) fn`,
  and prove with `grep -c` over the pair.
- A pin about a file the model-data migration RETIRES reads the `.2.99.0` backup: after `start_up()`,
  `models.ini.disabled` and `assignments.ini` are gone and their text stands in
  `<name>.2.99.0`, so a phase that seeds one and pins "the verb wrote it back not" asserts the live name
  `None` AND the backup equal to the seeded bytes. The phase's own verb is usually red ahead of that pair,
  so prove the pair with a solo probe that seeds and calls `start_up()` alone.
- Seed the manager's model pool with `std::fs::write`, never the client's `write_file`: a served write of
  `models.ini` is refused `model-profile-placement-key` when its text carries a placement key
  (`checks::PLACEMENT_KEYS`, run by `checks::model_profile_placement` only once `instances.ini` reads as
  migrated), so a pool section carrying `load-on-startup` cannot arrive through the served path.
- Seed an instance profile render's expected bytes with the seam that builds them
  (`pai_core::assign::render_profile(pool, profiles, profile)`), not the retired
  `assign::render(enabled, disabled, assignment)`.
- A refusal table `for (id, key, values) in [...]` whose rows carry `&["a","b"]` is 8 `E0308: expected
  &[&str], found &&[&str; 2]`: write each row's list as `&["a", "b"][..]`.
- Today's `delete_model_profile` refuses a profile whose section sits in `models.ini` with
  `model-profile-enabled` (the switch-off-and-unassigned rule), so a one-pool fixture that deletes a
  profile no entry lists reds on THAT, before the entry-listing rule exists: pin the refusal loosely
  (a keyed refusal naming the id) for the rule the slice has not shaped yet, and exactly for the one it has.
- Pin "a search stops at the end of its range and refuses" (IP3a D3: the port walk of
  `create_instance`) by seeding the WHOLE range? No: 57 000 instances make `write_served` render one
  file each. Add the narrowest builder seam beside `with_walk_max_entries` (`with_port_search(first,
  last)` + a `port_search` field + `proposed_port(record, first, last) -> Result<_, ManagerError>`),
  stubbed to today's behaviour (walk with `saturating_add`, `let _ = last;`, no refusal) so the ONLY
  red is the missing refusal, seed only the narrowed range, and pin the real ends in their own assert
  (`(FIRST_PROPOSED_PORT, LAST_PROPOSED_PORT) == (8581, 65535)`). Keep the `mpsc::recv_timeout` guard
  as a guard: the stub's walk answers in ms, the red is the missing `no-free-port`.
- A review criterion can sit on machinery that ALREADY answers right (IP3a D4: a host with a newline
  dies in the resolver's parse, `binding` → `UnresolvableHost`, before a write; D5: every IP3a verb
  calls `authorize_as`, whose `gate_principal` runs before the action). Write those as pins with a
  teeth round (the same verbs LAND for a member, so the refusal is the gate and not a later check) and
  report them green on arrival, not as reds.
- Prove each later red of a test that masks its own reds by cutting the earlier block out of the real
  source (a python line-range delete that asserts its two anchor lines first), running
  `--lib data_root_e2e` (~7 s incremental), and restoring with `cp` + `cmp -s`.
- A gate script that walks up from a message marker to its `assert` must also stop at `assert_eq!` (a
  value pin), and its marker must be the assert's MESSAGE, not an expected literal a sibling pin shares:
  a marker with two hits aborts the gate, and gating every assert of a test to prove it green needs each
  statement's exact range (up to the macro, down to its `);`).
- Prove the several reds of ONE phase one at a time by wrapping each assert in `if false { // X-PIN-BEGIN`
  and `} // X-PIN-END` (a script that finds the message, walks up to its `assert`, down to its `);`, and
  asserts each walk is short): run, gate the red, run again, repeat. Then gate the feature's asserts ALL:
  the phase going green there proves the fixtures are sound and the reds are exactly the missing feature,
  not a typo. Restore by deleting the two marker lines and prove with `grep -c` answering 0.
- Pin a render rule through the public seam (`Manager::render_failures()` re-renders every instance from
  the files on disk and answers the failures), so a fixture the client write refuses arrives by
  `std::fs::write` of the pool plus one `render_failures()` call. A client `write_file` of `models.ini`
  refuses a `[*]` section (`invalid-model-profile-id`), so the pool's global section is a hand edit.
- A GPU-probe count pin of a fit check is judged against the instance's rendered PAIR on disk, the last
  good state (`ARCHITECTURE.md` "Convergence"): read the pair (`models@<id>.ini`, `launch@<id>.ini`)
  BEFORE the save, since a pool hand-edit (`pool_files` + `assignments`) does NOT re-render it, so a save
  of an "unchanged" profile whose pool differs from the pair DOES probe. Pin it as three: the probe asked
  AT MOST once, `kept(&pair)` (both rendered files byte-identical, the refusal wrote nothing), and
  `m.not_converged()` naming the instance with the key (`model-profile-over-memory`). The mutant of the
  count must add a probe on the path the save takes (the memory fit check's `gpus.gpus()`, not the device
  bound's), and the phase's EARLIER exact-count pin (an `asks == ["gpus"]` block) masks it: delete that
  marker block to show the later pin's teeth.
- A pin that a READ of a NOT-converged instance asks the GPU probe nothing needs the not-converged state
  made by the MEMORY check (grow `ctx-size` past a small card, the `ip3c_fit_split_e2e` fixture):
  `check_render_memory` asks the seam only once a section grew against the pair on disk or the cards
  narrowed, so a device-only refusal (`model-profile-device-outside`) probes nothing at the read and the
  pin is a day-one green with no teeth.
- A phase's local binding shadows a module helper of the same name for the REST of the function (an early
  `let rows = rows(&m, &alice)` kills the helper for a later block, which is
  `error[E0618]: expected function, found Vec<Value>`): call it by full path
  (`crate::tests::convergence::rows(..)`) and name the binding.
- Pin a save's `running` as the running instances that DRIFT at the moment it answers, not the ones the
  save moved (IP3c review): a verb that moves no render still answers a running instance an earlier save
  left off its `applied` record, and the remove that puts a pair BACK on the record answers nobody. So a
  helper that pins "the pair keeps its bytes" (`unmoved`) cannot serve a restore: pin the restore's bytes
  against the pair taken BEFORE the save that moved it.
- `remove_instance` refuses a running instance (`instance-running`, values `[id]`): a removal pin keeps the
  id OUT of `FakeService::running`, else the phase reds on that refusal and never reaches its pair pins.
  `create_instance` proposes the first port the node holds nothing on, so a recreation of a removed id
  takes the port the removed instance held, and the record's `Port` (not the server profile's own) is what
  the rendered launch carries.
- Pin a per-instance fit (IP3c: one model profile, two card sets) with each instance on its OWN server
  profile through the record's `ServerProfile` key (`desk` → `CUDA0`, `lab` → `CUDA1`): with both
  instances following the pool's `[gpu] ServerProfile` a device refusal is node-wide, and the criterion's
  "the row moves to the other instance" half has no fixture.
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
- Compare two trees by the NAMES that differ (a set diff over `(name, text)` maps), never the whole
  `Vec<(name, bytes)>`: an `assert_eq!` of a whole tree prints a wall of bytes the report cannot read (one
  such red here printed 40 KB).
- A tree compare across TWO tempdir fixtures is never byte-equal: the files name their model files by
  absolute path. Normalise with the fixture's root spelled `<root>`, and plant the NORMALISED texts at the
  crash points (a file the run rewrites then lands with the new root's paths, one it leaves alone keeps the
  placeholder).
- Show every phase's day-one state in ONE invocation with a temporary gate per phase (an early
  `return;` gated on an env var listing the phases whose red was already seen): drop it with a script
  that asserts `grep -c` of the marker answers 0. Name the phases that came green in the report (an
  absence rule is a day-one guard).
- A test helper that hands back a manager spells its generics
  (`Manager<fakes::FakeService, fakes::FakeAuthority, fakes::FakeResolver>`): bare `Manager` is
  `error[E0107]: missing generics`, and a compile red is not a red.

### The isolated data-root helper (TS2)

- An isolation body runs in a CHILD of the test binary, which re-runs the calling test function, so the
  parent learns anything from it through two channels only: the child's stdout (the helper forwards it into
  its own failure text: pin a panicking body by `catch_unwind` + `payload.contains(<the body's marker>)`),
  and a file the child writes in a folder the PARENT put in the env the child inherits (the parent cannot
  pass per-child env: the helper spawns the child). Pin that inheritance as the report file's presence,
  with the reason in the expect.
- Because the child re-runs the calling function, a `catch_unwind` around the helper call wraps the CHILD's
  run of the body too: a panicking body written in the pin's own closure is swallowed there, the child
  passes, and the parent's catcher has nothing to catch. Put a panicking body in a `#[test]` of its own with
  no catcher (the TS2b phase shape) and spawn that path; likewise a report dir the child must write into is
  made and put in the env OUTSIDE the child's function, whose preamble the child runs again.
- An `#[ignore]`d child test (so it fails no suite run of its own: run plainly it parents itself, spawns a
  grandchild, and fails on that) runs only when the helper passes `--include-ignored`. The red reads
  `passed and failed Some((0, 0))` with exit 0 and the child's line `... ignored`; `--exact` keeps the
  helper's "exactly 1 passed" rule meaningful. Prove the pair green by running the test binary directly with
  that flag (its path is in the run's `Running` line): the panicking child prints `0 passed; 1 failed`, the
  reporting one `1 passed` plus its root path, which is then gone with its parent.
- With `--nocapture` the child's panic text lands on its STDERR only (`grep -c` over `2>/dev/null` answers
  0): a marker pin has teeth because the helper embeds the child's stderr in its failure text, so say so in
  the pin's comment and prove the channel with the two split-stream runs.
- Pin `isolated`'s failure cases so an `unimplemented!()` stub cannot pass them green: assert the caught
  payload carries the child's marker, and that a 0-test path's payload names the path. A bare
  `is_err()` is green on the stub.
- A body that writes under the data root asserts `data_root().starts_with(std::env::temp_dir())` BEFORE
  the write: a body running with no redirect writes into the user's real tree.
- "Distinct roots" across two isolated tests is a RACE pin: each body writes its marker, sleeps
  ~200 ms, then asserts the sibling's marker is absent. It has teeth only while both roots live.
- `redirect_for_tests` needs a temp-dir guard bound in the 4 lines above (`redirect_lint.rs`), so a stub
  must not call it: name it in `//` lines only (the scan strips comments).
- `tempfile` is a `pai-core` DEV-dependency: a helper in the lib that the other crates' tests reach
  through `feature = "test-support"` compiles for them only once the feature carries
  `dep:tempfile`. The coder's `Cargo.toml` job, not the tester's.
- rustfmt's `reorder_modules` sorts `pub mod` lines: a comment-attached `pub mod isolated;` lands where
  the sort puts it, so place the comment with it and let `fmt --check` (read-only) say where.

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
- A script that strips comments to find which identifiers a file uses (a source lint, a file split's
  helper closure) strips STRINGS FIRST, in ONE pass with the strings listed before the comment patterns:
  `re.sub(r'//[^\n]*')` run before the string pass reads the `//` of a `https://` inside a string as a
  comment and swallows the rest of the file, so the script reports every name after it as unused and a
  split silently drops the helpers its tests call. And read a callee's body with its comments gone and
  its strings KEPT: a call inside a template literal (`` `${size(x)}` ``) is stripped with the string, so
  a full clean() hides a real dependency.
- A source lint that reads a CALL's arguments reuses the file's paren-counting argument parser (the
  `call_flags`-style reader): a naive `split(')')` truncates at a nested call's paren (a `c.product()`
  argument reads as `c.product(`), so the lint reds on the very fix it pins. A doc comment the pin walks
  must be joined across lines: prose wraps.
- A source lint that walks `read_dir(<crate>/src)` is non-recursive, so it skips that crate's `src/tests/`
  (the lib's own unit tests): for a rule about the production code that is exactly the scope, and the
  test's comment says so.
- A source lint reading `include_str!` of a SIBLING file scans that file's test module too, so a new test
  of yours that calls the seam adds a call site and reddens the lint as collateral: split the read at
  `"\n#[cfg(test)]\nmod tests"` (the repo's own idiom) before parsing, and say so in the amended lint.
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
- A strings parity test between the web JSON and the GUI TOML reads the TOML line `key = "…"` and
  unescapes `\n` (one basic string, no escaped quotes in these hints). The two files do NOT share key
  names for the same word (`option-default` is `default` in `en_US.toml`), so parity is per key PAIR the
  brief names, never by name equality: a probe proved the helper red on the web-only key before the real
  test shipped. The parity pin sits after the page's own paragraph pin, so it is a day-one guard that
  reds when one file is left behind.
- Never add keys to `src/strings/en_US.json` and never weaken the strings guard: `App.test.tsx`'s
  `1a strings` compares the test mirror `EN` (`fe/src/test/bff.ts`) with `en_US.json` key by key, so a key
  you add to `EN` is the coder's signal and that one red is the intended one, not collateral damage from
  touching the fake. A key the brief REMOVES goes out of `EN` and into that test's `removed` list; a test
  that pins its words are on no page writes the sentence as a literal constant, because `en(key)` throws
  once the key leaves the map (the red then names your test, not the slice).

### Moving a test module out of a production file (a mechanical slice)

- Sweep for a test that reads its OWN source first: `grep -rn 'mod tests {' crates/<crate>/src` and
  `grep -rn 'include_str!' crates/<crate>/src`. A source lint that slices `src[..src.find("mod tests {")]` (the one in
  `pai-app`'s `model_profiles_tests.rs`) dies on its own `expect` once the module is a `mod tests;` declaration: the fix
  is that read and nothing else, `let production = include_str!("<file>.rs");`, because the whole file IS production now.
- An `include_str!` inside a moved body keeps resolving: a `<name>_tests.rs` sibling sits in the SAME directory as the
  file it came from. That is the only reason the sibling shape is safe.
- Dedent the body by 4 (rustfmt wants a top-level file at column 0), but NEVER a line that starts inside a multi-line
  literal: those bytes are fixture data, and a fixture line at column 0 is common (`[Qwen3-8B-Q4_K_M]\r\n\` in
  `model_profiles.rs`). A string/comment-aware scanner is required to know: a plain brace counter counts the braces in
  JSON fixtures, and a `'` rule that reads `'\\'` as a closed char literal, so both lose the module's closing brace.
  The shape that needs no scanner (pai-core TS1, 8 files): the body is `lines[i+2 .. end]` where `lines[i]` is
  `#[cfg(test)]`, `lines[i+1]` is exactly `mod tests {`, and `end` is the first line after it that is exactly `}` at
  column 0; assert `end == len(lines) - 2` (the block is the file's tail) and that the line above is blank. Off by one
  (body from `i+1`) writes a `mod tests {` with no closer, and rustfmt aborts with `unclosed delimiter`, which reads as
  a broken block, not as your slice bug.
- `git diff --stat -- <the moved files>` right after the move: each file's deletions == its block size (block lines -
  the reused `#[cfg(test)]`) proves the file carried NO other role's uncommitted lines, so HEAD is a valid baseline for
  the faithfulness diff and a plain `git show HEAD:<file>` reconstruction is safe (never `git checkout` a dirty tree).
- Move only the `mod tests` block: a file may sit a `#[cfg(test)] pub(crate) fn *_e2e()` phase right ABOVE it
  (`server_cfg_defaults_e2e` in `pai-core`'s `server_cfg.rs`, called by `src/tests/mod.rs`); that fn is production code
  the e2e test links, so it stays. And a file whose tests live in TWO NAMED `#[cfg(test)] mod <name>` modules
  (`vulkan.rs`: `an2_vulkan_parser`, `an2_vulkan_join`) does not fit the `<name>_tests.rs` + `mod tests;` shape without
  renaming every test in it (a module's NAME fixes the test path): skip it and say so, one path file per named module
  is the only name-preserving shape and it breaks the `<name>_tests.rs` convention.
- Format ONLY your new files (`rustfmt --edition 2021 <file>`), never `cargo fmt -p` in a tree carrying another role's
  uncommitted lines (the one safe shape: every TRACKED file of the crate is yours, then `cargo fmt -p <crate>` proven by
  a `git status --porcelain -- <crate>` and a `git diff --stat -- <crate>` taken before and after, both byte-equal). rustfmt at column 0 joins lines that fit and unwraps single-expression closures and match arms
  (`|p| { e }` to `|p| e`, `=> { "x" }` to `=> "x",`), so prove the move by comparing each new file, whitespace-squashed,
  with `git show HEAD:<file>`'s block body, and re-run the counts after formatting.
- Prove the counts by NAME LIST, not only totals: diff the sorted `^test .* \.\.\. (ok|ignored)` lines of the before and
  after logs (`pai-app`: 581 lines, `xtask`: 431), so a module rename shows as a diff and not as the same pass count.
  Get the BEFORE log by running the crate in a `git worktree add --detach <tmp> HEAD` with the repo's own
  `CARGO_TARGET_DIR` (its deps are reused, ~1 min), then `git worktree remove <tmp>`.
- A child `mod x;` declared inside a `#[path]` module is looked up BESIDE the file that DECLARES the module, not beside
  the `#[path]` file: xtask's `common/msi.rs` tests declare `mod engine_lifecycle; mod minimal; mod two_installers;`,
  whose files sat in `common/msi/tests/`. Moved to `common/msi_tests/` they still needed
  `#[path = "msi_tests/<x>.rs"]` on each `mod`, else E0583 names `common/<x>.rs` as the path it wanted.
- Grep the moved file's AND directory's NAME repo-wide (`grep -rn "msi/tests" xtask crates installer`): a test lint skips
  files by a hardcoded path (`nothing_names_the_registry_value_the_framework_dropped` in
  `xtask/src/common/msi_tests/engine_lifecycle.rs` skipped `xtask/src/common/msi/tests`), and a doc comment in one test
  file points at a test fn in another (`names_lint.rs` named `common/msi.rs`'s test). The first is a red, the second a
  stale pointer. Shipped strings that name a generator file (the systemd unit text in `linux/system_files.rs`) are
  production data: report them, edit nothing.
- Name your scratch script with `$OPENCODE_SESSION_ID`: `/tmp/opencode/` is shared with the other roles, and a peer
  running the same slice under the same name (`ts1_move.py`) overwrites yours mid-task, so its abort reads as your own.
- `cargo check -p <crate> --all-targets --target x86_64-pc-windows-gnu -v | grep -c -- --test` answers >= 1: the proof
  the `cfg(windows)` test code was type-checked. A `#[path]` at a missing file is a hard error, so the green check also
  proves every moved file was found.

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
- Census many files' reds with per-file JSON reports: `cd fe && npx vitest run src/<f>
  --reporter=json --outputFile=/tmp/jm.json` (run from `fe/`: `npm --prefix fe test` writes no
  outputFile). The JSON carries the titles of every red, and the import-time throw of a file the
  default reporter shows as `(0 test)` (a retired `en(key)` kills the module at import).
- Judge a flaky fe red solo x3 before anything is touched; state a full-suite red that is green as a
  file, do not chase it. A red that appears ONLY in the full parallel run is invisible solo and in one
  plain run: reproduce it by running the flaky set at `--maxWorkers=4` under CPU load (N burners of
  `while :; do :; done`, ~16 on this 24 CPU box) 3 to 6 times, reading each run's `Test Files` / `Tests`
  line. The rate here is ~1 red per 3 runs of 12 files, so a green solo x3 proves nothing. Give each
  harness its own log prefix: two harnesses writing `r$i.log` overwrite each other's runs. The known
  full-run flaky set (2026-11-25, green solo x3): `ModelProfileGpu3/4/6`, `TensorPlacement1/2` (the
  Tensor placement card, the suite's heaviest files).
- The focus-after-`findByText` flake is not only the h1 arrival: a refusal's alert→focus pins (the verb,
  a not-found paragraph) flake the same way (ModelProfileDelete 22 D.6/D.7, full run 2026-11-26); harden
  with `await waitFor(() => expect(document.activeElement).toBe(el))`, the suite's precedent in
  `App.test.tsx`.
- A control closed under a `[hidden]` ancestor answers no role query: pin the closure as `queryByRole`
  null (the ModelProfilesNew1 convention) and take the element only after the choice shows it;
  `isHidden(el)` pins only an element already in hand.
- `strings.test.ts` pins that every value of `en_US.json` is used by the code: a key added ahead of the
  sub-slice that shows it (22C2 carried `hf-repo-kept` for 22E) reds there, so the key goes into `EN`
  and the file with its own slice.

### The web UI (fe/): the fake bff and fixtures

- Run the whole fe suite ONCE after changing the shared fake (`src/test/bff.ts`).
- Keep a shared route's DEFAULT answer the shape the page reads TODAY, and let each suite's own fixture
  carry the slice's future shape. Resharing the default is not the day-one red: IP5's `GET` default losing
  `enabled` (a key the page still requires) reddened 1706 tests across 100 files, and the honest red is the
  one the page's own read prints in the slice's two suites. The same for a save reply: IP5's `running`
  ARRAY is additive, and dropping the document the reply echoed reddened three save suites that pin the
  echo; keep the echo, pin the array.
- A whole-page count (card titles, per-card field tables, tab-order lists) is derived in
  `src/test/modelProfileCards.ts`, not written per suite: move the SHARED table with the slice and let each
  suite filter the moved keys out of its own `FIELDS` copy (`FIELDS_IP5_ALL.filter((f) =>
  !PLACEMENT_KEYS.includes(f.key))`), and prune the fixture rows of a moved key, so every count derives
  from the new shape. Prune the tables before reading a red: a stale row is a `TypeError` on a fixture
  lookup, which is a red for the wrong reason.
- To answer one route twice with different replies, count the calls in a closure in the route's `Answer`
  (`let n = 0; hfFiles: () => (++n === 1 ? reply(401, …) : reply(200, …))`): an `Answer` gets no call index
  (only `modelProfileCreate` does), and a mid-test `vi.spyOn(globalThis, 'fetch')` clobbers the WHOLE fake.
- A GET list option of the fake (`serverProfiles`) is `Rows | Answer` and `Answer`'s fn takes only `init`,
  so a fixture that must answer 401 on the first read and the rows on the next (the re-read after a
  sign-in) needs a closure counter in the test: `() => (++reads === 1 ? 401 : reply(200, ROWS))`, as
  `ServerProfilesDelete1.test.tsx:769` does.
- Changing a key's ENGLISH in the mirror `EN` reddens every test that looks a control up by that text, not
  just the strings test: the 17e.14 label (`instance-models-all-label` to `Served profiles`) put 11 reds in
  `InstanceModels17e.test.tsx` at once, all reading `Unable to find … group "Served profiles"`. Count them
  and say the one cause in the report; the coder's one `en_US.json` line clears them.
- A key added to the shared `serverDoc()` reddens every strict body pin of the server form: `UNCHANGED`
  (compared with `toEqual` in eleven places of `ServerSave.test.tsx`), the explicit value lists of its
  `1b.32 (5.4)` pair, and the maps `SWITCHES_UNSET`, `SWITCHES` and `CARDS`/`FIELDS` of
  `InstancePage.test.tsx`. A new FOCUSABLE field moves every tab-order list by TWO stops (the field's
  About button, then its control): `ServerSave.test.tsx` `1b 6` and `InstancePage.test.tsx`
  `TAB_PATH_FORM`. Amend each in the same pass and name them in the report.
- When the brief gives the English of some new keys and NAMES others with no words (the refusal reasons
  `api-*`, a status like `instance-profile-server-switching`), put only the brief's sentences in `EN`, so
  `1a strings` pins exactly what the brief dictates; read the wording-free keys from `strings/en_US.json`
  itself (an `en_US.json` import plus a `words(key, …)` that substitutes `{0}`), and keep a `KEYS` list of
  every key the suite reads in a final test that names the ones the file lacks. Then a missing key reddens
  one named test, and any words the coder chose are green.
- Register a new route of the fake BEFORE an existing block that matches with `path.endsWith(...)`: the
  older block answers the new path first, and its call counter counts it too (the server-profile PUT
  counter ending `/server` took `PUT /api/instances/{id}/profiles/server`), so the red is a missing answer
  and an inflated count, not the slice.
- Keep a fixture constant looked up by accessible name clear of every other control's name; call
  `cleanup()` per iteration in a test that renders in a loop (RTL cleans only between tests); put
  `Element.prototype.scrollIntoView = () => {}` in the `beforeEach` of a file whose test clicks a jump
  button.
- When you retarget an old pin onto another field or a moved read, amend the old test to the new rule and
  say in a comment which slice retired the old one. When a review SUPERSEDES a pinned declaration (a
  browser-measured fix the next defect undoes), the pair is two reds: the absence pin (`no rule names X
  declares contain:` → `toEqual([])`) is red because the sheet still carries the old declaration, and the
  replacement's positive pin is red because the sheet lacks the new one.
- Retarget a "refusal marks the field" pin onto a field the page can mark: when the page's only LOAD-time
  error came from a retired key (the model page's `fit` row), make the error instead, typing an invalid
  value into a stay-key field and pressing Save (`top-k` = `abc` → `field-error-whole-number`), and give a
  page-loop fixture a `dirty` hook run after `ready` to perform that edit. A control named by a literal
  acronym (`Top-K`) has no strings key, so `en(key)` throws and the test spells the literal.
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
- A file's `loaded()` helper that waits for the page's `table` cannot open the EMPTY state (no table ever
  lands, so its `waitFor` times out as "expected null not to be null"): open the page and settle on the
  state's own text, `await findByText(en('<x>-empty'))`. The same for a page's own loading line: while the
  app boots the shell's U17a skeleton is what shows, so `findByText`, never `getByText`, in a test that
  opens the page.
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
  `Boot.test.tsx` does. Never start a page's slice list at `SHELL.length`: the shell's stop count grows
   (11 today, the nav's About link last), so that list starts one off. Take
   `names.indexOf(<the slice's first control>)` and slice from that.
- Read a state-dependent list (devices, a select that shows a placeholder while pending) only after the
  page's `landed()` helper, the deliberate pending case excepted. The probe also gates ACTIONS that read
  it (Add rule seeds the first probed GPU): clicking one before `landed()` seeds the pending-case value
  and the test reds on a correct component.
- Under `vi.useFakeTimers()` (it breaks RTL's `waitFor`/`findBy*`: a render that needs a real macrotask
  never happens) list `toFake` as `setTimeout/clearTimeout/setInterval/clearInterval/Date`,
  flush with `act(() => advanceTimersByTimeAsync(0))` repeated, put the answer's own timeout inside the
  fake clock, and pin delay boundaries tight (`advance(299)` absent, `advance(1)` present). Poll-driven
  rows never advance under real timers.
- A file whose reds are `findBy*` misses (a section that does not exist yet, so every test opens with
  `await findByRole(..)`) uses `vi.useFakeTimers({ shouldAdvanceTime: true })` (`InstancePage.test.tsx`
  405), not the `toFake` list: under the list an awaited `findBy*` on a MISSING element polls on timers
  nobody advances, so the test hangs to vitest's own timeout and prints no assertion, and the report's
  "expected reason" is unreadable.
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
  "declares nothing else" list and its title. A declaration the criterion takes AWAY reds the same lists
  (D4.24 drops `.hint-box`'s `min-width: 7rem`: the entry goes out of H1a.12). The brief's "tests each
  criterion changes" table names the layout tests, never the sheet readers: grep them for the class and
  the property the criterion moves (`grep -n "hint-box\|status-cell\|instance-id" *.test.ts`), that is
  where the pins of a declaration live. Read an array's own tail to see whether it is sorted.
`Reflow.test.ts`'s `declares` compares the property list IN the sheet's declaration order, so amending it
fixes where in the rule the new declaration sits: add it last, and name that in the report. Check the list
before amending it: H1a.12 collects `min-height` and `min-width` ONLY (a `max-width` or
`grid-template-columns` fix reds nothing there), and `CLASSES` keeps a slice namespace
(`instance-(rename|label|name)*`, `ot*`) outside the frozen list, so a class inside one needs no entry.
A new slice whose classes the brief describes without naming them adds its own allowance beside
`ofRename` (one regex over the slice's strings namespace, `/^instance-models[-a-z0-9]*$/`): inert while
no class exists, so `Reflow.test.ts` stays green on the day, and the coder's naming cannot red the
frozen list.
- A helper that reads a declaration's VALUE (`values(decls, 'position')` returns `fixed`) must be
  compared with a value literal: an expected `'position: fixed'` is a test-side red whose output reads
  "received 'fixed'", which is right. Compare like with like, and sweep every call site of the helper.
- `ListColumns.test.tsx`'s `declsOf(sel)` matches a selector part EXACTLY, so `'th, td'` matches
  nothing and the pin passes vacuously: pass one part (`'td'`), and prove the teeth with a css mutant
  that drops the declaration from that rule.
- Its `cascade()` scores specificity by counting `.` and `#` ONLY, so a `.x td:last-child` (real 0,2,1)
  TIES with `.y td` (0,1,1) and the later rule wins: a cascade pin of the property the `:last-child`
  rule sets is a day-one GREEN guard, and the red comes from the rule's OTHER properties (the L1 review
  D1: `text-align: end` and `width: 1%` red, `white-space` green). Its `:not(...)` is evaluated on the
  ELEMENT, so a fix that scopes with `tr:not(.confirm-row)` reads as still matching (proved: that
  mutant stays red) and only a fix that adds overriding declarations to the later rule goes green: say
  so in the report, the accepted fix shape is written into the contract.
- A declaration filter must allow the space after the colon: the reader's `norm` leaves
  `position: fixed`, so `/^position:(absolute|fixed)$/` matches NOTHING and the pin passes vacuously.
  Write `/^position:\s*(absolute|fixed)$/` and prove the teeth with a css mutant that ADDS the
  declaration (a pin of an absence that stays green under that mutant is a pin of nothing).
- A frozen `:hover` (or any `toEqual`) allowlist sits after another in one test: the first red masks
  it, so fixing one list reveals the next (H2b.6's shadow list hid its hover list, which 22 D4.21's
  `.ot-add` hovers move). Run the whole test after each amendment, not just the assertion you came for.
- A slice that RETIRES a look retires its sheet rules: take the class out of `Reflow.test.ts`'s
  `CLASSES` (and the property out of its R1.1 `sets()` list) in the same edit, so the day-one red names
  the rule the coder must delete (`expected ['.status-cell', '.status-cell > span'] to deeply equal []`),
  and rewrite that suite's own pin of the retired rule as the absence pin, naming the slice that retired
  it. A class the brief describes and does not name goes into a new allowance regex beside `ofRename`
  (inert while no class exists), never into the frozen list.
- When the retired look's class serves a page the slice does NOT touch, its sheet rule STAYS (the list's
  `.status-cell` and `.model-profiles-switch` until 24's InstanceModels17e, IP6): do not demand the rule's
  removal in the reflow list, scope the absence pin to the rendered page (no element of the class on the
  Models list), and leave the rule's removal to the slice that retires the other page.
- A width the brief names (a column that stays below 480 px, no sideways scroll) is unmeasurable in
  jsdom: pin the sheet text that carries the behaviour (the break every table cell inherits, the `td` of
  `th, td { overflow-wrap: anywhere }`, plus that no rule takes the NEW column off it) and hand the
  boundary itself to `fe/ux/check.js`, whose widths are 1280 and 320, naming the exact boundary
  unproven in the report.
- Pin a look the brief gives a control ("40 px, no border, no fill") as the CASCADE for the classes the
  element carries: read `button.classList` off the page, keep the top-level rules whose selector is the
  element's tag or a compound of those classes, order them by class count then sheet order, merge their
  declarations, and assert the effective value. The pin then follows the class names the coder chose, so
  it needs no assumption; accept the shorthand or its longhand parts (`border: none` or
  `border-style: none`) where the brief states the effect. Match a selector part by its LAST token against
  the element and its EARLIER tokens against the classes of the element's ancestors (read off the DOM): a
  rule requiring every token to be the element's own misses the scoped rule the coder writes
  (`.instance-header .x`), which is a red for a correct fix.
- A reader that matches a selector part on the WHITESPACE-STRIPPED text (`compact`, `sel.replace(/\s+/g,
  '')`) never sees a part naming a TAG ancestor: `.run-actions button` collapses to `.run-actionsbutton`,
  one token, matching no element, so the cascade of a button inside the run row reads EMPTY and the pin is
  red on a correct fix. Split the part on whitespace and combinators into compounds (the
  `Instances17c.test.tsx` `scopedBy` reader, `partsOf` + `compounds`), and use the compact form only where
  every part is a single compound (`.run-remove`).
- That `scopedBy` reader reads EVERY earlier compound as an ANCESTOR, so a SIBLING rule
  (`.field-control select + button`) never reaches the element and an absence pin of such a rule through
  it passes vacuously (proved: its `reach` for the switch's button was `[]`). The sibling-aware
  `rulesReaching` of `RestartOffer17d.test.tsx` walks `+`/`~` over PREVIOUS siblings; it needs a `matched`
  flag (breaking on an empty step leaves `nodes` at the previous step, so every descendant rule reads as
  reaching: 17 false hits) and takes one compound as `compounds(part)[0]` (`compounds` answers a LIST of
  them, and a nested array matches no element, so the pin reads empty).
- Pin a line the brief gives by LOOK ("a muted line under the select") and by its RELATION to a control,
  never by a new id literal: take the ids in the control's `aria-describedby` (every one must exist in the
  document), pick the element carrying the key's text exactly, then pin it is in the control's field, after
  the control, shown, not `aria-hidden`, no tab stop, not a control, and no `aria-live`/`status`/`alert`
  (a description, not a live region), and that the text is GONE once the state changes. Read the look from
  the sheet with the same last-token rule as the cascade above (tag, id or class), so the coder's class
  stays free; "wrapping at 320 px / 200 %" is layout, unproven in jsdom, so name it for the UX check.
  Add one page-level count of the text (`getAllByText(words).length === 1`): a shared key's words landing
  in a label, hint or option elsewhere is the defect this catches.
- Pin a cascade value that today's sheet already carries globally (the `h1`'s `overflow-wrap: anywhere`)
  as a presence pin on that global rule, not as a value of the scoped merge: the merge sees only the rules
  scoped to the new box, so a global declaration reads absent and the pin is red on a correct fix.
- A review's "the line keeps no margin" is NOT "no rule reaching it declares a margin": the sheet's global
  `p { margin: 0 0 var(--space-3) }` reaches the line (0,0,1), so the absence pin reds the review's own fix
  (`margin: 0` on its class, 0,1,0). Pin the CASCADE WINNER as `0` (rules reaching it, ordered by specificity
  counted in `.`/`#` then sheet order, the shorthand expanded: `margin: a b` puts `b` above and below,
  `margin: a b c d` puts `a` above and `c` below), and keep the absence pin only for a property no global
  rule carries (`min-height`). Same for a colour: "every rule giving it `var(--danger)` names its class" is
  pinnable, "every rule setting its colour names its class" reds the global
  `.secondary { color: var(--text) }` that reaches every secondary button. Learn which rules reach an
  element with a throwaway probe that prints them as JSON in a `toBe('SHOW')` message (vitest prints a
  `console.log` under a `stdout | <file> > <test>` header when the test runs, and swallows nothing), then
  delete it.
- A pin of the rule a review TAKES AWAY, read in sheet order, is green on the defect when the global class
  rule sits later in the file: `.about` (0,1,0) is at 1819, `.card-heading .about` (0,2,0), the rule that
  cleared the padding and border, at 1558, so a last-in-file read answers the field's `var(--space-2)` for
  the card's toggle and the pin passes on the bug. A card-scoped override of a global class is a
  SPECIFICITY read (the `cascadeOf` order), never a last-declaration read.
- A review that WITHDRAWS a shape it earlier advised is pinned as absences of every form the shape took: the
  control's own text (`norm(del.textContent)` equals the key), no element children (the spans,
  `children.length === 0`), no attribute duplicating the text (`hasAttribute('aria-label')` false), the
  brief's literal "the visible text is the accessible name" through `accName`, and NO rule of the sheet
  naming a class the withdrawn parts carried (`RULES.filter(r => /class-(short|long)/.test(r.sel))` → `[]`,
  the `@media` blocks included, jsdom applying no query). The class-name pins of the surviving class
  (`save-bar-delete`) are day-one greens, the coder keeping what the code review gave; the shared
  `accName`/`words` readers of tab-order lists need no change, the name they read is unchanged.
- To list the later reds of ONE fe test (a test stops at its first assertion), write a probe with a
  `pin(fails, name, fn)` collector, one try/catch per assertion, and `expect(fails).toEqual([])`: a single
  run prints every red the first red hides. Delete it and prove with `ls | grep -c`.
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
  a strict cascade pin would write one of the two into the contract. Where the brief states the gap as
  BEFORE the control and the row's shared `gap` drives the neighbours too (the run row's `--space-2` also
  spaces the Overview's tiles), the only fix that moves one control is a margin on it: name
  `margin-inline-start: var(--space-5)` on the verb as the accepted shape in the report.
- Pin a column's narrowness (17e.13: `width: 1%`, `white-space: nowrap`) with no class assumption: find
  the cell by its column index in the `columnheader`s, read its `classList` off the page, join the bodies
  of the top-level `.class { … }` rules it names, and assert the two declarations. Today's cell carries
  `muted`, so the red reads "expected ' color: var(--muted); ' to contain 'width: 1%'", and any class the
  coder adds goes green.
- A tile's link name carries the reader-only id of 17f.3 (`instance-id-said` in a `.visually-hidden`
  span), so `getByRole('link', { name })` with the visible name alone misses it, and `link.textContent`
  is "llama.cpp :9090, instance lab". Pin the VISIBLE words by joining the link's child nodes that are
  not `.visually-hidden`, and pin the href by identity.
- Pin "the two controls share one rule" (a brief: share the class or the rule, not a copy) by reading the
  selector GROUP of the rule that styles the existing class and asserting the new control carries one of
  its parts: a comma group and a shared class both pass, a copy of the declarations does not. Read it from
  the NEW control's cascade: per pinned property, the winning rule's group must carry a part equal to the
  existing rule's part (`.run-actions`, `.run-actions button`), so a copied rule reds with the property
  named.
- Pin "ONE rule styles both" (a review asks for one rule, not two that drift apart) by the RULE'S
  IDENTITY: collect the top-level rules that declare the value AND reach each element, assert one each,
  then assert the two lists name the same rule (its offset in the sheet). The selector text is the coder's,
  so pinning the text writes the fix into the contract.
- Pin a narrow-width override (a review: "below 480 px it keeps no start margin") by merging the top-level
  rules AND the rules of the `@media (max-width: 479.98px)` block, ordered by specificity then by OFFSET IN
  THE SHEET: a rule of the same specificity inside the block sits BEFORE the top-level rule it must beat
  (the block is at `index.css` 648, `.run-remove` at 2698) and LOSES. Name that in the assertion message,
  the coder writes the obvious rule first. Read the block's rules by range (the at-rule header's
  `[open, close]` offsets, the rules at depth 1 inside), never by a flat regex over the sheet.
- Pin a `grid-column` placement as the WINNING rule's value AND the block it sits in (`depth > 0` and its
  offset inside the at-rule's `[open, end]`): a top-level `.x { grid-column: 2 }` satisfies a value-only pin
  while the review named the `@media (min-width: 720px)` block that makes the parent a grid (22C1 D1, the
  block the 19 LS1 D1 and 21 TP1 D1 pins sit in). Prove the teeth with a mutant that moves the declaration
  out of the block: the value stays `2`, the pin goes red naming the offsets.
- Pin a hover-reveal that must sit in the flow (not a popover) by what only the flow shape answers: the
  box's `mouseleave` hides it, a `mouseover` of the box (not only of the control) shows it, and no rule
  reaching it declares `position: absolute`/`fixed`. A popover that is a child of the same wrapper passes
  every "the text is in the box" pin, so the text pins alone prove nothing (22 D4.13).
- A component the page hands NEW ids keeps its own state: an offer whose ids are replaced never
  unmounts, so its previous status line and alert carry over into the new offer. Pin the ABSENCE of
  the previous words (the alerts above the table, read with `queryAllByRole('alert')` + `before(el,
  table(main))`, or in the Save bar with `within(bar)`), never a class, and pin the new offer's own
  buttons in the same test so the red names the carry-over, not a missing offer.
- The sign-in dialog's close is where the page owes the focus back: `useSignInDialog(fromId)` closes
  with the focus on `fromId` (the Sign in) and on a signed-in with the id the page passes to `close`,
  so a control inside a child component needs its own id threaded through `onSignIn`. Pin it by
  signing in THROUGH the dialog (`findByRole('dialog')`, type, submit, `waitFor` the dialog gone);
  the alert-absence assert before the focus assert masks it, so prove the focus half red by flipping
  the earlier expectations to today's value in a `cp` copy, `-t`, restore with `cmp -s`.
- Prove the teeth of a CSS pin in a scratch css + scratch test pair (`cp` both, one string swap, `-t`).
  The test reads `index.css` beside itself, so a real-tree mutant of the css, restored with `cp` and
  proved with `cmp -s`, is enough. A snippet that ends at the rule's `}` puts the added declaration
  OUTSIDE the rule (the pin stays red on a correct fix): anchor inside, and read the rule back. Insert
  it WITH its trailing `;`: the reader splits the body on `;`, so a bare `position: sticky` before the
  rule's `}` merges with the declaration above into `position: sticky left: 0` and the full fix reads
  RED, which looks like a broken pin and is a broken mutant. Inserting a whole NEW rule anchors the
  snippet at the target rule's closing `}` too: an anchor that ends mid-rule nests the new rule, the
  reader's brace count gives it depth 1, `TOP` hides it, and the "fix" mutant reads red as if the pin
  had no teeth.
- A selector naming a control only inside `:has(...)` styles the ANCESTOR (`.field-control
  label:has(> input[type='radio'])` is the label's rule), so a pin of a declaration on the input
  (D2's `margin: 0`) strips `:has\([^()]*\)` from `r.flat` first: a fix that puts the declaration on
  the label rule is a no-op on the input and must stay red.
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
- jsdom gives a `<dt>` no accessible name from its own text, so `getByRole('term', { name })` answers 0 on
  a `dl.assets` row: read the rows with `getAllByRole('term')[i]` and say in the comment that a slice
  adding a row shifts the index.
- `closest()` answers `Element`: cast to `HTMLElement` where a helper takes one (typecheck TS2345).
- A link's accessible name joins the text of its children with NO separator, so a name built as text plus
  an inline `span` reads "Coding assistantID default": pin the space as a text node in the markup
  (`textContent` and the name both), and the span's own line in the sheet (16 17f review D1).
- When a later criterion moves that span OUT of the link (22 D4.34), the pin flips and so does its sheet
  pin: `link.textContent` is the name alone, `link.nextElementSibling` is the span (a space text node
  between is fine, the CELL must still read "name ID"), `span.closest('a')` is null, and the scoped
  `display: block` that stacked the id goes, so `Reflow.test.ts`'s pin of it flips from
  `.not.toEqual([])` to `toEqual([])` and covers every selector of the comma group (`.assets .instance-id`
  too, which the list-only filter misses).
- A column added to a list reds every cell-text array of the list tests: a cell whose only text is a
  `.visually-hidden` button name reads `''` through `rows()` / `shownText`, so each row array gains a
  trailing `''` (and a header list gains the new key). It also retargets every `td:last-child` pin (the
  Status column becomes the Actions column): grep `last-child` in the sheet tests, and check the
  `td:nth-child(n)` pins, which survive when the new column lands LAST.

### The web UI (fe/): nav and order literals

- Work every row of the brief's "Older tests that change" section before reporting. Where a row retargets
  an old pin (a refusal that gains a control), delete that row from the old table and cite the new test
  file in a comment; where the prior sub-slice pinned an absence structurally (`nextElementSibling` is
  null), retarget the same selector onto the thing this slice adds.
- The nav is a SIBLING of `main` (App.tsx): a nav link read with `within(main)` answers `found 0`, so take
  `screen.findByRole('navigation', { name: en('nav-label') })` and `within(nav)`, as NodesMenu1/2 do.
- A control the sheet gives TWO runs of words, one for a wide width and one for a narrow one, with the full
  words in `aria-label` (22 D4.30 review A1: the bar's Delete and its `-long`/`-short` spans), reads DOUBLED
  through `textContent` in jsdom: "Delete model profileDelete". Every tab-order list, `getAllByRole('button')
  .map(textContent)` and words pin of it reds. Read the accessible name: `accName` of `fe/src/test/bff.ts`
  (`aria-label` first, then the text, whitespace normalised). Sweep `textContent` in the name readers of the
  files that pin that control, and keep WCAG 2.5.3 in mind: the short visible word sits inside the label.
- A tab-order name map built from `textContent` IS the accessible name of a link or a button, so a tile's
  stops read `llama.cpp :9090, instance lab` and `Start llama.cpp :9090` (17f.3's reader-only id, 24 A1.3's
  hidden instance): spell the expected names as `` `${name}${en('instance-id-said', id)}` `` and
  `` `${en(key)} ${name}` ``, the way OverviewRun's `tileButton` names the verb.
- When a list row gains a second line under its name (22 D4.35 puts the engine there, muted, inside the
  name cell), sweep EVERY place a test derives a row's id from its first cell: `querySelector('td, th')?.textContent`,
  `text(cells[0])`, `map((r) => text(cellsOf(r)[0]))` in `ListColumns`, `ModelProfiles`,
  `ModelProfilesDelete1`, `ModelProfilesFilter`, `ModelProfilesOverMemory`, `ModelProfilesSwitch`,
  `U17cSort`, `ModelProfilePage`. Read it through `rowName(cell)` (`fe/src/test/bff.ts`: the cell's text
  without its `.muted` runs). The id alone names the row's controls, so a reader built on the cell's whole
  text reds on the engine copy, and a reader that spells the engine stays red after the fix. In a NEW list
  file write that reader from the start, and match the row on the id EQUALS the name cell
  (`cellText(name, true) === id`), not `includes(id)`: `includes` answers for the first row whose id merely
  contains the wanted one (the id `small` found the row whose Server profile line reads `smalldefault`).
- Amend `MODEL_PROFILE_CARDS` in `fe/src/test/modelProfileCards.ts` when a card gains always-visible
  controls (`extraStops`: a strings key is a button, `{about: key}` its Field's About toggle,
  `{input: key}` a text input, no button), and `EDITABLE_BY_ID` in `ModelProfileSave1.test.tsx` 2h.130,
  which keeps a LOCAL copy of the editable-field map ("a test file imports no other test file"), so a new
  editable control lands in both.
- A card control that joins the tab path (22 D4.31: the card's own About toggle behind its two-line intro)
  is added ONCE, in `fieldTabStops` and `cardButtons` of `fe/src/test/modelProfileCards.ts`, and the reds
  that follow name what is MISSING from the path (`ModelProfileCards1..5` 2e.91, `ModelProfileContent`
  2d.71 and 2d.78). `ModelProfilePage` 2d.67 lists every button of main with `queryAllByRole('button')`,
  which counts a `tabIndex={-1}` button, so it goes green while the tab path stays red: that pair tells
  "the control exists in DOM order" from "it is a Tab stop", so state both when reporting.
- When that card control becomes a TAB STOP (the designer's ruling of 2026-11-27: a card's own About sits
  right after the card's h3), the order lists of the OLDER tests move and the coder's helpers do not reach
  them: in a whole-page head, the stop right after the card before it is the card's About, not
  `fieldAboutToggles()[0]` (spell it `cardAboutToggle(MODEL_PROFILE_CARDS.find((c) => c.heading ===
  '<card key>'))`), and in a card-scoped `tabbable(cardOf(main, CARD))` list it lands at the FRONT, so
  every `slice(0, n)` grows by one and a `toBeGreaterThan(n)` of a later stop shifts with it. Grep
  `field-about` and the card's heading key in `fe/src/*.test.tsx` to find the lists, and keep the pin
  tight: pin the card About THEN the first field's About, so the ruling's order is the contract.
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
  When the move is to the END of the form (22 L2.B1): the shared head list loses the bar's stops, so every
  whole-page list must APPEND the shared `pageBottomTabStops()` (it sits in
  `fe/src/test/modelProfileCards.ts` with no consumer until the move), a head-window pin that spelled
  `saveBarTabStops()` in the head (`ModelProfileSave16` 2h.197) drops it there and gains a tail check
  (`slice(-saveBarTabStops().length)`), and a file's LOCAL whole-path const (`InstancePage`'s
  `TAB_PATH_FORM`) moves its three entries to the tail in the bar's own order (Discard, Save changes,
  Delete: 22 D4.30). The doubled-name red masks the order red: fix the names first, and the order red
  that follows names the moved stops in its diff hunk (`@@ -145,6 +145,9 @@`).
- Land a new nav entry in about a dozen order lists: sweep every standalone `en('nav-...'),` line (and
  the `EN[...]` variants), then check tails (`grep -L` for the new key; `grep -rn "slice(-[0-9]"` for
  windowed lists) and `NodePage`'s list. Then rewrite what the sweep cannot: a `parseRoute` pin of
  `not-found` for the new address, a group's href list, an `aria-current` case, a "gains no such key" pin.
  A windowed list that already carries the new key reds as "expected [x, …(11)] to deeply equal
  [x, …(12)]": the ARRAY grew, the window did not, so set `slice(0, n)` / `slice(-n)` to the array's own
  length. Read a group's entry by href (`links.filter((l) => l.href === '#/profiles/server')`, one helper
  in the file), not `links[0]`: a later entry shifts every index pin of that group. The enumeration pins
  that move with it are the counts, not the words: `HearthIcons` H8a.2's `svg.icon` count and H8a.8's
  `nav:` row of its `test.each` table, `NodesMenu1`/`NodesMenu2`'s group href lists, `ProfilesNav` 5b.1's
  `toHaveLength` and `navLinks()` arrays, and `Reflow` R1.8's frozen `CLASSES` (the new list page's table
  class joins it). To find the one class a slice adds to the sheet, diff it: read `src/index.css`, strip
  `/\*..\*/` comments, match `\.([a-zA-Z][\w-]*)`, and drop the names R1.8's namespaces allow
  (`ot-*`, `instance-name-*`, `instance-models-*`, `restart-offer-*`, `field-after`, `hf-*`,
  `save-bar-delete`, `card-heading`, plus `table-scroll` and the `woff2` of a `format()`).
- Pin an alias rule on both paths (address equals the state hash: no navigation; address moves:
  `setHash`).
- Find a tile or an instance link by its address (`a.tile-id[href="#/instances/<id>"]`), not by its text,
  in a helper shared by tests of a slice that renames it: the helper then survives the name rule, and the
  name is pinned where the brief states it.
- When a brief makes a read-only field editable, amend every fixture that pinned it read-only (the
  fan-out goes through the shared cards helper) and retarget the "nothing to focus" refusals.
- Flipping a status region's PLACEMENT on a list page reddens the whole-page tab-order snapshot too, not
  only the `before()` pins: the region carries a stop (the Sign in after a 401), so `2c.53` moves with it.
  Sweep `grep -rn "<region key>" fe/src/*.test.tsx` and expect the tab-order file among the reds.
- A section that gains controls adds stops to EVERY fixture of that page: 17d.6a's server-profile switch
  adds `instance-server-switch-label` and `instance-server-switch` to `TAB_PATH_INSTANCE` and `1c.46a`,
  because the fake's default pool row has `instance: null` and so the switch renders on every default
  instance-page fixture. Check the fake's default rows before writing a tab path.

### The web UI (fe/): the UX check

- Run `npm --prefix fe run build` then `npm --prefix fe/ux run check` (about 12 minutes) and report unique
  lines (`sed 's/^chromium //..' | sort | uniq -c`), not the summary's count: it prints one line per
  browser x width x theme (x text mode), so one unmet `expect` is 18 lines.
- Launch it from the REPO ROOT: `npm --prefix fe/ux run check` run inside `fe/` resolves `fe/fe/ux` and
  dies `ENOENT` with exit 254, which reads as a broken harness.
- The check reads the BUILT `fe/dist`, which `npm test` never rebuilds: an unmet `expect` in a state you
  did not touch is often the stale site (compare `ls -l dist/index.html` with the last `fe/src` commit),
  and the stash-baseline of the next bullet is what separates the two.
- A state's `set:` keys MERGE into the stub's state and stay for every later state, so `load()`'s reset
  query must name every state-dependent key (a `used=1` left set by one state flips the next state's
  fixture; the stub's own reset list is the only place that clears them).
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
- A state for a section the coder has not built: mark it by role and text (`main [role="switch"]`,
  `main tbody tr` with a `count`), never by a class the brief leaves unnamed, and answer the routes its
  page calls in `fe/ux/stub.py` (`do_GET` and `do_PUT`), else the state fails on a 404 the slice does not
  own. `python3 -m py_compile fe/ux/stub.py` is the cheap proof of that edit.
- `open.click` must match EXACTLY ONE element: `isVisible()` throws a strict-mode violation on a selector
  with several matches (a row switch: scope it `main tbody tr:first-child .model-profiles-switch input`),
  and the throw aborts the whole state in every browser, width and theme, which reads as a broken harness.
- Probe a new stub route by hand before trusting it: `python3 fe/ux/stub.py <abs fe/dist>` (no argv[1] is
  `IndexError: list index out of range`) prints its bound port on stdout. `/__set` is a GET: a
  `curl -X POST /__set?session=out` 404s, the session stays at its default, and the route you meant to
  probe signed-out answers 200, which reads as a missing gate. The stub's POST routes carry no session
  gate at all (the 17b/17c creates), so a signed-out POST answering 201 is the stub's shape, not a hole.
- Prove which UX-check failures are pre-existing by stashing the state edits
  (`git stash push -- fe/ux/check.js fe/ux/stub.py`), running the check, and popping: the skeleton states
  (`loading-node`, `loading-overview`) flake in one browser, and the count of 18 lines per new state is
  the 6 configs x 3 browsers. Judge the run by its `^<browser> ` lines and its `summary:` line: at exit
  the stub prints `BrokenPipeError` tracebacks of its own `wfile.write`, which are shutdown noise.
- A rule about WHERE something lands (a line under a name, a head over its column, where an id breaks) has
  no jsdom pin, jsdom draws no boxes. Its home is the state's `probe: '<name>'`: a fn in `PROBES`,
  `(page, width) => string[]`, run in the main pass after the settle (the state's `expect`, `click`,
  `open`, `hover`, `focus` hooks read presence, never geometry). Scope it by `width`, skip a control
  `getComputedStyle` calls `display: none` or a rect under 2 px (the folded head is hidden
  visually-hidden-style, so it is 1 px wide and has no column to align with), and read a text node's line
  breaks with a one-character `Range` per character over the parent's first text node: the breaks are
  BROWSER-specific (`overflow-wrap: anywhere` splits a numeric run inside a word in Firefox, not in
  Chromium), so the pin's value is what one browser shows. A rule of the WIDE layout (a nav column's px, a
  label on one line beside its icon) goes in the same place: gate it the other way
  (`if (width < 960) return []`, since the nav is not a column below the sheet's breakpoint), read the label
  with a `Range` over the entry's OWN text node (a label whose box sits under the icon's bottom has moved
  whole to a line of its own, the sheet's icon rule), and call the group's pills one line each when their
  `getBoundingClientRect().height` values agree. Probes run in the MAIN pass only (the text pass reads
  `scrollWidth` alone), so a probe never reddens at 200 % text.
- Probe one state in seconds instead of the 5 minute full run: a temp `.cjs` INSIDE `fe/ux/` (`playwright`
  resolves only from there, a `/tmp/opencode/x.cjs` dies `Cannot find module 'playwright'`), spawning
  `python3 -I fe/ux/stub.py <abs fe/dist> 0` and taking its port from a stdout `data` listener (a
  `stub.stdout.read()` loop answers `null` and dies `Cannot read properties of null`), then `rm` it. A
  probe's findings depend on the BUILD, so `npm --prefix fe run build` first.
- To prove a probe you just wrote, run ITS OWN code, not a copy: write the temp file as
  `fe/ux/.probe-lib.cjs`, the text of `check.js` with the trailing `main().then(` block cut and
  `module.exports = { <the fns>, stubPort, STATES, load, rendered }` appended, and reach it from an
  `.mjs` with `createRequire` on a path inside `fe/ux/` (the copy needs `fe/ux/node_modules` to resolve
  `@axe-core/playwright`, and `check.js` is CommonJS: `require` there, so an ESM `import` of it dies
  `require is not defined`). Take the port with the harness's `stubPort` (its first stdout line is the
  bare number, a `127.0.0.1:(\d+)` regex never matches and the probe hangs to its timeout), and call the
  harness's `rendered(page, state)` before the probe: the harness gates every probe behind it, so a probe
  that reads a nav which renders after its reads sees an empty group and reports a false red.
- A new stub `/__set` key goes in `DEFAULTS` (the stub's own reset list) AND in the reset query of
  `check.js`'s `load()`, and the fixture for a wrap rule takes the id cap (32 characters) WITH break
  points, so the cell is at its narrowest where the id has to break.
- `npx --prefix fe prettier --write fe/ux/check.js` is right (`fe/.prettierrc.json` is found from the
  file's own path, `fe/.prettierignore` keeps `ux/` out of `format:check`). Prove it moves no foreign line
  by formatting a copy of `git show HEAD:fe/ux/check.js` placed INSIDE `fe/ux/` and diffing it (0 lines).
  Formatting that copy outside `fe/` (in `/tmp`) applies Prettier's defaults, double quotes and semicolons,
  1014 changed lines, which reads as you having reformatted the whole harness.

### Behaviour that cannot run here

- Pin Windows, xtask packaging and CI behaviour as text and type checks only, and state in the report what
  Linux can and cannot prove.
- A real non-loopback socket pin (an address-aware HTTP read): take the host's own LAN address with a UDP
  socket `bind("0.0.0.0:0")` then `connect("192.0.2.1:80")` and read `local_addr()` (no packet leaves, the
  route is only picked), assert `!is_loopback()`, and bind the listener on that address. Here that is
  `enp14s0` 192.168.50.210, and `127.0.0.1:9931` (`server_cfg::DEFAULT_PORT`) answers refused, so a
  scripted seam, not a real read, must drive the default-port pins.
- Return that probe as an `Option` and skip the test with an `eprintln!("skipped: …")` when the connect,
  the bind or the address fails: a CI box with no route must not read as a red. Prove the skip with a
  mutant that points the connect at `256.256.256.256:80` (the parse fails, the helper answers `None`).
- A pin of a header the client WRITES needs a listener that captures the request head and sends it back
  over a channel: the slice's existing listener drains the head and discards it, so it pins a status and
  never a header. Bind `[::1]:0` for an IPv6 pin, and skip when that bind fails (a host with no IPv6).
  `Response` has no `Debug`: print `res.status` or the `Err` string, never `{res:?}`.

## Pitfalls

### A cargo run

- A run with no `Compiling <crate>` line judged a STALE binary: an assertion that passes in a run and
  fails in a probe of the same phase is the stale binary, never a flake.
- `cmd | tail` reports tail's 0, so a mutant that survives reads green.
- A cargo test binary does not inherit a variable set on the command line for a harness process; a unit
  test body does see it.
- Piping a long cargo log into `grep` dies on SIGPIPE.
- Run `cargo clippy -p <crate> --all-targets -- -D warnings` over a new phase before reporting: reds in
  YOUR test code are yours. New code that copies an older phase's `std::fs::write(p, &helper(..))` where
  the helper returns a `String` is clippy `needless_borrows_for_generic_args` (four sites here): drop the
  `&`. A closure the `on(..)`-style helper needs as `'static` must be `move`, and a `move` closure that
  calls the phase's `pool()` / `record()` file reader MOVES it, so read the revision into a `let` outside
  and pass `&rev`.
- A background run killed by the shell's timeout leaves a partial log full of `test result: ok` and no
  `FAILED`: judge a run complete only by its own exit line, and rerun the crate whole.
- `rm -rf` of a scratch copy WITH its cargo target dir runs minutes: it crosses the 5 min foreground
  timeout and lands in the background, which reads as a failed cleanup. Give the delete a long timeout
  or run it in the background, and prove the tree is gone with `ls -d <copy>` before reporting.
- Setting `PRIVATE_AI_FE_DIST` reruns the bff's build script and recompiles every target of the crate (it
  falls back to `crates/bff/fe-fixture`): set it only to escape a stale `fe/dist`. `PAI_BFF_FE_DIR` in the
  shell does nothing.
- `--test '*'` names no target for `pai-manager`.
- `cargo test -p pai-manager ip3c` runs nothing (every phase is a call inside the one `data_root_e2e`):
  the filter is `data_root_e2e`, with `--test-threads=1`, and the phases red one at a time. To show each
  phase's own red, swap the phase calls in `src/tests/mod.rs` temporarily, then put them back.
- `cargo fmt -p <crate>` rewrites EVERY file of the crate, the other agents' uncommitted work included:
  format one file with `rustfmt --edition 2021 <file>` (the crates' edition), and check with `--check`.
- The manager's D-Bus unit tests and the GPU-probe flooding test are load-sensitive (slow under a loaded
  machine, bound reds at 0..1800 ms).
- A test that touches the live `bench.db` path hangs the crate's suites behind the machine-wide lock.
- A `CARGO_TARGET_DIR` shared with a scratch copy keeps artifacts compiled THERE: `paths::strings_dir()`
  is `env!("CARGO_MANIFEST_DIR")/strings`, baked at compile time, so the runtime read of the strings
  files fails and EVERY test that resolves a key to its text reds with the key
  ("the key, not its text"), deterministically solo and with `--test-threads=1`. Probe
  `panic!("strings_dir={:?} t={}", pai_core::paths::strings_dir(), strings::t("<key>"))`;
  `cargo clean -p pai-core` + rerun is green. Never call it a parallel interaction.
- An attribute glued to the end of a doc comment leaves the test unregistered: it never runs, and clippy
  says `dead_code`.
- `map.get(k).is_none()` is a clippy `unnecessary_get_then_check` red under `-D warnings`: pin a key's
  ABSENCE with `!map.contains_key(k)`.
- A source lint over a `cfg(windows)` file is green on inspection, never "test passed", and "nothing runs
  here" is not proof a later guard passes.
- A text red is often a missing strings key: `en()` throws `no English for <key> in the test`. Look the
  key up in `fe/src/strings/en_US.json` first.
- Changing a string in `test/bff.ts`'s `EN` (splitting an `-intro` key, adding a `-about` key) goes red
  in every test that locates a control by that text (`ModelProfileGpu5`'s `factsEl` finds the card intro
  with `en(key)`): fan-out, not a typo. Prove it by patching `strings/en_US.json` the same way, running
  the file (its reds clear, the new behaviour's reds stay), and restoring the json from a backup.
- The fe files' local CSS reader (`[^{}]+\{([^{}]*)\}`) captures the rules nested in an `@media` block
  too, and `el.matches(sel)` ignores the block: a `display` pin that means "at the wide width" has to
  filter the rules by the at-rule's own offsets (`atRanges` + the match index).
- A card's About toggle renders only when its intro key has more than one line, so a shorter intro drops
  it from the Tab path: the card tab-order tests (`TensorPlacement1` 21 T.9) are part of that fan-out.
- A card that gains a control beside its heading puts the `h3` in a row of its own (`.card-heading`,
  22 D4.31 review A2), so every `section.card > h3` selector in the older tests finds NOTHING
  (`HearthAssets` H11a.11's ten-card order): grep `card > h3` when a heading gains a neighbour. A role
  query (`getByRole('heading', { name })`) is unaffected, which hides the break in the DOM-shape pins only.

- `redirect_for_tests` sets `PRIVATE_AI_DATA_ROOT` for the WHOLE process and never restores it, so a
  `TempDir` bound in a nested block is the leak: the block drops it, the redirect still points there, and
  the next write under the data root recreates `$TMPDIR/.tmpXXXXXX/<tree>` that nothing owns (one per
  test). The safe shapes are NOT leaks, do not "fix" them: a helper returning `(TempDir, PathBuf)`, a
  closure `|| -> (TempDir, PathBuf)`, `let (_d, on, off) = helper()`; the guard travels with the path.
- A source lint that matches braces to find a block range is worthless on this code: `format!("{x}")` and
  raw strings move the depth, so an fn-level guard reads as a 500-line block and every write in the file
  is flagged (hf.rs gave 24 false flags). Keep a lint to per-line rules and prove a scope rule by running
  the suite with a fresh `TMPDIR`.

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
- `pai_core::strings`' `READER` is a process-wide `OnceLock`: the FIRST test that calls `manager::run()`
  sets it to `Cli` for the whole process, so in a full parallel run the e2e's string reads flip to
  `cli.toml`. One caller exists at HEAD (`save-slots`); every `run()` test a slice adds raises the odds.
- `slot_save::slot_target` answers `None` for an argv with no `--slot-save-path` (snapshots off): a
  cross-check pin "the readiness and the save ask the same port" needs the flag IN the fixture, else the
  red is the fixture, not the rule.
- One `data_root_e2e` masks its own later reds: `pai-manager`'s service phases AND its D-Bus/pipe client
  phases are calls of ONE test, so a full run prints only the first panic. To show a LATER phase's
  day-one state, comment out the earlier phase's one CALL LINE in `src/tests/mod.rs` (a `// GATE`
  prefix), run, restore with `cp` + `cmp -s` and `grep -c "// GATE"` answering 0: one line per phase,
  no block surgery. A script that gates every registration up to a marker must gate the `#[cfg(..)]`
  attribute ABOVE each call too (an orphan attribute is a compile red, not a red), and the run works from
  one saved un-gated copy of `mod.rs` (`cp` it right after registering the phases, gate from the copy,
  restore from it).
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
- A zbus signature string concatenates its members with NO separator: `(String, Vec<String>, String)` is
  `(sass)`, `Vec<(String, String, Vec<String>)>` is `a(ssas)`, a unit reply is `""`. A pin that writes
  `(sas s)` never matches and the red reads like a missing method.
- `instance-profiles.ini` is NOT a served `ConfigFile` (`config_file` answers `None`, so a client
  `read_file` of it is `BadFileName`): the pool's CRUD writes go through the `ConfigFile::Models` slot of
  `write_served_file`, and the pool's LIST answers no revision (the file is one text) while its single
  read answers the whole text as the revision a compare-and-swap write carries back.
- `checks::check_binding` walks `paths::instance_ids()` with `skip` = the instance being written, then the
  bff's listen, so an address test needs the whole node: `127.0.0.1:8681` collides with an instance bound
  `0.0.0.0:8681` (the refusal names that instance), and with no `bff.ini` in the tree the web service's
  address is the DEFAULT `127.0.0.1:8590` (`AddressInUse("private-ai-bff")`).
- An uncalled e2e phase is invisible: its solo run looks green.
- A created instance's listed `port` is the RECORD's `Port` (`launch::with_record_address` overrides the
  server profile), and `create_instance` proposes it as the first free one at or above 8581
  (`proposed_port` walks the record's own `Port` keys): a listing pin after a create reads 8581 / 8582,
  never the port of the profile the create named, and an earlier `SetInstanceAddress` in the same phase
  moves what the NEXT create proposes, so a comment naming "the node holds 8581" goes stale.
- `pai_core::bff_cfg::versions_dir()` answers `None` under a test redirect (it resolves only when the
  config dir is exactly `/etc/llama.cpp`), so the engine's installed backends (`versions::installed`,
  the `llama.cpp` and `llama.cpp-<backend>` package files) cannot be pinned through a real path in an
  e2e phase: the rule needs a seam the test can point at a fixture dir, and a fixture that writes every
  plausible dir to cover the coder's choice is an assumption to name in the report.
- A tree with the earlier phase call lines gated prints ~33 `dead_code` warnings (each gated phase is now
  uncalled): run clippy only on the restored tree, and read those warnings as the gate.
- In a convergence fixture whose record sections carry their own `ServerProfile` (the shape that gives two
  instances running ONE instance profile on distinct cards), `SetServerProfile` moves the `ServerProfile`
  of the instance profile both run and changes NO render (the record's own key wins in
  `paths::instance_server_profile`), and `SetModelProfile` writes `assignments.ini`, inert once the record
  names an instance profile (its entry sections ARE the model list). Both land and answer the affected
  object with nothing moved, so a "the pair of X moves" pin cannot host them: pin them as landing with the
  pair's bytes unchanged, and name the render that DOES move (an entry save, an instance profile switch).
- An untracked file shows NOTHING in `git diff`: a test file a slice added is `??` in `git status
  --short`, so review it by reading it, not by the diff.
- A scripted probe insertion anchored on a LINE PREFIX splits the line: an anchor ending
  `    let text_of` matches the prefix of `let text_of = |..|` and the after-insert lands INSIDE
  it, and an insert before a `pub(crate) fn` line wedges between its `#[cfg(test)]` attribute and
  the fn, so the attribute binds the inserted helper. Anchor whole lines that end with `\n`, and
  append helper fns at EOF (these phase files end inside `#[cfg(test)]`).
- A zbus `Dict` iter gives `(&Value, &Value)`: there is no `.as_str()`; decode with
  `.try_clone()` then `.downcast::<String>()` (the `convergence.rs` idiom; a borrowed
  `Value::downcast` works, `downcast_ref`'s return needs no extra step only for Copy targets).
  Pair digests settle Root A pins fast: probe `applied_digest`/`pair_digest` beside the save in a
  scratch copy and read which pair's BYTES move (`pair_moved` = record present AND digests
  differ; no `applied` record = never asked).

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

### xtask

- xtask is a BINARY crate: `cargo test -p xtask` runs `--bin xtask`, and `linux/` is
  `#[cfg(target_os = "linux")]`, `windows/` `#[cfg(windows)]`, so a phase's tests exist on one OS only
  (the shared code's, in `common/`, run everywhere): state which OS can prove a pin. `--lib` answers
  `no library targets found`; reach the unit tests with `--bins` and filter after `--`.
- A new bundled library's licence is pinned inside `common/licenses.rs`'s own tests: two rows (the `.so`
  and the `.dll`) in `a_library_finds_its_licence_under_any_os_s_name` (its loop panics at the FIRST bad
  row, so the `.dll` row's red is masked by the `.so` row and they share one cause), and a
  `<name>_text_is_vendored_in_the_repo` presence pin over `installer/licenses/`. The SPDX half needs no
  new test: `the_rpm_field_names_every_text_a_linux_package_can_carry` walks TABLE and reds a row whose
  staged file has no `spdx` arm. `stem` strips `lib` and cuts at the first `.`, so
  `libtensilelite-host.so.1` maps under the prefix `tensilelite-host`.
- A stub seam in a binary crate needs `#[allow(dead_code)]` (a `pub fn` of a bin is dead until wired)
  AND `let _ = arg;` for each unused argument, else `cargo clippy -p xtask --all-targets -- -D warnings`
  reds the stub, not the tests.
- The licence seams' LAST argument is the web UI flag, which `call_flags` pops: a new seam argument
  (`product`) goes BEFORE it, or every `every_licence_call_passes_*` lint reds on the wiring.
- A rule about `windows/package.rs` / `macos/package.rs` is pinned as a source lint in
  `common/licenses.rs` (the file's own precedent, `include_str!("../windows/package.rs")`), since those
  phases compile on their OS only; prove the Windows phase still compiles with
  `cargo check -p xtask --all-targets --target x86_64-pc-windows-gnu` (the target and mingw gcc are
  installed here), and say a macOS-only edit cannot be compiled on this box.
- A package's `README.txt` is `licenses::stage_in`'s output, and the Linux copyright files embed that
  file, so a rule about a copyright's words is pinned by staging the product into a temp dir and feeding
  `README.txt` to `runtime_copyright`, never by rewriting the composition (`run()` builds it inline).
- A temp-dir fixture here is `std::env::temp_dir().join(format!("xtask-<name>-{pid}"))` + `remove_dir_all`
  on drop (no tempfile dep), with a `name` unique per test since `pid` is shared across parallel tests;
  relative paths written with `/` join fine on Windows, so a fixture's rel path is OS-free.
- A path a check PRINTS in its `bail!` is pinned by joining the components one by one
  (`dir.join("ui").join("apps")..`), never a `/`-joined literal: the literal reds on Windows' separators.
- `ui::warn`/`ui::dim`/`ui::say` go to the process stdout, uncapturable in a unit test, so a check's
  report branch (which provisioning path ran) has no teeth here: pin only the `Ok`/`Err` outcome and the
  `bail!` message, and say so in the report.
- `linux/package.rs` tests read `run()` as the text from `pub fn run(` to the NEXT `\nfn ` in the file, so
  a seam extracted from inside `run()` goes OUTSIDE that slice (before `pub fn run(`, beside `place` and
  `runpath`): a helper inserted after it truncates `run()` for four source-text pins. Prove the extraction
  with `cargo test -p xtask linux::package`.
- The staged-path bases in `linux/package.rs`: `LIB_DIR` is `usr/lib/private-ai`, `RT_DIR` is
  `usr/lib/private-ai/lib` and a dist-relative path starts with `lib/` too, so an expected
  `format!("{RT_DIR}/lib/llvm/lib/x.so")` reads `lib/lib/` and reddens a correct fix: base the expected
  staged path on `LIB_DIR`. `components::of` answers the owning package for a staged path
  (`lib/llvm/lib/libomp.so` is the engine's, its deep twin under a `x86_64-unknown-linux-gnu/` dir is the
  ROCm package's), so a placement rule pins the owner beside the path.

### The web UI (fe/)

- `fe/ux/` is in `.prettierignore` (`npm --prefix fe run format:check` never reads it): running
  `prettier --write` on `fe/ux/check.js` reflows lines the check does not care about, and the coder's diff
  then carries a tester's reflow of lines outside the slice. Edit those files by hand.
- `fe/ux/stub.py` answers at module level (`SERVER.serve_forever()`, no `__main__` guard): a
  `python3 -c "import stub"` written to read its fixture dicts binds a port and never returns, and the
  shell jobs queued behind it in the same session print nothing at all, which reads as a broken script.
  Read the file's dicts, and drive the routes only as `python3 fe/ux/stub.py <abs fe/dist>` in the
  background, killed by pid.
- Inside a probe's `page.evaluate`: a `createTreeWalker(…, SHOW_TEXT)` node has no `.closest` (ask its
  `parentElement`), and `check.js` is plain CommonJS JavaScript, not TypeScript: a `!` non-null assertion is
  `SyntaxError: missing ) after argument list` at load. Each kills the run before a state line prints,
  which reads as a broken harness: `node --check fe/ux/check.js` catches the second, a temp probe of the
  state (above) catches the first.
- `npx vitest` from the repo root resolves another config and the node environment: every test reds with
  `window is not defined`. `npx --prefix fe vitest run src/x.test.tsx` is the same trap: the reds are
  `TypeError: Cannot read properties of undefined (reading 'restore')` in every file's `afterEach`
  (`dialogs.restore()`), which reads as a broken teardown, not as the wrong harness. Use
  `npm --prefix fe test -- <file-part>`.
- `--reporter=basic` (vitest 5 dropped it) prints NOTHING, so a grep for the test names answers empty and
  reads as "the tests do not exist": run with the default reporter and read `Test Files` / `Tests`.
- `vitest run --list` (vitest 5) dies with `CACError: Unknown option --list`, and the default reporter of a
  green run prints NO per-file line, so a grep for a file's name answers empty: prove a test is registered
  with `-t '<title part>'` and read the `Tests N passed | M skipped` split.
- In this environment `npm --prefix fe test` exits 1 with an `EISDIR ... getPackageScopeConfig` and a
  `jsdom` message while every file passed: judge from the `Test Files` / `Tests` lines.
- A full `npm --prefix fe test` run's headline numbers move between runs (file count, test count, red
  count: 124/2474/39 and 156/1861/55 for the same tree), and a file red under the suite's load is green
  solo (`Instances17c` 17c.7a, a focus race): census the reds from the per-file `❯ src/…  N failed` lines
  and re-baseline the files you touched in a HEAD worktree, never against a remembered total.
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
- When the halves of one test must be COMPARED (one rule styling two pages) and cannot be split, read both
  halves into locals BEFORE the first assertion, and prove the later half is red for the defect's reason by
  temporarily flipping the earlier expectation to today's value, running with `-t`, and restoring from a
  `cp` copy proved with `cmp -s`. A helper that throws (a page-open helper that forgot `render(<App />)`
  after `cleanup()`) is otherwise invisible behind the first red.
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
- A page-open helper that renders without `cleanup()` first leaves TWO trees when a test opens two pages:
  every `getByRole('main')` after it reds with "Found multiple elements with the role \"main\"", which
  reads as a broken selector, not as the second render.
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
- Changing the TEXT of a shared key (adding none) fans out to every table that lists the words: the card
  files' `LOAD`/`FIELDS` `options` and `absentOption`, and the local option-text arrays of the save tests.
  Grep the key AND the old words: the FIELDS-driven tests (the Hardware card's `2e.107`, `2e.109`,
  `2e.110`) red beside the slice's own, and the brief's list names none of them.
- A full-suite run reds the heaviest page tests with `Test timed out in 5000ms` under the load (they pass
  in isolation, at baseline and with the change): rerun those files alone before reporting a timeout.
- vitest's `toBe` takes ONE argument: a failure message passed to it is a typecheck red, not a test red.
  The message is the second argument of `expect(x, 'msg')`.
- An ENABLED save or Discard button carries no `aria-disabled` attribute at all (`getAttribute` answers
  `null`), while a clean form sets it to `"true"`: a dirty pin asserts `getAttribute(..) ?? 'false'`.
- No jest-dom matchers are configured: `toContainElement` is a typecheck red (TS2339), not a test red.
  Pin containment with `expect(row.contains(el), 'msg').toBe(true)`.
- Never leave CPU burners running and never start them unless the brief asks for a load test: they compete with the local model and Strata on the same machine. Always name every red test of a full run (grep the log for FAIL), never "name lost".
- A whole-suite run shows `App.test.tsx` `1a.5` (the current page's link carries `aria-current="page"`) red
  once, while that file is green solo and the suite is green just before and just after: on a nav slice,
  rerun the file and then the suite before reporting, so a load flake is not reported as the slice's red.
- A labelling element that also carries a decorative muted line names the control with it: the Models row's switch read `Qwen3-27Bllama.cpp` once 22 D4.35 put the engine inside the `aria-labelledby` target, so 77 `getByRole('switch', {name: id})` and tab-name pins went red across ten files from ONE markup defect. Report the fan-out as one defect, sweep the test-side id readers to `rowName`, and pin the rule (the labelling element holds the id alone, the line sits beside it) in the slice's own file.
