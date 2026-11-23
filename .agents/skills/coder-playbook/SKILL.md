---
name: coder-playbook
description: What the coder has learned about implementing and running code safely. Use when changing production code.
---

Lessons only: the rules are in `AGENTS.md`.

- `pkill -f PATTERN` also matches the shell running it, and killed that shell. Use the task's own stop
  control, or a pattern that cannot match itself.
- When a test fails, check whether the test or the code is wrong before changing either.
- A screenshot that comes out blank proves nothing about a GUI. Check that it really shows the window.

## The shared tree, and proving anything about it

- The tree moves mid-task, in ways that erase production edits and whole files. Snapshot outside the repo
  before a sweeping pass (`cp -a crates xtask <snap>`), and `ls` a snapshot you plan to restore from: a
  `cp -r ... && echo ok` piped through `tail` printed no `ok` and created nothing. Nothing else is a
  restore source: `git stash` and `git checkout`/`git restore` are off limits, and a stash measures the
  tree without your slice, so the only proof is a rerun on the current tree.
- A mid-task history rewrite deletes the base commit (`git diff <sha>` answered "Not a valid object
  name") and moves files. Re-survey before editing (`git log --oneline -3`, `git status`).
- The reverse: a test file the coordinator says "landed" is sometimes not in the tree at all (twice in
  one task: `fe/src/ServerFieldOrder.test.tsx`). `ls` for the name before starting; a slice with no test
  file has nothing to turn green, so report that and write no test.
- Verify your whole set with `git diff <paths>` right before the test run, not only the file you are in.
- A failure that quotes an identifier your own edit removed is the tree moving, not a logic bug: re-read
  the whole file, apply what is left in one scripted pass (python, exact old text, print the result),
  then rerun the suite.
- Baseline red list, no stash: `git worktree add --detach /tmp/<x> HEAD`, copy in the tester's untracked
  files, symlink `fe/node_modules` from the repo, run there, `git worktree remove --force`. It separates
  a red you caused from a red you inherited.
- Prove a hypothesis where no one's tests live: copy the tree (`rsync -a --exclude build --exclude .git
  --exclude dist --exclude node_modules`, ~23 MB), set `CARGO_TARGET_DIR` to a fresh dir, patch
  production or the tests there, and rerun: the failures come out one at a time and the green copy shows
  the complete list. Never patch the repo's tests. The copy can share the repo's
  `build/linux/cargo-target`: registry crates are keyed by version, not by workspace path, so the deps
  are reused and only the workspace crates rebuild (a few minutes, no extra disk).
- The sandbox clock jumps by hours mid-task: a file you edited an hour ago reports an mtime from BEFORE
  your edit, and files move and grow under you. Content, not mtime, is the state: `md5sum` the pair,
  `grep -n` the exact text, and re-read a region before quoting it. A python patch script must anchor on
  exact text (`s.index(anchor)`, `assert count == 1`), never on a line number read a moment ago.
- The `edit` tool here intermittently fails to find text `awk` shows verbatim: append with a shell heredoc
  and check `git diff --stat`. And a call the harness rejects for a bad parameter key can still write its
  arguments into the file: after any rejected edit, `grep -n "parameter>" <file>`.

## Running the checks

- A loop that reports `eval "$c" 2>&1 | tail -6; echo EXIT=$?` prints tail's exit (0) for every command:
  a loop of green lines over red commands. Capture first (`out=$(eval "$c" 2>&1); code=$?`), then print
  `exit=$code` and the warning count of `$out`.
- A filtered web UI run is `npm --prefix fe test -- <file> -t '<test name>'`: npm runs the script with `fe/`
  as cwd, and `fe/vite.config.ts`'s `test.environment: 'jsdom'` is read from there. `npx --prefix fe vitest
  run <file>` keeps the repo root as cwd, finds no config, and reddens EVERY test with `window is not defined`.
- A new CSS class is a test edit: `Reflow.test.ts`'s `CLASSES` is an exhaustive allowlist of every class the
  sheet selects, so the suite is red until the name joins the string (append it, sorted; its comment above
  and the test title count the classes of each brief, so say the wording is stale, do not rewrite the title).
  `HearthTokens.test.ts` (min-height/min-width per rule), `HearthFonts.test.ts` (font-family), and
  `Reflow.test.ts` (overflow-wrap, box-shadow) are the same exhaustive shape per property: a new rule that
  sets one of those properties needs its line there.
- `npm --prefix fe/ux run check` proves the wrap and sideways-scroll criteria on the BUILT `fe/dist` (build
  first) with `fe/ux/stub.py`; the stub's profile carries `device` + `tensor-split` so the GPU rows are on
  the page it measures at 320 px and 200 % text. To read a wrap yourself, spawn the same stub and node from
  `fe/ux` (`NODE_PATH=<repo>/fe/ux/node_modules node /tmp/<x>.js`): a wrapping flex row that did NOT wrap has
  a width equal to its children's widths plus the gaps, and `align-items: baseline` puts the children's
  `top` at different pixels on ONE line, so top equality says nothing about wrapping.
- The full `npm --prefix fe/ux run check` runs all three browsers here and finishes in about 12 min
  (the states of `STATES` in `fe/ux/check.js`); `check.js` prints its finding lines ONLY at the end, and a browser
  that throws mid-check records one line and the next runs, so the summary is complete. (It used to die
  at WebKit's context, "is-controlled-by-automation set but automation is not allowed in the context".)
- A CSS test pins the SELECTOR as a string, not just its declarations: `block('.chip > .chip-full')` and a
  `TOP_SELECTORS.filter(...)` equality list. So a specificity ruling (brief 15 R7: a direct span child of
  `.chip` is matched by `.chip > span` at 0,1,1, which outranks a bare `.chip-full` at 0,1,0, so the
  expanded pill showed the ellipsis it exists to escape) is fixed in the sheet by the child selector
  `.chip > .chip-full` / `.chips-profile .chip > .chip-full`, declarations unchanged.
- The ux stub's tag vocabulary is the API's: `tags` is the UNION and `instanceTags` the instance's own, so the
  profile's chips are `tags` minus `instanceTags`. A state that wants 8 profile chips needs a union of 16 and
  `instanceTags` of the 8 own; `(LONG_TAGS, LONG_TAGS)` brings the profile none, and no page can render them.
- axe 4.14's `target-size` (`wcag22aa`, 24 px, WCAG 2.5.8) answers `true` for a small target that is FULLY
  obscured, and `false` for one in the tab order that is only partly covered. A page a few px taller shifts a
  control out from under the sticky `.save-bar` and turns a 20x20 checkbox that passed for months into two
  findings: the finding is the checkbox, not the change. Baseline it by measuring both trees.
- `HearthTokens.test.ts`'s H1a.12 is an exact ordered list of EVERY `min-height`/`min-width` in `index.css`
  (`selector: property: value`, in written order), as `Reflow.test.ts` R1.1 is for `overflow-wrap`: any new rule
  that sets one of those needs its line there, which is the tester's edit. Brief 17 U17c is that red twice over:
  the brief named only `HearthH2a.test.ts`'s list and the tester updated that one, leaving the same list in
  `HearthTokens.test.ts` red, so name BOTH files whenever a CSS rule of that kind is added.
  A CSS test's per-selector helper (`topStyling`/`single` in `TensorPlacement1.test.tsx`: TOP rules whose
  comma parts, whitespace-stripped, equal the name) demands EXACTLY ONE top-level rule naming that
  selector, so a selector that also has its own rule can never join a shared comma group beside it:
  brief 21 D5's `.ot-layers` gets its own five-declaration rule, not the `.ot-tensor, .ot-goes-on`
  group. A `flex-basis`-only item is safe there (`topBy`/`declaring` use exact-set equality on the
  declaration set, so a 6-declaration row rule never joins those pins).
- `white-space` and `text-align` have NO sheet-wide exhaustive list (unlike `min-height`/`min-width`,
  `font-family`, `overflow-wrap`, `box-shadow`), so adding them to a rule needs no tester file: the pins
  that name them are per-selector (`Reflow.test.ts` R1.5 on `.model-profiles th`/`td:nth-child(2)`/
  `td:last-child`, `HearthH9a.test.ts` on `.jump ul button`, `HearthAssets.test.tsx` on the toggle and the
  h3 button), and a rule's declaration count is pinned only for the selectors the brief counts.
  A comma-group selector is written by Prettier ONE part per line (the sheet's own
  `.model-profiles th,\\n.model-profiles td` shape), and `Reflow.test.ts`'s `flat`/`compact` collapse the
  newline, so the tester's `topBy('.a td:nth-child(2), .a td:nth-child(4)')` still matches: write the
  broken form, the joined one is a `format:check` red.
- A brief AMENDED to add a declaration to a rule reddens that rule's per-selector "declares nothing
  else" list, which the amendment never names: brief 20 N.11 gained `flex: 1 1 4rem` on
  `.nav-instance` and its own count test (`NodesMenu2.test.tsx` 20 N.11, `declared(instance.body)
  .sort()` == `['min-width', 'overflow-wrap']`, its title and message saying "two properties") is the
  stale pin. When a rule gains a property, grep `declared(` over every `fe/src/*.test.*` that names
  the selector, and report the list as the tester's. (A value pin is order-free: `prop()` takes the
  LAST declaration of a property in the block.)
- `format:check` can be red in the tester's new test file alone (a double-quoted string Prettier
  wants single). Prove your own file clean with `npx --prefix fe prettier --check fe/src/index.css`
  and report the test file; a quote change moves no assertion, so it is the tester's to make.
- `strings.test.ts`'s "every value is used" scans source text for `t(` + a string literal, so a key that reaches
  `t` as a VARIABLE, or as one arm of a ternary in the first argument, counts as unused and reddens that test
  with no code change of its own. Write one `t('<key>')` literal per key, at a call site, and pass the value on.
- `fe`'s tab-order tests (`ModelProfilesFilter.test.tsx` 2b.35/2d.60, `ModelProfilesSwitch.test.tsx`
  2c.53/2d.60) list EVERY focusable of the page by accessible name, in order, so any new control (a sortable
  header's button, brief 17 U17c) reddens them by name: the tester's list, never a production change.
- A CSS-reading test's block helper must start counting braces AFTER the opening `{` at depth 1. One that starts
  at depth 0 stops at the first INNER `}`, so a media block with ONE rule reads as holding none, and its
  "the one rule it holds" assertion can never go green: report it, do not pad the sheet with an empty rule.
- The same shape in a descendant-selector helper: the sheet's rule selector, read as text, has its whitespace
  dropped (`.card .skeleton` becomes `.card.skeleton`), so a helper matching that part against a name written
  WITHOUT the leading dot (`'card.skeleton'`) finds 0 rules for every real selector, and its
  `toBeGreaterThan(0)` is red by construction. The only sheet spelling that passes is `card .skeleton`,
  an element named `card`, which matches nothing. Fix it in the helper (strip `^[.]` from each part) or in
  the loop's names: a CSS test's selector names must be spelled WITH the dot.
- A wrapping chip list capped by `max-width: 100%` on the chip is capped by its PARENT, so the `ul` of a flex row
  needs `min-width: 0` too: with the list uncapped the widest item grows the document's `scrollWidth` and the
  ux check reports `overflow: scrollWidth N > 320`.
- A test that reads a control the device read fills has to wait for that read. The `landed()` helper of the
  model profile GPU tests waits for the card's as-of line, so a probe fixture with `probedAt: null` waits
  for NOTHING: its option-list assertion is a race, green under `npm --prefix fe test -- <file>` and red in
  every full-suite run here (LS2b `19 G.16: a probe that answered with no time`). Prove it test-side in a
  scratch copy of the file: the same body with a `waitFor` on the options goes green, and reading the
  options where the red one reads them shows today's list, so the read had not landed. Report it: the
  tester's fix is a `waitFor`, and a production change that made the read land synchronously would be the
  wrong cure.
- The full `npm --prefix fe test` (109 files, one jsdom worker each) loses one `SignInOnly.test.tsx`
  timing test about 1 run in 10 here (`09 S1a.10`, a `findByRole` on the nav after sign-in), and lost one
  `AboutPage.test.tsx` `18 AN.3` once in two runs on 2026-11-22 (green alone, and the whole suite green on
  the rerun); a detached HEAD worktree with the same `node_modules` is green 6/6. It lost one
  `ModelProfileContent.test.tsx` `2d.70, 2d.80` once in two runs on 2026-11-23, green alone and green on
  the full rerun, with a CSS-only edit in the tree. State the rate in the report, do not chase it.
- A text field that may not read (brief 21's Layers) keeps its LAST READABLE value in the row's state,
  beside the shown text: a pattern derived from the text at render time cannot honour the GUI's "last good
  value" (an unreadable text falls back to whatever the derivation gives, and if that is the loaded value
  the `render(rows) !== render(parse(loaded))` change test of 21 T.8 sends NO key, so the red test cannot
  go green). So: `row.layers` (null = every layer) set from `splitPattern` at load, updated only by a text
  `parseLayers` reads, composed in `rowRule`; the handler (`otSetLayersText`) is the pure fn, in the card's
  module, not in the page.
- React: `flushSync` (from `react-dom`) is the only way a committed state is readable in the next statement, and a
  `startWrite` nested inside `flushSync` does not commit. A `useOptimistic` overlay ENDS a task after the answer
  resolves, so a test that focuses an item the answer restored cannot be written with it.
- An `aria-activedescendant` index over a DERIVED list must carry the list it was walked onto
  (`{ at, of: options.join('\n') }`, read as `active.of === key ? active.at : null`): a stateless
  derivation, no effect, no setter cascade. A plain index survives a poll that moves a label, and
  then `aria-selected` marks the label nobody walked onto and Enter writes it. So the two rules pair:
  every key that hides the list (Escape as much as Tab) clears the active option too.
- `fe`'s lint (`eslint-plugin-react-hooks` recommended) answers `react-hooks/set-state-in-effect` for a
  synchronous `setState` in an effect body, so "clear this row's typed text when its editor goes away"
  cannot be an effect. The render-time guard passes the linter and the tests: `if (blocks === null &&
  typed !== null) setTyped(null)` beside the `useState` (React's adjust-state-during-render, the same
  render that drops the field). The pure derivation `blocks === null ? null : typed` also lints, but the
  text reappears when the row re-enters the mode (Auto, then Even).
- `cargo clippy -p <crate> --all-targets -- -D warnings` builds the lib WITHOUT the tests, so a
  `pub(crate)` item of a private module that only `#[cfg(test)]` code calls is dead there. Split it: keep
  the production function, mark the wrapper the tests call `#[cfg(test)]` (a fn simply stops being built,
  so production carries no `allow(dead_code)`), and do the same with a test-only answer enum, keeping the
  narrow production error beside it. A `#[cfg(test)]` variant that production's `match` names is the shape
  to avoid, not to allow.
- `cargo test -p <crate>` compiles the `#[cfg(test)]` modules of the workspace's PATH DEPENDENCIES
  (pai-cli's run builds `pai-app`'s test module), so a new public field of a core struct reddens the
  fixtures of every crate that builds one: `crates/app/src/hf_fit.rs`, `crates/cli/src/cli.rs` and
  `crates/config/src/tests/save_flow.rs` all write full `Shape { .. }` literals. Rust has no default
  for a struct literal, so no shape of the field keeps them compiling. Enumerate the E0063 sites,
  report them as the tester's, and prove production with `cargo clippy -p <crate> -- -D warnings`
  (no `--all-targets`).
- "Saturating, divide last" is NOT a `saturating_mul` chain: `(a * b * c).saturating_mul(...)/d`
  answers `u64::MAX / d` on an overflowing product, and a test pinning the ceiling wants `u64::MAX`.
  Write `a.checked_mul(b).and_then(|x| x.checked_mul(c)).map_or(u64::MAX, |x| x / d)`: exact when the
  product fits, the ceiling when it does not.
- Gate a request body before connecting: parse and content-type in one helper, answer the route's own
  `*-malformed` 400 first, and let a test pin that order with an empty calls list.
- `serde_json`'s `preserve_order` is ON for a crate built with `pai-app` in the same invocation and OFF in
  a bare `cargo test -p pai-bff`, so a body's `Map` iterates in insertion order in one build and in a hash
  order in the other: sort the edits so the writer's file order never depends on the map. A red only
  `preserve_order` shows is INVISIBLE in a bare `-p pai-bff` run: reproduce it with both packages in one
  invocation (`PRIVATE_AI_LLAMA_CPP_DIR=$PWD/build/linux/llama.cpp cargo test -p pai-bff -p pai-app --lib -- <name>`). Name the order in the doc comment as the code states it ("sorted by key").
- A closure binding used as a fallback is `clippy::redundant_closure` when written `|| other()`: pass
  `&other` (a `&F` with `F: Fn` is `FnOnce`), which keeps the binding for the arm below it.
- `cargo fmt -p <crate>` rewrites the crate's `tests/*.rs` too. When a tester's file is pending a lint,
  apply rustfmt's diff to your own file by hand (`--check` prints it) and report the test file; plain
  `cargo clippy -p <crate> -- -D warnings` (no `--all-targets`) shows the production code clean. After a
  rename that lengthens identifiers, `cargo fmt -p <crate>` per touched crate, and count
  `git status --porcelain -- '*.rs'` before and after to prove fmt touched no new file.
- rustfmt's order inside one `use` group: a plain `use` of a path BEFORE a `pub use` of the same prefix
  (`use pai_core::devices::backend_of;` above `pub use pai_core::devices::{gpu_pool, is_integrated, Gpu};`),
  and inside the braces lowercase items before the `CamelCase` one. A blank line + comment starts a group,
  so a re-export of a path that sorts before the group above it needs its own group, and the pair order is
  what `--check` wants first try.
- rustfmt: it sorts consecutive `mod` declarations (`reorder_modules`), so a `mod` added out of order
  fails `--check`; it breaks a one-line `let x = if c { a } else { b };` past 50 chars
  (`single_line_if_else_max_width`), so write the multi-line form. It also breaks a
  `return Err(format!("{}: {}", path.display(), strings::t("key")));` at an 8-space indent (82 cols,
  the call's args pass `fn_call_width`), so write that one in its broken form from the start. Shortening a test with `s == ""` is
  refused by `-D warnings` (`clippy::comparison_to_empty`): write `s.is_empty()`.
- The tester's test code reddens the linter under `--all-targets`, and most of it is not yours to fix:
  `for who in [&alice]` is `clippy::single_element_loop`; `.err().expect(msg)` is `clippy::err_expect`,
  whose identical mechanical fix is `.expect_err(msg)` (apply it, name it). A fix that changes what a test
  asserts stays the tester's. A binding a test clones and never reads (`let files = c.files.clone();`) is
  `unused_variables`, and the compiler's `_files` prefix loses no assertion: apply it and name it.
- A test helper of a test module whose ONLY callers are `#[cfg(unix)]` tests (a bound and a
  thread-race wrapper beside the FIFO tests) is `dead_code` in the Windows `--all-targets` clippy. The
  file's own convention is the fix and it moves no assertion: gate the helper `#[cfg(unix)]` and say the
  reason in its doc (the gpu_probe helper module gates `use scripted::*` that way; shipped for the
  fakes' `FakeService::stopped`/`running` and for a `let mut want` whose only `mut` use is a
  `#[cfg(unix)] want.insert(..)`: that one becomes the pair `#[cfg(unix)] let mut want` /
  `#[cfg(not(unix))] let want` with the same rows, the `insert` gate untouched). The same region reddens the
  Windows `cargo check --all-targets` with
  `unresolved import` when a `#[cfg(unix)] mod`'s users sit behind a bare `use <mod>::*;`: the un-gated
  import is the defect to report, never a production change.
- `nix` 0.31 `fcntl::OFlag` needs the `fs` feature (`OFlag::O_NONBLOCK.bits()` is the `c_int` for
  `OpenOptionsExt::custom_flags`, so a FIFO open answers ENXIO instead of blocking). Call the trait
  method fully qualified inside the `#[cfg(unix)]` statement: a `use std::os::unix::...` at the root is
  `unused_imports` on windows-gnu. That statement forces the `OpenOptions` builder into statement form,
  and rustfmt then breaks an `.open(path).map_err(..)` chain.
- A `pub fn` parameter the function no longer needs (a size the caller read before the open, now
  governed by the opened file's metadata) is `unused_variables` even when public: use it as the bound
  the opened metadata clamps (`file_size.min(opened)`), never with a `_` prefix in a public signature.
- A whole crate of mechanical fixes (15 needless `&` in the manager's test fixtures) goes in one call:
  `cargo clippy --fix --allow-dirty -p <crate> --all-targets -- -D warnings` names the file and how many
  fixes. Snapshot the file outside the repo first, `diff` it after: every line must be a `&` removed, and
  the fixer recompiles, so a coercion it cannot prove is left alone. `cargo fmt -p a -p b` takes several
  crates in one call.
- Enumerating the reds a panic hides in a one-test e2e, in the tree copy: rewrite `assert!(cond, msg)` as
  `if !(cond) { eprintln!("MARK"); eprintln!(msg); }` (find the `assert!(` above the needle, balance
  parens, split at the first top-level comma; a `"RED " + msg` prefix does not compile), and make a shared
  snapshot helper (`kept`) non-fatal too. Then `cargo test -p <crate> -- --nocapture`: captured stdout is
  printed ONLY when the test fails, so an all-patched copy that passes prints nothing and a plain grep of
  the marks is the full red list.
- Flakes to state in the report, not chase: xtask's script-sandbox tests share ONE temp dir per process
  (keyed by `std::process::id()`, the same in every thread), so a full `cargo test -p xtask` can fail one
  `linux::system_files` script test that passes in isolation and on a rerun; the bff's `tests/session.rs`
  is a wall of timing tests and loses one occasionally.

## Slice 18 AN2 (2026-11-22): the Vulkan child

- A source-text pin reads code with whole-line comments left out and stops a method at the first brace
  at column 4 (`code` / `impl_text` / `method_text` in `crates/manager/src/tests/vulkan_probe.rs`): the
  needle must be a call in that method's own body. AN2's wanted `run_hidden_probe_within` inside
  `VulkanProbe::probe`, so the spawn cannot live in a free helper beside the impl, and no comment can
  ever satisfy it.
- `ash` with `default-features = false, features = ["loaded"]` opens the loader at run time
  (`unsafe { ash::Entry::load() }`): the manager links no Vulkan and starts on a node that has none, and
  a hung ICD dies in the child. ash 0.38 is already in the local registry cache and in `Cargo.lock` (via
  `gpu-allocator`), so add it with `cargo --offline`; it compiles for windows-gnu. 0.38 has no builders:
  `vk::X::default().push_next(&mut y)`, and the extension names are consts
  (`vk::EXT_PCI_BUS_INFO_NAME`). `serde_json::Map` is not `Display` (wrap it in `Value::Object`), and
  `.map(CStr::as_ptr)` over an `iter()` of `Vec<&CStr>` is a `&&CStr` arity error: write the closure.
- A child that answers in words has no exit code to read: every reading that ends with no device list
  prints one of `ok` / `none` / `failed` as one JSON line, so "no loader", "driver refused", "killed at
  the bound", "flood past the cap" and "junk text" are all facts the parent parses. A hidden subcommand
  of the manager returns from `manager::run` before the SCM dispatcher arm, so the child on Windows never
  touches the service machinery.
- A cache window over a caller's clock is `now.saturating_sub(ran_at) < WINDOW`, read twice: once before
  the serial lock (a polled page inside the window never reaches a child) and once with the lock held (the
  callers that queued behind one child take its answer). That is `ProbedGpus`' pattern.
- The same pin for a FREE function (`free_fn` in `tests/vulkan_chain.rs`: signature to the first
  column-0 `}`) is why the child's device work cannot move into a helper: the needles
  `enumerate_device_extension_properties` and `instance_api` / `api_version` must sit in
  `read_devices`' own body.
- ash's loaded mode loads the CORE 1.1 calls through `get_instance_proc_addr` and its wrapper calls
  the pointer whatever it is: a 1.0 loader answers NULL for `vkGetPhysicalDeviceProperties2`, so a
  1.0 node is read only through the extension's own struct,
  `ash::khr::get_physical_device_properties2::Instance::new(&entry, &instance)` (`new` is safe, the
  call unsafe). A chained `vk::PhysicalDeviceProperties2::default().push_next(&mut x)` is the same
  for both routes, so one small enum over the two calls feeds `bus_id` and `driver_texts`.
- Destroy the instance on EVERY path with a `Drop` guard owning it, declared after the entry: locals
  drop in reverse order, so the instance dies before the loaded entry closes (ash's rule), and every
  `?` of the read destroys it. The one arm that returns early (no properties2) needs no destroy.
- A module `fn names` is shadowed by a local `let names` of the same fn: `names(&listed)` is E0618
  "expected function, found Vec" pointing at the far-away fn. Give the helper a distinct name.
- `&unsafe { call() }?` as an argument is E0716 (temporary freed at the statement's end) and
  `.map(|v| helper(&v))` over an owned `Vec` is E0515: bind the `Vec` in a `let`, then pass `&binding`.
- Two lints of new code: `iter().any(|x| *x == y)` is `clippy::manual_contains` (write
  `list.contains(&y)`), and an enum variant left unbuilt because its arm became an early `return` is
  `dead_code` under `-D warnings` (drop the variant).
- The node's GPU row cap is ONE const: `host::MAX_GPU_ROWS` (made `pub(crate)` for this), truncated in
  `gpu_drivers` AND at the end of `vulkan::join` (after the appended rows, so the kernel's rows, which
  come first, always survive and a device past 32 falls off). A test module that names the cap only in
  prose (`host::MAX_GPU_ROWS` in a doc comment) writes `const ROW_CAP: usize = 32` of its own, so the
  prose is no needle: only a `truncate` in `join`'s body turns it green.
- A Vulkan device with no `VK_EXT_pci_bus_info` is printed by the child (no `pci` key) and the join
  APPENDS it as an unjoined row (`a_device_with_no_bus_id_is_its_own_row`), so the child's module
  comment must not say "no bus id answers no device" (the one node answering no device is a node with
  no properties2 route at all: `read_devices` returns an empty list). The join is AN2's, not AN4's; the
  comment at `crates/manager/src/tests/vulkan_chain.rs:13` still says AN4, and a test file is not mine.

## Slice 18 AN5a (2026-11-22): one endpoint, several readers

- A count over one endpoint counts every reader of it. 18 AN.15 keeps the Overview's version line
  reading `GET /api/version` (13 N.10) beside the shell's new one, so a test that counts that route
  owes both: at `#/` the total is 2, and a sign-out and sign-in started at a page address ends at
  `#/` (09 S1a.9: the sign-in page's bar is `#/sign-in`, and `signedIn` moves it to `#/`), so the
  Overview mounts and its line reads again. Proof is the dumped sequence (`bff.calls.map`), which
  shows the route twice and `window.location.hash` as `#/`; it is not a race, so do not chase it.
- React runs a child's effect before its parent's. A fixture that hands ONE `Response` to every call
  of a route read by the Overview's line and by App (`version: reply(200, ..)`, `version: h.answer`)
  serves the body to the child; the shell's read then rejects and keeps its fallback label. The DOM
  dump proves both: `<p>Version 2.99.1</p>` and the `h2` still "Instances" from one reply. A fixture
  body as an OBJECT answers every call fresh, and both readers are served.
- Two tests in one file, same fixture, can pin incompatible DOM: `NodePage.test.tsx` 243 wants the
  first `dl` to be the host's four rows, 305 wants `querySelectorAll('h2, h3, dl')` to be exactly the
  four nodes from `H2 Engines` to `H2 GPUs`, so no top `dl` exists. Report the pair, keep the brief's
  page (AN.8).
- A "no other word" loop over `document.body.textContent` meets a new nav label: the brief's
  `nav-node` "Node info" contains the forbidden `No` (`HearthTilesBoot.test.tsx` 264 to 269 vs 18
  AN.6). And `strings.test.ts` 78 to 91 ("every value is used") pins the WHOLE brief's key table, so
  a slice that defines a later slice's keys (AN.1's 18 GPU keys) cannot pass it; `fe/ux/check.js`
  reads the same way, its `node` / `node-down` / `node-long` states pin AN5b's GPU blocks (five
  `dl.assets`, 32 `h3`, the manager-down paragraph). Check which slice a check's states belong to
  before promising it green.
- The web UI's `format:check` writes nothing: fix the files it names with
  `npx --prefix fe prettier --write fe/src/<f>.tsx` from the repo root, then rerun the check.
- Mid-slice, an earlier sub-slice's new files are UNTRACKED (`?? fe/src/Value.tsx`, `version.ts`), so
  `git diff <path>` prints nothing for them and proves nothing about your edit: check `git status
  --porcelain` and grep the file. And "X is imported nowhere" is a grep over production AND tests: the
  local `const textOf = (el) => ...` of five `Hearth*.test.tsx` files is not the export of `Value.tsx`.
- The fe readers point one way: `version.ts` imports `osIn`/`textIn` from `node.ts`, never the reverse
  (a cycle). So a shared reply-text reader lives in `node.ts` and is exported from there; note `node.ts`'s
  private `versionIn` keeps the OTHER rule (`value !== ''`, no trim), and `textIn` trims, so the two are
  not one function.
- A name a source lint matches is the lint's data. `crates/config/src/tests/control_lint.rs`'s
  `CONFIG_PATHS` names the config paths the GUI may read by the path fn's name: a `paths.rs` rename
  silently emptied the list and the lint passed with no bite. Update the lint's data in the same slice and
  say so. `config`'s `names_lint` scans manager and core `src/` test modules and refuses the bare
  `private-ai-manager`, so a fixture naming it reds `cargo test -p pai-config --lib` while the manager's
  own suite is green.
- A strings-file VALUE is pinned by the module tests of its callers: renaming a word reddens tests that
  quote the sentence, and the tester's red list misses some. `grep -rF "<the old sentence>" crates` is the
  list; every hit asserting text the rename is meant to change is acceptance, so update the literal (an
  exact equality stays exact) and name each one. Run every crate whose module tests show a shown text:
  core, app, cli, config, bff, manager. A `cli.toml` rename changes the refusals the manager and the bff
  ANSWER, so it reaches the browser with no wire change.
- A `checks` refusal text is pinned by TWO core tests over BOTH strings files, and a numbers-key test
  pins its `{N}` count: one text change is one line in `en_US.toml` and one in `cli.toml`, placeholders
  kept.

### Windows GNU check

- Windows-only code is type-checked with `--target x86_64-pc-windows-gnu` (check, then clippy).
  `pai-config` does not build there: skia-bindings (slint `renderer-skia`) has no prebuilt binaries for
  `windows-gnu`. `pai-app` and `pai-cli` need `PRIVATE_AI_LLAMA_CPP_DIR` set to a clone. Check
  windows-sys signatures in `~/.cargo/registry`.
- `cargo clippy -p xtask --target x86_64-pc-windows-gnu` prints two `xtask (manifest)` warnings
  (`serde_json`, `sha2`) and still exits 0.
- A lib-root `use` needed only inside `cfg(target_os = "linux")` code is `unused_imports` there: gate it
  with the same cfg.
- `Command::get_args()` yields `&OsStr`, not `&OsString`: collect with `.map(OsStr::to_os_string)`.
- A windows-sys feature a crate calls must be in that crate's own manifest, even when another dependency
  happens to enable it.
- `crates/core/src/client/dbus.rs` is `cfg(target_os = "linux")`, so that check does not compile it: say
  so in the report.
- `mod windows` is `#[cfg(windows)]` and windows-gnu is not unix, so NO file under `xtask/src/windows/`
  may import `crate::common::components` (nor `macos_pkg`): `xtask/src/common/mod.rs` gates them
  `#[cfg(unix)]` and `components.rs` itself calls `std::os::unix::fs::symlink`, so the import is E0432 and
  ungating it cascades into unix-only test code and a crate of dead code. A fact the Windows MSI needs
  (a package name, a dist version) goes in a module that compiles on both: put it in the ungated
  `common/versions_share.rs` and `pub use` it from `components`, so every unix caller
  (`components::ENGINE_NAME`, `components::dist_version`, `macos_pkg`) keeps its path and the moved item's
  test, in the old module's `mod tests { use super::*; }`, still resolves the re-export. Give a moved fn a
  caller on EACH target (`components::package()` calls `versions_share::runtime_package` for a runtime):
  then it needs no `allow(dead_code)` on unix or on windows. `licenses.rs` shows the other shape: a
  `#[cfg(target_os = "linux")] use crate::common::components::Component;`.
- A `pub fn` in `xtask/src/common/util.rs` whose only callers are Linux code is `dead_code` on windows-gnu
  (`cmd_without_ld_library_path`, from HEAD): a red of the Linux step's own commit, not of the slice. Proof
  it is not yours: `git worktree add --detach /tmp/x HEAD` (the worktree needs no `.cargo/config.toml`:
  pass `CARGO_TARGET_DIR=<repo>/build/linux/cargo-target`, the windows-gnu registry deps are reused) and
  the same clippy answers the one identical error. The Windows TYPE check itself is green: a plain
  `cargo check -p xtask --all-targets --target x86_64-pc-windows-gnu` ends `Finished` with that as
  1 warning, only `-D warnings` turns it into an error.
  The fix (shipped): gate the fn `#[cfg(unix)]` and gate with it the util test
  module's test that calls it (its sibling test of the same helper is already `#[cfg(unix)]`);
  `cmd` stays alive on Windows through `xtask/src/windows/*.rs`, so the gate cascades into nothing.
- Proof of a windows-gnu red you must not fix: copy the tree, in the copy gate the real import
  `#[cfg(unix)]` and add a `#[cfg(not(unix))]` stand-in `mod` with the same names, set `CARGO_TARGET_DIR`
  to a fresh dir, run the check there. Clean output isolates the gating red and proves your moved code
  compiles; the anchor asserts must be exact text (a `'# [cfg(unix)]'.replace('# ','')` anchor left
  `#pub mod components;` behind).
- A test's `const _: fn(&Path, &str, &str, &[(&str, Option<&str>)]) -> Result<()> = f;` signature pin is
  `clippy::type_complexity` under `--all-targets`; the repo's fix, used in core's `checks.rs` and
  `server_cfg.rs` test modules, is a bare `#[allow(clippy::type_complexity)]` above the item (no assertion
  moves).

## Slice 18 AN5b (2026-11-22): the GPU blocks, and an earlier sub-slice's helpers

- A sub-slice's test helpers are page-wide, so the NEXT sub-slice reddens them. AN5a's `h3s(main)`,
  `engineLists(main)` (`dl` whose previous sibling is an `H3`) and `lists(main).length` counted the whole
  `main`; AN.10 adds an `h3` + `dl.assets` per GPU, so four AN.8 tests went red on counts, not on the
  engine block they mean to pin. The tester's edit: scope the helper to the section (a `compareDocumentPosition`
  against the `h2` GPUs answers it).
- Three pins in that file are red by construction, proved in a tree copy where only the literal moved:
  the in-kernel driver test builds its row with `gpu()` = `GPUS[0]` (driver `amdgpu`) and expects `xe`
  (that is `GPUS[2]`), and compares `el.tagName` (HTML reports `SPAN`) with the literal `'span'`; the
  32-GPU test sets `driver: long, version: long` and expects the driver row as ONE `long`, which
  contradicts AN.11 and the same file's own joined-text test.
- `pgrep -af stub.py` matches the shell whose command text names it (the `pkill` lesson again). To prove
  no stub survives the UX check, use a pattern a shell line cannot carry: `pgrep -af 'python3? .*stub.py'`.
- `fe/ux/check.js` is the built-page proof of the GPU blocks: the `node` state wants `dl.assets` count 5
  (top, one engine, three GPUs), `node-down` the `node-info-unavailable` paragraph, `node-long` 32 `h3`
  at 320 px and 200 % text. Its `stub.py` stderr (`BrokenPipeError` in `do_GET`) is a browser closing,
  not a finding: only the `summary:` line's findings count.

## The bff

- Every body route gates and parses with the ONE helper (`session::json_body(&headers, &body)`): absent,
  unreadable, or another essence is the route's own `*-malformed` 400, answered before the first connect.
  `application/json; charset=utf-8` passes: the helper compares the essence.
- The session is read before the body in every route, so an over-cap body with no session is the session's
  401 and the extractor's 413 never runs: a doc comment states the order the code has. Only sign-in has
  no session, so it is the one route whose doc may say the cap answers first.
- The two-limit write shape (a route that reads, then writes): ONE blocking task under `timeout` for the
  connect and the reads, the client moved out of it into a SECOND task bounded by
  `write_timeout.saturating_sub(elapsed)` for the write; every write behind `WriteState::may_write()` so a
  task the deadline left behind writes nothing. A handler taking both limits needs
  `#[allow(clippy::too_many_arguments)]`.
- The framing and sniffing headers (`X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`) are ONE
  middleware in `lib.rs`, added as the LAST `.layer` of the router builder, the outermost position: the
  guard's 403, the body cap's 413 and the router's 404 and 405 are answered above every route, so only an
  outside layer can set them, and `HeaderMap::insert` replaces, which is what makes "exactly once" true
  whatever a route wrote. A Content-Security-Policy, once the web UI agrees one, goes in this layer.
- A middleware that needs per-router facts uses `axum::middleware::from_fn_with_state(policy, guard)`: the
  router's state stays `()`, the fn's first parameter is `State<T>`, then `Request`, then `Next`, and
  `T: Clone + Send + Sync + 'static`. `from_fn` is the same call with `()` state.
- A list handler over a manager whose trait reads rows in ONE call (`instances()`, no failure slot on the
  row type) and status per id (`status(id)`): ONE blocking task does the rows then a status per id (the
  test pins the sequence `["list", "instances", "status <id>"]`: rows asked ONCE), the status read is
  TOLERANT (a failed read is an empty list), and a rows failure answers 200 with an empty list and NO
  client detail, because the row type cannot name the instance a 404 belongs to.
- A bff write route with nothing to show: gate `json_body(headers, body)?` BEFORE the session (the route's
  own `*-malformed` 400 answers with no client call), then `session_user`, then the client in
  `spawn_blocking` under `timeout`. Map the checks refusal through the boot route's ONE
  `action_failure(422, field)`; a 403 from the client ends the session like the boot write's.
- A save's post-write state read: never `ok().flatten()` the record read. `None` means "no
  instance runs it", which answers `running: false`; a record read that FAILED is a state the
  bff cannot name, answered `running: true` like a failed status. A 3-arm `match` on the
  `Result<Option<String>, _>` keeps them apart, and the test pins it by the calls list
  (`read, write-if, instances`: no `status` of an instance the record cannot name).
- A list item whose extra fields come from a row looked up by id: `find` the row once per id ABOVE the
  status `match`, then answer a missing row with the field's own default (`row.map_or(0, |r| r.port)`,
  `row.map_or_else(Vec::new, |r| r.list.clone())`: no `||`, and the closure fixes the `Vec`'s type). The
  `json!` key order is not a pin: every test here compares parsed JSON, so `preserve_order` never shows.
- A list item's fields are pinned by the test's whole-object `json!` literals. Where the brief asks for a
  field (a port) the literals omit, the literals win: implement what is pinned and report the conflict.
  Every whole-list assertion elsewhere in the suite is then a stale pin: the bff's 13
  `tests/origin.rs` Host tests compare the whole list and went red on the new `tags` key alone.
  The same file reddened again on the boot flag, in two ways that are BOTH the tester's: its 10 literals
  gained `"boot": false` while that file's `FakeManager::boot_enabled` answers `Ok(*state)` (the shared
  `InstanceState`, so an Active instance answers `true`, and no code can answer the literal), and its
  `a_get_route_is_never_refused_for_these_headers` pins the pre-record server read (a 404
  `client-no-instance` and a calls list of `list, status, boot_enabled, read server@default.ini`) on a
  fixture that implements NO `instances()`, so the trait's default `Other` answers 502 the moment the
  route resolves the profile. Grep every file for the route's calls-list pins and for the whole-list
  literals BEFORE writing a per-item field.
- The additive-key red is the same in slice 16 (`removable` on the instance rows): 10 whole-row literals in
  `tests/instances.rs` and the 13 in `tests/origin.rs` went red beside the new test that pins the key.
  `grep -n '"instanceTags"' crates/bff/tests/*.rs` (the row literal's last key) enumerates every pin in one
  call, and the two counts in the report are the new test's rows and the stale literals.
- `session::error(status, key, args)` takes `&[u64]`: an answer whose `args` are NAMES (`client-no-server-profile
  [profile]`) needs the module's own helper building `{"key", "args"}` from `&[&str]`; the one in
  `instances.rs` is `keyed`/`refused` (the 422 that carries `field`).
- A write on a path a read already owns (`POST /api/instances` beside `router_with`'s `get(instances)`)
  merges in axum 0.8: `Router::merge` adds the methods of the same path, and `PUT`/`DELETE` there stay the
  router's 405. A module and a fn of one name (`mod instances` and `fn instances`) coexist, and the
  handler's `json!` row keeps its keys in the order the tests read them.
- A D-Bus row tuple that grows past clippy's `type_complexity` is a module-level `type InstanceRow = (...)`
  beside the other consts, with the signature `(s id, s engine, q port, ...)` in its doc comment.
- One `Host` name rule over both families: strip the brackets and `text.parse::<IpAddr>()` covers IPv4 and
  IPv6 (an IPv6 with a zone id parses as no address; `Ipv4Addr` and `Ipv6Addr` each need their own parse).
  `IpAddr::is_loopback()` is `127.0.0.0/8` and `::1`, `is_unspecified()` is `0.0.0.0` and `[::]`.
- A section essay written as `///` and followed by a blank line and a doc'd item reddens
  `clippy::empty_line_after_doc_comments` in the tester's `tests/origin.rs` under `--all-targets`. The
  fix is comment-only: drop the third `/` on every line of the essay (the item's own doc stays `///`).
  Assert the line count and the prefix of every line in the script, it wrote nothing when its count was
  off, and the file was unchanged.
- A pub builder over 7 arguments needs the repo's bare `#[allow(clippy::too_many_arguments)]` above the
  `pub fn` (`model_profiles.rs`), production carries no other mark for it. A policy read inside the
  middleware: call the source BEFORE taking the `Mutex` of the list, so the lock never spans the read.
- An axum handler here takes `Path(id)` first and the `Bytes` last, and an over-cap body is the
  extractor's rejection handed to `origin::body_refusal(&rejection, <fallback key>)`, which answers the 413
  (the cap is the `DefaultBodyLimit` layer in front of the whole router).
- A "the list asks only these calls" pin is a per-file grep, not one line: a new field per item (the boot
  flag) needs one more call per id, and the suite holds a second test pinning the CALL SEQUENCE of the same
  route. When the two disagree (`reading_the_instances_asks_only_the_read_only_calls` asserted four calls
  with no `boot_enabled`, in a test whose own doc says "each status and each boot flag", while
  `each_instance_carries_its_own_boot_flag` asserted the six) no code answers both: the JSON literals decide
  (three tests need the field), quote both lines.
- R1 as amended: a page-load route asks the manager NO `devices` (the call can run the engine's probe for
  up to 30 s under the 5 s read bound), so the server document's `limits.Threads`/`ThreadsBatch` cap at the
  bff's OWN `available_parallelism` (`server::document` takes no `cpus`; the save path reads its count at
  the call). `GET /api/node/devices` is the only `devices()` caller; a new call from a shared route reddens
  every suite whose fake implements no `devices()` with a 502 and adds a `"devices"` line to every exact
  `app.calls()` pin. `grep -n 'app.calls()' crates/bff/tests/<route>.rs` is the list.
- A fallback a test pins through a fixture answering NO answer (`ClientError::Unavailable`) and through a
  count of 0 keeps the route's status untouched: read the count as a tolerant value beside the file, never
  as a call whose failure reaches the route's `match`.
- A save route's BODY SHAPE gate outranks a record-resolution pin: `server.rs` gates `Tags` as
  `Kind::Tags::reads(None) == false` (the read always sends a list, so an absent `Tags` would clear the
  profile's tags), pinned shipped by `a_tags_that_is_not_a_list_of_strings_is_server_malformed`. A test that
  pins a record failure through a PUT therefore reaches it only with a well-shaped body (`save_body(&doc,
  &[])`); the two that sent `{"revision":"x","values":{}}` answered 400 `server-malformed` where they
  wanted 404/503. The gate also explains why the record lookup lives INSIDE `spawn_blocking`, after the
  shape check: the shipped test pins "no new connect" for a malformed body.
- A `Box<dyn ManagerClient + Send + Sync>` does not coerce to `&dyn ManagerClient`: call the helper as
  `profile_file(&*client, ..)`.
- The installed versions are one small file per engine/runtime package
  (`pai_core::versions::installed(dir)`, the bff's `GET /api/node`). Keep the reader's rules: regular files
  only, `llama.cpp` / `llama.cpp-[a-z0-9]+`, at most `MAX_FILE_BYTES`, first line trimmed, printable ASCII,
  empty is `None`, anything unreadable is SKIPPED (a broken file answers no version, not no list), the
  engine listed only when its own file is there, backends sorted by name. A `match` on two chained
  `strip_prefix` calls is `clippy::question_mark`: chain them with `?`. The reader takes at most
  `MAX_FILE_BYTES` + 1 bytes (`File::open` + `Read::take` + `read_to_end`): `read_to_end` answers
  `Result<usize>`, so its error arm is `.is_err()`. A source-lint test pins that shape (`.take(` present,
  `fs::read(` absent above `#[cfg(test)]`, comments stripped per line at `//`), so `fs::read_dir(` passes
  and a doc comment naming the cap is no needle.
- A value check the manager answers needs an arm in the bff's 422 field map beside `override-no-*`
  (`tensor-split-format` marks `tensor-split`, `moe-cache-with-tensor-split` marks `moe-cache-mib`).
  The map's fallback arm is `args[0]`, so a refusal whose args start with the profile's name
  (`model-profile-device-outside` is `[profile, instance, key, device]`) marks the profile id and not
  the control until a keyed arm reads `args.get(2)`; the test's `assert_refused(.., field)` is the pin.
  Since ruling (a) the device keys carry NO marker exemption in `parse_body`: `checks::device_list` and
  `tensor_split` judge `RawSection::get` (the value the preset reader cuts at the first `;`/`#`), so a
  marker never reaches them and every body key goes through the generic `checks::comment_markers`.
- Two bff tests over ONE value shape are unsatisfiable together, and the copy proves it in minutes:
  `save_device_value_with_a_comment_marker_is_422_comment_markers` wants `device = "ROCm0;x"` answered by
  the bff with 422 `comment-markers` and ZERO connects, while case 4 of
  `save_a_gpu_distribution_value_the_manager_refuses_marks_its_control` wants `mmproj-device = "ROCm0;x"`
  answered by the stub's faked `Outcome::Invalid { key: "device-bad-name", .. }`. The stub returns that
  outcome from its own write, so the manager's real rule NEVER runs and any bff gate on the value
  pre-empts it; the same marker in two keys of the same `DEVICE_LIST_KEYS` cannot be refused by the bff in
  one and reach the stub in the other. Nothing about the value separates them (`;` truncates both lines
  alike), so the only rule that passes both is keyed on the key name, which ruling (a) gives no reason
  for. Report it as the tester's: the coherent case keeps the passthrough and drops the marker
  (`mmproj-device = "ROCm0, ROCm1"` with args `["mmproj-device", " ROCm1"]`). The case loop stops at 4, so
  cases 5-7 stay unverified behind it; the field-map arms they need are shipped.
- A bff save's calls list ALWAYS ends `list, status <id>`: the 200 answers `running` from the record and the
  states, pinned by `model_profile_save.rs` 1865-1877 and 2589-2599. A test pinning
  `calls == [read, write-if]` for a save is unsatisfiable, and so is one that puts a
  `MODEL_PROFILE_KEYS_NOT_ON_WEB` key in `values`: `parse_body` answers 400 `model-profiles-malformed` for
  any key off `WEB_MODEL_PROFILE_KEYS` (pinned by `save_key_outside_the_web_list_is_400_and_reaches_no_manager`,
  which names `moe-cache-mib`). A manager rule about a key the web cannot write is pinned only through the
  field-mapping test, which sends a web key and reads the 422's `field`.
  G1 is that pair in full: `save_a_gpu_distribution_value_the_manager_refuses_marks_its_control` ends
  with a `moe-cache-mib` + `split-mode = tensor` case wanting 422, and deleting ONLY that case in a tree
  copy turns the test green, so the 400 is that key gate and NOT the three-key gate, the marker
  exemption or the `tensor-split-format` arm (all shipped and green). `fe/briefs/api-contract.md` fixes
  the list at "68 keys, seven left out" with `moe-cache-mib` out "until 2g": no rule lets a read-only
  key ride a sibling web key.
- A session-idle test cannot reach 401 by advancing twice a step SHORTER than `IDLE` with any read in
  between: `GET /api/session` is `Sessions::current`, which MOVES the deadline like a page read, so the last
  move lands at t=SHORT and the next request at t=2xSHORT is only SHORT old (1740 < 1800), served 200. A
  route that only peeks loses nothing. The pattern that works is `control.rs:1568`: advance `IDLE + 1s`
  after the action.

### The bff's test fakes, and reading a red there

- A calls-list prefix is a SUBSTRING of a logged line: `actor_of("read models.ini")` counts
  `read models.ini.disabled` too, and the fake PANICS when the prefix matches more than one line
  ("more than one call"), so such a test is unsatisfiable for a route that reads both pool files,
  beside the rows test that pins a profile from the disabled file. Read the fixture's helper before
  treating the red as yours. `sorted_calls()` sorts the lines (an order-free pin); the switch suite's
  calls pins are exact order.
- The association write's `running` is the effective-id set (`assign::rows` over both pool files and the
  section, before and after) differing AND the instance running, nothing else: ruled out (ARCHITECTURE.md
  slice 15 "As built") the second clause that pinned a restart for an `on`/`off` written under a `*` of
  `inherit` while `effective` was false on both sides, since the render emits the same sections. A record
  read back that fails answers `running: true` (the design says so, and a test pins it), so
  `read_back` returns the text beside a "came from the manager" flag rather than only the text.
- Two pins of the assignment PUT are unsatisfiable together: `the_assignment_put_answers_the_record_the_
  manager_left_after_the_write` counts `read assignments.ini >= 2` (the after-read the design states),
  while `the_assignment_put_asks_as_the_signed_in_user` calls `actor_of("read assignments.ini")`, whose
  helper asserts the prefix matches EXACTLY one line. Nothing about the request separates them. Keep the
  after-read (the design, and the `effective`-from-the-machine tests) and report the second red.
- The fakes implement ONLY the `ManagerClient` methods their own routes call, and the trait's defaults
  answer `ClientError::Other`, which the bff answers 502. Adding a manager call to a SHARED route
  reddens every suite whose fake lacks it: the model profile list's new `instances()` (the rows'
  `assignments`) is green in `model_profiles.rs`, which implements it, and 502 in
  `model_profiles_switch.rs`, which has only `list_instances`. Grep `crates/bff/tests/*.rs` for the
  method name before adding the call, and report the suite the fake cannot answer.
- Ask a per-instance `status` on a FRESH `connect(None)` created right before it, never on a client
  kept from an earlier connect: the fake logs `as <principal>` ONCE per connect, so a stored
  `connect(Some(user))` client logs a later `status` under the segment that opened it, and
  `actor_of("status <id>")` reads the wrong principal. Each read of the bff's own account
  (`instances`, `status`) is its own segment, and a record read BEFORE a write lands under the
  account of the client made first, which is the one the pin expects.
- A `Result<_, enum>` whose variant carries an axum `Response` is `clippy::result_large_err` (a body):
  box it in the ONE mapping fn that builds it (`Answer(Box<Response>)`) and add module
  `type Answered` / `type Outcome` aliases for the four-deep `Result` a route passes to `read_answer`
  (`clippy::type_complexity` otherwise). `Refusal.key` is already a `&'static str`: `.as_str()` on it
  is `clippy::needless_borrow`, and on a `&str` the method is unstable (E0658 `str_as_str`).
- A manager `NotAuthorized` on any route that acts as the signed-in user answers 403
  `session-not-administrator` AND ends the session: carry it out of the blocking task as its own
  variant and answer it in the shared `model_profiles::not_administrator`, which is also
  `read_answer`'s arm, so the key string lives in one place. A 403 built inside the task keeps the
  session alive, which a test pins by a following `GET /api/session`.
- No `assignments.ini` at all means no instance is given the model profile: the save answers `restart: []`
  with `running: false`, asks no state and no `list_instances`, whether or not the body wrote
  (`save_no_changes_with_no_record_restarts_nothing_and_asks_no_state`). The earlier
  `None if !changed => client.list_instances()` arm is ruled out (slice 15 "As built"): the save's limit is
  never spent on the state of an instance the record never named.

### W1 (2026-11-23): the write timeout on every bff write route

- One rule, one mapping: `server::timeout_answer(state.timed_out(), done)` is the 3-arm answer of a write
  route's limit at all 12 sites (Cancelled 503 `client-unavailable`, Asked 504 `client-timeout`, Done the
  route's own success). A single-call write gates INSIDE the blocking task, after the connect
  (`if !mine.may_write() { return ..; }`) and marks `mine.landed()` after the manager call; the handler's
  `Err(_)` arm is the only arm that changes. The `Ok(Ok(Err(failure)))` arm keeps `Unavailable` at 503:
  four shipped tests answer 503 FAST (`instances.rs` ~1637, `server_profiles.rs` 827-858, `control.rs`
  973/1354, `model_profile_create.rs` ~985), so a fake failure is not a slow call.
- `done` is built from what the route asked for, never from a read it can no longer make: a create's 201
  carries the id the body sent, the boot write answers the flag the body sent (its `boot_enabled` read-back
  is out of reach), a removal's or a verb's 204 the plain 204. `boot_write` makes TWO manager calls and the
  fake logs the read-back too, so a 504 test's log check sees only the write line while the fake sleeps
  past the deadline; the `may_write` early return's value is never read (return the body's flag).
- W1 changed NO bound: the control verbs ride `timeout` (200 ms in the test builders), not `write_timeout`.
  `tests/write_timeouts.rs` straddles them (TIMEOUT 200, WRITE_TIMEOUT 600, BLOCK 1000) and holds under
  either. Every "slow manager is 503" test left in the suite is a READ (`grep -n "does not answer in time"
  crates/bff/tests/*.rs`: `control.rs` 1262, `server_profiles.rs` 827, `instances.rs` 718/736), and the
  503 half of each W1 test (connect paused past the limit) is GREEN before the code, the guard.
- Proving the reds with a tester's UNTRACKED test file: `git stash push -- crates/bff/src` and rerun, then
  `pop` and `diff -q` every file against a snapshot. Take the snapshot AFTER the last `cargo fmt`: fmt
  normalizes a call's arguments and the `use` list order, and a pre-fmt snapshot reports a false MISMATCH
  on pop (the tree moves mid-task: the pop also carried in other agents' `fe/` edits, which are not yours).

### W2 (2026-11-23): the control writes under the write limit, a lost manager after the hand-over is 504

- `control::routes(connect, timeout, write_timeout, sessions)` is the last group that took one limit:
  `act`, `boot_write` and `tags_write` take `write_timeout`, `boot_read` keeps `timeout`, and `lib.rs`
  hands the pair. A route's doc has to say which limit its calls run under once the two differ.
- The mapper rule is one argument: the handler passes `state.sent()` as `write_asked` and the mapper's arm
  is `ClientError::Unavailable if write_asked => 504 client-timeout` above the plain 503. `sent()` is true
  ONLY in `WRITE_ASKED`: a write that `landed()` is Done, so the boot write's read-back dying is the read's
  answer, never a 504, and a failed connect (Idle) stays 503. Put the arm in the shared mapper
  (`action_failure`, `write_failure`, `create_failure`, `delete_failure`) and forward `write_asked` from the
  wrappers (`tags_failure`, `server_profiles::save_failure`), so no wrapper re-decides it.
- W2's "the existing tests stay green" is unachievable at the four sites W1 named as the FAST-503 pins, and
  no code answers both: the six tests that pin 503 for a call answered `Unavailable` after hand-over
  (`control.rs::a_manager_gone_is_unavailable_and_a_slow_one_times_out` and `the_boot_write_errors`,
  `instances.rs::the_instance_create_answers_the_refusals` and `the_instance_remove_answers_the_refusals`,
  `model_profile_create.rs::no_manager_is_unavailable_and_a_slow_one_times_out_on_both_routes`,
  `server_profiles.rs::no_manager_is_unavailable`) are the same route, the same fixture shape
  (`create: Err(Unavailable)` / `app_crud(Err(Unavailable), ..)` / the verb and `set_boot` slots), opposite
  status, as W2's `*_the_manager_is_lost_after_taking_is_504`. Report the six as the tester's, changing
  nothing: in `server_profiles.rs::no_manager_is_unavailable` only the create and delete rows move, its
  `profiles`/`read_fail` rows are READS and stay 503.
- The base reds, proved in a `git worktree add --detach /tmp/<x> HEAD` with the tester's tests copied in
  (fresh `CARGO_TARGET_DIR`, `PRIVATE_AI_FE_DIST=<worktree>/crates/bff/fe-fixture`): 12 in
  `write_timeouts.rs`, 3 of them the W2 (a) successes the read limit cut off (verbs 204, tags 204, boot
  write 200) and 9 the (b) 504s, one per write route. All 40 green after the change.

## Slices L3 / L4 (2026-11-22): a key onto the web, and the device bound

- A key that moves INTO `WEB_MODEL_PROFILE_KEYS` reddens the bff's exhaustive off-list test:
  `model_profile_save.rs`'s `save_key_outside_the_web_list_is_400_and_reaches_no_manager` names the key in
  its array (comment and all), and no production rule answers 400 and 200 for one body. Grep the bff tests
  for the key name BEFORE promising a key move green, and report the stale array entry.
- `MODEL_PROFILE_INT_KEYS` is a const-generic array: the length literal grows by hand beside the new
  entry. `config/src/form.rs`'s `int_fields` reads its ranges and lists only the GUI's TEXT int fields,
  so a key whose GUI control is a SpinBox int with a `<name>_auto` box (`n_cpu_moe`) needs no form entry
  and `every_integer_field_is_checked` (which walks `ui/types.slint`'s `_default` companions) stays green.
- Device names in the device bound are EXACT names compared case-insensitively, the rule of
  `ggml_backend_dev_by_name` (`striequals` over `ggml_backend_dev_name`,
  `ggml/src/ggml-backend-reg.cpp:345`): there is NO index matching, so `ROCm1` is outside
  `Devices = ROCm0,CUDA1`. A fixture that reads "a `ROCm0,CUDA1` profile where `ROCm1` passes" is
  counting indices, and it collapses into the phase beside it that pins an already-outside device as no
  news: the same shape, opposite expectation, one rule.
- The manager refuses a `[*]` section in a model profile file: `checks::model_profile_sections` runs
  `model_profile_id` on every changed section and `*` is outside the id charset, so
  `write_file("models.ini", ..)` with a `[*]` answers `invalid-model-profile-id ["*"]`. A `[*]` cascade
  therefore exists on disk only from a write the manager did not do, and the cascade judgement is reachable
  only on the paths that read the pool FROM DISK (`set_model_profile`, `set_server_profile`, an in-use
  server profile save, a `models.ini.disabled` save), never at the `models.ini` write that carries it.
- Proof of a fixture defect, with no test file touched: `cp <file> /tmp/opencode/<x>.bak`, patch by exact
  text in python, run, `cp` back, `diff -q` the pair. A refusal's own `kept(&before)` then trips on your
  scratch write, which is itself the proof the verb wrote nothing: read that panic as the assertion above
  it passing.

### Slice AN4 (2026-11-22): `/api/version` and the node's host facts

- `crates/bff/build.rs` embeds the web UI: the directory is `PRIVATE_AI_FE_DIST` when set (absolute,
  `rerun-if-env-changed`), else `fe/dist` when it holds an `index.html`, else `fe-fixture`. It exports the
  choice as `PAI_BFF_FE_DIR` (`cargo:rustc-env`), and `tests/fe.rs` compares the served bytes with THAT
  export, so with `fe/dist` present (another agent's `npm run build`) a run reads that. Pin the fixture for
  a whole-crate run with the SELECTOR, not the export (setting `PAI_BFF_FE_DIR` in the env does nothing):
  `PRIVATE_AI_FE_DIST=$PWD/crates/bff/fe-fixture cargo test -p pai-bff` (25 binaries, 676 tests).
- The bff's test builders hand the router a `timeout` of 200 ms, so a route bound by a LONGER limit (the
  node's info, `NODE_INFO_TIMEOUT` 10 s) must use its own const: `tests/node.rs` uses the 200 ms builder,
  and the fake's `node_info()` sleeps, so a route reading `timeout` reddens on the bound and a blocking
  sleep in the async handler (no `spawn_blocking`) stalls the timer of the bound test next to it.
- A polled read (idle deadline unmoved) that acts as the signed-in user needs the session's ID to end it on
  the manager's refusal: `Sessions::peeked` answers `(id, identity)` and `peek` maps it, so the deadline
  rule stays in one body.
- `/api/version` now answers `{version, name, os}` and reads os-release itself (`pai_core::host::os(root)`,
  `name` = `node::name(&listen::hostname())`, the same value `/api/node` answers). Its root is a builder
  parameter (`router_with_session_and_roots`), and `tests/instances.rs` and `tests/model_profiles.rs` pin
  the document against the REAL `pai_core::host::os("/")`, so every shipped builder keeps `/`: a builder
  handing another root reddens those suites. The manager's row and text bounds are the only caps
  (`NodeInfoReply`'s doc says so): the bff renames, sorts, caps and judges nothing.

## Slice L2 (2026-11-22): `GET /api/profiles/model/{id}/model-info`

- A route that must answer the manager's KEYED refusal beside the shared read failures: let the blocking
  body return `Result<Value, Response>` and hand that to `read_answer`, whose `answer` closure matches the
  inner `Result` (`Ok(doc)` / `Err(answer)`). `Result<Value, Response>` passes `clippy::result_large_err`
  (an enum carrying a `Response` does not), so no box and no `type Outcome` alias are needed.
- Relay the refusal's own `key` and `values` (`keyed(status, key, args)`, the shape `instances.rs` and
  `server_profiles.rs` each keep privately): the manager's `model_info` answers one keyed refusal,
  `no-model-profile-named`, whose arg is the id as the page spelled it, `*` included. The module's own
  `not_found` cannot stand in: it runs the echo rule of `checks::model_profile_id`, so it drops a `*`.
- Brief 19's failure line for this route (503 `session-unavailable`, 504 at 5 s) is stale: every `As` read
  of the bff answers 503 `client-unavailable` at `MANAGER_TIMEOUT` (5 s in the service, 200 ms in the test
  builders), which is what `tests/model_profile_model_info.rs` pins.

## Reading the host (core `host.rs`, `versions.rs`)

- `nix` 0.31 `sys::utsname::uname` needs the `feature` feature (not `sys`, not `fs`): the module is
  `feature! { #![feature = "feature"] pub mod utsname; }`. `uname()` answers `UtsName`, whose
  `release()` is an `&OsStr` (`to_string_lossy`).
- The Windows version read is `RtlGetVersion` (ntdll): features `Wdk_System_SystemServices` +
  `Win32_System_SystemInformation` + `Win32_Foundation`; it takes `OSVERSIONINFOW` and refuses the call
  unless `dwOSVersionInfoSize` carries `size_of::<OSVERSIONINFOW>()`, answering `STATUS_SUCCESS`.
  `GetVersionExW` is in the same module and is the one the manifest gates, so it is not the read.
- Configuration Manager as `crates/app/src/hwmon/windows.rs` does it: `CM_Get_Device_ID_ListW` with
  `CM_GETIDLIST_FILTER_CLASS | CM_GETIDLIST_FILTER_PRESENT` fills a `vec![0u16; len]` with a
  NUL-terminated list of ids closed by a second NUL, so split on EVERY 0 and drop the empties, then
  `CM_Locate_DevNodeW` each id. A `DEVPROP_TYPE_STRING` property is read into a `[u16; N]` passed as
  `*mut u8` with its size in BYTES, and the out `DEVPROPTYPE` must equal the type asked for, else the
  bytes are no string.
- Chaining onto a temporary in a `?`-terminated statement is E0716 (`id.to_ascii_uppercase()
  .strip_prefix("PCI\\")?`): `let upper = id.to_ascii_uppercase();` first, then the chain.
- A script that moves a block and re-inserts it at an anchor can drop the item header that sat between
  them (the `mod tests {` of the module it left: `unexpected closing delimiter`). Anchor with the
  boundary lines in the asserted text, and prove nothing was lost with
  `git diff <file> | grep -c "^-[^-]"` answering 0 (every pre-existing line survives in order).

## INI, config, the manager

- `ServerForm` is a SLINT struct (`crates/config/ui/types.slint`), so a `ServerConfig` field
  the GUI has no widget for (the server `tags`) cannot be a Rust-only field: add it to the
  struct as the file's own spelling (tags as one `a,b` string) and carry it in both
  conversions, or the Server tab's whole-file Save erases the line. No lint requires a widget
  per form field (`ui_bindings` names the widgets it checks), and the form's dirty compare is
  a whole-struct `!=`, so a field set on `server_form` and `server_form_base` together is
  inert. `config_to_form` and `form_to_config` are exhaustive struct literals: a new
  `ServerConfig` field reddens both. A no-widget field needs NO GUI wiring: `gui.rs`'s
  `load_server_into_ui` fills the tab with ONE `set_server_form(form)` of the whole struct
  (R2's `Devices` rode it as a plain `string`, back through `opt_nonblank`, like `tags`).
  And the CLI keeps a key it has no flag for with NO production change: `ServerCmd::Set`
  loads the WHOLE config and `ServerSet::apply` copies only the flags that arrived, so
  `server set` preserves `Devices` as shipped (its R2 test was green before any edit);
  the tester's `devices: _` in `show_lines_prints_every_field` is the Show row's stub,
  so `server show` prints no Devices row in that slice.
- A new `[Server]` key the web UI carries is five places: the `ServerConfig` field, `from_keys`, a
  `render_for_os` slot (the `tags_line` pattern: set renders `Key = value`, unset renders `""`, so give the slot
  its own line and the unset render carries a blank), `server_cfg::check`, and ONE `FIELDS` row in
  `crates/bff/src/server.rs` (`field_of` needs no arm: its default answers `args.first()` for any `checks::KEYS`
  key, and the refusal's text and field come from there). Adding the key to the shipped
  `crates/core/defaults/server@default.ini` reddens `tests/server.rs`'s hash and last-line pins: an
  unset-by-default key needs no line there, and the bff README's `[Server]` key count is the `FIELDS` count.
  A free-text key needs the marker rule in TWO parallel lists, and the core marker test is TWO tests, one
  red per list: `checks::SERVER_TEXT_KEYS` (the manager's `server_file`, judging the raw line) and
  `server_cfg::check`'s `texts` array (the GUI, CLI and bff, judging the value as set). A key whose own
  rule judges a shape no marker can ride in (`Devices`: `device_list` reads `ROCm0;x` as ONE valid entry)
  is the case that shows it, the marker rule being its only refusal. Order is free: each test sets one key.
- `binding_lint::strings_t_keys_and_the_language_file_match` reddens the whole `-p pai-config`
  suite when a strings key is used only OUTSIDE `crates/config` (a `checks::KEYS` refusal key
  the manager and core name, e.g. `server-bad-tags`): the whitelist data (`GUI_ONLY_KEYS`,
  `CLI_ONLY_FNS`) is in the tester's `binding_lint.rs`, so it is a red to report, never a
  production change.
- `set_key` writes the canonical `key = value`. A test that builds its expected text by rewriting a
  COMPACT fixture line (`ctx-size=8192` to `ctx-size=16384`) is red by construction, and a needle
  `matches("ctx-size=")` counts a canonical line as zero: fixture and needle must carry the canonical
  spelling. Check such a claim mechanically, line-wise, allowing only `key=` to `key = `.
- Two assertions on the same slice can be unsatisfiable together (a `rest.starts_with('\n')` after the
  header, and a `trim_end_matches` of the rest, which leaves the leading break in place). Report the exact
  pair and the fix; the writer is judged by the assertions that CAN hold.
- `assert_eq!`'s message is a format string, so a `{` in a compared value prints DOUBLED (`{{`) in the
  panic output: a script that diffs captured `left:`/`right:` lines must de-double both sides.
- `parse_sections` keeps the key lines before the first section header nowhere, so a key list written
  without a header reads as empty: to read such keys, parse `&format!("[keys]\n{text}")` and take the LAST
  section.
- A section copied from a file whose last line has no ending carries no ending of its own: append the
  pool's `eol` after every section not ending in `\n`, else the next header lands mid-line and glues two
  values into one. An empty text reads as CRLF (the writer default), so take the style from the other file.
- An instance IS a section of `instances.ini` (`paths::instance_ids()`): the launch reads the profile it
  runs from that section's `ServerProfile`, the per-instance render lands only for such a section, and the
  "existing id" checks are valid-id plus "is a section". So the control verbs pass for a section whose
  profile file is missing (the launch is the one that refuses), and a fixture that "adds an instance" by
  writing `server@<id>.ini` alone lists nothing: report it, no production change fixes it.
- Nothing diffs a file the manager renders beside a served one: the served write always writes a temp file
  and renames over the target (new inode, new mtime), after every pool or assignment write and for every
  instance at the manager's start. So a comment change in a generated header lands with no compare, and a
  running instance keeps the copy it read at launch until it restarts. The only byte compare in the write
  path is over the client's file.
- One shipped engine default file is six places: `crates/core/defaults/<file>.ini` and
  `defaults/windows/<file>.ini` (when Windows differs), `linux/system_files.rs`'s `files()` and
  `copy_missing_defaults()` (0644, copied only when missing: covers the `.deb` and the `.rpm`),
  `msi::engine_defaults()` plus a `<Component>` and its `<ComponentRef>` in the `.wxs`
  (`NeverOverwrite="yes" Permanent="yes"`), and the install and repair lists in
  `.github/workflows/installers-windows.yml`, pinned again by `xtask/src/ci_windows.rs`, which reads the
  workflow as text and wants every shipped default quoted there. The workflow and the systemd
  unit are tests' fixtures read as text: an ordering line is a plain `After=`, and `After=` alone is what
  the test accepts.
- The Linux `server@default.ini` is pinned a third way: `crates/bff/tests/server.rs`
  `the_revision_is_the_keyed_hash_of_the_text` compares a keyed SipHash vector of the file's BYTES, its
  last line and a no-`StateDir` line scan, so dropping a key line is a byte-exact job (LF, the whole
  line, no blank left at the tail), and that bff suite is the check beside `xtask`'s.
- The engine's build revision is `components::LLAMA_CPP_REVISION`: a bump moves the `.deb` Depends and the
  `.rpm` Requires and the version fixtures of `xtask/src/linux/package.rs`. The MSI's minimum is the
  release alone, so `windows/package.rs` does not follow.
- `components::of` is a PREFIX ladder: a rule for a file inside a folder another rule already owns must sit
  ABOVE that folder's rule (each `versions/<package>` goes to the package its own name names, ahead of the
  rule giving all of `usr/share/private-ai/llama.cpp/` to the engine), and a name that matches no
  `Component::package()` falls through to the folder's owner.
- Pin a file that must land in exactly one package, and be taken by rpmbuild, by reusing the test module's
  own machinery: write the staged files, `components::split(&stage, &roots)`, then
  `rpm_spec(&fields_v(component, version, &[], None), .., &tree)` + `check_files(&spec, &tree)` and
  `files_section(&spec).iter().any(|l| covers(l, &path))`. `fields_v` carries the version, so the file's
  content is pinned against the same version the package gets.
- `windows/package.rs`'s `bundled` map is keyed by the DIST LABEL (`"ROCm"`, `"CUDA"`, `"Vulkan SDK"`), not
  by a component: a per-runtime artefact (its version file) is written when `bundled.get(label)` is
  non-empty and named by `versions_share::runtime_package("<sdk>")` (the component's suffix: Windows
  builds no `Component`). A dist outside the `Dists` layout answers
  `versions_share::dist_version == None`: write the empty file (the reader answers no version) rather than
  skipping the package.
- Prose in a constant whose bytes are DATA is data: the disabled-file header lives in TWO byte-identical
  source constants (the app's writer and the bff's copy), and the bff's test reads the app's source at the
  literal `const DISABLED_FILE_HEADER: &str = "` and unescapes it, so a text edit changes both sources and
  keeps that declaration spelling. A word change inside it reddens a test in another crate: revert to
  `git show HEAD:<path>` in BOTH copies, and keep the revert as a literal string, not a rule (a scripted
  rule that keys on a word inside the constant re-corrupts the fix on the next pass).

## The manager's verbs, refusals and records

- The manager's e2e phases are `pub(crate) fn` called in sequence by ONE `#[test]`
  (`crates/manager/src/tests/mod.rs`'s `data_root_e2e`), so a filter naming the phase
  (`cargo test -p pai-manager manager_model_info`) runs 0 tests and says `ok`: green over
  nothing. Filter `data_root_e2e`, and read the panic's LINE to find which phase is red, since one
  panic hides every later phase.
- The GUI is the same shape: `src/tests/save_flow.rs` exposes no `#[test]`, its `run(&app)` is called by
  ONE shared `#[test]`, `tests::ui_bindings::editable_widgets_track_model_after_edit`, so
  `cargo test -p pai-config save_flow` runs 0 tests over lib, main and `tests/gui_binary.rs`. Before
  calling a filter green, `grep -rn "<module>::run" crates/config/src/tests` for the caller and filter
  on the test that calls it (16 s, one window).
- `pai_core::gguf_shape::shard_set` is name math (no disk read), so a shard-count bound is judged
  from it BEFORE any `metadata`/read of a shard; the cap is the manager's read, not the reader's.
- A refusal a client test decodes must be a key of `checks::KEYS` with a `text()` arm: `Refusal::decode`
  answers `Other` for any other key, so a new key added only to the strings files reddens the D-Bus and
  pipe round trips. `Refusal::new` is private and its key is `&'static str`, so manager code builds the
  struct literal. Its values are `&[&dyn Display]`, so a `&str` binding needs the DOUBLE borrow
  `Refusal::new("k", &[&value])`: one `&` is `&[str]` and E0277 says "the size for values of type `str`
  cannot be known".
- The wire has no error name the manager cannot answer: the D-Bus and pipe replies are exactly
  `error_reply`'s `ManagerError` kinds, so a client test that feeds an `org.privateai.Manager1.Error.X`
  reply X no variant produces is unreachable (`Unavailable`: a stream that breaks is the CLIENT's own
  `ClientError::Unavailable`, never a reply), and `client.rs`'s passing
  `any_other_error_name_is_other_and_keeps_name_and_message` pins every unmapped name as `Other`. Quote the
  variant list and the test line in the report.
- A D-Bus signature pin is zbus's OWN rendering: `Signature`'s `Display` writes a struct with NO commas, so
  the assertion is `"(ta(ssstt)tu)"`. The comma form `(t, a(ssstt), t, u)` is doc prose, and a test that
  pins it can never go green; quote the sibling pin of the same file (`"(ssstt)"`) as the proof.
- A wire change that makes new fields REQUIRED reddens a sibling test in the SAME file whose fixture omits
  them (the pipe `an_acting_client_asks_the_devices_for_the_signed_in_user` answers
  `{"probed_at": 0, "devices": []}` where `a_devices_row_of_the_wrong_shape_is_other` demands `Other` for a
  reply with no `ram_mib`). Nothing in the code answers both: quote the pair, keep the strict parse.
- The host's own reading (R1, `pai_core::host::ram_mib()` / `cpu_threads()`) is read ONLY in the
  production seam, never in a fake, so a test that hands its fake an inventory sees its own numbers. A test
  that calls the REAL seam and compares a whole `NodeDevices` (`tests/gpu_probe.rs` `devices_of`) has no
  literal that fits: `ram_mib: 0` reds on any machine with RAM. Report it, do not widen the literal.
- A widened GPU probe caches the parsed rows, not a reduced pool, and its cache rule is what the tests pin
  ("As built" (c) of the GPU distribution design reversed the first rule): cache only a probe that parsed a
  device line of the node's OWN (`!device.is_cpu()`); a CPU-only probe answers `(Some(at), [])` and caches
  nothing, so the node is read again at the next call, while a killed, silent or no-device probe answers
  `(None, [])` and caches nothing. `probed_at: None` with an empty list is the "no probe" answer, so a fake
  that answers `(None, [])` cannot be compared against a row list; the gate for a read that can prompt for
  nothing is the pair check alone (bff account and a current `pai` member, membership read per call,
  `asked_quietly` staying empty), and `principal: None` takes the quiet `control` check.
- A serial probe the callers queue on answers the QUEUED readers with the probe they waited for, failure
  included ("As built" (d)): a `Mutex<(u64, ProbeAnswer)>` attempt counter read BEFORE taking the serial
  lock and compared, once held, with the counter of that moment, so a reader behind a killed probe takes
  `(None, [])` and pays no second bound (the test asserts one exe run, both answers, and a total under
  `bound + 1500 ms`: a re-probe after a failed one lands at 2 x bound). Keep the cache read inside the
  lock as the first gate, so the successful case never touches the counter.
- The startup budget of an instance is checked with the EXISTING rule, not a new one:
  `checks::startup_loads` takes the RENDERED per-instance file (the sections that carry
  `load-on-startup = true`) and `checks::startup_count` the count against the server profile's `ModelsMax`,
  so judge the render the write produces, with the record's own key. Its refusal values are
  `[count, max, instance]`: a test comparing `values == ["2"]` compares the max.
- A server profile switch is NOT the step rule: it changes no model profile, so the whole count is judged
  against the NEW profile's cap (a step down to a lower cap is refused). Judge the render cut by the
  `assignments.ini` the call read with its `on_disk` closure, never by `render_of` (that one swallows a
  read failure with `.ok()`, so a broken record reads as an empty one and the switch lands).
- R3's pair memory check: at the shipped points `check_render_memory` measures
  `devices_usable(pool, instance_devices(id))` (core's pure rule, `""` = the whole pool, `none`/unknown
  ids = 0 = skip); the two shrink points (`set_server_profile`, and a `write_file` of an in-use
  `server@<p>.ini` whose `Devices` differ from the file on disk, compared as `from_text().devices`
  Options so a no-change save judges nothing) refuse only `need > new && need <= today`, the growth rule
  turned round, so a profile already over rides through. Gate `none` on either side BEFORE asking the GPU
  seam (the probe can run 30 s and answers nothing a CPU-only profile needs), and build the render from
  the `assignments` the verb read, as `check_render_of_full` does. The shrink check asks the seam the
  same way `check_render_memory` does: render + `estimated_of` first, `return Ok(())` when
  `needs.is_empty()`, only then `gpus.gpus()` (R3-PIN-10/11 count the seam's own `asked` log from a
  base read AFTER the manager is built, so a probe at construction is not counted).
- The per-instance render is the launch's real file (`assign::render`): judge a record with
  `checks::model_profile_conflicts` on that text, and skip the startup budget for an instance with no
  `server@<id>.ini`, as `check_renders` does: the launch refuses it and no render is judged on defaults of a
  file that is not there.
- The action log's absent principal is the BARE `-` (`record_action`'s `quoted(None)`), which the method's
  own doc comment, `action_log.rs` and 23 assertions in `service.rs` pin. A new test asking for
  `principal="-"` asks for a rule change, not an implementation: quote it and every one of those goes red.
- The `ModelsMax` step rule: judge `startup_count` only when the render the write produces starts MORE than the render on disk, and skip the judgement wholly when the section names no profile or its file is missing/unreadable. Pass the cap as `Option<Option<i32>>` (`cap`, judged only in the `if let`) so "no cap to judge" never rides on `None` (which means the engine's default), and take the current count from `render_of` (it reads the record from disk under the write lock, so it needs no argument).
- A record write (`assignments.ini`, `instances.ini`) goes through `write_served_file(..., &ConfigFile::Models, ...)`: mode 0644 whatever the umask, temp file renamed over the target. `ini::atomic_write` lands 0600, and the e2e pins the mode over a 0600 seed with the process masked `0o077`, so the swap shows as one red mode assertion.
- A "the profile my own section names is no conflict" answer must come BEFORE the address walk: two sections naming one server profile make the other instance's port look taken (`AddressInUse`), so a later check cannot rescue the early one.
- `ini::detect_eol` is `pub`: a writer outside core that appends a section writes it in the file's own style. An empty text reads as CRLF.
- The record's `append_record` styles ONLY its leading break (the design's "a newline first"), never the
  section it appends: every byte-pin of a created instance is LF (`"[pipe-i]\nServerProfile = pipe-i\n"`,
  and the empty-record `"[spared]\nServerProfile = free\n"`), and the whole-section style of the sibling
  `with_profile` is the shape that reddens them, because `detect_eol("")` is CRLF. The text the writer
  needs is the `record` the verb already read under `writing`, so pass it in, no second read.
- The manager's create/remove verbs have no test of their own: they are phases (`manager_instance_crud_e2e`,
  `manager_dbus_instance_crud_e2e`) of the ONE `tests::data_root_e2e`, so the filter is `data_root_e2e` and
  a `create_instance` filter runs 0 tests. The fakes' `stopped`/`running` builders and the `want` binding
  beside them are `#[cfg(unix)]`-gated (their only callers are the Linux/unix phases of the e2e), so the
  windows-gnu `--all-targets` clippy is green.
- `set_key` and `remove_key` already keep the section's trailing blank separator line: a writer that trims the section tail is the one that eats it. If a brief pins "every other byte kept", drop the tail trim, and expect the older assertion in the same test function (the `none` path, section text pinned without the blank) to go red: PIN-4a and that one are unsatisfiable together. Report the pair, keep the brief's rule.
- A `models@<id>.ini` render is written after the record change and its failure is the CALL's answer, so a
  verb's doc states which one the caller sees; the renders go through `render_all_locked` (no lock
  recursion) and a lock taken `unwrap_or_else(|e| e.into_inner())` is a documented pattern here.
- `assign::render` takes BOTH pool files, so a profile sitting in `models.ini.disabled` IS in effect on
  every instance whose section says `on` for it (an explicit `on` beats the file it lives in). Consequence
  the tester's fixtures do not agree on: a write of the disabled file that ADDS a profile with a missing
  `model` while the record keeps it `on` adds a missing pair to that render, so the delta call refuses it,
  and the block "a switch off never fails a resource check, whatever the file" (its pool has the profile in
  NO file today) is unsatisfiable with S9C-PIN-1 (same operation, profile already in that file, expected
  refusal). The pair to report; only a narrower rule ("judge a profile only if the file being written
  already holds it") gives both, and that is the tester's call, not the coder's.
- A bounded probe that drains its child's pipes: `Child::wait_with_output()` after `child.stdout.take()`
  answers an `Output` with EMPTY vectors, so the caller builds `Output { status, stdout, stderr }` from
  `try_wait`'s `ExitStatus` and the readers' bytes. Each reader runs to EOF (a cap drops the extra bytes,
  it never stops the read) and SENDS its bytes over a `std::sync::mpsc::channel`: a `JoinHandle` join is
  unbounded, and a grandchild of the child (a `sleep 600 &` of a shell script) inherits the write ends,
  so there is no EOF after the child exits and an unbounded join hangs the caller past its own bound.
  Once the child is gone each stream is taken with `recv_timeout(PROBE_READER_GRACE)`: a Timeout drops
  the receiver (the reader thread ends when the pipe closes) and takes that stream as EMPTY, a
  Disconnected (the reader died without sending) is the probe's `None`, and the kill path drops both
  receivers without waiting at all (its answer is `None`). Two consequences: the probe's worst-case cost
  is the TWO graces SUMMED (the streams are picked up in series), and the bytes a reader already kept are
  LOST at an expiry (it sends only at EOF), so a probe whose grandchild holds the pipe answers nothing
  for a block it had read.
- The model-file check (`check_model_files_of`) is a DELTA: it refuses the first (model profile, path) the
  new render is missing that the same instance's render today (pool pair and record on disk) is not
  already missing, in id order; the pairs compare whole, so a `model` pointed at a new path is a new pair.
  Pass `today` in as the render text: `render_of` reads the record from disk, so the caller supplies the
  pool pair of the render today and the record needs no argument. Consequence to report, not to code
  around: a save whose text equals the file on disk adds nothing, so it lands, and every fixture that
  saves the same pool twice with the model file deleted in between expects a refusal no rule that honours
  ARCHITECTURE.md (b) ("never re-judged", "a `*` narrowed never fail") can give.

## Renaming a word across the tree

- The ARCHITECTURE.md names table is the authority. Enumerate every site by grepping the literal before
  changing anything, then decide per area.
- A word-internal map breaks Rust in two specific ways. `mod presets;` is the FILE name, so renaming the
  declaration while the file keeps its name is an E0583 file-not-found; `use crate::presets;` is a module
  path, so the rule must be guarded with a `(?<!use )(?<!mod )(?<!::)` lookbehind. Check every `mod` and
  `pub mod` line by hand at the end (`grep -rn "^\s*(pub )?mod [a-z_]+;"`), reading each declaration's
  file, not its name.
- A multi-item `use crate::{a, presets, b};` binds the OLD name, so the bodies that say the new one fail
  E0433 while the import fails E0432. That mix (a few E0432, many E0433 naming the new name) is the
  signature: rename the item inside the list, in its alphabetical place. Grep for the item as a list member
  (`[,{][[:space:]]*<word>(,|[[:space:]]*\})`), not only for `use` at the line head.
- A name interpolated in a format string breaks the STRING, not the code: `format!("  {PRESETS} ")`
  becomes `{model_profiles}`, which reads as a struct field (E0609). Check every `{NAME}` with
  `grep -rn "{[A-Z][A-Z_0-9]*}"`, including the test text that quotes the string.
- A word-internal map over a file with a raw or multi-line string rewrites it MID-STRING (a token inside a
  raw string that seeds a fake INI in `app/src/bench/exec.rs`; a "no multi-line string in file" guard hid
  it). When the guard fires, rename that file with exact-text edits and let the red tests enumerate the
  sites.
- The production region of a file ends at the FIRST `#[cfg(test)]` line, not at `mod tests {`: many modules
  open with a `#[cfg(test)] mod fake` and keep production functions below it, so a whole-file script that
  renames a file name edits their test data too. Cut at the first `#[cfg(test)]`, and let the red tests
  enumerate the test-region lines.
- Check the `.slint` files before renaming anything the GUI binds: a name absent from the `.slint` stays
  in the Rust that binds it, and the Rust adapts around it. The `.slint`'s own property and callback names
  are the ground truth for a stale comment pointer.
- A WiX `Component Id` is MSI identity, not prose: harvested components derive their GUID from Id+path, so
  renaming it re-GUIDs the component. Words in a `.wxs` comment are prose, and a brief pinning "every other
  byte unchanged" keeps them as they are.
- Comment prose: apply the map to the comment text, NOT to the tokens the map is not about. Mask the code
  spans first (`` `...` ``, `snake_case`, `CamelCase`, `PascalCase::`) with a placeholder, rename, unmask;
  then drop the words that name an upstream llama.cpp key, a retired CLI name in a deprecation note, and a
  `.slint` name that still exists in the `.slint` (such a note follows the `.slint`, not the Rust).
- Rewrapped prose defeats exact old-to-new pairs and a global word map breaks words ("preset's", "preset
  files"): rework comment prose by WHOLE BLOCKS, exact `indent + text` of the block (the `//` prefixes
  stripped) mapped to the reworded block, longest key first, assert exact match, print the misses, rerun
  until the list is empty.
- The comment pointers to a renamed identifier or file go in the same slice (a stale pointer is a defect
  the rename introduced) and only those: replace EXACT old tokens (longest first, word-bounded) in the text
  after the `//`, never prose, with a KEEP list for names that stay and a printed list of every rewritten
  line. Doc comments that NAME a symbol an earlier slice renamed read as prose and their target is gone:
  grep each backticked name in the code before keeping a comment.
- Width when reflowing comments: `.slint` comments cap at 80 columns, and a box heading
  (`// ──── STATE ────`) keeps its ORIGINAL rule width, its two rules aligned to each other: compute the
  fill from the original total, never from 80. Only reflow the blocks you touch.
- Proof a pass is comment-only: every added diff line, trimmed of `&*—-`, starts with `//`. The Slint
  binding lint strips whole-line comments, so comment edits cannot trip it; the file still compiles, so the
  crate's tests rerun as normal.

## Web UI

- The default-box mechanics (a ticked box means "absent from the file") are shared in `fe/src/boxed.ts`. A
  page keeps `boxed: Record<string, Boxed>` beside the draft of the selects and derives dirtiness AND the
  PUT body from ONE `changed()`, so what is sent is exactly what is counted as changed.
- The save's per-field checks run BEFORE the `!dirty` guard.
- A rule that spans a group goes as data on the row (with the reason text resolved beside it), and rides
  the field's existing lock path, not a new one. Such a rule reads `loaded`, never the draft: a group that
  opens from a draft edit leaves fields editable while the governing key never reaches the wire, so a
  draft-based lock marks a row editable that the send gate then refuses, and a group that closes on a draft
  edit blocks a row the user just typed.
- The send rule of a checkbox row is DATA on the row (`onSend`/`offSend`, required beside `absent`), and
  the draft holds the box state as the strings the reading helper returns. A select made editable must take
  the loaded value when it is one of the row's OWN options first: the flag reading answers `on` for any
  value outside FALSEY, so a tri-state row's `default` reads as `on`, picking `default` is no change, and
  no PUT goes out at all.
- A value two pages read lives in ONE exported function both import. Match list items with `includes`, not
  a substring test: a substring test makes `ngram-map-k4v` answer `ngram-map-k`.
- A helper shared by two pages takes each caller's rule as a named parameter, never one page's rule.
- "enabled" is not a DOM property of an input, select or textarea, so `el.enabled` is `undefined` and a
  guard built on it is always true: read `.disabled`. A parameter named only in a type annotation is
  undefined at run time when the caller does not pass it, and `noUnusedParameters` does not catch a missing
  argument, so a new prop needs the argument at each call site.
- An `id` of the form `<key>-box` / `<key>-control` is not always in the DOM: a row whose `group` is `null`
  renders a plain labelled input, a select renders as a `select`, and a field locked by an unticked box has
  its control in the tree but disabled. Key every per-field text off the card's data and check the
  control's `disabled`; never assume a box.
- An editable text field with no default box is a plain labelled field, NOT a `role="group"` named by the
  label, and the shared field's `group` prop is dead once its read-only-only caller is gone. So a test that
  names a group with its label passes only while the field is read-only: read the brief's wording ("a plain
  labelled input") before building a group.
- A jump target is named by a slug carried in the card's own data (id helper + order from one table), and
  the `tabIndex={-1}` is a prop of the shared `Card` (absent means no id and no tabindex, so a page with no
  jump list keeps its DOM). The page then needs no ref, no effect and no index. Reaching every heading from
  one page effect needs a ref on a wrapper that CONTAINS the card (the card is rendered by the page, not
  inside the field component's output), and `:focus-visible` needs its own outline: the global
  `[tabindex='-1']:focus { outline: none }` strips the ring a test calls "marked visibly".
- The react-hooks rules of `npm --prefix fe run lint`: a state update that answers a prop change goes in
  the RENDER body guarded by a `useState` of the previous value, never in an effect, and a hook takes render
  values, never `ref.current`. For "the answer of a save reads a save in flight", pass `phase === 'saving'`
  and keep that phase on every edit path of the page.
- Vitest here has no `globals: true` and no setup file, so React Testing Library's auto-cleanup never
  registers. A test that renders the app twice fails `Found multiple elements with the role "main"` on the
  SECOND render, and a page that passed its whole body on the first is proof the production code is right:
  the fix (`cleanup()` in the shared helper) is the tester's.
- A new 422 refusal key needs ONE `case` in each of two tables, `ModelProfilePage.tsx` `REFUSALS.text`
  and `ModelProfiles.tsx` `failureText`, and nothing else: `refusalOf` takes the reply's own `field`
  (the `field` arm is only a fallback when the reply names none), and a field with no `<id>-control`
  in the DOM (the read-only Assets rows, and the Assets card starts closed) is dropped, so the bar
  shows the plain `save-refused` and no field is marked. `strings.test.ts`'s "every value is used"
  wants a literal `t('key'` in the code, so the table case is what makes the strings key legal.
- The tester's EN table in `fe/src/test/bff.ts` is a SECOND pin of `en_US.json`: `App.test.tsx`
  "1a strings: en_US.json holds the English the brief gives" compares the file against it, so adding
  entries there reddens App.test.tsx until the strings file has the keys (it is a baseline red of a
  strings slice, and the strings edit alone turns it green).
- `ModelProfilePage.test.tsx` "2d.63: once read, the h1 and the title take the id of the reply" is a
  focus-timing test that lost one full-suite run (parallel workers) and passed 4/4 in its own file
  and on a full rerun: report the flake, prove the tree green by rerunning, do not touch the page.
- Running a web UI test as `npx vitest run fe/src/<File>.test.tsx` from the repo root uses NO config (no
  jsdom): every test of a DOM file fails at 0 ms with `ReferenceError: window is not defined`, which reads
  as a total code break. The one path is `npm --prefix fe test -- src/<File>.test.tsx`. Its failures are
  `FAIL  src/File.test.tsx > name` lines: `grep -E '^ FAIL  src'` is the red list,
  `grep -E "Tests +[0-9]+ (failed|passed)"` the summary.
- `npx prettier --write` prints EVERY file it processes, changed or not: `git status --porcelain` after it
  proves no test file moved.
- When a test pins a CSS declaration's EXACT text (the tests' `prop()` trims only the ends, inner newlines
  stay), write it on ONE line with `/* prettier-ignore */` above it: the formatter breaks a long value and
  the pin goes red on the whitespace. `npx prettier --write <file>` answering "unchanged" proves the
  hand-written line is what the formatter keeps.
- CSS cascade: `input[role='switch']` and `input[type='checkbox']` tie at (0,1,1), so the rule written
  LATER wins per property: a switch track placed after the checkbox rules inherits its fills and overrides
  only the geometry. A `background` shorthand resets `background-image`, which is why a hatch declared only
  on the field and button rules cannot leak into the box or the track.
- A CSS test that pins a rule by its EXACT selector list and walks the cascade scans depth-0 rules only
  and skips a selector part containing `[` or `:`, so a body hidden by the `hidden` attribute must be
  reached by no top-level `display` rule (`[hidden] { display: none !important }` is the only one allowed).
  Mirror the test's own readers (rules from the brace balance, `prop()`, specificity, the chain match) in a
  scratch node script and run it on the sheet: every claim shows at once, and it catches a rule the
  harness wrote TWICE, which the selector-count pin reads as a red.
- `useWarnOnLeave` (SaveBar.tsx): ONE listener per dirty section. A page-level one in `App.tsx` on top of a
  form's own reddens every "beforeunload is armed only while the form is dirty" test: the two listeners go
  in TWO passive-effect cleanups and the test's synchronous `unloadBlocked()` dispatch lands BETWEEN them,
  so it reads the stale listener and "after a save" the browser is still asked. The section's owner arms its
  own (a `useState` of its dirty, not a ref), and `App`'s `dirty` stays what it was, for the nav guard.
- Report a section's dirty in the TURN that changes it: call the report from the field's `onChange` and
  from the save handler inside `flushSync`, keeping the effect only for a read that arrives on its own. A
  passive effect's `onDirty(false)` runs after the test's synchronous assertions, so a 204 leaves the page
  asking, and an effect that early-returns when its value is unchanged never reports at mount (which is how
  the page starts clean).
- `startAt` in `fe/src/test/bff.ts` is `history.replaceState`: the app keeps ONE history entry, so
  `window.history.back()` in such a test traverses nothing, fires no hashchange and a leave guard can never
  open from it. jsdom DOES append for a fragment navigation (`location.hash = '#/a'` grew 1 to 2 to 3), and
  a nav click a guard holds adds none. A test that navigates away, then calls `cleanup()` and renders again,
  gets the page at the hash the navigation left. Quote which of these a leave-guard test needs.
- `runStatus` (the same helper in `RunControls`, `Timeout`, `SessionRevoked`, `Boot`: the
  `before(s, h2) && !inBoot(main, s)` filter) pins exactly ONE `role="status"` between the h1 and the Server
  h2. A new status line the brief places in the header region (a field between Start at boot and the Server
  section, as brief 12 puts its tags field) reddens EVERY test that calls it, 54 in one slice. The fix is an
  `inTags`-style exclusion beside the existing `inBoot`, so it is the tester's, never a move of the field.
- `useWarnOnLeave` (SaveBar.tsx) takes ONE listener per dirty section. A page-level one in `App.tsx` beside the
  form's own reddens every "beforeunload is armed only while the form is dirty" test: the two removals are
  passive-effect cleanups and the test's synchronous `unloadBlocked()` dispatch lands BETWEEN them, so it reads
  the stale listener and the browser is still asked after a save. Each section arms its own (its own dirty in
  `useState`, not a ref); `App`'s `dirty` stays what it was, for the nav guard.
- Report a section's dirty in the TURN that changes it (from the field's `onChange`, and from the save handler
  inside `flushSync`), keeping the effect for a read that arrives on its own: a passive effect's `onDirty(false)`
  runs after the test's synchronous assertions, so a 204 leaves the page asking. An effect that early-returns
  when its value is unchanged never reports at mount, which is how the page starts clean (12 T1.13).
- `startAt` (`fe/src/test/bff.ts`) is `history.replaceState`: the app keeps ONE history entry, so `history.back()`
  traverses nothing, fires no hashchange, and a leave guard can never open from it; jsdom DOES append for a
  fragment navigation (`location.hash = '#/a'` grew 1 to 2 to 3), and a nav click a guard holds adds none. A test
  that navigates away, `cleanup()`s and renders again lands at the hash the navigation left.
- `runStatus` (the same helper in RunControls, Timeout, SessionRevoked and Boot: `before(s, h2) && !inBoot(...)`
  with `inBoot` scoped to the header) pins exactly ONE `role="status"` between the h1 and the Server h2. Any new
  status line the brief places in the header region reddens every test calling it (54 in brief 12). The fix is an
  `inTags`-style exclusion beside `inBoot`, so it is the tester's, never a move of the field.
- Making a read-only card editable turns red every fixture that pinned it read-only. The shared list under
  `fe/src/test/` spells the editable set and derives the tab stops from it, so the tab-order and "every
  control disabled" tests have ONE fix point there (an editable number field is TWO tab stops, its box and
  its input), and the fixtures pinning a full-value line must move to a field that stays read-only. Report
  the list, never edit the tests.

### The jump region, and a rule whose selector a test names

- A strings-file VALUE is the accessible NAME of a region: while `en_US.json` and the tester's
  `fe/src/test/bff.ts` EN table disagree, every `getByRole(..., { name: en(key) })` of that region is
  red, and the strings edit alone turns them green (H10 turned 21 baseline reds green). Check the
  tester's table first: it is the acceptance text.
- Renaming a shorthand rule to scope it (H10: `.jump button` to `.jump ul button`, so a new control in
  the same region keeps the global `button` + `.secondary` look) reddens every test that LOCATES the
  rule by its selector text (`flatBy('.jump button')`) and `HearthTokens`'s H1a.12 list of
  `selector: min-height: value` pairs, and its forced-colours twin must move with it (that block's
  comment names the rule it follows). The token guard COUNTS the forced-colours blocks: scope inside
  the existing one, adding a block is a red of its own.
- The specificity facts of this sheet: `button` and `.secondary` are (0,1,0), `.jump button` is (0,1,1),
  so an unscoped region rule beats the pill and the height; a `.jump .jump-toggle` override is (0,2,0)
  and beats the rail's `.jump-toggle { display: none }` (0,1,0), which is why scoping the entry rule is
  the only clean way. One declaration per property per rule: `border` and `border-width` in one body
  makes a later `border-width` dead (`declaration-block-no-redundant-longhand-properties`).
- State by CLASS, never the `hidden` attribute, when a wider window must override it: the hide goes in a
  `max-width` block that ends below the rail's `min-width` block, so the rail shows the list whatever
  the state is (the H10 tests read `className`, so `hidden` is red twice).
- jsdom `fireEvent.click` does not move focus the way a real click does, so "the focus stays on the
  control" needs `e.currentTarget.focus()` in the handler; a CSS-only sheet cannot satisfy it.
- A strings-key id makes a good `aria-labelledby` target (the label element carries `id={key}`): the
  repo's ids and keys match by convention. An element a sheet hides still names what points at it, so a
  hidden label keeps the region named.
- One new focusable control reddens, by one position, every "the tab stops are exactly" test in the
  region, the "the nav holds only its list" and "no new element, class or text" guards, and the app's
  own button-text list (`ModelProfilePage.test.tsx` 2d.67). Enumerate them, quote the brief line that
  asks for the control, change no test.
- A test file the tester owns can go back to HEAD under you mid-task (`git status` showed
  `HearthH2a.test.ts` clean after my edit of it). Re-check `git status` and the file text before
  trusting anything you wrote there, and write no test file at all.

## Brief 14 (2026-11-21): the GPU list, and what the UX check actually reads

- The UX check's summary prints only `axe: <rule> x<count>`, never which node. To learn the node, add a
  temporary loop in `fe/ux/check.js`'s `problems()` printing `v.nodes[].target` and
  `n.any[0].message`, run the check, then revert the loop (it is in `fe/.prettierignore`, and only your
  hunk may stay). A standalone axe script is NOT worth the setup: `NODE_PATH=fe/ux/node_modules` reaches
  `@axe-core/playwright`, but the page needs the stub's `__set?session=admin` fetch and a
  `waitForSelector` before it renders.
- axe's `aria-required-children` fires on a plain `<div>` child of a `<ul role="list">`, and only webkit
  reports it: chromium and firefox pass the same page. The fix is to leave the list a list of rows and put
  the buttons AFTER it (the shipped shape of GD1.4/GD2.4); `role="listitem"` on the div is the narrower
  fix (its role, not an `li` element, so a test that reads the list's rows as
  `[...list.children].filter(el => el.tagName === 'LI')` still counts only rows).
- A `<button>` with no class is a webkit `color-contrast` violation: the UA paints it grey (`#c0c0c0`)
  with white text at 1.81:1, and axe's own fix is the sheet's rule, so give the control the shipped
  `.secondary` (or `.primary`) class; the sheet already styles `[aria-disabled='true']` for both.
- The stub `fe/ux/stub.py` decides what the 320 px / 200 % pass ever sees: a control that only renders
  for a profile carrying certain `values` is unmeasured until the stub's `MODEL_PROFILE_DOC` carries them
  (the GPU list needed `split-mode`, `device` and `tensor-split` to reach the check at all).
- Vitest + RTL under `vi.useFakeTimers()` is a trap: RTL takes its non-jest `waitFor` branch, checks once
  synchronously, then polls on a faked `setInterval` and on DOM mutations. A render that needs a real
  macrotask never happens, so every `findByRole` on a fake clock hangs to `Test timed out in 5000ms`.
  A test that holds a request and advances the clock can only see what the flush after `advance` renders.
- The page has ONE page-level `role="status"` (the Save bar's alert) and `ModelProfileSave2` pins that
  count, so a "…ing…" line is a plain `<p className="muted">`, and an announcement line carries
  `aria-live`/`aria-atomic` ONLY while it has text: `ModelProfileContent` 2d.79 refuses any live region on
  arrival, and a second status reddens two shipped helpers at once.
- That count is a fixture guard in **27 files** (`queryAllByRole('status').filter(s => before(h2, s))`
  with `toHaveLength(1)`, the model-profile `h2` above every card, and the same helper in
  `fe/src/test/savealert.tsx`), so ANY `role="status"` a new card renders is counted. Brief 14 pins two
  in the GPU card (GD1.3's pending line, GD1.9's always-there move line), so the guard is what must
  narrow, to the bar's own `.save-status` (the harness, the tester's call). Ship the brief's region and
  report the count, do not trade it away: the guarded window is 1 to 2 microtasks (the fake bff rejects a
  `devices` route the test never asked, GD1.7's failure), so which old test lands in it moves between
  runs (28 to 30 reds of the same cause).
- An element in a sentence's `{0}` slot (GD1.7's `<time dateTime={probedAt}>{shown}</time>`) splits the
  paragraph's text, and RTL's `getByText`/`findByText` read DIRECT text children only: the `<p>` reads
  "Free memory as of ." and every test that locates that paragraph by its full sentence goes red, in a
  `ready()` helper of every suite, so one production line cascades (1 of 188 red became 169). Build it as
  `t(key, shown).split(shown)` around the element: the key stays pinned by `strings.test.ts`'s `\bt\('`
  regex (a `tAround(key)` helper reddens "values no code uses"), the parts are JSX expressions so the
  "no component writes text of its own in JSX" lint passes, and the paragraph's `textContent` is exactly
  the filled sentence. Locate such a paragraph by the slot's PREFIX (`/Free memory as of/`).
- When the tests AND the component they pin are untracked (`?? fe/src/GpuSplit.tsx`), a `git worktree`
  baseline has NO tests at all ("No test files found") and is no baseline: copy the working tree
  (`rsync -a --exclude build --exclude .git --exclude node_modules --exclude dist`, symlink
  `fe/node_modules`), restore the OLD production in the copy by exact-text python, run there, and the
  red list of the copy is the inherited one (there: 1 red at `ModelProfileGpu2.test.tsx:382`).
- Prove a stub's shape, not the page, is red: copy the file to `fe/src/Zz*.test.tsx` with the stub's
  reply fixed (GD1.6, GD1.8 and GD1.10 went green with `running: false` added), run the copy, then
  DELETE every `Zz*` file and show `git status --short fe` clean of them.
- One label, two names: `nameOf(checkbox)` is the whole label text, so a row labelled `${id} ${name}`
  answers a pin of the bare `id` list with the name appended. When one test lists the rows by name and
  another matches them by `id` prefix, the prefixed form satisfies both; the bare form satisfies neither.
- `Sign out` is in the banner (`SessionBar`, rendered by `App.tsx`), OUTSIDE `<main>`, so
  `within(main).getByRole('button', { name: en('session-sign-out') })` finds nothing for any page code.
- The card order is pinned from three directions at once: the jump list's neighbours, the "middle card"
  index of `ModelProfileSave16` 2h.196, and `fe/src/test/modelProfileCards.ts`'s `MODEL_PROFILE_CARDS`.
  A new card FIRST (the page prepends Assets to `cardJumps()`) shifts every positional pin: recompute the
  indices, and report the ones no order satisfies.
- `npm --prefix fe test -- src/<File>.test.tsx` is the single-file form (npm sets the cwd to `fe`, so the
  jsdom config applies). `npx --prefix fe vitest run src/<File>.test.tsx` from the repo root picks the
  node environment and dies on every test with `ReferenceError: window is not defined`.
- The 2026-11-21 ruling SHIPPED the GPU card's move line as a permanent `<p className="visually-hidden"
  id="model-profile-gpu-moved" role="status">` INSIDE the GPUs fieldset, after the `ul` (GD1.4's place, so
  GD1.3's and GD2.9's `within(fieldset)` counts find it), and the detecting line as a plain `<p>`: carry NO
  `aria-live`/`aria-atomic` (role=status is the whole of it), because 2d.79's second assertion is
  `[role=alert],[role=log],[aria-live]` equals `[]`. The 27 fixture guards are narrowed to
  `.closest('.save-bar')`; the one the pass missed is `ModelProfileContent` 2d.79's own
  `within(main).queryAllByRole('status')` `toHaveLength(1)`, unsatisfiable beside GD1.9's always-there
  region (report, do not trade the region away).
- GD2.2 and GD2.6 are ONE rule over the same trimmed weight: gate the share on `GPU_WEIGHT_RULE`, never on
  `Number.isFinite` (JS reads a number out of `-1`, `1e3`, `+1`, `0x10` and `' '`, so a `Number` gate shows
  a share on a vector the check refuses, five reds), and read the check on the COMPOSED vector: one written
  weight makes it non-null, so an empty weight beside it fails too (the all-empty vector composes `null`,
  the one exempt state). `flat` (the `even`/`auto` word) is `known && sum === 0`: the rule's non-negative
  numbers make a sum of 0 exactly the all-empty-or-0 case.
- A live-region test that was red at its FIRST assertion never exercised its later ones: after the region
  shipped, `GD1.9: the last move only` surfaced an arithmetic slip (rows `ROCm0 CUDA0 Vulkan0`, Move down
  on `CUDA0` from position 2 lands at 3, the literal says 2). Prove the numbers in a copy
  (`rsync -a --exclude node_modules fe/ /tmp/<x>/`, symlink the repo's `fe/node_modules`, copy the test
  file to `src/<Name>Probe.test.tsx`, insert `console.info` of the row ids and the region text, `npx vitest
  run src/<Name>Probe.test.tsx`): captured stdout prints on the failure, so the probe lines are the state.
- `getByText(sentence)` reads ONLY the element's DIRECT text children (`@testing-library/dom` 10.4.2,
  `dist/get-node-text.js`: `childNodes` filtered to `TEXT_NODE`, joined). So a paragraph of
  `"Free memory as of " + <time>20 Nov, 09:14</time> + "."` is NOT findable by the full sentence, and an
  element whose text you must match exactly cannot carry a text-bearing child. GD1.7's two shapes:
  `<p>{t(key, shown)}<time aria-hidden="true" dateTime={at} /></p>` (empty `time`, the sentence direct in
  the `p`) answers `getByText(asOf)`, `line.textContent === asOf` and `datetime`; a `time` WITH text
  answers `time.textContent === shown` and makes `textContent` name the moment twice. The pair is
  unsatisfiable, quote both lines.
- The UX check's `expect[].text` is a Playwright `locator(selector).filter({ hasText })`, a SUBSTRING: a
  state marker for text whose tail is locale-dependent (a `Date.toLocaleString` in the strings file's
  `{0}`) is the string up to `{0}`, `S[key].split('{0}')[0]`.
- `overflow-wrap: anywhere` on a flex label splits a SHORT id inside its own word before the longer name
  beside it moves down (the label is one flex item): the wrap the reflow test wants is `flex-wrap: wrap`
  on the label, so the break lands BETWEEN its items. `.check-list > li > .check` is in `Reflow.test.ts`
  R1.1's frozen `overflow-wrap` list, so a value change there is the tester's.
- GD1.5 splits `device` on `,` alone (an id holds a `/`) while GD2.1's `tensor-split` vector splits on
  `[,/]+`: two parsers, never one.
- A chip (`.chip > span` is `white-space: nowrap`) inside a `dd` that is a flex item of a wrapping row
  grows the row past its box: the row is a grid item, a grid item is floored at its min-content, and the
  min-content of a wrapping flex row is the LARGEST item's min-content contribution, so one 32-character
  tag scrolls the page sideways (the Overview at 320 px, brief 17 U17b.4). `min-width: 0` on the CHIP
  changes nothing: a flex item's contribution is capped from above only, and `.chip { max-width: 100% }`
  is indefinite during intrinsic sizing. The cure is `min-width: 0` on the ROW and on the CELL (the flex
  item and the grid item), then the list wraps by chips and the chip that does not fit cuts its text.
  Measure it in Chromium, jsdom lays out nothing (see the probe recipe below).
- Those two `min-width: 0` rules cannot name `.tile-facts`: `HearthTilesBoot.test.tsx` H6b.6 pins
  `.tile-facts`, `.tile-facts > div`, `.tile-facts dt` and `.tile-facts dd` to an EXACT declaration list
  and refuses any other rule whose selector contains `.tile-facts`. The tile's row div has no class, so
  `.tile dl > div` and `.tile dd` are the selectors that reach them. Any rule that sets `min-width`
  needs its `selector: min-width: value` line in `HearthTokens.test.ts`'s H1a.12 ledger, in sheet order
  (read the diff's insertion point), and the line is the tester's edit.
- The UX check's `hover` state covers `li.chip` on the instance page's `tags` state, NOT `.tile .chip`:
  the Overview's chip pill is only proven by the jsdom test, so a tile chip's pill has no real-browser
  proof. Say so when the brief's criterion is about a tile chip.

## Tests that cannot pass, and what to report

- A helper that reads a page's list with `within(getByRole('main')).findByRole('list')` reddens as
  `Found multiple elements with the role "list"` the moment a nested `role="list"` joins the page (the
  Overview's tiles list plus a tile's `ul.chips`, brief 17 U17b.1). No production markup escapes it:
  `findByRole('list')` with no name matches EVERY role=list in scope, so the brief's own
  `ul.chips role="list"` and the shipped `ul.tiles role="list"` (HearthTiles pins both) cannot coexist
  under that query. It is a test-side scoping fix (scope to the tile, or name the list): report the
  helper's file:line, edit no test.
- A cascade test's self-probe can restate its own expectations as the PRE-slice sheet (H10: the probe
  wants min-height `32px` and the winner `.jump button`, while the test it probes wants
  `var(--control-h)` and `inline-flex` from the identical call). Satisfy the test, quote the pair.
- "the page adds no live region" counts every region of the page, and this one has a page-level
  fixture: the Save bar's `p[role="status"]` (`SaveBar.tsx`) predates the slice, and a brief's "what
  must not change" list naming the bar forbids the only fix. Report, do not remove the cue.
- A test file's own stub of the save answer decides the bar's text: `fe/src/save.ts` needs
  `{revision, running}` and treats any other 200 as a failure, so a `form()` answering
  `200 {revision:'r2'}` can never leave the status at `Saved.` (it reads `You have unsaved changes.`), and
  every `statusText` pin of a save in that file is the stub's, not the page's.
- Two shipped files can pin opposite things about one new field: `ModelProfileSave1`'s own
  `EDITABLE_BY_ID` map (a field with no entry there must be natively DISABLED) against
  `fe/src/test/modelProfileCards.ts`'s `EDITABLE_FIELDS` (the shared list the tab-stop test walks). Name
  the pair, keep the shared list's reading, and add the new acceptance file's reading.
- A hint of several lines has two incompatible renderings pinned: `ModelProfileCards1` 2e.88 wants
  `paragraphs.length === lines.length` for every field with a hint, a new file wants ONE node whose
  collapsed text is the whole hint. The shipped per-line `<p>` stays; report the new file's line.

- A fixture helper that forwards its argument as the document's TOP-LEVEL members (H11a:
  `assetsDoc(extra)` = `modelProfileDoc(values, { ...extra })`) turns a test that overrides a `values` key
  into a pin of the top-level member: the `values` the page reads keeps the fixture's default, so the
  expectation is unreachable, and reading the phantom member to satisfy it is not the contract (the profile
  read is `{id, engine, engineAvailable, enabled, values, projectUrl, revision}`; only the LIST row
  carries `model`). Quote the fixture line and the contract line; implement the brief's read.
- Rows moved behind `hidden` close the ROLE queries to them: every fixture that reads a card's `dl` with
  `getAllByRole('term')` needs the disclosure clicked first, and a test the brief's pin list omits
  (`ModelProfileContent.test.tsx` 2d.81) is a red to report, not a production defect.
- `HearthTokens.test.ts` H1a.12 lists EVERY `min-height`/`min-width` of the sheet as
  `selector: min-height: value`, so any new one the brief asks for is a red there. The list is in
  WRITTEN order, so the position of the tester's new line says where in the sheet the rule must sit
  (brief 11 D2: `.tag-options: min-width: 0` last, so the block goes after `.tag-new button`), and a
  comment line appended to an existing rule's comment plus the sheet's repeated declarations can make
  `git diff --stat` report the CSS edit as insertions only: verify the block by grepping it, not by the
  deletion count.
- A received the test builds with `.map(...).sort()` compared to a literal the test did NOT `.sort()` is
  unsatisfiable by any code: the sorted order of the strings is fixed (`.table-scroll` before `.tile`), so
  no rule order in the sheet gives the pair. `HearthH2b.6` sorts BOTH sides, `Reflow.test.ts` R1.6 sorts
  only the received. Report the line, the tester adds the `.sort()`.
- `fe/.prettierignore` leaves `ux/`, `briefs/` and `DESIGN.md` unformatted on purpose: `prettier --write`
  on `fe/ux/check.js` reformats blocks the project keeps as they are, so revert every hunk but yours.
  `prettier --check .` honours that file, `prettier --check fe/ux/check.js` ignores it and reddens the
  pre-existing blocks: judge the tree, not one path. `npm --prefix fe run lint` matches `**/*.{ts,tsx}`
  only, so `fe/ux/check.js` is linted by nothing: match its style by hand (no semicolons, single quotes).
- A Playwright pass that finds nothing prints no line: prove it bites by injecting a planted
  `body { min-width: 500px }` in the same way the pass injects its CSS and reading `scrollWidth`. With
  the text pass (200 %, 200 %+1.4.12) the whole check is 78 loads per browser (today, 2026-11-21:
  11 states x 2 widths x 2 themes = 44, + 3 stored-choice states, + 2 text modes x 11 states) and about
  4 minutes. Its last line IS the report: `browsers chromium firefox webkit; ... ; N failing`.
- Two assertions in one new test file can be unsatisfiable together: one pins a default box ENABLED, the
  next pins the same input `disabled` and empty with its box `checked`, from the identical fixture. The
  other one's wording governs; report the exact triple and change nothing.
- Brief 14's new criteria make FIVE shipped GPU-card tests unsatisfiable (7 reds, all pre-existing):
  GD1.7's old as-of test wants `time.textContent === shown` beside the new "no second copy of the date"
  (`Gpu2.test.tsx` 359 vs 383); GD1.9's old pair wants both move buttons of a two-row split active beside
  the new "both directions have an end" (`Gpu3.test.tsx` 387 vs 506); GD2.1 wants NO weight field with one
  row checked beside GD2.3's `[[A,'3']]` for the same fixture (`Gpu4.test.tsx` 487 vs 729); GD1.4/GD2.4
  want Auto and Even after the whole list beside GD2.9's `names[last + 3] === AUTO`, which is only true
  when no unchecked row follows the last checked one (`Gpu4.test.tsx` 777 vs 1371); and GD2.2's old
  `test.each` is titled "under tensor" while its fixture sends NO `split-mode`, so it pins `even` on the
  blank (layer) mode the new test pins at `auto` (`Gpu4.test.tsx` 541 vs 595). The brief's rule wins each
  pair, the old fixture is what is wrong (the tensor case needs `'split-mode': 'tensor'`).
- A test that saves a checkbox to `"false"` and then expects checking it again to be clean contradicts its
  own pin of the round trip (a loaded `false`, checked, sends `null`): making the saved value the loaded
  value makes checking a change. Report it.
- A note that must sit "right after the group heading" cannot satisfy a test pinning
  `heading.nextElementSibling === line` when the heading is nested INSIDE the group as its first child. The
  DOM order is already right, the assertion is the test's slip.
- A `pub(crate)` item, a fixture list, a pinned revision constant: report it, the tester owns it. The bff's
  revision is a keyed hash over the file name, so renaming a config FILE moves every pinned revision
  constant in the bff's tests.
- `tests::data_root_e2e` runs every e2e phase in sequence in ONE test, so its first panic hides every
  later phase: the manager's "262 passed" says nothing about the pins of the D-Bus and pipe phases.
  Enumerate them in the copy, one patch at a time.
- `expect(el.querySelectorAll(x)).toEqual([])` never passes in vitest: a `NodeList` is not an `Array`, so
  an empty one answers `expected [] to deeply equal []` with `Received: NodeList []`. It reddens whatever
  the code does: prove it with a selector that matches nothing, and report the line (`.toHaveLength(0)`,
  or spread it into an Array, the way the shipped tests do).
- A helper documented "the status, when there is one" built on `getAllByRole` THROWS where it should
  answer nothing, so every later "the status is gone" assertion reddens the whole test
  (`U17aSkeletons.test.tsx`'s `statusReading`: 7 of its 19 reds, all of them the helper, not the page).
  Prove it by copying the file with `queryAllByRole` and running the copy: 19/19 green is the evidence.
- A page whose first read now announces (a `role="status"`, brief 17 U17a) reddens any shipped test whose
  precondition is "the page has no live region" read right after the h1: the read is open in the first
  painted frame, so the region is there by design. Settle the read first (`await
  within(screen.getByRole('main')).findByRole('list')`) and the test keeps its intent (`H4.7`).
  `screen.findByRole('list')` alone is ambiguous: the nav's `ul` answers the same role.
- A test that must be settled needs the fixture's default to have content: `fakeBff()`'s default instance
  list decides whether a `list` role exists at all.
- A passing vitest test prints no `console.log` (the config is silent): probe a value by asserting
  `expect(live).toEqual(['__probe__'])` and reading the AssertionError, not by logging.
- `npm --prefix fe test -- <file> -t '<name>'` treats the filter as a REGEX: a test name with
  parentheses (`H6a.3 (17a.2)`) matches nothing and the run reports "1 skipped", which looks like a pass.
- `tools.opencode.worktree_create` + a symlink to the repo's `fe/node_modules` runs the whole fe suite at
  HEAD in seconds (no `npm ci`), and the untracked scratch test can be copied in and out.
  `git worktree remove --force` / `tools.opencode.worktree_remove` takes the symlink with it.
- A text-pinning test that does `body.find(want)` and then slices `tail[want.len()..close]` with
  `close = tail.find(')')` PANICS (`byte range starts at 33 but ends at 26`) for EVERY code shape when
  `want` itself carries a `)` (`"paths::instance_state_dir().join("` puts one at 26, its length is 33).
  Such a pin fixes the SHAPE of the code, not just its words: keep the join inline in the call it pins.
  Work the slice arithmetic out in python before reshaping production code for a text test.
- A slice-11 e2e phase that calls `instance_command("gpu-1", ...)` after an earlier block reset
  `instances.ini` to `[default]` + `[lan-2]` can only get `NoConfig(instances.ini)`: an instance IS a
  section (the same test pins that with its `[Default]` block). Report the missing section, no
  production change gives a launch for an id the record has no section for.
- A launch text-pinning test that finds `paths::instance_state_dir().join(` and then slices
  `&body[at + want.len()..close]` (`close` = the first `)` after `at`) PANICS `byte range starts at 33
  but ends at 26` the moment the code hoists the join into a local and passes the local to the call: the
  needle's own length runs past the closing paren. Such a pin fixes the SHAPE of the code, so the join
  goes inline in the call it pins. Read the slice arithmetic before restructuring.
- A slice-11 e2e phase that calls `instance_command("gpu-1")` after the block that rewrote
  `instances.ini` to `[default]` + `[lan-2]` can only get `NoConfig(instances.ini)`: an instance IS a
  section, and the phase wrote no `[gpu-1]`. Grep the section headers the fixture writes before believing
  a launch refusal is a production bug.
- A fake manager that records `format!("set_instance_tags {id} {}", tags.join(","))` puts a TRAILING
  SPACE on the empty list, and the test asserting the clear compares against
  `"set_instance_tags default"` (no space) and, in the same block, `calls == ["set_instance_tags
  default"]` which the fake's own format can never produce. Two assertions, one fake, unsatisfiable: the
  record format is the tester's, report it.
- A pin of an instance's startup count needs the instance's `[id]` section in `assignments.ini`:
  `assign::render` leaves a model profile OFF when the assignment carries no key for it and no `*`
  (`None => keys.contains_key("*") && global`), so an instance with no assignment section starts nothing
  and the `ModelsMax` cap cannot fire. The manager's D-Bus phase seeds `assignments.ini` as `"[default]\n"`.
- A "wrote nothing" / "left no file of its own" assertion compares the file against a snapshot the SAME
  block took: a fixture that seeds a file after its snapshot, or a call the block accepted before it, moves
  the baseline the assertion then calls the verb's doing.
- To see a full jsdom dump of a failing `getByRole`/`getAllByRole` assertion, use
  `DEBUG_PRINT_LIMIT=200000 npx vitest run src/<File>.test.tsx`.
- A memory fixture that sizes its card FROM the estimate (`need(trained)/MIB + reserve + 32`, so the card
  has `need + 32 MiB` usable) and pins in the same phase "the big card FITS it at the trained context"
  cannot also refuse that need on that card later: the refusal needs `needed > available`, and a sibling
  case lands the SAME text on the SAME card, so no rule separates the pair. The card of a growth refusal
  must sit between the two needs.
- `[*]` in a pool file is refused a CLIENT write (`invalid-model-profile-id ["*"]`, pinned in the manager's
  render phase: "the client cannot write the global section"); it reaches a render only as a hand edit
  (`std::fs::write`). So a test that writes `[*]` with `write_file` and expects a memory refusal answers
  `invalid-model-profile-id` first, whatever the memory rule is.
- The memory check's delta is per section, matched by id, on the ESTIMATE: a byte-identical section
  estimates the same so it is no growth, a section the write changed keeps today's estimate as its
  baseline, and the refusal is the first grown section in render order whose OWN need is over the node.
  A sum over the render reddens "two profiles each at 60 % of the card land".
- The GPU seam is a probe: estimate both renders and find the growth FIRST, and call `gpus()` only once
  some section grew. The seam counts its calls (`FakeGpus::asked`, one Arc per manager), and a save of a
  key the estimate does not read must ask 0 times, a growing write exactly once (a loop that asks per
  instance is fine only because the refusal ends it).
- `set_model_profile` writes `assignments.ini`, not the pool, so its memory check needs the render of the
  record in hand (`assign::render` of `text`, as `check_model_files_of` builds it) beside `render_of`'s
  today: a check that judges the pool pair against itself sees no growth and lands every assignment.
- A field helper that collects its alerts with `within(main).getAllByRole('alert').filter(inField)` is
  red before the assertion runs whenever the page has NO alert at all (`getAllByRole` throws on an empty
  set), so a test asserting the field's alerts are `[]` needs a permanent alert outside the field. None is
  possible: `RunControls.test.tsx:563` asserts `queryAllByRole('alert')` is `[]` in the same `main`. The
  helper's `getAllByRole` is the defect. Same for a status helper that pins "one status line".
- A tab-order test that compares `stops.slice(at[0] + 1, at[at.length - 1])` with `order.slice(1)` can
  never pass: the slice drops the LAST element named in the title (JS `slice` end is exclusive) while the
  expected array keeps it. The received and the expected differ by exactly that node.
- A test under `vi.useFakeTimers({ toFake: ['setTimeout', ...] })` that flushes with the file's
  `await new Promise((r) => setTimeout(r, 0))` helper never settles: it dies at the 5 s test timeout, and
  the document its timeout leaves behind reddens the NEXT tests of the file with `Unable to find an
  accessible element with the role "main"`, so one bad clock looks like three failures. Prove it with two
  runs: the neighbour alone (`-t "C1.12: a pill"`) is green, the neighbour plus the timed-out test
  (`-t "C1.1[12]"`) is red.
- A brief's px sizes for a new control (`min-height: 32px`, `height`, `width`) collide with the exhaustive
  CSS pins: `HearthTokens.test.ts` H1a.12 lists EVERY `min-height`/`min-width` pair of the sheet,
  `HearthFonts.test.ts` every `font-family` selector, `Reflow.test.ts` R1.1 every `overflow-wrap` rule. A
  new control is written with the tokens the pinned rules already use (`var(--control-h)` through the
  shared field/button rules), declaring none of those properties.
- R1.8 freezes the set of classes the sheet selects, and amending that freeze is the tester's edit, not the
  coder's: a new control needs no class of its own when it is the only `button` inside its container
  (`.chip button` styles the chip's x). Name the class in the report so the tester can amend the list.
- A scratch probe's `console.log` is swallowed in a passing vitest file; `process.stdout.write` prints.
  Delete every probe file before reporting, and say the probe's result, not its path.
- To read a layout in a real browser, copy `fe/ux/check.js`'s own bootstrap: `spawn('python3', ['-I',
  'fe/ux/stub.py', DIST, '0'])`, read the port from the FIRST stdout line with `/^\d+/` (the stub prints
  only the number, no colon: `/:(\d+)/` never matches), `fetch(\`${base}/__set?...\`)` for the state, then
  `page.goto(\`${base}/#/…\`, { waitUntil: 'networkidle' })`. `require('playwright')` resolves only from
  `fe/ux`, so require it by absolute path (`fe/ux/node_modules/playwright`), and build `fe/dist` first:
  the stub serves the built files, an old `dist` reads as "the change does nothing". `p.kill()` the stub
  at the end.

## `fe/`: the page's identity, the address, and jsdom's history

- "expected `<h1 id="page-heading">` to be `<h1 id="page-heading">` ... serializes to the same string" is
  a REMOUNT: the h1 the test found is detached (`document.body.contains(h1)` false, `parentElement`
  undefined). The cause in `App.tsx` is the `key` of the page element: a key that changes with the
  address remounts the page. When an address is a rename of the page already shown (an alias, a legacy
  bookmark), key on the NORMALISED address (`const at = ...`), never on the raw hash. Prove a remount
  by observing `main`'s childList records from before `render`: `-[H1:page-heading]` then `+[H1:...]`.
- `expect(nav).toBe(expect.anything())` never passes: `toBe` is `Object.is`. Only `toEqual` accepts an
  asymmetric matcher, so a retarget that writes `toBe(expect.anything())` is a test bug, red for every
  case that expects the element.
- `react-hooks/set-state-in-effect` (error here) follows `setState` called from the effect THROUGH a
  component-scope function, so a wrapper does not help, and a dep on that function loops (a new identity
  every render). Moving the bar (`history.replaceState`) and moving the focus
  (`document.getElementById('page-heading')?.focus()`) is DOM work: put it in the effect with NO state
  update, and derive what the render reads (route, page key) from a normalised value.
- jsdom session history (30): `location.hash = x` appends an entry AFTER THE CURRENT entry and drops the
  forward ones, so `history.length` in a file that shares one window is order-dependent: it passes with
  `-t` and fails in the full run after any test that called `history.back()`. `history.replaceState`
  never changes the length. A test pinning `history.length` is only meaningful in a window it owns.
- `startAt()` (`fe/src/test/bff.ts`) is itself a `history.replaceState`. A `vi.spyOn(history,
  'replaceState')` installed before `open()` records that call too, so an "exactly one replaceState"
  assertion needs the spy installed after the address is set.

## Slice 11/12 (2026-11-20): pins of the rule a slice replaces

- The tester's new pins and the wire phases of the SAME tree can pin opposite rules:
  `crates/manager/src/tests/dbus_client.rs` (tags phase: port and profile_tags from the render
  `server@<id>.ini`, and a missing render = `DEFAULT_PORT`) and `manager/dbus.rs`'s Version test
  (literal protocol `4`) pin the rule the slice replaces; `manager_instance_tags_e2e` (service.rs,
  port from the profile the section names, 0 when its file is missing) and `dbus_client.rs:1094`
  (protocol `3`) pin the new one. No production code gives both: report the exact file:line pairs,
  change no test.
- `binding_lint::cli_print_paths_hold_no_language` scans EVERY print/format macro in `cli.rs`
  (the `all: true` list): a path segment literal like `join("default")` passed inside `print!` is
  text. Hoist the joined path into a local before the macro. The cli's own text pin slices the
  Show arm from `instance_state_dir()` to the first `,` and wants `.join(` in that span, so keep
  the join in the arm, not only the local.
- `bff::server::field_of` answers the marked field from `args.first()`, which is the REFUSAL'S
  REASON for a refusal whose first value is not a key (`server-bad-tags` starts with the reason):
  such a key needs its own arm. The bff's 422 pins the field lower-case (`tags`), the form ids.

## Slice 13 (2026-11-20): a new route's fake default, and what a page read pins

- A new route's default in `fe/src/test/bff.ts` decides how much of the older suite you redden: answering
  the fixture by default reddens every test that finds the list named by the FALLBACK label (measured on
  this slice: 45 red with the default, 26 without). Default to the rejection of an unknown route
  (`Promise.reject(new TypeError("unexpected " + route))`), export the fixture (`NODE`), and let a test
  that wants the host name it.
- `react-hooks/set-state-in-effect` also fires for `setState` in an effect's early-return branch (clearing
  the value when the session is gone). Carry the owner in the state (`{ user, name }`), never write from
  the effect for the empty session, and derive the fallback in the render by comparing the owner to the
  current session: no extra write, and a sign-in as the same user keeps the value.
- `startAt()` fires NO hashchange, so a page that "takes the focus on arrival" only takes it on a
  hashchange, at a sign-in and at a sign-out. A test that loads at a route and awaits the focus on its
  `h1` cannot pass beside `ProfilesNav.test.tsx` 5b.8 ("the first load does not move the focus",
  `document.body`). Adding a first-load focus raise reddens that, `App.test.tsx` 1a.6, `SignIn` 1a.11 and
  `HearthTheme` H4.7: the rule the four green files pin wins, report the pair.
- A page helper that resolves at the `h1` reads the LOADING state: the `h1` and the
  `p[role="status"]` coexist by the brief's own rule, and React's scheduler moves a promise-driven update
  to a macrotask (chained `await Promise.resolve()` never shows the content, one `setTimeout(0)` does).
  No production change helps, and those tests flake (the same file gives 6, 7 and 11 red in three runs).
  The helper must await the status disappearing.
- `bff.to('GET', path)` counts the reads of the SHELL and of a PAGE alike, so two criteria of one brief
  (one read per session, and one page read per arrival) are unsatisfiable together as counts; the page's
  own test pins 2 at arrival, the shell's counts 1 and names About in its loop. Quote both lines.
- jsdom never follows an anchor: `fireEvent.click(a)` on `href="#/about"` leaves `location.hash` at `#/`.
  A test must call `navigate()`; a production `onClick` handler cannot be what moves the address.
- The coder runs `format:check`, so the tester's unformatted new test files are yours to format. Prove
  prettier writes whitespace and quotes only: copy each file, run `prettier <file>` into a second copy,
  `diff` them, then `prettier --write`. Rerun the suite after.

## Slice 14 (2026-11-20): the manager's new verbs, and the bff's new routes

- `cargo test -p pai-bff` needs `PAI_BFF_FE_DIR=<repo>/crates/bff/fe-fixture` (the build script's
  embedded-UI directory: without it the crate does not build). It runs 19 targets and prints 19
  `test result:` lines: sum them (`awk '/^test result:/{p+=$4; f+=$6} END{...}'`, fields 4 and 6), the
  last line alone, of the target that happened to finish last, says nothing about the slice.
- A method added to `ManagerClient` with a default body keeps every older fake compiling untouched: the
  siblings' body is `Err(ClientError::Other("<method>: ..."))` (the bff's `tests/instances.rs` fake
  answers `Unimplemented` for the new verbs and needs no edit). The two transports override it.
- Windows GNU clippy on the manager's e2e: a `let mut` whose only `push` sits in a `#[cfg(unix)]` block
  is `unused_mut` on windows-gnu. The fix that keeps Linux green is `#[cfg_attr(not(unix),
  allow(unused_mut))]` above the binding; removing the `mut` reddens the unix build.
- `--all-targets` lints the tester's new file, and three fixes change no assertion: a
  `let mut fixture = Fixture::default(); fixture.x = ..` is `field_reassign_with_default` (write
  `Fixture { x: .., ..Fixture::default() }`), a `&[s.clone()]` of one element is
  `cloned_ref_to_slice_refs` (write `std::slice::from_ref(&s)`), and a closure returning a 5-tuple
  `Vec` is `type_complexity` (a local `type Row = (...)` right above it). Report the file you touched.
- To prove what a count assertion in a long e2e really holds, without editing the test: a temporary
  `eprintln!` in your OWN function, `cargo test -p <crate> --lib <test> -- --nocapture`, read the line,
  remove the line, then rerun fmt, clippy and the test. The manager's list holds
  `big, default, ghost, lan-2, secret, shared, spare, zero` at `service.rs:9721`, where the test wants 5.

## Slice 15 C1 (2026-11-21): the instance tags as chips

- Two lists, two roles, and they are NOT interchangeable: the chips shown are the field's OWN list
  (latched at mount, only a 204 moves it, so a poll never moves a chip), while the body of every write is
  built from the READ's own tags, and the "from the server profile" line filters the union against the
  chips. A write built from the read while the chips come from the state loses a chip the user added
  seconds ago; a profile line filtered against the read keeps a tag that is already a chip.
- A pill whose Confirm on an empty box CLOSES (focus back on the `+`) while Enter on an empty box leaves
  it open is not a slip: a test loop presses Enter on blank and expects the box to stay, then clicks
  Confirm on blank and expects the pill gone. One `add(byButton)` flag answers both.
- Focus moves that a test reads in the same tick (`document.activeElement` right after the click) need
  `flushSync` around the state change, then `document.getElementById(id).focus()`: the id of a control
  that mounts in this commit does not exist before the commit.
- The field is `<fieldset className="field-control" aria-labelledby="<id>-label">` under a
  `<span id="<id>-label">` (the shape of the private `LabelField`), NOT a `GroupField`: `GroupField` puts
  its `<legend>` in the group, and this field's label row carries the About toggle, whose name
  (`About Instance tags`) a tab-order test counts as a stop.
- A page-level focus map that names a control of a child field is a string the compiler cannot check: when
  the slice removes that control (the tags `Save tags` button) the page's `getElementById(...)?.focus()`
  silently lands on `body` (no lint, no type error, only a `document.activeElement` test reddens it). When
  the target depends on the child's own state the page cannot see (the pill open: focus its box, else the
  `+`), the presence test is the lookup itself: `document.getElementById(`${id}-box`) !== null` is exactly
  "rendered", since the child renders the box only while it is open. Keep the ids of a field in ONE place
  and grep the page for the removed id (`grep -rn "tags-save" fe/src`) when a control goes away.
- A section of the page that can be dirty on its own calls `useWarnOnLeave(dirty)` and the shared
  `useDirtyReport(dirty, onDirty)` itself; a page-level `useWarnOnLeave(tagsDirty)` beside the Server
  form's own listener arms the browser ask TWICE (a `beforeunload` probe reads one, but the leave guard
  double-asks). The page only ORs the two sections' dirty flags for the app.

## Slice 15 C2 (2026-11-21): the tag suggestions, three tests that cannot pass

- A role query for a `hidden` element is unreachable by name, in BOTH forms: `getByRole('listbox',
  { name })` throws (the default excludes the ARIA tree-excluded, `hidden` among them,
  [ByRole `hidden`](https://testing-library.com/docs/queries/byrole)), and with `hidden: true` the same
  element is listed with Name `""`, so the name form throws too. Probe it in a copy: the identical tree
  without `hidden` is found by role and name. So a test that asks
  `getByRole(listbox, { name })` and, a few lines below, `isHidden(list) === true` (the repo helper: an
  ancestor `[hidden]`) has no production shape: name it, do not un-hide the list to please one line.
- A listbox whose arrow walk must land on a label its own text filter hides: C2.4 builds "fast active"
  by typing `zz` (the only match is `zz-top`) and pressing ArrowDown twice, while C2.3 pins
  "from the last back to the first" as `arm` from a two-shown list (`a` matches `arm`, `fast` of three).
  One walk rule answers one of them: in a copy, walking the FULL option list turns C2.4 green and C2.3
  red at `from the last back to the first: expected 'zz-top' to be 'arm'`. The brief's decision 10
  ("those containing the typed text") and C2.3 stand: report C2.4's construction.
- A cap test that lists the labels of ONE source while a sibling test of the same file pins the OTHER
  source's label is the same contradiction in a fixture: C2.7 expects `arm, t0..t6` and drops `fast`,
  which its own fixture's instance `b` carries and which C2.6 asserts is suggested (`['fast']` alone
  when the profiles read fails). In the copy, dropping the instances' labels from the suggestion source
  turns C2.7 green and C2.6 red. The union, sorted, capped, is decision 10.
- Decision order matters: "sorted; those containing the typed text; at most 8" is filter THEN cap. A cap
  applied to the sorted list before the filter hides a label the visitor's text matches (text `t7` of
  ten labels shows nothing). Cap the filtered indices, keep the full sorted list as the option ids.
- Escape on a list that is not drawn: gate on the EXPANDED flag (a match exists), not on the open flag,
  else a box whose text matches nothing needs two Escapes to close, against C1.3.

## Slice U17c (2026-11-21): one live region, two sources

- The Model profiles status line (`p.model-profiles-filter-status`) carries the sort's words AND the
  filter's count, and the suite pins both shapes: every older test asserts the line EQUALS the count
  (no header clicked, so the announcement is empty), while `U17cSort.test.tsx`'s rule case asserts the
  line CONTAINS the sort's sentence with a filter on. Compose the two by joining the non-empty parts
  with a space; an `if (filter !== '')` that overwrites the announcement satisfies neither the case
  that clicked a header nor the 12 green ones of that file.
- A filter that matches nothing and a sort announcement never coexist: the table renders only when
  `shown.length > 0`, and with it the sortable header buttons, so there is no header to click while the
  no-match sentence stands. No precedence rule between the two is needed, and none is pinned.

## xtask: the runtime closure and the loader

- `ldd` honours the caller's `LD_LIBRARY_PATH`, so the Linux package phase's closure of the staged
  files can resolve a GPU runtime outside the dists the build is configured with (the check that
  follows then refuses the backend). The closure's `ldd` goes through `util::cmd_without_ld_library_path`
  (`Command::env_remove`), leaving the file's RUNPATH to decide. Do not set the variable off in the
  process: the helper removes it per child, so no other command is touched.
- A `get_envs()` pin wants exactly `[(key, None)]`, so the helper removes that one key and nothing else;
  the real-behaviour control sets the variable in the process, runs `env` through the helper and the
  plain `cmd`, and restores the saved value before asserting (a harness where the variable is simply
  unset must fail the control, not pass).
- `xtask/src/linux/package.rs`'s own source pin is `include_str!("package.rs")` sliced to `run()`
  (`split("pub fn run(").nth(1)` up to the next `\nfn `): the needles are the CALL texts, so a comment
  added inside `run()` is safe and `cmd("ldd")` must be gone from that region.
- A GGUF fixture writer that puts `n_kv` in the slot `parse_header` reads as `n_tensors` (the manager's
  `service.rs::write_gguf_kv`) parses whenever the two counts are EQUAL, which is why the 4-kv / 4-tensor
  transport pins are green and a 2-kv / 7-tensor one is answered `model-info-unreadable: a string of N
  bytes`. The reader's order is tensors then kv (`gguf_shape.rs`'s `parse_header`, and its own test writer
  `gguf()` writes it that way): say so, never "fix" the reader for a fixture.
- `paths::redirect_for_tests(dir)` makes `paths::data_root()` `<dir>/private-ai` (`APP_DIR`), NOT `dir`. A
  fixture naming a RELATIVE `model` has to put the file under `data_root()`: the instances' working
  directory is `launch::instance_current_dir(cfg!(windows), &data_root(), &instance_state_dir())`, the same
  `cwd` the GPU memory check resolves against, pinned by its own passing block.
- A phase that rewrites a shared pool file to pin one case (`std::fs::write(models.ini, "[gone] ...")` for a
  missing path) erases every section the LATER blocks of the same test read, so their first read answers
  `no-model-profile-named`. The rewrite is the defect; the later blocks are green once the pool is restored.
- The pins a stopped phase hides, without a tree copy: add `crates/manager/src/manager/scratch.rs` with one
  `#[test]` per phase (`crate::tests::pipe_client::manager_pipe_client_e2e()`), the phase fns are private so
  a filter runs 0 tests until `tests/mod.rs`'s `mod` lines go `pub(crate)`, run each with its own filter
  (every phase redirects the data root process-wide, so never two at once), then delete the file and the
  marks. `git status --short` proves nothing of it is left.
- A reason key that travels in a payload (`ModelInfoReply`'s `Reason { key, args }`), not as a
  `checks::Refusal`, needs a literal `strings::fill("<key>", ..)` arm in a source the CLI reaches, else
  `binding_lint` reds it as "in cli.toml but used by no source"; one `text()`-style fn with one arm per key
  is the link, and `checks::KEYS` needs no entry for such a key.


## Slice 18 AN3 (2026-11-22): the node's info over the two transports

- Two reds of the tester's AN3 set, both proved as the tester's, no production change:
  - `client/pipe.rs`'s `a_node_info_reply_of_the_wrong_shape_is_other` opens with
    `client_on(input(&[node_info_document()]))`, the document as the FRAME, while every other reply in the
    file is `json!({"ok": ...})`: `call_value` answers a frame with no `ok` / `error` at
    `reply.get("ok").cloned().ok_or_else(other)`, so that `.expect("the fixture's document")` can never be
    Ok. Proof it is that one line: a scratch copy of the test (same module, renamed, only that line wrapped
    in `{"ok": ...}`) passes whole, every shape loop included. Extract it by brace counting, run it with
    its own filter, delete it, `grep -c scratch` the file.
  - `manager/src/tests/node_info.rs`'s `a_vulkan_read_of_none_answers_the_word_and_no_rows_vulkan` asserts
    `name: Some("")` where its own comment says "the name is a hole, never a guess": the file's `row()`
    writes `name: Some(name.to_string())` unconditionally, and the fixture hands it `""`. The seam cannot
    carry `""` to the join (`fakes::text` maps `""` to `None`), `vulkan::join` leaves the kernel's name
    `None` when the child answered `none`, and `Some("")` is unreadable over D-Bus, where `""` IS the
    client's `None`.
- The AN3 wire: `NodeInfo() -> (s name, s version, s kernel, s vulkan, a(sssssbss) gpus)`; a row's Vulkan
  pair travels with its own `b` flag (the bus has no null, an empty text is the fact the host did not
  answer), and the pipe document writes EVERY key, so the client reads a missing key as a shape
  (`ClientError::Other`) and only a `null` as a hole. One `pai_core::vulkan::word` / `status_of` pair
  carries the three words for the child's parser, the pipe document and the bus, so no transport spells them.
- `ManagerClient::node_info` rides the trait's `Other` default, so the 15 fakes in `crates/app` and
  `crates/bff/tests` need nothing for AN3 (its "Done when" asks no bff route; AN4 adds them). The
  production wiring is two sites: `.with_node_facts(HostNodeFacts::for_node())` beside
  `.with_gpus(ProbedGpus::for_node())` in `manager/dbus.rs::serve` and `manager/windows/entry.rs`.
- A name imported and then used by its full path (`crate::vulkan::Status::Failed`) is `unused_imports`
  under `-D warnings`: import it and write the short path.

## Slice LS1 (2026-11-22): the model facts read beside the model profile read

- `fe/src/test/bff.ts` answers a route as `options.X ?? DEFAULT`, so a test that names a literal `null`
  body (`{ modelInfo: null }`) is answered the DEFAULT fixture: no production reader can tell "the body
  is null" from "the test named no body", so such a case is red by construction. Report the line.
- A new GET under an existing prefix reddens every pin whose helper filters by PREFIX:
  `ProfilesNav.test.tsx`'s `profileReads` is `path.startsWith('/api/profiles/model')` compared with an
  exact one-item list, so the facts GET of the same id counts and the test demands no facts read for the
  ids `19 G.1` demands it for. `bff.to(method, path)` is exact, `bff.calls.filter(startsWith)` is not:
  grep the suite for `startsWith('<the route>')` equality pins before promising a sub-route green.
- The test mirror of the strings, `EN` in `fe/src/test/bff.ts`, is what the text tests read, and
  `App.test.tsx` pins `en_US.json` to it key by key, so a production string can never lead it: a brief
  table and a test that counts the lines of the same value (`split('\n').length`) must agree first.
- To keep a read's answer from outliving the row it belongs to, store `{ id, read }` and gate in render
  (`facts?.id === id ? facts.read : undefined`): a `setX(initial)` at the top of the read effect is the
  eslint error `react-hooks/set-state-in-effect`, and the gate needs no effect at all.
- Whether a file was prettier-clean at HEAD: `git show HEAD:fe/src/x.tsx | npx --prefix fe prettier
  --stdin-filepath fe/src/x.tsx --check`. A copy in `/tmp` finds no config and reddens every file.

## Slice J1 (2026-11-22): `tensors` in the model info, `override-tensor` on the web

- llama.cpp v1.0.0 trims NOTHING on the `-ot` path: `parse_tensor_buffer_overrides` (`common/arg.cpp:239`)
  does `string_split(value, ',')`, `find('=')`, `substr` on both sides, and looks the RIGHT side up
  EXACTLY in a map keyed by `ggml_backend_buft_name` (so `CPU` is the name, `ggml-backend.cpp:1185`):
  `"a= CPU"` throws `unknown buffer type`, the model never loads; `"a=CPU, b=ROCm0"` does NOT throw,
  because only the device part is looked up, and `llama-model-loader.cpp:1334` compiles `" b"` with
  `std::regex` + `std::regex_search` and matches no tensor name: silent, the rule is dead. So a space rule
  must be judged on the RAW text on each side of the first `=` (the `,` split adds the space to the pattern
  of every rule after the first), and the `, ` separator the GUI's `render` never emits makes a `", "`
  value a hand-written line. (There is no `parse_patterns`: a name from a summary, not in this tree.)
- That rule and HEAD's `every_override_tensor_rule_names_a_pattern_and_a_device` are unsatisfiable
  together: its case wants `"blk\.1\.attn=CPU, ffn_.*_exps=ROCm1"` Ok with `changed = true` and J1's wants
  the structurally identical `"a=CPU, b=ROCm0"` refused as `override-tensor-space [2]` (same device
  spellings, same `changed`). Quote both lines, keep the design's rule.
- `checks::override_tensor` takes `changed: bool`. The two call sites in `pai-app` pass `false`
  (`model_profiles::validate_for_save` rewrites EVERY key of the profile, so no line there is "the save's
  own", and `tensor_override::validate`'s table trims the device and sanitizes the pattern): the new rules
  are the web save's, and the GUI's behaviour is untouched, which is what the app's 577 tests pin.
- Two array-length literals grow by hand when a refusal key joins a test list: `EXPECTED_KEYS: [&str; 13]`
  in `checks.rs`'s module (E0308 names the size), beside `MODEL_PROFILE_INT_KEYS`. Both are test data:
  a key a test names is required by that test's own acceptance.
- The model-info row travels as a tuple in TWO copies (`ModelInfoRow` in `crates/core/src/client/dbus.rs`
  and `crates/manager/src/manager/dbus.rs`): widen both, and the inner nested tuple renders in zbus's
  signature as `(bsstta(sst)b)` for `ModelFacts` + `(as)` for the key and args. The pipe reply is strict,
  so a new key is a new `parse_doc_line` arm and every document fixture (both transports' test modules,
  the manager's `tests/`) must carry it: a fixture that omits it is answered `Other`.
- The manager's `model_info` reads the shards ONCE (`gguf_shape::read_shards`) and feeds the same shards
  to `shape_of` and `tensor_rows`; a reader failure maps to `model-info-unreadable [path]` and drops the
  reader's text, so a path the pool names travels in the reply and no byte of the file does.
  `tensor_rows` answers `(rows, truncated)` with `MAX_TENSOR_ROWS` inclusive (20 001 tensors answer 20 000
  rows and the flag).
- The byte bound counts each row as `serde_json::to_string(<row>).len() + 1`, and that length is the SAME
  whatever the key order (`{"name":X,"bytes":N}` and `{"bytes":N,"name":X}` differ by zero bytes: the keys
  and values are the same text), so `json!({"name","bytes"})` in `pai-core` matches the test's `row_bytes`
  and the `preserve_order` flip of the build cannot move the cut. Cut at the FIRST row past either bound
  (a labelled `break 'shards`: the rows past it never travel, in any shard) and set the flag there; the
  test pins the tightness, so a per-row count 1 byte off the test's shifts `fits` and reddens the list.
- A 20 000-row fixture prints a 745 KB assertion line: the message ("7340 fit") is at its HEAD and the
  kept list is the whole rest, so read the failure with `sed -n '<line>p' | cut -c1-200`, and never `cat`
  the log (the tool showed only the tail of that one line).
- A fixture that hands the same file to two pins can be self-inconsistent: `service.rs`'s `plain_facts`
  lists 6 tensor rows while `write_gguf_kv` writes 7 for `plain.gguf` (the missing one is
  `output.weight`), and its sibling `moe_facts` does list its `output.weight`. The design says every
  tensor, so the fixture is the red, not the reader.
- `got.info.unwrap_or_else(|| panic!("no facts: {got:?}"))` is E0382 (the closure borrows `got` after the
  partial move): `got.info.as_ref().unwrap_or_else(..)` loses no assertion, apply it and name it.
- A test attribute glued to the tail of the doc-comment line above it
  (`/// ... accepted value.#[tokio::test]`, `crates/bff/tests/model_profile_save.rs:3202`) is a doc
  comment, so the test never runs and `--all-targets` clippy reddens the file with `dead_code` on the
  function and its const. Report the line, it is the tester's.

## Stop every stub you start

Probing the UI with `fe/ux/stub.py` by hand left eleven stubs running detached for the U17b slice
(2026-11-21). Start a stub in the foreground of one command that kills its pid on exit (`python3 -I
fe/ux/stub.py ... & pid=$!; ...; kill $pid`), and check `ps -eo pid,ppid,cmd | grep stub.py` is empty
before you report.

## Slice 19 LS1 (2026-11-23): the per-GPU block editor

- A model profile number field's text is NOT in the page's `values`/draft: while its default box is
  ticked the draft carries no key at all, and the text a user typed lives in `boxed[key]`
  (`boxText` from `fe/src/boxed.ts`, `''` when ticked). A card that reads a sibling number field
  (blocks mode reads `n-gpu-layers`) must read `boxed`, and `values[key]` answers `null` forever: proof
  is a dump of `boxed['n-gpu-layers']` next to `values`, in the scratch test.
- jsdom moves no focus: `fireEvent.change` and `fireEvent.keyDown` leave `document.activeElement` on
  `body`. A test that asserts a control "keeps the focus" while its row re-renders (`19 G.6` on the
  block slider) is green only if the handler focuses it: `event.currentTarget.focus({ preventScroll:
  true })` before the state write, which is a no-op in a real browser, where the drag owns the focus.
- A vector of ZEROS is the same state as an empty one (`tensor-split: 0,0` is Auto's, not a cut of
  nothing): a rule keyed on "every weight empty" must read `w === '' || Number(w) === 0`, the shape
  `gpuShares` already uses for the `*-share-auto` text.
- Two reds of `ModelProfileGpu6/7.test.tsx` are the tests', not the code's, proved by the helpers:
  `blockValues` returns `[id, blocks input value, slider value]`, so the Escape test's
  `input.value === '20'` and `[[A,'29','29'],..]` ask one element for two texts at once; and the
  19 G.9 uncheck test expects the row order `[CUDA0, ROCm0, Vulkan0]` where brief 14 GD1.5/GD1.8
  ("first the checked rows, then the unchecked at its GD1.5 place") and that file's own green
  `GD1.8: unchecking one moves it back among the unchecked rows` answer `[CUDA0, Vulkan0, ROCm0]`.
- `Reflow.test.ts`'s `CLASSES` needs the new class (`blocks-slider`) and brief 19 section 6 names only
  `HearthTokens.test.ts` H1a.12 among the files that change, so a brief that briefs a CSS class (19
  G.12's 24 px + `accent-color`) must name the R1.8 list too.

## Slice 19 LS1 review (2026-11-22): D1 the fieldset's grid, D2 Auto vs malformed

- A GPU fieldset is a `.field`, so from 720 px (`index.css` 1388) its children place themselves in the
  200px + 1fr grid: a paragraph that appears and goes (budget, Auto, malformed) takes the second cell of
  row 1 and the list drops into the label column. The review's fix, in that same media block, puts the
  three paragraphs (one shared class) and the list in `grid-column: 2`, and needs NO `grid-row`: the
  auto-placement cursor sits at the last explicitly-placed cell, so the un-placed `.check-list-actions`
  (Auto, Even) lands on the row after the list, in the control column, and the card's
  `p[role='status']` takes no cell because `.visually-hidden` is `position: absolute` (an abspos child of
  a grid container is not a grid item).
- A CSS rule naming a `gpu` selector reddens TWO exhaustive lists, and brief 19 section 6 names neither:
  `Reflow.test.ts` R1.8 (the classes the sheet selects, and the ids: `#root` plus
  `#model-profile-gpu-devices`) and `NodePage.test.tsx` 1191, every rule whose selector matches
  `/\bgpu\b/`, IN SHEET ORDER (a rule of the 720 px block at ~1405 comes before `.check-list .gpu-id` at
  ~1712). That helper folds the selector's whitespace, so a two-line comma group reads as `A, B`: write
  the expected entry with one space after the comma.
- `fe`'s eslint config sets no `argsIgnorePattern`, so the `_` prefix saves nothing: an unused LAST
  parameter of a `test.each` callback (`async (vector, _why)`) is `@typescript-eslint/no-unused-vars`
  (the `after-used` default spares `_what` when a later parameter is used). Drop the parameter, it loses
  no assertion: the title's `%s` comes from the tuple, not from the callback's list.

## Slice 19 LS2a (2026-11-23): the field slider, and the four card inventories

- A styled control that a brief gives NO class name for is selected structurally, not by a new class:
  the field slider is `.field-control input[type='range']`, which keeps `Reflow.test.ts` R1.8 (the frozen
  class list) untouched. `HearthTokens.test.ts` H1a.12 names the selector AS THE SHEET WRITES IT, quotes
  included: `.field-control input[type='range']: min-height: 24px` in single quotes in the JS list.
  `Reflow.test.ts`'s R1.8 list does not name it, so one file carries the selector, the other the freeze.
- When a locked Hardware row becomes editable, four suites that inventory "every control of the card"
  redden at once, and each reads one shared default profile: `ModelProfileSave1` (its `BASE` plus
  `EDITABLE_FIELDS` and `LABEL_TO_KEY`), `ModelProfileSave3` (the values of the 2h.150 table's read),
  `ModelProfileSave13` (its `AS_LOADED`, the "every key of the card present" read) and `ModelProfileSave16`
  (its `BASE`, the 194 greyed-out list). Adding the key to that one default per file turns all four green:
  a locked control in a "present" read is a defect in the read, not in the code. `n-cpu-moe` joins
  `NO_INPUT_MODE_KEYS`'s exception list for the same reason (0 to 4096, so `inputmode="numeric"`).
- Verify a one-line test fix without touching the tester's file: copy each red file to
  `fe/src/Zz<name>.test.tsx`, patch the copy with python exact-string replace, run the copies, `rm` them.
  Green copies plus the green `typecheck`/`lint`/`format:check` is the report.
- Two shapes of red test in brief 19's `ModelProfileLayers1.test.tsx`, both unfixable in production: a
  helper called as `form({})` whose body asserts the values of `form()`'s OWN default (`{...AS_LOADED}`),
  and a 422 test that types a value outside the range the same brief requires the client to check, so
  `save-invalid` refuses it and no PUT is ever sent (typing a value the client accepts reaches the 422).

## Slice N1a (2026-11-22): the nav's Nodes group, and the four shapes of red there

- **A nav link that comes from a read cannot be read synchronously after the nav appears.** The store
  read starts in the nav's own effect, so its answer lands a few microtasks AFTER the nav paints;
  `await screen.findByRole('navigation')` resolves on the mutation that inserts the nav, so a
  `getByRole('link', {name: <id>})` on the instance links right after it is a race production cannot
  win. Proof at HEAD 43dd658 with the OLD nav: right after the nav, its links are
  `['#/', '#/profiles/model', '#/node', '#/about']` (no instance link at all), and ONE
  `findByRole('link', {name:'default'})` later they are all there. So `NodesMenu1` 20 N.2 and the
  second 20 N.5 (`navOf()` then synchronous `getByRole`) are test-side; every suite that reads the
  links uses `open()`, which awaits one link first.
- **A count of elements inside a label that carries an icon contradicts the icon suite.** 20 N.4 asks
  `#nav-host.querySelectorAll('*').length` to be 1; `HearthIcons.test.tsx` H8a.1/H8a.2 ask the same
  element's first child to be `svg.icon` whose children ARE the five shapes of lucide `computer`, so
  the answer is 6. `querySelectorAll('*')` counts svg children.
- **One file can pin two icon counts.** `HearthIcons.test.tsx` H8a.2 (amended for brief 20) counts 7
  `svg.icon` in the nav; H8a.8's case table still says `nav: 4` (the pre-brief-20 nav), so it reddens
  on any code that satisfies H8a.2.
- **A helper that waits for a link by its bare id reddens every test whose fixture gives that instance
  a server profile.** `NodesMenu2.test.tsx`'s `open()` awaits `findByRole('link', {name:'default'})`;
  20 N.5 names that link `default, server profile qwen-27b`, so the five tests that open with `TWO`
  die at the 1000 ms `findByRole` timeout. Their bodies, with that name in the wait, are green (a
  scratch with the three bodies proves it, `isHidden` included in `tabbable`).
- **A CSS rule a brief mandates reddens the sheet inventories the tester did not amend.** Brief 20
  20 N.11's `.nav-instance { min-width: 0; overflow-wrap: anywhere }` is listed by `Reflow.test.ts`
  R1.1 (the rules that set `overflow-wrap`) and R1.8 (the frozen class list), and MISSING from
  `HearthTokens.test.ts` H1a.12 (every rule declaring `min-width`/`min-height`), which reads the
  declaration AS THE SHEET WRITES IT: `.nav-instance: min-width: 0`. `gap` is not one of its watched
  properties, so `.nav-label:has(> .icon)` reddens nothing there.
- **`vi.useFakeTimers()` with the DEFAULT `toFake` fakes what React's scheduler uses**: a scratch that
  renders `App` under it hangs at the first `findByRole` and dies at the 5 s test timeout. The suites
  that fake the clock fake `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`, `Date` only
  (`NodesMenu1.test.tsx`'s `fakeClock`), which is why they pass.
- **Prove a test-side red against the committed tree, in seconds**: `git worktree add
  /tmp/opencode/<x> HEAD`, `ln -s <repo>/fe/node_modules <x>/fe/node_modules`, drop the scratch in
  `<x>/fe/src/`, `npm --prefix <x>/fe test -- <name>`, then `git worktree remove --force <x>`. No
  install, and the tester's uncommitted edits stay out of the proof.

## Slice N1b (2026-11-22): a whole part with no production red, and the copy that proves it

- **A file red at its own helper proves nothing about the code**: all six `NodesMenu2` reds die in
  `open()`'s `findByRole('link', {name:'default'})`, so N.7 to N.12 assertions never run. With that one
  name widened to `/^default/` in a copy, all six go green on the SHIPPED code: the proof a part needs
  no production change is the copy, not a new assertion.
- **A whole-`fe` copy, and the one file outside `fe` a test reads**: `rsync -a --exclude node_modules
  --exclude dist --exclude ux --exclude briefs fe/ <x>/fe/`, symlink `fe/node_modules` in, and copy
  `crates/core/strings/` to `<x>/crates/core/` (`HearthTiles.test.tsx` 369 reads
  `../../crates/core/strings/en_US.toml`, ENOENT in a copy of `fe` alone). 3.9 MB, the full 107-file
  suite in 32 s, `npm --prefix <x>/fe test`.
- **`HearthIcons.test.tsx` H8a.8's nav icon count is a race, and its right number is 7** (house,
  computer, a box per instance, cpu, layers, info): the signed-in case awaits only the nav, so it reads
  5 when the list has not landed and 7 when it has. The wait must be `within(nav)` (the Overview tile is
  also a link named `default`, and a screen-level find answers "multiple elements").
- The 20 N.5 race (part a's list) is INVISIBLE in a filtered run: green alone, red only in a full-suite
  run. State the run you measured.

## Slice 21 TP1a (2026-11-22): the pure rule module, and the strings file it does NOT touch

- The tester ships the production file as a STUB that throws (`fe/src/tensorRules.ts`: the exported
  types and signatures, every body `throw new Error(.. stub ..)`), so the red is behaviour, not a load
  failure, and the coder REPLACES that file (it is UNTRACKED: `git diff` prints nothing for it).
- A pure module's label-key pin goes through the TEST's English table
  (`en()` in `fe/src/test/bff.ts`, its own `EN` object), not through `fe/src/strings.ts`. So a module
  whose table carries `label: 'model-profile-ot-kind-<id>'` goes green with the key in no strings file.
  Do NOT add those keys to `strings.ts` in that run: `strings.test.ts`'s "every value is used" scans
  source text for `t(` + a literal, and a label that reaches `t` only from a component is table data, so
  the keys belong with the slice that renders them. Check which file a label pin reads before widening.
- `fe`'s tsconfig has NO `strict` and no `noUncheckedIndexedAccess`, and its eslint
  (`tseslint.configs.recommended`) does NOT enable `no-non-null-assertion`: a tester's `KINDS[i]!` lints
  clean, and production indexing needs no `!` (guard an out-of-range index with
  `if (entry === undefined)`, which both checks accept). `erasableSyntaxOnly: true` rules out a
  `const enum` in this tree.
- Mirroring a Rust parser into TS, the divergences to name in the code: `u32::from_str` takes a leading
  `+` (a digits-only `/^[0-9]+$/` refuses it), and Rust's `split_once` is the FIRST occurrence, so
  `indexOf` not `lastIndexOf` (the block list `blk\.(1|3)\.x` splits at the first `)`).

## Slice 21 TP1b (2026-11-22): the Tensor card, and three test shapes that eat a whole file

- **A new card in the jump list makes an unamended test file leak its document.** jsdom has no
  `Element.prototype.scrollIntoView`, so the jump click raises an UNHANDLED `TypeError`
  (`ModelProfilePage.tsx` `jump`), React never unmounts, and EVERY later `screen.getByRole('main')` of
  that file dies on `Found multiple elements with the role "main"`: in `TensorPlacement1.test.tsx` that
  one missing line reddens three tests that never touch the jump, and vitest prints `Errors 1 error` at
  the bottom of the run. The fix is the test's, one line in its `beforeEach`, the shipped idiom
  (`ModelProfileGpu1.test.tsx:68`, `HearthJump.test.tsx:38`):
  `Element.prototype.scrollIntoView = () => {}`. Prove a cascade like this in a copy with that line
  added, and say which reds are the cascade.
- **A focus move to a control that MOUNTS in the same commit is handler work, not effect work.** The
  repo idiom is `flushSync(() => { setRows(..); setDirty(true) })` and then
  `document.getElementById(id).focus()` in the click handler (App.tsx, Chip.tsx, the page's own `jump`).
  The "keep the target id in a state and focus it from an effect" shape is an eslint ERROR here:
  `react-hooks/set-state-in-effect` flags the `setOtFocus(null)` the effect needs to arm itself once, so
  the effect cannot clear itself.
- **A card that empties on a removal keeps TWO things visible at once** (21 T.3 + 21 T.7): the plain
  `model-profile-ot-none` paragraph AND an empty `<ul role="list">`, so `Add rule` keeps its place in the
  tab path. Gate the list on `rows.length > 0 || dirty`, and keep the list mounted while the rows are an
  edit: with the list gone the test's own `queryByRole('list')` returns null and its tab-order loop has
  nothing to walk.
- Three shapes of red in `TensorPlacement1.test.tsx` the code cannot answer, all proved in a copy:
  a `ruleItems` helper built on `getAllByRole('listitem')` THROWS on the empty list the test expects at
  `toBe(0)` (992); an option-list assertion indexing `[3]` of a five-option list its own preceding line
  pins (`:703` pins `deviceOptions(probe)` + the missing option, so the extra option is `[4]`, 709); and
  `en('model-profile-ot-text')`, a TP2 key, read by a TP1 test (437, `en()` throws on an unknown key, and
  the key must not exist before TP2: the brief's own "Rules as text is TP2 and is not here").
- One flag answers two contradictory dirty pins: 21 T.6 (`:864`) sends the Pattern text typed with no
  blur, so the typed text IS the rule at once, and 21 T.8 (`:1056`) then pins the same edit's Save at ZERO
  PUTs under a label that says "a save of the edited row is a change". Take the brief's rule (the text is
  the rule), report the pair. The `landed()` helper of that file is what makes a device-list assertion
  flaky: 21 T.3 (`:488`) reads the Goes on list of a rule on `''` without awaiting the probe, so it sees
  the pending `[CPU, …]` list, while 21 T.7 (`:717`) pins the ANSWERED list (`no-device` first) for the
  same device. Same file, 1, 2 and 3 reds per run, measured.

## Slice 21 TP1c (2026-11-22): 21 T.8's change is a comparison, and it costs two T.6 tests

- `override-tensor` is changed when `rowsValue(otRows)` differs from
  `rowsValue(tensorRows(loaded.values['override-tensor'] ?? null))`, i.e. `render(rows)` vs
  `render(parse(loaded))` (21 T.8). One function feeds BOTH the save body and the card's `dirty` prop
  (the emptied `<ul>` of 21 T.7 stays while it is a change), so the `otDirty` flag is gone, and with it
  its `setOtDirty` in Add, Remove, the 200 arm and Discard (all four were redundant once the value is
  derived). `rowsValue([])` is `null` and `tensorRows(null)` is `[]`, so a key the file lacks and no row
  on screen are the same state: unchanged.
- The pair the TP1b section left open, resolved against the flag: the comparison turns T.8's
  "an edit undone is unchanged" green and reddens exactly two tests of the same file, `21 T.6` at 803 and
  835. Both demand a PUT whose `override-tensor` is the value they loaded (their own sent-value
  assertions at 831 and 853 quote the loaded string), which is the observable 21 T.8's first test (1035)
  pins at ZERO PUTs. Measured on the tree both ways: flag = 1 red (T.8), comparison = 2 reds (T.6 803,
  835), full suite 2978 passing beside them. No rule told by the brief separates them (the T.6 pair is a
  kind cycled through `embd` and a Layers text left unreadable; T.8 is a pattern typed back), and the
  only rule that would is per-control, which 21 T.8's own comment refuses. The tester's edit.

## Slice MP1a (2026-11-23): a model profile created through the manager

- The manager's `CreateModelProfile` takes its pick POSITIONALLY: D-Bus `(s profile, s from, a(ss)
  values)`, `As` `(sssa(ss))` with the principal first; the pipe
  `{"method","profile","from","values"}` with `profile` and `values` REQUIRED and `from` optional (absent
  is `""`). `principal` is present only on a name ending `As`. `ModelProfileSource`
  (`crates/core/src/client.rs`) is the CLIENT's shape, never the wire's: it crosses through
  `as_source() -> (String, Vec<(String, String)>)`, so the protocol number does not move.
- A created profile is born switched off and unassigned, so ARCHITECTURE step 7 ("a `model` file is not
  checked for existence here") means the WRITE must not judge the model files. `check_contents` /
  `check_and_write` carry `judge_model_files: bool` (`MODEL_FILES_JUDGE` / `MODEL_FILES_SKIP`), gated on
  the caller's own `check_model_files`, so every existing write is unchanged.
- The append to `models.ini.disabled` is built by hand (a missing file is created CRLF, a text that does
  not end with an ending gets one, NO blank separator before the new header):
  `ini::text::replace_section` inserts a separator the byte-exact pins reject.
- `pai_core::ini::text::section_text` of a section from the MIDDLE of a file keeps its trailing blank
  separator line. That is the whole clone argument: ARCHITECTURE's "the source's bytes as `section_text`
  gives them" reads as KEEP (the `pipe-clone` / `bus-clone` pins) and the `x-clone` pin of `service.rs`
  reads as STRIP, and nothing separates the sources. Stripping ships here; hand the pair over.
- The value refusals name the KEY (`model-profile-bad-text [key]`, `comment-markers [key]`, ARCHITECTURE
  3603-3606); every other refusal of the verb names the profile, and `no-model-profile-named` names the
  `from`. A table that computes one `wanted` for all its keys is the stale pin.
- A malformed pipe frame is answered `bad_request()` and the stream CLOSES (`serve_stream` returns
  `Err(InvalidData)`): all six malformed-request tests of `manager/pipe.rs` assert `is_err()`; a stream
  that survives a junk frame is not this transport's behaviour.
- Two lints of the tester's new test code, mechanical and assertion-free: a `#[cfg(test)] pub(crate) fn`
  of an e2e added AFTER `mod tests` is `clippy::items_after_test_module` (move it above, where every other
  e2e of the file sits), and `requests(&[body.clone()])` is `clippy::cloned_ref_to_slice_refs` (the file's
  own shape is `requests(std::slice::from_ref(&body))`).
- `cargo test -p pai-manager` runs in 4 s, so sweeping one e2e's red list is cheap: snapshot the test file
  in `/tmp/opencode/<x>.pristine` with its `md5sum`, patch an expectation by exact text, run, `cp` back,
  `md5sum` the pair.

## Slice MP1b (2026-11-23): the delete, behind a create that panics

- The `if !(cond) { eprintln! }` rewrite of a blocking assert SKIPS the call, so a later phase of the same
  e2e runs on a pool the create never seeded and every mark after it is an artifact. Catch instead:
  `macro_rules! check_eq { ($($arg:tt)*) => {{ let r = std::panic::catch_unwind(
  std::panic::AssertUnwindSafe(|| { assert_eq!($($arg)*); })); if r.is_err() { eprintln!("MARK"); }}; }}`
  (and the `assert!` twin): the args run, the phase continues, and one run lists every red. A copy that
  ends GREEN prints NO captured stdout, so count the marks with `--nocapture`.
- A python patch that inserts such a macro must insert it AFTER the `fn` line (at the file top it lands
  above the `//!` module docs, E0753) and must start its replacement range AFTER the inserted head: the
  head's own `assert_eq!` gets rewritten and the macro expands into itself (E0753 recursion limit).
- The manager's model-pool fixtures pass their text through `real_model_files(dir, text)` (service.rs),
  which rewrites every `model = /models/x.gguf` to a real file under `dir/models`. A pin built from the
  fixture's LITERAL text (`"[x-clone]\nmodel = /models/on1.gguf\n"`, `section(&off, "off-one")`) can never
  equal the file: build it from the rewritten `on` / `off` binding, or from `off_text()` read before the
  write. And `section_text` of the LAST section of a file ends with a blank line, of a MIDDLE section it
  does not, so a clone pin needs `trim_end_matches('\n')` plus one `\n`.
- Three self-contradictory fixtures of the MP1 delete phases, all create-side, all the tester's: a
  `assignments.ini` with a hand `[lan-2]\n* = on` serves EVERY profile, so no profile is free to delete in
  that fixture (the accepted delete needs `* = inherit`, as service.rs's `FREE` shows, with an explicit key
  for the one profile the test wants hand-served); `section("twin")` reads `models.ini.disabled` for a
  profile seeded in `models.ini`; a `model-profile-name-taken` case names an id (`bus-both`) the phase
  never creates.
- Proof that the delete side is complete: apply ONLY those test edits in the copy, no production change,
  and `cargo test -p pai-manager` goes 343/343 green. That is the report the coordinator needs: the two
  repo reds are `tests::data_root_e2e` (the create pins, one test, every MP1 phase behind it) and
  `manager::pipe::tests::a_malformed_model_profile_request_is_a_bad_request`.
- Rust in a patch script: `values: Vec<(&str, &str)>` elements need `vec![values[0].0]` (no stable
  `str::as_str`), and inside `find(|(_, v)| …)` the compare is `v.trim() != *v` (a `&&str`).

## Slice MP1 review (2026-11-23): the create's KEY rule

- The key check must sit in the values loop, BEFORE `create_source_shape`: a pick carrying a key that is
  not `model` / `hf-repo` / `hf-file` is already refused by the shape as `model-profile-bad-source
  [profile]`, while the service e2e, `pipe.rs` and `dbus.rs` all pin `model-profile-bad-text [key]` for
  the same call (`("dup","",[("a=b","1")])`). A rule placed after the shape can never reach its own pins.
- The refusal is the value's key (`model-profile-bad-text`, naming the key, ARCHITECTURE 3605). No new
  strings key: `char::is_control` covers `\0 \t \n \r \x7f` and `char::is_whitespace` covers U+00A0 and
  U+2003, so the pair plus `= [ ] ; #` and the empty key is the whole rule, and it fires before the lock
  and before any file read, so the whole-tree `snapshot()` pin holds.
- `cargo fmt -p pai-manager --check` stays red on three hunks that are ALL the tester's new test
  literals (`pipe.rs:2517`, `service.rs:20410` and `:20420`, a case and a reply list past rustfmt's
  width): production is clean, so report them, do not reformat a test module.

## Slice MP2 (2026-11-23): the bff's model profile create and delete

- A verb added to a path another suite already owns reddens THAT suite's "the method the contract does not
  list is the router's 405" loop: `grep -n "METHOD_NOT_ALLOWED" crates/bff/tests/*.rs` and read each verb
  list. MP2 (`POST /api/profiles/model`, `DELETE /api/profiles/model/{id}`) reddened three, one per file
  (`model_profile_read.rs` `post_and_delete_on_the_section_path_are_not_served`, `model_profile_save.rs`
  `save_is_the_only_write_method_the_model_profile_path_serves`, `model_profiles.rs`
  `model_profiles_refuse_another_method_without_asking_the_manager`), each naming one of the two new verbs
  among the 405s. No code answers 405 and 204 for one request, so the loops are the tester's: drop the
  served verb, the rest stay 405. Proved by copying the tree, dropping ONLY the verb, and running the crate
  whole: 655 green.
- `cargo test -p <crate>` stops at the FIRST failing test binary, so a whole-crate run reports the reds of
  the files that ran and NOTHING after them: those three reds surfaced one per run over three runs. A
  whole-crate run is `cargo test -p <crate> --no-fail-fast`, and its red list is
  `grep -n "^---- .* stdout" -A 6` (file, line, the `left:`/`right:` pair).
- `char::is_control()` includes `\t`, so a create gate that refuses "no control character" refuses the very
  tab its own trim rule strips (the fixture is `"  /models/a.gguf\t"`). The bff trims spaces and tabs, so
  its value rule is `c.is_control() && c != '\t'`; the manager's KEY rule (MP1 review) keeps `\t` because a
  key is never trimmed.
- A source gate must refuse a non-text value outright, never drop that key: dropping an `hf-file` whose
  value carries a line break leaves `hf-repo` alone, a valid source of two keys, so a body the test pins
  400-with-zero-connects answers 201. Loop the map and `text(value)?` each value; a key outside the
  allowed three is `return None`.
- A create or delete is ONE call of ONE connect, so it takes only `write_timeout` (no read phase, no
  `WriteState` two-limit shape), and 201 is the tuple `(StatusCode::CREATED, Json(json!({"id": id})))`.
  The delete's 404 and the two 409 ride the refusal's own KEY through the module's `keyed`, not
  `not_found`: the echo rule of `checks::model_profile_id` drops a `*`, and the manager names the id as the
  page spelled it. The 422 field map is NOT the save's `field_of`: a create's checks are about the form's
  three controls (`invalid-model-profile-id` to `id`, `no-model-profile-named` to `from`,
  `model-profile-no-model` / `model-profile-bad-source` to `values`), and only `comment-markers`, which
  names the key, takes `args[0]`.
- `cargo fmt -p <crate> --check | grep "^Diff in"` lists the files rustfmt wants: with the tester's
  untracked test file in the tree that list is the proof that applying the formatter moves no test. Here it
  named `src/lib.rs` alone, so `cargo fmt -p pai-bff` was safe.

## Slice 21 TP1 review (2026-11-23): D1 the card's sheet, D2 an empty list, D3 a pending read

- A review whose fix is "the sheet gained no rule" is pinned by tests that read `index.css` as TEXT, and
  they pin by PROPERTY SET and by NAMESPACE, not by class name, so the class names are free and the
  declarations are not. The three lints a new card's CSS walks into: `Reflow.test.ts` R1.8 (every class
  the sheet selects is in a frozen list, plus one namespace the tester grants for the card, here
  `/^(ot|tensor|placement)[-a-z0-9]*$/`, and every `#id`: exactly one new one, matching a regex the test
  writes); the 720 px test (ONE block styles `.field`, `grids.length === 1`, and the rule that places the
  card must match `/#model-profile-[\w-]*(card-tensor|ot)\b.*\bdiv\b/`, carry `grid-column: 2` and NO
  `grid-row`); and the sizes test, which takes every TOP-LEVEL rule whose declared-property SET is
  exactly `['flex','min-width']` (must be two, `1 1 16rem` and `1 1 12rem`) or exactly
  `['flex-basis','min-width']` (must be one, `100%`). An extra `gap` in a pinned block, or a
  `min-width: 0` on the row container, is red.
- The size census that catches the extra one is `HearthTokens.test.ts` H1a.12: EVERY `min-height` and
  `min-width` in the sheet, as `selector: prop: value`, IN FILE ORDER, with the card's namespace filtered
  out as a group. So `min-width: 0` goes on the flex items only: a plain fieldset's block-level row div
  needs none (`fieldset { min-width: 0 }` is already global), and a `width: 100%` on a descendant is
  outside the census. A `rem`/`ch` value is fine: the sheet already ships `flex: 1 1 16rem` and `7ch`.
- Probe those pins BEFORE editing: the tests' own reader is ~30 lines (comments stripped,
  `/([^{}]+)\{([^{}]*)\}/g`, nesting by counting braces before the match), and it runs as a plain
  `node` script. `execute` has no `node:fs`, so write the script to `/tmp/opencode/*.mjs` and run it: it
  prints which rules each pin matches, which ids and classes the sheet names, and turns a CSS guess into
  a check.
- A group of controls nested in a card is a PLAIN `fieldset`, never a second `.field`: from 720 px the
  grid is `200px minmax(0,1fr)` and the inner fieldset's control column is 0 px wide, so its selects are
  18 px and "Remove" stacks one letter per line. The outer `.field` fieldset keeps its class and its ONE
  wrapper `div` is what the media block puts in `grid-column: 2`; the legend (no class) takes the label
  cell and everything else flows.
- A label and its control are ONE flex item (a `span`): as two items the pair splits at 320 px, the
  "Pattern" label landing beside an unrelated button.
- That item span is itself a flex ROW when it holds a `select` (brief 21's TP1 review D4): an item's
  `min-width: 0` does NOT shrink the select inside it, a select is no flex item there, so it keeps its
  longest option's width and the page scrolls sideways (430 of 320 px). The fix is the item's own
  `display: flex; flex-wrap: wrap; align-items: center; gap` plus `flex: 1 1 12rem; min-width: 0` on
  the select, and `max-width: 100%` on a `ch`-wide `input.number` in the same kind of item (its 10 ch
  spills the control column at 200 % text). The CSS pins then force the SHAPE of the rules: a
  `topBy`-style helper (selector compact equality, `single`) matches only a rule whose selector is
  EXACTLY one name, so each item needs its own rule, while a `topStyling`-style helper splits a
  selector on `,` and a per-name `min-width` set does too (HearthTokens `sizes` collapses
  `\s+` to ONE space), so the two selects go in one comma group.
- D3, an option list built while a read is pending: it must contain the option matching the row's own
  value, else the `select` shows a value the row does not have (jsdom answers `sel.value` with the first
  option, so a rule on NO device read "CPU: system memory"; a real engine does the same).
- D2, a `role="list"` rendered with zero items is announced "list, 0 items" beside the paragraph that
  says why there are none: render the list on `rows.length > 0` alone, not `|| dirty`.
- A prop the component stops reading must leave the props type, the destructure and the page's props
  object: `tsconfig.app.json` sets `noUnusedParameters` and `noUnusedLocals`, so leaving `dirty` in the
  destructure is a typecheck red, and the doc comment that justified it ("the card keeps the answer")
  goes with it, per the rule that a comment states no fact the code no longer shows.

## Slice 16 17a (2026-11-23): a form moved to a new page, and the four reds the move leaves

- **A form cannot be on screen before its document.** `09 S1b.6 (C2, 17a)` moves the instance page's
  case to the new profile page and makes the FORM's own read the pending promise, then asks for
  `getByRole('heading', {name: en('server-network')})` at frame 0. At HEAD that case's pending promise
  was the BOOT field's read, and the form's read answered normally, so the form was there. A page whose
  read is pending has no `doc` to hand `ServerForm`, and the brief's loading rule (17a.6: the kept text
  in a `role="status"` at once, one `.skeleton` line then `CardBox` after 300 ms, pinned green by
  `16 17a.6 server profile page` in `U17aSkeletons.test.tsx`) forbids a form in that frame. Report it.
- **A Save press on a form with no edits sends nothing, and a test that asks for one PUT after a
  client-check refusal is a test-side red.** `save()` starts `if (!dirty || saving.current) return`
  (HEAD), and the green pins are `ServerSave.test.tsx` `1b.31: failing checks send nothing` (0 PUTs)
  and `ModelProfileSave10.test.tsx` `2h.166 ... nothing sent`. `1c.44 (17a.7)` types `abc` in Port,
  checks `save-invalid` and `aria-invalid` (both pass), then asserts `bff.to('PUT', ...)` is 1 with the
  message "one save, and no second one", while its own name says "sends nothing by itself".
- **A test that migrates a case to a page with a new id often pins the OLD id's path.** In
  `ServerProfilePage1.test.tsx` `GET = '/api/profiles/server/default'` while the case opens
  `#/profiles/server/edge%20box` (so the page correctly requests `.../edge%20box`), and
  `bff.calls[0]` is the app's `GET /api/session`, not the page's read.
- **A migrated case that keeps a `beforeEach` open of another hash never reaches the page it names.**
  Four of `InstancePage.test.tsx`'s reds are that: `16 17a.10`'s row-follows and `6 (17a.10)`'s Tab path
  open `#/profiles/server/default` in `beforeEach`, so the instance page never reads the pool, and
  `bff.to('GET', GET)` is 0. Inlining the `startAt(...)` of the hash the case is about fixes the copy.
- **The focus a page owes on arrival is the page's own, never the app's.** Raising the app's focus when
  the session read resolves in `App.tsx` reddened 84 previously green tests (`GD1.12`, `5b.1`,
  `15 C1.*`, `S1.*`, `1c.46a`, `H8b.3`: they arrive on a page and expect a specific element focused).
  The shipped pattern is the page's own `useEffect(() => { document.getElementById('page-heading')?.focus() }, [])`
  (`ModelProfilePage.tsx`, copied to the 17a pages and `NotFound.tsx`).
- `console.log` from a test is invisible in the default reporter: run with
  `npx vitest run src/<file> -t "<name>" --disable-console-intercept` and grep `-A` the marker.
- Vitest's failure lines start with a SPACE (` FAIL  src/X.test.tsx > name`), so `grep "^FAIL"` finds
  nothing: `grep "FAIL  src/"`. A whole-file failure hides that file's per-test names, so compare the
  failing-FILE sets, not only the names. A full `cp -a fe` (226 MB, `node_modules` in) outside the repo
  is the copy that proves a test-side red, and `fe` needs no `vitest.config.ts`.
- **`main` is the shell's boot skeleton at the first synchronous query after mount.** The session read
  resolves in a microtask, so a test that opens the app and reads a page's own text, `role="table"` or
  `[role="status"]` synchronously sees only `<p role="status">Loading…</p>` (the boot line, not the
  page's). Four of 17a's reds are that shape; the suite's own convention is to `await` a `findBy*` first
  (`ModelProfiles.test.tsx` 2a.4 awaits the sign-in box, `ProfilesNav.test.tsx` 5b.5 the nav link), and
  the boot line's words are the tell: a red quoting `Loading…` where a page's loading key is wanted is a
  frame-0 query, not a missing loading line. A page that mounts as soon as the session lands (a page
  with no read of its own, like the pool list) exposes this where the page it replaced, whose own read
  was still open, hid it: `InstanceTagChips.test.tsx` C1.2's tab order snapshots `tabbable(p.main)`
  before awaiting the boot checkbox, whose read lands later, so `indexOf` gives -1 about half the runs.
  The test-side fix is to capture the stops after the `findByRole`.
- **A strings key a test names must exist.** `en()` in `fe/src/test/bff.ts` throws
  `no English for <key> in the test`, so `en('signin-user')` (the key is `signin-username`) reddens the
  test at its first text assertion, whatever the page does. Grep the key in `fe/src/strings/en_US.json`
  before believing a text red.
- **A fixture's `startAt(n)` gates EVERY route, not the one it means.** The generic branch reads
  `start ?? -1 >= startAt`, so with the global at 0 an unstubbed `GET /api/session` is deferred and the
  boot gate stays open, which is what a "the page's loading line is there at once" test wants. A
  `deferred()` pool answer alone does not hold it: the session read answers at once, the page mounts, and
  the page's own `Loading…` line is the only status.
- A list criterion that says an item is kept and its unreadable fields defaulted (17a.4: a non-number
  port is 0, non-array tags none, a non-string instance none) and an expected-rows array that OMITS that
  same item are one fixture, opposite rows. The page that follows the criterion renders the row
  (`['bad', '', '', 'No instance']`), and only the expected array moves.
- The cheap scratch copy for a `fe` proof: `rsync -a --exclude build --exclude .git --exclude dist
  --exclude node_modules <repo> /tmp/opencode/<x>` (32 MB) plus
  `ln -s <repo>/fe/node_modules /tmp/opencode/<x>/fe/node_modules`: no install, `npm --prefix fe test`
  runs there, and the same tree with test-only edits going 32/32 green is the proof the production code
  is complete. A `git worktree add --detach` of HEAD, same symlink, separates an inherited red.
- Full-suite flakes to state, not chase (measured 2026-11-23, all green as a file and on reruns):
  `InstanceTagChips.test.tsx` 15 C1.2 tab order (~1 in 2 alone once the Server form left the instance
  page), `ServerSave.test.tsx` 1b.36b "the edits-dropped alert stays until the page is left" (green 3/3
  as a file, red under the parallel run with the pool list still loading), `App.test.tsx` 1a.4 nav order.
