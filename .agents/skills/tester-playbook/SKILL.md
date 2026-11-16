---
name: tester-playbook
description: What the tester has learned about writing reliable tests. Use when writing or debugging tests.
---

Lessons only: the rules are in `AGENTS.md`.

## General

- Never wait without a deadline (`recv_timeout`, `try_wait` loop, `method_timeout`): a test must fail,
  not hang. `recv_timeout` returns `Disconnected` at once when the thread panicked (a `todo!` stub):
  report it apart from `Timeout`, or the red reads as a hang.
- Check why a test is red, or why it is already green: a test can pass for the wrong reason (the field
  did not exist yet). Recheck such tests once the code exists.
- An independent rerun of cargo commands that prints `Finished ... in 0.0xs` is cargo replaying a cached
  pass (its fingerprint does match the current sources: `cargo check -v` prints `Fresh <pkg>`). To compile
  the coder's files yourself: `cargo clean -p <pkg>` first; it covers only the host target, so force the
  cross target with `cargo clean -p <pkg> --target <triple>`. The rerun's own log proves the compile when
  it prints `Compiling <pkg>`; a follow-up `cargo check -p <pkg> -v` printing `Fresh <pkg>` confirms the
  fingerprint (cargo's own `-v` changes no rustc fingerprint). For clippy `-v` is NOT a neutral probe: the
  clippy driver records `CLIPPY_ARGS`, so any `-v` run reports `Dirty <own pkg>: the environment variable
  CLIPPY_ARGS changed` and re-checks (that run is proof too, it re-runs every target). Write the `-v` log to a file and grep
  that file: piped through `head`/`grep -c` the pipe closes early (SIGPIPE), the log truncates, and the
  `Fresh <own pkg>` line (late in the dep list) reads as missing. Bare cargo commands here land in
  `build/windows/cargo-target` (the generated `.cargo/config.toml` pins `target-dir`, on Linux too).
  A clean rerun IS fast (0.4 s for every pai-bff target on 24 cores, deps cached): to prove it checked
  them all, `cargo clippy -p <pkg> --all-targets -v` and count `--crate-name <target>` (pai-bff today: 15
  lines, 14 of them its own: lib and bin each with a test twin (4), 9 integration binaries, the build
  script and its test twin; the 15th is the dependency `pai_core`). To prove a green
  `-D warnings` has teeth without touching the tree, add a restriction lint on the command line
  (`-- -D warnings -D clippy::expect_used`): exit 101 with diagnostics names the files, on both targets.
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
  included. A `todo!` body does NOT excuse the parameters: `-D warnings` fails `unused_variables`, so
  underscore-prefix them and tell the coder to strip the prefixes. A stub reached only from tests fails
  clippy with dead code until wired in: say so, add no `allow`; a `pub` stub in a lib crate is not dead
  code, so there is no dead-code red to report. A new parameter on a core method: a stub that takes it
  and keeps the old body gives assertion reds in every phase, where a `todo!` stops the e2e at its first
  call.
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
  `cargo test --test a --test b` stops at the first failing binary: add `--no-fail-fast`.
- Format one file with `rustfmt --edition 2021 <file>`, then `cargo fmt -p <crate> --check`.
- Clippy traps in test code: a doc line starting `1)` (`doc_lazy_continuation`); `&Path::new(..)`
  (`needless_borrow`); `option_env!("X").expect` (`option_env_unwrap`: use `env!` and accept the
  compile error as the red); `count >= n + 1` (`int_plus_one`: write `count > n`).
- `assert_eq!(log_of_String, vec![], ..)` is E0283 (many `String: PartialEq<_>` impls): write
  `Vec::<String>::new()`, the pattern this repo already uses.
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
- A rule about the `Host` name touches no bff test that sends no `Host` (the `App` helper sends one only
  when an extra names it): pin the no-Host path as accepted (HTTP/1.0, `curl`), and give every refusal a
  shape where only that rule can answer (a non-text `Host` with no origin header, a non-text `Origin`
  with no `Host`), else one red hides behind the other.
- `App::sign_in` asks the fake authenticator: after it, compare `asked()` with the baseline taken right
  after the sign-in, never with `is_empty()`.

## Text pins and source lints

- A text pin on a file the coder will rewrite: prove it greens on a candidate edit in a temp copy, so
  the red is not a broken helper. A "no old text left" pin must be narrow (pin the code shape, e.g.
  `"+llama.cpp.{"`): doc comments legitimately describe the old format.
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
- `binding_lint` (`rust_fns`) skips only what follows a `mod tests {` line: an e2e phase (a top-level
  `#[cfg(test)] pub(crate) fn ..._e2e`) counts as production, so calling a `CLI_ONLY_FNS` fn there
  (`sweep::run`) fails `cli_only_code_is_not_called_from_elsewhere`. Call it from a `#[test]` in the
  file's `mod tests` under `fake::scoped`, when it reaches no `paths::` (a plan it refuses, `reps: 0`,
  stops it before anything real).
- `strings::t`/`fill` return the KEY when missing: assert the message's content and
  `!text.starts_with("<key>")`, never only equality with `strings::fill("<key>", ..)`.
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
  included with `#[path = "..."] mod` (`tests/lock_walk.rs`).
- A clap doc comment is pinned on what the user reads: `Cli::command().find_subcommand(..)`,
  `.get_arguments().find(..)`, `.get_help()` (derive joins doc lines with spaces).
- `benchmarks/schema.sql` is `include_str!`ed by pai-app (`bench/db.rs`) and
  `the_repo_database_matches_the_schema` compares only columns: a comment between statements changes
  nothing SQLite records (`sqlite3 -readonly benchmarks/bench.db "select sql from sqlite_master"`), one
  inside a `CREATE` does.

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
  shows the patch is gone).
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
- A new argument on a method with many test call sites: a small Python pass that walks each call to its
  matching paren (skipping string literals and `fn ` definitions) inserting the argument, then `cargo
  fmt -p`; bump `[(..); N]` array lengths by hand.

## Manager, D-Bus, pipe

- A seam called through `&self` gets a fake with `Mutex` logs (seams must be `Sync`), read through an
  accessor; pin "nothing happened" on a refusal by empty logs plus the error kind.
- The manager's `FakeService` records every `ServiceControl` call, `status` (the probe) included: a
  batch that ends with a successful `Status` pins `vec![Call::Status(..)]` in the calls log, never an
  empty one; write the refusal tests' expectation as "nothing beside the Status probe".
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
  `SidTypeAlias` is 4, not 3. A registry test's guard restores every value in `Drop` (no panic there).
  The SCM reports states on its own: never-started reads `SERVICE_STOPPED` with exit code 1077. A service
  SID (`NT SERVICE\<name>`) is computable per [MS-LSAT] 3.1.1.1.2 (SHA-1 of the upper-case UTF-16LE name;
  `service_sid`).
- A token holds the local groups (aliases) its user was in at logon, as SIDs: a check by group name
  looks the SID up (`LookupAccountNameW`) and compares it whole; a change of membership counts at the
  next logon. Unverified (fetch failed): UAC sets only the admin groups deny-only, a custom group stays enabled.

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
