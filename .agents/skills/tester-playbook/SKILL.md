---
name: tester-playbook
description: What the tester has learned about writing reliable tests. Use when writing or debugging tests.
---

Lessons only: the rules, commands and env vars are in `AGENTS.md` and the area guides. Each lesson here is
a trap that recurs; a slice's history does not belong here.

## Before you start

- Verify a reported red or a named test file against the checkout (`git cat-file -t <commit>`, `ls`,
  `git worktree list`). A report naming a file or commit that is not here is not this session's red:
  report the mismatch and finish the brief you were given. Never write tests for a file that is not there.
- Before the first cargo run, look where it writes: the generated `.cargo/config.toml` pins
  `build/windows/cargo-target`, so a Linux run lands in the Windows tree. Use
  `CARGO_TARGET_DIR=$PWD/build/linux/cargo-target` and say which dir a run used (a cold dir is ~8 min of
  compiles, not a hung test).
- Look up a strings key in `fe/src/strings/en_US.json` before believing a text red: `en()` throws
  `no English for <key> in the test` for a key that does not exist.
- Name a test by the brief's criteria number, carried with the brief's own prefix: a page of brief 16
  cites `16 17b.9:`, never bare `17b.9:` (that id belongs to brief 17's UX pass). A defect a review found
  in a closed slice is cited by the review's label at the END of the title (`… (17b review D2)`): the
  criteria number says which rule is pinned, the label says which defect it closes, so one rule can be
  pinned twice as it grows stricter.

## Proving a red, and what a run proves

- A Rust test that cannot compile is not a red. Before reporting a list of reds, write a throwaway test
  that exercises the new signature, run it, delete it, then write the real tests. A throwaway proves
  reachability only (it compiles, a value is constructible), never that a call site exists: call it a probe.
- A stub of a manager verb that does not exist yet: `ManagerError::Io(std::io::ErrorKind::Unsupported)`
  (`ManagerError` has no `Other`), and the phase's refusal helper must take `key: &'static str`, because
  `checks::Refusal.key` is `&'static str`: a `&str` closure argument is a lifetime red, not a test red.
  `serde_json::from_str` inside a `json!` literal needs `::<serde_json::Value>` (the macro's `Into<Value>`
  leaves `T` ambiguous).
- A stub that adds a field to a struct other crates build as a literal is a COMPILE red there. Grep
  `Struct {`, run `cargo check --workspace --all-targets --exclude pai-config` before reporting, and fill
  each literal with the value its own fixture implies. A new field on a wire struct reds every target that
  reads it: prove the field-independent rules with the one file put back at HEAD and report both lists.
- Never wait without a deadline (`recv_timeout`, a `try_wait` loop): a test must fail, not hang. Report a
  `Disconnected` (the thread panicked, e.g. a `todo!` stub) apart from a `Timeout`. A `tokio::time::sleep`
  on a current-thread runtime is real wall clock: drive timeouts with `tokio::time::pause()`.
- Split criteria into one test each and expect a split: a rule that is an ABSENCE is green on day one (a
  guard), the rule that adds an action is the red. Report each test's day-one state by name. A test the
  coder must EDIT to go green pins more than one that merely passes.
- When the coder's work already contains the rule, the test you expect red is green: say so, and add an
  assertion that fails for a reason you cannot fix.
- A red is a contract: where a name's shape is free (a generated temp name, an ephemeral id) assert an
  observable (a suffix, a pattern), not the literal the coder is free to pick.
- An ORDER rule ("the record is written last") has no teeth from accepted calls: every refusal happens
  before the write. Pin it with a call that fails on the LAST step (make the render's path a directory so
  the rename fails) and assert the whole-tree snapshot is unchanged.
- A bookkeeping pin (a log or call count over calls made by earlier pins) never proves its own red: report
  it green today, deferred to the coder's green run. A count over a seam the builder also reads takes
  `let base = ...` after the builder and asserts deltas.
- A byte bound on a pipe payload pins twice: the arithmetic and the wire crossing (the red lands in the
  client's own frame check, which is the bound's evidence).
- "Nothing runs here" is not proof a later guard passes: a source lint over a `cfg(windows)` file is green
  on inspection, never "test passed". A file-name rule for one OS is a `cfg` split, not a table row.
- An attribute glued to the end of a doc comment makes the test unregistered (it never runs, and clippy
  says `dead_code`). Prove a test exists with `-- --list | grep -c "^<name>:"`, never by running it.
- Tests use a temp db; a test that touches the live `bench.db` path hangs the crate's suites behind the
  machine-wide lock.

## Mutants and scratch proofs

- A mutant script: copy the file to its own backup, apply ONE exact snippet (`src.count(old) == 1` or
  abort), run one filtered test, print the mutated line (indexed from the OLD snippet), the panic lines
  and the failing names, restore with plain `cp` and prove it with `cmp -s`. NEVER `cp -p`: the old mtime
  makes the next run reuse the MUTANT binary. A script that dies between write and restore leaves the file
  mutated: restore from the backup and `grep -n` before running anything else.
- Judge red from the OUTPUT (`FAILED`, `panicked at`), never the exit code: `cmd | tail` reports tail's 0
  and a survivor reads green.
- A gate copied verbatim into every guarded verb cannot be mutated in place (the snippet matches N times):
  include the verb's own first distinguishing line.
- Run nothing else with cargo on the crate while a prove loop rewrites the real source.
- A red that cannot be shown in the real tree (test-side reds behind an earlier red, pins behind a
  compile-blocked phase, pins in a long phase) is proven in a scratch copy: `rsync -a --exclude build
  --exclude .git --exclude dist --exclude node_modules <repo> /tmp/<x>` plus a symlink of `fe/node_modules`,
  or a `git worktree add --detach` of HEAD for an inherited red. Report the line as "seen in a scratch
  copy with X flipped", never as a green of the current tree, and name the scratch panic lines as such.
  Ways to isolate one phase or pin: a temporary `#[test]` calling just that phase; `#[cfg(any())]` over the
  blocked phases; a gate on an env var (`is_none()` wrapping the red pins' braces; an early `return` skips
  the whole phase); `// <SLICE>-PIN-<n> BEGIN/END` markers a script deletes. Restore byte for byte and
  prove it (`grep -c`, `cmp`, `git diff --stat`). A delete-script's pin blocks must be self-contained
  (own snapshot reads, cleanup tolerant of files a deleted pin created).
- Reported test-side reds are reproduced SOLO in the tree as it stands first; most of a coder's "red left
  in the tests" claims are stale test code and are the tester's to fix.
- Never run `cargo fmt -p <crate>` (without `--check`) when the tree carries other roles' uncommitted
  lines: it rewrites the whole crate. List the hunks with `--check | grep '^Diff in'`, hand-apply your own
  and prove with `--check` answering 0 and a `git status --porcelain` before/after diff. Where only Rust
  lines are yours, a `cargo fmt -p <crate>` is acceptable if a `git diff --stat` taken before and after
  moves the counts of YOUR files alone: keep both listings and quote them.

## Cargo runs

- A run with no `Compiling <crate>` line judged a STALE binary: an assertion that passes in a run and fails
  in a probe of the same phase is the stale binary, never a flake. Force the rebuild (add a `#[test]`, run,
  remove it, prove with `cmp` + `grep -c`).
- A GREEN rerun with no `Compiling` is a replay of the coder's fresh build: prove it by each test binary's
  mtime (the `Running` lines) post-dating the newest `*.rs` of the crate, and report that beside the totals.
  Compare each binary to the newest of its OWN inputs (its own test file + the crate's non-test `*.rs` +
  `Cargo.toml`): once the tester edits one test file, the crate-wide newest `.rs` makes every other binary
  look stale, and the check answers 0 only per target.
- A target that fails to compile stops the run; a target's test FAILURE stops it too. Use `--no-fail-fast`
  for the slice's map (every `test result:` line; `awk '/^---- /{name=$2} /panicked at/{print name" | "$0}'`
  for one line per red) and run the untouched binaries in a second invocation to show them green.
