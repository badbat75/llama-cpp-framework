//! End-to-end save / revert / delete / rename / clone flow over the REAL
//! Models-tab wiring, plus the Integrations tab's rebuild invariant.
//!
//! `ui_bindings.rs` covers binding *direction*; this covers the callback
//! *funnel*: save → `preset_written` → reload → reselect → re-baseline, the
//! Revert path, delete's deliberate clear-selection sequence, the New…
//! dialog's id de-conflict guard, the rename + clone funnels, the
//! discard-confirm guard on a dirty form (the wiring in `gui/models_tab.rs`
//! that no pure-Rust unit test can reach), the Integrations row-checkbox
//! contract (in-place toggle from the widget itself, full model rebuild for
//! everything else), and the Hugging Face dialog's network-free wiring (its
//! refusals, the hand-over at Download to the background downloads and their
//! window, a landed download creating its preset without disturbing a dirty
//! editor, cancel, failure, Show preset, Quit asking first), and the Delete
//! dialog's linked files
//! (`delete_with_files_phase`: the rows, the in-use lock, the ticks the
//! confirm acts on, the folder cleanup, a failed delete reported) with the
//! CLI's `preset delete --delete-files` under the same rules.
//!
//! Config IO is redirected at a temp dir through `LLAMA_CPP_CONFIG_DATA_ROOT`
//! (see `paths::data_root`), set here BEFORE the tabs are wired, so the flow
//! never touches the user's real `%LOCALAPPDATA%\llama.cpp`.
//!
//! Not a `#[test]` of its own: exposes `run(&app)`, called from ui_bindings'
//! single shared `#[test]`. Topology rationale: `src/tests/mod.rs`.

use i_slint_backend_testing::{self as itest, ElementHandle};
use slint::{ComponentHandle, Model};

use crate::gui::{AppState, AppWindow};

