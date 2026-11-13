//! The end-to-end test of this crate.
//!
//! WARNING: its phases redirect config IO by setting the process-wide
//! `LLAMA_CPP_CONFIG_DATA_ROOT` env var and never restore it, so they all run
//! in this ONE `#[test]`, one after the other. Unit tests elsewhere must not
//! touch `paths::` (or anything that resolves the config dir): cargo runs
//! tests on parallel threads, so they would race this one nondeterministically.

#[test]
fn data_root_e2e() {
    // ── System config paths (paths.rs `system_config_dir`, instances) ────
    crate::paths::instance_paths_e2e();

    // ── Service launch (launch.rs `instance_command`) ────────────────────
    crate::launch::instance_launch_e2e();
}