- `cargo test -p <crate> <word>` filters test names, never target names: reach a file with `--test <name>`
  and account for every `N filtered out` line. `-- <filter>` after `--test`/`--lib` is the only way to
  filter inside a target. A `#[test] #[serial]` in a lib is not covered by `--test <name>`: say which
  invocation covers it. `--lib` of a binary crate runs unit tests, `--bins` the built binary.
- Write a long cargo log to a file (never pipe into `grep`: SIGPIPE), grep `test result|Running|^error`,
  then `sed -n` each block.
- A cargo test binary does not inherit a variable set on the command line for a harness process; gate a
  behaviour on it in the test body (a unit test body does see it).
- A copy of the tree handed over as "green there" is read with `diff -rq crates <copy>`: it usually carries
  edits beyond the quoted diffs. Judge every hunk and name the incomplete list.
- The manager's D-Bus unit tests and the GPU-probe flooding test are load-sensitive (slow under a loaded
  machine, bound reds at 0..1800 ms): run them alone (`cargo test -p pai-manager --lib -- --test-threads=1
  data_root_e2e`, or the full path with `--exact`), then judge with solos plus the full-run map. Never edit
  the probe. `--test '*'` names no target for `pai-manager`.
- The bff embeds the web UI (`PRIVATE_AI_FE_DIST`, falling back to `crates/bff/fe-fixture`); setting the
  variable reruns the build script and recompiles every target of the crate: set it only to escape a stale
  `fe/dist`. Setting `PAI_BFF_FE_DIR` in the shell does nothing.
