---
name: tester-playbook
description: What the tester has learned about writing reliable tests. Use when writing or debugging tests.
---

Lessons only: the rules are in `AGENTS.md`.

## General

- Never wait without a deadline (`recv_timeout`, `try_wait` loop, `method_timeout`): a test must fail,
  not hang. `recv_timeout` returns `Disconnected` at once when the thread panicked (a `todo!` stub):
  report it apart from `Timeout`, or the red reads as a hang.
- A phase helper that runs the call on a thread (`on(what, &Arc<Client>, impl FnOnce(&Client) -> T +
  Send + 'static)`) takes a `'static` closure: it cannot borrow a local of the phase (`E0373 closure
  may outlive the current function`). Bind owned clones next to the call (`let (a, b) = (x.clone(),
  y.clone());`) and `move` them in; the originals stay for the asserts after it.
- Check why a test is red, or why it is already green: a test can pass for the wrong reason (the field
  did not exist yet). Recheck such tests once the code exists.
- The manager's and the core's e2e phases are ONE `#[test]` run in sequence (`src/tests/mod.rs`), so a red
  stops the run at its FIRST panic: a phase added at the end of the list reports nothing. To show a later
  phase's red, copy `tests/mod.rs` to the scratchpad, rewrite the body to call only that phase, run
  `cargo test -p <crate> -- tests::data_root_e2e`, then `cp` the copy back (never `git checkout`) and
  `grep -n` the registration to confirm the restore. A phase that redirects the data root runs alone.
- A stub inserted with `edit` lands wherever the anchor matches: the same stub inserted twice (two anchors
  both matching) is `E0592: duplicate definitions with name`. After inserting a stub, `grep -n "fn <name>"`
  and read the count; a `pub fn` stub in a lib crate raises no dead-code red, so the compile is the only
  sign.
- The shape of a callee read from a summary is a guess: `assign::render(enabled, disabled,
  assignment: Option<&str>) -> String` (not `&str`) and `ini::text::section_text -> Option<String>`, so the
  expected render is `render(pool, off, section_text(a, id).as_deref())` (the temporary lives to the end of
  the statement, so it is fine inside the `assert_eq!`) compared against `read(name)` as an `Option<String>`,
  not `.as_deref()`. A method's arity is pinned by a source-lint test in the same file
  (`write_file_if(who, name, contents, expected)`, no key flag): read that test, not the doc comment.
- The manager's render (slice: per-instance `presets@<id>.ini`): a successful write of `server@<id>.ini`
  or of a pool file renders it, so an exhaustive listing after a server-file write includes the render;
  the render text is `assign::render(pool, disabled, section)`, never the pool text (it opens with the
  generated header). A client write of `presets.ini` with a `[*]` section is refused `invalid-preset-id`
  `["*"]` (an existing check): a fixture carries the global section only by `std::fs::write` (a hand
  edit), then a legal pool-file write (e.g. `presets.ini.disabled`) re-renders it in. `* = inherit` on
  one section assigns EVERY pool preset to it, so a "preset X is the sibling's alone" pin is
  impossible under it: give the two assignment sections explicit distinct keys (`a = inherit`,
  `b = inherit`). A phase that writes into `engine_config_dir` after the redirect must
  `create_dir_all(&sys)` itself: only the CAS phase's `root(&files)` helper does it.
- A per-line exemption in a section checked by a whole-section rule: the rule runs line by line in code
  order, so a "this line is refused" pin is red today at the refusal key of an EARLIER line the
  exemption will later skip. Assert the expected refusal key, never `is_err()`, and report that red as
  the key-name, not as an existing refusal.
- Read the file you are extending before writing against it; never work from a remembered or summarised
  API. A context summary recites names (`App::app_writing`, `Outcome::Err(..)`, `rich_cfg()` in a route
  test) that the file does not have, and a whole `read` of a region can render lines that are not there
  at all: the helpers of the inline `mod tests` of a `src/` file are not the helpers of its
  `tests/<name>.rs`. Grep for the helper and the constant names you plan to use and read their bodies;
  a failed `edit` match is the signal to re-read, not to retype.
