//! Slint GUI wiring for the llama.cpp-framework configurator.
//!
//! ## Shape
//! `run()` builds `AppWindow` + `AppTray`, seeds the UI from disk, then wires
//! each tab's callbacks via the per-tab submodules `server_tab` / `models_tab` /
//! `integrations_tab` / `tray` (one file each, under `gui/`). Those submodules
//! reach this module's shared helpers, the `State` cache, and the generated
//! Slint types through `use super::*`. All shared UI state lives in the Slint
//! `AppState` global (ui/state.slint), reached with
//! `let s = app.global::<AppState>()`. `State` (below) is the small Rust-side
//! cache the callbacks share via `Rc<RefCell<…>>`: the loaded presets vector and
//! the new-preset dialog's model scan.
//!
//! ## Helper verb taxonomy (so a grep lands in the right family)
//! - `load_*` / `read_*` : server.ini ↔ UI (through `server_form::config_to_form`
//!   / `form_to_config`; the preset side is `form.rs`).
//! - `refresh_*`         : rebuild a list/section from disk or the device cache
//!   (`refresh_presets`, `refresh_file_options`, `refresh_device_options`,
//!   `refresh_integrations`, `refresh_run_status`).
//! - `apply_*`           : push a computed (labels, values, index) triple — or a
//!   whole form — into `AppState` through a small closure.
//! - `populate_*`        : fill a dialog's option lists (bind interfaces, dialog
//!   models).
//! - `spawn_*`           : run a slow probe off the UI thread, then apply the
//!   result via `invoke_from_event_loop` (`spawn_version_probe`, `spawn_device_probe`).
//! - `run_new_*` / `commit_new_preset` : the New / Clone → save funnel.
//!
//! ## Dirty tracking
//! Save/Revert enable off `*_dirty` in state.slint, each computed as
//! `form != form_base` (presets) / `server_form != server_form_base` (server).
//! Rust keeps the baseline in sync: `apply_form` sets form + base together;
//! `snapshot_server_base` re-baselines the server form after a save.
//!
//! ## Threading
//! The event loop is single-threaded. Slow work (`--list-devices`, `--version`,
//! `tasklist`, the stop wait) runs on `std::thread::spawn`; results come back
//! through `slint::invoke_from_event_loop` guarded by `Weak::upgrade`.
//!
//! `AppTray` is a SEPARATE Slint root — it does NOT use `AppState`; Rust pushes
//! state to it directly (`tray.set_server_running` / `tray.on_*`).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::form::{blank_form, form_to_preset, preset_to_form};
use crate::{
    devices, gguf, integrations, model_scan, net_ifaces, paths, presets, runstate, server_cfg,
    server_form, server_version,
};

slint::include_modules!();

// Per-tab callback wiring — one file each under `gui/`. Each `wire()` reaches the
// shared helpers, the `State` cache, and the generated Slint types via `use super::*`.
mod integrations_tab;
mod models_tab;
mod server_tab;
mod tray;

#[derive(Default)]
struct State {
    presets: Vec<presets::Preset>,
    // Full (unfiltered) model scan backing the new-preset dialog, so the search
    // box can filter without re-hitting disk on every keystroke.
    dialog_models_all: Vec<model_scan::FileOption>,
}

