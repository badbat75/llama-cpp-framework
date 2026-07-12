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
//! cache the callbacks share via `Rc<RefCell<…>>`: the loaded presets vector,
//! the new-preset dialog's model scan, the draft dropdown's (value, spec)
//! rows, and the discard dialog's parked action (`pending_discard` — the
//! continuation `confirm_discard_then` stashes until the user's verdict).
//! (The GPU-device probe result is cached in `devices::probed()` — it is
//! process-wide, written by the probe thread at startup and on Refresh/F5.)
//!
//! ## Helper verb taxonomy (so a grep lands in the right family)
//! - `load_*`            : server.ini ↔ UI (through `server_form::config_to_form`
//!   / `form_to_config`; the preset side is `form.rs`).
//! - `refresh_*` / `reload_*` : rebuild a list/section from disk or the device
//!   cache (`reload_presets`, `refresh_file_options`, `refresh_device_options`,
//!   `refresh_integrations`, `refresh_run_status`, `refresh_server_snapshot`;
//!   `reload_all_from_disk` is the everything-at-once hub shared by startup and
//!   Refresh/F5).
//! - `*_options`         : build a dropdown's (labels, values, index) model
//!   triple the caller hands to the matching `set_*` accessors (`device_options`,
//!   `scanned_options`).
//! - `apply_form`        : push a whole `PresetForm` + its baseline into `AppState`.
//! - `populate_*`        : fill a dropdown's parallel option arrays in place
//!   (`populate_bind_options` — the Server tab's bind-address list).
//! - `start_server_async` / `stop_server_async` : the canonical run-control paths
//!   shared by the Server tab and the tray, so both surfaces report a start/stop
//!   identically; both run off the UI thread and drive a transitional flag.
//! - `spawn_*`           : run a slow probe off the UI thread, then apply the
//!   result via `invoke_from_event_loop` (`spawn_version_probe`, `spawn_device_probe`).
//!
//! Tab-specific helpers live next to their only callers in `gui/`: the New / Clone
//! dialog funnel (`populate_dialog_models` … `commit_new_preset`) and the
//! Model-info box (`update_model_info`) are in `gui/models_tab.rs`.
//!
//! ## Dirty tracking
//! Save/Revert enable off `*_dirty` in state.slint, each computed as
//! `form != form_base` (presets) / `server_form != server_form_base` (server).
//! Rust keeps the baseline in sync: `apply_form` sets form + base together; the
//! server tab re-baselines its form after a save (`server_tab::snapshot_server_base`).
//!
//! ## Threading
//! The event loop is single-threaded. Slow work (`--list-devices`, `--version`,
//! `tasklist`, the stop wait) runs on `std::thread::spawn`; results come back
//! through `slint::invoke_from_event_loop` guarded by `Weak::upgrade`. One
//! sanctioned on-thread blocker: the native folder picker
//! (`server_tab::pick_dir`) is a modal OS dialog — blocking its caller is the
//! conventional behavior, not a stall to fix.
//!
//! `AppTray` and `LogWindow` are SEPARATE Slint roots — they do NOT use
//! `AppState`; Rust pushes state to them directly (`tray.set_server_running` /
//! `tray.on_*`; the log tail in `gui/log_window.rs`). `LogWindow` is a real
//! second window on the same event loop: non-modal, so the main window stays
//! interactive while it is open.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::form::{form_to_preset, preset_to_form};
use crate::{
    devices, gpu_split, integrations, model_scan, net_ifaces, paths, presets, runstate, server_cfg,
    server_form, server_version,
};

slint::include_modules!();

// Per-tab callback wiring — one file each under `gui/`. Each `wire()` reaches the
// shared helpers, the `State` cache, and the generated Slint types via `use super::*`.
mod integrations_tab;
mod log_window;
mod models_tab;
mod server_tab;
mod tray;

#[derive(Default)]
struct State {
    presets: Vec<presets::Preset>,
    // Full (unfiltered) model scan backing the new-preset dialog, so the search
    // box can filter without re-hitting disk on every keystroke.
    dialog_models_all: Vec<model_scan::FileOption>,
    // Paths of the currently filtered dialog rows, parallel to
    // AppState.dialog_model_labels. Rust-only data (only
    // `picked_dialog_model_path` reads it), so it lives here instead of as a
    // published Slint array no page ever binds.
    dialog_models_filtered: Vec<String>,
    // (--model-draft value, --spec-type) per draft-dropdown row, parallel to
    // AppState.draft_labels. Rust-only data (only `draft_picked` reads it), so
    // it lives here instead of as published Slint arrays no page ever binds.
    draft_rows: Vec<(String, String)>,
    // Action parked by `confirm_discard_then` while the discard-confirm dialog
    // is up; `confirm_discard` runs it, `cancel_discard` drops it.
    pending_discard: Option<Box<dyn Fn()>>,
}

