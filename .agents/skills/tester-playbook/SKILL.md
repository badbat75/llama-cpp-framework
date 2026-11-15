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
- One fixture per test: an id from one temp fixture will not match another. Use real names and values
  captured from the real system; a name-prefix match is tested with every real name sharing the prefix
  (`llama.cpp-v` starts `llama.cpp-vulkan-v...`), fixtures built from the full real file set.
- A fake holds whatever you write; the real system can answer differently. A green on a fake is no
  proof about the real thing.
- Prefer `todo!("<fn>")` stubs to a compile error, which hides every other test in the binary, lints
  included. A stub reached only from tests fails clippy with dead code until wired in: say so, add no
  `allow`. A new parameter on a core method: a stub that takes it and keeps the old body gives assertion
  reds in every phase, where a `todo!` stops the e2e at its first call.
- To pin a field a variant lacks without breaking the build, match `Variant(p, ..)` / `Variant { .. }`
  and check the new field through `{:?}`; tell the coder the intended shape. Naming a stub after a local
  of the caller makes the call `E0618` (the local shadows the fn): rename one and say which.
- Before handing over a large set of reds, prove they can go green with a throwaway candidate: back up
  every file it touches (stub, strings, wired files) to the scratchpad, write the candidate, run, copy
  back, `touch` the restored sources (a `tar` restore keeps old mtimes and cargo runs the stale
  candidate build), check `git status`. If the permission classifier refuses editing production files,
  leave that layer unproven, order the asserts so the earlier ones run first, and say so. A data file
  pinned by a test is the coder's: check by reading that all assertions can hold at once.
- Never `git stash` or `git checkout --` to compare: other sessions edit the same tree. Run one test on
  its own, or probe in a scratch copy and `cmp` it back. Plant lint violations in a scratch copy, never
  the real tree.
- Several filters go in one run: `cargo test -p <crate> -- a b c`; a loop of runs costs minutes each.
  `cargo test --test a --test b` stops at the first failing binary: add `--no-fail-fast`.
- Format one file with `rustfmt --edition 2021 <file>`, then `cargo fmt -p <crate> --check`.
- Clippy traps in test code: a doc line starting `1)` (`doc_lazy_continuation`); `&Path::new(..)`
  (`needless_borrow`); `option_env!("X").expect` (`option_env_unwrap`: use `env!` and accept the
  compile error as the red).
- Comments never cite PLAN.md step or slice numbers; name the feature.
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

## Text pins and source lints

- A text pin on a file the coder will rewrite: prove it greens on a candidate edit in a temp copy, so
  the red is not a broken helper. A "no old text left" pin must be narrow (pin the code shape, e.g.
  `"+llama.cpp.{"`): doc comments legitimately describe the old format.
- A pin on a file that also holds the test reads only the code before `#[cfg(test)]\nmod tests` (in
  xtask `main.rs`: before `#[cfg(test)]\nmod cli_tests {`, since `mod names_lint;` sits under an earlier
  `#[cfg(test)]`). A needle the test counts or forbids is split with `concat!` and kept out of the
  comment and assertion message. A top-level fn's body is found by `"\npub fn <name>("` (test fns are
  indented; presets.rs has test items before production fns).
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
  only in `ENGINE_DIR_SOURCE`. Run the whole crate's tests, not just your filter.
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
- Functions that take the directory as a parameter, manager calls refused at authorization or asking
  only `Status`, and pure helpers are plain `#[test]`s.
- A unit test whose expectation is a `paths::` default races the e2e redirect on another thread: assert
  the shape (`ends_with("state")`) and pin the exact value in an e2e phase. Reproduce such a race by
  looping `cargo test -p <crate> -- <unit> data_root_e2e`.
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
  `DiskClient`, and pai-bff's `tests/instances.rs` `FakeManager` (implement the test ones yourself).
- A server or client over `impl Read + Write`: a `Cursor` of peer frames for reads, an
  `Arc<Mutex<Vec<u8>>>` for writes, an optional failing `ErrorKind`; compare JSON as `serde_json::Value`.
  A real duplex: `UnixStream::pair()` with `set_read_timeout` (`manager/pipe.rs`,
  `crates/manager/src/tests/pipe_client.rs`). A frame cap needs a test in both directions on each side.
- Windows decisions are OS-neutral pure functions in `manager/windows.rs` and `pai_core::client::pipe`;
  only Win32 glue is `cfg(windows)`. Numeric Win32 values come from the local windows-sys source
  (`~/.cargo/registry/src/*/windows-sys-*/src`), meanings from learn.microsoft.com, a crate's behaviour
  from its `.crate` on static.crates.io. A registry test's guard restores every value in `Drop` (no
  panic there). The SCM reports states on its own: never-started reads `SERVICE_STOPPED` with exit code
  1077. A service SID (`NT SERVICE\<name>`) is computable per [MS-LSAT] 3.1.1.1.2 (SHA-1 of the
  upper-case UTF-16LE name; `service_sid`).

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