- Pre-existing failures of states or tests the slice did not touch (UX check, flaky full-suite files) are
  stated as pre-existing, with a proof the file is untouched (`git diff <path>`); do not chase them.

## The e2e phases (data root, the configurator)

- `pai-core`, `pai-app`, `pai-cli` and `pai-manager` each have one `#[test] fn data_root_e2e()` in
  `src/tests/mod.rs` calling the phases in sequence; the GUI's e2e is the one `#[test]` of
  `crates/config/src/tests/ui_bindings.rs`. Register a new phase in the same edit that writes it: an
  uncalled phase is invisible, its solo run looks green.
- A refusal aborts at its own phase, so every later phase is "unproven, not passed": prove each phase
  alone and report the blocked ones as blocked.
- The same test masks its own later reds: `pai-manager`'s service phases AND its D-Bus/pipe client
  phases are calls of ONE `data_root_e2e`, so a full run prints only the first panic. Prove each later
  red by cutting the earlier block out of the real source (a python line-range delete that asserts its
  two anchor lines first), run `--lib data_root_e2e` (~7 s incremental), restore with `cp` + `cmp -s`.
- A helper that trims a line's end (`str::trim_end`) erases a trailing non-breaking space: pin a
  Unicode-whitespace value with the raw line, never with the file's `keys_of`-style helper.
- A unit test over a temp dir is not a data-root e2e: it passes with no seam at all. A phase that drives a
  real save goes through `config_seam_e2e` (redirect, save through the seam, reload, assert).
- A phase shares ONE tree. By a late block earlier blocks have added and removed files: pin names (present
  and absent), never counts; snapshot "the refusal wrote nothing" AFTER the last block allowed to write;
  assert "nothing left in the user tree" at the END of a phase. A whole-file pin over a file the phase
  grows is `fixture + appended`.
- A phase that rewrites a file a LATER phase pins snapshots it at the phase's start and writes it back at
  the end (read the snapshot BEFORE the write loop). Prove the restore's teeth with a mutant that swaps the
  write for `let _ = found;`.
- A fixture a production path resolves as RELATIVE goes where that path says, computed by the SAME helper
  (`launch::instance_current_dir(..)`), never `dir.path().join(..)`.
- A fixture writer for a binary format must keep at least one fixture where two header counts differ (a
  swapped order is invisible when they are equal); mirror the reader's own test writer. A fixture over a
  file the test writes lists EVERY row the writer writes.
- A refusals-loop entry whose `why` says "an id taken by an earlier call" must carry an id the phase really
  created; a hand `* = on` in `assignments.ini` serves every profile, so keep it for the refusal pins and
  rewrite the file with one explicit key before an accepted delete.
- Two of the slice's own pins of ONE rule that disagree: the design's wording decides (a design naming
  `ini::text::section_text` means its bytes verbatim). Retarget the loose pin.
- A fixture text that one phase accepts as the write and a neighbour section carries as its taken value
  collides with a case-insensitive uniqueness rule: rename the NEIGHBOUR's fixture text (keep the
  canonical accepted text), and sweep the literal across EVERY file carrying that fixture with a count
  assert (`s.count(x)` per file), then hand-fix the case-variant texts of the taken pins, the reader
  expectations and the self-write. A refusal-block's record count is the number of refusals its loops
  make (bad + markers + taken): a later pin added to the block leaves the literal short.
