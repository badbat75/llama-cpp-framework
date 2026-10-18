//! Benchmark-tab callback wiring. The planning and aggregation live in
//! `crate::bench` (pure, unit-tested), the running in `crate::bench::exec`; this
//! file is the seam between them and `AppState`.
//!
//! Four things worth knowing before editing here.
//!
//! **The prompt is a FILE and this tab never holds it**. The workload card
//! carries a PATH (settings.ini `BenchPromptFile`, defaulting to
//! `config\bench-prompt.txt`), Edit hands that file to the system's default
//! text editor, and the text is read at Run. Three consequences live in this
//! file: `refresh_prompt_info` is the only reader and caches its result
//! `(path, mtime, len)`-gated, because it is called from every keystroke AND
//! from the 5 s status tick (that tick is what makes an external edit show up
//! with no callback to hang off); `plan_from_ui` fills the plan from that
//! CACHE, which is fine for a preview and would not be for a measurement; and
//! `on_bench_run` therefore re-reads the file itself before validating. Editing
//! in a window of ours was the first design and was dropped: a Slint TextEdit
//! is already sluggish at a few hundred KB (`log_window.rs` shows its tail as a
//! list of rows for exactly that), and the prompts worth putting in a file are the
//! long ones.
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

use std::path::Path;
use std::time::SystemTime;

use crate::bench::{self, exec, Mode, Plan};
use crate::ini;

/// The last read of the live prompt file, cached in `State`.
///
/// It exists so `refresh_prompt_info` is free to call from anywhere: from a
/// keystroke in an unrelated workload field, and from the 5 s status tick that
/// makes an external edit show up on its own. Both would otherwise re-read a
/// file that can be megabytes. The cache is a display cache ONLY: the run
/// re-reads the file itself, so a stale entry can never be what gets
/// benchmarked.
pub(super) struct PromptRead {
    /// The path as the field held it, so retyping the field invalidates this.
    path: String,
    /// `(modified, len)`, so an edit made outside does too. `None` when the
    /// file does not exist, which is itself a state worth noticing a change
    /// from.
    stamp: Option<(Option<SystemTime>, u64)>,
    /// The normalized text, for the preview. Empty when the read failed.
    text: String,
}

fn file_stamp(path: &Path) -> Option<(Option<SystemTime>, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok(), meta.len()))
}

/// The prompt file the tab is pointed at: the field, or the default when it is
/// empty (which is what the field's placeholder promises).
fn configured_prompt_path(s: &AppState) -> std::path::PathBuf {
    let field = s.get_bench_prompt_file().to_string();
    if field.trim().is_empty() {
        paths::bench_prompt_file()
    } else {
        std::path::PathBuf::from(field.trim())
    }
}

/// Re-read the prompt file when it (or the path) has changed, refreshing the
/// readout under the field and the cached text the preview is built from.
/// Returns whether anything changed, so a caller that has to rebuild the
/// preview knows to.
pub(super) fn refresh_prompt_info(app: &AppWindow, state: &Rc<RefCell<State>>) -> bool {
    let s = app.global::<AppState>();
    let path = configured_prompt_path(&s);
    let key = path.to_string_lossy().into_owned();
    let stamp = file_stamp(&path);
    if let Some(prev) = state.borrow().bench_prompt.as_ref() {
        if prev.path == key && prev.stamp == stamp {
            return false;
        }
    }
    let (text, info) = match bench::load_prompt_file(&path) {
        Ok(t) => {
            let summary = bench::prompt_summary(&t);
            (t, summary)
        }
        // The error is the readout: "cannot read …" under the field is the
        // whole diagnosis, and Run would report the same sentence anyway.
        Err(e) => (String::new(), e),
    };
    s.set_bench_prompt_info(SharedString::from(info));
    state.borrow_mut().bench_prompt = Some(PromptRead {
        path: key,
        stamp,
        text,
    });
    true
}

