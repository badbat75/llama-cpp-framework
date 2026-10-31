//! Benchmark-tab callback wiring. The planning and aggregation live in
//! `crate::bench` (pure, unit-tested), the running in `crate::bench::exec`; this
//! file is the seam between them and `AppState`.
//!
//! Four things worth knowing before editing here.
//!
//! **The prompt is a FILE and this tab never holds it**. The workload card
//! carries a PATH (settings.ini `BenchPromptFile`, defaulting to the shipped
//! `bench-prompt-43k-1.0.txt`), Edit hands that file to the system's default
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
    /// `(modified, len)`, so an edit made outside does too. `None` when the
    /// file does not exist, which is itself a state worth noticing a change
    /// from.
    stamp: Option<(Option<SystemTime>, u64)>,
    /// The normalized text, for the preview. Empty when the read failed.
    text: String,
}

/// One row of the prompt list.
pub(super) struct PromptEntry {
    path: std::path::PathBuf,
    on: bool,
    /// One of the two the standard spec pins: always listed, never removed.
    shipped: bool,
    read: Option<PromptRead>,
    /// The readout under the row.
    info: String,
}

fn file_stamp(path: &Path) -> Option<(Option<SystemTime>, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok(), meta.len()))
}

/// Seed the prompt list once: every shipped prompt (installed, read in
/// place), then the configured one when it is not one of them, ticked.
fn seed_prompt_list(state: &Rc<RefCell<State>>) {
    if !state.borrow().bench_prompts.is_empty() {
        return;
    }
    let configured = settings::load().bench_prompt_path();
    let mut rows: Vec<PromptEntry> = bench::shipped_prompts()
        .into_iter()
        .map(|path| PromptEntry {
            on: path == configured,
            path,
            shipped: true,
            read: None,
            info: String::new(),
        })
        .collect();
    if !rows.iter().any(|r| r.on) {
        rows.push(PromptEntry {
            path: configured,
            on: true,
            shipped: false,
            read: None,
            info: String::new(),
        });
    }
    state.borrow_mut().bench_prompts = rows;
}

/// The ticked prompts' paths, in list order.
fn ticked_paths(state: &Rc<RefCell<State>>) -> Vec<std::path::PathBuf> {
    state
        .borrow()
        .bench_prompts
        .iter()
        .filter(|r| r.on)
        .map(|r| r.path.clone())
        .collect()
}

/// Tick exactly `paths`, adding any that is not listed yet.
fn tick_only(state: &Rc<RefCell<State>>, paths: &[std::path::PathBuf]) {
    let mut st = state.borrow_mut();
    for r in st.bench_prompts.iter_mut() {
        r.on = paths.contains(&r.path);
    }
    for p in paths {
        if !st.bench_prompts.iter().any(|r| &r.path == p) {
            st.bench_prompts.push(PromptEntry {
                path: p.clone(),
                on: true,
                shipped: false,
                read: None,
                info: String::new(),
            });
        }
    }
}

/// Re-read every listed prompt whose file changed, and republish the rows.
/// Returns whether any text changed, so a caller that has to rebuild the
/// preview knows to.
pub(super) fn refresh_prompt_info(app: &AppWindow, state: &Rc<RefCell<State>>) -> bool {
    seed_prompt_list(state);
    let mut changed = false;
    {
        let mut st = state.borrow_mut();
        for r in st.bench_prompts.iter_mut() {
            let stamp = file_stamp(&r.path);
            if r.read.as_ref().is_some_and(|p| p.stamp == stamp) {
                continue;
            }
            let (text, info) = match bench::load_prompt_file(&r.path) {
                Ok(t) => {
                    let summary = bench::prompt_summary(&t);
                    (t, summary)
                }
                // The error is the readout: "cannot read ..." beside the row is
                // the whole diagnosis, and Run reports the same sentence.
                Err(e) => (String::new(), e),
            };
            r.info = info;
            r.read = Some(PromptRead { stamp, text });
            changed = true;
        }
    }
    let rows: Vec<BenchPromptRow> = state
        .borrow()
        .bench_prompts
        .iter()
        .map(|r| BenchPromptRow {
            path: SharedString::from(r.path.to_string_lossy().into_owned()),
            name: SharedString::from(r.path.file_name().unwrap_or_default().to_string_lossy().into_owned()),
            info: SharedString::from(r.info.clone()),
            on: r.on,
            shipped: r.shipped,
        })
        .collect();
    app.global::<AppState>()
        .set_bench_prompts(ModelRc::new(VecModel::from(rows)));
    changed
}