/// TEST-ONLY seam for the e2e flow test (src/tests/save_flow.rs): build the
/// shared `State`, seed the preset list from disk, and wire the Models +
/// Integrations tabs plus the discard dialog — nothing else (no tray, no
/// probes, no single-instance, no event loop). The caller must have redirected
/// config IO first (see `paths::data_root`).
#[cfg(test)]
pub(crate) fn wire_tabs_for_tests(app: &AppWindow) {
    let state = Rc::new(RefCell::new(State::default()));
    reload_presets(app, &state, None);
    models_tab::wire(app, &state);
    integrations_tab::wire(app);
    wire_discard_confirm(app, &state);
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

    reload_all_from_disk(&app, &state, tray.as_weak());

    s.set_presets_path(SharedString::from(
        paths::presets_ini().to_string_lossy().into_owned(),
    ));

    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        let state = state.clone();
        s.on_reload_all(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let s = app.global::<AppState>();
            // Reloading replaces BOTH forms from disk and rebuilds the
            // Integrations list, so unsaved edits on ANY tab need the
            // discard confirmation.
            let dirty = s.get_preset_dirty() || s.get_server_dirty() || integrations_dirty(&app);
            let action: Box<dyn Fn()> = {
                let app_weak = app_weak.clone();
                let tray_weak = tray_weak.clone();
                let state = state.clone();
                Box::new(move || {
                    let Some(app) = app_weak.upgrade() else {
                        return;
                    };
                    reload_all_from_disk(&app, &state, tray_weak.clone());
                    set_status(&app, "Reloaded configuration from disk.".into(), false);
                })
            };
            confirm_discard_then(&app, &state, dirty, action);
        });
    }
    wire_discard_confirm(&app, &state);

    let status_timer = slint::Timer::default();
    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        status_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(5),
            move || {
                // Periodic tick: keep an error footer red (clear_error = false) —
                // only an explicit action (F5 / Refresh / a new start) resets it.
                refresh_run_status(app_weak.clone(), tray_weak.clone(), false);
            },
        );
    }

    server_tab::wire(&app, &tray, &state);
    models_tab::wire(&app, &state);
    integrations_tab::wire(&app);
    tray::wire(&app, &tray);
    // Independent log-tail window (View logs). Both halves must outlive the
    // event loop like status_timer above — dropping the Timer stops the tail.
    // (The timer only RUNS while the window is open: armed on View logs,
    // stopped on close.)
    let (_log_window, _log_timer) = log_window::wire(&app)?;

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
/// the single-instance activation event, and by the tray's "Open window" item
/// (a bare `show()` would leave a minimized window minimized).
fn activate_window(app: AppWindow) {
    use slint::winit_030::WinitWindowAccessor;
    app.show().ok();
    app.window().with_winit_window(|w| {
        w.set_visible(true);
        w.set_minimized(false);
        w.focus_window();
    });
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
    refresh_server_snapshot(app);
    // Set form + base together so `server_dirty` reads false right after load.
    s.set_server_form(form.clone());
    s.set_server_form_base(form);
    // Re-project the freshly loaded device/tensor-split strings into the table
    // (and the mmproj dropdown's index) — see apply_form for why it's a rebuild.
    refresh_device_options(app);
}

/// Recompute the read-only projections of the SAVED server.ini: the Command
/// Line card and the chat-UI URL. Both must track the file, not the form — the
/// running server listens on what was saved, not on an unsaved edit. Shared by
/// `load_server_into_ui` and the Server tab's save handler. The URL uses
/// `client_host()`, so an all-interfaces bind (0.0.0.0) opens localhost instead
/// of a dead address.
fn refresh_server_snapshot(app: &AppWindow) {
    let s = app.global::<AppState>();
    let cmdline = runstate::command_line().unwrap_or_default();
    s.set_server_command_line(SharedString::from(cmdline));
    let cfg = server_cfg::load();
    s.set_chat_url(SharedString::from(client_base_url(&cfg)));
}

/// The client-facing base URL for a config: `client_host()` (0.0.0.0 →
/// localhost) + port. ONE home for the URL assembly — the chat URL, the
/// launched-URL snapshot, and `opencode_base_url` all build on it.
fn client_base_url(cfg: &server_cfg::ServerConfig) -> String {
    format!("http://{}:{}", cfg.client_host(), cfg.port_or_default())
}

