---
name: tester-playbook
description: What the tester has learned about writing reliable tests. Use when writing or debugging tests.
---

Lessons only: the rules are in `AGENTS.md`.

## Proving a red, and what a run proves

- Never wait without a deadline (`recv_timeout`, a `try_wait` loop, `method_timeout`): a test must fail,
  not hang. `recv_timeout` returns `Disconnected` at once when the thread panicked (a `todo!` stub):
  report that apart from `Timeout`, or the red reads as a hang.
- A Rust test that cannot compile is not a red. Before reporting a list of tests that should fail, write
  a throwaway test that exercises the new signature (call it, print the result, the `panicked at .. not a
  future` of a non-async block), run it, then delete it and write the real tests. Report the shape.
- A throwaway is a probe of reachability, not of behaviour: it shows a value is constructible and a call
  compiles, not that a call site exists. Name it in the report as a probe.
- When the coder's work already contains the rule, the test you expect red is green. Say so, and add an
  assertion that fails for a reason you cannot fix: the EN strings the brief gives that `en_US.json`
  lacks, the key pair that must be equal.
- In an e2e of many phases in one `#[test]`, a refusal aborts at its own phase, so every later phase is
  "unproven, not passed". Prove each phase alone, then report the phases the red blocks as blocked,
  never as passing.
- When a phase can only be proven in a red state, seed the pre-save state into a file and show the
  refusal, then write the final test asserting the good outcome. The red of a test that asserts success,
  naming the exact refusal, is a better report than an artificial case.
- `cargo test -p <crate>` runs no target when one target fails to compile (`--no-fail-fast` only unrolls
  the run). Report which binaries did not run, and run the untouched binaries in a second invocation
  (`--lib --test a --test b`) to show them green.
- `cargo test -p <crate> <word>` filters test names, never target names: `cargo test -p pai-bff listen`
  skipped most of `tests/listen.rs`. Run a file with `--test <name>` and account for every
  `N filtered out` line in the report.
- A long cargo log: write it to a file (never pipe it through `grep`, the SIGPIPE trap), then grep
  `test result|Running|^error` for the map and `sed -n '<a>,<b>p'` for each block: the `tail` of a
  two-binary run cuts the second binary's failure block.
- A cargo test binary does not inherit the shell environment: a variable set for one run
  (`RUST_BACKTRACE=1 cargo test ..`) reaches the harness as absent. Prove the harness with
  `cargo run -p <crate> --bin <name>`, and gate a behaviour on the variable in the test body, not at
  the module.
- A unit test over a temp dir is not a data-root e2e, and a temp-dir test is no proof of the e2e: the
  tree redirects to a per-test temp dir under `PRIVATE_AI_DATA_ROOT`, so it passes with no data-root
  seam at all, and the same input can fail in the e2e, where the tree is a different shape.
- `#[test] #[serial]` in a crate's lib is NOT covered by `cargo test --test <name>` (that names an
  integration binary), nor by a `--test <name> -- <filter>` run. Say which invocation covers it.
- A file-name rule that applies to one OS only is a `cfg` split, not an extra table row: a row aimed at
  Windows reds every Linux run of the same test. Pin the Windows half in the `cfg(windows)` block, keep
  the cross-platform rule in a shared helper, and say what Linux can and cannot prove.
- "Nothing runs here" is not proof a later guard passes: a source lint over a file no test on this OS
  compiles (a `cfg(windows)` module) is green on inspection, never "test passed". Say the file is
  Windows-only.
- A red test is a contract, not a hint about a name: where a name's shape is genuinely free (a generated
  temp name, an ephemeral id), assert an observable (a suffix that survives a rename, a pattern), not the
  literal the coder is free to pick.
- A shared lock is machine-wide, not per test binary: a test that touches the live `bench.db` path hangs
  the whole crate's suite behind the other binary's guard for ~120 s, until the lock-holder exits. Tests
  use a temp db; the shared path is read in a dedicated single-binary run.
- "No red for this criterion" is verified by the coder's diff: a criterion that is only an absence (the
  save writes no key) is pinned as `!doc.contains_key(..)`, which greens on day one; a test the coder
  must EDIT to go green is a stronger pin than a test that merely passes.

## The e2e phases (data root, the configurator)

- `pai-core`, `pai-app`, `pai-cli` and `pai-manager` each have one plain `#[test] fn data_root_e2e()` in
  `src/tests/mod.rs`, calling the modules' `<name>_e2e()` phase fns in sequence (`paths::redirect_for_tests`
  sets `PRIVATE_AI_DATA_ROOT` in-process; no flag, `--ignored` or feature gates it). The GUI has no
  `data_root_e2e`: its e2e is the single `#[test]` of `crates/config/src/tests/ui_bindings.rs`, which
  calls `save_flow`'s plain fn (`crates/config/src/tests/mod.rs`).
- Register a new phase in the body of `data_root_e2e` in the same edit that writes it. A phase that is not
  called there is invisible to the e2e run: its solo run looks green and the full-suite failure blames
  the tree.
- A phase that drives a real save goes through `config_seam_e2e` (`crates/app/src/server_control.rs`:
  redirect the tree, save through the seam, reload, assert), never through a direct write of
  the files: that passes without the seam.
- "Nothing left in the user tree" is a state of the tree, not of the process: assert it at the END of a
  phase, where it belongs, not as an isolated mid-phase probe.
- The GUI e2e is one `#[test]`, so a production line that half-did a rename kills it at its first phase
  and the later phases report nothing. Prove a pre-existing red predates your edit (`cp` the pristine
  file from your own backup, run, restore, `grep -c` to confirm the restore), name the production line
  the coder must flip, and say which of your asserts the red blocks.

## The cargo harness in this repo

- An e2e phase redirects the data tree to a temp dir with `paths::redirect_for_tests`, which asserts the
  paths follow it and stops before touching a real tree.