/// Every ticked prompt read FRESH, for a run: not the display cache, since the
/// point of a prompt in a file is that it can be edited while this window is
/// open. An empty one is refused by name.
fn read_ticked_prompts(state: &Rc<RefCell<State>>) -> Result<Vec<bench::spec::Prompt>, String> {
    let mut out = Vec::new();
    for path in ticked_paths(state) {
        let text = bench::load_prompt_file(&path)?;
        if text.trim().is_empty() {
            return Err(format!("The live mode needs a prompt: {} is empty.", path.display()));
        }
        out.push((Some(path), text));
    }
    if out.is_empty() {
        return Err("Tick at least one prompt.".into());
    }
    Ok(out)
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
        .add_filter("Prompts", &["txt", "json", "md", "prompt"])
        .add_filter("All files", &["*"]);
    if let Some(parent) = start.parent().filter(|p| p.is_dir()) {
        dialog = dialog.set_directory(parent);
    }
    if let Some(name) = start.file_name().and_then(|n| n.to_str()) {
        dialog = dialog.set_file_name(name);
    }
    dialog.pick_file()
}

/// Persist the first ticked prompt into settings.ini (`BenchPromptFile`, what
/// `bench sweep` reads with no --prompt-file), read-modify-write so this
/// cannot wipe another key. Called when a run starts.
fn persist_prompt_path(app: &AppWindow, path: &Path) {
    // The default stays empty, so the setting never pins today's data folder.
    let field = if path == paths::bench_prompt_file() {
        String::new()
    } else {
        path.to_string_lossy().into_owned()
    };
    let mut cfg = settings::load();
    if cfg.bench_prompt_file.trim() == field {
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
        // As stored, not resolved: empty stays empty (the default), so saving
        // the field back never pins today's data folder into settings.ini.
        s.set_bench_db(SharedString::from(settings::load().bench_db.trim()));
        s.set_bench_temp(SharedString::from("0"));
        s.set_bench_max_tokens(SharedString::from(bench::DEFAULT_MAX_TOKENS.to_string()));
        s.set_bench_reps(SharedString::from(bench::DEFAULT_REPS.to_string()));
        s.set_bench_prompt_lens(SharedString::from(bench::DEFAULT_PROMPT_LENS));
        s.set_bench_depths(SharedString::from(bench::DEFAULT_DEPTHS));
        s.set_bench_n_gen(SharedString::from(bench::DEFAULT_GEN.to_string()));
        s.set_bench_spec_options(string_model(spec_options(state)));
        let mut keys = vec!["none".to_string()];
        keys.extend(bench::sweep::SWEEPABLE.iter().map(|(k, _)| k.to_string()));
        s.set_bench_ab_keys(string_model(keys));
        s.set_bench_spec(SharedString::from(SPEC_CUSTOM));
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
            let head = match steps_preview(&s, state, &plan.presets) {
                Some(Ok(text)) => format!("{text}\n\nThe first request:\n"),
                Some(Err(e)) => {
                    s.set_bench_preview(SharedString::from(format!("Run will refuse: {}.", e.trim_end_matches('.'))));
                    return;
                }
                None => String::new(),
            };
            let first = format!("{head}{}", bench::live_preview(port, &chosen[0].id, &plan));
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
    // A spec fixes repetitions and the output cap: its fields are greyed out
    // and not what runs.
    if mode == Mode::Live && chosen_spec(s).is_some() && !s.get_bench_spec_override() {
        return out;
    }
    out.extend(ini::int_problem(
        "Repetitions",
        s.get_bench_reps().as_str(),
        &ini::INT_POSITIVE,
    ));
    match mode {
        Mode::Live => out.extend(ini::int_problem(
            "Max output tokens",
            s.get_bench_max_tokens().as_str(),
            &ini::INT_NON_NEGATIVE,
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
        // re-reads the files into the plan it actually runs: the preview may
        // be up to one 5 s tick behind the file, a benchmark may not be behind
        // at all.
        // The first ticked prompt's: what the preview shows.
        prompt: state
            .borrow()
            .bench_prompts
            .iter()
            .find(|r| r.on)
            .and_then(|r| r.read.as_ref())
            .map(|p| p.text.clone())
            .unwrap_or_default(),
        prompt_file: Some(
            ticked_paths(state)
                .first()
                .cloned()
                .unwrap_or_else(paths::bench_prompt_file),
        ),
        // Checked = "leave the preset's own --temp alone", which is a third
        // instruction and not a missing one, so it must send NO temperature
        // rather than a number.
        temp: if s.get_bench_temp_default() {
            None
        } else {
            Some(ini::parse_float(s.get_bench_temp().as_str()).unwrap_or(0.0))
        },
        max_tokens: int(s.get_bench_max_tokens(), bench::DEFAULT_MAX_TOKENS),
        concurrency: bench::Concurrency::from_str(s.get_bench_concurrency().as_str())
            .unwrap_or_default(),
        prompt_lens: bench::parse_int_list(s.get_bench_prompt_lens().as_str()),
        depths: bench::parse_int_list(s.get_bench_depths().as_str()),
        n_gen: int(s.get_bench_n_gen(), bench::DEFAULT_GEN),
        // Only a named campaign or a question needs one: without either, the
        // run is a campaign of its own named after its stamp, as before.
        campaign: {
            let name = s.get_bench_campaign().trim().to_string();
            let question = s.get_bench_question().trim().to_string();
            (!name.is_empty() || !question.is_empty()).then(|| bench::record::Campaign {
                name: if name.is_empty() {
                    bench::stamp(bench::now_secs())
                } else {
                    name
                },
                question,
                label: None,
            })
        },
        spec: None,
    }
}

fn remember_spec_file(state: &Rc<RefCell<State>>, path: &Path) {
    let mut st = state.borrow_mut();
    if !st.bench_spec_files.iter().any(|p| p == path) {
        st.bench_spec_files.push(path.to_path_buf());
    }
}

/// The Spec combo moved: tick that spec's prompts (so the list shows what it
/// runs, and Override starts from them) and drop Override.
fn spec_changed(app: &AppWindow, state: &Rc<RefCell<State>>) {
    let s = app.global::<AppState>();
    if let Some(Ok(spec)) = chosen_spec(&s) {
        let paths: Vec<std::path::PathBuf> = spec.prompts.iter().map(|(p, _)| p.clone()).collect();
        tick_only(state, &paths);
    }
    s.set_bench_spec_override(false);
    s.set_bench_override_base(SharedString::default());
    refresh_prompt_info(app, state);
    refresh_preview(app, state);
}

/// Write what the card describes as a spec file of the user's: the chosen
/// spec with Override's changes, or for custom one pass of the Workload
/// fields; the ticked prompts pinned by their digest. Named after the file
/// (never `standard` or `custom`), Version 1. Returns the name.
fn save_spec(app: &AppWindow, state: &Rc<RefCell<State>>, path: &Path) -> Result<String, String> {
    let s = app.global::<AppState>();
    let name = path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.is_empty() || name == "standard" || name == SPEC_CUSTOM {
        return Err(format!(
            "the file name names the spec, and `{name}` is not one of yours: pick another"
        ));
    }
    let bad = invalid_numbers(&s, Mode::Live);
    if !bad.is_empty() {
        return Err(bad.join("; "));
    }
    let plan = Plan {
        mode: Mode::Live,
        ..plan_from_ui(app, state)
    };
    let pin = |prompts: Vec<bench::spec::Prompt>| -> Vec<(std::path::PathBuf, String)> {
        prompts
            .into_iter()
            .filter_map(|(f, t)| f.map(|f| (f, bench::prompt_info(&t).sha256)))
            .collect()
    };
    let mut spec = match chosen_spec(&s) {
        Some(spec) => {
            let mut spec = spec?;
            let changed = overridden(&s, state);
            if changed.contains(&"prompt") {
                spec.prompts = pin(read_ticked_prompts(state)?);
            }
            let derive = derive_from_ui(&s, state, &plan, &[]);
            spec.max_tokens = derive.max_tokens.unwrap_or(spec.max_tokens);
            for p in spec.passes.iter_mut() {
                p.temp = derive.temp.unwrap_or(p.temp);
                p.reps = derive.reps.unwrap_or(p.reps);
                p.concurrency = derive.concurrency.unwrap_or(p.concurrency);
            }
            spec
        }
        None => bench::spec::Spec {
            name: String::new(),
            version: String::new(),
            sha256: String::new(),
            prompts: pin(read_ticked_prompts(state)?),
            max_tokens: plan.max_tokens,
            passes: vec![bench::spec::Pass {
                name: "main".into(),
                temp: plan.temp,
                reps: plan.reps,
                concurrency: (s.get_bench_concurrency() != "all").then_some(plan.concurrency),
                overrides: Vec::new(),
            }],
            published: false,
        },
    };
    spec.name = name.clone();
    spec.version = "1".into();
    let text = bench::spec::to_ini(&spec, &paths::config_dir());
    // What reads back is what was meant: a spec this tab cannot load again is
    // not saved.
    bench::spec::parse(&text, &paths::config_dir())?;
    crate::ini::atomic_write(path, &text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(name)
}

/// What Run starts: one plan, or several into one campaign (a spec, several
/// prompts, or concurrency `all`), the same split as `bench sweep`.
enum Job {
    One(Box<Plan>),
    Steps(Vec<exec::Step>, String),
}

/// The Spec combo's first entry: no spec, the Workload fields as set.
const SPEC_CUSTOM: &str = "custom";
/// The combo's prefix for a spec file of the user's (loaded or saved).
const SPEC_FILE: &str = "file: ";

/// The Spec combo: custom, every published version of `standard`
/// (`standard.versions`, newest first), then the user's spec files.
fn spec_options(state: &Rc<RefCell<State>>) -> Vec<String> {
    let mut versions: Vec<String> = bench::spec::published_versions(bench::spec::STANDARD_VERSIONS)
        .into_iter()
        .map(|(v, _)| v)
        .collect();
    versions.sort_by_key(|v| std::cmp::Reverse(bench::spec::version_key(v)));
    let mut out: Vec<String> = versions.iter().map(|v| format!("standard-{v}")).collect();
    out.insert(0, SPEC_CUSTOM.into());
    out.extend(
        state
            .borrow()
            .bench_spec_files
            .iter()
            .map(|p| format!("{SPEC_FILE}{}", p.display())),
    );
    out
}

/// The spec the combo names: None for custom.
fn chosen_spec(s: &AppState) -> Option<Result<bench::spec::Spec, String>> {
    let v = s.get_bench_spec();
    if v.is_empty() || v == SPEC_CUSTOM {
        return None;
    }
    Some(if let Some(n) = v.strip_prefix("standard-") {
        bench::spec::standard_version(n)
    } else if let Some(path) = v.strip_prefix(SPEC_FILE) {
        bench::spec::load(path)
    } else {
        Err(format!("unknown spec {v}"))
    })
}

/// The Workload fields Override can take over besides the prompts, by the
/// name `Derive::fields` records, with their values as the card shows them.
fn workload_values(s: &AppState) -> [(&'static str, String); 4] {
    [
        (
            "temp",
            if s.get_bench_temp_default() {
                "preset".into()
            } else {
                s.get_bench_temp().to_string()
            },
        ),
        ("max-tokens", s.get_bench_max_tokens().to_string()),
        ("concurrency", s.get_bench_concurrency().to_string()),
        ("reps", s.get_bench_reps().to_string()),
    ]
}

const BASE_SEP: char = '\u{1f}';

/// What Override changed of the spec: `prompt` when the ticked prompts are not
/// the spec's, then each field changed since the tick. None without a spec or
/// the tick.
fn overridden(s: &AppState, state: &Rc<RefCell<State>>) -> Vec<&'static str> {
    let Some(Ok(spec)) = chosen_spec(s) else {
        return Vec::new();
    };
    if !s.get_bench_spec_override() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut ticked = ticked_paths(state);
    let mut pinned: Vec<std::path::PathBuf> = spec.prompts.iter().map(|(p, _)| p.clone()).collect();
    ticked.sort();
    pinned.sort();
    if ticked != pinned {
        out.push("prompt");
    }
    let base = s.get_bench_override_base();
    let base: Vec<&str> = base.split(BASE_SEP).collect();
    out.extend(
        workload_values(s)
            .iter()
            .enumerate()
            .filter(|(i, (_, v))| base.get(*i) != Some(&v.as_str()))
            .map(|(_, (f, _))| *f),
    );
    out
}

/// The spec values Override replaces, from `plan` (`plan_from_ui`'s) and the
/// ticked `prompts` read fresh.
fn derive_from_ui(
    s: &AppState,
    state: &Rc<RefCell<State>>,
    plan: &Plan,
    prompts: &[bench::spec::Prompt],
) -> bench::spec::Derive {
    let changed = overridden(s, state);
    let on = |f: &str| changed.contains(&f);
    bench::spec::Derive {
        prompt: on("prompt").then(|| prompts.to_vec()),
        temp: on("temp").then_some(plan.temp),
        max_tokens: on("max-tokens").then_some(plan.max_tokens),
        reps: on("reps").then_some(plan.reps),
        concurrency: on("concurrency")
            .then(|| (s.get_bench_concurrency() != "all").then_some(plan.concurrency)),
    }
}

fn prompt_label(p: &bench::spec::Prompt) -> String {
    p.0.as_ref()
        .and_then(|f| f.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "prompt".into())
}

/// The job the tab describes. `plan` is `plan_from_ui`'s; `prompts` the
/// ticked ones read fresh (empty for a synthetic run, or a spec that keeps its
/// own), the first already in `plan`.
fn job_from_ui(
    s: &AppState,
    state: &Rc<RefCell<State>>,
    plan: Plan,
    prompts: Vec<bench::spec::Prompt>,
) -> Result<Job, String> {
    let named = plan.campaign.clone();
    let into = |name: String, steps: Vec<exec::Step>| {
        let question = named.as_ref().map(|c| c.question.clone()).unwrap_or_default();
        let steps = steps
            .into_iter()
            .map(|st| exec::Step {
                plan: Plan {
                    campaign: Some(bench::record::Campaign {
                        name: name.clone(),
                        question: question.clone(),
                        label: None,
                    }),
                    ..st.plan
                },
                ..st
            })
            .collect();
        Job::Steps(steps, name)
    };
    let explicit = s.get_bench_campaign().trim().to_string();
    let ab = ab_from_ui(s, &plan.presets)?;
    let ab_key = ab.as_ref().map(|(k, _)| k.as_str());
    let expand = |steps: Vec<exec::Step>| match &ab {
        Some((k, vs)) => bench::spec::ab_expand(steps, k, vs),
        None => steps,
    };
    if let Some(spec) = chosen_spec(s) {
        if plan.presets.is_empty() {
            return Err("select one or more presets".into());
        }
        let spec = spec?;
        let derive = derive_from_ui(s, state, &plan, &prompts);
        let steps = expand(bench::spec::tab_steps(&spec, &plan, &derive, ab_key)?);
        let name = if explicit.is_empty() {
            bench::spec::default_campaign(&spec)
        } else {
            explicit
        };
        return Ok(into(name, steps));
    }
    if plan.mode != Mode::Live {
        if ab.is_none() {
            return Ok(Job::One(Box::new(plan)));
        }
        let name = if explicit.is_empty() { bench::stamp(bench::now_secs()) } else { explicit };
        let one = vec![exec::Step {
            label: String::new(),
            overrides: Vec::new(),
            plan,
        }];
        return Ok(into(name, expand(one)));
    }
    let all = s.get_bench_concurrency() == "all";
    let scenarios = if all {
        common_scenarios(&plan.presets)?
    } else {
        vec![plan.concurrency]
    };
    if prompts.len() <= 1 && scenarios.len() == 1 && !all && ab.is_none() {
        return Ok(Job::One(Box::new(plan)));
    }
    let mut steps = Vec::new();
    for p in &prompts {
        for c in &scenarios {
            let mut label = Vec::new();
            if prompts.len() > 1 {
                label.push(prompt_label(p));
            }
            if all {
                label.push(c.as_str().to_string());
            }
            steps.push(exec::Step {
                label: label.join(" · "),
                overrides: Vec::new(),
                plan: Plan {
                    prompt: p.1.clone(),
                    prompt_file: p.0.clone(),
                    concurrency: *c,
                    ..plan.clone()
                },
            });
        }
    }
    let name = if explicit.is_empty() {
        bench::stamp(bench::now_secs())
    } else {
        explicit
    };
    Ok(into(name, expand(steps)))
}

/// The A/B key and its values, each checked against every selected preset.
/// None for `none`.
fn ab_from_ui(s: &AppState, presets: &[String]) -> Result<Option<(String, Vec<String>)>, String> {
    let key = s.get_bench_ab_key().to_string();
    if key.is_empty() || key == "none" {
        return Ok(None);
    }
    let text = s.get_bench_ab_values();
    if text.trim().is_empty() {
        return Err(format!("A/B on {key}: type the values to compare"));
    }
    let values = bench::sweep::parse_values(text.as_str())?;
    if values.len() < 2 {
        return Err(format!("A/B on {key} needs two or more values"));
    }
    for p in presets {
        bench::sweep::check_values(p, &key, &values)?;
    }
    Ok(Some((key, values)))
}

/// The concurrency scenarios every one of `presets` supports.
fn common_scenarios(presets: &[String]) -> Result<Vec<bench::Concurrency>, String> {
    let mut all = bench::Concurrency::ALL.to_vec();
    for p in presets {
        let mine = bench::sweep::all_scenarios(p)?;
        all.retain(|c| mine.contains(c));
    }
    Ok(all)
}

fn path_name(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// The preview's head for a run of several steps: what each one is, without
/// reading the prompts (a spec's are checked against their pins at Run).
fn steps_preview(
    s: &AppState,
    state: &Rc<RefCell<State>>,
    presets: &[String],
) -> Option<Result<String, String>> {
    let head = runs_preview(s, state, presets);
    let ab = match ab_from_ui(s, presets) {
        Ok(None) => return head,
        Ok(Some((k, vs))) => format!(
            "A/B on {k}: every step once per value ({}), written to the presets for it and put back after.",
            vs.join(" | ")
        ),
        Err(e) => return Some(Err(e)),
    };
    Some(match head {
        None => Ok(ab),
        Some(Ok(h)) => Ok(format!("{ab}\n{h}")),
        Some(Err(e)) => Err(e),
    })
}

fn runs_preview(
    s: &AppState,
    state: &Rc<RefCell<State>>,
    presets: &[String],
) -> Option<Result<String, String>> {
    let ticked: Vec<String> = ticked_paths(state).iter().map(|p| path_name(p)).collect();
    if let Some(spec) = chosen_spec(s) {
        return Some(spec.and_then(|spec| {
            let scenarios = common_scenarios(presets)?;
            let changed = overridden(s, state);
            let cap = if changed.contains(&"max-tokens") {
                s.get_bench_max_tokens().trim().parse::<i32>().unwrap_or(0)
            } else {
                spec.max_tokens
            };
            let mut lines = vec![format!(
                "Spec {}-{}, every request at its rules (output cap {}):",
                spec.name,
                spec.version,
                if cap > 0 { cap.to_string() } else { "none".into() }
            )];
            if !spec.published {
                lines.push(format!(
                    "A spec of yours, not a published version of standard: recorded as spec = custom, derived from {}-{}.",
                    spec.name, spec.version
                ));
            }
            if !changed.is_empty() {
                lines.push(format!(
                    "Override: {} from the Workload card in every step; recorded as spec = custom, derived from {}-{}.",
                    changed.join(", "),
                    spec.name,
                    spec.version
                ));
            }
            let ui = workload_values(s);
            let ui_of = |f: &str| {
                changed
                    .contains(&f)
                    .then(|| ui.iter().find(|(k, _)| *k == f).map(|(_, v)| v.clone()))
                    .flatten()
            };
            let prompts: Vec<String> = if changed.contains(&"prompt") {
                ticked.clone()
            } else {
                spec.prompts.iter().map(|(f, _)| path_name(f)).collect()
            };
            for name in &prompts {
                for pass in &spec.passes {
                    let cs: Vec<String> = match ui_of("concurrency") {
                        Some(c) if c != "all" => vec![c],
                        Some(_) => scenarios.iter().map(|c| c.as_str().to_string()).collect(),
                        None => match pass.concurrency {
                            Some(c) => vec![c.as_str().to_string()],
                            None => scenarios.iter().map(|c| c.as_str().to_string()).collect(),
                        },
                    };
                    let temp = ui_of("temp")
                        .unwrap_or_else(|| pass.temp.map_or("preset".to_string(), |t| t.to_string()));
                    let reps = ui_of("reps").unwrap_or_else(|| pass.reps.to_string());
                    let over = if pass.overrides.is_empty() {
                        String::new()
                    } else {
                        let o: Vec<String> =
                            pass.overrides.iter().map(|(k, v)| format!("{k} = {v}")).collect();
                        format!(", presets set {} (llama-server restarted)", o.join(", "))
                    };
                    lines.push(format!(
                        "- {} · {name}: temp {temp}, {reps} reps, concurrency {}{over}",
                        pass.name,
                        cs.join(", ")
                    ));
                }
            }
            Ok(lines.join("\n"))
        }));
    }
    if Mode::from_str(s.get_bench_mode().as_str()) != Mode::Live {
        return None;
    }
    let all = s.get_bench_concurrency() == "all";
    if ticked.len() <= 1 && !all {
        return None;
    }
    Some((|| {
        let mut parts = Vec::new();
        if ticked.len() > 1 {
            parts.push(format!("prompts {}", ticked.join(", ")));
        }
        if all {
            let cs: Vec<&str> = common_scenarios(presets)?.iter().map(|c| c.as_str()).collect();
            parts.push(format!("concurrency {}", cs.join(", ")));
        }
        Ok(format!("In turn, into one campaign: {}.", parts.join(" x ")))
    })())
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
fn start_run(app: &AppWindow, state: &Rc<RefCell<State>>, job: Job) {
    let s = app.global::<AppState>();
    s.set_bench_running(true);
    s.set_bench_rows(model(Vec::<BenchResultRow>::new()));
    s.set_bench_showing(SharedString::default());
    s.set_bench_report_campaign(SharedString::default());
    s.set_bench_progress(SharedString::from("Starting…"));
    set_status(app, "Benchmark running…".into(), false);

    // Held until the run is done; refused rather than waited for, so the
    // window never hangs on a sweep running in a terminal.
    let gpu = match bench::gpu_lock::try_acquire() {
        Ok(g) => std::sync::Arc::new(std::sync::Mutex::new(Some(g))),
        Err(e) => {
            s.set_bench_running(false);
            s.set_bench_progress(SharedString::default());
            set_status(app, format!("Not run: {e}."), true);
            return;
        }
    };
    let (order, mode) = match &job {
        Job::One(plan) => (plan.presets.clone(), plan.mode),
        Job::Steps(steps, _) => (
            steps.first().map(|s| s.plan.presets.clone()).unwrap_or_default(),
            Mode::Live,
        ),
    };
    let app_weak = app.as_weak();
    let on_event = move |event: exec::Event| {
        if matches!(event, exec::Event::Done(_)) {
            gpu.lock().map(|mut g| g.take()).ok();
        }
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
    };
    let handle = match job {
        Job::One(plan) => exec::spawn(*plan, on_event),
        Job::Steps(steps, campaign) => exec::spawn_steps(steps, campaign, on_event),
    };
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
        app.global::<AppState>().on_bench_prompt_toggle(move |i| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            if let Some(r) = state.borrow_mut().bench_prompts.get_mut(i as usize) {
                r.on = !r.on;
            }
            refresh_prompt_info(&app, &state);
            refresh_preview(&app, &state);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_bench_prompt_add(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let start = ticked_paths(&state)
                .first()
                .cloned()
                .unwrap_or_else(paths::bench_prompt_file);
            let Some(picked) = pick_prompt_file(&start) else {
                return;
            };
            let mut paths = ticked_paths(&state);
            if !paths.contains(&picked) {
                paths.push(picked);
            }
            tick_only(&state, &paths);
            refresh_prompt_info(&app, &state);
            refresh_preview(&app, &state);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_bench_prompt_remove(move |i| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            {
                let mut st = state.borrow_mut();
                if st.bench_prompts.get(i as usize).is_some_and(|r| !r.shipped) {
                    st.bench_prompts.remove(i as usize);
                }
            }
            refresh_prompt_info(&app, &state);
            refresh_preview(&app, &state);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_bench_prompt_edit(move |i| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let Some(path) = state.borrow().bench_prompts.get(i as usize).map(|r| r.path.clone()) else {
                return;
            };
            // The system's default .txt handler (`paths::open_in_desktop`, as
            // for the report and the Integrations folder). Deliberately not a
            // window of ours: see the callback's declaration in state.slint.
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
        let state = state.clone();
        app.global::<AppState>().on_bench_spec_changed(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            spec_changed(&app, &state);
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_bench_spec_load(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let Some(path) = rfd::FileDialog::new()
                .set_title("Load a benchmark spec")
                .add_filter("Spec files", &["ini"])
                .add_filter("All files", &["*"])
                .pick_file()
            else {
                return;
            };
            match bench::spec::load(&path.to_string_lossy()) {
                Ok(spec) => {
                    // The same rules as a published version ARE that version.
                    let pick = if spec.published {
                        format!("standard-{}", spec.version)
                    } else {
                        remember_spec_file(&state, &path);
                        format!("{SPEC_FILE}{}", path.display())
                    };
                    let s = app.global::<AppState>();
                    s.set_bench_spec_options(string_model(spec_options(&state)));
                    s.set_bench_spec(SharedString::from(pick));
                    spec_changed(&app, &state);
                    set_status(&app, format!("Loaded spec {}-{}.", spec.name, spec.version), false);
                }
                Err(e) => set_status(&app, format!("Spec not loaded: {e}"), true),
            }
        });
    }
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.global::<AppState>().on_bench_spec_save(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let Some(path) = rfd::FileDialog::new()
                .set_title("Save the benchmark spec")
                .add_filter("Spec files", &["ini"])
                .set_file_name("my-spec.ini")
                .save_file()
            else {
                return;
            };
            match save_spec(&app, &state, &path) {
                Ok(name) => {
                    remember_spec_file(&state, &path);
                    let s = app.global::<AppState>();
                    s.set_bench_spec_options(string_model(spec_options(&state)));
                    s.set_bench_spec(SharedString::from(format!("{SPEC_FILE}{}", path.display())));
                    spec_changed(&app, &state);
                    set_status(&app, format!("Saved spec {name} v1 to {}.", path.display()), false);
                }
                Err(e) => set_status(&app, format!("Spec not saved: {e}"), true),
            }
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
            // Read the prompt files HERE, not from the display cache: the
            // point of keeping a prompt in a file is that it can be edited
            // while this window is open, and a run that measured the previous
            // version of the text would be wrong in the one way nothing
            // downstream could detect (the report's own digest would agree
            // with itself). A spec brings its own prompts, read and checked
            // against their pins when its steps are built, unless Override
            // took the prompts over.
            let s = app.global::<AppState>();
            let spec_set = chosen_spec(&s).is_some() && !overridden(&s, &state).contains(&"prompt");
            let mut prompts = Vec::new();
            if plan.mode == Mode::Live && !spec_set {
                match read_ticked_prompts(&state) {
                    Ok(p) => prompts = p,
                    Err(e) => {
                        set_status(&app, e, true);
                        return;
                    }
                }
                plan.prompt_file = prompts[0].0.clone();
                plan.prompt = prompts[0].1.clone();
                if let Some(first) = plan.prompt_file.clone() {
                    persist_prompt_path(&app, &first);
                }
            }
            if let Some(problem) = bench::validate(&plan).filter(|_| !spec_set) {
                set_status(&app, problem, true);
                return;
            }
            let job = match job_from_ui(&s, &state, plan, prompts) {
                Ok(job) => job,
                Err(e) => {
                    set_status(&app, format!("Not run: {e}."), true);
                    return;
                }
            };
            // The synthetic engine cannot share the GPU with llama-server: a
            // second resident copy of the model spills into shared memory and
            // reads as a backend regression rather than as a mistake. So stop
            // it first, through the canonical path (which snapshots the
            // conversation when that is enabled), and run only once it is gone.
            let needs_stop = matches!(&job, Job::One(p) if p.mode == Mode::Synthetic)
                && app.global::<AppState>().get_server_running();
            if needs_stop {
                let state = state.clone();
                let after = move |app: &AppWindow| {
                    start_run(app, &state, job);
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
            start_run(&app, &state, job);
        });
    }
    {
        let app_weak = app.as_weak();
        app.global::<AppState>().on_bench_override_toggled(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let s = app.global::<AppState>();
            let base = if s.get_bench_spec_override() {
                let v: Vec<String> = workload_values(&s).into_iter().map(|(_, v)| v).collect();
                v.join(&BASE_SEP.to_string())
            } else {
                String::new()
            };
            s.set_bench_override_base(SharedString::from(base));
            s.invoke_bench_changed();
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
