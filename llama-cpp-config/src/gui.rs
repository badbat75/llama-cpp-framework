// Slint GUI wiring for llama.cpp-framework configurator.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::form::{blank_form, form_to_preset, preset_to_form};
use crate::{
    devices, gguf, ini, integrations, model_scan, net_ifaces, paths, presets, runstate, server_cfg,
    server_version,
};

slint::include_modules!();

#[derive(Default)]
struct State {
    presets: Vec<presets::Preset>,
    // Full (unfiltered) model scan backing the new-preset dialog, so the search
    // box can filter without re-hitting disk on every keystroke.
    dialog_models_all: Vec<model_scan::FileOption>,
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Single-instance: if the configurator is already running, hand off to it
    // (it surfaces its window) and exit instead of opening a second window.
    #[cfg(windows)]
    let _instance = match crate::single_instance::acquire() {
        crate::single_instance::Acquire::Secondary => return Ok(()),
        crate::single_instance::Acquire::Primary(guard) => guard,
    };

    let app = AppWindow::new()?;
    app.global::<AppState>()
        .set_app_version(SharedString::from(env!("CARGO_PKG_VERSION")));
    let tray = AppTray::new()?;
    let state = Rc::new(RefCell::new(State::default()));

    // Surface the live window when another launch pokes the activation event.
    #[cfg(windows)]
    {
        let app_weak = app.as_weak();
        _instance.spawn_listener(move || {
            let _ = app_weak.upgrade_in_event_loop(activate_window);
        });
    }

    load_server_into_ui(&app);
    refresh_presets(&app, &state);
    refresh_run_status(app.as_weak(), tray.as_weak());
    refresh_file_options(&app);
    spawn_version_probe(app.as_weak());
    spawn_device_probe(app.as_weak());

    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_sync_device_dropdowns(move || {
            if let Some(app) = app_weak.upgrade() {
                refresh_device_options(&app);
            }
        });
    }

    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_model_changed(move || {
            if let Some(app) = app_weak.upgrade() {
                update_model_info(&app);
            }
        });
    }

    app.global::<AppState>()
        .set_presets_path(SharedString::from(
            paths::presets_ini().to_string_lossy().into_owned(),
        ));

    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        app.global::<AppState>().on_refresh_status(move || {
            refresh_run_status(app_weak.clone(), tray_weak.clone());
        });
    }

    // Reload everything from disk: server.ini, presets.ini, the models-dir
    // scan, and the integration state. Lets the user pick up files added to
    // the models directory or config files edited by hand, without a restart.
    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_reload_all(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            load_server_into_ui(&app);
            refresh_presets(&app, &state);
            refresh_file_options(&app);
            refresh_integrations(&app);
            refresh_run_status(app.as_weak(), tray_weak.clone());
            set_status(&app, "Reloaded configuration from disk.".into(), false);
        });
    }

    let status_timer = slint::Timer::default();
    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        status_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(5),
            move || {
                refresh_run_status(app_weak.clone(), tray_weak.clone());
            },
        );
    }

    wire_server_tab(&app, &tray);
    wire_models_tab(&app, &state);
    wire_integrations_tab(&app);
    wire_tray(&app, &tray);

    // Closing the window hides it to the tray instead of quitting; the visible
    // tray icon keeps the event loop alive. Use the tray's "Quit" to exit.
    app.window()
        .on_close_requested(|| slint::CloseRequestResponse::HideWindow);

    app.show()?;
    tray.show()?;
    slint::run_event_loop()?;
    Ok(())
}

// ── Per-tab callback wiring (called from run) ─────────────────