- A wire shape that grows (a D-Bus row signature) reds every pin of it AND every tuple literal that
  reads it, in phases of older slices: sweep `grep -n <signature>` and the tuple types crate-wide, and
  update the stale signature in the prose of the test's own header comment.
- A phase that rewrites a file a LATER phase reads as its `base` reds that later phase when its payload
  equals what the earlier phase left (a compare-and-swap then accepts a stale content). Snapshot both
  files at the phase's start and write them back at its end, as the file's sibling phases do.
- A mutant of a validation gate is shown only on a tree green in that phase first: with an unrelated
  fixture red upstream the panic you print is the fixture, not the gate.
- A rule about the KEYS of a request cannot be pinned through client tables whose variants carry fixed
  keys: use the raw-wire tables (`manager/pipe.rs`, `manager/dbus.rs`).
- A manager verb gets one phase pinned over all three transports (direct, pipe, D-Bus); a bound a client
  checks (frame size) reds in the client, not the manager.

## The bff (Rust)

- Each `crates/bff/tests/*.rs` has its OWN fake manager: a new verb of `ManagerClient` goes in that fake's
  `impl` block (the red is then compile red, one `E0407` per verb: count and list them), and the cell a
  second instance needs (`Live::instances`) exists in one file only. A route whose id the fake does not
  list answers `no_instance`: that is the gate, not a broken harness.
- Read the trait before promising an `E0407`: a verb an earlier slice added WITH a default body (the
  older-protocol transports, e.g. `set_instance_tags`, `instances`) is overridable, so the fake compiles and
  the red is the router's 404 on the missing route. Count `E0407` only for a verb the trait does not declare.
- The fan-out of a key added to a bff list row: every whole-row `json!` literal is one that carries a key
  unique to the row (`grep -c '"removable"' crates/bff/tests/*.rs` finds them all, in every file that pins
  the list), and the file's `row()`-style fake helper is where the new field gets its default. List them by
  file and count, patch none (the rule under "Renames, wire and text").
- A verb added to a path an existing suite pins as "not served" reds that suite's 405 loop. The design
  decides who owns the loop: drop the served verb, keep the rest (day-one guards), point the doc comment
  at the file that pins the new verb.
- Timeout pins: a `Live` cell the test sets AFTER the router is built (`call_pause_ms`, `connect_pause_ms`,
  `call_lost`, `connect_fails`) makes the fake log the call and only then sleep or fail, so a 504 pin can
  assert the manager received it, and a failed connect stays 503 with an empty log (the guard that 504 is
  only for a call that crossed). Pin elapsed BOTH ways (`>= sleep && < limit`); the lower bound proves the
  success really ran past the read limit. A multi-call write sets the pause per call.
- `Unavailable` is 503 only where nothing was handed over (failed connect, a read) and 504 on a write that
  crossed: before renaming a test ending "is_unavailable", read which of the two it drives. Reads keep
  503.
- Gating a route (session or permission): the source's route-list doc comments go false with the gate; list
  them for the coder (a tester edits no `src`).
- A list criterion that keeps an item with defaulted fields and an expected-rows array that omits it are
  one fixture, opposite rows.

## Fakes, logs and source lints

- A source lint reads production code with whole-line comments dropped; a name a lint matches is the lint's
  data. A lint over the files of a package split pins only the OWNERSHIP rule it names.
- A pin on a decision is a pure function over its inputs; test the function, not a run that happens to
  reach it. A reader over a fixture root is pinned with the fixture's exact names (the case rules compare
  exactly, and `write_file` refuses a `[*]` section).
- A shared fake that answers `?? DEFAULT` hides a missing answer; give it prefix filters and assert the
  asked routes by exact list (`list, status <id>`), never by one substring.

## Renames, wire and text

- A rename touches the ARCHITECTURE.md names table first: enumerate every site by grepping the literal;
  a word-internal map breaks identifiers and `mod` file names. A stale pointer in a comment is a defect.
