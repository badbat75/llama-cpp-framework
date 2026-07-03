// End-to-end / cross-cutting tests that don't belong to any single module.
//
// Kept here (under src/, as `#[cfg(test)] mod tests` from main.rs) rather than in
// a top-level `tests/` directory: a top-level dir would compile as a separate
// integration crate that can only see the binary's *public* API, which a binary
// crate doesn't expose — so these would need a lib/bin split. As internal test
// modules they reach `crate::…` directly and use dev-dependencies normally.
//
// Per-module unit tests stay inline (a `#[cfg(test)] mod tests` at the bottom of
// each source file); only tests that span modules or need a built `AppWindow`
// live here.

// NOTE: all e2e phases run inside ui_bindings' single #[test] (the Slint
// testing backend is a process-global, single-threaded platform); save_flow
// exposes a plain fn called from there.
mod save_flow;
mod ui_bindings;
