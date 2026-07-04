//! End-to-end save / revert / delete flow over the REAL Models-tab wiring.
//!
//! `ui_bindings.rs` covers binding *direction*; this covers the callback
//! *funnel*: save → `preset_written` → reload → reselect → re-baseline, the
//! Revert path, delete's deliberate clear-selection sequence, and the New…
//! dialog's id de-conflict guard — the wiring in `gui/models_tab.rs` that no
//! pure-Rust unit test can reach.
//!
//! Config IO is redirected at a temp dir through `LLAMA_CPP_CONFIG_DATA_ROOT`
//! (see `paths::data_root`), set here BEFORE the Models tab is wired, so the
//! flow never touches the user's real `%LOCALAPPDATA%\llama.cpp`.
//!
//! Not a `#[test]` of its own: the Slint testing backend is a process-global,
//! single-threaded platform, so all e2e phases share ui_bindings' single
//! `#[test]` (and its window) — this module exposes `run(&app)` and is called
//! from there after the binding assertions.

use slint::{ComponentHandle, Model};

use crate::gui::{AppState, AppWindow};

pub(super) fn run(app: &AppWindow) {
    // Redirect ALL config IO before anything below reads or writes a path.
    // The TempDir guard lives to the end of the flow.
    let dir = tempfile::tempdir().expect("tempdir");
    std::env::set_var("LLAMA_CPP_CONFIG_DATA_ROOT", dir.path());

    crate::gui::wire_models_tab_for_tests(app);
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
    let mut sform = st.get_server_form();
    sform.models_dir = dir.path().to_string_lossy().as_ref().into();
    st.set_server_form(sform);
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
}
