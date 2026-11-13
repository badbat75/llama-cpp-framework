---
name: tester-playbook
description: What the tester has learned about writing reliable tests. Use when writing or debugging tests.
---

Lessons only: the rules are in `AGENTS.md`.

- Never wait without a deadline. A test polling for a value hung forever on a stub in another thread: a
  test must fail, not hang.
- Check why a test is red, or why it is already green. A validation test once passed for the wrong reason,
  because the field did not exist yet. Recheck such tests once the code exists.
- Keep one fixture per test. An id taken from one temporary fixture will not match another, and the test
  fails with the wrong error.
- Use real names and values in fixtures, captured from the real system. They find what invented data hides.
- A fake holds whatever you write, while the real system can answer differently. Do not treat a passing
  test on a fake as proof about the real thing.
- In `llama-cpp-config`, a test that needs `LLAMA_CPP_CONFIG_DATA_ROOT` (anything reading `paths::`) is not
  a `#[test]` of its own: write a `#[cfg(test)] pub(crate) fn <name>_e2e()` that sets the redirect to its
  own tempdir, and call it at the end of `editable_widgets_track_model_after_edit` in
  `src/tests/ui_bindings.rs`; filter on that name to run it. Pure helpers stay plain unit tests. Nothing
  unsets the variable safely, so the real-path branches (`/etc`, `%ProgramData%`) go untested.
- Stubs with `todo!("<fn>")` give a red that names the missing function; prefer them to a compile error,
  which hides every other test in the binary.
- `llama-cpp-config` is a binary-only crate (no `lib.rs`): tests run in the `llama-cpp-config` bin's test
  target, so logic for a second `[[bin]]` must sit in a module of the main crate (`mod x;` in `main.rs`) to
  be tested. Wiring the second binary is the coder's job.
- To pin a launch, build a `std::process::Command` and read it back with `get_program`, `get_args`,
  `get_envs` and `get_current_dir`; take the exe as a parameter, because `paths::llama_server_exe()` finds
  nothing from the test binary's directory. Compute the expected args with the existing `server_args()`
  instead of hard-coding the list.
- A single file can be formatted with `rustfmt --edition 2021 <file>`; `cargo fmt -p <crate> --check` then
  shows whether anything else is left.
- To pin a field a variant does not have yet without breaking the build, match with `Variant(p, ..)` (it
  compiles for one field or more) and check the new field through `{:?}`; tell the coder the intended
  shape. A compile error would hide every other test in the binary, the lints included.
- Since `src/lib.rs` exists, the binary roots (`main.rs`, `bin/*.rs`) are separate crates no module can
  call, while `lib.rs` is reachable from every module: a lint may exempt the former, never the latter.
- To make a write fail without a fake filesystem, chmod the directory `0o555` (Unix), probe that a write
  really fails (root ignores the mode) and put the mode back before asserting. Then check the old content
  and that the directory listing is unchanged, which also proves no temp file was left.
- A seam the code calls through `&self` (a service manager, polkit) gets a fake with `RefCell` logs read
  back through an accessor on the struct under test; assert the recorded calls are empty to pin "nothing
  happened" on a refusal, as well as the returned error kind.
- Comments never cite PLAN.md step or slice numbers: steps are renumbered and removed when done. Name
  the feature instead ("running llama-server as a service").
- To test a zbus interface without a bus (zbus 5, `zbus::blocking`): `UnixStream::pair()`, the server end
  `Builder::async_io_unix_stream(s0).server(Guid::generate())?.p2p().serve_at(..)?.build()` on its own
  thread (both ends handshake at once; plain `unix_stream` is deprecated), the client end
  `.p2p().method_timeout(..)` (the deadline: a panicking method otherwise hangs the call). A p2p call
  carries no sender; to stamp one as the bus would, `client.inner().set_unique_name(":1.42")` after the
  build (dev feature `bus-impl`). Error replies named under `org.freedesktop.DBus.Error` come back as
  `zbus::Error::FDO`, any other name as `MethodError`.
