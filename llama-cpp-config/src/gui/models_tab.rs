//! Models-tab callback wiring (the preset editor + New / Clone / Rename dialogs).
//!
//! Shared state, generated Slint types, and the `refresh_*` / `reload_presets` /
//! `apply_form` helpers live in the parent `gui` module; `use super::*` pulls
//! them in. The Models-tab-only helper families live HERE next to their callers
//! instead of in the shared hub:
//! - `update_model_info` — the GGUF "Model info" box;
//! - the New / Clone dialog funnel (`populate_dialog_models` … `commit_new_preset`);
//! - the discard-guarded navigation bodies (`do_select_preset`,
//!   `open_new_dialog`, `open_clone_dialog`, and the Rename seed behind
//!   `request_rename`) — the actions `confirm_discard_then` parks;
//! - `preset_written` — the reload + reselect + re-baseline invariant every
//!   write path (save / rename / clone / New…) funnels through;
//! - `apply_draft_pick` — the merged MTP/DFlash draft-pick policy (incl. the
//!   gemma4 auto-pin), unit-tested below.

use super::*;

// The Model-info box is the only helper that reaches `gguf` directly; gui.rs no
// longer does, so pull it in here rather than through the parent's `use super::*`.
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
            // Switching presets replaces the form — unsaved edits need the
            // discard confirmation first.
            let dirty = app.global::<AppState>().get_preset_dirty();
            let action: Box<dyn Fn()> = {
                let app_weak = app_weak.clone();
                let state = state.clone();
                Box::new(move || {
                    let Some(app) = app_weak.upgrade() else {
                        return;
                    };
                    do_select_preset(&app, &state, index);
                })
            };
            confirm_discard_then(&app, &state, dirty, action);
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
                    // Drop the id from opencode.json too, or OpenCode keeps
                    // offering a model llama-server no longer knows.
                    sync_opencode_after_preset_change(&app, id.as_str(), None);
                    // Not preset_written(): after a delete we clear the selection
                    // and show an empty editor, so the file/integration refreshes
                    // must run AFTER the form is blanked, not against the neighbour
                    // reload_presets would otherwise select.
                    reload_presets(&app, &state, None);
                    app.global::<AppState>().set_selected_preset_index(-1);
                    apply_form(&app, PresetForm::default());
                    refresh_file_options(&app, &state);
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
            // Committing the new preset will replace the form (preset_written
            // reloads + reselects), so guard dirty edits at the entry point —
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
            // Same entry-point guard as New…/Clone… — committing the rename
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
                        // Carry an exposed opencode.json model over to the new id.
                        sync_opencode_after_preset_change(
                            &app,
                            old_id.as_str(),
                            Some(new_id.as_str()),
                        );
                        preset_written(&app, &state, Some(new_id.as_str()));
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
            let mut form = s.get_form();
            // The server-wide fallback is the SAVED device (where the model will
            // actually run at launch) — not the live, possibly-unsaved Server
            // form, which would leak an edited-then-reverted device into a save.
            let server_device = crate::server_cfg::load().device.unwrap_or_default();
            apply_draft_pick(&mut form, &value, &spec, &server_device);
            s.set_form(form);
            // The draft-device dropdown may have been changed programmatically.
            refresh_device_options(&app);
            update_model_info(&app);
        });
    }

    wire_gpu_table(app);
}

/// The per-preset GPU distribution table's four callbacks — the twin of
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
            let sel = gpu_split::toggle(&devices::probed(), &preset_selection(&app), id.as_str());
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
            let sel = gpu_split::set_weight(
                &devices::probed(),
                &preset_selection(&app),
                id.as_str(),
                weight,
            );
            set_preset_selection(&app, &sel);
            refresh_gpu_scalars(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        s.on_preset_gpu_auto(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let sel = gpu_split::set_auto(&devices::probed(), &preset_selection(&app));
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
            let sel = gpu_split::set_even(&devices::probed(), &preset_selection(&app));
            set_preset_selection(&app, &sel);
            refresh_gpu_rows(&app);
        });
    }
}

// ── Discard-guarded navigation (select / New / Clone) ─────────────────

/// The actual preset switch (the guarded body of `on_select_preset`): load the
/// indexed preset into the editor and refresh its dependent dropdowns.
fn do_select_preset(app: &AppWindow, state: &Rc<RefCell<State>>, index: i32) {
    let st = state.borrow();
    if let Some(p) = usize::try_from(index).ok().and_then(|i| st.presets.get(i)) {
        app.global::<AppState>().set_selected_preset_index(index);
        apply_form(app, preset_to_form(p));
        drop(st);
        refresh_file_options(app, state);
    }
}

/// Open the model picker in plain "New" mode (the guarded body of
/// `on_new_preset`). "New…" is always create-from-scratch — independent of any
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
            .cloned()
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

// ── Draft-pick policy ─────────────────────────────────────────────────

