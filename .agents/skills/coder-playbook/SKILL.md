---
name: coder-playbook
description: What the coder has learned about implementing and running code safely. Use when changing production code.
---

Lessons only: the rules are in `AGENTS.md`.

- `pkill -f PATTERN` also matches the shell running it, and killed that shell. Use the task's own stop
  control, or a pattern that cannot match itself.
- When a test fails, check whether the test or the code is wrong before changing either.
- A screenshot that comes out blank proves nothing about a GUI. Check that it really shows the window.
- The framework is five crates under `crates/`: `core` (`pai-core`, no GUI), `manager` (`pai-manager`,
  the `pai-manager` bin, core only), `app` (`pai-app`, the configurator's logic without Slint),
  `cli` (`pai-cli`, the `pai` bin, `cli.rs`) and `config` (`pai-config`, GUI only, its arguments
  hand-parsed in `launch.rs`). config's and cli's `lib.rs` bring pai-app's modules in with
  `use pai_app::{...}`, so `crate::presets::x` still resolves in gui and cli; pai-app's modules are
  `pub mod`, so an item gui or cli needs is `pub`. The `cfg(test)` items pai-config's tests use
  (`server_control::fake`, the test branch of `backend()`, `hf_fit::tests`' GGUF writer) are
  `cfg(any(test, feature = "test-support"))`; a normal `cargo build -p pai-config` leaves the feature
  off (check: `touch crates/app/src/lib.rs && cargo build -p pai-config -v`, no `feature="test-support"`
  on the `pai_app` rustc line). The config and manager
  lib roots `use pai_core::{paths, ...}` so `crate::paths::x` still resolves; a module split between
  core and config (`server_cfg`, `slot_state`, `runstate` over `pai_core::launch`) re-exports the core
  half with `pub use`. A `#[cfg(test)]` item of core is invisible to the other crates' tests: gate it
  `#[cfg(any(test, feature = "test-support"))]`, which their dev-dependency on core turns on (the
  strings table's `cli.toml` merge is one). Most modules are private, so `pub` does not silence
  dead_code there, and a function only tests call fails `clippy --all-targets -D warnings` until a
  caller lands; a `pub` item of a `pub mod` is the library's API and never dead.
- The three framework crates take one version, `[workspace.package] version` in the root `Cargo.toml`
  (`version.workspace = true`); xtask's `version::framework()` reads it from there.
- `binding_lint` (in config) reads the sources of all three crates (`src_dirs`) by file name, and the
  strings files in `crates/core/strings/`: only the binary roots may say `cli::` (`lib.rs` may not: the
  GUI can reach it), and a key printed outside `cli.rs` and `CLI_ONLY_FNS` must be in both `cli.toml` and
  `en_US.toml`. Rules match by file name, so a moved file keeps its name or the lists change with it.
- A debug binary in `cargo-target/debug` finds no llama-server (`paths::llama_bin` layouts). To show a
  launcher's refusals, hardlink it into a scratch dir beside an empty, non-executable `llama-server`.
- Code comments never cite PLAN.md step numbers: they change on renumbering. Name the feature or caller instead.
- zbus (5.x, `async-io`) ticks every connection's tasks, method handlers included, on that
  connection's own executor thread. A blocking call made from a handler on the same connection
  waits for a reply that thread can never read: give outgoing calls (systemd, polkit) a connection
  of their own (`manager/dbus.rs`). A sync `fn` handler also blocks every other call on that connection
  while it runs (polkit waiting for a password): the methods are `async fn` that run the core call
  through `blocking::unblock`, the core shared as `Arc<Manager>` with no outer mutex.
- In `manager.rs` only `main` and `run` are CLI-only for `binding_lint`: a new helper fn that prints
  a `cli.toml` key needs the key in `en_US.toml` too, so keep such printing inside `run`.
- Each crate's tests that redirect the data tree (`paths::redirect_for_tests`, which sets
  `PRIVATE_AI_DATA_ROOT`) run as phases of ONE `#[test]` in its `src/tests/mod.rs` (`data_root_e2e`
  in core and manager, the UI test in config).
- The names lints (`config/src/tests/names_lint.rs`, `xtask/src/names_lint.rs`) scan code lines for
  the old project names; whole-line comments are skipped, so a test that must name an old name spells
  it in halves with `concat!`. xtask's lint also scans comments for the old package name: only
  `components::OLD_BASE_NAME` spells it (the `.deb`/`.rpm` takeover relations are built from it), and
  in `scripts/` and `installer/` the old registry keys appear only inside `<RegistrySearch>` lines.
- Several test-name filters go after `--`: `cargo test -p pai-config -- names_lint cli::`.
- Moving a crate changes the depth of its repo-root paths: `build.rs` (`../../resources`,
  `../../build`, `../../patches`), `include_str!` of `benchmarks/` (`bench/db.rs`, `bench/spec.rs`),
  `CARGO_MANIFEST_DIR` joins (`bench/db.rs` test, `kv_cache.rs` test, `paths::bench_share_dir`'s
  `.parent()` chain) and xtask's `repo_root().join(...)` (`configurator.rs`, `licenses.rs`,
  `strings_share.rs`, `version.rs`, `linux/package.rs`). A sed on `^name = "..."` in its Cargo.toml also
  hits the `[[bin]]` entry of the same name.
- `main.rs` (config) is a print path shared by the GUI and the CLI: a key it prints (directly or
  through a shared fn such as `Migration::errors`) goes in both `cli.toml` and `en_US.toml`, and
  only once `strings::set_reader` has run. Windows-only code (`startup::win`) is not compiled by the
  agent commands on Linux: say so in the report.
- Windows-only code is type-checked on Linux against the GNU target (mingw-w64 and the rustup target
  installed): `cargo check -p pai-core -p pai-manager -p xtask --all-targets --target
  x86_64-pc-windows-gnu`, then clippy with the same target. pai-config does not build there:
  skia-bindings (slint `renderer-skia`) has no prebuilt binaries for `windows-gnu` (404) and its full
  build asserts an MSVC install (`win_vc`); its build.rs also wants `build/windows/llama.cpp` unless
  `PRIVATE_AI_LLAMA_CPP_DIR` names a clone. Check windows-sys signatures in `~/.cargo/registry`.
- `binding_lint` counts a key's values only at a literal `strings::fill("key", &[..])`: a key marked
  with `strings::key` in a table (outside a fn with such a fill) must take no `{N}`. A key carried in a
  value and shown later (`checks::Refusal::text`) gets one literal `fill` arm per key. A key that moves
  into a source the CLI reaches leaves `GUI_ONLY_KEYS` and goes in `cli.toml`.
- Never `git stash` to compare against HEAD: other agents edit the same working tree at the same time.
  Use `git worktree add` or `git show HEAD:<path>` instead.
- A helper generic over a zbus call's body needs serde's traits: `zbus::export::serde` re-exports
  serde, so no `serde` dependency is added for it (`pai-core` `client/dbus.rs`).
- `binding_lint` reads `cli.rs` only up to the line `mod tests {`, so a `#[cfg(test)]` e2e phase with
  literal test data in `cli.rs` must come after that module; clippy's `items_after_test_module` then
  needs `#[allow(clippy::items_after_test_module)]` on `mod tests` (the attribute on its own line keeps
  the `mod tests {` line the lint looks for).
- Every start, stop and run probe of llama-server goes through `server_control::backend()`
  (`control_lint`); only `runstate.rs` and `server_control.rs` may call `runstate::{start,stop,is_running}`.
  Under the test redirect the Linux backend never dials D-Bus (`system_connect` is `Unavailable`).
- server.ini, presets.ini and presets.ini.disabled are read and written only through
  `server_control::backend()` (`read_config`, `write_config`, `config_path`; `read_sections`,
  `read_section` for parsed reads), enforced by `control_lint`. An edit is read-modify-write over
  `pai_core::ini::text::*` (string transforms; the path writers wrap them). `presets.rs` runs its file
  logic over the `PresetFiles` trait: `Backend` in the program, the `#[cfg(test)]` `PathFiles` behind the
  old path-taking helpers (`listed_from`, `move_section`, `home_in`, `rename_in`) its unit tests call.
- A Slint `Text` answers `ElementHandle::find_by_accessible_label` with its text; an element under
  `if` is absent from the search, one with `visible: false` is not. A method only tests call on a
  private type is gated `#[cfg(test)]` (dead_code otherwise).
- A tester's e2e phase placed after `mod tests` trips clippy's `items_after_test_module`: report it
  (the tester moves the phase above `mod tests`); `cli.rs` keeps an allow only for `binding_lint`.
- `clippy --all-targets` also lints the tester's test code (`redundant_closure` on `|| f()` in an e2e
  phase): run plain `cargo clippy -p <crate> -- -D warnings` to show the production code is clean, and
  report the test-code hits to the tester instead of editing them.
