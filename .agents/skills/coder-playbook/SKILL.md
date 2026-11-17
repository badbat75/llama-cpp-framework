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
  A phase fn (`#[cfg(test)] pub(crate) fn ..._e2e`) placed after `mod tests` fails clippy's
  `items_after_test_module` under `-D warnings`: it goes above the test module.
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
  Use `git worktree add` or `git show HEAD:<path>` instead. The tree really does move under you: one
  session restored `installers-windows.yml` from HEAD while I was editing it, my two accepted edits to
  it were gone, and the tests' red list had changed with it. Re-read a file right before editing it
  (`git status --porcelain`, its mtime), and rerun the red list after each edit: the "red" the tester
  reported is only true of the tree at that moment.
- A helper generic over a zbus call's body needs serde's traits: `zbus::export::serde` re-exports
  serde, so no `serde` dependency is added for it (`pai-core` `client/dbus.rs`).
- `binding_lint` reads `cli.rs` only up to the line `mod tests {`, so a `#[cfg(test)]` e2e phase with
  literal test data in `cli.rs` must come after that module; clippy's `items_after_test_module` then
  needs `#[allow(clippy::items_after_test_module)]` on `mod tests` (the attribute on its own line keeps
  the `mod tests {` line the lint looks for).
- Every start, stop and run probe of llama-server goes through `server_control::backend()`
  (`control_lint`); only `runstate.rs` and `server_control.rs` may call `runstate::{start,stop,is_running}`.
  Under the test redirect the Linux backend never dials D-Bus (`system_connect` is `Unavailable`).
