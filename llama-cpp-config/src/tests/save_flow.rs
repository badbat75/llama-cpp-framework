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

    // Stand in for the `--list-devices` probe (there is no llama-server here), so
    // the GPU distribution table has rows to drive further down. Process-wide,
    // like the real probe cache; seeded before the wiring so every rebuild sees it.
    crate::devices::set_probed(crate::devices::parse(
        "  CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)\n  \
         ROCm1: AMD Radeon AI PRO R9700 (32624 MiB, 32462 MiB free)\n",
    ));

    crate::gui::wire_tabs_for_tests(app);
    let st = app.global::<AppState>();

    // ── Guard rails: a save with no id (or no model) errors, writes nothing ──
    st.invoke_save_preset();
    assert!(st.get_status_is_error(), "empty-id save must set an error");
    assert!(
        !crate::paths::presets_ini().exists(),
        "a rejected save must not create presets.ini"
    );

    // ── Guard rails: save() itself enforces the `;`/`#` path validation ──
    // The pure `validate_for_save` fns have their own unit tests; THIS pins
    // the call-site wiring (`save()`'s first line, both sides) — deleting
    // either call used to pass the whole suite, silently regressing the
    // v1.2.11 reload-truncated guard.
    let mut form = st.get_form();
    form.id = "hostile".into();
    form.model = r"C:\Models #1\m.gguf".into();
    st.set_form(form);
    st.invoke_save_preset();
    assert!(
        st.get_status_is_error(),
        "a `#` model path must be rejected by presets::save"
    );
    assert!(
        !crate::paths::presets_ini().exists(),
        "a path-rejected save must not create presets.ini"
    );
    let hostile_server = crate::server_cfg::ServerConfig {
        models_dir: Some(r"E:\llm ; models".into()),
        ..Default::default()
    };
    assert!(
        crate::server_cfg::save(&hostile_server).is_err(),
        "a `;` ModelsDir must be rejected by server_cfg::save"
    );
    assert!(
        !crate::paths::server_ini().exists(),
        "a path-rejected save must not create server.ini"
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

    // ── GPU distribution table: the whole --device/--tensor-split funnel ──
    // The table is the ONLY writer of `device` + `tensor_split` now, so this
    // drives the real row checkboxes and asserts the strings that reach the INI.
    // It starts from the clone above, whose device is "CUDA1" — a device the
    // probe doesn't know, which must still show as a checked row (a save that
    // silently dropped it would rewrite the user's config).
    let row_ids = || -> Vec<String> {
        let rows = st.get_preset_gpu_rows();
        (0..rows.row_count())
            .map(|i| rows.row_data(i).expect("row").id.to_string())
            .collect()
    };
    // The clone's stale CUDA1 pin is the only checked device, so it heads the
    // table; the probed GPUs follow in probe order. The row order IS the split
    // order — which is what makes the drag handle (exercised below) meaningful.
    assert_eq!(
        row_ids(),
        ["CUDA1", "CUDA0", "ROCm1"],
        "the checked device leads, then the probe"
    );
    let rows = st.get_preset_gpu_rows();
    let unknown = rows.row_data(0).expect("row 0");
    assert!(unknown.enabled && !unknown.detected, "the stale CUDA1 pin is kept");
    assert_eq!(rows.row_data(1).expect("row 1").vram.as_str(), "12.0 GB (10.6 free)");

    let gpu_checkbox = |id: &str| {
        ElementHandle::find_by_accessible_label(app, format!("gpu-{id}").as_str())
            .next()
            .unwrap_or_else(|| panic!("no checkbox for {id}"))
    };
    gpu_checkbox("CUDA1").invoke_accessible_default_action(); // uncheck the stale pin
    assert_eq!(st.get_form().device.as_str(), "");
    assert_eq!(row_ids(), ["CUDA0", "ROCm1"], "dropping the pin drops its row");
    gpu_checkbox("ROCm1").invoke_accessible_default_action();
    gpu_checkbox("CUDA0").invoke_accessible_default_action();
    assert_eq!(
        st.get_form().device.as_str(),
        "ROCm1,CUDA0",
        "a checked device is APPENDED — the split order is the order you checked"
    );
    assert_eq!(row_ids(), ["ROCm1", "CUDA0"], "the rows follow the split order");
    assert_eq!(st.get_preset_gpu_selected(), 2);
    assert_eq!(
        st.get_form().tensor_split.as_str(),
        "",
        "two devices start in Auto: llama.cpp splits by free VRAM"
    );
    assert_eq!(st.get_preset_gpu_total(), 0, "Auto ⇒ no weight denominator");

    st.invoke_preset_gpu_even();
    assert_eq!(st.get_form().tensor_split.as_str(), "1,1");
    st.invoke_preset_gpu_weight("ROCm1".into(), 3);
    assert_eq!(
        st.get_form().tensor_split.as_str(),
        "3,1",
        "the weight follows its device's position in the list"
    );
    assert_eq!(st.get_preset_gpu_total(), 4, "the Share column's denominator");
    assert!(
        st.get_preset_gpu_summary().contains("--tensor-split 3,1"),
        "summary: {}",
        st.get_preset_gpu_summary()
    );
    st.invoke_preset_gpu_auto();
    assert_eq!(
        st.get_form().tensor_split.as_str(),
        "",
        "Auto clears the vector — it is NOT the same launch as Even"
    );
    st.invoke_preset_gpu_even();
    st.invoke_preset_gpu_weight("ROCm1".into(), 3);

    // The drag handle: promote CUDA0 to the head of the split. Position 0 is
    // llama.cpp's main_gpu, and a checkbox can only append — so this is the only
    // way to get there. The weight must ride along with its device.
    st.invoke_preset_gpu_move("CUDA0".into(), -1);
    assert_eq!(st.get_form().device.as_str(), "CUDA0,ROCm1");
    assert_eq!(st.get_form().tensor_split.as_str(), "1,3", "weights rode along");
    assert_eq!(row_ids(), ["CUDA0", "ROCm1"], "the rows follow the new order");
    st.invoke_preset_gpu_move("CUDA0".into(), 1); // back, so the INI below is 3,1
    assert_eq!(st.get_form().device.as_str(), "ROCm1,CUDA0");

    st.invoke_save_preset();
    assert!(!st.get_status_is_error(), "{}", st.get_status_text());
    let ini = std::fs::read_to_string(crate::paths::presets_ini()).expect("presets.ini");
    assert!(
        ini.contains("device = ROCm1,CUDA0") && ini.contains("tensor-split = 3,1"),
        "the table's selection must reach the INI:\n{ini}"
    );

    // The rebuild half of the one-way `for`-row binding contract (same shape as
    // the Integrations phase below): the click above self-assigned `checked`,
    // permanently breaking THAT delegate's binding. Revert rebuilds the whole row
    // model from disk, so the checkbox must follow — a set_row_data
    // "optimization" in refresh_gpu_rows would leave it stale and lying.
    gpu_checkbox("CUDA0").invoke_accessible_default_action(); // uncheck → dirty
    assert_eq!(st.get_form().device.as_str(), "ROCm1");
    st.invoke_revert_preset();
    assert_eq!(st.get_form().device.as_str(), "ROCm1,CUDA0");
    assert_eq!(
        gpu_checkbox("CUDA0").accessible_checked(),
        Some(true),
        "a rebuild must reach the clicked checkbox"
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
    // The pending toggle is what the F5/Refresh discard guard consults —
    // integrations_dirty compares the UI rows against the on-disk enabled set.
    assert!(
        crate::gui::integrations_dirty(app),
        "a pending toggle must read as integrations-dirty"
    );
    // A preset write path rebuilds the list with MERGE semantics: the pending
    // toggle must survive (fresh delegate, preserved enabled flag) — only the
    // reset paths (F5 behind the guard, Integrations Save/Revert) drop it.
    st.invoke_save_preset(); // → preset_written → refresh_integrations (merge)
    itest::mock_elapsed_time(std::time::Duration::from_millis(1));
    assert!(
        !st.get_status_is_error(),
        "re-save failed: {}",
        st.get_status_text()
    );
    let cb = ElementHandle::find_by_accessible_label(app, label.as_str())
        .next()
        .expect("row checkbox after merge rebuild");
    assert_eq!(
        cb.accessible_checked(),
        Some(true),
        "a preset save must not wipe a pending Integrations toggle"
    );
    assert!(
        crate::gui::integrations_dirty(app),
        "the pending toggle must stay integrations-dirty across a preset save"
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
    assert!(
        !crate::gui::integrations_dirty(app),
        "a rebuild from disk must clear the integrations-dirty signal"
    );

    // ── Integrations Save writes opencode.json (create_dir_all + reset leg) ──
    // Under the redirect the parent dir (<tmp>\opencode\) does NOT exist — the
    // exact "OpenCode never ran here" shape the v1.2.13 create_dir_all fix is
    // for. Re-toggle a row on, Save, and assert the file appears, the id is
    // registered, and the save (→ refresh_integrations_reset) re-baselined to
    // clean. Deleting the create_dir_all block, or dropping the reset call,
    // fails here.
    assert!(
        !crate::paths::opencode_user_config().exists(),
        "opencode.json (and its parent dir) must be absent before the first save"
    );
    let cb = ElementHandle::find_by_accessible_label(app, label.as_str())
        .next()
        .expect("row checkbox before save");
    cb.invoke_accessible_default_action(); // re-expose row 0
    assert!(
        crate::gui::integrations_dirty(app),
        "re-toggle must be dirty"
    );
    st.invoke_save_integrations();
    itest::mock_elapsed_time(std::time::Duration::from_millis(1));
    assert!(
        !st.get_status_is_error(),
        "Integrations Save failed (missing opencode dir?): {}",
        st.get_status_text()
    );
    assert!(
        crate::paths::opencode_user_config().exists(),
        "save must create opencode.json even when its dir never existed"
    );
    assert!(
        crate::integrations::opencode_model_ids().contains(&first_id.to_string()),
        "the toggled preset must be registered in opencode.json"
    );
    assert!(
        !crate::gui::integrations_dirty(app),
        "save (→ refresh_integrations_reset) must re-baseline to clean"
    );

    // ── CLI: `preset delete <typo>` errors instead of a false "Removed" ─────
    // The v1.2.13 lookup-before-delete guard; the redirect keeps cli::run on
    // the temp tree. (`load_all` resolves real paths, so this can't be an
    // inline unit test — only the redirect harness reaches it.)
    use crate::cli::{Cli, Command, PresetCmd};
    assert!(
        crate::cli::run(Cli {
            command: Command::Preset(PresetCmd::Delete {
                id: "no-such-preset".into(),
            }),
        })
        .is_err(),
        "deleting a nonexistent preset must error, not report success"
    );
}
