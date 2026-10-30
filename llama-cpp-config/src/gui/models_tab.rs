//! Models-tab callback wiring (the preset editor + New / Clone / Rename / Delete dialogs).
//!
//! Shared state, generated Slint types, and the `refresh_*` / `reload_presets` /
//! `apply_form` helpers live in the parent `gui` module; `use super::*` pulls
//! them in. The Models-tab-only helper families live HERE next to their callers
//! instead of in the shared hub:
//! - `update_model_info`: the GGUF "Model info" box;
//! - the New / Clone dialog funnel (`populate_dialog_models` … `commit_new_preset`;
//!   the Hugging Face source ends elsewhere: its preset is created in the
//!   background by `gui/downloads.rs`, which leaves the editor alone and only
//!   refreshes the list, the pickers and `presets_moved`);
//! - the discard-guarded navigation bodies (`do_select_preset`,
//!   `open_new_dialog`, `open_clone_dialog`, and the Rename seed behind
//!   `request_rename`): the actions `confirm_discard_then` parks;
//! - `preset_written`: the reload + reselect + re-baseline invariant every
//!   write path (save / rename / clone / New…) funnels through;
//! - the Delete dialog (`DeleteDialog`, `open_delete_dialog`,
//!   `delete_ticked_files`): the preset's linked model files, each with a
//!   tick that starts cleared and cannot be set on a file another preset
//!   uses; the confirm removes the section FIRST, then the ticked files
//!   (in-use re-checked against the presets as they stand then) and the
//!   folders they leave empty, through `linked_files.rs`;
//! - `apply_draft_pick`: the merged MTP/DFlash draft-pick policy (incl. the
//!   gemma4 auto-pin), unit-tested below.

use super::*;

// The Model-info box and the draft pick's speculator lookup are the only helpers
// that reach `gguf` directly; gui.rs no longer does, so pull it in here rather
// than through the parent's `use super::*`.
use crate::gguf;

// ── Callback wiring (entry point) ─────────────────────────────────────

