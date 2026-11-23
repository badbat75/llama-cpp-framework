---
name: coder-playbook
description: How the coder implements and runs code safely in this project, and the pitfalls met so far. Use when changing production code.
---

Rules are in `AGENTS.md`. Format of this file: `AGENTS.md`, under "Skills".

## How to

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
  signature a slice grew) and rerun to green; every patch is one defect to report, with its line.
- Flakes: reproduce solo, state it, move on. The manager's D-Bus and gpu-probe tests under load, xtask's
  script-sandbox tests (one temp dir per process), the bff's `tests/session.rs` timing tests, `fe`'s
  `ServerSave.test.tsx` navigation test `1b.36b: the edits-dropped alert stays until the page is left`
  (one of 3100 under `--maxWorkers=4`, green solo and on the rerun), the UX
  check's pre-existing states.

### Running the checks

- Capture a loop's output before judging it: `out=$(eval "$c" 2>&1); code=$?`, then print `exit=$code`
  and the warning count. Judge from `FAILED`/`panicked`/`error`.
- Enumerate a crate's test targets with `--no-fail-fast`.
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

### Source lints and renames

- Before renaming anything the GUI binds, check the `.slint` files. A source-text pin reads code without
  whole-line comments and ends a method at its first brace; the production region of a file ends at the
  first `#[cfg(test)]` line; a name a lint matches is the lint's data (`control_lint.rs`).
- Renaming a word: the ARCHITECTURE.md names table is the authority; enumerate every site by grepping the
  literal first. Apply the map to comment text only, not to code tokens, and fix pointers to a renamed
  identifier in the same slice. `.slint` comments cap at 80 columns. Prove a comment-only pass: every
  added diff line trimmed of `&*-` starts with `//`.
- A strings-file VALUE is pinned by its callers' tests (and is a region's accessible name in the web UI).
  A refusal a client decodes must be a key of `checks::KEYS` with a `text()` arm; a `checks` refusal text
  is pinned by core tests over BOTH strings files. Prose in a constant whose bytes are DATA is data.

### INI, config, the manager, packaging