- An additive wire key reds every whole-object `json!` literal in the file: list them, do not patch one.
- A ruling that moves a key onto a list reddens two pins by construction (an off-list 400 loop and the
  exclusion test whose NAME carries the count): rename and re-count; grep `exclude_the_`.
- A strings-file value is pinned by its callers' tests; `App.test.tsx`'s `1a strings` compares the test
  mirror `EN` (`fe/src/test/bff.ts`) with `en_US.json` key by key. Adding keys to `EN` is the coder's
  signal: that one red is the acceptable one. Never add keys to `en_US.json` and never weaken the guard.

## The web UI (fe/)

### Running and probing

- vitest runs from `fe/` only (`npm --prefix fe test -- <file-part> [-t '<title>']`): `npx vitest` from
  the repo root resolves another config and the node environment (`window is not defined`). The type
  check that sees test files is `npm --prefix fe run typecheck`. Prettier is part of the test edit: run
  `format:check` on your files.
- In this environment `npm --prefix fe test` exits 1 with an `EISDIR ... getPackageScopeConfig` and a
  `jsdom` message while every file passed: judge from the `Test Files` / `Tests` lines.
- `npx --prefix fe prettier --check src/x.test.tsx` matches NO file and still ends "All matched files use
  Prettier code style!": judge from its `[error] No files matching` line, and run prettier with the cwd in
  `fe/` (or paths from the repo root). `prettier --write` on a file the coder left clean moves only your
  own lines: keep a `cp` of the file and `diff` it after.
- vitest swallows `console.log`: probe a harness fact with a deliberately failing assert carrying the
  value, in a `.test.tsx` you delete afterwards (prove with `ls | grep -c`).
- Group reds by cause: strip ANSI and `grep -oE "(TestingLibraryElementError|AssertionError|TypeError): .{0,60}"
  | sort | uniq -c`. A `TypeError: unexpected <route>` means the fake bff was asked a route its defaults do
  not cover: a fixture gap, not the slice's red.
- A flaky fe red is judged solo x3 before anything is touched; a full-suite red that is green as a file
  is stated, not chased.

### The fake bff and fixtures

- The fe fake (`src/test/bff.ts`) is shared: after changing it run the whole suite ONCE. A page-open helper
  that calls `fakeBff()` itself stubs the fetch with its own fake: a second fake means a second set of
  answers. A fixture's `startAt(n)` gates every route, not the one it means. `startAt` is
  `history.replaceState`: the app keeps one entry.
- A fixture constant looked up by accessible name must not collide with another control's name; a test
  that renders in a loop needs `cleanup()` per iteration (RTL cleans only between tests); a file whose test
  clicks a jump button needs `Element.prototype.scrollIntoView = () => {}` in `beforeEach` (without it the
  unhandled error reds three unrelated later tests).
- A route read by several pages: a count over one endpoint counts every reader; React runs a child's
  effect before its parent's, so a fixture handing ONE `Response` to every call starves the second reader.
- Retargeting an old pin onto another field or a moved read: amend the old test to the new rule and say in
  a comment which slice retired the old one.
- A 422 whose `field` names a control the form checks itself (a port) is UNREACHABLE with a value that
  check refuses: the check sends nothing, so the reply never arrives. Pin it with a value the check
  accepts, and the args exactly as the bff sends them.
- Keys added to the shared `EN` map red `App.test.tsx` `1a strings` for every one not yet in
  `src/strings/en_US.json`: that IS the strings-file pin of a text criterion. Report it as an intended
  red of the slice, not as collateral damage from touching the fake.
- A page check that does not exist reds as "Unable to find the bar's alert text", never as a request
  count: the check is absent so the save went out. That is the right red when a sibling test of the
  same slice proves, on the same fixture, that the PUT does go out.

### Async settling (the trap that recurs)

- Never query before the async read settles. Every shell part that arrives with the session (nav, instance
  links, banner) reds a synchronous `getByRole('navigation')` helper: add an async finder, settle signed-out
  states by the sign-in `h1`, and settle on the page's `h1` before asserting loading text.
- Tab-order snapshots: take EVERY async element (`await findByRole(..)`) first, THEN `tabbable(main)`;
  settle the nav's instance links first too. A snapshot taken earlier prints `indexOf = -1` for a real tab
  stop (about one run in two). Adding a focusable control reddens every document-wide tab-order list by
  one position: grep the suite and amend the tester's lists. Pin "nothing between A and B" with indices
  taken from the settled list.
