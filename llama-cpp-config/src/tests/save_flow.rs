//! End-to-end save / revert / delete / rename / clone flow over the REAL
//! Models-tab wiring, plus the Integrations tab's rebuild invariant.
//!
//! `ui_bindings.rs` covers binding *direction*; this covers the callback
//! *funnel*: save → `preset_written` → reload → reselect → re-baseline, the
//! Revert path, delete's deliberate clear-selection sequence, the New…
//! dialog's id de-conflict guard, the rename + clone funnels, the
//! discard-confirm guard on a dirty form — the wiring in `gui/models_tab.rs`
//! that no pure-Rust unit test can reach — and the Integrations row-checkbox
//! contract (in-place toggle from the widget itself, full model rebuild for
//! everything else).
//!
//! Config IO is redirected at a temp dir through `LLAMA_CPP_CONFIG_DATA_ROOT`
//! (see `paths::data_root`), set here BEFORE the tabs are wired, so the flow
//! never touches the user's real `%LOCALAPPDATA%\llama.cpp`.
//!
//! Not a `#[test]` of its own — exposes `run(&app)`, called from ui_bindings'
//! single shared `#[test]`. Topology rationale: `src/tests/mod.rs`.

use i_slint_backend_testing::{self as itest, ElementHandle};
use slint::{ComponentHandle, Model};

use crate::gui::{AppState, AppWindow};

