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
//! the new-preset dialog's model scan, and the draft dropdown's (value, spec)
//! rows. (The GPU-device probe result is cached in `devices::probed()` — it is
//! process-wide, set once from the probe thread.)
//!
//! ## Helper verb taxonomy (so a grep lands in the right family)
//! - `load_*`            : server.ini ↔ UI (through `server_form::config_to_form`
//!   / `form_to_config`; the preset side is `form.rs`).
//! - `refresh_*` / `reload_*` : rebuild a list/section from disk or the device
//!   cache (`reload_presets`, `refresh_file_options`, `refresh_device_options`,
//!   `refresh_integrations`, `refresh_run_status`, `refresh_server_snapshot`;
//!   `reload_all_from_disk` is the everything-at-once hub shared by startup and
//!   the Refresh button).
//! - `*_options`         : build a dropdown's (labels, values, index) model
//!   triple the caller hands to the matching `set_*` accessors (`device_options`,
//!   `scanned_options`).
//! - `apply_form`        : push a whole `PresetForm` + its baseline into `AppState`.
//! - `populate_*`        : fill a dropdown's parallel option arrays in place
//!   (`populate_bind_options` — the Server tab's bind-address list).
//! - `start_server` / `stop_server_async` : the canonical run-control paths shared
//!   by the Server tab and the tray, so both surfaces report a start/stop identically.
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
//! through `slint::invoke_from_event_loop` guarded by `Weak::upgrade`.
//!
//! `AppTray` is a SEPARATE Slint root — it does NOT use `AppState`; Rust pushes
//! state to it directly (`tray.set_server_running` / `tray.on_*`).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::form::{form_to_preset, preset_to_form};
use crate::{
    devices, integrations, model_scan, net_ifaces, paths, presets, runstate, server_cfg,
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
    // (--model-draft value, --spec-type) per draft-dropdown row, parallel to
    // AppState.draft_labels. Rust-only data (only `draft_picked` reads it), so
    // it lives here instead of as published Slint arrays no page ever binds.
    draft_rows: Vec<(String, String)>,
}

