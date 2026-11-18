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

    // ── Manager core: the API key withheld from a reader without configure
    crate::manager::service::manager_withholds_api_key_e2e();

    // ── Manager core: a write that keeps the API key ────────────────────
    crate::manager::service::manager_keeps_api_key_e2e();

    // ── Manager core: the instance files are the engine's ───────────────
    crate::manager::service::manager_engine_dir_e2e();

    // ── Manager core while polkit waits for a password ──────────────────
    crate::manager::service::manager_waits_e2e();

    // ── Manager core: the content checks before a write ─────────────────
    crate::manager::service::manager_checks_e2e();

    // ── Manager core: the per-instance model profiles renders and assignments.ini ─
    crate::manager::service::manager_assignments_e2e();

    // ── Manager core: a render that cannot be written, and its report ─────
    crate::manager::service::manager_render_failure_e2e();

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

    // ── Manager core: acting for a signed-in user (the bff names a member)
    crate::manager::service::manager_principal_e2e();

    // ── Manager over D-Bus: acting for a signed-in user (the `As` methods)
    #[cfg(target_os = "linux")]
    crate::manager::dbus::manager_dbus_principal_e2e();

    // ── pai-core's D-Bus client, acting for a signed-in user ─────────────
    #[cfg(target_os = "linux")]
    dbus_client::manager_dbus_client_principal_e2e();

    // ── Manager core: the log of who acted (one record per guarded call) ──
    crate::manager::service::manager_action_log_e2e();

    // ── Manager core: a write that swaps only on the text it expected ────
    crate::manager::service::manager_write_file_if_e2e();
}
