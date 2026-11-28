//! The end-to-end tests of this crate: one `#[test]` per phase (TS2b).
//!
//! Every phase runs through `pai_core::isolated::isolated` (ARCHITECTURE.md
//! "Isolated data-root tests (TS2)"): the `#[test]` re-runs itself in a child
//! process of this test binary, the child redirects the process-wide
//! `PRIVATE_AI_DATA_ROOT` to a temp dir of its own and runs the phase there,
//! and the parent fails unless the child passed. libtest runs the parents in
//! parallel, so each phase runs alone, in its own process, under a data root
//! of its own, and a phase that needs a file reads one its own setup wrote.
//! The path `isolated` is given is the path libtest names the test by, so it
//! is `tests::<name>` here. Unit tests elsewhere must not touch `paths::` (or
//! anything that resolves the config dir): cargo runs them on parallel threads
//! beside these tests, and the redirect of a phase's child is process-wide.
use pai_core::isolated::isolated;

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
// isolated phases below; the ones that write a shell script are `#[cfg(unix)]`.
mod gpu_probe;
// The Vulkan child of slice AN2 (manager/service.rs `VulkanProbe`). Same
// shape: the exe is a script the test names, no `paths::`, scripts `#[cfg(unix)]`.
mod vulkan_probe;
// The Vulkan child's extension and version plan (manager/vulkan_child.rs
// `chain_plan`): a pure decision, no loader opened, so it runs on every OS.
mod vulkan_chain;
// The node's own info of slice AN3 (manager/service.rs `node_info`): its gate
// and its document, every figure from a fake seam. No `paths::`, so these plain
// `#[test]`s run beside the isolated phases below.
mod node_info;
// GT5a's Windows rule (ARCHITECTURE.md "ROCm instances load models with
// `load-mode = none`"): a text check of the production sources, so it runs on
// every OS and touches no `paths::`.
mod gt5a_windows_gate;
// IP3b (ARCHITECTURE.md "Instance profiles", slice IP3 part b): the entry-key
// save, the verbs that answer the running instances, `CreateModelProfile` into
// `models.ini` with the stay keys only, and the retirement of the assignment
// record. Every phase redirects the data root, so each runs as an isolated
// test below.
mod instance_profile_entries;
// IP3c (ARCHITECTURE.md "Convergence: idempotent apply, drift and the last good
// render"): the fit split judged per instance, the rendered pair as the last good
// state, the drift of a running instance, and the rows that report both. The rows
// are read over the stream, so this module is Unix, as `pipe_client` is.
#[cfg(unix)]
mod convergence;

#[test]
fn manager_e2e() {
    // ── Manager core (manager/service.rs `Manager`) ──────────────────────
    // Both seams are fakes.
    isolated("tests::manager_e2e", crate::manager::service::manager_e2e);
}

#[test]
fn manager_instance_files_e2e() {
    // ── Manager core: the instance files hold a secret ──────────────────
    isolated(
        "tests::manager_instance_files_e2e",
        crate::manager::service::manager_instance_files_e2e,
    );
}

#[test]
fn manager_withholds_api_key_e2e() {
    // ── Manager core: the API key withheld from a reader without configure
    isolated(
        "tests::manager_withholds_api_key_e2e",
        crate::manager::service::manager_withholds_api_key_e2e,
    );
}

#[test]
fn manager_keeps_api_key_e2e() {
    // ── Manager core: a write that keeps the API key ────────────────────
    isolated(
        "tests::manager_keeps_api_key_e2e",
        crate::manager::service::manager_keeps_api_key_e2e,
    );
}

#[test]
fn manager_engine_dir_e2e() {
    // ── Manager core: the instance files are the engine's ───────────────
    isolated(
        "tests::manager_engine_dir_e2e",
        crate::manager::service::manager_engine_dir_e2e,
    );
}

#[test]
fn manager_waits_e2e() {
    // ── Manager core while polkit waits for a password ──────────────────
    isolated(
        "tests::manager_waits_e2e",
        crate::manager::service::manager_waits_e2e,
    );
}

#[test]
fn manager_checks_e2e() {
    // ── Manager core: the content checks before a write ─────────────────
    isolated(
        "tests::manager_checks_e2e",
        crate::manager::service::manager_checks_e2e,
    );
}