pub(super) fn run(app: &AppWindow) {
    // Redirect ALL config IO before anything below reads or writes a path.
    // The TempDir guard lives to the end of the flow.
    let dir = tempfile::tempdir().expect("tempdir");
    std::env::set_var("LLAMA_CPP_CONFIG_DATA_ROOT", dir.path());

    crate::gui::wire_tabs_for_tests(app);
    let st = app.global::<AppState>();

    // ── Guard rails: a save with no id (or no model) errors, writes nothing ──
    st.invoke_save_preset();
    assert!(st.get_status_is_error(), "empty-id save must set an error");
    assert!(
        !crate::paths::presets_ini().exists(),
        "a rejected save must not create presets.ini"
    );

    // ── Save: reload + reselect + re-baseline ────────────────────────────
    let model_path = dir.path().join("models").join("e2e.gguf");
    let mut form = st.get_form();
    form.id = "e2e".into();
    form.model = model_path.to_string_lossy().as_ref().into();
    st.set_form(form);
    st.invoke_save_preset();
    assert!(
        !st.get_status_is_error(),
        "save failed: {}",
        st.get_status_text()
    );
    let ini = std::fs::read_to_string(crate::paths::presets_ini()).expect("presets.ini written");
    assert!(ini.contains("[e2e]"), "saved section missing:\n{ini}");
    assert_eq!(
        st.get_selected_preset_index(),
        0,
        "save must reselect the saved preset"
    );
    assert_eq!(st.get_form().id.as_str(), "e2e");
    assert!(
        !st.get_preset_dirty(),
        "save must re-baseline the form (Save/Revert disabled)"
    );

    // ── Edit → Revert restores the on-disk value ─────────────────────────
    let mut form = st.get_form();
    form.device = "CUDA0".into();
    st.set_form(form);
    assert!(st.get_preset_dirty(), "an edit must mark the form dirty");
    st.invoke_revert_preset();
    assert_eq!(
        st.get_form().device.as_str(),
        "",
        "revert must restore the saved value"
    );
    assert!(!st.get_preset_dirty());

    // ── Delete clears the file, the selection, and the editor ────────────
    st.invoke_delete_preset("e2e".into());
    let ini = std::fs::read_to_string(crate::paths::presets_ini()).unwrap_or_default();
    assert!(!ini.contains("[e2e]"), "deleted section still on disk");
    assert_eq!(
        st.get_selected_preset_index(),
        -1,
        "delete must clear the selection"
    );
    assert_eq!(
        st.get_form().id.as_str(),
        "",
        "delete must blank the editor"
    );

    // ── New… twice on the same model must NOT overwrite the first preset ──
    // Drive the real dialog funnel: New… scans models_dir for the picker, the
    // pick derives the id from the file name — so the second pass hits a live
    // id and must save under the first free suffix instead of clobbering.
    std::fs::create_dir_all(model_path.parent().unwrap()).expect("models dir");
    std::fs::write(&model_path, b"").expect("model file");
    // The scans read the SAVED server.ini (not the live form), so point the
    // saved ModelsDir at the temp tree. Writes under the redirect (see above).
    let mut cfg = crate::server_cfg::load();
    cfg.models_dir = Some(dir.path().to_string_lossy().into_owned());
    crate::server_cfg::save(&cfg).expect("save server.ini");
    for _ in 0..2 {
        st.invoke_new_preset();
        assert!(
            st.get_dialog_model_labels().row_count() >= 1,
            "New… dialog must list the scanned model"
        );
        st.set_dialog_model_index(0);
        st.invoke_pick_new_empty();
        assert!(
            !st.get_status_is_error(),
            "New… failed: {}",
            st.get_status_text()
        );
    }
    let ini = std::fs::read_to_string(crate::paths::presets_ini()).expect("presets.ini");
    assert!(
        ini.contains("[e2e]") && ini.contains("[e2e-2]"),
        "second New… must de-conflict to e2e-2, not overwrite:\n{ini}"
    );

    // ── Rename: funnel = rename + reselect + re-baseline ─────────────────
    st.invoke_rename_preset("e2e".into(), "e2e-renamed".into());
    assert!(
        !st.get_status_is_error(),
        "rename failed: {}",
        st.get_status_text()
    );
    let ini = std::fs::read_to_string(crate::paths::presets_ini()).expect("presets.ini");
    assert!(
        !ini.contains("[e2e]\n") && !ini.contains("[e2e]\r\n"),
        "old section must be gone:\n{ini}"
    );
    assert!(
        ini.contains("[e2e-renamed]"),
        "renamed section missing:\n{ini}"
    );
    assert_eq!(
        st.get_form().id.as_str(),
        "e2e-renamed",
        "rename must reselect the renamed preset"
    );
    assert!(!st.get_preset_dirty(), "rename must re-baseline the form");

    // ── Clone: copies the source's parameters onto the picked model ──────
    // Give the source a distinguishing parameter first, then clone it onto
    // the (only) scanned model; the id derives from the file name and "e2e"
    // is free again after the rename.
    let mut form = st.get_form();
    form.device = "CUDA1".into();
    st.set_form(form);
    st.invoke_save_preset();
    assert!(!st.get_status_is_error());
    st.invoke_clone_preset();
    assert!(st.get_show_new_kind_picker(), "Clone… must open the picker");
    assert_eq!(
        st.get_new_dialog_source_id().as_str(),
        "e2e-renamed",
        "the picker must surface the clone source"
    );
    st.set_dialog_model_index(0);
    // The picker's Clone button hides the modal before firing the callback —
    // mirror that, or the dialog state leaks into the next phase.
    st.set_show_new_kind_picker(false);
    st.invoke_pick_new_clone();
    assert!(
        !st.get_status_is_error(),
        "clone failed: {}",
        st.get_status_text()
    );
    assert_eq!(
        st.get_form().id.as_str(),
        "e2e",
        "clone must reselect the new preset under the de-conflicted id"
    );
    assert_eq!(
        st.get_form().device.as_str(),
        "CUDA1",
        "clone must copy the source's parameters"
    );

    // ── Dirty guard: navigation on a dirty form asks before discarding ───
    let mut form = st.get_form();
    form.ctx_size = 12345;
    st.set_form(form);
    assert!(st.get_preset_dirty());
    st.invoke_new_preset();
    assert!(
        st.get_show_discard_dialog(),
        "New… on a dirty form must raise the discard dialog"
    );
    assert!(
        !st.get_show_new_kind_picker(),
        "the parked action must wait for the verdict"
    );
    st.invoke_cancel_discard();
    assert!(!st.get_show_discard_dialog());
    assert_eq!(
        st.get_form().ctx_size,
        12345,
        "cancel must keep the unsaved edits"
    );
    st.invoke_new_preset();
    st.invoke_confirm_discard();
    assert!(
        st.get_show_new_kind_picker(),
        "confirm must run the parked action"
    );
    st.set_show_new_kind_picker(false);

    // ── Rename… gets the same dirty guard (it reloads from disk on commit) ──
    assert!(
        st.get_preset_dirty(),
        "form still dirty from the phase above"
    );
    st.invoke_request_rename();
    assert!(
        st.get_show_discard_dialog(),
        "Rename… on a dirty form must raise the discard dialog"
    );
    assert!(
        !st.get_show_rename_dialog(),
        "the rename dialog must wait for the verdict"
    );
    st.invoke_confirm_discard();
    assert!(
        st.get_show_rename_dialog(),
        "confirm must open the rename dialog"
    );
    assert_eq!(
        st.get_rename_old_id().as_str(),
        st.get_form().id.as_str(),
        "the dialog must be seeded with the current preset id"
    );
    st.set_show_rename_dialog(false);

    // ── Integrations: a model rebuild must reach the row checkboxes ──────
    // The row CheckBox binds one-way (`checked: item.enabled`) — sanctioned
    // ONLY because the in-place toggle originates from the clicked widget
    // itself, and every OTHER enabled-state change rebuilds the whole model
    // (refresh_integrations replaces the ModelRc → fresh delegates). This
    // pins the rebuild half: click a row checkbox (the self-assign that
    // permanently breaks that delegate's binding), then drive a Rust-side
    // reload and assert the checkbox followed. A set_row_data "optimization"
    // in refresh_integrations would leave the clicked checkbox stale here.
    st.set_current_tab(2);
    st.invoke_revert_integrations(); // (re)build integration_models from disk
    itest::mock_elapsed_time(std::time::Duration::from_millis(1));
    let models = st.get_integration_models();
    assert!(
        models.row_count() >= 1,
        "the presets saved above must be listed"
    );
    let first_id = models.row_data(0).expect("row 0").id;
    assert!(
        !models.row_data(0).expect("row 0").enabled,
        "nothing is exposed in opencode.json yet"
    );
    let label = format!("integration-{first_id}");
    let cb = ElementHandle::find_by_accessible_label(app, label.as_str())
        .next()
        .expect("row checkbox");
    cb.invoke_accessible_default_action(); // the user click that breaks the binding
    assert!(
        st.get_integration_models()
            .row_data(0)
            .expect("row 0")
            .enabled,
        "the toggle callback must flip the row in place"
    );
    st.invoke_revert_integrations(); // Rust-side rebuild: back to disk state
    itest::mock_elapsed_time(std::time::Duration::from_millis(1));
    let cb = ElementHandle::find_by_accessible_label(app, label.as_str())
        .next()
        .expect("row checkbox after rebuild");
    assert_eq!(
        cb.accessible_checked(),
        Some(false),
        "a rebuild must recreate the delegate so the checkbox tracks the model again"
    );
}
