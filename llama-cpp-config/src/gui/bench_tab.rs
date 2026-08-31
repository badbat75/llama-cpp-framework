//! Benchmark-tab callback wiring. The planning and aggregation live in
//! `crate::bench` (pure, unit-tested), the running in `crate::bench::exec`; this
//! file is the seam between them and `AppState`.
//!
//! Three things worth knowing before editing here.
//!
//! **The selection is an ORDERED list of preset ids, kept Rust-side**
//! (`State.bench_selection`), not a set derived from the row model. Order is
//! meaning: position 1 is the baseline every ratio in the results table is
//! measured against, so checking a box APPENDS and unchecking closes the gap.
//! Every change rebuilds the whole row model, which is what keeps the rows'
//! one-way `checked` bindings honest (the same contract as the GPU table's).
//!
//! **The synthetic mode stops llama-server itself**, through
//! `gui::stop_server_async_with`, and never through `runstate::stop`: that path
//! also saves the conversation snapshot when the user has asked for one, and it
//! is what keeps the footer, the tray and the run-status generation counter in
//! step. Killing the server from under them would make the next periodic tick
//! report a deliberate stop as "llama-server is no longer running", in red. The
//! server is NOT restarted afterwards: the run's closing status says it is down,
//! and starting it again is the user's call.
//!
//! **The preview is built by the code that builds the real thing**
//! (`bench::synthetic_argv` + `runstate::render_command_line`, or
//! `bench::live_body`), never by a second formatter that could drift from it.
//! Same rule, and the same reason, as the Server tab's Command Line card.

use super::*;

use crate::bench::{self, exec, Mode, Plan};
use crate::ini;

/// Seed / re-seed everything the tab reads from disk: the preset rows, the list
/// of past runs, and the derived preview. Part of the `reload_all_from_disk` hub
/// (startup seed + Refresh/F5).
pub(super) fn refresh(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    // First seed only: leave a workload the user has edited alone across an F5.
    if s.get_bench_reps().is_empty() {
        s.set_bench_prompt(SharedString::from(bench::DEFAULT_PROMPT));
        s.set_bench_temp(SharedString::from("0"));
        s.set_bench_max_tokens(SharedString::from(bench::DEFAULT_MAX_TOKENS.to_string()));
        s.set_bench_reps(SharedString::from(bench::DEFAULT_REPS.to_string()));
        s.set_bench_prompt_lens(SharedString::from(bench::DEFAULT_PROMPT_LENS));
        s.set_bench_depths(SharedString::from(bench::DEFAULT_DEPTHS));
        s.set_bench_n_gen(SharedString::from(bench::DEFAULT_GEN.to_string()));
    }
    // A preset can be renamed or deleted under us; a selection naming one that
    // is gone would fail the run's preflight, so prune it here instead.
    let known: Vec<String> = presets::load_all().into_iter().map(|p| p.id).collect();
    state
        .borrow_mut()
        .bench_selection
        .retain(|id| known.contains(id));
    refresh_rows(app, state);
    refresh_runs(app);
}