#[test]
fn manager_assignments_e2e() {
    // ── Manager core: the per-instance model profiles renders and assignments.ini ─
    isolated(
        "tests::manager_assignments_e2e",
        crate::manager::service::manager_assignments_e2e,
    );
}

#[test]
fn manager_set_model_profile_e2e() {
    // ── Manager core: SetModelProfile, the verb that writes assignments.ini ─
    isolated(
        "tests::manager_set_model_profile_e2e",
        crate::manager::service::manager_set_model_profile_e2e,
    );
}

#[test]
fn manager_model_file_check_e2e() {
    // ── Manager core: the model-file check of every write that lands one ──
    isolated(
        "tests::manager_model_file_check_e2e",
        crate::manager::service::manager_model_file_check_e2e,
    );
}

#[test]
fn manager_devices_e2e() {
    // ── Manager core: the node's device list (Devices and DevicesAs) ──────
    isolated(
        "tests::manager_devices_e2e",
        crate::manager::service::manager_devices_e2e,
    );
}

#[test]
fn manager_memory_check_e2e() {
    // ── Manager core: the GPU memory check of a model profile pool write ──
    isolated(
        "tests::manager_memory_check_e2e",
        crate::manager::service::manager_memory_check_e2e,
    );
}

#[test]
fn manager_pair_memory_check_e2e() {
    // ── Manager core: the same check measured on the server profile's cards ─
    isolated(
        "tests::manager_pair_memory_check_e2e",
        crate::manager::service::manager_pair_memory_check_e2e,
    );
}

#[test]
fn manager_device_bound_check_e2e() {
    // ── Manager core: the device bound for device-draft and mmproj-device ──
    isolated(
        "tests::manager_device_bound_check_e2e",
        crate::manager::service::manager_device_bound_check_e2e,
    );
}

#[test]
fn manager_model_info_e2e() {
    // ── Manager core: the layer facts of a saved model profile (ModelInfo) ──
    // The memory checks pin `estimated_of`, whose resolution the read shares
    // with this one; each phase proves its own, under a data root of its own.
    isolated(
        "tests::manager_model_info_e2e",
        crate::manager::service::manager_model_info_e2e,
    );
}

#[test]
fn manager_model_tensors_e2e() {
    // ── Manager core: the tensor list of the model info reply (J1) ───────
    isolated(
        "tests::manager_model_tensors_e2e",
        crate::manager::service::manager_model_tensors_e2e,
    );
}

#[test]
fn manager_model_files_e2e() {
    // ── Manager core: the node's models folder listing (ModelFiles, M3) ──
    isolated(
        "tests::manager_model_files_e2e",
        crate::manager::service::manager_model_files_e2e,
    );
}

#[test]
fn manager_model_files_walk_bound_e2e() {
    // ── Manager core: the models walk's bound of entries visited (M3 G2) ──
    isolated(
        "tests::manager_model_files_walk_bound_e2e",
        crate::manager::service::manager_model_files_walk_bound_e2e,
    );
}

#[test]
fn manager_set_server_profile_e2e() {
    // ── Manager core: SetServerProfile, the verb that writes instances.ini ─
    isolated(
        "tests::manager_set_server_profile_e2e",
        crate::manager::service::manager_set_server_profile_e2e,
    );
}

#[test]
fn manager_instance_tags_e2e() {
    // ── Manager core: the tags of an instance, and the list with them ────
    isolated(
        "tests::manager_instance_tags_e2e",
        crate::manager::service::manager_instance_tags_e2e,
    );
}

#[test]
fn manager_instance_label_e2e() {
    // ── Manager core: the label of an instance (IL1) ──────────────────────
    isolated(
        "tests::manager_instance_label_e2e",
        crate::manager::service::manager_instance_label_e2e,
    );
}

#[test]
fn manager_server_profile_crud_e2e() {
    // ── Manager core: creating and deleting the node's server profiles ────
    isolated(
        "tests::manager_server_profile_crud_e2e",
        crate::manager::service::manager_server_profile_crud_e2e,
    );
}

#[test]
fn manager_model_profile_create_e2e() {
    // ── Manager core: creating and cloning a model profile ────────────────
    isolated(
        "tests::manager_model_profile_create_e2e",
        crate::manager::service::manager_model_profile_create_e2e,
    );
}