- `--lib` of a binary crate runs the unit tests; `--bins` runs the built binary and exits. `--lib` of
  `pai-app` prints `0 passed; 0 failed; 36 filtered out` for a filter that names an integration test: the
  filter belongs to `--test <name>`.
- `cargo test -p <crate> -- <filter>`: cargo eats the filter for the test target (0 tests, exit 101).
  Reach a target with `--test <name>`; reach a `#[path]` module's tests with a unique filter (a module
  prefix), then run the whole crate to show the rest green.
- `crates/app/src/tests/gate_lint.rs` is pulled in by `#[path]` from `crates/cli/src/tests/mod.rs` (and
  `crates/config/src/tests/control_lint.rs`), not declared in app's `tests/mod.rs`.
- The bff's unit-test list is in `crates/bff/src/lib.rs` (its `tests/*.rs` are NOT the unit tests). The
  bff's `build.rs` embeds the web UI from `PAI_BFF_FE_DIR`, so `cargo test -p pai-bff` fails "the web UI
  is not built" until `xtask build framework` has run; to test the bff without it, set
  `PAI_BFF_FE_DIR=<repo>/crates/bff/fe-fixture` (a stub `index.html`). `tests/fe.rs` needs the real
  `fe/dist`.
- The manager's D-Bus unit tests are slow and flaky on a loaded machine: green on an idle one, reds at
  0..1800 ms (the 10 ms bound of the token probe) under load. Run them alone, then judge:
  `cargo test -p pai-manager --lib -- --test-threads=1 data_root_e2e` (`dbus_client` is a module of that
  test).
- A `tokio::time::sleep` in an async test on a current-thread runtime is real wall clock, not advanced by
  the test's manual runtime pumps: it burns the whole budget (1200 ms of sleeps, 2.6 s) and greens
  nothing. Drive the timeout with `tokio::time::pause()`, and use
  `#[tokio::test(flavor = "multi_thread")]` when a real sleep is wanted.
- The web UI's tests run from `fe/` only (`npm --prefix fe test`, `npx vitest run` inside `fe`): `npx
  vitest` from the repo root resolves a different config and lies about the files it ran.
- The type check that sees the test files is `npm --prefix fe run typecheck`; a bare
  `tsc -p tsconfig.json` checks the app only, so a test file's own type errors reach only the npm script.
  `npm --prefix fe run test -- <file>` does not filter (vitest reads the positional as its start command):
  run `cd fe && npx vitest run src/<file>.test.tsx`.
- Run `npx prettier --write <touched files>` BEFORE the final test run: prettier rewrites long
  `test('<criterion>: ...')` titles into several lines, so a run before that reformats looks like a
  changed test set. It also rewrites comments (a sentence that ends a comment line right after an operator
  merges with the line below) and breaks an `as` cast on a multi-line type argument: read the diff it
  makes, a joined comment is cosmetic, a reformatted type assertion is not. Prettier, ESLint and `tsc -b`
  print nothing on a pass, so quote the command line, not an output line.
- For a per-file map of failures and the counts of a full run, use
  `npx vitest run --reporter=json --outputFile=<f>` and read `testResults[].name`,
  `assertionResults[].fullName` and `failureMessages` with node (`--reporter=basic` does not exist in
  this vitest; the default reporter's `FAIL` lines are wrapped, so `grep "FAIL src"` finds nothing;
  `grep -E "^ FAIL" | sed 's/ >.*//' | sort | uniq -c` counts per file).
- An `edit` can report success on an untracked file and not land on disk: after editing a file the slice
  created, `grep -n` the new text and read the test NAMES in the run output before reporting. A file the
  slice created is untracked, so `git diff` prints nothing about it.

## Stubs, signatures and call sites

- When tests and production call a stub with the same name, the production sites break first, so the red
  is a compile error, not an assertion; and a `Self { ..Default::default() }` literal in the stub breaks
  on a new required field. Give the tests an explicit `Default` + `..` constructor so a new field cannot
  break them.
- A test that builds a struct literally is the one that reds when a field is added: grep the struct's
  literal sites before writing the report, and add the field to the shared constructor, not to each test.
- A new parameter of a public builder: the call sites are the tester's, so add it LAST and give each test
  file one `const` for its value (free for a route that never reads it): the coder then touches only the
  production file. Inside the callee the new name must not clash with an existing local: rename the local
  in the throwaway candidate and tell the coder which name is free.
- A new trait method: every `impl` of that trait is a call site. The shared bff fake (`FakeClient` =
  `DiskClient` in `tests/control.rs`) has `impl` blocks in other test binaries: adding the method to the
  trait breaks their compile and the red reports nothing. Extend the shared fake first.
- A new method on a test fake that the production code does not call is untested and the fake's
  `unimplemented!()` never fires; the red comes from a `noticed()`-style list assert. Write the red that
  way, not on the method itself.
- A stub that delegates to the old function is not red: it compiles, runs, every assertion greens, which
  reads as if the signature change already happened. A delegation that silently DROPS the new parameter
  is the red, not the stub: write `unimplemented!()`, name the signature in the report so the coder knows
  what to land, and add the parameter to the test's call sites (the tester's), not to the callee.
- When the stub takes a parameter, pin it as the constant the code must pass (`const _: fn(&_, &str,
  &str, &[&str]) = ..;`), not as a needle in the body: a source lint drops comments, so a comment naming
  the needle greens. Pin the argument as DATA (a `starts_with("const ...: &str = \"")`-style assert) and
  pin the rule that reads it, not its prose (a `cfg`-keyed table, not a literal).
- When a rule moves into a `cfg`'d module, a needle the existing tests pin is gone: retarget each test
  (its needle, the module's file, and its cfg-guard: `#[cfg(windows)]` / `cfg!(windows)` / `cfg!(unix)`)
  and name what Linux can no longer see.