/// Rebuild the preset row model from disk crossed with the selection, then
/// re-derive everything downstream of it (the preview, the baseline).
fn refresh_rows(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    let selection = state.borrow().bench_selection.clone();
    let rows: Vec<BenchPresetRow> = presets::load_all()
        .into_iter()
        .map(|p| {
            let pos = selection.iter().position(|id| id == &p.id);
            BenchPresetRow {
                id: SharedString::from(p.id),
                // The file's BASE NAME: the full path would elide to nothing,
                // and what matters here is whether two rows are the same file.
                model: SharedString::from(file_name(&p.model)),
                selected: pos.is_some(),
                order: pos.map_or(0, |i| i as i32 + 1),
            }
        })
        .collect();
    s.set_bench_presets(model(rows));
    s.set_bench_selected_count(selection.len() as i32);
    s.set_bench_baseline(SharedString::from(
        selection.first().cloned().unwrap_or_default(),
    ));
    refresh_preview(app, state);
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Rebuild the pasteable preview and the mode's caveat block.
fn refresh_preview(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    let plan = plan_from_ui(app, state);
    s.set_bench_caveats(SharedString::from(
        bench::caveats(plan.mode)
            .iter()
            .map(|c| format!("- {c}"))
            .collect::<Vec<_>>()
            .join("\n"),
    ));

    let cfg = server_cfg::load();
    let all = presets::load_all();
    let chosen: Vec<presets::Preset> = plan
        .presets
        .iter()
        .filter_map(|id| all.iter().find(|p| &p.id == id).cloned())
        .collect();
    if chosen.is_empty() {
        s.set_bench_preview(SharedString::from(
            "Select one or more presets above to see what will run.",
        ));
        return;
    }

    let text = match plan.mode {
        Mode::Synthetic => match paths::llama_bench_exe() {
            Some(exe) => {
                let env = runstate::env_vars(&cfg);
                let sweeps = bench::synthetic_sweeps(&plan);
                // One block per (preset, sweep): the prefill and decode halves
                // are separate invocations, and the preview has to show that or
                // it would misrepresent both the runtime and the test count.
                let mut blocks = Vec::new();
                for p in &chosen {
                    for sweep in &sweeps {
                        blocks.push(format!(
                            "# {} / {}\n{}",
                            p.id,
                            sweep.name,
                            runstate::render_command_line(
                                &exe.to_string_lossy(),
                                &bench::synthetic_argv(p, &cfg, &plan, sweep),
                                &env,
                            )
                        ));
                    }
                }
                blocks.join("\n\n")
            }
            None => "llama-bench.exe not found next to llama-server. Build llama.cpp first, \
                     or reinstall: the installer stages it into bin\\."
                .to_string(),
        },
        Mode::Live => {
            let port = u16::try_from(cfg.port_or_default()).unwrap_or(8080);
            let first = bench::live_preview(port, &chosen[0].id, &plan);
            if chosen.len() == 1 {
                first
            } else {
                // The bodies differ in one field, so repeating them whole would
                // bury that difference in noise.
                let rest: Vec<&str> = chosen[1..].iter().map(|p| p.id.as_str()).collect();
                format!(
                    "{first}\n\nThen the same request for: {} (only \"model\" differs).",
                    rest.join(", ")
                )
            }
        }
    };
    s.set_bench_preview(SharedString::from(text));
}

/// Re-list the saved runs, newest first, keeping the current pick if it is still
/// there.
fn refresh_runs(app: &AppWindow) {
    let s = app.global::<AppState>();
    let runs = bench::past_runs();
    let showing = s.get_bench_showing().to_string();
    let idx = runs
        .iter()
        .position(|(stamp, _)| stamp == &showing)
        .map_or(-1, |i| i as i32);
    s.set_bench_run_labels(string_model(
        runs.iter().map(|(st, _)| bench::stamp_label(st)).collect(),
    ));
    s.set_bench_run_values(string_model(
        runs.iter().map(|(st, _)| st.clone()).collect(),
    ));
    s.set_bench_run_index(idx);
}

/// Read the whole tab into a `Plan`. Numbers ride as text (the same reason every
/// other numeric field in this app does: a SpinBox edits itself on a stray
/// scroll), so an unparseable field falls back to its default rather than
/// failing the run.
fn plan_from_ui(app: &AppWindow, state: &Rc<RefCell<State>>) -> Plan {
    let s = app.global::<AppState>();
    let int = |v: SharedString, fallback: i32| ini::parse_int(v.as_str()).unwrap_or(fallback);
    let mode = Mode::from_str(s.get_bench_mode().as_str());
    Plan {
        mode,
        presets: state.borrow().bench_selection.clone(),
        reps: int(s.get_bench_reps(), bench::DEFAULT_REPS).max(1),
        prompt: s.get_bench_prompt().to_string(),
        // Checked = "leave the preset's own --temp alone", which is a third
        // instruction and not a missing one, so it must send NO temperature
        // rather than a number.
        temp: if s.get_bench_temp_default() {
            None
        } else {
            Some(ini::parse_float(s.get_bench_temp().as_str()).unwrap_or(0.0))
        },
        max_tokens: int(s.get_bench_max_tokens(), bench::DEFAULT_MAX_TOKENS),
        prompt_lens: bench::parse_int_list(s.get_bench_prompt_lens().as_str()),
        depths: bench::parse_int_list(s.get_bench_depths().as_str()),
        n_gen: int(s.get_bench_n_gen(), bench::DEFAULT_GEN),
    }
}

/// Push a result set into the table.
fn apply_points(app: &AppWindow, points: &[bench::Point], order: &[String]) {
    let rows: Vec<BenchResultRow> = bench::rows(points, order)
        .into_iter()
        .map(|r| BenchResultRow {
            test: SharedString::from(r.test),
            preset: SharedString::from(r.preset),
            mean: SharedString::from(r.mean),
            sd: SharedString::from(r.sd),
            samples: r.samples,
            ratio: SharedString::from(r.ratio),
            note: SharedString::from(r.note),
        })
        .collect();
    app.global::<AppState>().set_bench_rows(model(rows));
}

/// Start the run: clear the table, flip the running flag, and hand the plan to a
/// worker thread whose events come back through the event loop.
fn start_run(app: &AppWindow, state: &Rc<RefCell<State>>, plan: Plan) {
    let s = app.global::<AppState>();
    s.set_bench_running(true);
    s.set_bench_rows(model(Vec::<BenchResultRow>::new()));
    s.set_bench_showing(SharedString::default());
    s.set_bench_report_path(SharedString::default());
    s.set_bench_progress(SharedString::from("Starting…"));
    set_status(app, "Benchmark running…".into(), false);

    let order = plan.presets.clone();
    let mode = plan.mode;
    let app_weak = app.as_weak();
    let handle = exec::spawn(plan, move |event| {
        let app_weak = app_weak.clone();
        let order = order.clone();
        slint::invoke_from_event_loop(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let s = app.global::<AppState>();
            match event {
                exec::Event::Progress(text) => {
                    s.set_bench_progress(SharedString::from(text));
                }
                exec::Event::Points(points) => apply_points(&app, &points, &order),
                exec::Event::Done(result) => {
                    s.set_bench_running(false);
                    s.set_bench_progress(SharedString::default());
                    match result {
                        Ok(report) => {
                            s.set_bench_report_path(SharedString::from(
                                report.to_string_lossy().into_owned(),
                            ));
                            let tail = if mode == Mode::Synthetic {
                                " llama-server is still stopped; start it when you need it."
                            } else {
                                ""
                            };
                            set_status(
                                &app,
                                format!("Benchmark finished. Report: {}.{tail}", report.display()),
                                false,
                            );
                        }
                        Err(e) => set_status(&app, format!("Benchmark: {e}"), true),
                    }
                    refresh_runs(&app);
                }
            }
        })
        .ok();
    });
    state.borrow_mut().bench_handle = Some(handle);
}