#[test]
fn manager_model_profile_delete_e2e() {
    // ── Manager core: deleting a model profile ────────────────────────────
    isolated(
        "tests::manager_model_profile_delete_e2e",
        crate::manager::service::manager_model_profile_delete_e2e,
    );
}

#[test]
fn manager_instance_crud_e2e() {
    // ── Manager core: creating and removing an instance ───────────────────
    isolated(
        "tests::manager_instance_crud_e2e",
        crate::manager::service::manager_instance_crud_e2e,
    );
}

#[test]
fn manager_instances_file_e2e() {
    // ── Manager core: instances.ini, the instance record, read-only ──────
    isolated(
        "tests::manager_instances_file_e2e",
        crate::manager::service::manager_instances_file_e2e,
    );
}

#[test]
fn manager_existing_id_e2e() {
    // ── Manager core: a verb's existence check reads the record ──────────
    isolated(
        "tests::manager_existing_id_e2e",
        crate::manager::service::manager_existing_id_e2e,
    );
}

#[test]
fn manager_profiles_in_use_e2e() {
    // ── Manager core: the checks follow the profile an instance runs ──────
    isolated(
        "tests::manager_profiles_in_use_e2e",
        crate::manager::service::manager_profiles_in_use_e2e,
    );
}

#[test]
fn manager_render_failure_e2e() {
    // ── Manager core: a render that cannot be written, and its report ─────
    isolated(
        "tests::manager_render_failure_e2e",
        crate::manager::service::manager_render_failure_e2e,
    );
}

#[cfg(not(windows))]
#[test]
fn manager_gt5a_rocm_load_mode_e2e() {
    // ── Manager core: a ROCm instance's render loads with `load-mode = none` ─
    // (GT5a, ARCHITECTURE.md "ROCm instances load models with `load-mode =
    // none`".) Linux only: the render's rule is Linux, and the Windows rule is
    // the cfg lint of `gt5a_windows_gate`.
    isolated(
        "tests::manager_gt5a_rocm_load_mode_e2e",
        crate::manager::service::manager_gt5a_rocm_load_mode_e2e,
    );
}

#[test]
fn manager_ip2_migration_e2e() {
    // ── Manager core: IP2, the migration of the model data model at start ─
    isolated(
        "tests::manager_ip2_migration_e2e",
        crate::manager::service::manager_ip2_migration_e2e,
    );
}

#[test]
fn manager_ip2_resolution_e2e() {
    // ── Manager core: IP2, the resolution through the record after it ────
    isolated(
        "tests::manager_ip2_resolution_e2e",
        crate::manager::service::manager_ip2_resolution_e2e,
    );
}

#[test]
fn manager_ip2_stray_assignments_e2e() {
    // ── Manager core: IP2, the stray assignments.ini deleted at start ────
    isolated(
        "tests::manager_ip2_stray_assignments_e2e",
        crate::manager::service::manager_ip2_stray_assignments_e2e,
    );
}

#[test]
fn manager_ip2_write_refusals_e2e() {
    // ── Manager core: IP2, the refusals of the gone files and moved keys ──
    isolated(
        "tests::manager_ip2_write_refusals_e2e",
        crate::manager::service::manager_ip2_write_refusals_e2e,
    );
}

#[test]
fn manager_ip2_backups_once_e2e() {
    // ── Manager core: IP2 review D1, the backups once and of the old shape ─
    isolated(
        "tests::manager_ip2_backups_once_e2e",
        crate::manager::service::manager_ip2_backups_once_e2e,
    );
}

#[test]
fn manager_ip2_server_profile_port_from_record_e2e() {
    // ── Manager core: IP2 review D2, a profile's port from the record ─────
    isolated(
        "tests::manager_ip2_server_profile_port_from_record_e2e",
        crate::manager::service::manager_ip2_server_profile_port_from_record_e2e,
    );
}

#[test]
fn manager_ip2_backup_atomic_e2e() {
    // ── Manager core: IP2 review D3, no backup at its final name until whole ─
    isolated(
        "tests::manager_ip2_backup_atomic_e2e",
        crate::manager::service::manager_ip2_backup_atomic_e2e,
    );
}