/// Apply a draft-picker selection to the form: set `model_draft` + `spec_type`
/// from the picked row (MTP heads → draft-mtp, DFlash drafters → draft-dflash,
/// "(none)" → empty), then auto-pin an unconfigured draft to ONE device — the
/// multi-device "auto" split crashes gemma4 MTP heads. Pin to the SAME GPU the
/// model runs on so both land together: the preset's own `device` selection
/// wins, else the server-wide default (`server_device`, all layers on it);
/// otherwise fall back to CPU, which always works. A draft the user already
/// configured (auto off or a device pinned) is left alone.
///
/// Both device fields can now name SEVERAL GPUs ("ROCm1,CUDA0" — the distribution
/// table writes them in split order), but `--device-draft` pins one drafter, and
/// the whole point here is to avoid the multi-device split. So the pin is the
/// FIRST device of whichever list applies — the one llama.cpp's split starts from.
///
/// This only fires for a draft FILE. Embedded MTP heads (spec-type alone) never
/// reach it: llama.cpp ignores both keys for them and the UI disables both fields.
fn apply_draft_pick(form: &mut PresetForm, value: &str, spec: &str, server_device: &str) {
    form.model_draft = value.into();
    form.spec_type = spec.into();
    if !value.is_empty() && form.n_gpu_layers_draft_auto && form.device_draft.is_empty() {
        form.n_gpu_layers_draft_auto = false;
        // The model's own selection wins over the server default: pinning the
        // draft to a different device than the model is the exact split this
        // heuristic exists to avoid.
        let list = if form.device.is_empty() {
            server_device
        } else {
            form.device.as_str()
        };
        match gpu_split::parse_device_list(list).first() {
            Some(pin) => {
                form.device_draft = pin.as_str().into();
                form.n_gpu_layers_draft = ALL_LAYERS;
            }
            None => form.n_gpu_layers_draft = 0,
        }
    }
}

// ── Write invariant: reload + reselect + re-baseline ──────────────────

/// Refresh everything that depends on the preset set after a presets.ini write:
/// the (re-selected) preset list (`reload_presets`), the file/device dropdowns,
/// and the Integrations tab. `want` picks the selection like `reload_presets`.
/// The invariant every write path follows — save / rename / clone all funnel
/// through here. (select/revert do NOT: they don't mutate disk, so integrations
/// stay put; delete keeps its own sequence because it clears the selection.)
fn preset_written(app: &AppWindow, state: &Rc<RefCell<State>>, want: Option<&str>) {
    reload_presets(app, state, want);
    refresh_file_options(app, state);
    refresh_integrations(app);
}

// ── Model-info box (GGUF reads) ───────────────────────────────────────