/// `tray` is `None` only in the test seam (`gui::wire_tabs_for_tests`), which
/// builds no tray; the sole thing it gates is the automatic llama-server stop a
/// synthetic run needs, and in a test there is no server to stop.
pub(super) fn wire(
    app: &AppWindow,
    state: &Rc<RefCell<State>>,
    tray: Option<slint::Weak<AppTray>>,
) {
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_bench_mode_picked(move |value| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            // The whole workload card is kept across the switch (each mode
            // reads its own half), so this only re-derives what the mode
            // decides: the preview and the caveat block.
            app.global::<AppState>().set_bench_mode(value);
            refresh_preview(&app, &state);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_bench_toggle_preset(move |id| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            {
                let mut st = state.borrow_mut();
                let id = id.to_string();
                match st.bench_selection.iter().position(|x| x == &id) {
                    // Unchecking closes the gap, so the remaining rows keep a
                    // contiguous 1..n order and the baseline stays the top one.
                    Some(i) => {
                        st.bench_selection.remove(i);
                    }
                    // Checking APPENDS: the order is the run order, and the
                    // first is the baseline.
                    None => st.bench_selection.push(id),
                }
            }
            refresh_rows(&app, &state);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_bench_changed(move || {
            if let Some(app) = app_weak.upgrade() {
                refresh_preview(&app, &state);
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let tray_weak = tray.clone();
        let state = state.clone();
        app.global::<AppState>().on_bench_run(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            if app.global::<AppState>().get_bench_running() {
                return;
            }
            let plan = plan_from_ui(&app, &state);
            if let Some(problem) = bench::validate(&plan) {
                set_status(&app, problem, true);
                return;
            }
            // The synthetic engine cannot share the GPU with llama-server: a
            // second resident copy of the model spills into shared memory and
            // reads as a backend regression rather than as a mistake. So stop
            // it first, through the canonical path (which snapshots the
            // conversation when that is enabled), and run only once it is gone.
            let needs_stop =
                plan.mode == Mode::Synthetic && app.global::<AppState>().get_server_running();
            if needs_stop {
                let state = state.clone();
                let after = move |app: &AppWindow| {
                    start_run(app, &state, plan);
                };
                match tray_weak.clone() {
                    Some(tray) => {
                        stop_server_async_with(app.as_weak(), tray, Some(Box::new(after)))
                    }
                    // No tray only in the test seam, where nothing is running.
                    None => set_status(
                        &app,
                        "Stop llama-server before a synthetic benchmark.".into(),
                        true,
                    ),
                }
                return;
            }
            start_run(&app, &state, plan);
        });
    }
    {
        let state = state.clone();
        app.global::<AppState>().on_bench_cancel(move || {
            if let Some(h) = state.borrow().bench_handle.as_ref() {
                h.cancel();
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_bench_run_picked(move |stamp| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let s = app.global::<AppState>();
            let (jsonl, md, _log) = bench::run_paths(stamp.as_str());
            match bench::load_run(&jsonl) {
                Ok(loaded) => {
                    s.set_bench_showing(stamp.clone());
                    s.set_bench_baseline(SharedString::from(
                        loaded.presets.first().cloned().unwrap_or_default(),
                    ));
                    s.set_bench_report_path(SharedString::from(md.to_string_lossy().into_owned()));
                    apply_points(&app, &loaded.points, &loaded.presets);
                    set_status(
                        &app,
                        format!(
                            "Showing the {} run of {}.",
                            loaded.mode_label,
                            bench::stamp_label(stamp.as_str())
                        ),
                        false,
                    );
                }
                Err(e) => set_status(&app, format!("Cannot read that run: {e}"), true),
            }
        });
    }
    {
        app.global::<AppState>().on_bench_open_folder(|| {
            let dir = bench::bench_dir();
            // Created on demand by a run, so a machine that has never benched
            // has no folder to open yet.
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::process::Command::new("explorer").arg(dir).spawn();
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_bench_open_report(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let path = app.global::<AppState>().get_bench_report_path();
            if !path.is_empty() {
                // `explorer <file>` hands it to the default handler, the same
                // one-liner the Integrations tab uses for its folder.
                let _ = std::process::Command::new("explorer")
                    .arg(path.as_str())
                    .spawn();
            }
        });
    }
}