#[test]
fn manager_ip2_crash_mid_migration_e2e() {
    // ── Manager core: IP2 review D4a, a start after a crash mid-migration ──
    isolated(
        "tests::manager_ip2_crash_mid_migration_e2e",
        crate::manager::service::manager_ip2_crash_mid_migration_e2e,
    );
}

#[test]
fn manager_ip2_failed_migration_serves_e2e() {
    // ── Manager core: IP2 review D4b, a failed migration serves the old tree ─
    isolated(
        "tests::manager_ip2_failed_migration_serves_e2e",
        crate::manager::service::manager_ip2_failed_migration_serves_e2e,
    );
}

#[test]
fn manager_ip2_empty_render_e2e() {
    // ── Manager core: IP2 review D5, the render of an instance given nothing ─
    isolated(
        "tests::manager_ip2_empty_render_e2e",
        crate::manager::service::manager_ip2_empty_render_e2e,
    );
}

#[test]
fn manager_ip3a_instance_profiles_e2e() {
    // ── Manager core: IP3a, the instance profile verbs ────────────────────
    // (ARCHITECTURE.md "Instance profiles", slice IP3 part a.)
    isolated(
        "tests::manager_ip3a_instance_profiles_e2e",
        crate::manager::service::manager_ip3a_instance_profiles_e2e,
    );
}

#[test]
fn manager_ip3a_fix_empty_host_address_e2e() {
    // ── Manager core: IP3a review D1, the empty host judged on the profile ─
    isolated(
        "tests::manager_ip3a_fix_empty_host_address_e2e",
        crate::manager::service::manager_ip3a_fix_empty_host_address_e2e,
    );
}

#[test]
fn manager_ip3a_fix_port_of_recordless_e2e() {
    // ── Manager core: IP3a review D2a, the port of a recordless instance ───
    isolated(
        "tests::manager_ip3a_fix_port_of_recordless_e2e",
        crate::manager::service::manager_ip3a_fix_port_of_recordless_e2e,
    );
}

#[test]
fn manager_ip3a_fix_port_of_bff_e2e() {
    // ── Manager core: IP3a review D2b, the web service's port ─────────────
    isolated(
        "tests::manager_ip3a_fix_port_of_bff_e2e",
        crate::manager::service::manager_ip3a_fix_port_of_bff_e2e,
    );
}

#[test]
fn manager_ip3a_fix_no_free_port_e2e() {
    // ── Manager core: IP3a review D3, the port range walked out ───────────
    isolated(
        "tests::manager_ip3a_fix_no_free_port_e2e",
        crate::manager::service::manager_ip3a_fix_no_free_port_e2e,
    );
}

#[test]
fn manager_ip3a_fix_host_control_char_e2e() {
    // ── Manager core: IP3a review D4, a host carrying a control char ──────
    isolated(
        "tests::manager_ip3a_fix_host_control_char_e2e",
        crate::manager::service::manager_ip3a_fix_host_control_char_e2e,
    );
}

#[test]
fn manager_ip3a_fix_principal_gate_e2e() {
    // ── Manager core: IP3a review D5, the principal gate of the write verbs ─
    isolated(
        "tests::manager_ip3a_fix_principal_gate_e2e",
        crate::manager::service::manager_ip3a_fix_principal_gate_e2e,
    );
}

#[test]
fn ip3b_create_model_profile_e2e() {
    // ── Manager core: IP3b 1, `CreateModelProfile` into `models.ini` ──────
    // (ARCHITECTURE.md "Instance profiles", slice IP3 part b: the pool, and the
    // stay keys only of a copy.)
    isolated(
        "tests::ip3b_create_model_profile_e2e",
        instance_profile_entries::ip3b_create_model_profile_e2e,
    );
}

#[test]
fn ip3b_entry_save_e2e() {
    // ── Manager core: IP3b 2, the entry-key save and its render ──────────
    isolated(
        "tests::ip3b_entry_save_e2e",
        instance_profile_entries::ip3b_entry_save_e2e,
    );
}

#[test]
fn ip3b_entry_save_refusals_e2e() {
    // ── Manager core: IP3b 3, the entry save's refusals ──────────────────
    isolated(
        "tests::ip3b_entry_save_refusals_e2e",
        instance_profile_entries::ip3b_entry_save_refusals_e2e,
    );
}