/// TEST-ONLY seam for the e2e save-flow test (src/tests/save_flow.rs): build the
/// shared `State`, seed the preset list from disk, and wire the Models tab —
/// nothing else (no tray, no probes, no single-instance, no event loop). The
/// caller must have redirected config IO first (see `paths::data_root`).
#[cfg(test)]
pub(crate) fn wire_models_tab_for_tests(app: &AppWindow) {
    let state = Rc::new(RefCell::new(State::default()));
    reload_presets(app, &state, None);
    models_tab::wire(app, &state);
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
    spawn_version_probe(app.as_weak());
    spawn_device_probe(app.as_weak());

    s.set_presets_path(SharedString::from(
        paths::presets_ini().to_string_lossy().into_owned(),
    ));

    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        s.on_refresh_status(move || {
            // Explicit user refresh (F5): also clear a stale error state.
            refresh_run_status(app_weak.clone(), tray_weak.clone(), true);
        });
    }

    {
        let app_weak = app.as_weak();
        let tray_weak = tray.as_weak();
        let state = state.clone();
        s.on_reload_all(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            reload_all_from_disk(&app, &state, tray_weak.clone());
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
    s.set_chat_url(SharedString::from(format!(
        "http://{}:{}",
        cfg.client_host(),
        cfg.port_or_default()
    )));
}

fn populate_bind_options(app: &AppWindow, current: &str) {
    let s = app.global::<AppState>();
    let (labels, values, index) = bind_options(current);
    s.set_bind_labels(labels);
    s.set_bind_values(values);
    s.set_bind_index(index);
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
/// the ModelsDir scan, the integration state, and the live run status. The one
/// home shared by `run()`'s startup seed and the Refresh button (`reload_all`),
/// so a newly added disk-backed `refresh_*` hub is wired in exactly one place.
fn reload_all_from_disk(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    tray_weak: slint::Weak<AppTray>,
) {
    load_server_into_ui(app);
    reload_presets(app, state, None);
    refresh_file_options(app, state);
    refresh_integrations(app);
    refresh_run_status(app.as_weak(), tray_weak, true);
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
    let models_dir = s.get_server_form().models_dir.to_string();
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

/// Rebuild the three GPU-device dropdowns (server-wide, per-preset main,
/// per-preset draft) from the cached `--list-devices` result
/// (`devices::probed()`), recomputing each selected index against the current
/// server.ini / form values.
fn refresh_device_options(app: &AppWindow) {
    let s = app.global::<AppState>();
    let devs = devices::probed();
    let form = s.get_form();
    let server_device = s.get_server_form().device;

    let (lbl, val, idx) = device_options(devs, server_device.as_str(), "(all detected devices)");
    s.set_server_dev_labels(lbl);
    s.set_server_dev_values(val);
    s.set_server_dev_index(idx);

    let (lbl, val, idx) = device_options(devs, form.device.as_str(), "(server default)");
    s.set_preset_dev_labels(lbl);
    s.set_preset_dev_values(val);
    s.set_preset_dev_index(idx);

    let (lbl, val, idx) =
        device_options(devs, form.device_draft.as_str(), "(auto / same as model)");
    s.set_preset_draft_dev_labels(lbl);
    s.set_preset_draft_dev_values(val);
    s.set_preset_draft_dev_index(idx);
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

/// Start llama-server, then reflect the outcome on both the window and the tray.
/// The single canonical start path shared by the Server tab's Start button and
/// the tray menu, so a failed start reports identically from either surface. On
/// error we set the footer's error flag directly rather than calling
/// `refresh_run_status` (which clears that flag as it re-probes) — the mistake
/// the two hand-rolled copies used to make differently.
fn start_server(app_weak: slint::Weak<AppWindow>, tray_weak: slint::Weak<AppTray>) {
    match runstate::start() {
        Ok(()) => {
            if let Some(app) = app_weak.upgrade() {
                set_status(&app, "llama-server started.".into(), false);
            }
            // Confirm the run state (and clear any stale error) off the UI thread.
            refresh_run_status(app_weak, tray_weak, true);
        }
        Err(e) => {
            if let Some(app) = app_weak.upgrade() {
                let s = app.global::<AppState>();
                set_status(&app, format!("Failed to start: {e}"), true);
                s.set_server_status_is_error(true);
                s.set_server_running(false);
            }
            if let Some(tray) = tray_weak.upgrade() {
                tray.set_server_running(false);
            }
        }
    }
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
            if let Some(app) = app_weak.upgrade() {
                let s = app.global::<AppState>();
                s.set_server_stopping(false);
                s.set_server_running(running);
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
/// is reset: explicit actions (startup, F5, Refresh, a new start) pass `true`;
/// the periodic status timer passes `false` so it can't silently wipe the red
/// state a failed start / stop-timeout deliberately set.
fn refresh_run_status(
    app_weak: slint::Weak<AppWindow>,
    tray_weak: slint::Weak<AppTray>,
    clear_error: bool,
) {
    std::thread::spawn(move || {
        let running = runstate::is_running();
        slint::invoke_from_event_loop(move || {
            if let Some(app) = app_weak.upgrade() {
                let s = app.global::<AppState>();
                s.set_server_running(running);
                if clear_error {
                    s.set_server_status_is_error(false);
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
}

// ── Integrations helpers ──────────────────────────────────────────────

/// Rebuild the Integrations tab from disk: the opencode.json base URL + the
/// per-preset toggle list + the Claude Code env snippet, all derived from
/// server.ini (port/host) and presets.ini. Call after any change to those.
fn refresh_integrations(app: &AppWindow) {
    let s = app.global::<AppState>();
    let cfg = server_cfg::load();
    // client_host, not the bind hostname: 0.0.0.0 is a listen address — a
    // client pointed at it gets a connection error, so map it to localhost.
    let base_url = format!("http://{}:{}/v1", cfg.client_host(), cfg.port_or_default());

    let all_presets = presets::load_all();
    let example = all_presets.first().map(|p| p.id.as_str());
    let claude_env = integrations::claude_code_env_script(&base_url, example);
    s.set_integration_claude_env(SharedString::from(claude_env));
    s.set_integration_base_url(SharedString::from(base_url));

    s.set_integration_provider_active(integrations::detect_opencode_provider());

    let enabled_ids = integrations::opencode_model_ids();
    let items: Vec<IntegrationModel> = all_presets
        .iter()
        .map(|p| IntegrationModel {
            id: SharedString::from(p.id.clone()),
            label: SharedString::from(integrations::friendly_model_name(&p.id, &p.model)),
            enabled: enabled_ids.contains(&p.id),
        })
        .collect();
    s.set_integration_models(model(items));
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
