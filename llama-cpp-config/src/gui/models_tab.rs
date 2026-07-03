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
                    refresh_presets(&app, &state);
                    let st = state.borrow();
                    if let Some(i) = st.presets.iter().position(|x| x.id == p.id) {
                        s.set_selected_preset_index(i as i32);
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