#[test]
fn ip3b_add_remove_e2e() {
    // ── Manager core: IP3b 4, the entry add and remove answer the running ─
    isolated(
        "tests::ip3b_add_remove_e2e",
        instance_profile_entries::ip3b_add_remove_e2e,
    );
}

#[test]
fn ip3b_server_switch_e2e() {
    // ── Manager core: IP3b 5, the server profile switch of a profile ─────
    isolated(
        "tests::ip3b_server_switch_e2e",
        instance_profile_entries::ip3b_server_switch_e2e,
    );
}

#[test]
fn ip3b_entry_memory_e2e() {
    // ── Manager core: IP3b 6, the memory check measures the entry keys ───
    isolated(
        "tests::ip3b_entry_memory_e2e",
        instance_profile_entries::ip3b_entry_memory_e2e,
    );
}

#[test]
fn ip3b_entry_memory_shrink_e2e() {
    // ── Manager core: IP3b 6b, the same check never refuses a shrink ─────
    isolated(
        "tests::ip3b_entry_memory_shrink_e2e",
        instance_profile_entries::ip3b_entry_memory_shrink_e2e,
    );
}

#[test]
fn ip3b_pool_is_one_file_e2e() {
    // ── Manager core: IP3b 7, one pool file, no assignment record ────────
    isolated(
        "tests::ip3b_pool_is_one_file_e2e",
        instance_profile_entries::ip3b_pool_is_one_file_e2e,
    );
}

#[cfg(unix)]
#[test]
fn ip3c_fit_split_e2e() {
    // ── Manager core: IP3c 1, the fit split, the rendered pair, the intrinsic refusal ─
    // (ARCHITECTURE.md "Convergence: idempotent apply, drift and the last good
    // render": the save that fits one instance and not the other.)
    isolated("tests::ip3c_fit_split_e2e", convergence::ip3c_fit_split_e2e);
}

#[cfg(unix)]
#[test]
fn ip3c_drift_e2e() {
    // ── Manager core: IP3c 3, the drift of a running instance, and the save's answer ─
    isolated("tests::ip3c_drift_e2e", convergence::ip3c_drift_e2e);
}

