---
name: coder-playbook
description: What the coder has learned about implementing and running code safely. Use when changing production code.
---

Lessons only: the rules are in `AGENTS.md`.

- `pkill -f PATTERN` also matches the shell running it, and killed that shell. Use the task's own stop
  control, or a pattern that cannot match itself.
- When a test fails, check whether the test or the code is wrong before changing either.
- A screenshot that comes out blank proves nothing about a GUI. Check that it really shows the window.
- `llama-cpp-config` is a library (`src/lib.rs`, every module) plus two bins (`main.rs`,
  `bin/private-ai-manager.rs`). Most modules are private, so `pub` does not silence dead_code there,
  and a function only tests call fails `clippy --all-targets -D warnings` until a caller lands.
  A `pub` item of a `pub mod` (`manager`, `manager::service`) is the library's API and never dead:
  check clippy without an `#[allow(dead_code)]` before adding one.
- `binding_lint` reads the sources by file name: only the binary roots may say `cli::` (`lib.rs` may
  not: the GUI can reach it), and a key printed outside `cli.rs` and `CLI_ONLY_FNS` must be in both
  `cli.toml` and `en_US.toml`.
- A debug binary in `cargo-target/debug` finds no llama-server (`paths::llama_bin` layouts). To show a
  launcher's refusals, hardlink it into a scratch dir beside an empty, non-executable `llama-server`.
- Code comments never cite PLAN.md step numbers: they change on renumbering. Name the feature or caller instead.
- zbus (5.x, `async-io`) ticks every connection's tasks, method handlers included, on that
  connection's own executor thread. A blocking call made from a handler on the same connection
  waits for a reply that thread can never read: give outgoing calls (systemd, polkit) a connection
  of their own (`manager/dbus.rs`).
- In `manager.rs` only `main` and `run` are CLI-only for `binding_lint`: a new helper fn that prints
  a `cli.toml` key needs the key in `en_US.toml` too, so keep such printing inside `run`.
- Moving the configurator crate changes the depth of its repo-root paths: `build.rs` (`../../resources`,
  `../../build`, `../../patches`), `include_str!` of `benchmarks/` (`bench/db.rs`, `bench/spec.rs`),
  `CARGO_MANIFEST_DIR` joins (`bench/db.rs` test, `kv_cache.rs` test, `paths::bench_share_dir`'s
  `.parent()` chain) and xtask's `repo_root().join(...)` (`configurator.rs`, `licenses.rs`,
  `strings_share.rs`, `version.rs`, `linux/package.rs`). A sed on `^name = "..."` in its Cargo.toml also
  hits the `[[bin]]` entry of the same name.