fn populate_bind_options(app: &AppWindow, current: &str) {
    let s = app.global::<AppState>();
    let (labels, values, index) = bind_options(current);
    s.set_bind_labels(labels);
    s.set_bind_values(values);
    s.set_bind_index(index);
}

// ── Discard-confirm guard ────────────────────────────────────────────

/// Run `action` now — or, when the caller's form is `dirty`, park it in
/// `State.pending_discard` and raise the discard-confirm dialog instead. The
/// dialog's Confirm runs the parked action, Cancel drops it (keeping the
/// edits). The guard shared by every navigation that replaces a dirty form:
/// preset switch, Refresh/`reload_all`, and the New…/Clone… entry points.
fn confirm_discard_then(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    dirty: bool,
    action: Box<dyn Fn()>,
) {
    if dirty {
        state.borrow_mut().pending_discard = Some(action);
        app.global::<AppState>().set_show_discard_dialog(true);
    } else {
        action();
    }
}

/// Wire the discard dialog's verdict callbacks. Called from `run()` and from
/// the e2e seam (`wire_tabs_for_tests`) — the guarded handlers live in both
/// wirings.
fn wire_discard_confirm(app: &AppWindow, state: &Rc<RefCell<State>>) {
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_confirm_discard(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            app.global::<AppState>().set_show_discard_dialog(false);
            // Take the action out of the RefCell BEFORE running it — it will
            // borrow `state` itself (reload_presets & co.).
            let action = state.borrow_mut().pending_discard.take();
            if let Some(action) = action {
                action();
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_cancel_discard(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            app.global::<AppState>().set_show_discard_dialog(false);
            state.borrow_mut().pending_discard = None;
        });
    }
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
            orig_index: i as i32,
        })
        .collect()
}

/// Seed / re-seed every disk-backed piece of the UI: server.ini, presets.ini,
/// the ModelsDir scan, the integration state, the live run status, and the
/// llama-server version + device probes (the exe can change under us — e.g. a
/// `02-build.ps1` rerun while the configurator is open). The one home shared by
/// `run()`'s startup seed and Refresh/F5 (`reload_all`), so a newly added
/// disk-backed `refresh_*` hub is wired in exactly one place.
fn reload_all_from_disk(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    tray_weak: slint::Weak<AppTray>,
) {
    load_server_into_ui(app);
    reload_presets(app, state, None);
    refresh_file_options(app, state);
    // Reset variant: F5's whole point is "back to disk", and the caller sits
    // behind the integrations_dirty discard guard.
    refresh_integrations_reset(app);
    refresh_run_status(app.as_weak(), tray_weak, true);
    spawn_version_probe(app.as_weak());
    spawn_device_probe(app.as_weak());
}

/// Reload `presets.ini` into `state`, rebuild the (filtered) list model, then
/// pick the selection and apply its form:
/// - `want = Some(id)` selects that preset if it exists (used after save/clone/rename);
/// - `want = None` keeps the current preset — matched by the loaded form's ID
///   first (a hand-edited presets.ini may have shifted the indices), falling
///   back to the old index when the id is gone;
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
    let cur_id = s.get_form().id;
    let idx = match want {
        Some(id) => all.iter().position(|p| p.id == id).map(|i| i as i32),
        None => all
            .iter()
            .position(|p| !cur_id.is_empty() && p.id == cur_id.as_str())
            .map(|i| i as i32)
            .or_else(|| (prev_sel >= 0 && (prev_sel as usize) < all.len()).then_some(prev_sel)),
    }
    .unwrap_or(if all.is_empty() { -1 } else { 0 });

    state.borrow_mut().presets = all;
    s.set_selected_preset_index(idx);

    let st = state.borrow();
    match usize::try_from(idx).ok().and_then(|i| st.presets.get(i)) {
        Some(p) => apply_form(app, preset_to_form(p)),
        None => apply_form(app, PresetForm::default()),
    }
}

