//! Settings-tab callback wiring (the configurator's own preferences).
//!
//! Unlike the form tabs there is no Save/Revert and no dirty state: each toggle
//! APPLIES on click: the checkbox is already flipped through its two-way
//! binding when the callback fires, so the handler persists that value and, on
//! a failed write, pushes the real state back into the property (the two-way
//! binding is what makes that rollback reach the widget). Because nothing here
//! is ever pending, the tab needs no leg in the F5/Refresh discard guard.
//!
//! Backing stores, deliberately different per toggle: "Start at login" and
//! "start minimized" together ARE the platform login entry, HKCU Run or XDG
//! autostart (`startup.rs`: presence + whether the stored command carries `--minimized`; no INI mirror
//! that Task Manager's Startup panel could desync), "start llama-server when
//! the system starts" (under the manager) IS the instance's service enabled
//! or not, asked of the manager, while "start llama-server on launch" (on
//! Direct) and the log-rotation pair live in settings.ini (`settings.rs`).
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
use crate::strings;

/// Seed / re-seed the tab from its two sources. Part of the
/// `reload_all_from_disk` hub (startup seed + Refresh/F5).
pub(super) fn refresh(app: &AppWindow) {
    let s = app.global::<AppState>();
    s.set_startup_supported(startup::is_supported());
    let enabled = startup::is_enabled();
    s.set_start_at_login(enabled);
    // With no login entry there is nothing to read the tray choice from; default
    // the (disabled) checkbox to minimized: the recommended shape, and what a
    // fresh enable will then write.
    s.set_start_minimized_to_tray(if enabled {
        startup::starts_minimized()
    } else {
        true
    });
    let cfg = settings::load();
    s.set_start_server_on_launch(cfg.start_server_on_launch);
    let backend = crate::server_control::backend();
    let service = backend.kind() == crate::server_control::Kind::Manager;
    s.set_server_is_service(service);
    // No manager to ask reads as off, never a stale on.
    s.set_start_at_boot(service && backend.boot_enabled().unwrap_or(false));
    s.set_log_rotate(cfg.log_rotate);
    s.set_log_rotate_kb(SharedString::from(cfg.log_rotate_kb.to_string()));
    s.set_log_rotate_applies(backend.rotates_log_file());
    s.set_log_rotate_note(SharedString::from(strings::t(
        crate::server_control::log_rotate_note_key(crate::server_control::Os::current()),
    )));
}

/// Persist one settings.ini change read-modify-write (so no toggle's save can
/// wipe another key), reporting the outcome in the footer; on a failed write
/// the caller gets `false` and pushes the real state back into the property.
fn persist_settings(
    app: &AppWindow,
    edit: impl FnOnce(&mut settings::Settings),
    ok: String,
) -> bool {
    let mut cfg = settings::load();
    edit(&mut cfg);
    match settings::save(&cfg) {
        Ok(()) => {
            set_status(app, ok, false);
            true
        }
        Err(e) => {
            set_status(app, strings::fill("settings-save-failed", &[&e]), true);
            false
        }
    }
}

/// Rewrite (or delete) the login entry from the two startup properties as they
/// currently stand, rolling both back to the registry's real state when the
/// write fails. The shared tail of the two startup toggles.
fn apply_startup_entry(app: &AppWindow, ok_message: String) {
    let s = app.global::<AppState>();
    let want_enabled = s.get_start_at_login();
    let want_minimized = s.get_start_minimized_to_tray();
    match startup::set_enabled(want_enabled, want_minimized) {
        Ok(()) => set_status(app, ok_message, false),
        Err(e) => {
            s.set_start_at_login(startup::is_enabled());
            if startup::is_enabled() {
                s.set_start_minimized_to_tray(startup::starts_minimized());
            }
            set_status(app, strings::fill("startup-change-failed", &[&e]), true);
        }
    }
}

