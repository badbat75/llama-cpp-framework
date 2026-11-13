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
