//! Server-tab callback wiring (the server.ini editor), plus the nav-rail run
//! controls (`start_server` / `stop_server` — thin wrappers over the parent's
//! `start_server_async` / `stop_server_async`, which the tray shares).
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
                    // A RUNNING server keeps the config it was launched with —
                    // the save only changes the file — so surface the restart
                    // step instead of implying the change is live.
                    let msg = if s.get_server_running() {
                        format!(
                            "Saved {} — restart llama-server to apply.",
                            paths::server_ini().display()
                        )
                    } else {
                        format!("Saved {}", paths::server_ini().display())
                    };
                    set_status(&app, msg, false);
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
        app.global::<AppState>().on_revert_server(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            load_server_into_ui(&app);
            // No refresh_file_options / refresh_integrations here: a form
            // revert never touches disk, and both hubs derive from the SAVED
            // config — the only observable effect of calling them was wiping
            // pending Integrations toggles.
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

    wire_gpu_table(app);

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

/// The server-wide GPU distribution table's four callbacks. Each one derives a
/// new selection with `gpu_split` (the pure rules live there, unit-tested) and
/// writes it back into `server_form.device` + `.tensor_split` — those two strings
/// ARE the state; the table holds no copy.
///
/// Note which refresh each one ends with. Toggle / Auto / Even REBUILD the row
/// model, because they change rows the user didn't click and the delegates' one-
/// way bindings only survive a rebuild. A weight edit deliberately does NOT: it
/// changes no other row's weight (the SpinBoxes are disabled in Auto mode, so
/// there is never a seed-the-others step), and rebuilding would recreate the very
/// SpinBox being typed into — multi-digit weights would be impossible. See
/// GpuSplitTable's binding note in ui/components.slint.
fn wire_gpu_table(app: &AppWindow) {
    let s = app.global::<AppState>();
    {
        let app_weak = app.as_weak();
        s.on_server_gpu_toggle(move |id| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let sel = gpu_split::toggle(&server_selection(&app), id.as_str());
            set_server_selection(&app, &sel);
            refresh_gpu_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_server_gpu_move(move |id, delta| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let sel = gpu_split::move_by(&server_selection(&app), id.as_str(), delta);
            set_server_selection(&app, &sel);
            refresh_gpu_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_server_gpu_weight(move |id, weight| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let sel = gpu_split::set_weight(&server_selection(&app), id.as_str(), weight);
            set_server_selection(&app, &sel);
            refresh_gpu_scalars(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_server_gpu_auto(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let sel = gpu_split::set_auto(&server_selection(&app));
            set_server_selection(&app, &sel);
            refresh_gpu_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_server_gpu_even(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let sel = gpu_split::set_even(&server_selection(&app));
            set_server_selection(&app, &sel);
            refresh_gpu_rows(&app);
        });
    }
}

/// Native folder picker for the "Browse…" button, seeded at `start`. Server-tab
/// only, so it lives here rather than in the shared hub. Deliberately BLOCKS
/// the UI thread (the sanctioned exception to gui.rs's threading contract): a
/// native modal dialog is supposed to hold its owner, and pumping our loop
/// underneath it would let the 5 s status tick repaint a window the user can't
/// interact with anyway.
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