pub(super) fn wire(app: &AppWindow) {
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_toggle_start_at_login(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let message = if app.global::<AppState>().get_start_at_login() {
                strings::t("start-at-login-on")
            } else {
                strings::t("start-at-login-off")
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
            // The checkbox is only enabled while "Start at login" is on
            // (there is no entry to rewrite otherwise), but guard anyway:
            // with it off the choice is UI-only until a fresh enable
            // writes it.
            if !app.global::<AppState>().get_start_at_login() {
                return;
            }
            let message = if app.global::<AppState>().get_start_minimized_to_tray() {
                strings::t("login-minimized-on")
            } else {
                strings::t("login-minimized-off")
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
                    strings::t("server-on-launch-on")
                } else {
                    strings::t("server-on-launch-off")
                };
                if !persist_settings(&app, |c| c.start_server_on_launch = want, ok) {
                    s.set_start_server_on_launch(settings::load().start_server_on_launch);
                }
            });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_toggle_start_at_boot(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let want = app.global::<AppState>().get_start_at_boot();
            // The manager may wait for an administrator's password: off the
            // UI thread.
            let app_weak = app.as_weak();
            std::thread::spawn(move || {
                let shown =
                    crate::server_control::toggle_boot(crate::server_control::backend(), want);
                slint::invoke_from_event_loop(move || {
                    if let Some(app) = app_weak.upgrade() {
                        app.global::<AppState>().set_start_at_boot(shown.enabled);
                        set_status(&app, shown.message, shown.error);
                    }
                })
                .ok();
            });
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
                strings::t("log-rotate-on")
            } else {
                strings::t("log-rotate-off")
            };
            if !persist_settings(&app, |c| c.log_rotate = want, ok) {
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
                    set_status(&app, strings::fill("not-a-size-in-kb", &[&text]), true);
                }
                return;
            };
            if kb == stored {
                // Focus loss with nothing typed: no write, no footer noise.
                s.set_log_rotate_kb(SharedString::from(kb.to_string()));
                return;
            }
            let ok = if kb == 0 {
                strings::t("log-rotate-every")
            } else {
                strings::fill("log-rotate-larger-than", &[&kb])
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

/// E2E phase, run from `tests/ui_bindings.rs`'s single `#[test]` (it needs the
/// testing backend that test owns): under the manager journald rotates the
/// log, so the rotation pair is greyed out (its values kept) with a note
/// saying so; on `Direct` it stays as it was.
#[cfg(test)]
pub(super) fn e2e() {
    use crate::server_control::{fake, Backend};
    use i_slint_backend_testing::ElementHandle;
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());

    let app = AppWindow::new().expect("build AppWindow");
    install_strings(app.global::<Strings>());
    app.window().set_size(slint::PhysicalSize::new(1400, 3200));
    app.show().expect("realize window");
    let st = app.global::<AppState>();
    st.set_current_tab(4); // Settings
    let toggle = strings::t("rotate-log-toggle");
    let note = strings::t("log-rotate-journald");
    assert_ne!(
        note, "log-rotate-journald",
        "the note is in the strings file"
    );
    let enabled = |label: &str| {
        ElementHandle::find_by_accessible_label(&app, label)
            .next()
            .unwrap_or_else(|| panic!("no widget with accessible-label {label:?}"))
            .accessible_enabled()
    };
    // The note is built only under the manager (an `if` in the page): a
    // `visible: false` element still answers a search.
    let shown = |label: &str| {
        ElementHandle::find_by_accessible_label(&app, label)
            .next()
            .is_some()
    };

    // Direct: the per-user file is rotated by these two, as before.
    fake::scoped(Backend::Direct, || refresh(&app));
    assert!(st.get_log_rotate(), "the default: on");
    assert_eq!(enabled(&toggle), Some(true), "Direct: rotation editable");
    assert_eq!(
        enabled("log-rotate-kb"),
        Some(true),
        "Direct: threshold editable"
    );
    assert!(!shown(&note), "Direct: no journald note");

    // The manager (the test build's default backend): journald rotates.
    refresh(&app);
    assert_eq!(
        enabled(&toggle),
        Some(false),
        "Manager: rotation greyed out"
    );
    assert_eq!(
        enabled("log-rotate-kb"),
        Some(false),
        "Manager: threshold greyed out"
    );
    assert!(st.get_log_rotate(), "greyed out, not cleared");
    assert_eq!(st.get_log_rotate_kb(), "1024", "the stored value kept");
    assert!(
        shown(&note),
        "Manager: the note says journald rotates the log"
    );

    // ── Start llama-server: at boot under the manager, at launch on Direct
    let at_boot = strings::t("start-at-boot-toggle");
    let at_launch = strings::t("start-llama-server-toggle");
    assert_ne!(
        at_boot, "start-at-boot-toggle",
        "the toggle is in the strings file"
    );
    let checked = |label: &str| {
        ElementHandle::find_by_accessible_label(&app, label)
            .next()
            .unwrap_or_else(|| panic!("no widget with accessible-label {label:?}"))
            .accessible_checked()
    };

    // Direct (macOS): the launch toggle, settings.ini's StartServerOnLaunch.
    fake::scoped(Backend::Direct, || refresh(&app));
    assert!(shown(&at_launch), "Direct: the start at launch");
    assert!(!shown(&at_boot), "Direct: no start at boot");

    // The manager: the start at boot, the manager's own state, both ways.
    for on in [true, false] {
        let log = fake::log();
        let c = fake::FakeClient::new(&log, pai_core::client::InstanceState::Inactive);
        *c.boot.lock().unwrap() = on;
        fake::scoped(fake::backend(c), || refresh(&app));
        assert!(shown(&at_boot), "Manager: the start at boot");
        assert!(!shown(&at_launch), "Manager: no start at launch");
        assert_eq!(checked(&at_boot), Some(on), "the manager's state: {on}");
        assert!(
            log.lock()
                .unwrap()
                .iter()
                .any(|e| e == "boot_enabled default"),
            "read from the manager: {:?}",
            log.lock().unwrap()
        );
    }

    // No manager to ask (the test build's default backend): off, never a
    // stale on.
    refresh(&app);
    assert_eq!(checked(&at_boot), Some(false), "no manager: off");
    app.hide().ok();
}
