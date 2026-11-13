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
  the `private-ai-manager` bin, core only) and `config` (`pai-config`, GUI + CLI). The config and manager
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
- Each crate's tests that set `LLAMA_CPP_CONFIG_DATA_ROOT` run as phases of ONE `#[test]` in its
  `src/tests/mod.rs` (`data_root_e2e` in core and manager, the UI test in config).
- Moving a crate changes the depth of its repo-root paths: `build.rs` (`../../resources`,
  `../../build`, `../../patches`), `include_str!` of `benchmarks/` (`bench/db.rs`, `bench/spec.rs`),
  `CARGO_MANIFEST_DIR` joins (`bench/db.rs` test, `kv_cache.rs` test, `paths::bench_share_dir`'s
  `.parent()` chain) and xtask's `repo_root().join(...)` (`configurator.rs`, `licenses.rs`,
  `strings_share.rs`, `version.rs`, `linux/package.rs`). A sed on `^name = "..."` in its Cargo.toml also
  hits the `[[bin]]` entry of the same name.