/// Rebuild every file-backed dropdown (model / mmproj / the unified draft
/// picker) from a fresh `ModelsDir` scan, then cascade into the device dropdowns
/// (`refresh_device_options`) and the Model-info box (`models_tab::update_model_info`).
/// Call after anything that changes `models_dir`, a form file field, or the
/// selected preset — this is the hub a newly-added file-backed dropdown extends.
fn refresh_file_options(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    // SAVED config, not the live form: like every other client-facing
    // projection (chat URL, Command Line card), the scans must agree with
    // what a launch would use — an unsaved ModelsDir edit must not make the
    // pickers list models the server won't find. The Server tab's Save
    // handler re-runs this hub, so the scans follow the file.
    let models_dir = server_cfg::load().models_dir_or_default();
    let form = s.get_form();

    // Note: the trailing update_model_info() re-walks mtps\ / dflashs\ on its
    // own (gguf::external_drafters) — a cheap directory listing, kept so that
    // helper stays self-contained for its other callers (model_changed,
    // draft_picked), which have no scan in hand.
    let model_scan_result = model_scan::list(&models_dir, model_scan::Category::Model.subdir());
    let mmproj_scan_result = model_scan::list(&models_dir, model_scan::Category::Mmproj.subdir());
    let mtp_scan_result = model_scan::list(&models_dir, model_scan::Category::Mtp.subdir());
    let dflash_scan_result = model_scan::list(&models_dir, model_scan::Category::Dflash.subdir());

    let (lbl, val, idx) = scanned_options(
        model_scan::Category::Model,
        model_scan_result,
        form.model.as_str(),
    );
    s.set_model_labels(lbl);
    s.set_model_values(val);
    s.set_model_index(idx);

    let (lbl, val, idx) = scanned_options(
        model_scan::Category::Mmproj,
        mmproj_scan_result,
        form.mmproj.as_str(),
    );
    s.set_mmproj_labels(lbl);
    s.set_mmproj_values(val);
    s.set_mmproj_index(idx);
    // Draft picker: MTP heads (mtps\) and DFlash drafters (dflashs\) share one
    // dropdown (both feed --model-draft). Only the labels + index go to Slint;
    // the (value, spec) rows the pick policy needs stay in `State.draft_rows`.
    let (draft_labels, draft_values, draft_specs, draft_idx) = model_scan::build_draft_options(
        mtp_scan_result,
        dflash_scan_result,
        form.model_draft.as_str(),
        form.spec_type.as_str(),
    );
    s.set_draft_labels(string_model(draft_labels));
    s.set_draft_index(draft_idx);
    state.borrow_mut().draft_rows = draft_values.into_iter().zip(draft_specs).collect();

    refresh_device_options(app);
    models_tab::update_model_info(app);
}

/// Rebuild the two GPU-device dropdowns — the draft device and the image
/// encoder's — from the cached `--list-devices` result (`devices::probed()`),
/// recomputing each selected index against the current server.ini / form values.
/// The MAIN device is not a dropdown: it can name several GPUs in split order, so
/// it rides the GPU distribution table instead (`refresh_gpu_rows`).
fn refresh_device_options(app: &AppWindow) {
    let s = app.global::<AppState>();
    let devs = devices::probed();
    let form = s.get_form();
    let mmproj_device = s.get_server_form().mmproj_device;

    let (lbl, val, idx) = device_options(&devs, mmproj_device.as_str(), "(default: first GPU)");
    s.set_mmproj_dev_labels(lbl);
    s.set_mmproj_dev_values(val);
    s.set_mmproj_dev_index(idx);

    let (lbl, val, idx) =
        device_options(&devs, form.device_draft.as_str(), "(auto / same as model)");
    s.set_preset_draft_dev_labels(lbl);
    s.set_preset_draft_dev_values(val);
    s.set_preset_draft_dev_index(idx);

    refresh_gpu_rows(app);
}

/// Rebuild BOTH GPU distribution tables (server-wide + per-preset) from the
/// device probe crossed with each form's `device` / `tensor_split` strings.
///
/// Always a full rebuild of the row model, never an in-place row edit: the
/// delegates bind one-way (`checked: row.enabled`, `value: row.weight`) and a
/// click self-assigns them, so only fresh delegates are guaranteed to show the
/// truth. That is affordable because it is not on the weight-typing path — a
/// weight edit changes no other row (see GpuSplitTable's binding note), so its
/// handler updates the derived scalars and leaves the model alone.
fn refresh_gpu_rows(app: &AppWindow) {
    let s = app.global::<AppState>();
    let devs = devices::probed();
    s.set_server_gpu_rows(model(gpu_split::build_rows(&devs, &server_selection(app))));
    s.set_preset_gpu_rows(model(gpu_split::build_rows(&devs, &preset_selection(app))));
    refresh_gpu_scalars(app);
}