pub(super) fn wire(app: &AppWindow, state: &Rc<RefCell<State>>) {
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_select_preset(move |index| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            // Switching presets replaces the form; unsaved edits need the
            // discard confirmation first.
            let dirty = app.global::<AppState>().get_preset_dirty();
            // The action may run later (after the discard dialog), and the
            // list can be re-read meanwhile (a background download creating
            // its preset, gui/downloads.rs): park the preset's ID, not its
            // index, and resolve it when the action runs.
            let Some(id) = usize::try_from(index)
                .ok()
                .and_then(|i| Some(state.borrow().presets.get(i)?.preset.id.clone()))
            else {
                return;
            };
            let action: Box<dyn Fn()> = {
                let app_weak = app_weak.clone();
                let state = state.clone();
                Box::new(move || {
                    let Some(app) = app_weak.upgrade() else {
                        return;
                    };
                    let index = state.borrow().presets.iter().position(|l| l.preset.id == id);
                    if let Some(index) = index {
                        do_select_preset(&app, &state, i32::try_from(index).unwrap_or(-1));
                    }
                })
            };
            confirm_discard_then(&app, &state, dirty, action);
        });
    }
    // The MoE cache hint's floors, recomputed by Slint whenever the form (or
    // `model_info_serial`) changes; see `moe_cache_floor_hint` in state.slint.
    app.global::<AppState>()
        .on_moe_cache_floor_hint(|form, _serial| {
            preset_moe_floor(&form_to_preset(&form))
                .map(|f| f.hint())
                .unwrap_or_default()
                .into()
        });
    // The drafter widgets' view of the `--spec-type` list (presets.rs), pure
    // over `form.spec_type` so Slint re-reads them on every change of it.
    {
        let s = app.global::<AppState>();
        s.on_spec_type_drafter(|t| match presets::spec_type_drafter(&t) {
            "" => "none".into(),
            d => d.into(),
        });
        s.on_spec_type_has(|t, entry| presets::spec_type_has(&t, &entry));
        s.on_spec_type_with_drafter(|t, d| form_spec_type(presets::spec_type_with_drafter(&t, &d)));
        s.on_spec_type_with_ngram(|t, entry, on| {
            form_spec_type(presets::spec_type_with_ngram(&t, &entry, on))
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
            let mut p = form_to_preset(&s.get_form());
            // Belt and braces: `update_model_info` already pruned the FORM, but
            // the save is the boundary that reaches DISK, and a preset that
            // llama-server then refuses to load is the failure being prevented.
            // Gated on `model_info_ready`: false means the GGUF could not be
            // read, where "no MTP metadata" is not evidence of no MTP.
            if s.get_model_info_ready() {
                presets::prune_inactive_draft_keys(&mut p, s.get_model_info_embeds_mtp());
                presets::prune_inactive_moe_cache(&mut p, s.get_model_info_has_moe());
                // The cache's one model-dependent refusal: below one token's
                // experts in every layer group the context throws and the model
                // never loads. The LOAD floor is a true lower bound
                // (`moe_cache::floor`), so this never refuses a value that could
                // have worked. 0 is "no cache" and passes.
                let floor = preset_moe_floor(&p)
                    .map_or(0, |f| crate::moe_cache::mib(f.load_bytes()));
                if let Some(v) = p.moe_cache_mib.filter(|&v| v > 0 && (v as u64) < floor) {
                    set_status(
                        &app,
                        format!(
                            "Not saved: MoE cache {v} MiB is below this preset's {floor} MiB floor, the experts of one token in every group of layers; llama.cpp would refuse to load it. Raise it, set 0, or tick default."
                        ),
                        true,
                    );
                    return;
                }
            }
            // The form shows only the selected n-gram type's tuning, so the other
            // types' keys would sit hidden in the INI (needs no GGUF).
            presets::prune_unused_ngram_keys(&mut p);
            if p.id.is_empty() {
                set_status(&app, "Preset id is empty.".into(), true);
                return;
            }
            if p.model.is_empty() {
                set_status(&app, "Pick a model file before saving.".into(), true);
                return;
            }
            // Asked of the FORM, not of `p`: the conversion has already read a
            // mistyped integer as an absent key, which is what is being refused.
            let bad = crate::form::invalid_numbers(&s.get_form());
            if !bad.is_empty() {
                set_status(
                    &app,
                    format!(
                        "Not saved: {}. Correct it, or tick its default box to leave the key unset.",
                        bad.join("; ")
                    ),
                    true,
                );
                return;
            }
            match presets::save(&p) {
                Ok(()) => {
                    set_status(&app, format!("Saved preset [{}]", p.id), false);
                    // Rebuild + re-select the just-saved preset (re-baselining the
                    // form so Save/Revert go disabled) and refresh the dependents.
                    preset_written(&app, &state, Some(&p.id));
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
            refresh_file_options(&app, &state);
            let label = app.global::<AppState>().get_form().id;
            set_status(&app, format!("Reloaded [{label}] from presets.ini"), false);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_request_delete(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            open_delete_dialog(&app, &state);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_delete_file_toggle(move |i| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            state.borrow_mut().delete_dialog.toggle(i);
            push_delete_rows(&app, &state.borrow().delete_dialog);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_delete_cancel(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            state.borrow_mut().delete_dialog = DeleteDialog::default();
            app.global::<AppState>().set_show_delete_dialog(false);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_delete_preset(move |id| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            app.global::<AppState>().set_show_delete_dialog(false);
            // Taken whatever happens next: ticks never outlive their dialog.
            let dialog = std::mem::take(&mut state.borrow_mut().delete_dialog);
            if id.is_empty() {
                return;
            }
            // The section first: a preset that cannot be removed keeps its
            // files too (they would be the files it still loads).
            if let Err(e) = presets::delete(id.as_str()) {
                set_status(&app, format!("Delete failed: {e}"), true);
                return;
            }
            let outcome = delete_ticked_files(&dialog, id.as_str());
            let (text, is_error) = crate::linked_files::report(id.as_str(), &outcome);
            set_status(&app, text, is_error);
            // Not preset_written(): after a delete we clear the selection
            // and show an empty editor, so the file/integration refreshes
            // must run AFTER the form is blanked, not against the neighbour
            // reload_presets would otherwise select. refresh_file_options is
            // also the rescan that drops deleted files from the pickers.
            reload_presets(&app, &state, None);
            app.global::<AppState>().set_selected_preset_index(-1);
            apply_form(&app, PresetForm::default());
            refresh_file_options(&app, &state);
            presets_moved(&app, &state);
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
            // Committing the new preset will replace the form (preset_written
            // reloads + reselects), so guard dirty edits at the entry point:
            // a discard dialog can't stack on the picker modal later.
            let dirty = app.global::<AppState>().get_preset_dirty();
            let action: Box<dyn Fn()> = {
                let app_weak = app_weak.clone();
                let state = state.clone();
                let pending_clone_base = pending_clone_base.clone();
                Box::new(move || {
                    let Some(app) = app_weak.upgrade() else {
                        return;
                    };
                    open_new_dialog(&app, &state, &pending_clone_base);
                })
            };
            confirm_discard_then(&app, &state, dirty, action);
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
            // Same entry-point guard as New… (the clone's commit also replaces
            // the form).
            let dirty = app.global::<AppState>().get_preset_dirty();
            let action: Box<dyn Fn()> = {
                let app_weak = app_weak.clone();
                let state = state.clone();
                let pending_clone_base = pending_clone_base.clone();
                Box::new(move || {
                    let Some(app) = app_weak.upgrade() else {
                        return;
                    };
                    open_clone_dialog(&app, &state, &pending_clone_base);
                })
            };
            confirm_discard_then(&app, &state, dirty, action);
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
            let Some(path) = picked_dialog_model_path(&app, &state) else {
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
            let Some(path) = picked_dialog_model_path(&app, &state) else {
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
        app.global::<AppState>().on_request_rename(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            // Same entry-point guard as New…/Clone…: committing the rename
            // reloads the form from disk (preset_written), discarding edits.
            let dirty = app.global::<AppState>().get_preset_dirty();
            let action: Box<dyn Fn()> = {
                let app_weak = app_weak.clone();
                Box::new(move || {
                    let Some(app) = app_weak.upgrade() else {
                        return;
                    };
                    let s = app.global::<AppState>();
                    let id = s.get_form().id;
                    s.set_rename_old_id(id.clone());
                    s.set_rename_new_id(id);
                    s.set_show_rename_dialog(true);
                })
            };
            confirm_discard_then(&app, &state, dirty, action);
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
                        preset_written(&app, &state, Some(new_id.as_str()));
                    }
                    Err(e) => set_status(&app, format!("Rename failed: {e}"), true),
                }
            });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>()
            .on_toggle_preset_enabled(move |id, enabled| {
                let Some(app) = app_weak.upgrade() else {
                    return;
                };
                // Not behind the discard guard, and not a form reload: the switch
                // moves the section between the two files without changing a
                // word of it, so an unsaved edit stays in the form and its Save
                // follows the preset into whichever file now holds it.
                match presets::set_enabled(id.as_str(), enabled) {
                    Ok(()) => {
                        let state_word = if enabled { "Enabled" } else { "Disabled" };
                        // The router reads presets.ini once, at launch.
                        let tail = match (app.global::<AppState>().get_server_running(), enabled) {
                            (true, _) => ": restart llama-server to apply.",
                            (false, true) => ".",
                            (false, false) => ": llama-server will not offer it.",
                        };
                        set_status(&app, format!("{state_word} [{id}]{tail}"), false);
                    }
                    Err(e) => set_status(&app, format!("Could not switch [{id}]: {e}"), true),
                }
                // Rebuilt on failure too: the click already flipped the switch
                // and broke its one-way binding, and only a fresh row puts it
                // back to what the files say.
                refresh_preset_list(&app, &state);
                presets_moved(&app, &state);
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
            apply_dialog_models(&app, &state, q.as_str());
        });
    }
    // Model-info box + draft-pick policy fire only from the Models page
    // (models_page.slint), so they're wired here rather than in run()'s seed.
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_model_changed(move || {
            if let Some(app) = app_weak.upgrade() {
                update_model_info(&app);
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>()
            .on_kv_cache_inputs_changed(move || {
                if let Some(app) = app_weak.upgrade() {
                    refresh_kv_cache_warning(&app);
                }
            });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_draft_picked(move |index| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let s = app.global::<AppState>();
            // Map the picked row to its (--model-draft, --spec-type) pair via
            // the Rust-side rows cache (parallel to AppState.draft_labels).
            let picked = usize::try_from(index)
                .ok()
                .and_then(|i| state.borrow().draft_rows.get(i).cloned());
            let Some((value, spec)) = picked else {
                return;
            };
            // The FILE's own header is the authority on which speculator it
            // needs; the row's spec only says which folder it was found in, and
            // that folder cannot tell a DSpark drafter from a DFlash one. The
            // guess survives as the fallback, for the header that stays silent
            // (a blockless MTP head) and the GGUF that won't open at all: see
            // `gguf::draft_spec_type`.
            let spec = if value.is_empty() {
                spec
            } else {
                gguf::draft_spec_type(std::path::Path::new(&value))
                    .map(str::to_string)
                    .unwrap_or(spec)
            };
            let mut form = s.get_form();
            apply_draft_pick(&mut form, &value, &spec);
            s.set_form(form);
            // The draft-device dropdown may have been changed programmatically.
            refresh_device_options(&app);
            update_model_info(&app);
        });
    }

    wire_gpu_table(app);
    wire_tensor_table(app);
}

/// The tensor-placement table's callbacks, and the tensor picker's, over `form.override_tensor`. Same
/// two-step shape as `wire_gpu_table`: derive the new value with `tensor_override`,
/// write it back, refresh.
///
/// The exceptions are `pattern` and `layers`, the two text fields. They refresh the
/// derived strings ONLY: a row rebuild would recreate the LineEdit mid-keystroke
/// and drop the caret. It can afford to, for the same reason the GPU table's
/// weight edit can: neither edit changes another row (see TensorOverrideTable's
/// binding note).
fn wire_tensor_table(app: &AppWindow) {
    let s = app.global::<AppState>();
    {
        let app_weak = app.as_weak();
        s.on_preset_tensor_add(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let rules = tensor_override::add(&preset_overrides(&app), &devices::probed());
            set_preset_overrides(&app, &rules);
            refresh_tensor_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_preset_tensor_remove(move |index| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let rules = tensor_override::remove(&preset_overrides(&app), row_index(index));
            set_preset_overrides(&app, &rules);
            refresh_tensor_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_preset_tensor_kind(move |index, kind| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let rules =
                tensor_override::set_kind(&preset_overrides(&app), row_index(index), kind.as_str());
            set_preset_overrides(&app, &rules);
            refresh_tensor_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_preset_tensor_device(move |index, device| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let rules = tensor_override::set_device(
                &preset_overrides(&app),
                row_index(index),
                device.as_str(),
            );
            set_preset_overrides(&app, &rules);
            refresh_tensor_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_preset_tensor_pattern(move |index, pattern| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let rules = tensor_override::set_pattern(
                &preset_overrides(&app),
                row_index(index),
                pattern.as_str(),
            );
            set_preset_overrides(&app, &rules);
            refresh_tensor_scalars(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_preset_tensor_layers(move |index, layers| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let rules = tensor_override::set_layers(
                &preset_overrides(&app),
                row_index(index),
                layers.as_str(),
            );
            set_preset_overrides(&app, &rules);
            refresh_tensor_scalars(&app);
        });
    }
    wire_tensor_picker(app);
}

/// The tensor picker (`tensor_picker.rs`): opened on one rule, it lists the
/// model's tensors with a tick each; Apply writes the ticked set back into that
/// rule as a regex and rebuilds the table. The picker itself lives in
/// `TENSOR_PICKER`; every callback edits it and re-pushes the list.
fn wire_tensor_picker(app: &AppWindow) {
    let s = app.global::<AppState>();
    {
        let app_weak = app.as_weak();
        s.on_preset_tensor_pick(move |index| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let tensors = model_tensors();
            if tensors.is_empty() {
                return;
            }
            let picker =
                tensor_picker::Picker::open(&preset_overrides(&app), row_index(index), tensors);
            TENSOR_PICKER.with(|p| *p.borrow_mut() = Some(picker));
            refresh_tensor_picker(&app);
        });
    }
    // One shape for the four list edits: change the picker, re-push the list.
    fn on_edit(app: &AppWindow, edit: impl Fn(&mut tensor_picker::Picker)) {
        TENSOR_PICKER.with(|p| {
            if let Some(p) = p.borrow_mut().as_mut() {
                edit(p);
            }
        });
        refresh_tensor_picker(app);
    }
    {
        let app_weak = app.as_weak();
        s.on_tensor_picker_toggle(move |line| {
            if let Some(app) = app_weak.upgrade() {
                on_edit(&app, |p| p.toggle(row_index(line)));
            }
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_tensor_picker_expand(move |line| {
            if let Some(app) = app_weak.upgrade() {
                on_edit(&app, |p| p.expand(row_index(line)));
            }
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_tensor_picker_all(move |on| {
            if let Some(app) = app_weak.upgrade() {
                on_edit(&app, |p| p.set_all(on));
            }
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_tensor_picker_expand_all(move |on| {
            if let Some(app) = app_weak.upgrade() {
                on_edit(&app, |p| p.expand_all(on));
            }
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_tensor_picker_apply(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let Some(picker) = TENSOR_PICKER.with(|p| p.borrow_mut().take()) else {
                return;
            };
            let pattern = picker.pattern();
            if !pattern.is_empty() {
                let rules =
                    tensor_override::set_pattern(&preset_overrides(&app), picker.rule, &pattern);
                set_preset_overrides(&app, &rules);
            }
            refresh_tensor_picker(&app);
            refresh_tensor_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_tensor_picker_close(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            TENSOR_PICKER.with(|p| *p.borrow_mut() = None);
            refresh_tensor_picker(&app);
        });
    }
}

/// The per-preset GPU distribution table's four callbacks: the twin of
/// `server_tab::wire_gpu_table`, over `form.device` + `form.tensor_split`. The
/// rebuild-vs-scalars-only distinction is documented there.
fn wire_gpu_table(app: &AppWindow) {
    let s = app.global::<AppState>();
    {
        let app_weak = app.as_weak();
        s.on_preset_gpu_toggle(move |id| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let sel = gpu_split::toggle(&preset_selection(&app), id.as_str());
            set_preset_selection(&app, &sel);
            refresh_gpu_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_preset_gpu_move(move |id, delta| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let sel = gpu_split::move_by(&preset_selection(&app), id.as_str(), delta);
            set_preset_selection(&app, &sel);
            refresh_gpu_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_preset_gpu_weight(move |id, weight| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let sel = gpu_split::set_weight(&preset_selection(&app), id.as_str(), weight);
            set_preset_selection(&app, &sel);
            refresh_gpu_scalars(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_preset_gpu_blocks(move |id, blocks| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let Some(layout) = preset_split_layout(&app) else {
                return;
            };
            let current = preset_selection(&app);
            let sel = gpu_split::set_blocks(&current, id.as_str(), blocks, layout);
            // Committing the same count again (which leaving the field after an
            // Enter does) must not rebuild: the rebuild recreates the delegate,
            // and re-entering here from its teardown is how that turns into a loop.
            if sel == current {
                return;
            }
            set_preset_selection(&app, &sel);
            // Unlike a weight edit this DOES rebuild: the count that was typed is
            // paid for by the other rows, so their cells and ranges are stale until
            // the model is rebuilt. That is affordable only because the callback is
            // commit-based (see `blocks_edited` in components.slint).
            refresh_gpu_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_preset_gpu_auto(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let sel = gpu_split::set_auto(&preset_selection(&app));
            set_preset_selection(&app, &sel);
            refresh_gpu_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_preset_gpu_even(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let sel = gpu_split::set_even(&preset_selection(&app));
            set_preset_selection(&app, &sel);
            refresh_gpu_rows(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_preset_split_mode_picked(move |v| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let st = app.global::<AppState>();
            let mut f = st.get_form();
            f.split_mode = v;
            st.set_form(f);
            // The mode decides what the columns mean (blocks vs ratio vs
            // nothing editable), so a pick is a full re-projection.
            refresh_gpu_rows(&app);
        });
    }
}

// ── Discard-guarded navigation (select / New / Clone) ─────────────────

/// The actual preset switch (the guarded body of `on_select_preset`): load the
/// indexed preset into the editor and refresh its dependent dropdowns.
fn do_select_preset(app: &AppWindow, state: &Rc<RefCell<State>>, index: i32) {
    let st = state.borrow();
    if let Some(l) = usize::try_from(index).ok().and_then(|i| st.presets.get(i)) {
        app.global::<AppState>().set_selected_preset_index(index);
        apply_form(app, preset_to_form(&l.preset));
        drop(st);
        refresh_file_options(app, state);
    }
}

/// Open the model picker in plain "New" mode (the guarded body of
/// `on_new_preset`). "New…" is always create-from-scratch, independent of any
/// current selection, so it can never silently turn into a clone.
fn open_new_dialog(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    pending_clone_base: &Rc<RefCell<Option<presets::Preset>>>,
) {
    *pending_clone_base.borrow_mut() = None;
    populate_dialog_models(app, state);
    let s = app.global::<AppState>();
    s.set_new_dialog_source_id(SharedString::from(""));
    s.set_show_new_kind_picker(true);
}

/// Open the model picker in Clone mode (the guarded body of `on_clone_preset`).
/// The clone source is always the selected preset (the button is disabled
/// otherwise): stash it and surface its id in the dialog so it's clear what is
/// being copied.
fn open_clone_dialog(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    pending_clone_base: &Rc<RefCell<Option<presets::Preset>>>,
) {
    let s = app.global::<AppState>();
    let selected = {
        let st = state.borrow();
        let idx = s.get_selected_preset_index();
        usize::try_from(idx)
            .ok()
            .and_then(|i| st.presets.get(i))
            .map(|l| l.preset.clone())
    };
    let Some(p) = selected else {
        set_status(app, "Select a preset to clone first.".into(), true);
        return;
    };
    populate_dialog_models(app, state);
    s.set_new_dialog_source_id(SharedString::from(p.id.clone()));
    *pending_clone_base.borrow_mut() = Some(p);
    s.set_show_new_kind_picker(true);
}

// The "all layers on GPU" sentinel lives in form.rs next to the slider
// fallbacks that share it (and its Slint mirror is test-asserted there).
use crate::form::ALL_LAYERS;

/// A `--spec-type` list in the form's spelling: empty is "none" (`preset_to_form`).
fn form_spec_type(spec_type: String) -> SharedString {
    if spec_type.is_empty() { "none".into() } else { spec_type.into() }
}

// ── Draft-pick policy ─────────────────────────────────────────────────

/// Apply a draft-picker selection to the form: set `model_draft` and the
/// drafter entry of `spec_type` from the picked row (`spec` already resolved by
/// the caller from the drafter's own GGUF header, with its folder as the
/// fallback; "(none)" removes it). The `ngram-*` entries stay listed: they draft
/// from the context, whatever the draft file. Then
/// auto-pin an unconfigured draft to ONE device: the multi-device "auto" split
/// crashes gemma4 MTP heads. Pin to the SAME GPU the
/// model runs on so both land together: the preset's own `device` selection
/// (all layers on it); otherwise fall back to CPU, which always works. A draft the user already
/// configured (auto off or a device pinned) is left alone.
///
/// `device` can name SEVERAL GPUs ("ROCm1,CUDA0"; the distribution table writes
/// them in split order), but `--device-draft` pins one drafter, and the whole
/// point here is to avoid the multi-device split. Which one is
/// `draft_pin_device`'s call.
///
/// This only fires for a draft FILE. Embedded MTP heads (spec-type alone) never
/// reach it: llama.cpp ignores both keys for them and the UI disables both fields.
fn apply_draft_pick(form: &mut PresetForm, value: &str, spec: &str) {
    form.model_draft = value.into();
    form.spec_type = form_spec_type(presets::spec_type_with_drafter(&form.spec_type, spec));
    if !value.is_empty() && form.n_gpu_layers_draft_auto && form.device_draft.is_empty() {
        form.n_gpu_layers_draft_auto = false;
        // Pinning the draft to a different device than the model is the exact
        // split this heuristic exists to avoid.
        match draft_pin_device(form, spec) {
            Some(pin) => {
                form.device_draft = pin.into();
                form.n_gpu_layers_draft = ALL_LAYERS;
            }
            None => form.n_gpu_layers_draft = 0,
        }
    }
}

/// The one device `apply_draft_pick` pins a draft file to, `None` when the
/// preset names none. Normally the FIRST of `device`: the one llama.cpp's split
/// starts from, and the only one that runs under split-mode none.
///
/// A DFlash / DSpark drafter is the exception under a layer split: with the
/// full vocabulary it has no lm head of its own and runs the TARGET's
/// `output.weight` (`src/models/dflash.cpp`), which the layer split leaves on
/// the device holding the last layers, i.e. the LAST of the list with a share
/// (a blank `tensor-split` gives every device one). Pinned anywhere else, the
/// draft context aborts at load: "pre-allocated tensor (output.weight) in a
/// buffer (ROCm1) that cannot run the operation". Measured on 2x R9700 with
/// Qwen3.8-27B + DFlash2; a reduced-vocab drafter ships its own head and runs
/// on either card, so the last one is safe for both.
fn draft_pin_device(form: &PresetForm, spec: &str) -> Option<String> {
    let devices = gpu_split::parse_device_list(form.device.as_str());
    let shares_target_head = matches!(spec, "draft-dflash" | "draft-dspark");
    if shares_target_head
        && gpu_split::SplitMode::parse(form.split_mode.as_str()) == gpu_split::SplitMode::Layer
    {
        let weights = gpu_split::parse_weights(form.tensor_split.as_str());
        return devices
            .iter()
            .enumerate()
            .rev()
            .find(|(i, _)| weights.is_empty() || weights.get(*i).is_some_and(|w| *w > 0))
            .map(|(_, d)| d.clone());
    }
    devices.into_iter().next()
}

// ── Write invariant: reload + reselect + re-baseline ──────────────────

/// Refresh everything that depends on the preset set after a presets.ini write:
/// the (re-selected) preset list (`reload_presets`), the file/device dropdowns,
/// and the tabs that read the enabled set (`presets_moved`). `want` picks the
/// selection like `reload_presets`. The invariant every write path follows:
/// save / rename / clone all funnel through here. (select/revert do NOT: they
/// don't mutate disk; delete keeps its own sequence because it clears the
/// selection, and the on/off switch its own because it must keep the form.)
fn preset_written(app: &AppWindow, state: &Rc<RefCell<State>>, want: Option<&str>) {
    reload_presets(app, state, want);
    refresh_file_options(app, state);
    presets_moved(app, state);
}

/// The tail every preset write shares: the consumers of the ENABLED set, which
/// a save can change (a `ctx-size` moves an opencode entry's `limit.context`)
/// as surely as a switch or a delete can. opencode.json follows when it carries
/// the provider (`follow_integrations`), and the Benchmark tab re-lists, since a
/// preset that is off, renamed or gone can no longer be run (its refresh also
/// drops it from the selection).
pub(super) fn presets_moved(app: &AppWindow, state: &Rc<RefCell<State>>) {
    follow_integrations(app);
    bench_tab::refresh(app, state);
}

// ── Delete dialog: the preset, and the files it links ─────────────────

/// The Delete dialog's Rust half (`State.delete_dialog`): the preset it is
/// about, the files it links (`linked_files::linked_files`, in-use check
/// included) and one tick per file, all cleared at open. The Slint rows are
/// rebuilt from this on every change (`push_delete_rows`).
#[derive(Default)]
pub(super) struct DeleteDialog {
    id: String,
    files: Vec<crate::linked_files::LinkedFile>,
    ticked: Vec<bool>,
}

impl DeleteDialog {
    /// Flip one tick; a file in use (or an index out of range) stays as it is.
    fn toggle(&mut self, i: i32) {
        let Ok(i) = usize::try_from(i) else {
            return;
        };
        if let (Some(f), Some(t)) = (self.files.get(i), self.ticked.get_mut(i)) {
            if !f.in_use() {
                *t = !*t;
            }
        }
    }

    fn ticked_count(&self) -> usize {
        self.files
            .iter()
            .zip(&self.ticked)
            .filter(|(f, t)| **t && !f.in_use())
            .count()
    }
}

/// Open the Delete dialog on the selected preset: look it up in both INI files,
/// list the files it links that are on disk, every tick cleared.
fn open_delete_dialog(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    let id = s.get_form().id.to_string();
    if id.is_empty() {
        return;
    }
    let listed = presets::load_listed();
    let Some(target) = listed.iter().find(|l| l.preset.id.eq_ignore_ascii_case(&id)) else {
        set_status(app, format!("No preset [{id}] on disk to delete."), true);
        return;
    };
    let all: Vec<presets::Preset> = listed.iter().map(|l| l.preset.clone()).collect();
    let files = crate::linked_files::linked_files(&target.preset, &all);
    let dialog = DeleteDialog {
        id: target.preset.id.clone(),
        ticked: vec![false; files.len()],
        files,
    };
    s.set_delete_target_id(dialog.id.as_str().into());
    s.set_delete_target_file(if target.enabled { "presets.ini" } else { "presets.ini.disabled" }.into());
    push_delete_rows(app, &dialog);
    state.borrow_mut().delete_dialog = dialog;
    s.set_show_delete_dialog(true);
}

/// Project the dialog's Rust half onto its rows, its count and its summary.
fn push_delete_rows(app: &AppWindow, dialog: &DeleteDialog) {
    let s = app.global::<AppState>();
    let rows: Vec<DeleteFileRow> = dialog
        .files
        .iter()
        .zip(&dialog.ticked)
        .map(|(f, &t)| DeleteFileRow {
            label: f.row_label().into(),
            detail: f.row_detail().into(),
            checked: t && !f.in_use(),
            in_use: f.in_use(),
        })
        .collect();
    s.set_delete_file_rows(model(rows));
    s.set_delete_files_ticked(dialog.ticked_count() as i32);
    s.set_delete_files_summary(
        crate::linked_files::selection_summary(&dialog.files, &dialog.ticked).into(),
    );
}

/// After the section is gone: delete the files ticked in `dialog`, when it was
/// opened on `id`. The in-use check is run AGAIN against the presets as they
/// stand now, so a file that became shared since the dialog opened (by a hand
/// edit or the CLI) is kept, not deleted. The empty-folder cleanup is bounded
/// by the SAVED ModelsDir, the one the pickers scan.
fn delete_ticked_files(dialog: &DeleteDialog, id: &str) -> crate::linked_files::Outcome {
    if !dialog.id.eq_ignore_ascii_case(id) {
        return Default::default();
    }
    let all: Vec<presets::Preset> = presets::load_listed().into_iter().map(|l| l.preset).collect();
    let chosen: Vec<crate::linked_files::LinkedFile> = dialog
        .files
        .iter()
        .zip(&dialog.ticked)
        .filter(|(_, t)| **t)
        .map(|(f, _)| crate::linked_files::LinkedFile {
            used_by: crate::linked_files::users(&f.files, id, &all),
            ..f.clone()
        })
        .collect();
    if chosen.is_empty() {
        return Default::default();
    }
    let models_dir = server_cfg::load().models_dir_or_default();
    crate::linked_files::delete(&chosen, &models_dir, false)
}

// ── Model-info box (GGUF reads) ───────────────────────────────────────

/// Fill the read-only "Model info" box from the selected model's GGUF header
/// (read via `ggml-base.dll`), enriched with the headers of the mmproj and the
/// draft file this preset SELECTS, and nothing that it doesn't (see
/// `gguf::draft_line`). Called whenever the model/mmproj/draft field changes
/// (combo pick, preset load).
/// `pub(super)` so the shared `refresh_file_options` in gui.rs can drive it.
pub(super) fn update_model_info(app: &AppWindow) {
    let s = app.global::<AppState>();
    let form = s.get_form();
    let model = form.model.to_string();

    // Reset the optional rows; the success path re-enables the ones that apply.
    s.set_model_info_has_moe(false);
    set_model_experts(0, 0, 0);
    // Every path below may change the Rust-side model facts the MoE cache hint
    // reads (`moe_cache_floor_hint`), so its binding is told to re-run.
    s.set_model_info_serial(s.get_model_info_serial().wrapping_add(1));
    s.set_model_info_has_mmproj(false);
    s.set_model_info_has_draft_file(false);
    s.set_model_info_embeds_mtp(false);
    // Reset the slider maxima; 0 = unknown → the UI falls back to a 0..99 range.
    s.set_model_info_n_layer(0);
    s.set_model_info_draft_n_layer(0);
    // Chat-template row: reset alongside the sibling `has_*` / `n_layer` resets
    // for coherence. Unlike those (which gate always-visible controls), this row
    // lives behind `model_info_ready`, so the value is never shown until the
    // success path below overwrites it; the gate, not this reset, is what keeps
    // a stale template from lingering.
    s.set_model_info_chat_template(SharedString::from("none"));
    s.set_chat_template_preview(SharedString::from(""));
    // The two reasoning rows are read from that same template, so they reset with
    // it, and to "unknown", never to "no": an unread GGUF is not evidence that a
    // model cannot think (the same rule the embd warning below follows).
    s.set_model_info_thinking(SharedString::from("unknown"));
    s.set_model_info_past_thinking(SharedString::from("unknown"));
    // Unlike the rows above, the embd WARNING is read by the Tensor-placement
    // table, which is NOT behind `model_info_ready`, so a stale one would keep
    // accusing the next model (or an unreadable one) of the previous model's
    // unpinnable embedding type. It must be cleared on every entry, not just
    // repainted on success.
    s.set_model_info_embd(SharedString::from("n/a"));
    s.set_model_info_embd_warning(SharedString::from(""));
    // Same rule for the tensor-mode arch verdict: the GPU table reads it under
    // split mode `tensor` whether or not `model_info_ready`, so it must not
    // outlive the model it was read from.
    s.set_model_info_tensor_arch_warning(SharedString::from(""));

    // Both failure paths reset `model_info_n_layer` above, and the GPU table
    // projects its weights onto exactly that, so they have to go through
    // `refresh_gpu_rows` (which subsumes the tensor refresh) rather than the
    // tensor-only one, or the block column would keep counting against the
    // PREVIOUS model. This is not a keystroke path, so the full rebuild is free.
    if model.trim().is_empty() {
        s.set_model_info_ready(false);
        s.set_model_info_note(SharedString::from(
            "Select a model to see its GGUF details.",
        ));
        set_model_tensors(Vec::new());
        refresh_gpu_rows(app);
        return;
    }

    let Some(info) = gguf::read_model_info(std::path::Path::new(&model)) else {
        s.set_model_info_ready(false);
        s.set_model_info_note(SharedString::from(
            "Metadata unavailable: is llama.cpp's ggml-base library beside the app, and the file a valid GGUF?",
        ));
        set_model_tensors(Vec::new());
        refresh_gpu_rows(app);
        return;
    };

    // The Tensor placement table's coverage lines and picker. Every shard is
    // read; a split model with a shard missing gets no list rather than half.
    set_model_tensors(gguf::read_tensors(std::path::Path::new(&model)).unwrap_or_default());
    // With the every-shard list above, what the MoE cache floor reads: the first
    // shard alone would judge a split model on a subset of its layers.
    set_model_experts(info.expert_count, info.expert_used, info.n_layer);
    s.set_model_info_kind(SharedString::from(info.kind_line()));
    s.set_model_info_n_layer(info.n_layer as i32);
    s.set_model_info_has_moe(info.is_moe);
    s.set_model_info_moe(SharedString::from(info.moe_offload_line()));
    s.set_model_info_arch_quant(SharedString::from(info.arch_quant_line()));
    // Feeds the GPU table's mode strip under split mode `tensor` (state.slint
    // `preset_split_mode_warning`): an arch llama.cpp's tensor parallelism
    // refuses at load, read once here rather than on every mode pick.
    s.set_model_info_tensor_arch_warning(SharedString::from(info.sm_tensor_warning()));
    s.set_model_info_embd(SharedString::from(info.embd_line()));
    // Feeds the Tensor-placement table's warning strip (see refresh_tensor_scalars).
    // Set BEFORE the refresh below, which reads it back off AppState rather than
    // re-opening the GGUF on every pattern keystroke.
    s.set_model_info_embd_warning(SharedString::from(info.embd_pin_warning()));
    s.set_model_info_layers_ctx(SharedString::from(info.layers_ctx_line()));
    s.set_model_info_attn(SharedString::from(info.attn_line()));
    s.set_model_info_memory(SharedString::from(info.memory_line()));
    // The row reports only what this preset LOADS: the model's own MTP heads
    // plus the picked draft file, hence the preset's `--model-draft` here. It
    // used to also advertise the best filename match in `mtps\`, which is what
    // made it read the same whether a draft was selected or not (see
    // `draft_line`).
    s.set_model_info_draft(SharedString::from(gguf::draft_line(
        &info,
        form.model_draft.as_str(),
    )));
    // Chat template: short status for the InfoRow, raw text for the Preview
    // modal (empty when none; also gates the Preview button visibility).
    s.set_model_info_chat_template(SharedString::from(info.chat_template_line()));
    s.set_chat_template_preview(SharedString::from(
        info.chat_template.clone().unwrap_or_default(),
    ));
    s.set_model_info_thinking(SharedString::from(info.thinking_line()));
    s.set_model_info_past_thinking(SharedString::from(info.past_thinking_line()));
    // Enables the speculative-decoding controls even before an external draft is
    // picked, when the model itself embeds MTP/nextn heads.
    let embeds_mtp = info.nextn_predict_layers > 0;
    s.set_model_info_embeds_mtp(embeds_mtp);
    // …and, now that the model is known, drop any speculative keys it cannot act
    // on. The widgets that hold them are `enabled: draft_active`, so a value the
    // user never typed (a Clone's, or the model this preset used to point at)
    // would otherwise sit there greyed out and unreadable until llama-server
    // failed on it. Pruning the form makes the change visible AND leaves the
    // preset dirty, so Save is reachable: the file on disk is where the damage
    // lives, and a preset the user only OPENS is never re-saved otherwise.
    let mut pruned_form = form.clone();
    let dropped = prune_inactive_draft_fields(&mut pruned_form, embeds_mtp);
    // Same reasoning for the MoE cache, whose widget is disabled on a dense
    // model: a positive value there fails the load (`prune_inactive_moe_cache`).
    let moe_cache_dropped = !info.is_moe && !pruned_form.moe_cache_mib_default;
    if moe_cache_dropped {
        pruned_form.moe_cache_mib_default = true;
    }
    if !dropped.is_empty() || moe_cache_dropped {
        s.set_form(pruned_form);
        // Different reasons, and naming the wrong one would teach the user the
        // wrong lesson: with embedded MTP heads only the PLACEMENT keys die.
        let mut parts = Vec::new();
        if !dropped.is_empty() {
            let why = if embeds_mtp {
                "they place a separate draft FILE, and this preset has none"
            } else {
                "this model has no MTP heads and no draft file"
            };
            parts.push(format!("Dropped {}: {why}.", dropped.join(", ")));
        }
        if moe_cache_dropped {
            parts.push("Dropped moe-cache-mib: this model has no experts.".to_string());
        }
        set_status(app, format!("{} Save to persist.", parts.join(" ")), false);
    }
    // Two tables depend on BOTH their own rules and the model, and only the model
    // changed here: the Tensor-placement warning (its embd verdict) and the GPU
    // split's block column, which counts against this model's `block_count`.
    // `refresh_gpu_rows` re-derives both; it has to rebuild the ROWS, not just
    // the scalars, because the per-row counts and ranges live in the row model.
    refresh_gpu_rows(app);

    // Optional: the selected mmproj's clip header.
    let mmproj = form.mmproj.to_string();
    if !mmproj.trim().is_empty() {
        if let Some(mp) = gguf::read_mmproj_info(std::path::Path::new(&mmproj)) {
            s.set_model_info_mmproj(SharedString::from(mp.mmproj_line()));
            s.set_model_info_has_mmproj(true);
        }
    }

    // Optional: the selected draft/MTP/DFlash file's own header.
    let draft = form.model_draft.to_string();
    if !draft.trim().is_empty() {
        if let Some(d) = gguf::read_model_info(std::path::Path::new(&draft)) {
            s.set_model_info_draft_file(SharedString::from(d.draft_file_line()));
            s.set_model_info_draft_n_layer(d.n_layer as i32);
            s.set_model_info_has_draft_file(true);
        }
    }

    s.set_model_info_ready(true);
}

// ── New / Clone dialog funnel ─────────────────────────────────────────
// The picker dialog → save path, used only by the wiring above. Moved here from
// `gui.rs` so the whole flow sits next to its callers. Reaches `gui`'s shared
// helpers (`model`, `reload_presets`, `refresh_*`, `set_status`) via `use super::*`.

fn populate_dialog_models(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    // SAVED config, like refresh_file_options; see the note there.
    let models_dir = server_cfg::load().models_dir_or_default();
    let scanned = model_scan::list(&models_dir, model_scan::Category::Model.subdir());
    state.borrow_mut().dialog_models_all = scanned;
    s.set_dialog_filter(SharedString::from(""));
    apply_dialog_models(app, state, "");
}

/// Filter the cached dialog model scan by a case-insensitive substring on the
/// label and publish the result. Filtering label and path from ONE matched
/// list keeps `dialog_model_index` consistent with the paths; the paths are
/// Rust-only data (only `picked_dialog_model_path` reads them), so they stay
/// in `State.dialog_models_filtered` rather than as a published Slint array.
fn apply_dialog_models(app: &AppWindow, state: &Rc<RefCell<State>>, filter: &str) {
    let q = filter.to_lowercase();
    let (labels, paths): (Vec<SharedString>, Vec<String>) = state
        .borrow()
        .dialog_models_all
        .iter()
        .filter(|f| q.is_empty() || f.label.to_lowercase().contains(&q))
        .map(|f| (SharedString::from(f.label.clone()), f.path.clone()))
        .unzip();
    state.borrow_mut().dialog_models_filtered = paths;
    let s = app.global::<AppState>();
    s.set_dialog_model_labels(model(labels));
    s.set_dialog_model_index(-1);
}

fn picked_dialog_model_path(app: &AppWindow, state: &Rc<RefCell<State>>) -> Option<PathBuf> {
    let idx = app.global::<AppState>().get_dialog_model_index();
    let i = usize::try_from(idx).ok()?;
    state
        .borrow()
        .dialog_models_filtered
        .get(i)
        .map(PathBuf::from)
}

/// Preset id for a newly picked model file, de-conflicted against every preset,
/// the switched-off ones included (a namesake parked in the disabled file would
/// collide the day it is switched back on). The id derives from the file name,
/// so picking a model that already has a preset (or a different file that
/// sanitizes to the same id) would otherwise make `presets::save`
/// wholesale-replace the tuned section. Both New and Clone must route through
/// this: first free `<id>`, `<id>-2`, …, compared ignoring case, with the
/// `preset` fallback for a name that sanitizes to nothing (an empty id would
/// also be undeletable: on_delete_preset early-returns on one). The rule
/// itself is `presets::new_preset_id`, shared with the Hugging Face download.
fn deconflicted_id(path_str: &str) -> String {
    let existing: Vec<String> = presets::load_listed()
        .into_iter()
        .map(|l| l.preset.id)
        .collect();
    presets::new_preset_id(path_str, &existing)
}

fn run_new_empty(app: &AppWindow, state: &Rc<RefCell<State>>, path: PathBuf) {
    let path_str = path.to_string_lossy().into_owned();
    let id = deconflicted_id(&path_str);
    let p = presets::Preset::new_default(id.clone(), path_str);
    commit_new_preset(
        app,
        state,
        p,
        format!("Added [{id}]: tweak parameters and Save."),
    );
}

fn run_new_clone(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    base: presets::Preset,
    path: PathBuf,
) {
    let path_str = path.to_string_lossy().into_owned();
    let id = deconflicted_id(&path_str);
    let mut cloned = presets::Preset {
        id: id.clone(),
        model: path_str,
        ..base.clone()
    };
    // A clone keeps every field of its base, including the speculative ones,
    // which belong to the base's MODEL, not to the preset. Cloning an MTP preset
    // onto a model without MTP heads writes `spec-type = draft-mtp` straight to
    // disk, where the UI greys it out instead of showing it and llama-server
    // refuses to load the model at all. This path never passes through the form,
    // so it prunes against the target's own header. Unreadable GGUF = keep
    // everything: see `prune_inactive_draft_keys` on why absence isn't evidence.
    // The MoE cache rides the same rule: dead (and fatal) on a dense model, and
    // fatal below the target's own one-token floor, which a base on a different
    // model says nothing about.
    let dropped = match gguf::read_model_info(&path) {
        Some(info) => {
            let mut dropped =
                presets::prune_inactive_draft_keys(&mut cloned, info.nextn_predict_layers > 0);
            let below_floor = cloned.moe_cache_mib.is_some_and(|v| {
                let floor = gguf::read_tensors(&path)
                    .and_then(|t| {
                        crate::moe_cache::floor(
                            &crate::moe_cache::Model {
                                tensors: &t,
                                expert_count: info.expert_count,
                                expert_used: info.expert_used,
                                n_layer: info.n_layer,
                            },
                            &cloned,
                        )
                    })
                    .map_or(0, |f| crate::moe_cache::mib(f.load_bytes()));
                v > 0 && (v as u64) < floor
            });
            if presets::prune_inactive_moe_cache(&mut cloned, info.is_moe) || below_floor {
                cloned.moe_cache_mib = None;
                dropped.push("moe-cache-mib");
            }
            dropped
        }
        None => Vec::new(),
    };
    let note = if dropped.is_empty() {
        String::new()
    } else {
        format!(
            " Dropped {} (not applicable to this model).",
            dropped.join(", ")
        )
    };
    commit_new_preset(
        app,
        state,
        cloned,
        format!("Cloned [{}] -> [{id}], saved.{note}", base.id),
    );
}

fn commit_new_preset(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    p: presets::Preset,
    success_status: String,
) {
    let saved = presets::save(&p);
    finish_new_preset(app, state, &p, saved, success_status);
}

/// The shared end of every new preset: the status, then `preset_written`
/// (reload + select the new id + rescan the pickers + the integrations).
fn finish_new_preset(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    p: &presets::Preset,
    saved: std::io::Result<()>,
    success_status: String,
) {
    match saved {
        Ok(()) => {
            // Status first, like the other write paths, so a failed opencode.json
            // follow-up inside preset_written is not papered over by the success.
            set_status(app, success_status, false);
            preset_written(app, state, Some(&p.id));
        }
        Err(e) => set_status(app, format!("Save failed: {e}"), true),
    }
}

// Pure-struct tests (no Slint backend needed: PresetForm is a plain generated
// struct). The gemma4-MTP auto-pin policy used to live untestable inside the
// .slint pick handler; these pin its three branches.
#[cfg(test)]
mod tests {
    use super::*;

    fn form_with_auto_draft() -> PresetForm {
        PresetForm {
            n_gpu_layers_draft_auto: true,
            ..Default::default()
        }
    }

    #[test]
    fn pick_pins_the_draft_to_the_presets_first_device() {
        // The model runs on CUDA1 then CUDA0; the draft must land on the head
        // of that list: model and draft must share ONE device.
        let mut f = PresetForm {
            n_gpu_layers_draft_auto: true,
            device: "CUDA1,CUDA0".into(),
            ..Default::default()
        };
        apply_draft_pick(&mut f, r"C:\m\mtps\head.gguf", "draft-mtp");
        assert_eq!(f.device_draft, "CUDA1");
        assert_eq!(f.n_gpu_layers_draft, 99);
    }

    // A full-vocab DFlash drafter runs the target's output head, which a
    // layer split leaves on the last device with a share: pinned to the first,
    // the draft context aborts at load (2x R9700, Qwen3.8-27B + DFlash2).
    #[test]
    fn pick_pins_a_dflash_draft_to_the_device_holding_the_output_head() {
        let mut f = PresetForm {
            n_gpu_layers_draft_auto: true,
            device: "ROCm0,ROCm1".into(),
            tensor_split: "1,1".into(),
            ..Default::default()
        };
        apply_draft_pick(&mut f, "/m/dflashs/d.gguf", "draft-dflash");
        assert_eq!(f.device_draft, "ROCm1");

        // A device with no share holds no layers, so not the head either.
        let mut f = PresetForm {
            n_gpu_layers_draft_auto: true,
            device: "ROCm0,ROCm1".into(),
            tensor_split: "1,0".into(),
            ..Default::default()
        };
        apply_draft_pick(&mut f, "/m/dflashs/d.gguf", "draft-dspark");
        assert_eq!(f.device_draft, "ROCm0");

        // Split-mode none runs only the first device: everything is there.
        let mut f = PresetForm {
            n_gpu_layers_draft_auto: true,
            device: "ROCm0,ROCm1".into(),
            split_mode: "none".into(),
            ..Default::default()
        };
        apply_draft_pick(&mut f, "/m/dflashs/d.gguf", "draft-dflash");
        assert_eq!(f.device_draft, "ROCm0");
    }

    #[test]
    fn pick_falls_back_to_cpu_without_a_device() {
        let mut f = form_with_auto_draft();
        apply_draft_pick(&mut f, r"C:\m\dflashs\d.gguf", "draft-dflash");
        assert_eq!(f.spec_type, "draft-dflash");
        assert!(!f.n_gpu_layers_draft_auto);
        assert_eq!(f.device_draft, "");
        assert_eq!(f.n_gpu_layers_draft, 0);
    }

    #[test]
    fn pick_leaves_a_user_configured_draft_alone() {
        // Auto already off → the user chose an offload; don't second-guess it.
        let mut f = PresetForm {
            n_gpu_layers_draft_auto: false,
            n_gpu_layers_draft: 7,
            ..Default::default()
        };
        apply_draft_pick(&mut f, r"C:\m\mtps\head.gguf", "draft-mtp");
        assert_eq!(f.n_gpu_layers_draft, 7);
        assert_eq!(f.device_draft, "");

        // Device already pinned → keep it, even with auto on.
        let mut f = PresetForm {
            n_gpu_layers_draft_auto: true,
            device_draft: "CUDA1".into(),
            ..Default::default()
        };
        apply_draft_pick(&mut f, r"C:\m\mtps\head.gguf", "draft-mtp");
        assert!(f.n_gpu_layers_draft_auto);
        assert_eq!(f.device_draft, "CUDA1");
    }

    // A draft file swaps the drafter entry of a chain; the ticked n-grams stay.
    #[test]
    fn pick_keeps_the_ngram_entries() {
        let mut f = form_with_auto_draft();
        f.spec_type = "ngram-map-k4v,draft-mtp".into();
        apply_draft_pick(&mut f, r"C:\m\dflashs\d.gguf", "draft-dflash");
        assert_eq!(f.spec_type, "ngram-map-k4v,draft-dflash");
        apply_draft_pick(&mut f, "", "none");
        assert_eq!(f.spec_type, "ngram-map-k4v");
    }

    #[test]
    fn picking_none_clears_without_pinning() {
        let mut f = form_with_auto_draft();
        apply_draft_pick(&mut f, "", "");
        assert_eq!(f.model_draft, "");
        assert_eq!(f.spec_type, "none");
        assert!(f.n_gpu_layers_draft_auto, "(none) must not flip auto off");
        assert_eq!(f.device_draft, "");
    }

    #[test]
    fn delete_dialog_ticks_only_files_nobody_else_uses() {
        let lf = |name: &str, used: &[&str]| crate::linked_files::LinkedFile {
            role: crate::linked_files::Role::Model,
            path: name.into(),
            files: vec![name.into()],
            size: 1,
            used_by: used.iter().map(|u| u.to_string()).collect(),
        };
        let mut d = DeleteDialog {
            id: "p".into(),
            files: vec![lf("a.gguf", &[]), lf("b.gguf", &["other"])],
            ticked: vec![false, false],
        };
        d.toggle(1);
        assert_eq!(d.ticked, [false, false], "an in-use file cannot be ticked");
        d.toggle(0);
        d.toggle(5);
        d.toggle(-1);
        assert_eq!(d.ticked, [true, false]);
        assert_eq!(d.ticked_count(), 1);
        d.toggle(0);
        assert_eq!(d.ticked_count(), 0, "a second click unticks");
        // A dialog opened on another preset deletes nothing.
        d.toggle(0);
        assert_eq!(delete_ticked_files(&d, "someone-else"), Default::default());
    }
}
