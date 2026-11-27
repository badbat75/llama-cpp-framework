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
// The Vulkan child of slice AN2 (manager/service.rs `VulkanProbe`). Same
// shape: the exe is a script the test names, no `paths::`, scripts `#[cfg(unix)]`.
mod vulkan_probe;
// The Vulkan child's extension and version plan (manager/vulkan_child.rs
// `chain_plan`): a pure decision, no loader opened, so it runs on every OS.
mod vulkan_chain;
// The node's own info of slice AN3 (manager/service.rs `node_info`): its gate
// and its document, every figure from a fake seam. No `paths::`, so these plain
// `#[test]`s run beside the e2e below.
mod node_info;
// GT5a's Windows rule (ARCHITECTURE.md "ROCm instances load models with
// `load-mode = none`"): a text check of the production sources, so it runs on
// every OS and touches no `paths::`.
mod gt5a_windows_gate;
// IP3b (ARCHITECTURE.md "Instance profiles", slice IP3 part b): the entry-key
// save, the verbs that answer the running instances, `CreateModelProfile` into
// `models.ini` with the stay keys only, and the retirement of the assignment
// record. Every phase redirects the data root, so they run in the one test
// below, in order.
mod instance_profile_entries;

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

    // ── Manager core: the device bound for device-draft and mmproj-device ──
    crate::manager::service::manager_device_bound_check_e2e();

    // ── Manager core: the layer facts of a saved model profile (ModelInfo) ──
    // After the memory checks: those pin `estimated_of`, whose resolution the
    // read shares with this one, so the e2e proves them before this phase ends.
    crate::manager::service::manager_model_info_e2e();

    // ── Manager core: the tensor list of the model info reply (J1) ───────
    crate::manager::service::manager_model_tensors_e2e();

    // ── Manager core: the node's models folder listing (ModelFiles, M3) ──
    crate::manager::service::manager_model_files_e2e();

    // ── Manager core: the models walk's bound of entries visited (M3 G2) ──
    crate::manager::service::manager_model_files_walk_bound_e2e();

    // ── Manager core: SetServerProfile, the verb that writes instances.ini ─
    crate::manager::service::manager_set_server_profile_e2e();

    // ── Manager core: the tags of an instance, and the list with them ────
    crate::manager::service::manager_instance_tags_e2e();

    // ── Manager core: the label of an instance (IL1) ──────────────────────
    crate::manager::service::manager_instance_label_e2e();

    // ── Manager core: creating and deleting the node's server profiles ────
    crate::manager::service::manager_server_profile_crud_e2e();

    // ── Manager core: creating and cloning a model profile ────────────────
    crate::manager::service::manager_model_profile_create_e2e();

    // ── Manager core: deleting a model profile ────────────────────────────
    crate::manager::service::manager_model_profile_delete_e2e();

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

    // ── Manager core: a ROCm instance's render loads with `load-mode = none` ─
    // (GT5a, ARCHITECTURE.md "ROCm instances load models with `load-mode =
    // none`".) Linux only: the render's rule is Linux, and the Windows rule is
    // the cfg lint of `gt5a_windows_gate` below.
    #[cfg(not(windows))]
    crate::manager::service::manager_gt5a_rocm_load_mode_e2e();

    // ── Manager core: IP2, the migration of the model data model at start ─
    crate::manager::service::manager_ip2_migration_e2e();

    // ── Manager core: IP2, the resolution through the record after it ────
    crate::manager::service::manager_ip2_resolution_e2e();

    // ── Manager core: IP2, the stray assignments.ini deleted at start ────
    crate::manager::service::manager_ip2_stray_assignments_e2e();

    // ── Manager core: IP2, the refusals of the gone files and moved keys ──
    crate::manager::service::manager_ip2_write_refusals_e2e();

    // ── Manager core: IP2 review D1, the backups once and of the old shape ─
    crate::manager::service::manager_ip2_backups_once_e2e();

    // ── Manager core: IP2 review D2, a profile's port from the record ─────
    crate::manager::service::manager_ip2_server_profile_port_from_record_e2e();

    // ── Manager core: IP2 review D3, no backup at its final name until whole ─
    crate::manager::service::manager_ip2_backup_atomic_e2e();

    // ── Manager core: IP2 review D4a, a start after a crash mid-migration ──
    crate::manager::service::manager_ip2_crash_mid_migration_e2e();

    // ── Manager core: IP2 review D4b, a failed migration serves the old tree ─
    crate::manager::service::manager_ip2_failed_migration_serves_e2e();

    // ── Manager core: IP2 review D5, the render of an instance given nothing ─
    crate::manager::service::manager_ip2_empty_render_e2e();

    // ── Manager core: IP3a, the instance profile verbs ────────────────────
    // (ARCHITECTURE.md "Instance profiles", slice IP3 part a.)
    crate::manager::service::manager_ip3a_instance_profiles_e2e();

    // ── Manager core: IP3a review D1, the empty host judged on the profile ─
    crate::manager::service::manager_ip3a_fix_empty_host_address_e2e();

    // ── Manager core: IP3a review D2a, the port of a recordless instance ───
    crate::manager::service::manager_ip3a_fix_port_of_recordless_e2e();

    // ── Manager core: IP3a review D2b, the web service's port ─────────────
    crate::manager::service::manager_ip3a_fix_port_of_bff_e2e();

    // ── Manager core: IP3a review D3, the port range walked out ───────────
    crate::manager::service::manager_ip3a_fix_no_free_port_e2e();

    // ── Manager core: IP3a review D4, a host carrying a control char ──────
    crate::manager::service::manager_ip3a_fix_host_control_char_e2e();

    // ── Manager core: IP3a review D5, the principal gate of the write verbs ─
    crate::manager::service::manager_ip3a_fix_principal_gate_e2e();

    // ── Manager core: IP3b 1, `CreateModelProfile` into `models.ini` ──────
    // (ARCHITECTURE.md "Instance profiles", slice IP3 part b: the pool, and the
    // stay keys only of a copy.)
    instance_profile_entries::ip3b_create_model_profile_e2e();

    // ── Manager core: IP3b 2, the entry-key save and its render ──────────
    instance_profile_entries::ip3b_entry_save_e2e();

    // ── Manager core: IP3b 3, the entry save's refusals ──────────────────
    instance_profile_entries::ip3b_entry_save_refusals_e2e();

    // ── Manager core: IP3b 4, the entry add and remove answer the running ─
    instance_profile_entries::ip3b_add_remove_e2e();

    // ── Manager core: IP3b 5, the server profile switch of a profile ─────
    instance_profile_entries::ip3b_server_switch_e2e();

    // ── Manager core: IP3b 6, the memory check measures the entry keys ───
    instance_profile_entries::ip3b_entry_memory_e2e();

    // ── Manager core: IP3b 6b, the same check never refuses a shrink ─────
    instance_profile_entries::ip3b_entry_memory_shrink_e2e();

    // ── Manager core: IP3b 7, one pool file, no assignment record ────────
    instance_profile_entries::ip3b_pool_is_one_file_e2e();

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

    // ── Manager over D-Bus: creating and deleting a model profile ────────
    #[cfg(target_os = "linux")]
    crate::manager::dbus::manager_dbus_model_profiles_e2e();

    // ── Manager over D-Bus: creating and removing an instance ───────────
    #[cfg(target_os = "linux")]
    crate::manager::dbus::manager_dbus_instance_crud_e2e();

    // ── Manager over D-Bus: the instance profile verbs (IP3a) ───────────
    #[cfg(target_os = "linux")]
    crate::manager::dbus::manager_dbus_ip3a_instance_profiles_e2e();

    // ── pai-core's client of the manager over D-Bus ─────────────────────
    // pai_core::client::dbus against the interface, peer to peer, no bus.
    #[cfg(target_os = "linux")]
    dbus_client::manager_dbus_client_e2e();

    // ── pai-core's client: creating and deleting a model profile ─────────
    #[cfg(target_os = "linux")]
    dbus_client::manager_dbus_client_model_profiles_e2e();

    // ── pai-core's client: a member's model profile create and delete ────
    #[cfg(target_os = "linux")]
    dbus_client::manager_dbus_client_model_profiles_as_e2e();

    // ── pai-core's client: the instance profile verbs (IP3a) ────────────
    #[cfg(target_os = "linux")]
    dbus_client::manager_dbus_client_instance_profiles_e2e();

    // ── pai-core's client: a member's instance profile verbs (IP3a) ─────
    #[cfg(target_os = "linux")]
    dbus_client::manager_dbus_client_instance_profiles_as_e2e();

    // ── pai-core's client: the node's models folder over the bus (M3 G1) ──
    #[cfg(target_os = "linux")]
    dbus_client::manager_dbus_client_model_files_e2e();

    // ── pai-core's client of the manager over a stream ──────────────────
    // pai_core::client::pipe against manager::pipe::serve_stream, on a
    // Unix socket pair (the named pipe's OS-neutral protocol).
    #[cfg(unix)]
    pipe_client::manager_pipe_client_e2e();

    // ── pai-core's client over a stream: a model profile's life ─────────
    #[cfg(unix)]
    pipe_client::manager_pipe_client_model_profiles_e2e();

    // ── pai-core's client over a stream: the instance profiles (IP3a) ───
    #[cfg(unix)]
    pipe_client::manager_pipe_client_instance_profiles_e2e();

    // ── pai-core's client: the node's models folder over a stream (M3 G1) ─
    #[cfg(unix)]
    pipe_client::manager_pipe_client_model_files_e2e();

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