- Read a state-dependent list (devices, a select that shows a placeholder while pending) only after the
  page's `landed()` helper; the deliberate pending case is the only exception. The probe also gates
  ACTIONS that read it (Add rule seeds the first probed GPU): clicking one before `landed()` seeds the
  pending-case value and the test reds on a correct component.
- `vi.useFakeTimers()` breaks RTL's `waitFor`/`findBy*` (a render that needs a real macrotask never
  happens). List `toFake` as `setTimeout/clearTimeout/setInterval/clearInterval/Date`, flush with
  `act(() => advanceTimersByTimeAsync(0))` repeated, put the answer's own timeout inside the fake clock,
  and pin delay boundaries tight (`advance(299)` absent, `advance(1)` present). Poll-driven rows never
  advance under real timers.
- `role="status"` has no name from content: find it with `getAllByRole('status').find(el =>
  el.textContent?.trim() === text)`, and pin an absence by the same filter, never `queryByRole(.., {name})`.
  A page-level live region breaks every page-wide status count: pin the count the brief rules.
- A `loaded()`-style helper that accepts any shape of a read's answer (a table, or the empty text) must
  poll with `waitFor` over a predicate: one `queryByRole` snapshot races the reply and reds with a DOM
  dump showing the element it sought.

### jsdom and CSS pins