- Run a source-lint extractor against the file before assuming it found anything: print the extracted
  body and eyeball it, else a broken regex greens vacuously. A test that asserts "extracted X" on a stub
  body of the same shape proves the extractor finds the real thing.
- A helper's name is a contract: grep every use before renaming a test helper (one rename broke 34 call
  sites in 8 files), and say in the report that the rename is the coder's, not yours.
- bff connects are per ROUTE, not per manager call: a write and its read-back are ONE connect. Count
  routes in the connect assert; the read-back belongs to the calls log. A fake that returns
  `self.act(..)?` on a failed write logs no read, so the calls log also pins "no read-back after a
  failed write".
- A refusal assert on a 204 prints nothing when its message is the body: write
  `assert_eq!(status, code, "the body was {:?}", body)`, and in a table loop put the case label on the
  "nothing was asked" assert that runs after it, since the first refusal reds and stops the loop.
- "The refusal asked the manager for nothing" on a log the test's own helpers also fill: pin it as the log
  UNCHANGED across the refused request (`let before = app.calls(); .. assert_eq!(app.calls(), before)`).
  An absolute list of the helper's own calls is the harness's shape, not the refusal's, and reds on its
  own.
- A request that must be refused by one of two rules whose ORDER the design fixes nowhere: pin it as "one
  of the two codes, and both fakes' logs empty", and write in the file header that the order is not
  pinned. An axum body-limit layer answers before a middleware guard, so a code-pair assert is the only
  shape that greens either way.

## Fakes, logs and gates (the manager, D-Bus, the pipe)

- `DiskClient`'s `call()` prints `DISK: <verb>` on stdout and `libtest` captures it, so a stdout assert
  passes solo and fails on a full-suite run (captured, "Call stdout:"). Assert on the recorded calls
  list, and keep the print for manual `--nocapture` runs: the print of a call the code skipped is the
  proof, an assert on the print is not.