- A GUI start-up hook with a text-check test (`the_gui_offers_the_import_at_start` slices `gui::run`
  up to its closing `\n}\n`) needs the named call literally inside `run`, not in a helper it calls.
- xtask compiles only its own OS's module (`linux/`, `windows/` or `macos/`): a `common/` helper that
  only the other OSes' phases call is dead code on Linux and fails clippy. Give the shared helper one
  signature every OS calls (`bench_share::stage(dest)`), not a Linux variant beside the old one.
- `xtask/cmake/<os>.toml` is picked up by file name alone (`cmake_options::load`); a new OS file needs
  no code change, only the header and a reason per entry in the style of `macos.toml`.
- The tests of `linux/system_files.rs` scan the generated maintainer scripts for `rm ` and the bus and
  polkit policies for `own=`, `<policy`, `<action id=`: a comment written into them must not hold
  those substrings (`form `, `perform `, `term ` contain `rm `). An rpm scriptlet body has no line
  starting with `%` and no `%` in its comments (rpm expands them).
- The manager's error (name, message) pairs and `state_name` live in `manager/service.rs`, OS-neutral;
  `dbus::Error`'s `From<ManagerError>` takes its message from there, while `dbus::error_reply` stays
  derived from the zbus enum so `pipe`'s equality test still checks the zbus names. The pipe frames
  are read and written by `pai_core::client::pipe::{read_frame, write_frame}` on both sides.
