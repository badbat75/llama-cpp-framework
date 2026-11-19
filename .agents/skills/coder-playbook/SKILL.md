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

- `cargo clippy -p <crate> --all-targets -- -D warnings` builds the lib WITHOUT the tests, so a
  `pub(crate)` item of a private module that only `#[cfg(test)]` code calls is dead there. Split it: keep
  the production function, mark the wrapper the tests call `#[cfg(test)]` (a fn simply stops being built,
  so production carries no `allow(dead_code)`), and do the same with a test-only answer enum, keeping the
  narrow production error beside it. A `#[cfg(test)]` variant that production's `match` names is the shape
  to avoid, not to allow.
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
- rustfmt: it sorts consecutive `mod` declarations (`reorder_modules`), so a `mod` added out of order
  fails `--check`; it breaks a one-line `let x = if c { a } else { b };` past 50 chars
  (`single_line_if_else_max_width`), so write the multi-line form. Shortening a test with `s == ""` is
  refused by `-D warnings` (`clippy::comparison_to_empty`): write `s.is_empty()`.
- The tester's test code reddens the linter under `--all-targets`, and most of it is not yours to fix:
  `for who in [&alice]` is `clippy::single_element_loop`; `.err().expect(msg)` is `clippy::err_expect`,
  whose identical mechanical fix is `.expect_err(msg)` (apply it, name it). A fix that changes what a test
  asserts stays the tester's.
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
- One `Host` name rule over both families: strip the brackets and `text.parse::<IpAddr>()` covers IPv4 and
  IPv6 (an IPv6 with a zone id parses as no address; `Ipv4Addr` and `Ipv6Addr` each need their own parse).
  `IpAddr::is_loopback()` is `127.0.0.0/8` and `::1`, `is_unspecified()` is `0.0.0.0` and `[::]`.
- An axum handler here takes `Path(id)` first and the `Bytes` last, and an over-cap body is the
  extractor's rejection handed to `origin::body_refusal(&rejection, <fallback key>)`, which answers the 413
  (the cap is the `DefaultBodyLimit` layer in front of the whole router).

## INI, config, the manager

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
- The engine's build revision is `components::LLAMA_CPP_REVISION`: a bump moves the `.deb` Depends and the
  `.rpm` Requires and the version fixtures of `xtask/src/linux/package.rs`. The MSI's minimum is the
  release alone, so `windows/package.rs` does not follow.
- Prose in a constant whose bytes are DATA is data: the disabled-file header lives in TWO byte-identical
  source constants (the app's writer and the bff's copy), and the bff's test reads the app's source at the
  literal `const DISABLED_FILE_HEADER: &str = "` and unescapes it, so a text edit changes both sources and
  keeps that declaration spelling. A word change inside it reddens a test in another crate: revert to
  `git show HEAD:<path>` in BOTH copies, and keep the revert as a literal string, not a rule (a scripted
  rule that keys on a word inside the constant re-corrupts the fix on the next pass).

## The manager's verbs, refusals and records

- A refusal a client test decodes must be a key of `checks::KEYS` with a `text()` arm: `Refusal::decode`
  answers `Other` for any other key, so a new key added only to the strings files reddens the D-Bus and
  pipe round trips. `Refusal::new` is private and its key is `&'static str`, so manager code builds the
  struct literal.
- The startup budget of an instance is checked with the EXISTING rule, not a new one:
  `checks::startup_loads` takes the RENDERED per-instance file (the sections that carry
  `load-on-startup = true`) and `checks::startup_count` the count against the server profile's `ModelsMax`,
  so judge the render the write produces, with the record's own key. Its refusal values are
  `[count, max, instance]`: a test comparing `values == ["2"]` compares the max.
- A server profile switch is NOT the step rule: it changes no model profile, so the whole count is judged
  against the NEW profile's cap (a step down to a lower cap is refused). Judge the render cut by the
  `assignments.ini` the call read with its `on_disk` closure, never by `render_of` (that one swallows a
  read failure with `.ok()`, so a broken record reads as an empty one and the switch lands).
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

## Tests that cannot pass, and what to report

- A cascade test's self-probe can restate its own expectations as the PRE-slice sheet (H10: the probe
  wants min-height `32px` and the winner `.jump button`, while the test it probes wants
  `var(--control-h)` and `inline-flex` from the identical call). Satisfy the test, quote the pair.
- "the page adds no live region" counts every region of the page, and this one has a page-level
  fixture: the Save bar's `p[role="status"]` (`SaveBar.tsx`) predates the slice, and a brief's "what
  must not change" list naming the bar forbids the only fix. Report, do not remove the cue.

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
  `selector: min-height: value`, so any new one the brief asks for is a red there.
- `fe/.prettierignore` leaves `ux/`, `briefs/` and `DESIGN.md` unformatted on purpose: `prettier --write`
  on `fe/ux/check.js` reformats blocks the project keeps as they are, so revert every hunk but yours.
- Two assertions in one new test file can be unsatisfiable together: one pins a default box ENABLED, the
  next pins the same input `disabled` and empty with its box `checked`, from the identical fixture. The
  other one's wording governs; report the exact triple and change nothing.
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
- A pin of an instance's startup count needs the instance's `[id]` section in `assignments.ini`:
  `assign::render` leaves a model profile OFF when the assignment carries no key for it and no `*`
  (`None => keys.contains_key("*") && global`), so an instance with no assignment section starts nothing
  and the `ModelsMax` cap cannot fire. The manager's D-Bus phase seeds `assignments.ini` as `"[default]\n"`.
- A "wrote nothing" / "left no file of its own" assertion compares the file against a snapshot the SAME
  block took: a fixture that seeds a file after its snapshot, or a call the block accepted before it, moves
  the baseline the assertion then calls the verb's doing.
- To see a full jsdom dump of a failing `getByRole`/`getAllByRole` assertion, use
  `DEBUG_PRINT_LIMIT=200000 npx vitest run src/<File>.test.tsx`.

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
