//! Cross-cutting tests of the crate.
//!
//! The phases that redirect the data tree (`paths::redirect_for_tests`, which
//! sets the process-wide `PRIVATE_AI_DATA_ROOT` and never restores it) run in
//! ONE `#[test]`, `data_root_e2e`, one after the other. Unit tests elsewhere
//! must not touch `paths::` (or anything that resolves the config dir): cargo
//! runs tests on parallel threads, so they would race this one.

mod deps_lint;
mod lock_walk;

#[test]
fn data_root_e2e() {
    // ── Login entry (startup.rs `set_enabled`, the XDG autostart file) ───
    #[cfg(target_os = "linux")]
    crate::startup::startup_e2e();

    // ── The 2.4 tree and login entry migrated (migration.rs) ─────────────
    #[cfg(target_os = "linux")]
    crate::migration::migration_e2e();

    // ── Starting and stopping through the manager (server_control.rs) ────
    crate::server_control::server_control_e2e();

    // ── The configuration files through the seam (server_control.rs) ─────
    crate::server_control::config_seam_e2e();

    // ── The sweep reads the value to restore through the seam ────────────
    crate::bench::sweep::sweep_config_e2e();

    // ── The per-user config handed to the manager (migration.rs) ─────────
    crate::migration::config_import_e2e();
}
