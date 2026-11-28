//! Cross-cutting tests of the crate.
//!
//! The phases that redirect the data tree (`paths::redirect_for_tests`, which
//! sets the process-wide `PRIVATE_AI_DATA_ROOT` and never restores it) each run
//! through `pai_core::isolated::isolated` (ARCHITECTURE.md "Isolated data-root
//! tests (TS2)"): the `#[test]` re-runs itself in a child process of this test
//! binary, the child redirects the data root to a temp dir of its own and runs
//! the phase there, and the parent fails unless the child passed. libtest runs
//! the parents in parallel, so each phase runs alone, in its own process, under
//! a data root of its own, and a phase that needs a file reads one its own
//! setup wrote. The path `isolated` is given is the path libtest names the test
//! by, so it is `tests::<name>` here. Unit tests elsewhere must not touch
//! `paths::` (or anything that resolves the config dir): cargo runs them on
//! parallel threads beside these tests, and the redirect of a phase's child is
//! process-wide.
use pai_core::isolated::isolated;

mod deps_lint;
// The `model-profile` phases, run as isolated tests below.
mod model_profile;
mod split_lint;

/// The text scan shared with pai-config's lint of the GUI's stop worker
/// (`crates/app/src/tests/gate_lint.rs`), used by `cli.rs`'s lint of
/// `snapshot_before_stop`.
#[path = "../../../app/src/tests/gate_lint.rs"]
pub(crate) mod gate_lint;

#[test]
fn cli_control_e2e() {
    // ── The `control` commands on the manager backend (cli.rs) ───────────
    isolated("tests::cli_control_e2e", crate::cli::cli_control_e2e);
}

#[test]
fn cli_config_e2e() {
    // ── `server set` and `model-profile enable` through the seam (cli.rs) ─
    isolated("tests::cli_config_e2e", crate::cli::cli_config_e2e);
}

#[test]
fn cli_server_checks_e2e() {
    // ── `server set` refuses what the Server tab refuses (cli.rs) ────────
    isolated(
        "tests::cli_server_checks_e2e",
        crate::cli::cli_server_checks_e2e,
    );
}

#[test]
fn cli_import_e2e() {
    // ── The per-user config handed to the manager (cli.rs) ───────────────
    isolated("tests::cli_import_e2e", crate::cli::cli_import_e2e);
}

#[test]
fn cli_server_profile_e2e() {
    // ── The server file is the profile the record names (cli.rs) ─────────
    isolated(
        "tests::cli_server_profile_e2e",
        crate::cli::cli_server_profile_e2e,
    );
}

#[test]
fn model_profile_commands_e2e() {
    // ── `model-profile delete|disable|enable` on the test backend ────────
    isolated(
        "tests::model_profile_commands_e2e",
        model_profile::model_profile_commands_e2e,
    );
}

#[test]
fn model_profile_delete_files_e2e() {
    // ── `model-profile delete --delete-files` on disk (cli.rs, linked_files)
    isolated(
        "tests::model_profile_delete_files_e2e",
        model_profile::model_profile_delete_files_e2e,
    );
}
