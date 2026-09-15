//! Settings-tab callback wiring (the configurator's own preferences).
//!
//! Unlike the form tabs there is no Save/Revert and no dirty state: each toggle
//! APPLIES on click: the checkbox is already flipped through its two-way
//! binding when the callback fires, so the handler persists that value and, on
//! a failed write, pushes the real state back into the property (the two-way
//! binding is what makes that rollback reach the widget). Because nothing here
//! is ever pending, the tab needs no leg in the F5/Refresh discard guard.
//!
//! Backing stores, deliberately different per toggle: "Start with Windows" and
//! "start minimized" together ARE the HKCU Run registry entry (`startup.rs`:
//! presence + whether the stored command carries `--minimized`; no INI mirror
//! that Task Manager's Startup panel could desync), while "start llama-server
//! on launch" and the log-rotation pair live in settings.ini (`settings.rs`).
//! `refresh` re-reads all of them, so Refresh/F5 picks up out-of-band changes
//! like any other disk-backed state.
//!
//! The rotation threshold is the tab's one text field, and it does not persist
//! per keystroke: `commit_log_rotate_kb` fires on Enter and on focus loss (the
//! page's `changed has-focus` handler), parses the digits-only text, and on an
//! empty or overflowing value restores the stored one instead of writing a
//! default the user never typed. A commit that changes nothing writes nothing,
//! so tabbing through the field leaves the footer alone.

use super::*;

/// Seed / re-seed the tab from its two sources. Part of the
/// `reload_all_from_disk` hub (startup seed + Refresh/F5).
pub(super) fn refresh(app: &AppWindow) {
    let s = app.global::<AppState>();
    s.set_startup_supported(startup::is_supported());
    let enabled = startup::is_enabled();
    s.set_start_with_windows(enabled);
    // With no Run entry there is nothing to read the tray choice from; default
    // the (disabled) checkbox to minimized: the recommended shape, and what a
    // fresh enable will then write.
    s.set_start_minimized_to_tray(if enabled {
        startup::starts_minimized()
    } else {
        true
    });
    let cfg = settings::load();
    s.set_start_server_on_launch(cfg.start_server_on_launch);
    s.set_log_rotate(cfg.log_rotate);
    s.set_log_rotate_kb(SharedString::from(cfg.log_rotate_kb.to_string()));
}

/// Persist one settings.ini change read-modify-write (so no toggle's save can
/// wipe another key), reporting the outcome in the footer; on a failed write
/// the caller gets `false` and pushes the real state back into the property.
fn persist_settings(app: &AppWindow, edit: impl FnOnce(&mut settings::Settings), ok: String) -> bool {
    let mut cfg = settings::load();
    edit(&mut cfg);
    match settings::save(&cfg) {
        Ok(()) => {
            set_status(app, ok, false);
            true
        }
        Err(e) => {
            set_status(app, format!("Saving settings.ini failed: {e}"), true);
            false
        }
    }
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
        app.global::<AppState>().on_toggle_start_minimized(move || {
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
                let ok = if want {
                    "llama-server will start when llama-cpp-config launches."
                } else {
                    "llama-server will no longer start automatically."
                };
                if !persist_settings(&app, |c| c.start_server_on_launch = want, ok.into()) {
                    s.set_start_server_on_launch(settings::load().start_server_on_launch);
                }
            });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_toggle_log_rotate(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let s = app.global::<AppState>();
            let want = s.get_log_rotate();
            let ok = if want {
                "llama-server.log will be filed away when the server stops."
            } else {
                "llama-server.log will grow across runs."
            };
            if !persist_settings(&app, |c| c.log_rotate = want, ok.into()) {
                s.set_log_rotate(settings::load().log_rotate);
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_commit_log_rotate_kb(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let s = app.global::<AppState>();
            let text = s.get_log_rotate_kb();
            let stored = settings::load().log_rotate_kb;
            // The field is digits-only (`InputType.number`), so what can still
            // fail is an empty field or an overflow; both restore the stored
            // value rather than persist a default the user did not type.
            let Some(kb) = crate::ini::parse_int(text.trim()).and_then(|n| u32::try_from(n).ok())
            else {
                s.set_log_rotate_kb(SharedString::from(stored.to_string()));
                if !text.trim().is_empty() {
                    set_status(&app, format!("'{text}' is not a size in KB."), true);
                }
                return;
            };
            if kb == stored {
                // Focus loss with nothing typed: no write, no footer noise.
                s.set_log_rotate_kb(SharedString::from(kb.to_string()));
                return;
            }
            let ok = if kb == 0 {
                "Every non-empty log will be filed away.".to_string()
            } else {
                format!("Logs larger than {kb} KB will be filed away.")
            };
            if persist_settings(&app, |c| c.log_rotate_kb = kb, ok) {
                // Normalize what was typed (leading zeros) to what was stored.
                s.set_log_rotate_kb(SharedString::from(kb.to_string()));
            } else {
                s.set_log_rotate_kb(SharedString::from(stored.to_string()));
            }
        });
    }
}
