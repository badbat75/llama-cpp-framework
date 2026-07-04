//! Server-tab callback wiring (the server.ini editor).
//!
//! Shared state, generated Slint types, and the `load_*` / `refresh_*` / `set_status`
//! helpers all live in the parent `gui` module; `use super::*` pulls them in.

use super::*;

pub(super) fn wire(app: &AppWindow, tray: &AppTray, state: &Rc<RefCell<State>>) {
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_save_server(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let s = app.global::<AppState>();
            let cfg = server_form::form_to_config(&s.get_server_form());
            match server_cfg::save(&cfg) {
                Ok(()) => {
                    set_status(
                        &app,
                        format!("Saved {}", paths::server_ini().display()),
                        false,
                    );
                    // Re-derive the saved-config projections (Command Line
                    // card + chat URL) from the file just written.
                    refresh_server_snapshot(&app);
                    refresh_file_options(&app, &state);
                    refresh_integrations(&app);
                    snapshot_server_base(&app);
                }
                Err(e) => set_status(&app, format!("Save failed: {e}"), true),
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_revert_server(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            load_server_into_ui(&app);
            refresh_file_options(&app, &state);
            refresh_integrations(&app);
            set_status(
                &app,
                format!("Reloaded {}", paths::server_ini().display()),
                false,
            );
        });
    }
    // Browse callback needs nothing from `app` — it works purely on its argument.
    app.global::<AppState>()
        .on_browse_models_dir(move |current| {
            let start = if !current.is_empty() {
                PathBuf::from(current.as_str())
            } else {
                PathBuf::from(server_cfg::default_models_dir())
            };
            pick_dir(&start)
                .map(|p| SharedString::from(p.to_string_lossy().into_owned()))
                .unwrap_or(current)
        });

    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        app.global::<AppState>().on_start_server(move || {
            start_server_async(app_weak.clone(), tray_weak.clone());
        });
    }
    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        app.global::<AppState>().on_stop_server(move || {
            stop_server_async(app_weak.clone(), tray_weak.clone());
        });
    }
}

/// Native folder picker for the "Browse…" button, seeded at `start`. Server-tab
/// only, so it lives here rather than in the shared hub.
fn pick_dir(start: &std::path::Path) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Pick a folder")
        .set_directory(start)
        .pick_folder()
}

/// Re-baseline the server form after a save so `server_dirty` reads false until
/// the next edit — the server analog of `apply_form`'s base handling.
fn snapshot_server_base(app: &AppWindow) {
    let s = app.global::<AppState>();
    s.set_server_form_base(s.get_server_form());
}