- Windows manager glue lives in `manager/windows/` (`scm.rs`, `pipe_server.rs`, `entry.rs`, all
  `cfg(windows)`), the decisions in `manager/windows.rs`. `ImpersonateNamedPipeClient` takes the
  security context "of the last message read from the pipe": read a byte before it (the server
  hands the byte back to `serve_stream` through a prefixed reader). `windows-service` 0.8 is on
  windows-sys 0.61 too; the SCM calls themselves are raw windows-sys so `image_path` is what
  `CreateServiceW` gets. A `windows-sys` fn whose signature names another module's type needs that
  feature too (`ConnectNamedPipe` needs `Win32_System_IO`).
- config's `names_lint` refuses the bare string `private-ai-manager` in code (only
  `private-ai-manager.service` passes) and scans core and manager too: run
  `cargo test -p pai-config -- names_lint binding_lint` after touching manager or core strings.
- A `strings::key` in a source the CLI reaches (`server_control.rs` is one) puts the key in
  `binding_lint`'s cli.toml check even when only the GUI shows it: it needs `cli.toml` or an entry in
  the test's `GUI_ONLY_KEYS`. Settle which with the tester before writing the key there.
- A strings key used inside `server_control.rs` or `bench/` (sources a CLI command reaches, e.g.
  `bench run` through `Backend::log_text`) needs a `cli.toml` entry even if the CLI only discards
  the text: binding_lint is static. `log-journal-denied`/`-failed` are in both files for that reason.
- A periodic GUI probe that would spawn a process to read a log (journalctl under the manager)
  reads only on a flip: `gui.rs` keeps the last sample in a static `AtomicBool` and swaps it.
- WiX (v4+ schema, the pinned toolset) generates a component's GUID only when its key path is one file
  or one registry value: a CreateFolder-only component (a folder as key path, e.g. the
  `%ProgramData%\private-ai` folders with their PermissionEx) needs an explicit `Guid`, fixed forever.
  The `.wxs` tests in `xtask/src/common/msi.rs` run on Linux; nothing there builds the MSI.
- A parameter used only inside a `#[cfg(unix)]` block is an unused variable on the Windows target:
  only the GNU-target clippy shows it. Name it in the `cfg(not(unix))` branch (`let _ = (file, group);`).
- `pai_core::ini::atomic_write` sets no mode or group: the manager writes its served files through
  `service::write_served_file` (0640 + group `private-ai` for `server@<id>.ini`, 0644 for the presets).
- clippy's `items_after_test_module` looks only at the FIRST `#[cfg(test)]` module of a file and stays
  quiet when a `mod` follows it: turning a leading `#[cfg(test)] mod fake` into
  `cfg(any(test, feature = ...))` made it fire on the `mod tests` with e2e phases after it.
- A module going from private to `pub mod` (the move into pai-app) exposes its inherent `from_str`s to
  `clippy::should_implement_trait`; they carry an `allow` rather than a rewrite.
- `cargo check -p pai-app --target x86_64-pc-windows-gnu` needs `PRIVATE_AI_LLAMA_CPP_DIR` set to the
  Linux clone (`build/linux/llama.cpp`): the build script reads the Windows clone path by default.
- `binding_lint`'s `cli_print_paths_hold_no_language` scans every string literal inside a print macro of
  a `main.rs` (either crate's), `env!("CARGO_PKG_VERSION")` included: print a fixed line through a
  `const X: &str = concat!(env!(..), ..)` outside the macro (`println!("{X}")`).
- A tester phase in another crate's tests that calls a module you move breaks that crate's whole lib
  test build. To verify the rest without editing tests, rsync the tree (no `build/`, `.git`, `dist/`)
  to `/tmp`, drop the phase there and run with `CARGO_TARGET_DIR` set to the repo's
  `build/linux/cargo-target` (the dependencies are reused); report the phase.
- The Windows GNU check of xtask prints two `unused dependency` manifest warnings (`serde_json`,
  `sha2`: used only by the other OSes' modules). They predate the work and do not fail clippy.
- A Linux/macOS package's relations come from `linux/package.rs`: `old_package` (None for a component
  2.4 never had, so no Conflicts/Replaces/Provides/Obsoletes), `exact_requires` (which framework
  packages it pins at its own version) and `depends_line` (the `.deb` Depends, pure, tested).
