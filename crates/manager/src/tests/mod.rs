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
// The Windows service plan against the instance record. It runs on every OS:
// the plan is one of windows.rs' OS-neutral decisions.
mod windows_sync;
// The GPU probe of the engine (manager/service.rs `ProbedGpus`). Its tests
// name the exe themselves and touch no `paths::`, so they run beside the
// e2e below; the ones that write a shell script are `#[cfg(unix)]`.
mod gpu_probe;

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

    // ── Manager core: SetModelProfile, the verb that writes assignments.ini ─
    crate::manager::service::manager_set_model_profile_e2e();

    // ── Manager core: the model-file check of every write that lands one ──
    crate::manager::service::manager_model_file_check_e2e();

    // ── Manager core: the node's device list (Devices and DevicesAs) ──────
    crate::manager::service::manager_devices_e2e();

    // ── Manager core: the GPU memory check of a model profile pool write ──
    crate::manager::service::manager_memory_check_e2e();

    // ── Manager core: the same check measured on the server profile's cards ─
    crate::manager::service::manager_pair_memory_check_e2e();

    // ── Manager core: SetServerProfile, the verb that writes instances.ini ─
    crate::manager::service::manager_set_server_profile_e2e();

    // ── Manager core: the tags of an instance, and the list with them ────
    crate::manager::service::manager_instance_tags_e2e();

    // ── Manager core: creating and deleting the node's server profiles ────
    crate::manager::service::manager_server_profile_crud_e2e();

    // ── Manager core: creating and removing an instance ───────────────────
    crate::manager::service::manager_instance_crud_e2e();

    // ── Manager core: instances.ini, the instance record, read-only ──────
    crate::manager::service::manager_instances_file_e2e();

    // ── Manager core: a verb's existence check reads the record ──────────
    crate::manager::service::manager_existing_id_e2e();

    // ── Manager core: the checks follow the profile an instance runs ──────
    crate::manager::service::manager_profiles_in_use_e2e();

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

    // ── Manager over D-Bus: creating and deleting the node's profiles ────
    #[cfg(target_os = "linux")]
    crate::manager::dbus::manager_dbus_server_profiles_e2e();

    // ── Manager over D-Bus: creating and removing an instance ───────────
    #[cfg(target_os = "linux")]
    crate::manager::dbus::manager_dbus_instance_crud_e2e();

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

    // ── Windows: the SCM's services follow instances.ini ─────────────────
    crate::tests::windows_sync::manager_windows_sync_e2e();
}