- The manager's gate (`GateAuthority`) answers a `Subject` of a NAME, not a SID. Test code cannot mint an
  identity with a group (that needs the Windows token APIs): to pin a rule over a group (an
  administrator's extra grant), add a fact to the `Subject`, do not mint a token.
- The manager's fakes are `manager::service::fakes` (`crates/manager/src/manager/service.rs`); pai-app's
  test backend is `fake::scoped` (`crates/app/src/server_control.rs`). Grep which one a crate has before
  naming it.
- `WriterLog` records `(name, pid)`: a pid-keyed helper (`asked_admin`, `asked_bff`, `set_unique_name`,
  `is_authorized_quietly`) needs a unique name per test, else a parallel test's write counts as yours.
- `SYSTEM_DIR_ALLOWED` gates writing a file under the system dirs, and the gate's own list is pinned by a
  source lint: a new function that writes there needs both, and the pin's needle list must gain the name.
- A CAS fake that models a save records the write, THEN pauses, THEN compares-and-swaps against the live
  file (the shipped text as fallback): a pause before the record cannot show "the write reached the
  manager", and a CAS with no live file cannot show a conflict. A `status` that reads a `running` cell
  pins the contract's running-after-write rule (true also when the status read fails: a separate outcome
  cell, not the write's).
- A keyed digest pin: the published SipHash-2-4 vectors are offline in the vendored crate
  (`~/.cargo/registry/src/*/siphasher-*/src/tests.rs`, 64 of them) and a reference program is two `rustc`
  lines, no cargo and no network. GATE any reference by reproducing all 64 first: a self-validating
  reference is a check, a hand-transcribed one is not. One assert of
  `format!("{:016x}", SipHasher::new_with_key(&key).finish())` on the published empty-text vector pins the
  algorithm, the key bytes and the hex format at once. A rename of a name that is an INPUT of such a digest
  moves EVERY constant keyed on it: enumerate them (`grep -n '"[0-9a-f]\{16\}"' crates/bff/tests/*.rs`),
  never work from the coder's list, and recompute in a scratch program that reproduces the route's own
  input.
- `/tmp/opencode/<name>` is shared with the other agents: pick a name of your own, and never build a
  reference from the code under test (it proves nothing).
- A live shared tree (the coder rewriting the same crate while you write): append your section at the
  file tail and your helpers INSIDE the existing blocks, re-read the whole file before every edit, and
  recompile against the CURRENT trait. A parallel merge silently adopts your harness and rewrites the
  tests around it.

## Behaviour that cannot run here (Windows, xtask, packages, CI)

- The manager's Windows decisions are OS-neutral in `crates/manager/src/manager/windows.rs`, so
  `cargo test -p pai-manager` on Linux covers them (its `#[cfg(unix)]` tests included). The Win32 glue
  under `manager/windows/` is `cfg(windows)`: on Linux it is compiled by nobody, so pin its wiring with
  source lints over the text, comments dropped (a comment naming a needle never greens it): one
  `entry_fn(name)` extractor (signature to the `\n}\n` at column 0) plus a needle scan of the fn text.
- Pin shared service glue by its signature and by what the callers pass, not by its body: the shape
  (`fn(u32)` + `-> bool` before the body's `{`), the parameter READ, each caller's rule NAME forbidden in
  the body, and the arguments of `register(` read inside EACH caller's function text (contains its rule,
  not the other's). A mask name that is a suffix of the other's needs a count that subtracts the prefixed
  variant, and an assert on the parameter count makes a missing parameter red.
- Prove "every test of the slice ran" from the run's own output: write the FULL output to a file, then
  count `grep -c "<module>::tests::<name> "` per name, each 1. Prove the FIRST run compiled the coder's
  files (the `.fingerprint` dirs predate his mtimes, and the run's `Running unittests ...` binary hash
  equals a new fingerprint dir), since a second run of the same command is a replay.
- `crates/manager/src/tests/windows_sync.rs` is declared unconditionally and runs on every OS: it plans
  the services against `instances.ini`. The Win32 glue itself is still compiled by nobody on Linux.
- xtask's Rust unit tests (`cargo test -p xtask`) cover the Linux package files by checking generated
  text: `linux::system_files`'s `rules_answer(js, &args)` runs the polkit rule under `node -`, so a JS
  needle in `xtask/src/linux/system_files.rs` is a real red (the polkit rules the `.deb` and `.rpm` ship
  are generated from it).
- An xtask script's temp file is named `xtask-scripts-{name}-<pid>`: the test creates it before the child
  starts and its own cleanup removes it, so a test that RUNS the script cannot assert the script's trap.
  Pin the trap as a source needle, and pin the removal by the file's absence after the call. The same
  needle shape (a literal list of the directories a script cleans) is how a Windows-only rule that needs
  real state is pinned here: a contract, not behaviour, so say what it does not prove.
- `crates/bff/tests/listen.rs`'s `Sandbox` runs the BUILT binary with `--listen`, and `check_address` is
  the pure validator: the unit tests prove the rule, the binary tests prove it reaches the CLI, and both
  are needed. Put the fail-closed shapes of the validator in the REFUSAL test (a bracketed text that is
  no address, a port over 65535), because the authority is parsed before the address is read. A
  wildcard bind (`0.0.0.0:0`) is proven from a loopback client, no second interface.
- A runtime file shipped by the package is DATA, not code to rename: `crates/core/defaults/*.ini` (the
  starting files installed into `/etc/llama.cpp/` and `%ProgramData%\llama.cpp\`), the prompts in
  `benchmarks/prompts/` (sha256-pinned by a spec's `Prompts = <file>@<sha>` lines), and the campaign
  reports in `benchmarks/bench.db`. Report the residue as data, not as a miss.
- The guard for a bff error code is per route: the bff ends a session with 403 `session-not-administrator`
  when the manager refuses a call made as the signed-in user, and answers 502 `client-not-authorized` on a
  session-free read where the manager refused the bff itself. The guard for the second lives in
  `tests/instances.rs` and `tests/control.rs`: run those by name and report them as guards.

## Source lints and text pins

- A source lint scans the file's TEXT, not its behaviour, and it drops comments: a needle in a doc comment
  is not a scan hit. Pin production needles (a bare `ConfigFile::X` reference, not a `use`), and report
  comments as not scanned, so the gate lands in the code.
- `src_dirs()` exists in `crates/config/src/tests/binding_lint.rs` and `control_lint.rs`, each its own;
  `deps_lint.rs` is in `crates/app/src/tests/` and `crates/cli/src/tests/`, `lock_walk.rs` in
  `crates/app/src/tests/`. Grep for the function you mean.
- The CLI-only lint has an explicit function list, and the CLI print paths (`server show`, `bench query`,
  whose output is read in place) have their own: a new function of either kind must be added to the list,
  and the GUI reaches a gate only through the app's `server_control` / `startup_loads`.
- A source lint whose name list AND its scan cases are flipped together stays GREEN by construction (the
  scan is generic over the list): report it as the guard that the rule now covers the new names, not as a
  red.
- A rule that lives only in CSS, or only in a script, is pinned by reading the file's text (comments
  stripped, rules parsed). It is green the day it is written: report it as a guard and say which half of
  the criterion it cannot reach (jsdom has no layout).
- A comment line a writer emits is often pinned nowhere (`git grep` of the exact sentence: one hit, the
  writer itself): the red is a new byte-for-byte assert of each comment line. Prove a "the old word is
  gone" assert can go green by reading EVERY literal of the writer, and order it AFTER the byte-for-byte
  asserts: the red that stops the test is the wording one, and that is the red to report.
- "A section an earlier version saved must still load" is GREEN at once when the parser ignores the lines
  in question (the INI reader strips `;` comments): report it as a guard. Its fixture is the old text
  verbatim, CRLF included.
- Prove a harness stub with a throwaway test (write, run, delete) before writing a red that leans on it:
  jsdom has no `matchMedia` (the test stubs its own, `fe/src/HearthTheme.test.tsx`) and no
  `Window.isSecureContext` at all (`fe/src/InsecureSignIn.test.tsx`), so the "property absent" case is
  jsdom's own state and a `true` case is the real guard.
- A locator that matches a selector EXACTLY is a rename bomb: a later slice that rewrites that selector
  reds dozens of tests. That is fine when the brief orders the move, but then update the locators in the
  same step and keep every exemption in the same file tolerant of both shapes.
- A seam handed over as pure compile red (no stubs): prove the test code's own hygiene with
  `cargo check -p <crate> --all-targets --target x86_64-pc-windows-gnu`. The errors must name only the
  missing items (E0425); anything else is a typo in the tests. A `&[OsString]` seam fed from
  `Command::get_args` is tested with an invalid-UTF-8 `OsString` built by `OsStringExt::from_vec` in a
  `#[cfg(unix)]` test.
- xtask's per-OS modules are `#[cfg(unix)]`: a test importing one needs the same cfg. Run
  `cargo check -p xtask --all-targets --target x86_64-pc-windows-gnu` before handing over.
- An older commit's worktree (`git worktree add --detach build/<name> <rev>`, gitignored): copy the
  gitignored `resources/llama.ico` and `llama-brand-logo.svg`, set
  `PRIVATE_AI_LLAMA_CPP_DIR=<repo>/build/linux/llama.cpp` and a separate `CARGO_TARGET_DIR`.
- A new `ManagerClient` method needs an impl in every `impl ManagerClient for` (grep it: `DbusClient`,
  pai-app's `FakeClient` and `DiskClient`, the `FakeManager` of each `crates/bff/tests/*.rs`).
- A fake's shared log in a returned tuple trips `type_complexity`: alias it in `fakes` (`Checked`).
- zbus runs every task of a connection on one executor thread: a sync `fn` method blocks every other call
  on that connection.
- To probe a phase past a red one in an e2e `#[test]`, comment its call in `tests/mod.rs` with a marker
  (`// DIAG:`), and restore with `cp` from a backup plus `diff`, never `git checkout`.
- A "the rule does not exist yet" pin is a COUNT, not a declaration: a helper that throws
  `expected exactly one rule: <what>, found 0` names the missing rule in its own red, where reading a
  property of an empty array reads as a fixture typo and a "never contains" over a missing rule greens.

## The web UI (fe/)

- The shared tables are `fe/src/test/bff.ts` and `fe/src/test/modelProfileCards.ts`; the mount helpers are
  `startAt(hash)` (the hash only), `fakeBff(options)` and `render(<App />)`, with the teardown
  `cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); startAt('')`. `fakeBff` answers a signed-in
  session and the instance list by default, and `App` is a default export.
- `fakeBff(options)` returns `{ fetch, calls, to, setInstances }` ONLY: `calls` is `Call[]` of
  `{ method, path, body }` (no `"GET /path"` string to match), `to(method, path)` answers an ARRAY
  (`expect(bff.to(..)).toHaveLength(1)`, `toBe(1)` never), and every answer override (`session`,
  `instances`, `boot`, `version`, `signIn`.) is a CONSTRUCTOR option with no handle to change it after,
  each one a `Response` (`reply(200, {...})`) or a fn returning one, never a plain object. So a page
  helper that renders takes the bff and returns `{ bff, view }`, not one of them, and a test that needs
  both destructures. `held()` is `{ answer, resolve, signals }` (`answer(init)` IS the fetch body, it
  honours the abort signal); `deferred()` is the `{ promise, resolve }` one.
- The bff's route spellings: the instance page href is `#/instances/<encoded id>` (plural,
  `route.ts::instanceHref`), its boot state is `GET|PUT /api/instances/<encoded>/boot`, the session is
  `GET|POST|DELETE /api/session` (signed in 200 `{"user"}`, signed out 401 `session-required`). A route
  the fake does not know REJECTS with `TypeError: unexpected <route>`, which reads as a broken fetch.
- While the session is read `App` renders `<p role="status">` with the key `loading` IN PLACE OF the
  page, so `main` holds no list, no heading and nothing to query: await `findByText(en('loading'))`, and
  scope every list/table query to `within(screen.getByRole('main'))`, since the nav's `ul` (the Overview
  link) is the document's first `list`.
- jsdom answers `dt` and `dd` as the roles `term` and `definition`, and the spec's `div` wrapper inside a
  `dl` is invisible to a role query, so `within(dl).getAllByRole('term')` reaches a
  `dl > div > dt + dd` row: probe it with a throwaway file (write, run, delete) before pinning a
  labelled pair.
- Before pinning an ABSENCE the brief did not state (a reset rule, a normalising rule a row relies on),
  read the sheet: `index.css` has no reset naming `dl, dt, dd`, so such a pin is a red the coder cannot
  pass without inventing a rule. What jsdom cannot see (the user-agent `dd` indent, a `list-style`) goes
  in the report as OPEN.
- `modelProfileDoc(values, extra)` puts `extra.values` OVER the whole default `values` object, so a
  fixture that names only its own card's keys drops `model` and `spec-type` too (the page still renders).
  A key the file lacks is ABSENT from `values`: a `''` entry is a value, not an absence.
- A card added to the shared card list reds EVERY whole-page test of the suite, not only the new file: the
  card-order and tab-path tests of each cards file, the count / strings-parity tests, the page test's tab
  path, the heading-order tests, and `App.test.tsx`'s strings test (the coder's key pin). Count them in
  the report as collateral reds of the shared list, one line per file.
- Making an EXISTING card editable is done in the shared table, not in a page test: its label keys in
  `EDITABLE_FIELDS`, and for a number field with a default box, its label key in `BOXED_FIELDS`.
  `fieldTabStops` and `controlEnabled` derive from it, so the collateral reds before the coder are the
  tab-path tests. Prove the table rule by the DELTA of the tab list: a delta that differs per fixture
  means the rule is not read from the fixture. A checkbox's tab stop is named by its TOGGLE text
  (`<key>-toggle`), not by the field's label, and a boxed number adds a stop for its `field-default` box.
- Older tests that pin a newly editable card READ-ONLY stay green in the red step and red after the
  coder: grep the suite for the card's key prefixes, list them for the coordinator, and never edit them
  in the red step. A card made editable also invalidates read-only pins in files outside the slice's.
- `pageStartTabStops()` is the stops every whole-page tab test expects before the first field, so a
  control a later slice inserts in the banner is retargeted ONCE, in the shared helper. Once a jump
  button carries a card title, that title is no longer unique: "no card `h3` is a tab stop" compares
  tabbable ELEMENTS, never NAMES (a name list holds every title twice and is true whatever the code does).
- Typing into a control of a card that is not editable yet is a SILENT no-op (React skips `onChange` on a
  disabled control): the value stays, the form stays clean, the save bar stays inert, and a refusal test
  reds at `findByText(en('save-invalid'))` after ~1 s and reads like a missing check. One debug print of
  `.disabled` / `.value` / the box's `checked` tells it apart in a single run (the box is `<key>-box`, the
  input `<key>-control`); report the red as "the field is not editable", not as "the check is missing".
