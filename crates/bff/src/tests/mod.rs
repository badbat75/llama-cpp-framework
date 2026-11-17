//! Cross-cutting tests of the crate: the lock lint and the manager's time
//! limits. The HTTP routes, the listen address and the executable are tested
//! through the public API in `tests/`.

mod deps_lint;
mod timeouts;
