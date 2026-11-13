---
name: coder-playbook
description: What the coder has learned about implementing and running code safely. Use when changing production code.
---

Lessons only: the rules are in `AGENTS.md`.

- `pkill -f PATTERN` also matches the shell running it, and killed that shell. Use the task's own stop
  control, or a pattern that cannot match itself.
- When a test fails, check whether the test or the code is wrong before changing either.
- A screenshot that comes out blank proves nothing about a GUI. Check that it really shows the window.
- The framework is three crates under `crates/`: `core` (`pai-core`, no GUI), `manager` (`pai-manager`,
  the `pai-manager` bin, core only) and `config` (`pai-config`, GUI + CLI). The config and manager
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
  of their own (`manager/dbus.rs`).
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