- A new `[Server]` key carried by the web UI touches several places (the `ServerConfig` field,
  `from_keys`, the web key list, the const-generic array length, the strings, the bff's off-list test);
  a key moved onto `WEB_MODEL_PROFILE_KEYS` reddens the bff's exhaustive off-list test. `ServerForm` is a
  Slint struct: a `ServerConfig` field needs its mirror. One shipped engine default file lives in several
  places (`crates/core/defaults/` and the package tables), pinned a third way by the bff's server test.
- `set_key` and `remove_key` keep the section's trailing blank separator line (a trimming writer eats it);
  `parse_sections` drops key lines before the first header; a section copied from a file with no final
  newline needs one appended; `ini::detect_eol` styles a writer's appended section. Writers outside core
  use the shared helpers.
- A record (`assignments.ini`, `instances.ini`) is written through `write_served_file(..,
  &ConfigFile::Models, ..)`: mode 0644, temp file renamed over. A render written after a record change
  turns its failure into the call's answer. `assign::render` takes both pool files, so a profile in
  `models.ini.disabled` is in effect. An instance IS a section of `instances.ini`.
- Manager verbs, refusals and records: e2e phases are `pub(crate) fn` called in sequence by ONE `#[test]`
  (the GUI's `save_flow::run` likewise); create/remove verbs have no test of their own. A value check the
  manager answers needs an arm in the bff's 422 field map. A shard-count bound is name math. Judge a
  write by the render it produces (the startup count only when the render starts MORE than the one on
  disk; a profile switch changes no model profile). The model-file check is a DELTA. A probe cache keeps
  the parsed rows; a serial probe answers queued readers with the probe they waited for.
- A manager rule is pinned a second time by the refusal tables of `manager/dbus.rs` and `manager/pipe.rs`
  (the wire phase lists `(args, kind, needle)` rows). Change the rule and those rows are the last reds of
  `data_root_e2e`, one row at a time, and a row that pins the case the new rule accepts is a witness pair
  with the phase that pins the new rule: report the pair, change no test.
- Packaging: `components::of` is a PREFIX ladder (a rule for a file inside an owned folder must come
  first); `windows/package.rs`'s `bundled` map is keyed by the dist label; the engine revision is
  `components::LLAMA_CPP_REVISION` and a bump moves the packages' Depends. Pin a file that must land in
  one package by reusing the test module's helpers.

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

### The web UI (fe/): running and CSS

- Run a filtered file as `npm --prefix fe test -- <file> -t '<title>'` (cwd `fe/`, so the jsdom config is
  read). `format:check` writes nothing: fix the named files with `npx prettier --write <file>` (it lists
  every file it processes: check `git status --porcelain` after). The full suite under load reddens
  scores of files with `Test timed out in 5000ms` (vitest spawns one worker per file, 116 here): rerun
  with `npm --prefix fe test -- --maxWorkers=4`, which is green, and rerun a named file solo, before
  believing any of it.
- New CSS is a test edit: `Reflow.test.ts` (`CLASSES`, `overflow-wrap`, `box-shadow`),
  `HearthTokens.test.ts` (`min-height`/`min-width` per rule, an exact ordered list), `HearthFonts.test.ts`
  (`font-family`) are exhaustive per property; a CSS test pins the SELECTOR as a string and a rule's
  "declares nothing else" list. Append (sorted) and say the title or comment is stale rather than
  rewriting it. A test-side CSS helper must start counting braces after the opening `{` at depth 1.
  `white-space` and `text-align` have no sheet-wide list. R1.8's id list is a
  `Set` and its class list allows a card's own namespace: a new rule naming an id
  the sheet already selects, or a class of that namespace, is free; a new id
  outside it is red.
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

### The web UI (fe/): shared helpers and pins

- Shared logic lives in ONE exported function both pages import (default-box mechanics in
  `fe/src/boxed.ts`; a rule that spans a group is data on the row; the send rule of a checkbox row is
  data `onSend`/`offSend`); a helper shared by two pages takes each caller's rule as a named parameter.
  Per-field checks of a save run BEFORE the `!dirty` guard. A new 422 refusal key needs a `case` in each
  of the two tables (`REFUSALS.text` and its sibling).
- Making a read-only card editable: amend the shared list under `fe/src/test/` that spells the editable
  set and derives tab stops from it (one fix point; an editable number field is two tab stops). On the
  model profile page that list is `MODEL_PROFILE_CARDS` in `fe/src/test/modelProfileCards.ts` (`fields`
  per card, `extraStops` for a control that is no 2e field): one new `Field` there reddens ten tests
  until the tester adds it: `2d.67` (ModelProfilePage, the card button list), `2d.71` and `2d.78`
  (ModelProfileContent), `2h.130` (ModelProfileSave1, which sweeps every `[id]` control and reads its
  enabled state from `EDITABLE_BY_ID`, so an unknown id is expected DISABLED), and `2e.91` in
  ModelProfileCards1 to 5. A card of a new sub-slice that adds a field is that list's job.
- One new focusable control reddens by one position every document-wide "tab stops are exactly" test
  (a page's, or the banner's): amend the tester's lists, never production.
- A Save check keyed by a control id must gate on that control being rendered (a Layers text the row
  keeps while its kind hides the field): a mark on an id with no element is invisible and `focusMarked`
  has nothing to focus.

### The web UI (fe/): React and the harness

- A test file that renders needs its own `cleanup`.
- To print data from a scratch test (the default reporter hides `console.log` in a piped run): assert it
  against an impossible value, `expect(rows.join(' / '), 'PROOF').toBe('X')`, and read the received
  string.
- Write a comment about a `switch` group ABOVE the group's first `case` label.
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
  set).
- Reading helpers shared across pages point one way (no import cycles between readers).

### The web UI (fe/): the UX check

- Run `npm --prefix fe/ux run check` (all three browsers on the BUILT `fe/dist`, build first, against
  `fe/ux/stub.py`, about 12 minutes): it proves wrap and sideways-scroll criteria. Carry in the stub's
  documents every value a control needs to render, or that control is unmeasured (the tag vocabulary is
  the API's). Its summary prints `axe: <rule> x<count>`, not the node: add a probe in a scratch copy of
  the check to learn the node.

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
- The `edit` tool intermittently misses text `awk` shows verbatim: append with a heredoc and check
  `git diff --stat`. After a rejected edit call, `grep -n "parameter>" <file>`.

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
  written multi-line; a `return Err(format!(..))` at deep indent is written broken.
- clippy: `s == ""` is `comparison_to_empty` (use `is_empty()`); `|| other()` as a fallback is
  `redundant_closure` (pass `&other`); `iter().any(|x| *x == y)` is `manual_contains`; a `Result` whose
  error carries an axum `Response` is `result_large_err`; a pub builder over 7 args takes
  `#[allow(clippy::too_many_arguments)]`; a tuple past `type_complexity` becomes a module-level `type`; a
  `///` essay followed by a blank line before the item is a lint.

### Source lints and strings

- A word-internal rename map breaks `mod` file names, multi-item `use` lists, names interpolated in
  format strings, raw or multi-line strings and WiX component Ids (MSI identity, not prose); rewrapped
  prose defeats exact pairs.
- `binding_lint` reddens the whole `-p pai-config` run when `strings::t` keys and the language file
  differ.
- A D-Bus signature pin is zbus's own rendering (a struct has no commas); a wire change with new REQUIRED
  fields reddens a sibling test in the same file whose fixture omits them.

### The bff (Rust)

- The test builders pass a 200 ms read limit, so a route bound by a longer limit needs the test's own
  pause.
- A `Box<dyn ManagerClient + Send + Sync>` does not coerce to `&dyn ManagerClient`: pass `&*boxed`.
- Setting `PRIVATE_AI_FE_DIST` reruns the bff's build script and recompiles every target of the crate.

### The web UI (fe/)

- `npx vitest` from the repo root has no config and reds every test with `window is not defined`.
- Vitest here has no globals and no setup file: RTL's auto-cleanup never runs by itself.
- `fireEvent.click` does not move focus like a real click.
- Specificity: `button` and `.secondary` are (0,1,0), `.jump button` is (0,1,1); `input[role='switch']`
  and `input[type='checkbox']` tie, so source order decides.
- The `EN` table of `fe/src/test/bff.ts` is a second pin of `en_US.json` (`App.test.tsx` `1a strings`);
  "every value is used" scans for `t(` + a string literal, so a key that reaches `t` otherwise reds it.
  A strings key id makes a good `aria-labelledby` target.
- Queries run before an async read settles: a test that reads a control a device read fills, or
  snapshots tab stops before the awaited element lands, is the TEST's race (`indexOf = -1`, a
  `Loading...` status quoted where a page's line is wanted). A page that returns `null` while its read is
  pending is not the defect. State the shape, do not touch production.
- `vi.useFakeTimers()` breaks RTL's `findBy*`; a test that holds a request and advances the clock sees
  only what the flush after `advance` renders. A fixture handing ONE `Response` to several readers of an
  endpoint starves the second (React runs a child's effect before its parent's); a count over one
  endpoint counts every reader.
- ESLint `no-fallthrough` reddens a comment written BETWEEN grouped `case` labels of a `switch` ("Expected
  a 'break' statement before 'case'"). `querySelectorAll` over a CSS selector yields `Element`, so a loop
  that calls `.focus()` needs `querySelectorAll<HTMLElement>`.
- The react-hooks lint answers `set-state-in-effect` for a state update that answers a prop change:
  derive it during render or key the component.
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

### The UX check

- axe: `aria-required-children` fires on a plain `<div>` child of a `<ul role="list">` (webkit only);
  `target-size` (24 px) is satisfied by a small target fully clear of others, not by one partly covered,
  so a page a few px taller shifts the result; a `<button>` with no class gets a UA grey that fails
  webkit `color-contrast` (give it `.secondary` or `.primary`; the sheet styles `[aria-disabled='true']`
  for both).
- A wrapping chip list capped by `max-width: 100%` on the chip is capped by its PARENT: the `ul` of a
  flex row needs `min-width: 0`.

### Rust library traps

- `nix` features (`fs` for `OFlag`, `feature` for `uname`); `Command::get_args()` yields `&OsStr`;
  chaining onto a temporary in a `?`-terminated statement is E0716 (bind first); `&unsafe { call() }?`
  is E0716; a module fn shadowed by a same-named local is E0618; ash in loaded mode needs a `Drop` guard
  owning the instance, declared after the entry, so every path destroys it. A child process drained for
  output uses `wait_with_output()` after taking its pipes, under a bound; a cache window over a caller's
  clock is `now.saturating_sub(ran_at) < WINDOW`.