pub(super) fn run(app: &AppWindow) {
    // Redirect ALL config IO before anything below reads or writes a path.
    // The TempDir guard lives to the end of the flow.
    let dir = tempfile::tempdir().expect("tempdir");
    std::env::set_var("LLAMA_CPP_CONFIG_DATA_ROOT", dir.path());
    // A prompt of the user's, configured before the tab first reads the
    // setting: the shipped ones are read-only, so the external-edit check
    // further down needs a file of its own.
    let prompt_path = crate::paths::config_dir().join("my-prompt.txt");
    std::fs::create_dir_all(crate::paths::config_dir()).expect("config dir");
    std::fs::write(&prompt_path, "Summarize the release notes.\n").expect("a user prompt");
    let mut cfg = crate::settings::load();
    cfg.bench_prompt_file = prompt_path.to_string_lossy().into_owned();
    crate::settings::save(&cfg).expect("point BenchPromptFile at it");

    // Stand in for the `--list-devices` probe (there is no llama-server here), so
    // the GPU distribution table has rows to drive further down. Process-wide,
    // like the real probe cache; seeded before the wiring so every rebuild sees it.
    // The CPU line is deliberate: the real probe always prints it, the GPU table
    // must FILTER it out (`--device` won't take it) and the tensor-placement table
    // must OFFER it (`--override-tensor` explicitly targets it), both asserted below.
    crate::devices::set_probed(crate::devices::parse(
        "  CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)\n  \
         ROCm1: AMD Radeon AI PRO R9700 (32624 MiB, 32462 MiB free)\n  \
         CPU: AMD Ryzen 9 9900X (63090 MiB, 48233 MiB free)\n",
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
    // the call-site wiring (`save()`'s first line, both sides): deleting
    // either call used to pass the whole suite, silently regressing the
    // v1.2.11 reload-truncated guard.
    //
    // From a NEW preset's form, not the blank editor's: the blank struct has every
    // "default" box unticked over empty text, which the numeric refusal (tested
    // below) would reject before `presets::save` is ever reached.
    let mut form = crate::form::preset_to_form(&crate::presets::Preset::default());
    form.id = "hostile".into();
    form.model = r"C:\Models #1\m.gguf".into();
    st.set_form(form);
    st.invoke_save_preset();
    assert!(
        st.get_status_is_error() && !st.get_status_text().contains("Not saved:"),
        "a `#` model path must be rejected by presets::save: {}",
        st.get_status_text()
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
    let mut form = crate::form::preset_to_form(&crate::presets::Preset::default());
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

    // ── An n-gram speculator survives the save without a draft ──
    // The save prunes the speculative keys a model cannot act on
    // (`presets::prune_inactive_draft_keys`). `ngram-mod` drafts from the
    // context, so on a model with no draft file and no MTP heads it must reach
    // the INI, while the draft-model key beside it (`spec-draft-n-max`) and a
    // `draft-mtp` that would fail the load are still dropped.
    st.set_model_info_ready(true);
    st.set_model_info_embeds_mtp(false);
    let mut form = st.get_form();
    form.spec_type = "ngram-mod".into();
    form.spec_draft_n_max = "8".into();
    form.spec_draft_n_max_default = false;
    form.spec_ngram_mod_n_match = "32".into();
    form.spec_ngram_mod_n_match_default = false;
    form.spec_ngram_simple_size_n = "8".into();
    form.spec_ngram_simple_size_n_default = false;
    st.set_form(form);
    st.invoke_save_preset();
    assert!(!st.get_status_is_error(), "{}", st.get_status_text());
    let saved = std::fs::read_to_string(crate::paths::presets_ini()).expect("presets.ini");
    assert!(
        saved.contains("spec-type = ngram-mod"),
        "an n-gram spec-type needs no draft and must be saved:\n{saved}"
    );
    assert!(
        !saved.contains("\nspec-draft-n-max ="),
        "the draft-model keys still go without a draft:\n{saved}"
    );
    assert!(
        saved.contains("spec-ngram-mod-n-match = 32"),
        "the selected n-gram type keeps its tuning:\n{saved}"
    );
    assert!(
        !saved.contains("\nspec-ngram-simple-size-n ="),
        "an unselected n-gram type's tuning is dropped:\n{saved}"
    );
    // The save reselects the preset, and its model (no file here) cannot be
    // read, which clears `model_info_ready`: stand in for the header again.
    st.set_model_info_ready(true);
    st.set_model_info_embeds_mtp(false);
    let mut form = st.get_form();
    form.spec_type = "draft-mtp".into();
    st.set_form(form);
    st.invoke_save_preset();
    assert!(!st.get_status_is_error(), "{}", st.get_status_text());
    let saved = std::fs::read_to_string(crate::paths::presets_ini()).expect("presets.ini");
    assert!(
        !saved.contains("\nspec-type ="),
        "draft-mtp without MTP heads must not reach the INI:\n{saved}"
    );
    assert!(
        !saved.contains("\nspec-ngram-mod-n-match ="),
        "with ngram-mod off its tuning goes too:\n{saved}"
    );
    st.set_model_info_ready(false);
    let ini = saved;

    // ── A mistyped integer refuses the save instead of dropping the key ──
    // `form_to_preset` reads unparseable text as unset, and an unset ctx-size is
    // the model's TRAINED context: a preset saved that way loads at 262144 and
    // pages. 245760 typed next to the old value overflows i32.
    let mut form = st.get_form();
    form.ctx_size = "196608245760".into();
    form.ctx_size_default = false;
    st.set_form(form);
    st.invoke_save_preset();
    assert!(
        st.get_status_is_error() && st.get_status_text().contains("ctx-size = `196608245760`"),
        "an overflowing ctx-size must be refused by name: {}",
        st.get_status_text()
    );
    assert_eq!(
        std::fs::read_to_string(crate::paths::presets_ini()).expect("presets.ini"),
        ini,
        "a refused save must leave presets.ini untouched"
    );
    assert!(
        st.get_preset_dirty(),
        "the edit stays in the form to be corrected"
    );
    st.invoke_revert_preset();

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
    // pick derives the id from the file name, so the second pass hits a live
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
    // The picker's Clone button hides the modal before firing the callback:
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
    // It starts from the clone above, whose device is "CUDA1", a device the
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
    // order, which is what makes the drag handle (exercised below) meaningful.
    assert_eq!(
        row_ids(),
        ["CUDA1", "CUDA0", "ROCm1"],
        "the checked device leads, then the probe"
    );
    let rows = st.get_preset_gpu_rows();
    let unknown = rows.row_data(0).expect("row 0");
    assert!(
        unknown.enabled && !unknown.detected,
        "the stale CUDA1 pin is kept"
    );
    assert_eq!(
        rows.row_data(1).expect("row 1").vram.as_str(),
        "12.0 GB (10.6 free)"
    );

    let gpu_checkbox = |id: &str| {
        ElementHandle::find_by_accessible_label(app, format!("gpu-{id}").as_str())
            .next()
            .unwrap_or_else(|| panic!("no checkbox for {id}"))
    };
    gpu_checkbox("CUDA1").invoke_accessible_default_action(); // uncheck the stale pin
    assert_eq!(st.get_form().device.as_str(), "");
    assert_eq!(
        row_ids(),
        ["CUDA0", "ROCm1"],
        "dropping the pin drops its row"
    );
    gpu_checkbox("ROCm1").invoke_accessible_default_action();
    gpu_checkbox("CUDA0").invoke_accessible_default_action();
    assert_eq!(
        st.get_form().device.as_str(),
        "ROCm1,CUDA0",
        "a checked device is APPENDED: the split order is the order you checked"
    );
    assert_eq!(
        row_ids(),
        ["ROCm1", "CUDA0"],
        "the rows follow the split order"
    );
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
    assert_eq!(
        st.get_preset_gpu_total(),
        4,
        "the Share column's denominator"
    );
    // The split-mode pick funnels through Rust (the combo is a MappedComboBox:
    // the pick writes the form, and the refresh re-projects the rows), and the
    // mode bends the columns: under `none` the vector is dead, so the head row
    // reads 100% and the other CHECKED row says "unused" out loud (a "—" would
    // read as merely unchecked).
    st.invoke_preset_split_mode_picked("none".into());
    assert_eq!(st.get_form().split_mode.as_str(), "none");
    assert_eq!(st.get_preset_gpu_mode().as_str(), "none");
    assert_eq!(st.get_preset_split_mode_index(), 1);
    {
        let rows = st.get_preset_gpu_rows();
        assert_eq!(rows.row_data(0).expect("row 0").share.as_str(), "100%");
        assert_eq!(rows.row_data(1).expect("row 1").share.as_str(), "unused");
    }
    // Back to the merged "layer (default)" entry: index and effective mode
    // follow, and the ratio the mode never touched is still there.
    st.invoke_preset_split_mode_picked("default".into());
    assert_eq!(st.get_form().split_mode.as_str(), "default");
    assert_eq!(st.get_preset_split_mode_index(), 0);
    assert_eq!(st.get_preset_gpu_mode().as_str(), "layer");
    assert_eq!(st.get_form().tensor_split.as_str(), "3,1");
    st.invoke_preset_gpu_auto();
    assert_eq!(
        st.get_form().tensor_split.as_str(),
        "",
        "Auto clears the vector; it is NOT the same launch as Even"
    );
    st.invoke_preset_gpu_even();
    st.invoke_preset_gpu_weight("ROCm1".into(), 3);

    // The drag handle: promote CUDA0 to the head of the split. Position 0 is
    // llama.cpp's main_gpu, and a checkbox can only append, so this is the only
    // way to get there. The weight must ride along with its device.
    st.invoke_preset_gpu_move("CUDA0".into(), -1);
    assert_eq!(st.get_form().device.as_str(), "CUDA0,ROCm1");
    assert_eq!(
        st.get_form().tensor_split.as_str(),
        "1,3",
        "weights rode along"
    );
    assert_eq!(
        row_ids(),
        ["CUDA0", "ROCm1"],
        "the rows follow the new order"
    );
    st.invoke_preset_gpu_move("CUDA0".into(), 1); // back, so the INI below is 3,1
    assert_eq!(st.get_form().device.as_str(), "ROCm1,CUDA0");

    // The same table in BLOCKS: the unit the split is actually cut in. It only
    // appears once a model header has been read (`preset_split_positions` is 0
    // until then), so
    // stand one in: 33 blocks is Ornith-1.0-9B (32 trunk + 1 nextn), i.e. 34
    // positions once the output layer is counted.
    st.set_model_info_n_layer(33);
    st.invoke_preset_gpu_blocks("ROCm1".into(), 29);
    assert_eq!(
        st.get_form().tensor_split.as_str(),
        "29,5",
        "the counts ARE the weights: 3,1 projected to 26/8, and 29 takes 3 off the back"
    );
    assert_eq!(st.get_preset_split_positions(), 34);
    let rows = st.get_preset_gpu_rows();
    assert_eq!(
        (
            rows.row_data(0).expect("row 0").blocks,
            rows.row_data(1).expect("row 1").blocks
        ),
        (29, 5),
        "what was typed survives its own projection"
    );
    assert_eq!(
        rows.row_data(1).expect("row 1").blocks_label.as_str(),
        "blocks 29-32 + output",
        "the tail names the output layer: it is a position, not a block"
    );
    // …and it reached the widget. The thumb binds one-way to `row.blocks` and
    // discards that binding the moment it is dragged, so what keeps it honest is
    // the delegate being rebuilt on every commit: this is the assertion that
    // fails if a commit ever stops rebuilding the rows.
    assert_eq!(
        ElementHandle::find_by_accessible_label(app, "blocks-ROCm1")
            .next()
            .expect("no slider for ROCm1")
            .accessible_value()
            .unwrap_or_default()
            .parse::<f64>()
            .map(|v| v.round() as i64)
            .unwrap_or(-1),
        29,
        "the slider shows the committed count"
    );
    // Re-committing the same count (which leaving the field after Enter does) must
    // not churn the vector.
    st.invoke_preset_gpu_blocks("ROCm1".into(), 29);
    assert_eq!(st.get_form().tensor_split.as_str(), "29,5");
    // `row` bends the same table off blocks: the bytes follow row fractions
    // there, so the projection must retire even while the model is known, and
    // come straight back when the mode does.
    st.invoke_preset_split_mode_picked("row".into());
    assert_eq!(
        st.get_preset_split_positions(),
        0,
        "row mode has no block cut"
    );
    // `tensor` is a ratio too (weights and KV are cut along a tensor axis by
    // the same vector), so the block cut retires there as well, the combo
    // lands on its own entry rather than collapsing onto layer as it did while
    // the mode was not offered.
    st.invoke_preset_split_mode_picked("tensor".into());
    assert_eq!(st.get_form().split_mode.as_str(), "tensor");
    assert_eq!(st.get_preset_gpu_mode().as_str(), "tensor");
    assert_eq!(st.get_preset_split_mode_index(), 2);
    assert_eq!(
        st.get_preset_split_positions(),
        0,
        "tensor mode has no block cut"
    );
    // The two checked GPUs are a ROCm and a CUDA device, the pair that aborts
    // at load under every AllReduce but `none`: the strip says so until the
    // Server tab's SDK Tuning knob is on `none`, and with that set, flash
    // attention untouched and a supported arch it stays quiet.
    assert!(
        st.get_preset_split_mode_warning()
            .as_str()
            .contains("Tensor-split AllReduce"),
        "tensor across CUDA + ROCm must point at the AllReduce knob"
    );
    {
        let mut f = st.get_server_form();
        f.cuda_allreduce = "none".into();
        st.set_server_form(f);
    }
    assert_eq!(st.get_preset_split_mode_warning().as_str(), "");
    // ...and speaks up on the one precondition the form itself can break:
    // flash attention off is a load failure under tensor mode. Computed in
    // .slint straight off the form, so no Rust refresh sits between the edit
    // and the strip.
    {
        let mut f = st.get_form();
        f.flash_attn = "off".into();
        st.set_form(f);
        assert!(
            st.get_preset_split_mode_warning()
                .as_str()
                .contains("flash attention"),
            "tensor + flash-attn off must warn"
        );
        let mut f = st.get_form();
        f.flash_attn = "default".into();
        st.set_form(f);
        assert_eq!(st.get_preset_split_mode_warning().as_str(), "");
    }
    st.invoke_preset_split_mode_picked("default".into());
    assert_eq!(
        st.get_preset_split_mode_warning().as_str(),
        "",
        "the strip is about tensor mode only"
    );
    // The KV-cache strip is the opposite case: computed in Rust
    // (kv_cache::warning) and reached through AppWindow's `changed
    // kv_cache_inputs` handler, which runs only when change handlers are
    // processed. So this pins the wiring, the handler and the callback, not the
    // rules (kv_cache.rs tests those): iq4_nl on these CUDA/ROCm devices lands on
    // the CPU, and putting the types back clears the strip.
    {
        let original = st.get_form();
        let mut f = original.clone();
        f.cache_type_k = "iq4_nl".into();
        f.cache_type_v = "iq4_nl".into();
        st.set_form(f);
        itest::mock_elapsed_time(std::time::Duration::from_millis(16));
        assert!(
            st.get_preset_kv_cache_warning()
                .as_str()
                .contains("on the CPU"),
            "iq4_nl on CUDA/ROCm must warn, got {:?}",
            st.get_preset_kv_cache_warning()
        );
        st.set_form(original);
        itest::mock_elapsed_time(std::time::Duration::from_millis(16));
        assert_eq!(st.get_preset_kv_cache_warning().as_str(), "");
    }
    assert_eq!(
        st.get_preset_split_positions(),
        34,
        "layer gets the blocks back"
    );
    // Back to a plain ratio for the INI assertions below.
    st.invoke_preset_gpu_even();
    st.invoke_preset_gpu_weight("ROCm1".into(), 3);
    assert_eq!(st.get_form().tensor_split.as_str(), "3,1");
    // A model change has to re-project the table, not just the Model-info box:
    // the block column counts against THIS model's block_count, so leaving it on
    // the previous one would have every row quoting a budget that no longer
    // exists. Here the fixture GGUF is unreadable, so the count drops to 0 and
    // the column falls back to ratios: the same path a real switch takes.
    st.invoke_model_changed();
    assert_eq!(
        st.get_preset_split_positions(),
        0,
        "an unreadable model takes the block column back out"
    );
    assert_eq!(
        st.get_preset_gpu_rows().row_data(0).expect("row 0").blocks,
        0
    );
    assert_eq!(
        st.get_form().tensor_split.as_str(),
        "3,1",
        "…without touching the ratio it is projecting"
    );

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
    // model from disk, so the checkbox must follow: a set_row_data
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

    // ── Tensor placement: the whole --override-tensor funnel ─────────────
    // The table is the ONLY writer of `override_tensor`, and the value's grammar
    // is unforgiving: llama.cpp splits it on `,` and each rule at its FIRST `=`
    // BEFORE parsing, so a rule that survives that without a device is a throw at
    // load. This drives the real callbacks and asserts the string that reaches the
    // INI, comma and equals signs included.
    let ot_rows = || st.get_preset_tensor_rows();
    assert_eq!(
        ot_rows().row_count(),
        0,
        "a preset starts with no overrides"
    );
    assert!(
        st.get_preset_tensor_summary().starts_with("(no overrides"),
        "summary: {}",
        st.get_preset_tensor_summary()
    );

    st.invoke_preset_tensor_add();
    let row = ot_rows().row_data(0).expect("row 0");
    assert_eq!(row.kind_index, 0, "Add seeds the Embedding table…");
    assert_eq!(row.device.as_str(), "CUDA0", "…on the first probed GPU");
    assert!(!row.custom, "a canned kind hides the regex field");
    assert!(row.detected && row.problem.is_empty());
    assert_eq!(
        st.get_form().override_tensor.as_str(),
        r"^token_embd\.weight=CUDA0",
        "the one rule that pays for this table: the embedding table off pinned host RAM"
    );
    assert!(st.get_preset_tensor_warning().is_empty());

    // The CPU is a first-class target here, and the contrast with the GPU table
    // above (which filters it out, since --device won't take it) is the point.
    let dev_values: Vec<String> = {
        let v = st.get_preset_tensor_dev_values();
        (0..v.row_count())
            .map(|i| v.row_data(i).expect("value").to_string())
            .collect()
    };
    assert_eq!(dev_values, ["CUDA0", "ROCm1", "CPU"]);
    assert!(
        !row_ids().contains(&"CPU".to_string()),
        "…while the GPU distribution table must NOT offer it"
    );

    // A second rule: the experts regex, sent to the CPU (i.e. exactly --cpu-moe).
    st.invoke_preset_tensor_add();
    st.invoke_preset_tensor_kind(1, "exps".into());
    st.invoke_preset_tensor_device(1, "CPU".into());
    assert_eq!(
        st.get_form().override_tensor.as_str(),
        r"^token_embd\.weight=CUDA0,\.ffn_(up|down|gate|gate_up)_(ch|)exps=CPU",
        "two rules, comma-joined: `,` and `=` ARE the grammar"
    );

    // Custom clears the regex and reveals the row's text field (a row's kind is
    // DERIVED from its pattern, so it could not both stay `\.ffn_…_exps` and read
    // as Custom). The regex it then takes is SANITIZED: a `{1,2}` quantifier would
    // tear its own rule in half at llama.cpp's comma split.
    st.invoke_preset_tensor_kind(1, "custom".into());
    let row = ot_rows().row_data(1).expect("row 1");
    assert!(row.custom, "Custom must reveal the regex field");
    assert_eq!(row.device.as_str(), "CPU", "…while the device stays put");
    st.invoke_preset_tensor_pattern(1, r"blk\.{1,2}\.attn=x".into());
    assert_eq!(
        st.get_form().override_tensor.as_str(),
        r"^token_embd\.weight=CUDA0,blk\.{12}\.attnx=CPU",
        "the two separators are stripped, so the value is still TWO rules"
    );

    st.invoke_save_preset();
    assert!(!st.get_status_is_error(), "{}", st.get_status_text());
    let ini = std::fs::read_to_string(crate::paths::presets_ini()).expect("presets.ini");
    assert!(
        ini.contains(r"override-tensor = ^token_embd\.weight=CUDA0,blk\.{12}\.attnx=CPU"),
        "the table's rules must reach the INI:\n{ini}"
    );

    // The rebuild half of the one-way `for`-row binding contract, as in the GPU
    // phase, but for the row's LineEdit, whose handler deliberately does NOT
    // rebuild (that would recreate the field mid-keystroke and drop the caret).
    // The widget's own write breaks its binding permanently; only Revert's full
    // rebuild can bring it back, and it must.
    let ot_pattern = || {
        ElementHandle::find_by_accessible_label(app, "ot-pattern-1")
            .next()
            .expect("the Custom row's regex field")
    };
    assert_eq!(
        ot_pattern().accessible_value().unwrap_or_default(),
        r"blk\.{12}\.attnx"
    );
    ot_pattern().set_accessible_value(r"attn_norm"); // the edit that breaks the binding
    assert_eq!(
        st.get_form().override_tensor.as_str(),
        r"^token_embd\.weight=CUDA0,attn_norm=CPU",
        "a keystroke in the regex field must reach the form"
    );
    st.invoke_revert_preset();
    assert_eq!(
        ot_pattern().accessible_value().unwrap_or_default(),
        r"blk\.{12}\.attnx",
        "a rebuild must reach the edited regex field"
    );

    // A rule with NO device is what a hand-edited INI leaves behind, and llama.cpp
    // throws on it during arg parsing; the model just never loads. So write it the
    // way it really arrives (into the file, then reload) and pin both halves of the
    // answer: the table explains it, and the save refuses to write it back.
    let ini = std::fs::read_to_string(crate::paths::presets_ini()).expect("presets.ini");
    let hand_edited = ini.replace(
        r"override-tensor = ^token_embd\.weight=CUDA0,blk\.{12}\.attnx=CPU",
        r"override-tensor = token_embd\.weight",
    );
    assert_ne!(hand_edited, ini, "the line to hand-edit must be there");
    std::fs::write(crate::paths::presets_ini(), &hand_edited).expect("hand-edit presets.ini");
    st.invoke_revert_preset(); // reloads from disk → rebuilds the table
    assert!(
        st.get_preset_tensor_warning().contains("token_embd"),
        "warning: {}",
        st.get_preset_tensor_warning()
    );
    assert_eq!(
        ot_rows().row_data(0).expect("row 0").problem.as_str(),
        "pick a device"
    );
    st.invoke_save_preset();
    assert!(
        st.get_status_is_error(),
        "a rule with no device must be refused by presets::save"
    );

    // Repair it through the table (which is the cure the warning implies) and
    // leave the preset saved and clean for the phases below.
    st.invoke_preset_tensor_device(0, "CUDA0".into());
    assert!(st.get_preset_tensor_warning().is_empty());
    st.invoke_save_preset();
    assert!(!st.get_status_is_error(), "{}", st.get_status_text());

    // ── Bind to: one checkbox per address, `server_form.hostname` IS the list ──
    // The rows are one-way; each click must rewrite the comma-separated list AND
    // rebuild the model, or checking 0.0.0.0 would leave the other rows showing
    // the ticks it just cleared. A stale saved address keeps its (checked) row.
    let mut f = st.get_server_form();
    f.hostname = "localhost,10.9.9.9".into();
    st.set_server_form(f);
    crate::gui::populate_bind_options(app, "localhost,10.9.9.9");
    // Pages are conditional on current_tab: the checkboxes exist only on 0.
    let tab_before = st.get_current_tab();
    st.set_current_tab(0);
    let bind = |value: &str| {
        ElementHandle::find_by_accessible_label(app, format!("bind-{value}").as_str())
            .next()
            .unwrap_or_else(|| panic!("no bind checkbox for {value}"))
    };
    let bind_row = |value: &str| {
        let rows = st.get_bind_rows();
        (0..rows.row_count())
            .filter_map(|i| rows.row_data(i))
            .find(|r| r.value.as_str() == value)
            .unwrap_or_else(|| panic!("no bind row for {value}"))
    };
    assert!(bind_row("10.9.9.9").checked && bind_row("localhost").checked);
    bind("0.0.0.0").invoke_accessible_default_action();
    assert_eq!(st.get_server_form().hostname.as_str(), "0.0.0.0");
    assert!(
        !bind_row("localhost").checked && !bind_row("localhost").enabled,
        "0.0.0.0 is exclusive: the other rows are cleared and locked"
    );
    bind("0.0.0.0").invoke_accessible_default_action();
    assert_eq!(
        st.get_server_form().hostname.as_str(),
        "localhost",
        "unchecking the last address falls back to the default"
    );
    assert!(bind_row("localhost").checked && bind_row("localhost").enabled);
    let cfg = crate::server_form::form_to_config(&st.get_server_form());
    assert_eq!(cfg.client_host(), "localhost");
    st.set_current_tab(tab_before);

    // ── Dirty guard: navigation on a dirty form asks before discarding ───
    let mut form = st.get_form();
    form.ctx_size = "12345".into();
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
        st.get_form().ctx_size.as_str(),
        "12345",
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

    // ── The on/off switch: the preset MOVES between two files, the form stays ──
    // Switching a preset off moves its section, verbatim, from presets.ini (the
    // file llama-server reads) to presets.ini.disabled. It is not a navigation:
    // an unsaved edit must survive it, and the Save that follows must land in
    // whichever file now holds the preset, or saving would switch it back on.
    let presets_ini = crate::paths::presets_ini();
    let parked_ini = crate::paths::presets_disabled_ini();
    let read = |p: &std::path::Path| std::fs::read_to_string(p).unwrap_or_default();
    let switch = |id: &str| {
        ElementHandle::find_by_accessible_label(app, format!("enabled-{id}").as_str())
            .next()
            .unwrap_or_else(|| panic!("no on/off switch for {id}"))
    };
    let listed_enabled = |id: &str| {
        st.get_presets()
            .iter()
            .find(|p| p.id.as_str() == id)
            .unwrap_or_else(|| panic!("{id} is not listed"))
            .enabled
    };
    let mut form = st.get_form();
    assert_eq!(
        form.id.as_str(),
        "e2e",
        "the clone is still the one selected"
    );
    form.ctx_size = "65536".into();
    form.ctx_size_default = false;
    st.set_form(form);
    let dirty_form = st.get_form();
    assert!(st.get_preset_dirty());
    assert_eq!(
        switch("e2e").accessible_checked(),
        Some(true),
        "presets start on"
    );
    let section_before = crate::ini::section_text(&presets_ini, "e2e").expect("[e2e] on disk");

    switch("e2e").invoke_accessible_default_action(); // the user's click: off
    itest::mock_elapsed_time(std::time::Duration::from_millis(1));
    assert!(!st.get_status_is_error(), "{}", st.get_status_text());
    assert!(
        !read(&presets_ini).contains("[e2e]"),
        "llama-server's file must no longer carry it"
    );
    assert_eq!(
        crate::ini::section_text(&parked_ini, "e2e")
            .expect("[e2e] parked")
            .trim_end(),
        section_before.trim_end(),
        "the section moves verbatim, it is not re-rendered"
    );
    assert!(!listed_enabled("e2e"), "the row says off");
    assert_eq!(
        switch("e2e").accessible_checked(),
        Some(false),
        "the rebuilt row's switch shows it"
    );
    assert_eq!(
        st.get_form(),
        dirty_form,
        "the unsaved edit survives the switch"
    );
    assert!(st.get_preset_dirty());
    let sel = st.get_selected_preset_index();
    assert_eq!(
        st.get_presets()
            .iter()
            .find(|p| p.orig_index == sel)
            .map(|p| p.id.to_string()),
        Some("e2e".to_string()),
        "the selection follows the preset to its new place in the list"
    );
    assert!(
        st.get_bench_presets()
            .iter()
            .all(|r| r.id.as_str() != "e2e"),
        "a preset llama-server does not offer cannot be benchmarked"
    );

    // Saving a switched-off preset edits it where it is parked.
    st.invoke_save_preset();
    assert!(!st.get_status_is_error(), "{}", st.get_status_text());
    assert!(
        read(&parked_ini).contains("ctx-size = 65536") && !read(&presets_ini).contains("[e2e]"),
        "the save must not switch the preset back on"
    );
    assert!(!listed_enabled("e2e"));

    // Ids stay unique across BOTH files: New… on the same model must not take the
    // parked id (switching it back on would then overwrite one of the two), and a
    // rename onto it is refused. (The server-table phase above saved a server.ini
    // without a ModelsDir, so point the picker's scan back at the temp tree.)
    let mut cfg = crate::server_cfg::load();
    cfg.models_dir = Some(dir.path().to_string_lossy().into_owned());
    crate::server_cfg::save(&cfg).expect("save server.ini");
    assert!(
        !st.get_preset_dirty(),
        "the save above re-baselined the form"
    );
    st.invoke_new_preset();
    st.set_dialog_model_index(0);
    st.invoke_pick_new_empty();
    assert!(!st.get_status_is_error(), "{}", st.get_status_text());
    assert_eq!(
        st.get_form().id.as_str(),
        "e2e-3",
        "e2e (off) and e2e-2 are taken"
    );
    st.invoke_rename_preset("e2e-3".into(), "e2e".into());
    assert!(
        st.get_status_is_error() && st.get_status_text().contains("already exists"),
        "a rename onto a switched-off preset must be refused: {}",
        st.get_status_text()
    );
    st.invoke_delete_preset("e2e-3".into());

    // And back on: the section returns to presets.ini, edit included.
    switch("e2e").invoke_accessible_default_action();
    itest::mock_elapsed_time(std::time::Duration::from_millis(1));
    assert!(!st.get_status_is_error(), "{}", st.get_status_text());
    assert!(read(&presets_ini).contains("ctx-size = 65536"));
    assert!(!read(&parked_ini).contains("[e2e]"));
    assert!(listed_enabled("e2e"));
    assert!(st
        .get_bench_presets()
        .iter()
        .any(|r| r.id.as_str() == "e2e"));
    // The rebuild half of the switch's one-way binding: that click broke its
    // delegate's binding for good, so a change made ELSEWHERE (the CLI's path,
    // then a reload) reaches the widget only if the row is recreated.
    crate::presets::set_enabled("e2e", false).expect("switch off behind the UI");
    st.invoke_reload_all();
    assert_eq!(
        switch("e2e").accessible_checked(),
        Some(false),
        "a rebuild must reach the clicked switch"
    );
    crate::presets::set_enabled("e2e", true).expect("and back on");
    st.invoke_reload_all();
    assert_eq!(switch("e2e").accessible_checked(), Some(true));

    // ── Integrations: OpenCode lists the ENABLED presets, and follows them ──
    // Every preset change above already ran the opencode.json follow-up, and it
    // must not have created anything: only Save adds the provider. Under the
    // redirect even the parent dir (<tmp>\opencode\) is missing, the exact
    // "OpenCode never ran here" shape the v1.2.13 create_dir_all fix is for.
    st.set_current_tab(3); // Integrations (2 is Benchmark)
    let opencode = crate::paths::opencode_user_config();
    assert!(
        !opencode.exists(),
        "a preset change must never create opencode.json on its own"
    );
    assert!(!st.get_integration_provider_active());
    assert!(
        st.get_integration_status().contains("Save adds one"),
        "{}",
        st.get_integration_status()
    );
    let enabled_ids = || {
        let mut v: Vec<String> = crate::presets::load_all()
            .into_iter()
            .map(|p| p.id)
            .collect();
        v.sort();
        v
    };
    let opencode_ids = || {
        let mut v = crate::integrations::opencode_model_ids();
        v.sort();
        v
    };
    st.invoke_save_integrations();
    assert!(
        !st.get_status_is_error(),
        "Integrations Save failed (missing opencode dir?): {}",
        st.get_status_text()
    );
    assert!(
        opencode.exists(),
        "save must create opencode.json and its dir"
    );
    assert_eq!(
        opencode_ids(),
        enabled_ids(),
        "one model per enabled preset"
    );
    assert!(st.get_integration_provider_active() && st.get_integration_in_sync());

    // Now that the provider exists, a switch follows through on its own.
    st.set_current_tab(1);
    switch("e2e-2").invoke_accessible_default_action(); // off
    itest::mock_elapsed_time(std::time::Duration::from_millis(1));
    assert!(!st.get_status_is_error(), "{}", st.get_status_text());
    assert!(
        !crate::integrations::opencode_model_ids().contains(&"e2e-2".to_string()),
        "switched off in the Models tab = gone from OpenCode"
    );
    assert_eq!(opencode_ids(), enabled_ids());
    assert!(st.get_integration_in_sync());
    switch("e2e-2").invoke_accessible_default_action(); // back on
    itest::mock_elapsed_time(std::time::Duration::from_millis(1));
    assert_eq!(opencode_ids(), enabled_ids(), "…and back");

    // A file that drifted (written by a version with the old checkbox list, or
    // edited by hand) is NAMED by a refresh, never rewritten by it; Save repairs.
    let mut v: serde_json::Value =
        serde_json::from_str(&read(&opencode)).expect("opencode.json is JSON");
    v["provider"]["llama.cpp"]["models"]
        .as_object_mut()
        .expect("models object")
        .remove("e2e-2");
    std::fs::write(&opencode, serde_json::to_string_pretty(&v).unwrap()).expect("hand-edit");
    st.invoke_reload_all();
    assert!(!st.get_integration_in_sync());
    assert!(
        st.get_integration_status()
            .contains("enabled but not listed: e2e-2"),
        "{}",
        st.get_integration_status()
    );
    assert!(
        !crate::integrations::opencode_model_ids().contains(&"e2e-2".to_string()),
        "a refresh only reports"
    );
    st.invoke_save_integrations();
    assert!(st.get_integration_in_sync());
    assert_eq!(opencode_ids(), enabled_ids());

    // ── Benchmark: the selection is an ORDERED list, and order is meaning ───
    // Position 1 is the baseline every ratio in the results table divides by,
    // so this pins the three things that cannot be seen from the row model:
    // checking APPENDS, unchecking CLOSES THE GAP (rather than leaving a hole
    // or re-sorting), and the baseline follows position 1. It also pins the
    // preview funnel, whose whole point is that it is built by the code that
    // builds the real command: a preview that stopped tracking the mode would
    // show a llama-bench line for a run that sends HTTP, or the reverse.
    for id in ["bench-a", "bench-b"] {
        crate::presets::save(&crate::presets::Preset {
            id: id.into(),
            model: format!(r"E:\models\{id}.gguf"),
            ..Default::default()
        })
        .expect("seed a benchmark preset");
    }
    st.invoke_reload_all(); // the seam's cut-down F5: re-read presets.ini
    let bench_row = |id: &str| {
        st.get_bench_presets()
            .iter()
            .find(|r| r.id.as_str() == id)
            .unwrap_or_else(|| panic!("no benchmark row for {id}"))
    };
    assert_eq!(bench_row("bench-a").order, 0, "nothing is selected yet");

    st.invoke_bench_toggle_preset("bench-b".into());
    st.invoke_bench_toggle_preset("bench-a".into());
    assert_eq!(
        (bench_row("bench-b").order, bench_row("bench-a").order),
        (1, 2),
        "checking appends, so the FIRST one checked is the baseline"
    );
    assert_eq!(st.get_bench_selected_count(), 2);
    assert_eq!(st.get_bench_baseline(), "bench-b");
    assert!(
        st.get_bench_preview().contains("bench-b"),
        "the live preview must name the preset it will request: {}",
        st.get_bench_preview()
    );

    st.invoke_bench_toggle_preset("bench-b".into());
    assert_eq!(
        (bench_row("bench-b").order, bench_row("bench-a").order),
        (0, 1),
        "unchecking must close the gap, not leave the survivor at position 2"
    );
    assert_eq!(st.get_bench_baseline(), "bench-a");

    // The mode is what decides which engine runs, and the preview and the
    // caveats must both follow it: llama-bench takes no prompt and cannot load
    // a drafter, which is exactly what the caveat block exists to say.
    assert!(st.get_bench_is_live(), "live is the default mode");
    assert!(
        st.get_bench_caveats().contains("cache_prompt"),
        "the live caveats must name the cold-prefill guarantee"
    );
    st.invoke_bench_mode_picked("synthetic".into());
    assert!(!st.get_bench_is_live());
    assert!(
        st.get_bench_caveats().contains("No speculative decoding"),
        "the synthetic caveats must name the drafter it cannot load"
    );
    // No llama-server is running here, so a synthetic run needs no stop and
    // reaches the engine, which then reports the missing exe rather than
    // silently doing nothing. Either way the tab must not be left "running".
    st.invoke_bench_mode_picked("live".into());

    // A run with nothing selected is refused by the shared validator rather
    // than spawning a worker that would find out on its own.
    st.invoke_bench_toggle_preset("bench-a".into());
    assert_eq!(st.get_bench_selected_count(), 0);
    st.invoke_bench_run();
    assert!(
        st.get_status_is_error() && st.get_status_text().contains("at least one preset"),
        "an empty selection must be refused: {}",
        st.get_status_text()
    );
    assert!(
        !st.get_bench_running(),
        "a refused run must not arm the tab"
    );

    // ── Benchmark: the live prompt is a FILE the tab only points at ─────────
    // Two things no unit test can reach, because both are about the tab
    // meeting the real disk: the list holds every shipped prompt (installed,
    // read in place) plus the configured one of the user's, ticked; and an
    // edit made OUTSIDE this process is picked up. That last one is the whole
    // design: Edit hands the file to the system's text editor, so if the tab
    // only ever read the file once, every prompt change after launch would be
    // silently ignored.
    let row = |i: usize| st.get_bench_prompts().row_data(i).expect("a prompt row");
    let shipped = crate::bench::shipped_prompts();
    assert!(shipped.len() >= 3, "43k, 110k and the code prompt are shipped");
    assert_eq!(st.get_bench_prompts().row_count(), shipped.len() + 1);
    for (i, p) in shipped.iter().enumerate() {
        assert_eq!(row(i).path.as_str(), p.to_string_lossy());
        assert!(row(i).shipped && !row(i).on, "a shipped prompt, not the configured one");
    }
    let mine = shipped.len();
    assert_eq!(row(mine).path.as_str(), prompt_path.to_string_lossy());
    assert!(row(mine).on && !row(mine).shipped);
    let seeded_info = row(mine).info.to_string();
    assert!(
        seeded_info.contains(&crate::sha256::short(b"Summarize the release notes.\n")),
        "the readout carries the digest of what is on disk: {seeded_info}"
    );

    // Edited behind the app's back, exactly as Notepad would, CRLF and a BOM
    // included. The readout must follow, and the digest must be the digest of
    // the NORMALIZED text: the same prompt saved LF must hash the same, or
    // "these two runs used the same prompt" stops being answerable.
    std::fs::write(&prompt_path, "\u{feff}Explain flash attention.\r\n")
        .expect("edit the prompt file out of process");
    st.invoke_reload_all();
    let edited_info = row(mine).info.to_string();
    assert_ne!(edited_info, seeded_info, "an external edit must be noticed");
    assert!(
        edited_info.contains(&crate::sha256::short(b"Explain flash attention.\n")),
        "the digest is of the text as SENT, BOM stripped and CRLF folded: {edited_info}"
    );
    st.invoke_bench_toggle_preset("bench-a".into()); // the preview needs one
    assert!(
        st.get_bench_preview().contains("Explain flash attention."),
        "the request preview follows the file too: {}",
        st.get_bench_preview()
    );
    // Ticking a shipped prompt makes the run one step per prompt, and a
    // conversation prompt previews as one, with its tools.
    let code = shipped
        .iter()
        .position(|p| p.extension().is_some_and(|e| e == "json"))
        .expect("the code prompt is shipped");
    st.invoke_bench_prompt_toggle(code as i32);
    let code_name = shipped[code].file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        st.get_bench_preview().contains(&format!("{code_name}, my-prompt.txt"))
            || st.get_bench_preview().contains(&format!("my-prompt.txt, {code_name}")),
        "two ticked prompts run in turn: {}",
        st.get_bench_preview()
    );
    st.invoke_bench_prompt_toggle(code as i32);

    // An empty prompt file is refused BY NAME: with the text out in a file,
    // "the live mode needs a prompt" does not say where to put one.
    std::fs::write(&prompt_path, "   \r\n").expect("blank the prompt file");
    st.invoke_reload_all();
    st.invoke_bench_run();
    assert!(
        st.get_status_is_error() && st.get_status_text().contains("my-prompt.txt"),
        "a blank prompt file must be reported by name: {}",
        st.get_status_text()
    );
    assert!(!st.get_bench_running(), "and must not arm the tab");
    st.invoke_bench_toggle_preset("bench-a".into());
    std::fs::write(&prompt_path, "Summarize the release notes.\n").expect("restore the prompt file");
    st.invoke_reload_all();

    // ── Benchmark: the Database field is settings.ini `BenchDb` ──────────────
    // Empty is the default and must STAY empty (seeding the resolved path would
    // pin today's data folder into settings.ini); Enter saves a typed path, a
    // missing file is announced rather than an error, and a path the INI would
    // truncate is refused by name instead of saved.
    assert_eq!(st.get_bench_db().as_str(), "", "unset reads as the default");
    assert!(
        st.get_bench_db_info()
            .contains(&*crate::bench::db::default_path().to_string_lossy()),
        "the readout resolves the default: {}",
        st.get_bench_db_info()
    );
    let other_db = crate::bench::db::default_path().with_file_name("other.db");
    st.set_bench_db(other_db.to_string_lossy().as_ref().into());
    st.invoke_bench_db_accepted();
    assert_eq!(
        crate::settings::load().bench_db_path(),
        other_db,
        "Enter saves the typed database"
    );
    assert!(
        st.get_bench_db_info().contains("not there yet"),
        "a new database is announced, not refused: {}",
        st.get_bench_db_info()
    );
    st.set_bench_db(r"D:\bench #2.db".into());
    st.invoke_bench_db_accepted();
    assert!(
        st.get_status_is_error() && st.get_status_text().contains("BenchDb"),
        "a '#' in the path is refused by name: {}",
        st.get_status_text()
    );
    assert_eq!(
        crate::settings::load().bench_db_path(),
        other_db,
        "and not saved"
    );
    st.set_bench_db("".into());
    st.invoke_bench_db_accepted();
    assert!(
        crate::settings::load().bench_db.is_empty(),
        "cleared = default again"
    );

    // ── CLI: `preset delete <typo>` errors instead of a false "Removed" ─────
    // The v1.2.13 lookup-before-delete guard; the redirect keeps cli::run on
    // the temp tree. (`load_all` resolves real paths, so this can't be an
    // inline unit test: only the redirect harness reaches it.)
    use crate::cli::{Cli, Command, PresetCmd};
    assert!(
        crate::cli::run(Cli {
            command: Command::Preset(PresetCmd::Delete {
                id: "no-such-preset".into(),
                delete_files: Vec::new(),
                even_if_shared: false,
            }),
        })
        .is_err(),
        "deleting a nonexistent preset must error, not report success"
    );

    // ── CLI: `preset disable|enable` is the same move the switch makes ──────
    // Case-insensitive lookup (like show/delete), acting on the STORED id, and
    // the OpenCode provider configured above follows it too.
    let preset_cmd = |cmd: PresetCmd| {
        crate::cli::run(Cli {
            command: Command::Preset(cmd),
        })
    };
    preset_cmd(PresetCmd::Disable { id: "E2E".into() }).expect("preset disable");
    assert!(crate::presets::load_all().iter().all(|p| p.id != "e2e"));
    assert!(crate::presets::load_listed()
        .iter()
        .any(|l| l.preset.id == "e2e" && !l.enabled));
    assert!(
        !crate::integrations::opencode_model_ids().contains(&"e2e".to_string()),
        "the CLI makes the same opencode.json follow-up as the GUI"
    );
    preset_cmd(PresetCmd::Disable { id: "e2e".into() }).expect("already off is not an error");
    preset_cmd(PresetCmd::Enable { id: "e2e".into() }).expect("preset enable");
    assert!(crate::presets::load_all().iter().any(|p| p.id == "e2e"));
    assert!(crate::integrations::opencode_model_ids().contains(&"e2e".to_string()));
    assert!(
        preset_cmd(PresetCmd::Enable {
            id: "no-such-preset".into()
        })
        .is_err(),
        "an unknown id must error"
    );

    // ── New preset from Hugging Face: the dialog wiring that needs no network ──
    // gui/hf_dialog.rs. The search, the listing and the transfer need Hugging
    // Face and a llama-server, so this drives everything around them (under
    // cfg(test) the request workers and the child are not even spawned): the
    // modal gate, the one field that searches or lists, the refusals that must
    // hold before any request leaves, the search rows handing over to the
    // listing and Back, and the hand-over to the background downloads
    // (gui/downloads.rs): the modal closing at Download, the Downloads
    // window's rows (progress, cancel, resume, failure, a second download of
    // the same repository refused), the preset created without touching a
    // dirty editor, Show preset through the discard guard, and Quit asking
    // first.
    // The New dialog's "From Hugging Face" button hides the picker, then opens
    // this one: mirror both (an earlier phase may have left the picker up).
    st.set_show_new_kind_picker(false);
    st.invoke_hf_open();
    assert!(st.get_show_hf_dialog() && st.get_modal_open(), "the dialog is a modal");
    assert_eq!(st.get_hf_phase(), 0);
    assert!(st.get_hf_show_results() && !st.get_hf_has_results(), "an empty results panel first");
    assert!(st.get_hf_token().is_empty(), "the token field is never prefilled");
    assert_eq!(st.invoke_hf_input_problem("org/repo:Q4_K_M".into()).as_str(), "");
    assert_eq!(st.invoke_hf_input_problem("".into()).as_str(), "");
    assert_eq!(st.invoke_hf_input_problem("qwen3 8b".into()).as_str(), "", "words are a search");
    assert!(!st.invoke_hf_input_problem("a/b/c".into()).is_empty());
    assert!(st.invoke_hf_input_is_query("qwen3 8b".into()));
    assert!(!st.invoke_hf_input_is_query("org/repo".into()), "a spec lists without searching");
    // A token that cannot be one is refused IN the dialog before any request
    // (a listing or a search alike), and echoed nowhere.
    let status_before = st.get_status_text();
    st.set_hf_token("hf_secret with space".into());
    for input in ["org/repo", "qwen3 8b"] {
        st.set_hf_input(input.into());
        st.set_hf_error("".into());
        st.invoke_hf_submit();
        assert_eq!(st.get_hf_phase(), 0, "a refused token starts no request ({input})");
        assert!(!st.get_hf_error().is_empty(), "the refusal shows in the dialog");
        for text in [st.get_hf_error(), st.get_hf_note(), st.get_status_text()] {
            assert!(!text.contains("hf_secret"), "the token leaked into {text:?}");
        }
    }
    assert_eq!(st.get_status_text(), status_before);
    st.set_hf_token("".into());
    st.invoke_hf_download();
    assert_eq!(st.get_hf_phase(), 0, "nothing to download before a listing");

    // Search: words go to the search, one request at a time.
    st.set_hf_input("qwen3 8b".into());
    st.invoke_hf_submit();
    assert_eq!(st.get_hf_phase(), 1, "searching");
    assert!(st.get_hf_busy() && st.get_hf_error().is_empty());
    st.invoke_hf_pick_result(0);
    assert_eq!(st.get_hf_phase(), 1, "no row to pick while searching");
    let hit = |repo: &str, downloads: u64, quants: &[&str]| crate::hf::SearchHit {
        repo: repo.into(),
        downloads,
        gated: false,
        private: false,
        quants: quants.iter().map(|q| q.to_string()).collect(),
        mmproj: true,
        drafters: Vec::new(),
        kind: crate::hf::ModelKind::Unknown,
        params: None,
    };
    crate::gui::land_search_for_tests(
        app,
        "qwen3 8b",
        Ok(vec![
            crate::hf::SearchHit {
                kind: crate::hf::ModelKind::Dense,
                params: Some(8_190_735_360),
                ..hit("Qwen/Qwen3-8B-GGUF", 644_587, &["Q4_K_M", "Q8_0"])
            },
            hit("unsloth/Qwen3-8B-GGUF", 131_220, &["Q2_K", "Q4_K_M", "BF16"]),
        ]),
    );
    assert_eq!(st.get_hf_phase(), 2, "searched");
    assert!(st.get_hf_has_results() && st.get_hf_show_results());
    assert!(st.get_hf_results_title().contains("\"qwen3 8b\""), "{}", st.get_hf_results_title());
    let labels: Vec<String> = st.get_hf_result_labels().iter().map(|s| s.to_string()).collect();
    assert_eq!(
        labels,
        ["Qwen/Qwen3-8B-GGUF  ·  dense 8.2B  ·  644.6k downloads", "unsloth/Qwen3-8B-GGUF  ·  131.2k downloads"],
        "the model marker only where the search knows it"
    );
    assert_eq!(st.get_hf_result_details().row_data(1).unwrap().as_str(), "Q2_K, Q4_K_M, BF16  ·  with mmproj");
    // Picking a row lists that repository: its id in the field, results kept.
    st.invoke_hf_pick_result(1);
    assert_eq!(st.get_hf_phase(), 3, "listing the picked repository");
    assert_eq!(st.get_hf_input().as_str(), "unsloth/Qwen3-8B-GGUF");
    assert_eq!(st.get_hf_result_index(), 1);
    assert!(st.get_hf_has_results() && !st.get_hf_show_results(), "Back is offered, the candidates show");
    // Back drops the listing in flight and restores the query.
    st.invoke_hf_back_to_results();
    assert_eq!(st.get_hf_phase(), 2);
    assert_eq!(st.get_hf_input().as_str(), "qwen3 8b");
    assert_eq!(st.get_hf_result_labels().row_count(), 2, "the rows survive Back");
    // A failed listing of a picked row falls back to the results, error shown.
    st.invoke_hf_pick_result(0);
    crate::gui::land_listing_for_tests(app, Err(crate::hf::HfError::NotFoundOrPrivate { repo: "Qwen/Qwen3-8B-GGUF".into() }));
    assert_eq!(st.get_hf_phase(), 2);
    assert!(st.get_hf_error().contains("Access token field"), "{}", st.get_hf_error());
    assert_eq!(st.get_hf_result_labels().row_count(), 2);
    // A listing that lands shows the candidates, the default one picked.
    st.invoke_hf_pick_result(0);
    assert!(st.get_hf_error().is_empty(), "a new request clears the old error");
    let file = |path: &str, size: u64| crate::hf::RepoFile { path: path.into(), size, oid: String::new() };
    let candidate = |tag: &str| crate::hf::Candidate {
        tag: tag.into(),
        shards: vec![file(&format!("Qwen3-8B-{tag}.gguf"), 5 << 30)],
        size: 5 << 30,
        mmproj: None,
        mtp: None,
        dflash: None,
        dspark: None,
    };
    // Measured against no GPU (the machine's own probe must not matter here).
    crate::gui::land_listing_with_budget_for_tests(
        app,
        Ok(crate::hf::Listing {
            repo: "Qwen/Qwen3-8B-GGUF".into(),
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            project_url: "https://huggingface.co/Qwen/Qwen3-8B-GGUF".into(),
            candidates: vec![candidate("Q8_0"), candidate("Q4_K_M")],
            default: 1,
        }),
        None,
    );
    assert_eq!(st.get_hf_phase(), 4, "listed");
    assert_eq!(st.get_hf_candidate_labels().row_count(), 2);
    assert_eq!(st.get_hf_candidate_index(), 1, "the default quantization starts picked");
    assert!(st.get_hf_selection_summary().contains("Qwen3-8B-Q4_K_M.gguf"));
    assert!(st.get_hf_has_results(), "Back is still offered from the listing");
    // No GPU probed (none under test): every row says the estimate is
    // unavailable, no basis line, no recommendation.
    let details: Vec<String> = st.get_hf_candidate_details().iter().map(|s| s.to_string()).collect();
    assert_eq!(details, vec![format!("estimate unavailable: {}", crate::hf_fit::NO_GPU); 2]);
    assert!(st.get_hf_estimate_basis().is_empty());

    // With a GPU: rows read "estimating" until their header lands; once all
    // have, the recommended one is marked and the pick moves to it (the
    // user had not chosen), here Q8_0 over the default Q4_K_M.
    let listing = || crate::hf::Listing {
        repo: "Qwen/Qwen3-8B-GGUF".into(),
        commit: "0123456789abcdef0123456789abcdef01234567".into(),
        project_url: "https://huggingface.co/Qwen/Qwen3-8B-GGUF".into(),
        candidates: vec![candidate("Q8_0"), candidate("Q4_K_M")],
        default: 1,
    };
    let budget = || crate::hf_fit::Budget {
        gpus: vec![crate::hf_fit::Gpu { id: "CUDA0".into(), name: "Test GPU".into(), total_mib: 24 << 10 }],
        ram: Some(28 << 30),
        ram_total: Some(32 << 30),
    };
    let shape = |gib: u64| {
        std::sync::Arc::new(crate::hf_fit::Shape {
            arch: "qwen3".into(),
            moe: false,
            total: gib << 30,
            token_embd: 0,
            globals: 0,
            layers: vec![crate::hf_fit::Layer { experts: 0, other: (gib << 30) / 4 }; 4],
            kv: 1 << 30,
            ctx: 32768,
        })
    };
    st.invoke_hf_pick_result(0);
    crate::gui::land_listing_with_budget_for_tests(app, Ok(listing()), Some(budget()));
    assert_eq!(st.get_hf_phase(), 4);
    assert!(st.get_hf_estimate_basis().starts_with("Estimated for Test GPU"), "{}", st.get_hf_estimate_basis());
    assert!(st.get_hf_candidate_details().iter().all(|d| d.starts_with("estimating")));
    assert_eq!(st.get_hf_candidate_index(), 1, "the default while estimates load");
    crate::gui::land_estimate_for_tests(app, 1, Ok(shape(5)));
    assert_eq!(st.get_hf_candidate_details().row_data(1).unwrap().as_str(), "all on GPU: 6.0 of 22.0 GiB with a 32k context");
    assert!(st.get_hf_candidate_details().row_data(0).unwrap().starts_with("estimating"), "rows fill in one by one");
    assert_eq!(st.get_hf_candidate_index(), 1, "no recommendation before every row has its answer");
    crate::gui::land_estimate_for_tests(app, 0, Ok(shape(9)));
    assert_eq!(st.get_hf_candidate_index(), 0, "the pick moved to the recommendation");
    assert!(st.get_hf_candidate_labels().row_data(0).unwrap().ends_with("recommended"));
    assert!(st.get_hf_candidate_details().row_data(0).unwrap().starts_with("recommended, the best quality that fits in GPU memory: "));
    assert!(st.get_hf_selection_summary().contains("Qwen3-8B-Q8_0.gguf"));
    // A row the user clicked stays picked: the recommendation only marks.
    st.invoke_hf_pick_result(0);
    crate::gui::land_listing_with_budget_for_tests(app, Ok(listing()), Some(budget()));
    st.invoke_hf_select(1);
    crate::gui::land_estimate_for_tests(app, 0, Ok(shape(9)));
    crate::gui::land_estimate_for_tests(app, 1, Err(crate::hf::HfError::Network("boom".into())));
    assert_eq!(st.get_hf_candidate_index(), 1, "the user's pick is kept");
    assert!(st.get_hf_candidate_labels().row_data(0).unwrap().ends_with("recommended"));
    assert_eq!(st.get_hf_candidate_details().row_data(1).unwrap().as_str(), "estimate unavailable: network error: boom");

    // The extras count, and the choices drive them: every row has a 12 GiB
    // mmproj (12.5 with its compute) and a DFlash drafter. Taken (the
    // default), the Q8_0 (10 GiB) only fits with its encoder on the CPU, so
    // the Q4_K_M (6 + 12.5 of 22) is recommended and stays picked.
    let with_extras = |tag: &str| crate::hf::Candidate {
        mmproj: Some(file("mmproj-F16.gguf", 12 << 30)),
        dflash: Some(file("Qwen3-8B-DFlash-Q8_0.gguf", 1 << 30)),
        ..candidate(tag)
    };
    let listing_x = || crate::hf::Listing { candidates: vec![with_extras("Q8_0"), with_extras("Q4_K_M")], ..listing() };
    st.invoke_hf_pick_result(0);
    crate::gui::land_listing_with_budget_for_tests(app, Ok(listing_x()), Some(budget()));
    crate::gui::land_estimate_for_tests(app, 0, Ok(shape(9)));
    crate::gui::land_estimate_for_tests(app, 1, Ok(shape(5)));
    let detail = |i: usize| st.get_hf_candidate_details().row_data(i).unwrap().to_string();
    assert_eq!(st.get_hf_candidate_index(), 1);
    assert!(st.get_hf_take_mmproj());
    assert_eq!(
        detail(1),
        "recommended, the best quality that fits in GPU memory: all on GPU: 18.5 of 22.0 GiB with a 32k context; \
         with the image encoder (12.5 GiB) on CUDA0"
    );
    assert_eq!(detail(0), "all on GPU: 10.0 of 22.0 GiB with a 32k context; with the image encoder (12.0 GiB) on the CPU");
    // Untick the mmproj: every row re-estimated without it (no fetch), the
    // Q8_0 recommended again and picked, the tick still off on it.
    st.set_hf_take_mmproj(false);
    st.invoke_hf_choices_changed();
    assert_eq!(detail(1), "all on GPU: 6.0 of 22.0 GiB with a 32k context");
    assert!(detail(0).starts_with("recommended, the best quality that fits in GPU memory: all on GPU: 10.0 of 22.0 GiB"), "{}", detail(0));
    assert_eq!(st.get_hf_candidate_index(), 0, "the pick follows the recommendation");
    assert!(!st.get_hf_take_mmproj(), "the choice survives the move");
    assert!(st.get_hf_selection_summary().starts_with("Fetches 1 file"), "{}", st.get_hf_selection_summary());
    // Pick the drafter: its header is read first (rows estimating), then
    // counted on the card beside the model.
    st.set_hf_drafter("DFlash".into());
    st.invoke_hf_choices_changed();
    assert!(detail(0).starts_with("estimating") && detail(1).starts_with("estimating"));
    crate::gui::land_draft_for_tests(app, "Qwen3-8B-DFlash-Q8_0.gguf", Ok(std::sync::Arc::new(crate::hf_fit::DraftShape::from_size(1 << 30))));
    assert!(
        detail(0).ends_with("all on GPU: 11.8 of 22.0 GiB with a 32k context; with the DFlash drafter (1.8 GiB) on CUDA0"),
        "{}",
        detail(0)
    );
    assert_eq!(st.get_hf_drafter().as_str(), "DFlash");
    // Tick the mmproj back: the Q4_K_M again, both extras on its row.
    st.set_hf_take_mmproj(true);
    st.invoke_hf_choices_changed();
    assert_eq!(st.get_hf_candidate_index(), 1);
    assert!(st.get_hf_take_mmproj() && st.get_hf_drafter().as_str() == "DFlash", "both wishes kept on the new pick");
    assert!(
        detail(1).ends_with("all on GPU: 20.2 of 22.0 GiB with a 32k context; with the image encoder (12.5 GiB) on CUDA0 and the DFlash drafter (1.8 GiB) on CUDA0"),
        "{}",
        detail(1)
    );
    // A row the user clicked stays picked through a toggle.
    st.invoke_hf_select(0);
    st.set_hf_take_mmproj(false);
    st.invoke_hf_choices_changed();
    st.set_hf_take_mmproj(true);
    st.invoke_hf_choices_changed();
    assert_eq!(st.get_hf_candidate_index(), 0, "the user's pick is kept");

    // A typed spec lists directly and drops the results; its failure returns
    // to the empty start.
    st.set_hf_input("org/repo".into());
    st.invoke_hf_submit();
    assert_eq!(st.get_hf_phase(), 3, "a spec skips the search");
    assert!(!st.get_hf_has_results() && st.get_hf_result_labels().row_count() == 0);
    crate::gui::land_listing_for_tests(app, Err(crate::hf::HfError::NoGguf { repo: "org/repo".into() }));
    assert_eq!(st.get_hf_phase(), 0);
    assert!(st.get_hf_error().contains("no GGUF"), "{}", st.get_hf_error());
    st.invoke_hf_back_to_results();
    assert_eq!(st.get_hf_phase(), 0, "no results to go back to");

    // ── Download: the job goes to the background (gui/downloads.rs) ──
    // The modal closes at once, the Downloads window takes the job, and the
    // editor, even with an unsaved edit, is never touched by what lands.
    st.invoke_hf_dismiss();
    assert!(!st.get_show_hf_dialog());
    let models_dir = dir.path().to_path_buf();
    {
        let mut cfg = crate::server_cfg::load();
        cfg.models_dir = Some(models_dir.to_string_lossy().into_owned());
        crate::server_cfg::save(&cfg).expect("save server.ini");
    }
    let win = crate::gui::downloads_window_for_tests();
    let rows = || win.get_rows().iter().collect::<Vec<_>>();
    assert!(rows().is_empty() && st.get_downloads_summary().is_empty(), "no download: no footer indicator");
    assert!(!crate::gui::quit_called_for_tests());
    // A dirty editor first.
    let first = st.get_presets().row_data(0).expect("a preset is listed");
    st.invoke_select_preset(first.orig_index);
    let edited_id = st.get_form().id.to_string();
    let mut form = st.get_form();
    form.ctx_size = "8192".into();
    form.ctx_size_default = false;
    st.set_form(form);
    assert!(st.get_preset_dirty());
    let editor_kept = |why: &str| {
        assert_eq!(st.get_form().id.as_str(), edited_id, "{why}: the editor keeps its preset");
        assert!(st.get_preset_dirty(), "{why}: and its unsaved edit");
        let sel = st.get_selected_preset_index();
        let row = st.get_presets().iter().find(|p| p.orig_index == sel).expect("the selection is listed");
        assert_eq!(row.id.as_str(), edited_id, "{why}: the selection stays on it");
    };
    let open_listing = |l: crate::hf::Listing, budget: Option<crate::hf_fit::Budget>| {
        st.set_show_new_kind_picker(false);
        st.invoke_hf_open();
        st.set_hf_input(l.repo.as_str().into());
        st.invoke_hf_submit();
        assert_eq!(st.get_hf_phase(), 3);
        crate::gui::land_listing_with_budget_for_tests(app, Ok(l), budget);
        assert_eq!(st.get_hf_phase(), 4);
    };
    open_listing(listing(), None);
    st.set_hf_token("hf_e2esecret".into());
    st.invoke_hf_download();
    assert!(!st.get_show_hf_dialog() && !st.get_modal_open(), "Download closes the modal: the main window is free");
    assert_eq!(st.get_hf_phase(), 0);
    assert!(st.get_status_text().contains("in the background"), "{}", st.get_status_text());
    let r = rows();
    assert_eq!(r.len(), 1, "the window has the job");
    assert_eq!(r[0].title.as_str(), "Qwen/Qwen3-8B-GGUF  ·  Q4_K_M", "the default pick");
    assert!(r[0].detail.contains("Qwen3-8B-Q4_K_M.gguf"), "{}", r[0].detail);
    assert_eq!((r[0].state.as_str(), r[0].overall.as_str()), ("Downloading", "Starting…"));
    assert!(r[0].can_cancel && r[0].show_progress && !r[0].can_dismiss);
    assert_eq!(st.get_downloads_summary().as_str(), "1 download running");
    assert!(win.window().is_visible(), "the window comes up with the job");
    editor_kept("a started download");
    let qwen = r[0].id;
    for text in [st.get_status_text(), r[0].title.clone(), r[0].detail.clone(), r[0].message.clone()] {
        assert!(!text.contains("hf_e2esecret"), "the token leaked into {text:?}");
    }
    st.set_hf_token("".into());
    // A second download of the same repository (any case) is refused in the
    // dialog: both would write its staging folder.
    open_listing(crate::hf::Listing { repo: "qwen/qwen3-8b-gguf".into(), ..listing() }, None);
    st.invoke_hf_download();
    assert!(st.get_show_hf_dialog(), "refused: the dialog stays");
    assert!(st.get_hf_error().contains("already downloading"), "{}", st.get_hf_error());
    assert_eq!(rows().len(), 1);
    // The dialog closes while a download runs: nothing runs IN it.
    st.invoke_hf_dismiss();
    assert!(!st.get_show_hf_dialog());
    // Progress lands in the row.
    crate::gui::land_progress_for_tests(
        app,
        qwen,
        &crate::hf::Progress {
            done: 1 << 30,
            total: 5 << 30,
            file: "Qwen3-8B-Q4_K_M.gguf".into(),
            file_done: 1 << 30,
            file_total: 5 << 30,
            files_done: 0,
            files_total: 1,
        },
    );
    let r = rows();
    assert!(r[0].overall.starts_with("1.00 GiB of 5.00 GiB (20%)"), "{}", r[0].overall);
    assert!(r[0].file.starts_with("Qwen3-8B-Q4_K_M.gguf: "), "{}", r[0].file);
    assert_eq!(r[0].overall_fraction, 0.2);
    // Quit asks first while it runs; Keep running takes it back.
    crate::gui::request_quit_for_tests(app);
    assert!(!crate::gui::quit_called_for_tests(), "a running download makes Quit ask");
    assert!(win.get_quit_pending() && win.get_quit_active() == 1);
    win.invoke_keep_running();
    assert!(!win.get_quit_pending());
    // Cancel: the row says so, and the partial is kept for Resume.
    win.invoke_cancel(qwen);
    assert_eq!(rows()[0].state.as_str(), "Cancelling…");
    crate::gui::land_transfer_for_tests(app, qwen, Err(crate::hf::HfError::Cancelled));
    let r = rows();
    assert_eq!(r[0].state.as_str(), "Cancelled");
    assert!(r[0].message.contains("kept") && r[0].can_resume && r[0].can_dismiss && !r[0].is_error);
    assert!(!st.get_status_is_error(), "a cancel is not an error");
    assert_eq!(st.get_downloads_summary().as_str(), "Downloads: 1 cancelled");
    // Resume, then a failure: in the row AND in the footer.
    win.invoke_resume(qwen);
    assert_eq!(rows()[0].state.as_str(), "Downloading");
    crate::gui::land_transfer_for_tests(app, qwen, Err(crate::hf::HfError::DiskFull("no space left".into())));
    let r = rows();
    assert_eq!(r[0].state.as_str(), "Failed");
    assert!(r[0].is_error && r[0].message.contains("no space left"), "{}", r[0].message);
    assert!(st.get_status_is_error());
    editor_kept("a cancel and a failure");
    // A finished one creates and lists the preset WITHOUT selecting it,
    // placed per card against the budget the listing was estimated with: a
    // 4-block MoE of 80 MiB on cards of 48 and 24 MiB usable keeps block 0's
    // experts in RAM and cuts the rest 3 + 2 (hf.rs has the same fixture,
    // `placed_download`).
    let model = models_dir.join("models").join("Hf-GGUF").join("hfmodel-Q4_K_M.gguf");
    std::fs::create_dir_all(model.parent().unwrap()).expect("repo folder");
    {
        use crate::hf_fit::tests::{gguf, write_sparse, W};
        const MIB: u64 = 1 << 20;
        let mut tensors = vec![("token_embd.weight".to_string(), 8 * MIB)];
        for b in 0..4 {
            tensors.push((format!("blk.{b}.attn_q.weight"), 2 * MIB));
            tensors.push((format!("blk.{b}.ffn_up_exps.weight"), 16 * MIB));
        }
        tensors.push(("output.weight".into(), 4 * MIB));
        let t: Vec<(&str, u64)> = tensors.iter().map(|(n, s)| (n.as_str(), *s)).collect();
        let (h, size) = gguf(
            &[
                ("general.architecture", W::Str("qwen3moe")),
                ("qwen3moe.block_count", W::U32(4)),
                ("qwen3moe.context_length", W::U32(4096)),
                ("qwen3moe.embedding_length", W::U32(256)),
                ("qwen3moe.attention.head_count", W::U32(4)),
                ("qwen3moe.attention.head_count_kv", W::U32(1)),
                ("qwen3moe.attention.key_length", W::U32(64)),
                ("qwen3moe.attention.value_length", W::U32(64)),
            ],
            &t,
        );
        write_sparse(&model, &h, size);
    }
    let card = |id: &str, name: &str, usable_mib: u64| crate::hf_fit::Gpu {
        id: id.into(),
        name: name.into(),
        total_mib: usable_mib + (crate::hf_fit::GPU_RESERVE >> 20),
    };
    let pair = crate::hf_fit::Budget {
        gpus: vec![card("ROCm1", "AMD Radeon AI PRO R9700", 48), card("CUDA0", "NVIDIA GeForce RTX 4070 SUPER", 24)],
        ram: Some(28 << 30),
        ram_total: Some(32 << 30),
    };
    open_listing(crate::hf::Listing { repo: "org/Hf-GGUF".into(), ..listing() }, Some(pair));
    st.invoke_hf_download();
    assert!(!st.get_show_hf_dialog(), "another repository downloads beside the failed one");
    let hf_job = rows()[0].id;
    assert_ne!(hf_job, qwen, "newest first");
    crate::gui::land_transfer_for_tests(
        app,
        hf_job,
        Ok(crate::hf::Downloaded {
            repo: "org/Hf-GGUF".into(),
            project_url: "https://huggingface.co/org/Hf-GGUF".into(),
            model: model.clone(),
            mmproj: None,
            draft: None,
        }),
    );
    let r = rows();
    assert_eq!(r[0].state.as_str(), "Done", "{}", r[0].message);
    assert!(r[0].message.contains("[hfmodel-Q4_K_M]") && r[0].message.contains("as estimated"), "{}", r[0].message);
    assert!(r[0].can_show && r[0].can_dismiss && !r[0].can_resume);
    assert_eq!(st.get_downloads_summary().as_str(), "Downloads: 1 done, 1 failed");
    assert!(!st.get_status_is_error(), "{}", st.get_status_text());
    assert!(st.get_status_text().contains("[hfmodel-Q4_K_M]"), "{}", st.get_status_text());
    assert!(st.get_status_text().contains("placed on the GPUs as estimated"), "{}", st.get_status_text());
    editor_kept("a created preset");
    assert!(st.get_presets().iter().any(|p| p.id.as_str() == "hfmodel-Q4_K_M"), "the new preset is listed");
    let ini = std::fs::read_to_string(crate::paths::presets_ini()).expect("presets.ini");
    assert!(
        ini.contains("[hfmodel-Q4_K_M]") && ini.contains("https://huggingface.co/org/Hf-GGUF"),
        "the derived preset is saved with its project URL:\n{ini}"
    );
    for line in ["device = ROCm1,CUDA0", "tensor-split = 3,2", "n-cpu-moe = 1", "ctx-size = 4096", "; placement = estimated at download"] {
        assert!(ini.contains(line), "the placement is written ({line}):\n{ini}");
    }
    assert!(
        st.get_model_labels().iter().any(|l| l.contains("hfmodel-Q4_K_M")),
        "the pickers were rescanned and list the new file"
    );
    // Show preset: refused (with a note) while a dialog is open in the main
    // window; else on the Models tab, through the discard guard.
    st.invoke_hf_open();
    win.invoke_show_preset(hf_job);
    assert!(rows()[0].note.contains("close it"), "{}", rows()[0].note);
    editor_kept("a Show under a modal");
    st.invoke_hf_dismiss();
    st.set_current_tab(0);
    win.invoke_show_preset(hf_job);
    assert_eq!(st.get_current_tab(), 1, "Show goes to the Models tab");
    assert!(rows()[0].note.is_empty(), "the note goes with the next Show");
    assert!(st.get_show_discard_dialog(), "a dirty form asks before Show replaces it");
    st.invoke_cancel_discard();
    editor_kept("a Show whose discard was cancelled");
    win.invoke_show_preset(hf_job);
    st.invoke_confirm_discard();
    assert_eq!(st.get_form().id.as_str(), "hfmodel-Q4_K_M", "Show selects the new preset");
    assert_eq!(st.get_form().model.as_str(), model.to_string_lossy());
    let f = st.get_form();
    assert!(f.placement_note.starts_with("estimated at download for a 4k context"), "{}", f.placement_note);
    assert_eq!((f.device.as_str(), f.tensor_split.as_str(), f.n_cpu_moe, f.fit), ("ROCm1,CUDA0", "3,2", 1, false));
    assert!(!st.get_preset_dirty(), "the derived preset loads clean: the form round-trips it");
    win.invoke_show_preset(hf_job);
    assert!(!st.get_show_discard_dialog(), "already in the editor: nothing to ask");
    // Dismiss takes a finished row; a confirmed Quit stops what runs and
    // quits once it has landed.
    win.invoke_dismiss(hf_job);
    assert_eq!(rows().len(), 1);
    win.invoke_resume(qwen);
    crate::gui::request_quit_for_tests(app);
    win.invoke_confirm_quit();
    assert!(win.get_quitting());
    assert_eq!(rows()[0].state.as_str(), "Cancelling…", "Quit cancels cooperatively");
    assert!(!crate::gui::quit_called_for_tests(), "not before the child has stopped");
    crate::gui::land_transfer_for_tests(app, qwen, Err(crate::hf::HfError::Cancelled));
    assert!(crate::gui::quit_called_for_tests(), "the last transfer landed: the loop ends");
    win.invoke_clear_finished();
    assert!(rows().is_empty() && st.get_downloads_summary().is_empty());
    crate::gui::request_quit_for_tests(app);
    assert!(crate::gui::quit_called_for_tests(), "nothing runs: Quit quits at once");
    win.hide().ok();

    delete_with_files_phase(app, &models_dir, &model);
}

/// Deleting a preset together with the model files it links (gui/models_tab.rs
/// `DeleteDialog` over src/linked_files.rs), then the CLI's `--delete-files`:
/// the rows list what is on disk, every tick starts cleared, a file another
/// preset uses (in either INI file) cannot be ticked, the confirm removes the
/// section and then the ticked files and their emptied repo folders, the
/// pickers are rescanned, and a file that fails to delete is reported without
/// blocking the preset's deletion.
fn delete_with_files_phase(app: &AppWindow, models_dir: &std::path::Path, hf_model: &std::path::Path) {
    use crate::presets::Preset;
    let st = app.global::<AppState>();
    let mut cfg = crate::server_cfg::load();
    cfg.models_dir = Some(models_dir.to_string_lossy().into_owned());
    crate::server_cfg::save(&cfg).expect("save server.ini");
    let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
    let touch = |p: &std::path::Path, n: usize| {
        std::fs::create_dir_all(p.parent().unwrap()).expect("folder");
        std::fs::write(p, vec![0u8; n]).expect("file");
    };
    let repo = |sub: &str, name: &str| models_dir.join(sub).join("org").join(name);
    let shard1 = repo("models", "Split-GGUF").join("split-00001-of-00002.gguf");
    let shard2 = repo("models", "Split-GGUF").join("split-00002-of-00002.gguf");
    let mmproj = repo("mmprojs", "Split-GGUF").join("mmproj-F16.gguf");
    let drafter = repo("mtps", "Split-GGUF").join("split-mtp.gguf");
    for (f, n) in [(&shard1, 3), (&shard2, 4), (&mmproj, 2), (&drafter, 1)] {
        touch(f, n);
    }
    crate::presets::save(&Preset {
        id: "split".into(),
        model: path(&shard1),
        mmproj: path(&mmproj),
        model_draft: path(&drafter),
        spec_type: "draft-mtp".into(),
        ..Preset::default()
    })
    .expect("save split");
    // Shares the mmproj, and is switched OFF: the in-use check reads both files.
    crate::presets::save(&Preset {
        id: "sharer".into(),
        model: path(hf_model),
        mmproj: path(&mmproj),
        ..Preset::default()
    })
    .expect("save sharer");
    crate::presets::set_enabled("sharer", false).expect("park sharer");
    st.invoke_reload_all();
    let select = |id: &str| {
        let row = st
            .get_presets()
            .iter()
            .find(|p| p.id.as_str() == id)
            .unwrap_or_else(|| panic!("{id} is not listed"));
        st.invoke_select_preset(row.orig_index);
        assert_eq!(st.get_form().id.as_str(), id);
    };
    select("split");
    assert!(
        st.get_model_labels().iter().any(|l| l.contains("split-00001")),
        "the split model is in the picker before the delete"
    );

    // Opening lists the three files, none ticked, the shared mmproj locked.
    st.invoke_request_delete();
    assert!(st.get_show_delete_dialog() && st.get_modal_open(), "the dialog is a modal");
    assert_eq!(st.get_delete_target_id().as_str(), "split");
    assert_eq!(st.get_delete_target_file().as_str(), "presets.ini");
    let rows = || st.get_delete_file_rows().iter().collect::<Vec<_>>();
    let r = rows();
    assert_eq!(r.len(), 3, "model, mmproj, drafter");
    assert_eq!(r[0].label.as_str(), "Model: split-00001-of-00002.gguf (2 shards)");
    assert!(r.iter().all(|r| !r.checked), "keeping the files is the default");
    assert!(!r[0].in_use && r[1].in_use && !r[2].in_use);
    assert!(r[1].detail.contains("in use by [sharer]"), "{}", r[1].detail);
    assert_eq!(st.get_delete_files_ticked(), 0);
    assert!(st.get_delete_files_summary().starts_with("Only the preset"));
    // Cancel drops the ticks: a reopen starts cleared again.
    st.invoke_delete_file_toggle(0);
    assert_eq!(st.get_delete_files_ticked(), 1);
    st.invoke_delete_cancel();
    assert!(!st.get_show_delete_dialog());
    st.invoke_request_delete();
    assert_eq!(st.get_delete_files_ticked(), 0, "ticks never outlive their dialog");
    // The shared mmproj cannot be ticked; the model and the drafter can, the
    // drafter through the row's own box (the .slint wiring).
    st.invoke_delete_file_toggle(1);
    assert!(!rows()[1].checked, "an in-use file cannot be ticked");
    st.invoke_delete_file_toggle(0);
    ElementHandle::find_by_accessible_label(app, "delete-file-2")
        .next()
        .expect("the drafter row's box")
        .invoke_accessible_default_action();
    let r = rows();
    assert!(r[0].checked && !r[1].checked && r[2].checked);
    assert_eq!(st.get_delete_files_ticked(), 2);
    let summary = st.get_delete_files_summary();
    assert!(
        summary.contains("split-00001-of-00002.gguf (2 shards)") && summary.contains("split-mtp.gguf"),
        "the summary names what goes: {summary}"
    );

    // Confirm: the section, then the files, then the emptied repo folders.
    st.invoke_delete_preset("split".into());
    assert!(!st.get_show_delete_dialog());
    assert!(!st.get_status_is_error(), "{}", st.get_status_text());
    let status = st.get_status_text();
    assert!(
        status.starts_with("Deleted [split]; removed 3 files") && status.contains("empty folders removed"),
        "{status}"
    );
    assert!(crate::presets::load_listed().iter().all(|l| l.preset.id != "split"));
    assert!(!shard1.exists() && !shard2.exists() && !drafter.exists());
    assert!(mmproj.exists(), "the shared mmproj stays");
    assert!(!repo("models", "Split-GGUF").exists() && !repo("mtps", "Split-GGUF").exists());
    assert!(repo("mmprojs", "Split-GGUF").is_dir(), "a folder with content stays");
    assert!(
        models_dir.join("models").is_dir() && models_dir.join("mtps").is_dir(),
        "the subfolders themselves stay"
    );
    assert!(
        st.get_model_labels().iter().all(|l| !l.contains("split-00001")),
        "the pickers were rescanned"
    );
    assert_eq!(st.get_selected_preset_index(), -1, "delete still clears the selection");

    // A file that fails to delete (a folder now stands at its path) is
    // reported, and the preset is deleted anyway. `sharer` is parked, and its
    // model is in use by the Hugging Face preset, its mmproj no longer.
    select("sharer");
    st.invoke_request_delete();
    assert_eq!(st.get_delete_target_file().as_str(), "presets.ini.disabled");
    let r = rows();
    assert_eq!(r.len(), 2);
    assert!(r[0].in_use && r[0].detail.contains("[hfmodel-Q4_K_M]"), "{}", r[0].detail);
    assert!(!r[1].in_use, "nobody else uses the mmproj now");
    st.invoke_delete_file_toggle(1);
    std::fs::remove_file(&mmproj).expect("remove the mmproj");
    std::fs::create_dir(&mmproj).expect("a folder in its place");
    st.invoke_delete_preset("sharer".into());
    assert!(
        st.get_status_is_error() && st.get_status_text().contains("could not delete"),
        "{}",
        st.get_status_text()
    );
    assert!(st.get_status_text().starts_with("Deleted [sharer]"));
    assert!(crate::presets::load_listed().iter().all(|l| l.preset.id != "sharer"));
    assert!(hf_model.exists(), "the in-use model was never offered");
    assert!(mmproj.is_dir(), "the folder that refused is still there");
    std::fs::remove_dir(&mmproj).expect("tidy up");

    // ── CLI: `preset delete --delete-files`, the same rules ───────────────
    use crate::cli::{Cli, Command, FileKind, PresetCmd};
    let delete = |id: &str, which: &[FileKind], even_if_shared: bool| {
        crate::cli::run(Cli {
            command: Command::Preset(PresetCmd::Delete {
                id: id.into(),
                delete_files: which.to_vec(),
                even_if_shared,
            }),
        })
    };
    let cli_model = repo("models", "Cli-GGUF").join("cli.gguf");
    let cli_draft = repo("mtps", "Cli-GGUF").join("cli-mtp.gguf");
    touch(&cli_model, 2);
    touch(&cli_draft, 1);
    crate::presets::save(&Preset {
        id: "cli-del".into(),
        model: path(&cli_model),
        model_draft: path(&cli_draft),
        ..Preset::default()
    })
    .expect("save cli-del");
    crate::presets::save(&Preset {
        id: "cli-other".into(),
        model: path(hf_model),
        model_draft: path(&cli_draft),
        ..Preset::default()
    })
    .expect("save cli-other");
    let listed = |id: &str| crate::presets::load_listed().iter().any(|l| l.preset.id == id);
    let err = delete("cli-del", &[FileKind::All], false).expect_err("a shared drafter is refused");
    assert!(err.to_string().contains("[cli-other]"), "{err}");
    assert!(listed("cli-del") && cli_model.exists() && cli_draft.exists(), "a refusal changes nothing");
    delete("cli-del", &[FileKind::All], true).expect("forced");
    assert!(!listed("cli-del") && !cli_model.exists() && !cli_draft.exists());
    assert!(!repo("models", "Cli-GGUF").exists() && !repo("mtps", "Cli-GGUF").exists());
    // Without the flag, every file stays.
    delete("cli-other", &[], false).expect("plain delete");
    assert!(!listed("cli-other") && hf_model.exists());
}