fn wire_server_tab(app: &AppWindow, tray: &AppTray) {
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_save_server(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let cfg = read_server_from_ui(&app);
            match server_cfg::save(&cfg) {
                Ok(()) => {
                    set_status(
                        &app,
                        format!("Saved {}", paths::server_ini().display()),
                        false,
                    );
                    let cmdline = runstate::command_line().unwrap_or_default();
                    app.global::<AppState>()
                        .set_server_command_line(SharedString::from(cmdline));
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
    {
        let app_weak = app.as_weak();
        app.global::<AppState>()
            .on_browse_models_dir(move |current| {
                let _app = app_weak.upgrade();
                let start = if !current.is_empty() {
                    PathBuf::from(current.as_str())
                } else {
                    PathBuf::from(server_cfg::default_models_dir())
                };
                pick_dir(&start)
                    .map(|p| SharedString::from(p.to_string_lossy().into_owned()))
                    .unwrap_or(current)
            });
    }

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

fn wire_models_tab(app: &AppWindow, state: &Rc<RefCell<State>>) {
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_select_preset(move |index| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let st = state.borrow();
            if let Some(p) = usize::try_from(index).ok().and_then(|i| st.presets.get(i)) {
                app.global::<AppState>().set_selected_preset_index(index);
                apply_form(&app, preset_to_form(p));
                drop(st);
                refresh_file_options(&app);
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_save_preset(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let p = form_to_preset(&app.global::<AppState>().get_form());
            if p.id.is_empty() {
                set_status(&app, "Preset id is empty.".into(), true);
                return;
            }
            if p.model.is_empty() {
                set_status(&app, "Pick a model file before saving.".into(), true);
                return;
            }
            match presets::save(&p) {
                Ok(()) => {
                    set_status(&app, format!("Saved preset [{}]", p.id), false);
                    refresh_presets(&app, &state);
                    let st = state.borrow();
                    if let Some(i) = st.presets.iter().position(|x| x.id == p.id) {
                        app.global::<AppState>().set_selected_preset_index(i as i32);
                    }
                    drop(st);
                    refresh_file_options(&app);
                    refresh_integrations(&app);
                }
                Err(e) => set_status(&app, format!("Save failed: {e}"), true),
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_revert_preset(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            refresh_presets(&app, &state);
            let idx = app.global::<AppState>().get_selected_preset_index();
            let st = state.borrow();
            if let Some(p) = usize::try_from(idx).ok().and_then(|i| st.presets.get(i)) {
                let label = p.id.clone();
                apply_form(&app, preset_to_form(p));
                drop(st);
                refresh_file_options(&app);
                set_status(&app, format!("Reloaded [{label}] from presets.ini"), false);
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_delete_preset(move |id| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            if id.is_empty() {
                return;
            }
            match presets::delete(id.as_str()) {
                Ok(()) => {
                    set_status(&app, format!("Deleted [{id}]"), false);
                    refresh_presets(&app, &state);
                    app.global::<AppState>().set_selected_preset_index(-1);
                    apply_form(&app, blank_form());
                    refresh_file_options(&app);
                    refresh_integrations(&app);
                }
                Err(e) => set_status(&app, format!("Delete failed: {e}"), true),
            }
        });
    }

    // Holds the preset a Clone is based on, between opening the picker and the
    // user confirming a target model. `new_dialog_source_id == ""` means the
    // picker is in plain "New" (empty-preset) mode; non-empty means Clone.
    let pending_clone_base: Rc<RefCell<Option<presets::Preset>>> = Rc::new(RefCell::new(None));
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        let pending_clone_base = pending_clone_base.clone();
        app.global::<AppState>().on_new_preset(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            // "New…" is always create-from-scratch — independent of any current
            // selection, so it can never silently turn into a clone.
            *pending_clone_base.borrow_mut() = None;
            populate_dialog_models(&app, &state);
            app.global::<AppState>()
                .set_new_dialog_source_id(SharedString::from(""));
            app.global::<AppState>().set_show_new_kind_picker(true);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        let pending_clone_base = pending_clone_base.clone();
        app.global::<AppState>().on_clone_preset(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            // Clone source is always the selected preset (the button is disabled
            // otherwise). Stash it and surface its id in the dialog so it's clear
            // what is being copied.
            let selected = {
                let st = state.borrow();
                let idx = app.global::<AppState>().get_selected_preset_index();
                usize::try_from(idx)
                    .ok()
                    .and_then(|i| st.presets.get(i))
                    .cloned()
            };
            let Some(p) = selected else {
                set_status(&app, "Select a preset to clone first.".into(), true);
                return;
            };
            populate_dialog_models(&app, &state);
            app.global::<AppState>()
                .set_new_dialog_source_id(SharedString::from(p.id.clone()));
            *pending_clone_base.borrow_mut() = Some(p);
            app.global::<AppState>().set_show_new_kind_picker(true);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        let pending_clone_base = pending_clone_base.clone();
        app.global::<AppState>().on_pick_new_empty(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            *pending_clone_base.borrow_mut() = None;
            let Some(path) = picked_dialog_model_path(&app) else {
                set_status(&app, "Pick a model from the list first.".into(), true);
                return;
            };
            run_new_empty(&app, &state, path);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_pick_new_clone(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let Some(path) = picked_dialog_model_path(&app) else {
                set_status(&app, "Pick a model from the list first.".into(), true);
                return;
            };
            let Some(base) = pending_clone_base.borrow_mut().take() else {
                set_status(&app, "Clone source no longer available.".into(), true);
                return;
            };
            run_new_clone(&app, &state, base, path);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>()
            .on_rename_preset(move |old_id, new_id| {
                let Some(app) = app_weak.upgrade() else {
                    return;
                };
                match presets::rename(old_id.as_str(), new_id.as_str()) {
                    Ok(()) => {
                        set_status(&app, format!("Renamed [{old_id}] -> [{new_id}]"), false);
                        reload_presets(&app, &state, Some(new_id.as_str()));
                        refresh_file_options(&app);
                        refresh_integrations(&app);
                    }
                    Err(e) => set_status(&app, format!("Rename failed: {e}"), true),
                }
            });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_filter_presets(move |q| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let st = state.borrow();
            let summaries = preset_summaries(&st.presets, q.as_str());
            app.global::<AppState>()
                .set_presets(ModelRc::from(Rc::new(VecModel::from(summaries))));
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_filter_dialog_models(move |q| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let st = state.borrow();
            apply_dialog_models(&app, &st.dialog_models_all, q.as_str());
        });
    }
}

fn wire_integrations_tab(app: &AppWindow) {
    refresh_integrations(app);
    {
        let app_weak = app.as_weak();
        app.global::<AppState>()
            .on_toggle_integration_model(move |index| {
                let Some(app) = app_weak.upgrade() else {
                    return;
                };
                let idx = usize::try_from(index).unwrap_or(usize::MAX);
                let models = app.global::<AppState>().get_integration_models();
                if idx < models.iter().count() {
                    let mut v: Vec<IntegrationModel> = models.iter().collect();
                    if let Some(entry) = v.get_mut(idx) {
                        entry.enabled = !entry.enabled;
                    }
                    let rc = Rc::new(VecModel::from(v));
                    app.global::<AppState>()
                        .set_integration_models(ModelRc::from(rc));
                }
            });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_save_integrations(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let models = app.global::<AppState>().get_integration_models();
            let checked: Vec<String> = models
                .iter()
                .filter(|m| m.enabled)
                .map(|m| m.id.to_string())
                .collect();
            let base_url = app
                .global::<AppState>()
                .get_integration_base_url()
                .to_string();
            match integrations::save_opencode_models(&checked, &base_url) {
                Ok(()) => {
                    set_status(&app, "Saved model list to opencode.json.".into(), false);
                    refresh_integrations(&app);
                }
                Err(e) => set_status(&app, format!("Save failed: {e}"), true),
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_revert_integrations(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            refresh_integrations(&app);
            set_status(&app, "Reloaded integration state.".into(), false);
        });
    }
}

fn wire_tray(app: &AppWindow, tray: &AppTray) {
    {
        let app_weak = app.as_weak();
        tray.on_open_window(move || {
            if let Some(app) = app_weak.upgrade() {
                app.show().ok();
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        tray.on_start_server(move || {
            let (is_err, msg) = match runstate::start() {
                Ok(()) => (false, "llama-server started.".to_string()),
                Err(e) => (true, format!("Failed to start: {e}")),
            };
            if let Some(app) = app_weak.upgrade() {
                set_status(&app, msg, is_err);
            }
            refresh_run_status(app_weak.clone(), tray_weak.clone());
        });
    }
    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        tray.on_stop_server(move || {
            stop_server_async(app_weak.clone(), tray_weak.clone());
        });
    }
    tray.on_quit_app(|| {
        slint::quit_event_loop().ok();
    });
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Bring the running instance's window to the front: re-show it if it was hidden
/// to the tray, un-minimize, and take focus. Called when a second launch pokes
/// the single-instance activation event.
#[cfg(windows)]
fn activate_window(app: AppWindow) {
    use slint::winit_030::WinitWindowAccessor;
    app.show().ok();
    app.window().with_winit_window(|w| {
        w.set_visible(true);
        w.set_minimized(false);
        w.focus_window();
    });
}

fn pick_dir(start: &std::path::Path) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Pick a folder")
        .set_directory(start)
        .pick_folder()
}

// ── Server helpers ───────────────────────────────────────────────────

fn load_server_into_ui(app: &AppWindow) {
    let cfg = server_cfg::load();
    app.global::<AppState>().set_server_port(SharedString::from(
        cfg.port
            .map(|v| v.to_string())
            .unwrap_or_else(|| "8080".into()),
    ));
    let hostname = cfg.hostname.unwrap_or_else(|| "localhost".into());
    populate_bind_options(app, &hostname);
    app.global::<AppState>()
        .set_server_hostname(SharedString::from(hostname));
    app.global::<AppState>()
        .set_server_mlock(cfg.mlock.unwrap_or(true));
    app.global::<AppState>()
        .set_server_threads(SharedString::from(
            cfg.threads.map(|v| v.to_string()).unwrap_or_default(),
        ));
    app.global::<AppState>()
        .set_server_cache_reuse(SharedString::from(
            cfg.cache_reuse.map(|v| v.to_string()).unwrap_or_default(),
        ));
    app.global::<AppState>()
        .set_server_threads_batch(SharedString::from(
            cfg.threads_batch.map(|v| v.to_string()).unwrap_or_default(),
        ));
    app.global::<AppState>()
        .set_server_models_max(SharedString::from(
            cfg.models_max.map(|v| v.to_string()).unwrap_or_default(),
        ));
    app.global::<AppState>()
        .set_server_models_dir(SharedString::from(
            cfg.models_dir
                .unwrap_or_else(server_cfg::default_models_dir),
        ));
    app.global::<AppState>()
        .set_server_device(SharedString::from(cfg.device.unwrap_or_default()));
    app.global::<AppState>()
        .set_server_split_mode(SharedString::from(
            // "default" is the combo's sentinel for "inherit / layer"; the combo
            // two-way-binds to this, so store the sentinel rather than "".
            cfg.split_mode.unwrap_or_else(|| "default".into()),
        ));
    app.global::<AppState>()
        .set_server_tensor_split(SharedString::from(cfg.tensor_split.unwrap_or_default()));
    let cmdline = runstate::command_line().unwrap_or_default();
    app.global::<AppState>()
        .set_server_command_line(SharedString::from(cmdline));
    snapshot_server_base(app);
}

fn populate_bind_options(app: &AppWindow, current: &str) {
    let mut opts = net_ifaces::list_options();
    let mut index = opts.iter().position(|o| o.value == current);
    if index.is_none() && !current.is_empty() {
        opts.insert(
            2,
            net_ifaces::BindOption {
                label: format!("{current} (no longer present)"),
                value: current.to_string(),
            },
        );
        index = Some(2);
    }
    let labels: Vec<SharedString> = opts.iter().map(|o| o.label.clone().into()).collect();
    let values: Vec<SharedString> = opts.iter().map(|o| o.value.clone().into()).collect();
    app.global::<AppState>()
        .set_bind_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
    app.global::<AppState>()
        .set_bind_values(ModelRc::from(Rc::new(VecModel::from(values))));
    app.global::<AppState>()
        .set_bind_index(index.unwrap_or(0) as i32);
}

fn read_server_from_ui(app: &AppWindow) -> server_cfg::ServerConfig {
    server_cfg::ServerConfig {
        port: ini::parse_int(app.global::<AppState>().get_server_port().as_str()),
        hostname: Some(app.global::<AppState>().get_server_hostname().to_string()),
        mlock: Some(app.global::<AppState>().get_server_mlock()),
        threads: ini::parse_int(app.global::<AppState>().get_server_threads().as_str()),
        cache_reuse: ini::parse_int(app.global::<AppState>().get_server_cache_reuse().as_str()),
        threads_batch: ini::parse_int(app.global::<AppState>().get_server_threads_batch().as_str()),
        models_max: ini::parse_int(app.global::<AppState>().get_server_models_max().as_str()),
        models_dir: Some(app.global::<AppState>().get_server_models_dir().to_string()),
        device: {
            let d = app.global::<AppState>().get_server_device().to_string();
            if d.trim().is_empty() {
                None
            } else {
                Some(d)
            }
        },
        split_mode: {
            // "" and the combo sentinel "default" both mean "no explicit split".
            let s = app.global::<AppState>().get_server_split_mode().to_string();
            match s.trim() {
                "" | "default" => None,
                other => Some(other.to_string()),
            }
        },
        tensor_split: {
            let s = app
                .global::<AppState>()
                .get_server_tensor_split()
                .to_string();
            if s.trim().is_empty() {
                None
            } else {
                Some(s)
            }
        },
    }
}

/// Copy the current server fields into their `*_base` baselines so the UI's
/// `server_dirty` reads false until the user edits something again.
fn snapshot_server_base(app: &AppWindow) {
    app.global::<AppState>()
        .set_server_port_base(app.global::<AppState>().get_server_port());
    app.global::<AppState>()
        .set_server_hostname_base(app.global::<AppState>().get_server_hostname());
    app.global::<AppState>()
        .set_server_mlock_base(app.global::<AppState>().get_server_mlock());
    app.global::<AppState>()
        .set_server_threads_base(app.global::<AppState>().get_server_threads());
    app.global::<AppState>()
        .set_server_cache_reuse_base(app.global::<AppState>().get_server_cache_reuse());
    app.global::<AppState>()
        .set_server_threads_batch_base(app.global::<AppState>().get_server_threads_batch());
    app.global::<AppState>()
        .set_server_models_max_base(app.global::<AppState>().get_server_models_max());
    app.global::<AppState>()
        .set_server_models_dir_base(app.global::<AppState>().get_server_models_dir());
    app.global::<AppState>()
        .set_server_device_base(app.global::<AppState>().get_server_device());
    app.global::<AppState>()
        .set_server_split_mode_base(app.global::<AppState>().get_server_split_mode());
    app.global::<AppState>()
        .set_server_tensor_split_base(app.global::<AppState>().get_server_tensor_split());
}

// ── Preset helpers ───────────────────────────────────────────────────

/// `true` if the preset matches the (case-insensitive) filter on id or model.
fn preset_matches(p: &presets::Preset, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let q = filter.to_lowercase();
    p.id.to_lowercase().contains(&q) || p.model.to_lowercase().contains(&q)
}

/// Build the `[PresetSummary]` list model, dropping rows that don't match the
/// filter. Each surviving row carries its `orig_index` into `state.presets` so
/// `select_preset()` and the selection highlight stay correct under a filter
/// (the list `for` index is NOT stable once rows are removed).
fn preset_summaries(presets: &[presets::Preset], filter: &str) -> Vec<PresetSummary> {
    presets
        .iter()
        .enumerate()
        .filter(|(_, p)| preset_matches(p, filter))
        .map(|(i, p)| PresetSummary {
            id: p.id.clone().into(),
            model: p.model.clone().into(),
            orig_index: i as i32,
        })
        .collect()
}

fn refresh_presets(app: &AppWindow, state: &Rc<RefCell<State>>) {
    reload_presets(app, state, None);
}

/// Reload `presets.ini` into `state`, rebuild the (filtered) list model, then
/// pick the selection and apply its form:
/// - `want = Some(id)` selects that preset if it exists (used after save/clone/rename);
/// - `want = None` keeps the current index if still in range (a plain refresh);
/// - either way it falls back to the first preset, or `-1` + a blank form when
///   the list is empty.
///
/// Callers that also need to refresh the file/device dropdowns or integrations
/// do so themselves afterward — this only owns the preset list + selection.
fn reload_presets(app: &AppWindow, state: &Rc<RefCell<State>>, want: Option<&str>) {
    let all = presets::load_all();
    let summaries = preset_summaries(&all, app.global::<AppState>().get_presets_filter().as_str());
    app.global::<AppState>()
        .set_presets(ModelRc::from(Rc::new(VecModel::from(summaries))));

    let prev_sel = app.global::<AppState>().get_selected_preset_index();
    let idx = match want {
        Some(id) => all.iter().position(|p| p.id == id).map(|i| i as i32),
        None => (prev_sel >= 0 && (prev_sel as usize) < all.len()).then_some(prev_sel),
    }
    .unwrap_or(if all.is_empty() { -1 } else { 0 });

    state.borrow_mut().presets = all;
    app.global::<AppState>().set_selected_preset_index(idx);

    let st = state.borrow();
    match usize::try_from(idx).ok().and_then(|i| st.presets.get(i)) {
        Some(p) => apply_form(app, preset_to_form(p)),
        None => apply_form(app, blank_form()),
    }
}

fn refresh_file_options(app: &AppWindow) {
    let models_dir = app.global::<AppState>().get_server_models_dir().to_string();
    let form = app.global::<AppState>().get_form();

    let model_scan_result = model_scan::list(&models_dir, model_scan::Category::Model.subdir());
    let mmproj_scan_result = model_scan::list(&models_dir, model_scan::Category::Mmproj.subdir());
    let mtp_scan_result = model_scan::list(&models_dir, model_scan::Category::Mtp.subdir());
    let dflash_scan_result = model_scan::list(&models_dir, model_scan::Category::Dflash.subdir());

    apply_scanned(
        app,
        model_scan::Category::Model,
        model_scan_result,
        form.model.as_str(),
        |app, lbl, val, idx| {
            app.global::<AppState>().set_model_labels(lbl);
            app.global::<AppState>().set_model_values(val);
            app.global::<AppState>().set_model_index(idx);
        },
    );
    apply_scanned(
        app,
        model_scan::Category::Mmproj,
        mmproj_scan_result,
        form.mmproj.as_str(),
        |app, lbl, val, idx| {
            app.global::<AppState>().set_mmproj_labels(lbl);
            app.global::<AppState>().set_mmproj_values(val);
            app.global::<AppState>().set_mmproj_index(idx);
        },
    );
    // Draft picker: MTP heads (mtps\) and DFlash drafters (dflashs\) share one
    // dropdown (both feed --model-draft); `draft_specs` carries the matching
    // --spec-type the UI applies when a row is picked.
    let (draft_labels, draft_values, draft_specs, draft_idx) = model_scan::build_draft_options(
        mtp_scan_result,
        dflash_scan_result,
        form.model_draft.as_str(),
        form.spec_type.as_str(),
    );
    app.global::<AppState>()
        .set_draft_labels(string_model(draft_labels));
    app.global::<AppState>()
        .set_draft_values(string_model(draft_values));
    app.global::<AppState>()
        .set_draft_specs(string_model(draft_specs));
    app.global::<AppState>().set_draft_index(draft_idx);

    refresh_device_options(app);
    update_model_info(app);
}

/// Fill the read-only "Model info" box from the selected model's GGUF header
/// (read via `ggml-base.dll`), enriched with the selected mmproj and draft
/// headers plus a cross-reference of the framework's MTP/DFlash drafters. Called
/// whenever the model/mmproj/draft field changes (combo pick, preset load).
fn update_model_info(app: &AppWindow) {
    let form = app.global::<AppState>().get_form();
    let model = form.model.to_string();

    // Reset the optional rows; the success path re-enables the ones that apply.
    app.global::<AppState>().set_model_info_has_moe(false);
    app.global::<AppState>().set_model_info_has_mmproj(false);
    app.global::<AppState>()
        .set_model_info_has_draft_file(false);
    app.global::<AppState>().set_model_info_embeds_mtp(false);
    // Reset the slider maxima; 0 = unknown → the UI falls back to a 0..99 range.
    app.global::<AppState>().set_model_info_n_layer(0);
    app.global::<AppState>().set_model_info_draft_n_layer(0);

    if model.trim().is_empty() {
        app.global::<AppState>().set_model_info_ready(false);
        app.global::<AppState>()
            .set_model_info_note(SharedString::from("Select a model to see its details."));
        return;
    }

    let Some(info) = gguf::read_model_info(std::path::Path::new(&model)) else {
        app.global::<AppState>().set_model_info_ready(false);
        app.global::<AppState>()
            .set_model_info_note(SharedString::from(
            "Metadata unavailable — is ggml-base.dll beside the app, and the file a valid GGUF?",
        ));
        return;
    };

    let models_dir = app.global::<AppState>().get_server_models_dir().to_string();
    let ext = gguf::external_drafters(&models_dir, &model);
    app.global::<AppState>()
        .set_model_info_kind(SharedString::from(info.kind_line()));
    app.global::<AppState>()
        .set_model_info_n_layer(info.n_layer as i32);
    app.global::<AppState>().set_model_info_has_moe(info.is_moe);
    app.global::<AppState>()
        .set_model_info_moe(SharedString::from(info.moe_offload_line()));
    app.global::<AppState>()
        .set_model_info_arch_quant(SharedString::from(info.arch_quant_line()));
    app.global::<AppState>()
        .set_model_info_layers_ctx(SharedString::from(info.layers_ctx_line()));
    app.global::<AppState>()
        .set_model_info_attn(SharedString::from(info.attn_line()));
    app.global::<AppState>()
        .set_model_info_draft(SharedString::from(gguf::draft_line(&info, &ext)));
    // Enables the speculative-decoding controls even before an external draft is
    // picked, when the model itself embeds MTP/nextn heads.
    app.global::<AppState>()
        .set_model_info_embeds_mtp(info.nextn_predict_layers > 0);

    // Optional: the selected mmproj's clip header.
    let mmproj = form.mmproj.to_string();
    if !mmproj.trim().is_empty() {
        if let Some(mp) = gguf::read_mmproj_info(std::path::Path::new(&mmproj)) {
            app.global::<AppState>()
                .set_model_info_mmproj(SharedString::from(mp.mmproj_line()));
            app.global::<AppState>().set_model_info_has_mmproj(true);
        }
    }

    // Optional: the selected draft/MTP/DFlash file's own header.
    let draft = form.model_draft.to_string();
    if !draft.trim().is_empty() {
        if let Some(d) = gguf::read_model_info(std::path::Path::new(&draft)) {
            app.global::<AppState>()
                .set_model_info_draft_file(SharedString::from(d.draft_file_line()));
            app.global::<AppState>()
                .set_model_info_draft_n_layer(d.n_layer as i32);
            app.global::<AppState>().set_model_info_has_draft_file(true);
        }
    }

    app.global::<AppState>().set_model_info_ready(true);
}

/// Rebuild the three GPU-device dropdowns (server-wide, per-preset main,
/// per-preset draft) from the cached `--list-devices` result, recomputing each
/// selected index against the current server.ini / form values.
fn refresh_device_options(app: &AppWindow) {
    let devs = cached_devices(app);
    let form = app.global::<AppState>().get_form();

    apply_device(
        app,
        &devs,
        app.global::<AppState>().get_server_device().as_str(),
        "(all detected devices)",
        |app, lbl, val, idx| {
            app.global::<AppState>().set_server_dev_labels(lbl);
            app.global::<AppState>().set_server_dev_values(val);
            app.global::<AppState>().set_server_dev_index(idx);
        },
    );
    apply_device(
        app,
        &devs,
        form.device.as_str(),
        "(server default)",
        |app, lbl, val, idx| {
            app.global::<AppState>().set_pdev_labels(lbl);
            app.global::<AppState>().set_pdev_values(val);
            app.global::<AppState>().set_pdev_index(idx);
        },
    );
    apply_device(
        app,
        &devs,
        form.device_draft.as_str(),
        "(auto / same as model)",
        |app, lbl, val, idx| {
            app.global::<AppState>().set_pdraft_labels(lbl);
            app.global::<AppState>().set_pdraft_values(val);
            app.global::<AppState>().set_pdraft_index(idx);
        },
    );
}

/// Reconstruct the cached device list from the two parallel Slint arrays the
/// async probe fills in (`all_device_ids` / `all_device_labels`).
fn cached_devices(app: &AppWindow) -> Vec<devices::DeviceOption> {
    let ids = app.global::<AppState>().get_all_device_ids();
    let labels = app.global::<AppState>().get_all_device_labels();
    let n = ids.row_count().min(labels.row_count());
    (0..n)
        .filter_map(|i| {
            Some(devices::DeviceOption {
                id: ids.row_data(i)?.to_string(),
                label: labels.row_data(i)?.to_string(),
            })
        })
        .collect()
}

/// Wrap a `Vec<String>` as a Slint string model (the `[string]` properties the
/// dropdowns bind to).
fn string_model(items: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::from(Rc::new(VecModel::from(
        items
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )))
}

fn apply_device(
    app: &AppWindow,
    devs: &[devices::DeviceOption],
    current: &str,
    empty_label: &str,
    apply: impl FnOnce(&AppWindow, ModelRc<SharedString>, ModelRc<SharedString>, i32),
) {
    let (labels, values, idx) = devices::build_options(devs, current, empty_label);
    apply(app, string_model(labels), string_model(values), idx);
}

fn apply_scanned(
    app: &AppWindow,
    category: model_scan::Category,
    scanned: Vec<model_scan::FileOption>,
    current: &str,
    apply: impl FnOnce(&AppWindow, ModelRc<SharedString>, ModelRc<SharedString>, i32),
) {
    let (labels, values, idx) = model_scan::build_options(category, scanned, current);
    apply(app, string_model(labels), string_model(values), idx);
}

// ── Dialog helpers ───────────────────────────────────────────────────

fn populate_dialog_models(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let models_dir = app.global::<AppState>().get_server_models_dir().to_string();
    let scanned = model_scan::list(&models_dir, model_scan::Category::Model.subdir());
    state.borrow_mut().dialog_models_all = scanned;
    app.global::<AppState>()
        .set_dialog_filter(SharedString::from(""));
    let st = state.borrow();
    apply_dialog_models(app, &st.dialog_models_all, "");
}

/// Filter the cached dialog model scan by a case-insensitive substring on the
/// label and publish the result. Filtering both arrays together keeps
/// `dialog_model_index` consistent with `dialog_model_values`.
fn apply_dialog_models(app: &AppWindow, all: &[model_scan::FileOption], filter: &str) {
    let q = filter.to_lowercase();
    let labels: Vec<SharedString> = all
        .iter()
        .filter(|f| q.is_empty() || f.label.to_lowercase().contains(&q))
        .map(|f| SharedString::from(f.label.clone()))
        .collect();
    let values: Vec<SharedString> = all
        .iter()
        .filter(|f| q.is_empty() || f.label.to_lowercase().contains(&q))
        .map(|f| SharedString::from(f.path.clone()))
        .collect();
    app.global::<AppState>()
        .set_dialog_model_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
    app.global::<AppState>()
        .set_dialog_model_values(ModelRc::from(Rc::new(VecModel::from(values))));
    app.global::<AppState>().set_dialog_model_index(-1);
}

fn picked_dialog_model_path(app: &AppWindow) -> Option<PathBuf> {
    let idx = app.global::<AppState>().get_dialog_model_index();
    if idx < 0 {
        return None;
    }
    let values = app.global::<AppState>().get_dialog_model_values();
    let i = usize::try_from(idx).ok()?;
    if i >= values.row_count() {
        return None;
    }
    let s = values.row_data(i)?;
    Some(PathBuf::from(s.to_string()))
}

fn run_new_empty(app: &AppWindow, state: &Rc<RefCell<State>>, path: PathBuf) {
    let id = presets::make_id(&path.to_string_lossy());
    let p = presets::Preset::new_default(id.clone(), path.to_string_lossy().into_owned());
    commit_new_preset(
        app,
        state,
        p,
        format!("Added [{id}] — tweak parameters and Save."),
    );
}

fn run_new_clone(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    base: presets::Preset,
    path: PathBuf,
) {
    let path_str = path.to_string_lossy().into_owned();
    let base_id = presets::make_id(&path_str);
    // The id is derived from the picked model, so cloning onto the same model
    // (or one that already has a preset) would otherwise overwrite it. Pick the
    // first free "<id>", "<id>-2", … instead of clobbering.
    let existing: Vec<String> = presets::load_all().into_iter().map(|p| p.id).collect();
    let id = unique_id(&base_id, &existing);
    let cloned = presets::Preset {
        id: id.clone(),
        model: path_str,
        ..base.clone()
    };
    commit_new_preset(
        app,
        state,
        cloned,
        format!("Cloned [{}] -> [{id}] (same parameters) — saved.", base.id),
    );
}

/// First of `base`, `base-2`, `base-3`, … that isn't already in `existing`.
fn unique_id(base: &str, existing: &[String]) -> String {
    if !existing.iter().any(|e| e == base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|cand| !existing.iter().any(|e| e == cand))
        .unwrap_or_else(|| base.to_string())
}

fn commit_new_preset(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    p: presets::Preset,
    success_status: String,
) {
    match presets::save(&p) {
        Ok(()) => {
            reload_presets(app, state, Some(&p.id));
            refresh_file_options(app);
            refresh_integrations(app);
            set_status(app, success_status, false);
        }
        Err(e) => set_status(app, format!("Save failed: {e}"), true),
    }
}

// ── Status / version ─────────────────────────────────────────────────

fn set_status(app: &AppWindow, text: String, is_error: bool) {
    app.global::<AppState>()
        .set_status_text(SharedString::from(text));
    app.global::<AppState>().set_status_is_error(is_error);
}

fn spawn_version_probe(app_weak: slint::Weak<AppWindow>) {
    std::thread::spawn(move || {
        let version = server_version::probe();
        slint::invoke_from_event_loop(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            app.global::<AppState>()
                .set_server_version(SharedString::from(version.unwrap_or_default()));
        })
        .ok();
    });
}

/// Enumerate GPU devices off the UI thread (`--list-devices` spawns llama-server
/// and can take a few hundred ms — including a CUDA init), then publish the
/// result and rebuild the device dropdowns via the event loop.
fn spawn_device_probe(app_weak: slint::Weak<AppWindow>) {
    std::thread::spawn(move || {
        let list = devices::list();
        let ids: Vec<SharedString> = list.iter().map(|d| d.id.clone().into()).collect();
        let labels: Vec<SharedString> = list.iter().map(|d| d.label.clone().into()).collect();
        slint::invoke_from_event_loop(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            app.global::<AppState>()
                .set_all_device_ids(ModelRc::from(Rc::new(VecModel::from(ids))));
            app.global::<AppState>()
                .set_all_device_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
            // Recompute the dropdown indices now that the device list exists.
            refresh_device_options(&app);
        })
        .ok();
    });
}

/// Trigger a stop and drive the transitional "Stopping…" state.
///
/// The forced `taskkill` returns quickly, but the process can linger in
/// `tasklist` for a second or two while its GPU context unwinds — so the kill
/// and the wait-for-exit run off the UI thread and `server_stopping` stays true
/// until the process actually disappears. The wait is capped so a wedged
/// process can't pin the UI in "Stopping…" forever.
fn stop_server_async(app_weak: slint::Weak<AppWindow>, tray_weak: slint::Weak<AppTray>) {
    if let Some(app) = app_weak.upgrade() {
        app.global::<AppState>().set_server_stopping(true);
        set_status(&app, "Stopping llama-server…".into(), false);
    }
    std::thread::spawn(move || {
        let result = runstate::stop();
        let mut running = runstate::load().is_some();
        let step = std::time::Duration::from_millis(300);
        let cap = std::time::Duration::from_secs(15);
        let mut waited = std::time::Duration::ZERO;
        while running && waited < cap {
            std::thread::sleep(step);
            waited += step;
            running = runstate::load().is_some();
        }
        slint::invoke_from_event_loop(move || {
            if let Some(app) = app_weak.upgrade() {
                app.global::<AppState>().set_server_stopping(false);
                app.global::<AppState>().set_server_running(running);
                match result {
                    Ok(()) if !running => {
                        app.global::<AppState>().set_server_status_is_error(false);
                        set_status(&app, "llama-server stopped.".into(), false);
                    }
                    Ok(()) => {
                        app.global::<AppState>().set_server_status_is_error(true);
                        set_status(
                            &app,
                            "Stop timed out — llama-server is still running.".into(),
                            true,
                        );
                    }
                    Err(e) => {
                        app.global::<AppState>().set_server_status_is_error(true);
                        set_status(&app, format!("Failed to stop: {e}"), true);
                    }
                }
            }
            if let Some(tray) = tray_weak.upgrade() {
                tray.set_server_running(running);
            }
        })
        .ok();
    });
}

/// Probe the llama-server process off the UI thread (`tasklist` can take
/// hundreds of ms) and apply the result via the event loop, mirroring
/// `spawn_version_probe`.
fn refresh_run_status(app_weak: slint::Weak<AppWindow>, tray_weak: slint::Weak<AppTray>) {
    std::thread::spawn(move || {
        let running = runstate::load().is_some();
        slint::invoke_from_event_loop(move || {
            if let Some(app) = app_weak.upgrade() {
                app.global::<AppState>().set_server_running(running);
                app.global::<AppState>().set_server_status_is_error(false);
            }
            if let Some(tray) = tray_weak.upgrade() {
                tray.set_server_running(running);
            }
        })
        .ok();
    });
}

// ── Form <-> Preset conversion ───────────────────────────────────────

/// Set the editable form AND its baseline together, so the UI's `preset_dirty`
/// (`form != form_base`) reads false right after a (re)load or save and only
/// turns true once the user actually edits a field.
fn apply_form(app: &AppWindow, form: PresetForm) {
    app.global::<AppState>().set_form_base(form.clone());
    app.global::<AppState>().set_form(form);
}

// ── Integrations helpers ──────────────────────────────────────────────

fn refresh_integrations(app: &AppWindow) {
    let cfg = server_cfg::load();
    let port = cfg.port.unwrap_or(8080);
    let hostname = cfg.hostname.unwrap_or_else(|| "localhost".into());
    let base_url = format!("http://{hostname}:{port}/v1");
    app.global::<AppState>()
        .set_integration_base_url(SharedString::from(base_url));

    let claude_env = integrations::claude_code_env_script(&format!("http://{hostname}:{port}/v1"));
    app.global::<AppState>()
        .set_integration_claude_env(SharedString::from(claude_env));

    let active = integrations::detect_opencode_provider();
    app.global::<AppState>()
        .set_integration_provider_active(active);

    let enabled_ids = integrations::opencode_model_ids();
    let all_presets = presets::load_all();

    let mut items: Vec<IntegrationModel> = Vec::new();
    for p in &all_presets {
        let label = integrations::friendly_model_name(&p.id, &p.model);
        items.push(IntegrationModel {
            id: SharedString::from(p.id.clone()),
            label: SharedString::from(label),
            enabled: enabled_ids.contains(&p.id),
        });
    }
    let rc = Rc::new(VecModel::from(items));
    app.global::<AppState>()
        .set_integration_models(ModelRc::from(rc));
}
