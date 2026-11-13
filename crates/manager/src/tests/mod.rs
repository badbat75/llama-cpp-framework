//! The end-to-end test of this crate.
//!
//! WARNING: its phases redirect config IO by setting the process-wide
//! `PRIVATE_AI_DATA_ROOT` env var and never restore it, so they all run
//! in this ONE `#[test]`, one after the other. Unit tests elsewhere must not
//! touch `paths::` (or anything that resolves the config dir): cargo runs
//! tests on parallel threads, so they would race this one nondeterministically.

#[test]
fn data_root_e2e() {
    // ── Manager core (manager/service.rs `Manager`) ──────────────────────
    // Both seams are fakes.
    crate::manager::service::manager_e2e();

    // ── Manager core while polkit waits for a password ──────────────────
    crate::manager::service::manager_waits_e2e();

    // ── Manager over D-Bus (manager/dbus.rs `ManagerInterface`) ──────────
    // Served peer to peer on a socket pair, no bus.
    #[cfg(target_os = "linux")]
    crate::manager::dbus::manager_dbus_e2e();

    // ── Manager over D-Bus while polkit waits for a password ────────────
    #[cfg(target_os = "linux")]
    crate::manager::dbus::manager_dbus_waits_e2e();
}
