//! Server-tab callback wiring (the server.ini editor).
//!
//! Shared state, generated Slint types, and the `load_*` / `refresh_*` / `set_status`
//! helpers all live in the parent `gui` module; `use super::*` pulls them in.

use super::*;

pub(super) fn wire(app: &AppWindow, tray: &AppTray) {
    {
        let app_weak = app.as_weak();
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
                    let cmdline = runstate::command_line().unwrap_or_default();
                    s.set_server_command_line(SharedString::from(cmdline));
                    refresh_file_options(&app);
                    refresh_integrations(&app);
                    snapshot_server_base(&app);
                }
                Err(e) => set_status(&app, format!("Save failed: {e}"), true),
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_revert_server(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            load_server_into_ui(&app);
            refresh_file_options(&app);
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
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            match runstate::start() {
                Ok(()) => {
                    set_status(&app, "llama-server started.".into(), false);
                    refresh_run_status(app.as_weak(), tray_weak.clone());
                }
                Err(e) => {
                    set_status(&app, format!("Failed to start: {e}"), true);
                    app.global::<AppState>().set_server_status_is_error(true);
                }
            }
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
