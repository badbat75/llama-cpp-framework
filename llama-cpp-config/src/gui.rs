// Slint GUI wiring for llama.cpp-framework configurator.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::{
    devices, ini, integrations, model_scan, net_ifaces, paths, presets, runstate, server_cfg,
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
    let app = AppWindow::new()?;
    let tray = AppTray::new()?;
    let state = Rc::new(RefCell::new(State::default()));

    load_server_into_ui(&app);
    refresh_presets(&app, &state);
    refresh_run_status(app.as_weak(), tray.as_weak());
    refresh_file_options(&app);
    spawn_version_probe(app.as_weak());
    spawn_device_probe(app.as_weak());

    {
        let app_weak = app.as_weak();
        app.on_sync_device_dropdowns(move || {
            if let Some(app) = app_weak.upgrade() {
                refresh_device_options(&app);
            }
        });
    }

    app.set_presets_path(SharedString::from(
        paths::presets_ini().to_string_lossy().into_owned(),
    ));

    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        app.on_refresh_status(move || {
            refresh_run_status(app_weak.clone(), tray_weak.clone());
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

    // ── Server tab callbacks ─────────────────────────────────────────
    {
        let app_weak = app.as_weak();
        app.on_save_server(move || {
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
                    app.set_server_command_line(SharedString::from(cmdline));
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
        app.on_revert_server(move || {
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
        app.on_browse_models_dir(move |current| {
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
        app.on_start_server(move || {
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
                    app.set_server_status_is_error(true);
                }
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        app.on_stop_server(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            match runstate::stop() {
                Ok(()) => {
                    set_status(&app, "llama-server stopped.".into(), false);
                    refresh_run_status(app.as_weak(), tray_weak.clone());
                }
                Err(e) => {
                    set_status(&app, format!("Failed to stop: {e}"), true);
                    app.set_server_status_is_error(true);
                }
            }
        });
    }

    // ── Models tab callbacks ─────────────────────────────────────────
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.on_select_preset(move |index| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let st = state.borrow();
            if let Some(p) = usize::try_from(index)
                .ok()
                .and_then(|i| st.presets.get(i))
            {
                app.set_selected_preset_index(index);
                apply_form(&app, preset_to_form(p));
                drop(st);
                refresh_file_options(&app);
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.on_save_preset(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let p = form_to_preset(&app.get_form());
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
                        app.set_selected_preset_index(i as i32);
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
        app.on_revert_preset(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            refresh_presets(&app, &state);
            let idx = app.get_selected_preset_index();
            let st = state.borrow();
            if let Some(p) = usize::try_from(idx)
                .ok()
                .and_then(|i| st.presets.get(i))
            {
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
        app.on_delete_preset(move |id| {
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
                    app.set_selected_preset_index(-1);
                    apply_form(&app, blank_form());
                    refresh_file_options(&app);
                    refresh_integrations(&app);
                }
                Err(e) => set_status(&app, format!("Delete failed: {e}"), true),
            }
        });
    }

    let pending_clone_base: Rc<RefCell<Option<presets::Preset>>> = Rc::new(RefCell::new(None));
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        let pending_clone_base = pending_clone_base.clone();
        app.on_new_preset(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let selected = {
                let st = state.borrow();
                let idx = app.get_selected_preset_index();
                usize::try_from(idx)
                    .ok()
                    .and_then(|i| st.presets.get(i))
                    .cloned()
            };
            populate_dialog_models(&app, &state);
            match selected {
                None => {
                    *pending_clone_base.borrow_mut() = None;
                    app.set_new_dialog_source_id(SharedString::from(""));
                }
                Some(p) => {
                    app.set_new_dialog_source_id(SharedString::from(p.id.clone()));
                    *pending_clone_base.borrow_mut() = Some(p);
                }
            }
            app.set_show_new_kind_picker(true);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        let pending_clone_base = pending_clone_base.clone();
        app.on_pick_new_empty(move || {
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
        app.on_pick_new_clone(move || {
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
        app.on_rename_preset(move |old_id, new_id| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            match presets::rename(old_id.as_str(), new_id.as_str()) {
                Ok(()) => {
                    set_status(
                        &app,
                        format!("Renamed [{old_id}] -> [{new_id}]"),
                        false,
                    );
                    let all = presets::load_all();
                    let summaries = preset_summaries(&all, app.get_presets_filter().as_str());
                    app.set_presets(ModelRc::from(Rc::new(VecModel::from(summaries))));
                    let new_idx = all
                        .iter()
                        .position(|q| q.id == new_id.as_str())
                        .map(|i| i as i32)
                        .unwrap_or(-1);
                    let renamed = all.iter().find(|q| q.id == new_id.as_str()).cloned();
                    state.borrow_mut().presets = all;
                    app.set_selected_preset_index(new_idx);
                    if let Some(p) = renamed {
                        apply_form(&app, preset_to_form(&p));
                    }
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
        app.on_filter_presets(move |q| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let st = state.borrow();
            let summaries = preset_summaries(&st.presets, q.as_str());
            app.set_presets(ModelRc::from(Rc::new(VecModel::from(summaries))));
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.on_filter_dialog_models(move |q| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let st = state.borrow();
            apply_dialog_models(&app, &st.dialog_models_all, q.as_str());
        });
    }

    // ── Integrations tab callbacks ────────────────────────────────────
    refresh_integrations(&app);
    {
        let app_weak = app.as_weak();
        app.on_toggle_integration_model(move |index| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let idx = usize::try_from(index).unwrap_or(usize::MAX);
            let models = app.get_integration_models();
            if idx < models.iter().count() {
                let mut v: Vec<IntegrationModel> = models.iter().collect();
                if let Some(entry) = v.get_mut(idx) {
                    entry.enabled = !entry.enabled;
                }
                let rc = Rc::new(VecModel::from(v));
                app.set_integration_models(ModelRc::from(rc));
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.on_save_integrations(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let models = app.get_integration_models();
            let checked: Vec<String> = models
                .iter()
                .filter(|m| m.enabled)
                .map(|m| m.id.to_string())
                .collect();
            let base_url = app.get_integration_base_url().to_string();
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
        app.on_revert_integrations(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            refresh_integrations(&app);
            set_status(&app, "Reloaded integration state.".into(), false);
        });
    }

    // ── System-tray callbacks ─────────────────────────────────────────
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
            let (is_err, msg) = match runstate::stop() {
                Ok(()) => (false, "llama-server stopped.".to_string()),
                Err(e) => (true, format!("Failed to stop: {e}")),
            };
            if let Some(app) = app_weak.upgrade() {
                set_status(&app, msg, is_err);
            }
            refresh_run_status(app_weak.clone(), tray_weak.clone());
        });
    }
    tray.on_quit_app(|| {
        slint::quit_event_loop().ok();
    });

    // Closing the window hides it to the tray instead of quitting; the visible
    // tray icon keeps the event loop alive. Use the tray's "Quit" to exit.
    app.window()
        .on_close_requested(|| slint::CloseRequestResponse::HideWindow);

    app.show()?;
    tray.show()?;
    slint::run_event_loop()?;
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────

fn pick_dir(start: &std::path::Path) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Pick a folder")
        .set_directory(start)
        .pick_folder()
}

// ── Server helpers ───────────────────────────────────────────────────

fn load_server_into_ui(app: &AppWindow) {
    let cfg = server_cfg::load();
    app.set_server_port(SharedString::from(
        cfg.port.map(|v| v.to_string()).unwrap_or_else(|| "8080".into()),
    ));
    let hostname = cfg.hostname.unwrap_or_else(|| "localhost".into());
    populate_bind_options(app, &hostname);
    app.set_server_hostname(SharedString::from(hostname));
    app.set_server_mlock(cfg.mlock.unwrap_or(true));
    app.set_server_threads(SharedString::from(
        cfg.threads.map(|v| v.to_string()).unwrap_or_default(),
    ));
    app.set_server_cache_reuse(SharedString::from(
        cfg.cache_reuse.map(|v| v.to_string()).unwrap_or_default(),
    ));
    app.set_server_threads_batch(SharedString::from(
        cfg.threads_batch.map(|v| v.to_string()).unwrap_or_default(),
    ));
    app.set_server_models_max(SharedString::from(
        cfg.models_max.map(|v| v.to_string()).unwrap_or_default(),
    ));
    app.set_server_models_dir(SharedString::from(
        cfg.models_dir.unwrap_or_else(server_cfg::default_models_dir),
    ));
    app.set_server_device(SharedString::from(cfg.device.unwrap_or_default()));
    let cmdline = runstate::command_line().unwrap_or_default();
    app.set_server_command_line(SharedString::from(cmdline));
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
    app.set_bind_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
    app.set_bind_values(ModelRc::from(Rc::new(VecModel::from(values))));
    app.set_bind_index(index.unwrap_or(0) as i32);
}

fn read_server_from_ui(app: &AppWindow) -> server_cfg::ServerConfig {
    server_cfg::ServerConfig {
        port: ini::parse_int(app.get_server_port().as_str()),
        hostname: Some(app.get_server_hostname().to_string()),
        mlock: Some(app.get_server_mlock()),
        threads: ini::parse_int(app.get_server_threads().as_str()),
        cache_reuse: ini::parse_int(app.get_server_cache_reuse().as_str()),
        threads_batch: ini::parse_int(app.get_server_threads_batch().as_str()),
        models_max: ini::parse_int(app.get_server_models_max().as_str()),
        models_dir: Some(app.get_server_models_dir().to_string()),
        device: {
            let d = app.get_server_device().to_string();
            if d.trim().is_empty() { None } else { Some(d) }
        },
    }
}

/// Copy the current server fields into their `*_base` baselines so the UI's
/// `server_dirty` reads false until the user edits something again.
fn snapshot_server_base(app: &AppWindow) {
    app.set_server_port_base(app.get_server_port());
    app.set_server_hostname_base(app.get_server_hostname());
    app.set_server_mlock_base(app.get_server_mlock());
    app.set_server_threads_base(app.get_server_threads());
    app.set_server_cache_reuse_base(app.get_server_cache_reuse());
    app.set_server_threads_batch_base(app.get_server_threads_batch());
    app.set_server_models_max_base(app.get_server_models_max());
    app.set_server_models_dir_base(app.get_server_models_dir());
    app.set_server_device_base(app.get_server_device());
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

/// Build the `[PresetSummary]` model, marking each row visible per the filter.
/// Hiding (rather than removing) rows keeps indices aligned with `state.presets`
/// so `select_preset(i)` stays correct while a filter is active.
fn preset_summaries(presets: &[presets::Preset], filter: &str) -> Vec<PresetSummary> {
    presets
        .iter()
        .map(|p| PresetSummary {
            id: p.id.clone().into(),
            model: p.model.clone().into(),
            visible: preset_matches(p, filter),
        })
        .collect()
}

fn refresh_presets(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let presets = presets::load_all();
    let summaries = preset_summaries(&presets, app.get_presets_filter().as_str());

    let model = ModelRc::from(Rc::new(VecModel::from(summaries)));
    app.set_presets(model);

    let prev_sel = app.get_selected_preset_index();
    state.borrow_mut().presets = presets;

    let st = state.borrow();
    let len = st.presets.len() as i32;
    if prev_sel >= 0 && prev_sel < len {
        if let Some(p) = st.presets.get(prev_sel as usize) {
            apply_form(app, preset_to_form(p));
        }
    } else if len > 0 {
        app.set_selected_preset_index(0);
        if let Some(p) = st.presets.first() {
            apply_form(app, preset_to_form(p));
        }
    } else {
        app.set_selected_preset_index(-1);
        apply_form(app, blank_form());
    }
}

fn refresh_file_options(app: &AppWindow) {
    let models_dir = app.get_server_models_dir().to_string();
    let form = app.get_form();

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
            app.set_model_labels(lbl);
            app.set_model_values(val);
            app.set_model_index(idx);
        },
    );
    apply_scanned(
        app,
        model_scan::Category::Mmproj,
        mmproj_scan_result,
        form.mmproj.as_str(),
        |app, lbl, val, idx| {
            app.set_mmproj_labels(lbl);
            app.set_mmproj_values(val);
            app.set_mmproj_index(idx);
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
    app.set_draft_labels(string_model(draft_labels));
    app.set_draft_values(string_model(draft_values));
    app.set_draft_specs(string_model(draft_specs));
    app.set_draft_index(draft_idx);

    refresh_device_options(app);
}

/// Rebuild the three GPU-device dropdowns (server-wide, per-preset main,
/// per-preset draft) from the cached `--list-devices` result, recomputing each
/// selected index against the current server.ini / form values.
fn refresh_device_options(app: &AppWindow) {
    let devs = cached_devices(app);
    let form = app.get_form();

    apply_device(
        app,
        &devs,
        app.get_server_device().as_str(),
        "(all detected devices)",
        |app, lbl, val, idx| {
            app.set_server_dev_labels(lbl);
            app.set_server_dev_values(val);
            app.set_server_dev_index(idx);
        },
    );
    apply_device(
        app,
        &devs,
        form.device.as_str(),
        "(server default)",
        |app, lbl, val, idx| {
            app.set_pdev_labels(lbl);
            app.set_pdev_values(val);
            app.set_pdev_index(idx);
        },
    );
    apply_device(
        app,
        &devs,
        form.device_draft.as_str(),
        "(auto / same as model)",
        |app, lbl, val, idx| {
            app.set_pdraft_labels(lbl);
            app.set_pdraft_values(val);
            app.set_pdraft_index(idx);
        },
    );
}

/// Reconstruct the cached device list from the two parallel Slint arrays the
/// async probe fills in (`all_device_ids` / `all_device_labels`).
fn cached_devices(app: &AppWindow) -> Vec<devices::DeviceOption> {
    let ids = app.get_all_device_ids();
    let labels = app.get_all_device_labels();
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
        items.into_iter().map(SharedString::from).collect::<Vec<_>>(),
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
    let models_dir = app.get_server_models_dir().to_string();
    let scanned = model_scan::list(&models_dir, model_scan::Category::Model.subdir());
    state.borrow_mut().dialog_models_all = scanned;
    app.set_dialog_filter(SharedString::from(""));
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
    app.set_dialog_model_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
    app.set_dialog_model_values(ModelRc::from(Rc::new(VecModel::from(values))));
    app.set_dialog_model_index(-1);
}

fn picked_dialog_model_path(app: &AppWindow) -> Option<PathBuf> {
    let idx = app.get_dialog_model_index();
    if idx < 0 {
        return None;
    }
    let values = app.get_dialog_model_values();
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
    let id = presets::make_id(&path_str);
    let cloned = presets::Preset {
        id: id.clone(),
        model: path_str,
        ..base.clone()
    };
    commit_new_preset(
        app,
        state,
        cloned,
        format!("Cloned [{}] -> [{id}] (new model, same parameters) — saved.", base.id),
    );
}

fn commit_new_preset(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    p: presets::Preset,
    success_status: String,
) {
    match presets::save(&p) {
        Ok(()) => {
            let all = presets::load_all();
            let summaries = preset_summaries(&all, app.get_presets_filter().as_str());
            app.set_presets(ModelRc::from(Rc::new(VecModel::from(summaries))));
            let new_idx = all
                .iter()
                .position(|q| q.id == p.id)
                .map(|i| i as i32)
                .unwrap_or(-1);
            state.borrow_mut().presets = all;
            app.set_selected_preset_index(new_idx);
            apply_form(app, preset_to_form(&p));
            refresh_file_options(app);
            refresh_integrations(app);
            set_status(app, success_status, false);
        }
        Err(e) => set_status(app, format!("Save failed: {e}"), true),
    }
}

// ── Status / version ─────────────────────────────────────────────────

fn set_status(app: &AppWindow, text: String, is_error: bool) {
    app.set_status_text(SharedString::from(text));
    app.set_status_is_error(is_error);
}

fn spawn_version_probe(app_weak: slint::Weak<AppWindow>) {
    std::thread::spawn(move || {
        let version = server_version::probe();
        slint::invoke_from_event_loop(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            app.set_server_version(SharedString::from(version.unwrap_or_default()));
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
            app.set_all_device_ids(ModelRc::from(Rc::new(VecModel::from(ids))));
            app.set_all_device_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
            // Recompute the dropdown indices now that the device list exists.
            refresh_device_options(&app);
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
                app.set_server_running(running);
                app.set_server_status_is_error(false);
            }
            if let Some(tray) = tray_weak.upgrade() {
                tray.set_server_running(running);
            }
        })
        .ok();
    });
}


// ── Form <-> Preset conversion ───────────────────────────────────────

fn blank_form() -> PresetForm {
    PresetForm::default()
}

/// Set the editable form AND its baseline together, so the UI's `preset_dirty`
/// (`form != form_base`) reads false right after a (re)load or save and only
/// turns true once the user actually edits a field.
fn apply_form(app: &AppWindow, form: PresetForm) {
    app.set_form_base(form.clone());
    app.set_form(form);
}

fn preset_to_form(p: &presets::Preset) -> PresetForm {
    PresetForm {
        id: p.id.clone().into(),
        model: p.model.clone().into(),
        mmproj: p.mmproj.clone().into(),
        model_draft: p.model_draft.clone().into(),
        spec_type: if p.spec_type.is_empty() {
            "none".into()
        } else {
            p.spec_type.clone().into()
        },
        spec_draft_n_max: p
            .spec_draft_n_max
            .map(|v| v.to_string())
            .unwrap_or_default()
            .into(),
        n_gpu_layers_draft: p
            .n_gpu_layers_draft
            .map(|v| v.to_string())
            .unwrap_or_default()
            .into(),
        device_draft: p.device_draft.clone().into(),
        device: p.device.clone().into(),
        ctx_size: p.ctx_size.unwrap_or(32768),
        n_gpu_layers: p.n_gpu_layers.unwrap_or(99),
        parallel: p.parallel.unwrap_or(4),
        batch_size: p.batch_size.unwrap_or(512),
        ubatch_size: p.ubatch_size.unwrap_or(512),
        cache_type_k: if p.cache_type_k.is_empty() {
            "q8_0".into()
        } else {
            p.cache_type_k.clone().into()
        },
        cache_type_v: if p.cache_type_v.is_empty() {
            "q8_0".into()
        } else {
            p.cache_type_v.clone().into()
        },
        flash_attn: p.flash_attn.unwrap_or(true),
        cache_ram: p.cache_ram.unwrap_or(8192),
        jinja: p.jinja.unwrap_or(true),
        reasoning: if p.reasoning.is_empty() {
            "auto".into()
        } else {
            p.reasoning.clone().into()
        },
        reasoning_format: if p.reasoning_format.is_empty() {
            "auto".into()
        } else {
            p.reasoning_format.clone().into()
        },
        n_cpu_moe: p.n_cpu_moe.map(|v| v.to_string()).unwrap_or_default().into(),
        temp: p.temp.map(|v| v.to_string()).unwrap_or_default().into(),
        top_k: p.top_k.map(|v| v.to_string()).unwrap_or_default().into(),
        top_p: p.top_p.map(|v| v.to_string()).unwrap_or_default().into(),
        min_p: p.min_p.map(|v| v.to_string()).unwrap_or_default().into(),
        repeat_penalty: p.repeat_penalty.map(|v| v.to_string()).unwrap_or_default().into(),
        presence_penalty: p.presence_penalty
            .map(|v| v.to_string())
            .unwrap_or_default()
            .into(),
        chat_template_kwargs: p.chat_template_kwargs.clone().into(),
    }
}

fn form_to_preset(f: &PresetForm) -> presets::Preset {
    presets::Preset {
        id: f.id.to_string(),
        model: f.model.to_string(),
        mmproj: f.mmproj.to_string(),
        model_draft: f.model_draft.to_string(),
        spec_type: match f.spec_type.as_str() {
            "" | "none" => String::new(),
            other => other.to_string(),
        },
        spec_draft_n_max: ini::parse_int(f.spec_draft_n_max.as_str()),
        n_gpu_layers_draft: ini::parse_int(f.n_gpu_layers_draft.as_str()),
        device_draft: f.device_draft.to_string(),
        device: f.device.to_string(),
        ctx_size: Some(f.ctx_size).filter(|v| *v > 0),
        n_gpu_layers: Some(f.n_gpu_layers).filter(|v| *v > 0),
        parallel: Some(f.parallel).filter(|v| *v > 0),
        batch_size: Some(f.batch_size).filter(|v| *v > 0),
        ubatch_size: Some(f.ubatch_size).filter(|v| *v > 0),
        cache_type_k: f.cache_type_k.to_string(),
        cache_type_v: f.cache_type_v.to_string(),
        flash_attn: Some(f.flash_attn),
        cache_ram: Some(f.cache_ram).filter(|v| *v > 0),
        jinja: Some(f.jinja),
        reasoning: f.reasoning.to_string(),
        reasoning_format: f.reasoning_format.to_string(),
        n_cpu_moe: ini::parse_int(f.n_cpu_moe.as_str()),
        temp: ini::parse_float(f.temp.as_str()),
        top_k: ini::parse_int(f.top_k.as_str()),
        top_p: ini::parse_float(f.top_p.as_str()),
        min_p: ini::parse_float(f.min_p.as_str()),
        repeat_penalty: ini::parse_float(f.repeat_penalty.as_str()),
        presence_penalty: ini::parse_float(f.presence_penalty.as_str()),
        chat_template_kwargs: f.chat_template_kwargs.to_string(),
    }
}

// ── Integrations helpers ──────────────────────────────────────────────

fn refresh_integrations(app: &AppWindow) {
    let cfg = server_cfg::load();
    let port = cfg.port.unwrap_or(8080);
    let hostname = cfg.hostname.unwrap_or_else(|| "localhost".into());
    let base_url = format!("http://{hostname}:{port}/v1");
    app.set_integration_base_url(SharedString::from(base_url));

    let claude_env = integrations::claude_code_env_script(&format!("http://{hostname}:{port}/v1"));
    app.set_integration_claude_env(SharedString::from(claude_env));

    let active = integrations::detect_opencode_provider();
    app.set_integration_provider_active(active);

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
    app.set_integration_models(ModelRc::from(rc));
}