- What a field renders depends on `editable`: read-only is a `<label>` + input (no `role="group"`), an
  editable field with a default box is a `<fieldset>` named by the label. So a "no group named by the
  label, the textbox carries the name" pin is GREEN on a read-only card and RED on an editable one: pin
  it in the file whose field is already editable to carry the red, repeat it as a guard, report which is
  which.
- A card's controls are NOT in the document when its `h1` is (the cards render with the read): a
  `getByRole('combobox', ..)` right after the `h1` reds "Unable to find an accessible element with the
  role combobox", which hides the slice's real red. Use `findByRole`, and assert the fixture's `values`
  carry the row's keys.
- Inside a card there is no group and no note per field: only the card's group and the card's note, so a
  structural page test can assert a card is the region's first child and every row a group with its
  legend. A checkbox whose text the brief gives in two states is found as the group's only checkbox,
  never by name.
- A rule of a shared component (the sign-in, shown in `main` and in the session-ended dialog) is one
  rule: test the page case and the dialog case in the same file. The dialog needs no new page: a Save
  answered 401 `session-required`, the bar's Sign in, then `getByRole('dialog', { name })`.
- A page whose save answers only the generic failure today: every keyed-answer test reds on the alert
  TEXT, and the guard-key test of "anything else is the generic failure" is GREEN by construction: report
  it as a guard, it is the net the new reading must not break.
