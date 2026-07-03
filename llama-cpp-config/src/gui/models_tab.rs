//! Models-tab callback wiring (the preset editor + New / Clone / Rename dialogs).
//!
//! Shared state, generated Slint types, and the `refresh_*` / `reload_presets` /
//! `apply_form` helpers live in the parent `gui` module; `use super::*` pulls
//! them in.

use super::*;

pub(super) fn wire(app: &AppWindow, state: &Rc<RefCell<State>>) {
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
            let s = app.global::<AppState>();
            let p = form_to_preset(&s.get_form());
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
                    // Rebuild the list from disk and re-select the just-saved preset
                    // (re-baselining the form so Save/Revert go disabled).
                    reload_presets(&app, &state, Some(&p.id));
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
            // Reload from disk keeping the current selection; reload_presets
            // re-applies (and re-baselines) the form, so no second apply here.
            reload_presets(&app, &state, None);
            refresh_file_options(&app);
            let label = app.global::<AppState>().get_form().id;
            set_status(&app, format!("Reloaded [{label}] from presets.ini"), false);
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
                    // Rebuild the list from disk, then clear the selection: after a
                    // delete we show an empty editor rather than auto-selecting a
                    // neighbour.
                    reload_presets(&app, &state, None);
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
            let s = app.global::<AppState>();
            s.set_new_dialog_source_id(SharedString::from(""));
            s.set_show_new_kind_picker(true);
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
            let s = app.global::<AppState>();
            // Clone source is always the selected preset (the button is disabled
            // otherwise). Stash it and surface its id in the dialog so it's clear
            // what is being copied.
            let selected = {
                let st = state.borrow();
                let idx = s.get_selected_preset_index();
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
            s.set_new_dialog_source_id(SharedString::from(p.id.clone()));
            *pending_clone_base.borrow_mut() = Some(p);
            s.set_show_new_kind_picker(true);
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
            app.global::<AppState>().set_presets(model(summaries));
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

// ── New / Clone dialog funnel ─────────────────────────────────────────
// The picker dialog → save path, used only by the wiring above. Moved here from
// `gui.rs` so the whole flow sits next to its callers. Reaches `gui`'s shared
// helpers (`model`, `reload_presets`, `refresh_*`, `set_status`) via `use super::*`.

fn populate_dialog_models(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    let models_dir = s.get_server_form().models_dir.to_string();
    let scanned = model_scan::list(&models_dir, model_scan::Category::Model.subdir());
    state.borrow_mut().dialog_models_all = scanned;
    s.set_dialog_filter(SharedString::from(""));
    let st = state.borrow();
    apply_dialog_models(app, &st.dialog_models_all, "");
}

/// Filter the cached dialog model scan by a case-insensitive substring on the
/// label and publish the result. Filtering both arrays from ONE matched list
/// keeps `dialog_model_index` consistent with `dialog_model_values`.
fn apply_dialog_models(app: &AppWindow, all: &[model_scan::FileOption], filter: &str) {
    let s = app.global::<AppState>();
    let q = filter.to_lowercase();
    let matched: Vec<&model_scan::FileOption> = all
        .iter()
        .filter(|f| q.is_empty() || f.label.to_lowercase().contains(&q))
        .collect();
    let labels: Vec<SharedString> = matched
        .iter()
        .map(|f| SharedString::from(f.label.clone()))
        .collect();
    let values: Vec<SharedString> = matched
        .iter()
        .map(|f| SharedString::from(f.path.clone()))
        .collect();
    s.set_dialog_model_labels(model(labels));
    s.set_dialog_model_values(model(values));
    s.set_dialog_model_index(-1);
}

fn picked_dialog_model_path(app: &AppWindow) -> Option<PathBuf> {
    let s = app.global::<AppState>();
    let idx = s.get_dialog_model_index();
    if idx < 0 {
        return None;
    }
    let values = s.get_dialog_model_values();
    let i = usize::try_from(idx).ok()?;
    if i >= values.row_count() {
        return None;
    }
    Some(PathBuf::from(values.row_data(i)?.to_string()))
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
