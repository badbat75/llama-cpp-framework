//! Settings-tab callback wiring (the configurator's own preferences).
//!
//! Unlike the form tabs there is no Save/Revert and no dirty state: each toggle
//! APPLIES on click — the checkbox is already flipped through its two-way
//! binding when the callback fires, so the handler persists that value and, on
//! a failed write, pushes the real state back into the property (the two-way
//! binding is what makes that rollback reach the widget). Because nothing here
//! is ever pending, the tab needs no leg in the F5/Refresh discard guard.
//!
//! Backing stores, deliberately different per toggle: "Start with Windows" and
//! "start minimized" together ARE the HKCU Run registry entry (`startup.rs` —
//! presence + whether the stored command carries `--minimized`; no INI mirror
//! that Task Manager's Startup panel could desync), while "start llama-server
//! on launch" lives in settings.ini (`settings.rs`). `refresh` re-reads all of
//! them, so Refresh/F5 picks up out-of-band changes like any other disk-backed
//! state.

use super::*;

/// Seed / re-seed the tab from its two sources. Part of the
/// `reload_all_from_disk` hub (startup seed + Refresh/F5).
pub(super) fn refresh(app: &AppWindow) {
    let s = app.global::<AppState>();
    s.set_startup_supported(startup::is_supported());
    let enabled = startup::is_enabled();
    s.set_start_with_windows(enabled);
    // With no Run entry there is nothing to read the tray choice from; default
    // the (disabled) checkbox to minimized — the recommended shape, and what a
    // fresh enable will then write.
    s.set_start_minimized_to_tray(if enabled {
        startup::starts_minimized()
    } else {
        true
    });
    s.set_start_server_on_launch(settings::load().start_server_on_launch);
}

/// Rewrite (or delete) the Run entry from the two startup properties as they
/// currently stand, rolling both back to the registry's real state when the
/// write fails. The shared tail of the two startup toggles.
fn apply_startup_entry(app: &AppWindow, ok_message: String) {
    let s = app.global::<AppState>();
    let want_enabled = s.get_start_with_windows();
    let want_minimized = s.get_start_minimized_to_tray();
    match startup::set_enabled(want_enabled, want_minimized) {
        Ok(()) => set_status(app, ok_message, false),
        Err(e) => {
            s.set_start_with_windows(startup::is_enabled());
            if startup::is_enabled() {
                s.set_start_minimized_to_tray(startup::starts_minimized());
            }
            set_status(app, format!("Startup change failed: {e}"), true);
        }
    }
}

pub(super) fn wire(app: &AppWindow) {
    {
        let app_weak = app.as_weak();
        app.global::<AppState>()
            .on_toggle_start_with_windows(move || {
                let Some(app) = app_weak.upgrade() else {
                    return;
                };
                let message = if app.global::<AppState>().get_start_with_windows() {
                    "llama-cpp-config will start with Windows.".into()
                } else {
                    "Removed llama-cpp-config from Windows startup.".into()
                };
                apply_startup_entry(&app, message);
            });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>()
            .on_toggle_start_minimized(move || {
                let Some(app) = app_weak.upgrade() else {
                    return;
                };
                // The checkbox is only enabled while "Start with Windows" is on
                // (there is no entry to rewrite otherwise), but guard anyway:
                // with it off the choice is UI-only until a fresh enable
                // writes it.
                if !app.global::<AppState>().get_start_with_windows() {
                    return;
                }
                let message = if app.global::<AppState>().get_start_minimized_to_tray() {
                    "The logon launch will start minimized to the tray.".into()
                } else {
                    "The logon launch will open the configurator window.".into()
                };
                apply_startup_entry(&app, message);
            });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>()
            .on_toggle_start_server_on_launch(move || {
                let Some(app) = app_weak.upgrade() else {
                    return;
                };
                let s = app.global::<AppState>();
                let want = s.get_start_server_on_launch();
                // Read-modify-write so a future settings.ini key can't be wiped
                // by this toggle's save.
                let mut cfg = settings::load();
                cfg.start_server_on_launch = want;
                match settings::save(&cfg) {
                    Ok(()) => set_status(
                        &app,
                        if want {
                            "llama-server will start when llama-cpp-config launches.".into()
                        } else {
                            "llama-server will no longer start automatically.".into()
                        },
                        false,
                    ),
                    Err(e) => {
                        s.set_start_server_on_launch(settings::load().start_server_on_launch);
                        set_status(&app, format!("Saving settings.ini failed: {e}"), true);
                    }
                }
            });
    }
}