- jsdom applies no stylesheet: "in the DOM and not shown" is two pins (the element's link to
  `.visually-hidden`, accepting both nestings, and the sheet's CSS text). `closest` answers a complex
  selector. Pin a note's order as `note.nextElementSibling === field`; pin "the icon is the only element"
  as `children.length === 1`.
- Absence pins: `queryAllByRole(..)` with `toHaveLength(0)` (greens both no-list and empty-list shapes);
  never `.value` on a `queryByRole(..)!`; index options from `options.length - 1`, never by a literal.
  An absence pin of a LATER sub-slice's key is structural (`nextElementSibling` is null), not `en(key)`.
  `queryByRole('group')` is never unique on a page with a form (every field's `<fieldset>` is a group):
  a disclosure's open and gone pins carry the group's accessible name.
- A focus-at-open pin is read BEFORE the test presses any other control of the opened thing (a hint
  toggle takes the focus), and the control it names is declared above it.
- A text that must be there WHEN the focus lands (an error before the screen reader reads the field):
  attach a `focus` listener that records, at that moment, the `aria-describedby` names and whether the
  text is in the region, then press. Asserting the text and the focus afterwards is GREEN when both
  rendered in the same tick with the focus first, which is the defect.
- An `aria-disabled` control is not a shield: its handler runs on click. Pin "inactive" by pressing it
  and asserting the open thing is still THERE (the same element), its status line unchanged, the focus
  not moved to the opener, and the request count still one. Then resolve the request that was already
  in flight and pin the reply's outcome: that is the case the review found.
- A terminal state (deleted, not found) is pinned as NOT carrying: after it, change the address to
  another id and pin the other page (its h1, its own rows, the read of its route) and the absence of the
  first id's paragraph. A keyed remount passes this on day one: it is a guard, not a red.
- A refusal text must outlive the re-read that follows it (16 17b.9): the re-read drops the control the
  refusal was about, so the pin waits for the re-read to land (the row names the new state) and only
  then looks for the text, in the element the criteria name (`p[role="alert"][tabindex="-1"]`), focused.
- CSS-reading helpers: the block walker starts at depth 1 after the opening `{`; a rule inside `@media`
  also matches a flat regex; match descendant rules by their WHOLE selector (whitespace normalised); an
  at-rule's body is a list of RULES. A pin of a declaration's exact text must tolerate inner newlines.
- A CSS pin naming a class the brief leaves to the coder (its section 6: "named by the class the coder
  gives them") is an assumption you are writing into the contract: the reader's `found 0` is the red, and
  the name you chose must be in the report, because the coder has to use exactly that class.
- Frozen allowlists compared with `toEqual` move with the brief: a new class reds `Reflow.test.ts` R1.8
  `CLASSES`; a new `overflow-wrap` reds R1.1; a ruled size reds `HearthTokens.test.ts` H1a.12 (ordered
  list, one rule's entries in the test loop's order; per-card sets are sorted on both sides); a
  declaration added to a pinned rule reds that rule's "declares nothing else" list and its title. Amend
  them in the same edit as the layout test, name them in the report, and leave none "for the coder". Read
  an array's own tail to see whether it is sorted.
- Prove the teeth of a CSS pin in a scratch css + scratch test pair (`cp` both, one string swap, `-t`).
- A cascade or selector-ordering pin: a probe selector whose ancestor ORDER is inverted never matches;
  specificity ties go to source order.
- A test that cannot fail in jsdom (real layout, focus movement, `fireEvent.click` moving no focus,
  clipping) goes to the UX check or is stated as unproven.

### Nav and order literals

- A brief's "Older tests that change" section is the fan-out list: work every row of it before reporting.
  Where a row retargets an old pin (a refusal that gains a control), delete that row from the old table
  and cite the new test file in a comment; where the prior sub-slice pinned an absence structurally
  (`nextElementSibling` is null), retarget the same selector onto the thing this slice adds.
- A card that gains always-visible controls: amend `MODEL_PROFILE_CARDS` in `fe/src/test/modelProfileCards.ts`
  (`extraStops`: a strings key is a button, `{about: key}` its Field's About toggle, `{input: key}` a text
  input, no button). `ModelProfileSave1.test.tsx` 2h.130 keeps a LOCAL copy of the editable-field map
  (`EDITABLE_BY_ID`, "a test file imports no other test file") so a new editable control lands in both.
- A new nav entry lands in about a dozen order lists. Sweep every standalone `en('nav-...'),` line (and the
  `EN[...]` variants), then check tails (`grep -L` for the new key; `grep -rn "slice(-[0-9]"` for windowed
  lists) and `NodePage`'s list. Also rewrite what the sweep cannot: a `parseRoute` pin of `not-found` for
  the new address, a group's href list, an `aria-current` case, a "gains no such key" pin.
- An alias rule has two paths (address equals the state hash: no navigation; address moves: `setHash`):
  pin both.
- A brief that makes a read-only field editable fans out through the shared cards helper and reddens every
  fixture that pinned it read-only: amend them, and retarget "nothing to focus" refusals.
- Pins of a save rule stated as a comparison (render of the rows differs from the loaded one) cannot be
  satisfied by a `dirty` flag; keep the brief's case (type, type back, expect 0 PUTs).

### The UX check (`fe/ux`)

- `npm --prefix fe run build` then `npm --prefix fe/ux run check` takes about 12 minutes. It prints one
  line per browser x width x theme (x text mode): one unmet `expect` is 18 lines, so report unique lines
  (`sed 's/^chromium //..' | sort | uniq -c`), not the summary's count.
- A new state whose reads the stub holds needs `waitUntil: domcontentloaded` and every key it can set in
  the reset query of `load()`. A state's `expect` markers are read BEFORE `click`/`open`/`hover`; reading
  the nav at 320 px needs the `menu: true` field. What the stub (`stub.py`) serves decides what the pass
  sees: a control that renders only for certain values is unmeasured until the stub carries them.
- Playwright `waitFor()` counts a `.visually-hidden` element as visible; `hasText` is a normalised
  substring of `textContent`. One marker cannot pin a `count` together with a `text` only one element
  carries: split them. A `hover: { on, then }` acts on the FIRST match of `on`.
- Name a disclosure the state opens by its production icon: `button:has(svg path[d="M5 12h14"])` (the plus;
  `M12 5v14` is shared with arrow-down, the table sort). `main form` is unique (the sign-in form sits in a
  dialog) and `main [role="group"]` outside the model pages. A marker text interpolated raw from
  `S['key']` names the text "undefined" and fails the state today, pinning it the moment the key lands;
  a `?.`-guarded one degrades to a selector-only marker that passes loosely today. Pick per state.
- Verify the LENGTH of a repeated-character fixture literal with `python3`, not by eye.

## Behaviour that cannot run here

- Windows, xtask packaging and CI behaviour are pinned as text and type checks only; state in the report
  what Linux can and cannot prove, and that a `cfg(windows)` file was inspected, not run. A source lint
  over such a file greens on inspection.
