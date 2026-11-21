---
name: tester-playbook
description: What the tester has learned about writing reliable tests. Use when writing or debugging tests.
---

Lessons only: the rules are in `AGENTS.md`.

## Proving a red, and what a run proves

- Never wait without a deadline (`recv_timeout`, a `try_wait` loop, `method_timeout`): a test must fail,
  not hang. `recv_timeout` returns `Disconnected` at once when the thread panicked (a `todo!` stub):
  report that apart from `Timeout`, or the red reads as a hang.
- A Rust mutant run: a script that copies the file to its own backup, applies ONE exact snippet
  (`src.count(old) == 1` or abort, never "replace the first"), runs one filtered test, prints the mutated
  line's number plus the panic/assertion lines and the failing test names, then restores with `cp` and
  proves it with `cmp -s`. Judge red from the OUTPUT (`FAILED` / `panicked at`), never from the exit code:
  `cmd | tail` reports `tail`'s 0 and a survivor reads as green.
- A gate whose first four lines are copied verbatim into every guarded verb (`authorize_as` + lock +
  `known_engine` in the manager's verbs) cannot be mutated in place: the snippet matches 3 times. Include
  the verb's OWN first distinguishing line in the snippet, so one mutant lands on one verb.
- An ORDER rule ("the record is written last") has no teeth from an accepted call: every refusal happens
  BEFORE the write, so a record-first mutant survives a refusals loop. The pin is a call that fails on the
  LAST step: make the render's path a directory (`create_dir(sys.join("models@blocked.ini"))`), so
  `write_served_file`'s rename fails, and assert the whole-tree snapshot is unchanged.
- A mutant of the session gate that keeps compiling: `sessions.current(&headers).unwrap_or_else(||
(String::new(), crate::session::Identity { user: String::new() }))` (a `peek` swap is a compile red:
  `peek` answers `Option<Identity>`, not the tuple).
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
  never as passing. To see one phase's red at all, add a temporary `#[test]` in `tests/mod.rs` calling
  just that phase, run it, delete it and prove the delete with `grep -c` (a phase behind a red one
  reports nothing in the shared run, so its red is otherwise unprovable).
- A phase whose pins are a mix of reds and day-one guards: gate each RED pin's own opening brace
  (`{` -> `if std::env::var_os("X_REST_FIRST").is_none() {`, braces balanced), run once with the variable
  set, and the guard pins and every later phase report. An `is_some() { return; }` line skips the WHOLE
  phase, not the block: it proves nothing about the pins after it. `sed` the guard back to `{` after the
  run and prove with `grep -c`. A gate read in the test body DOES see a variable set on the cargo command
  line (the note above is about a harness process, not a unit test's body).
- When a phase can only be proven in a red state, seed the pre-save state into a file and show the
  refusal, then write the final test asserting the good outcome. The red of a test that asserts success,
  naming the exact refusal, is a better report than an artificial case.
- A long phase with many pins: wrap each pin in `// <SLICE>-PIN-<n> BEGIN/END` comment markers and prove
  each pin's own red with a script that in a scratch copy deletes the PRE regions and pins 1..N-1, runs
  that one phase, and restores byte for byte (`cmp -s`). The reported panic lines are of the scratch copy,
  not of the file; say so in the report.
- A pin block a delete-script removes must be SELF-CONTAINED: its own snapshot reads (`let kept = read(..)`
  inside the block), no variable shared across regions, so a pins-deleted copy still compiles; and the
  block's green-run cleanup must tolerate files a deleted pin created (`let _ = remove_file(..)`), else
  the copy panics in cleanup and the pin's own red is never reached.
- A bookkeeping pin (a log or call COUNT over calls made by earlier pins) can never prove its own red:
  with those pins deleted the count passes vacuously. Report it as green today, deferred to the coder's
  green run.
- A count pin over a seam the builder also reads (the router's constructor takes the machine's names from
  the same source): the absolute count reds on the builder's own read. Take `let base = source.asks()`
  after the builder and assert deltas, and in a loop take the baseline INSIDE it, so one case's red names
  that case. State in a comment that the builder's read is the coder's choice, so a pin never forces it.
- Write each criterion of a guard as its own test, and expect a split: a rule that is an ABSENCE ("with
  `[Bff] Hosts` set, nothing is re-read", "`localhost` never asks the machine") is green on day one, and
  the rule that adds an action ("re-read on an unknown name") is the red. Report each test's day-one state
  by name, and say which green tests are guards, not reds the coder needs.
- A prove loop rewrites the real source while it runs: run nothing else with cargo on that crate
  meanwhile (the target-dir lock serialises, and a concurrent run judges the scratch copy).
- `cargo test -p <crate>` runs no target when one target fails to compile (`--no-fail-fast` only unrolls
  the run). Report which binaries did not run, and run the untouched binaries in a second invocation
  (`--lib --test a --test b`) to show them green.
- A red target's TEST FAILURE also stops the run (cargo: `error: test failed, to rerun pass --lib`), so the
  later files never report. Use `--no-fail-fast` for the slice's map: one run, every target's
  `test result:` line, and `awk '/^---- /{name=$2} /panicked at/{print name" | "$0}'` for one line per red.
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
- An e2e phase that rewrites a file a LATER phase pins (a `[Server]` profile whose `ModelsMax` the next
  block reads): snapshot the text at the phase's start (`let found = read(name).expect(..)`), write it
  back with `std::fs::write` at the block's end. Prove the restore's teeth with a mutant that swaps the
  write line for `let _ = found;` (compiles, so the red is the later phase's panic line, not a compile).
- A phase's own "the write landed" loop and the restore coexist: read the snapshot BEFORE the loop, never
  after, and keep the block's asserts on the writes (the restore belongs to the tree, not to the pin).
- A phase that drives a real save goes through `config_seam_e2e` (`crates/app/src/server_control.rs`:
  redirect the tree, save through the seam, reload, assert), never through a direct write of
  the files: that passes without the seam.
- "Nothing left in the user tree" is a state of the tree, not of the process: assert it at the END of a
  phase, where it belongs, not as an isolated mid-phase probe.
- A multi-block phase shares ONE tree: by a late block, earlier blocks have added files and removed others,
  so an absolute `len() == N` and an "the refusal removed nothing" naming a file a previous block deleted are
  both stale fixtures. Before fixing one, walk the phase's own blocks and write down the names the tree
  holds at that line; then pin names (the row list by id, present and absent), never a count, and split the
  list with a `cfg(unix)` insert for a row only a symlink creates. A name pin reds naming the row; a count
  reds naming nothing.
- A "the refusal wrote nothing" baseline read at the phase's start reds on the previous block's LEGITIMATE
  write (a create block appends a section, so a later refusals loop naming the start's text reds on it).
  Snapshot the file's text right AFTER the block that is allowed to write, and pin refusals against that.
- The GUI e2e is one `#[test]`, so a production line that half-did a rename kills it at its first phase
  and the later phases report nothing. Prove a pre-existing red predates your edit (`cp` the pristine
  file from your own backup, run, restore, `grep -c` to confirm the restore), name the production line
  the coder must flip, and say which of your asserts the red blocks.
- To show the red of a phase that a blocked one sits behind, flip the value the earlier assert pins for
  one run (a `sed` of the pinned constant, e.g. `PROTOCOL`), read the later phase's panic line, then sed
  it back and prove the restore with `grep -n` of the line and `git diff --stat` of that file. Report the
  line as "seen with the protocol pin flipped", never as a green of the current tree.

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
  bff's `build.rs` embeds the web UI and exports `PAI_BFF_FE_DIR` for `tests/fe.rs`, so `cargo test -p
pai-bff` fails "the web UI is not built" until `xtask build framework` has run. The build script's OWN
  input is `PRIVATE_AI_FE_DIST` (absolute, or it asserts): the script already falls back to
  `crates/bff/fe-fixture` (a stub `index.html`) when `fe/dist/index.html` is missing, so an
  unset-variable run is the fixture; a stale `fe/dist` is overridden with
  `PRIVATE_AI_FE_DIST=$PWD/crates/bff/fe-fixture`. Setting `PAI_BFF_FE_DIR` in the shell does nothing
  (cargo's `rustc-env` wins). `tests/fe.rs` runs on the fixture too (it reads `env!("PAI_BFF_FE_DIR")`
  and branches on `fixture()`), so a fixture run is 6 green, not a stub failure.
- A route that validates the instance id against the record answers `no_instance` for every id the fake
  does not list, so a second instance needs a list the test can set: in `crates/bff/tests/server.rs` that
  is `Live::instances` (empty = the one `default`) behind `App::set_instances`. Each `tests/*.rs` has its
  OWN fake, so the cell exists in one file only, and a route whose id is unknown is the red that proves the
  gate, not a broken harness.
- The manager's D-Bus unit tests are slow and flaky on a loaded machine: green on an idle one, reds at
  0..1800 ms (the 10 ms bound of the token probe) under load. Run them alone, then judge:
  `cargo test -p pai-manager --lib -- --test-threads=1 data_root_e2e` (`dbus_client` is a module of that
  test).
- A `tokio::time::sleep` in an async test on a current-thread runtime is real wall clock, not advanced by
  the test's manual runtime pumps: it burns the whole budget (1200 ms of sleeps, 2.6 s) and greens
  nothing. Drive the timeout with `tokio::time::pause()`, and use
  `#[tokio::test(flavor = "multi_thread")]` when a real sleep is wanted.
- The web UI's tests run from `fe/` only (`npm --prefix fe test`, `npx vitest run` inside `fe`): `npx
vitest` from the repo root resolves a different config and lies about the files it ran. It runs the NODE
environment too (the `environment: 'jsdom'` of `fe/vite.config.ts` is not read), so every component test
reds `ReferenceError: window is not defined` in `startAt`, which reads as a broken harness. A scoped run is
`npm --prefix fe test -- src/X.test.tsx src/Y.test.tsx` (several files in one positional filter).
- The type check that sees the test files is `npm --prefix fe run typecheck`; a bare
  `tsc -p tsconfig.json` checks the app only, so a test file's own type errors reach only the npm script.
  `npm --prefix fe test -- <part-of-the-file-name>` DOES filter (vitest 5 reads the positional as a file
  path pattern: one `Test Files` line), and `-t "<the title>"` for one test; a bare `npx vitest` from the
  repo root stays the broken invocation.
- vitest prints no `console.log` (the reporter swallows stdout), so a probe of a harness fact must ASSERT
  its value in a deliberately failing assert (`expect(shown).toBe('the probe answer')`): the red line
  carries the string. Write the probe as a `.test.tsx` in `src/`, run it, `rm` it, prove with
  `ls src | grep -c`.
- `role="status"` is a name-from-AUTHOR role (ARIA), so `getByRole('status', { name: the text })` finds
  nothing for `<p role="status">the text</p>` and the red reads "Unable to find an accessible element with
  the role status" on markup that is there. Find it as
  `getAllByRole('status').find((el) => el.textContent?.trim() === text)` (the existing fe tests do exactly
  this). The same is true of an absence pin: `queryAllByRole('status')` plus the text filter, never
  `queryByRole(.., { name })`.
- A page's delay rule (the skeleton after 300 ms, U17a) needs the test's own clock: `vi.useFakeTimers`
  with `toFake` listing `setTimeout/clearTimeout/setInterval/clearInterval/Date` (React's scheduling stays
  real), a `flush()` of `act(() => advanceTimersByTimeAsync(0))` x20 to land the microtasks, and the
  answer's own `setTimeout` inside the fake clock (a `late(answer, 500)` wrapper) so it lands when the
  test advances it. Pin the boundary tight: `advance(299)` absent, `advance(1)` present.
- jsdom applies no stylesheet, so "the text is in the DOM and not shown" is two pins: the element's link to
  `.visually-hidden` (accept BOTH nestings, `el.closest('.visually-hidden') ??
  el.querySelector('.visually-hidden')`, so the coder's markup shape stays free), and the CSS text of the
  `.visually-hidden` rule, which the sheet test already pins.
- A new `fe/ux` state whose reads the stub holds needs two things the existing states do not: the
  navigation's `waitUntil` must be `domcontentloaded` (the network never goes idle, `networkidle` only
  ends at the navigation timeout), and the reset query in `load()` must carry EVERY key a state can set,
  since the stub keeps keys a `/__set` did not carry: a key missing from the reset leaves every later
  state with a held read, and each of them reports `not rendered`.
- The UX check (`npm --prefix fe run build` then `npm --prefix fe/ux run check`, ~2 min) prints one line per
  browser x width x theme (x text mode), so ONE unmet `expect` of one state is 18 lines: report the unique
  lines (`sed 's/^chromium //..' | sort | uniq -c`), not the summary's count. A missing selector reads
  `not rendered: <selector> x<count>`; an axe tag reads `axe: <tag> x<n>`. Failures of states the slice did
  NOT touch land in the same summary (today: `instance-server: axe: target-size x2`, 8 lines) and are
  pre-existing: say so, and prove the state is untouched with `git diff fe/ux/check.js`.
- A `hover: { on, then }` pin acts on the FIRST match of `on`, so it only greens when that first element
  provably triggers the reveal. A pill that needs a CUT chip (15 R5) cannot be pinned on a state whose
  first chip is short: pick a state whose first match is the widest, or leave the reveal to the jsdom test,
  where the test can SAY which element is cut (`measure()` sets `clientWidth`/`scrollWidth`).
- jsdom's `closest` answers a COMPLEX selector (`el.closest('li > *')`): the element's own parent is the
  `li` gives the element ITSELF, a nested one gives the li's child. So a "these sit in one wrapper" pin by
  that locator is testable today, and the shape of the red is the field itself: pin
  `group === input` false FIRST (its message names the defect: the field is a direct child of the row),
  then the containment pins (`group.contains(label)`, `..contains(share)`) and the parent pin, and prove
  the containment pins' own red in a scratch copy with the first pin cut (they sit behind it).
- A new fe CSS class reds `Reflow.test.ts` R1.8's frozen `CLASSES` list, and a new `overflow-wrap`
  declaration reds R1.1's `sets('overflow-wrap')` list: both are allowlists the CODER amends, so name them
  in the report and leave them for after the layout test. The same is true of `HearthTokens.test.ts`
  H1a.12's min-size list: a brief that rules a control's size (15 R2: the chip x 24 px, the + and the
  pill's icons 32 px) wants entries ADDED, and all three lists are compared with `toEqual`, so a ruling
  that adds a class (`chip-full`), a declaration (`.chip-full { overflow-wrap: anywhere }`) or a size is a
  test edit of the same slice, never a "leave it for the coder".
- Those allowlist arrays are in a fixed order: the H1a.12 list is the order the rules appear in
  `index.css`, R1.1's is sorted by selector. Run the test and read the diff's insertion point, do not
  guess: a well-aimed entry in the wrong slot reds as a whole-list mismatch that names nothing.
  `HearthH2a.test.ts`'s sizes are a `test.each`, so a new entry reports as
  `section 1 sizes: <selector>`, one line naming the selector.
- `App.test.tsx`'s `1a strings` compares the test helper's `EN` map (`src/test/bff.ts`) against
  `en_US.json`, so putting a brief's new strings in `EN` (which `en()` needs to name them) reds that
  test naming exactly the keys the sheet lacks. Report it as the strings criterion's red, never as
  collateral, and never work around it by writing the English inline in the new file.
- `Icon.tsx` answers an unknown name with an EMPTY svg (`SHAPES[name]` is undefined), so the red of a
  new icon is `expected [] to deeply equal [ <path d>, .. ]` with all eight svg attributes passing.
  A literal name is a COMPILE red (the prop is the `IconName` union), which is why the shape tests
  import the module through a variable (`const ICON_MODULE = './Icon.tsx'`) and cast: a union widening
  the coder forgets then shows as the path-data red, not as a file that will not run.
- A fe test file has no exported helper to reuse (importing one registers its tests), so a CSS-text
  reader is COPIED into the new file, as `U17aSkeletons.test.tsx` and `Reflow.test.ts` each did.
- A CSS text pin whose selector ALSO appears inside a `@media` block: the flat rule regex matches it
  TWICE, so a `block(sel)` "exactly one rule" assert reds on a rule that exists. Strip every at-rule
  (header plus its block, by brace depth) into a `topBlock(sel)`, pin the top-level rule there, and pin
  the media block on its own: its header, the selectors its body holds, and its declarations.
- An at-rule reader whose header regex match ENDS at the block's `{` starts the brace walk INSIDE it, so
  its depth is 1, not 0: from 0 the walk stops at the block's first inner `}` and the body of a
  one-rule block is an unbalanced `.chip > span {` on which the rule regex finds NOTHING, which reads as
  "the media block holds no rule" (a red the tester owns, not the coder's).
- A media block's body is a list of RULES, not of declarations: `decls(body)` returns the rule's header
  as if it were a declaration (`.chip > span { white-space: normal`). Take the inner rule's own body
  (the same `matchAll`'s group 2) and read the declarations from it.
- In an ordered min-size allowlist, the entries of ONE rule follow the test loop's order
  (`min-height` then `min-width`), NOT the lines of the sheet: `.tag-new input` writes `min-width` above
  `min-height`, and the list must name min-height first.
- A token swap inside existing classes (17a's review: `.skeleton` `--surface` to `--bg`, plus
  `.card .skeleton, .card .skeleton-box, .tile .skeleton { background: var(--surface) }`) touches NONE of
  the three fe allowlists: R1.8 lists classes, R1.1 `overflow-wrap`, H2a `TEXT_FREE` heights. Say that in
  the report, it is the answer to "which Hearth list moves".
- Pin a descendant CSS rule per WHOLE selector, not as one rule the brief's text spells: match the
  whitespace-dropped form against `compact.split(',')`, and that form KEEPS THE LEADING DOT (the reader is
  `m[1].replace(/\s+/g, '')`): `.card .skeleton` compacts to `.card.skeleton`, so a selector written
  `card.skeleton` matches nothing and the pin reds on a rule that is in the sheet. Its `readable` is
  `compact.split('.').join(' .').trim()` (a leading-dot split starts with `''`, so no `.` prefix: that
  yields `..card .skeleton`). The coder's one rule or three both pass; pin `!compact.includes('@')`
  beside it: a media-scoped rule greens the selector at one width only. The top-level class keeps its own
  `valueFor(cls, ..)` pin, which reads only the exact-`.cls` rules, so the pair (page fill, box fill) is
  two reds naming each other.
- The teeth of a CSS-text pin are proven on a copy of the SHEET, not on the real one (the rule exists, so
  the pin is green and vacuous-looking): a scratch test in `src/` that reads a scratch css in `src/` (same
  dir, so `join(import.meta.dirname, ..)` needs one string swap) with the rule cut out of it; the red
  reads `expected 0 to be greater than 0` naming the selector. `rm` both and prove with
  `ls src | grep -c`.
- A missing wrapper element (17a's review: the skeleton tiles not in `.tiles`) has an ORDER: pin the
  WRAPPER count first (`section.querySelectorAll('.tiles').length === 1`), then each child's
  `closest('.tiles')` against that one node, then the wrapper's own child count, then that the wrapper sits
  under the state's `aria-hidden` root. A `closest` pin first is vacuous when there is no wrapper at all,
  and a wrapper outside the hidden root would make the shape a list to read.
- A red the coordinator reports in a file whose area is NOT in the coder's diff (a routing/sign-in test,
  no routing file touched): run it solo three times and read the full-suite map. Three green solos plus a
  green in the full run is a flake, and the report says so with the runs; editing production code to
  chase it is out of the tester's scope.
- React 19 builds `onMouseEnter`/`onMouseLeave` from pointerover/pointerout and mouseover/mouseout
  (react-dom registers `onPointerEnter` on `["pointerout", "pointerover"]`), so
  `fireEvent.mouseEnter(el)`, a non-bubbling `mouseenter`, reaches NO React handler: fire pointerOver +
  mouseOver + mouseEnter (and the three mirrors to leave), which covers a handler and a plain listener.
- jsdom focus for a React handler: `el.focus()` dispatches `focus` AND the bubbling `focusin`, which
  React's root-delegated `onFocus` hears; `fireEvent.focus(el)` is non-bubbling and invisible to it.
  Same for a `tabindex="0"` element that is not focusable in the fixture: assert
  `document.activeElement === el` first, its red then names the missing `tabindex`.
- An optimistic list (R6, `useOptimistic`): its refusal ends in the SAME state as the pessimistic code
  (the chip back, focus on it), so a refusal pin greens on day one and proves nothing about optimism.
  Pin the optimism in the PENDING state, where the two differ: the new chip listed while the PUT is
  held, the removed chip gone and focus already moved. Say in the report which pin is a shape guard.
- A criterion that allows TWO shapes for one text (the announcement alone, or ahead of the count) is
  pinned with `toContain(sentence)`, never `toBe`; its paired "a later change replaces it with the count"
  pin greens on day one and sits BEHIND the red, so prove it in a scratch copy with the announcement pin
  cut (report it as a guard).
- A brief that AMENDS an existing criterion (15 R5 amends C1.2's tab order, R6 amends C1.5, C1.8, C1.9):
  rename the test `15 C1.n (as 15 section 10 Rm amends it): <the new wording>` and keep every assertion
  the ruling does not contradict, so the run's list of reds answers the criteria one by one.
- A new tab stop in a `thead` (17c.2's header buttons) joins an exact `tabbable()` list AFTER the
  filter row's Clear button, not after the filter field, and before the page-level `session-sign-in`
  of a 401, which sits in `main` above the table. A shared `head` array ending at the filter label
  cannot carry it: keep the new stops in their own array and place them in each list, since a list
  with Clear is not a list without it.
- A fe test with two halves in one `test()` (the fe check, then a 422 refusal): the first red stops the
  second, so prove the second alone in a scratch COPY in the same dir (`cp src/X.test.tsx
src/XScratch.test.tsx`, cut half 1 with a script that asserts its snippet `count == 1`, run `-t SCRATCH`,
  `rm`, prove with `ls` + `ls src | grep -c`). A copy in `src/` keeps every helper, no re-import. The
  same file's helpers declared BELOW the test are reachable from it: a `function` declaration hoists, so a
  GD2.6 test calls GD2.7's `splitFormat`.
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
- The coder rewrites the slice's own test files in the same tree while the tester works: one write keeps
  the inserts made before its read and drops the later ones. Re-`grep` every test name you added right
  after each insert, and again over all the files before the run: a test of yours that is absent from the
  run output was clobbered, it is not a green.
- A rewritten test file has new helpers (`form(values, options)`; `weightOf` returning the input, not an
  object): read the file's helper block again before writing into it. A call in the old shape can pass
  vacuously (an object where a mode string was expected yields a nonsense mode, so "no fields" asserts
  for the wrong reason), which reads as "the defect is already fixed".
- RTL's default text matcher reads an element's DIRECT text children only, so a second visible copy
  inside a child (`<p>Sentence.<time>date</time></p>`) is invisible to `getByText('Sentence.')`: pin
  "the text shows once" on `element.textContent`, which the matcher ignores. The same rule bites the
  LOCATOR: once a brief puts a sentence's `{0}` slot inside a child, `getByText(the whole sentence)`
  matches nothing, so a pin that found the paragraph by its text must locate it through the child
  (`time.closest('p')`). The widest blast radius is a shared readiness probe: the as-of move reded 169
  tests from four helper lines (a `findByText(sentence)` in every GPU file's `ready()`). Fix the probe on
  the CHILD's text (`findByText(theLocaleDate)` finds the `time`), a sync presence pin by scanning the
  container's `p`s for a `textContent` equal to the whole sentence (that list gives a "one line" count for
  free), and an absence pin by the sentence's HEAD, `en(key, '{0}').split('{0}')[0]`, so a line with ANY
  moment is caught, a regex's teeth are kept, and no English prose is written into the test.
- When a brief AMENDS a value an existing suite reads through a SHARED helper (17b.1: the tile's Tags value,
  which `InstanceTiles.test.tsx`'s `facts()` read as the joined text), change the READER and leave the
  assertions: read the readable form (the list's items, joined for reading), and every existing pin of that
  value greens today and stays a pin of its row, its order and its wording, while the new shape (the
  `ul[role=list]` of `li.chip`, its CSS, its pill) is pinned in the new file, where the brief's criteria name
  it. Two pin shapes cannot survive that and move to the new file: a "no element's text contains X" scan
  (17b.2, which reds on the `dd`/`ul`/`li` ANCESTORS of the items), and a locator that found the value by
  that text.
- Reconciling an OLD pin against a NEW one: change only what the brief contradicts (a locator, a
  blanket "active" that the brief's ends rule refutes, a fixture key the title already claims, an
  order the fieldset's DOM order settles). Where the brief settles the rule IN THE OLD PIN'S FAVOUR,
  keep its assertion and report the red naming the production line: weakening a brief-pinned assert to
  green the suite hides the defect. Say in the report what a production fix there costs the suite (the
  shared helper locator, e.g. a `findByText(sentence)` in every file's `ready()`).

## Stubs, signatures and call sites

- When tests and production call a stub with the same name, the production sites break first, so the red
  is a compile error, not an assertion; and a `Self { ..Default::default() }` literal in the stub breaks
  on a new required field. Give the tests an explicit `Default` + `..` constructor so a new field cannot
  break them.
- A test that builds a struct literally is the one that reds when a field is added: grep the struct's
  literal sites before writing the report, and add the field to the shared constructor, not to each test.
- A new `[Server]` key of `ServerConfig` is a five-file fan-out of exhaustive-literal guards that red BY
  CONSTRUCTION once the field exists (`launch::server_args_covers_every_config_field`,
  `server_cfg::rich_config_round_trips_through_ini`, cli's `server_set_apply_copies_every_field` and
  `show_lines_prints_every_field` (its destructure wants `field: _,`), config's
  `rich_server_config_round_trips`, bff's `rich_cfg`), plus `crates/config/src/server_form.rs`'s
  `form_to_config` (production: the GUI has no field, so stub `devices: None` and report the GUI's
  key-preservation gap). The field's own stub (the struct's field plus the one `from_keys` read line) is
  what keeps the workspace compiling; a field NO render line makes every "the file keeps the key" pin a
  real red, and `render`/`check`/`server_args` stay the coder's. In `crates/bff/tests/server.rs` the
  hand-maintained `KEYS` array is the document's contract: gain the key there and
  `unset_keys_are_null_in_the_shipped_file` reds naming exactly the missing field.
- `checks::device_list` takes an EMPTY value as unset (`Ok(())`), as every list rule here: pin the blank
  as taken and the refusal table on the shapes only (`"ROCm0, ROCm1"` is refused for the ` ROCm1` entry,
  so the refusal's `values` are `["Devices", " ROCm1"]` with the leading space, the entry as split).
- A key judged ONLY by a shape rule is invisible to the marker rule: `RawSection::get` cuts the value at
  the first `;`/`#`, so `server_file`'s `device_list` reads `ROCm0` from `ROCm0;x` and passes. The teeth
  live in the raw-keys loop (`SERVER_TEXT_KEYS`), so pin the refusal at BOTH layers, the file check (the
  raw line, args `["Devices"]`) and `server_cfg::check` (the value as set), plus the bff's route 422 with
  `field` and "nothing written" on EVERY save route. The day-one red reads `Ok(())` / 200, and the bff
  unit `refused(..)` panic prints the whole rendered file WITH the marker line: that is the defect's
  evidence, not a harness typo.
- A server key a client has no field for (R2's `Devices`): each whole-file writer is judged on its own
  merge rule. `pai server set` is a merge BY CONSTRUCTION (`ServerSet` has no such flag and `apply`
  copies only the flags it is given), so its "the file keeps the key" test greens on day one; the GUI's
  `form_to_config` (`devices: None`) is the real red, pinned like the `Tags` test in `server_form.rs`.
  `pai config import` writes a file only when the manager lacks it (`Ok(Some(_)) => continue` in
  `migration.rs`), an inspected absence with no test to write.
- A field added to a struct every client decodes (`InstanceInfo`) is a three-level red: the row's own
  signature pin (D-Bus `a(sssqasas)`, pinned as `<Row as Type>::SIGNATURE.to_string()`, zvariant has no
  `Signature::as_str`), the transport's decode of the OLD shape (pin it refused), and every JSON document
  that carries the item whole. Grep the JSON literals (`grep -c '"engine":' crates/bff/tests/*.rs`) and
  gain the field in each, in the same step, so the collateral reds are one list, not a mystery.
- The same field on the Rust struct reds COMPILE, not behaviour: `E0560 has no field named` at every
  literal and `E0609 no field` at every read, so write each fixture ONCE per file (the manager's
  `cards()` / `listed()`, the bff's `probed()` / `listed()`) and the red list stays short. Read the
  reply's whole shape as one tuple, not field by field: a D-Bus reply widened at the end
  (`ta(ssstt)tu`) is pinned as `<(u64, Vec<DeviceRow>, u64, u32) as Type>::SIGNATURE`, which compiles
  and passes the day it is written, so report it as the wire guard beside the round trip's red.
  Write that literal in zvariant's form, NO commas between a struct's members: a pin whose own string
  carries commas is a red the coder can never pass (the sibling row pin `(ssstt)` shows the form).
- A test that asserts a struct of hardware readings for EXACT equality (the probe box of
  `pai-manager/src/tests/gpu_probe.rs` answering `NodeDevices`) cannot name the machine's RAM and CPU
  count, so a new figure of that kind forces the reading to be a seam the test hands in. Leave those
  literals alone (they red at compile, the coder gains the field) and report the shape: pin the figure in
  the fake that answers a number (`FakeGpus::with_inventory(..)`), and say a production path reading the
  host answers its own numbers and reds there. Where the test drives the REAL seam (a scripted exe, the
  RAM and CPUs read off the live machine), split the assert: `assert_eq!` on the members the fixture
  decides (the moment and the list, taken through a `probe_answer(&node)` helper so the whole struct is
  never a literal), plus `node.ram_mib > 0 && node.cpu_threads > 0`, never a host value; two answers of
  that same seam DO compare whole. A fake seam keeps a fixed literal (no inventory answers 0 on both).
- When a route gains a read (the record read in front of the file read), retarget the EXISTING call-log
  pins of that route, not only the new tests: a pin that asserts `["read server@default.ini"]` after a
  read that must now be `["instances", "read ..."]` is a red the coder reads as a broken fake. Grep the
  call-log literals of the file and prefix the new call in each; report how many pins were retargeted.
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
- The exception: when a criterion is a NEW action beside an ABSENCE rule, a stub that reproduces today's
  behaviour exactly (a guard seam that reads its source once, at construction, and never again) gives the
  coder the full map, the exact red of the action and the green of the absence, with no test that reads as
  a harness typo. Say in the stub, in the report and in the file header that it is a TEST STUB that models
  today, and that the coder replaces its body.
- `//!` is legal ONLY above the file's first item: a section header appended at the tail of a test file
  with `//!` is `E0753: expected outer doc comment`, one error per line. Write mid-file headers with `//`,
  and when a script turns them back into the module doc, stop at the first `use`.
- A doc comment on a struct FIELD must start every line with `///`: one bare prose line is
  `expected ':', found ','`, a SYNTAX error of the LIB target. It stops the crate's test binary from
  building at all, so the field reds you wrote report nothing and the run reads as "no red found". Read
  the FIRST error of the run before judging the list.
- A route added under a path an existing test pins as 405 for every verb (`POST /api/instances`,
  `DELETE /api/instances/{id}`) makes that pin a false red: narrow it, in the same edit, to the verbs no
  route answers, and pin the new paths' own unrouted verbs as a new test.
- A fake whose start/stop/restart answer from ONE cell cannot pin "the refusal lands on start and restart
  only" (a stop needs no model profile). Give the odd verb its own `Option<...>` cell defaulted `None` in
  `Answer::ok()`, so every `Answer { verb, ..Answer::ok() }` literal keeps compiling and behaving: the
  refusals of the shared verb stay what they were.
- The server documents' thread limit is the bff's OWN `available_parallelism` (amended R1: a document
  read must ask the manager no `devices`, that call can run the engine probe for 30 s under the 5 s read
  bound). Pin it by arming the fake's `devices()` (it records and answers 8, the machine has 24) and
  asserting `limits.Threads == [1, cpus]` plus `!calls.iter().any(|c| c == "devices")` on both the
  instance document and `GET /api/profiles/server/{id}` (server.rs's full-router harness routes it on
  the same fake). `limits_are_the_checked_ranges_and_the_cpu_count` is the un-armed guard of the same value.
- A fake's new verb that answers `Err(Unavailable)` by default is safe only for a route no test pins:
  grep the routes that would now read it, and say which answers the default reaches.
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
- The manager's GPU memory check runs at TWO judging points today (`check_profile_memory` at a model-profile
  pool write, `check_profile_memory_of` at an assignment): a rule that must also fire where `available`
  shrinks (a server-profile switch, a save of a profile in use) is green at NEITHER today, so pin those as
  the reds and keep "a save that changes no `Devices` judges nothing" and "already over is not re-judged" as
  day-one guards. Size a fixture's cards from the estimate of the GGUF the phase itself writes
  (`full_gpu_bytes` + `pool_usable` + `GPU_RESERVE`, the same pure API the check uses) and assert the profile
  fits the pair and NOT one card: a refusal that never arrives then names the fixture, not the rule.
  `devices::Gpu::usable()` is already there (total less the reserve), `pool_usable` sums it over `gpu_pool`
  (one card seen by two backends once), and `checks::device_list` takes `Devices = none` alone, refusing it
  beside another name.
- The GPU seam's "a write asked it 0 times" pin: read the count AFTER `manager_on` as a delta
  (`let base = asked.lock().unwrap().len(); .. asks[base..].is_empty()`), so a probe at the manager's
  own construction, the coder's choice, never reddens the absence.
- The shrink check (the two points where `available` shrinks: a server profile switch, a save of an
  in-use profile that changes `Devices`) asked the seam BEFORE it estimated anything, while
  `check_render_memory` asks only once a section grew, so "a render with nothing to estimate asks the
  seam 0 times" is the red there and the mixed list (`ROCm0,ROCm9` on a node with a ROCm0 and a CUDA1,
  refused with the one card's MiB) is a day-one guard: `devices_usable` skips an unknown name per name.
  The render with nothing to estimate is an empty `models.ini`, or a profile in effect with an
  `hf-repo` key (a `FETCH_KEYS` section names no file of the node, so the estimate gives up), and
  neither judging point runs the model-file check, so those writes LAND and the count is the only pin.
- A gate's env var name has no `-`: bash reads `R3-PIN-10=1 cargo ..` as a command name, so the run
  tests the UNGATED file and the red names the pin you meant to skip. Marker `R3-PIN-10`, variable
  `SKIP10`. Match a cut's boundary lines with `strip().startswith`, their prose runs on.
- A pin block with two sub-cases (the rule at no-profiles, then at no-estimate): prove the second's red
  by DELETING the first's lines in a scratch copy, not by gating, since a gate skips the whole block.
  Each sub-case writes its own fixture (`record` / `profile_with` / `pool_files` / `assignments`) and
  redeclares its `gpus` / `asked` / `base`, so the cut copy compiles.
- `WriterLog` records `(name, pid)`: a pid-keyed helper (`asked_admin`, `asked_bff`, `set_unique_name`,
  `is_authorized_quietly`) needs a unique name per test, else a parallel test's write counts as yours.
- A new verb of a probe that a test file already fakes (the fake exe and its counter file of
  `pai-manager`'s `tests/gpu_probe.rs`) belongs in THAT harness: widen the shared fake with an answer cell
  (`FakeGpus::with_devices(..)` / `.no_devices()`, the latter taking `self`, so
  `FakeGpus::new(&[]).no_devices()`), never a second fake. The same counter then pins both "one probe per
  call" and "a refusal ran no probe", which two harnesses cannot.
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

## Gating a route (a session or permission gate slice)

- A gate's acceptance is the refusal, and a route deleted passes the 401 pin as well as a route gated.
  Retarget every existing pin of the answer behind the gate (add the cookie, keep the body assertion
  whole), so the read must still answer, and pin the refusal beside it.
- The gate's check must not consume what it checks: the bff's `Sessions::current` refreshes the idle
  deadline, so a gate written with it keeps a page alive forever. Pin the deadline: sign in, ask every
  gated route, advance the fake clock under the bound (alive), past it (dead).
- A gate keeps its place in the order in front of it. Pin a refusal that answers earlier (a foreign
  `Host`, an `Origin`, a 404, a 405) WITH a valid cookie, so it stays its own code, and the gate's 401
  with none. A guard that answers after the gate is a behaviour change nobody asked for.
- A route the bff does not route yet answers axum's bare 404 with an empty body and an empty content type,
  so the first red of EVERY test that goes through the shared `assert_error` / `assert_session_required`
  helper prints `content-type ""` and reads like a harness typo. Say in the report that each of those reds
  is the unrouted path, and pin `the route answers only the write` (GET/POST/DELETE 405, the router before
  the session) as its own test: it is a 404 today and greens with the route.
- Where signing in is impossible (the binary's real authenticator is the machine's manager, 503 under the
  test redirect), pin the gate at the socket (401, the exact body, no `Set-Cookie`) and keep the answer's
  shape pinned at router level with a fake authenticator. Say which level pins what.
- A child-process test's readiness probe keyed to a status the slice gates (poll `/api/version` for 200)
  hangs to its deadline and reads as "nothing served". Make the probe accept the pinned status.
- `crates/bff/tests/pai_bff_binary.rs` spawns a child per test: on a loaded machine a signal test's exit
  code flakes (exit 1 with an empty stderr, green alone). Reproduce with `--test-threads=1` before naming
  it, and report it as a flake, not as a red of the slice.
- The source's route list (the doc comments naming `GET /api/version` as open) goes false with the gate:
  report the line for the coder (a tester edits no `src`), and quote a clean `clippy` to show the tests
  needed no source change to compile.

## A bff route family (a new resource under `/api`)

- A new verb of `ManagerClient` handed to a bff test's own fake goes in that fake's
  `impl ManagerClient for FakeManager` block, so the file's red is COMPILE red: one
  `E0407 ... is not a member of trait` per verb, `E0432` for the row type the trait returns,
  `E0599` for a new `ClientError` variant. Count them and list them: a red list that names ONLY
  missing slice items is also the proof the tests carry no typo.
- The bff's `ManagerClient::instances()` default body is `Err(Other)`, so a fake
  that never overrides it makes every LIST route (`GET /api/profiles/model`) answer
  502 `client-other`, while a route that does NOT read it (the model-profile switch:
  its `restart` comes from `assignments.ini` + `Status`, never the list) keeps its
  `sorted_calls` pin clean. So when a switch/list file's fake gains `instances()`,
  ONLY the file's `.list()` tests change; add it answering from the same `instances`
  cell as `list_instances`. A `restart` that is derived (assignments + status) never
  reads `Instances()`, so do not prefix `instances` into a switch's calls pin.
- A prefix-matching call-log helper (`actor_of(prefix)` over recorded lines) collides
  when one call is a PREFIX of a sibling the same route makes (`read models.ini` also
  hits `read models.ini.disabled`), so its "exactly one call" assert reds on the pair.
  The SAME collision when the route makes the identical call twice (reads the file,
  writes, reads it back for the answer): a write route with a read-back pins the WRITE
  by exact line, never the read. Add an exact-line variant (`line == call`) and
  retarget only the colliding site;
  leave the prefix sites (a verb with no sibling, `set-server ` with a trailing space)
  as they are. Where every ask of the repeated call must carry the actor (the design's
  read-back answers for the signed-in user), a third variant takes the count the route
  owes and returns EVERY actor in order (`actors_of_exact(call, times)` ->
  `Vec<Option<String>>`, its red naming "asked N times, the route owes M"), so both
  reads are pinned, not one of them silently.
- A "the state read times out so running is true with restart []" switch test only
  reaches that branch if a `Status` is actually ASKED: give the fixture an instance
  the profile is served on (`assignments.ini` `[default]\r\nQwen = on\r\n` + that
  instance `Active`). With no served instance the whole-task timeout passes on the
  quick calls, no status is asked, and `running` is false: that is a DIFFERENT test
  (`...no_instance_is_given_restarts_nothing`, an absence, sorted_calls has no status).
- Which account a route acts for is pinned on the connect log: `principals_of(..)` is
  `vec![None]` for a read the bff answers with its own rights, `vec![Some("alice")]` for a write
  that acts for the signed-in user. One entry per manager connect (the session routes connect
  never), so a route that connects twice is visible.
- A route gated with `Sessions::current` MOVES the idle deadline: sign in, advance the fake clock
  just under the timeout, and assert the answer is not 401. Assert "not 401", not a status: the
  body's code is the rest of the slice's business, and the deadline pin stays green when it moves.
- A deadline pin that tells `current` from `peek` needs steps measured from the LAST answering read,
  since every answer moves the deadline: a second step of the same length as the first lands inside
  the window the first one opened, so the read answers 200 and a "the session is over" assert there
  passes ONLY when the code is wrong. Pin the move as a clock past the sign-in's deadline and inside
  the moved one (assert 200), then one step past the last answer (assert 401, the manager asked
  nothing): that pair says the deadline moved and stays finite.
- The `current` -> `peek` mutant is a COMPILE red, not a behaviour one: `peek` answers
  `Option<Identity>`, `current` `Option<(String, Identity)>`, so swapping the token at a route that
  destructures the tuple is `E0308 expected Identity, found (_, _)`. The compiling behaviour mutant keeps
  the tuple: `sessions.peek(&headers).map(|identity| (String::new(), identity))` (the empty id only
  weakens the session-end path; the deadline is what the mutant moves). Report both shapes.
- A document's `revision` is a keyed hash under the router's own secret: a save's body takes it
  from the GET of the SAME router (a helper that reads, edits one value, saves), never from a
  constant. To pin the revision the save ANSWERS, recompute SipHash-2-4 with
  `SipHasher::new_with_key(&[u8; 16])` and `format!("{:016x}")` over the text the fake recorded.
- A fake that answers a pool AND a record keeps them consistent: a profile the record says is used
  must carry that `instance` in its own row too, else the fake answers "used" and "unused" in the
  same call and the pin proves nothing.
- A hand-built server save body is 400 `server-malformed` unless `values` carries `Tags` as an
  array (`Kind::Tags`'s `reads` is false for an absent key, unlike every other kind): add
  `"Tags": []` beside the keys the test edits.
- To reach a save whose record read fails, skip the document read (it fails too): the PUT body is
  `{"revision": revision_of(&profile_text(port), &REVISION_KEY), "values": {...}}`, the revision of
  the text the fake holds, so the write lands and only the record read fails, after it. A `rows`
  cell that answers `Err` is enough: the save asks `instances` once, after the write.
- Format test code with `rustfmt --edition 2021 <the files you wrote>`, never
  `cargo fmt -p <crate>` (no `--check`): that rewrites `src/` of the crate too. Map the
  `cargo fmt -p <crate> --check` diff's `Diff in <file>:<line>` ranges against
  `grep -n "^#\[cfg(test)\]" <file>`: `crates/manager/src/manager/service.rs` has one `#[cfg(test)]`
  per e2e phase (not one `mod tests`), so a diff at line 9075 is test code.
- A `///` doc comment on a local `let` closure inside a test fn warns `unused_doc_comments`, which
  kills the coder's `cargo clippy --all-targets -- -D warnings`: write those comments with `//`.
- A key that is OFF the web key list reds a save as 400 `model-profiles-malformed` (the bff's own body
  check), before the manager is read, so every 422 pin of a "the keys move to the web" slice sits behind
  that 400. Report the 400 as the first red and the 422 table as its own test, so one red names one rule.
  The same ordering bites a VALUE: once the bff refuses `;`/`#` itself (ruling a), a "the manager's device
  rule refuses it" row cannot carry a `;` value, it dies at the bff's `comment-markers` with no write, so
  pick a value the bff passes and only the manager's rule refuses (`"ROCm0, ROCm1"`).
- `serde_json` here has no `preserve_order`: a body's `Map` is sorted, and iterating `&Map` yields
  `(&String, &Value)` pairs, so a keys loop is `.as_object().keys()`. The sorted order is also the order
  `ini::text::set_key` appends several NEW keys in (the bff builds its edits from that map), which is what
  a byte-for-byte expected file must spell.
- A route whose deadline is its own constant (not the injected read timeout) has no cheap timeout test: pin
  the constant's value as its own test, and in the past-deadline test give the REQUEST a limit above the
  constant (a `get_within(limit, ..)` helper; the shared 5 s deadline kills it first) and pay the seconds.
- An axum body is read with `response.into_body().collect().await.expect(..).to_bytes()`: a `Collected` has
  no `iter`, and that typo reads as a harness bug inside a compile-red target.

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
- A new shipped file has two separate pins, in two files: WHOSE package it is (`components::of` in
  `xtask/src/common/components.rs`, testable on Linux, and the red when a shared folder's prefix rule
  swallows it: `usr/share/private-ai/llama.cpp/` claims everything below it, so a per-package rule must sit
  AHEAD of that rule) and WHAT the package holds (the staging and `%files` lists in
  `xtask/src/linux/package.rs`, `build_deb`/`build_rpm`). Pin the first; when the second file is another
  agent's, say so and name the helper the content pin needs (`dist_version` reads a
  `<Name>/<Version>/<os>` path). The Windows `share\...` ship is a `.wxs` row plus the staging in
  `xtask/src/windows/package.rs`: on Linux that half is not runnable, and a `cfg(windows)` arm of `of` is
  compiled by nobody, so say Linux proves the rule, not the MSI.
- The guard for a bff error code is per route: the bff ends a session with 403 `session-not-administrator`
  when the manager refuses a call made as the signed-in user, and answers 502 `client-not-authorized` on a
  session-free read where the manager refused the bff itself. The guard for the second lives in
  `tests/instances.rs` and `tests/control.rs`: run those by name and report them as guards.
- A rule AMENDED to an absence (a read that used to ask a verb must now ask none) is pinned by arming
  the fake's verb (the fake keeps: it records, and answers a number that differs from the bff's own,
  e.g. 8 vs the machine's 24) and asserting the call log has NO entry for it and the derived figure is
  the bff's own. The same slice must retarget every exact call-log pin the earlier slice prefixed with
  the verb (grep the file for the quoted verb, e.g. `"devices"`): an unretargeted pin is a false green
  of the new rule and a red the coder reads as a broken fake. The pair route can ride the same
  full-router harness (`/api/profiles/server/{id}` routes beside `/api/instances/{id}/server`); a
  separate test file whose fake does NOT implement the verb answers the trait default (`Err(Other)`,
  records nothing), so it has no pin to retarget: report "no devices expectation found" for it.
- A fake's new verb that answers `Err(Unavailable)` by default is safe only for a route no test pins:
  grep the routes that would now read it, and say which answers the default reaches.

## Source lints and text pins

- A source lint scans the file's TEXT, not its behaviour, and it drops comments: a needle in a doc comment
  is not a scan hit. Pin production needles (a bare `ConfigFile::X` reference, not a `use`), and report
  comments as not scanned, so the gate lands in the code.
- To pin that one ARGUMENT is a derived value (the state folder joined with the instance's id), take the
  text from the needle to the argument's own separator: `let rest = &arm[arm.find(needle)..]`, then
  `&rest[..rest.find(',').expect(..)]`, and require `.join(` inside THAT. A `contains` over the whole arm is
  green for a join of anything beside the needle, and over the whole file for a join in an unrelated
  function; say in the report that the lint pins the shape, not the id.
- A method extractor (an `fn` indented in an `impl`) must stop at the method's OWN closing brace, the
  column-4 `\n    }`: one that stops at the next column-0 brace swallows every method beside it and passes
  on a needle in a neighbour. The brace is written `"\n    }"`: in an escaping context a doubled `}}` is a
  literal, and the extractor dies "the end of `fn X`" on a method that exists. Prove each extractor on a
  needle you know is there, and give the wiring lint's loop the self-checks (the text contains today's
  `Manager::new(`) BEFORE the loop, so an empty extraction never passes.
- `src_dirs()` exists in `crates/config/src/tests/binding_lint.rs` and `control_lint.rs`, each its own;
  `deps_lint.rs` is in `crates/app/src/tests/` and `crates/cli/src/tests/`, `lock_walk.rs` in
  `crates/app/src/tests/`. Grep for the function you mean.
- The CLI-only lint has an explicit function list, and the CLI print paths (`server show`, `bench query`,
  whose output is read in place) have their own: a new function of either kind must be added to the list,
  and the GUI reaches a gate only through the app's `server_control` / `startup_loads`.
- A source lint whose name list AND its scan cases are flipped together stays GREEN by construction (the
  scan is generic over the list): report it as the guard that the rule now covers the new names, not as a
  red.
- A refusal key of `checks::KEYS` is counted as USED by `crates/config`'s `binding_lint` only where a
  `strings::fill` names it, and for a check key that place is its OWN arm of `Refusal::text` in core: a key
  in `en_US.toml` with no arm is that lint's red (`in en_US.toml but used nowhere ...`), and the arm is the
  coder's, never the tester's. Pin the missing arm with a loop over `KEYS` asserting
  `Refusal { key, .. }.text() != key` and that no `{0}` survives: it names the red the coder can pass, and
  catches every later key added with no arm.
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
  pai-app's `FakeClient` and `DiskClient`, the `FakeManager` of each `crates/bff/tests/*.rs`). The way out
  is a DEFAULT body on the trait saying "this transport has no such verb" (`ClientError::Other`): no fake
  breaks, the bff's read pins the fallback row, and only the fake that must answer gains a body. Report the
  default as the bridge to an older manager, the coder keeps it.
- A new field of `ServerConfig` reds the bff's own unit tests first (they build the struct literally:
  `src/server.rs::rich_cfg`, its `body_of`, and the three `body_to_file_*` compares), and a new key of the
  Server page's document reds four pins spread over two files (`the_values_keys_are_the_web_s_field_names`
  in `src`, `values_are_typed_by_ini_key` and `unset_keys_are_null_in_the_shipped_file` in `tests`). Grep
  the key list literals, they are the map.
- A whole-page form writes the file from the document it was handed, so a field the document does not carry
  is ERASED by a save of the page that never shows it. That is a real criterion: pin it as "a save of the
  document read unchanged keeps the value", not as an absence.
- A fake's shared log in a returned tuple trips `type_complexity`: alias it in `fakes` (`Checked`).
- A stale pin retargeted to a settled rule greens on day one (the code already has the rule), so its teeth
  are the report: prove it by moving the FIXTURE's value (the port of the profile file the section names, not
  the one named after the instance) and by moving the expected constant of the missing-file case (the rule is
  0, never the default port), one run each, restoring with `cp` + `cmp -s` between the two.
- A list answer that gains a member reds every test that asserts the item WHOLE. In the bff that is a JSON
  literal: `grep -c '"engine":' crates/bff/tests/*.rs` is the map (the list pins of `tests/instances.rs`,
  every document of `tests/origin.rs`). Gain the member in each literal in the same step, and give the fake a
  DEFAULT answer for the read the new member comes from (a `boot_enabled` answering `Ok(false)`), so the
  file's other tests keep their shape; pin the "the read fails, the member is null and the list never
  shortens" case as its own test.
- A package-content rule (which files a component's `.deb`/`.rpm` ships) is pinned in
  `xtask/src/linux/package.rs`'s `mod tests`, the file the slice's staging change edits. When another agent
  owns that file for the slice, pin only the OWNERSHIP rule in `xtask/src/common/components.rs`
  (`of("usr/share/private-ai/llama.cpp/versions/<package>")` is the component whose `package()` it is, ahead
  of the rule that gives the whole engine share folder to llama.cpp) and report the content pin as a
  hand-over, naming the helpers it needs (`scratch`, `put`, `ELF`, `place`).
- zbus runs every task of a connection on one executor thread: a sync `fn` method blocks every other call
  on that connection.
- zbus types come from Rust shapes, not from the signature you meant: a fixed-size array
  `["x".to_string()]` serializes as a STRUCT `(s)`, so an `as` argument needs `vec![...]`; a Rust tuple is
  a struct, so a rows method answers `a(ssqasas)`. A `Vec<Vec<String>>` never compares against a
  `[(&str, &str)]` table: join each list with `,` in the assertion.
- Pin a row's signature as `<Row as Type>::SIGNATURE.to_string()`: this zvariant has NO `as_str` on
  `Signature`, so comparing `SIGNATURE` with a `&str` is a compile error, not a red. The red of a new field
  prints `(ssqasas)` vs `(sssqasas)`, which names exactly the field the transport must carry.
- zbus does NOT refuse a 0-parameter method called with an extra argument, so an `expect_err` on that call is
  a red the coder cannot pass. Where the design gives a method no input, pin the no-argument call and its
  reply signature (`ServerProfiles` with `&()`, answering `a(ssqass)`), and drop the extra-arg case: the shape
  of a call the design never fixes is zbus's business, not a rule of ours. Prove the shape loop you keep is
  not vacuous by dropping one case's bad argument: a legal call answers and the `expect_err` panics naming
  that case.
- A client test that feeds an `{"error":{...}}` frame can only name a wire kind the decoder maps
  (`ClientError::from_reply`'s list, the manager's own): a kind it lacks decodes as `Other`, so an
  `Unavailable` reply frame is a red nobody can pass. `Unavailable` is the TRANSPORT's own (no reply,
  a failed write, a cut frame), pinned for every method by the `EVERY_METHOD` loops: pin a refusal as
  the kind the gate actually sends (`NotAuthorized`), and say in the doc comment where the transport's
  failure is pinned.
- A fixture closure that builds "the list" with its rows (`cards(t)`, `listed(t)`) is not the empty
  answer: calling it with `None` pins the rows and drops only the time. Where the design says "no
  probe answer is no cards and no time", write the empty value out (`NodeDevices { probed_at: None,
devices: Vec::new() }`), so the test reds naming a card, not a clock.
- `Refusal` is public in `pai_core::checks` (`key: &'static str`, `values: Vec<String>`), NOT re-exported by
  `pai_core::client`: import it from `checks`, and write the key as a bare literal.
- One `data_root_e2e` runs every phase in one process over one tree: a phase that writes the record must
  restore it at its own end, else the next phase's "a refused call wrote nothing" assert reds on the
  previous phase's bytes, and the red names a file nobody touched in that phase.
- A manager e2e that starts an instance needs that instance's RENDER, not just the pool: a start whose
  `models@<id>.ini` names no model profile is refused `instance-no-model-profiles`, so a phase that is
  about a caller, a log or a transport must seed `assignments.ini` AND the render, and a seed of a profile
  the phase writes later must be in the assignment too (the manager re-renders from the pool on every
  accepted pool write, so a hand-seeded render is clobbered by the write, and an assignment naming a
  profile the pool lacks is refused `model-profile-model-missing`).
- A fake service answering ONE `state` for every unit cannot pin "a remove refuses the running instance"
  and "the removes run on a stopped tree" at once: give the fake a `running: Vec<String>` cell
  (`FakeService::new().stopped()` / `.running(&["lan-2"])`), and pin the refusal as the status call of that
  one id, so "no refusal calls remove" stays visible in the call list.
- `ini::text::set_key` with an empty value writes the key with a trailing space (`Tags = `): assert
  `contains("Tags = \n")`; `"Tags =\n"` never matches.
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
  each one a `Response` (`reply(200, {...})`) or a fn returning one, never a plain object. The stub
  records EVERY route in `calls` before routing it, session routes included, so
  `to('POST', '/api/session')` counts a sign-in answered by a plain `Answer`: never extend the harness
  for that. So a page helper that renders takes the bff and returns `{ bff, view }`, not one of them,
  and a test that needs
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
- A new `role="status"` paragraph inside a section reds every older helper that pins "the bar's status is
  the only `role=status` below the section's h2" (`status()` of ModelProfileSave*, `oneStatus` of
  test/savealert.tsx): brief 14's GD1.3 "Detecting GPUs…" is that second status. The pin is a RACE (it
  reds only while the new read is pending), so the suite's failure count moves between runs (25, 26, ...):
  report the count as a range and name the one reason, never as N separate defects.
- To tell a real red from cross-test pollution, run it alone: `npx vitest run src/<file>.test.tsx -t "<the
test name's first words>"`. A red that survives solo is real; a flake (brief 09's S1a.10 nav alias) passes
  solo and in a plain run, and reds only under the json reporter.
- A fetch wrapper that logs `(await r.clone().text())` of a fake's Response can print a TRUNCATED body
  (the tee cuts at a chunk boundary: `{"revision":"r2"}` for `{"revision":"r2","running":false}`). Never
  conclude from that print that a reply lacks a key; grep the fake for the literal that builds it.
- A card added to the shared card list reds EVERY whole-page test of the suite, not only the new file: the
  card-order and tab-path tests of each cards file, the count / strings-parity tests, the page test's tab
  path, the heading-order tests, and `App.test.tsx`'s strings test (the coder's key pin). Count them in
  the report as collateral reds of the shared list, one line per file.
- A brief that names a landmark from a `<p>` the CSS hides is two pins: the attributes in the new file
  (`aria-labelledby` present, `aria-label` absent, the `<p>`'s classes, its text), and the
  `getByRole(.., { name })` reading as a guard. Retarget every role-by-name query of the suite in the same
  step and report them as collateral reds of the reword: a hidden label reaches an assistive technology,
  never a layout test, so the visible reading is the UX pass's.
- Under `vi.useFakeTimers()` (the list pages), `findByText(new text)` for a text the code does not render
  yet never settles: the test dies at the 5 s vitest timeout, which reads as a broken harness. A guard
  criterion (an existing behaviour pinned for a new key) must not wait for the new text: `flush()` inside
  the mount helper, then assert the alert's COUNT (`queryAllByRole('alert')` length, the element's class),
  so its green means the guard and not the absence of a wait.
- A new user string is a two-step red: put the key and the brief's English (verbatim, backticks included)
  in the `EN` table of `fe/src/test/bff.ts` ONLY, and pin the same text against `en_US.json` in
  `strings.test.ts`. `App.test.tsx`'s "1a strings" pin then reds with `keys missing or with other text:
expected [ …(N) ]` naming the keys: that is the coder's hook, report it as such.
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
- A shared start list that GAINS a card's disclosure reds every file whose local tail list starts with
  that same disclosure (`CARD_TAB_STOPS` led with `en('model-profile-assets')`): drop it from the local
  list in the same step, else the expected list names the toggle twice and the delta reads as an
  off-by-one nobody can trace. In the same step, a test title that said "the first card's About toggle"
  must name `fieldAboutToggles()[0]`: the card's rows carry no About toggle, only the 2e fields do.
- Retiring the last use of a destructured mount result (`const { main } = await loaded(..)` followed by
  the deleted `await engineSelect(main)`) is a `tsc -b` TS6133 that the Vitest run never shows: drop the
  binding with the pin. The same for a helper the retarget leaves uncalled: delete it, its doc comment and
  its now-unused string constant (`engineSelect` and `ENGINE` in `ModelProfileSave16`).
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
- A field moved from a `form` (label on the input) to a `fieldset` (legend = `role="group"`) reds every
  helper that locates it by its textbox plus `closest('form')` (the `inTags` of RunControls,
  SessionRevoked, Timeout and Boot's `runStatus`, which keep the field's own status out of "one run
  status in the header"), every whole-page tab path, and any "the page has exactly N inputs" inventory
  whose text input is now inside a pill. Retarget all of them to `queryByRole('group', { name })` in the
  same step, and report them as one collateral line per file, named by the ONE reason.
- Pin a field's Action feedback by a ZONE, not by a node, when the brief's layout sketch is a code block
  whose indentation is the only clue: "the one `role=status` inside the group, or between the group and
  the section's `h2`" passes both readings and still excludes the header's run status, the boot field's
  and the save bar's. A strict "after the fieldset" pin reds a compliant implementation that nests them.
- Icon-only buttons whose words are a visually hidden span (`+` "Add tag", "Add", "Cancel",
  "Remove tag v2") are reached by `getByRole('button', { name })` on the exact words, never by their
  classes and never by `input.closest('div')` for the pill: exact-name matching keeps "Add" apart from
  "Add tag", so the pill's two buttons need no container walk.
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
  the page renders. `afterEach` needs `vi.useRealTimers()` beside `vi.unstubAllGlobals()`, and BEFORE any
  awaited flush: a test that dies under the fake clock leaves an act queue open, so every LATER test of
  the file dies in its own `afterEach` as `Error: Test timed out in 5000ms`, which reads as three broken
  tests. The tell is a file whose duration triples: `grep -c "Test timed out"` over the log must be 0
  before a red is reported.
- A file that needs BOTH the page's 5 s poll and the leave guard keeps them apart: only the poll test
  passes the clock to the mount helper, the guard tests stay on real timers (`pressBack` is
  `act` + two `await new Promise((r) => setTimeout(r, 0))`, the `LeaveGuard.test.tsx` shape), and the
  clocked mount uses `flush()` + synchronous queries, never `findBy*`.
- A new strings key reds two existing pins, report both: `App.test.tsx` "the English the brief gives"
  lists every `EN` key `src/strings/en_US.json` lacks (the coder's key pin), and the same file's reverse
  lint scans the source, so the key goes in the test's `EN` table ONLY in the red step: adding it to
  `en_US.json` is a red no tester may fix (the production `t('key')` call is the coder's).
- A new focusable control in the banner goes red in EVERY document-wide tab-order list: grep the suite
  for the lists that start at the skip link and insert the new stop in each; a banner-scoped list
  (`within(nav)`) needs nothing. A new LAST nav item is the same red in the exact nav-link lists: append
  it there, and let a loop over the nav's named groups skip a group with no label
  (`group.hasAttribute('aria-labelledby')`), so a criterion that gives the new group no label pins nothing
  beside itself. Queries that only check a link exists need nothing.
- A read the shell adds to every signed-in render (a host name beside the version): give the shared fake's
  new option a DEFAULT answer (the reply the brief shows) so the suite's untouched tests do not meet a
  rejected `unexpected GET <route>` fetch, and let a test that wants a failed read name its own answer.
  Then pin "the page asks nothing for it" as a FILTERED call list (`calls.filter((c) =>
c.path.endsWith('/boot'))`), never the whole `bff.calls`: an unfiltered emptiness assert reds on the
  shell's new read, which is not what the criterion says.
- When that host read becomes a HEADING (`<h2>{host ?? t('instances-title')}</h2>`, the Instances section),
  every older `getByRole('heading', { name: en('...-title') })` of the suite goes red at once: the shared
  fake's default `NODE.name` is the name the section answers, and the key's text shows only while the read
  is pending. Retarget each to the node name (or a level + `closest('section')` walk), and report them as
  collateral reds of the heading, never as harness typos. (`<h2>{host ?? t('instances-title')}</h2>`, the Instances section), every older `getByRole('heading', { name: en('...-title') })` of the suite goes red at once: the shared fake's default `NODE.name` is the name the section answers, and the key text shows only while the read is pending. Retarget each to the node name (or a level + `closest('section')` walk), and report them as collateral reds of the heading, never as harness typos.
- A plain object passed as a `fakeBff` constructor option is the body of the 200 only where the route
  itself wraps non-functions. A helper that gates EVERY option behind a "session gone ? 401 : answer"
  wrapper (the shape of `endedBff`) must be handed the body (`node: NODE`), never `{ body: NODE }`: its
  `reply(200, after)` lands the wrapper's own object as the body and the read parses nothing.
- A state that moves from a per-row read to a field of the list answer: retarget ONLY the reads the
  criterion removes. The instance page's own field reads the same path and stays, so rename the constant to
  say whose read it is (`TILE_BOOT` to `BOOT`) and report which pins stayed, else the coder deletes a read
  the brief keeps. In a test that counts a route across a page change, the detour page must be one that
  makes no such read, and the comment must say so: that count is the red that pins the removal.
- Every nav query needs `within(nav)` (the Overview table names the same instance ids as the nav), and a
  link click in jsdom does not always move `window.location.hash`: pin a close on `aria-expanded` and the
  landing page by its heading. The browser's Forward is testable after a leave guard: a typed hash adds a
  history entry, so build the history with a typed hash, never with a second render.
- `getByRole('button')` returns `HTMLElement`, so `.disabled` is TS2339 in the type check and passes the
  Vitest run (Vitest never type-checks): cast at the query. A helper that wraps an async finder is async:
  `within(findX(main))` is TS2345 until awaited, in the helper and at every call site.
- `act` comes from `@testing-library/react`, not from vitest (importing it from vitest dies "act is not a
  function"): import it in any file that dispatches an event the app's own listener installs outside
  React's tree. `fireEvent.click` on an `<a href="#/...">` (an in-app link) navigates in jsdom and
  settles, no `Not implemented: navigation` (only a real URL prints that), so a nav-link leave is
  `await act(async () => { fireEvent.click(navLink(name)); await new Promise((r) => setTimeout(r, 0)) })`:
  that is how the router sees it. The hash moves only after that flush (read it in the same tick and it is
  still the old one), so an "opens the page" pin takes the link's `href` attribute and the landing heading.
  `@testing-library/user-event` is NOT a dependency of `fe/`.
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
  punctuation finds 0 rules, which reads as "the rule is missing" when it is there. So a descendant
  selector handed to the exact-match locator must pass through the SAME strip as the parse, else the
  locator reds "found 0" on a rule that exists, and that typo reds a criterion expected green. Exempt a sanctioned
  case by the DECLARATIONS it carries, never by a selector name (a name-based allowlist exempts a rule
  that only looks the same), and read a selector list's shared body, not each selector.
- A rule ADDED to a forced-colours block reds the system-colour whitelist of `fe/src/HearthTokens.test.ts` (its `SYSTEM` list plus the word-by-word exemptions: a new system colour such as `LinkText` is the coder's edit there, never the tester's), and a NEW forced block reds that test's exact block count: writing the rule inside an existing block keeps the count. The same file pins every `min-height`/`min-width` as an ORDERED per-rule list, so a rule the brief REPLACES must be replaced in place, not appended.
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
- A cascade scan weighs the chain of real ancestors, element outwards (`[nav, section, main, div.layout,
#root, body, html]`). A probe selector whose ancestor ORDER is inverted (`main .layout .jump`, when
  `main` sits INSIDE `.layout`) matches nothing, so the probe returns `[]` where a violation is expected,
  and the scan reads toothless. Write every probe selector from the real ancestor list, top down.
- A probe compares one property, and only when the rule under test DECLARES it: a competitor on
  `max-width` against a rule that declares `display` is skipped and greens vacuously. Give the probe's
  rule the property the probe weighs.
- The specificity a scan prints comes from its own helper, not from hand arithmetic: a hand-written
  `(0,2,2)` in the expected string reds a probe whose scan correctly printed `(0,2,1)` (`:has()` takes the
  specificity of its argument, so `.layout section:has(> .jump)` is 0,2,1). Run the probe, read the line
  it emits, then write that line.
- End a cascade scan's self-probe block with a probe of the FINISHED shape, a mini sheet carrying the
  rules the slice is meant to add next to the real neighbours (`main > *` and the short-viewport block):
  it proves the scans stay silent on the target, so the coder's green is reachable, and it costs no
  mounts. In such a sheet, a tie written after is a violation: only a LOWER specificity buys silence.
  Write the mini sheet's entry rule scoped the way the slice will scope it (`.jump ul button`, not
  `.jump button`), else the probe's "the pill wins" expectation is wrong on its own sheet.
- Weigh every element of the slice in ONE pass: collect each chain's misses into one array and assert
  once. A test that asserts chain by chain stops at the first chain, and the report shows the label's
  misses while the toggle's (`32px` beating the 40 px pill) stay hidden from the coder.
- A mount helper that ends in `findByRole` / `ready(main)` (the GPU files' `form()`) must be called BEFORE
  `fakeClock()`: its finds poll on faked timers and deadlock, so every clocked test of the file dies
  `Error: Test timed out in 5000ms` (five reds, one root cause, and the reason line names nothing).
- `Field.tsx`'s `Hint` renders one `<p>` per `\n` of the hint, so `getByText(<whole multi-line text>)`
  NEVER matches, for a hint of any length: pin it the way `HearthAssetsEngine` does (`.field-hint`,
  normalized `textContent`). A hint key whose text has `\n` is a red no coder can pass.
- Vitest's jsdom loads no stylesheet, so `getComputedStyle` of a mounted page sees the user agent only: a
  cascade question ("does the pill win over the entry rule?") is answered by the parser plus specificity
  arithmetic over the file's text, never by the DOM. Weigh the real `index.css` chains and keep a
  hand-written mini sheet for the shape the slice must reach.
- A focus ring's outline cannot be weighed when the sheet already carries
  `[tabindex='-1']:focus { outline: none }` (jsdom matches `:focus` in a selector, so every chain reports
  `outline: none` whatever the code does): pin the ring's DOM half (the class, no `tabindex`, enabled),
  guard that no rule naming the slice's class declares an `outline`, and report the rendered ring OPEN for
  the UX pass.
- The rules a narrow-viewport scan weighs: the top level plus every block whose media window can hold
  the width, so `@media (min-width:`, `prefers-color-scheme:` and `forced-colors:` blocks are out. The
  same ranges answer "the slice put its rule in the block it named" and "no other block declares it",
  which is how a `display: none` at one width is pinned without a browser.
- A whole-page mount's `bff.calls` starts with `/api/session` and `/api/instances`: pin "this slice
  writes nothing" by filtering the calls by method, never as the whole path list, and pin one read with
  `bff.to('GET', path)`.
- The model-profile section holds card intro `<p>`s and card fields that are `select`s: a bare
  `getByRole('paragraph')` or `getByRole('combobox')` inside it reds "found multiple". Query by name
  (`{ name: en('model-profile-engine') }`) or by class (`section.querySelector('p.notice')`).
- Prettier 3.10 in a `semi: false` package KEEPS the semicolons a file already has: that file stays
  format-clean, and a semicolon-less statement appended to it stays clean too, so an added assertion
  needs no style edit in the existing file.
- A model-profile Save is INERT on a clean form (no PUT goes out), so every refusal test of a new card must
  make a real change before clicking Save: a checkbox, a Move, a typed weight, an edit of a key the card
  shares. A click on a clean form sends nothing and the refusal's `findByText` dies at its 1 s timeout,
  which reads as "the check is missing". For a key the page composes (`tensor-split` is sent only with a
  checked row), the change must touch THAT state, else the injected 422 never lands on the key.
- A refusal injected by a `modelProfileSave` option is a CONSTRUCTOR option, so a test that wants both a
  device refusal (422, sent) and a client-check refusal (nothing sent) needs two saves in one page: change
  the device, Save (the refusal shows), then break a weight and Save again (the check stops it) to pin that
  the two error lines sit in the brief's order and focus goes to the check's field.
- A card intro that names a key in prose ("the split order ... `tensor-split`") makes every
  `queryByText(/tensor-split/)` absence pin inside that card a permanent miss: pin the ABSENCE of the
  FILLED text (the string built with `en(key, value)`), never of a substring of the key's name.
- The test table's `en(key, ...args)` throws `no English for <key> in the test`, and its args are STRINGS:
  a "the key exists" test is `expect(en(key)).toBeTruthy()` with no query at all, and a positional
  (`{1}`) filled from numbers is TS2345 in `npm --prefix fe run typecheck` while the Vitest run greens
  (Vitest never type-checks). A fixture row typed `number | string | null` needs the same `Number(..)`
  at every arithmetic site, for the same reason.
- `npm --prefix fe test -- <word>` filters by SUBSTRING of the file path, so `ModelProfileSave1` runs
  `ModelProfileSave10` to `19` too. Quote the coverage of a collateral run by the FILES it ran, not by the
  filter words.
- A card added to `test/modelProfileCards.ts` reds 30-35 tests in ~14 files, all naming the card COUNT or
  the card ORDER: `App.test.tsx` 1a (the strings pin, its message lists the new keys), `HearthJump`,
  `HearthJumpRail`, `HearthJumpToggle`, `HearthAssets`, `HearthAssetsEngine`, `ModelProfileCards1` to `5`,
  `ModelProfileContent`, `ModelProfilePage`, `ModelProfileSave16`. One is the opaque
  `Unable to fire a "click" event - please provide a DOM element`: a jump test that indexes the buttons by
  position. Report all of them as collateral reds of the shared list, one line per file.

- Run fe vitest from `fe/` with `TMPDIR=/tmp/opencode`: vitest writes `node_modules/.vite` (the source tree
  is gitignored only under `dist`/`coverage`, so a sandbox that read-only binds the repo dies with
  `EROFS ... unlink tsconfig.json.tmp`), and `npm --prefix fe <script>` runs npm in that sandbox as well (it
  creates `fe/npm-cache`, and a sandboxed `npm --prefix fe test` stalls in the background on an esbuild
  install postinstall, printing nothing).
- `fe/vite.config.ts` has no `setupFiles`: jest-dom's matchers (`toBeInTheDocument`, `toHaveAttribute`,
  `toHaveClass`) are NOT installed, and `@testing-library/user-event` is not a dependency. Read
  `package.json` and the config before importing; use `.className`, `.textContent`, `toBeTruthy()`.
- Prettier's config is found per package: `fe/package.json` carries the fe's rules (no semicolons, width 100) and the repo root has no `package.json`, so a root-level `npx prettier --write <file outside fe/>`
  uses Prettier's defaults (printWidth 80) and rewrites lines nobody touched. Format from `fe/` with
  repo-root paths (`npx prettier --write ../crates/...`). `npm --prefix fe run format:check` covers
  `crates/bff/ui/*.html`, `.github/workflows`, `LICENSE` and `README.md`, which a production commit may
  have left unformatted: format them and prove with `git diff -- <file>` that only whitespace moved.
- Right after `render(<App />)` the Overview's `main` holds only the loading `<p role="status">` (the
  session has not settled): a synchronous `main.querySelector` finds nothing and every case of the file
  dies in its own helper. Await the section's heading (`findByRole('heading', { level: 2 })`) before any
  synchronous query. The same applies to a page whose own read lands after its h1: the h1 is on screen
  while the lists are not, so a criteria that reads them awaits the page's `h2` first.
- "The page takes the focus on its h1" is a property of an arrival, not of a load: the app focuses the h1
  when the route changes and on nothing else (a direct load at the hash leaves the focus on `body`, and
  the sign-in page uses `autofocus` instead). So reach the page with the fake's `navigate()`, never with
  `startAt()`, and keep `startAt()` for a criterion that pins the signed-out state of a page.
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
- A test of a component the slice has not written imports it by a path kept IN A VARIABLE
  (`const M = './Icon.tsx'; await import(M)`): a literal specifier is resolved by vite's import analysis
  at TRANSFORM time, so the whole file collects 0 tests and the suite dies "Failed to resolve import",
  which reports no per-case red. The variable form loads the file and each case reds on its own import,
  naming the module. The type check cannot see a variable specifier, so `typecheck` is clean: report the
  missing module as a runtime red, never as a TS2307.
- `HTMLCollection[i]` is `undefined` out of range, not `null`, and `expect(el).not.toBeNull()` greens on
  `undefined`: the red is then the opaque `Cannot read properties of undefined (reading 'tagName')`. Guard
  a child the slice must add with `toBeTruthy()` and a label, so the red names the host that has no icon.
- Deleting keys from the test's shared `EN` table reds nothing in `strings.test.ts` (its parity check reads
  `src/strings/en_US.json` and the source files, not the table); deleting them from the JSON while
  production still uses them reds its "every value is used" check, so a strings pin in a new file of the
  slice is a duplicate guard of that one.
- In a mutation scratch copy of `fe/`, `src/HearthTiles.test.tsx` is ALWAYS red: it reads
  `../../crates/core/strings/en_US.toml` from `import.meta.dirname`, which resolves outside the copy. Judge
  a survivor on the rest of the full-suite failures, and never report that file as a kill.
- Install `pushState` / `replaceState` spies AFTER the mount: the harness's own `startAt(hash)` writes the
  bar, so a spy installed before it counts the harness, and "one replaceState of `#/`" reds on a correct
  app. The same rule saves the `history.length` baseline.
- jsdom's `history.length` cannot pin "no entry added" for a fragment typed and then replaced inside the
  `hashchange` handler (a bare `replaceState` there moved jsdom's length by 2, a browser's by 0): pin it as
  `pushState` NOT called plus the exact URL of the `replaceState`, and write the limit in the comment.
- An alias rule usually has TWO code paths: the arrival whose address equals the state hash takes the
  "no navigation" branch, and the arrival that MOVES the address takes the `setHash` branch. A test from
  the Overview alone leaves the second path a survivor: pin both.
- When a shell part starts arriving with the session (a nav, a Menu button, the banner's Sign in), every
  synchronous `getByRole('navigation')` helper reds: add an async finder helper and take the part once the
  session answered, and settle signed-out states by the sign-in `h1`, never by a nav finder (that burns
  its 1 s timeout and hides the real red). A helper that waits for the banner's signed-out Sign in link
  needs retargeting the same way once Sign out lands on the sign-in page.
- On the fake clock a testing-library `findBy` / `findAllBy` DEADLOCKS: its polling interval is a faked
  timer and nothing advances it, so the case dies at the real 5 s timeout, which reads as a broken test.
  Write one `appear(find)` helper that loops the sync getter plus `flush()` and use it everywhere on the
  fake clock; keep a real `findBy` only in a case on the real clock.
- A mount helper that takes the fake bff only because `fakeBff()` installs itself as `globalThis.fetch`
  trips `TS6133 'bff' is declared but its value is never read` in the type check: have it return
  `{ bff, main }` and let each case destructure what it asserts.
- Sign out lands on `#/` with NO notice at all (09 S1a.11), so "the notice never shows after Sign out" is
  pinned as an EMPTY notices list, not as `signin-required`: the latter reds on a correct app.
- A field of `InstancePage` (`GroupField`, `Boot`) renders its status slot `<p role="status">` empty AT
  REST, so "this field carries no live region" is false today and is not a red to write: pin what the
  brief says, that its read paragraph (`bootReadText`) is a plain `<p>` with no `role`.
- `test.each` over array items APPENDS the arguments past the `%s` placeholders to the title, so a builder
  function in the tuple prints as source text in the test name. Order the tuple so every value the title
  shows has its placeholder and the builders come last, and type the tuple with all its elements.
- A query for an element the app adds in a task of its own (the Sign in beside an alert that arrives with
  the save's reply) must NOT be synchronous: `within(form).getAllByRole('button', { name })` right after the
  click dies "Unable to find an accessible element with the role button and name X", and the DOM it prints is
  the state BEFORE the reply landed, with no `role="alert"` in it, which reads as an implementation gap. Await
  the first observable (`await screen.findByText(text)`) and only then take the element after it. A throwaway
  probe (`ZzProbe*.test.tsx`, run solo, deleted after) that prints the form's `outerHTML` settles in one run
  whether the code is missing the control or the test read too early; in a probe, `JSON.stringify` of an
  element array dies "Converting circular structure to JSON" (React's fiber hangs off the node): map to
  `textContent` first.
- A test of the browser's Back needs a history entry behind the page: a mount helper that starts AT the
  page's hash has nothing to go back to, and the leave guard never fires, so the dialog pin dies "Unable to
  find role=dialog". Build the history the way `LeaveGuard.test.tsx` does (start on the Overview, then a
  typed hash inside `act`). The same helper breaks the next mount in the same test: a clean save that
  navigates away leaves the bar on the Overview, so `startAt` back on the page before mounting again, else
  the second mount dies "Unable to find role=heading and name <id>".
- "A clean save asks nothing when leaving" is pinned as the guard ABSENT (`queryAllByRole('dialog')` empty,
  no `preventDefault` on a dispatched `beforeunload`), never as a hidden dialog: the app puts no guard in the
  document at all when the form is clean (LeaveGuard 1b.38a, 38e), so a pin that waits for a `hidden` dialog
  is a red no coder can pass.
- A new `<p role="status">` anywhere in the instance page's header reds every "one run status" pin of the
  suite (53 tests in `RunControls`, `SessionRevoked`, `Timeout` fail `expected [...] to have a length of 1
but got 2`), a new CSS class reds the class allowlist of `Reflow.test.ts` and the size pins of
  `HearthTokens.test.ts`, a new control reds every whole-page tab list (`Boot`, `InstancePage`,
  `ServerSave`), and a page-wide key scan reds `InstancePage.test.tsx` `1a.27e` (a withheld key rendered
  nowhere: it finds the tags box). Count them per file as collateral reds of the coder's change; the fix is
  the coder's retarget, never the tester's, in the red step.
- When the coordinator briefs the retarget: a shared helper copied into several files (`runStatus` lives in
  `RunControls`, `SessionRevoked`, `Timeout`, `Boot`, each its own, one with the boot filter inlined) means
  FOUR edits, so grep the helper's name for the copies, and mirror the existing exemption (an
  `inTags(main, el)` found by the new field's accessible name + `closest('form')`, beside `inBoot`'s group).
  Add the exclusion to the status helper ONLY: a brief whose alert renders only with text keeps
  `runAlerts`/`runSignIns` whole, and widening them hides a real second alert.
- A document-wide `querySelectorAll('input')` type scan is retargeted to the region whose absence it pins
  (`getByRole('heading', { level: 2 }).closest('section')`, expect `[]`), and the edit must keep every
  OTHER assert of that test: an `oldString` that spans them deletes them (found by re-reading the test
  body after the edit, not by the run).
- The CSS pins are per-slice LEDGERS, and a brief that says "nothing new needs a style" makes them the red
  the coder must pass, not a list to extend: `Reflow` R1.8 is every class the sheet selects (plus the one
  its brief allowed), `HearthTokens` H1a.12 every `min-height`/`min-width` pair as
  `selector: prop: value` in sheet order. Measure the exact delta with a scratch node script that copies
  the test's own parser and takes the expected list FROM the test source by regex, never retyped (vitest
  truncates the arrays in the message).
- A `sets('<property>')` ledger (Reflow R1.1: every rule that sets overflow-wrap, as `selector = value`)
  is the red for a rule that ADDS the declaration, so a brief asking for a new break updates that ledger
  AND gains its own `declares` test in the same step; the class freeze (R1.8 `CLASSES`) is the pin that
  goes GREEN the day the sheet carries the classes, so it is an update-pins item, not a red, and what
  names the amending brief is its title and comment, never a count (a count comment goes stale next slice).
- A field outside the Server form is NOT remounted by a sign-in as another account (the page keys only
  `ServerSection` on `signIns.rereads`), so its box keeps its text and its dirty flag: that one missing
  rule is three reds in a row (the box's value, the `beforeunload` ask, the discard dialog on a nav
  click). Prove the two behind the first by flipping its expected value with a `DIAG` marker, then restore
  with `cp` + `diff` (byte-identical), and report the notice's Server-form half as a green guard.
- A suite-load flake to expect: `HearthTheme` H4.7's `document.title` lags the page's h1 by a tick, so it
  reds in a full-suite run and passes solo. Prove it with three solo runs plus a second full suite before
  naming it; report it as a flake, not as a red.
- One line per red, with the reason, for a file that does NOT mount:
  `npx vitest run src/<file> --reporter=json --outputFile=/tmp/opencode/<name>.json` (vitest 5 accepts
  `--outputFile` for the json reporter), then read the file with node: each
  `testResults[].assertionResults[]` carries `status`, `title` and `failureMessages`. The reporter option is
  a flag of its own (`--outputFile=`, not `--reporter-option outputFile=`). For a file that MOUNTS this
  reporter is useless: it runs the file outside jsdom (`vite.config.ts`'s `environment: 'jsdom'` is not
  applied), so every test dies `ReferenceError: window is not defined` in the harness's `beforeEach`
  (`startAt`), one line for every red, naming nothing. Prove a mounting file's red from the default
  reporter's log.
- The default reporter prints the detail of the FIRST test carrying each distinct error and collapses the
  rest into a two-line `FAIL file > title` block, so `grep -c "form src/<file>"` under-counts a red. Prove
  "every red is the expected one" by counting the DISTINCT `*Error:` lines over the whole log: strip the
  colours first (`sed 's/\x1b\[[0-9;]*m//g'`), then
  `grep -oE "[A-Za-z]+Error: [^\n]{0,90}" | sort | uniq -c`: one line, every red, is the proof the file has
  no typo hiding behind a shared cause.

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
- A stylesheet-cascade reader over element chains needs two rules the browser has: a compound carrying an
  attribute selector, a pseudo-class or a pseudo-element NEVER matches a chain built from tag/id/classes
  (the sheet's `[hidden] { display: none !important }` else wins every display read), and a shorthand is
  folded into its longhand sides (a later `padding:` beats an earlier `padding-left:` at a tie, that is
  how an icon placement is undone). Self-probe both on doctored sheets, and make "nothing declared" read
  as `''` so a missing rule is a miss, never a pass.
- Run `npx prettier --write` on your new fe test files BEFORE editing them further: prettier rewraps, and
  an edit built from the pre-format text misses. A `write` that "succeeded" may not be what the disk
  holds after a formatter pass: re-read the file before judging which draft is live. A `test()` name or a
  message with an apostrophe is written in DOUBLE quotes: in single quotes it breaks prettier's parse and
  the file never formats, which reads as a broken file.
- `within()` needs an `HTMLElement`, not the `Element` a `querySelector` gives: cast at the query, not at
  each `within`. The fe gates reject `toHaveAttribute` and friends (no setupFiles, no jest-dom): pin with
  `getAttribute(...)` / `toBe`.
- `expect(el).not.toBeNull()` narrows nothing for `tsc -b`: every later use of that `Element | null` reds
  `possibly null`. Guard, then bind a `const el = found as HTMLElement`, and keep the guard's label.
- A disclosure that hides an EXISTING card's body turns every test that reads that body into one that
  opens it first: put the open step BEFORE the test's first role query of the rows (a scripted insert
  after the mount block lands after `pairs(card)` and reds the test for the wrong reason). The tests that
  only locate the card's heading stay green, so enumerate the readers by grepping `definitionsWithLabels`
  / `getAllByRole('definition')`, not the card's name.
- A new focusable control inside one card joins EVERY whole-page tab list. Where a file builds the list
  from a shared `CARD_TAB_STOPS` the edit is one line; the files that inline the Engine stops
  (`ModelProfileCards1` to `5`, `ModelProfilePage`) each need their own insert, and a list whose tail is a
  `tabbed.slice(start.length)` compare takes the control at the tail, not in its `start` prefix.
- A jump entry is clicked from the list container by its TEXT: once a card carries a disclosure named by
  its own title, `getByRole('button', { name: title })` matches the entry and the toggle. And a shared
  card list already holds the card the slice styles (`cardHeadings()` includes Assets): "the cards in the
  order of 83" is that list, not `[ASSETS, ...list]`, which reds as a card missing.
- A spy that records state while the app's own handler runs must never throw: read by
  `document.getElementById` and return nulls for a missing element. A throwing spy inside React's dispatch
  becomes an unhandled rejection, and the run then reports no per-assert reason.
- React's StrictMode re-runs only the MOUNT pass of an effect, so an effect that early-returns at mount (a
  read whose data arrives later, `session` still null) is untouched by it: a StrictMode test expecting an
  in-flight answer dropped by that mount cleanup is GREEN by construction. Render under `<StrictMode>` to
  pin the tree `main.tsx` really builds (the suites that `render(<App />)` do not), and drive a cleanup
  race with a real state change (a sign-in, a page write answering 401), where React does run cleanup and
  rerun; a `held()` reply plus that change shows an answer the app drops.
- A tile's fact list is one `dl` that grows (`tile-facts` holds port, tags, boot): a helper returning
  "the tile's `dl`" for one row returns the whole list, so a restored `dt`/`dd` count must name the rows
  the items in that test carry (the plain `INSTANCES` and `item()` ones carry only boot, so 1), never a
  blanket 1.
- Clean up a test's leftover process by its exact PID (have the script write it to a file), never with
  `pkill -f <words>`: on 2026-11-19 `pkill -f "sleep 600"` also matched the coordinator's launcher, whose
  command line carried the brief's text, and killed it.

## Mutants of a security rule (slice 14 close, 2026-11-20)

- A mutant is a one-line break of the rule, run, then `cp` from your own backup and `cmp` (the next mutant
  starts from the identical bytes). Back up every file you touch first, and `sed -i "<line>s/../../"` by
  line number where the text is not unique in the file (five identical `session-required` guards in the
  bff's `server_profiles.rs`); print the line back to prove the mutation landed.
- Pick the mutation that reds the rule's OWN pin. Inverting an existence check (`is_ok()` to `is_err()`)
  reds the first success assert, which proves only that the check runs; `is_ok() && false` reds the rule's
  own assert ("a second create of the same name: left `Ok(())`"). Report both when you ran both.
- A `FakeAuthority` answers every `Action` the same, so an allow-list refusal never tells `Configure` from
  `Control`: the mutant that swaps the gate's action is caught ONLY by a pin of the asked list
  (`asked(&c) == vec![(caller, Action::Configure); N]`). Where such a pin is absent, write it.
- The record read of a delete: `read_record(..).map_err(io)?` to `.unwrap_or_default()` reds the Io pin
  (`the read's own failure, not "unused": Ok(())`); `record_holder(&record, profile)` to a name no section
  names reds the in-use pin. Those two are the smallest breaks and each reds one assert.
- The pipe's `port()` closure answers `Option<u16>` and `port()?` is a `u16` (the frame's `Request` carries
  `u16`, not `Option<u16>`), so a parse mutant keeps that shape
  (`u16::try_from(v.get("port").and_then(|p| p.as_u64()).unwrap_or(0)).ok()`); a mutant of the wrong type is
  a COMPILE red, which is not a mutant.
- The manager's wire rules (the `configure` gate, the name rule, the port parse, the `As` forms) are pinned
  by `#[test]`s in `manager/pipe.rs`, so they are filterable (`cargo test -p pai-manager --lib <name>`) and
  run OUTSIDE `data_root_e2e`: a mutant there needs no harness change. The core's rules (never-overwrite,
  in-use, Io) live only in the `data_root_e2e` phase.
- A name-rule bypass in a `pipe.rs` unit test resolves the REAL `/etc/llama.cpp` (those tests set no
  `PRIVATE_AI_DATA_ROOT`), so the mutant answers `Io` (`entity not found`, `permission denied`) and the
  refusals red. Prove no file landed (`ls /etc/x.ini`); the EACCES is the only thing keeping the machine.
- zbus rejects an arity mismatch on a method that DECLARES parameters, and IGNORES an extra argument on a
  zero-parameter one: a D-Bus pin that `ServerProfiles("llama.cpp")` is an error reds with the array
  answered. Pin such a rule on the pipe (`BadRequest`) and, for D-Bus, pin that the verb ran
  (nothing asked), never zbus's strictness.
- An absolute row/file COUNT pin inside a shared-tree e2e phase reds on the phase's own earlier blocks (the
  crud phase's 14 blocks create `big`, `zero`, `ghost`, `spare`: a later block's `len() == 5` saw 8). Take
  a baseline in the block and assert a delta, as for a call log.
- To map the phases behind an e2e red, comment the red phase's call in `tests/mod.rs` with a `// DIAG:`
  marker (`sed -i '<line>s|^    |    // DIAG: |'`), run, restore with `cp` + `cmp`: one DIAG run showed the
  second red (masked by the first), the second showed the rest of the e2e green.

## Mutants and wire kinds (G1+G2 close, 2026-11-21)

- A `sed` that replaces a SUBSTRING eats the line (`s/if self\.x/is true || self.x/` leaves `if is true`, a
  COMPILE red, not a mutant): change the WHOLE line (`sed -i '<line>c\        if true {'`) and print it back.
- A mutant that reds a test which is ALREADY red (a fixture step of my own) proves nothing. Say so, and
  report the mutant as unprovable until that step is fixed.
- The `Devices` gate is two halves, each needing its own mutant: the caller's polkit-quiet check (mutant
  `if true`) reds the asked-list pin ("control asked without interaction", the list empty), and a swallowed
  `gate_principal(who)?` reds the non-member refusal ("bob is no member of `pai`", the full list answered).
- Dropping the `As` principal at the pipe's parse (`"DevicesAs" => Request::Devices(None)`) is caught ONLY
  by the transport whose parse you broke: the pipe test reds, the D-Bus one stays green. That split is the
  proof the two parses are separate pins, not a missed kill.
- Pin a `ClientError::from_reply` mapping only for a kind the manager's `error_kind` list emits: there is no
  `Unavailable` kind, so a pipe test asserting `Err(Unavailable)` from an `error_reply("Unavailable")` frame
  is a red only passable by inventing a protocol kind (the transport's Unavailable for that method is
  already pinned by the `EVERY_METHOD` tests).
- A save's calls pin takes the file's convention: the bff's save answer carries `running`, so every save in
  `crates/bff/tests/model_profile_save.rs` expects `list` then `status <id>` AFTER the write; a new test
  pinning 2 calls reds on 4.
- An idle-deadline pin: the step after the read that MOVED the deadline must exceed `IDLE` measured from
  that read (`SHORT = IDLE - 60 s` leaves the session alive, so the closing 401 assert reads 200).
- `cargo test -p pai-bff` runs targets in alphabetical order and stops at the first red: a red in
  `model_profile_save` hides `node_devices` and nine files after it. Run the rest by name in a second
  invocation, and account for every file.

## Scripted edits of a big test file (slice 15, 2026-11-21)

- Never a whole-file regex with `.*` and `re.S` on a 3 000-line test file: one greedy match ate
  everything from a mid-file helper to the file's last brace (1 863, 1 618 and 569 lines gone from
  three files at once). Edit with exact-string `str.replace` and a per-pattern count assert
  (`assert t.count(old) == 14, (old[:60], t.count(old))`) BEFORE the replace, and splice a tail at a
  line index you read back first (`lines[i].startswith("// ---")`), printing the line count before and
  after. A script that replaces blindly truncates in silence; a count assert stops it mid-file.
- `cp` every file to your own scratch dir before a scripted bulk edit. When it goes wrong, the
  recovery sources are `git show HEAD:<path>` of YOUR OWN files (never `git checkout`), plus the
  broken tail you kept: the helpers you had already authored are in the intact head, so the fix is a
  replay of your edits, not a rewrite. The OpenCode snapshot repo
  (`~/.local/share/opencode/snapshot/<hash>/<hash>`) is a git repo of the tree, but its newest commit
  can hold the PRE-edit HEAD content, and `tool-output/` and the `session_part` table of `opencode.db`
  hold no tool results: do not count on them, keep every scripted edit in one reusable block.
- To pin the SET of manager calls without pinning the order the route makes them in, add
  `fn sorted_calls(&self) -> String { calls.sort(); calls.join(" ") }` to the file's fake: one string
  literal compares, and a `Vec<String>` never compares against a `[&str]` table. Name in the assert
  message why the order is free (the coder's choice).
- Where a reply-shape pin (the exact JSON of a 200) sits in the same test as a call-log pin, the shape
  red STOPS the test and the call-log red is never reached: report the test as red for the shape, and
  say the call-list assert behind it is unproven today.
- A positional fake hook (`read_pause_at(position, ms)`: "the read at index 3") is a fixture that dies
  the day the route gains a read: give it the hook the rule names ("the read of `models.ini` that
  FOLLOWS a write"), which holds whatever order the route settles on.

## Mutants of the bff's association routes (slice 15 close, 2026-11-21)

- The BEHAVIOURAL gate mutant (the compile one is the `current` → `peek` swap): replace the route's
  three-line `let Some((session, identity)) = sessions.current(&headers) else { return ... };` with one
  line `sessions.current(&headers).unwrap_or_else(|| (String::new(), crate::session::Identity { user:
String::new() }))` (the bff's `Identity` has no `Default`, so a bare `unwrap_or_default()` is a COMPILE
  red). It keeps `headers` and `sessions` used, so no warning, and the gate test reds naming the ONE
  route mutated: its loop passes the routes whose gate is intact, so the panic body (`200 {"running":
false}` vs 401) is the URI's own.
- The principal mutant is the verb's own connect: `connect(Some(user))` → `connect(None)`. Where the
  client comes from a shared helper (`assign_files`), re-take `connect(None)?` AT THE CALL SITE and
  rename the helper's binding `_client`: mutating the helper kills every route that shares it, and the
  red names one test out of three, which is not a per-route proof.
- A principal pin whose prefix is a prefix of a sibling call reds its "exactly one call" assert
  vacuously: `actor_of("read models.ini")` matches `read models.ini.disabled` too (2 hits, the assert
  prints both). Pin a prefix no other recorded call starts with (a verb plus its args), and prove the
  pin's own green against a call list the route really makes.
- A read a whole route family gains is asked of EVERY `tests/*.rs` fake of the family: a fake with no
  answer for it reds as 502 `client-other` on the route (`switch_then_list_agree...`), and the sibling
  files' call-log pins red as a list with one extra entry (`model_profile_save.rs:1869`). Retarget the
  fake AND the pins of every file, not only the new one.
- An additive member of a served item (`GET /api/instances` gaining `serverProfile`) reds every
  whole-item JSON literal in the crate: 13 of them in `tests/origin.rs` alone, green in
  `tests/instances.rs` the coder updated. `grep -c '"engine":' crates/bff/tests/*.rs` is the map, and
  it names origin.rs as well as instances.rs.
- Drive the mutant loop from a script that `cp`s the pristine file back BEFORE each mutant and prints
  `md5sum` plus `cmp -s` after each restore: seven mutants, seven `restore=identical`, one md5 through
  the whole run. The `cmp` is of the pristine to the live path, not of the live path to itself.

## Reviewer's findings on a landed slice (red tests over uncommitted code, slice 15, 2026-11-21)

- A rule that is "the answer is the record as the MANAGER left it" cannot be pinned by a fake whose files
  cell is shared: applying the write with the bff's own rule makes the computed answer and the read-back
  answer agree, so the test greens on a bff that never re-reads. Add a fixture knob that answers a text the
  bff CANNOT compute (`after_read(text)`, answered for that file once `wrote` is set), and pin the
  after-read happened (`read assignments.ini` counted `>= 2`), else a route that computes is green.
- `running` of an assignment write has three separable rules; write one test each or a fix for one greens
  the others: (a) the effective SET is what counts, so a key landing under a `*` that already ignores it
  (`* = off` plus `gamma = off`, and `inherit` under it) is `running: false` with `assert_no_call("status")`;
  (b) for the whole-pool `*`, build a section where every row has its own key (`* = inherit` +
  `alpha = on`, `beta = on`, `gamma = off`), so removing the `*` changes the text and not the set;
  (c) the answer is the after-read's, so the `after_read` knob is the only fixture that proves it.
- The write-limit seam is PUBLIC (`pai_bff::router_with_session_and_write_timeout`), so a test file needs
  only a twin of its `app_for` (`app_for_write_timeout(fixture, write_timeout)`), with `app_connecting`
  kept as a one-line wrapper so no existing call site moves. The 200-side test (delay 350 ms inside a
  900 ms limit) is the red that names the read-limit cut; on the 503-side, assert
  `started.elapsed() >= write_timeout` too: the key alone greens today, because the route cuts at the READ
  limit and answers the same 503 at 200 ms.
- A `NotAuthorized` refusal has two answers and the brief's key can be the wrong one: a refusal of the
  SIGNED-IN user's `As` call is 403 `session-not-administrator` and ends the session; a refusal of a
  connect with NO principal (the instances record, the state) is 502 `client-not-authorized` and ends
  nothing (`crates/bff/README.md` line on `session-not-administrator`, `fe/briefs/api-contract.md`,
  ARCHITECTURE's "As built" of slice 5, and `tests/instances.rs::a_refusal_by_the_manager_is_not_authorized`).
  Pin the documented key, and report the brief's key as a divergence for the coordinator to settle.
- A restart-candidate rule ("the record's sections that the manager LISTS") is pinned through the fake's
  list cell (`set_instances(&["default"])`) with a section naming a `ghost` that is `Active`: the red is
  `restart: ["ghost"], running: true` against `[], false`, and the extra assert
  `!calls.iter().any(|c| c == "status ghost")` names the call the route must not make. Pin no exact call
  list: the fix adds a `list`/`instances` call the test must not pre-judge.

## Retargeting tests when a ruling deletes a branch (slice 15 rework, 2026-11-21)

- A ruling that removes a code branch retargets CALL-LOG pins, not only answer pins: a pin of a call the
  route stops making (`actor_of_exact("status default")` in a principal test) reds as
  `no call "status default"`, which reads as a broken fake. Move that test's input to the case that still
  earns the call (write the key that changes the effective set), and say in its doc comment why the
  fixture is that one.
- A `running`/`restart` rule stated as "the effective set changed" needs BOTH sides of the "no change"
  case, because a rule like "an `on`/`off` under a `*` of `inherit` is always a change" passes one of
  them: a key that leaves the profile effective on neither side (`gamma = off` under `* = inherit`, the
  profile in the disabled file) AND one that leaves it effective on BOTH sides (`beta = on` under
  `* = inherit`, the profile enabled). One test each, each with `assert_no_call("status", ..)`.
- A ruled-out fallback is pinned as an ABSENCE over a fixture where the fallback CHANGES THE ANSWER: for
  "no `assignments.ini` => `restart: []`, no state asked", make the listed instance `Active`, so today's
  `list_instances()` fallback answers `restart: ["default"], running: true` and the red names the answer.
  With the instance idle the fallback answers `[], false` and the test greens vacuously; add the
  no-`status`/no-`list` loop so the pin is visible even when the shapes coincide.
- A test retargeted to exercise a branch it only reached through the ruled-out path is GREEN on day one
  (the retarget keeps it meaningful, it is not a red): report it by name as a green guard.
- `rustfmt --edition 2021 <file>` adds no noise when the file's `rustfmt --check` (or `cargo fmt -p <crate>
--check`) printed only YOUR hunks beforehand: the changes rustfmt applies are exactly the hunks
  `--check` prints. Do not judge the blast radius from `git diff --stat` of a file the coder already
  modified, that counts the coder's uncommitted lines as yours; check HEAD's copy of the file is
  rustfmt-clean (`git show HEAD:<file> > /tmp/...; rustfmt --check`) to be sure.

## Telling a harness red from a code red (brief 14, fe/, 2026-11-21)

Over a slice whose code is uncommitted, every red is a claim: read the brief and the component before
you keep it. 34 reds became 6 real ones; the rest were the test's own assumptions:

- A strings value with `\n` is NEVER one text: `Field.tsx`'s `Hint` renders one `<p>` per line and
  `getByText` normalises whitespace, so `getAllByText(en('a-b-hint'))` matches nothing. Pin the hint
  block (`.field-hint` by id, its children count = the lines) and each line with `queryByText(line)`.
  A hidden hint stays in the DOM, so pin "closed" as `hidden` + `aria-expanded`, never as absence.
- A row's accessible name is the whole label ("CUDA0 NVIDIA GeForce RTX 5080"), not its id: a name
  list that expects the id alone reds on correct code. Pin the full label, it is also GD's content.
- App chrome is OUTSIDE the page's `<main>` (the session bar's Sign out), so `within(main)` finds
  none: scope such checks to `document.body`, like App.test.tsx does.
- A button that names itself (a nav toggle, a Move button) needs a text-content fallback in a
  `nameOf()` helper, and WCAG 2.5.3 needs a `visibleText()` that drops `.visually-hidden` children:
  jsdom applies no CSS, so `textContent` counts them and "Move up" reads as "Move up CUDA0".
- A document-wide button list that must be one per card is scoped to the `ul`: the jump `<nav>` also
  holds the "On this page" toggle. A card index in a jump-list pin comes from the shared card list
  (`TITLES.indexOf(KV)`), never a literal, so a card joining the page moves nothing.
- A shared `save()` helper that waits only for the PUT races the answer: read the bar's status inside
  `waitFor` in the two tests that assert it. Do NOT add a blanket "status left unsaved" wait to the
  helper: a dirty-after-save defect then reddens every unrelated save test (12 new reds, all one bug).
- A post-save Discard restores the value the form was LAST SAVED with, not the first loaded one:
  edit, save, edit, Discard expects the saved value.
- The brief beats your reading: GD2.2 under `none` is "100% on the first checked row, unused on the
  others" even where the weight fields are hidden. Re-read the criterion's own words first.
- Ledger guards (HearthFonts families, HearthTokens sizes, Reflow's overflow-wrap and class freeze)
  take the brief's new selectors, naming the brief and the DESIGN.md line in the comment; a card
  list, a jump order and an editable-field map are collateral, retarget them from the shared table.

## A card's live region breaks every page-wide status count (brief 14 GD1.9, 2026-11-21)

A card that keeps its own `role="status"` in the DOM from its first render (a move line) makes every
"one status line once the page is loaded" helper red. The rule and the sweep:

- Pick the bar's line by containment (`s.closest('.save-bar') !== null`), never by counting the
  page's regions after the h2. Keep the h2 (or `settingsH2()`) lookup as a bare call: the settings
  section being loaded is the assertion, and dropping the `const h2 =` keeps `tsc` quiet.
- A "nothing is announced on arrival" pin has the same shape: the bar's line empty by containment,
  plus the card's region pinned present-and-empty as
  `.filter((s) => s.closest('.save-bar') === null).map(text)` -> `['']`, so the criterion keeps its
  teeth when a third region lands. `role="status"` alone is not `[aria-live]`, so the existing
  "no alert, no log, no aria-live" scan stays green beside it.
- A move announcement's position is the NEW index + 1 (`t(key, id, String(to + 1), ..)`): in a
  chained test, derive each expectation from the order the PREVIOUS move left, not from the
  fixture's. A second move written from the original order is a test-side red (2 of 3, Move down,
  the string says 3).
- The helper is NOT only in `test/savealert.tsx` (`profileStatus`, `serverStatus`, `oneStatus`):
  every self-contained test file repeats its own `status()`. Sweep the pattern, do not trust one
  file: `grep -rn "before(h2, s)" fe/src` found 15 model-profile-page copies (ModelProfileSave1-12,
  14, 16, ModelProfileOverMemory) beside savealert.tsx. The Server-page copies (ServerSave, Boot,
  RunControls, Boot, Timeout, InstanceTags) have no such card and stay.
- Only the files whose device read is still PENDING when the helper runs red first (the GPU card's
  "Detecting GPUs…" line is `role="status"` today): 12 of 15. Patch all 15, the rest go red the day
  the card's region lands. After the sweep, drop the `before` import in a file whose last use it was.
- A per-file fake that merges the sent values (`modelProfileSave: (body) => ...`) must answer the
  contract's `{ revision, running }`; a bare `{ revision }` is a lie the page forgives and the next
  save's status line depends on.
- jsdom's `fireEvent.click` does NOT move focus, so "the alert goes without moving focus" is pinned
  by reading `document.activeElement` BEFORE the act and comparing after: the real focus is whatever
  the last scripted act set (a Move button), and asserting the clicked control owns it reds on
  correct code.

## A share computed by `Number()` (brief 14 GD2.2 / GD2.6, 2026-11-21)

- A `test.each` of "this weight gives no share" greens VACUOUSLY when the bad value drives the sum to
  0 (`-1` beside `1`: `sum > 0` fails, so no share for a reason unrelated to the rule). Give the
  partner a positive weight (5) so today's code shows a share and the red names the share.
- A share built with `Number(weight)` accepts what the GD2.6 pattern (`^(\d+(\.\d*)?|\.\d+)$`)
  refuses: `-1`, `1e3`, `+1`, `0x10`, and a whitespace-only weight (`Number(' ')` is 0, and a
  non-`''` string passes a `weight !== ''` gate). Pin the pattern's refusals by name, and pin the
  share ABSENCE on every row (`shares(main)` -> `[null, null]` plus a fieldset-wide scan for a
  `Share: ` leaf), so a share on one row reds.
- A rule gated on "composed non-null" makes an EMPTY weight a failure beside a number. The red of
  such a test is the missing error at `findByText(INCOMPLETE)`; prove the leak it stops with a
  throwaway probe that prints the fake's sent values (today `tensor-split` went out as `",1"`, a
  hole in the vector), then restore the file with `cp` from a backup + `cmp -s` and
  `grep -c PROBE` -> 0.

## A record append and the manager's one-test e2e (slice 16, 2026-11-21)

- EVERY test in `crates/manager/src/tests/` is a phase of the single `#[test] tests::data_root_e2e`
  (`mod.rs`), so a "new test" there is a block inside a phase fn and its red reports as
  `tests::data_root_e2e`; the only invocation is `cargo test -p pai-manager --lib data_root_e2e`
  (all phases, ~2 s, so establish the green baseline first). Phases share one temp tree: append a
  block at the PHASE's TAIL, where no later block pins the record seed, writing your own fixture.
- An append-to-a-file rule ("a newline first when the text does not end with one") is invisible to a
  substring assert: the corrupted text CONTAINS the new header
  (`ServerProfile = tail[pipe-n]\nServerProfile = pipe-n\n`), so "the new section is there" greens
  vacuously. Pin the NEIGHBOUR: parse the file (`pai_core::ini::parse_sections`) and assert the last
  existing section's key is unchanged, plus `text.starts_with(old_text)` for the untouched prefix.
- The CRLF twin of such a rule is real when the file's OTHER writers keep the style they find
  (`pai_core::ini`'s `set_key`/`delete_section` detect it), even though the shipped defaults are LF:
  the case is an administrator's hand edit. In one loop over `(style, record)` fixtures the first
  case's red blocks the second, so prove the second by adding the final newline to the FIRST fixture
  for one run (LF then greens), and restore with `cp` + `cmp -s`.

## Four harness defects of one fe chip file (brief 15 C1, 2026-11-21)

Fixing the defects the coder lists in a file, each verified against the brief first:

- A list helper on `getAllByRole` is a defect at every call site that asserts ABSENCE (it throws
  before `toEqual([])` runs). Sweep the file, do not fix only the named one: `tagAlerts` (named) and
  `tagSignIns` (not named, used at the "the Sign in is gone" pin of C1.10) both go
  `queryAllByRole`. The teeth stay: each call site that expects one asserts `toHaveLength(1)` or
  indexes `[0]!`.
- A shared flush helper that awaits a real `setTimeout` under `vi.useFakeTimers` never settles: the
  test dies at the 5 s vitest timeout and the aborted render leaves `<div />`, so the NEXT tests in
  the file red "no role main" and read as a broken harness. Branch inside the helper on
  `vi.isFakeTimers()` and use `vi.advanceTimersByTimeAsync(0)` (vitest 5 exposes it); keep
  `vi.useRealTimers()` FIRST in `afterEach` so the shared flush there takes the real branch.
- A "nothing between A and B" tab-order pin built as `stops.slice(at[0] + 1, at[at.length - 1])`:
  `slice`'s end is EXCLUSIVE, so it drops B while the expectation `order.slice(1)` keeps it. The fix
  is `+ 1`, never trimming the expectation.
- "The chips stay" counted as `within(field).getAllByRole('listitem').length` counts the brief's
  SECOND labelled list (the profile's fixed chips) too. Use the file's own-list helper and map the
  chip texts: `[..]` of texts pins more than a count and needs no fixture knowledge.
- A brief-pinned focus rule can red on an id the slice DELETED: the page's sign-in switch
  (`InstancePage.tsx:415`) still focuses `${uid}-tags-save`, the brief-12 Save tags button C1 removes,
  `getElementById` answers null and focus stays on `<body>`. That is a code red, keep the test. When a
  slice deletes a control, grep the page for its id suffix (`grep -n "tags-save" fe/src/*.tsx`) and
  report the line; prove the half of a two-part test behind the first red with a scratch copy in
  `src/` (`-t SCRATCH`, then `rm` + `ls src | grep -c Scratch` -> 0).

## Pins of a sizing ruling, and a write built off a late read (brief 15 C1, 2026-11-21)

- The H1a.12 min-size ledger lists EVERY `min-height`/`min-width` and forbids `24px` on a rule whose
  last compound is `button`. A brief that rules a 24 px icon button (a chip's x) therefore cannot be
  pinned by adding `.chip button: min-height: 24px` to that frozen list: the sheet sizes such a
  control with `width`/`height`, which the ledger does not list. Add to the ledger only the `min-*`
  the ruling FORCES (a flex child's `min-width: 0`), never every size the brief names, and append
  entries in SHEET order (the tags block sits after `.save-actions`), which is not sorted order.
- "Takes a row of its own" under a wrapping flex row (`.field-control`) is `flex-basis: 100%`, the
  sheet's own pattern (`.boot-status`, `.boot-alert`): an item's width alone reads as beside the
  chips at 1280 px. Pin the descendant form when the sheet already scopes the rule
  (`.field-control .field-error`), and say in the report which selector you chose.
- A re-read whose answer must stay pending: `instances: () => (reads++ === 0 ? reply(200, x) : new
Promise<Response>(() => {}))`. Never `held()` with one shared `Response`: a body is consumed once,
  so the second read throws and the red reads as a fetch failure. Pin a "the write is built from what
  is on screen, not from the last read" rule by answering the list with a value NO chip carries
  (`instanceTags: ['other']`) and asserting the write body: the three reds name a dropped add, a
  re-sent removal and the read's value.
- The UX check has no state that opens a control by clicking, and a pill with no `aria-expanded`
  cannot be read open: add an `open: { click, then }` entry beside the state's `click`, call the
  helper in BOTH the main pass and the 200 % text pass (the width ruling is measured there), and
  return [] when the control is not visible at that width. Give the stub a `tags=few|long` key so the
  widest state (8 tags of 32 characters) resets per state, like `list=many` does.

## A CSS ruling that renames a selector (brief 15 section 10 R7, 2026-11-21)

- A rename that out-ranks a sibling (`.chip-full` at 0,1,0 losing to `.chip > span`'s 0,1,1, so the
  expanded pill showed the ellipsis it exists to escape) fans out to EVERY selector allowlist that
  names the class: the brief's "tests that change" list is the designer's view, not the sheet's.
  `grep -rn '<class>' fe/src/*.test.ts*` first: here three files, and R1.8's CLASS list is NOT one of
  them (the element keeps its class, only the rule's selector grows).
- `sets()` in `Reflow.test.ts` names a rule by `flat` (whitespace-collapsed selector, media rules
  included) and sorts both sides, so a rename is one entry, slot-free. The H1a.12 ledger is
  `sel: prop: value` in SHEET order, so a rename keeps its slot: read the diff's insertion point.
- Pin the renamed rule with `block(sel)`: its own "exactly one rule whose selector is X" assert reds as
  `expected [] to deeply equal [X]`, naming the missing selector. Pin the sibling's override as
  `TOP_SELECTORS.filter(..).toEqual(['.chips-profile .chip > .chip-full'])`: an exact list names the
  old selector in the diff, `toHaveLength(1)` names nothing.

## The UX check's hover state, and one throw losing every result (brief 15 C1 review 2)

- Playwright 1.65: `Locator.isVisible()` resolves with `{ strict: true }` (read in
  `fe/ux/node_modules/playwright-core/lib/coreBundle.js`), so a state's `hover: { on: 'li.chip' }` over
  16 chips throws AT `isVisible`, which sits OUTSIDE the helper's `try`: it escapes `check` and kills
  `main` before it prints one collected line. The two pins: `.first()` on the locator (the same chip
  the cut probe reads, `document.querySelector` answering the first), and a per-browser `catch` in
  `main` pushing `${name}: check threw: ...`, so the run ends with every line and the summary.
- `hover()` needs `{ force: true }` on a control a ruling draws a covering pill over: once the pill is
  open the pointer's target is covered, the actionability hit-target check throws, and the helper
  reads that as "the pill never appeared", a finding that is the harness's, not the page's.
- WebKit dies mid-sweep on a loaded machine (`webkit: check threw: browserContext.newPage: Target
  page, context or browser has been closed`) while chromium and firefox report normally. Prove it with
  a webkit-only scratch copy of `check.js` IN `fe/ux/` (so its relative `require`s and `stub.py`
  resolve), `BROWSERS = ['webkit']`, run twice: 0 failing solo is a flake, say so. The per-browser
  `catch` in `main` is what makes it visible at all: with a bare throw the run loses chromium's and
  firefox's lines with it.

## A whole-document save round trip (the Server form's unedited keys, 2026-11-21)

- A "every key the read returned goes back as read" loop over `doc.values` reds on the switches whose
  read `null` the form writes as `false` (`RocprofAttach`, `AttnRotDisable`: `toValues` answers a
  boolean, the contract says the two write the same). Set them `false` in the fixture, so the loop's
  only reds are the keys the form really drops; a loop over a stock `serverDoc()` is a red the coder
  cannot pass.
- The read's `Tags` is ALWAYS a list (`crates/bff/src/server.rs` `"Tags": cfg.tags`) and a save body
  carrying `Tags: null` is 400 `server-malformed` (its own test `assert_malformed(body_with("Tags",
  Value.Null), ..)`), so the preservation pin for an empty list is `Array.isArray(sent.Tags)` +
  `toEqual([])`, never `toBeNull()`; `Devices` is the opposite (a string, `null` = the key absent).
- A key-preserving pin must also assert the KEY (`Object.keys(sent) toContain 'Tags'`) before its
  value: an absent key reads as `undefined to deeply equal [...]` which hides that the save sent 19
  keys, while the key assert prints the whole sent list (`['Port', 'Hostname', …(17)]`), the defect's
  evidence.
- Do NOT pin the rule with an invented key the form does not know: the contract ignores unknown keys
  in a save body and the render drops them (`The file is rendered whole from the values`), so the fe
  sending one back is promised by nothing, and the pin forces one implementation (merge-every-read-key)
  over another (carry the two named keys). Pin the general rule as the read's key set and values.
- The Server page's own test file copies `ServerSave.test.tsx`'s `loaded` / `box` / `saveButton` /
  `save` helpers and its `stubDialog()` in `beforeEach` with the `dialogs.restore()` in `afterEach`
  (a page-wide locator that counts `role="status"` needs the same teardown); a new file keeps the
  coder's edits out of the slice, and `npm --prefix fe test -- <NewFileName>` runs it alone.
- The server-profile routes (`GET|PUT /api/profiles/server/{id}`) have NO page: no `fe/src` code
  fetches them, so a defect that names "the server profile routes" is pinnable only on the bff side.
  Report the fe's reach, and never add a route to the shared fake to fake the coverage.

## A nested `role="list"` breaks every scoped list locator (brief 17 U17b, 2026-11-21)

- A brief that puts a `ul role="list"` INSIDE an item of another list (17b.1: the chips `ul.chips`
  inside each tile's Tags `dd`) reds every `within(x).findByRole('list')` that scopes to the OUTER
  list, with `Found multiple elements with the role "list"`. The fix keeps every assertion of the
  caller: `findAllByRole('list')` + `filter((el) => el.closest('.tile') === null)` + a
  `toHaveLength(1)` (that count is what the old `findByRole` pinned), one local helper per file.
- The red list names only the helpers whose FIXTURE carries the new nested element: three files here
  (`Instances.test.tsx`, `HearthTiles.test.tsx`, `HearthTilesBoot.test.tsx`) have the same unscoped
  locator and stayed green on their tag-free fixtures. Enumerate the pattern with
  `grep -rn "findByRole('list')" fe/src`, never from the reds: a green helper today is the suite that
  breaks the day a fixture gains tags. Say so and leave them (the brief named four sites).
- `HearthTokens.test.ts`'s H1a.12 message is ONE single-quoted string: a clause carrying an
  apostrophe (`the tile's fact row`) is a SYNTAX error of the test file, and the RULES reader drops
  `/* */` so a sheet comment never pollutes a selector. Run `npx prettier --write` on the touched
  files FIRST: prettier prints the code frame of such a string before any test runs. The prose lists
  in this file avoid apostrophes: write "the fact row of a tile".
- The known fe flakes, with solo-rerun counts, so a report can name them as flakes: `09 S1a.10: at
  #/sign-in signed in, the nav reads the alias as the Overview` fails 1 in 11 solo runs
  (`expected '#/sign-in' to be '#/'` at `SignInOnly.test.tsx:722`: the alias `replaceState` assert
  races the effect), `2d.63` (ModelProfilePage) and `13 N.3` (NodeNav) 0 in 3, and all three are
  green in a full green suite (2609 tests). One green solo after one red solo is a flake: report the
  runs, do not touch production code for it.
