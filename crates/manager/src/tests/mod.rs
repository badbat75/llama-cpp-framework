//! The end-to-end test of this crate.
//!
//! WARNING: its phases redirect config IO by setting the process-wide
//! `PRIVATE_AI_DATA_ROOT` env var and never restore it, so they all run
//! in this ONE `#[test]`, one after the other. Unit tests elsewhere must not
//! touch `paths::` (or anything that resolves the config dir): cargo runs
//! tests on parallel threads, so they would race this one nondeterministically.

// pai-core's D-Bus client against the interface (Linux only).
#[cfg(target_os = "linux")]
mod dbus_client;
// pai-core's stream client against the stream server (the named pipe's
// protocol), joined by a Unix socket pair.
#[cfg(unix)]
mod pipe_client;

#[test]
fn data_root_e2e() {
    // ── Manager core (manager/service.rs `Manager`) ──────────────────────
    // Both seams are fakes.
    crate::manager::service::manager_e2e();

    // ── Manager core: the instance files hold a secret ──────────────────
    crate::manager::service::manager_instance_files_e2e();

    // ── Manager core while polkit waits for a password ──────────────────
    crate::manager::service::manager_waits_e2e();

    // ── Manager core: the content checks before a write ─────────────────
    crate::manager::service::manager_checks_e2e();

    // ── Manager over D-Bus (manager/dbus.rs `ManagerInterface`) ──────────
    // Served peer to peer on a socket pair, no bus.
    #[cfg(target_os = "linux")]
    crate::manager::dbus::manager_dbus_e2e();

    // ── Manager over D-Bus while polkit waits for a password ────────────
    #[cfg(target_os = "linux")]
    crate::manager::dbus::manager_dbus_waits_e2e();

    // ── Manager over D-Bus: a refused write's key and values ─────────────
    #[cfg(target_os = "linux")]
    crate::manager::dbus::manager_dbus_checks_e2e();

    // ── pai-core's client of the manager over D-Bus ─────────────────────
    // pai_core::client::dbus against the interface, peer to peer, no bus.
    #[cfg(target_os = "linux")]
    dbus_client::manager_dbus_client_e2e();

    // ── pai-core's client of the manager over a stream ──────────────────
    // pai_core::client::pipe against manager::pipe::serve_stream, on a
    // Unix socket pair (the named pipe's OS-neutral protocol).
    #[cfg(unix)]
    pipe_client::manager_pipe_client_e2e();
}