- "Any answer that is not a shaped 200" is ONE test with a table of answer cases (204, a 200 missing a
  field, a 200 that is not JSON, a 500 with a key, a rejected fetch): one page per row and
  `cleanup(); vi.unstubAllGlobals()` at the end of each iteration, so each row reports its own label.
- A time limit needs no new clock: `api.ts` aborts a non-GET at 120 000 ms, so a `held()` request plus
  `advance(119_999)` / `advance(1)` (the `fakeClock`/`flush`/`advance` pattern of `Timeout.test.tsx`:
  fake timers only, so the microtasks stay real) shows the abort is wired while the red is the message
  the page renders. `afterEach` needs `vi.useRealTimers()` beside `vi.unstubAllGlobals()`.
- A new strings key reds two existing pins, report both: `App.test.tsx` "the English the brief gives"
  lists every `EN` key `src/strings/en_US.json` lacks (the coder's key pin), and the same file's reverse
  lint scans the source, so the key goes in the test's `EN` table ONLY in the red step: adding it to
  `en_US.json` is a red no tester may fix (the production `t('key')` call is the coder's).
- A new focusable control in the banner goes red in EVERY document-wide tab-order list: grep the suite
  for the lists that start at the skip link and insert the new stop in each; a banner-scoped list
  (`within(nav)`) needs nothing.
- Every nav query needs `within(nav)` (the Overview table names the same instance ids as the nav), and a
  link click in jsdom does not always move `window.location.hash`: pin a close on `aria-expanded` and the
  landing page by its heading. The browser's Forward is testable after a leave guard: a typed hash adds a
  history entry, so build the history with a typed hash, never with a second render.
- `getByRole('button')` returns `HTMLElement`, so `.disabled` is TS2339 in the type check and passes the
  Vitest run (Vitest never type-checks): cast at the query. A helper that wraps an async finder is async:
  `within(findX(main))` is TS2345 until awaited, in the helper and at every call site.
- `act` comes from `@testing-library/react`, not from vitest (importing it from vitest dies "act is not a
  function"): import it in any file that dispatches an event the app's own listener installs outside
  React's tree. `fireEvent.click` on an `<a>` makes jsdom print `Not implemented: navigation`, and a
  focus assertion still passes under it, so the noise hides: use `userEvent.click` when the activation
  matters.
- A button named by `aria-label` is pinned without a labelling helper: `computeAccessibleName` is not
  exported by this `@testing-library/dom` and jsdom computes no accessible name, so pair
  `getByRole('button', { name })` (it honours `aria-label`) with `not.toHaveAttribute('aria-label')`.
  jsdom fires no click from Enter or Space on a button, so "toggles with Enter and Space" has no harness:
  pin the closest proxy (a native `type="button"`, no key handler) and report the criterion OPEN.
- A suite timeout in `fe` is a mount-count problem: every `render(<App />)` costs 0.15-0.25 s, so a case
  with a loop that mounts per iteration passes solo and times out under the full suite. Count the MOUNTS
  A TEST MAKES, not the call sites in its text (one call site inside a 9-case loop mounts 9 times, and the
  local wrappers of `render` differ per file: read them), keep it at 3, and turn a `for` with a mount
  inside into `test.each` over that data; split, never merge. A slow test with 3 mounts or fewer is a
  clock problem (many sequential fetches or timers), not this one.
- `test.each` over an array of ARRAYS spreads the elements as POSITIONAL arguments: the callback is
  `(key, value)`, not `([key, value])`. Object items allow `$prop` in the title, array items use `%s`, and
  a `%s` consumes one argument IN ORDER, so a `%s` typed into the prose eats a case value and a literal
  `%` must be escaped. A `.flatMap` that builds a table of tuples needs the tuple type spelled twice, or
  `tsc` reds `Target requires 4 element(s) but source may have fewer`.
- `globalThis.fetch = vi.fn().mockImplementation(..)` is an ASSIGNMENT, not a stub: it survives
  `vi.unstubAllGlobals()` and it is not a mount, so it stays in the each-body (a title cannot inject a
  value into a callback body). Drop the loop's per-iteration `cleanup()` / `vi.unstubAllGlobals()` when
  the file's `afterEach` does it; they MUST stay between the mounts of a multi-mount case, else Testing
  Library fails `Found multiple elements with the role main`.
- A test file past ~900 lines splits into `<Name>1` / `<Name>2` with the helpers COPIED: a test file
  imports no other test file, and the copy keeps each file runnable on its own. Splitting one back means
  hand-reverting the setup edits: truncate at a known line and edit the header, imports and hooks.
- Mutation runs prove a fe test has teeth: `rsync -a --exclude node_modules --exclude dist --exclude ux
  fe/ /tmp/<x>/` plus a symlink to the real `node_modules`, driven by a script that restores the pristine
  sources (kept OUTSIDE the copy) after each mutant, so no mutant survives and the repo is never touched.
  A mutant is a survivor only after a FULL-suite run (a rule is often pinned in a file outside the
  slice's own). A patch that only changes a value nothing reads is no test at all.
- Re-read the source for every patch anchor just before a run, and re-read a quoted acceptance criterion
  in `fe/briefs/<slice>.md`: a context summary recites helpers the file does not have, and a numbered
  criterion is a markdown list item, so a bare-text grep for the number finds nothing and reads as "the
  brief has no such criterion".
- The fe suite's total moves between runs of an UNCHANGED baseline: account for your own tests by name and
  get the baseline by EXCLUDING your files, never by subtracting the totals. A guard that is red in a
  two-file run and green in solo runs of the same test, with no file touched between, has no explanation
  in the tree: rerun it alone and report the green.
- A CSS text pin parses the stylesheet (comments stripped, rules from the brace balance, nesting from the
  balance before the selector). The selector group of that parse is the whole text before the `{`, at-rule
  prefix included, so the light block is the one with no `@media` and the dark block starts at it; a
  declaration pinned on a `@media` WINDOW is green when any nested rule of the window carries it, so pin
  "the media block" claims on the window and "the control" claims on the rules. A whitespace-stripped
  `compact` form is the exact-match key: `.nav a` compacts to `.nava`, and a needle that drops the
  punctuation finds 0 rules, which reads as "the rule is missing" when it is there. Exempt a sanctioned
  case by the DECLARATIONS it carries, never by a selector name (a name-based allowlist exempts a rule
  that only looks the same), and read a selector list's shared body, not each selector.
- A rule scoped under a class is pinned on the PARSED selector PARTS (each part must start with the
  class), not on a substring of the group: the group of a nested rule carries its at-rule window, so a
  substring is green for a rule the brief placed elsewhere. Pin "only the properties the brief lists" as
  a set equality over the rule's declaration names: that is what refuses an extra property.
- Compare a declaration's VALUE with whitespace collapsed (`\s+` to one space, trimmed): Prettier re-wraps
  a long value (a shadow stack, a gradient list) and an exact compare reds on the wrap, not on the code.
- Adding tokens to `index.css` reds the older slice's exact-list pins (the token-name list of each theme
  block) and its COUNT pins over at-rules (the forced-colours block count; a system-colour scan whose
  whitelist is one property reads a `display: none` inside a forced block as a system colour). Grep those
  pins before the run and report them as collateral reds of the coder's change; never edit them in the red
  step.
- Decorative art of a page is pinned on two planes: DOM in the `.tsx` (structure, order, emptiness, tab
  order) and stylesheet text in a `.ts` (every colour, size and mask declaration). The CSS reader is COPIED
  into each file, since a test file imports no other test file.
- A brief that says "the box holds A then B" is pinned on the wrapper's `children`
  (`toEqual([a, b])`), not on `main.children`: the coder may nest the box at any depth.
- A contrast rule is computed in the test (sRGB-relative luminance, `(L1+0.05)/(L2+0.05)`) from the
  PARSED hexes, never from ratio constants copied from the brief; the brief's own stated ratios go in as
  the unit-check of that function, proven by a node probe first, else the unit-check is a red the coder
  cannot pass.

- Run fe vitest from `fe/` with `TMPDIR=/tmp/opencode`: vitest writes `node_modules/.vite` (the source tree
  is gitignored only under `dist`/`coverage`, so a sandbox that read-only binds the repo dies with
  `EROFS ... unlink tsconfig.json.tmp`), and `npm --prefix fe <script>` runs npm in that sandbox as well (it
  creates `fe/npm-cache`, and a sandboxed `npm --prefix fe test` stalls in the background on an esbuild
  install postinstall, printing nothing).
- `fe/vite.config.ts` has no `setupFiles`: jest-dom's matchers (`toBeInTheDocument`, `toHaveAttribute`,
  `toHaveClass`) are NOT installed, and `@testing-library/user-event` is not a dependency. Read
  `package.json` and the config before importing; use `.className`, `.textContent`, `toBeTruthy()`.
- Prettier's config is found per package: `fe/package.json` carries the fe's rules (no semicolons, width
  100) and the repo root has no `package.json`, so a root-level `npx prettier --write <file outside fe/>`
  uses Prettier's defaults (printWidth 80) and rewrites lines nobody touched. Format from `fe/` with
  repo-root paths (`npx prettier --write ../crates/...`). `npm --prefix fe run format:check` covers
  `crates/bff/ui/*.html`, `.github/workflows`, `LICENSE` and `README.md`, which a production commit may
  have left unformatted: format them and prove with `git diff -- <file>` that only whitespace moved.
- Right after `render(<App />)` the Overview's `main` holds only the loading `<p role="status">` (the
  session has not settled): a synchronous `main.querySelector` finds nothing and every case of the file
  dies in its own helper. Await the section's heading (`findByRole('heading', { level: 2 })`) before any
  synchronous query.
- `within(ul)` matches a `li` nested inside a tile's own `<ul>` (the selector is `ul > li`): pin "a tile
  holds nothing nested" on `element.children`, never on a `within` list query.
- A shape an ARIA role restores (a `ul` with `role="list"`) is not reachable by a role query alone: when
  the brief names the class, pin `element.className` / the tag name, and say in the message that the list
  must be a real list.
- A control the app turns into a link only when the session is signed in (a tile that navigates): the
  whole-page test of that page needs `fakeBff({ session: 'signed-out' })`, else the coder's change adds
  the links the pin forbids and an unrelated test reds. Deleting a source-text pin (a literal in the
  `.tsx`) leaves the criterion's BEHAVIOUR pins: grep them in the same file and report them as its coverage.
- "Focus lands on the heading, nothing readable precedes it": no test can pin a focus that the app takes
  on first arrival, because `PageHeading` focuses only when `FocusRequest > 0`. Pin the reading-order half
  (walk the text-bearing nodes between the skip link and the heading, then the tab stops of `main`) and
  name the existing test that carries the focus claim.
- A test title that quotes a brief line carries its punctuation into the lint: a brief written
  `(line 222: -)` made an em-dash message. Name the brief's section, never quote its prose.
- Deleting keys from the test's shared `EN` table reds nothing in `strings.test.ts` (its parity check reads
  `src/strings/en_US.json` and the source files, not the table); deleting them from the JSON while
  production still uses them reds its "every value is used" check, so a strings pin in a new file of the
  slice is a duplicate guard of that one.