pub fn run() -> anyhow::Result<()> {
    // Single-instance: if the configurator is already running, hand off to it
    // (it surfaces its window) and exit instead of opening a second window.
    #[cfg(windows)]
    let _instance = match crate::single_instance::acquire() {
        crate::single_instance::Acquire::Secondary => return Ok(()),
        crate::single_instance::Acquire::Primary(guard) => guard,
    };

    let app = AppWindow::new()?;
    let s = app.global::<AppState>();
    s.set_app_version(SharedString::from(env!("CARGO_PKG_VERSION")));
    // Upper bound for the Server tab's CPU-thread sliders: this machine's logical
    // processor count. Leaves the Slint fallback in place if the query fails.
    if let Ok(n) = std::thread::available_parallelism() {
        s.set_cpu_threads_max(i32::try_from(n.get()).unwrap_or(i32::MAX));
    }
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
        s.on_sync_device_dropdowns(move || {
            if let Some(app) = app_weak.upgrade() {
                refresh_device_options(&app);
            }
        });
    }

    {
        let app_weak = app.as_weak();
        s.on_model_changed(move || {
            if let Some(app) = app_weak.upgrade() {
                update_model_info(&app);
            }
        });
    }

    s.set_presets_path(SharedString::from(
        paths::presets_ini().to_string_lossy().into_owned(),
    ));

    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        s.on_refresh_status(move || {
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
        s.on_reload_all(move || {
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

    server_tab::wire(&app, &tray);
    models_tab::wire(&app, &state);
    integrations_tab::wire(&app);
    tray::wire(&app, &tray);

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

/// Wrap a `Vec<T>` as a Slint list model (the `[T]` properties the UI binds to).
/// One home for the `ModelRc::from(Rc::new(VecModel::from(..)))` incantation.
fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(items)))
}

/// A `Vec<String>` as a `[string]` model (the dropdowns bind to `SharedString`).
fn string_model(items: Vec<String>) -> ModelRc<SharedString> {
    model(items.into_iter().map(SharedString::from).collect())
}

// ── Server helpers ───────────────────────────────────────────────────

fn load_server_into_ui(app: &AppWindow) {
    let s = app.global::<AppState>();
    let form = server_form::config_to_form(&server_cfg::load());
    populate_bind_options(app, form.hostname.as_str());
    let cmdline = runstate::command_line().unwrap_or_default();
    s.set_server_command_line(SharedString::from(cmdline));
    // Set form + base together so `server_dirty` reads false right after load.
    s.set_server_form(form.clone());
    s.set_server_form_base(form);
}

fn populate_bind_options(app: &AppWindow, current: &str) {
    let s = app.global::<AppState>();
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
    s.set_bind_labels(model(labels));
    s.set_bind_values(model(values));
    s.set_bind_index(index.unwrap_or(0) as i32);
}

/// Re-baseline the server form after a save so `server_dirty` reads false until
/// the next edit — the server analog of `apply_form`'s base handling.
fn snapshot_server_base(app: &AppWindow) {
    let s = app.global::<AppState>();
    s.set_server_form_base(s.get_server_form());
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
    let s = app.global::<AppState>();
    let all = presets::load_all();
    let summaries = preset_summaries(&all, s.get_presets_filter().as_str());
    s.set_presets(model(summaries));

    let prev_sel = s.get_selected_preset_index();
    let idx = match want {
        Some(id) => all.iter().position(|p| p.id == id).map(|i| i as i32),
        None => (prev_sel >= 0 && (prev_sel as usize) < all.len()).then_some(prev_sel),
    }
    .unwrap_or(if all.is_empty() { -1 } else { 0 });

    state.borrow_mut().presets = all;
    s.set_selected_preset_index(idx);

    let st = state.borrow();
    match usize::try_from(idx).ok().and_then(|i| st.presets.get(i)) {
        Some(p) => apply_form(app, preset_to_form(p)),
        None => apply_form(app, blank_form()),
    }
}

fn refresh_file_options(app: &AppWindow) {
    let s = app.global::<AppState>();
    let models_dir = s.get_server_form().models_dir.to_string();
    let form = s.get_form();

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
            let s = app.global::<AppState>();
            s.set_model_labels(lbl);
            s.set_model_values(val);
            s.set_model_index(idx);
        },
    );
    apply_scanned(
        app,
        model_scan::Category::Mmproj,
        mmproj_scan_result,
        form.mmproj.as_str(),
        |app, lbl, val, idx| {
            let s = app.global::<AppState>();
            s.set_mmproj_labels(lbl);
            s.set_mmproj_values(val);
            s.set_mmproj_index(idx);
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
    s.set_draft_labels(string_model(draft_labels));
    s.set_draft_values(string_model(draft_values));
    s.set_draft_specs(string_model(draft_specs));
    s.set_draft_index(draft_idx);

    refresh_device_options(app);
    update_model_info(app);
}

/// Fill the read-only "Model info" box from the selected model's GGUF header
/// (read via `ggml-base.dll`), enriched with the selected mmproj and draft
/// headers plus a cross-reference of the framework's MTP/DFlash drafters. Called
/// whenever the model/mmproj/draft field changes (combo pick, preset load).
fn update_model_info(app: &AppWindow) {
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

    if model.trim().is_empty() {
        s.set_model_info_ready(false);
        s.set_model_info_note(SharedString::from("Select a model to see its details."));
        return;
    }

    let Some(info) = gguf::read_model_info(std::path::Path::new(&model)) else {
        s.set_model_info_ready(false);
        s.set_model_info_note(SharedString::from(
            "Metadata unavailable — is ggml-base.dll beside the app, and the file a valid GGUF?",
        ));
        return;
    };

    let models_dir = s.get_server_form().models_dir.to_string();
    let ext = gguf::external_drafters(&models_dir, &model);
    s.set_model_info_kind(SharedString::from(info.kind_line()));
    s.set_model_info_n_layer(info.n_layer as i32);
    s.set_model_info_has_moe(info.is_moe);
    s.set_model_info_moe(SharedString::from(info.moe_offload_line()));
    s.set_model_info_arch_quant(SharedString::from(info.arch_quant_line()));
    s.set_model_info_layers_ctx(SharedString::from(info.layers_ctx_line()));
    s.set_model_info_attn(SharedString::from(info.attn_line()));
    s.set_model_info_draft(SharedString::from(gguf::draft_line(&info, &ext)));
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

/// Rebuild the three GPU-device dropdowns (server-wide, per-preset main,
/// per-preset draft) from the cached `--list-devices` result, recomputing each
/// selected index against the current server.ini / form values.
fn refresh_device_options(app: &AppWindow) {
    let s = app.global::<AppState>();
    let devs = cached_devices(app);
    let form = s.get_form();
    let server_device = s.get_server_form().device;

    apply_device(
        app,
        &devs,
        server_device.as_str(),
        "(all detected devices)",
        |app, lbl, val, idx| {
            let s = app.global::<AppState>();
            s.set_server_dev_labels(lbl);
            s.set_server_dev_values(val);
            s.set_server_dev_index(idx);
        },
    );
    apply_device(
        app,
        &devs,
        form.device.as_str(),
        "(server default)",
        |app, lbl, val, idx| {
            let s = app.global::<AppState>();
            s.set_pdev_labels(lbl);
            s.set_pdev_values(val);
            s.set_pdev_index(idx);
        },
    );
    apply_device(
        app,
        &devs,
        form.device_draft.as_str(),
        "(auto / same as model)",
        |app, lbl, val, idx| {
            let s = app.global::<AppState>();
            s.set_pdraft_labels(lbl);
            s.set_pdraft_values(val);
            s.set_pdraft_index(idx);
        },
    );
}

/// Reconstruct the cached device list from the two parallel Slint arrays the
/// async probe fills in (`all_device_ids` / `all_device_labels`).
fn cached_devices(app: &AppWindow) -> Vec<devices::DeviceOption> {
    let s = app.global::<AppState>();
    let ids = s.get_all_device_ids();
    let labels = s.get_all_device_labels();
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

// ── Status / version ─────────────────────────────────────────────────

fn set_status(app: &AppWindow, text: String, is_error: bool) {
    let s = app.global::<AppState>();
    s.set_status_text(SharedString::from(text));
    s.set_status_is_error(is_error);
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
            let s = app.global::<AppState>();
            s.set_all_device_ids(model(ids));
            s.set_all_device_labels(model(labels));
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
                let s = app.global::<AppState>();
                s.set_server_stopping(false);
                s.set_server_running(running);
                match result {
                    Ok(()) if !running => {
                        s.set_server_status_is_error(false);
                        set_status(&app, "llama-server stopped.".into(), false);
                    }
                    Ok(()) => {
                        s.set_server_status_is_error(true);
                        set_status(
                            &app,
                            "Stop timed out — llama-server is still running.".into(),
                            true,
                        );
                    }
                    Err(e) => {
                        s.set_server_status_is_error(true);
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
                let s = app.global::<AppState>();
                s.set_server_running(running);
                s.set_server_status_is_error(false);
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
    let s = app.global::<AppState>();
    s.set_form_base(form.clone());
    s.set_form(form);
}

// ── Integrations helpers ──────────────────────────────────────────────

fn refresh_integrations(app: &AppWindow) {
    let s = app.global::<AppState>();
    let cfg = server_cfg::load();
    let port = cfg.port.unwrap_or(8080);
    let hostname = cfg.hostname.unwrap_or_else(|| "localhost".into());
    let base_url = format!("http://{hostname}:{port}/v1");
    s.set_integration_base_url(SharedString::from(base_url));

    let claude_env = integrations::claude_code_env_script(&format!("http://{hostname}:{port}/v1"));
    s.set_integration_claude_env(SharedString::from(claude_env));

    s.set_integration_provider_active(integrations::detect_opencode_provider());

    let enabled_ids = integrations::opencode_model_ids();
    let items: Vec<IntegrationModel> = presets::load_all()
        .iter()
        .map(|p| IntegrationModel {
            id: SharedString::from(p.id.clone()),
            label: SharedString::from(integrations::friendly_model_name(&p.id, &p.model)),
            enabled: enabled_ids.contains(&p.id),
        })
        .collect();
    s.set_integration_models(model(items));
}