- The snapshot-gate lint (`crates/app/src/tests/gate_lint.rs`, included by `#[path]` into pai-cli's
  and pai-config's tests) pins the spellable shapes: the gate's argument must START with an
  identifier (`client_saves_snapshots(&server)` reads as no backend: the `&` leaves the head empty),
  so bind `let server = &crate::server_control::backend();` and pass `server` (a `&Backend` binding
  answers `server.stop()` too); pai-cli's test pins the literal `snapshot_before_stop(server)`, so
  the parameter name is `server`. A gate whose answer is bound to a name needs an `if` testing it
  between the gate and the call; the guard clause `if !gate(..) { return; }` is the other shape.
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
- The fe mirrors every `checks` refusal text of `crates/core/strings/` under `api-<key>` in
  `fe/src/strings/en_US.json` (and in the fake's table `fe/src/test/bff.ts`): a core text change leaves
  it stale. The fe is a separate session's files: report it, do not edit it.
- `checks.rs` `RawSection` compares byte for byte where a save keeps the bytes (`unchanged_in` and
  `known_key_line`: id, key and raw value exact), and case-insensitively only in the name rules
  (`preset-name-taken`, `preset_ids_free`). `get` returns the value cut at `;`/`#`, `keys` holds the raw
  value: `comment_markers` on `PRESET_TEXT_KEYS` is the only raw-line rule, so it is the only one with
  the exemption of a line the current preset files already hold ("Saving a preset from the web", c1);
  `known_sections` joins the two files' sections, which needs `RawSection: Clone`.
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
- `ini::text` writers share one geometry, no second scanner: `find_section_header` (span ends at `]`),
  the body from the first `\n` after it, `next_section_start(content, header_end)` as the end (scan from
  `header_end`, never from the body start: an empty body would splice into the NEXT section).
  `body_start <= body_end` always, so an empty body needs no clamp and the key lands in its section.
  The key-line matcher is `line_starts_with_key` (any case, `=` after the name, so `ctx-size-draft` is
  not `ctx-size`). `detect_eol` returns `&'static str`, so a helper picking a line's ending takes
  `eol: &'static str` (a plain `&str` fails to compile).
- A test-name filter is a substring over the whole path: `remove_key set_key` also matches
  `checks::tests::preset_keys_with_no_check_today_pass` ("preSET_KEYs"), so a 13-test red list turns
  into a 14-test green list. Count the names, not the number.
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
- `pai-bff` (`crates/bff`) follows the manager's layout: `bff::main`/`run` are its CLI_ONLY_FNS and
  `bff.rs` a CLI print path, so every message is a `cli.toml` key printed inside those two fns. Its
  `/api/version` body uses `serde_json`, a normal dependency (axum's `json` feature links it anyway).
  `pai_core::ini::parse_sections` does not strip a UTF-8 BOM (`trim` keeps U+FEFF): strip it first.
- The bff listens on any `ip:port` (`listen.rs`, plain HTTP, no loopback gate) and `origin.rs` guards
  the NAME: `is_our_host`/`is_our_name` accept `localhost` and any IP literal (bracketed IPv6), refuse
  any other name; the Origin rule is separate and unchanged. Dropping a `ListenError` variant means
  deleting its key from `cli.toml` in the same change, `binding_lint` fails on an unused one. The
  trailing dot: `is_our_name` strips ONE dot from the `Host` before the list/address/`localhost`
  check, so `localhost.` and `fqdn.` read bare while `.`, `..` and `name..` stay refused, and the
  `Origin` comparison keeps both spellings whole; `listen::list_name` (trim, lowercase, one dot off)
  is the one shaper of every list entry (the built name, a `resolv.conf` domain, a `Hosts` item), so
  no entry ends with a dot and `search .` adds nothing; a host name empty once trimmed builds NO
  automatic name, and the `Hosts` override applies whatever the host name is.
- pai-bff's session reply (GET/POST `/api/session`) is exactly `{"user"}`: `Identity` holds the user
  name alone (every session is a `pai` member, sign-in 403s anyone else). The server document's
  `withheld` comes only from the manager's `ManagerFile.withheld` flag: set, the bff blanks
  `OpencodeApiKey` (null) and adds `"withheld": ["OpencodeApiKey"]` even when the text still names
  a key; unset, the key goes to every session and there is no `withheld` member.
- pai-bff embeds the web UI through `crates/bff/build.rs` (`PRIVATE_AI_FE_DIST`, absolute only, else
  `fe/dist` with an `index.html`, else `fe-fixture`) into OUT_DIR's `fe_files.rs`. A missing
  `rerun-if-changed` path makes cargo rerun the script and rebuild the crate at EVERY build (cargo's
  `fingerprint/mod.rs`; the book is silent), so the fixture branch watches only existing paths
  (`fe/src`, `fe/index.html`): a plain `npm run build` is embedded only once one of those changes.
  The tests stay right meanwhile: build.rs exports the chosen dir as `PAI_BFF_FE_DIR`
  (`cargo:rustc-env`, seen by the integration tests too), which `tests/fe.rs` reads instead of
  deciding again. `fe/dist` is gitignored (`dist/`).
- pai-bff's serving lives in `serve.rs` (no strings: it returns `ServeError`, `bff::run` fills the
  keys); the graceful stop is `with_graceful_shutdown(stop_signal())` on a current-thread runtime
  with `enable_io` (tokio's signal driver rides on the IO driver). `serve::windows` hands
  `pai-bff service` to the SCM dispatcher like the manager's `entry.rs`; ERROR 1063 means a console.
- A test helper that only `#[cfg(unix)]` tests call is dead code on the Windows target: the GNU
  `clippy --all-targets` fails on it while plain `clippy` (no tests) passes. Report it to the tester.
- Adding a `Component` to `components::ALL` touches every `match` over it in `linux/package.rs`
  (`version_of`, `texts`), `old_package` (no 2.4 predecessor), `exact_requires`/`depends_on_llama`
  and the rpm `Requires(...)` of a package with scriptlets.
- pai-bff's serving runtime (`serve.rs`) is built by hand: a handler that uses `tokio::time` needs
  `.enable_time()` there, or it panics at the first request. The router tests run under
  `#[tokio::test]` (timer on) and miss it; only the binary test (`tests/pai_bff_binary.rs`) catches it.
  Likewise a handler that extracts `ConnectInfo<SocketAddr>` (the sign-in lockout) needs `serve.rs`
  to serve `into_make_service_with_connect_info::<SocketAddr>()`, or it answers 500: the router
  tests insert `ConnectInfo` into the request's extensions by hand and cannot see it.
- The bff's revision secret: `getrandom` 0.3 has NO default `std` feature, so
  `std::io::Error: From<getrandom::Error>` (the only way a missing draw reaches `ServeError` and the
  error line) needs `features = ["std"]` in `crates/bff/Cargo.toml`. `siphasher` 1.0.4's
  `sip::SipHasher::new_with_key(&[u8; 16])` hashes through `std::hash::Hasher::write`: import that
  trait and write two statements, because `io::Write` is implemented too and chaining
  `write(..).finish()` does not type. `tests/router.rs` and `tests/fe.rs` pin `pai_bff::router()` as
  infallible (`.oneshot(request)`), so the service's fallible builder is a second `pub fn
  try_router()` that `serve.rs` calls, `router()` staying the tests' wrapper.
- A limit checked under a mutex before an `.await` (the sign-in lockout around `spawn_blocking`) must
  reserve its slot under that same lock, or concurrent requests all pass the check. Give the slot back
  from a `Drop` guard (`session.rs` `InFlight`): axum drops the handler future when the client goes
  away, and a decrement written after the `.await` would then never run. Never drop the guard while
  holding the store's guard (it locks the same mutex).
- The sign-in lockout counts the 403 refusals (NotAdministrator) in a list of their own per
  (lowercased user, IP) within the window, never in `failures`; the gate answers 429 on
  `failures + in_flight >= MAX` or on `refusals + in_flight >= MAX` (two match arms, never a sum of the
  two). That is what makes a burst deterministic with no locking trick: every asked check of a pair is
  in flight or already recorded, so at the k-th admission both counters read k-1 and the pair is asked
  at most MAX times, in every timing.
- Bounding one check: `tokio::time::timeout(bound, spawn_blocking(..))` answers at the bound while the
  blocking thread keeps running, so the check's result must be written to the store only by the handler
  after the await (a timed-out handler drops the join handle and records nothing), and the `InFlight`
  guard's Drop frees the slot, so the request after a timeout is checked, not refused 429.
- A test that pins "403 is answered only after the password check" and then asserts the fake's per-user
  call count stayed at the lockout limit cannot pass: a 403 for a *new* (user, IP) pair needs a call, so
  the count is limit + 1. The only state that answers 403 without asking (a memo by user name) breaks
  the burst test's `forbidden == calls`. Report it to the tester; do not add the memo.
- A field named `connect` typed `Arc<dyn Fn() -> R>`: `self.connect()` is a hard error ("field, not
  a method"); call the field with `(self.connect)()`. A method call on `Box<dyn Trait>` (the bff's
  `Client` alias) needs no `use` of the trait: the alias's dyn type names it and rustc resolves
  the method.
- The fe's JSX-text lint (`strings.test.ts`) flags letters between any `>` and the next `<` with no
  `{`/`}` between them, code included (`</p>\n  else if (x) content =`): write each branch as a braced
  `if (..) { return <..> }` so a brace always sits between the tags.
- xtask's macOS module type-checks on Linux too: `cargo check -p xtask --all-targets --target
  aarch64-apple-darwin` (the rustup target is installed; no C build script runs). It shows the
  `common/` items only Linux calls as dead code there (`OLD_BASE_NAME` already was before step 11).
- The engine's package version carries an epoch (`1:1.0.0+pai1`): `util::version_gt` compares it
  first, rpm reads versions with `%{EVR}`, and the `.rpm` spells it as `Epoch`/`Version`/`Release`
  (`package::rpm_evr`); a file name drops the epoch (`components::file_stem`).
- `components::of` routes by first match: a file of the bff under a directory the core claims by
  prefix (`usr/lib/systemd/`, `usr/lib/sysusers.d/`) needs its exact path in the bff branch, which
  comes before the core's prefixes. A service that calls the system bus cannot use `DynamicUser=`
  (dbus-daemon drops it at Hello): the bff has a static sysusers.d user instead.
- zbus serializes a Rust array (`[String; 1]`) as a struct (`(s)`), not a D-Bus array: pass a slice
  (`let units: &[&str] = &[unit.as_str()]`) for an `as` argument (`EnableUnitFiles`).
- `binding_lint` finds keys only as literal `strings::t("key")`: write `if on { strings::t("a") }
  else { strings::t("b") }`, never `strings::t(if on { "a" } else { "b" })` (the key reads as unused).
- The engine's maintainer scripts (`system_files::engine_deb_scripts`, `engine_rpm_scriptlets`) are
  run by the tests in a sandbox with fake systemctl etc.: call every tool by name (no `/usr/bin/...`),
  and keep every `/run/...` path to `/run/systemd/system` and the record `RECORD`, which the sandbox
  rewrites. A text-check test that splits `install.rs` at the first `#[cfg(test)]` means a
  production item gated `#[cfg(test)]` above `run` hides `run` from it: use
  `#[cfg_attr(not(test), allow(dead_code))]` for a helper only tests still call.
- typescript-eslint cannot load TypeScript 7.0 (it ships no compiler API until 7.1; typescript-eslint
  issue #12518, peer `typescript <6.1.0`): `fe/` stays on TypeScript 6.0.x. Prettier has no width that
  matches the hand-wrapped fe code, and a `vi.fn(answer)` over `() => Promise<Response>` types
  `mock.calls` as `[]`: type it `vi.fn<typeof globalThis.fetch>` (a local `fetch` shadows `typeof fetch`).
- A checked-in data file whose exact text a test pins against a render (the engine's
  `crates/core/defaults/server@default.ini`) is easiest made from the failing assert itself: leave it
  empty, run the test, and `ast.literal_eval` the `right:` value into the file. In `rpm_spec`, a system
  file under `/usr/share/private-ai/` is already taken by that directory's `%files` line: listing it by
  name too makes rpmbuild warn "File listed twice".
- `cargo clippy -p xtask --target x86_64-pc-windows-gnu` prints two `xtask (manifest)` warnings
  (`serde_json` and `sha2`, unused on that target) and still exits 0: they come from
  `xtask/Cargo.toml`, not from the code, and are not a lint failure.
- xtask's `common/components.rs` is `#[cfg(unix)]` (`common/mod.rs`): a type the command line needs
  on Windows too (`Product`, the `build`/`package` argument) is declared in `main.rs` and re-exported
  there with `pub use crate::Product;` (which needs the enum `pub`, not `pub(crate)`).
- Linux `xtask package` stages the framework from `cargo-target/release` and the engine from
  `llama.cpp-cmake/bin` minus `configurator::NEXT_TO_SERVER` (the build still copies pai-config and pai
  there for the dev tree and macOS's packaging). The text-check tests slice `run` from `pub fn run(`
  to the next `\nfn `: keep its steps inline (`if engine { .. }`), and keep the literal
  `"cargo-target/release/pai-bff"` and `place("pai-bff")` a test looks for.
- The engine's directory name on Windows (`%ProgramData%\llama.cpp\`) is spelled once, as
  `paths::ENGINE_DIR`: config's names lint refuses `join("llama.cpp")` and `ProgramData\llama.cpp`
  in code lines (comments pass). A helper outside `main`/`run` that fills only `cli.toml` keys
  belongs in binding_lint's `CLI_ONLY_FNS` (the tester adds it); never copy CLI keys into en_US.toml.
- The names lints (`cargo test -p xtask names_lint`, `cargo test -p pai-config -- names_lint`) pin
  old-name rules and older installer text that a new slice can contradict (the engine now owns
  `Software\llama.cpp`, the framework MSI's name dropped the llama.cpp release). Run both before
  starting: a contradiction goes to the tester; never split an attribute or move a key into a
  comment to slip past a lint.
- Windows `xtask package` loops over `msi::products(product)`, each with its own staging dir
  (`msi::staging_dir`), `.wxs`, licence page and wix-obj beside it (`stage.with_extension(..)`); the
  engine's defaults come from `crates/core/defaults/windows/` through `msi::engine_defaults`.
- llama.cpp.wxs sequence conditions are evaluated by `msi/tests/engine_lifecycle.rs` per scenario: the
  old package of a major upgrade has `Installed` + `REMOVE=ALL` + `UPGRADINGPRODUCTCODE`, a repair
  `Installed` + `REINSTALL`, the new package of an upgrade looks like a first install (no `Installed`).
  Write `NOT (REMOVE~="ALL")` with parentheses. config's names lint also scans core's test modules:
  a tester's `join("llama.cpp")` there fails `cargo test -p pai-config -- names_lint`; report it.
  It scans pai-app's test modules too: a test literal naming the repo directory
  (`llama.cpp-framework`, e.g. in a sample path) fails `no_code_names_the_old_project`; report it.
- The macOS `.pkg` decisions (prefixes, names, identifiers, versions, install order, the pgrep
  pattern) live in `xtask/src/common/macos_pkg.rs`, pure and tested on Linux; `macos/package.rs` and
  `macos/install.rs` only call them (text-checked). `/opt/llama.cpp` is spelled once, as
  `macos_pkg::ENGINE_DIR` (xtask's names lint); pai-core spells it `root.join("opt").join(ENGINE_DIR)`.
  pai-core's macOS branch type-checks on Linux too: `cargo check -p pai-core --target aarch64-apple-darwin`.
- `xtask::common::macos_pkg` is `#[cfg(unix)]`: a test outside it that `use`s it breaks the Windows
  `--all-targets` check (E0432) though the Linux tests pass. Run the Windows check before reporting,
  and send such a test back to the tester (it needs `#[cfg(unix)]`) rather than ungating the module.
- `server_cfg::render(cfg)` is `render_for(cfg, false)`; pai-app's `save` passes
  `dirs_left_unset(backend.kind(), Os::current())`, true for the manager on Linux and Windows, where an unset
  ModelsDir/StateDir is written as a hint and `save` creates no models dir (the instances use the
  engine's folders). An instance's cwd comes from `launch::instance_current_dir`, set after
  `launch_command` (which still sets `paths::data_root()` for `start()`).
- `windows::psd1_scalars` reads only `'quoted'` strings and `[int]N`/`N`: a bool in config-build.psd1
  is written `[int]0|1` (`Minimal`), since a `$true` would read back as missing. macOS's
  config-build.toml takes a new field with `#[serde(default)]` so an older config still parses.
- `scripts/*.ps1` carry a UTF-8 BOM: a Python edit that reads with `utf-8` keeps U+FEFF in the text, and
  writing it back with `utf-8-sig` doubles the BOM. Read and write with the same codec (`utf-8-sig`
  both ways) and check `head -c 6 <file> | xxd`. No pwsh is installed here: a `.ps1` change is checked
  by eye and by the text tests only.
- `xtask/src/ci_windows.rs` pins `.github/workflows/installers-windows.yml` as text: every line that
  says `msiexec` (a step name or a message too) must carry `/l*v`, and the order checks find the FIRST
  `xtask -- <phase>`, so name the phases in step names without the ` -- `. Comment lines are dropped,
  and a step is the text between two `\n      - ` lines: a check "before the repair" goes in the
  repair's own step. Check the YAML still parses: `python3 -c "import yaml; yaml.safe_load(open(P))"`.
- `xtask/src/ci_macos.rs` pins `installers-macos.yml` the same way: a check "between package and
  install" matches literal text, so spell each package name out (a `for c in config cli core` loop
  fails it). xtask's names lint rejects `opt/llama.cpp` in any non-comment line of `xtask/src/*.rs`,
  tests included: a test needle naming the engine prefix must be split (`concat!`), report it.
- A GitHub Action's Node runtime is the `runs: using:` of its `action.yml` at the tag:
  `gh api repos/actions/<name>/contents/action.yml?ref=v7 --jq .content | base64 -d`; its majors via
  `gh api repos/actions/<name>/releases`. A workflow argument like `$env:RUNNER_TEMP\dists` is a bare
  PowerShell token, expanded without quotes; the windows runners have only C: and D:.
- The ModelsDir the GUI and CLI scan, download into and browse from is `pai_app::server_cfg::models_dir()`
  (the instance's folder under the Windows manager when unset), never `models_dir_or_default()`;
  seeding ModelsDir into the server file asks `server_cfg::seeds_models_dir`. Text-check tests pin both.
- xtask's Windows GNU check prints cargo's manifest lint `unused dependency` (`serde_json`, `sha2`,
  used only by Linux code); it is not a rustc warning, so `-D warnings` still passes. Not a regression.
- WiX extensions reach `wix build` from `installer/wix-toolset.psd1` `Extensions`, passed as `-ext` to
  every MSI (`windows/package.rs`), so a new `util:` element needs only its `xmlns:util` on the root.
- Some tests pin a caller's source text (`server_control::tests::app_save_body` wants a literal
  `render_for(` in pai-app's `server_cfg::save`): grep the tests for a function's name before
  renaming or swapping the call, even when the brief suggests it.
- fe CSS: `input[type='text']` (0,1,1) beats a bare class (0,1,0), so a class meant to restyle an input
  needs `input.cls` too. `npm --prefix fe run` runs in `fe/`, where Prettier reads `fe/.prettierignore`
  (default `--ignore-path` is `.gitignore` and `.prettierignore` in the cwd); it ignores the designer's
  Markdown (`DESIGN.md`, `briefs/`), which no formatter touches.
- Web UI (`fe/`, from the web UI session's coder): a fixed-size drawn control (`appearance: none` with
  width and height) inside a flex row needs `flex: none`, or a wrapping sibling shrinks it at narrow
  widths; only a rendered check at 375px catches it.
- A new engine (or bff) file in `xtask/src/linux/system_files.rs` `files()` also needs its path in the
  exclusion list of `the_system_files_and_scripts_name_the_core_package`, which wants the core's header
  on every other file: when the tester's tests did not add it, report the conflict, do not edit it.
- Web UI: after an `await` in a save handler the closure's draft is stale; read the rendered DOM after
  `flushSync` or use a state updater. A sticky bar whose height changes needs its scroll padding
  measured at runtime (`ResizeObserver`, guarded: jsdom has none).
- `ManagerClient` is implemented outside core in four places: `server_control::fake` (`FakeClient`,
  `DiskClient`) in pai-app, and `FakeManager` in `crates/bff/tests/{server,instances}.rs`. A trait
  signature change breaks all four; the bff ones are test files (a mechanical fix, say so).
- The manager withholds `OpencodeApiKey` from readers without configure (`ManagerFile.withheld`); a
  save of such a reader carries `keep_api_key` (D-Bus `WriteFile (ssb)`, pipe member), and
  `Manager::write_file` merges the held key under its `writing` lock before the content checks.
  pai-app's `ManagerBackend` remembers the last server-file read's flag (`api_key_withheld`).
- Web UI: a poll's deadline is a timer of its own armed when the action starts, not a check when the
  next poll fires; a store's "read after the in-flight one" needs a cancel hook.
- Web UI: an async handler that needs "still mounted" or a prop's newest value after an `await` keeps
  it in a ref written only by effects, with mounted set true in the effect's setup (StrictMode mounts
  twice). Opacity on a focusable control dims its focus ring too.
- A server.ini rule lives once in `pai_core::checks` (a `pub fn` per rule, run on the file text by
  `server_file`); `pai_core::server_cfg::check` runs the same fns on the `ServerConfig` values, since
  `render` writes an out-of-range value as the commented hint and the text check would pass it.
  `save` (pai-app, which re-exports the core's `server_cfg`) calls `check` on both backends, so
  `pai server set` and the GUI refuse alike; the CPU count is a parameter, so the core stays free of
  `available_parallelism`.
- `gguf::read_model_info`/`read_tensors` go through `gguf::ffi`, which loads llama.cpp's ggml library
  at run time: under `cargo test` it finds none and returns `None`, so a check that must work on the
  tests' fixture GGUFs (`hf_fit::tests::gguf` + `write_sparse`) reads with the pure-Rust
  `hf_fit` reader instead (`hf_fit::local_experts`, used by the save's MoE cache floor).
- Web UI: `strings.test.ts` counts only literal `t('key')` calls, so write `cond ? t('a') : t('b')`,
  never `t(cond ? 'a' : 'b')`.
- The manager links libpam by its run-time name (`#[link(name = "libpam.so.0", kind = "dylib",
  modifiers = "+verbatim")]`, `manager/accounts.rs`): no `libpam.so` symlink or `pam_appl.h` (libpam0g-dev)
  is installed here, and none is needed to build. Users and groups by name go through `nix` (`user`
  feature, already in the lock); `nix::libc` gives PAM's conversation its `calloc`/`strdup`.
- xtask's `.deb` and `.rpm` of a package are built from ONE staged tree, `.deb` first: a file whose
  text differs per format (the core's PAM service) is written by each build (`write_deb_scripts`,
  `write_rpm_files`) and the `.deb` build removes its copy with `DEBIAN/`. The `cargo::unused_dependencies`
  manifest warnings on the Windows GNU target are older than this and do not fail `-D warnings`.
- The tree is shared: a process can restore tracked files from git while an agent works (it happened
  mid-task, a few minutes after the tests were green: the edits were gone). Re-read a file before
  editing it, report the exact files changed, rerun the red list before trusting a green result, and
  never "fix" it with `git stash`, use `git show HEAD:<path>` to read the base. The reset can land
  AFTER your own post-edit `git diff --stat` shows everything in place: a test that fails quoting code
  you already replaced (a `todo!()` message from a body you rewrote, a strings key the file has) is
  the tree moving, not a logic bug, and it can hit only SOME of your files; re-read each and re-apply
  before debugging.
- windows-sys: `PSID` is `*mut c_void`, so a `SID` buffer goes in as `sid.as_mut_ptr().cast::<c_void>()`,
  not `as_ptr()` (that is a `*mut u8`, E0308). `SID_NAME_USE` is an `i32` newtype, so pass
  `&mut use as *mut i32`. `LookupAccountNameW` is under `Win32::Security` and asks for the size with a
  null buffer and 0 in the size argument: its first call returning `ERROR_INSUFFICIENT_BUFFER` is the
  query, not a failure. The account name is UTF-16 without a trailing NUL. A windows-sys feature a
  crate calls must be in THAT crate's manifest: `GetComputerNameW` and `MAX_COMPUTERNAME_LENGTH` are
  `Win32_System_WindowsProgramming`, and the manager type-checked on windows-gnu without it only
  because `if-addrs` (a pai-core dependency) enables that feature (a leak, not a licence).
- The referenced domain `LookupAccountNameW` reports for a local account is the computer's NetBIOS
  name (its doc: for a computer not joined to a domain the buffer receives the computer name), so
  compare it with `GetComputerNameW`: buffer of `MAX_COMPUTERNAME_LENGTH + 1` u16 and `nSize` set to
  that (15 characters plus the NUL the call writes, a 15 as input overflows). Its
  `GetComputerNameExW` sibling `ComputerNameDnsHostname` is the DNS name, longer whenever the NetBIOS
  name is a truncation of it, so it never matches. `SidTypeAlias` alone is not enough: a trusted
  domain's local group reports the same type, so the domain must equal the computer's name
  (case-insensitively; the call reports it upper-case).
- The machine's names by system call (bff `listen::hostname`/`resolv_conf`): nix 0.31
  `unistd::gethostname()` takes no buffer and returns `Result<OsString>` (the `hostname` feature, so
  the name is read with `to_string_lossy()`). windows-sys 0.61 `GetComputerNameExW` needs
  `Win32_System_SystemInformation` alone: its signature names only core types
  (`COMPUTER_NAME_FORMAT` is an `i32` alias with the `ComputerNameDns*` consts beside it, `PWSTR` is
  `*mut u16`, so `name.as_mut_ptr()` needs no cast), `nsize` in and out, the length written without
  the NUL. There is no system call for the DNS domain on unix (`getdomainname(2)` is the NIS domain,
  `man 2`), so `/etc/resolv.conf` stays the source there; on Windows the domain is handed to
  `host_names` as a synthetic `search <domain>` line. No test of the crate reads the machine (the
  fixtures pin `host_names` alone), so its suite stays green with those two calls broken: check the
  parity by hand (`python3 -c "import socket; print(socket.gethostname())"`).
- `pipe_server.rs`'s text test slices `fn client_facts(` and `fn group_sid(` each to the first
  `\n}\n` and wants `ADMIN_GROUP` and `local_alias_sid(` literally inside them, and forbids
  `OnceLock`, `LazyLock` and `static PAI` anywhere in the file: the `pai` SID is looked up per
  connection, never cached.
- WiX: every `Component` needs a KeyPath of its own and no two may share one, and a `util:Group` is
  neither a file, a folder nor a registry key, so it needs a registry value of its own as its key path
  (the `SystemPath` pattern: `HKLM\Software\private-ai` + a marker value). A machine-wide object that
  outlives the product is `RemoveOnUninstall="no"`: on a major upgrade (`MajorUpgrade` removes the
  product first) anything removed with it is gone for a repair. The MSI text tests read the file as
  text, so write the element the test pins in ONE line.
- `ci_windows.rs`: every step is the byte span from its `name:` to the next, and a step with
  `shell: powershell` runs with `$ErrorActionPreference = 'Stop'`, so a group check must not let
  `Get-LocalGroup` throw when the group is missing: `Get-LocalGroup -Name x -ErrorAction SilentlyContinue
  | Out-Null` and `if ($?)`. A needle written as `$env:USERNAME` fails if the file quotes it.
- A principal gate (a call acting for a user a trusted caller vouches for) goes at ONE choke point at the
  head of the method, before `config_file`/`file_path`, so every refusal reads and writes nothing; polkit
  is not asked at all for such a call (its subject is the caller, never the user). The bff's name bound is
  the bff's `MAX_USER_BYTES` in `crates/bff/src/session.rs`, mirrored as `MAX_PRINCIPAL_BYTES` in
  `service.rs`: change both together.
- When the tester leaves a shim trait (`AsRequester`) so existing call sites compile, the production faces
  send the concrete type (`&Requester`) and the shim's doc says so; do not fold the seam away: a test that
  passes `None::<&Caller>` to a generic method fixes the type parameter, and the faces' signature
  (`serve_stream(&App, &Caller)`) is pinned by `tests/manager.rs` (a unix socket peer, not a bff).
- A client bound to a principal (`PipeClient::acting_for(&self, p) -> Self`) shares ONE connection:
  the stream is `Arc<Mutex<S>>`, and the bound clone carries `principal: Option<String>` beside it, so
  the client stays `Send + Sync` (the bff/app box it as `Box<dyn ManagerClient>`). A `Self` returned
  from `&self` cannot own the stream, hence the `Arc`.
- To read a manager error by its raw NAME (so `version()` turns a `BadRequest` reply into `Ok(None)`
  for an old manager), split the call into `call_value() -> Result<Value, Failure>` with
  `enum Failure { Reply(name, message), Client(ClientError) }` and an `into_client_error()`. Every
  other caller keeps the `ClientError::from_reply` mapping; `from_reply` must NOT change, because a
  test pins `BadRequest -> ClientError::Other`.
- Parsing the `As` request forms: the arm site needs the `?`, not a helper closure.
  `let principal = || Some(Principal(v.get("principal")?.as_str()?.to_string()));` then
  `Request::Start(arg("id")?, principal().map(Some)?)`. A `?` inside a `bound` closure returns None
  from the CLOSURE, so the request is built with a None principal and the `As` call answers
  NotAuthorized, not BadRequest. Gate agreement once, before the match:
  `if v.get("principal").is_some() != method.ends_with("As") { return None }` (a principal on a plain
  method, or none on an `As`, is BadRequest); then list BOTH spellings as separate arms ("Start" and
  "StartAs"), a single arm keyed on an `as_form` flag does not fire.
- A tester's `calls` assertion can contradict its own reply assertion in the same test: three
  `manager::pipe` tests pin the `{"ok":"active"}` reply of a `Status` probe (so the fake records a
  `Call::Status`, pinned by sibling green tests) AND assert `calls == vec![]`, forgetting their own
  probe. No implementation satisfies both: report the exact diff (one extra `Call::Status`), do not
  edit the test. Same for a test-module `type_complexity` on a `[(&str, fn(&dyn Trait) -> .., Value); N]`
  array: it fails `clippy --all-targets -D warnings`, so run plain `cargo clippy -p <crate> -- -D
  warnings` to show production is clean and send the test hits to the tester.
- The `edit` tool here returns no output: re-read or grep after EVERY edit before the next one. One
  duplicated free function slipped in that way (rustc catches it, not the tool). It can also answer
  "successfully updated" while writing nothing: the next edit then failed on its own oldString, and the
  file was still the old text. A scripted replace (python, exact old text, print of the result) plus
  `git diff --stat <path>` is the only proof an edit landed.
- zbus builds a call's body by zipping the `Proxy::call` body with the method's signature, ONE value per
  argument: a body of `&("alice", &(id,))` serializes as `(s(s))` and fails `(s,s)`, so a generic
  `fn guarded(&self, method: &str, body: impl Serialize)` cannot type the arguments of every method and
  each guarded method writes its own FLAT tuple (`&(principal, id)`, `&(principal, name, text, keep)`).
  `write_file` already did.
- The manager's D-Bus client tests are NOT in `crates/core`: they are phases of `data_root_e2e` in
  `crates/manager/src/tests/dbus_client.rs`, where the real manager, polkit fake and BFF fake run in one
  process. Each `StreamTransport` opened there is a NEW connection, so a client built on its own one gets
  a new sender name (`:1.9` for the test's first, `:1.42` for a later phase's). A phase that adds calls
  from a second connection makes a `asked_bff == N x Caller(":1.9")` assertion red: the manager records
  the sender it saw, the test expectation is the stale part; report it.
- `crates/core/src/client/dbus.rs` is `cfg(target_os = "linux")` (`client.rs`), so the windows-gnu
  clippy of `pai-core` does not compile it; say so in the report.
- Each guarded manager method (start, stop, restart, read_file, write_file, set_boot_enabled) wraps
  its body in a NAMED closure (`let run = || -> Result<(), ManagerError> { .. }; let answer = run();`),
  then hands `(action, target, who, &answer)` to `Manager::record_action` and returns the answer.
  `record_action` is generic over the Ok type, so `read_file`'s `Option<ManagerFile>` shares the one
  helper, and its `who` is `Option<&Requester>` (`read_file` passes `who.as_ref()`: the bare
  `caller=- principal=-` of a request with no requester is pinned). An immediately-invoked
  `(|| { .. })()` of a 3-line body is `clippy::redundant_closure_call`'s pattern; the named closure
  (what `write_served_file` already does) is not.
- The action record's words are literal text in `service.rs` BEFORE the first `#[cfg(test)]`: the
  e2e phase slices the source at `"\n#[cfg(test)]\n"` and wants `action=`, `target=`, `caller=`,
  `principal=`, `outcome=` there and `strings::t(`/`strings::fill(` nowhere in the file, so the
  renderer stays in this file and asks no strings file. The failure name of the outcome comes from
  the private `error_kind` match beside `error_reply` (the same words); `ok` and `refused` are bare,
  any other name is `{:?}`-quoted, and `format!("{:?}", s)` escaping a quote, a newline or a NUL is
  what makes one record per call unforgeable.
- An edit that inserts before a function must carry that function's whole body in BOTH oldString and
  newString: an oldString of the signature plus one body line and a newString of only the signature
  deleted that line, and the file read back broken.
- The action log's sinks (`manager/action_log.rs`): `WriterLog::new` takes an `Arc<Mutex<W>>`, so a
  service entry writes `WriterLog::new(Arc::new(Mutex::new(std::io::stderr())))`; its record holds
  that lock across the write AND the flush (the tests count one flush per record), and keeps logging
  on a poisoned writer (`lock().unwrap_or_else(|e| e.into_inner())`; Rust's `Stderr` is unbuffered, so
  its doc must not call it a buffered pipe, the flush is for the writers that do buffer).
  `FileActionLog` opens with `OpenOptions::create(true).append(true)` per record (never a held
  handle) after a `create_dir_all(parent)` it ignores failing, and every error path is `let _ =` /
  `let Ok(..) = .. else { return }`. `manager_log_path()` is the parent of `paths::system_config_dir()`
  + `logs` + `MANAGER_LOG_FILE`: under the data-root redirect `system_config_dir` is
  `<root>/system-config`, so the log lands at `<root>/logs/private-ai-manager.log`, inside the test's tree.
- One line per record under threads: `FileActionLog` carries an `Arc<Mutex<()>>` (an inner
  `Mutex<()>` is not `Sync`, so the `Arc`: `with_log` asks for `ActionLog + Send + Sync + 'static`),
  held from `create_dir_all` to the write, and the record goes out as ONE
  `write_all(format!("{line}\n")..)`. `writeln!(file, ..)` is TWO writes: it glues lines and leaves
  empty ones; the single write of an append-mode file keeps other processes' appends whole too. A
  guard bound to a bare name is an `unused_variable` in the test build: name it `_writing` (a
  `let _ = ..` drops the guard at once, which is the whole point of taking the lock).
- `file_log_records_from_several_threads_never_interleave_within_a_line` reproduces that race in ONE
  run (131 empty lines), so "was it red?" is cheap: rsync the tree (no `build/`, `.git`, `dist/`,
  `node_modules/`) to `/tmp/opencode/<x>`, patch the sink back to the two-write form there, run with
  `CARGO_TARGET_DIR=/tmp/opencode/<x>-target` (the repo's target dir is shared with the other agents).
- A tester's negative-path test can pin an impossible case: the action log's
  `an_unwritable_path_loses_the_record_and_never_panics` makes the log path's PARENT a directory
  (`dir/logs`) and asserts `!dir/logs/<file>.exists()`, while its comment says "the file's place
  holds a directory". Any sink that creates the file (the sibling test pins that, and the e2e block
  does) creates it there, so no implementation passes. The fix is the tester's:
  `create_dir_all(&over_a_dir)` (the path itself becomes the directory, append then fails EISDIR)
  and assert `over_a_dir.is_dir()`, since `!exists()` is false for the directory the test made.
- The shared `CARGO_TARGET_DIR` holds artifacts of other worktrees: cargo reuses a `pai-bff` build whose
  baked `CARGO_MANIFEST_DIR` is `build/<worktree>/crates/core`, so every strings lookup misses (the
  binary prints the raw key `bff-version`) and seven `pai_bff_binary` tests fail with "error-line". Not a
  code defect: prove it with `strace -e trace=openat ./build/windows/cargo-target/debug/pai-bff --version`
  (it opens `build/<worktree>/crates/core/strings/cli.toml`), then `touch crates/core/src/lib.rs`, rebuild
  from this tree and rerun.
- The bff's principal seam: `Connect = Fn(Option<&str>)`; only `server::read` names a principal, and it
  comes from `Sessions::current` (which returns the session id AND the user), never from a header or a
  query. `Sessions::end(id)` is the ONE removal: sign-out and the manager's 403 share it; a
  `NotAuthorized` read ends that session, every other answer (503, 404, 502) keeps it. `system_connect`
  binds the client per OS with `acting_for(user)` (both `DbusClient` and `PipeClient` return `Self`), so
  the `match` inside each `cfg` block casts each arm to `Client`; the macOS branch needs
  `let _ = principal;`, plain clippy (not the windows-gnu one) shows the unused parameter.
- The bff's control routes (`crates/bff/src/control.rs`) take the verbs as fn values, not an enum:
  `type Verb = fn(&Client, &str) -> Result<(), ClientError>`, so one handler calls `verb(&client, id)`
  for start/stop/restart and `set_boot_enabled` (which takes `&bool`; the handler maps the body flag to
  it and maps its own error: boot has no "no such instance"). The module exports ONE `routes(connect,
  timeout, sessions)` merged in `router_with_session`, so the route paths and the `sessions`-ends rules
  stay in the module the tests cite. Boot is a write plus a `boot_enabled` read-back on the SAME client
  inside one `spawn_blocking`; the reply is the read's value, and a read-back failure goes through the
  SAME `action_failure` as the write (the boot write has no read rule of its own). Only `boot_read` and
  `instances` are session-free, so only there `NotAuthorized` is 502 `client-not-authorized`.
- The bff's two keys, one rule: `session-not-administrator` goes out EXACTLY when the bff ends a
  session because the manager refused its user (`server::read`, `control::action_failure`, each with a
  `sessions.end` beside it); `client-not-authorized` is the manager refusing the bff's OWN account on a
  session-free read, always 502 (`lib.rs` `instances`, `control::read_failure`), where no session lives
  to end. Renaming the first never touches `read_failure`/`instances`; the fe maps both keys and needs
  no second session read to tell them apart.
- The bff's PUT boot parses its body AFTER the session check, so 401/404 (no session) beat 400
  (malformed body) for the same request; the JSON-body rule (content-type essence, `application/json`
  and `+json`) is `session::json_body`, which the sign-in body now shares (one rule, one place).
  A 0 ms `tokio::time::timeout` over a `JoinHandle` is an exhausted budget, not "no bound": the handler
  must answer in the first poll. The bff's tests use 100 ms, which a 50 ms blocking fake outlives and a
  200 ms one survives (mock timers: `tokio::time::sleep` in the test, not real sleeps).
- When the tester's fake answers each operation from its own field (`Answer.verb` / `.set_boot` /
  `.boot`), a test asserting on the boot route driven from `app_verbs(Err(..))` (which sets `verb`
  alone) can never fire: the boot write answers `Ok` and the read-back replies the state, 200. Four
  `tests/control.rs` failures were that (a 404 and three 502 expectations on PUT boot). Send them back
  with the field name; `app_boot_write` is the helper that drives them.
- A test that counts `fake.principals` counts CONNECTS, and a route that writes and reads back on one
  client adds one entry for two calls. `an_action_connects_as_the_signed_in_user` expects five entries
  for `[start, stop, restart, boot write, boot read]`, while
  `the_boot_write_answers_the_state_read_back` pins the same two calls as one connect: no
  implementation satisfies both, and the brief's "reads back over the same client" is the tie-break.
  Report the exact count the fake records (4), do not add a second connect for the read.
- The body cap is `DefaultBodyLimit::max(N)` as a layer: a handler that takes `Bytes` never sees a body
  over it, and the layer's own rejection is axum's plain-text 413, not the route's JSON shape. When a
  test pins the shape, take `Result<Bytes, BytesRejection>` and map it:
  `FailedToBufferBody::LengthLimitError` is the cap (413 `payload-too-large`), everything else the
  extractor found is the route's own `malformed` key. The limit counts the bytes the stream delivers,
  not a `Content-Length` header. Exactly at the cap passes: `http_body_util::Limited` errors only when a
  frame exceeds the remaining budget, so `max(N)` reads N bytes and refuses N+1.
- A `middleware::from_fn` guard wraps the layers under it on the way in and sees the answer on the way
  out, so every layer ABOVE it runs for a refusal too. A guard that must not touch shared state (the
  session middleware ends a session on an expired idle deadline) goes on the OUTERMOST layer, and checks
  the method first: a `GET` answers with no header work at all. `from_fn` needs a `Clone`able request
  body (`axum::body::Body` is), and hands the request to `next.run` untouched.
- `http::uri::Authority::from_str` is tolerant in ways a guard cannot accept: `user@localhost:3000`
  parses with host `localhost` and port 3000, and `localhost:99999` parses with NO port at all (`port()`
  and `port_u16()` are `None`). So require `authority.as_str() == text` (a path,
  a query, a fragment, a trailing dot all fail it), reject a `@`, and read the port out of the text after
  the host (`authority.host()` keeps the brackets of an IPv6 literal, so `&text[host.len()..]` is the
  port) so a port that is not a number is refused where the parser drops it. `Uri` is worse for this:
  `Uri::from_str("http://localhost:3000")` gives `path() == ""` where the same URI with a `/` gives
  `"/"`, and there is no `Uri::fragment()` accessor. Lowercase the header value before comparing: an
  `Origin`'s host is ASCII (punycode), so `to_ascii_lowercase` is enough.
- The bff's route tests drive `axum::ServiceExt::oneshot` on the router: `Request::builder().uri(path)`
  leaves no authority in the request URI, so a same-origin guard reads the `Origin` and `Host` HEADERS
  (the tests pass `("host", ..)` pairs), never `request.uri()`. `serve` is `axum::serve`, which needs a
  `Host` header on every HTTP/1.1 request, so the header is there in the real service too.
- Two `tests/origin.rs` assertions were unsatisfiable by any code: `assert!(app.asked().is_empty())`
  after the test's own `app.sign_in()`, which is itself one authenticator ask. The tester has since
  rewritten that file to snapshot `app.asked()` after the sign-in and compare, so an `asked()` assert
  in a test that signs in first means the snapshot helper; the guard cannot fix it. Say so, do not
  bend the guard, and do not touch the test file.
- `HeaderMap::get_all` in http 1.x returns `GetAll`, which has no `len()` and no indexing, only
  `IntoIterator`: to tell "missing" from "one" from "two or more", `let mut it = v.into_iter();`
  then one `next()` for presence and a second for the duplicate. The `HeaderValue::to_str()` failure
  (non-ASCII) and the count are the whole check: a header that is not text is as unusable as a
  foreign one, so the guard answers 403, never 400.
- Reading a `Host` name: `Uri::host_str()` and `http::uri::Authority::host()` are `None` for an IPv6
  literal (`[::1]:3000`), so a loopback test parses the text as `Ipv4Addr`/`Ipv6Addr` (strip the
  brackets first). `Ipv4Addr::is_loopback()` is all of 127.0.0.0/8 and `Ipv6Addr::is_loopback()` is
  `::1` only. The bff reads the `Host` HEADER (its route tests pass `("host", ..)` pairs, and
  hyper's HTTP/1.1 parser requires the header), so the same `host_and_port` helper the `Origin`
  guard uses works for both, with the port tolerant (`OURS = false`).
- `SameSite=Strict` does not pin a port: `http://localhost:9999` is the same SITE as the bff on 3000
  and the browser sends the cookie there. So a Host check is not redundant with the cookie: the
  cookie makes a rebinding attack reach the session's `user` (not the session id), and the Host rule
  is what stops the sign-in POST, where the attacker chooses the name. The `Origin`-absent rule that
  covers a browser's same-origin POST (`curl -X POST` sends none) is `Sec-Fetch-Site: same-origin`.
- Layer order under a `from_fn` guard: `.layer(cap).layer(from_fn(guard))` puts the guard
  OUTERMOST, so a guard refusal is answered before the body cap touches the request. Put the
  `DefaultBodyLimit` on the final router (one cap, in `router_with_session`) rather than one per
  module, and then the module layers must go, and with them their `DefaultBodyLimit` imports.
- The guard's keys (`request-cross-origin`, `request-bad-host`) are served as the JSON body's `key`,
  so they are not `strings::t` calls and `binding_lint` never sees them; the text lives in the fe's
  strings file, so a new guard key needs an entry there (a bff-side coder cannot write it: the fe is
  another session's tree).
- From the web UI's UX check (2026-11-16): a check script proves each page rendered before it reports
  "0 failing", and binds port 0 rather than probing a fixed port; React 19 `useId` ids look like
  `_r_0_-Port`, so a stable selector is `[id$="-Port-label"]`.
- `binding_lint`'s `strings_t_keys_and_the_language_file_match` pins the VALUES too, in test code: a
  key whose text is `{0}{1}{2}` read as `strings::t("key")` fails it ("values passed do not match the
  text's {N}"), so a test that reads a filled key must call `strings::fill("key", &[&a, &b, &c])`
  with exactly that many. Report it to the tester (it is their test), and prove the fix in a copy:
  the file can be restored byte-identically (md5) afterwards.
- A message shared by two callers needs its OWN key in both strings files: `binding_lint` finds keys
  only at a literal `strings::t`/`fill`, so a helper in a module the CLI and the GUI both reach cannot
  borrow a caller's key (`curl-timed-out` is not the http client's).
- A blocking `TcpStream::read` that reaches its `set_read_timeout` answers `WouldBlock` on Unix and
  may answer `TimedOut` on Windows (std docs): treat both as the deadline. Never arm a zero duration:
  it makes the socket non-blocking, so a spent deadline ends the request instead.
- Widening a timeout's meaning to the WHOLE request: read every caller's budget before claiming no
  regression (`bench::live_timeout` is 900 s + 100 ms per token, a whole-generation budget; the probe's
  5 s is a whole-request budget), and say in the report that a caller with a per-read reading would
  now be cut earlier.
- After changing `crates/core`, a Windows GNU check of its dependents can answer `Finished` in
  seconds from cache: `touch crates/core/src/*.rs` and rerun to see `Checking pai-manager` /
  `Checking pai-bff` really print.
- A file the `edit` tool touches next to an insertion can LOSE a line (an oldString spanning into the
  next line, newString not carrying it): after every `crates/core/strings/*.toml` edit,
  `git diff <file>` must show exactly the intended insertions and nothing else.
- The generated (gitignored) `.cargo/config.toml` on this machine pins `target-dir = "build/windows/cargo-target"` on EVERY OS:
  a bare `cargo test`/`clippy` here writes there, not into `build/linux/cargo-target` (that is what
  `xtask configure` sets through `CARGO_TARGET_DIR`). Look for a debug binary under
  `build/windows/cargo-target/debug/` on Linux too.
- `cli_print_paths_hold_no_language` scans EVERY print/format literal of a file listed in
  `CLI_PRINT_PATHS` (manager.rs's flag includes `format!`), not just `main`/`run`: a
  `format!("/proc/{pid}/cmdline")` in manager.rs is a hit (proc/pid/cmdline read as words). Build
  such a path with `Path::new("/proc").join(..).join("cmdline")` outside any print/format macro.
- A lib-root `use pai_core::x` used only inside `cfg(target_os = "linux")` code is an
  `unused_imports` hit on windows-gnu: gate the root `use` with the same cfg (the fn-local `use`
  inside `run` needs it too).
- `Command::get_args()` yields `&OsStr`, not `&OsString`: `.cloned()` fails (the glue is
  cfg(windows): only the windows-gnu check shows it). Collect with `.map(OsStr::to_os_string)`.
- windows-service 0.8.1: `ServiceControl::Preshutdown` exists; the raw code of a control is
  `control.raw_service_control_type()` (not `raw_value()`); `ServiceControlAccept::from_bits_truncate(u32)`
  maps the OS-neutral accept bits (bitflags 2).
- The text-pinned glue tests slice `fn name(` to the first column-0 `\n}\n` and want literal
  needles inside that slice: keep `instance_save_target(`, `save_all(`, `SAVE_BUDGET` and
  `child.kill()` literally inside `instance_main` (extracting the save loop into a helper fails
  them). Their `entry_code()` drops only WHOLE-LINE comments: a needle in a trailing `code // ..`
  counts, one in a `///` doc comment does not, and `register` must contain no `ends_instance(` /
  `ends_manager(` at all (it takes `ends: fn(u32) -> bool` and calls it on
  `control.raw_service_control_type()`). Same for the accept mask: `set` takes
  `accept: fn(bool) -> u32` and fills `controls_accepted:` from `accept(state == Running)` (5
  params, the mask name not `controls_accepted`, no `controls_accepted(` in its body); each main
  passes its own rule by BARE name, because `instance_mask_asks` counts every `controls_accepted`
  that is not part of `manager_controls_accepted` (serve_main must read 0, instance_main > 0).
- A key of cli.toml alone used in `manager/windows/entry.rs` (not a CLI_ONLY_FNS fn) trips
  binding_lint's `alone` check: put it in en_US.toml with the identical cli.toml value; the
  include_str! strings tests of core then pass too.
- rustfmt joins a call onto ONE line only when its argument list has NO trailing comma (and its
  args fit `fn_call_width` 75): write every `set(..)` / `register(..)` call you want kept
  one-argument-per-line with a trailing comma and `cargo fmt -p <crate> --check` accepts it.
- `SERVICE_ACCEPT_SHUTDOWN` is 0x4 and `SERVICE_ACCEPT_PAUSE_CONTINUE` 0x2 (SERVICE_STATUS,
  learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_status); the windows-sys
  0.61 names match. A `from_bits_truncate` of a wrong 0x2 reports the service as pausable.
- scm.rs's pins (`scm_fn`) cut a body at the MIN of the first `\n}\n` and `\n    }\n`: no inner block
  of a pinned fn may close at 4 indent (it truncates the slice). Every needle of
  `restart_answers_once_the_stop_is_accepted` must sit INSIDE `fn restart` (a helper doing the wait
  and the start fails the `at > spawn` test), while the preshutdown set may be a helper:
  `preshutdown_set` follows ONE top-level `fn` called from the body (first match in file order wins)
  and needs the four needles (`ChangeServiceConfig2W(`, `SERVICE_CONFIG_PRESHUTDOWN_INFO`,
  `dwPreshutdownTimeout`, `preshutdown_timeout_ms`, the last in the helper's own parameter) in it.
- `call_arguments(hay, name)` appends its own `(`, so pass a BARE name ("open_service", ".cancel").
  `start_sets_the_preshutdown_timeout_on_the_service_it_opened` passes `preshutdown_set`'s needle,
  which always ends with `(`, so it searches `set_preshutdown_timeout((`: no call with the handle as
  its first argument can match, and the only shape that does is a helper taking ONE tuple parameter
  (`set_preshutdown_timeout((&service, spec.preshutdown_timeout_ms))`). The needle is the test's slip
  (every sibling passes a bare name): report it with the one-line fix
  `call_arguments(&body, call.trim_end_matches('('))`, and prove that fix in a /tmp copy of the tree.
- windows-sys 0.61 preshutdown: `ChangeServiceConfig2W(SC_HANDLE, SERVICE_CONFIG, *const c_void) -> BOOL`
  with `SERVICE_CONFIG` a plain `u32` newtype const (`SERVICE_CONFIG_PRESHUTDOWN_INFO` = 7) and
  `SERVICE_PRESHUTDOWN_INFO { dwPreshutdownTimeout: u32 }`. `CreateServiceW`'s `dwAccess` must carry
  `SERVICE_CHANGE_CONFIG` or the returned handle cannot call `ChangeServiceConfig2W`.
- One sink shared by `Manager::with_log` (`impl ActionLog + Send + Sync + 'static`) and
  `ScmService::with_log` (`Arc<dyn ActionLog + Send + Sync>`) needs the blanket
  `impl ActionLog for Arc<T>` (service.rs); `NoActionLog` is `pub` so `ScmService::new` defaults to it.
  A record rendered by a thread with no request has no caller/principal: pass the failure text
  (`ServiceError(pub String)`'s `.0`, e.g. `scm_error(id, TIMEOUT).0`) to `restart_failure_line`.
- Web UI: a new strings key must land in `fe/src/strings/en_US.json` with the EXACT text the tester
  pinned in the `EN` table of `fe/src/test/bff.ts` (App.test.tsx compares the two files). Never retype
  it: eval that object literal out of the test file (it is an `export const` starting at column 0) and
  append only the keys missing from the JSON.
- Web UI: a card page's full-page tests read the card and field order from the shared list
  `fe/src/test/presetCards.ts`, so a new card goes where that list puts it (for the preset page, right
  after Assets), and a field's label is matched as a substring, so labels that contain each other
  ("n-gpu-layers" in "tensor-split") must be asserted by the exact `<label>` text, not `includes`.
- Web UI: in `fe/src/index.css` the later `.card .note { padding: 0 }` (0,2,0) beats a bare `.note`
  (0,1,0): card note spacing goes on `.card .note`, taking `--space-*` tokens (the gap below a card
  comes from the grid, so `margin-bottom: 0` keeps it).
- Web UI: the hints are copied verbatim from the GUI's strings (`crates/core/strings/en_US.toml`) and
  pinned in the `EN` table, so the fit-to-vram hint carries the word `tensor-split` and a hidden
  (`.hidden`, `display: none`) element stays in `textContent`. A card test that asserts a field name is
  NOT in `card.textContent` cannot pass while the mandated hint is in that card: report the test, never
  drop or reword the mandated hint.
- `clippy::todo` is a restriction lint, ALLOW by default (clippy 0.1.100, rustc 1.100): a `todo!()` the
  tester left in a test-only match arm does NOT fail `cargo clippy -p <crate> --all-targets --
  -D warnings`. Proof: the same run with `-W clippy::todo` prints it. So a slice can go green with a
  stub in place; name the stub and the slice that fills it in the report.
- The manager's principal gate refuses a caller that is not the bff's account BEFORE
  `PasswordChecker::is_admin`, so a test that counts `asked_admin` once per call must not count a
  non-bff call: `check_principal` asks the account only for the bff's (`service.rs`, the same order
  `read_file` and `write_file_as` pin in their green tests).
- A file can be rewritten under you while you read it (the tester replaced a whole test module between
  my `read` and my `grep`: same lines, different text, `grep` found nothing for a name `read` had just
  shown). A `grep` that misses text you read a moment ago means the file MOVED, not a look-alike glyph:
  re-read the exact region right before each edit, and edit surgically, never rewrite a file.
- One stale expectation in the crate's single `data_root_e2e` hides every phase that runs after it. To
  check the rest without editing a test as a fix: make the minimal temporary edit, run the test, then
  restore it with the exact inverse edit and re-read the lines to prove it. Report both the run and the
  temporary edit; the tester owns the expectation.
- Web UI (`fe/`): `PresetCards.tsx`'s `readAs` is the ONE place a boolean key is read (the `Reading`
  union), so a key with its own engine rule is a new member of that union and a new arm of `readAs`,
  never a branch in `PresetField`. A checkbox row shows `row.absent` when its key is absent, so a key
  whose engine default is on (`fit`) sets `absent: true`; a key that takes a value keeps the empty
  value present in `heldFor` (the engine is passed `--key ""`). `Field.tsx` needs no change for a new
  field error: `CheckboxField`/`GroupField` already take `error` and `describes()` names the error
  first then the written-as line and sets `aria-invalid` only with an error.
- Web UI: a group of 90 is an `h4` SIBLING right before `<div role="group" aria-labelledby>` (the
  pattern `fe/DESIGN.md` writes), not a heading nested in the division: the tests' `before(a, b)` is
  `compareDocumentPosition`, an ancestor precedes its descendants, so `before(h4, group)` is false for
  a nested h4. A card's rows nest as data (a `group` row carrying `rows`), and the one renderer
  dispatches on it, which is what the brief asks for.
- Web UI, unsatisfiable assertions the tester must fix (report, never edit): `PresetCards5.test.tsx`
  `nameOf(el)` returns `textContent` for anything that is not INPUT/SELECT/TEXTAREA, so
  `nameOf(group) === heading` cannot hold for a group that holds fields (it also demands
  `before(h4, group)`); its 113 test builds the stub key as `` `${heading}-size-n` `` without the
  `spec-` prefix so `SET[key]` is `undefined`; its 81 test scopes `getByRole('button', {name: 'About
  Lookup n-gram'})` to the CARD, which 113 makes three times ambiguous. The page-level 2d.68 pins the
  heading outline as `1 2` + one `3` per card, so any h4 in a card turns it red.
- Web UI: one test file is `cd fe && npx vitest run src/<File>.test.tsx` and one test adds
  `-t "<test name>"` (`npm --prefix fe test -- <name>` passes the name as a FILE pattern and finds
  nothing). `npm --prefix fe run typecheck` is `tsc -b`: it prints nothing when clean, so the exit
  status is the result. Format before the checks with `cd fe && npx prettier --write <files>`, since
  the npm scripts run in `fe/` (that is where `.prettierignore` lives).
- The Server form's two routes share their keys in `crates/bff/src/server.rs` `FIELDS`: a
  `Field { key, kind, read, write }` table (the INI key, the `Kind` of JSON type the read gives, a
  `fn(&ServerConfig) -> Value` and a `fn(&mut ServerConfig, Option<&Value>)`). The GET builds
  `values` from `read`, the save validates against `kind` and assembles with `write`: a new server
  key of the web form goes in that ONE table, never in a second list.
- That body-to-file chain (`BodyAnswer`, `body_to_file`, `field_of`, `Kind::reads`, `Field`'s `kind`
  and `write`) is dead code until the `PUT` route calls it: `clippy -p pai-bff --all-targets
  -D warnings` reports exactly 5 `never used`/`never read` hits, all of them that chain. To show
  nothing hides behind them: widen the items to `pub` and append a `pub fn` in `lib.rs` that calls
  `body_to_file`, run plain clippy (zero output), then restore `lib.rs` from a `/tmp` copy and the
  `pub(crate)`s with `sed`. No `allow` attribute.
- serde_json's `as_i64()` is None for `4.5` and Some(4) for `4.0`, and `as_bool`/`as_str` refuse the
  other JSON types: one `Kind` per key plus those accessors IS the "the value is of the type the read
  gives it" check; put `i32::try_from` inside it so a huge number cannot truncate into a range check.
  A blank text is unset via `server_cfg::opt_nonblank` (the same rule the INI read uses), a base URL
  via `server_cfg::normalize_base_url` (its `and_then` chain takes `as_str` directly).
- `checks::Refusal` is `{ key: &'static str, values: Vec<String> }`: a struct literal that moves
  `values` into one field and borrows it in another must write the BORROWING field FIRST (a literal's
  fields are evaluated in the order they are written, the other order is E0382).
- `ServerConfig` derives `Default` (the plain switches land `false`), and `server_cfg::check(cfg,
  cpus)` is in `pai-core`, so the bff runs every content check without linking `pai-app`.
- The server save (`server::save`) answers in that order: session (401, no connect), body
  (`Result<Bytes, BytesRejection>` for `origin::body_refusal`'s 413, then `session::json_body`, then
  `revision` as a string, then `body_to_file` with `available_parallelism`), then ONE
  `spawn_blocking` over ONE `connect(Some(user))` reading, comparing, `write_file_if`ing and asking
  `status`. Compute the answer's `revision` from the rendered text BEFORE the closure: the closure
  moves the text, and the same key hashes both.
- The `running` rule is folded INSIDE that closure (`Err(_) => true`), so a status that fails after
  a write that landed is a 200 with `running: true`, never an error answer; the 200 body is exactly
  `{"revision", "running"}`.
- A save test's `calls` list cannot omit the route's own read next to its own
  `expected == WITH_KEY` CAS assert: the revision is a keyed one-way hash, so the expected text can
  only come from a read in that request; the same test's `principals == [Some(user); 2]` (the form's
  GET plus the save) pins ONE connect for read + write + status. Report the exact list the fake
  records, never split the write over two connects.
- A save test that reuses a document read before another session's save pins a stale revision: the
  409-writes-nothing rule of its own green sibling leaves one write where it expects two, so the
  test must re-read between saves.
- `!text.contains("ModelsDir = ")` cannot pin an unset key: `render_for(cfg, true)` writes the
  mandated hint `; ModelsDir = /srv/llm  ; ..`, which carries that substring. The check is on the
  line head (`lines().any(|l| l.starts_with("ModelsDir = "))`).
- The save's 503/504 rule keys on ONE `Arc<AtomicU8>` state (server.rs `WRITE_IDLE`,
  `WRITE_ASKED`, `WRITE_CANCELLED`), each side moving Idle to its own value with a
  `compare_exchange`: the blocking task just before `write_file_if` (a lost CAS means the handler
  already answered 503, so the task returns without writing), the handler in its timeout arm (won:
  503, the write was never asked and the task will never ask it; lost: the state is Asked, the
  write is in flight, 504). A `store(true)` flag is the race: a task parked at the connect or the
  read wakes after the 503, the revision still matches, and it writes the file the browser was told
  was untouched. The flag read alone cannot close that, only the CAS can, so `save_failure` keeps a
  bool (`state.load() == WRITE_ASKED`: a task that answered cannot hold Cancelled) with the guard
  arm `Unavailable if write_asked` for a stream that died on a SENT write.
- The bff has NO shared session-check or timeout helper: "reuse the server read's helpers"
  means `session::error` and `Sessions::current`/`end` plus the two-line shape
  (`spawn_blocking` of a closure annotated `-> Result<_, ClientError>`, then
  `tokio::time::timeout(timeout, ask).await`). A route reading TWO files keeps them in ONE
  blocking task with `?` between, so the first file's refusal answers before the second read.
  `GET /api/presets` maps its own three answers (403 + end, 503 for Unavailable and the time
  limit, 502 `client-other` for the rest, no 404, `ClientError::Service` included in "else"):
  `control::action_failure` is NOT reusable (it has 404 and `client-service-failed`).
  Order rule mirrors `pai_app::presets::listed_in` (on-file order, then the off-file sections
  whose id, compared `eq_ignore_ascii_case`, the on file lacks); a file the manager answers
  `None` is an empty group, and `ini::parse_sections` already trims values and cuts inline
  `;`/`#` comments, so `model` needs no parsing of its own.
- The preset switch (`crates/bff/src/presets.rs` `set_enabled`, `PUT /api/presets/{id}/enabled`) shares
  the save's write state: `server::WriteState` (`pub(crate)`, `new`/`may_write`/`sent`/`timed_out`), one
  `Arc` per request. `may_write()` is a CAS Idle -> Asked and answers TRUE when the state is already
  Asked, because a move is TWO writes: a handler that 504'd on the first leaves its task to ask the
  second (a test releases its parked write and expects both to land). The second write needs no guard:
  the state moves to Cancelled only from Idle, so once a write was asked the handler can only 504.
- The switch answers `enabled` from a `read_file` of `presets.ini` AFTER the last write
  (`fe/briefs/api-contract.md`: "`enabled` is read back after the write"), the list's rule as well (a
  section in both files is enabled): seven manager calls, `read presets.ini` between the last write
  and `list`.
- The switch's 404 `no-preset-named` args: echo the id only when `checks::preset_id(id).is_ok()` AND it
  is not all dots. `valid_preset_id` accepts a run of dots, and the contract says `.` and `..` name no
  preset, so a name that can never be a section is answered with `args: []`.
- `DISABLED_FILE_HEADER` stays a byte-identical copy in the bff (it links `pai-core`, not `pai-app`):
  `the_disabled_header_is_the_configurators_one` reads `crates/app/src/presets.rs` as TEXT and pins the
  literal `const DISABLED_FILE_HEADER: &str = "` in THAT file, so "move the const to `pai-core`" turns a
  green test red; the same test keeps the two copies equal.
- The bff's refusal keys are literals in the code, not `strings::t` calls, and no lint pins the set:
  `server-malformed` and `boot-malformed` have no `en_US.toml` entry, so `presets-malformed`,
  `preset-changed-elsewhere` and `client-timeout` need none (the fe carries the text). `no-preset-named`
  is a check key already in `en_US.toml`, with `{0}` the id.
- Two manager limits, both `pub` in `bff/src/lib.rs`: `MANAGER_TIMEOUT` bounds a read, `WRITE_TIMEOUT`
  (longer) a write, because a save's read-back and status ride the task of a write that already asked,
  and a write that lands after the answer is the write's outcome, not a lost one. The seam that pins
  them is `router_with_session_and_write_timeout`; `try_router` (the production signature) takes both
  and `router_with_session` passes the same value for both. A write handler takes ONE limit, named
  `write_timeout`: its own reads ride the same blocking task, so it has no read limit to pass.
- `WriteState` is 3-valued (Idle / Asked / Done): `timed_out()` answers `TimedOut::{Cancelled, Asked,
  Done}`, `landed()` moves Asked to Done after the LAST write of the call, and `sent()` is true of
  Asked only: a Done task is not "outcome unknown", so every 504 test stays a 504 with the CAS. The
  Done answer is the route's own 200: `running: true` always (a status the bff cannot read), and for
  the switch `enabled` is the flag the request asked, because a move that landed is exactly a
  `presets.ini` with the section when the flag is true and without it when false.
- `WriteState::nothing_to_write()` (Idle -> Done, `server.rs`) is how a call that asks NO write reaches
  that 200: `presets::save` calls it when the new text equals the file's. A save with no changes that
  rides a slow read lands in the TIMEOUT arm (`TimedOut::Cancelled` from Idle is a 503), so settle the
  state and let the Done arm answer: a flag on `WriteState`, a handler-side branch, or a second
  mechanism are the same thing built twice.
- A manager call that FAILS after a write landed must not be a 503: the switch's read back of
  `presets.ini` answers the landed move (`switch_failure` never sees it), and only
  `ClientError::NotAuthorized` propagates, the 403 that ends the session. A `?` on any post-write
  read is a 503 on a move that stands in the files.
- llama-server's global `[*]` is not a preset: `rows()` drops it from BOTH files (one `is_global`),
  and `switch()` answers `Answer::NotFound` for it AFTER the two file reads (the route reads first),
  which leaves the id failing `checks::preset_id` so the 404 carries `args: []`.
- `read` on a source file has handed back content that is not in the file (a fabricated whole
  function, real line numbers, an md5 of its own output as a line): a `write` based on it destroyed
  `presets.rs` and the compiler saw a file of 84 lines. Verify the text with bash before editing
  (`awk 'NR>=a && NR<=b' file`, `md5sum`, the compiler's own line numbers), prefer `edit` over a
  whole-file `write` for an existing source, and `git checkout -- <path>` restores the file.
- When a test's keyed hash (the bff's `revision`) disagrees, settle it with an independent
  implementation checked against a vector the repo already pins: `crates/bff/tests/server.rs` pins
  the published SipHash-2-4 of `""` (`726fdb47dd0e0e31`) and of the shipped server file, so a script
  that reproduces those is standard SipHash-2-4 and can rule on every other constant
  (`/tmp/opencode/check_vectors.py`). A hand-written SipHash in a test file can be non-standard:
  `preset_read.rs`'s `siphash_2_4` keyed v2/v3 with the literals `7450_6172_7661_7465` and
  `4d6f_6e67_6f6c_6973` (no key), so its own `KEY_A`/`KEY_B` assertions were the red part while its
  five pinned `REV_*` constants were the standard hash. The round is
  `v1 = ROTL(v1,13); v1 ^= v0; v0 = ROTL(v0,32)`: writing `v0 ^= v1` reads the same and hashes
  something else, so never trust a fresh script before a published vector passes it.
- matchit's `{id}` is ONE percent-decoded segment: an id with an encoded slash (`alpha/%2A` decodes
  to `alpha/*`) misses the route and gets the router's own 404 (empty body, content-type `""`, no
  connect, no reads), which an `assert_key` of a JSON body reports as a content-type panic. A
  2-segment GET route to catch it also answers `GET /api/presets/{id}/enabled`, which
  `presets_switch.rs` pins as 405 with no manager call: no route shape satisfies both, report it.
- `cargo fmt -p <crate>` reformats the whole package: run it, then `git diff --stat -- crates` to
  show the format pass touched only the files of the slice.
- The bff's read routes share their failure mapping as one `fn read_answer<T>(sessions, session,
  outcome, answer)`: `outcome` is `Result<Result<Result<T, ClientError>, tokio::task::JoinError>,
  tokio::time::error::Elapsed>` (the Elapsed of `tokio::time::timeout` is at `time::error::Elapsed`,
  NOT `time::Elapsed`: nothing re-exports it), and the success value goes to a `FnOnce(T) -> Response`
  so the caller keeps its own 200 and its own 404 arms.
- A test file's pinned digest and its fixture can disagree: `preset_read.rs`'s `REV_ZETA`
  (`06d798fe3318872c`) is the digest of the `ZETA` const (two keys), while the fixture
  `ALPHA_PLUS_ZETA_CRLF` appends a `[Zeta]` with ONE key, so the read of `Zeta` there can never answer
  it (the section text is a prefix of `ZETA`'s). Its own helper agrees with the code (the green
  `the_revision_is_stable_across_another_section` pins `revision_of(&KEY_A, ON, ZETA)`); the tester's
  fix is to append the real `ZETA` text to that fixture (its doc comment says that is what it holds).
  Rule a pinned constant by hashing the fixture's bytes with a script that first passes a published
  vector, before changing the code.
- Two assertions on the same slice can be unsatisfiable together: `set_key`'s header-only test pins
  `rest.starts_with('\n')` (a break after the header) and then
  `rest.trim_end_matches(['\r','\n']) == "n-cpu-moe = 8"`, and a trim at the END leaves a leading
  break in place, so no writer passes both. Report the exact pair and the fix (`trim_matches`, both
  ends); the writer is judged by the assertions that CAN hold.
- `ini::text::set_key` writes the canonical `key = value` (the configurator's writer too). A test that
  builds its expected text by rewriting a COMPACT fixture line (`ctx-size=8192` to `ctx-size=16384`)
  is red by construction, and a needle `matches("ctx-size=")` counts a canonical line as zero: the
  fixture and the needle must carry the canonical spelling. Check such a claim mechanically
  (line-wise, allowing only `key=` to `key = `), not by eye.
- `assert_eq!`'s message is a format string, so a `{` in a compared value prints DOUBLED (`{{`) in the
  panic output: a script that diffs captured `left:`/`right:` lines must de-double both sides.
- `set_key`'s equal-value byte-for-byte case applies only when the single line spells the key in the
  exact case given (`line_spells_key`, ini.rs); the duplicate count stays any-case, so a key held twice
  still collapses onto the first occurrence.
- The preset keys the web may write are `pai_core::checks::WEB_PRESET_KEYS`, the configurator-only ones
  `checks::PRESET_KEYS_NOT_ON_WEB`; `pai-app`'s `web_key_lists_are_exactly_what_render_section_writes`
  pins the union against `render_section(&full_preset())` read by `ini::parse_sections` and keeps the
  lists disjoint, so the lists travel with the writer and name no count in a comment.
- A `pub(crate)` item of a private module that only `#[cfg(test)]` code calls fails
  `cargo clippy -p <crate> --all-targets -- -D warnings` (the plain lib target builds without the
  tests): the rule and its variant are dead there. Keep the wrapper the unit tests call and split the
  part production calls (`body_to_text` = `parse_body` + `apply` in `bff/src/presets.rs`, so the body's
  checks answer BEFORE the manager is connected to, which a test pins by an empty call list). The
  wrapper a test-only caller reaches is `#[cfg(test)]` (a fn simply stops being built, so production
  carries no allow. Do the same with its answer enum: the LAST
  `#[cfg_attr(not(test), allow(dead_code))]` went by splitting it, the narrow production error
  (`BodyError { Malformed, Refused { key, args, field } }`) beside the wrapper's whole answer
  (`PresetBodyAnswer`, all of it `#[cfg(test)]`, its `Text` variant included, names unchanged so
  `src/tests/preset_body.rs` compiles untouched). A `#[cfg(test)]` variant that production's `match`
  names is the shape to avoid, not to allow. So `parse_body` takes the `Value` `json_body` parsed,
  and the `#[cfg(test)]` wrapper does its own `serde_json::from_slice`: that is what keeps the
  wrapper's `Malformed` test (raw bytes in) true.
- Every bff body route gates the content type with the ONE helper `session::json_body(&headers, &body)`
  (the sign-in, the server save, the switch, the preset save): absent, unreadable, or another essence is
  the route's own `*-malformed` 400, answered BEFORE the first connect, so a test pins it with an empty
  calls list. `application/json; charset=utf-8` passes: the helper compares the essence.
- `serde_json`'s `preserve_order` is ON for a crate built with `pai-app` in the same invocation and OFF
  in a bare `cargo test -p pai-bff`, so a body's `Map` iterates in insertion order in one build and in
  a hash order in the other: `parse_body` sorts the edits (`edits.sort_by(|a, b| a.0.cmp(&b.0))`) so
  `apply` writes the keys of a body in file order whatever the map. A red that only `preserve_order`
  shows is INVISIBLE in a bare `-p pai-bff` run: reproduce it with both packages in one invocation
  (`PRIVATE_AI_LLAMA_CPP_DIR=$PWD/build/linux/llama.cpp cargo test -p pai-bff -p pai-app --lib -- <name>`).
  Name the order in the doc comment as the code states it ("sorted by key"), not as a property.
- The bff's two-limit write shape (`presets::save`, the split `server::save` does not need): ONE
  blocking task under `timeout` for the connect and the reads, the `Client` moved out of it into a
  SECOND task bounded by `write_timeout.saturating_sub(Instant::now().elapsed())` for the write and
  the states after it; every write behind `WriteState::may_write()` so a task the deadline left behind
  writes nothing. A route handler that takes both limits is 8 arguments: `#[allow(clippy::too_many_arguments)]`
  (precedent in `app/src/bench/exec.rs`, `app/src/hf_fit.rs`, `xtask/src/linux/package.rs`).
- In the bff, `session::json_body(headers, body)` PARSES a request (`Option<Value>`); a JSON answer is
  `(StatusCode::OK, axum::Json(v)).into_response()`, and an over-`origin::MAX_BODY` body is the
  extractor's rejection handed to `origin::body_refusal(&rejection, <fallback key>)`, which answers the
  413 `payload-too-large` (the cap is the `DefaultBodyLimit` layer in front of the whole router). An
  axum handler here takes `Path(id)` first and the `Bytes` last, as the existing routes do.
- The SESSION is read before the body in every bff route: an over-`MAX_BODY` body with no session is
  401 `session-bad-credentials`, because the extractor (413 `payload-too-large`) never runs when the
  session check fails first. Only sign-in has no session, so it is the one route whose doc may say the
  cap answers before the shape check. Four comments said "the cap answers first" for the server save,
  the switch, the preset save and the sign-in: a doc comment states the order the code has.
- `cargo fmt -p <crate>` rewrites the crate's `tests/*.rs` too. When a test file of the tester's is
  pending a lint, apply rustfmt's diff to your own file by hand (`cargo fmt -p <crate> --check` prints
  it) and report the test file: `cargo clippy -p <crate> -- -D warnings` shows the production code clean.
- The bff's framing and sniffing headers (`X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`)
  are ONE middleware in `lib.rs` (`framing`), added as the LAST `.layer` of
  `router_with_session_and_write_timeout`, which is the OUTERMOST position: the guard's 403, the body
  cap's 413 and the router's own 404 and 405 are answered above every route, so only an outside layer
  can set them. `HeaderMap::insert` replaces, which is what makes "exactly once" true whatever a route
  wrote. The two pairs sit in one `const [(HeaderName, HeaderValue); 2]`: both `from_static`s are
  `const fn` in http 1.5. A Content-Security-Policy, once the web UI agrees one, goes in this layer.
- Web UI: the default-box mechanics (a ticked box means "absent from the file") are shared in
   `fe/src/boxed.ts` (`Boxed`, `boxedFrom`, `boxText`, `tickBox`), which `ServerForm.tsx` uses too. A page
   with boxes keeps `boxed: Record<string, Boxed>` beside the draft of the selects, and derives dirtiness
   AND the PUT body from ONE `changed()`, so what is sent is exactly what is counted as changed. The number
   range table of the preset page is `PresetCards.tsx`'s `INT_KEYS` keyed by INI key (its rows built with
   `bounded()`), not a flag spread through the field table.
- Web UI: the save's per-field checks run BEFORE the `!dirty` guard: the tester's test unticks the box of
   a field absent from the file and clicks Save with nothing changed, expecting the mark, the bar's
   `save-invalid` alert and no PUT.
- Web UI: making a read-only card editable turns red every fixture that pinned it read-only. The shared
   list `fe/src/test/presetCards.ts` spells the editable set (`EDITABLE_FIELDS`) and derives the tab stops
   (`fieldTabStops`) and `isEditableField` from it, so the tab-order and "every control disabled" tests of
   PresetCards1/2/3/4/5, PresetContent and PresetSave1 have ONE fix point there (an editable number field
   is TWO tab stops, its box and its input); the full-value-line fixtures (PresetFullValue, PresetSave1
   2h.134) must move to a field that stays read-only. Report the list, never edit the tests.
- Web UI: two tests in one new file can be unsatisfiable together. `PresetSave6.test.tsx`'s 2h.155 asserts
   the default box of an absent field (batch-size, kv-unified-per-slot, and kv-unified while parallel is
   ticked) is ENABLED, while 2h.156 asserts that same input is `disabled` and empty with its box `checked`,
   from the identical fixture. 156's wording governs; report the exact triple.
- `npm --prefix fe test` lists failures as `FAIL  src/File.test.tsx > name` lines, so
   `grep -E '^ FAIL  src'` is the red list and `grep -E "Tests +[0-9]+ (failed|passed)"` the summary.
- The `edit` tool here intermittently fails to find text that `awk` shows verbatim: append with a shell
   heredoc (`cat >> file <<'EOF'`) and check `git diff --stat`.
- Web UI: a helper shared by two pages takes each caller's rule as a named parameter, never one page's
  rule (fe session, 2h.4).
- A middleware that needs per-router facts uses `axum::middleware::from_fn_with_state(policy, guard)`:
  the router's state stays `()`, the fn's first parameter is `State<T>` (so `State<HostPolicy>`),
  then `Request`, then `Next`, and `T: Clone + Send + Sync + 'static` (a `Vec` inside an `Arc`).
  `from_fn` is the same call with `()` state.
- One `Host` name rule over both families: strip the brackets and `text.parse::<IpAddr>()` covers IPv4
  and IPv6 (an IPv6 with a zone id parses as no address, `Ipv4Addr`/`Ipv6Addr` each need their own
  parse). `IpAddr::is_loopback()` is `127.0.0.0/8` and `::1`, `is_unspecified()` is `0.0.0.0`/`[::]`,
  so "the listen serves the loopback" is those two.
- A `tool_use` call the harness rejects for a bad parameter key (`new_string` for `newString`) can
  still write its arguments into the file: the file gained `</parameter>` and `<parameter=newString>`
  lines. After any rejected edit, `grep -n "parameter>" <file>` before the next one.
- `ini::parse_sections` keeps the key lines BEFORE the first section header nowhere, so a key list
  written without a header reads as empty. To read the keys of an assignment section that may arrive
  header-less, parse `&format!("[keys]\n{text}")` and take the LAST section (the text's own header,
  when it has one, becomes that last section).
- `ini::detect_eol` is `pub(crate)`: `assign::render` writes its generated header in the pool's style
  with it, taking the style from `disabled` when `enabled` is empty (an empty text reads as CRLF, the
  writer default). A copied section's text has no ending of its own when it is the last one of a file
  with no final break, so the render appends its `eol` after every section not ending in `\n`, else the
  next section header lands mid-line and glues two values into one.
- The assignment render rule the tests pin: `* = on` / `* = off` replace the file-based global switch
  for EVERY preset of the pool, so `* = on` reaches into `presets.ini.disabled`; `* = inherit` and a
  bare `<id> = inherit` follow the file that holds the section. A value that is not `inherit`, `on` or
  `off` is an ABSENT key (drop it in `assigned` with `BTreeMap::retain`, so it also stops counting as
  an `*`). One id is one preset: `ini::find_section_header` answers the FIRST case-insensitive match,
  so a `BTreeSet` of the lowered ids renders an id written twice, in one file or in both files, once.
- rustfmt breaks a one-line `let x = if c { a } else { b };` when the if-else text is over 50 chars
  (`single_line_if_else_max_width`, the default): write the multi-line form. Shortening the test with
  `s == ""` is `clippy::is_empty`, which `-D warnings` refuses.
- Running a web UI test as `npx vitest run fe/src/<File>.test.tsx` from the repo root uses NO config
  (no `fe/vite.config.ts`, so no jsdom): every test of a DOM file fails at 0 ms with
  `ReferenceError: window is not defined` and an `afterEach` `dialogs.restore()` on undefined, which
  reads as a total code break. The one path is `npm --prefix fe test -- src/<File>.test.tsx`.
- Web UI checkbox row made editable: the draft holds the box state as the strings `"true"`/`"false"`
  (what `checkboxState` returns), so the send rule is DATA on the row, required beside `absent`
  (`onSend`/`offSend`: `fit` `"on"`/`"off"`, the flags `"true"`/`"false"`, a key whose engine default
  is on `null`/`"false"`), and `changed()` loops `checkboxRows()` like `selectRows()`. An editable box
  takes `disabled={!editable}`, writes the string on click, and needs the editable branch of
  `editableWritten` (widen it to `SelectRow | CheckboxRow`: a row whose loaded value is not the box's
  own reading, like `jinja` `0`, spells the written-as line).
- Web UI select made editable: `readAs('flag', ..)` answers `on` for ANY value outside FALSEY, so
  `selectState` must take the loaded value when it is one of the row's OWN options first, else a
  tri-state row's `default` reads as `on` and picking `default` is no change, so no PUT at all. A row
  that sends its own state (no `absent`) is one optional `ownValue?: true` on the row and one `..` arm
  in the renderer; a branch on `row.reading === 'flag'` inside `selectSend` is the shape that made
  `reasoning off` send `null` once the card grew an `auto` option.
- Web UI: an editable text field with no default box is a plain labelled `LabelField` (the Server page
  2g.146, the Chat kwargs alike), NOT a `role="group"` named by the label, and `TextField`'s `group`
  prop is dead once its read-only-only caller is gone. So a test that names a group with its label
  passes only while the field is read-only: read the brief's wording ("a plain labelled input")
  before building a group.
- Web UI: a test that saves a checkbox to `"false"` and then expects checking it again to be clean
  contradicts its own last section, which pins criterion 133 (a loaded `false` checked sends `null`):
  138's apply-after-save (`applied()` in `PresetPage.tsx`) makes the sent `"false"` the loaded value,
  so checking IS a change. PresetCards3's 133 test pins the same round trip as TWO PUTs. Report it.
- A render the manager writes beside a served file (the `presets@<id>.ini` of a server or pool write)
  shows in EVERY listing assertion of the manager's phases: survey all of them (`listing(`, `files_in(`,
  `names()`) before driving `data_root_e2e`, whose ~30 phases run in ONE `#[test]` and show one red at
  a time. The easy to miss: a phase that lists only the file it wrote (`["server@slow.ini"]` of the
  writing-lock phase), and one that lists the pair `["presets.ini", "server@default.ini"]`.
- A tester fixture that puts a served file in place with `std::fs::write` needs
  `create_dir_all(paths::engine_config_dir())` first: `redirect_for_tests` creates no directory, so the
  write answers `Os { code: 2, kind: NotFound }` and the whole `data_root_e2e` dies at phase 1.
- `checks::preset_sections` runs `preset_id` on every CHANGED section, so a pool `presets.ini` holding
  the global `[*]` section is refused (`invalid-preset-id`, `["*"]`) while `assign::render` treats that
  very section as the global one it always copies first: a gap in the design, not a coder fix. Never add
  the `*` exemption alone: the GUI and `pai` would then accept a preset named `*`, and llama.cpp
  cascades the global section onto EVERY preset (`common/preset.cpp` `cascade`, `docs/preset.md`), so
  `load-on-startup = true` there loads every model, and a render count that counts `[*]` as one section
  under-counts. Take the ruling from the architect.
- Prove a hypothesis where no one's tests live: patch the production file in the `/tmp/opencode/<x>`
  copy and run there. When the whole `data_root_e2e` goes green with that one line, that is the
  evidence the coordinator needs, and the real tree is left exactly as the tests describe.
- A tester's `for who in [&alice]` is `clippy::single_element_loop` under `--all-targets`: report it,
  and show the production code is clean with plain `cargo clippy -p <crate> -- -D warnings`.
- One more shipped engine default file is four edits and no staging code: the text goes in
  `crates/core/defaults/` (and `defaults/windows/` when Windows differs), `msi::engine_defaults()` stages
  it into `share\defaults\` and `linux::system_files::files()` into the Llama component's
  `usr/share/private-ai/llama.cpp/`, both loops are data-driven, and `components::of` routes that prefix
  already. Windows needs a `<Component>` in `installer/llama.cpp.wxs` (the defaults are
  `NeverOverwrite="yes" Permanent="yes"`, an auto GUID works when the key path is one file) plus its
  `<ComponentRef>`, and `copy_missing_defaults()` is where a default reaches `/etc/llama.cpp/`.
- `msi::needle_check_list` reads `.github/workflows/installers-windows.yml` as text and wants the
  install-check list to quote EVERY shipped default's name in single quotes; the workflow's repair step
  is a separate list. Both files are tests' fixtures: a `ci_windows.rs` step is the text between
  `- name:` lines, comment lines dropped.
- The instance unit's dependency list is pinned by name (`unit_dependencies_include_the_manager`): an
  ordering line is a plain `After=`, and `After=` alone is what the test accepts (no `Wants=`,
  no `BindsTo=`), so a unit that must start after the manager adds one line.