#[cfg(unix)]
#[test]
fn ip3c_not_converged_survives_restart_e2e() {
    // ── Manager core: IP3c 4, not converged and drift survive a manager restart ─
    isolated(
        "tests::ip3c_not_converged_survives_restart_e2e",
        convergence::ip3c_not_converged_survives_restart_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn ip3c_saves_answer_convergence_e2e() {
    // ── Manager core: IP3c 7, the five re-rendering saves answer `{running, notConverged}`, over the stream and over D-Bus ─
    isolated(
        "tests::ip3c_saves_answer_convergence_e2e",
        convergence::ip3c_saves_answer_convergence_e2e,
    );
}

#[cfg(unix)]
#[test]
fn ip3c_removal_takes_the_pair_e2e() {
    // ── Manager core: IP3c review 1, the removal takes the whole pair (launch included), the recreation is judged with no file standing ─
    isolated(
        "tests::ip3c_removal_takes_the_pair_e2e",
        convergence::ip3c_removal_takes_the_pair_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn ip3c_two_profiles_one_entry_two_card_sets_e2e() {
    // ── Manager core: IP3c review 3, one instance profile on two instances with card sets of their own ─
    isolated(
        "tests::ip3c_two_profiles_one_entry_two_card_sets_e2e",
        convergence::ip3c_two_profiles_one_entry_two_card_sets_e2e,
    );
}

#[cfg(unix)]
#[test]
fn ip3c_states_ask_nothing_e2e() {
    // ── Manager core: IP3c review 4, the instance list asks the probe nothing and the unit state only of drifters ─
    isolated(
        "tests::ip3c_states_ask_nothing_e2e",
        convergence::ip3c_states_ask_nothing_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn ip3c_not_converged_reads_ask_nothing_e2e() {
    // ── Manager core: IP3c review 8, the reads of a not-converged instance ask the GPU probe nothing ─
    isolated(
        "tests::ip3c_not_converged_reads_ask_nothing_e2e",
        convergence::ip3c_not_converged_reads_ask_nothing_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn ip3c_running_is_drifted_e2e() {
    // ── Manager core: IP3c review 5, a save's `running` is the running instances that now drift ─
    isolated(
        "tests::ip3c_running_is_drifted_e2e",
        convergence::ip3c_running_is_drifted_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn ip3c_startup_is_a_fit_check_e2e() {
    // ── Manager core: IP3c review 7, the load-on-startup count is a fit check of the instance ─
    isolated(
        "tests::ip3c_startup_is_a_fit_check_e2e",
        convergence::ip3c_startup_is_a_fit_check_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_e2e() {
    // ── Manager over D-Bus (manager/dbus.rs `ManagerInterface`) ──────────
    // Served peer to peer on a socket pair, no bus.
    isolated(
        "tests::manager_dbus_e2e",
        crate::manager::dbus::manager_dbus_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_waits_e2e() {
    // ── Manager over D-Bus while polkit waits for a password ────────────
    isolated(
        "tests::manager_dbus_waits_e2e",
        crate::manager::dbus::manager_dbus_waits_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_checks_e2e() {
    // ── Manager over D-Bus: a refused write's key and values ─────────────
    isolated(
        "tests::manager_dbus_checks_e2e",
        crate::manager::dbus::manager_dbus_checks_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_server_profiles_e2e() {
    // ── Manager over D-Bus: creating and deleting the node's profiles ────
    isolated(
        "tests::manager_dbus_server_profiles_e2e",
        crate::manager::dbus::manager_dbus_server_profiles_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_model_profiles_e2e() {
    // ── Manager over D-Bus: creating and deleting a model profile ────────
    isolated(
        "tests::manager_dbus_model_profiles_e2e",
        crate::manager::dbus::manager_dbus_model_profiles_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_instance_crud_e2e() {
    // ── Manager over D-Bus: creating and removing an instance ───────────
    isolated(
        "tests::manager_dbus_instance_crud_e2e",
        crate::manager::dbus::manager_dbus_instance_crud_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_ip3a_instance_profiles_e2e() {
    // ── Manager over D-Bus: the instance profile verbs (IP3a) ───────────
    isolated(
        "tests::manager_dbus_ip3a_instance_profiles_e2e",
        crate::manager::dbus::manager_dbus_ip3a_instance_profiles_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_client_e2e() {
    // ── pai-core's client of the manager over D-Bus ─────────────────────
    // pai_core::client::dbus against the interface, peer to peer, no bus.
    isolated(
        "tests::manager_dbus_client_e2e",
        dbus_client::manager_dbus_client_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_client_model_profiles_e2e() {
    // ── pai-core's client: creating and deleting a model profile ─────────
    isolated(
        "tests::manager_dbus_client_model_profiles_e2e",
        dbus_client::manager_dbus_client_model_profiles_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_client_model_profiles_as_e2e() {
    // ── pai-core's client: a member's model profile create and delete ────
    isolated(
        "tests::manager_dbus_client_model_profiles_as_e2e",
        dbus_client::manager_dbus_client_model_profiles_as_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_client_instance_profiles_e2e() {
    // ── pai-core's client: the instance profile verbs (IP3a) ────────────
    isolated(
        "tests::manager_dbus_client_instance_profiles_e2e",
        dbus_client::manager_dbus_client_instance_profiles_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_client_instance_profiles_as_e2e() {
    // ── pai-core's client: a member's instance profile verbs (IP3a) ─────
    isolated(
        "tests::manager_dbus_client_instance_profiles_as_e2e",
        dbus_client::manager_dbus_client_instance_profiles_as_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_client_model_files_e2e() {
    // ── pai-core's client: the node's models folder over the bus (M3 G1) ──
    isolated(
        "tests::manager_dbus_client_model_files_e2e",
        dbus_client::manager_dbus_client_model_files_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_client_convergence_e2e() {
    // ── pai-core's client: the states of the apply over the bus (IP3c review 6) ──
    isolated(
        "tests::manager_dbus_client_convergence_e2e",
        dbus_client::manager_dbus_client_convergence_e2e,
    );
}

#[cfg(unix)]
#[test]
fn manager_pipe_client_e2e() {
    // ── pai-core's client of the manager over a stream ──────────────────
    // pai_core::client::pipe against manager::pipe::serve_stream, on a
    // Unix socket pair (the named pipe's OS-neutral protocol).
    isolated(
        "tests::manager_pipe_client_e2e",
        pipe_client::manager_pipe_client_e2e,
    );
}

#[cfg(unix)]
#[test]
fn manager_pipe_client_model_profiles_e2e() {
    // ── pai-core's client over a stream: a model profile's life ─────────
    isolated(
        "tests::manager_pipe_client_model_profiles_e2e",
        pipe_client::manager_pipe_client_model_profiles_e2e,
    );
}

#[cfg(unix)]
#[test]
fn manager_pipe_client_instance_profiles_e2e() {
    // ── pai-core's client over a stream: the instance profiles (IP3a) ───
    isolated(
        "tests::manager_pipe_client_instance_profiles_e2e",
        pipe_client::manager_pipe_client_instance_profiles_e2e,
    );
}

#[cfg(unix)]
#[test]
fn manager_pipe_client_model_files_e2e() {
    // ── pai-core's client: the node's models folder over a stream (M3 G1) ─
    isolated(
        "tests::manager_pipe_client_model_files_e2e",
        pipe_client::manager_pipe_client_model_files_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_client_ip4a_e2e() {
    // ── pai-core's D-Bus client: the apply's rows, and the two new methods ─
    // (ARCHITECTURE.md "Instance profiles", slice IP4 part a: the client decodes
    // `running` and `notConverged` of every write that re-renders, and the entry
    // save and the server profile switch of a profile are client methods.)
    isolated(
        "tests::manager_dbus_client_ip4a_e2e",
        dbus_client::manager_dbus_client_ip4a_e2e,
    );
}

#[cfg(unix)]
#[test]
fn manager_pipe_client_ip4a_e2e() {
    // ── pai-core's client over a stream: the apply's rows, the new methods ─
    // (IP4a, the same pins as the D-Bus phase above, over the frame.)
    isolated(
        "tests::manager_pipe_client_ip4a_e2e",
        pipe_client::manager_pipe_client_ip4a_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_client_ip4b_e2e() {
    // ── pai-core's D-Bus client: the row's instance profile and host, and ─
    // the port a create answers (IP4b: the row's two fields appended, the
    // states riding the same read, `CreateInstance` by instance profile).
    isolated(
        "tests::manager_dbus_client_ip4b_e2e",
        dbus_client::manager_dbus_client_ip4b_e2e,
    );
}

#[cfg(unix)]
#[test]
fn manager_pipe_client_ip4b_e2e() {
    // ── pai-core's client over a stream: the same pins, on the frame ──────
    // (IP4b, the rows of `Instances` and `ListInstances`, and the create.)
    isolated(
        "tests::manager_pipe_client_ip4b_e2e",
        pipe_client::manager_pipe_client_ip4b_e2e,
    );
}

#[test]
fn manager_principal_e2e() {
    // ── Manager core: acting for a signed-in user (the bff names a member)
    isolated(
        "tests::manager_principal_e2e",
        crate::manager::service::manager_principal_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_principal_e2e() {
    // ── Manager over D-Bus: acting for a signed-in user (the `As` methods)
    isolated(
        "tests::manager_dbus_principal_e2e",
        crate::manager::dbus::manager_dbus_principal_e2e,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn manager_dbus_client_principal_e2e() {
    // ── pai-core's D-Bus client, acting for a signed-in user ─────────────
    isolated(
        "tests::manager_dbus_client_principal_e2e",
        dbus_client::manager_dbus_client_principal_e2e,
    );
}

#[test]
fn manager_action_log_e2e() {
    // ── Manager core: the log of who acted (one record per guarded call) ──
    isolated(
        "tests::manager_action_log_e2e",
        crate::manager::service::manager_action_log_e2e,
    );
}

#[test]
fn manager_write_file_if_e2e() {
    // ── Manager core: a write that swaps only on the text it expected ────
    isolated(
        "tests::manager_write_file_if_e2e",
        crate::manager::service::manager_write_file_if_e2e,
    );
}

#[test]
fn manager_windows_sync_e2e() {
    // ── Windows: the SCM's services follow instances.ini ─────────────────
    isolated(
        "tests::manager_windows_sync_e2e",
        crate::tests::windows_sync::manager_windows_sync_e2e,
    );
}