/// Native file picker for the prompt, seeded at the current file. Blocks the UI
/// thread for the same reason the Server tab's folder picker does: a native
/// modal dialog is supposed to hold its owner (see `gui.rs`'s threading note).
///
/// Text files are the filter, "all files" the escape hatch: a prompt is plain
/// text but nothing says it has to be named `.txt`.
fn pick_prompt_file(start: &Path) -> Option<std::path::PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Pick a prompt file")
        .add_filter("Text files", &["txt", "md", "prompt"])
        .add_filter("All files", &["*"]);
    if let Some(parent) = start.parent().filter(|p| p.is_dir()) {
        dialog = dialog.set_directory(parent);
    }
    if let Some(name) = start.file_name().and_then(|n| n.to_str()) {
        dialog = dialog.set_file_name(name);
    }
    dialog.pick_file()
}

/// Persist the prompt file path into settings.ini, read-modify-write so this
/// cannot wipe another key. Called when the path is PICKED and when a run
/// starts, not on every keystroke: the write is an fsync (`ini::atomic_write`),
/// and one per typed character is not what durability is for.
fn persist_prompt_path(app: &AppWindow) {
    let s = app.global::<AppState>();
    let field = s.get_bench_prompt_file().to_string();
    let mut cfg = settings::load();
    if cfg.bench_prompt_file == field {
        return;
    }
    cfg.bench_prompt_file = field;
    if let Err(e) = settings::save(&cfg) {
        // The only reachable failure is a path carrying ';' or '#', which the
        // INI comment rule would truncate on reload. Say so: the run itself
        // still works, it just will not be remembered.
        set_status(app, format!("Prompt file not remembered: {e}"), true);
    }
}

/// The benchmark database the field names, or the default when it is empty:
/// what the Saved runs list reads. A run reads settings.ini instead
/// (`db::work_path`), which is why Run persists the field first.
fn configured_db_path(s: &AppState) -> std::path::PathBuf {
    settings::Settings {
        bench_db: s.get_bench_db().to_string(),
        ..settings::Settings::default()
    }
    .bench_db_path()
}

/// Native file picker for the database, seeded at the current file. Picks an
/// EXISTING file: a new database is a typed path, created by the first run.
fn pick_db_file(start: &Path) -> Option<std::path::PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Pick a benchmark database")
        .add_filter("SQLite databases", &["db", "sqlite", "sqlite3"])
        .add_filter("All files", &["*"]);
    if let Some(parent) = start.parent().filter(|p| p.is_dir()) {
        dialog = dialog.set_directory(parent);
    }
    dialog.pick_file()
}

/// Persist the database path into settings.ini, read-modify-write. Returns
/// false (with the reason on the status line) when it cannot be saved: the
/// only reachable failure is a `;`/`#` in the path, which would reload
/// truncated, and a run must then not start, or it would land in the database
/// settings.ini still names rather than the one on screen.
fn persist_db_path(app: &AppWindow) -> bool {
    let field = app.global::<AppState>().get_bench_db().trim().to_string();
    let mut cfg = settings::load();
    if cfg.bench_db.trim() == field {
        return true;
    }
    cfg.bench_db = field;
    match settings::save(&cfg) {
        Ok(()) => true,
        Err(e) => {
            set_status(app, format!("Benchmark database not saved: {e}"), true);
            false
        }
    }
}

