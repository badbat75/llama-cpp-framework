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
mod lock_walk;

#[cfg(target_os = "linux")]
#[test]
fn startup_e2e() {
    // ── Login entry (startup.rs `set_enabled`, the XDG autostart file) ───
    isolated("tests::startup_e2e", crate::startup::startup_e2e);
}

#[cfg(target_os = "linux")]
#[test]
fn migration_e2e() {
    // ── The 2.4 tree and login entry migrated (migration.rs) ─────────────
    isolated("tests::migration_e2e", crate::migration::migration_e2e);
}

#[test]
fn server_control_e2e() {
    // ── Starting and stopping through the manager (server_control.rs) ────
    isolated(
        "tests::server_control_e2e",
        crate::server_control::server_control_e2e,
    );
}

#[test]
fn config_seam_e2e() {
    // ── The configuration files through the seam (server_control.rs) ─────
    isolated(
        "tests::config_seam_e2e",
        crate::server_control::config_seam_e2e,
    );
}

#[test]
fn model_profile_checks_every_backend_e2e() {
    // ── The model profile checks on every backend (model_profiles.rs) ──────────────────
    isolated(
        "tests::model_profile_checks_every_backend_e2e",
        crate::model_profiles::model_profile_checks_every_backend_e2e,
    );
}

#[test]
fn hf_keys_survive_a_save_e2e() {
    // ── A Hugging Face model profile survives a save (model_profiles.rs) ──
    isolated(
        "tests::hf_keys_survive_a_save_e2e",
        crate::model_profiles::hf_keys_survive_a_save_e2e,
    );
}

#[test]
fn api_key_kept_e2e() {
    // ── A save after the manager withheld the API key ───────────────────
    isolated(
        "tests::api_key_kept_e2e",
        crate::server_control::api_key_kept_e2e,
    );
}

#[test]
fn sweep_config_e2e() {
    // ── The sweep reads the value to restore through the seam ────────────
    isolated(
        "tests::sweep_config_e2e",
        crate::bench::sweep::sweep_config_e2e,
    );
}

#[test]
fn config_import_e2e() {
    // ── The per-user config handed to the manager (migration.rs) ─────────
    isolated(
        "tests::config_import_e2e",
        crate::migration::config_import_e2e,
    );
}

#[test]
fn restore_reads_the_instance_state_dir_e2e() {
    // ── The restore reads the instance's state folder (slot_state.rs) ────
    isolated(
        "tests::restore_reads_the_instance_state_dir_e2e",
        crate::slot_state::restore_reads_the_instance_state_dir_e2e,
    );
}

#[test]
fn api_key_withheld_e2e() {
    // ── opencode.json's apiKey when the manager withheld it (integrations.rs)
    isolated(
        "tests::api_key_withheld_e2e",
        crate::integrations::api_key_withheld_e2e,
    );
}

#[test]
fn sweep_checks_values_with_the_save_e2e() {
    // ── The sweep's up-front check runs the save's checks (bench/sweep.rs)
    isolated(
        "tests::sweep_checks_values_with_the_save_e2e",
        crate::bench::sweep::sweep_checks_values_with_the_save_e2e,
    );
}