## Renames, wire and text

- Before flipping a wire key, check it exists today (`grep -c '^<key> = ' crates/core/strings/*.toml`,
  `grep -rn '"<key>"' crates`): a design's rename list names keys of the whole target state, and some do
  not exist yet. Renaming a key that is not there invents a red no coder can pass.
- A wire rename touches assertions that live in PRODUCTION files (core's `checks.rs` `mod tests`, the
  manager's D-Bus and pipe modules, the app's model-profile module). Flip with a LINE-RANGE sed from the
  file's first test marker (the line from `grep -n 'cfg(test)'`), never the whole file, and never a whole
  file where `#[cfg(test)]` sits on single items. Prove the split after: the old spelling remains ONLY on
  production lines.
- The bff's own keys are in NO strings file: pin them in `crates/bff/tests` only. The keys the manager and
  core emit are in both strings files, and a strings-key rename is pinned on both sides in
  `crates/core/src/checks.rs`, which reads both with `include_str!`: the new key resolves with today's
  text byte for byte, and each old name is absent.
- A route move: pin the OLD path as the router's bare 404 in one new test (status, empty content type,
  empty body, zero connects, the session survives): red today, greens when the route is gone. Do not
  rename the GUI's display keys: the wire slice renames only the keys the wire emits, and those drag
  every `.slint` and the GUI tabs in.