/// The derived numbers the tables render off: how many devices are checked, the
/// weight sum (0 = Auto, and the Share column's denominator), and the flags line.
/// Separate from `refresh_gpu_rows` because the weight-edit path needs THESE
/// without the row rebuild that would recreate the SpinBox being typed into.
fn refresh_gpu_scalars(app: &AppWindow) {
    let s = app.global::<AppState>();

    let server = server_selection(app);
    s.set_server_gpu_selected(gpu_count(&server));
    s.set_server_gpu_total(gpu_weight_total(&server));
    s.set_server_gpu_summary(gpu_split::summary(&server).into());

    let preset = preset_selection(app);
    s.set_preset_gpu_selected(gpu_count(&preset));
    s.set_preset_gpu_total(gpu_weight_total(&preset));
    s.set_preset_gpu_summary(gpu_split::summary(&preset).into());
}

fn gpu_count(sel: &gpu_split::GpuSelection) -> i32 {
    i32::try_from(gpu_split::parse_device_list(&sel.device).len()).unwrap_or(i32::MAX)
}

fn gpu_weight_total(sel: &gpu_split::GpuSelection) -> i32 {
    gpu_split::parse_weights(&sel.tensor_split).iter().sum()
}

// The selection IS the form's `device` + `tensor_split` pair — there is no third
// copy of it, so a hand-edited INI stays authoritative and nothing can desync.
// The four `*_gpu_*` callbacks per tab all follow the same two steps: derive the
// new selection with `gpu_split`, then `set_*_selection` + a refresh.

pub(crate) fn server_selection(app: &AppWindow) -> gpu_split::GpuSelection {
    let f = app.global::<AppState>().get_server_form();
    gpu_split::GpuSelection {
        device: f.device.to_string(),
        tensor_split: f.tensor_split.to_string(),
    }
}

pub(crate) fn preset_selection(app: &AppWindow) -> gpu_split::GpuSelection {
    let f = app.global::<AppState>().get_form();
    gpu_split::GpuSelection {
        device: f.device.to_string(),
        tensor_split: f.tensor_split.to_string(),
    }
}

pub(crate) fn set_server_selection(app: &AppWindow, sel: &gpu_split::GpuSelection) {
    let s = app.global::<AppState>();
    let mut f = s.get_server_form();
    f.device = sel.device.clone().into();
    f.tensor_split = sel.tensor_split.clone().into();
    s.set_server_form(f);
}

pub(crate) fn set_preset_selection(app: &AppWindow, sel: &gpu_split::GpuSelection) {
    let s = app.global::<AppState>();
    let mut f = s.get_form();
    f.device = sel.device.clone().into();
    f.tensor_split = sel.tensor_split.clone().into();
    s.set_form(f);
}

/// A dropdown's `(labels, values, index)` as Slint models, ready to hand to the
/// three matching `set_*` accessors. The two builders below wrap the raw
/// `Vec<String>` triples from `devices` / `model_scan` so each call site is three
/// plain `s.set_*` lines against the `AppState` it already holds.
type OptionModels = (ModelRc<SharedString>, ModelRc<SharedString>, i32);

fn device_options(
    devs: &[devices::DeviceOption],
    current: &str,
    empty_label: &str,
) -> OptionModels {
    let (labels, values, idx) = devices::build_options(devs, current, empty_label);
    (string_model(labels), string_model(values), idx)
}

// (The raw device list itself lives in `devices::probed()` — a Rust-side cache,
// not Slint state, because no .slint file ever reads it.)

fn scanned_options(
    category: model_scan::Category,
    scanned: Vec<model_scan::FileOption>,
    current: &str,
) -> OptionModels {
    let (labels, values, idx) = model_scan::build_options(category, scanned, current);
    (string_model(labels), string_model(values), idx)
}

fn bind_options(current: &str) -> OptionModels {
    let (labels, values, idx) = net_ifaces::build_options(&net_ifaces::interfaces(), current);
    (string_model(labels), string_model(values), idx)
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
/// and can take a few hundred ms — including a CUDA init), park the result in
/// `devices::probed()`, then rebuild the device dropdowns via the event loop.
fn spawn_device_probe(app_weak: slint::Weak<AppWindow>) {
    std::thread::spawn(move || {
        devices::set_probed(devices::list());
        slint::invoke_from_event_loop(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            // Recompute the dropdown indices now that the device list exists.
            refresh_device_options(&app);
        })
        .ok();
    });
}