- A `#[tokio::test]` runtime is one thread: a fake that sleeps on the async path stops the test, not the
  route. A sleep inside a fake's method is safe when the route runs that method inside `spawn_blocking`
  (the bff's save: connect, read, write, status in one blocking task). The bff's session routes
  (`/api/session`, sign-in) never connect, so a connect hook is inert for them.
- An independent rerun of cargo commands that prints `Finished ... in 0.0xs` is cargo replaying a cached
  pass (its fingerprint does match the current sources: `cargo check -v` prints `Fresh <pkg>`). To compile
  the coder's files yourself: `cargo clean -p <pkg>` first; it covers only the host target, so force the
  cross target with `cargo clean -p <pkg> --target <triple>`. The rerun's own log proves the compile when
  it prints `Compiling <pkg>`; a follow-up `cargo check -p <pkg> -v` printing `Fresh <pkg>` confirms the
  fingerprint (cargo's own `-v` changes no rustc fingerprint). For clippy `-v` is NOT a neutral probe: the
  clippy driver records `CLIPPY_ARGS`, so any `-v` run reports `Dirty <own pkg>: the environment variable
  CLIPPY_ARGS changed` and re-checks (that run is proof too, it re-runs every target). Write the `-v` log to a file and grep
  that file: piped through `head`/`grep -c` the pipe closes early (SIGPIPE), the log truncates, and the
  `Fresh <own pkg>` line (late in the dep list) reads as missing. A `grep -c` that counts 0 exits 1, so a
  chained `cargo ...; echo exit=$?; ...; grep -c ...` prints a trailing `Exit code: 1` after green runs:
  report the per-command `exit=$?` lines, not the chain's. Bare cargo commands here land in
  `build/windows/cargo-target` (the generated `.cargo/config.toml` pins `target-dir`, on Linux too).
  A clean rerun IS fast (0.6 s for every pai-bff target on 24 cores, deps cached), and its own log is the
  proof: right after `cargo clean -p <pkg>` clippy prints `Compiling <pkg>` when it builds the package's
  own artifacts from scratch, and `Checking <pkg>` when a `cargo test` of the same sources left them
  cached (checked: pai-manager, `Checking pai-manager` + `Finished in 0.98s`, exit 0: a `Checking` line
  is the real check, not a replay). cargo prints that line ONCE for the whole package, so a count of 1
  covers the lib test, the bins and every `tests/*.rs` binary (pai-bff: 15 of them; pai-manager: 2
  binaries, the 259-test lib, which holds `tests::data_root_e2e`, and a 0-test bin). Cargo indents its status lines (`   Compiling pai-bff ...`), so
  grep `Compiling <pkg>` with NO `^`: an anchored count is 0 on a run that really compiled, and `grep -c`
  exiting 1 reads as a failure of the chain. The next plain `cargo test` replays the compile
  (`Finished ... in 0.05s`, no `Compiling`) yet its own `Running <bin>` and per-test `ok` lines prove the
  binaries ran. The `-v`
  count of `--crate-name <target>` works only on a run that really checks: a `Fresh` run prints NO
  `--crate-name` line at all (checked: 0 after a clean run), so from `-v` take the `Fresh <own pkg>` /
  `Dirty <own pkg>` line, which says the previous run's fingerprint matches the current sources. To prove a green
  `-D warnings` has teeth without touching the tree, add a restriction lint on the command line
  (`-- -D warnings -D clippy::expect_used`): exit 101 with diagnostics names the files, on both targets.
- A clean rerun of the whole configurator side is cheap, so order it, do not reason about replays: `cargo
  clean -p` of pai-core, pai-app, pai-config, pai-cli, pai-bff removes 5.2 GiB and the five `cargo test`
  runs take 2 min total on 24 cores (core 7 s, app --lib 19 s, cli 4 s, config 61 s, bff 21 s). Run the
  sequence as ONE background script that writes a log per command and appends `exit=$?` per command to one
  exits file: a shell chain's exit hides which command failed. Each log's single `Compiling pai-<pkg>` is
  the proof that crate compiled; a log with `Compiling pai-config` AND `Compiling pai-core` AND
  `Compiling pai-app` is normal (the cleaned crates it links are rebuilt too).
- A `cargo test` rerun always runs the test binary; only its compile can be a replay. `-v` (before
  `--`) shows `Fresh <pkg>` (fingerprint checked against the current sources) or `Dirty`/`Compiling`.
  For the independent check after a coder, `cargo clean -p <pkg>` and rerun when in doubt.
  One probe for both facts when a run's log has 0 `Compiling` lines: `cargo test -p <pkg> --test <a>
  --test <b> --lib -v -- --list > file` writes to the file, then grep it for `Fresh <pkg>` (current
  sources) AND for the slice's test names (they are in the binary that ran).
- One fixture per test: an id from one temp fixture will not match another. Use real names and values
  captured from the real system; a name-prefix match is tested with every real name sharing the prefix
  (`llama.cpp-v` starts `llama.cpp-vulkan-v...`), fixtures built from the full real file set.
- A fake holds whatever you write; the real system can answer differently. A green on a fake is no
  proof about the real thing.
- An assert message is a format string: a `{:?}` in it with no argument is a compile error ("1 positional
  argument in format string, but no arguments were given"). Write `{{:?}}` to name Rust's quoting in the
  message.
- `let got: Vec<&str> = log_of_the_call().iter().map(String::as_str).collect();` is E0716 (the cloned Vec
  is a temporary, freed while `got` borrows it): bind it first (`let recs = log_of_the_call();`).
- A seam the core does not call yet: a private field only ever written (`action_log` behind a `with_log`
  setter) raises NO `dead_code`/clippy red in a lib crate (checked: `clippy --all-targets -D warnings`
  clean), so an unwired setter has no stub red to report and the reds are the assertions.
- A `cases: [(&str, fn(&dyn ManagerClient) -> Option<ClientError>, Value); N]` table trips
  `clippy::type_complexity`: alias it (`type Case<'a> = (...)`), and rustfmt wants that alias written
  multi-line.
- Prefer `todo!("<fn>")` stubs to a compile error, which hides every other test in the binary, lints
  included. The exception the coordinator may order for a small pure helper: tests importing a not-yet
  private `super::` fn/enum red as `E0432: no <name> in <module>`; the whole test binary stops
  compiling, so say the existing tests in the crate did not run. Assert with `matches!` so the enum
  needs no `PartialEq`/`Debug` derive. A `todo!` body does NOT excuse the parameters: `-D warnings` fails `unused_variables`, so
  underscore-prefix them and tell the coder to strip the prefixes. A stub reached only from tests fails
  clippy with dead code until wired in: say so, add no `allow`; a `pub` stub in a lib crate is not dead
  code, so there is no dead-code red to report. A new parameter on a core method: a stub that takes it
  and keeps the old body gives assertion reds in every phase, where a `todo!` stops the e2e at its first
  call.
- `pub const X: Duration = todo!()` is NOT a stub: const-eval panics at the definition (`E0080`) and
  stops the crate. Stub a constant with today's behaviour's value; the red is the assertion.
- To pin a field a variant lacks without breaking the build, match `Variant(p, ..)` / `Variant { .. }`
  and check the new field through `{:?}`; tell the coder the intended shape. Naming a stub after a local
  of the caller makes the call `E0618` (the local shadows the fn): rename one and say which.
- Before handing over a large set of reds, prove they can go green with a throwaway candidate: back up
  every file it touches (stub, strings, wired files) to the scratchpad, write the candidate, run, copy
  back, `touch` the restored sources (a `tar` restore keeps old mtimes and cargo runs the stale
  candidate build), check `git status`. If the permission classifier refuses editing production files,
  leave that layer unproven, order the asserts so the earlier ones run first, and say so. A data file
  pinned by a test is the coder's: check by reading that all assertions can hold at once.
- A file the current slice created is untracked: `git diff` on it prints nothing, which is no sign your
  edits were lost (`git status --short` shows `??`).
- Never `git stash` or `git checkout --` to compare: other sessions edit the same tree. Run one test on
  its own, or probe in a scratch copy and `cmp` it back. Plant lint violations in a scratch copy, never
  the real tree.
- Several filters go in one run: `cargo test -p <crate> -- a b c`; a loop of runs costs minutes each.
  `cargo test --test a --test b` stops at the first failing binary: add `--no-fail-fast`, which is a cargo
  flag, so it goes BEFORE the `--` of a name filter, never after it (after it, libtest rejects it).
- Format one file with `rustfmt --edition 2021 <file>`, then `cargo fmt -p <crate> --check`.
- Clippy traps in test code: a doc line starting `1)` (`doc_lazy_continuation`); `&Path::new(..)`
  (`needless_borrow`); `option_env!("X").expect` (`option_env_unwrap`: use `env!` and accept the
  compile error as the red); `count >= n + 1` (`int_plus_one`: write `count > n`); a `///` on a `let`
  (a closure helper inside an e2e fn) is `unused_doc_comments`, use `//`; `out.push_str(*line)` over a
  `&[&str]` (`explicit_auto_deref`: write `out.push_str(line)`, the argument coerces on its own).
- A cargo filter is a SUBSTRING, so a new test name can drag in an old one: `-- remove_key set_key`
  also ran `preset_keys_with_no_check_today_pass` (`preset_keys` holds `set_key`). Account for every
  non-red line by name, and name new tests so they cannot collide with an existing name.
- `cargo test` takes ONE filter as its positional; further names are "unexpected argument". Several
  filters go after `--` (above); a filter that matches only bins prints "0 tests" noise: add `--lib`.
- `assert_eq!(log_of_String, vec![], ..)` is E0283 (many `String: PartialEq<_>` impls): write
  `Vec::<String>::new()`, the pattern this repo already uses.
- A `[(label, |c| ..); N]` table of setters: `as fn(Cfg) -> Cfg` on the first element fixes the element
  type, so a later closure that captures a local is E0308 "expected fn pointer, found closure". A table of
  values (`(key, threads, threads_batch)` with `Some(n)`/`None`) has no coercion to fight.
- A pure function MOVING crates (not wrapped): the behaviour tests move with it, calling `crate::<mod>::<fn>`
  in the destination's inline `mod tests`. The red is E0425 "cannot find function" on every call site, which
  stops that crate's whole test binary: report the count and that its existing tests did not run. The source
  crate keeps ONE behaviour test through the public entry (green before and after the move, machine-derived
  bounds computed in the test: `cpus + 1` from `available_parallelism`) plus a source lint over the code half
  that requires the re-export line and forbids `fn <name>(` (a second copy of the rule).
- Comments never cite PLAN.md step or slice numbers; name the feature.
- rustfmt reorders consecutive `mod` declarations: a new `pub mod x;` goes in the sorted spot, with its
  own comment line (it travels with the item). A bare `assert_eq!(a, b)` (no message) over ~60 chars of
  body is split one-per-line by rustfmt; adding a message keeps it one line. Format each file
  (`rustfmt --edition 2021 <file>`) as you write, `cargo fmt -p <crate> --check` at the end.
- A helper that reads a generic writer (`fn written<W: Write>(log: &WriterLog<W>)`) needs `W: Clone` for
  the `.clone()`: pin it to the concrete buffer (`&WriterLog<Vec<u8>>`) and read a custom sink's own field
  in the test that uses it.
- "Unwritable path" that also fails for root: a path component that is a regular file (`ENOTDIR`, both for
  `create_dir_all` and the open), never a chmod (root ignores modes). A `read_dir` listing is unordered:
  sort before comparing to a `vec!`.
- Appending file sink ("path cannot be written loses the record"): making the PARENT a directory is NOT an
  unwritable case, the sink rightly creates the file there (the assert `!path.exists()` reds on its own
  file). Pin it as the LOG PATH ITSELF being a directory (`create_dir_all(&path)`, the append open fails
  `EISDIR`): assert `path.is_dir()` (catches a sink that removes the dir and writes a file) and a
  `read_dir(&path)` that is empty (catches a fallback written inside). The temp root's own listing misses
  files created inside a subdirectory, so it is not the "nothing was written" check for a nested path.
- Compile the test binary early and often with `cargo test -p <crate> --lib -- --list`: it type-checks
  every test without running one. The shared tree moves under a slice (HEAD changed `Caller` into
  `Requester` + `AsRequester` while these tests were being written): re-read the seam's signatures before
  finalising asserts, and read the red to see which seam moved.
- `git grep` sees only tracked files: after an un-added move, use `grep -rn`.
- A run-time-read fixture (`read_to_string(CARGO_MANIFEST_DIR/..)`) reds naming the missing file;
  `include_str!` would stop the crate compiling.
- A staleness or "not written again" rule is tested with `File::set_modified(UNIX_EPOCH + secs)`: no
  sleeps, both orders. Expected text can come from the code itself on a sibling temp path (`startup.rs`
  `fresh_entry`), keeping the test free of the machine's binaries.
- To make a write fail: chmod the directory `0o555`; to make a read fail while the atomic write still
  succeeds: chmod the FILE `0o000` (a directory in its place fails the rename too: green for the wrong
  reason). Probe that it really fails (root ignores modes; a passing run hides the skip's `eprintln!`,
  check with `--nocapture`), restore the mode before asserting; check old content and an unchanged
  listing (no temp file left).
- Force a copy fallback by injecting the rename (`&dyn Fn(&Path, &Path)`) and failing it for one path.
- To make a real rename fail: a DIRECTORY at the target name (a file cannot be renamed onto one,
  `EISDIR` on Linux, checked with python3's `os.rename`; empty is enough, non-empty is the firmer fixture).
- A "one deadline bounds the WHOLE request, not one read" pin: a fake that writes its reply one byte
  every 300 ms (`for byte in text.as_bytes()`, `if write_all(&[*byte]).is_err() { break }` then sleep: a
  broken pipe is the client giving up, so the fake thread dies with it, no leak). Run the call on a
  thread with a `recv_timeout` channel: the red reads "did not return within N s", never a hang.
- A `format!` with no placeholder (a fake's canned reply body) is `clippy::useless_format` under
  `-D warnings`: write `.to_string()` on the raw string, with single braces.
- A real chown(2) seam works without root: an owner may give a file any group it is in (`id -Gn`).
- A fake one-shot runner's `ExitStatus` on Unix: `ExitStatusExt::from_raw(code << 8)`, under
  `#[cfg(unix)]` (the user's Windows `cargo test --workspace` compiles these tests too).
- A fake child for an injected spawner: `sh -c '<printf ...>; exec sleep 30'` (`exec`, so your pid is the
  one killed); "killed and reaped" is `/proc/<pid>` gone, polled with a deadline; `'` is `'"'"'`.
- A shell snippet is checked with `sh -n` on stdin; an `at(text, needle)` position helper pins presence
  and order.
- A throwaway git repo in a test: `-c user.name=.. -c user.email=.. -c commit.gpgsign=false -c
  tag.gpgsign=false`, `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_NOSYSTEM=1`.
- Review of a check: one that re-observes what was already true before the action proves nothing;
  mutate first, then check the action undid or kept it.
- `cargo test -p <crate> --test <name> -- --list` type-checks one integration test binary (and its
  helpers) without running a test: the cheapest way to see the whole file compile.
- `crates/bff/tests/pai_bff_binary.rs` spawns the real binary and grabs ports through `free_port()`
  (bind, release, rebind): one test in 12 flaked on the first cold run after `cargo clean` and never
  reproduced (11 full-suite greens + a solo `--test` run). Probe solo, then rerun the suite 2-3 times
  before reporting a red from that binary.
- bff oneshot: `Request::builder().header(name, value)` appends (`http-1.5.0/src/request.rs:899`), so
  the same name twice reaches the router as two values; `HeaderName::from_static` wants the lowercase
  name. A header the bff cannot read: `HeaderValue::from_bytes` accepts `>= 0x20` except `0x7F`, plus
  tab (`http-1.5.0/src/header/value.rs:563`) and `to_str` fails on any byte `>= 0x80` (`:249`): that
  value is on the wire and unreadable, which is the shape a guard must fail closed on.
- axum 0.8: `get(handler)` answers `HEAD` as well (axum strips the body, so "the body is empty" proves
  nothing for `HEAD`); a method the path does not route is the router's 405 with an empty body. So the
  pair "405 for this app's own page, 403 for a foreign page, same verb" is what pins a guard that runs
  before the router; when a guard checks a verb list (`writes`) rather than the method, pin that half
  and report the list as the scope question.
- A pure body-to-value step: build every failure body from the FULL well-typed body with ONE key
  changed, so the refusal cannot come from a key the test left absent (an absent key has no JSON type,
  the design leaves it open, a coder reading it as 400 reds a 422 test for the wrong reason).
  Pin `render_for(cfg, true)` against `false` with the blank-dirs case only: with both dirs set the two
  renders are identical, so the equality of the whole file cannot tell the flag.
- The INI value asymmetry a writer must refuse: the bff trims a value with `trim_matches([' ', '\t'])`,
  the core's reader trims with `str::trim`, which is Unicode-aware (`ini.rs`: `line.trim()`, then
  `t[eq + 1..].trim()`), so a value with U+00A0 or U+2028 at an END is written whole and read back
  shorter. `char::is_control()` is the Cc category only, so both slip past the control-character rule. An
  interior U+00A0 survives the reader (read back identical), so pin that as the guard of the rule's scope
  and keep the plain space/tab ends accepted (they belong to the trim rule).
- A `for value in [..]` reds at its first element, so one run proves one element. Prove the rest are red
  for their own reason with a standalone `rustc --edition 2021` probe that replicates the rule's
  predicates and the reader's trim and prints the answer per value (20 lines beat copying the crate out of
  the workspace, where `path = "../core"` deps break).
- A `pub(crate)` stub in a PRIVATE module, reached only from tests: `-D warnings` fails `dead_code` on
  the function, the enum and its variants (3 messages, lib and lib test). `pub` does not clear it (the
  module is private, so the item is still unreachable), and the caller that clears it is a later slice:
  report the three, add no `allow`.
- A rule about the `Host` name touches no bff test that sends no `Host` (the `App` helper sends one only
  when an extra names it): pin the no-Host path as accepted (HTTP/1.0, `curl`), and give every refusal a
  shape where only that rule can answer (a non-text `Host` with no origin header, a non-text `Origin`
  with no `Host`), else one red hides behind the other.
- A rule that becomes a function of an INJECTED context (the `Host` rule of the listen address): the
  accept side is GREEN at once, a stub that ignores the new argument keeps today's answer, so the reds
  are only the refusals the new context adds. Report the green accepts as guards, name them, and give
  the refusal both shapes (the rebinding pair, and the bare `Host` with no fetch metadata).
  `tests/origin.rs` now: `app_with_hosts(hosts: &[&str], listen: SocketAddr)`, `new_app()` is a loopback
  listen, so every pre-existing loopback test keeps its meaning; a LAN-name test hands
  `LISTEN_LAN = 192.168.50.210:8590`, a localhost test a loopback or the wildcard.
  A "a trailing dot is the SAME name" rule (the FQDN root): only the ACCEPT side is red, every refusal
  the rule must keep is green today (a `Host` of `.` / `..` / `name..` / `localhost..` is refused now,
  and stays refused: the test is the guard that a fix strips ONE ending dot and not all of them, so
  report it as green, and make the double dot use a name whose bare form IS in the list, where a fix
  that strips all dots goes red). A blank-host-name rule: the `Some(hosts)` side is green at once (the
  list path never reads the host name), only the automatic-names list is red.
  For a `tests/*.rs` file, proving the asserts a first red blocks: `cp tests/x.rs tests/zz_probe.rs`
  (a copy in `tests/` is its own binary, helpers included), `sed -i '<ranges>d'` the predecessors,
  `cargo test -p <crate> --test zz_probe -- <filter>`, then `rm` the copy BEFORE clippy and `git status`
  (one sed call, both ranges against the original line numbers).
- The header value lists `at_host`/`host_line` build are `Vec<(&str, HeaderValue)>`: they fit only
  `ask_raw`/`sign_in_raw`/`boot_raw`. Handing one to a text helper (`post_with`, `delete_session_with`,
  `put_with`, which take `&[(&str, &str)]`) is E0308, one error per call site.
- `App::sign_in` asks the fake authenticator: after it, compare `asked()` with the baseline taken right
  after the sign-in, never with `is_empty()`.
- The bff's save replies `{"revision", "running"}` only, so a SECOND save in one test builds its body by
  hand: the `revision` from the 200, the `values` from the read (`values_with(&read_doc, ..)`). A body
  built from the 200's document panics in the helper (`expect("values")`).
- The bff's fakes: `FakeManager::refused()` gates `read_file` only, so a read-refusal list refuses reads
  and leaves the writes free (that is what pins "the read refused ends the session, nothing is written").
  A list of principals there, not a flag: `Live::write_refused` and `Live::read_refused` are both
  `Vec<String>` read with `.contains(&principal)`. When a handed harness returns on `refused()` BEFORE
  the list check, the list field is never read: its setter refuses NOTHING, `-D warnings`'s
  `field is never read` is the only red naming it, and every test built on that setter answers 200.
 - The `tests/server.rs` fake answers a save at three points, one pair of cells each (`write_outcome` +
   `write_pause_ms`, `read_outcome` + `read_pause_ms`, `connect_outcome` + `connect_pause_ms`, all inert
   by default so adding them greens everything): the 503 and 504 of a save turn on which call was
   reached, so a test sets the cell AFTER the form's own read, letting the read that carries the
   revision succeed. Its call log records the trait methods only (`read <name>`,
   `write-if <principal> <name>`, `status <id>`): the `As` methods are not logged, and the route's own
   re-read adds a SECOND `read server@default.ini` after the GET's, so the log of a save stopped at its
   read holds two reads, and one stopped at the connect holds one.
 - A route that reads a file back after its writes (the switch re-reads `presets.ini` so the `enabled`
   it answers is the text on the machine): the exact-call-list pin of that route lists the read-back
   too, in the position the route makes the call (read the route's body, not the doc comment of the
   test). An exact list of six calls beside a sibling test that demands a read after the last write is
   a contradiction: keep the read-back (it is the contract), extend the list to the calls the route
   makes, and rename a test called "reads both files once" once the list repeats a read.
- The race "the route answered and the blocking task went on": dropping the `JoinHandle` of a
  `spawn_blocking` task does NOT stop it, so a save cut off at the connect or the read answers 503 and
  its task writes when its parked call returns. A fixed sleep cannot test it (the write lands when the
  read returns, not after a known delay): give the fake a one-shot GATE (an `AtomicBool` armed by the
  test and consumed by the next call, which parks in a 5 ms poll loop bounded by a `GATE_LIMIT`, counting
  `gated` on entry and `released` on exit). The test arms it after the form's read, awaits the 503,
  asserts `gated == 1` and nothing written, releases, polls the `released` counter with a deadline, then
  polls `writes()` for a bounded `GRACE` and asserts it empty. Keep the gate OUT of the shared calls log
  (a `record` there breaks the exact-log asserts of the sibling tests); give it its own counters. Parking
  inside the fake's method is safe because the save's connect/read/write run in `spawn_blocking`, and a
  `tokio::time::sleep` poll loop after the route answered is safe (the single-thread runtime gets to poll
  the task's completion). `GRACE` is a heuristic bound: a fixed task writes nothing at all, so a green
  burns the whole `GRACE`; 500 ms against a 200 ms timeout is enough on this machine, and the red exits
  the loop at once (the write lands ~1 ms after the release).
- `serde_json::Value` maps in this repo sort keys, so an object's key order in a reply is not the
  contract: a "the keys are these names, in this order" pin must sort BOTH sides (the `json!` literal
  arrives alphabetical, a `Vec<&str>` of field names does not).
- A unit expectation of "the machine's CPU count" is red by construction (dev box 24, the test module's
  `CPUS` 16): pin the wiring in the unit test (the bound it is handed) and the machine's count through
  the public API in the integration test (the `limits` maximum accepted, `max + 1` refused naming the key).
- A test the coder reports as unpassable: check it against the design and the fake's behaviour, and when
  the expectation is simply wrong, do not just delete the assert, cover an uncovered path of the same rule
  (the write-refusal 403 was pinned, the read-refusal 403 was not).
- `-D warnings` scans `tests/*.rs` and `#[cfg(test)]` too: `.iter().any(|u| *u == x)` is
  `clippy::manual_contains` (write `.contains(&x)`), and a field added to a fake that nothing reads is a
  `dead_code` red. The lib test binary of the crate a slice is writing often does not compile until the
  module's helpers are fixed, so its unit tests only run after that: report that they did not run before.
- A bff route that is not routed yet: EVERY status assert in the new `tests/<route>.rs` reds at the
  router's 404 (the 405 test too, it greens once the path exists with GET only), so the file is N status
  reds plus the guards that hold by construction (the sibling route still answers). Say which green is a
  guard, and prove the red is the missing route, not a broken helper: the same red `left: 404` on every
  status, one per expected code. A test that EXPECTS 404 passes its status assert by accident on that
  router reply, so pin its body too (key, and `args`): its red is then
  `left: Content-Type "" right: "application/json"`, which names the missing route.
- The bff does not link `pai-app`: a route listing preset SECTIONS parses through
  `pai_core::ini::parse_sections`. Its header is the contract for what a fixture yields, read it instead
  of running a probe crate (pai-core built from a separate workspace: minutes): values TRIMMED, everything
  from the FIRST `;` or `#` on stripped ("a;b" reads "a", so a fixture value must never contain one), key
  lookup EXACT-case (`keys.get("model")`), section order kept, a bare `[a]` is a section with no keys. So
  `model = x ; note` answers "x" and a section without the key answers `""` through `unwrap_or_default`.
- `fe/briefs/api-contract.md` and the served code can disagree on a 403 key: for `GET /api/presets` the
  contract lists 403 `client-not-authorized`, while the served signed-in read (`GET /api/server`) answers
  403 `session-not-administrator` and ENDS the session when the manager refuses the `As` principal (PLAN:
  a refusal ends the session). Pin the served sibling, and report the contract line to the coordinator as
  the one assert to change if they choose the contract's key.
 - "A write that LANDED is answered 200, not 504, when the read that follows it runs out": 504 stays only
   where the write's own outcome is unknown. That flips the existing post-write-504 tests (the tester
   changes them, the coordinator says so) while the write-parked 504 tests stay as the guards of the rule.
   To slow ONLY the read-back (the route reads the file again after its writes, e.g. the switch's
   `enabled`): give the fake a positional pause on the n-th read (`Live::set_read_pause_at(3, ms)`, a
   `read_count` incremented in `read_file`), so the form's own read that carries the revision stays fast.
   Pin the 200's body with the value the read-back CARRIES (the switch answers the file's, not the move's),
   and with the value the move produced when the read-back itself is the one that runs out.
 - A new optional parameter on a shared builder (`router_with_session`): add a sibling
   (`router_with_session_and_write_timeout`) named after the existing seam pattern
   (`session::routes_with_sign_in_timeout`), keep the old arity delegating, and stub it `pub` (a pub stub
   in a lib crate is not dead code, no lint red) with the new argument inert, so the reds are the
   assertions. The old harnesses then need no edit: only the harness that pins the new rule passes the
   value. Time-limit constants that are private to the bff lib (`MANAGER_TIMEOUT`) are NOT reachable from
   `tests/*.rs`: say the unit pin must live inside the crate, and pin the rule behaviourally in the
   integration file (a call that outlives the read limit but not the write limit answers 200 under the
   write limit, a GET of the same delay answers 503, so the limits really differ).
 - llama-server's global `[*]` section is not a preset: `ini::parse_sections` strips the brackets, so its
   id is `*`, and `checks::preset_id` refuses `*` - a switch of it is 404 `no-preset-named` with EMPTY
   `args` (the id never passes the check to be echoed). Test the id raw, url-encoded (`%2A`) and bracketed
   (`%5B%2A%5D`): axum's `Path` decodes all three to `*`, and the list-exclusion pin needs the section in
   `presets.ini` with real keys beside a real preset. The same rule answers `.` and `..` with empty args
   (`preset_id` refuses a path of dots), so a "the id is echoed back" pin takes the plain names.
 - A preset route reads `presets.ini` and `presets.ini.disabled` through `ManagerClient::read_file` (as
   `src/presets.rs::list` does), NOT an `As*` method: a new preset read needs no new trait method, and its
   call log is `read presets.ini` then `read presets.ini.disabled` over ONE connect. Keep the fake's files in
   an `Arc<Mutex<Vec<(String, String)>>>` shared into the connect hook, with the log closing over it: a test
   can then change a file between two reads (the mid-read 503) while every other test builds its own app.
 - A digest the route computes (the section's revision): take the pinned constants from an INDEPENDENT
   implementation written outside the repo (a `python3` SipHash-2-4 in the scratchpad, validated by
   reproducing the vectors published in `tests/server.rs`), paste the hex into the test and name in its
   comment what it is the hash of. In the test recompute the expected value from the same input as a
   self-check, and refuse the near misses: the whole file, the section text alone, a neighbour section and
   the unkeyed hash (`fnv1a`, copied from `tests/server.rs`). Pin the digest's INPUT behaviourally, not only
   by constants: three files that parse to the SAME keys (one key written twice, the same text in CRLF)
   answer three tokens, and a file with blank lines after the section answers the token of the one without.
   A digest of a canonical re-render of the parsed keys passes the constants and fails that.
  - A helper IN THE TEST FILE that recomputes the digest is the weak link: a SipHash-2-4 written from memory can
    init `v2 = "genersa" ^ "Parvate"` and `v3 = "sbytesd" ^ "Gomoliris"` (no key at all, the four constants
    XORed in pairs), which reproduces a broken route for EVERY input, so the helper-vs-route assert greens while
    both disagree with the pinned constants. Validate such a helper against the published vectors (key `00..0f`:
    `""` = `726fdb47dd0e0e31`, one `00` = `74f839c593dc67fd`, `00..07` = `93f5f5799a932462`, `00..0e` =
    `a129ca6149be45e5`) and pin four in the file (empty, one byte, a half block, a full block plus 6: each one
    separates the broken init from the reference one). The init is `v0 ^= k0; v1 ^= k1; v2 ^= k0; v3 ^= k1`.
    Name every pinned constant with the bytes it digests: a `the_revision_is_stable_across_another_section` that
    asserts `REV_NEIGHBOUR` (documented as the digest of `[Bigger]`) against a read of `[Zeta]` greens on a route
    that digests the WHOLE file, and its "differs from the one-section answer" is then the only red. Each test's
    expectation is the constant of ITS section.
   - A fixture that appends a section must append the section CONST verbatim: a `ALPHA_PLUS_ZETA_CRLF` whose
     `[Zeta]` holds ONE key digests something else than `REV_ZETA` (the digest of the two-key `ZETA`), so the
     read of it reds `left: 0cc070d44f6d02ae right: 06d798fe3318872c` while the route is right. Fix the
     fixture, and put `revision_of(&KEY, ON, ZETA)` beside the constant as a self-check, so a fixture that
     drifts from the const reds. Check a claimed digest mismatch by extracting the test file's own helper
     (`sed -n '<lines>p' tests/x.rs > helper.rs`, `include!("helper.rs")` in a `rustc --edition 2021` probe):
     it reproduces the published vectors, so its numbers are the reference (a python SipHash written from
     memory failed every vector).
  - When the rule CHANGES the digest's input (the section's text: cut its last non-blank line's own ending, so a
    last section with no final line break stays put when the break is added or a section is appended): write the
    scratch program so it prints the value of the OLD rule for every fixture and check those against the constants
    being replaced (all six matched: the scratch model of the input is the route's, so the new constants are the
    rule's and not a typo). Compute the new ones from the fixtures, and add the rule to the TEST's helper
    (`hashed_section`), so every existing `revision_of` use needs no edit. A fixture with NO final break greens
    under both rules (nothing to cut): its test is a guard that the cut does not eat the section, report it as
    green, and make the red the form that has an ending to cut.
  - A path below an existing route (`/api/presets/alpha/%2A` when only `/api/presets/{id}` and
    `/api/presets/{id}/enabled` are routed) is answered by the ROUTER, not the app: 404 with empty Content-Type,
    empty body, ZERO connects. The router's 404 carries the app's JSON only where a path exists under the prefix
    (`*`, `%2A`, `%5B%2A%5D`, `..` reach the handler); a path below it is bare, and a GET of a sub-segment
    (`alpha/enabled`) is a bare 405, so the app cannot serve one without colliding with the method's route. Pin
    such a form as its own test (status + `content_type() == ""` + body `""` + `connects() == 0`), never as one
    element of the app's list: its `left: Content-Type ""` red reads the same as a missing route.
  - The sandbox clock jumps: a `cargo clippy`/`test` log written AFTER your last edit can say `Finished in
    0.06s` with zero `Compiling` lines (cargo compares mtimes, and the file's moved backwards), i.e. a replay of
    an older check. A fresh log with 0 `Compiling` lines is no proof: `cargo clean -p <pkg>` first (the real
    check of pai-bff's 16 targets prints `Compiling pai-bff` once and finishes in 0.6 s, deps cached).
 - A `Reply` accessor that reads a field of `json()` returns `String`: `json()` hands an owned `Value`, so an
   `as_str()` of it borrows a temporary (E0716). Read the STATUS before any body field: on the router's 404
   of an unrouted path the body is empty, so a field read first panics "not JSON (EOF while parsing)", which
   hides the missing route.
 - The split marker of a source lint that pins "the app calls the core's fn, it keeps no copy" is
   `mod tests {`, NOT `#[cfg(test)]`: `crates/app/src/presets.rs` has ten `#[cfg(test)]` items from line 66,
   so a split at the first reads a header with no calls, and the require-needle goes red for the wrong
   reason. The require-needle (`"<fn>("`) is green today at the call site, so the red of the move is the
   forbid-needle (`"fn <fn>("`): report which line reds.
- A design that accepts a CLOSED list of names ("the 84 keys of the brief's table") is not a list to invent:
  the brief's prose names are often wrong (`frequency-penalty` is `frequency_penalty`, `chat-template` is
  `chat-template-file`, `rope-scaling-type` does not exist: about 40 of my 64 guesses were not real keys).
  Rebuild it from the authoritative sources and cross-check both: the check tables (`crates/core/src/checks.rs`
  `PRESET_INT_KEYS`/`PRESET_FLOAT_KEYS`/`PRESET_ENUM_KEYS`) and the reader that consumes the key
  (`crates/app/src/presets.rs`), and report how you built it. When the test loops over a constant the coder
  completes, that list travels in production code: a name that reaches no rule is a key no check guards.
- A list constant the coordinator orders left EMPTY for the coder: every test that iterates it is vacuously
  GREEN, so name that in the report and put the red in a presence assert (the constant `contains` two named
  keys) and in a literal case beside the loop (`foo`). The loop over an excluded list is vacuous too.
- Verify a design's key list against the writer before pinning them equal, no compile needed: a python pass
  over the writer's lines. One regex is not enough: the keys arrive as `emit_*(&mut out, "k", ..)` (often
  split across lines, needs `re.S`), as `format!("k = {}` and as a bare `push_str("k = true\r\n")`: my first
  pass found 2 of 75. Feed it the design's union, print both diffs.
- `ini::Section::keys` is a `BTreeMap`: its keys come out sorted, so a `Vec<&str>` of them compares straight
  against a sorted union. `render_section` has no `if` guard around an `emit_*`, so a fixture with every
  field set (`full_preset`) yields every key the writer can emit, and a union-equality red prints the
  writer's own list: read the count there to prove the fixture needs no change.
- `set_key` appends a key the section does not have AFTER the section's last real line (a blank line at the
  end of the file stays below it), so two new keys of one body land on the two lines below the line the
  section ended with. Pin that: write the body keys sorted (the order then holds with and without
  `preserve_order`), assert the two lines at `from.len() + 1` and `+ 2`, and prove the rest of the file byte
  for byte with a `replaced` anchor whose window ENDS at the section's last line, repeating that line inside
  `to` (the file's own last two lines are now the appended ones, so an anchor up to `from.len()-1` never
  matches). A reverse-order body pins presence once inside the section, not the order: the line index of the
  key depends on the map order.
- Element coercion propagates into an array literal, so `(key, Some("12"))` with `key: &&str` compiles where
  `&[(&str, Option<&str>)]` is wanted: `LIST.iter().chain(std::iter::once(&"foo"))` needs no deref.
- `todo!("... {id} ...")` interpolates: a brace in the message is `E0425: found format placeholder "{id}"`,
  which stops the crate. Keep a stub message free of braces.
- `serde_json`'s `preserve_order` is a **per-invocation** feature here: `pai-app` turns it on, `pai-bff` does
  not, so the map order of a parsed body depends on the package set of the run (`cargo test -p pai-bff --lib`
  builds it plain, `-p pai-bff -p pai-app` unifies the features). Cargo.lock records no features, so
  `cargo tree -p pai-bff -e features -i serde_json` is the proof for the invocation you care about.
  Pin an order only when it holds in both (write the fixture keys sorted, then the request order equals the
  map order under both); anything else is presence compared sorted, and the run that changes the order must
  be reported. Once the production rule is "the edits are applied sorted", the key-order pin DOES hold in
  both, and to show it red exercise the feature on: `cargo test -p pai-bff --lib
  --features serde_json/preserve_order <filter>` (2026-11-17). Write the body in an order that is NOT the
  key order (a block whose body order equals the key order greens by luck), and cover three keys, so a
  two-key order is not mistaken for the whole rule: the appended-keys test of `tests/preset_body.rs` sends
  body order, reverse key order and a shuffled three-key body in one test.
- A non-JSON body reaches the handlers of an axum service that consumes `Result<Bytes, BytesRejection>`:
  the extractor never looks at Content-Type, so a route that wants the JSON check takes it from the headers
  (the bff's `json_body` reads the essence before `;`). A test of that sends the header on the request, so
  the request helper takes the header list and `req.request.headers_mut().extend(list)` (the `Parts::from`
  form drops the method and URI, `RequestParts::try_into_request` keeps it). One helper that takes the
  Content-Type as an argument keeps the request to a single value; `Vec::insert` on
  `request.headers_mut()` appends, which makes a multi-value header and leaves the check's answer unpinned.
  Pin the accept side of the same rule too (a `charset` suffix is served), and prove each refusal case red
  on its own by running the list with the case first: one red names one index.
- A body fixture with a control character goes through `serde_json::to_string`, so the bytes reach the parser
  escaped as the browser sends them (`\u0000`, not a raw NUL), and write DEL as `\u{7f}` (the character is
  non-printing in the source).
- A verb added BESIDE a path that already serves (the `PUT` of a routed `GET /api/presets/{id}`): every status
  assert of the new file reds `left: 405 right: <expected>`, the 404 tests included (the router's 405 answers
  before the app can name an unknown id). Two greens need naming: the "PUT is the only write verb" pin greens
  by construction (POST/DELETE/PATCH are 405 today), and so does a "the session survives every non-refusal"
  assert (no route ran, so no session was touched).
- The bff's body cap is the router's `DefaultBodyLimit` (`MAX_CONTENT_LEN`, private to the lib): it answers 413
  `payload-too-large` BEFORE the origin guard, the session and any manager call, so the test asserts zero
  connects. Pad a VALUE of the contract's body to cross it, so a 413 cannot be read as a malformed body.
- "The manager's words never reach the browser" is a `!body.contains(needle)` over EVERY failure reply
  (400/401/403/404/409/413/502/503/504): cut the needle from the message with
  `msg.split_whitespace().collect::<Vec<_>>().join(" ")`, never with format magic, so the needle cannot vanish.
- 503 and 504 of one save are the SAME fake cell (a parked call) told apart by what the test proved: no write
  asked (file text unchanged, write log empty) is 503 `client-unavailable`; a write already SENT is 504
  `client-timeout` and the gate release makes the file land AFTER the answer, which is the outcome the answer
  cannot name. The project's rule: once a write was ASKED its outcome is unknown, so an explicit
  `Unavailable` answered by the write is 504 `client-timeout` too, not 503 (the server save, the switch's
  `switch_when_the_write_answers_unavailable_is_504`, and `switch_failure`'s `Unavailable if write_asked`
  arm all answer that way); 503 is only where no write was asked.
- Two limits of one route are pinned by their DIFFERENCE: a call slower than the read limit answers 200 under
  the write limit (a write, and the state read after a landed write), a read of the same delay answers 503.
  `MANAGER_TIMEOUT` is private to the bff lib, so the integration file pins it behaviourally, and the save
  limit needs a builder arg (`router_with_session_and_write_timeout`).
- The bff's fixed-clock seam (`App::clock`) cannot pin an IDLE deadline: the deadline never moves on a stopped
  clock, so the read after the save answers 401 whatever the route does. Give the App a `tick: Arc<AtomicU64>`
  and a builder that hands the session `fixed + tick`; `advance(ms)` then a signed-in read pins the move as a
  number (the session's `now_ms` ticks at 5 ms, so ms units resolve). Every other test leaves the tick at 0.
- A fake's helper nobody calls is a `-D warnings` red in `tests/*.rs`: an unused gate is 4 fields and 4
  accessors, an unused setter one more. Build the machinery a test needs, and when a test does not need it,
  delete it before reporting; the connect refusal needs an outcome cell only (a refused connect opens no
  stream, so a pause and a gate there are dead weight).
- The 422 field table: `field` is `args[0]` ONLY for a rule whose first arg is a key. A rule naming a value, a
  position, a count or nothing maps to its control: `fit-target-format` to `fit-target`,
  `moe-cache-with-tensor-split` (empty `args`) to `moe-cache-mib`, `load-on-startup-over-max`/`-unlimited` to
  `load-on-startup`, `preset-no-model` to `model`, `override-no-pattern`/`-no-device` to `override-tensor`. A
  refusal about the preset's NAME (`invalid-preset-id`, `preset-name-taken`) carries NO `field` member (not
  null). Read the real `args` from `crates/core/src/checks.rs` (`grep -n '"<key>"'`): the built `field_of`
  takes `args[0]` and drops the rest, so a table copied from a design's prose reds on `args`, and the
  `left: 405` of an unrouted route hides it. A rule whose `args` has no key at all (`min-above-max`) has an
  `args[0]` that is a key, and a rule whose `args` is `[]` has none: pin both.
- `AsStatusAll` (the instance states behind `running`) is asked for the file that holds the section, so a save
  of `presets.ini.disabled` ends its exact call list at the WRITE: no `list`, no `status`. Pin that, and pin
  the safe hint the other way round: a state read that fails AFTER a write that landed is a 200 with
  `running: true`.

## Text pins and source lints

- A byte-exact text transform: build the fixtures from lines (`fn file_of(lines: &[&str], eol: &str)`)
  and loop the test over `["\n", "\r\n"]`, so both ending styles and the "no CR in an LF file" pin share
  one body; gate that last assert on `eol == "\n"` (it reds by construction on a CRLF fixture). Count the
  lines a writer matches with the module's own predicate (`text.lines().filter(|l| line_starts_with_key(l, key)).count()`),
  so "exactly one line remains" is pinned by the same rule the writer uses, not a re-spelled one.

- A coordinator's decision that changes ONE rule of a writer (the INI `set_key`: an equal value returns
  the text byte for byte, a rewritten line keeps its OWN ending) leaves the writer's other paths alone:
  the append path is NOT covered, so an appended last line keeps taking the file's ending (the pre-existing
  pin of the last-line-header test says so), and a new key lands after the section's LAST real line, not
  after the sibling you were thinking of. Read the sibling pins of the same function before writing an
  expectation the decision does not name; two guesses of mine were wrong in one run.
- A pin of "prefix, one break of an unpinned style, the content": a `trim_end_matches(['\r','\n'])` on the
  slice taken after the prefix cannot remove the LEADING break, so a `rest.starts_with('\n')` assert beside
  it is a contradiction and the test is red by construction (red reads `left: "\r\nn-cpu-moe = 8"`). Trim
  BOTH ends (`trim_matches(['\r','\n'])`) for the content, and keep the break's presence in its own assert.
- Proving each assert of a multi-assert test reds for its own reason (scratch copy, `//` the predecessors,
  `cmp` back): a block is the `let fixture = ..` statement AND the `assert_eq!` that follows it, each
  ended by a paren-balance walk. Commenting the `let` alone leaves its assert live, every variant dies as
  `E0425: cannot find value` and prints no `test result` line. An assert that only passes when an earlier
  one of its own fixture group is live is the same trap.
- A fixture that spells key lines compactly (`ctx-size=8192`) to show untouched bytes: the EXPECTED text of
  a REWRITTEN line is `key = value` (`set_key`'s rule, ARCHITECTURE.md "Saving a preset from the web"), not
  the fixture's spelling. A count pin on the compact needle (`matches("ctx-size=")`) drops when the line is
  rewritten, so its expectation must name what the smaller count proves (the compact spelling survives only
  on the untouched section's line), and the compact line's byte-for-byte survival gets its own
  `contains("{needle}{nl}")` pin, with `nl` in the needle so the CRLF pass pins the ending too.
- In a slice under a coordinator's decision, some reds in the filter are the PRE-EXISTING tests (the
  previous slice's red the reviewer is now extending: `set_key` glued an appended key to a final-line
  header). Say which reds predate your edits; `git diff` cannot tell them in a tree a coder left dirty.

- A pin that a source literal IS a text constant of another crate (the bff's disabled-file header, the
  configurator's `DISABLED_FILE_HEADER`): extract the raw literal, then DECODE it before comparing
  (a `\` line continuation drops the newline AND the next line's indent; then `\r`, `\n`, `\t`, `\"`,
  `\\`), and compare the decoded text with the constant read from the other source (cut it from
  `const NAME: &str = ` to the line's `;`). Do not compare line counts: the extraction starts with the
  opening line's continuation `\`, so the counts differ by one on an identical text (`left: 4 right: 5`),
  and a green of a count compare proves nothing about the bytes.
- A rename of a FILE NAME splits into three kinds of site, and only two go red: a fixture that CARRIES the
  name inside prose data (the disabled file's header as the input file of a save test) is data and stays
  green; a test holding its OWN copy of the prose is red only through the twin-keeper that decodes and
  compares it to the owner crate's constant; and the owner crate usually has NO verbatim pin at all
  (`parked.starts_with(DISABLED_FILE_HEADER)` is self-referential, green under any text). So a prose rename
  needs a NEW verbatim fixture in the owner crate: derive the expected name from the code that owns it
  (`ConfigFile::Presets.manager_name()`) and add `!HEADER.contains("<old name>")` beside it, so the pin
  cannot rot with the next rename.
- A tuple pairing `(the manager's file, the name on disk in the user's directory)` (the hand-over's
  `USER_FILES`): a fixture that builds the per-user directory with the MANAGER's names greens the defect,
  because the code under test reads exactly those names, and every existing test passes. The fixtures that
  carry the user-side names must be built from constants for each side (`PRESETS_USER` = `presets.ini` in
  the dir, `PRESETS_SYS` = `models.ini` in `with_file`/`writes`), and the negative needs the gate of the
  rule isolated: an offer that requires a per-user `server.ini` reds "offered" only when the manager
  already holds the server file, else it is green for the missing gate.
- A text pin on a file the coder will rewrite: prove it greens on a candidate edit in a temp copy, so
  the red is not a broken helper. A "no old text left" pin must be narrow (pin the code shape, e.g.
  `"+llama.cpp.{"`): doc comments legitimately describe the old format.
- Before writing a pin, list the test module's existing helpers and tests (`grep -n "    fn "`): a slice
  that adds a rule usually lands beside a previous slice's pin of the same file (the preshutdown-at-`start`
  test was already there, its name one word from the new one). Extend the existing test's asserts, do not
  add a twin: two tests of one rule make the coder satisfy the looser one.
- `rustfmt` reflows a multi-line assert message you just wrote, so re-read the region before a follow-up
  edit: the exact text you wrote is not in the file.
- A pin of the form "this call is the condition of the start" (`body[..find(".is_current(")]` ends with
  `if`): the text before the needle ends with the RECEIVER (`if !scm.gate`), not the `if`. Strip the
  receiver: loop { take trailing identifiers; if what remains ends with `.`, take it and continue, else
  stop }. A keyword (`if`) is eaten as an identifier if the loop keeps going past a dot-less stop, so the
  stop is the whole rule. Trace the helper on `if gate`, `if !scm.gate`, `if self.gate`, `let stale = gate`,
  `debug_assert!(!gate` before trusting the green.
- A pin "the generation this restart took is the one its thread asks" (a name bound at `.begin(` must
  reach `.is_current(`): the tolerant reading accepts `let (id, scm, generation) = (.., self.gate.begin(id))`,
  and needs the begin args and the ask args separately, so `call_arguments(hay, name)` (the manager's helper
  takes the name WITHOUT `(`) must not be handed a needle with the paren: it appends `(` and starts after it.
  A `preshutdown_set`-style helper returns a needle that ENDS with `(`, so pass it as `call.trim_end_matches('(')`;
  the pre-fix search `X((` panics, so a green of such a test also proves the binary carries the fix.
- A Python mirror of a Rust test helper is the cheapest way to check a set of text pins can all go green at
  once (patch the glue in a /tmp copy, print ok/RED per pin). Transcribe the helper exactly: a mirror that
  starts its paren count at the `(` of the call off-balances every needle and reports a false RED.
- A stub struct whose field no `todo!` body reads fails `-D warnings` with `field <x> is never read` (the
  only lint red: `clippy::todo` did not fire on `todo!("RestartGate::begin")` in this crate, checked). Say
  so, add no `allow`: the wiring clears it.
- A pin on a file that also holds the test reads only the code before `#[cfg(test)]\nmod tests` (in
  xtask `main.rs`: before `#[cfg(test)]\nmod cli_tests {`, since `mod names_lint;` sits under an earlier
  `#[cfg(test)]`). A needle the test counts or forbids is split with `concat!` and kept out of the
  comment and assertion message. A top-level fn's body is found by `"\npub fn <name>("` (test fns are
  indented; presets.rs has test items before production fns).
- A pin that a record/line format is "literal text, not a strings-file text" (the manager's log): in the
  phase, `std::include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/<file>.rs"))`, cut at
  `"\n#[cfg(test)]\n"` (that is where `mod fakes` starts, so the phases and this test are off the check),
  then require each field needle (`action=`, `target=`, `caller=`, `principal=`, `outcome=`) and forbid
  `strings::t(` / `strings::fill(`. The require-needles red today (no renderer yet); the forbid-needles
  are green by construction (the file has no strings call), so report them as guards, not reds. Check the
  counts with a `python3` one-off before claiming either.
- A build script, a GitHub workflow or a `.wxs` cannot run in a test: pin its text with comment lines
  dropped first. When a workflow step checks reply fields, confirm names and values against the producer
  by hand (`/api/instances` in `crates/bff/src/lib.rs`, states in `crates/core/src/client.rs`).
- A `body.contains("Word")` lint greens on a comment naming the word (the handler's `Preshutdown`
  lint is green today on a comment alone, the code routes through `ends_instance`): require the call
  shape (`name(`) and the routing (the guard call and the send target), not the bare word.
- An order pin `find(call) < find(kill)` pins text order, not the executed path: relocating the block
  into a sibling branch whose text precedes the kill needles greens while the save is skipped at
  runtime; pin the branch condition around the call. The `.expect("keeps killing the child")` on the
  kill needle is what stops gaming by removing the kill (keep it).
- A source lint that ignores test code uses `production_lines` (`tests/control_lint.rs`): it drops
  `#[cfg(test)]` / `#[cfg(all(test ..` items by brace counting (not `any(test, ..)`, compiled in
  production). "Only the seam may name X" is one more scan there with a `*_hits(line)` helper and its
  own test (model: `only_the_seam_reads_the_instances_log`).
- The configurator's lints (`control_lint`, `binding_lint`, `names_lint`) read files by path
  (`src_dirs`): a new crate holding configurator or printing code (pai-bff) must be added to each, or
  they pass while scanning less. If its keys are in `cli.toml` alone, its printing fns go in
  `CLI_ONLY_FNS` (by a file name no other crate has, never `lib.rs`/`main.rs`) and the file in
  `CLI_PRINT_PATHS`; `cli_print_paths_hold_no_language` reads only those files. The scans see a key only
  as `strings::t(` / `strings::fill(` (a bare imported `fill(` is missed; `env!` inside `println!` is
  flagged as a literal).
- `strings_t_keys_and_the_language_file_match` derives the used keys from the sources at run time (no
  key list to edit in `binding_lint.rs`), so a new `cli.toml` key greens with the lint UNTOUCHED, and it
  also refuses a key in `cli.toml` no CLI-reachable source uses and a `{N}` the call does not fill. A
  green of that lint is a guard only if the three lists cover the crate: `src_dirs` is
  `["app", "bff", "cli", "config", "core", "manager"]` in all three lints, `CLI_ONLY_FNS` and
  `CLI_PRINT_PATHS` match by path tail (`path.ends_with(file)`, so `bff.rs` is a whole crate's stem:
  keep it unique).
- `only_the_framework_s_own_files_are_in_its_system_directory` scans every crate's `src` (paths are
  `crates/<pkg>/src/...` tails, slashes on Windows too: the lint rewrites `\` to `/`), so a file that
  legitimately names `paths::system_config_dir` for the framework's OWN file goes in `SYSTEM_DIR_ALLOWED`
  with its reason (today: `paths.rs`, the bff's `listen.rs`, the manager's `action_log.rs`: its log sits
  in `logs` beside the framework's config dir, so it takes that dir's parent). The scan is
  production-lines only.
- `binding_lint` (`rust_fns`) skips only what follows a `mod tests {` line: an e2e phase (a top-level
  `#[cfg(test)] pub(crate) fn ..._e2e`) counts as production, so calling a `CLI_ONLY_FNS` fn there
  (`sweep::run`) fails `cli_only_code_is_not_called_from_elsewhere`. Call it from a `#[test]` in the
  file's `mod tests` under `fake::scoped`, when it reaches no `paths::` (a plan it refuses, `reps: 0`,
  stops it before anything real).
- `strings::t`/`fill` return the KEY when missing: assert the message's content and
  `!text.starts_with("<key>")`, never only equality with `strings::fill("<key>", ..)`.
- Pin "the text is the strings file's, not built inline": `assert_ne!` the message against the inline
  `format!` shape FIRST (so the red names that inline text), then require it to start with the template
  read through the strings API, `strings::t("<key>").split('{').next()` (a missing key reads as the key,
  so the same assert reds on it). A template that fills the HALF-WRITTEN name greens the file-name
  contains (the final name is its prefix): pin `!msg.contains("-partial")` too, to pin the final name.
- A `pai-core` test reads the REAL strings file: `paths::strings_dir()` takes `<exe>/../../share` only
  when it exists, else `CARGO_MANIFEST_DIR/strings`, so no redirect is needed. A key used by code the
  CLI reaches goes in `cli.toml` as well as `en_US.toml` (binding_lint), and `fill`'s arity must match
  the template's placeholders exactly (`mistuned`: `{0}..{N-1}`, no gap, no extra).
- The names lints (`crates/config/src/tests/names_lint.rs`, `xtask/src/names_lint.rs`) flag old names in
  code lines (doc comments skipped). In tests split them with `concat!`: `concat!("llama-cpp",
  "-config")`, `concat!("llama", ".cpp")` for `join("llama.cpp")`, `concat!(r"C:\ProgramData\llama",
  r".cpp\models")`, `concat!("/opt/llama", ".cpp/...")` (xtask, even in a workflow needle), or a const
  (`const ENGINE: &str = "llama.cpp"`). Sample paths must not contain the repo's directory name: write
  `/home/dev/src/pai/...`. Exemptions: only `crates/core/src/migrate.rs` names the old tree
  (`MIGRATION_NAMES`; `hits` matches the path by its tail); a bare `private-ai-manager` only in
  `MANAGER_SERVICE_NAMES` and `WINDOWS_SERVICE_MODULE` (use the constant elsewhere); `/opt/llama.cpp`
  only in `ENGINE_DIR_SOURCE`. Run the whole crate's tests, not just your filter. A new name that must
  CARRY the service name (`private-ai-manager.log`) is spelled `concat!("private-ai", "-manager.log")`
  in production code and the same in its test; only whole-line comments are skipped, so a comment on a
  code line is scanned.
- Since `src/lib.rs` exists, binary roots (`main.rs`, `bin/*.rs`) are separate crates; a lint may exempt
  them, never `lib.rs`.
- Cargo.lock lists dev and build deps of every target, one per line: a walk over it over-reports, never
  misses (`crates/app/src/tests/deps_lint.rs`). A helper two crates need goes in a test-free file
  included with `#[path = "..."] mod` (`tests/lock_walk.rs`). The include is relative to the MODULE's
  directory and resolved on the filesystem: `mod tests` inside `crates/cli/src/cli.rs` has base
  `crates/cli/src/cli/tests/`, which does not exist, so any include with `..` from there fails
  (`couldn't find file`, E0583). Declare the include in a module backed by a real directory
  (`crates/cli/src/tests/mod.rs`: `#[path = "../../../app/src/tests/<x>.rs"] pub(crate) mod <x>;`) and
  `use crate::tests::<x>;` where needed.
- Gate lints ("the call sits behind the decision") share one scanner (`crates/app/src/tests/gate_lint.rs`,
  used by pai-cli's `snapshot_before_stop` and pai-config's `the_stop_worker_saves_snapshots_only_on_the_local_backend`).
  Two traps: a call needle includes its `(`, so an argument-text walk starts right after the needle at
  depth 1 (requiring a `(` there fails on the real code); and a gate found inside an `if`-condition whose
  block holds neither the call nor a `return` must `return false`, NOT fall through to the bound-name
  fallback: the fallback's "an `if` between gate and call" is satisfied by the gate's own `if`, so a gate
  guarding only another call would green.
- No "any `if` between the gate and the call" fallback at all: it greens on the `if` inside `verify` and
  `modify`, and on an `if` whose condition names nothing of the gate (the GUI worker with its `saves &&`
  deleted). Enumerate the `if`s (whole-word `if`, condition to the `{` at paren depth 0, block by brace
  count) and require the gate call, or the name a `let` bound its answer to, as a whole identifier in the
  condition of the `if` whose block holds the call, un-negated there; negated in a block that returns and
  holds no call (a guard clause). A name needle is bounded on the right, a call needle is not: a call
  needle ends with `(`, so its right side is the first argument char (`saves(b)`), and a right-boundary
  check there fails on the real code (`on_the_fly` is the case the right check must refuse).
- A scan of the text BEFORE a call must skip a qualified path and keep the stop character: the gate of
  `if !crate::server_control::gate(..)` is governed by the `!` past the path, of `let on = crate::..::gate(..)`
  by the `=`. `head[..i]` drops the `!`; taking the name from `head[i..]` keeps the separator space
  (`" on"`), which matches a plain `if on` by accident and fails the negated guard, so the red names a
  shape that never existed.
- Probe the shape table with `rustc --edition 2021` over a `include!` of the helper (its `//!` header must
  become `//`: E0753 outside a module root): 2 s per run, no Slint link. Run the same table against a copy
  of the helper taken BEFORE the rewrite: every refusal printing `old=true` is the teeth of the new test.
- `gate_lint.rs` reaches the crates that use it by `#[path]` and is NOT declared in `crates/app/src/tests/mod.rs`,
  so `cargo fmt -p pai-app` leaves it alone: the fmt check must name pai-config and pai-cli.
- A clap doc comment is pinned on what the user reads: `Cli::command().find_subcommand(..)`,
  `.get_arguments().find(..)`, `.get_help()` (derive joins doc lines with spaces).
- `benchmarks/schema.sql` is `include_str!`ed by pai-app (`bench/db.rs`) and
  `the_repo_database_matches_the_schema` compares only columns: a comment between statements changes
  nothing SQLite records (`sqlite3 -readonly benchmarks/bench.db "select sql from sqlite_master"`), one
  inside a `CREATE` does.
- A render that copies INI sections "byte for byte" (the `presets@<id>.ini` render): pin it with a
  `ini::text::section_text(pool, id)` needle (id WITHOUT brackets: the match is on the stripped,
  case-insensitive name), `body[from..].find(needle)` walking forward (pins presence AND order) plus
  `headers(body) == ids` (pins absence and the `[*]` first). Whole-string equality over the
  concatenated needles over-pins: it also fixes the blank lines the render puts BETWEEN sections,
  which no design says. Trim the needle's `['\r','\n']` at the END: a section's trailing breaks are
  not its bytes, and a writer that drops them is right.
- Two reading traps of that design: an unassigned preset (no key, no `*`) is NOT rendered, so a writer
  that defaults a missing key to the global switch greens the `*` tests and is wrong; and the pool's
  `[*]` is in the render always, so every expectation starts with it. A "renders nothing" pin whose
  expectation is an empty body greens by construction on a stub returning `String::new()`: pair it in
  the same test with a non-vacuous case (the id absent while another is present), and let the header
  assert be the red that names the stub there.
- Prove a pure helper's red set goes green WITHOUT touching production code: build a probe
  (`rustc --test --edition 2021 probe.rs`, 1 s). Copy the test file, cut the stub `pub fn` block with
  python (`src[:src.index("pub fn render(")] + src[src.index("#[cfg(test)]"):]`), rewrite
  `use super::render;` to `use crate::render;`, and `include!` that file from the probe next to a
  candidate fn and a mini-module copying ONLY the helpers the tests call. `crates/core/src/ini.rs`
  cannot be `include!`ed whole (`use crate::strings;`, `crate::checks::`), so copy
  `section_text` with `find_section_header` + `next_section_start` + `line_start_of`.
- `body.contains(&section(..).trim_end_matches(['\r','\n']))` is `clippy::needless_borrows_for_generic_args`
  under `-D warnings`; drop the `&` (the argument coerces on its own).
- The probe's test cut is regenerated from the real file every round (`awk '/^#\[cfg\(test\)\]/{f=1} f'
  crates/<c>/src/<f>.rs > probe_tests.rs`): a driver that `continue`s on a compile failure leaves its
  PATCHED cut on disk and reads it back as pristine, so the next case silently tests a stripped module.
  Write each patched cut to a NEW path and never write the pristine copy.
- libtest prints the per-test panic text and the `test result:` line on **stdout**: a probe driver that
  greps only stderr reports a red run as green. Read `run.stdout + run.stderr`, and print the
  `panicked at` line, so a green says "NO PANIC".
- Teeth of a pin that sits AFTER a pin red today: `//` the predecessors in the cut only shows the pins
  whose group is live. For the rest, mutate the CANDIDATE (one `if on {` → `if on || !in_enabled {`, an
  early `return`), so the earlier pins pass and only the target pin can fire. Four mutations of one
  candidate, one per pin, is the proof, and the unmutated candidate green is the joint-satisfiability
  proof of the whole set.
- A render that takes its line-ending style from the file it reads: an EMPTY pool file makes
  `ini::detect_eol("")` answer CRLF, so the generated header lands CRLF in an LF render. Pin it as
  "an LF pool rendered with a CR" and state the assumption (the empty file takes the style of the file
  that has content).

- A rename of a DISPLAYED word where the strings KEYS stay (the values are the coder's data): every test that
  builds its expectation through `strings::t("<key>", ..)` / `fill("<key>", ..)` needs no edit and greens by
  construction; only the tests that spell the value verbatim go red. Find them with a word n-gram scan
  (2..=4 grams of each value, both strings files) against every `crates/**/*.rs`, then read each hit: a
  substring inside a key name, a Rust id (`preset_no_model`), an INI token (`--models-preset`) or a fallback
  id is not a text pin. Capitalise the expectation as the FILE has it (`Preset` -> `Model profile`).
  The coupling of a command rename to the values: a new lint over both strings files forbidding the retired
  command name (`pai preset`), in the crate that already lints them.
- A CLI verb rename: clap builds each subcommand from a struct field list (`Command::new(name)`), so a help
  read on `Cli::command()` cannot tell the verbs apart by text: the commands list lives in the binary's
  `--help` test. Pin the retired verb as its own test (exit 2 + `unrecognized subcommand '<verb>'`); clap's
  stderr names only that verb, and its usage line is `pai <COMMAND>` with no list, so never assert the list
  there. The inline parse tests take the verb as an argv string: renaming those argv is the red, and an
  expect prose naming the command is a second red per phase.
- Where a brief redacts the words of a rename, the names table in `ARCHITECTURE.md` settles them; report the
  line. If it renames the strings KEYS and the brief pins VALUES only, the brief governs: say so, and note
  that the key-based expectations then follow the values on their own.
- A test whose expectation is a filled string, and a doc comment naming the CLI command, are separate text
  pins of the same rename: rustfmt reflows the longer value into a multi-line `assert!(..)` (format each
  file, then `cargo fmt -p <crate> --check`).

## Data-root e2e and the configurator

- A test that needs `PRIVATE_AI_DATA_ROOT` (anything reading `paths::`) is a phase fn, not a `#[test]`:
  `#[cfg(test)] pub(crate) fn <name>_e2e()` calling `crate::paths::redirect_for_tests(dir.path())` on its
  own tempdir, called from pai-config's `editable_widgets_track_model_after_edit`
  (`src/tests/ui_bindings.rs`) or pai-app's `data_root_e2e` (`crates/app/src/tests/mod.rs`). Put the
  phase ABOVE the module's `mod tests` (`items_after_test_module`). Append a new phase LAST: a red phase
  hides every later one. To see later phases or blocks red, on a backup copy replace the other calls with
  `{}` (deleting leaves `#[cfg(..)]` lines dangling) or turn the red block into `if false {`, then
  restore. The same holds for `manager_checks_e2e`.
- Proving EVERY block of a multi-block phase reds for its own reason: a `python3` pass that finds the
  phase's top-level blocks (the lines equal to `    {`, one per `// ──` header plus any inner block the
  header opens) and, for each block, prefixes ALL THE OTHERS with `if false ` (valid: a statement block
  becomes an `if` statement, no brace added), runs the filtered test, prints the panic line. The
  CUMULATIVE prefix (disable the first `i`) proves only the first block: a disabled block never runs, so
  every later run reports block 1's panic. A block that asserts "nothing was recorded" passes on a stub
  by design: report it as a guard, not a red. Restore by `cp` and `sha256sum` against the backup, and
  restore in a `finally` (an exception mid-loop leaves the tree patched; `git diff --stat` after each run
  shows the patch is gone). Two traps: a `    {` that opens a nested ITEM (a `fn f<B>(..)` whose signature
  wraps past a `where`, so rustfmt puts the brace on its own line) is not a statement block, `if false {`
  there is `expected one of #, {, lifetime, or type` and nothing runs, so accept a `    {` only when the
  previous code line (whole-line comments skipped) ends with `}` or `;`; and a run that prints no
  `panicked at` is that patch compile error, not a green, so print the run's `error` tail too.
- Blocks of ONE phase share its data root, so a later block's read expects what an earlier block wrote
  (a presets file written in block 1): say so in a comment. The cumulative probe then shows a LATER block
  red on the fixture (`left: Ok(Some(<older text>))`), not on its point: read the red, do not chase it; the
  throwaway candidate greens the phase in order.
- Functions that take the directory as a parameter, manager calls refused at authorization or asking
  only `Status`, and pure helpers are plain `#[test]`s.
- A unit test whose expectation is a `paths::` default races the e2e redirect on another thread: assert
  the shape (`ends_with("state")`) and pin the exact value in an e2e phase. Reproduce such a race by
  looping `cargo test -p <crate> -- <unit> data_root_e2e`.
- A phase that `set_current_dir`s (the cwd is PROCESS-global) makes every relative path a race with a test
  running in parallel: read what you wrote by absolute path (`root.path().join("logs/x")`,
  `env!("CARGO_MANIFEST_DIR")`). It shows as `left: []` in the FULL crate run and passes filtered, so the
  candidate check runs the whole crate, not only the phase.
- Needing the variable UNSET: re-run one ignored test in a child (`current_exe() --exact <path>
  --include-ignored --test-threads 1`, a marker env var, `env_remove` the redirect, a deadline, `"1
  passed"` in stdout; `system_connect_never_dials_from_a_test_binary`).
- An e2e expectation that differs per OS branches on `cfg!(..)`, never on the pure function under test.
  ModelsDir seeding is pinned in pai-app's `config_seam_e2e`, the instance's unset dirs in pai-core's
  `instance_launch_e2e`.
- `server_cfg::render` (pai-core) writes ModelsDir, StateDir and Hostname RESOLVED through `paths::`
  (`render_for(.., leave_dirs_unset)` does not): a unit test rendering a `ServerConfig` sets both, a
  shipped starting file names the service user's paths. pai-config's `server_form::config_to_form` also
  seeds the field with `models_dir_or_default()`, so "unset stays unset" at render covers `pai`, not the
  GUI's Save. `render` also writes an unset `OpencodeApiKey`, and an out-of-range `Port`, `ModelsMax` or
  thread count, as a commented hint: pin checks on `ServerConfig` values (`server_cfg::check`), not on
  rendered text.
- A plain `#[test]` that reaches `server_cfg::save` (any success path) sets `models_dir` and `state_dir`
  to temp paths: where the dirs are NOT left unset (macOS, `Os::current()`) the save ends with
  `create_dir_all(cfg.models_dir_or_default())`, so an unset pair creates the real home-derived folder.
  A unit test must NOT call `paths::redirect_for_tests` (process-wide, never restored: only an e2e phase).
- A source lint on a call SHAPE (`code.contains("check(cfg, cpus)")`) breaks when production renames a
  local; the behaviour test that sees the refusal already pins the call. Keep such a lint to the "no
  second copy of the rule" guard (`!code.contains("fn check(")`) and name the behaviour test in its doc.
- `#[cfg(test)]` is off in a library compiled as a dependency: items another crate's tests need go
  behind a `test-support` feature, or the dependent silently takes the real backend. pai-app's fake
  manager is `#[cfg(test)]`: a GUI path depending on the manager's answer is tested in pai-app with that
  state passed down as a parameter.
- In a test build `server_control::backend()` is `fake::current()`: the backend `fake::scoped(b, || ..)`
  set on this thread (not spawned ones), else a manager over `DiskClient` (every action and probe
  `Unavailable`; files at the per-user paths). Pin the manager's bytes with `FakeClient` (`with_file`,
  `files`, `fail_write`, `fail_write_of` to refuse one file mid-sequence, `fake::writes(&log)`;
  `fake::actions` omits probes and reads); a second fake sharing `files` continues from the first.
  `os_backend()` is the real choice.
- To order a llama-server call against a fake's, share one log: `server_control::fake::http_stub(&log)`
  answers `/models`, `/health` and a slot restore on a loopback port; point `Port` at it in an e2e phase.
- A raw-socket HTTP fake (`TcpListener` on 127.0.0.1:0, `listener.accept()` yields the tuple
  `(stream, addr)`): record the request BEFORE answering a step that waits (a `Hang`), else a caller that
  dies on its deadline reads as "no request was sent" and the red cannot tell the two failures apart.
  Record the ask once per request, not once per request and once per step (a double record shows as two
  saves per model). Its own file write is `let _ = std::fs::write(..)`: a panic kills the listener thread
  and every later call fails "Connection refused" / "the server closed the connection", so the red names a
  dead fake instead of the behaviour.
- A "one deadline for the whole run" rule: N models where ONE hangs for the fake's `HANG` (5 s) under a
  budget of 1 s, and `assert!(took < 4 s)` - a per-model budget would need N x HANG and greens.
- Moving a leg's logic into `pai-core`: leave the old crate's behaviour tests in place (they drive the
  public entry, so they keep pinning it through the wrapper) and pin the delegation itself - a source lint
  over the code half (split at `"\n#[cfg(test)]\n"`) that requires the core call and forbids the old inline
  shape - plus ONE end-to-end through the public entry. Do not re-test the core's rules in the wrapper.
- To pin a refusal of a value that looks like a flag (`--port -1`), parse the port raw and filter only the
  path-like value on `starts_with('-')`: filtering both makes `-1` a missing flag, so a fallback-to-default
  implementation greens for the wrong reason.
- The GUI's Server save re-reads the file at once (`refresh_server_snapshot`): fake state meant to change
  "after the first save" must change before it.
- A preset range test over `checks::PRESET_INT_KEYS` builds each Preset with `Preset::from_keys` and
  checks `render_section` writes it back. A range change there also needs `form::form_to_preset`'s
  `parse_int_in(.., &ini::INT_*)` literal changed (`invalid_numbers_match_what_the_conversion_drops`).
- A model-header check is testable without libggml-base: write the GGUF with `hf_fit::tests::{gguf,
  write_sparse, W}` (sparse, so 4 GiB costs nothing) and read it with the pure-Rust `hf_fit` reader;
  `gguf::read_*` go through `ffi`, which finds no library from a test binary.
- To pin a launch, build a `Command` and read back `get_program`/`get_args`/`get_envs`/`get_current_dir`;
  take the exe as a parameter (`paths::llama_server_exe()` finds nothing from a test binary) and compute
  expected args with `server_args()`.
- pai-config's Windows code is compiled by nobody: pin each per-OS choice as a pure function of
  `server_control::Os` tested for every variant, `Os::current()` written with `cfg!`.
- slint: `ElementHandle` has no visibility query (a `visible: false` element is still found); pin a
  sometimes-shown note as an `if`. A `Text`'s accessible label is its text. Placeholders:
  `accessible_placeholder_text()` over `ElementQuery::from_root(app).match_descendants()...find_all()`
  (one match per ancestor: assert non-empty, never a count). Each page is `if current_tab == N`: set it
  in the phase and put it back.
- Who launches binaries with which arguments: login entry `gui [--minimized]` (`startup.rs`), HW Monitor
  `sdk-devices` (`hwmon/sdk.rs`), xtask's packaging reads `pai-config --version`, the MSI runs `control
  stop-and-close` (`installer/msi-stop-processes.ps1`). Check them before dropping an argument.
- An executable is tested from `crates/<c>/tests/*.rs` with `env!("CARGO_BIN_EXE_<bin>")`:
  `PRIVATE_AI_DATA_ROOT` and a `DBUS_SYSTEM_BUS_ADDRESS` to a missing socket on the CHILD only, a
  `try_wait` deadline, only commands reaching no backend (a non-test build takes the real manager).
  A graceful stop: `Command::new("kill").args(["-TERM", &pid])`, expect `code() == Some(0)` (death by
  signal reads `None`).
- After a move between crates, diff `-- --list` names against the old crate in a worktree (`LC_ALL=C
  sort`): only accounted renames. A stub at the move destination keeps tests compiling; tell the coder to
  replace it with `git mv -f`. For an older commit's worktree (`git worktree add --detach build/<name>
  <rev>`): copy the gitignored `resources/llama.ico` and `llama-brand-logo.svg`, set
  `PRIVATE_AI_LLAMA_CPP_DIR=<repo>/build/linux/llama.cpp` and a separate `CARGO_TARGET_DIR`.
 - For a move of a PURE fn (no new deps), the accounting needs no worktree build: per file, take the test
   half (`awk '/#\[cfg\(test\)\]/{t=1} t'`), `grep -oE "fn [a-z_0-9]+"`, `sort`, and `diff` it against
   `git show HEAD:<file>` (HEAD is the coder's base while the tree is uncommitted). A pure move shows as
   the same names lost by the source file and gained by the destination, helpers (`checked_base`, `not_int`)
   included: count those, do not assume. Names only, so a renamed test would pass uncounted: report the list.
 - After a move, grep the OLD path in every consumer: code (`grep -rn "pai_app::server_cfg" --include=*.rs
   crates`: a `pub use pai_core::server_cfg::*;` re-export makes the old call path legal, so a hit is not
   automatically stale) and the docs pages (`crates/config/README.md` names the check's crate: it read
   `pai-app`; the coder does not edit docs, so the report sends the stale sentence to the `docs` agent).
- A new argument on a method with many test call sites: a small Python pass that walks each call to its
  matching paren (skipping string literals and `fn ` definitions) inserting the argument, then `cargo
  fmt -p`; bump `[(..); N]` array lengths by hand.

- One constant two crates use (the save's budget): pin the VALUE in the owning crate with
  `assert_eq!(CONST, Duration::from_secs(N))`, a missing const is `E0425` and stops that crate's whole
  test binary (report which tests did not run); pin each USER's delegation with a source lint over the code
  half: require the core's path (`slot_save::SAVE_BUDGET`, a needle any of `use pai_core::slot_save::…`,
  `pai_core::slot_save::…` or `slot_save::…` satisfies) and forbid the copied literal
  (`concat!("from_secs(", "240)")`, narrow enough not to hit the file's other timeouts).
- A unit line added to a shipped systemd unit is pinned by appending the exact line to the existing unit
  test's `for want in [..]` list (`section(&unit, "[Service]")` yields trimmed lines, no leading space:
  the `format!`'s `\` continuations strip them). The loop reports only the FIRST missing line, so one red
  pins the whole list: the coder must land every entry.

## Manager, D-Bus, pipe

- A new `ManagerError` variant breaks the exhaustive matches the faces hold over it: `From<ManagerError>`
  in `dbus.rs` and the expected-mapping in pipe.rs's `every_error_has_its_name_and_reads_back` test.
  Arm both with `todo!("..: slice N")` naming the faces' slice that owns the mapping; do NOT add the
  variant to `every_error()` in the core-method slice: the round-trip needs the client's variant, a
  later slice's, and the error tests stay green on the untouched list.
- A phase can give every block its own data root: a local `fn root(files)` that calls
  `paths::redirect_for_tests` on a fresh tempdir and seeds the served dir; no coupling between blocks,
  and the "the directory is not there" case gets a root that never saw a `create_dir_all`.
- A seam called through `&self` gets a fake with `Mutex` logs (seams must be `Sync`), read through an
  accessor; pin "nothing happened" on a refusal by empty logs plus the error kind.
- The manager's `FakeService` records every `ServiceControl` call, `status` (the probe) included: a
  batch that ends with a successful `Status` pins `vec![Call::Status(..)]` in the calls log, never an
  empty one; write the refusal tests' expectation as "nothing beside the Status probe".
- A later block of a shared e2e phase asserts its count as a DELTA on a baseline taken INSIDE the block
  (beside `admins_before`/`bff_before`, take `calls_before` too): an intervening block that ran a probe
  (`boot_enabled` records `Call::BootEnabled` in the SAME service-calls log as `status`) makes the previous
  block's absolute count stale (8, not 7). A "the write adds no unit call" assert, with the pipe sibling
  pinning `vec![]`, goes: `assert_eq!(calls(&c).len(), calls_before, "a file write touches no unit")`.
- A core method signature change when the transport faces (`dbus.rs`, `pipe.rs`) are off-limits: a local
  bound (`trait AsRequester { fn as_requester(&self) -> Requester }`, implemented for the old bare
  `Caller` and the new `Requester`) keeps every face call site compiling; the stub keeps the old body, so
  the no-principal tests stay green and only the new calls red on assertions. Say in the report that the
  bound goes away when the faces send the new type. An `Option<&Caller>` parameter goes generic the same
  way: `foo(None, ..)` stops inferring → `None::<&Caller>` at each test site.
- A stub that ignores a new rule runs the OLD path, so in a plain unit test a principal `write_file` with
  an allowing authority writes the real `paths::engine_config_dir()` (Linux `/etc/llama.cpp`, where a
  `read_file` shows as `Err(Io(PermissionDenied))`: a harmless read, the red names a real path). Keep
  every call that can succeed (writes, starting an existing instance) in a data-root phase; plain tests
  only pin refusals the implementation reaches before any path.
- "A member removed between two calls of ONE manager" (no cache) needs shared mutable state in the fake:
  seed an `Arc<Mutex<BTreeSet<String>>>` from its fixture list and answer `is_admin` from it (existing
  tests keep passing, the answers are the same); two managers over different fixtures pin nothing.
- Proof that every block of a long e2e phase greens: a throwaway candidate implementation, run, then
  restore. It catches a wrong expectation in a later block (there: an earlier block had rewritten the
  file the later block compared against). Restore the backup, then re-apply the test fixes you found.
- Teeth of a refusal block, shown by probing production: script the probe (python: exact anchors, patch,
  run, `shutil.copy` the snapshot back) and keep the snapshot taken AFTER your test edits and BEFORE the
  first probe, to prove the restore with `cmp`/`sha256sum`. A red in an EARLIER block of the phase (the
  coder's own) panics first: disable that block with `if false {` on its opening brace to reach yours.
  In a loop, the first refusal red hides the block's file-on-disk check: temporarily turn the loop's
  `assert_eq!` into an outcome `eprintln!` so the block runs on to that assertion. Pick the written
  content so the content checks ACCEPT it: an ungated write then really lands, and the red reads
  `left: "<new text>"`, not a check refusal.
- A "seam X was never asked" assertion stays green under a probe that removes the whole gate (nothing is
  asked at all): it needs its own probe that REORDERS the production checks, leaving the refusal itself
  in place.
- `Authority` has `is_authorized` (may prompt) and `is_authorized_quietly` (never: a challenge is no);
  `FakeAuthority::new(allow).quietly(q)` logs the quiet ones in `asked_quietly`. A changed D-Bus reply
  signature is pinned with `r.body().deserialize::<OldShape>().is_err()`.
- "A slow seam holds up nobody": `fakes::Gate` in `manager/service.rs` (`GateAuthority`,
  `GateResolver`): wait with `gate.arrived(n)`, run other calls via `on_thread(..)` + `recv_timeout`,
  open the gate before panicking on a timeout, check `try_recv()` still empty. Pin a race by holding one
  call at a seam inside the critical section and accepting either serialization.
- A reservation under a lock released by a `Drop` guard in async code: no `.await` between reservation
  and guard (cancellation leaks it), every `MutexGuard` on that lock dropped before the guard (else
  deadlock). A cancelled request leaves its `spawn_blocking` running.
- A per-connection identity behind a shared `Authority` is a thread-local set around the call
  (`TokenAuthority::with_caller`): pin it cleared on return and on panic, unseen by other threads.
- `$MAINPID` exists only for a unit's `ExecStop=` and is unset when systemd knows the main process exited
  (man 5 systemd.service, `ExecStop=`): pin the unset case through `run(&["save-slots", "default"])` in a
  plain unit test, first `assert!(std::env::var_os("MAINPID").is_none(), ..)` (a systemd-only name, so the
  assert never fires in a test, and the run must touch no `paths::`: `data_root_e2e` owns the process-wide
  data root). A candidate arm that drops the id shows as `unused_variables`: tell the coder to prefix it.
- zbus 5 without a bus (`zbus::blocking`): `UnixStream::pair()`; server
  `Builder::async_io_unix_stream(s0).server(Guid::generate())?.p2p().serve_at(..)?.build()` on its own
  thread; client `.p2p().method_timeout(..)`. Stamp a sender with `client.inner().set_unique_name(":1.42")`
  (dev feature `bus-impl`). `org.freedesktop.DBus.Error.*` replies come back as `zbus::Error::FDO`,
  others as `MethodError`. A destination `Some(BUS_NAME)` is accepted on p2p.
- "No manager": serve at another path (`UnknownObject` as `FDO`) or drop the server (`InputOutput` at
  once). No system bus: `DBUS_SYSTEM_BUS_ADDRESS=unix:path=<tempdir>/none`, in one test of the binary. To
  prove a run never dials the real bus, point the variable at a logging Unix listener (`python3 -I`) and
  run `cargo test` filtered on `bench:: cli:: slot_state gui::` plus the e2e.
- zbus 5 (async-io) runs every task of a connection on one executor thread: a sync `fn` method blocks
  every other call on that connection.
- Another D-Bus service (systemd) is faked by a `#[zbus::interface(name =
  "org.freedesktop.systemd1.Manager")]` struct served p2p at `/org/freedesktop/systemd1` (snake_case
  methods become `GetUnitFileState`...). Signatures: local `man 5 org.freedesktop.systemd1`; `is-enabled`
  states: `man 1 systemctl`.
- A new `ManagerClient` method needs an impl in `DbusClient`, `PipeClient`, pai-app's `FakeClient` and
  `DiskClient`, and pai-bff's `tests/instances.rs` and `tests/server.rs` `FakeManager` (implement the
  test ones yourself). A new `Authority` method: `FakeAuthority`, `GateAuthority` (test), `PolkitAuthority`,
  `TokenAuthority` (`todo!` stubs).
- The Linux `Caller` is a bus name, not a uid: "who is this account" needs the bus
  (`GetConnectionUnixUser`), so it sits on `Authority` (`is_bff`), the uid rule a pure fn; on Windows the
  thread-local `TokenFacts` of `TokenAuthority`. A pipe test for a missing argument of a NEW method is
  green before it exists (an unknown method is BadRequest too): recheck it after the coder.
- A fake's shared log in a returned tuple trips `type_complexity`: alias it in `fakes` (`Checked`).
- A server or client over `impl Read + Write`: a `Cursor` of peer frames for reads, an
  `Arc<Mutex<Vec<u8>>>` for writes, an optional failing `ErrorKind`; compare JSON as `serde_json::Value`.
  A real duplex: `UnixStream::pair()` with `set_read_timeout` (`manager/pipe.rs`,
  `crates/manager/src/tests/pipe_client.rs`). A frame cap needs a test in both directions on each side.
  The fakes log in `Arc<Mutex<..>>`, so `Arc<Manager<..>>` is `Send + Sync`: one side of a socket-pair
  round trip is `thread::spawn(move || serve_stream(&core, caller, server_end))`, and `drop(client)` is
  what ends it (EOF reads `Ok(None)`), so no channel is needed to stop it.
- The manager's principal refusals are all `ManagerError::NotAuthorized`, whichever check failed (not
  the bff, ill-formed name, no member): pipe tests pin `error("NotAuthorized", ..)`, the client's
  `NotAdministrator` is its own reading of that name.
- A shared asked-seam log (`asked_admin`) over an e2e block counts one entry per call that REACHES that
  seam: a call refused at an earlier gate (a non-bff caller stopped by `is_bff` in `check_principal`)
  adds none. Trace every call of the block through the gate order before writing the expected list, and
  say the order in the assert message, so the list pins the gate order, not a call count.
- The manager's log of who acted (`ActionLog` / `Manager::with_log`): pin the record as the fake log's
  whole lines (`action=".." target=".." caller=".." principal=".." outcome=..`, values `{:?}`-quoted, `-`
  for no caller/principal, `ok`/`refused` bare, a failure's kind quoted). One hostile-name table where
  EACH case gets its own manager and its own fake log, so "exactly one record" is provable; pin Rust's own
  escaping by formatting the same value in the test (`format!("{:?}", caller.0)`), never by hand-writing
  escapes. A candidate that greens it: each guarded method becomes a wrapper over a `*_inner(&Requester)`
  plus one `log_call(action, target, who: Option<&Requester>, outcome: Option<&ManagerError>)` (the
  `Option<&ManagerError>` shape takes both `Result<(), _>` and a `Result<Option<..>, _>` via
  `.as_ref().err()`); the full crate green with that candidate proves the wrappers are behaviour-preserving.
  The `log_call` helper goes in `impl<S: ServiceControl, A: Authority, R: Resolver> Manager<S, A, R>` (a
  bare `impl Manager` is E0107: the struct is generic). A failure's outcome: `format!("{e:?}")` cut at the
  first `(`, then `{:?}`-quoted, so `Service(_)` and `AddressInUse(..)` both give the bare kind name.
- In a plain `#[test]` of `manager/pipe.rs` (no data root) `Stop` is the only verb that may succeed:
  `start`, `restart` and `set_boot_enabled` ask `existing_id`, a file on disk. A refusal by a pure check
  carries any verb (id `../x` is InvalidId, name `server.ini` is BadFileName). A success that needs a
  real file belongs in `src/tests/pipe_client.rs`. `stop` is FS-free end to end (authorize, `valid_id`,
  the fake service), so it is the verb for a plain test of a log sink wired on `with_log`; a sink's
  red is its own `todo!`, not a stray read of `/etc/llama.cpp`.
- The sinks of the action log (`manager/action_log.rs`): one line per record is pinned as the sink's whole
  bytes (`{line}\n`, nothing added), the flush by a `Write` that counts its `flush` calls, the no-mixing
  rule by N threads x M long self-naming payloads through ONE `Arc<Mutex<W>>` writer with `handle.join()`
  counted, not propagated (the stub panics on every thread; the count goes in the assert message, so the
  red says "8 writer threads panicked, 96 records asked for: 0 lines").
  `WriterLog<W>` over `Arc<Mutex<W>>` is what makes the lock span the write AND the flush: the writer
  accessor returns the `Arc`, so the test reads it and the threads share one log.
- The FILE sink's no-mixing rule: one temp path, 12 threads x 24 long self-naming records, a
  `Barrier::new(threads)` waited after EVERY record so all threads enter `record()` at once: red 10/10
  today (122-145 empty lines per run) from `writeln!`'s two writes. A candidate fix matters:
  `write!(f, "{line}\n")` is NOT one write (`write_fmt` emits each format fragment separately), it stays
  red 10/10; `file.write_all(format!("{line}\n").as_bytes())` is one `write(2)`, atomic under `O_APPEND`
  on Linux and Windows, greens 10/10. Single-write-per-record is not observable deterministically
  through the public API (no FIFO/seqpacket over a path sink): the thread test is the pin.
- A path the service derives from `paths::` is pinned in two parts: a pure `<name>_path_in(root: &Path)`
  for the shape (a Windows path reads on Linux the same, `Path::join` is separator-agnostic: use slashes)
  and the exact live value in the ONE phase that already redirects (`data_root_e2e`), asserted
  `starts_with(<tempdir>)` as well, so a real `%ProgramData%` write cannot hide green. A phase's `core`
  closure is not generic (a closure cannot be): a block that installs a different sink type builds
  `Manager::new(..).with_log(..)` itself.
- Gate order is pinned by the fakes' logs, never by an echo shape invented for the reply:
    `FakePasswords::asked_admin` shows the principal reached the core, and that the gate runs before the
  action's own check (`SetBootEnabledAs` of `../x` asks the membership, then answers InvalidId); an
  empty polkit `asked` shows the `Requester` carried the principal, since a call that fell back to the
  caller's own would ask.
- `asked_bff` records the caller of every `is_bff` question (the face sends the header's sender), so a
  phase spanning two stamped connections expects that log by caller, not as one total: `bff_asks(9)`
  extended with `other_asks(6)`. Only the `As` methods ask it (a plain guarded call asks polkit, not
  `is_bff`), so the count is the principal calls, not every guarded call.
- The D-Bus face is pinned with arguments as a tuple in the declared order (`&("alice", "default")`),
  since zbus maps the members positionally: a reversed implementation answers for the instance named
  "alice", a red no client test catches (the trait hides the wire). Over p2p a missing method replies
  `org.freedesktop.DBus.Error.UnknownMethod` (`FDO`), so every new method reds there first; the pipe's
  red for the same is `BadRequest`. A stub's `todo!` in the core client panics with its own name
  (`not yet implemented: DbusClient::acting_for`): in the one `data_root_e2e` test that red stops the
  phase, so the face's red and the client's red are seen in two runs, not one.
- `set_unique_name` works on the client connection of a p2p pair too, so the bff caller rule (`:1.9`) is
  pinned from the server face (`Builder::..caller(..)`) and from the client (`the_bff_caller_runs_as_the_principal`)
  without inventing a seam. Stamp each test's own name; the manager's `Caller` of a p2p server is what was
  stamped, `None` when nothing was.
- A new read like `Version()`: at the face pin the body as `(Vec<String>, Vec<String>, u64)` through a
  tuple `match` (an array pattern of three `String` is E0282, `["x".to_string(); 3]` is E0277: write
  `["a", "b", "c"].map(String::from)`; `vec![x.clone(); N]` is fine), and pin it against
  `env!("CARGO_PKG_VERSION")` of the serving crate, not a literal version. The client side pins
  `Some(..)` and, for a missing method, `Ok(None)`: serve a peer object at the manager's path exposing
  only a `Ping`, and say in the report that `Ok(None)` on UnknownMethod is the assumed shape (the pipe
  does that, the bus face's answer is the coder's).
- The object path `/org/privateai/Manager1` is private to `crates/core/src/client/dbus.rs`, so a fake-peer
  interface repeats the literal with a comment that it follows the client. The core client's own tests
  (`crates/core/src/client/dbus.rs::tests`) use `#[tokio::test]` + `zbus::connection::Builder` + a local
  `#[zbus::interface]` peer; the manager's D-Bus tests use `zbus::blocking` on a thread.
- A bound client's own type is the coder's: pin the frame it sends through a helper taking
  `fn(&dyn ManagerClient) -> Option<ClientError>`, so an owned or a borrowed bound client both compile;
  keep the bound client in a block when the test uses the unbound one after it.
- `unused_must_use` under `-D warnings` in a threaded test: `join().expect(..)` on a thread returning an
  `io::Result` yields that result, and so does `recv_timeout(..).expect(..)`: bind each and assert it.
- Windows decisions are OS-neutral pure functions in `manager/windows.rs` and `pai_core::client::pipe`;
  only Win32 glue is `cfg(windows)`. Numeric Win32 values come from the local windows-sys source
  (`~/.cargo/registry/src/*/windows-sys-*/src`), meanings from learn.microsoft.com, a crate's behaviour
  from its `.crate` on static.crates.io. `SID_NAME_USE` (i32) counts `SidTypeDomain` (3), so
  `SidTypeAlias` is 4, not 3. Check a Win32 IDENTIFIER the same way before pinning it: the preshutdown
  window is `SERVICE_CONFIG_PRESHUTDOWN_INFO` with a `SERVICE_PRESHUTDOWN_INFO` (the `PRESHUTDOWN_SERVICE_STATUS`
  named in a brief is no such name: that pin would never green, so `grep -rn` the crate for the identifier
  first). A per-instance state that a delayed-start thread shares goes in `ScmService` behind an `Arc`
  (the service is built once in `entry.rs` and cloned, so a plain field is a per-clone copy; `ScmService`
  is `Send + Sync`). The Win32 glue a pin reads lives in the submodule `manager/windows/scm.rs`, its text
  read by `scm_code()` / `scm_fn(name)` in `manager/windows.rs::tests`. A registry test's guard restores
  every value in `Drop` (no panic there). The SCM reports states on its own: never-started reads
  `SERVICE_STOPPED` with exit code 1077. A service SID (`NT SERVICE\<name>`) is computable per [MS-LSAT]
  3.1.1.1.2 (SHA-1 of the upper-case UTF-16LE name; `service_sid`).
- A seam handed over as pure compile red (no stubs, coordinator's order): prove the test code's own hygiene
  with `cargo check -p <crate> --all-targets --target x86_64-pc-windows-gnu`: the error list must name ONLY
  the missing items (E0425), the site counts differing by exactly the `#[cfg(unix)]` test's share; anything
  else is a typo in the tests. No `running N tests` line in the filtered `cargo test` proves the crate's
  other tests did not run. A `&[OsString]` seam (from `Command::get_args`) takes lossy UTF-8 internally;
  build an invalid-UTF-8 `OsString` in a `#[cfg(unix)]` test with `OsStringExt::from_vec` (bytes with
  0xff), expect U+FFFD in the answer.
- A token holds the local groups (aliases) its user was in at logon, as SIDs: a check by group name
  looks the SID up (`LookupAccountNameW`) and compares it whole; a change of membership counts at the
  next logon. Unverified (fetch failed): UAC sets only the admin groups deny-only, a custom group stays enabled.
- The manager's render (`presets@<id>.ini`): a render that cannot be written is made by RENAMING the
  rendered file away (`<name>.held`) and putting a DIRECTORY in its place, so the atomic write's rename
  fails `EISDIR`; `create_dir_all` on the file that is already there is `AlreadyExists`, not a fixture.
  Tell the instance that sits AFTER the broken ones (in `assignments.ini`) a preset only the NEW pool has:
  the red then shows the missing `[d]`, not a one-trailing-newline diff. Pin the start's wiring with the
  house source lint (`include_str!("dbus.rs")` + `split("#[cfg(test)]")` over the production half): require
  the report's name and forbid the discarding line (`let _ = core.render_all()`) as a second needle.
- A probe that comments out the earlier asserts of a phase must KEEP every `let` the later asserts read
  (`let before`, `let reported`): commenting a binding makes the probe's red E0425, not an assertion. And
  re-take the `.bak` in the right order: the probe writes the real file FROM the bak, so an `edit` applied
  to the real file after a probe lands in the probe's copy (restore, then re-apply the edit).

## Packaging, installers, CI (xtask)

- Most xtask path code has no test: after a path change check the targets exist by hand and say so.
  `linux::package::run` reads the real `build/linux/`: pin decisions as pure functions over a
  `build_root: &Path`. xtask has no `tempfile`: `temp_dir()` + pid behind a `Drop` guard.
- xtask's `windows/` (`cfg(windows)`) and `macos/` never run on Linux: pin choices as items in `common/`
  (`common::msi::CARGO_EXES`, `common/macos_pkg.rs`) plus a text check that the per-OS code uses them.
  Per-OS modules are `#[cfg(unix)]`, so a test importing one needs it too; run `cargo check -p xtask
  --all-targets --target x86_64-pc-windows-gnu` (and `aarch64-apple-darwin` for the Mac side) before
  handing over.
- xtask's CLI: `Cli::try_parse_from` (`cli_tests`); compare `ValueEnum` names via
  `to_possible_value().unwrap().get_name()`. clap 4 refuses `--flag -1` without `allow_hyphen_values`
  (`--flag=-1` parses): panic with clap's error so it is not read as a refusal.
- A new `Component` variant compiles with `todo!` arms in `suffix` (`common/components.rs`) and `texts`
  (`linux/package.rs`).
- A new test file beside a big inline `mod tests`: declare `mod <name>;` inside it (`common/msi/tests/`);
  `use super::*;` reaches private helpers.
- The `.wxs` is parsed with roxmltree (`<?if?>` are processing instructions, entities decoded); `dacl()`
  reads `PermissionEx Sddl` into ACEs so an ACL is pinned by meaning. Extension elements are matched by
  namespace URI (the prefix is not kept). Conditions are pinned by meaning with `holds(cond, props)` and
  `runs_in(doc, action)` in `common/msi/tests/engine_lifecycle.rs` over named scenarios; with
  `MajorUpgrade` afterInstallValidate, a "before files are replaced on upgrade" action runs in the OLD
  package (`UPGRADINGPRODUCTCODE`).
- MSI facts: conditions never compare versions, so a minimum version of another product is `Upgrade` +
  `UpgradeVersion OnlyDetect="yes" Minimum=..` plus a `Launch` on its property. An AppSearch signature
  with no Signature row is a directory search (`appsearch-table`, `drlocator-table` on
  learn.microsoft.com). WiX schema pages (docs.firegiant.com/wix/schema/wxs|util/<element>/) put the
  attribute table past offset 100000; what the compiler accepts is in `Compiler.cs` at the pinned tag.
  Windows Installer removes empty folders it created: a "kept after uninstall" check writes a file there
  first. An msiexec exit code (1603) alone is any fatal error: pair it with the `/l*v` log message.
- WiX Util `Group` under a `Component` creates a local group; by default it is REMOVED at every uninstall
  where it exists (members or not, pre-existing or not; `src/ext/Util/ca/scagroup.cpp` at the tag), so a
  MajorUpgrade empties it: keep one with `RemoveOnUninstall="no"`. Raw sources at
  `raw.githubusercontent.com/wixtoolset/wix/v7.0.1/src/ext/Util/...` (`wixext/UtilCompiler.cs`, `ca/`).
- Linux packages: `check_files` (`linux/package.rs`) checks a spec's `%files` against a temp tree as
  rpmbuild does; reuse it for every new path. ROCm libraries find kpack archives at `<lib>/../.kpack`.
  A shared-library fixture follows CMake's symlink chain (`.so -> .so.N -> .so.N.M.P`), compared after
  `canonicalize`; read the soname from the clone, never an older note. ggml loads backends from
  `GGML_BACKEND_DIR`, then the exe's dir, then cwd, first of equal score winning.
- Maintainer scripts are run in `Sandbox` (`linux/system_files.rs`): fake `systemctl`,
  `deb-systemd-invoke`, `getent`, `chown`, `install`, `systemd-tmpfiles`... log their args; `/run`,
  `/etc/llama.cpp`, `/etc/systemd/system` and other system paths are moved, others refused; `systemctl`
  logs `has-default=yes|no` to order a copy against a start. Add every new tool a script calls to the
  fakes. `run_chroot` refuses any `/etc` path not under `$DPKG_ROOT`. Enabled template instances exist
  only as `multi-user.target.wants` links (`systemctl list-unit-files` lists the template alone): pin
  the links. rpm scriptlets run the same way, `$1` as argument.
- The sandbox helper matches whole whitespace-separated tokens (`call`), so a file name inside a
  path token is never seen: `install -m 0644 ... /usr/share/.../presets.ini` is not a call of
  `presets.ini`. To pin "the copy of <file> against <action>", add a helper that finds the word
  and then the file name in the tokens after it.
- xtask is a bin crate: `cargo test -p xtask --lib` lists 0 tests. Use `cargo test -p xtask --bins
  --no-fail-fast -- <filters>`, and `--bins -- --list` to type-check all of them.
- The xtask suite is flaky in its parallel run: `the_engine_prerm_records_the_running_instances_and_stops_them`
  failed once in six full runs (the recorded list short of `llama.cpp@gpu1.service`) and is green alone,
  in four further full runs and with `--test-threads=1`; its `Sandbox::new("engine-prerm")` root is
  `temp_dir()/xtask-scripts-<name>-<pid>`, so the loop's iterations share one directory. Report it as a
  pre-existing flake, with the reruns that came back green, never as the slice's defect.
- Packaging facts (this machine): local rpmbuild has no `%systemd_post` (write scriptlets by hand); rpm 6
  turns a sysusers.d `m` line into `Requires: group(..)`; debhelper snippets to mirror are in
  `/usr/share/debhelper/autoscripts/`; dbus' trigger is `interest-noawait` (the policy may not be
  reloaded before a postinst starts a Type=dbus unit). A unit on the system bus cannot use
  `DynamicUser=` under dbus-daemon (dropped at Hello): use a sysusers.d user. One `StateDirectoryMode=`
  applies to every `StateDirectory=`: another mode ships as a tmpfiles.d line, applied again by the
  core's postinst after `systemd-sysusers` (a line whose owner is missing is skipped, exit 65: keep `||
  true`). Every new header-free or engine-headed file goes on the exclusion list of
  `the_system_files_and_scripts_name_the_core_package`. `paths::installed_share_dir` is
  `<exe>/../../share`, so a `/usr/libexec/private-ai` program finds no `cli.toml`.
- polkit rules (`.rules`, JavaScript) are pinned by meaning: `rules_answer` (`linux/system_files.rs`)
  runs them under node (a build prerequisite) against a stand-in `polkit` object. polkit facts, from its
  source (github.com/polkit-org/polkit, `src/polkitbackend/`): a rule returning nothing falls back to
  the action XML's defaults (`polkitbackendduktapeauthority.c`); uid 0 is always authorized unless
  `ALWAYS_CHECK` (`polkitbackendinteractiveauthority.c`). The `.deb`/`.rpm` share one staged tree, so a
  per-format file (`/etc/pam.d/private-ai`) is written by `write_deb_scripts` / `write_rpm_files` into
  the core's root; the test helper `files_section` reads `%config(noreplace) <path>` lines by path.
- Check a candidate unit with `systemd-analyze verify` and `systemd-analyze security --offline=true` on
  a /tmp copy with `ExecStart=/usr/bin/true`. Local man pages (systemd 259) are citable.
- Version order: `dpkg --compare-versions a gt b`, `rpm --eval '%{lua: print(rpm.vercmp(a, b))}'`
  (`2.4.0+x` sorts above `2.4.0`); rpm-spec(5), rpm-version(7). Distribution package contents:
  `apt-get download` into a fresh /tmp dir, `dpkg -c`; Fedora specs via
  `curl -sL https://src.fedoraproject.org/rpms/<n>/raw/rawhide/f/<n>.spec`; docs.fedoraproject.org
  blocks scripts, read it through web.archive.org and say so. The predecessor `.deb`/`.rpm` sit in
  `dist/`.
- journalctl's refusal texts: systemd's `src/shared/journal-util.c`; `strings` on the installed
  `libsystemd-shared*.so` gives this version's wording.
- llama-server: a missing `LLAMA_CACHE` lists empty and a download creates it; only `--slot-save-path`
  must exist before start.
- CI workflows (`xtask/src/ci_windows.rs`, `ci_macos.rs`) are pinned as text (`after()` for order,
  `steps()` per step, `switches(..)` regions); prove helpers with a throwaway workflow. Runner contents:
  `actions/runner-images` (`windows-2025` is the VS 2026 image: `Windows2025-VS2026-Readme.md`;
  `macos-15-arm64-Readme.md`). A `pwsh` step exits with the last native `$LASTEXITCODE`: a step printing
  a command allowed to fail ends with `exit 0` (pin it).

## bff and web UI

- Session reply shape: pin it with `Reply::assert_user_reply(user)` (`tests/session.rs`): the
  key-count assert (object has exactly one key, `user`) runs before the value assert, so a reply with
  an extra key reds as "carries more than the user", never as a name typo. Every fake `Identity`
  literal is `Identity { user }` (no role since the pai-only sign-in slice): dropping a field is
  pinned by E0063 at each literal. A `grep can_configure tests/` after such a slice must still hit
  the manager's `(ok, can_configure)` tuple (`tests/authenticator.rs`) and the 403 non-member docs:
  those are the manager's check, they stay.
- Server file key withholding (`tests/server.rs`): the `FakeManager` answers
  `Result<Option<ManagerFile>, ClientError>` via `whole(text)` / `withheld(text)` helpers. The flag
  alone must null the key: `document()` nulled it only via its own `!can_configure` (kept_here), so
  withheld=true with a key in the text reds `left: String("sk-test")` against `Value::Null`, an
  assertion red past the compile red. Three failing integration binaries block every pai-bff test
  target: bound the red with `cargo test -p pai-bff --no-fail-fast --test <the rest> --lib`.
- Sign-in split for tests: the manager's `(ok, can_configure)` mapping belongs to `SystemAuthenticator`
  (fake `ManagerClient` over a `Connect`, plain `#[test]`s in `tests/authenticator.rs`); the HTTP
  mapping (403 `session-not-administrator` is no credential failure: refusals are a counter of their
  own per (lowercased name, IP) within the window, refusals + in-flight reaching the limit makes the
  next request 429, and the 429 check runs first) is pinned in `tests/session.rs` with the `Account`
  fake holding `answer: Result<Identity, AuthError>`, so the fake models the seam's mapped outcome,
  not the pair.
- A "the authenticator was never asked" pin in a test that signs in first compares a baseline taken
  after the sign-in (`let before = app.asked();` ... `assert_eq!(app.asked(), before, ..)`), never
  `is_empty()`: `App::sign_in` asks it once by design (the red lands on the assert's own line, as in
  `tests/origin.rs`). `assert_asks_nothing` keeps its empty checks (connects, calls, principals):
  sign-in reaches none of those seams, only the authenticator.
- A counter rule is pinned as `N` sequential requests of that kind (each asked and answered), then the
  next request 429 without asking, then the same after the window moved (the count expires), then the
  two counters apart (4 of one + 4 of the other: the 5th of each is asked, and locks). Every pin needs
  a fresh `App`: one lockout per fixture. A burst is deterministic in the same shape: each request
  that passes the gate is asked and answers 403, so `403 count == calls` and `403 + 429 == burst`, and
  the asked count lands exactly on the limit serialized or concurrent.
- A "does not lock the other pair" test whose authenticator log is by name, not by (name, address):
  put the asked-while-locked assert right after the locked pair's 429. A later request of the fresh
  pair (same name, other address) is legitimately asked, and a 403 answers only after the ask, so the
  name's total ends at `MAX_FAILURES + 1`, never `MAX_FAILURES`.
- A per-account `hang: Duration` in the fake authenticator (sleep in `authenticate`, before the answer)
  serves the fast and the hanging account in one fake; let the hanging answer be a 200 so an unbounded
  check reads `left: 200, right: 503`, the reason for the red. Keep the sleep 20x the injected bound
  and assert the reply came in less than half the sleep: the bound answers fast, a wait for the answer
  is an order of magnitude late. Sleeps run in `spawn_blocking`, the runtime joins them at the end.
- Inject a production const (the sign-in bound) into an integration test with a new `pub` builder in
  the module that takes it (`session::routes_with_sign_in_timeout(auth, clock, bound)`): one new
  function, no call site of `router_with_session` changes (other test files keep compiling), a `pub`
  stub in a lib crate is no dead code, underscore-prefix the unused parameter. Its 10 s production
  value cannot be waited out: pin it with a source lint over `src/*.rs` (the `const SIGN_IN_TIMEOUT`
  line carries `from_secs(10)`, and the name appears at least twice: declared and used), never
  `assert_eq!(crate::SIGN_IN_TIMEOUT, ..)` (a missing const is a compile error that hides the binary).
- `todo!()` emits no clippy diagnostic on this toolchain (checked with `-W clippy::todo`): stub reds
  under `-D warnings` are only `dead_code` on fields the stub does not read; underscore-prefix stub
  params. A `todo!` in an axum handler body unwinds the `oneshot` caller: a test task spawned
  concurrently reads it as `JoinError::Panic`, not a 503; a panic inside a `spawn_blocking` closure
  is caught by the `JoinHandle` and mapped by the handler (`unwrap_or`).
- An axum router is tested with `router().oneshot(request)` and `BodyExt::collect` in `#[tokio::test]`;
  a route asking the manager through `router_with(connect, timeout)` and a fake `ManagerClient`, every
  request under `tokio::time::timeout`; a sleeping fake runs in `spawn_blocking`, which the runtime
  awaits at its end, so keep sleeps short. A new route needs no stub (axum's empty 404 is the red);
  check the panics point at reply assertions, not the sign-in helper.
- `ConnectInfo<SocketAddr>` is inserted into extensions for `oneshot`, which hides missing
  `into_make_service_with_connect_info`: pin the wiring with one request to the binary. The binary's
  socket test reserves a free port, polls a raw HTTP/1.1 GET over `TcpStream` with a deadline, kills the
  child in a `Drop` guard.
- A rate limit or lockout: fake that sleeps in the seam, `multi_thread` runtime, a `JoinSet` of N
  requests, assert the seam's call count; try the key spellings the backend treats as one.
- A value sent by role: add a second user to the fake authenticator and `sign_in_as(user)`; assert the
  secret's text is nowhere in the body, not only the `null`.
- The embedded page is tested on what the build embedded (`PAI_BFF_FE_DIR` from `build.rs`, `tests/fe.rs`
  `embedded()`), following `index.html`'s `/assets/...`; only `crates/bff/fe-fixture` exercises CSS.
  That env var is baked by the build script, so a build script output left by a removed worktree points at
  a path that no longer exists: `cargo clean -p pai-bff -p pai-core` (plus `--target <triple>` for the
  cross check) before the rerun, and read `tests/fe.rs` as the observable that the rebuild baked this
  repo's path (5 tests, byte-for-byte, they red naming the missing dir on a stale one).
- Read a JSON type from `fe/src/ServerForm.tsx` (`fromValues`/`toValues`) before pinning it
  (`RocblasUseHipblaslt` is `"on"`/`"off"`).
- fe tests (Vitest, jsdom): globals off, so call `cleanup()` in `afterEach`; mock fetch with
  `vi.stubGlobal('fetch', vi.fn<typeof globalThis.fetch>(..))` (bare `vi.fn` types calls as `[][]`).
  Vitest does not type-check: run `npm --prefix fe run typecheck`. Debian's npm 9.2.0 dies on mismatched
  peer ranges (`reading 'edgesOut'`): align versions, try installs in a copy of `package.json`.
- Read new strings through a `text(key)` helper that throws on a missing key. `strings.test.ts` sees a
  key only as a literal `t('key'` and flags letters between `>` and `<` in a `.tsx`; check JSX text with
  `ts.createSourceFile(.., ScriptKind.TSX)`, not a regex.
- Several components read `/api/instances`: a `Response` body reads once, so gate reads
  (`() => gate.promise.then(() => reply(..))`) instead of sharing one. For a deduplicating read, hold one
  pending when the next consumer's is due. Focus moves to the `h1` on route change: `navigate()` first.
  A `hidden` hint is still in `textContent`. A `#` link moves `location.hash` in a later task: wait one
  `setTimeout(0)` in `act`. jsdom's `HTMLDialogElement` lacks `showModal`/`close`. Scope queries to
  their section (header controls break whole-main queries). After an interrupted session run the file
  first. Mutation runs go in a scratch copy of `fe/` with `node_modules` symlinked.
 - A mutant that swaps one table value for a value the SAME table computes is EQUIVALENT, not a coverage
   gap: `selectState`'s `row.absent` equals `row.options[0].value` for all 19 select rows of
   `PresetCards.tsx`, so `options[0].value` cannot red any test. Prove equivalence by scanning the table
   (python over the source, one line per row: key, absent, first option), and kill the NEAR mutant
   (`absent: 'f32'`, a row whose absent is not the first option): that one is killable, and the case you
   write for it pins the equality the equivalent mutant destroyed.
 - A number field's `inputmode` is DERIVED from the range table (`numericMode` reads `INT_KEYS[].min`), so
   a mutant that gives one key a minimum fires the input-mode pins too: count those reds as collateral of
   one change, not as extra coverage.
- A mutant sweep: a python driver that applies each mutant with `str.replace` guarded by
   `count(old) == 1` (a regex pasted into sed dies on `/` and `\\`), runs the named test files, greps
   `×` + the `Tests ` line, and restores a `.pristine` copy after EACH mutant. A "check only changed
   fields" mutant (an unchanged loaded value escapes the check) is killed by the untick-and-empty refusal
   rows: the untick leaves the text equal to the loaded text, so those fields fall out of the changed set.
- A hand check against the real bff: drive the snap's `geckodriver --port <free>` over plain WebDriver
  HTTP from `python3 -I` (`-headless`, screenshots as base64 over HTTP, 2 s after each load; Firefox
  stops at 500 px wide). Kill it with `snap run --shell firefox.geckodriver -c 'kill -TERM <pid>'`. The
  bff's target dir here is `build/windows/cargo-target`; `grep -a -c <asset name>` on the binary
  confirms it embeds the new `fe/dist`.
- Web UI: the fake bff's `deferred()` ignores abort, so a GET held for 15 s or more of fake time
  must use `held()`, or a timeout test passes without a real browser's behaviour.
- Web UI: a settings-group checkbox takes its accessible name from the row's `<label>`, not from the
  group title, so `getByRole('checkbox', { name: <group title> })` finds nothing (the group itself
  answers to that name). Query it as `box(main, fieldLabel(main, t.<key>))` and keep the group title
  for the `role=group` query.
- Web UI: a form control is `aria-disabled` only when it is really inactive (`{cond || undefined}`
  writes no attribute), so `getAttribute('aria-disabled')` is null on a live control: use the file's
  `inactive()` helper, and for a "nothing happens" click on a live control read the writes log, not the
  attribute.
- Web UI: a 401 on the file read renders the read-failed state with no form, so a test that waits for
  a section heading (the form) of a withheld document fails on the setup. Pin "the secret is nowhere"
  with the whole `document.documentElement.outerHTML` (the section heading alone lives in `main`), and
  after a 27d-style read failure assert `status-error`, the group's absence and that no input is a
  text box.
- Web UI: a disabled control is skipped by Tab, so a tab-order test for a non-configuring session
  reds while the page still disables it, and goes green when the slice makes it live: cite both the
  tab criterion and the state criterion there.
- Web UI: session-change tests should split the axes: same `user` with the other `canConfigure` (the
  page must not move) and a different `user` (the page reloads). Keep the banner's role word in the
  dedicated role-word test, so the session tests stay green when that wording changes.
- Web UI: when a later slice adds a control or live region to a page, the earlier slice's tab-order
  and "no status" tests break by design; update them in place and cite both criteria.
- Web UI: `input type="password"` is found by `getByLabelText(.., { selector: "input" })`, not by
  `getByRole("textbox")` (dom-accessibility-api gives it no role here).
- Web UI: the `EN` table in `fe/src/test/bff.ts` is the mirror App.test.tsx's "1a strings" test compares
  with `fe/src/strings/en_US.json`, so adding a new key there reds that test (`keys missing or with other
  text: ['<key>']`) until the coder adds it to `en_US.json`. That is the new string's own red, report it
  as such. `strings.test.ts` (the "every key the code uses" scan) skips test files, so a test-only EN key
  never trips it.
- Web UI: run Vitest from `fe/` (`cd fe && npx vitest run <file>`); `npx`/`npm exec` from the repo root
  resolves no local vitest. The three checks (`typecheck`, `lint`, `format:check`) do run from the repo
  root as `npm --prefix fe run ..`. `typecheck` before the suite: it reports an unused helper (TS6133)
  that Vitest ignores. Keep a run's output small through
  `grep -E "✓|×|Tests |Test Files |Error|expected|Unable" | cut -c1-200 | head -50`.
- Web UI: the reds of a big table-driven file are read as a population, not one by one:
  `grep -E "AssertionError|TestingLibraryElementError" | sort | uniq -c` over the run's output shows in one
  pass that every red names a missing control and no red is a helper typo (`TypeError`) or an unhandled
  rejection. Prettier reformats a fresh test file and the fixtures it edited: run
  `npx prettier --write <files>` from `fe/` before `format:check`, then re-run the file (the format pass
  must not be trusted to keep the reds).
- Web UI: `fe/src/test/presetCards.ts` is the shared source of the whole-page assertions, so adding a card
  to `PRESET_CARDS` for a new slice reds, by design, the earlier slices' tests that list the page:
  PresetContent's heading-order and tab-order, PresetPage's "one About toggle per field", PresetCards1's
  card order / tab order / strings, PresetCards2's tab order. They read as "the page has 27 of the 40
  expected" (`expected [ Array(27) ] to deeply equal [ Array(40) ]`, `expected '1 2 3 3' to be
  '1 2 3 3 3 3'`), the shape that names the shared list, so report them as the slice's shared-fixture reds
  and do not touch those files. Prove a red is yours from its assertion shape, never with `git stash`
  (shared tree, see General).
- Web UI: a checkbox field is a `role=group` (so `fieldControl()` and the name queries work on it), but its
  input carries no `aria-describedby`, so `descriptions` is empty: the "the hint text is not on the control"
  assert is vacuous there, and the hint's presence is pinned only by opening it. `fieldControl()` throws for
  a control that is not a group (a range), so a table with a range row needs its own query.
- Web UI: in jsdom an `<option>` has no `checked` (it reads `undefined`, only `selected` exists), so a
  select's current choice is pinned by `select.selectedIndex` (its option's text, the thing the user
  reads) or by `.value`, never `input.checked`; note `selectedIndex` is 0 by default, so the
  "an option is selected" guard is weak and the text compare carries the test.
- Web UI: the full fe suite has pre-existing flakes in `PresetPage.test.tsx` (2d.56, 2d.63) when the
  suite is loaded: they pass in their own run. Check a suspect test in its own run before blaming the
  slice (files are isolated, so one file cannot pollute another); a clean-tree stash of a shared helper
  only proves what the file-alone run does not.
- Web UI: to hand a handler a null `document.activeElement`, define an own getter on the `document`
  instance (shadows the prototype), delete it in `finally`, and probe with your own `hashchange`
  listener that the stub held when the handlers ran.
- Web UI: a throw from a React click handler is swallowed at jsdom's listener boundary: fireEvent
  returns, the test continues, and vitest reports the TypeError as Unhandled at the end, so the
  assertion after it also reds.
- bff `Connect` (`Arc<dyn Fn(Option<&str>) -> Result<Client, ClientError> + Send + Sync>`): each test
  binary's fake connect records the principal it was called with in an `Arc<Mutex<Vec<Option<String>>>>`
  (`principals_of(&log)` reads it), and every fake closure is written `let connect: Connect =
  Arc::new(move |principal: Option<&str>| ..)`, so the type is pinned at the closure: against the old
  zero-argument `Connect` all four binaries red as `E0593: closure is expected to take 0 arguments, but
  it takes 1 argument` (six sites), which is the signature red, kept separate from the assertion reds.
- bff principal refusals: give the `FakeManager` (`tests/server.rs`) `as_user: Option<String>` (the
  account a read answers for) and `refuse: Vec<String>` (`is_admin` is the membership), and answer the
  read of a refused account with `ClientError::NotAuthorized`, the kind the manager gives every principal
  refusal. One app then refuses alice and answers bob, so 403, "the session that acted is the session
  that ends" and "an unrefused user keeps their session" run in one test, each with its own cookie.
- bff "this refusal ends the session" is pinned as the NEXT request with the same cookie answering 401
  `session-required` (the reply's `Set-Cookie` clear is not pinned: not in the design, say so in the
  report). Pin the negatives too, since they are the rule's edge: the same cookie after a 503
  (`Unavailable`) and after a 502 (`Io`) is still 200, and a 403 on the session-free `/api/instances`
  ends nothing (that route has no session, and it is where a 403 from the bff's own rule lands).
- bff idle deadline: `Sessions::current` refreshes the session's `used` stamp on EVERY cookie-bearing
   request, so a probe (`GET /api/session`) moves the deadline: a "the poll must not keep a session
   alive" rule is pinned with one app per point and the probe LAST (sign in, advance just under the
   timeout, run the request under test, land the clock exactly on the sign-in's deadline, probe alive; a
   second app lands one second past it, probe 401). Never probe mid-test, the probe greens the next
   point for the wrong reason. The control is the same steps with an action, which does move it. The
   check is `> IDLE`, so the boundary sits one second past the timeout, and a Duration step is
   `IDLE - SHORT + Duration::from_secs(1)` (no `+ 1`).
- bff read-back is not an echo: keep the manager's state in an `Arc<Mutex<bool>>` cell shared by every
   connect of one app, and let a scripted write leave it at `after_write`, which may differ from the body.
   Assert the reply against the cell for divergent pairs (asked `true`, left `false`) and pin the calls log
   as `["set_boot_enabled <id> <asked>", "boot_enabled <id>"]` with ONE principal: the read ran after the
   write, on the same connect, so a reply that echoes the body, a skipped read and a second
   session-carrying connect all red.
- `let app = app();` twice in one test shadows the builder fn: the second call is `E0618: expected
   function, found App`. Name the apps (`alive`, `gone`). A fake holding a `Result` field cannot
   `match self.answer.field { Err(e) => .. }` (E0507, moves out of a shared reference): `.as_ref()` plus
   `e.clone()`.
- The request helper of a bff test file takes `extra: &[(&str, &str)]` and parses each name with
   `name.parse::<HeaderName>()`: that is how "a name in the request is never the principal" is pinned for
   a header (`x-user: root`) as well as a query parameter, one builder for both. The sign-in helper posts
   `{"username", "password"}` and inserts `ConnectInfo` into the request's extensions (the session route
   needs it, a control route does not).
- A stub that keeps the old `connect(None)` call greens the session-free routes (the instances route,
  sign-in) and reds only the principal and session-end asserts: `left: [None], right: [Some("alice")]`
  for the principal, `left: 200, right: 403` for the refusal (the stub connects an unrefused principal),
  `left: 200, right: 401` for the session end. Adapt the existing fakes mechanically (a closure ignoring
  the argument) so their tests stay green and the reds name the slice.
- Probe an older commit's sources without touching the shared tree: `git worktree add --detach build/<name>
  <rev>` (gitignored), copy the test files in, write the worktree's own `.cargo/config.toml` with an
  ABSOLUTE `target-dir` pointing at the shared tree's (`build/windows/cargo-target`), else cargo rebuilds
  every dependency; pai-bff needs no `pai-manager` dependency and its `build.rs` falls back to
  `crates/bff/fe-fixture` when `fe/dist` is missing, so a bff probe compiles in two minutes. Clean up with
  `git worktree remove --force` (the config you wrote is untracked).
- bff connects are per ROUTE, not per manager call (ARCHITECTURE.md: "every route behind a session
  connects as the session's user"): the boot write and its read-back are two calls of ONE connect
  (`control.rs`'s `boot_write`), so start + stop + restart + `PUT .../boot` is 4 entries in
  `principals_of` and 5 in the calls log; count routes in the connect assert, the read-back belongs to
  the calls log. A fake that returns `self.act(..)?` on the write logs no read, so the calls log also
  pins "no read-back after a failed write".
- A bff fake whose `verb` fails and whose `set_boot` answers OK makes a `PUT .../boot` answer 200
  `{"enabled": <state>}`: a test driving the boot route's failure needs `set_boot: Err(..)` as well
  (builder `app_failing(failure)`: verb and set_boot). A `for app in [app_verbs(..), app_boot_write(..)]`
  loop that asks every URI on each app is a latent bug: the first app's red stops the loop, the second
  app's broken half (a verb answered 200) never runs; one app failing both verb and set_boot covers both
  URI groups in one pass.
- "The refusal asked the manager for nothing" on a log the test's own helpers also fill (a sign-in, a
  fake authenticator that never reaches the manager at all): pin it as the log UNCHANGED across the
  refused request (`let before = app.calls(); .. assert_eq!(app.calls(), before)`). An absolute
  `vec!["set_boot_enabled ..", "boot_enabled .."]` there is the helper's shape, not the refusal's, and
  reds on its own.
- A request that must be refused by one of two rules whose ORDER the design fixes nowhere (a cross-origin
  body over the cap): pin it as "one of the two codes, and both fakes' logs empty", and write in the file
  header that the order is not pinned. An axum body-limit layer (`DefaultBodyLimit`) answers before a
  middleware guard, so a code-pair assert is the only shape that greens either way.
- A refusal assert on a 204 (`No Content`) prints nothing when its message is the body: write
  `assert_eq!(status, code, "the body was {:?}", body)`, and in a table loop put the case label on the
  "nothing was asked" assert that runs after it, since the first refusal reds and stops the loop.
- The port edge of a same-origin rule comes from `SameSite=Strict` comparing the SITE (scheme, host,
  registrable domain), never the port: a page of another local app on `http://localhost:<other>` sends the
  cookie. So pin `Origin` against `Host` in the three shapes a browser really sends: same host with
  another port, the scheme's DEFAULT port written bare (`http://localhost` against `localhost:3000`), and
  a `Host` naming no port (`localhost`, the Origin carries `:3000`). `Sec-Fetch-Site` is pinned as a
  table: `same-origin` and `none` pass, `same-site` and `cross-site` are refused, a bare `null` (the
  literal Firefox sends for a non-HTTP origin) and anything else are refused.

- fe: the type check that sees test files is `npm --prefix fe run typecheck` (`tsc -b tsconfig.json
  tsconfig.test.json`); `npx tsc --noEmit -p tsconfig.json` checks the app only, so a test file's own type
  errors reach only the npm script. `npm --prefix fe run test -- <file>` does not filter (vitest reads the
  positional as its start command: "command 'start' not found"): run
  `cd fe && npx vitest run src/<file>.test.tsx`. For per-file counts over the whole suite use
  `npx vitest run --reporter=json --outputFile=<file>` and read `numFailedTests` with node
  (`--reporter=basic` is gone in vitest 3.2.4: `reporter.getEntries is not a function`). Format with
  `npx prettier --write <file>`, then `npm --prefix fe run format:check` from the repo root.
- fe: a card added to the shared `PRESET_CARDS` list (`fe/src/test/presetCards.ts`) reds EVERY whole-page
  test of the suite, not only the new file: the card-order and tab-path tests of each cards file, the card
  count / strings-parity / card-order test of the content test, the page test's tab path, the heading-order
  and About-list tests, and `App.test.tsx`'s strings test (the key list the coder has to land). Count them
  in the report as collateral reds of the shared list, with one line per file.
- fe: making an EXISTING card editable is done in the shared table, not in a page test: add its label keys
  to `EDITABLE_FIELDS` and, for a number field with a default box, its label key to `BOXED_FIELDS`
  (`fe/src/test/presetCards.ts`). `fieldTabStops` and `controlEnabled` derive from it, so the collateral reds
  before the coder are exactly the tab-path tests (`2e.91` in each PresetCards file, `2d.71` and `2d.78` in
  PresetContent): a card with 3 selects and 4 boxed numbers adds 11 stops (4 boxes + 7 controls). The reds
  AFTER the coder are the read-only pins of the same card: `PresetFullValue` (134: a full-value line shows
  only under a DISABLED control, so every long `cache-ram` / `cache-type-k` line of the card vanishes) and
  `PresetCards2`'s `${key}: the select disabled` / unit / inputmode pins. List them for the coordinator, do
  not edit them.
- fe: React skips `onChange` on a DISABLED control, so `fireEvent.change` on a not-yet-editable field records
  nothing: the form stays clean, the Save button stays `aria-disabled`, and a `save()` helper's `waitFor`
  times out ~1 s later at `expected [] to have a length of 1`. That is the right red for a card the page keeps
  read-only, and it is why every "sends" test of a card that is not editable yet reds at the PUT count while
  every "enabled" test reds at `expected true to be false` on `.disabled`. A client-check test of such a card
  reds at `Unable to find ... save-invalid` (typing did nothing, so no check ran), NOT at the error text: say
  so, the coder must not read it as a missing string.
- fe: `presetDoc(values, extra)` puts `extra.values` OVER the whole default `values` object, so a preset-page
  fixture that names only its own card's keys drops `model` and `spec-type` too: the page renders (the Assets
  card shows them as unset), so the fixture of a card slice needs no `model`. A key the file lacks is ABSENT
  from `values`: a `''` entry is a value, not an absence, and shows as an extra option, not `default`.
- fe: inside a card there is no group and no note per field: only the card's group and the card's note (a
  note per field was the draft card's shape, the preset cards have none), so a structural page test can
  assert a card is the region's first child and that every row is a group with its legend. A checkbox whose
  text the brief gives in two states (the Fit toggle: allowed and not allowed) is found as the group's only
  checkbox (`[role="group"]` + `queryAllByRole("checkbox")`, expect one), never by name.
  `container.querySelector("main")` is an `HTMLElement` (no `.main`): use `container.querySelector('[role="main"]')`.
- fe: pin a "the hint shows no extra text" rule (the slider maxima, a switch that is not offered) only after
  proving the needles are absent from the pinned strings themselves: a python pass over the strings table
  prints the offenders, else the red names the strings table and not the card. Extract the pinned EN texts
  from `crates/core/strings/en_US.toml` in the same pass: `{0}` is literal there (the tests' `en()` compares
  exactly), and the GUI strips the " (MiB)" a Slint widget adds, so the table text is the bare sentence.
- fe: when one card's state depends on the document (the Fit toggle reads the doc's context), give the
  render helper a handler parameter and a section builder that sends the values EXACTLY (no
  normalisation, no added keys): "shown exactly" and "an absent key stays absent" are untestable through a
  builder that rewrites the document. A card whose row table drives three `test.each` blocks needs one
  helper taking the field kind (`range`, `checkbox`, `select`, `text`, `device select`): the device selects
  share one list of option texts, so "its option is selected" is the only select assertion that pins a value.

- bff: a route's answer is `{"key", "args": []}` and the test binaries share a local `assert_error(status,
  key)` helper, so a slice that changes only the KEY reds on its `assert_eq!(json!, ..)` (the status assert
  greens) and the panic prints the whole body: `left: {..."key": String("client-not-authorized")}`,
  `right: ...String("session-not-administrator")`. Read that pair before chasing: it names which half moved.
  The key split to keep straight: 403 `session-not-administrator` is what the bff sends exactly when it ends
  a session on a refusal (`NotAuthorized` on a call made as the signed-in user: the server read, the control
  verbs, the boot write); 502 `client-not-authorized` is the session-free reads (`GET /api/instances`,
  `GET .../boot`), where the manager refused the bff itself. The guard for the second lives in
  `tests/instances.rs` and `tests/control.rs`: run those two tests by name and report them as guards.
- A long cargo log: `tail -70` of a two-binary run cuts the second binary's failure block (the first
  binary's summary looks like the whole result). Write it to a file, then grep `test result|Running|^error`
  for the map and `sed -n '<a>,<b>p'` for each block.
- `cargo test -p <crate> --no-fail-fast` runs NOTHING when one target fails to compile (cargo stops after
  the compile step, `--no-fail-fast` only unrolls the run): report which binaries did not run, and run the
  untouched binaries in a second invocation (`--lib --test a --test b`) to show them green.
- A keyed hash the code must match: the published SipHash-2-4 vectors are offline in the vendored crate
  (`~/.cargo/registry/src/*/siphasher-1.0.4/src/tests.rs`, 64 of them: key `00 01 .. 0f`, input the first
  n of those same bytes, output little-endian). A reference program is two `rustc` lines (build the rlib
  from that registry source, then `--extern siphasher=...rlib`), no cargo and no network; reproducing all
  64 is the check, because a hand-written Python SipHash goes wrong in the round: `v1 ^= v0` uses the
  ROTATED `v0` (`v1 = ROTL(v1,13); v0 = ROTL(v0,32); v1 ^= v0`).
- Pin the printing convention with the vector: `format!("{:016x}", SipHasher::new_with_key(&key).finish())`
  of the empty text under `00 01 .. 0f` is the published `726fdb47dd0e0e31`, so one assert pins the
  algorithm, the key bytes and the hex format at once. Say in the report that the coder's `finish()` must
  be printed as `{:016x}` (the same `{hash:016x}` shape the FNV revision used).
- A parameter added to a public router builder: the test call sites are the tester's, so add it LAST and
  give each test file one `const REVISION_KEY: [u8; 16]` (its value is free for a route that never reads
  it): the coder then touches only `lib.rs`. Inside the callee the new name must not clash with an
  existing local (`let (status, key) = ..`): rename the LOCAL in the throwaway candidate and tell the
  coder which name is free.
- A byte alphabet check on a `bytes()` closure is `(b'a'..=b'f').contains(&b)`: a `char` range there is
  E0308 ("expected `u8`, found `char`").
- Checking a pinned digest of a file's text (a revision of the shipped `server@default.ini`): feed the
  file's exact bytes through the authors' reference C (`curl` `siphash.c`/`siphash.h`/`vectors.h` from
  `veorq/SipHash`, CC0; the entry point is `siphash(in, inlen, k, out, outlen)`, the older `siphash24(..)`
  name is gone), and GATE the program by reproducing all 64 `vectors_sip64` first: a reference that
  self-validates is a check, a hand-transcribed one is not (mine reds on every value). The `out` bytes
  are the word little-endian, so `%016llx` of the LE word is the shape `{:016x}` prints.
- `/tmp/opencode/<name>` is shared with the other agents: a scratch dir of a common name (`sipref`) was
  already full of the coder's rlib and program built FROM the crate under test. Pick a name of your own;
  a reference built from the code under test proves nothing.
- The bff's key-draw failure has no test and none is cheap: `try_router()` (the `getrandom::fill` of the
  revision secret), `ServeError::Key` and its `bff-revision-key-failed` text have no seam to inject a
  failing OS random source, so report them as untested, and say that "a restart makes every open form
  stale" is pinned only indirectly (two test-supplied keys, not two `try_router()` routers).

- A live shared tree (the coder rewriting the same crate while you write): append your section at the
  file tail and your helpers INSIDE the existing blocks (`impl App`, `impl Reply`, the fake's cells);
  a parallel merge silently adopts your harness and rewrites the tests around it, so re-read the whole
  file before every edit and recompile against the CURRENT trait (`read_file_as` with a principal and
  `write_file_if` with 5 parameters were HEAD fictions; the trait's identity rides the connect).
- A CAS fake that must model a save: record the write, THEN pause, THEN compare-and-swap against the
  live file (shipped text as fallback): a pause before the record cannot show "504, the write reached
  the manager"; a CAS with no live file cannot show 409 Changed. A `status` that reads a `running` cell
  pins the contract's running-after-write rule (true also when the status read fails: a separate
  outcome cell, not the write's).
- Multi-line `app.put(Some(&c), &save_body(&doc, &[("Port", json!(N))]))` call sites: do not hand-balance
  the closers across many tests (five of them, `)]` order, three repair rounds). Add `App::save(cookie,
  doc, overrides)` so each call site is one balanced call, and build the balanced tail by scanning the
  save_body span, never by counting parens by eye.
- `cargo test -p <crate> <word>` filters TEST names, never target names: `cargo test -p pai-bff listen`
  ran 2 tests and skipped the `tests/listen.rs` tests whose names hold no "listen". Run a file with
  `--test <name>`, and account for every `N filtered out` line in the report.
- Flipping a refusal whose enum variant and strings key both go away: removing the last `Variant(..)`
  use in the tests is what makes the test binary COMPILE, so the red is the assertion
  (`left: Err(NotLoopback(0.0.0.0:8590)) right: Ok(0.0.0.0:8590)`), naming the dead variant. The strings
  key of that variant is pinned by the sentence the executable prints, in `tests/pai_bff_binary.rs`
  (the red quotes it: "is not a loopback address"); the key's removal from `cli.toml` is already pinned
  by `crates/config/src/tests/binding_lint.rs` ("in cli.toml but used by no source the CLI reaches"), a
  lint in ANOTHER crate: report it, add no second lint.
- A guard that runs before the router (the bff's `origin::guard`): a test that expects the LATER rule's
  key on a request the EARLIER rule refuses reds `left: {"key":"request-bad-host"} right:
  {"key":"request-cross-origin"}`, which names the rule that must change; a test that expects
  acceptance reds `left: 403 right: 204`. Prove the rest of a list element by element with a std-only
  `rustc --edition 2021` probe of the pure predicate (the host split, the address parse): it prints
  today's answer for every element, no crate build.
- Pin the fail-closed shapes beside a new "any IP literal is ours" accept list, in the REFUSAL test: a
  bracketed text that is no address (`[not-an-address]:8590`) and a port over 65535
  (`192.168.1.10:85909`), because the authority is parsed before the address is read. Free wildcard
  port: `TcpListener::bind("0.0.0.0:0")`, then GET `127.0.0.1:<port>` (a loopback client reaches a
  wildcard listener, so the test proves the bind-all with no second interface).
- `vec![("host", host), ("origin", format!("http://{host}").as_str())]` is E0716 (the temporary dies at
  the end of the `let`): bind the `format!` in its own `let` and take `origin.as_str()`.
- A guard rule whose input moves to "computed once at the start, passed into the router": turn the
  test file's `new_app()` into `app_with_hosts(&[])` and build the router in a new sibling
  (`router_with_session_and_hosts`), so every existing test keeps running and pins the empty-list
  case for free, one builder and no twins. Stub the sibling delegating (old arity, `_hosts` inert):
  the accept tests red `left: 403 right: 204`, the test that expects the LATER rule's key on a
  request the new name rule must let through reds `right: request-cross-origin`, and every
  pre-existing refusal plus the "IPs are answered whatever the list holds" test green BY CONSTRUCTION:
  name those guards in the report.
- "The config list replaces the automatic names, it does not widen them" is pinned by putting one of
  the automatic names (`aorus.local`) in the refusal list beside a list that holds only a longer one
  (`aorus.bombicci.local`). A name set compared as a set: one generic `fn sorted<S: AsRef<str>>(names:
  &[S]) -> Vec<String>` sorts the `Vec<String>` the code returns and the `&[&str]` literal alike. Of a
  `resolv.conf` reader pin the lowercase keywords (glibc's are `search`/`domain`, do NOT pin `SEARCH`),
  the case of the host name and of the `Hosts` value, a tab-separated list, a bare `search` line, `#`
  and `;` comments, `nameserver`/`options` lines, and that an empty `Hosts` value sets nothing, the
  same answer `parse_hosts` gives for that file, so the chain `parse_hosts` then `host_names` holds.

## Web UI (fe/) navigation and CSS pins

- The browser's Forward is testable after the guard's Keep editing: a typed hash adds a history entry
  and the guard puts the address back with a traversal, so the history is `[Models, preset, Models]`
  and `window.history.forward()` (in a task, like `back()`, as `LeaveGuard.test.tsx`'s `pressForward`)
  re-asks a dirty form. A page reached by `startAt(<hash>)` has no forward entry, so build the history
  with a typed hash, never with a second render.
- A rule that lives only in CSS (a sticky save bar, D4's `scroll-padding-bottom`) is pinned by reading
  `index.css` (`readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')`, comments stripped, then
  match the selector and the declaration). It is green the day it is written: report it as a guard,
  not a red, and say which half of the criterion it cannot reach (jsdom has no layout).
- "Any answer that is not a shaped 200" is one test with a table of answer cases (204, a 200 missing
  `running`, a 200 missing `revision`, a 200 with a non-string `revision`, a 200 that is not JSON, a
  500 with a key, a `Promise.reject(new TypeError('Failed to fetch'))`): one page per row and
  `cleanup(); vi.unstubAllGlobals()` at the end of each iteration, so each row reports its own label.
  A fe time limit is the one row to leave out when the code has no clock to control (fake timers on a
  fetch the stub owns pin the stub): report it untested.
- A 5xx whose body names a secret (`{ key: 'secret-detail' }`) is the fixture that makes "the bar's
  alert cleared" testable in a slice with no client checks and no keyed answers yet: the alert exists
  to clear, and the same row asserts the key never reaches `document.body`.
- A page whose save answers only the generic `save-failed` today: every keyed-answer test reds on the
  alert TEXT (`Unable to find an element with the text: <brief English>`), and the guard-key test of
  "anything else is the generic failure" (403 `request-cross-origin`, `request-bad-host`, 413
  `payload-too-large`, 400 `presets-malformed`) is GREEN by construction: report it as a guard, it is
  the net the new reading must not break.
- A fe time-limit test needs no new clock in the page: `api.ts call()` already aborts a non-GET at
  120 000 ms, so a `held()` PUT plus `advance(119_999)` / `advance(1)` (the `fakeClock`/`flush`/`advance`
  of `Timeout.test.tsx`: fake `setTimeout`/`clearTimeout`/`setInterval`/`clearInterval`/`Date` only, so
  the microtasks stay real) shows the abort is wired while the red is the message the page renders.
  `afterEach` needs `vi.useRealTimers()` beside `vi.unstubAllGlobals()`.
- `App.test.tsx`'s strings pin (`en_US.json holds the English the brief gives`) lists every `EN` key that
  `en_US.json` lacks, so adding the slice's strings to `src/test/bff.ts` grows that existing red by one
  per key: name it as pre-existing (the 2h.2 422 keys were already in it), not as a new failure.
- A preset page reload test (409 then Reload) needs a stateful read stub: `preset: () => ++reads === 1 ?
  reply(200, presetDoc({}, { revision: 'r1', values: A })) : reply(200, presetDoc({}, { revision: 'r-9',
  values: B }))` (`presetDoc`'s `extra` replaces `values` wholesale, not merged), and the second reply
  can be a failure reply to test the failed read of the same flow.
- A test file past ~900 lines splits into `<Name>1` / `<Name>2` with the helpers COPIED: a test file
  imports no other test file, and the copy keeps each file runnable on its own. Splitting one back
  means hand-reverting the setup edits (imports, `beforeEach`, `afterEach`): a file the slice created
  is untracked, so `git checkout` cannot restore it. Truncate at a known line and edit the header,
  the import list and the hooks by hand, then re-run the file to show it reds on its own criteria.
- jsdom has no `Window.isSecureContext` at all (grep of `node_modules/jsdom/lib`: no implementation), so
  `window.isSecureContext` is `undefined` in every fe test: the "property absent" case is jsdom's own
  state and a `true` case is a real guard, both green. Set it with
  `Object.defineProperty(window, 'isSecureContext', { value, configurable: true })`, and remove it with
  `delete` in the `afterEach`, after `cleanup()`.
- A rule of a shared component (the sign-in, shown in `main` and in the session-ended dialog) is one
  rule: test the page case and the dialog case in the same file. The dialog needs no new page: the
  preset page's Embedding mode select, Save, a `presetSave` answered 401
  (`{ key: 'session-required' }`), the bar's Sign in, then `screen.getByRole('dialog', { name: en('signin-title') })`
  and the `stubDialog().opened` pin, all in one mount.
- A preset page's card controls are NOT in the document when its h1 is (the cards render with the read):
  a `getByRole('combobox', ...)` right after the h1 reds "Unable to find an accessible element with the
  role combobox", which hides the slice's real red. Use `findByRole`, and assert the fixture's `values`
  carry the row's keys (`{ embeddings: 'true', pooling: 'mean' }`).
- `getByRole('button')` returns `HTMLElement`, so `.disabled` is TS2339 in `npm --prefix fe run typecheck`
  and passes the Vitest run: Vitest never type-checks. Cast `as HTMLButtonElement` at the query.
- `src/test/presetCards.ts` is the one table behind every whole-page expectation of the preset page
  (headings, About toggles, tab path). Extending it for a newly editable card turns the older whole-page
  tests red BY DESIGN (`PresetCards1..5` 2e.91, `PresetContent` 2d.71 and 2d.78, all tab paths): list
  those in the report as the reds the coder's change greens, and never edit them in the red step. A
  checkbox's tab stop is named by its TOGGLE text (`<key>-toggle`), not by the field's label, so the
  table needs a label to toggle mapping; a boxed number adds a stop for its `field-default` box as well.
  A field that needs a row KEY (the n-gram labels repeat: `Lookup n-gram` stands for three keys) joins
  as `{ label, key, boxed? }`, and the union test is `'key' in field`, NEVER `typeof field === 'object'`:
  `shown(...)` is an object with no `key`, so `fieldLabel` returns undefined and `labelText` dies
  `Cannot read properties of undefined (reading 'shown')` in every test that calls `fieldLabels()`
  (`PresetPage.test.tsx` 2d.67's button list). That red is a helper crash, not a behaviour gap.
  Prove a table rule for a newly editable card by the DELTA of the tab list: the shared `presetDoc` read
  holds `spec-type = ngram-mod,draft-mtp`, so the Speculative card adds exactly 9 stops (one
  `field-default` per boxed field of an ENABLED group, one control stop per control whose key is in the
  read: a boxed field whose key is ABSENT is ticked, so its box is tabbable and its input is not). Seven
  tab-path reds by the same 9 is the proof; a delta that differs per fixture means the rule is not read
  from the fixture.
  Older tests that pin the newly editable card READ-ONLY stay green in the red step and red after the
  coder, so list them: grep `src/*.test.tsx` for the card's key prefixes. For 2h.9: `PresetCards5`'s
  2e.85 `test.each` of 16 "every control is natively disabled" cases and its 2e.91/113 local list of the
  16 About toggles; `PresetFullValue`'s six tests that use `spec-draft-type-k` as their example of a
  read-only select with a full-value line; `PresetFlags` 944, `PresetSave1` 635-686, `PresetSave7`
  256/296. Retargeted in the green step, never in the red step.
  A control a later slice inserts between the back link and the Engine field (2h.10's jump `<nav>`) is
  retargeted ONCE, in the shared helper: `pageStartTabStops()` is the stops every whole-page tab test
  expects before the Engine field, so the five cards files and PresetContent follow without each being
  edited. Once a jump button carries a card title that title is no longer unique, so "no card `h3` is a tab
  stop" compares tabbable ELEMENTS (`expect(tabbableElements(main)).not.toContain(h3)`, the h3 from a
  `queryAllByRole('heading')`), never NAMES: a name list holds the ten titles twice (h3 and button) and a
  name comparison is true whatever the code does. Per-row 422 alert shapes belong in the fixture table as an
  expected-alert column (`''` = the plain `save-refused`, else `save-refused-at` + check text + field label +
  card title), so both shapes stay in one TABLE of one file. A string the brief REMOVES is pinned by its
  English literal (`expect(within(main).queryByText('Part of these settings ...')).toBeNull()`): `en()`
  throws for a key the test EN table no longer carries, so the red reads as a helper typo. Pin it ABSENT plus
  what replaces it where it sits, and scope the query `within(theCard)`: two cards carry the same paragraph
  (`getMultipleElementsFoundError`), which is a fixture shape, not a code defect.
- Typing into a control of a card that is not editable yet is a SILENT no-op: `fireEvent.change` on a
  `disabled` input leaves the value, the form stays clean and the save bar inert, so a refusal test reds
  at `findByText(en('save-invalid'))` after ~1 s and reads like a missing check. One debug print of
  `.disabled` / `.value` / the box's `checked` tells it apart in a single run (the box is `<key>-box`,
  the input `<key>-control`); report the red as "the field is not editable", not as "the check is
  missing".
- What a text field renders depends on `editable`: read-only is a `<label>` + input (`div.field`, NO
  `role="group"`), an editable field with a default box is a `<fieldset>` (`role="group"` named by the
  label). So a "no group named by the label, the textbox carries the name" pin is GREEN on a read-only
  card and RED on an editable one: pin it in the file whose field is already editable (Chat kwargs) to
  carry the red, and repeat it as a guard on the new card's field, reporting which is which.
- `npx vitest run` prints one `FAIL` line per failing test, so `grep -E "^ FAIL" | sed 's/ >.*//' |
  sort | uniq -c` counts per file and finds an extra flake in a full-suite run (one run showed 27 failed
  against the stable 26, all listed names identical). Run the slice's two files first (seconds), the full
  suite once for the counts: the full run is minutes.
- Run `npx prettier --write <touched files>` BEFORE the final test run: prettier rewrites long
  `test('<criterion>: ...')` titles into several lines, and a run before that reformats looks like a
  changed test set. Prettier, ESLint and `tsc -b` stay silent on a pass, so quote the command line, not
  an output line.
- To give every red its own reason in one file of 30, run
  `npx vitest run <file> --reporter=json --outputFile=/tmp/opencode/x.json` and read
  `testResults[0].assertionResults[].failureMessages[0]` with `node -e`: it prints title and first
  message per test. `--reporter=basic` does not exist in this vitest (it dies at startup
  "Failed to load custom Reporter from basic"), and the default reporter's `FAIL` lines are wrapped
  with spacing, so `grep "FAIL src"` finds nothing.
- A `fe/src/PresetSaveN.test.tsx` is self-contained: it imports only `./test/bff.ts` and
  `./strings.ts` and repeats the helpers of the earlier files ("a test file imports no other test
  file"). A context summary recites helpers from `fe/src/test/presetCards.ts` (`specGroupEnabled`,
  `select`, `EDITABLE_FIELDS`) that the file does not have: grep the file's own `function` lines, and
  read the group table (`GROUPS: { name, fields }`) for the keys, since the card's own keys
  (`spec-type`, `spec-draft-type-k`) are not all in it.

## Windows service glue (the manager, cfg(windows) entry)

- `crates/manager/src/manager/windows.rs` is the OS-neutral module (no `cfg(windows)` gate): its tests,
  the `#[cfg(unix)]` ones included, run in `cargo test -p pai-manager` on Linux, so the rerun covers the
  slice's Windows decisions. The Win32 glue (`windows/entry.rs`) is `cfg(windows)` and is compiled by
  nobody on Linux: pin its wiring with source lints over the text, comments dropped (a comment naming a
  needle never greens it), one `entry_fn(name)` extractor (signature to the `\n}\n` at column 0).
- Glue shared by the two services (the control handler, the status report) must take the service's rule
  as a parameter: pin the signature shape (`fn(u32)` + `-> bool` before the body's `{`), require the
  parameter be READ (`raw_service_control_type()`), and forbid each caller's rule NAME in the body
  (`ends_instance(` / `ends_manager(`), so one rule fixed inside greens red. Pin each caller's wiring by
  reading the arguments of `register(` inside ITS function text, both ways (contains its rule, not the
  other's). A mask name that is a SUFFIX of the other's (`controls_accepted` of
  `manager_controls_accepted`) needs a count that subtracts the prefixed variant; forbid the shared fn's
  mask parameter being NAMED like one caller's rule, and assert the parameter count (`params.split(',').count() > N`)
  so a missing mask parameter reds.
- Verify "every test of the slice ran" from the run's own output: write the FULL output to a file
  (`cargo test -p pai-manager > file 2>&1`, never piped through `grep`, the SIGPIPE trap), then one
  `grep -c "manager::windows::tests::<name> "` per name printed with `echo` in a loop; each count must
  be 1. A second run of the same command (`Finished in 0.0xs`) is a replay of the test binary, fine for
  listing names: prove the FIRST run compiled the coder's files by checking the fingerprint dirs
  (`.fingerprint/pai-manager-*`) predate his mtimes, and that the run's `Running unittests ...` binary
  hash equals a new fingerprint dir.

- Web UI tests (`fe/`, relayed by the web UI session, 2026-11-16): a big fake-clock jump stalls a
  poll that is rescheduled in a React commit, so advance in 1 s steps; in a sampled loop the a11y
  role query is the cost, not the poll, so take the node once and read `textContent`; query a group
  by its `aria-labelledby` target, and scope queries to the group when labels repeat.
- Web UI: a test that mounts the page more than about four times belongs in `it.each`, one mount per
  row: two such loops passed Vitest's 5 s limit in full runs (fe session, 2h.4). Never leave a
  half-converted test file: back it up before the first edit, not after.
- Never `git checkout --` or `git restore` a file another session may have left dirty (a playbook, a
  shared test file): on 2026-11-17 a revert dropped 27 lines two sessions had added that day. Undo your
  own edit by hand.
- A card made editable invalidates read-only pins outside the slice's file: grep the suite for the
  field ids first (web UI session, 2h.5). A mutant sweep varies the send predicate per reading (flag,
  truthy, text, select), not only the card list.

## Renaming a file across crates (a rename sub-slice)

- Flip ONLY what names the file on disk: the constants the tests hand the fake, the call-log literals
  (`"read models.ini"`, `"write-if alice models.ini"`), the assert messages, the doc comments. NOT the
  file's own CONTENT: the `DISABLED_FILE_HEADER` prose names the sibling file inside the INI text
  ("Same format as presets.ini, llama-server only reads presets.ini, back to presets.ini"), and
  `the_disabled_header_is_the_configurators_one` pins that text equal to `crates/app/src/presets.rs`,
  which the prose/wire sub-slice renames later. Sed with a per-line skip:
  `sed -i -e '/Same format as/{b}' -e '/nothing here is offered/{b}' -e '/back to presets\.ini/{b}' -e 's/presets\.ini/models.ini/g' <file>`.
- A source lint whose name list AND its scan cases are flipped together stays GREEN by construction
  (`control_lint`'s `CONFIG_NAMES` + cases): the scan is generic over the list. Report it as the guard
  that the seam rule now covers the new names, not as a red. Its `paths::` function list
  (`CONFIG_PATHS`: `presets_ini`, `presets_disabled_ini`) does NOT rename (only the file names do).
- A rename half done shows up as a pre-existing test red you did not cause, and it names the seam that
  kept the old name: `crates/app/src/server_control.rs` `ConfigFile::name()` still answers `"presets.ini"`
  while the fake manager's gate (flipped) manages `models.ini`, so the GUI e2e dies at its FIRST phase
  (save_flow.rs:113) with `client-bad-file-name` "The manager does not manage a file of that name".
  Prove it predates your edit: `cp` the pristine file from your own backup, run, `cp` the edited one back,
  `grep -c` to confirm the restore. Then name the production line the coder must flip, and say which
  asserts of yours the red blocks (the e2e is ONE `#[test]`, so later phases report nothing).
- The GUI's file names also live in production as bare literals (`models_tab.rs` feeds
  `delete_target_file`), and a `state.slint` default property: flipping the test's expected string is the
  red pin, the literals are the coder's.
- `cargo fmt -p <crate> --check` after a rename: shorter names collapse an `assert_eq!` to one line
  (rustfmt), so format the touched file, do not hand-wrap.
- The red shape of a file-name flip on the bff's fake: the route asks for the old name, the fake holds the
  new one, so it sees no file → 404 `no-preset-named` on every content test, `[]` on the list, and the
  call-list asserts read `left: ["read presets.ini"] right: ["read models.ini"]` (7 of them: the exact
  call lists are the real pin of the file names).
- A name that is an INPUT of a keyed digest moves EVERY constant keyed on it, so enumerate them
  (`grep -n '"[0-9a-f]\{16\}"' crates/bff/tests/*.rs`), never work from the coder's list (they named 4 of 10,
  and the near-miss constants `REV_WHOLE_FILE` / `REV_NEIGHBOUR` are keyed too). Recompute in a scratch
  `rustc` program that `include!`s the test file's own fixtures and `siphash_2_4` / `hashed_section` /
  `revision_of` (`sed -n '<lines>p'` into `fixtures.rs` + `helper.rs`): print the OLD-name value of every
  fixture and match it against the constant being replaced (all 10 matched, so the scratch model of the input
  is the route's), then print the NEW-name ones. Cross-check the new values against the crate the production
  code links: `rustc --edition 2018 --crate-type lib ~/.cargo/registry/src/*/siphasher-1.0.4/src/lib.rs -o
  libsiphasher.rlib`, then `--extern siphasher=libsiphasher.rlib` (its `write`/`finish` are trait methods, so
  the probe needs `use std::hash::Hasher`). Validate BOTH against the published vectors first.
- A migration test that reads the moved file through `paths::presets_ini()` breaks when that helper's
  RETURNED name is renamed, not when the migration is: the 2.4 tree's names are data, so pin them by the
  literal path (`paths::config_dir().join("presets.ini")`) and add the guard of the decision
  (`!paths::config_dir().join("models.ini").exists()`: the tree move renames nothing).
- The hand-over (`import_user_config`) has TWO names per file (`migration.rs` `USER_FILES`): it READS the
  per-user dir under the OLD name (`presets.ini`, `presets.ini.disabled`) and WRITES the manager's new one
  (`models.ini`, `models.ini.disabled`), pinned by `import_reads_nothing_under_the_managers_own_names`. So in an
  e2e phase that seeds the per-user dir, the seed and the read-back of the user's file keep `presets.ini`,
  while the fake's `fail_write_of` key, the `fake::writes` list and the manager's `files.get(..)` are
  `models.ini`. A seed under `models.ini` is read by NOTHING: the phase that expects a refusal reports
  `panicked at ...: refused: succeeded` (the import finds no file and answers success), which reads like a
  broken fake, not a wrong name. Grep the seeds (`grep -rn 'join("models.ini' crates --include=*.rs`) to tell
  the manager/engine fixtures (`sys.join`, a temp dir) from a per-user seed.
- The WORD and VERB rename of the same family (displayed texts, the CLI's command name) is pinned at three
  levels, all in the crates, no fe/: the binary test asserts the retired verb is GONE, not an alias (`pai
  preset` → exit 2 + stderr `unrecognized subcommand 'preset'`), the help test pins the command list as an
  exact array (`["server", "model-profile", "control", "bench", "config", "help"]`), and a split-lint scans
  BOTH strings files for the retired invocation (`line.contains("pai preset")`). The completeness check of
  the text rename is a grep of the two strings files for the old word: the residue that stays is DATA, so
  report it as such, not as a miss (`hf-preset-repo` names the file `preset.ini`, `spec-temp` names the
  token a parser accepts, "a number or `preset`").

## Mutation runs (fe/, proving a test has teeth)

- The scratch copy: `rsync -a --exclude node_modules --exclude dist --exclude ux fe/ /tmp/<x>/`, then
  `ln -s <repo>/fe/node_modules /tmp/<x>/node_modules`. Drive it from a python script that restores
  the pristine sources (kept OUTSIDE the copy) with `cp` after each mutant, so no mutant survives the
  run and the repo is never touched. A scripted sweep takes seconds per mutant: 4 test files 7 s, the
  full `fe` suite 19 s.
- Names of the failures come from `npx vitest run --reporter=json --outputFile=/tmp/x.json` in the
  copy: `testResults[].name` is the file and `assertionResults[].fullName` the test, which is what to
  report (the console output of 19 runs is unreadable).
- Re-read the source for every patch anchor just before the run: a context summary recites functions
  the file does not have (a summary named `flagsOf`, `isEditable`, `hasText`, `buildSaveRows`; today
  the same rules live in the `CARDS` table's `editable` flag, `ruleLocked`, `shown`, `selectState`,
  `selectSend`, `editableWritten`). A driver that reports "anchor not unique" caught all four. The same holds for a quoted acceptance criterion: re-read it in
   `fe/briefs/<slice>.md` before retargeting, and grep the bare number (a criterion is a numbered list item
   ` 196.`, so `/^- \*\*196\*\*/` matches nothing and reads as "the brief has no 196").
- A mutant is a survivor only after a FULL-suite run: of 19, three were killed by files outside the
  slice's own (the 176 lock rule is pinned in `PresetCards4.test.tsx`, the sends in `PresetSave12`).
- The surviving mutants of 2h.7 were the readings no control reaches: a tri-state select's option
  `default` short-circuits `selectState`, so the flag reading of the word `default` shows only on an
  off / on row (`no-host`), where 122 makes it ON; and a read-only row is invisible to the save's
  check (`checkRows` filters `editable === true`) so a mutant that makes it block needs two edits:
  the check list and the box state it validates against.
- A patch that only changes a value nothing reads is no test at all: making a checkbox row
  `editable: true` adds no save row while the page's `boxed` map holds no entry for it. Design the
  mutant where a real decision is reached, and say in the report which ones cannot be reached.

## Wire renames (routes, error keys, strings keys)

- Before flipping a key, check it exists today (`grep -c '^<key> = ' crates/core/strings/*.toml`, and
  `grep -rn '"<key>"' crates`): a design's rename list names keys of the whole target state, and four of
  the ten do not exist yet (`preset-over-memory`, `preset-model-missing`, `client-no-presets`,
  `preset-engine-mismatch`). Renaming a key that is not there invents a red no coder can pass.
- A wire rename touches assertions that live in PRODUCTION files (core's `checks.rs` `mod tests`, the
  manager's `dbus.rs` / `pipe.rs` / `service.rs`, app's `presets.rs`): flip with a LINE-RANGE sed from the
  file's first test marker (`sed -i '802,$ {s/"old"/"new"/g}'`, the marker line from
  `grep -n 'cfg(test)'`), never the whole file, and never a whole file where `#[cfg(test)]` sits on
  single items (app's `presets.rs`: ten of them from line 66, so address the exact lines). Prove the split
  after: the old spelling must remain ONLY on production lines.
- A strings-key rename is pinned on both sides in `crates/core/src/checks.rs` (it reads both files with
  `include_str!`): the new key resolves in `cli.toml` AND `en_US.toml` with today's text byte for byte
  (the texts were already reworded, so equality is the pin), and `!table.contains_key("<old>")` for each
  old name. `KEYS.contains` in the existing `every_refusal_key_is_known_and_in_both_strings_files` is what
  makes the core list part of the rename.
- The bff's own keys (`model-profiles-malformed`, `model-profile-changed-elsewhere`) are in NO strings
  file: pin them in `crates/bff/tests` only. The four the manager and core emit are in both files.
- A route move: pin the OLD path as the router's bare 404 in one new test (status + `content_type == ""`
  + body `""` + zero connects + the session survives): it is red today (the old route answers 200) and
  greens when the route is gone. Do not rename the GUI's display keys (`deleted-preset`,
  `preset-id-charset` in `GUI_ONLY_KEYS`, `by_label(&app, "preset-ctx-size")`): the wire slice renames
  only the keys the wire emits, and those keys drag every `.slint` and the GUI tabs in.
- In `data_root_e2e` crates (core, manager, app) a key rename reds at the FIRST phase that asserts it, so
  the later phases' pins of the same key (the manager's `pipe_client` / `dbus_client` phase asserts) cannot
  go red on their own today: report them as blocked, and name the phases that DO prove the mechanism
  (core's `a_refusal_round_trips_through_its_wire_form` for the wire encode, the manager's
  `every_error_has_its_name_and_reads_back` for the D-Bus name).