/// Seed / re-seed everything the tab reads from disk: the preset rows, the list
/// of past runs, and the derived preview. Part of the `reload_all_from_disk` hub
/// (startup seed + Refresh/F5).
pub(super) fn refresh(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    // First seed only: leave a workload the user has edited alone across an F5.
    // The prompt PATH is disk-backed (settings.ini) unlike the rest of the
    // workload, and is seeded here for the same reason the others are: an F5
    // must not throw away a path typed but not yet run. Its FILE is re-read on
    // every refresh below, which is the half that matters.
    if s.get_bench_reps().is_empty() {
        let configured = settings::load().bench_prompt_path();
        // Both shipped prompts, not just the configured one: the long one has
        // to be THERE for Browse… to find it next door.
        bench::ensure_shipped_prompt_files();
        bench::ensure_prompt_file(&configured);
        s.set_bench_prompt_file(SharedString::from(
            configured.to_string_lossy().into_owned(),
        ));
        // As stored, not resolved: empty stays empty (the default), so saving
        // the field back never pins today's data folder into settings.ini.
        s.set_bench_db(SharedString::from(settings::load().bench_db.trim()));
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
    // Before the rows: `refresh_rows` ends in the preview, which is built from
    // the cached prompt text this fills in.
    refresh_prompt_info(app, state);
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
pub(super) fn refresh_preview(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    let plan = plan_from_ui(app, state);
    s.set_bench_caveats(SharedString::from(
        bench::caveats(plan.mode)
            .iter()
            .map(|c| format!("- {c}"))
            .collect::<Vec<_>>()
            .join("\n"),
    ));
    // The preview is "exactly what Run will do", and Run will refuse.
    let bad = invalid_numbers(&s, plan.mode);
    if !bad.is_empty() {
        s.set_bench_preview(SharedString::from(format!(
            "Run will refuse: {}.",
            bad.join("; ")
        )));
        return;
    }

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
            None => format!(
                "{} not found next to llama-server. Build llama.cpp first, \
                 or reinstall: the installer stages it into bin\\.",
                crate::paths::binary_name("llama-bench")
            ),
        },
        Mode::Live => {
            let port = u16::try_from(cfg.port_or_default())
                .unwrap_or(crate::server_cfg::DEFAULT_PORT as u16);
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

/// The campaigns in the database at `path`, newest first, with the readout
/// under the Database field. A missing file is not an error: it is a machine
/// that has never benched, or a new database the next run creates.
fn saved_campaigns(path: &Path) -> (Vec<bench::db::CampaignRow>, String) {
    if !path.is_file() {
        return (
            Vec::new(),
            format!(
                "{} (not there yet: the first finished run creates it)",
                path.display()
            ),
        );
    }
    let read =
        bench::db::open_ro(path).and_then(|c| bench::db::campaigns(&c).map_err(|e| e.to_string()));
    match read {
        Ok(runs) => {
            let info = format!(
                "{} ({} campaign{})",
                path.display(),
                runs.len(),
                if runs.len() == 1 { "" } else { "s" }
            );
            (runs, info)
        }
        Err(e) => (Vec::new(), format!("cannot read: {e}")),
    }
}

/// Re-list the saved campaigns, newest first, keeping the current pick if it
/// is still there.
fn refresh_runs(app: &AppWindow) {
    let s = app.global::<AppState>();
    let (runs, info) = saved_campaigns(&configured_db_path(&s));
    s.set_bench_db_info(SharedString::from(info));
    let showing = s.get_bench_showing().to_string();
    let idx = runs
        .iter()
        .position(|c| c.name == showing)
        .map_or(-1, |i| i as i32);
    s.set_bench_run_labels(string_model(
        runs.iter()
            .map(|c| {
                format!(
                    "{}  ({}, {}, {} run{})",
                    c.name,
                    c.started,
                    c.tools,
                    c.runs,
                    if c.runs == 1 { "" } else { "s" }
                )
            })
            .collect(),
    ));
    s.set_bench_run_values(string_model(runs.iter().map(|c| c.name.clone()).collect()));
    s.set_bench_run_index(idx);
}

/// A campaign's table: every run's points under the run's label, in run
/// order (a tab run's labels are its preset ids, a sweep's `key=value`).
fn load_campaign(db: &Path, name: &str) -> Result<(Vec<bench::Point>, Vec<String>, bool), String> {
    let conn = bench::db::open_ro(db)?;
    let (runs, report) = bench::db::campaign_runs(&conn, name).map_err(|e| e.to_string())?;
    if runs.is_empty() {
        return Err(format!("no runs in campaign {name}"));
    }
    let mut points = Vec::new();
    let mut order = Vec::new();
    for (label, _tool, json) in &runs {
        points.extend(bench::record::points_from_json(label, json));
        order.push(label.clone());
    }
    Ok((points, order, report.is_some()))
}

/// The integer fields THIS mode reads whose text is not a number the run takes,
/// one status-line phrase each; empty when the tab can run. Asked before
/// `plan_from_ui`, whose fallback would otherwise run a mistyped Repetitions as
/// the default 3, or drop a mistyped entry from Depths, without a word: a
/// benchmark of a different workload than the one on screen. The fields of the
/// other mode are hidden, so they are not held against this one.
fn invalid_numbers(s: &AppState, mode: Mode) -> Vec<String> {
    let mut out = Vec::new();
    out.extend(ini::int_problem(
        "Repetitions",
        s.get_bench_reps().as_str(),
        &ini::INT_POSITIVE,
    ));
    match mode {
        Mode::Live => out.extend(ini::int_problem(
            "Max output tokens",
            s.get_bench_max_tokens().as_str(),
            &ini::INT_POSITIVE,
        )),
        Mode::Synthetic => {
            out.extend(bench::int_list_problem(
                "Prompt lengths",
                s.get_bench_prompt_lens().as_str(),
            ));
            out.extend(ini::int_problem(
                "Generate",
                s.get_bench_n_gen().as_str(),
                &ini::INT_NON_NEGATIVE,
            ));
            out.extend(bench::int_list_problem(
                "Depths",
                s.get_bench_depths().as_str(),
            ));
        }
    }
    out
}

/// Read the whole tab into a `Plan`. Numbers ride as text (the same reason every
/// other numeric field in this app does: a SpinBox edits itself on a stray
/// scroll), so an unparseable field falls back to its default here; Run and the
/// preview ask `invalid_numbers` first, so that fallback is never what runs.
fn plan_from_ui(app: &AppWindow, state: &Rc<RefCell<State>>) -> Plan {
    let s = app.global::<AppState>();
    let int = |v: SharedString, fallback: i32| ini::parse_int(v.as_str()).unwrap_or(fallback);
    let mode = Mode::from_str(s.get_bench_mode().as_str());
    Plan {
        mode,
        presets: state.borrow().bench_selection.clone(),
        reps: int(s.get_bench_reps(), bench::DEFAULT_REPS).max(1),
        // The CACHED text, which is what the preview needs. `on_bench_run`
        // re-reads the file into the plan it actually runs: the preview may be
        // up to one 5 s tick behind the file, a benchmark may not be behind at
        // all.
        prompt: state
            .borrow()
            .bench_prompt
            .as_ref()
            .map(|p| p.text.clone())
            .unwrap_or_default(),
        prompt_file: Some(configured_prompt_path(&s)),
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
        campaign: None,
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
    s.set_bench_report_campaign(SharedString::default());
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
                        Ok(campaign) => {
                            s.set_bench_report_campaign(SharedString::from(campaign.as_str()));
                            let tail = if mode == Mode::Synthetic {
                                " llama-server is still stopped; start it when you need it."
                            } else {
                                ""
                            };
                            set_status(
                                &app,
                                format!(
                                    "Benchmark finished: campaign {campaign} in bench.db.{tail}"
                                ),
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
                // Cheap unless the path or the file actually moved: the gate
                // inside is what lets a keystroke in the Temperature field call
                // this without re-reading a megabyte.
                refresh_prompt_info(&app, &state);
                refresh_preview(&app, &state);
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>()
            .on_bench_pick_prompt(move |current| {
                let Some(app) = app_weak.upgrade() else {
                    return current;
                };
                let start = std::path::PathBuf::from(current.as_str());
                let Some(picked) = pick_prompt_file(&start) else {
                    return current;
                };
                let picked = SharedString::from(picked.to_string_lossy().into_owned());
                // Set it here rather than waiting for the caller's assignment:
                // the persist and the re-read below both read the property.
                let s = app.global::<AppState>();
                s.set_bench_prompt_file(picked.clone());
                persist_prompt_path(&app);
                refresh_prompt_info(&app, &state);
                picked
            });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_bench_edit_prompt(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let path = configured_prompt_path(&app.global::<AppState>());
            // Seed a file that is not there yet: Edit on a path picked for a
            // prompt not written yet must open something, and the framework's
            // own prompt is a better starting point than an empty buffer.
            bench::ensure_prompt_file(&path);
            // The system's default .txt handler (`paths::open_in_desktop`, as
            // for the report and the Integrations folder). Deliberately not a window of ours: see
            // the callback's declaration in state.slint.
            match paths::open_in_desktop(&path) {
                Ok(_) => set_status(
                    &app,
                    format!(
                        "Opened {}. Save it and the readout follows within a few seconds.",
                        path.display()
                    ),
                    false,
                ),
                Err(e) => set_status(&app, format!("Cannot open {}: {e}", path.display()), true),
            }
            refresh_prompt_info(&app, &state);
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_bench_db_changed(move || {
            if let Some(app) = app_weak.upgrade() {
                refresh_runs(&app);
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_bench_db_accepted(move || {
            if let Some(app) = app_weak.upgrade() {
                if persist_db_path(&app) {
                    set_status(
                        &app,
                        "Benchmark database saved to settings.ini.".into(),
                        false,
                    );
                }
                refresh_runs(&app);
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_bench_pick_db(move |current| {
            let Some(app) = app_weak.upgrade() else {
                return current;
            };
            let s = app.global::<AppState>();
            let Some(picked) = pick_db_file(&configured_db_path(&s)) else {
                return current;
            };
            let picked = SharedString::from(picked.to_string_lossy().into_owned());
            // Set here, as for the prompt: the persist and the re-list read
            // the property, not the caller's pending assignment.
            s.set_bench_db(picked.clone());
            persist_db_path(&app);
            refresh_runs(&app);
            picked
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
            let mode = Mode::from_str(app.global::<AppState>().get_bench_mode().as_str());
            let bad = invalid_numbers(&app.global::<AppState>(), mode);
            if !bad.is_empty() {
                set_status(&app, format!("Not run: {}.", bad.join("; ")), true);
                return;
            }
            // The run writes where settings.ini points (`db::work_path`, as
            // `bench sweep` does), so the field the list is reading goes there
            // first, typed but never confirmed with Enter included.
            if !persist_db_path(&app) {
                return;
            }
            let mut plan = plan_from_ui(&app, &state);
            // Read the prompt file HERE, not from the display cache: the point
            // of keeping the prompt in a file is that it can be edited while
            // this window is open, and a run that measured the previous version
            // of the text would be wrong in the one way nothing downstream
            // could detect (the report's own digest would agree with itself).
            if plan.mode == Mode::Live {
                if let Some(path) = plan.prompt_file.clone() {
                    match bench::load_prompt_file(&path) {
                        Ok(text) => plan.prompt = text,
                        Err(e) => {
                            set_status(&app, e, true);
                            return;
                        }
                    }
                }
                persist_prompt_path(&app);
            }
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
        app.global::<AppState>().on_bench_run_picked(move |name| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let s = app.global::<AppState>();
            match load_campaign(&configured_db_path(&s), name.as_str()) {
                Ok((points, order, has_report)) => {
                    s.set_bench_showing(name.clone());
                    s.set_bench_baseline(SharedString::from(
                        order.first().cloned().unwrap_or_default(),
                    ));
                    s.set_bench_report_campaign(if has_report {
                        name.clone()
                    } else {
                        SharedString::default()
                    });
                    apply_points(&app, &points, &order);
                    set_status(&app, format!("Showing campaign {name}."), false);
                }
                Err(e) => set_status(&app, format!("Cannot read that campaign: {e}"), true),
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_bench_open_report(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let s = app.global::<AppState>();
            let name = s.get_bench_report_campaign();
            if name.is_empty() {
                return;
            }
            // The report lives in bench.db; the editor gets a copy in the temp
            // directory, rewritten on every open so it never goes stale.
            let report = bench::db::open_ro(&configured_db_path(&s)).and_then(|c| {
                bench::db::campaign_runs(&c, name.as_str())
                    .map_err(|e| e.to_string())
                    .and_then(|(_, r)| r.ok_or_else(|| "no report".to_string()))
            });
            let path = std::env::temp_dir().join(format!("llama-cpp-config-{name}.md"));
            match report.and_then(|r| std::fs::write(&path, r).map_err(|e| e.to_string())) {
                // The default .md handler, as for the prompt file.
                Ok(()) => {
                    let _ = paths::open_in_desktop(&path);
                }
                Err(e) => set_status(&app, format!("Cannot open the report: {e}"), true),
            }
        });
    }
}