/// Superseding counter for run-state probes: bumped on the UI thread whenever a
/// start/stop transition begins or lands. `refresh_run_status` stamps its
/// sample with the generation it saw and its apply closure discards a stale
/// one — otherwise a slow periodic `tasklist` sampled *before* a transition
/// could apply *after* it and flip the footer/tray back for up to a tick.
static RUN_STATUS_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn bump_run_status_gen() {
    RUN_STATUS_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// Start llama-server without blocking the UI thread (`runstate::start` opens
/// with a `tasklist` probe — hundreds of ms — before the spawn itself) and
/// drive the transitional "Starting…" state, mirroring `stop_server_async`.
/// The single canonical start path shared by the Server tab's Start button and
/// the tray menu, so a failed start reports identically from either surface. On
/// error we set the footer's error flag directly rather than calling
/// `refresh_run_status` (which clears that flag as it re-probes) — the mistake
/// the two hand-rolled copies used to make differently.
fn start_server_async(app_weak: slint::Weak<AppWindow>, tray_weak: slint::Weak<AppTray>) {
    if let Some(app) = app_weak.upgrade() {
        let s = app.global::<AppState>();
        // Re-entry guard: the window's buttons hide during a transition, but
        // the tray menu doesn't — a second click mid-transition is a no-op.
        if s.get_server_starting() || s.get_server_stopping() {
            return;
        }
        s.set_server_starting(true);
        set_status(&app, "Starting llama-server…".into(), false);
    }
    bump_run_status_gen();
    std::thread::spawn(move || {
        let result = runstate::start();
        // Snapshot the client URL from the config `start()` ACTUALLY launched
        // with (returned, not re-loaded — a save landing mid-start must not
        // leak into the snapshot): the RUNNING server keeps listening there
        // even if a later save changes server.ini, so Open-chat must prefer it.
        // `Ok(None)` = already running: WE launched nothing, so there is no
        // launch config to pin — leave launched_url alone (per its contract,
        // it stays empty for a server started outside the GUI).
        let launched_url = result
            .as_ref()
            .ok()
            .and_then(|launched| launched.as_ref())
            .map(client_base_url);
        slint::invoke_from_event_loop(move || {
            bump_run_status_gen();
            let running = result.is_ok();
            if let Some(app) = app_weak.upgrade() {
                let s = app.global::<AppState>();
                s.set_server_starting(false);
                s.set_server_running(running);
                if let Some(url) = &launched_url {
                    s.set_launched_url(SharedString::from(url.as_str()));
                }
                match &result {
                    Ok(Some(_)) => {
                        s.set_server_status_is_error(false);
                        set_status(&app, "llama-server started.".into(), false);
                    }
                    Ok(None) => {
                        s.set_server_status_is_error(false);
                        set_status(&app, "llama-server is already running.".into(), false);
                    }
                    Err(e) => {
                        s.set_server_status_is_error(true);
                        set_status(&app, format!("Failed to start: {e}"), true);
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

/// Trigger a stop and drive the transitional "Stopping…" state.
///
/// The forced `taskkill` returns quickly, but the process can linger in
/// `tasklist` for a second or two while its GPU context unwinds — so the kill
/// and the wait-for-exit run off the UI thread and `server_stopping` stays true
/// until the process actually disappears. The wait is capped so a wedged
/// process can't pin the UI in "Stopping…" forever.
fn stop_server_async(app_weak: slint::Weak<AppWindow>, tray_weak: slint::Weak<AppTray>) {
    if let Some(app) = app_weak.upgrade() {
        let s = app.global::<AppState>();
        // Same re-entry guard as start: the tray menu keeps offering "Stop
        // server" for the whole (up to 15 s) stop wait — without it, each
        // extra click spawns another taskkill + wait thread and double-bumps
        // the status generation.
        if s.get_server_starting() || s.get_server_stopping() {
            return;
        }
        s.set_server_stopping(true);
        set_status(&app, "Stopping llama-server…".into(), false);
    }
    bump_run_status_gen();
    std::thread::spawn(move || {
        runstate::stop();
        let mut running = runstate::is_running();
        let step = std::time::Duration::from_millis(300);
        let cap = std::time::Duration::from_secs(15);
        let mut waited = std::time::Duration::ZERO;
        while running && waited < cap {
            std::thread::sleep(step);
            waited += step;
            running = runstate::is_running();
        }
        slint::invoke_from_event_loop(move || {
            bump_run_status_gen();
            if let Some(app) = app_weak.upgrade() {
                let s = app.global::<AppState>();
                s.set_server_stopping(false);
                s.set_server_running(running);
                if !running {
                    // No process to point Open-chat at any more.
                    s.set_launched_url(SharedString::default());
                }
                // `stop()` can't fail; the re-polled run state is the outcome.
                if running {
                    s.set_server_status_is_error(true);
                    set_status(
                        &app,
                        "Stop timed out — llama-server is still running.".into(),
                        true,
                    );
                } else {
                    s.set_server_status_is_error(false);
                    set_status(&app, "llama-server stopped.".into(), false);
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
/// `spawn_version_probe`. `clear_error` decides whether the footer's error flag
/// is reset: the reload-from-disk actions (startup, F5, Refresh) pass `true`;
/// the periodic status timer passes `false` so it can't silently wipe the red
/// state a failed start / stop-timeout deliberately set. (The start/stop paths
/// don't call this — they manage the error flag directly.)
fn refresh_run_status(
    app_weak: slint::Weak<AppWindow>,
    tray_weak: slint::Weak<AppTray>,
    clear_error: bool,
) {
    std::thread::spawn(move || {
        let sampled_gen = RUN_STATUS_GEN.load(std::sync::atomic::Ordering::SeqCst);
        let running = runstate::is_running();
        slint::invoke_from_event_loop(move || {
            // A start/stop transition began or landed after this sample was
            // taken — its result is authoritative, ours is stale. Drop it.
            if RUN_STATUS_GEN.load(std::sync::atomic::Ordering::SeqCst) != sampled_gen {
                return;
            }
            if let Some(app) = app_weak.upgrade() {
                let s = app.global::<AppState>();
                let was_running = s.get_server_running();
                s.set_server_running(running);
                if !running {
                    // The server stopped outside the GUI — drop the stale
                    // launch snapshot so a future Open-chat falls back to the
                    // saved config.
                    s.set_launched_url(SharedString::default());
                }
                if clear_error {
                    s.set_server_status_is_error(false);
                }
                // A running→stopped flip with no transition in flight is an
                // external kill or a later crash: `start()` watches a grace
                // window and reports an immediate launch death itself, but a
                // process that dies LATER (an external taskkill, or a crash once
                // the model is loaded) only shows up on this tick — surface it
                // instead of silently flipping the footer to "Stopped" under a
                // status line still saying "llama-server started.". Neutral
                // wording — a crash and an external taskkill look the same here.
                if was_running && !running && !s.get_server_starting() && !s.get_server_stopping() {
                    s.set_server_status_is_error(true);
                    set_status(
                        &app,
                        format!(
                            "llama-server is no longer running — see {}",
                            crate::paths::server_log().display()
                        ),
                        true,
                    );
                }
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
    // The GPU table is a projection of form.device / form.tensor_split, so a form
    // it didn't write (preset switch, Revert, reload) has to be re-projected —
    // and as a full rebuild, since the delegates' one-way bindings don't survive
    // a click. Same reason load_server_into_ui rebuilds after seeding its form.
    refresh_gpu_rows(app);
}

// ── Integrations helpers ──────────────────────────────────────────────

/// Rebuild the Integrations tab from disk: the opencode.json base URL + the
/// per-preset toggle list + the Claude Code env snippet, all derived from
/// server.ini (port/host) and presets.ini. Call after any change to those.
///
/// This variant MERGES: ids already in the UI model keep their in-UI enabled
/// flag (a pending, unsaved toggle), only new ids take the on-disk value — so
/// the preset/server write paths (save/rename/clone/delete, server save) don't
/// silently wipe pending toggles the F5 guard would have asked about. For the
/// paths whose meaning IS "back to disk", use `refresh_integrations_reset`.
fn refresh_integrations(app: &AppWindow) {
    rebuild_integrations(app, true);
}

/// Reset-to-disk variant of `refresh_integrations`: pending row toggles are
/// dropped. Used by the startup seed / F5 (`reload_all_from_disk`, which sits
/// behind the `integrations_dirty` discard guard) and the Integrations tab's
/// own Save/Revert.
fn refresh_integrations_reset(app: &AppWindow) {
    rebuild_integrations(app, false);
}

fn rebuild_integrations(app: &AppWindow, keep_pending: bool) {
    let s = app.global::<AppState>();
    let base_url = opencode_base_url(&server_cfg::load());

    let all_presets = presets::load_all();
    let example = all_presets.first().map(|p| p.id.as_str());
    let claude_env = integrations::claude_code_env_script(&base_url, example);
    s.set_integration_claude_env(SharedString::from(claude_env));
    s.set_integration_base_url(SharedString::from(base_url));

    s.set_integration_provider_active(integrations::detect_opencode_provider());

    // Either way the ModelRc is REPLACED, never patched row-by-row — the row
    // CheckBox's one-way binding contract requires fresh `for` delegates on
    // every non-widget-originated change (see gui/integrations_tab.rs).
    let pending: std::collections::BTreeMap<String, bool> = if keep_pending {
        s.get_integration_models()
            .iter()
            .map(|m| (m.id.to_string(), m.enabled))
            .collect()
    } else {
        Default::default()
    };
    let enabled_ids = integrations::opencode_model_ids();
    let items: Vec<IntegrationModel> = all_presets
        .iter()
        .map(|p| IntegrationModel {
            id: SharedString::from(p.id.clone()),
            label: SharedString::from(integrations::friendly_model_name(&p.id, &p.model)),
            enabled: pending
                .get(&p.id)
                .copied()
                .unwrap_or_else(|| enabled_ids.contains(&p.id)),
        })
        .collect();
    s.set_integration_models(model(items));
}

/// Unsaved Integrations edits. The row toggles have no dirty flag like the two
/// forms (they live only in the UI model), so compare the enabled set against
/// the on-disk opencode.json instead. Consulted by the F5/Refresh discard
/// guard — `reload_all_from_disk` rebuilds the list and would otherwise wipe
/// pending toggles without the confirmation the form tabs get.
pub(crate) fn integrations_dirty(app: &AppWindow) -> bool {
    let enabled_ids = integrations::opencode_model_ids();
    app.global::<AppState>()
        .get_integration_models()
        .iter()
        .any(|m| m.enabled != enabled_ids.iter().any(|id| id == m.id.as_str()))
}

/// The opencode provider base URL derived from the SAVED server.ini — the
/// client_host, not the bind hostname: 0.0.0.0 is a listen address, a client
/// pointed at it gets a connection error, so it maps to localhost.
fn opencode_base_url(cfg: &server_cfg::ServerConfig) -> String {
    format!("{}/v1", client_base_url(cfg))
}

/// Keep opencode.json in step with a preset rename (`new_id = Some`) or delete
/// (`None`): when the old id is exposed as a model there, rewrite the models
/// list with it renamed / dropped — otherwise OpenCode keeps offering an id
/// `llama-server --models-preset` no longer knows, and the stale entry isn't
/// even visible in the Integrations tab (its rows are built from presets). A
/// no-op when the id wasn't exposed, so this can never create the provider
/// section as a side effect. A failure only flags the footer — the file
/// self-heals on the next Integrations save.
fn sync_opencode_after_preset_change(app: &AppWindow, old_id: &str, new_id: Option<&str>) {
    let ids = integrations::opencode_model_ids();
    if !ids.iter().any(|i| i == old_id) {
        return;
    }
    let checked: Vec<String> = ids
        .iter()
        .filter(|i| i.as_str() != old_id)
        .cloned()
        .chain(new_id.map(str::to_string))
        .collect();
    let base_url = opencode_base_url(&server_cfg::load());
    if let Err(e) = integrations::save_opencode_models(&checked, &base_url) {
        set_status(app, format!("opencode.json update failed: {e}"), true);
    }
}

// Pure-struct tests (PresetSummary is a plain generated struct — no Slint
// backend needed, same class as models_tab's apply_draft_pick tests).
#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: &str, model: &str) -> presets::Preset {
        presets::Preset {
            id: id.into(),
            model: model.into(),
            ..Default::default()
        }
    }

    // The invariant select_preset() depends on: a filtered row's `orig_index`
    // points into the UNFILTERED presets vector (the list `for` index is not a
    // stable handle once rows are dropped).
    #[test]
    fn preset_summaries_keep_orig_index_under_filter() {
        let all = vec![
            p("alpha", r"C:\m\models\a.gguf"),
            p("bravo", r"C:\m\models\qwen3.gguf"),
            p("charlie", r"C:\m\models\c.gguf"),
        ];

        // Filter matches case-insensitively on the id OR the model path.
        let rows = preset_summaries(&all, "QWEN");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id.as_str(), "bravo");
        assert_eq!(rows[0].orig_index, 1, "index into the unfiltered vector");

        let rows = preset_summaries(&all, "charlie");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].orig_index, 2);

        // No filter: everything, in order, indices dense.
        let rows = preset_summaries(&all, "");
        assert_eq!(rows.len(), 3);
        assert!(rows
            .iter()
            .enumerate()
            .all(|(i, r)| r.orig_index == i as i32));

        // No match: empty list (the UI then shows -1 / a blank form).
        assert!(preset_summaries(&all, "zzz").is_empty());
    }
}