- In a `data_root_e2e` crate a key rename reds at the FIRST phase that asserts it, so the later phases'
  pins of the same key cannot go red on their own: report them as blocked, and name the phases that DO
  prove the mechanism (the wire encode in core, the D-Bus name in the manager).
- A route's answer is `{"key", "args": []}` with a shared `assert_error(status, key)` helper, so a slice
  that changes only the KEY reds on the key assert (the status assert greens) and the panic prints both
  bodies: read that pair, it names which half moved.
- Flip ONLY what names a file on disk (the constants the tests hand the fake, the call-log literals, the
  assert messages, the doc comments), NOT the file's own CONTENT: a header's prose names the sibling file
  inside the INI text, and a test pins that prose equal to a source file a later slice renames. Sed with a
  per-line skip for those lines.
- A migration test that reads a moved file through a `paths::` helper breaks when that helper's RETURNED
  name is renamed, not when the migration is: the old tree's names are data, so pin them by the literal
  path, and add the guard of the decision (the new name is absent from the moved tree).
- A hand-over that reads the user's file under the OLD name and writes the manager's new one means a seed
  under the NEW name is read by NOTHING, and the phase that expects a refusal reports `refused: succeeded`,
  which reads like a broken fake, not a wrong name. Grep the seeds to tell a temp-dir fixture from a
  per-user seed.
- The word and verb rename of the same family is pinned at three levels, all in the crates: the binary
  test asserts the retired verb is GONE (not an alias), the help test pins the command list as an exact
  array, and a split lint scans BOTH strings files for the retired invocation. The residue of a text
  rename that is DATA (a key's name, an accepted option value) is reported as data, not as a miss.
- Auditing a rename for a LOST test, without any checkout: compare per changed file
  (`git show HEAD:<f>` vs the worktree file) and normalize the rename vocabulary on BOTH sides (HEAD may
  already carry the new word where an earlier sub-slice renamed). The repo-wide totals are not comparable,
  and a `^\+.*#\[test\]` count cannot tell a rename from a deletion; the set of test-fn NAMES can. Same for
  a WEAKENED assertion: extract each assert from the diff's removed lines by a balanced-paren scan,
  normalize (vocabulary + all whitespace), and require it as a substring of that file's added blob;
  rustfmt re-wrapping and a raw string are artifacts to read by hand, never to "fix".
- Pin a retired CLI flag in the crate's inline `Cli::try_parse_from` tests, NOT in `tests/pai_binary.rs`:
  a `pai bench run` that PARSES takes the machine-wide GPU lock and blocks forever ("Waiting: another
  process uses the GPUs", exit 124 under `timeout 60`), so a binary-level run of that form is a deadline
  panic that touches the real lock. In the binary test pin `bench run --help` only.
- clap derives a long flag from the FIELD name, so write the parse tests from the ARGUMENT STRINGS only
  (`Cli::try_parse_from([..]).is_ok()`): the test then holds for either way the coder renames it. Learn
  the refusal wording BEFORE the rename by running the built binary with a made-up flag, and pin that
  line, never the `tip: a similar argument exists:` line that follows (it depends on the edit distance to
  the real flags). A help pin bans the FLAG, not the WORD: `--temp preset` and `<PRESET>` stay.
- A text sweep covers more than `.rs` literals: the shipped defaults, xtask's unit/polkit/desktop
  literals, and the strings files. To split production from test code, print each hit beside the file's
  FIRST `#[cfg(test)]` / `mod tests` line (a hit below it is test code; a file under `tests/` has no
  `mod tests` and reads as production to that pass). Report the residue that must stay (a name, an
  accepted option value) as DATA, not as a miss.
- Never `git checkout --` / `git restore` a file another session may have left dirty (a playbook, a shared
  test file): a revert once dropped 27 lines two sessions had added that day. Undo your own edit by hand,
  and re-read the tail of a shared file before the last edit.