/// Fill the read-only "Model info" box from the selected model's GGUF header
/// (read via `ggml-base.dll`), enriched with the selected mmproj and draft
/// headers plus a cross-reference of the framework's MTP/DFlash drafters. Called
/// whenever the model/mmproj/draft field changes (combo pick, preset load).
/// `pub(super)` so the shared `refresh_file_options` in gui.rs can drive it.
pub(super) fn update_model_info(app: &AppWindow) {
    let s = app.global::<AppState>();
    let form = s.get_form();
    let model = form.model.to_string();

    // Reset the optional rows; the success path re-enables the ones that apply.
    s.set_model_info_has_moe(false);
    s.set_model_info_has_mmproj(false);
    s.set_model_info_has_draft_file(false);
    s.set_model_info_embeds_mtp(false);
    // Reset the slider maxima; 0 = unknown → the UI falls back to a 0..99 range.
    s.set_model_info_n_layer(0);
    s.set_model_info_draft_n_layer(0);
    // Chat-template row: reset alongside the sibling `has_*` / `n_layer` resets
    // for coherence. Unlike those (which gate always-visible controls), this row
    // lives behind `model_info_ready`, so the value is never shown until the
    // success path below overwrites it — the gate, not this reset, is what keeps
    // a stale template from lingering.
    s.set_model_info_chat_template(SharedString::from("none"));
    s.set_chat_template_preview(SharedString::from(""));

    if model.trim().is_empty() {
        s.set_model_info_ready(false);
        s.set_model_info_note(SharedString::from(
            "Select a model to see its GGUF details.",
        ));
        return;
    }

    let Some(info) = gguf::read_model_info(std::path::Path::new(&model)) else {
        s.set_model_info_ready(false);
        s.set_model_info_note(SharedString::from(
            "Metadata unavailable — is ggml-base.dll beside the app, and the file a valid GGUF?",
        ));
        return;
    };

    // SAVED config, like refresh_file_options — see the note there.
    let models_dir = server_cfg::load().models_dir_or_default();
    let ext = gguf::external_drafters(&models_dir, &model);
    s.set_model_info_kind(SharedString::from(info.kind_line()));
    s.set_model_info_n_layer(info.n_layer as i32);
    s.set_model_info_has_moe(info.is_moe);
    s.set_model_info_moe(SharedString::from(info.moe_offload_line()));
    s.set_model_info_arch_quant(SharedString::from(info.arch_quant_line()));
    s.set_model_info_layers_ctx(SharedString::from(info.layers_ctx_line()));
    s.set_model_info_attn(SharedString::from(info.attn_line()));
    s.set_model_info_draft(SharedString::from(gguf::draft_line(&info, &ext)));
    // Chat template: short status for the InfoRow, raw text for the Preview
    // modal (empty when none — also gates the Preview button visibility).
    s.set_model_info_chat_template(SharedString::from(info.chat_template_line()));
    s.set_chat_template_preview(SharedString::from(
        info.chat_template.clone().unwrap_or_default(),
    ));
    // Enables the speculative-decoding controls even before an external draft is
    // picked, when the model itself embeds MTP/nextn heads.
    s.set_model_info_embeds_mtp(info.nextn_predict_layers > 0);

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
    // SAVED config, like refresh_file_options — see the note there.
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

/// Preset id for a newly picked model file, de-conflicted against the live
/// presets. The id derives from the file name, so picking a model that already
/// has a preset — or a different file that sanitizes to the same id — would
/// otherwise make `presets::save` wholesale-replace the tuned section. Both
/// New and Clone must route through this: first free `<id>`, `<id>-2`, ….
fn deconflicted_id(path_str: &str) -> String {
    let mut base_id = presets::make_id(path_str);
    if base_id.is_empty() {
        // make_id keeps only [A-Za-z0-9.\-_]; a model file named entirely in a
        // non-ASCII script (CJK, Cyrillic, …) sanitizes to "" → unique_id("")
        // stays "" → a `[]` section save() would then reject (and the GUI
        // couldn't delete, on_delete_preset early-returns on an empty id).
        // Fall back to a usable stem before de-conflicting.
        base_id = "preset".to_string();
    }
    let existing: Vec<String> = presets::load_all().into_iter().map(|p| p.id).collect();
    presets::unique_id(&base_id, &existing)
}

fn run_new_empty(app: &AppWindow, state: &Rc<RefCell<State>>, path: PathBuf) {
    let path_str = path.to_string_lossy().into_owned();
    let id = deconflicted_id(&path_str);
    let p = presets::Preset::new_default(id.clone(), path_str);
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
    let id = deconflicted_id(&path_str);
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

fn commit_new_preset(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    p: presets::Preset,
    success_status: String,
) {
    match presets::save(&p) {
        Ok(()) => {
            preset_written(app, state, Some(&p.id));
            set_status(app, success_status, false);
        }
        Err(e) => set_status(app, format!("Save failed: {e}"), true),
    }
}

// Pure-struct tests (no Slint backend needed — PresetForm is a plain generated
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
    fn pick_pins_draft_to_server_gpu_when_set() {
        let mut f = form_with_auto_draft();
        apply_draft_pick(&mut f, r"C:\m\mtps\head.gguf", "draft-mtp", "CUDA0");
        assert_eq!(f.model_draft, r"C:\m\mtps\head.gguf");
        assert_eq!(f.spec_type, "draft-mtp");
        assert!(!f.n_gpu_layers_draft_auto);
        assert_eq!(f.device_draft, "CUDA0");
        assert_eq!(f.n_gpu_layers_draft, 99);
    }

    #[test]
    fn pick_prefers_the_presets_own_device_over_the_server_default() {
        // The model is pinned to CUDA1; the draft must follow it there, not to
        // the server-wide CUDA0 — model and draft must share ONE device.
        let mut f = PresetForm {
            n_gpu_layers_draft_auto: true,
            device: "CUDA1".into(),
            ..Default::default()
        };
        apply_draft_pick(&mut f, r"C:\m\mtps\head.gguf", "draft-mtp", "CUDA0");
        assert_eq!(f.device_draft, "CUDA1");
        assert_eq!(f.n_gpu_layers_draft, 99);
    }

    #[test]
    fn pick_falls_back_to_cpu_without_a_server_device() {
        let mut f = form_with_auto_draft();
        apply_draft_pick(&mut f, r"C:\m\dflashs\d.gguf", "draft-dflash", "");
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
        apply_draft_pick(&mut f, r"C:\m\mtps\head.gguf", "draft-mtp", "CUDA0");
        assert_eq!(f.n_gpu_layers_draft, 7);
        assert_eq!(f.device_draft, "");

        // Device already pinned → keep it, even with auto on.
        let mut f = PresetForm {
            n_gpu_layers_draft_auto: true,
            device_draft: "CUDA1".into(),
            ..Default::default()
        };
        apply_draft_pick(&mut f, r"C:\m\mtps\head.gguf", "draft-mtp", "CUDA0");
        assert!(f.n_gpu_layers_draft_auto);
        assert_eq!(f.device_draft, "CUDA1");
    }

    #[test]
    fn picking_none_clears_without_pinning() {
        let mut f = form_with_auto_draft();
        apply_draft_pick(&mut f, "", "", "CUDA0");
        assert_eq!(f.model_draft, "");
        assert_eq!(f.spec_type, "");
        assert!(f.n_gpu_layers_draft_auto, "(none) must not flip auto off");
        assert_eq!(f.device_draft, "");
    }
}
